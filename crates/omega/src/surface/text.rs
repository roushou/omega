use crate::{Args, Error, Input};
use omega_proto::{IntoValue, Values};

/// One committed edit, excluding an unfinished input-method composition.
#[derive(Debug, Clone)]
pub struct TextEdit {
    pub text: String,
    pub revision: u32,
    pub reset: u32,
}
impl Input for TextEdit {
    fn decode(args: Args) -> Result<Self, Error> {
        if args.len() != 1 {
            return Err(Error::invalid("expected one text edit"));
        }
        let values: Values = args
            .get(0)
            .ok_or_else(|| Error::invalid("expected a text edit map"))?;
        Ok(Self {
            text: values
                .get("text")
                .ok_or_else(|| Error::invalid("edit requires text"))?,
            revision: values
                .get("revision")
                .ok_or_else(|| Error::invalid("edit requires a revision"))?,
            reset: values
                .get("reset")
                .ok_or_else(|| Error::invalid("edit requires a reset revision"))?,
        })
    }
    fn encode(self) -> Vec<omega_proto::omega::Value> {
        vec![
            Values::new()
                .with("text", self.text)
                .with("revision", self.revision)
                .with("reset", self.reset)
                .into_value(),
        ]
    }
}
/// Controlled text that preserves newer renderer edits across delayed views.
#[derive(Debug, Clone, Default)]
pub struct TextValue {
    text: String,
    revision: u32,
    reset: u32,
}
impl TextValue {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Default::default()
        }
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn revision(&self) -> u32 {
        self.revision
    }
    pub fn reset_revision(&self) -> u32 {
        self.reset
    }
    /// Apply only edits from the current reset generation, in revision order.
    pub fn apply(&mut self, edit: TextEdit) -> bool {
        if edit.reset != self.reset || edit.revision <= self.revision {
            return false;
        }
        self.text = edit.text;
        self.revision = edit.revision;
        true
    }
    /// Explicitly replace text, invalidating edits made before this reset.
    pub fn reset(&mut self, text: impl Into<String>) {
        self.reset = self
            .reset
            .checked_add(1)
            .expect("text reset revision exhausted");
        self.revision = 0;
        self.text = text.into();
    }
}
