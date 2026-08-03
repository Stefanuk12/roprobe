use serde::{Deserialize, Serialize};
use squash::ReverseDeserialize;

/// The severity of one relayed console line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    Print,
    Info,
    Warn,
    Error,
}

/// One line of a client's console output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ReverseDeserialize)]
pub struct LogEntry {
    pub level: LogLevel,
    pub content: String,
}
