//! What a widget draws.
//!
//! A tree of nodes, built by the things it is made of:
//!
//! ```
//! # use omega::ui::{Row, Text};
//! # use omega::View;
//! let ui: View = Row::new()
//!     .gap(6)
//!     .child(Text::new("80%").bold())
//!     .child(Text::new("charging").muted())
//!     .into();
//! ```
//!
//! Containers accept primitives, [`View`] values and user-defined [`Component`]s.
//! Keys are finalized at publication; component instances scope their internal keys.

pub(crate) mod bind;
mod content;
mod control;
mod display;
mod layout;
mod node;
mod style;
mod text;
mod widget;
pub use widget::{WidgetIdentity, WidgetRef};

/// The icon set a shell draws: what [`Icon::new`] names.
pub use omega_proto::Glyph;

pub use bind::{Bind, CommandRef};
pub use content::{Metric, Section};
pub use control::{Button, Choice, ChoiceValue, Field, Form, FormInput, List, Slider, Toggle};
pub use display::{Graph, Image, Progress};
pub use layout::{Column, Grid, Row, Separator, Spacer, Stack};
pub use node::{Align, Emphasis, Node, Role, Size, Tone};
pub use text::{Header, Icon, Text};

/// Compatibility name for [`View`]. New widgets and components return `View`.
pub type Ui = View;

mod view;
pub use view::{Component, View};

#[cfg(test)]
mod tests {
    use super::{Column, Row, Text, View};

    #[test]
    fn positional_keys_follow_explicit_parent_keys() {
        let ui: View = Column::new()
            .child(Text::new("first"))
            .child(Row::new().key("named").child(Text::new("nested")))
            .child(Row::new().child(Text::new("last")))
            .into();
        let root = ui.into_tree().root.unwrap();
        assert_eq!(root.key, "root");
        assert_eq!(root.children[0].key, "root.0");
        assert_eq!(root.children[1].key, "named");
        assert_eq!(root.children[1].children[0].key, "named.0");
        assert_eq!(root.children[2].children[0].key, "root.2.0");
    }

    #[test]
    fn explicit_empty_keys_and_named_descendants_are_preserved() {
        let ui: View = Row::new()
            .key("")
            .child(Text::new("positional"))
            .child(Text::new("named").key("stable"))
            .into();
        let root = ui.into_tree().root.unwrap();
        assert_eq!(root.key, "");
        assert_eq!(root.children[0].key, ".0");
        assert_eq!(root.children[1].key, "stable");
    }
}
