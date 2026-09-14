//! Desktop-entry identities, never paths or command lines.
use crate::{FromValue, IntoValue, omega::Value};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ApplicationId(String);
impl ApplicationId {
    pub fn parse(id: impl Into<String>) -> Result<Self, ApplicationIdError> {
        let id = id.into();
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
        Self::parse(String::from_value(value)?).ok()
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
            assert!(ApplicationId::parse(bad).is_err(), "{bad:?}");
        }
        for good in [
            "org.gnome.Nautilus.desktop",
            "my app.desktop",
            "subdir-tool.desktop",
        ] {
            let id = ApplicationId::parse(good).unwrap();
            assert_eq!(
                ApplicationId::from_value(&id.clone().into_value()),
                Some(id)
            );
        }
    }
}
