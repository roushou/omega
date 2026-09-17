/// A literal systemd unit name, including its type suffix.
/// Patterns, filesystem paths, and command-line options are rejected.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitName(String);

impl std::str::FromStr for UnitName {
    type Err = NameError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl TryFrom<&str> for UnitName {
    type Error = NameError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl TryFrom<String> for UnitName {
    type Error = NameError;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        let valid_suffix = [
            "service",
            "target",
            "socket",
            "timer",
            "path",
            "mount",
            "automount",
            "swap",
            "slice",
            "scope",
        ];
        let valid = name.len() <= 255
            && !name.starts_with('-')
            && name
                .rsplit_once('.')
                .is_some_and(|(stem, suffix)| !stem.is_empty() && valid_suffix.contains(&suffix));
        if !valid {
            return Err(NameError(name));
        }

        let mut bytes = name.bytes();
        while let Some(byte) = bytes.next() {
            if byte == b'\\' {
                if bytes.next() != Some(b'x')
                    || !bytes.next().is_some_and(|b| b.is_ascii_hexdigit())
                    || !bytes.next().is_some_and(|b| b.is_ascii_hexdigit())
                {
                    return Err(NameError(name));
                }
            } else if !byte.is_ascii_alphanumeric() && !b":_.@-".contains(&byte) {
                return Err(NameError(name));
            }
        }
        Ok(Self(name))
    }
}

impl UnitName {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(super) fn is_service(&self) -> bool {
        self.0.ends_with(".service")
    }
}

impl std::fmt::Display for UnitName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid literal systemd unit name {0:?}")]
pub struct NameError(String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_names_are_literal_and_carry_a_type() {
        for name in [
            "omega.service",
            "graphical-session.target",
            "worker@desk.service",
            r"worker@a\x20b.service",
        ] {
            assert_eq!(UnitName::try_from(name).unwrap().as_str(), name);
        }
        for name in [
            "",
            ".service",
            "omega",
            "../omega.service",
            "-x.service",
            "*.service",
            "a\nb.service",
            "a%.service",
            r"a\q.service",
        ] {
            assert!(UnitName::try_from(name).is_err(), "{name:?}");
        }
    }
}
