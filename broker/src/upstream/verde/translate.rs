// ai assisted cos no way am i writing all of this

use serde::{Deserialize, Serialize};
use squash::roblox::{Color3 as WireColor3, Udim as WireUdim, Udim2 as WireUdim2};

use super::protocol::OperationOutcome;
use crate::{
    protocol::{DomValue, EnumFamily, NamedValue, OpResult, Operation, Reads},
    server::Mirror,
};

// ===========================================================================
// Inbound: verde JSON -> generic Operation
// ===========================================================================

/// Parse one verde `operation` object into a generic [`Operation`], or `None` if
/// the tag is unknown or a required field is missing/mistyped. The wire shape is
/// deserialized into [`RawOperation`] and then lowered; a deserialize failure
/// (bad tag, missing/mistyped field, unrecognisable value) yields `None`.
pub fn to_operation(operation: &serde_json::Value) -> Option<Operation> {
    RawOperation::deserialize(operation).ok().and_then(lower_operation)
}

/// The verde operation wire format: an internally-tagged union on `type`, with
/// each variant's fields named exactly as verde sends them.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum RawOperation {
    RenameInstance {
        #[serde(rename = "nodeId")]
        node_id: String,
        #[serde(rename = "newName")]
        new_name: String,
    },
    DeleteInstance {
        #[serde(rename = "nodeId")]
        node_id: String,
    },
    // An absent / null newParentId is a detach, not a failure — Option handles both.
    MoveNode {
        #[serde(rename = "nodeId")]
        node_id: String,
        #[serde(rename = "newParentId")]
        new_parent_id: Option<String>,
    },
    CreateInstance {
        #[serde(rename = "parentId")]
        parent_id: String,
        #[serde(rename = "className")]
        class_name: String,
    },
    AddTag {
        #[serde(rename = "nodeId")]
        node_id: String,
        #[serde(rename = "tagName")]
        tag_name: String,
    },
    RemoveTag {
        #[serde(rename = "nodeId")]
        node_id: String,
        #[serde(rename = "tagName")]
        tag_name: String,
    },
    SetProperty {
        #[serde(rename = "nodeId")]
        node_id: String,
        #[serde(rename = "propertyName")]
        property_name: String,
        #[serde(rename = "propertyValue")]
        property_value: RawValue,
    },
    SetAttribute {
        #[serde(rename = "nodeId")]
        node_id: String,
        #[serde(rename = "attributeName")]
        attribute_name: String,
        #[serde(rename = "attributeValue")]
        attribute_value: RawValue,
    },
    AddAttribute {
        #[serde(rename = "nodeId")]
        node_id: String,
        #[serde(rename = "attributeName")]
        attribute_name: String,
        #[serde(rename = "attributeType")]
        attribute_type: String,
    },
    RemoveAttribute {
        #[serde(rename = "nodeId")]
        node_id: String,
        #[serde(rename = "attributeName")]
        attribute_name: String,
    },
    RenameAttribute {
        #[serde(rename = "nodeId")]
        node_id: String,
        #[serde(rename = "oldName")]
        old_name: String,
        #[serde(rename = "newName")]
        new_name: String,
    },
    GetProperties {
        #[serde(rename = "nodeId")]
        node_id: String,
    },
    // Verde's console/command bar, accepting either field name for the source.
    RunCode {
        #[serde(rename = "code", alias = "source")]
        code: String,
    },
}

/// Lower a parsed [`RawOperation`] to the generic [`Operation`]. Total except for
/// `add_attribute`, whose type string may name no known default.
fn lower_operation(raw: RawOperation) -> Option<Operation> {
    Some(match raw {
        RawOperation::RenameInstance { node_id, new_name } => Operation::Rename {
            node: node_id,
            name: new_name,
        },
        RawOperation::DeleteInstance { node_id } => Operation::Delete { node: node_id },
        RawOperation::MoveNode {
            node_id,
            new_parent_id,
        } => Operation::Move {
            node: node_id,
            parent: new_parent_id,
        },
        RawOperation::CreateInstance {
            parent_id,
            class_name,
        } => Operation::Create {
            parent: parent_id,
            class: class_name,
        },
        RawOperation::AddTag { node_id, tag_name } => Operation::AddTag {
            node: node_id,
            tag: tag_name,
        },
        RawOperation::RemoveTag { node_id, tag_name } => Operation::RemoveTag {
            node: node_id,
            tag: tag_name,
        },
        RawOperation::SetProperty {
            node_id,
            property_name,
            property_value,
        } => Operation::SetProperty {
            node: node_id,
            name: property_name,
            value: property_value.into_dom(),
        },
        RawOperation::SetAttribute {
            node_id,
            attribute_name,
            attribute_value,
        } => Operation::SetAttribute {
            node: node_id,
            name: attribute_name,
            value: attribute_value.into_dom(),
        },
        // Adding an attribute is a set with the type's default value.
        RawOperation::AddAttribute {
            node_id,
            attribute_name,
            attribute_type,
        } => Operation::SetAttribute {
            node: node_id,
            name: attribute_name,
            value: attribute_default(&attribute_type)?,
        },
        RawOperation::RemoveAttribute {
            node_id,
            attribute_name,
        } => Operation::RemoveAttribute {
            node: node_id,
            name: attribute_name,
        },
        RawOperation::RenameAttribute {
            node_id,
            old_name,
            new_name,
        } => Operation::RenameAttribute {
            node: node_id,
            old: old_name,
            new: new_name,
        },
        // The property list is filled from the API dump by the broker (it knows
        // the node's class); the client just reads the given names.
        RawOperation::GetProperties { node_id } => Operation::GetProperties {
            node: node_id,
            properties: Vec::new(),
        },
        RawOperation::RunCode { code } => Operation::RunCode { source: code },
    })
}

/// The default [`DomValue`] for a new attribute of verde's `attributeType`.
fn attribute_default(kind: &str) -> Option<DomValue> {
    Some(match kind {
        "string" => DomValue::String(String::new()),
        "number" => DomValue::Float(0.0),
        "boolean" => DomValue::Bool(false),
        _ => return None,
    })
}

/// A verde value by shape. Untagged: serde tries each variant top-to-bottom and
/// takes the first that fits, so the declaration order below *is* the shape
/// precedence (UDim2 before the numeric vectors, NumberRange before Rect, etc.),
/// matching what the old hand-written `if let` chain did.
#[derive(Deserialize)]
#[serde(untagged)]
enum RawValue {
    Bool(bool),
    // Any JSON number (int or float) lands here as f64, as the old `as_f64` did.
    Number(f64),
    Text(String),
    Shape(RawShape),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawShape {
    // UDim2's X/Y are objects; must precede the numeric Vector shapes.
    UDim2 {
        #[serde(rename = "X")]
        x: RawUDim,
        #[serde(rename = "Y")]
        y: RawUDim,
    },
    Vector3 {
        #[serde(rename = "X")]
        x: f64,
        #[serde(rename = "Y")]
        y: f64,
        #[serde(rename = "Z")]
        z: f64,
    },
    Vector2 {
        #[serde(rename = "X")]
        x: f64,
        #[serde(rename = "Y")]
        y: f64,
    },
    Color3 {
        #[serde(rename = "R")]
        r: f64,
        #[serde(rename = "G")]
        g: f64,
        #[serde(rename = "B")]
        b: f64,
    },
    UDim {
        #[serde(rename = "Scale")]
        scale: f64,
        #[serde(rename = "Offset")]
        offset: f64,
    },
    // Numeric Min/Max — must precede the Rect shape whose Min/Max are objects.
    NumberRange {
        #[serde(rename = "Min")]
        min: f64,
        #[serde(rename = "Max")]
        max: f64,
    },
    // A Rect: Min/Max are Vector2-shaped objects (mirrors `describe`'s Rect shape).
    Rect {
        #[serde(rename = "Min")]
        min: RawVec2,
        #[serde(rename = "Max")]
        max: RawVec2,
    },
    // An enum object: hand the client a scalar it can resolve to the live enum.
    // Name is checked before Value, matching the old order.
    EnumByName {
        #[serde(rename = "Name")]
        name: String,
    },
    EnumByValue {
        #[serde(rename = "Value")]
        value: f64,
    },
}

#[derive(Deserialize)]
struct RawUDim {
    #[serde(rename = "Scale")]
    scale: f64,
    #[serde(rename = "Offset")]
    offset: f64,
}

#[derive(Deserialize)]
struct RawVec2 {
    #[serde(rename = "X")]
    x: f64,
    #[serde(rename = "Y")]
    y: f64,
}

impl RawValue {
    fn into_dom(self) -> DomValue {
        match self {
            RawValue::Bool(b) => DomValue::Bool(b),
            RawValue::Number(n) => DomValue::Float(n),
            RawValue::Text(s) => DomValue::String(s),
            RawValue::Shape(shape) => shape.into_dom(),
        }
    }
}

impl RawShape {
    fn into_dom(self) -> DomValue {
        match self {
            RawShape::UDim2 { x, y } => DomValue::UDim2(WireUdim2 {
                x: WireUdim {
                    scale: x.scale as f32,
                    offset: x.offset as f32,
                },
                y: WireUdim {
                    scale: y.scale as f32,
                    offset: y.offset as f32,
                },
            }),
            RawShape::Vector3 { x, y, z } => DomValue::Vector3(x as f32, y as f32, z as f32),
            RawShape::Vector2 { x, y } => DomValue::Vector2(x as f32, y as f32),
            RawShape::Color3 { r, g, b } => DomValue::Color3(WireColor3 {
                r: to_u8(r),
                g: to_u8(g),
                b: to_u8(b),
            }),
            RawShape::UDim { scale, offset } => DomValue::UDim(WireUdim {
                scale: scale as f32,
                offset: offset as f32,
            }),
            RawShape::NumberRange { min, max } => DomValue::NumberRange(min as f32, max as f32),
            RawShape::Rect { min, max } => {
                DomValue::Rect(min.x as f32, min.y as f32, max.x as f32, max.y as f32)
            }
            RawShape::EnumByName { name } => DomValue::String(name),
            RawShape::EnumByValue { value } => DomValue::Float(value),
        }
    }
}

// ===========================================================================
// Outbound: OpResult -> verde operation_result
// ===========================================================================

/// Render the client's raw [`OpResult`] into verde's `operation_result` shape,
/// resolving references and enums against `mirror`, with `class` the read node's
/// class name used to attach inspector metadata (category / read-only / order).
pub fn to_outcome(result: OpResult, mirror: &Mirror, class: Option<&str>) -> OperationOutcome {
    match result {
        OpResult::Ok => OperationOutcome {
            success: true,
            error: None,
            data: None,
        },
        OpResult::Err(code) => OperationOutcome {
            success: false,
            error: Some(code),
            data: None,
        },
        OpResult::Reads(reads) => OperationOutcome {
            success: true,
            error: None,
            data: Some(to_json(&render_reads(reads, mirror, class))),
        },
        OpResult::Output(output) => OperationOutcome {
            success: true,
            error: None,
            data: Some(to_json(&OutputData { output })),
        },
    }
}

/// Serialize an outbound payload into the `Value` that [`OperationOutcome::data`]
/// carries. The structs below only hold JSON-representable data, so this cannot
/// fail in practice.
fn to_json<T: Serialize>(value: &T) -> serde_json::Value {
    serde_json::to_value(value).expect("operation_result payload is always serializable")
}

/// The `data` payload for a `run_code` result.
#[derive(Serialize)]
struct OutputData {
    output: String,
}

/// The `data` payload for a read: property/attribute lists plus tags.
#[derive(Serialize)]
struct ReadsData {
    properties: Vec<Property>,
    tags: Vec<String>,
    attributes: Vec<Attribute>,
}

/// A property object for verde's inspector: the shared fields plus, flattened in,
/// whatever extra keys the value type needs (instance-ref / enum metadata).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Property {
    name: String,
    #[serde(rename = "type")]
    type_name: String,
    category: String,
    layout_order: u32,
    is_read_only: bool,
    value: RenderValue,
    // Exactly one of these is populated for ref / enum values; both `None`
    // otherwise. Flatten spreads the inner keys into this object (or nothing).
    #[serde(flatten)]
    reference: Option<InstanceRef>,
    #[serde(flatten)]
    enumeration: Option<EnumMeta>,
}

/// Attributes are always plain values — no category/refs/enums.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Attribute {
    name: String,
    #[serde(rename = "type")]
    type_name: String,
    value: RenderValue,
}

/// Instance-reference metadata, flattened onto a [`Property`].
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstanceRef {
    is_instance_reference: bool,
    referenced_instance_id: String,
    referenced_instance_name: String,
    referenced_instance_class: String,
}

/// Enum metadata (the option list), flattened onto a [`Property`].
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EnumMeta {
    is_enum: bool,
    enum_values: Vec<EnumOption>,
}

/// One selectable enum entry. Note the lowercase `name`/`value` keys (verde's
/// option list uses these, unlike the PascalCase geometry shapes).
#[derive(Serialize)]
struct EnumOption {
    name: String,
    value: u32,
}

/// The `value` of a property or attribute. Untagged, so each variant serializes
/// to its inner shape with no discriminant — reproducing the shapes the old
/// `json!` calls built.
#[derive(Serialize)]
#[serde(untagged)]
enum RenderValue {
    Bool(bool),
    // Integer-valued types (Int / Int32 / BrickColor / bare enum fallback):
    // serialized without a decimal point.
    Int(i64),
    // Real-valued types (Float / Float32); f32 sources are widened to f64 so the
    // rendering matches what `json!` produced for them.
    Number(f64),
    // String, ContentId, resolved Ref id, byte blobs, and the debug fallback.
    Text(String),
    Vector2(Vec2<f64>),
    Vector3(Vec3<f64>),
    Vector2int16(Vec2<i16>),
    Vector3int16(Vec3<i16>),
    Color(Color),
    UDim(UDimJson),
    UDim2(UDim2Json),
    NumberRange(RangeJson),
    Rect(RectJson),
    CFrame(CFrameJson),
    Ray(RayJson),
    NumberSequence(NumberSequenceJson),
    ColorSequence(ColorSequenceJson),
    Enum(EnumValueJson),
    // Font's Weight/Style field types live in `DomValue::Font` and aren't visible
    // in this module, so this one value is serialized as-is. Swap in a plain
    // struct (like the others) once those types are known.
    Font(serde_json::Value),
}

/// `{ "X": .., "Y": .. }` for any numeric component type.
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Vec2<T> {
    x: T,
    y: T,
}

/// `{ "X": .., "Y": .., "Z": .. }` for any numeric component type.
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Vec3<T> {
    x: T,
    y: T,
    z: T,
}

/// `{ "R": .., "G": .., "B": .. }`, components normalised to 0..1.
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Color {
    r: f64,
    g: f64,
    b: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct UDimJson {
    scale: f64,
    offset: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct UDim2Json {
    x: UDimJson,
    y: UDimJson,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct RangeJson {
    min: f64,
    max: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct RectJson {
    min: Vec2<f64>,
    max: Vec2<f64>,
}

/// Verde displays a CFrame as position plus its look vector `-(R02, R12, R22)`.
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct CFrameJson {
    position: Vec3<f64>,
    rotation: Vec3<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct RayJson {
    origin: Vec3<f64>,
    direction: Vec3<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct NumberSequenceJson {
    keypoints: Vec<NumberKeypoint>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct NumberKeypoint {
    time: f64,
    value: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ColorSequenceJson {
    keypoints: Vec<ColorKeypoint>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ColorKeypoint {
    time: f64,
    value: Color,
}

/// An enum value as verde shows it: the ordinal plus its resolved name.
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct EnumValueJson {
    value: u32,
    name: String,
}

/// Structural / security-internal properties verde's inspector hides — the
/// client sends everything it can read, this is verde's own presentation choice.
const HIDDEN_PROPERTIES: &[&str] = &[
    "Parent",
    "ClassName",
    "Capabilities",
    "Sandboxed",
    "RobloxLocked",
];

fn render_reads(reads: Reads, mirror: &Mirror, class: Option<&str>) -> ReadsData {
    let catalog = mirror.enum_catalog();
    let properties = reads
        .properties
        .iter()
        .filter(|read| !HIDDEN_PROPERTIES.contains(&read.name.as_str()))
        .map(|read| render_property(read, mirror, &catalog, class))
        .collect();
    ReadsData {
        properties,
        tags: reads.tags,
        attributes: reads
            .attributes
            .iter()
            .map(|read| render_attribute(read, mirror, &catalog))
            .collect(),
    }
}

/// A property for verde's inspector: the rendered value and type, with `category`,
/// `isReadOnly` and `layoutOrder` from the bundled API dump keyed by `class`.
fn render_property(
    read: &NamedValue,
    mirror: &Mirror,
    catalog: &[EnumFamily],
    class: Option<&str>,
) -> Property {
    let rendered = describe(&read.value, mirror, catalog);
    let metadata = class
        .and_then(|c| super::api::property_meta(c, &read.name))
        .unwrap_or_default();
    Property {
        name: read.name.clone(),
        type_name: rendered.type_name,
        category: metadata.category,
        layout_order: metadata.layout_order,
        is_read_only: metadata.read_only,
        value: rendered.value,
        reference: rendered.reference,
        enumeration: rendered.enumeration,
    }
}

fn render_attribute(read: &NamedValue, mirror: &Mirror, catalog: &[EnumFamily]) -> Attribute {
    let rendered = describe(&read.value, mirror, catalog);
    Attribute {
        name: read.name.clone(),
        type_name: rendered.type_name,
        value: rendered.value,
    }
}

/// One value rendered for verde: its `type` name, JSON value, and the optional
/// ref / enum metadata that gets flattened onto a property (attributes drop it).
struct Rendered {
    type_name: String,
    value: RenderValue,
    reference: Option<InstanceRef>,
    enumeration: Option<EnumMeta>,
}

impl Rendered {
    fn plain(type_name: &str, value: RenderValue) -> Self {
        Self {
            type_name: type_name.to_string(),
            value,
            reference: None,
            enumeration: None,
        }
    }
}

fn describe(value: &DomValue, mirror: &Mirror, catalog: &[EnumFamily]) -> Rendered {
    match value {
        DomValue::Bool(b) => Rendered::plain("boolean", RenderValue::Bool(*b)),
        DomValue::Float(n) => Rendered::plain("number", RenderValue::Number(*n)),
        DomValue::Int(n) => Rendered::plain("number", RenderValue::Int(*n as i64)),
        DomValue::Float32(n) => Rendered::plain("number", RenderValue::Number(*n as f64)),
        DomValue::Int32(n) => Rendered::plain("number", RenderValue::Int(*n as i64)),
        DomValue::String(s) => Rendered::plain("string", RenderValue::Text(s.clone())),
        DomValue::ContentId(s) => Rendered::plain("string", RenderValue::Text(s.clone())),
        DomValue::Ref(id) => describe_ref(id, mirror),
        DomValue::Enum(family, value) => describe_enum(*family, *value, catalog),
        DomValue::Vector3(x, y, z) => {
            Rendered::plain("Vector3", RenderValue::Vector3(vec3(*x, *y, *z)))
        }
        DomValue::Vector2(x, y) => Rendered::plain("Vector2", RenderValue::Vector2(vec2(*x, *y))),
        DomValue::Vector3int16(x, y, z) => Rendered::plain(
            "Vector3int16",
            RenderValue::Vector3int16(Vec3 {
                x: *x,
                y: *y,
                z: *z,
            }),
        ),
        DomValue::Vector2int16(x, y) => {
            Rendered::plain("Vector2int16", RenderValue::Vector2int16(Vec2 { x: *x, y: *y }))
        }
        DomValue::Color3(c) | DomValue::Color3uint8(c) => {
            Rendered::plain("Color3", RenderValue::Color(color(c)))
        }
        DomValue::UDim(u) => Rendered::plain("UDim", RenderValue::UDim(udim(u))),
        DomValue::UDim2(u) => Rendered::plain(
            "UDim2",
            RenderValue::UDim2(UDim2Json {
                x: udim(&u.x),
                y: udim(&u.y),
            }),
        ),
        DomValue::NumberRange(min, max) => Rendered::plain(
            "NumberRange",
            RenderValue::NumberRange(RangeJson {
                min: *min as f64,
                max: *max as f64,
            }),
        ),
        DomValue::Rect(min_x, min_y, max_x, max_y) => Rendered::plain(
            "Rect",
            RenderValue::Rect(RectJson {
                min: vec2(*min_x, *min_y),
                max: vec2(*max_x, *max_y),
            }),
        ),
        DomValue::BrickColor(number) => {
            Rendered::plain("BrickColor", RenderValue::Int(*number as i64))
        }
        DomValue::CFrame(c) => Rendered::plain("CFrame", RenderValue::CFrame(cframe(c))),
        DomValue::Ray(ox, oy, oz, dx, dy, dz) => Rendered::plain(
            "Ray",
            RenderValue::Ray(RayJson {
                origin: vec3(*ox, *oy, *oz),
                direction: vec3(*dx, *dy, *dz),
            }),
        ),
        DomValue::Font(family, weight, style) => Rendered::plain(
            "Font",
            RenderValue::Font(serde_json::json!({
                "Family": family,
                "Weight": weight,
                "Style": style,
            })),
        ),
        DomValue::NumberSequence(keypoints) => {
            let keypoints = keypoints
                .iter()
                .map(|kp| NumberKeypoint {
                    time: kp.time as f64,
                    value: kp.value as f64,
                })
                .collect();
            Rendered::plain(
                "NumberSequence",
                RenderValue::NumberSequence(NumberSequenceJson { keypoints }),
            )
        }
        DomValue::ColorSequence(keypoints) => {
            let keypoints = keypoints
                .iter()
                .map(|kp| ColorKeypoint {
                    time: kp.time as f64,
                    value: color(&kp.value),
                })
                .collect();
            Rendered::plain(
                "ColorSequence",
                RenderValue::ColorSequence(ColorSequenceJson { keypoints }),
            )
        }
        // Opaque byte blobs (e.g. Player.User): render as a string rather than a
        // Rust-debug byte array — a 16-byte id (optionally behind a 1-byte tag, as
        // Player.User has) formats as a dashed UUID, anything else plain hex.
        DomValue::BinaryString(bytes) => {
            Rendered::plain("string", RenderValue::Text(render_bytes(&bytes.0)))
        }
        // Rarely-inspected types: a best-effort string keeps verde displaying
        // something without the broker growing a bespoke shape for each.
        other => Rendered::plain("string", RenderValue::Text(format!("{other:?}"))),
    }
}

/// An opaque byte blob as text: a 16-byte id (optionally behind a leading 1-byte
/// tag, as `Player.User` carries) as a dashed UUID, otherwise plain hex.
fn render_bytes(bytes: &[u8]) -> String {
    let id = match bytes.len() {
        17 => &bytes[1..],
        16 => bytes,
        _ => return bytes.iter().map(|b| format!("{b:02x}")).collect(),
    };
    let hex = id.iter().map(|b| format!("{b:02x}")).collect::<String>();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

fn describe_ref(id: &str, mirror: &Mirror) -> Rendered {
    let (name, class) = mirror
        .resolve_ref(id)
        .unwrap_or_else(|| (id.to_string(), String::new()));
    Rendered {
        type_name: "Instance".into(),
        value: RenderValue::Text(id.to_string()),
        reference: Some(InstanceRef {
            is_instance_reference: true,
            referenced_instance_id: id.to_string(),
            referenced_instance_name: name,
            referenced_instance_class: class,
        }),
        enumeration: None,
    }
}

fn describe_enum(family_index: u16, value: u32, catalog: &[EnumFamily]) -> Rendered {
    let Some(family) = catalog.get(family_index as usize) else {
        // No catalog entry: fall back to the bare value.
        return Rendered::plain("EnumItem", RenderValue::Int(value as i64));
    };
    let name = family
        .item(value)
        .map(|entry| entry.name.clone())
        .unwrap_or_default();
    let enum_values = family
        .items
        .iter()
        .map(|entry| EnumOption {
            name: entry.name.clone(),
            value: entry.value,
        })
        .collect();
    Rendered {
        type_name: family.name.clone(),
        value: RenderValue::Enum(EnumValueJson { value, name }),
        reference: None,
        enumeration: Some(EnumMeta {
            is_enum: true,
            enum_values,
        }),
    }
}

/// Round to three decimals, verde's position/vector display precision.
fn round3(n: f32) -> f64 {
    (n as f64 * 1000.0).round() / 1000.0
}

fn vec2(x: f32, y: f32) -> Vec2<f64> {
    Vec2 {
        x: round3(x),
        y: round3(y),
    }
}

fn vec3(x: f32, y: f32, z: f32) -> Vec3<f64> {
    Vec3 {
        x: round3(x),
        y: round3(y),
        z: round3(z),
    }
}

fn color(c: &WireColor3) -> Color {
    Color {
        r: c.r as f64 / 255.0,
        g: c.g as f64 / 255.0,
        b: c.b as f64 / 255.0,
    }
}

fn udim(u: &WireUdim<f32>) -> UDimJson {
    UDimJson {
        scale: u.scale as f64,
        offset: u.offset as f64,
    }
}

/// Verde displays a CFrame as position plus its look vector `-(R02, R12, R22)`.
fn cframe(c: &[f32; 12]) -> CFrameJson {
    CFrameJson {
        position: vec3(c[0], c[1], c[2]),
        rotation: vec3(-c[5], -c[8], -c[11]),
    }
}

fn to_u8(n: f64) -> u8 {
    (n * 255.0).round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::protocol::{EnumEntry, EnumFamily};

    /// Build a generic [`DomValue`] from one verde value by shape, where enums and
    /// instance references can't be typed here (verde is loose and the broker has no
    /// reflection) so they ride a scalar the client resolves against the live type.
    ///
    /// Kept as a named entry point (used by tests / callers) over the [`RawValue`]
    /// wire form.
    fn to_value(value: &serde_json::Value) -> Option<DomValue> {
        RawValue::deserialize(value).ok().map(RawValue::into_dom)
    }


    #[test]
    fn renders_byte_blobs_as_uuid_or_hex() {
        // Player.User: 1-byte tag + 16-byte id -> dashed UUID (tag dropped).
        let mut user = vec![1u8];
        user.extend([0u8; 16]);
        assert_eq!(render_bytes(&user), "00000000-0000-0000-0000-000000000000");
        // A bare 16-byte id also formats as a UUID.
        assert_eq!(
            render_bytes(&[0xab; 16]),
            "abababab-abab-abab-abab-abababababab"
        );
        // Anything else falls back to plain hex.
        assert_eq!(render_bytes(&[0x00, 0xff, 0x10]), "00ff10");
    }

    #[test]
    fn parses_operations_into_generic_ops() {
        assert_eq!(
            to_operation(&json!({ "type": "rename_instance", "nodeId": "n", "newName": "X" })),
            Some(Operation::Rename {
                node: "n".into(),
                name: "X".into()
            }),
        );
        // add_attribute folds into SetAttribute with the type default.
        assert_eq!(
            to_operation(
                &json!({ "type": "add_attribute", "nodeId": "n", "attributeName": "S", "attributeType": "number" })
            ),
            Some(Operation::SetAttribute {
                node: "n".into(),
                name: "S".into(),
                value: DomValue::Float(0.0)
            }),
        );
        assert_eq!(
            to_operation(&json!({ "type": "run_code", "code": "print(1)" })),
            Some(Operation::RunCode {
                source: "print(1)".into()
            }),
        );
        assert_eq!(
            to_operation(&json!({ "type": "future_op", "nodeId": "n" })),
            None
        );
    }

    #[test]
    fn renders_run_code_output() {
        let outcome = to_outcome(OpResult::Output("hello\n42".into()), &Mirror::new(), None);
        assert!(outcome.success);
        assert_eq!(outcome.data, Some(json!({ "output": "hello\n42" })));
    }

    #[test]
    fn properties_carry_metadata_from_the_api_dump() {
        let reads = Reads {
            properties: vec![NamedValue {
                name: "Anchored".into(),
                value: DomValue::Bool(true),
            }],
            tags: vec![],
            attributes: vec![],
        };
        let data = to_outcome(OpResult::Reads(reads), &Mirror::new(), Some("Part"))
            .data
            .expect("reads data");
        let prop = &data["properties"].as_array().unwrap()[0];
        assert_ne!(
            prop["category"], "Other",
            "a known property gets its real category"
        );
        assert_eq!(prop["isReadOnly"], false, "Anchored is editable");
        assert!(prop["layoutOrder"].is_number());
    }

    #[test]
    fn builds_dom_values_from_verde_shapes() {
        assert_eq!(to_value(&json!(true)), Some(DomValue::Bool(true)));
        assert_eq!(to_value(&json!(3.5)), Some(DomValue::Float(3.5)));
        assert_eq!(
            to_value(&json!({ "X": 1.0, "Y": 2.0, "Z": 3.0 })),
            Some(DomValue::Vector3(1.0, 2.0, 3.0))
        );
        assert_eq!(
            to_value(&json!({ "Min": 1.0, "Max": 2.0 })),
            Some(DomValue::NumberRange(1.0, 2.0))
        );
        // An enum object becomes a scalar the client resolves to the live type.
        assert_eq!(
            to_value(&json!({ "Name": "Ball" })),
            Some(DomValue::String("Ball".into()))
        );
    }

    #[test]
    fn renders_reads_with_resolved_refs_and_enums() {
        let mirror = Mirror::new();
        mirror.set_enum_catalog(vec![
            EnumFamily {
                name: "Filler".into(),
                items: vec![],
            },
            EnumFamily {
                name: "PartType".into(),
                items: vec![
                    EnumEntry {
                        name: "Ball".into(),
                        value: 0,
                    },
                    EnumEntry {
                        name: "Block".into(),
                        value: 1,
                    },
                ],
            },
        ]);

        let reads = Reads {
            properties: vec![
                NamedValue {
                    name: "Anchored".into(),
                    value: DomValue::Bool(true),
                },
                NamedValue {
                    name: "Shape".into(),
                    value: DomValue::Enum(1, 1),
                },
                NamedValue {
                    name: "Adornee".into(),
                    value: DomValue::Ref("missing".into()),
                },
            ],
            tags: vec!["Enemy".into()],
            attributes: vec![NamedValue {
                name: "Speed".into(),
                value: DomValue::Float(16.0),
            }],
        };
        let data = to_outcome(OpResult::Reads(reads), &mirror, None)
            .data
            .expect("reads data");
        let props = data["properties"].as_array().unwrap();

        assert_eq!(props[0]["value"], json!(true));
        assert_eq!(props[0]["type"], "boolean");

        assert_eq!(props[1]["type"], "PartType");
        assert_eq!(props[1]["isEnum"], true);
        assert_eq!(props[1]["value"], json!({ "Value": 1, "Name": "Block" }));
        assert_eq!(
            props[1]["enumValues"][0],
            json!({ "name": "Ball", "value": 0 })
        );

        // An unmirrored ref falls back to the id as its name.
        assert_eq!(props[2]["isInstanceReference"], true);
        assert_eq!(props[2]["referencedInstanceId"], "missing");
        assert_eq!(props[2]["referencedInstanceName"], "missing");

        assert_eq!(data["tags"][0], "Enemy");
        assert_eq!(
            data["attributes"][0],
            json!({ "name": "Speed", "type": "number", "value": 16.0 })
        );
    }

    #[test]
    fn rounds_vectors_and_reads_cframe_look_vector() {
        let mirror = Mirror::new();
        let reads = Reads {
            properties: vec![
                NamedValue {
                    name: "Position".into(),
                    value: DomValue::Vector3(1.23456, 0.0, 0.0),
                },
                NamedValue {
                    name: "CFrame".into(),
                    value: DomValue::CFrame([
                        5.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0,
                    ]),
                },
            ],
            tags: vec![],
            attributes: vec![],
        };
        let data = to_outcome(OpResult::Reads(reads), &mirror, None)
            .data
            .unwrap();
        let props = data["properties"].as_array().unwrap();
        assert_eq!(props[0]["value"], json!({ "X": 1.235, "Y": 0.0, "Z": 0.0 }));
        // Identity rotation: look vector is -(R02,R12,R22) = (0,0,-1).
        assert_eq!(
            props[1]["value"]["Rotation"],
            json!({ "X": 0.0, "Y": 0.0, "Z": -1.0 })
        );
    }
}