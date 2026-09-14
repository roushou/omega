//! The session, from logind.
//!
//! It reports whether anybody is using the machine, and serves the five
//! actions that end a session one way or another. What counts as idle is the
//! session manager's policy, which is exactly why it is read from logind
//! rather than decided here.
//!
//! Every call is non-interactive. logind will ask polkit to prompt when a
//! caller allows it, and a broker that allowed it would block on a dialog
//! with the action channel held open behind it — so a session that may not
//! reboot is told so instead of hanging.

use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use zbus::fdo::PropertiesProxy;
use zbus::zvariant::OwnedObjectPath;
use zbus::{Connection, Proxy};

use omega_proto::omega::{IdleState, StatePatch, StateTopic, action, state_topic};
use omega_proto::{ActionKind, SystemTopic};

use crate::broker::{Broker, BrokerError, Cadence, opaque_debug};
use crate::dbus;

/// The system bus, the login manager, and this session on it.
struct Link {
    manager: Proxy<'static>,
    session: Proxy<'static>,
    changes: zbus::fdo::PropertiesChangedStream,
}

impl Link {
    const SERVICE: &'static str = "org.freedesktop.login1";
    const MANAGER: &'static str = "/org/freedesktop/login1";
    const MANAGER_IFACE: &'static str = "org.freedesktop.login1.Manager";
    const SESSION_IFACE: &'static str = "org.freedesktop.login1.Session";

    /// logind resolves the caller's own session under this name, so the
    /// broker does not have to trust `XDG_SESSION_ID` being set or right.
    const OURS: &'static str = "auto";

    async fn open() -> Result<Self, BrokerError> {
        let connection = Connection::system()
            .await
            .map_err(BrokerError::unreadable)?;
        let manager = Proxy::new(
            &connection,
            Self::SERVICE,
            Self::MANAGER,
            Self::MANAGER_IFACE,
        )
        .await
        .map_err(BrokerError::unreadable)?;

        let path: OwnedObjectPath = manager
            .call("GetSession", &(Self::OURS))
            .await
            .map_err(BrokerError::unreadable)?;
        let session = Proxy::new(
            &connection,
            Self::SERVICE,
            path.clone(),
            Self::SESSION_IFACE,
        )
        .await
        .map_err(BrokerError::unreadable)?;

        let properties = PropertiesProxy::builder(&connection)
            .destination(Self::SERVICE)
            .map_err(BrokerError::unreadable)?
            .path(path)
            .map_err(BrokerError::unreadable)?
            .build()
            .await
            .map_err(BrokerError::unreadable)?;
        let changes = properties
            .receive_properties_changed()
            .await
            .map_err(BrokerError::unreadable)?;

        Ok(Self {
            manager,
            session,
            changes,
        })
    }

    /// Whether anybody is using the machine.
    ///
    /// Every field independently: a locked session is not necessarily idle,
    /// and an idle one is not necessarily locked.
    async fn idle(&self) -> IdleState {
        IdleState {
            idle: dbus::property(&self.session, "IdleHint")
                .await
                .unwrap_or(false),
            idle_since: dbus::property(&self.session, "IdleSinceHint")
                .await
                .unwrap_or(0),
            locked: dbus::property(&self.session, "LockedHint")
                .await
                .unwrap_or(false),
        }
    }

    /// Ask the manager for something, without letting it prompt.
    async fn manage(&self, method: &str) -> Result<(), BrokerError> {
        self.manager
            .call_method(method, &(false))
            .await
            .map(|_| ())
            .map_err(BrokerError::unreadable)
    }

    async fn lock(&self) -> Result<(), BrokerError> {
        self.session
            .call_method("Lock", &())
            .await
            .map(|_| ())
            .map_err(BrokerError::unreadable)
    }
}

opaque_debug!(Link);

#[derive(Debug)]
pub struct Logind {
    link: Option<Link>,
    tick: Cadence,
}

impl Default for Logind {
    fn default() -> Self {
        Self::new()
    }
}

impl Logind {
    /// The floor under the signals. logind reports `IdleHint` changing, but
    /// how long it has been idle only moves with the clock.
    pub const REFRESH: Duration = Duration::from_secs(30);

    pub fn new() -> Self {
        Self {
            link: None,
            tick: Cadence::after(Self::REFRESH),
        }
    }

    /// The logind method an action asks for.
    ///
    /// Omega's names and logind's are not the same words for the same things
    /// — `Sleep` is `Suspend`, `Shutdown` is `PowerOff` — and this is the one
    /// place that has to know it.
    fn method(action: &action::Kind) -> Option<&'static str> {
        match action {
            action::Kind::Sleep(_) => Some("Suspend"),
            action::Kind::Hibernate(_) => Some("Hibernate"),
            action::Kind::Reboot(_) => Some("Reboot"),
            action::Kind::Shutdown(_) => Some("PowerOff"),
            _ => None,
        }
    }
}

#[async_trait]
impl Broker for Logind {
    fn name(&self) -> &'static str {
        "logind"
    }

    fn topics(&self) -> &'static [SystemTopic] {
        &[SystemTopic::Idle]
    }

    fn actions(&self) -> &'static [ActionKind] {
        &[
            ActionKind::Lock,
            ActionKind::Sleep,
            ActionKind::Hibernate,
            ActionKind::Reboot,
            ActionKind::Shutdown,
        ]
    }

    fn disconnect(&mut self) {
        self.link = None;
    }

    async fn connect(&mut self) -> Result<(), BrokerError> {
        self.link = Some(Link::open().await?);
        Ok(())
    }

    async fn wake(&mut self) -> Result<(), BrokerError> {
        let Self { link, tick, .. } = self;
        let link = link.as_mut().ok_or_else(BrokerError::gone)?;
        // Both arms are cancel-safe: a signal stream is a receiver, and an
        // interval keeps its own deadline.
        tokio::select! {
            change = link.changes.next() => match change {
                Some(_) => Ok(()),
                None => Err(BrokerError::gone()),
            },
            _ = tick.wait() => Ok(()),
        }
    }

    async fn read(&mut self) -> Result<StatePatch, BrokerError> {
        let link = self.link.as_ref().ok_or_else(BrokerError::gone)?;
        Ok(StatePatch {
            topics: vec![StateTopic {
                topic: SystemTopic::Idle.as_str().into(),
                revision: 0, // the Hub assigns the real revision
                value: Some(state_topic::Value::Idle(link.idle().await)),
            }],
        })
    }

    async fn act(&mut self, action: &action::Kind) -> Result<Option<StatePatch>, BrokerError> {
        // What it does not serve is refused before the connection is touched.
        // Otherwise an action routed here by mistake reports the bus being
        // down, which is a true statement about the wrong thing.
        let method = match action {
            action::Kind::Lock(_) => None,
            other => match Self::method(other) {
                Some(method) => Some(method),
                None => return Err(BrokerError::Unserved(ActionKind::of(other))),
            },
        };

        let link = self.link.as_ref().ok_or_else(BrokerError::gone)?;
        match method {
            Some(method) => link.manage(method).await?,
            None => link.lock().await?,
        }

        // Nothing observable changed that this broker reports: it has no
        // topics, and the machine going to sleep is not news it can deliver.
        Ok(None)
    }
}
