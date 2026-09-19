//! Desktop-entry identities, never paths or command lines.
use crate::{FromValue, IntoValue, omega::Value};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ApplicationId(String);
impl std::str::FromStr for ApplicationId {
    type Err = ApplicationIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl TryFrom<&str> for ApplicationId {
    type Error = ApplicationIdError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl TryFrom<String> for ApplicationId {
    type Error = ApplicationIdError;

    fn try_from(id: String) -> Result<Self, Self::Error> {
        if id.len() > 255
            || !id.ends_with(".desktop")
            || id.len() <= 8
            || id.contains(['/', '\\'])
            || id.chars().any(char::is_control)
        {
            return Err(ApplicationIdError(id));
        }
        Ok(Self(id))
    }
}

impl ApplicationId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ApplicationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid desktop-entry id: {0:?}")]
pub struct ApplicationIdError(String);
impl FromValue for ApplicationId {
    fn from_value(value: &Value) -> Option<Self> {
        Self::try_from(String::from_value(value)?).ok()
    }
}

impl IntoValue for ApplicationId {
    fn into_value(self) -> Value {
        self.0.into_value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ids_are_file_names_not_paths() {
        for bad in [
            "",
            "firefox",
            ".desktop",
            "../foo.desktop",
            "/foo.desktop",
            "a\n.desktop",
        ] {
            assert!(ApplicationId::try_from(bad).is_err(), "{bad:?}");
        }
        for good in [
            "org.gnome.Nautilus.desktop",
            "my app.desktop",
            "subdir-tool.desktop",
        ] {
            let id = ApplicationId::try_from(good).unwrap();
            assert_eq!(
                ApplicationId::from_value(&id.clone().into_value()),
                Some(id)
            );
        }
    }
}
