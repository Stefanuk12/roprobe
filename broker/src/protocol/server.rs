use serde::{Deserialize, Serialize};

use super::DomId;
use crate::upstream::Upstream;

/// Contains all the possible outbound events to a client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    /// Greet a client whose connection was just accepted.
    Hello,
    /// Confirm that an upstream connection was enabled or disabled.
    UpstreamChanged { upstream: Upstream, enabled: bool },
    /// Ask the client to mirror a node's immediate children (lazy population).
    /// `None` requests the watch root's top level, bootstrapping the tree.
    RequestChildren(Option<DomId>),
    /// Ask the client to mirror a single node by id, without its children.
    RequestNode(DomId),
    /// Ask the client to snapshot a subtree (recursive, with pruning) by id.
    /// `None` snapshots the whole watch scope.
    RequestSnapshot(Option<DomId>),
    /// Ask the client to search `from`'s descendants for nodes whose name
    /// contains `query`, mirroring the matches (and their ancestors).
    Search { from: DomId, query: String },
    /// Ask the client to mirror several single nodes by id, in one patch.
    RequestNodes(Vec<DomId>),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the request-message wire layouts the Luau client mirrors
    /// (`client/src/lib/Messages.luau`). Tuple variants: the payload lands
    /// first, the variant tag byte last.
    #[test]
    fn request_wire_layouts_are_pinned() {
        // RequestChildren(None) -> bootstrap the top level: Option `None` flag, then tag 2.
        assert_eq!(ServerMessage::RequestChildren(None).to_bytes().unwrap(), vec![0, 2]);
        // RequestChildren(Some(id)) -> string payload + Option `Some` flag, then tag 2.
        assert_eq!(
            ServerMessage::RequestChildren(Some("9".into())).to_bytes().unwrap(),
            vec![b'9', 1, 1, 2],
        );
        // RequestNode(id) -> string payload, then tag 3.
        assert_eq!(ServerMessage::RequestNode("9".into()).to_bytes().unwrap(), vec![b'9', 1, 3]);
        // RequestSnapshot(None) -> Option `None` flag, then tag 4.
        assert_eq!(ServerMessage::RequestSnapshot(None).to_bytes().unwrap(), vec![0, 4]);
        // RequestSnapshot(Some(id)) -> string payload + Option `Some` flag, then tag 4.
        assert_eq!(
            ServerMessage::RequestSnapshot(Some("9".into())).to_bytes().unwrap(),
            vec![b'9', 1, 1, 4],
        );
        // Search: a struct variant — fields reversed (query, then from), tag 5 last.
        assert_eq!(
            (ServerMessage::Search { from: "a".into(), query: "b".into() }).to_bytes().unwrap(),
            vec![b'b', 1, b'a', 1, 5],
        );
        // RequestNodes: a Vec<DomId> — elements reversed then VLQ count, tag 6 last.
        assert_eq!(
            ServerMessage::RequestNodes(vec!["a".into(), "b".into()]).to_bytes().unwrap(),
            vec![b'b', 1, b'a', 1, 2, 6],
        );
    }
}