//! Installed desktop applications and explicit activation.
//!
//! A surface holds [`Applications`] to observe catalogue changes; behavior holds
//! [`Launcher`](super::Launcher) to activate a typed ID. Search and selection stay instance-local.
//!
//! ```no_run
//! # async fn example(launcher: &omega::platform::applications::Launcher) -> omega::Result<()> {
//! use omega::platform::applications::ApplicationId;
//! let id = "org.gnome.Nautilus.desktop".parse::<ApplicationId>()
//!     .map_err(|e| omega::Error::invalid(e.to_string()))?;
//! launcher.launch(&id).await?;
//! # Ok(()) }
//! ```
crate::wiring::reading! {
    /// Visible installed applications, localized for this desktop session.
    Applications: omega_proto::omega::ApplicationsState
}

pub use omega_proto::{ApplicationId, ApplicationIdError};

/// A desktop entry localized and filtered by the platform service.
#[derive(Debug, Clone, PartialEq)]
pub struct Application {
    id: ApplicationId,
    entry: omega_proto::omega::Application,
}

impl TryFrom<omega_proto::omega::Application> for Application {
    type Error = omega_proto::ApplicationIdError;

    fn try_from(entry: omega_proto::omega::Application) -> Result<Self, Self::Error> {
        Ok(Self {
            id: ApplicationId::try_from(entry.id.clone())?,
            entry,
        })
    }
}

impl Application {
    /// Stable desktop-entry identity, independent of the localized label.
    pub fn id(&self) -> &ApplicationId {
        &self.id
    }
    /// Localized display name.
    pub fn name(&self) -> &str {
        &self.entry.name
    }
    /// Localized description, empty when absent.
    pub fn description(&self) -> &str {
        &self.entry.description
    }
    /// Localized application category, empty when absent.
    pub fn generic_name(&self) -> &str {
        &self.entry.generic_name
    }
    /// Localized search keywords.
    pub fn keywords(&self) -> &[String] {
        &self.entry.keywords
    }
    /// A freedesktop icon name or absolute local path, suitable for `Image::icon`.
    pub fn icon(&self) -> &str {
        &self.entry.icon
    }
    /// Whether activation requires a terminal emulator.
    pub fn is_terminal(&self) -> bool {
        self.entry.terminal
    }
}

impl Applications {
    /// Read the validated collection, distinguishing pending, unavailable, and
    /// malformed readings from a successfully empty collection.
    pub fn snapshot(&self) -> crate::platform::Reading<Vec<Application>> {
        self.context.read().applications.clone()
    }

    /// Visible installed entries. `None` means no valid current catalogue; an empty
    /// vector means the service successfully found no visible applications.
    /// Use [`Self::snapshot`] to distinguish invalid readings from absence.
    ///
    /// ```
    /// # fn example(apps: &omega::platform::applications::Applications) {
    /// if let Some(entries) = apps.entries() {
    ///     for app in entries { let key = app.id().to_string(); }
    /// }
    /// # }
    /// ```
    pub fn entries(&self) -> Option<Vec<Application>> {
        self.snapshot().into_value()
    }
}
