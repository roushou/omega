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

use crate::broker::{Broker, BrokerError, Cadence};

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
        let connection = Connection::system().await.map_err(Self::unreadable)?;
        let manager = Proxy::new(
            &connection,
            Self::SERVICE,
            Self::MANAGER,
            Self::MANAGER_IFACE,
        )
        .await
        .map_err(Self::unreadable)?;

        let path: OwnedObjectPath = manager
            .call("GetSession", &(Self::OURS))
            .await
            .map_err(Self::unreadable)?;
        let session = Proxy::new(
            &connection,
            Self::SERVICE,
            path.clone(),
            Self::SESSION_IFACE,
        )
        .await
        .map_err(Self::unreadable)?;

        let properties = PropertiesProxy::builder(&connection)
            .destination(Self::SERVICE)
            .map_err(Self::unreadable)?
            .path(path)
            .map_err(Self::unreadable)?
            .build()
            .await
            .map_err(Self::unreadable)?;
        let changes = properties
            .receive_properties_changed()
            .await
            .map_err(Self::unreadable)?;

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
            idle: self.property("IdleHint").await.unwrap_or(false),
            idle_since: self.property("IdleSinceHint").await.unwrap_or(0),
            locked: self.property("LockedHint").await.unwrap_or(false),
        }
    }

    async fn property<T>(&self, name: &str) -> Option<T>
    where
        T: TryFrom<zbus::zvariant::OwnedValue>,
        <T as TryFrom<zbus::zvariant::OwnedValue>>::Error: Into<zbus::Error>,
    {
        self.session.get_property(name).await.ok()
    }

    /// Ask the manager for something, without letting it prompt.
    async fn manage(&self, method: &str) -> Result<(), BrokerError> {
        self.manager
            .call_method(method, &(false))
            .await
            .map(|_| ())
            .map_err(Self::unreadable)
    }

    async fn lock(&self) -> Result<(), BrokerError> {
        self.session
            .call_method("Lock", &())
            .await
            .map(|_| ())
            .map_err(Self::unreadable)
    }

    fn unreadable(error: impl std::fmt::Display) -> BrokerError {
        BrokerError::Unreadable {
            subsystem: "logind",
            detail: error.to_string(),
        }
    }
}

impl std::fmt::Debug for Link {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Link")
    }
}

#[derive(Debug)]
pub struct Logind {
    link: Option<Link>,
    tick: Cadence,
    primed: bool,
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
            primed: false,
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

    async fn next(&mut self) -> Result<StatePatch, BrokerError> {
        if self.link.is_none() {
            self.link = Some(Link::open().await?);
            self.primed = false;
        }

        if self.primed {
            let closed = {
                let Self { link, tick, .. } = self;
                let link = link.as_mut().expect("opened above");
                tokio::select! {
                    change = link.changes.next() => change.is_none(),
                    _ = tick.wait() => false,
                }
            };
            if closed {
                self.link = None;
                self.primed = false;
                return Err(BrokerError::Unreadable {
                    subsystem: "logind",
                    detail: "the bus closed".into(),
                });
            }
        }

        let idle = self.link.as_ref().expect("opened above").idle().await;
        self.primed = true;
        Ok(StatePatch {
            topics: vec![StateTopic {
                topic: SystemTopic::Idle.as_str().into(),
                revision: 0, // the Hub assigns the real revision
                value: Some(state_topic::Value::Idle(idle)),
            }],
        })
    }

    async fn act(&mut self, action: &action::Kind) -> Result<Option<StatePatch>, BrokerError> {
        if self.link.is_none() {
            self.link = Some(Link::open().await?);
        }
        let link = self.link.as_ref().expect("opened above");

        match action {
            action::Kind::Lock(_) => link.lock().await?,
            other => match Self::method(other) {
                Some(method) => link.manage(method).await?,
                None => return Err(BrokerError::Unserved(ActionKind::of(other))),
            },
        }

        // Nothing observable changed that this broker reports: it has no
        // topics, and the machine going to sleep is not news it can deliver.
        Ok(None)
    }
}
