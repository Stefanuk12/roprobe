use serde::{Deserialize, Serialize};
use squash::ReverseDeserialize;

use super::{DomId, LogEntry, OpResult, Operation};
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
    NewSession(SessionInfo),
    /// CONTROL: A session has been removed.
    RemoveSession(SessionId),
    /// CONTROL: A batch of console output relayed from the session with the given id.
    SessionLog {
        id: SessionId,
        entries: Vec<LogEntry>,
    },
    /// CONTROL: The outcome of a [`super::ClientMessage::RunCode`].
    RunResult {
        session: SessionId,
        request: u32,
        result: OpResult,
    },
}

/// One connected client session as reported to the `sessions` command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ReverseDeserialize)]
pub struct SessionInfo {
    pub id: SessionId,
    pub username: Option<String>,
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

    /// Pins [`SessionInfo`]'s frame — a plain struct, so its fields land in
    /// declaration order — and the two messages that carry it. The extension
    /// mirrors this layout by hand (`extension/src/broker/message/server.ts`).
    #[test]
    fn session_info_wire_layout_is_pinned() {
        let named = SessionInfo {
            id: SessionId(7),
            username: Some("N".into()),
            peer: "p".into(),
            active: true,
            security_level: 3,
        };
        #[rustfmt::skip]
        let expected = vec![
            7, 0, 0, 0,         // id (SessionId is a newtype over u32)
            b'N', 1, 1,         // username: Some -> payload + 0x01 flag
            b'p', 1,            // peer
            1,                  // active
            3,                  // security_level (u8)
        ];
        assert_eq!(squash::serde_serialize(&named).unwrap(), expected);

        // A client that never named itself collapses the username to a lone flag.
        let anonymous = SessionInfo {
            username: None,
            ..named.clone()
        };
        assert_eq!(
            squash::serde_serialize(&anonymous).unwrap(),
            vec![7, 0, 0, 0, 0, b'p', 1, 1, 3],
        );

        // Sessions is a Vec: elements reversed, VLQ count last, then tag 8.
        assert_eq!(
            ServerMessage::Sessions(vec![named.clone()])
                .to_bytes()
                .unwrap(),
            [expected.clone(), vec![1, 8]].concat(),
        );
        // NewSession carries one straight, tag 9 last.
        assert_eq!(
            ServerMessage::NewSession(named.clone()).to_bytes().unwrap(),
            [expected, vec![9]].concat(),
        );

        // Read back, not just written: the `sessions` command decodes these, and a
        // plain `Deserialize` derive reads every field out of the next one's bytes.
        for info in [named, anonymous] {
            let ServerMessage::Sessions(back) =
                ServerMessage::from_bytes(ServerMessage::Sessions(vec![info.clone()]).to_bytes().unwrap())
                    .unwrap()
            else {
                panic!("decoded a different variant");
            };
            assert_eq!(back, vec![info.clone()]);

            let ServerMessage::NewSession(back) =
                ServerMessage::from_bytes(ServerMessage::NewSession(info.clone()).to_bytes().unwrap())
                    .unwrap()
            else {
                panic!("decoded a different variant");
            };
            assert_eq!(back, info);
        }
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

    /// Pins [`ServerMessage::RunResult`]'s frame: a struct variant, so its fields
    /// land reversed (`result`, `request`, `session`) with tag 12 last.
    #[test]
    fn run_result_wire_layout_is_pinned() {
        assert_eq!(
            (ServerMessage::RunResult {
                session: SessionId(7),
                request: 1,
                result: OpResult::Output("x".into()),
            })
            .to_bytes()
            .unwrap(),
            // "x" then OpResult tag 3, request (u32 LE), session (u32 LE), tag.
            vec![b'x', 1, 3, 1, 0, 0, 0, 7, 0, 0, 0, 12],
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

