//! The session, from logind.
//!
//! The first broker that only writes. It projects no topics — Omega has no
//! `session` or `idle` topic yet — and exists to serve the five actions that
//! end a session one way or another. `next` is `Pending` forever, which the
//! driver handles by simply never waking on it.
//!
//! Every call is non-interactive. logind will ask polkit to prompt when a
//! caller allows it, and a broker that allowed it would block on a dialog
//! with the action channel held open behind it — so a session that may not
//! reboot is told so instead of hanging.

use async_trait::async_trait;
use zbus::zvariant::OwnedObjectPath;
use zbus::{Connection, Proxy};

use omega_proto::omega::action;
use omega_proto::{ActionKind, SystemTopic};

use crate::broker::{Broker, BrokerError};

/// The system bus, the login manager, and this session on it.
struct Link {
    manager: Proxy<'static>,
    session: Proxy<'static>,
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
        let session = Proxy::new(&connection, Self::SERVICE, path, Self::SESSION_IFACE)
            .await
            .map_err(Self::unreadable)?;

        Ok(Self { manager, session })
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

#[derive(Debug, Default)]
pub struct Logind {
    link: Option<Link>,
}

impl Logind {
    pub fn new() -> Self {
        Self::default()
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

    /// None. Whether the session is idle or locked is worth a topic and does
    /// not have one yet; a broker may exist only to be asked.
    fn topics(&self) -> &'static [SystemTopic] {
        &[]
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

    async fn next(&mut self) -> Result<omega_proto::omega::StatePatch, BrokerError> {
        // Nothing to report, ever. The driver selects on this alongside
        // shutdown and incoming actions, so a future that never resolves is
        // simply a branch that never wins.
        std::future::pending().await
    }

    async fn act(
        &mut self,
        action: &action::Kind,
    ) -> Result<Option<omega_proto::omega::StatePatch>, BrokerError> {
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
