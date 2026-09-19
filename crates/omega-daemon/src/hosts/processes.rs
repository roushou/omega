//! Authentication and connections for supervised command-process incarnations.
use super::startup::Startup;
use crate::{
    Shutdown,
    process::{
        SpawnToken,
        session::{Request, SessionLink},
    },
};
use omega_proto::host::{HostId, InvocationId, ProcessId};
use omega_proto::{Manifest, Refusal, Values};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::{Notify, mpsc};

#[derive(Debug, Clone)]
pub(super) struct Process {
    pub id: ProcessId,
    pub host: HostId,
    pub invocation: Option<InvocationId>,
    pub manifest: Arc<Manifest>,
    pub settings: Values,
    pub identity: crate::process::SpawnIdentity,
    pub session: Option<SessionLink>,
    pub stop: Shutdown,
    startup: Arc<Mutex<Startup>>,
}

#[derive(Debug, Default)]
pub(super) struct Processes {
    next: AtomicU64,
    records: Mutex<BTreeMap<ProcessId, Process>>,
    changed: Notify,
}

impl Processes {
    pub(super) fn inspect(&self, host: &HostId) -> Vec<omega_proto::omega::CommandProcessStatus> {
        self.records
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .filter(|process| &process.host == host)
            .map(|process| omega_proto::omega::CommandProcessStatus {
                id: process.id.get(),
                phase: if process.stop.is_triggered() {
                    "stopping"
                } else if process.session.is_some() {
                    "running"
                } else {
                    "starting"
                }
                .into(),
            })
            .collect()
    }

    pub(super) fn contains(&self, id: ProcessId) -> bool {
        self.records
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(&id)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.records
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty()
    }

    pub(super) fn prepare(
        &self,
        host: HostId,
        invocation: Option<InvocationId>,
        manifest: Arc<Manifest>,
        settings: Values,
        startup: Arc<Mutex<Startup>>,
    ) -> Result<Process, Refusal> {
        let next = self
            .next
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| Refusal::exhausted("process identities exhausted"))?
            + 1;
        let id =
            ProcessId::try_from(next).map_err(|error| Refusal::precondition(error.to_string()))?;
        let process = Process {
            id,
            host,
            invocation,
            manifest,
            settings,
            identity: crate::process::SpawnIdentity::new(
                SpawnToken::mint().map_err(|error| Refusal::unavailable(error.to_string()))?,
            ),
            session: None,
            stop: Shutdown::new(),
            startup,
        };
        self.records
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, process.clone());
        Ok(process)
    }
    pub(super) fn bind(&self, id: ProcessId, pid: i32) {
        if let Some(process) = self
            .records
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(&id)
        {
            process.identity.bind(pid);
        }
    }
    pub(super) fn authenticate(
        &self,
        pid: i32,
        token: &str,
        hash: &str,
    ) -> Result<Option<Process>, Refusal> {
        let mut records = self.records.lock().unwrap_or_else(|e| e.into_inner());
        let Some(process) = records
            .values_mut()
            .find(|process| process.identity.token().as_str() == token)
        else {
            return Ok(None);
        };
        if process.stop.is_triggered() {
            return Err(Refusal::unauthenticated("command process is stopping"));
        }
        if !process.identity.claims(pid, token) {
            return Err(Refusal::unauthenticated(
                "command host PID does not match spawn",
            ));
        }
        if process.manifest.hash() != hash {
            return Err(Refusal::precondition("command host manifest mismatch"));
        }
        if process.session.is_some() {
            return Err(Refusal::precondition("command process already connected"));
        }
        process.identity.bind(pid);
        Ok(Some(process.clone()))
    }
    pub(super) fn connect(
        self: &Arc<Self>,
        id: ProcessId,
        requests: mpsc::Sender<Request>,
    ) -> Result<Connection, Refusal> {
        let mut records = self.records.lock().unwrap_or_else(|e| e.into_inner());
        let process = records
            .get_mut(&id)
            .ok_or_else(|| Refusal::unavailable("command process expired"))?;
        if process.session.is_some() {
            return Err(Refusal::precondition("command process already connected"));
        }
        if process.stop.is_triggered() {
            return Err(Refusal::unavailable("command process is stopping"));
        }
        process
            .startup
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .succeeded();
        process.session = Some(SessionLink {
            manifest: Some(process.manifest.clone()),
            bytes: Arc::new(tokio::sync::Semaphore::new(8 * 1024 * 1024)),
            requests,
            stop: process.stop.clone(),
        });
        let stop = process.stop.clone();
        drop(records);
        self.changed.notify_waiters();
        Ok(Connection {
            processes: self.clone(),
            id,
            stop,
        })
    }
    pub(super) fn fail_startup(&self, id: ProcessId, message: &str) {
        let records = self.records.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(process) = records.get(&id)
            && process.session.is_none()
            && !process.stop.is_triggered()
        {
            process
                .startup
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .failed(message);
            process.stop.trigger();
        }
        drop(records);
        self.changed.notify_waiters();
    }

    pub(super) async fn ready(&self, id: ProcessId) -> Result<SessionLink, Refusal> {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            {
                let records = self.records.lock().unwrap_or_else(|e| e.into_inner());
                let process = records.get(&id).ok_or_else(|| {
                    Refusal::unavailable("command process exited before connecting")
                })?;
                if process.stop.is_triggered() {
                    return Err(Refusal::unavailable("command process disconnected"));
                }
                if let Some(session) = &process.session {
                    return Ok(session.clone());
                }
            }
            changed.await;
        }
    }
    pub(super) async fn reaped(&self, id: ProcessId) {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            if !self
                .records
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .contains_key(&id)
            {
                return;
            }
            changed.await;
        }
    }
    pub(super) fn remove(&self, id: ProcessId) {
        if let Some(process) = self
            .records
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id)
        {
            process.stop.trigger();
        }
        self.changed.notify_waiters();
    }
    pub(super) fn disconnect(&self, id: ProcessId) {
        if let Some(process) = self
            .records
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
        {
            process.stop.trigger();
        }
        self.changed.notify_waiters();
    }
}

#[derive(Debug)]
pub(crate) struct Connection {
    processes: Arc<Processes>,
    id: ProcessId,
    stop: Shutdown,
}

impl Connection {
    pub(crate) async fn cancelled(&self) {
        self.stop.wait().await;
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.processes.disconnect(self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn startup_failure_cannot_overwrite_a_connected_process_or_admit_a_stopped_one() {
        let processes = Arc::new(Processes::default());
        let startup = Arc::new(Mutex::new(Startup::default()));
        startup.lock().unwrap().failed("previous failure");
        let process = processes
            .prepare(
                "worker".parse().unwrap(),
                None,
                Arc::new(Manifest::new(&"worker".parse().unwrap(), "1")),
                Values::new(),
                startup.clone(),
            )
            .unwrap();
        let (sender, _) = mpsc::channel(1);
        let connection = processes.connect(process.id, sender).unwrap();
        assert!(startup.lock().unwrap().error().is_none());
        drop(connection);
        processes.fail_startup(process.id, "exited before handshake");
        assert!(startup.lock().unwrap().error().is_none());
        processes.remove(process.id);

        let process = processes
            .prepare(
                "worker".parse().unwrap(),
                None,
                Arc::new(Manifest::new(&"worker".parse().unwrap(), "1")),
                Values::new(),
                startup.clone(),
            )
            .unwrap();
        processes.fail_startup(process.id, "handshake timed out");
        processes.fail_startup(process.id, "process disconnected");
        assert_eq!(startup.lock().unwrap().error(), Some("handshake timed out"));
        let (sender, _) = mpsc::channel(1);
        assert!(processes.connect(process.id, sender).is_err());
        processes.remove(process.id);
    }

    #[tokio::test]
    async fn authentication_pins_pid_manifest_and_connection_and_revokes_on_exit() {
        let processes = Arc::new(Processes::default());
        let manifest = Arc::new(Manifest::new(&"worker".parse().unwrap(), "1"));
        let process = processes
            .prepare(
                "worker".parse().unwrap(),
                None,
                manifest.clone(),
                Values::new(),
                Arc::new(Mutex::new(Startup::default())),
            )
            .unwrap();
        processes.bind(process.id, 42);
        assert!(
            processes
                .authenticate(43, process.identity.token().as_str(), &manifest.hash())
                .is_err()
        );
        assert!(
            processes
                .authenticate(42, process.identity.token().as_str(), "wrong")
                .is_err()
        );
        assert!(
            processes
                .authenticate(42, "unknown", &manifest.hash())
                .unwrap()
                .is_none()
        );
        assert!(
            processes
                .authenticate(42, process.identity.token().as_str(), &manifest.hash())
                .unwrap()
                .is_some()
        );
        let (sender, _receiver) = mpsc::channel(1);
        let guard = processes.connect(process.id, sender.clone()).unwrap();
        assert!(processes.connect(process.id, sender).is_err());
        assert!(processes.ready(process.id).await.is_ok());
        drop(guard);
        assert!(processes.ready(process.id).await.is_err());
        assert!(
            processes
                .authenticate(42, process.identity.token().as_str(), &manifest.hash())
                .is_err()
        );
        processes.remove(process.id);
        processes.reaped(process.id).await;
        assert!(processes.is_empty());
    }
}
