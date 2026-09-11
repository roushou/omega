//! Desktop notifications, over the session bus.
//!
//! A write-only broker like logind, and for the same reason: what a
//! notification daemon knows — what is on screen, what was dismissed — is
//! worth a topic and does not have one. Sending is what units ask for.
//!
//! The session bus, not the system one. Notifications belong to a logged-in
//! session and the daemon runs inside one.

use std::collections::HashMap;

use async_trait::async_trait;
use zbus::zvariant::Value;
use zbus::{Connection, Proxy};

use omega_proto::omega::{StatePatch, action};
use omega_proto::{ActionKind, SystemTopic};

use crate::broker::{Broker, BrokerError, opaque_debug};

/// One notification, as `org.freedesktop.Notifications` takes it.
///
/// The signature is eight positional arguments, four of which Omega has no
/// opinion about. Naming them here is what stops the call site being a row of
/// bare literals nobody can read.
#[derive(Debug, Clone, PartialEq)]
pub struct Sent {
    pub summary: String,
    pub body: String,
    pub icon: String,
    /// Milliseconds. Negative is the daemon's own default, which is what a
    /// unit that said nothing means.
    pub timeout_ms: i32,
}

impl Sent {
    /// What the ontology's notification becomes.
    pub fn of(notify: &omega_proto::omega::Notify) -> Self {
        Self {
            summary: notify.summary.clone(),
            body: notify.body.clone(),
            icon: notify.icon.clone(),
            // Zero is "no timeout given" on the wire and "never expire" on
            // the bus. A unit that asked for nothing wants the desktop's
            // default, not a notification that stays until it is clicked.
            timeout_ms: match notify.timeout_ms {
                0 => -1,
                given => i32::try_from(given).unwrap_or(i32::MAX),
            },
        }
    }
}

struct Link {
    notifications: Proxy<'static>,
}

impl Link {
    const SERVICE: &'static str = "org.freedesktop.Notifications";
    const PATH: &'static str = "/org/freedesktop/Notifications";

    async fn open() -> Result<Self, BrokerError> {
        let connection = Connection::session()
            .await
            .map_err(BrokerError::unreadable)?;
        let notifications = Proxy::new(&connection, Self::SERVICE, Self::PATH, Self::SERVICE)
            .await
            .map_err(BrokerError::unreadable)?;
        Ok(Self { notifications })
    }

    async fn send(&self, sent: &Sent) -> Result<(), BrokerError> {
        let hints: HashMap<&str, Value<'_>> = HashMap::new();
        let actions: Vec<&str> = Vec::new();
        self.notifications
            .call_method(
                "Notify",
                &(
                    // Who it is from. Every notification in the tray says
                    // "omega" rather than the unit that sent it, because the
                    // daemon is the one holding the bus — a unit naming
                    // itself here would be a unit choosing how it is
                    // attributed.
                    "omega",
                    // Replaces nothing: Omega has no notification ids yet, so
                    // every send is a new one.
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
            .map(|_| ())
            .map_err(BrokerError::unreadable)
    }
}

opaque_debug!(Link);

#[derive(Debug, Default)]
pub struct Notifications {
    link: Option<Link>,
}

impl Notifications {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Broker for Notifications {
    fn name(&self) -> &'static str {
        "notifications"
    }

    /// None yet. What is on screen and what was dismissed is worth a topic,
    /// and claiming one it does not fill would make a unit wait forever.
    fn topics(&self) -> &'static [SystemTopic] {
        &[]
    }

    fn actions(&self) -> &'static [ActionKind] {
        &[ActionKind::Notify]
    }

    fn disconnect(&mut self) {
        self.link = None;
    }

    async fn connect(&mut self) -> Result<(), BrokerError> {
        self.link = Some(Link::open().await?);
        Ok(())
    }

    async fn act(&mut self, action: &action::Kind) -> Result<Option<StatePatch>, BrokerError> {
        let action::Kind::Notify(notify) = action else {
            return Err(BrokerError::Unserved(ActionKind::of(action)));
        };

        self.link
            .as_ref()
            .ok_or_else(BrokerError::gone)?
            .send(&Sent::of(notify))
            .await?;
        Ok(None)
    }
}
