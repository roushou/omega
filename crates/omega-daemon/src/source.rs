//! State sources: producers the daemon brokers.

use std::time::Duration;

use async_trait::async_trait;
use tokio::time::MissedTickBehavior;

use omega_proto::omega::StatePatch;

use crate::hub::Hub;

/// A producer of state. Sources poll on their own interval; the [`Hub`] owns
/// values and revisions.
#[async_trait]
pub trait StateSource: Send + 'static {
    /// A poll failure is logged; the source keeps ticking.
    type Error: std::error::Error + Send + Sync + 'static;

    fn name(&self) -> &'static str;

    fn interval(&self) -> Duration {
        Duration::from_secs(2)
    }

    async fn poll(&mut self) -> Result<StatePatch, Self::Error>;

    /// Start polling on the runtime, publishing into `hub`.
    fn spawn(mut self, hub: Hub) -> tokio::task::JoinHandle<()>
    where
        Self: Sized,
    {
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(self.interval());
            tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

            loop {
                tick.tick().await;
                match self.poll().await {
                    Ok(patch) => hub.publish_state(patch),
                    Err(e) => {
                        tracing::error!(source = self.name(), error = %e, "state source failed")
                    }
                }
            }
        })
    }
}
