//! Operator-scoped development adoption of a plugin identity.

use omega_proto::UnitName;

use crate::supervisor::Supervisor;

/// Connection-owned development adoptions, released when the connection closes.
#[derive(Debug)]
pub(super) struct Adoptions {
    supervisor: Supervisor,
    taken: std::sync::Mutex<std::collections::BTreeMap<UnitName, crate::units::UnitToken>>,
}

impl Adoptions {
    pub(super) fn new(supervisor: Supervisor) -> Self {
        Self {
            supervisor,
            taken: std::sync::Mutex::new(std::collections::BTreeMap::new()),
        }
    }

    pub(super) fn taken(&self, name: UnitName, token: crate::units::UnitToken) {
        let mut taken = self.taken.lock().unwrap_or_else(|e| e.into_inner());
        taken.insert(name, token);
    }
}

impl Drop for Adoptions {
    fn drop(&mut self) {
        for (name, token) in self.taken.lock().unwrap_or_else(|e| e.into_inner()).iter() {
            tracing::info!(unit = %name, "adoption ended; returning the unit to the supervisor");
            self.supervisor.release_unit(name, token);
        }
    }
}
