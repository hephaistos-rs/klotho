//! Side-band multiplexing (`side-band-64k`): pack data, progress messages and a
//! fatal error share one stream of pkt-lines, each starting with its band number.

use std::io::{self, Write};

use super::pktline::{self, MAX_DATA};

const PACK_DATA: u8 = 1;
const PROGRESS: u8 = 2;
const ERROR: u8 = 3;

/// Writes everything it's given to band 1, split into full pkt-lines.
pub struct SidebandWriter<W> {
    inner: W,
}

impl<W: Write> SidebandWriter<W> {
    pub fn new(inner: W) -> Self {
        Self { inner }
    }

    pub fn into_inner(self) -> W {
        self.inner
    }

    pub fn progress(&mut self, message: &str) -> io::Result<()> {
        self.band(PROGRESS, message.as_bytes())
    }

    /// Tells the client the operation failed. The client prints `message` and stops.
    pub fn error(&mut self, message: &str) -> io::Result<()> {
        self.band(ERROR, message.as_bytes())
    }

    fn band(&mut self, band: u8, data: &[u8]) -> io::Result<()> {
        for chunk in data.chunks(MAX_DATA - 1) {
            let mut packet = Vec::with_capacity(chunk.len() + 1);
            packet.push(band);
            packet.extend_from_slice(chunk);
            pktline::write_data(&mut self.inner, &packet)?;
        }
        Ok(())
    }
}

impl<W: Write> Write for SidebandWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let len = buf.len().min(MAX_DATA - 1);
        self.band(PACK_DATA, &buf[..len])?;
        Ok(len)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_data_into_band_1_packets() {
        let mut writer = SidebandWriter::new(Vec::new());
        writer.write_all(&vec![7u8; MAX_DATA + 10]).unwrap();
        let out = writer.into_inner();
        // One full packet (65520 bytes on the wire) and one of 12 data bytes.
        assert_eq!(&out[..5], b"fff0\x01");
        assert_eq!(&out[65520..65525], b"0010\x01");
        assert_eq!(out.len(), 65520 + 4 + 1 + 11);
    }
}
