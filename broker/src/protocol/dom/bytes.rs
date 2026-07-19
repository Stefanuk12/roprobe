use serde::{Deserialize, Serialize};

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Default)]
pub struct DomBytes(pub Vec<u8>);

impl Serialize for DomBytes {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&self.0)
    }
}

impl<'de> Deserialize<'de> for DomBytes {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct BytesVisitor;
        impl<'de> serde::de::Visitor<'de> for BytesVisitor {
            type Value = DomBytes;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "a byte buffer")
            }
            fn visit_byte_buf<E: serde::de::Error>(self, v: Vec<u8>) -> Result<DomBytes, E> {
                Ok(DomBytes(v))
            }
            fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<DomBytes, E> {
                Ok(DomBytes(v.to_vec()))
            }
        }
        deserializer.deserialize_byte_buf(BytesVisitor)
    }
}
