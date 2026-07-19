use serde::{Deserialize, Serialize};
use squash::ReverseDeserialize;

use super::{DomId, DomValue};

/// One operation the client applies against the live game.
#[derive(Clone, PartialEq, PartialOrd, Debug, Serialize, Deserialize)]
pub enum Operation {
    Rename { node: DomId, name: String },
    Delete { node: DomId },
    Move { node: DomId, parent: Option<DomId> },
    Create { parent: DomId, class: String },
    AddTag { node: DomId, tag: String },
    RemoveTag { node: DomId, tag: String },
    SetProperty { node: DomId, name: String, value: DomValue },
    SetAttribute { node: DomId, name: String, value: DomValue },
    RemoveAttribute { node: DomId, name: String },
    RenameAttribute { node: DomId, old: String, new: String },
    /// `properties` is the exact list of property names the client should read (filled from the API dump by the node's class), empty when the class is unknown so the client falls back to its own enumeration.
    GetProperties { node: DomId, properties: Vec<String> },
    RunCode { source: String },
}

/// The outcome the client reports for a relayed [`Operation`].
#[derive(Clone, PartialEq, PartialOrd, Debug, Serialize, Deserialize)]
pub enum OpResult {
    Ok,
    Reads(Reads),
    Err(String),
    Output(String),
}

/// A node's raw properties, tags, and attributes.
#[derive(Clone, PartialEq, PartialOrd, Debug, Serialize, ReverseDeserialize)]
pub struct Reads {
    pub properties: Vec<NamedValue>,
    pub tags: Vec<String>,
    pub attributes: Vec<NamedValue>,
}

/// One `name -> value` read.
#[derive(Clone, PartialEq, PartialOrd, Debug, Serialize, ReverseDeserialize)]
pub struct NamedValue {
    pub name: String,
    pub value: DomValue,
}

/// Used so the broker can have a record of a session's possible enums.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default, Serialize, ReverseDeserialize)]
pub struct EnumFamily {
    pub name: String,
    pub items: Vec<EnumEntry>,
}

/// One `{ name, value }` item of an [`EnumFamily`].
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default, Serialize, ReverseDeserialize)]
pub struct EnumEntry {
    pub name: String,
    pub value: u32,
}

impl EnumFamily {
    /// The item whose value is `value`, if any (for resolving an enum's name).
    pub fn item(&self, value: u32) -> Option<&EnumEntry> {
        self.items.iter().find(|entry| entry.value == value)
    }
}

impl Operation {
    /// Encode into a Squash binary frame (test/relay helper).
    #[cfg(test)]
    pub fn to_bytes(&self) -> squash::Result<Vec<u8>> {
        squash::serde_serialize(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operations_round_trip() {
        let cases = vec![
            Operation::Rename { node: "n".into(), name: "New".into() },
            Operation::Delete { node: "n".into() },
            Operation::Move { node: "n".into(), parent: Some("p".into()) },
            Operation::Move { node: "n".into(), parent: None },
            Operation::Create { parent: "p".into(), class: "Part".into() },
            Operation::AddTag { node: "n".into(), tag: "Enemy".into() },
            Operation::RemoveTag { node: "n".into(), tag: "Enemy".into() },
            Operation::SetProperty { node: "n".into(), name: "Anchored".into(), value: DomValue::Bool(true) },
            Operation::SetAttribute { node: "n".into(), name: "Health".into(), value: DomValue::Float(50.0) },
            Operation::RemoveAttribute { node: "n".into(), name: "Health".into() },
            Operation::RenameAttribute { node: "n".into(), old: "A".into(), new: "B".into() },
            Operation::GetProperties { node: "n".into(), properties: vec!["Anchored".into(), "Name".into()] },
            Operation::RunCode { source: "print('hi')".into() },
        ];
        for op in cases {
            let mut bytes = squash::serde_serialize(&op).unwrap();
            let back: Operation = squash::serde_deserialize(&mut bytes).unwrap();
            assert_eq!(back, op);
        }
    }

    #[test]
    fn op_results_round_trip() {
        let cases = vec![
            OpResult::Ok,
            OpResult::Err("unknown_node".into()),
            OpResult::Output("hi\n42".into()),
            OpResult::Reads(Reads {
                properties: vec![
                    NamedValue { name: "Anchored".into(), value: DomValue::Bool(true) },
                    NamedValue { name: "PrimaryPart".into(), value: DomValue::Ref("ref".into()) },
                    NamedValue { name: "Shape".into(), value: DomValue::Enum(3, 1) },
                ],
                tags: vec!["Enemy".into()],
                attributes: vec![NamedValue { name: "Health".into(), value: DomValue::Float(50.0) }],
            }),
        ];
        for result in cases {
            let mut bytes = squash::serde_serialize(&result).unwrap();
            let back: OpResult = squash::serde_deserialize(&mut bytes).unwrap();
            assert_eq!(back, result);
        }
    }

    /// Pins the frames the Luau `operation` codec reproduces by hand: struct
    /// variants land reversed, the u8 tag last.
    #[test]
    fn operation_wire_layouts_are_pinned() {
        // GetProperties { node, properties }: struct variant, fields reversed —
        // the properties Vec (elements reversed + VLQ count) first, then node, tag 10.
        assert_eq!(
            (Operation::GetProperties { node: "n".into(), properties: vec!["p".into()] }).to_bytes().unwrap(),
            vec![b'p', 1, 1, b'n', 1, 10],
        );
        // RunCode: single string field, tag 11 last.
        assert_eq!((Operation::RunCode { source: "s".into() }).to_bytes().unwrap(), vec![b's', 1, 11]);
        // Rename: fields reversed (name, node), tag 0.
        assert_eq!(
            (Operation::Rename { node: "n".into(), name: "X".into() }).to_bytes().unwrap(),
            vec![b'X', 1, b'n', 1, 0],
        );
        // Move with no parent: Option None flag, then node, tag 2.
        assert_eq!((Operation::Move { node: "n".into(), parent: None }).to_bytes().unwrap(), vec![0, b'n', 1, 2]);
        // SetProperty carrying DomValue::Bool(true): value (payload 1 + DomValue
        // tag 0) reversed-first, then name, then node, then Operation tag 6.
        assert_eq!(
            (Operation::SetProperty { node: "n".into(), name: "A".into(), value: DomValue::Bool(true) })
                .to_bytes()
                .unwrap(),
            vec![1, 0, b'A', 1, b'n', 1, 6],
        );
    }

    /// Pins the [`OpResult`] frames the Luau `opResult` codec reproduces.
    #[test]
    fn op_result_wire_layouts_are_pinned() {
        let ser = |r: &OpResult| squash::serde_serialize(r).unwrap();

        assert_eq!(ser(&OpResult::Ok), vec![0]);
        assert_eq!(ser(&OpResult::Err("x".into())), vec![b'x', 1, 2]);
        assert_eq!(ser(&OpResult::Output("x".into())), vec![b'x', 1, 3]);

        // Reads { properties, tags, attributes } is a plain struct in a tuple variant (tag 1), fields forward with each vec reversed + VLQ count, and the one NamedValue { name, value } a plain struct: name then value forward.
        let result = OpResult::Reads(Reads {
            properties: vec![NamedValue { name: "A".into(), value: DomValue::Bool(true) }],
            tags: vec!["t".into()],
            attributes: vec![],
        });
        assert_eq!(
            ser(&result),
            vec![
                b'A', 1, // NamedValue.name
                1, 0,    // NamedValue.value: DomValue::Bool(true) -> payload 1, tag 0
                1,       // properties VLQ count
                b't', 1, 1, // tags "t" + count
                0,       // attributes VLQ count
                1,       // OpResult tag (Reads)
            ],
        );
    }
}
