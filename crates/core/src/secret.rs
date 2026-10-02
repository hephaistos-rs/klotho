//! Random secrets and how they're stored. Tokens are high-entropy, so a fast
//! hash (SHA-256) is enough to store them; passwords aren't, so they get Argon2id.

use sha2::{Digest, Sha256};

const BASE62: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// `len` random base62 characters (about 5.95 bits each).
pub fn random_base62(len: usize) -> String {
    let mut out = String::with_capacity(len);
    let mut byte = [0u8; 1];
    while out.len() < len {
        getrandom::fill(&mut byte).expect("the system's randomness source failed");
        // 62 * 4 = 248: rejecting 248..=255 keeps every character equally likely.
        if byte[0] < 248 {
            out.push(BASE62[usize::from(byte[0] % 62)] as char);
        }
    }
    out
}

/// `value` in base62, left-padded with `0` to `width` characters.
pub fn base62(mut value: u64, width: usize) -> String {
    let mut digits = Vec::new();
    while value > 0 {
        digits.push(BASE62[(value % 62) as usize]);
        value /= 62;
    }
    while digits.len() < width {
        digits.push(b'0');
    }
    digits.reverse();
    String::from_utf8(digits).expect("base62 digits are ASCII")
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

/// CRC-32 (IEEE), for token checksums. Not a security measure: it lets typos
/// be rejected without a database lookup.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_the_standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn base62_pads_and_encodes() {
        assert_eq!(base62(0, 3), "000");
        assert_eq!(base62(61, 1), "z");
        assert_eq!(base62(62, 2), "10");
        assert_eq!(base62(u64::from(u32::MAX), 6).len(), 6);
    }

    #[test]
    fn random_base62_has_the_length_asked_for() {
        let a = random_base62(30);
        assert_eq!(a.len(), 30);
        assert!(a.bytes().all(|b| b.is_ascii_alphanumeric()));
        assert_ne!(a, random_base62(30));
    }
}
