use serde::{Deserialize, Serialize};

use super::{DomId, LogEntry, Operation};
use crate::{server::SessionId, upstream::Upstream};

/// Contains all the possible outbound events to a client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    /// Greet a client whose connection was just accepted.
    Hello,
    /// Confirm that an upstream connection was enabled or disabled.
    UpstreamChanged { upstream: Upstream, enabled: bool },
    /// Ask the client to mirror a node's immediate children (lazy population); `None` requests the watch root's top level, bootstrapping the tree.
    RequestChildren(Option<DomId>),
    /// Ask the client to mirror a single node by id, without its children.
    RequestNode(DomId),
    /// Ask the client to snapshot a subtree (recursive, with pruning) by id, or the whole watch scope when `None`.
    RequestSnapshot(Option<DomId>),
    /// Ask the client to search `from`'s descendants for nodes whose name
    /// contains `query`, mirroring the matches (and their ancestors).
    Search { from: DomId, query: String },
    /// Ask the client to mirror several single nodes by id, in one patch.
    RequestNodes(Vec<DomId>),
    /// Relay an upstream (verde) operation for the client to apply against the real game, `id` correlating the [`super::ClientMessage::OperationResult`] the client replies with.
    Operation { id: u32, op: Operation },
    /// Answer to [`super::ClientMessage::ListSessions`] (only sent to control connections): the connected sessions and which one is active.
    Sessions(Vec<SessionInfo>),

    // Control messages //

    /// CONTROL: A new session was added.
    NewSession(SessionId),
    /// CONTROL: A session has been removed.
    RemoveSession(SessionId),
    /// CONTROL: A batch of console output relayed from the session with the given id.
    SessionLog {
        id: SessionId,
        entries: Vec<LogEntry>,
    },
}

/// One connected client session as reported to the `sessions` command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: SessionId,
    pub peer: String,
    pub active: bool,
    pub security_level: u8,
}

impl ServerMessage {
    /// Encode into a Squash binary frame.
    pub fn to_bytes(&self) -> squash::Result<Vec<u8>> {
        squash::serde_serialize(self)
    }

    /// Decode a Squash-encoded binary frame (used by control commands).
    pub fn from_bytes(frame: impl Into<Vec<u8>>) -> squash::Result<Self> {
        squash::serde_deserialize(&mut frame.into())
    }
}

impl TryFrom<ServerMessage> for tokio_tungstenite::tungstenite::Message {
    type Error = squash::Error;
    fn try_from(value: ServerMessage) -> Result<Self, Self::Error> {
        // Ride a text frame as base64 — the executor's socket rejects binary frames.
        value
            .to_bytes()
            .map(|bytes| Self::Text(super::text_encode(&bytes).into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the request-message wire layouts the Luau client mirrors (`client/src/lib/Messages.luau`), where tuple variants put the payload first and the variant tag byte last.
    #[test]
    fn request_wire_layouts_are_pinned() {
        // RequestChildren(None) -> bootstrap the top level: Option `None` flag, then tag 2.
        assert_eq!(
            ServerMessage::RequestChildren(None).to_bytes().unwrap(),
            vec![0, 2]
        );
        // RequestChildren(Some(id)) -> string payload + Option `Some` flag, then tag 2.
        assert_eq!(
            ServerMessage::RequestChildren(Some("9".into()))
                .to_bytes()
                .unwrap(),
            vec![b'9', 1, 1, 2],
        );
        // RequestNode(id) -> string payload, then tag 3.
        assert_eq!(
            ServerMessage::RequestNode("9".into()).to_bytes().unwrap(),
            vec![b'9', 1, 3]
        );
        // RequestSnapshot(None) -> Option `None` flag, then tag 4.
        assert_eq!(
            ServerMessage::RequestSnapshot(None).to_bytes().unwrap(),
            vec![0, 4]
        );
        // RequestSnapshot(Some(id)) -> string payload + Option `Some` flag, then tag 4.
        assert_eq!(
            ServerMessage::RequestSnapshot(Some("9".into()))
                .to_bytes()
                .unwrap(),
            vec![b'9', 1, 1, 4],
        );
        // Search: a struct variant — fields reversed (query, then from), tag 5 last.
        assert_eq!(
            (ServerMessage::Search {
                from: "a".into(),
                query: "b".into()
            })
            .to_bytes()
            .unwrap(),
            vec![b'b', 1, b'a', 1, 5],
        );
        // RequestNodes: a Vec<DomId> — elements reversed then VLQ count, tag 6 last.
        assert_eq!(
            ServerMessage::RequestNodes(vec!["a".into(), "b".into()])
                .to_bytes()
                .unwrap(),
            vec![b'b', 1, b'a', 1, 2, 6],
        );
    }

    /// Pins [`ServerMessage::SessionLog`]'s frame: a struct variant, so `entries`
    /// lands before `id` with tag 11 last.
    #[test]
    fn session_log_wire_layout_is_pinned() {
        use crate::protocol::{LogEntry, LogLevel};

        assert_eq!(
            (ServerMessage::SessionLog {
                id: SessionId(7),
                entries: vec![LogEntry {
                    level: LogLevel::Warn,
                    content: "a".into()
                }],
            })
            .to_bytes()
            .unwrap(),
            // entry (level tag 2, then "a"), entries VLQ count, id (u32 LE), tag.
            vec![2, b'a', 1, 1, 7, 0, 0, 0, 11],
        );
    }

    /// Pins [`ServerMessage::Operation`]'s frame: a struct variant whose fields land reversed (`op` before `id`) with tag 7 last, the `op` payload a typed [`Operation`] mirrored by hand in the Luau `operationFrame` codec.
    #[test]
    fn operation_wire_layout_is_pinned() {
        // op = Delete { node: "n" }: node string then Operation tag 1; then id 1
        // (u32 LE); then ServerMessage tag 7.
        assert_eq!(
            (ServerMessage::Operation {
                id: 1,
                op: Operation::Delete { node: "n".into() }
            })
            .to_bytes()
            .unwrap(),
            vec![b'n', 1, 1, 1, 0, 0, 0, 7],
        );
    }
}
