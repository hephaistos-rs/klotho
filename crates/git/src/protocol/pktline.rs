//! pkt-lines, git's framing for everything on the wire (`gitprotocol-common`).
//!
//! A pkt-line is a 4-digit hex length (counting the 4 digits themselves) and
//! then the data. Three lengths are special: `0000` (flush), `0001` (delimiter)
//! and `0002` (response end). Lengths 1–3 are invalid.

use std::io::{self, Read, Write};

use super::ProtocolError;

/// The most data one pkt-line can carry: 65520 bytes minus the 4-byte length.
pub const MAX_DATA: usize = 65516;

pub fn write_data(out: &mut (impl Write + ?Sized), data: &[u8]) -> io::Result<()> {
    assert!(data.len() <= MAX_DATA, "pkt-line data of {} bytes is too long", data.len());
    write!(out, "{:04x}", data.len() + 4)?;
    out.write_all(data)
}

/// A text line: `line` plus the trailing newline git expects.
pub fn write_line(out: &mut (impl Write + ?Sized), line: &str) -> io::Result<()> {
    assert!(line.len() < MAX_DATA, "pkt-line text of {} bytes is too long", line.len());
    writeln!(out, "{:04x}{line}", line.len() + 5)
}

pub fn write_flush(out: &mut (impl Write + ?Sized)) -> io::Result<()> {
    out.write_all(b"0000")
}

pub fn write_delim(out: &mut (impl Write + ?Sized)) -> io::Result<()> {
    out.write_all(b"0001")
}

#[derive(Debug, PartialEq, Eq)]
pub enum Packet<'a> {
    Data(&'a [u8]),
    Flush,
    Delim,
    ResponseEnd,
}

impl<'a> Packet<'a> {
    /// The data as a text line, without its trailing newline. Non-UTF-8 data and
    /// special packets are errors, since every text line in the protocol is ASCII.
    pub fn as_line(&self) -> Result<&'a str, ProtocolError> {
        match self {
            Packet::Data(data) => std::str::from_utf8(data)
                .map(|line| line.strip_suffix('\n').unwrap_or(line))
                .map_err(|_| ProtocolError::new("a protocol line isn't valid text")),
            _ => Err(ProtocolError::new("expected a text line")),
        }
    }
}

/// Reads pkt-lines from a complete request body.
pub struct Reader<'a> {
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Self { rest: input }
    }

    pub fn is_empty(&self) -> bool {
        self.rest.is_empty()
    }

    /// The next packet, or `None` at the end of the input.
    pub fn next_packet(&mut self) -> Result<Option<Packet<'a>>, ProtocolError> {
        if self.rest.is_empty() {
            return Ok(None);
        }
        let header = self.rest.get(..4).ok_or_else(|| ProtocolError::new("truncated pkt-line length"))?;
        let packet = match parse_length(header)? {
            Length::Special(packet) => packet,
            Length::Data(len) => {
                let data = self.rest.get(4..len).ok_or_else(|| ProtocolError::new("truncated pkt-line"))?;
                self.rest = &self.rest[len..];
                return Ok(Some(Packet::Data(data)));
            }
        };
        self.rest = &self.rest[4..];
        Ok(Some(packet))
    }
}

/// Reads pkt-lines one at a time from a stream, for requests too large to
/// hold in memory (a push carries its pack after the pkt-lines). Reads exactly
/// the bytes of each packet, so the stream is positioned right after it.
pub struct StreamReader<R> {
    inner: R,
    buf: Vec<u8>,
}

impl<R: Read> StreamReader<R> {
    pub fn new(inner: R) -> Self {
        Self { inner, buf: Vec::new() }
    }

    pub fn into_inner(self) -> R {
        self.inner
    }

    /// The next packet, or `None` if the stream ends before one starts.
    pub fn next_packet(&mut self) -> Result<Option<Packet<'_>>, ReadError> {
        let mut header = [0u8; 4];
        let mut filled = 0;
        while filled < 4 {
            match self.inner.read(&mut header[filled..]) {
                Ok(0) if filled == 0 => return Ok(None),
                Ok(0) => return Err(ProtocolError::new("truncated pkt-line length").into()),
                Ok(n) => filled += n,
                Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
                Err(err) => return Err(err.into()),
            }
        }
        match parse_length(&header)? {
            Length::Special(packet) => Ok(Some(packet)),
            Length::Data(len) => {
                self.buf.resize(len - 4, 0);
                self.inner.read_exact(&mut self.buf).map_err(|err| match err.kind() {
                    io::ErrorKind::UnexpectedEof => ProtocolError::new("truncated pkt-line").into(),
                    _ => ReadError::Io(err),
                })?;
                Ok(Some(Packet::Data(&self.buf)))
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

enum Length {
    Special(Packet<'static>),
    /// The whole packet's length, header included.
    Data(usize),
}

fn parse_length(header: &[u8]) -> Result<Length, ProtocolError> {
    let len = std::str::from_utf8(header)
        .ok()
        .and_then(|hex| usize::from_str_radix(hex, 16).ok())
        .ok_or_else(|| ProtocolError::new("invalid pkt-line length"))?;
    Ok(match len {
        0 => Length::Special(Packet::Flush),
        1 => Length::Special(Packet::Delim),
        2 => Length::Special(Packet::ResponseEnd),
        3 => return Err(ProtocolError::new("invalid pkt-line length 3")),
        _ if len > MAX_DATA + 4 => return Err(ProtocolError::new("pkt-line is too long")),
        _ => Length::Data(len),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_lines_with_their_own_length() {
        let mut out = Vec::new();
        write_line(&mut out, "version 2").unwrap();
        write_delim(&mut out).unwrap();
        write_flush(&mut out).unwrap();
        assert_eq!(out, b"000eversion 2\n00010000");
    }

    #[test]
    fn reads_data_and_special_packets() {
        let mut reader = Reader::new(b"0014command=ls-refs\n000100000002");
        assert_eq!(reader.next_packet().unwrap().unwrap().as_line().unwrap(), "command=ls-refs");
        assert_eq!(reader.next_packet().unwrap(), Some(Packet::Delim));
        assert_eq!(reader.next_packet().unwrap(), Some(Packet::Flush));
        assert_eq!(reader.next_packet().unwrap(), Some(Packet::ResponseEnd));
        assert_eq!(reader.next_packet().unwrap(), None);
    }

    #[test]
    fn rejects_malformed_input() {
        for bad in [&b"00"[..], b"zzzz", b"0003", b"0010short", b"ffffxxxx"] {
            assert!(Reader::new(bad).next_packet().is_err(), "{:?}", String::from_utf8_lossy(bad));
            assert!(StreamReader::new(bad).next_packet().is_err(), "{:?}", String::from_utf8_lossy(bad));
        }
    }

    #[test]
    fn stream_reader_stops_right_after_each_packet() {
        let mut reader = StreamReader::new(&b"0009line\n0000PACK"[..]);
        assert_eq!(reader.next_packet().unwrap().unwrap().as_line().unwrap(), "line");
        assert_eq!(reader.next_packet().unwrap(), Some(Packet::Flush));
        assert_eq!(reader.into_inner(), b"PACK");
        assert_eq!(StreamReader::new(&b""[..]).next_packet().unwrap(), None);
    }
}
