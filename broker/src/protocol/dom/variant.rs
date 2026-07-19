use rbx_dom_weak::types::{
    Axes, BrickColor, CFrame, Color3, Color3uint8, ColorSequence, ColorSequenceKeypoint, Content,
    CustomPhysicalProperties, Enum, Faces, Font, FontStyle, FontWeight, Matrix3, NumberRange,
    NumberSequence, NumberSequenceKeypoint, PhysicalProperties, Ray, Rect, Ref, Region3,
    Region3int16, UDim, UDim2, Variant, Vector2, Vector2int16, Vector3, Vector3int16,
};
use serde::{Deserialize, Serialize};
use squash::roblox::{
    Axes as WireAxes, Color3 as WireColor3, ColorSequenceKeypoint as WireColorKeypoint,
    Faces as WireFaces, NumberSequenceKeypoint as WireNumberKeypoint, Region3 as WireRegion3,
    Udim as WireUdim, Udim2 as WireUdim2,
};

use super::{DomBytes, DomId};

/// A mirror of `rbx_types::ContentType`, specialised for binary format.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
pub enum ContentValue {
    None,
    Uri(Option<String>),
    Object(DomId),
}

/// A subet of `rbx_types::Variant`, specialised for binary format.
#[derive(Clone, PartialEq, PartialOrd, Debug, Serialize, Deserialize)]
pub enum DomValue {
    Bool(bool),
    Float(f64),
    Int(i64),
    String(String),
    Ref(DomId),
    Enum(u16, u32),
    Vector2(f32, f32),
    Vector3(f32, f32, f32),
    Color3(WireColor3),
    UDim(WireUdim<f32>),
    UDim2(WireUdim2<f32>),
    NumberRange(f32, f32),
    Rect(f32, f32, f32, f32),
    BrickColor(u16),
    CFrame([f32; 12]),
    Float32(f32),
    Int32(i32),
    ContentId(String),
    BinaryString(DomBytes),
    Color3uint8(WireColor3),
    Vector2int16(i16, i16),
    Vector3int16(i16, i16, i16),
    Ray(f32, f32, f32, f32, f32, f32),
    Region3(WireRegion3<f32>),
    Region3int16(i16, i16, i16, i16, i16, i16),
    Axes(WireAxes),
    Faces(WireFaces),
    Font(String, u16, u8),
    NumberSequence(Vec<WireNumberKeypoint<f32>>),
    ColorSequence(Vec<WireColorKeypoint>),
    PhysicalProperties(Option<[f32; 6]>),
    OptionalCFrame(Option<[f32; 12]>),
    Content(ContentValue),
}

/// Build an `rbx` CFrame from `GetComponents()` order (see [`DomValue::CFrame`]).
fn cframe_from_components(c: [f32; 12]) -> CFrame {
    CFrame::new(
        Vector3::new(c[0], c[1], c[2]),
        Matrix3::new(
            Vector3::new(c[3], c[4], c[5]),
            Vector3::new(c[6], c[7], c[8]),
            Vector3::new(c[9], c[10], c[11]),
        ),
    )
}

impl DomValue {
    /// Convert a [`DomValue`] into a [`rbx_types::Variant`], resolving instance references via `resolve`.
    pub fn into_variant(self, resolve: impl FnOnce(&DomId) -> Option<Ref>) -> Variant {
        match self {
            DomValue::Bool(b) => Variant::Bool(b),
            DomValue::Float(f) => Variant::Float64(f),
            DomValue::Int(i) => Variant::Int64(i),
            DomValue::String(s) => Variant::String(s),
            DomValue::Ref(id) => Variant::Ref(resolve(&id).unwrap_or_else(Ref::none)),
            DomValue::Enum(_index, v) => Variant::Enum(Enum::from_u32(v)),
            DomValue::Vector2(x, y) => Variant::Vector2(Vector2::new(x, y)),
            DomValue::Vector3(x, y, z) => Variant::Vector3(Vector3::new(x, y, z)),
            DomValue::Color3(c) => Variant::Color3(Color3::new(
                c.r as f32 / 255.0,
                c.g as f32 / 255.0,
                c.b as f32 / 255.0,
            )),
            DomValue::UDim(u) => Variant::UDim(UDim::new(u.scale, u.offset as i32)),
            DomValue::UDim2(u) => Variant::UDim2(UDim2::new(
                UDim::new(u.x.scale, u.x.offset as i32),
                UDim::new(u.y.scale, u.y.offset as i32),
            )),
            DomValue::NumberRange(min, max) => Variant::NumberRange(NumberRange::new(min, max)),
            DomValue::Rect(min_x, min_y, max_x, max_y) => Variant::Rect(Rect::new(
                Vector2::new(min_x, min_y),
                Vector2::new(max_x, max_y),
            )),
            DomValue::BrickColor(number) => Variant::BrickColor(
                BrickColor::from_number(number).unwrap_or(BrickColor::MediumStoneGrey),
            ),
            DomValue::CFrame(c) => Variant::CFrame(cframe_from_components(c)),
            DomValue::Float32(f) => Variant::Float32(f),
            DomValue::Int32(i) => Variant::Int32(i),
            DomValue::ContentId(url) => Variant::ContentId(url.into()),
            DomValue::BinaryString(bytes) => Variant::BinaryString(bytes.0.into()),
            DomValue::Color3uint8(c) => Variant::Color3uint8(Color3uint8::new(c.r, c.g, c.b)),
            DomValue::Vector2int16(x, y) => Variant::Vector2int16(Vector2int16::new(x, y)),
            DomValue::Vector3int16(x, y, z) => Variant::Vector3int16(Vector3int16::new(x, y, z)),
            DomValue::Ray(ox, oy, oz, dx, dy, dz) => {
                Variant::Ray(Ray::new(Vector3::new(ox, oy, oz), Vector3::new(dx, dy, dz)))
            }
            DomValue::Region3(r) => {
                let (p, s) = (r.position, r.size);
                Variant::Region3(Region3::new(
                    Vector3::new(p.x - s.x / 2.0, p.y - s.y / 2.0, p.z - s.z / 2.0),
                    Vector3::new(p.x + s.x / 2.0, p.y + s.y / 2.0, p.z + s.z / 2.0),
                ))
            }
            DomValue::Region3int16(ax, ay, az, bx, by, bz) => Variant::Region3int16(
                Region3int16::new(Vector3int16::new(ax, ay, az), Vector3int16::new(bx, by, bz)),
            ),
            DomValue::Axes(a) => {
                let bits = (a.x as u8) | (a.y as u8) << 1 | (a.z as u8) << 2;
                Variant::Axes(Axes::from_bits(bits).expect("valid axis bits"))
            }
            DomValue::Faces(f) => {
                let bits = (f.right as u8)
                    | (f.top as u8) << 1
                    | (f.back as u8) << 2
                    | (f.left as u8) << 3
                    | (f.bottom as u8) << 4
                    | (f.front as u8) << 5;
                Variant::Faces(Faces::from_bits(bits).expect("valid face bits"))
            }
            DomValue::Font(family, weight, style) => Variant::Font(Font {
                family,
                weight: FontWeight::from_u16(weight).unwrap_or(FontWeight::Regular),
                style: FontStyle::from_u8(style).unwrap_or(FontStyle::Normal),
                cached_face_id: None,
            }),
            DomValue::NumberSequence(keypoints) => Variant::NumberSequence(NumberSequence {
                keypoints: keypoints
                    .into_iter()
                    .map(|kp| NumberSequenceKeypoint::new(kp.time, kp.value, kp.envelope))
                    .collect(),
            }),
            DomValue::ColorSequence(keypoints) => Variant::ColorSequence(ColorSequence {
                keypoints: keypoints
                    .into_iter()
                    .map(|kp| {
                        ColorSequenceKeypoint::new(
                            kp.time as f32 / 255.0,
                            Color3::new(
                                kp.value.r as f32 / 255.0,
                                kp.value.g as f32 / 255.0,
                                kp.value.b as f32 / 255.0,
                            ),
                        )
                    })
                    .collect(),
            }),
            DomValue::PhysicalProperties(custom) => Variant::PhysicalProperties(match custom {
                None => PhysicalProperties::Default,
                Some(p) => PhysicalProperties::Custom(CustomPhysicalProperties::new(
                    p[0], p[1], p[2], p[3], p[4], p[5],
                )),
            }),
            DomValue::OptionalCFrame(cframe) => {
                Variant::OptionalCFrame(cframe.map(cframe_from_components))
            }
            DomValue::Content(content) => Variant::Content(match content {
                ContentValue::None => Content::none(),
                ContentValue::Uri(uri) => match uri {
                    Some(uri) => Content::from_uri(uri),
                    None => Content::none(),
                },
                ContentValue::Object(id) => {
                    Content::from_referent(resolve(&id).unwrap_or_else(Ref::none))
                }
            }),
        }
    }

    /// Make sure the value is acceptable for a Roblox attribute.
    pub fn is_attribute_safe(&self) -> bool {
        matches!(
            self,
            DomValue::Bool(_)
                | DomValue::Float(_)
                | DomValue::String(_)
                | DomValue::Vector2(..)
                | DomValue::Vector3(..)
                | DomValue::Color3(..)
                | DomValue::UDim(..)
                | DomValue::UDim2(..)
                | DomValue::NumberRange(..)
                | DomValue::Rect(..)
                | DomValue::BrickColor(_)
                | DomValue::CFrame(_)
                | DomValue::Font(..)
                | DomValue::NumberSequence(_)
                | DomValue::ColorSequence(_)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `GetComponents()` hands the rotation over row-major and `Matrix3`'s `x`/`y`/`z` vectors are the matrix *rows* (rbx_xml's `<CFrame>` codec writes `orientation.x` out as `R00, R01, R02`), pinned here so a refactor can't silently transpose it.
    #[test]
    fn cframe_components_map_to_rows() {
        let value = DomValue::CFrame([
            10.0, 20.0, 30.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0,
        ]);
        let expected = CFrame::new(
            Vector3::new(10.0, 20.0, 30.0),
            Matrix3::new(
                Vector3::new(1.0, 2.0, 3.0),
                Vector3::new(4.0, 5.0, 6.0),
                Vector3::new(7.0, 8.0, 9.0),
            ),
        );
        assert_eq!(value.into_variant(|_| None), Variant::CFrame(expected));
    }
}
