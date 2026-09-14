//! D-Bus property access and interface snapshots.

use std::collections::HashMap;

use zbus::names::InterfaceName;
use zbus::zvariant::OwnedValue;
use zbus::{Proxy, fdo::PropertiesProxy};

use crate::broker::BrokerError;

/// Read an optional D-Bus property, returning `None` if unavailable.
pub(crate) async fn property<T>(proxy: &Proxy<'_>, name: &str) -> Option<T>
where
    T: TryFrom<OwnedValue>,
    <T as TryFrom<OwnedValue>>::Error: Into<zbus::Error>,
{
    proxy.get_property(name).await.ok()
}

/// Decode a property from GetAll or GetManagedObjects results.
pub(crate) fn field<T>(properties: &HashMap<String, OwnedValue>, name: &str) -> Option<T>
where
    T: TryFrom<OwnedValue>,
{
    T::try_from(properties.get(name)?.try_clone().ok()?).ok()
}

/// Read all properties of one interface in a single D-Bus call.
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
