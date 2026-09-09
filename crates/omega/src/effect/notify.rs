//! Telling the person something.

use std::time::Duration;

use omega_proto::omega::{Notify as NotifyAction, action};

use crate::context::Context;
use crate::effect::does;

/// Permission to raise a desktop notification.
#[derive(Debug)]
pub struct Notify {
    context: Context,
}

does!(Notify, Notify);

impl Notify {
    /// The shortest form: a line of text.
    pub fn send(&self, summary: impl Into<String>) {
        self.show(Notification::new(summary));
    }

    /// A notification built up first, for one that needs more than a line.
    pub fn show(&self, notification: Notification) {
        self.act(action::Kind::Notify(notification.into_action()));
    }
}

/// What to say, and how.
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

    /// A **freedesktop icon name**, resolved by the notification daemon
    /// against the desktop's icon theme — `battery-caution`, `network-wired`.
    ///
    /// Not one of this shell's [`Glyph`] names, which look the same and are a
    /// different set: `battery-quarter` is a glyph here and no icon theme
    /// carries it, so passing one draws nothing at all. A string rather than
    /// an enum because the set belongs to whichever icon theme is installed,
    /// and Omega does not own it.
    ///
    /// [`Glyph`]: crate::Glyph
    pub fn icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = icon.into();
        self
    }

    /// How long it stays up. Left unset, the notification daemon decides.
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
