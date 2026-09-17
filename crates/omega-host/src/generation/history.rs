use super::GenerationId;
use crate::{Layout, TomlFile, TomlSchema};
use serde::{Deserialize, Serialize};

/// Acceptance and its predecessor are committed as one document.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct History {
    pub accepted: Option<GenerationId>,
    pub previous: Option<GenerationId>,
}
impl TomlSchema for History {
    fn decode(source: &str) -> Result<Self, crate::TomlError> {
        crate::Toml::deserialize(source)
    }

    fn encode(&self) -> Result<String, crate::TomlError> {
        crate::Toml::serialize(self)
    }

    const KIND: &'static str = "generation history";
    type Key<'a> = ();
    fn locate(layout: &Layout, _: ()) -> TomlFile<Self> {
        TomlFile::at(layout.generation_history())
    }
}
