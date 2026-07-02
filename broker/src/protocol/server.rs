use serde::{Deserialize, Serialize};

/// Contains all the possible outbound events to a client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    /// Greet a client whose connection was just accepted.
    Hello,
}

impl ServerMessage {
    /// Encode into a Squash binary frame.
    pub fn to_bytes(&self) -> squash::Result<Vec<u8>> {
        squash::serde_serialize(self)
    }
}

impl TryFrom<ServerMessage> for tokio_tungstenite::tungstenite::Message {
    type Error = squash::Error;
    fn try_from(value: ServerMessage) -> Result<Self, Self::Error> {
        value.to_bytes().map(Into::into).map(Self::Binary)
    }
}