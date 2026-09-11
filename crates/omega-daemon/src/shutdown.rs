//! Ordered shutdown.
//!
//! A daemon that simply returns from `main` takes its units down with a
//! `SIGKILL` from `kill_on_drop`: no chance to flush, no chance to exit
//! cleanly. This is the signal every long-lived task selects on so the daemon
//! can stop accepting, ask its units to leave, and wait for them.

use futures_util::FutureExt;
use tokio::sync::watch;

#[derive(Debug, Clone)]
pub struct Shutdown {
    tx: watch::Sender<Reason>,
    rx: watch::Receiver<Reason>,
}

#[derive(Debug, Clone)]
enum Reason {
    Running,
    Requested,
    Failed(TaskFailure),
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{task} failed: {message}")]
pub struct TaskFailure {
    task: String,
    message: String,
}

impl Shutdown {
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(Reason::Running);
        Self { tx, rx }
    }

    /// Begin shutting down. Idempotent: the second signal changes nothing,
    /// which is what makes a doubled Ctrl-C harmless.
    pub fn trigger(&self) {
        self.tx.send_if_modified(|reason| {
            if matches!(reason, Reason::Running) {
                *reason = Reason::Requested;
                true
            } else {
                false
            }
        });
    }

    pub fn is_triggered(&self) -> bool {
        !matches!(*self.rx.borrow(), Reason::Running)
    }

    pub fn failure(&self) -> Option<TaskFailure> {
        match &*self.rx.borrow() {
            Reason::Failed(error) => Some(error.clone()),
            _ => None,
        }
    }

    /// Critical task panics stop the daemon; retryable operation errors stay
    /// with the subsystem that knows how to recover.
    pub(crate) async fn supervise(
        &self,
        task: String,
        future: impl std::future::Future<Output = ()>,
    ) {
        if let Err(payload) = std::panic::AssertUnwindSafe(future).catch_unwind().await {
            let message = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "non-string panic payload".into());
            let error = TaskFailure { task, message };
            tracing::error!(%error, "critical background task failed");
            self.tx.send_if_modified(|reason| {
                if matches!(reason, Reason::Failed(_)) {
                    false
                } else {
                    *reason = Reason::Failed(error.clone());
                    true
                }
            });
            std::panic::resume_unwind(payload);
        }
    }

    /// Resolves when shutdown begins, immediately if it already has.
    pub async fn wait(&self) {
        let mut rx = self.rx.clone();
        let _ = rx
            .wait_for(|reason| !matches!(reason, Reason::Running))
            .await;
    }
}

impl Default for Shutdown {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn task_failures_wake_shutdown_and_survive_a_later_stop_request() {
        let shutdown = Shutdown::new();
        let watched = shutdown.clone();
        let task = tokio::spawn(async move {
            watched
                .supervise("broker example".into(), async {
                    panic!("broken invariant")
                })
                .await;
        });
        shutdown.wait().await;
        assert!(task.await.unwrap_err().is_panic());
        shutdown.trigger();
        let failure = shutdown.failure().unwrap();
        assert_eq!(failure.task, "broker example");
        assert_eq!(failure.message, "broken invariant");
    }

    #[tokio::test]
    async fn normal_completion_and_cancellation_are_not_task_failures() {
        let shutdown = Shutdown::new();
        shutdown.supervise("normal".into(), async {}).await;
        let watched = shutdown.clone();
        let task = tokio::spawn(async move {
            watched
                .supervise("cancelled".into(), std::future::pending())
                .await;
        });
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(!shutdown.is_triggered());
        assert!(shutdown.failure().is_none());
    }
}
