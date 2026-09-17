use std::path::PathBuf;

/// How the manager loaded the unit. Unknown future states retain their spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadState {
    Loaded,
    NotFound,
    Masked,
    Error,
    BadSetting,
    Other(String),
}

impl LoadState {
    fn parse(value: &str) -> Self {
        match value {
            "loaded" => Self::Loaded,
            "not-found" => Self::NotFound,
            "masked" => Self::Masked,
            "error" => Self::Error,
            "bad-setting" => Self::BadSetting,
            other => Self::Other(other.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveState {
    Active,
    Reloading,
    Refreshing,
    Inactive,
    Failed,
    Activating,
    Deactivating,
    Other(String),
}

impl ActiveState {
    fn parse(value: &str) -> Self {
        match value {
            "active" => Self::Active,
            "reloading" => Self::Reloading,
            "refreshing" => Self::Refreshing,
            "inactive" => Self::Inactive,
            "failed" => Self::Failed,
            "activating" => Self::Activating,
            "deactivating" => Self::Deactivating,
            other => Self::Other(other.into()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Active => "active",
            Self::Reloading => "reloading",
            Self::Refreshing => "refreshing",
            Self::Inactive => "inactive",
            Self::Failed => "failed",
            Self::Activating => "activating",
            Self::Deactivating => "deactivating",
            Self::Other(value) => value,
        }
    }

    /// Whether systemd considers the unit active. This says nothing about
    /// application protocol readiness or which executable is running.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active | Self::Reloading | Self::Refreshing)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Enablement {
    Enabled,
    Runtime,
    Disabled,
    Static,
    Masked,
    MaskedRuntime,
    Unspecified,
    Other(String),
}

impl Enablement {
    fn parse(value: &str) -> Self {
        match value {
            "enabled" => Self::Enabled,
            "enabled-runtime" => Self::Runtime,
            "disabled" => Self::Disabled,
            "static" => Self::Static,
            "masked" => Self::Masked,
            "masked-runtime" => Self::MaskedRuntime,
            "" => Self::Unspecified,
            other => Self::Other(other.into()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Enabled => "enabled",
            Self::Runtime => "enabled-runtime",
            Self::Disabled => "disabled",
            Self::Static => "static",
            Self::Masked => "masked",
            Self::MaskedRuntime => "masked-runtime",
            Self::Unspecified => "unspecified",
            Self::Other(value) => value,
        }
    }

    /// Persistent enablement; runtime-only enablement does not survive logout/reboot.
    pub fn is_persistent(&self) -> bool {
        matches!(self, Self::Enabled)
    }
}

/// Observed manager properties from one `systemctl show` invocation. These are
/// distinct from the contents of a unit file and from application readiness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub load: LoadState,
    pub active: ActiveState,
    pub enablement: Enablement,
    pub sub_state: String,
    pub fragment: Option<PathBuf>,
    pub needs_reload: bool,
}

impl Status {
    pub(super) const PROPERTIES: &'static str =
        "--property=LoadState,ActiveState,UnitFileState,SubState,FragmentPath,NeedDaemonReload";

    pub(super) fn parse(source: &str) -> Result<Self, StatusError> {
        let mut fields = [None; 6];
        for line in source.lines().filter(|line| !line.is_empty()) {
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| StatusError(format!("invalid property line {line:?}")))?;
            let index = match key {
                "LoadState" => 0,
                "ActiveState" => 1,
                "UnitFileState" => 2,
                "SubState" => 3,
                "FragmentPath" => 4,
                "NeedDaemonReload" => 5,
                _ => return Err(StatusError(format!("unexpected property {key}"))),
            };
            if fields[index].replace(value).is_some() {
                return Err(StatusError(format!("duplicate property {key}")));
            }
        }
        let [
            Some(load),
            Some(active),
            Some(enablement),
            Some(sub),
            Some(fragment),
            Some(reload),
        ] = fields
        else {
            return Err(StatusError("missing requested properties".into()));
        };
        if load.is_empty() || active.is_empty() || sub.is_empty() {
            return Err(StatusError("empty service state".into()));
        }
        let needs_reload = match reload {
            "yes" => true,
            "no" => false,
            _ => return Err(StatusError("NeedDaemonReload must be yes or no".into())),
        };
        Ok(Self {
            load: LoadState::parse(load),
            active: ActiveState::parse(active),
            enablement: Enablement::parse(enablement),
            sub_state: sub.into(),
            fragment: (!fragment.is_empty()).then(|| fragment.into()),
            needs_reload,
        })
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid systemd status: {0}")]
pub struct StatusError(String);

#[cfg(test)]
mod tests {
    use super::*;
    const STATUS: &str = "LoadState=loaded\nActiveState=active\nUnitFileState=enabled\nSubState=running\nFragmentPath=/a b/test.service\nNeedDaemonReload=no\n";

    #[test]
    fn status_preserves_runtime_and_file_state_independently() {
        let status = Status::parse(STATUS).unwrap();
        assert!(status.active.is_active());
        assert!(status.enablement.is_persistent());
        assert_eq!(status.fragment, Some("/a b/test.service".into()));
        let status = Status::parse(
            &STATUS
                .replace("ActiveState=active", "ActiveState=failed")
                .replace("UnitFileState=enabled", "UnitFileState=enabled-runtime"),
        )
        .unwrap();
        assert_eq!(status.active, ActiveState::Failed);
        assert!(!status.enablement.is_persistent());
    }

    #[test]
    fn missing_units_are_data_but_missing_properties_are_errors() {
        let status = Status::parse("LoadState=not-found\nActiveState=inactive\nUnitFileState=\nSubState=dead\nFragmentPath=\nNeedDaemonReload=no\n").unwrap();
        assert_eq!(status.load, LoadState::NotFound);
        assert_eq!(status.fragment, None);
        assert!(Status::parse("").is_err());
        assert!(Status::parse(&(STATUS.to_owned() + "ActiveState=failed\n")).is_err());
        let status =
            Status::parse(&STATUS.replace("ActiveState=active", "ActiveState=future-state"))
                .unwrap();
        assert_eq!(status.active, ActiveState::Other("future-state".into()));
    }
}
