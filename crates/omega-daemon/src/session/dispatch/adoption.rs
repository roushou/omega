//! Operator-scoped development adoption of a plugin identity.

use omega_proto::PluginName;

use crate::supervisor::Supervisor;

/// Connection-owned development adoptions, released when the connection closes.
#[derive(Debug)]
pub(super) struct Adoptions {
    supervisor: Supervisor,
    taken: std::sync::Mutex<std::collections::BTreeMap<PluginName, crate::plugins::PluginToken>>,
}

impl Adoptions {
    pub(super) fn new(supervisor: Supervisor) -> Self {
        Self {
            supervisor,
            taken: std::sync::Mutex::new(std::collections::BTreeMap::new()),
        }
    }

    pub(super) fn taken(&self, name: PluginName, token: crate::plugins::PluginToken) {
        let mut taken = self.taken.lock().unwrap_or_else(|e| e.into_inner());
        taken.insert(name, token);
    }
}

impl Drop for Adoptions {
    fn drop(&mut self) {
        for (name, token) in self.taken.lock().unwrap_or_else(|e| e.into_inner()).iter() {
            tracing::info!(plugin = %name, "adoption ended; returning the plugin to the supervisor");
            self.supervisor.release_plugin(name, token);
        }
    }
}
