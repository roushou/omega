//! Validated MPRIS player identities.
use crate::{FromValue, IntoValue, omega::Value};
use std::fmt;

/// A player's well-known bus-name suffix, including any instance component.
/// This names an application endpoint, not a particular process lifetime.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlayerId(String);

impl std::str::FromStr for PlayerId {
    type Err = PlayerIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl TryFrom<&str> for PlayerId {
    type Error = PlayerIdError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl TryFrom<String> for PlayerId {
    type Error = PlayerIdError;

    /// Parse the suffix of an MPRIS well-known bus name.
    fn try_from(id: String) -> Result<Self, Self::Error> {
        if id.len() + "org.mpris.MediaPlayer2.".len() > 255
            || !id.split('.').all(|part| {
                !part.is_empty()
                    && !part.as_bytes()[0].is_ascii_digit()
                    && part
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
            })
        {
            return Err(PlayerIdError(id));
        }
        Ok(Self(id))
    }
}

impl PlayerId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PlayerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A suffix that cannot identify an MPRIS player.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid media player id: {0:?}")]
pub struct PlayerIdError(String);

impl FromValue for PlayerId {
    fn from_value(value: &Value) -> Option<Self> {
        Self::try_from(String::from_value(value)?).ok()
    }
}
impl IntoValue for PlayerId {
    fn into_value(self) -> Value {
        self.0.into_value()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ids_obey_well_known_bus_name_rules() {
        for id in ["vlc", "chromium.instance123", "a-b", "_player", "UPPER"] {
            assert!(PlayerId::try_from(id).is_ok(), "{id}");
        }
        for id in [
            "",
            ".vlc",
            "vlc.",
            "vlc..instance",
            "123",
            "vlc.123",
            "/vlc",
            "vlc name",
            "é",
        ] {
            assert!(PlayerId::try_from(id).is_err(), "{id}");
        }
        assert!(PlayerId::try_from("a".repeat(232)).is_ok());
        assert!(PlayerId::try_from("a".repeat(233)).is_err());
    }
    #[test]
    fn an_explicit_empty_target_is_not_automatic_selection() {
        use crate::{
            ActionKind,
            omega::{MediaKey, action, media_key},
        };
        let action = action::Kind::MediaKey(MediaKey {
            key: media_key::Key::MediaPlay as i32,
            player_id: Some(String::new()),
        });
        assert_eq!(ActionKind::of(&action), ActionKind::MediaKey);
        assert!(action.validate().is_err());
    }
    #[test]
    fn a_peer_that_cannot_route_targets_is_refused() {
        assert!(crate::Handshake::negotiate(crate::MIN_PROTOCOL_VERSION - 1).is_err());
    }
}
