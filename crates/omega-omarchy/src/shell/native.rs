use super::*;

/// Omarchy plugin names are distinct from Omega unit names.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct NativeId(String);
impl NativeId {
    pub fn parse(value: impl Into<String>) -> Result<Self, ShellError> {
        let value = value.into();
        if value.is_empty()
            || !value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        {
            return Err(ShellError::Invalid(format!(
                "invalid native plugin id {value:?}"
            )));
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl<'de> Deserialize<'de> for NativeId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Typed options for Omarchy's native clock. Formats use Qt date/time syntax.
#[derive(Debug, Clone)]
pub struct Clock {
    native: Native,
}
impl Default for Clock {
    fn default() -> Self {
        Self {
            native: Native::new("omarchy.clock"),
        }
    }
}
impl Clock {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn format(mut self, format: impl Into<String>) -> Self {
        self.native
            .options
            .insert("format".into(), format.into().into());
        self
    }
    pub fn alternate_format(mut self, format: impl Into<String>) -> Self {
        self.native
            .options
            .insert("formatAlt".into(), format.into().into());
        self
    }
    pub fn vertical_format(mut self, format: impl Into<String>) -> Self {
        self.native
            .options
            .insert("verticalFormat".into(), format.into().into());
        self
    }
}
impl From<Clock> for BarItem {
    fn from(clock: Clock) -> Self {
        Self::Native(clock.native)
    }
}
