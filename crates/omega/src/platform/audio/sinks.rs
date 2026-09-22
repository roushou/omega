//! Available output sinks and default-device selection.

use omega_proto::omega::SinkInfo;

crate::wiring::reading! {
    /// The output sinks a machine can send audio to.
    Sinks: omega_proto::omega::AudioSinksState
}

/// One output sink, as PulseAudio reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct Sink {
    name: String,
    description: String,
}

impl From<SinkInfo> for Sink {
    fn from(sink: SinkInfo) -> Self {
        Self {
            name: sink.name,
            description: sink.description,
        }
    }
}

impl Sink {
    /// The PulseAudio sink name, used to select the default device.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The human-readable description.
    pub fn description(&self) -> &str {
        &self.description
    }
}

impl Sinks {
    /// The available sinks, or an empty collection when unavailable.
    pub fn all(&self) -> Vec<Sink> {
        self.read()
            .map(|state| state.sinks.into_iter().map(Sink::from).collect())
            .unwrap_or_default()
    }

    /// The sink with this name, or `None` when it is not available.
    pub fn at(&self, name: &str) -> Option<Sink> {
        self.all().into_iter().find(|sink| sink.name == name)
    }
}
