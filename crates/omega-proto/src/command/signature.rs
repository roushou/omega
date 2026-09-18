use crate::omega::{CommandEndpoint, CommandType};
use prost::Message;
use sha2::{Digest, Sha256};

impl CommandEndpoint {
    pub fn signature(&self, plugin: &str) -> Vec<u8> {
        let mut endpoint = self.clone();
        endpoint.description.clear();
        endpoint.input = endpoint.input.as_ref().map(CommandType::canonical);
        endpoint.output = endpoint.output.as_ref().map(CommandType::canonical);
        let mut hash = Sha256::new();
        hash.update((plugin.len() as u64).to_le_bytes());
        hash.update(plugin.as_bytes());
        hash.update(endpoint.encode_to_vec());
        hash.finalize().to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::omega::{CommandField, command_type::Kind};
    struct Fixture;
    impl Fixture {
        fn endpoint() -> CommandEndpoint {
            CommandEndpoint {
                id: "set".into(),
                input: Some(CommandType::of(Kind::Text)),
                output: Some(CommandType::of(Kind::Unit)),
                description: "Set a value".into(),
            }
        }
    }
    #[test]
    fn signature_excludes_prose_but_includes_owner_and_types() {
        let mut endpoint = Fixture::endpoint();
        let signature = endpoint.signature("first");
        endpoint.description = "new prose".into();
        assert_eq!(signature, endpoint.signature("first"));
        assert_ne!(signature, endpoint.signature("second"));
        endpoint.output = Some(CommandType::of(Kind::Text));
        assert_ne!(signature, endpoint.signature("first"));
    }
    #[test]
    fn record_field_order_does_not_change_signature() {
        let mut endpoint = Fixture::endpoint();
        endpoint.input = Some(CommandType {
            fields: ["b", "a"]
                .map(|name| CommandField {
                    name: name.into(),
                    value: Some(CommandType::of(Kind::Text)),
                })
                .to_vec(),
            ..CommandType::of(Kind::Record)
        });
        let signature = endpoint.signature("first");
        endpoint.input.as_mut().unwrap().fields.reverse();
        assert_eq!(signature, endpoint.signature("first"));
    }
}
