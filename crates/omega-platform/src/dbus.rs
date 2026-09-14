//! Reading properties off a bus.
//!
//! Five brokers speak D-Bus and every one of them wanted the same two things:
//! a property that reads as absent rather than as a failure, and every
//! property of an interface in one call. The bound on the first is not one
//! anybody derives twice on purpose.

use std::collections::HashMap;

use zbus::names::InterfaceName;
use zbus::zvariant::OwnedValue;
use zbus::{Proxy, fdo::PropertiesProxy};

use crate::broker::BrokerError;

/// One property, or `None` where the subsystem does not have it.
///
/// Missing reads as absent rather than as a failure: a walk across a bus is a
/// chain of optional steps, and a wired device that has no signal strength
/// has not gone wrong.
pub(crate) async fn property<T>(proxy: &Proxy<'_>, name: &str) -> Option<T>
where
    T: TryFrom<OwnedValue>,
    <T as TryFrom<OwnedValue>>::Error: Into<zbus::Error>,
{
    proxy.get_property(name).await.ok()
}

/// One value out of a map of them, with the same rule.
///
/// For `GetAll` and `GetManagedObjects`, which answer with the properties
/// already gathered.
pub(crate) fn field<T>(properties: &HashMap<String, OwnedValue>, name: &str) -> Option<T>
where
    T: TryFrom<OwnedValue>,
{
    T::try_from(properties.get(name)?.try_clone().ok()?).ok()
}

/// Every property of one interface, in one call.
///
/// One round trip rather than one per property: six reads of a battery taken
/// separately are six chances for the answers to disagree with each other.
pub(crate) async fn properties(
    proxy: &PropertiesProxy<'_>,
    interface: &str,
) -> Result<HashMap<String, OwnedValue>, BrokerError> {
    let interface = InterfaceName::try_from(interface).map_err(BrokerError::unreadable)?;
    proxy
        .get_all(interface)
        .await
        .map_err(BrokerError::unreadable)
}
