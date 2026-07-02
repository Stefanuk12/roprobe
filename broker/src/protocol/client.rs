use serde::{Deserialize, Serialize};
use tokio_tungstenite::tungstenite::Message;

/// Contains all the possible inbound events from a client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Ask the broker to shut itself down gracefully.
    Shutdown,
}

impl ClientMessage {
    /// Decode a Squash-encoded binary frame into a typed message.
    pub fn from_bytes(frame: impl Into<Vec<u8>>) -> squash::Result<Self> {
        squash::serde_deserialize(&mut frame.into())
    }

    /// Encode into a Squash binary frame.
    pub fn to_bytes(&self) -> squash::Result<Vec<u8>> {
        squash::serde_serialize(self)
    }

    pub fn kind(&self) -> &'static str {
        match self {
            ClientMessage::Shutdown => "shutdown",
        }
    }
}

impl TryFrom<ClientMessage> for Message {
    type Error = squash::Error;
    fn try_from(value: ClientMessage) -> Result<Self, Self::Error> {
        value.to_bytes().map(Into::into).map(Self::Binary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_round_trips_as_a_single_tag_byte() {
        let bytes = ClientMessage::Shutdown.to_bytes().unwrap();
        assert_eq!(bytes, [0x00]);
        assert!(matches!(
            ClientMessage::from_bytes(bytes).unwrap(),
            ClientMessage::Shutdown
        ));
    }
}
