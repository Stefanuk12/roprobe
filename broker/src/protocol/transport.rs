//! The broker's frames ride WebSocket *text* frames as standard base64 because the target executor's socket rejects binary frames (closing with 1007 "invalid frame payload data") and UTF-8-validates text, mirrored by the Luau `Messages` codec (`crypt.base64encode`/`decode`).

use base64::{Engine, engine::general_purpose::STANDARD};

/// Base64-encode raw bytes into a UTF-8-safe text payload.
pub fn text_encode(bytes: &[u8]) -> String {
    STANDARD.encode(bytes)
}

/// Decode a base64 text payload back to raw bytes, or `None` if it isn't valid.
pub fn text_decode(text: &str) -> Option<Vec<u8>> {
    STANDARD.decode(text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips_arbitrary_bytes() {
        let bytes = vec![0x00, 0x01, 0x7f, 0x80, 0xff, b'a', 0xde, 0xad];
        assert_eq!(text_decode(&text_encode(&bytes)), Some(bytes));
        // Standard alphabet + padding, matching the executor's crypt.base64encode.
        assert_eq!(text_encode(b"abc"), "YWJj");
        assert_eq!(text_encode(b"ab"), "YWI=");
        assert_eq!(text_encode(&[]), "");
        assert_eq!(text_decode(""), Some(vec![]));
    }

    #[test]
    fn base64_decode_rejects_malformed_input() {
        assert_eq!(text_decode("not valid base64!!"), None);
    }
}
