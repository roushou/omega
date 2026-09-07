//! Ordered shutdown.
//!
//! A daemon that simply returns from `main` takes its units down with a
//! `SIGKILL` from `kill_on_drop`: no chance to flush, no chance to exit
//! cleanly. This is the signal every long-lived task selects on so the daemon
//! can stop accepting, ask its units to leave, and wait for them.

use tokio::sync::watch;

#[derive(Debug, Clone)]
pub struct Shutdown {
    tx: watch::Sender<bool>,
    rx: watch::Receiver<bool>,
}

impl Shutdown {
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(false);
        Self { tx, rx }
    }

    /// Begin shutting down. Idempotent: the second signal changes nothing,
    /// which is what makes a doubled Ctrl-C harmless.
    pub fn trigger(&self) {
        let _ = self.tx.send(true);
    }

    pub fn is_triggered(&self) -> bool {
        *self.rx.borrow()
    }

    /// Resolves when shutdown begins, immediately if it already has.
    pub async fn wait(&self) {
        let mut rx = self.rx.clone();
        let _ = rx.wait_for(|triggered| *triggered).await;
    }
}

impl Default for Shutdown {
    fn default() -> Self {
        Self::new()
    }
}
