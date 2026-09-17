//! Availability and validation of a received platform reading.

use omega_proto::SystemTopic;

/// A received snapshot, including its availability and validation outcome.
/// Empty collections inside `Ready` are successful readings. Invalid readings
/// replace older data and recover when a newer valid revision arrives.
///
/// ```
/// use omega::platform::{Reading, applications::Applications};
/// fn summary(apps: &Applications) -> String {
///     match apps.snapshot() {
///         Reading::Ready(apps) => format!("{} applications", apps.len()),
///         Reading::Pending => "Loading applications".into(),
///         Reading::Unavailable => "Applications unavailable".into(),
///         Reading::Invalid(error) => error.to_string(),
///     }
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Reading<T> {
    /// The daemon has not reported this topic yet.
    #[default]
    Pending,
    /// The daemon explicitly reported no current value.
    Unavailable,
    /// The received value failed validation; no partial collection is exposed.
    Invalid(ReadingError),
    /// A complete, validated value.
    Ready(T),
}

impl<T> Reading<T> {
    /// Consume a valid value. Inspect the enum to distinguish other outcomes.
    pub fn into_value(self) -> Option<T> {
        match self {
            Self::Ready(value) => Some(value),
            _ => None,
        }
    }
}

/// A malformed platform reading at a particular wire revision.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("invalid {topic} reading at revision {revision}: {message}")]
pub struct ReadingError {
    pub topic: SystemTopic,
    pub revision: u64,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{applications::Applications, audio::Media, bluetooth::Bluetooth};
    use crate::{
        Surface, View,
        surface::{Events, Task},
        testing::{Drawn, State},
    };
    use omega_proto::omega::{
        Application, ApplicationsState, BluetoothDevice, BluetoothState, MediaState, PlayerInfo,
    };

    #[derive(crate::Surface)]
    struct Readings {
        applications: Applications,
        media: Media,
        bluetooth: Bluetooth,
    }
    impl Surface for Readings {
        type Model = ();
        type Message = std::convert::Infallible;
        type Effects = ();
        fn update(&self, _: &mut (), message: Self::Message, _: &()) -> Task<Self::Message> {
            match message {}
        }
        fn render(&self, _: &(), _: &Events<Self::Message>) -> View {
            assert!(matches!(self.applications.snapshot(), Reading::Invalid(_)));
            assert!(matches!(self.media.snapshot(), Reading::Invalid(_)));
            assert!(matches!(self.bluetooth.snapshot(), Reading::Invalid(_)));
            assert!(self.applications.entries().is_none());
            assert!(self.media.players().is_empty());
            assert!(self.bluetooth.known_devices().is_empty());
            assert!(!self.media.has_reading());
            assert!(self.bluetooth.get().is_none());
            crate::ui::Text::new(self.applications.reading_error().unwrap()).into()
        }
    }

    #[test]
    fn malformed_fixtures_reach_typed_accessors_without_panicking() {
        let state = State::new()
            .with(ApplicationsState {
                applications: vec![Application {
                    id: "../bad.desktop".into(),
                    ..Default::default()
                }],
            })
            .with(MediaState {
                players: vec![PlayerInfo {
                    id: "bad player".into(),
                    ..Default::default()
                }],
            })
            .with(BluetoothState {
                devices: vec![BluetoothDevice {
                    id: "bad path".into(),
                    ..Default::default()
                }],
                ..Default::default()
            });
        let drawn = Drawn::of::<Readings>(&state).unwrap();
        assert!(
            drawn
                .text()
                .contains("invalid applications reading at revision 1")
        );
    }
}
