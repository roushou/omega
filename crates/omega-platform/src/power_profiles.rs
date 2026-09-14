//! Observe and control power-profiles-daemon.
//! Property signals refresh active profile state after local or external changes.

use async_trait::async_trait;
use futures_util::StreamExt;
use std::collections::HashMap;
use zbus::fdo::PropertiesProxy;
use zbus::zvariant::OwnedValue;
use zbus::{Connection, Proxy};

use omega_proto::omega::{
    PowerProfile, PowerProfileState, StatePatch, StateTopic, action, state_topic,
};
use omega_proto::{ActionKind, SystemTopic};

use crate::broker::{Broker, BrokerError, opaque_debug};
use crate::dbus;

/// Map protocol profiles to service names.
fn spelled(profile: PowerProfile) -> Option<&'static str> {
    match profile {
        PowerProfile::Saver => Some("power-saver"),
        PowerProfile::Balanced => Some("balanced"),
        PowerProfile::Performance => Some("performance"),
        PowerProfile::Unspecified => None,
    }
}

/// Map unknown service profile names to Unspecified.
fn named(id: &str) -> PowerProfile {
    match id {
        "power-saver" => PowerProfile::Saver,
        "balanced" => PowerProfile::Balanced,
        "performance" => PowerProfile::Performance,
        _ => PowerProfile::Unspecified,
    }
}

struct Link {
    profiles: Proxy<'static>,
    changes: zbus::fdo::PropertiesChangedStream,
}

impl Link {
    const SERVICE: &'static str = "net.hadess.PowerProfiles";
    const PATH: &'static str = "/net/hadess/PowerProfiles";

    async fn open() -> Result<Self, BrokerError> {
        let connection = Connection::system()
            .await
            .map_err(BrokerError::unreadable)?;
        let profiles = Proxy::new(&connection, Self::SERVICE, Self::PATH, Self::SERVICE)
            .await
            .map_err(BrokerError::unreadable)?;

        let properties = PropertiesProxy::builder(&connection)
            .destination(Self::SERVICE)
            .map_err(BrokerError::unreadable)?
            .path(Self::PATH)
            .map_err(BrokerError::unreadable)?
            .build()
            .await
            .map_err(BrokerError::unreadable)?;
        let changes = properties
            .receive_properties_changed()
            .await
            .map_err(BrokerError::unreadable)?;

        Ok(Self { profiles, changes })
    }

    async fn state(&self) -> PowerProfileState {
        // `Profiles` is an array of dicts, one per profile, each carrying the
        // drivers behind it. Only the name is Omega's business.
        let offered: Vec<HashMap<String, OwnedValue>> = dbus::property(&self.profiles, "Profiles")
            .await
            .unwrap_or_default();

        let available: Vec<i32> = offered
            .iter()
            .filter_map(|profile| dbus::field::<String>(profile, "Profile"))
            .map(|id| named(&id) as i32)
            .filter(|profile| *profile != PowerProfile::Unspecified as i32)
            .collect();

        PowerProfileState {
            active: dbus::property::<String>(&self.profiles, "ActiveProfile")
                .await
                .map(|id| named(&id))
                .unwrap_or(PowerProfile::Unspecified) as i32,
            available,
            degraded: dbus::property(&self.profiles, "PerformanceDegraded")
                .await
                .unwrap_or_default(),
        }
    }

    async fn set(&self, profile: PowerProfile) -> Result<(), BrokerError> {
        // Reject unspecified profiles rather than selecting a default.
        let id = spelled(profile)
            .ok_or_else(|| BrokerError::unreadable("SetPowerProfile names no profile"))?;
        self.profiles
            .set_property("ActiveProfile", id)
            .await
            .map_err(BrokerError::unreadable)
    }
}

opaque_debug!(Link);

#[derive(Debug, Default)]
pub struct PowerProfiles {
    link: Option<Link>,
}

impl PowerProfiles {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Broker for PowerProfiles {
    fn name(&self) -> &'static str {
        "power-profiles"
    }

    fn topics(&self) -> &'static [SystemTopic] {
        &[SystemTopic::PowerProfile]
    }

    fn actions(&self) -> &'static [ActionKind] {
        &[ActionKind::SetPowerProfile]
    }

    fn disconnect(&mut self) {
        self.link = None;
    }

    async fn connect(&mut self) -> Result<(), BrokerError> {
        self.link = Some(Link::open().await?);
        Ok(())
    }

    /// Only the signal. There is nothing here a clock would reveal: a profile
    /// changes when something changes it, and the daemon says so.
    async fn wake(&mut self) -> Result<(), BrokerError> {
        let link = self.link.as_mut().ok_or_else(BrokerError::gone)?;
        match link.changes.next().await {
            Some(_) => Ok(()),
            None => Err(BrokerError::gone()),
        }
    }

    async fn read(&mut self) -> Result<StatePatch, BrokerError> {
        let link = self.link.as_ref().ok_or_else(BrokerError::gone)?;
        Ok(StatePatch {
            topics: vec![StateTopic {
                topic: SystemTopic::PowerProfile.as_str().into(),
                revision: 0, // the Hub assigns the real revision
                value: Some(state_topic::Value::PowerProfile(link.state().await)),
            }],
        })
    }

    async fn act(&mut self, action: &action::Kind) -> Result<Option<StatePatch>, BrokerError> {
        let action::Kind::SetPowerProfile(set) = action else {
            return Err(BrokerError::Unserved(ActionKind::of(action)));
        };
        let link = self.link.as_ref().ok_or_else(BrokerError::gone)?;

        link.set(PowerProfile::try_from(set.profile).unwrap_or(PowerProfile::Unspecified))
            .await?;

        // The reading the write produced, so a panel does not wait for the
        // signal to come back around before it redraws.
        Ok(Some(StatePatch {
            topics: vec![StateTopic {
                topic: SystemTopic::PowerProfile.as_str().into(),
                revision: 0,
                value: Some(state_topic::Value::PowerProfile(link.state().await)),
            }],
        }))
    }
}
