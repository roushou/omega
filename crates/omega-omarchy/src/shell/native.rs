use super::*;

/// Omarchy plugin names are distinct from Omega plugin names.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct NativeId(String);
impl std::str::FromStr for NativeId {
    type Err = ShellError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl TryFrom<&str> for NativeId {
    type Error = ShellError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl TryFrom<String> for NativeId {
    type Error = ShellError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
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
}

impl NativeId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for NativeId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_from(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
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
