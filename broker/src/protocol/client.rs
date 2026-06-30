use serde::{Deserialize, Serialize};

/// Contains all the possible inbound events from a client.=
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum ClientMessage {
    /// Ask the broker to shut itself down gracefully.
    Shutdown,
}

impl ClientMessage {
    /// Parse a JSON text frame into a typed message.
    pub fn from_json(frame: &str) -> serde_json::Result<Self> {
        serde_json::from_str(frame)
    }

    pub fn kind(&self) -> &'static str {
        match self {
            ClientMessage::Shutdown => "shutdown",
        }
    }
}
