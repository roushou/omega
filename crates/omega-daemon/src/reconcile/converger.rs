//! The one thing that converges.
//!
//! Convergence is triggered from several places — the daemon starting, a
//! rebuild landing, a unit connecting — and it can take a while: rendering a
//! bar instance waits on a unit, which may be slow or wedged. Two things
//! follow from that, and both are structural rather than careful:
//!
//! - it runs in its own task, so a slow unit cannot stop the daemon from
//!   accepting connections or answering a signal;
//! - it runs *one at a time*, and requests that arrive while it is working
//!   are merged into the next pass rather than starting a second one.
//!
//! So a trigger is a request, never a call.

use std::sync::{Arc, Mutex};

use tokio::sync::Notify;

use omega_core::Layout;
use omega_document::{DocumentFile, StateDocument};
use omega_manifest::StateConfig;

use crate::error::DaemonError;
use crate::hub::Hub;
use crate::manifest::ManifestStore;
use crate::reconcile::{
    BarProvider, ConfigProvider, EnvironmentProvider, Reconciler, UnitProvider,
};
use crate::shutdown::Shutdown;
use crate::supervisor::Supervisor;
use crate::units::UnitTable;

/// What a pass has to do. Merging is a union: a reload asked for while a
/// plain convergence is pending upgrades the pending one rather than queuing
/// behind it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Work {
    /// Re-read the built state: what was built, what it is for, and the
    /// manifests the daemon vouches for.
    pub reload: bool,
}

impl Work {
    /// Plan against what is already loaded.
    pub const CONVERGE: Self = Self { reload: false };
    /// A build landed; everything derived from the state dir is stale.
    pub const REBUILD: Self = Self { reload: true };

    pub fn merge(self, other: Self) -> Self {
        Self {
            reload: self.reload || other.reload,
        }
    }
}

/// The queue of pending work: at most one pass, however many triggers.
#[derive(Debug, Default)]
struct Queue {
    pending: Mutex<Option<Work>>,
    wake: Notify,
}

impl Queue {
    fn push(&self, work: Work) {
        let mut pending = self.lock();
        *pending = Some(pending.map_or(work, |queued| queued.merge(work)));
        drop(pending);

        // `Notify` holds a permit, so a request that arrives before the
        // worker waits is not lost.
        self.wake.notify_one();
    }

    fn take(&self) -> Option<Work> {
        self.lock().take()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Work>> {
        self.pending.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// A handle on the converger. Cloneable, and never blocks the caller.
#[derive(Debug, Clone)]
pub struct Converger {
    queue: Arc<Queue>,
}

impl Converger {
    /// Start converging in the background.
    pub fn spawn(context: Context, shutdown: Shutdown) -> Self {
        let converger = Self {
            queue: Arc::new(Queue::default()),
        };

        let worker = Worker {
            queue: converger.queue.clone(),
            document: StateDocument::default(),
            context,
            shutdown,
        };
        tokio::spawn(worker.run());

        // Everything the daemon runs comes from the built state, so the first
        // pass reads it.
        converger.request(Work::REBUILD);
        converger
    }

    /// Ask for a pass. Returns immediately: whether one is running, pending,
    /// or neither, the request is recorded and nothing waits.
    pub fn request(&self, work: Work) {
        self.queue.push(work);
    }
}

/// Everything a pass needs to converge with.
#[derive(Debug, Clone)]
pub struct Context {
    pub layout: Layout,
    pub hub: Hub,
    pub supervisor: Supervisor,
    pub units: UnitTable,
}

struct Worker {
    queue: Arc<Queue>,
    document: StateDocument,
    context: Context,
    shutdown: Shutdown,
}

impl Worker {
    async fn run(mut self) {
        loop {
            let Some(work) = self.next().await else {
                return;
            };

            if work.reload {
                match self.reload() {
                    Ok(document) => self.document = document,
                    // A half-written or broken build must not take down a
                    // daemon that is running the last good one.
                    Err(e) => {
                        tracing::error!(error = %e, "cannot adopt the new build; keeping the running one")
                    }
                }
            }

            if let Err(e) = self.converge().await {
                tracing::warn!(error = %e, "cannot converge");
            }
        }
    }

    /// The next pass to run, or `None` when the daemon is stopping.
    async fn next(&self) -> Option<Work> {
        loop {
            if let Some(work) = self.queue.take() {
                return Some(work);
            }

            tokio::select! {
                _ = self.queue.wake.notified() => continue,
                _ = self.shutdown.wait() => return None,
            }
        }
    }

    /// Re-read everything derived from the state dir. The manifests the
    /// daemon vouches for are replaced together with the document that says
    /// what to do with them.
    fn reload(&self) -> Result<StateDocument, DaemonError> {
        let config = self.context.layout.file::<StateConfig>(()).read()?;
        let manifests = ManifestStore::load(&config, &self.context.layout)?;
        self.context.supervisor.adopt(Arc::new(manifests));

        Ok(DocumentFile::of(&self.context.layout).read_or_default()?)
    }

    async fn converge(&self) -> Result<(), DaemonError> {
        let config = self.context.layout.file::<StateConfig>(()).read()?;
        let manifests = Arc::new(ManifestStore::load(&config, &self.context.layout)?);

        Reconciler::new()
            // Config first: a unit must not be started before the settings
            // its own handshake will carry are on file.
            .with(ConfigProvider::new(
                self.context.units.clone(),
                self.context.supervisor.clone(),
            ))
            .with(UnitProvider::new(
                self.context.supervisor.clone(),
                &self.context.layout,
                config.names().cloned(),
            ))
            .with(EnvironmentProvider::new(&self.context.layout))
            // Bars last: a widget instance can only be rendered by a unit
            // that is already running.
            .with(BarProvider::new(
                self.context.hub.clone(),
                self.context.units.clone(),
                manifests,
            ))
            .converge(&self.document)
            .await;

        self.context.supervisor.publish_status();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triggers_that_arrive_together_are_one_pass() {
        let queue = Queue::default();

        queue.push(Work::CONVERGE);
        queue.push(Work::CONVERGE);
        queue.push(Work::CONVERGE);

        assert_eq!(queue.take(), Some(Work::CONVERGE));
        assert_eq!(queue.take(), None, "one pass answers them all");
    }

    #[test]
    fn a_rebuild_upgrades_a_pending_pass() {
        let queue = Queue::default();

        // A unit connected, and then a build landed before the pass ran: the
        // pass has to re-read the build, not plan against the old one.
        queue.push(Work::CONVERGE);
        queue.push(Work::REBUILD);
        assert_eq!(queue.take(), Some(Work::REBUILD));

        // ...and the other way round: a plain convergence does not downgrade
        // a pending rebuild.
        queue.push(Work::REBUILD);
        queue.push(Work::CONVERGE);
        assert_eq!(queue.take(), Some(Work::REBUILD));
    }
}
