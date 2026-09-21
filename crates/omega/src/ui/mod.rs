//! Declarative views and composable UI primitives.
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

//! # Keyboard shortcuts
//!
//! Attach a keymap to a view builder, `View`, or `Component`. Focused controls
//! handle native editing first; unconsumed keys reach the nearest matching map.
//!
//! ```
//! use omega::keyboard::{Chord, Key, Keymap};
//! use omega::ui::Column;
//! # fn view(events: &omega::surface::Events<()>) -> omega::View {
//! let keys = Keymap::single(Chord::new(Key::Escape), events.send(()));
//! Column::new().shortcuts(keys).into()
//! # }
//! ```

pub(crate) mod bind;
mod content;
mod control;
mod display;
mod layout;
mod navigation;
mod node;
mod style;
mod text;

/// Supported glyphs for [`Icon::new`].
pub use omega_proto::Glyph;

pub use bind::Bind;
pub use content::{
    Detail, ItemRow, Labelled, LevelControl, Metric, PanelHeader, SearchSelect, Section,
};
pub use control::{
    Button, Checkbox, Choice, ChoiceValue, Dialog, Disclosure, Dropdown, Field, Form, FormInput,
    List, Slider, Toggle,
};
pub use display::{Badge, EmptyState, Graph, Image, Keycap, Progress};
pub use layout::{Column, Grid, Row, Scroll, Separator, Spacer, Stack};
pub use navigation::ViewError;
pub use node::{Align, Emphasis, Node, Role, Size, Tone};
pub use text::{Header, Icon, Text};

/// Compatibility name for [`View`]. New surfaces and components return `View`.
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
