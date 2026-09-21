//! Send notifications through the session bus and track the ones Omega has
//! raised until the server reports them closed. A freedesktop client cannot
//! enumerate the server's history, so the reading is the daemon's own list.

use std::collections::HashMap;

use async_trait::async_trait;
use futures_util::StreamExt;
use zbus::zvariant::Value;
use zbus::{Connection, MatchRule, MessageStream, Proxy};

use omega_proto::omega::{
    ActiveNotification, NotificationsState, StatePatch, StateTopic, action, state_topic,
};
use omega_proto::{ActionKind, SystemTopic};

use crate::broker::{Broker, BrokerError, opaque_debug};

/// Arguments for `org.freedesktop.Notifications.Notify`.
#[derive(Debug, Clone, PartialEq)]
pub struct Sent {
    pub summary: String,
    pub body: String,
    pub icon: String,
    /// Timeout in milliseconds. Negative selects the notification service's default.
    pub timeout_ms: i32,
}

impl Sent {
    /// What the ontology's notification becomes.
    pub fn of(notify: &omega_proto::omega::Notify) -> Self {
        Self {
            summary: notify.summary.clone(),
            body: notify.body.clone(),
            icon: notify.icon.clone(),
            // Map unspecified timeout to the service default; bus zero means never expire.
            timeout_ms: match notify.timeout_ms {
                0 => -1,
                given => i32::try_from(given).unwrap_or(i32::MAX),
            },
        }
    }
}

struct Link {
    connection: Connection,
    /// `NotificationClosed` signals, matched without borrowing the connection.
    closed: MessageStream,
}

impl Link {
    const SERVICE: &'static str = "org.freedesktop.Notifications";
    const PATH: &'static str = "/org/freedesktop/Notifications";

    async fn open() -> Result<Self, BrokerError> {
        let connection = Connection::session()
            .await
            .map_err(BrokerError::unreadable)?;

        let rule = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(Self::SERVICE)
            .map_err(BrokerError::unreadable)?
            .interface(Self::SERVICE)
            .map_err(BrokerError::unreadable)?
            .member("NotificationClosed")
            .map_err(BrokerError::unreadable)?
            .build();

        let closed = MessageStream::for_match_rule(rule, &connection, None)
            .await
            .map_err(BrokerError::unreadable)?;

        Ok(Self { connection, closed })
    }

    async fn send(&self, sent: &Sent) -> Result<u32, BrokerError> {
        let proxy = Proxy::new(&self.connection, Self::SERVICE, Self::PATH, Self::SERVICE)
            .await
            .map_err(BrokerError::unreadable)?;

        let hints: HashMap<&str, Value<'_>> = HashMap::new();
        let actions: Vec<&str> = Vec::new();
        let reply = proxy
            .call_method(
                "Notify",
                &(
                    // Use the daemon's application name for notification attribution.
                    "omega",
                    // Replaces nothing: each send is a new notification.
                    0u32,
                    sent.icon.as_str(),
                    sent.summary.as_str(),
                    sent.body.as_str(),
                    actions,
                    hints,
                    sent.timeout_ms,
                ),
            )
            .await
            .map_err(BrokerError::unreadable)?;

        reply
            .body()
            .deserialize::<u32>()
            .map_err(BrokerError::unreadable)
    }

    /// Wait for the server to close one notification, returning its id.
    async fn closed_id(&mut self) -> Result<u32, BrokerError> {
        let message = self
            .closed
            .next()
            .await
            .ok_or_else(|| BrokerError::Unreadable("notifications stopped".into()))?
            .map_err(BrokerError::unreadable)?;
        let (id, _reason): (u32, u32) = message
            .body()
            .deserialize()
            .map_err(BrokerError::unreadable)?;
        Ok(id)
    }
}

opaque_debug!(Link);

#[derive(Debug, Default)]
pub struct Notifications {
    link: Option<Link>,
    raised: Vec<ActiveNotification>,
}

impl Notifications {
    pub fn new() -> Self {
        Self::default()
    }

    fn patch(&self) -> StatePatch {
        StatePatch {
            topics: vec![StateTopic {
                topic: SystemTopic::Notifications.as_str().into(),
                revision: 0, // the Hub assigns the real revision
                value: Some(state_topic::Value::Notifications(NotificationsState {
                    notifications: self.raised.clone(),
                })),
            }],
        }
    }

    fn remember(&mut self, id: u32, sent: &Sent) -> StatePatch {
        self.raised.retain(|held| held.id != id);
        self.raised.push(ActiveNotification {
            id,
            summary: sent.summary.clone(),
            body: sent.body.clone(),
            icon: sent.icon.clone(),
        });
        self.patch()
    }

    fn forget(&mut self, id: u32) {
        self.raised.retain(|held| held.id != id);
    }
}

#[async_trait]
impl Broker for Notifications {
    fn name(&self) -> &'static str {
        "notifications"
    }

    fn topics(&self) -> &'static [SystemTopic] {
        &[SystemTopic::Notifications]
    }

    fn actions(&self) -> &'static [ActionKind] {
        &[ActionKind::Notify]
    }

    fn disconnect(&mut self) {
        self.link = None;
        self.raised.clear();
    }

    async fn connect(&mut self) -> Result<(), BrokerError> {
        self.link = Some(Link::open().await?);
        Ok(())
    }

    async fn wake(&mut self) -> Result<(), BrokerError> {
        let id = self
            .link
            .as_mut()
            .ok_or_else(BrokerError::gone)?
            .closed_id()
            .await?;
        self.forget(id);
        Ok(())
    }

    async fn read(&mut self) -> Result<StatePatch, BrokerError> {
        Ok(self.patch())
    }

    async fn act(&mut self, action: &action::Kind) -> Result<Option<StatePatch>, BrokerError> {
        let action::Kind::Notify(notify) = action else {
            return Err(BrokerError::Unserved(ActionKind::of(action)));
        };

        let sent = Sent::of(notify);
        let id = self
            .link
            .as_ref()
            .ok_or_else(BrokerError::gone)?
            .send(&sent)
            .await?;

        // The raised list changed; publish it without waiting for the next wake.
        Ok(Some(self.remember(id, &sent)))
    }
}
