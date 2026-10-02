//! Reading a blob a piece at a time, so serving a large file doesn't hold it in
//! memory (NFR-PERF-013).
//!
//! gix only reads whole objects into a buffer, so this reads the object
//! database itself, where it can:
//!
//! - a loose object is one zlib stream, `blob <size>\0` and then the content;
//! - a blob stored whole in a pack is a zlib stream starting just after its entry
//!   header. Packs are memory-mapped, so reading one costs page cache, not heap.
//!
//! A blob stored as a delta has to be rebuilt in memory, and falls back to gix.
//! git doesn't delta-compress files above `core.bigFileThreshold` (512 MiB by
//! default), so the largest files are the ones that stream.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};

use gix::ObjectId;
use gix::zlib::Decompress;
use gix_pack::data::entry::Header;

use crate::{Error, Result};

/// A blob's content, readable with [`Read`].
pub struct BlobReader {
    size: u64,
    source: Source,
}

enum Source {
    Memory(io::Cursor<Vec<u8>>),
    Inflate { input: Input, state: Box<Decompress>, remaining: u64 },
}

/// Compressed bytes: a loose object file, or a pack from an offset onwards.
enum Input {
    Loose(BufReader<File>),
    Pack { pack: gix_pack::data::File, at: u64 },
}

impl BlobReader {
    /// Opens the blob `id`.
    pub fn open(repo: &gix::Repository, id: ObjectId) -> Result<Self> {
        let header = repo.find_header(id).map_err(Error::git)?;
        if header.kind() != gix::object::Kind::Blob {
            return Err(Error::Git(format!("{id} is a {}, not a blob", header.kind()).into()));
        }
        let objects = repo.common_dir().join("objects");
        let hex = id.to_hex().to_string();
        match File::open(objects.join(&hex[..2]).join(&hex[2..])) {
            Ok(file) => return Self::loose(file, header.size()),
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }
        if let Some(reader) = Self::packed(&objects.join("pack"), repo.object_hash(), id)? {
            return Ok(reader);
        }
        let data = repo.find_blob(id).map_err(Error::git)?.detach().data;
        Ok(Self { size: data.len() as u64, source: Source::Memory(io::Cursor::new(data)) })
    }

    /// The size of the content in bytes.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Whether the whole blob was loaded into memory (a delta in a pack).
    pub fn in_memory(&self) -> bool {
        matches!(self.source, Source::Memory(_))
    }

    fn loose(file: File, size: u64) -> Result<Self> {
        let mut reader = Self {
            size,
            source: Source::Inflate {
                input: Input::Loose(BufReader::new(file)),
                state: Box::default(),
                remaining: u64::MAX,
            },
        };
        // Skip the `blob <size>\0` header.
        let mut byte = [0u8];
        loop {
            reader.read_exact(&mut byte)?;
            if byte[0] == 0 {
                break;
            }
        }
        if let Source::Inflate { remaining, .. } = &mut reader.source {
            *remaining = size;
        }
        Ok(reader)
    }

    /// The blob from whichever pack holds it, if it's stored whole.
    fn packed(dir: &std::path::Path, hash: gix::hash::Kind, id: ObjectId) -> Result<Option<Self>> {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err.into()),
        };
        for entry in entries {
            let path = entry?.path();
            if path.extension().is_none_or(|ext| ext != "idx") {
                continue;
            }
            let index = gix_pack::index::File::at(&path, hash).map_err(Error::git)?;
            let Some(position) = index.lookup(id) else {
                continue;
            };
            let pack = gix_pack::data::File::at(path.with_extension("pack"), hash).map_err(Error::git)?;
            let entry = pack.entry(index.pack_offset_at_index(position)).map_err(Error::git)?;
            if entry.header != Header::Blob {
                return Ok(None);
            }
            let input = Input::Pack { pack, at: entry.data_offset };
            let source = Source::Inflate { input, state: Box::default(), remaining: entry.decompressed_size };
            return Ok(Some(Self { size: entry.decompressed_size, source }));
        }
        Ok(None)
    }
}

impl Read for BlobReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match &mut self.source {
            Source::Memory(cursor) => cursor.read(buf),
            Source::Inflate { input, state, remaining } => {
                if *remaining == 0 || buf.is_empty() {
                    return Ok(0);
                }
                let len = buf.len().min(usize::try_from(*remaining).unwrap_or(usize::MAX));
                let read = gix::zlib::stream::inflate::read(input, state, &mut buf[..len])?;
                if read == 0 {
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "blob ended early"));
                }
                *remaining -= read as u64;
                Ok(read)
            }
        }
    }
}

impl Read for Input {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let available = self.fill_buf()?;
        let len = available.len().min(buf.len());
        buf[..len].copy_from_slice(&available[..len]);
        self.consume(len);
        Ok(len)
    }
}

impl BufRead for Input {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        match self {
            Input::Loose(reader) => reader.fill_buf(),
            Input::Pack { pack, at } => Ok(pack.entry_slice(*at..pack.pack_end() as u64).unwrap_or_default()),
        }
    }

    fn consume(&mut self, amount: usize) {
        match self {
            Input::Loose(reader) => reader.consume(amount),
            Input::Pack { at, .. } => *at += amount as u64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_all(mut reader: BlobReader) -> Vec<u8> {
        let mut out = Vec::new();
        // Small reads, to cross many inflate calls.
        let mut buf = [0u8; 4096];
        loop {
            let n = reader.read(&mut buf).unwrap();
            if n == 0 {
                return out;
            }
            out.extend_from_slice(&buf[..n]);
        }
    }

    #[test]
    fn loose_blobs_stream_without_their_header() {
        let dir = tempfile::tempdir().unwrap();
        let repo = gix::init_bare(dir.path().join("r.git")).unwrap();
        let content: Vec<u8> = (0..3_000_000u32).map(|n| (n * 7 % 251) as u8).collect();
        let id = repo.write_blob(&content).unwrap().detach();

        let reader = BlobReader::open(&repo, id).unwrap();
        assert_eq!(reader.size(), content.len() as u64);
        assert!(!reader.in_memory());
        assert_eq!(read_all(reader), content);

        let empty = repo.write_blob(b"").unwrap().detach();
        assert!(read_all(BlobReader::open(&repo, empty).unwrap()).is_empty());
    }

    #[test]
    fn only_blobs_open() {
        let dir = tempfile::tempdir().unwrap();
        let repo = gix::init_bare(dir.path().join("r.git")).unwrap();
        let tree = repo.empty_tree().edit().unwrap().write().unwrap().detach();
        assert!(BlobReader::open(&repo, tree).is_err());
    }
}
