use prost::Message;

// Catalogue responses share the SDK completion queue without enabling protobuf JSON.
impl crate::IntoValue for crate::omega::CommandCatalogue {
    fn into_value(self) -> crate::omega::Value {
        crate::omega::Value {
            kind: Some(crate::omega::value::Kind::BytesValue(self.encode_to_vec())),
        }
    }
}
impl crate::FromValue for crate::omega::CommandCatalogue {
    fn from_value(value: &crate::omega::Value) -> Option<Self> {
        match value.kind.as_ref()? {
            crate::omega::value::Kind::BytesValue(bytes) => Self::decode(bytes.as_slice()).ok(),
            _ => None,
        }
    }
}
