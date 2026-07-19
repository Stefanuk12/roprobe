use std::{path::PathBuf, process};

use serde::{Deserialize, Serialize};

use crate::protocol::Handshake;

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default, Serialize, Deserialize)]
pub struct Lockfile {
    #[serde(flatten)]
    pub handshake: Handshake,
    pub pid: u32,
}

impl Lockfile {
    /// Resolve the path of the lockfile in /tmp folder.
    pub fn path() -> PathBuf {
        std::env::temp_dir().join("roprobe").join("broker.json")
    }

    /// Generate new lockfile and handshake from a port.
    pub fn new(port: u16) -> Self {
        Self {
            handshake: Handshake::generate(port),
            pid: process::id(),
        }
    }

    /// Read and parse the lockfile, if a well-formed one exists.
    pub fn read() -> Option<Self> {
        let raw = std::fs::read_to_string(Self::path()).ok()?;
        serde_json::from_str(&raw).ok()
    }

    /// Write the lockfile for the current process so others can attach to it.
    pub fn write(&self) -> std::io::Result<PathBuf> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }

        let body = serde_json::to_string(&self).expect("serialize lockfile");
        std::fs::write(&path, body)?;
        Ok(path)
    }

    pub fn remove(&self) {
        let _ = std::fs::remove_file(Self::path());
    }
}

impl From<Handshake> for Lockfile {
    fn from(handshake: Handshake) -> Self {
        Self {
            handshake,
            pid: process::id(),
        }
    }
}

impl Drop for Lockfile {
    fn drop(&mut self) {
        self.remove();
    }
}