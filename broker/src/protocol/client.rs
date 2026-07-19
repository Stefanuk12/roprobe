use serde::{Deserialize, Serialize};
use tokio_tungstenite::tungstenite::Message;

use crate::{server::SessionId, upstream::Upstream};

use super::{DomPatch, EnumFamily, OpResult};

/// Contains all the possible inbound events from a client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Ask the broker to shut itself down gracefully.
    Shutdown,
    /// Enable or disable the broker's connection to an upstream tool.
    SetUpstream { upstream: Upstream, enabled: bool },
    /// Apply an incremental DOM patch to the session's mirror.
    UpdateDom(DomPatch),
    /// The client's `Enum:GetEnums()` catalog (each family with its items), sent
    /// once on connect so the broker can resolve `DomValue::Enum` family indices
    /// back to Roblox family names and enum options.
    EnumFamilies(Vec<EnumFamily>),
    /// The result of applying a relayed [`super::ServerMessage::Operation`].
    OperationResult { id: u32, result: OpResult },
    /// Ask the broker to make *this* client the active (forwarded) session.
    RequestActive,
    /// Control-only: make the session with the given id active (sent by the `swap` command, not the Luau client).
    SwapActive(SessionId),
    /// Control-only: ask for the connected-session list (sent by the `sessions` command), answered with a [`super::ServerMessage::Sessions`].
    ListSessions,
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
            ClientMessage::SetUpstream { .. } => "set-upstream",
            ClientMessage::UpdateDom(..) => "update-dom",
            ClientMessage::EnumFamilies(..) => "enum-families",
            ClientMessage::OperationResult { .. } => "operation-result",
            ClientMessage::RequestActive => "request-active",
            ClientMessage::SwapActive(..) => "swap-active",
            ClientMessage::ListSessions => "list-sessions",
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
    use std::collections::HashMap;

    use squash::roblox::{
        Axes as WireAxes, Color3 as WireColor3, ColorSequenceKeypoint as WireColorKeypoint,
        Faces as WireFaces, NumberSequenceKeypoint as WireNumberKeypoint, Region3 as WireRegion3,
        Udim as WireUdim, Udim2 as WireUdim2, Vector3 as WireVector3,
    };

    use super::*;
    use crate::protocol::{
        ContentValue, DomBytes, DomInstance, DomPatch, DomUpdate, DomValue, TagChange,
    };

    #[test]
    fn shutdown_round_trips_as_a_single_tag_byte() {
        let bytes = ClientMessage::Shutdown.to_bytes().unwrap();
        assert_eq!(bytes, [0x00]);
        assert!(matches!(
            ClientMessage::from_bytes(bytes).unwrap(),
            ClientMessage::Shutdown
        ));
    }

    #[test]
    fn request_active_is_a_pinned_single_tag_byte() {
        // The Luau client mirrors this tag (frames.luau `clientMessage`); keep it
        // pinned so a reorder can't silently break the takeover message.
        let bytes = ClientMessage::RequestActive.to_bytes().unwrap();
        assert_eq!(bytes, [0x05]);
        assert!(matches!(
            ClientMessage::from_bytes(bytes).unwrap(),
            ClientMessage::RequestActive
        ));
    }

    #[test]
    fn set_upstream_round_trips() {
        let msg = ClientMessage::SetUpstream {
            upstream: Upstream::Verde,
            enabled: false,
        };
        let bytes = msg.to_bytes().unwrap();
        println!("set-upstream frame: {bytes:?}");
        assert!(matches!(
            ClientMessage::from_bytes(bytes).unwrap(),
            ClientMessage::SetUpstream {
                upstream: Upstream::Verde,
                enabled: false,
            }
        ));
    }

    #[test]
    fn enum_families_round_trips_and_is_pinned() {
        use crate::protocol::{EnumEntry, EnumFamily};

        let msg = ClientMessage::EnumFamilies(vec![EnumFamily {
            name: "A".into(),
            items: vec![EnumEntry {
                name: "X".into(),
                value: 1,
            }],
        }]);
        // One family (plain struct: name then items forward), items a Vec of one EnumEntry (name then value forward), then family VLQ count and tag 3.
        #[rustfmt::skip]
        let expected = vec![
            b'A', 1,            // family.name
            b'X', 1,            // items[0].name
            1, 0, 0, 0,         // items[0].value (u32 LE)
            1,                  // items VLQ count
            1,                  // families VLQ count
            3,                  // ClientMessage tag (EnumFamilies)
        ];
        assert_eq!(msg.to_bytes().unwrap(), expected);
        let ClientMessage::EnumFamilies(families) =
            ClientMessage::from_bytes(msg.to_bytes().unwrap()).unwrap()
        else {
            panic!("decoded a different variant");
        };
        assert_eq!(families[0].name, "A");
        assert_eq!(
            families[0].items,
            vec![EnumEntry {
                name: "X".to_string(),
                value: 1
            }]
        );
    }

    /// Pins [`ClientMessage::OperationResult`]: a struct variant (fields reversed — `result` before `id`), tag 4 last, with the `result` payload a typed [`OpResult`] mirrored by hand in the Luau codec.
    #[test]
    fn operation_result_round_trips_and_is_pinned() {
        let msg = ClientMessage::OperationResult {
            id: 1,
            result: OpResult::Ok,
        };
        // OpResult::Ok is a bare unit variant: just its tag byte 0, then id 1
        // (u32 LE), then ClientMessage tag 4.
        assert_eq!(msg.to_bytes().unwrap(), vec![0, 1, 0, 0, 0, 4]);
        let ClientMessage::OperationResult { id, result } =
            ClientMessage::from_bytes(msg.to_bytes().unwrap()).unwrap()
        else {
            panic!("decoded a different variant");
        };
        assert_eq!((id, result), (1, OpResult::Ok));
    }

    #[test]
    fn update_dom_round_trips() {
        let msg = ClientMessage::UpdateDom(DomPatch {
            upserts: vec![
                DomInstance {
                    id: "a1".into(),
                    parent: None,
                    class: "Folder".into(),
                    name: "Stuff".into(),
                    has_children: true,
                    properties: HashMap::new(),
                    attributes: HashMap::new(),
                    tags: None,
                },
                DomInstance {
                    id: "b2".into(),
                    parent: Some("a1".into()),
                    class: "Part".into(),
                    name: "Brick".into(),
                    has_children: false,
                    properties: HashMap::from([("Anchored".to_string(), DomValue::Bool(true))]),
                    attributes: HashMap::from([("Health".to_string(), DomValue::Float(50.0))]),
                    tags: Some(vec!["Enemy".into()]),
                },
                DomInstance {
                    id: "c0".into(),
                    parent: Some("a1".into()),
                    class: "ObjectValue".into(),
                    name: "Pointer".into(),
                    has_children: false,
                    properties: HashMap::from([("Value".to_string(), DomValue::Ref("b2".into()))]),
                    attributes: HashMap::new(),
                    tags: None,
                },
            ],
            removals: vec!["c3".into()],
            updates: vec![],
        });

        let bytes = msg.to_bytes().unwrap();
        let ClientMessage::UpdateDom(patch) = ClientMessage::from_bytes(bytes).unwrap() else {
            panic!("decoded a different variant");
        };
        assert_eq!(patch.upserts.len(), 3);
        assert_eq!(patch.upserts[0].id, "a1");
        assert_eq!(patch.upserts[0].has_children, true);
        assert_eq!(patch.upserts[1].has_children, false);
        assert_eq!(patch.upserts[0].tags, None);
        assert_eq!(patch.upserts[1].parent.as_deref(), Some("a1"));
        assert_eq!(
            patch.upserts[1].properties.get("Anchored"),
            Some(&DomValue::Bool(true))
        );
        assert_eq!(
            patch.upserts[1].attributes.get("Health"),
            Some(&DomValue::Float(50.0))
        );
        assert_eq!(patch.upserts[1].tags, Some(vec!["Enemy".to_string()]));
        assert_eq!(
            patch.upserts[2].properties.get("Value"),
            Some(&DomValue::Ref("b2".into()))
        );
        assert_eq!(patch.removals, vec!["c3".to_string()]);
    }

    /// Pins the wire layout the Luau client mirrors (`client/src/lib/Messages.luau`), which must change with it if this breaks.
    #[test]
    fn update_dom_wire_layout_is_pinned() {
        let msg = ClientMessage::UpdateDom(DomPatch {
            upserts: vec![
                DomInstance {
                    id: "i".into(),
                    parent: Some("p".into()),
                    class: "C".into(),
                    name: "N".into(),
                    has_children: true,
                    properties: HashMap::from([("K".to_string(), DomValue::Int(7))]),
                    attributes: HashMap::from([("A".to_string(), DomValue::Bool(true))]),
                    tags: Some(vec!["t".into()]),
                },
                DomInstance {
                    id: "j".into(),
                    parent: None,
                    class: "D".into(),
                    name: "M".into(),
                    has_children: false,
                    properties: HashMap::new(),
                    attributes: HashMap::new(),
                    tags: None,
                },
            ],
            removals: vec!["r".into(), "s".into()],
            updates: vec![],
        });

        #[rustfmt::skip]
        let expected = vec![
            // upserts: elements reversed on the wire, VLQ count last
            // upserts[1]: plain struct, fields in place, declaration order
            b'j', 1,            // id: bytes + VLQ len
            0,                  // parent: None -> 0x00 flag only
            b'D', 1,            // class
            b'M', 1,            // name
            0,                  // has_children: false
            0,                  // properties: empty map count
            0,                  // attributes: empty map count
            0,                  // tags: None -> 0x00 flag only
            // upserts[0]
            b'i', 1,            // id
            b'p', 1, 1,         // parent: Some -> payload + 0x01 flag
            b'C', 1,            // class
            b'N', 1,            // name
            1,                  // has_children: true
            // properties: per entry value first then key, VLQ count last
            7, 0, 0, 0, 0, 0, 0, 0, // DomValue::Int payload (i64 LE)
            2,                  // DomValue tag (Bool=0, Float=1, Int=2, String=3, Ref=4)
            b'K', 1,            // key
            1,                  // map count
            // attributes: same map layout as properties
            1,                  // DomValue::Bool payload (true)
            0,                  // DomValue tag (Bool)
            b'A', 1,            // key
            1,                  // map count
            // tags: Some(vec) -> vec (elements reversed, count last) + 0x01 flag
            b't', 1,
            1,                  // tags VLQ count
            1,                  // Some flag
            2,                  // upserts VLQ count
            // removals (elements reversed on the wire, count last)
            b's', 1,
            b'r', 1,
            2,                  // removals VLQ count
            0,                  // updates VLQ count (empty)
            2,                  // ClientMessage tag (Shutdown=0, SetUpstream=1, UpdateDom=2, EnumFamilies=3)
        ];
        assert_eq!(msg.to_bytes().unwrap(), expected);

        // DomValue::Ref pinned on its own: payload string then tag byte 4, reproduced by hand in the Luau `Messages.luau` encoder.
        assert_eq!(
            squash::serde_serialize(&DomValue::Ref("9".into())).unwrap(),
            vec![b'9', 1, 4]
        );
    }

    /// Pins a [`DomUpdate`]'s wire layout — the key-removal encoding (`None` values) and the tag-delta variant — mirrored by the Luau `dom-update` check.
    #[test]
    fn dom_update_wire_layout_is_pinned() {
        let update = DomUpdate {
            id: "u".into(),
            properties: HashMap::from([("P".to_string(), Some(DomValue::Bool(false)))]),
            attributes: HashMap::from([("B".to_string(), None)]),
            tags: TagChange::Delta {
                add: vec!["a".into()],
                remove: vec!["r".into()],
            },
        };

        #[rustfmt::skip]
        let expected = vec![
            // plain struct: fields in place, declaration order
            b'u', 1,            // id
            // properties: Option<DomValue> values, per entry value then key
            0,                  // DomValue::Bool payload (false)
            0,                  // DomValue tag (Bool)
            1,                  // Some flag
            b'P', 1,            // key
            1,                  // map count
            // attributes: a None value removes the key
            0,                  // None flag
            b'B', 1,            // key
            1,                  // map count
            // tags: Delta is a struct variant — fields reversed, tag last
            b'r', 1,            // remove[0]
            1,                  // remove VLQ count
            b'a', 1,            // add[0]
            1,                  // add VLQ count
            2,                  // TagChange tag (None=0, Replace=1, Delta=2)
        ];
        assert_eq!(squash::serde_serialize(&update).unwrap(), expected);

        let mut bytes = expected.clone();
        let back: DomUpdate = squash::serde_deserialize(&mut bytes).unwrap();
        assert_eq!(back.id, "u");
        assert_eq!(back.properties.get("P"), Some(&Some(DomValue::Bool(false))));
        assert_eq!(back.attributes.get("B"), Some(&None));
        assert_eq!(
            back.tags,
            TagChange::Delta {
                add: vec!["a".into()],
                remove: vec!["r".into()]
            }
        );

        // The other TagChange arms, pinned standalone.
        assert_eq!(squash::serde_serialize(&TagChange::None).unwrap(), vec![0]);
        assert_eq!(
            squash::serde_serialize(&TagChange::Replace(vec!["t".into()])).unwrap(),
            vec![b't', 1, 1, 1]
        );
    }

    /// Pins every `DomValue` variant's frame (tuple-variant fields land on the wire *reversed*, last field first, tag byte last), which the Luau `Messages.luau` encoder must reproduce by hand.
    #[test]
    fn dom_value_wire_frames_are_pinned() {
        fn le(fields: &[f32], tag: u8) -> Vec<u8> {
            let mut out: Vec<u8> = fields.iter().rev().flat_map(|f| f.to_le_bytes()).collect();
            out.push(tag);
            out
        }

        let cases: Vec<(DomValue, Vec<u8>)> = vec![
            // Enum carries its family index (u16 into the client's GetEnums table):
            // value (u32) reversed-first, then the index, tag last.
            (DomValue::Enum(3, 7), vec![7, 0, 0, 0, 3, 0, 5]),
            (DomValue::Vector2(1.0, 2.5), le(&[1.0, 2.5], 6)),
            (DomValue::Vector3(1.0, 2.5, -3.0), le(&[1.0, 2.5, -3.0], 7)),
            // Color3 now rides squash's u8 codec: `Color3 { b, g, r }` forward, tag last.
            (
                DomValue::Color3(WireColor3 {
                    r: 128,
                    g: 64,
                    b: 255,
                }),
                vec![255, 64, 128, 8],
            ),
            // UDim now rides squash's `Udim`: offset then scale, both f32.
            (
                DomValue::UDim(WireUdim {
                    offset: 10.0,
                    scale: 0.5,
                }),
                [
                    10.0f32.to_le_bytes().to_vec(),
                    0.5f32.to_le_bytes().to_vec(),
                    vec![9],
                ]
                .concat(),
            ),
            (DomValue::NumberRange(1.0, 2.5), le(&[1.0, 2.5], 11)),
            (
                DomValue::Rect(0.0, 0.5, 1.0, 2.5),
                le(&[0.0, 0.5, 1.0, 2.5], 12),
            ),
            (DomValue::BrickColor(194), vec![194, 0, 13]),
            (
                DomValue::CFrame([1.0, 2.0, 3.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]),
                le(
                    &[1.0, 2.0, 3.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
                    14,
                ),
            ),
        ];
        for (value, expected) in cases {
            assert_eq!(
                squash::serde_serialize(&value).unwrap(),
                expected,
                "frame mismatch for {value:?}"
            );
            let mut bytes = expected.clone();
            let back: DomValue = squash::serde_deserialize(&mut bytes).unwrap();
            assert_eq!(back, value);
        }

        // UDim2 is squash's `Udim2 { y, x }`: each `Udim` forward, y before x.
        let udim2 = DomValue::UDim2(WireUdim2 {
            x: WireUdim {
                offset: 10.0,
                scale: 0.5,
            },
            y: WireUdim {
                offset: -2.0,
                scale: 0.25,
            },
        });
        let expected = [
            (-2.0f32).to_le_bytes().to_vec(),
            0.25f32.to_le_bytes().to_vec(),
            10.0f32.to_le_bytes().to_vec(),
            0.5f32.to_le_bytes().to_vec(),
            vec![10],
        ]
        .concat();
        assert_eq!(squash::serde_serialize(&udim2).unwrap(), expected);
        let mut bytes = expected.clone();
        assert_eq!(
            squash::serde_deserialize::<DomValue>(&mut bytes).unwrap(),
            udim2
        );
    }

    /// Same contract as `dom_value_wire_frames_are_pinned`, for the variants past `CFrame` (tags 15..), mirrored by hand in `Messages.luau`.
    #[test]
    fn extended_dom_value_wire_frames_are_pinned() {
        fn f32s(fields: &[f32]) -> Vec<u8> {
            fields.iter().rev().flat_map(|f| f.to_le_bytes()).collect()
        }
        fn i16s(fields: &[i16]) -> Vec<u8> {
            fields.iter().rev().flat_map(|f| f.to_le_bytes()).collect()
        }
        // Forward (non-reversed) f32 bytes: a plain struct's serde fields are
        // written in declaration order, unlike the buffered-and-reversed tuples.
        fn f32f(fields: &[f32]) -> Vec<u8> {
            fields.iter().flat_map(|f| f.to_le_bytes()).collect()
        }

        let identity_at_origin = [1.0, 2.0, 3.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let cases: Vec<(DomValue, Vec<u8>)> = vec![
            (DomValue::Float32(1.5), [f32s(&[1.5]), vec![15]].concat()),
            (
                DomValue::Int32(-7),
                [(-7i32).to_le_bytes().to_vec(), vec![16]].concat(),
            ),
            (DomValue::ContentId("r".into()), vec![b'r', 1, 17]),
            // Raw bytes + VLQ length, like a string but without UTF-8 rules.
            (
                DomValue::BinaryString(DomBytes(vec![0, 255])),
                vec![0, 255, 2, 18],
            ),
            // Color3uint8 is the same squash u8 codec — `Color3 { b, g, r }` forward.
            (
                DomValue::Color3uint8(WireColor3 {
                    r: 255,
                    g: 128,
                    b: 0,
                }),
                vec![0, 128, 255, 19],
            ),
            (
                DomValue::Vector2int16(1, -2),
                [i16s(&[1, -2]), vec![20]].concat(),
            ),
            (
                DomValue::Vector3int16(1, -2, 3),
                [i16s(&[1, -2, 3]), vec![21]].concat(),
            ),
            (
                DomValue::Ray(1.0, 2.0, 3.0, 0.0, 1.0, 0.0),
                [f32s(&[1.0, 2.0, 3.0, 0.0, 1.0, 0.0]), vec![22]].concat(),
            ),
            // Region3 rides squash's center+size codec: size (z,y,x) then position (z,y,x), forward; min (-1,-2,-3)/max (1,2,3) => center (0,0,0), size (2,4,6).
            (
                DomValue::Region3(WireRegion3 {
                    size: WireVector3::new(2.0, 4.0, 6.0),
                    position: WireVector3::new(0.0, 0.0, 0.0),
                }),
                [f32f(&[6.0, 4.0, 2.0, 0.0, 0.0, 0.0]), vec![23]].concat(),
            ),
            (
                DomValue::Region3int16(-1, -2, -3, 1, 2, 3),
                [i16s(&[-1, -2, -3, 1, 2, 3]), vec![24]].concat(),
            ),
            // Axes rides squash's u16 codec (X+Z, with Roblox's coupled faces — Left/Right from X, Back/Front from Z): back+front+left+right = 0x1d low byte, x+z = 0x05 high byte.
            (
                DomValue::Axes(WireAxes {
                    x: true,
                    y: false,
                    z: true,
                    top: false,
                    bottom: false,
                    left: true,
                    right: true,
                    back: true,
                    front: true,
                }),
                vec![29, 5, 25],
            ),
            // Faces rides squash's u8 codec (back=1, bottom=2, front=4, left=8,
            // right=16, top=32): front + top set = 0b100100 = 36.
            (
                DomValue::Faces(WireFaces {
                    back: false,
                    bottom: false,
                    front: true,
                    left: false,
                    right: false,
                    top: true,
                }),
                vec![36, 26],
            ),
            // (family, weight u16, style u8), fields reversed like any tuple variant.
            (
                DomValue::Font("X".into(), 700, 1),
                [
                    vec![1],
                    700u16.to_le_bytes().to_vec(),
                    vec![b'X', 1],
                    vec![27],
                ]
                .concat(),
            ),
            // Vec elements reversed; each keypoint is a `NumberSequenceKeypoint`
            // struct written forward as (value, envelope, time) — the layout of
            // `Squash.NumberSequenceKeypoint(f32)` on the client.
            (
                DomValue::NumberSequence(vec![
                    WireNumberKeypoint {
                        value: 1.0,
                        envelope: 0.0,
                        time: 0.0,
                    },
                    WireNumberKeypoint {
                        value: 0.5,
                        envelope: 0.0,
                        time: 1.0,
                    },
                ]),
                [f32f(&[0.5, 0.0, 1.0]), f32f(&[1.0, 0.0, 0.0]), vec![2, 28]].concat(),
            ),
            // ColorSequence now rides squash's `ColorSequenceKeypoint` (u8 color + u8
            // time), written forward per keypoint; the client mirrors it with
            // `serdeArray(Squash.ColorSequenceKeypoint())`.
            (
                DomValue::ColorSequence(vec![WireColorKeypoint {
                    value: WireColor3 {
                        r: 10,
                        g: 20,
                        b: 30,
                    },
                    time: 128.0,
                }]),
                vec![30, 20, 10, 128, 1, 29],
            ),
            (DomValue::PhysicalProperties(None), vec![0, 30]),
            // [density, friction, elasticity, frictionWeight, elasticityWeight,
            // acousticAbsorption], reversed like any array, then Some flag + tag.
            (
                DomValue::PhysicalProperties(Some([0.7, 0.3, 0.5, 0.6, 0.8, 0.9])),
                [f32s(&[0.7, 0.3, 0.5, 0.6, 0.8, 0.9]), vec![1, 30]].concat(),
            ),
            (DomValue::OptionalCFrame(None), vec![0, 31]),
            (
                DomValue::OptionalCFrame(Some(identity_at_origin)),
                [f32s(&identity_at_origin), vec![1, 31]].concat(),
            ),
            // Content nests its own enum (payload + inner tag, then outer tag), and Uri's source is optional: Some adds a 0x01 flag after the string, None collapses to a lone 0x00.
            (DomValue::Content(ContentValue::None), vec![0, 32]),
            (
                DomValue::Content(ContentValue::Uri(Some("u".into()))),
                vec![b'u', 1, 1, 1, 32],
            ),
            (DomValue::Content(ContentValue::Uri(None)), vec![0, 1, 32]),
            (
                DomValue::Content(ContentValue::Object("5".into())),
                vec![b'5', 1, 2, 32],
            ),
        ];
        for (value, expected) in cases {
            assert_eq!(
                squash::serde_serialize(&value).unwrap(),
                expected,
                "frame mismatch for {value:?}"
            );
            let mut bytes = expected.clone();
            let back: DomValue = squash::serde_deserialize(&mut bytes).unwrap();
            assert_eq!(back, value);
        }
    }
}
