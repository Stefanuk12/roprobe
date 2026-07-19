use std::fmt::Write as _;

use rand::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default, Serialize, Deserialize)]
pub struct Handshake {
    pub port: u16,
    pub token: String,
}

impl Handshake {
    /// Mint a handshake for `port` with a fresh 256-bit hex auth token (hex avoids URL-encoding concerns when the extension passes it back as a `?token=` query parameter).
    pub fn generate(port: u16) -> Self {
        let mut bytes = [0u8; 32];
        let mut rng = rand::rng();
        rng.fill_bytes(&mut bytes);
        let mut token = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            let _ = write!(token, "{byte:02x}");
        }
        Self { port, token }
    }

    /// The single JSON line printed to stdout for the spawning parent to read.
    pub fn to_line(&self) -> String {
        serde_json::to_string(self).expect("serialize handshake")
    }
}
