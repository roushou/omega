//! Desktop notifications.

use std::time::Duration;

use omega_proto::omega::{Notify as NotifyAction, action};

use crate::runtime::context::Context;
use crate::wiring::does;

/// Permission to raise a desktop notification.
#[derive(Debug)]
pub struct Notify {
    context: Context,
}

does!(Notify, Notify);

impl Notify {
    /// Send a notification containing only a summary.
    pub fn send(&self, summary: impl Into<String>) -> crate::effect::Effect {
        self.show(Notification::new(summary))
    }

    /// Send a notification with additional fields.
    pub fn show(&self, notification: Notification) -> crate::effect::Effect {
        self.act(action::Kind::Notify(notification.into_action()))
    }
}

/// Notification content and display options.
#[derive(Debug, Clone, Default)]
pub struct Notification {
    summary: String,
    body: String,
    icon: String,
    timeout: Option<Duration>,
}

impl Notification {
    pub fn new(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            ..Default::default()
        }
    }

    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = body.into();
        self
    }

    /// Set a freedesktop icon name, such as `battery-caution` or `network-wired`.
    /// The notification service resolves it using the desktop icon theme.
    /// Omega [`Glyph`](crate::ui::Glyph) names are not icon-theme names.
    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = icon.into();
        self
    }

    /// Request a display duration. If unset, the notification service chooses it.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    fn into_action(self) -> NotifyAction {
        NotifyAction {
            summary: self.summary,
            body: self.body,
            icon: self.icon,
            timeout_ms: self
                .timeout
                .map(|timeout| timeout.as_millis().min(u128::from(u32::MAX)) as u32)
                .unwrap_or_default(),
        }
    }
}
