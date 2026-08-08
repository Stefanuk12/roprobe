use serde::{Deserialize, Serialize};
use squash::ReverseDeserialize;

use super::{DomBytes, DomId, DomValue};

/// Which way a captured call crossed the network boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteDirection {
    /// The client called the server (`FireServer`, `InvokeServer`, `Fire`, `Invoke`).
    Outgoing,
    /// The server called the client (`OnClientEvent`, `OnClientInvoke`, `Event`, `OnInvoke`).
    Incoming,
}

/// A remote a call crossed, or an instance one of its arguments referenced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ReverseDeserialize)]
pub struct InstanceRef {
    pub id: DomId,
    pub class: String,
    pub name: String,
    /// `GetFullName()`, or the name alone when that call is blocked.
    pub path: String,
}

/// The call site a captured call came from.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, ReverseDeserialize)]
pub struct CallSource {
    /// `getcallingscript()`'s full name, absent when the caller has no script.
    pub script: Option<String>,
    /// The calling function's name from `debug.info`, absent for anonymous ones.
    pub function_name: Option<String>,
    /// The chunk name from `debug.info`, which need not be a real instance path.
    pub chunk: Option<String>,
    pub line: Option<u32>,
    /// Whether `checkcaller()` says our own executor thread made the call.
    pub is_executor: bool,
    /// The `Actor` a call was captured under, when it came from an actor VM.
    pub actor: Option<String>,
}

/// A function value, which cannot cross the wire, described well enough to find it.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, ReverseDeserialize)]
pub struct FunctionRef {
    pub name: Option<String>,
    pub chunk: Option<String>,
    pub line: Option<u32>,
}

/// One `key -> value` pair of a captured table's hash part.
#[derive(Debug, Clone, PartialEq, Serialize, ReverseDeserialize)]
pub struct LuaEntry {
    pub key: LuaValue,
    pub value: LuaValue,
}

/// A captured Luau table, split the way Luau stores one.
#[derive(Debug, Clone, PartialEq, Default, Serialize, ReverseDeserialize)]
pub struct LuaTable {
    /// Per-call ordinal a [`LuaValue::Cycle`] in the same call points back at.
    pub id: u32,
    /// The contiguous `1..n` part, in order.
    pub array: Vec<LuaValue>,
    /// Everything else, in whatever order `pairs` yielded it.
    pub entries: Vec<LuaEntry>,
    /// Whether the capture caps (depth, entry count) dropped part of this table.
    pub truncated: bool,
    /// Whether the table carried a metatable, which is not captured.
    pub metatable: bool,
}

/// One argument (or return value) of a captured call.
///
/// Roblox datatypes ride the existing [`DomValue`] rather than being restated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LuaValue {
    Nil,
    Bool(bool),
    Number(f64),
    /// Arbitrary bytes: a Luau string is not required to be UTF-8, and a packed
    /// binary payload is exactly the case worth reading.
    String(DomBytes),
    Table(LuaTable),
    Instance(InstanceRef),
    /// Any Roblox datatype
    Roblox(DomValue),
    Buffer(DomBytes),
    Function(FunctionRef),
    /// A back-reference to the [`LuaTable::id`] of a table already sent in this call.
    Cycle(u32),
    /// Anything else (`thread`, foreign userdata), by `typeof`.
    Opaque(String),
}

impl LuaValue {
    /// The variant's name, for logging a value without dumping its payload.
    pub fn kind(&self) -> &'static str {
        match self {
            LuaValue::Nil => "nil",
            LuaValue::Bool(..) => "bool",
            LuaValue::Number(..) => "number",
            LuaValue::String(..) => "string",
            LuaValue::Table(..) => "table",
            LuaValue::Instance(..) => "instance",
            LuaValue::Roblox(..) => "roblox",
            LuaValue::Buffer(..) => "buffer",
            LuaValue::Function(..) => "function",
            LuaValue::Cycle(..) => "cycle",
            LuaValue::Opaque(..) => "opaque",
        }
    }
}

/// One captured remote call, as the client saw it.
#[derive(Debug, Clone, PartialEq, Serialize, ReverseDeserialize)]
pub struct RemoteCall {
    /// Per-session, monotonic.
    pub id: u32,
    pub direction: RemoteDirection,
    pub remote: InstanceRef,
    /// The method that carried it (`FireServer`, `OnClientEvent`, etc).
    pub method: String,
    pub arguments: Vec<LuaValue>,
    /// What a `RemoteFunction`/`BindableFunction` answered with; `None` for events
    /// and for an invoke whose result was never seen.
    pub returns: Option<Vec<LuaValue>>,
    pub source: CallSource,
    /// The client's `os.clock()` at capture, which shares no epoch with ours.
    pub timestamp: f64,
}

/// What the client should capture, pushed to each session as it connects and
/// whenever the extension changes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ReverseDeserialize)]
pub struct SpyConfig {
    /// Whether to hook at all.
    pub enabled: bool,
    /// Capture `FireServer`/`InvokeServer` (and the bindable equivalents).
    pub outgoing: bool,
    /// Capture `OnClientEvent`/`OnClientInvoke` (and the bindable equivalents).
    pub incoming: bool,
    /// Capture `BindableEvent`/`BindableFunction` alongside the networked ones.
    pub bindables: bool,
    /// How deep to walk a table argument before truncating.
    pub max_depth: u8,
    /// How many entries of one table to capture before truncating.
    pub max_entries: u16,
    /// How many bytes of one string or buffer to capture.
    pub max_bytes: u32,
}

impl Default for SpyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            outgoing: true,
            incoming: true,
            bindables: false,
            max_depth: 6,
            max_entries: 128,
            max_bytes: 4096,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance() -> InstanceRef {
        InstanceRef {
            id: "i".into(),
            class: "RemoteEvent".into(),
            name: "N".into(),
            path: "P".into(),
        }
    }

    /// Pins [`InstanceRef`]: a plain struct, so its fields land in declaration
    /// order, which the Luau and TypeScript codecs reproduce by hand.
    #[test]
    fn instance_ref_wire_layout_is_pinned() {
        #[rustfmt::skip]
        let expected = vec![
            b'i', 1,            // id
            b'R', b'e', b'm', b'o', b't', b'e', b'E', b'v', b'e', b'n', b't', 11, // class
            b'N', 1,            // name
            b'P', 1,            // path
        ];
        assert_eq!(squash::serde_serialize(&instance()).unwrap(), expected);

        let mut bytes = expected;
        let back: InstanceRef = squash::serde_deserialize(&mut bytes).unwrap();
        assert_eq!(back, instance());
    }

    /// Pins [`CallSource`]: a plain struct of four `Option`s around a `bool`, so a
    /// missing field collapses to a lone `0x00` flag.
    #[test]
    fn call_source_wire_layout_is_pinned() {
        let source = CallSource {
            script: Some("s".into()),
            function_name: None,
            chunk: None,
            line: Some(7),
            is_executor: true,
            actor: None,
        };
        #[rustfmt::skip]
        let expected = vec![
            b's', 1, 1,         // script: Some -> payload + 0x01 flag
            0,                  // function_name: None
            0,                  // chunk: None
            7, 0, 0, 0, 1,      // line: Some(7) -> u32 LE + 0x01 flag
            1,                  // is_executor
            0,                  // actor: None
        ];
        assert_eq!(squash::serde_serialize(&source).unwrap(), expected);

        let mut bytes = expected;
        let back: CallSource = squash::serde_deserialize(&mut bytes).unwrap();
        assert_eq!(back, source);
    }

    /// Pins every [`LuaValue`] variant's frame. Single-field tuple variants write
    /// the payload then the tag byte; the nested structs land forward inside it.
    #[test]
    fn lua_value_wire_frames_are_pinned() {
        let cases: Vec<(LuaValue, Vec<u8>)> = vec![
            (LuaValue::Nil, vec![0]),
            (LuaValue::Bool(true), vec![1, 1]),
            (
                LuaValue::Number(1.5),
                [1.5f64.to_le_bytes().to_vec(), vec![2]].concat(),
            ),
            // A string rides raw bytes + a VLQ length, so a non-UTF-8 payload survives.
            (LuaValue::String(DomBytes(vec![0, 255])), vec![0, 255, 2, 3]),
            (
                LuaValue::Instance(instance()),
                [
                    squash::serde_serialize(&instance()).unwrap(),
                    vec![5],
                ]
                .concat(),
            ),
            // A Roblox datatype nests DomValue whole: its payload, its own tag, then ours.
            (LuaValue::Roblox(DomValue::Bool(true)), vec![1, 0, 6]),
            (LuaValue::Buffer(DomBytes(vec![7])), vec![7, 1, 7]),
            (
                LuaValue::Function(FunctionRef {
                    name: Some("f".into()),
                    chunk: None,
                    line: None,
                }),
                vec![b'f', 1, 1, 0, 0, 8],
            ),
            (LuaValue::Cycle(3), vec![3, 0, 0, 0, 9]),
            (LuaValue::Opaque("thread".into()), vec![b't', b'h', b'r', b'e', b'a', b'd', 6, 10]),
        ];
        for (value, expected) in cases {
            assert_eq!(
                squash::serde_serialize(&value).unwrap(),
                expected,
                "frame mismatch for {}",
                value.kind()
            );
            let mut bytes = expected.clone();
            let back: LuaValue = squash::serde_deserialize(&mut bytes).unwrap();
            assert_eq!(back, value);
        }
    }

    /// Pins [`LuaTable`]: a plain struct, so its fields land forward, with each
    /// `Vec` reversed and counted last the way squash writes one.
    #[test]
    fn lua_table_wire_layout_is_pinned() {
        let table = LuaTable {
            id: 1,
            array: vec![LuaValue::Bool(false)],
            entries: vec![LuaEntry {
                key: LuaValue::String(DomBytes(b"k".to_vec())),
                value: LuaValue::Nil,
            }],
            truncated: true,
            metatable: false,
        };
        #[rustfmt::skip]
        let expected = vec![
            1, 0, 0, 0,         // id (u32 LE)
            0, 1,               // array[0]: LuaValue::Bool(false)
            1,                  // array VLQ count
            b'k', 1, 3,         // entries[0].key: LuaValue::String("k")
            0,                  // entries[0].value: LuaValue::Nil
            1,                  // entries VLQ count
            1,                  // truncated
            0,                  // metatable
        ];
        assert_eq!(squash::serde_serialize(&table).unwrap(), expected);

        let mut bytes = expected;
        let back: LuaTable = squash::serde_deserialize(&mut bytes).unwrap();
        assert_eq!(back, table);

        // Nested whole inside the LuaValue::Table variant, tag 4 last.
        assert_eq!(
            squash::serde_serialize(&LuaValue::Table(LuaTable::default())).unwrap(),
            vec![0, 0, 0, 0, 0, 0, 0, 0, 4],
        );
    }

    /// Pins [`RemoteCall`]: a plain struct, fields forward, nesting the shapes above.
    #[test]
    fn remote_call_wire_layout_is_pinned() {
        let call = RemoteCall {
            id: 2,
            direction: RemoteDirection::Incoming,
            remote: instance(),
            method: "M".into(),
            arguments: vec![LuaValue::Nil],
            returns: None,
            source: CallSource::default(),
            timestamp: 0.0,
        };
        #[rustfmt::skip]
        let expected = [
            vec![2, 0, 0, 0],                           // id (u32 LE)
            vec![1],                                    // direction (Outgoing=0, Incoming=1)
            squash::serde_serialize(&instance()).unwrap(),
            vec![b'M', 1],                              // method
            vec![0, 1],                                 // arguments: [Nil], VLQ count
            vec![0],                                    // returns: None
            vec![0, 0, 0, 0, 0, 0],                     // source: four Nones, false, None
            0.0f64.to_le_bytes().to_vec(),              // timestamp
        ]
        .concat();
        assert_eq!(squash::serde_serialize(&call).unwrap(), expected);

        let mut bytes = expected;
        let back: RemoteCall = squash::serde_deserialize(&mut bytes).unwrap();
        assert_eq!(back, call);
    }

    /// A call carrying every value shape, round-tripped rather than pinned: the
    /// pins above cover the layout, this covers them composing.
    #[test]
    fn a_call_carrying_every_value_shape_round_trips() {
        let call = RemoteCall {
            id: u32::MAX,
            direction: RemoteDirection::Outgoing,
            remote: instance(),
            method: "InvokeServer".into(),
            arguments: vec![
                LuaValue::Nil,
                LuaValue::Bool(false),
                LuaValue::Number(-0.5),
                LuaValue::String(DomBytes(vec![0, 128, 255])),
                LuaValue::Table(LuaTable {
                    id: 0,
                    array: vec![LuaValue::Number(1.0), LuaValue::Cycle(0)],
                    entries: vec![LuaEntry {
                        key: LuaValue::Roblox(DomValue::Vector3(1.0, 2.0, 3.0)),
                        value: LuaValue::Instance(instance()),
                    }],
                    truncated: false,
                    metatable: true,
                }),
                LuaValue::Buffer(DomBytes(vec![1, 2, 3])),
                LuaValue::Function(FunctionRef {
                    name: Some("callback".into()),
                    chunk: Some("Script".into()),
                    line: Some(12),
                }),
                LuaValue::Opaque("thread".into()),
            ],
            returns: Some(vec![LuaValue::Bool(true)]),
            source: CallSource {
                script: Some("Players.X.PlayerScripts.Main".into()),
                function_name: Some("send".into()),
                chunk: Some("Main".into()),
                line: Some(40),
                is_executor: false,
                actor: Some("Actor".into()),
            },
            timestamp: 1234.5,
        };

        let mut bytes = squash::serde_serialize(&call).unwrap();
        let back: RemoteCall = squash::serde_deserialize(&mut bytes).unwrap();
        assert_eq!(back, call);
    }

    /// Pins [`SpyConfig`]: a plain struct of flags and caps, fields forward.
    #[test]
    fn spy_config_wire_layout_is_pinned() {
        let config = SpyConfig {
            enabled: true,
            ..SpyConfig::default()
        };
        #[rustfmt::skip]
        let expected = vec![
            1,                  // enabled
            1,                  // outgoing
            1,                  // incoming
            0,                  // bindables
            6,                  // max_depth (u8)
            128, 0,             // max_entries (u16 LE)
            0, 16, 0, 0,        // max_bytes (u32 LE)
        ];
        assert_eq!(squash::serde_serialize(&config).unwrap(), expected);

        let mut bytes = expected;
        let back: SpyConfig = squash::serde_deserialize(&mut bytes).unwrap();
        assert_eq!(back, config);
    }
}
