//! Clipboard reading and writing.

use omega_proto::omega::{ClearClipboard, WriteClipboard, action};

use crate::runtime::context::Context;

use crate::wiring::does;

crate::wiring::reading! {
    /// Current plain-text clipboard content. History is not provided.
    Clipboard: omega_proto::omega::ClipboardState
}

impl Clipboard {
    /// The current text, or `None` when the clipboard is unavailable or empty.
    pub fn text(&self) -> Option<String> {
        self.read()
            .map(|state| state.text)
            .filter(|text| !text.is_empty())
    }
}

/// Permission to change the clipboard.
#[derive(Debug)]
pub struct Write {
    context: Context,
}

does!(Write, Clipboard);

impl Write {
    /// Replace the clipboard with this text.
    pub fn set(&self, text: impl Into<String>) -> crate::effect::Effect {
        self.act(action::Kind::WriteClipboard(WriteClipboard {
            text: text.into(),
        }))
    }

    /// Remove clipboard content.
    pub fn clear(&self) -> crate::effect::Effect {
        self.act(action::Kind::ClearClipboard(ClearClipboard {}))
    }
}
