//! Standing in for a unit while somebody works on it.

use omega_proto::UnitName;

use crate::supervisor::Supervisor;

/// The units one connection has taken over, given back when it ends.
///
/// An adoption lasts exactly as long as the connection that asked for it.
/// Tying it to anything else — a timeout, a second op the caller has to
/// remember — would leave a unit dead on the machine because a terminal was
/// closed.
#[derive(Debug)]
pub(super) struct Adoptions {
    supervisor: Supervisor,
    taken: std::sync::Mutex<Vec<UnitName>>,
}

impl Adoptions {
    pub(super) fn new(supervisor: Supervisor) -> Self {
        Self {
            supervisor,
            taken: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub(super) fn taken(&self, name: UnitName) {
        let mut taken = self.taken.lock().unwrap_or_else(|e| e.into_inner());
        if !taken.contains(&name) {
            taken.push(name);
        }
    }
}

impl Drop for Adoptions {
    fn drop(&mut self) {
        for name in self.taken.lock().unwrap_or_else(|e| e.into_inner()).iter() {
            tracing::info!(unit = %name, "adoption ended; returning the unit to the supervisor");
            self.supervisor.release_unit(name);
        }
    }
}
