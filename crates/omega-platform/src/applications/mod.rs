//! GIO owns desktop-entry interpretation and activation on a dedicated main context.
mod worker;
use crate::{Broker, BrokerError};
use async_trait::async_trait;
use omega_proto::{
    ActionKind, SystemTopic,
    omega::{StatePatch, StateTopic, action, state_topic},
};

/// Catalogue observation and application activation share one GIO main context.
#[derive(Debug, Default)]
pub struct Applications {
    worker: Option<worker::Worker>,
}
impl Applications {
    pub fn new() -> Self {
        Self::default()
    }
    fn worker(&mut self) -> Result<&mut worker::Worker, BrokerError> {
        self.worker.as_mut().ok_or_else(BrokerError::gone)
    }
}
#[async_trait]
impl Broker for Applications {
    fn name(&self) -> &'static str {
        "applications"
    }
    fn topics(&self) -> &'static [SystemTopic] {
        &[SystemTopic::Applications]
    }
    fn actions(&self) -> &'static [ActionKind] {
        &[ActionKind::LaunchApp]
    }
    async fn connect(&mut self) -> Result<(), BrokerError> {
        // A stalled native worker must not accumulate replacement threads.
        if self.worker.as_ref().is_none_or(worker::Worker::is_closed) {
            self.worker = Some(worker::Worker::start()?);
        }
        Ok(())
    }
    async fn wake(&mut self) -> Result<(), BrokerError> {
        self.worker()?.changed().await
    }
    async fn read(&mut self) -> Result<StatePatch, BrokerError> {
        let applications = self.worker()?.catalogue().await?;
        Ok(StatePatch {
            topics: vec![StateTopic {
                topic: SystemTopic::Applications.to_string(),
                revision: 0,
                value: Some(state_topic::Value::Applications(applications)),
            }],
        })
    }
    async fn act(&mut self, action: &action::Kind) -> Result<Option<StatePatch>, BrokerError> {
        match action {
            action::Kind::LaunchApp(app) => self.worker()?.launch(app.clone()).await?,
            other => return Err(BrokerError::Unserved(ActionKind::of(other))),
        }
        Ok(None)
    }
}
