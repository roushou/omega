//! Things that show a reading without taking one.

use crate::ui::node::Node;
use crate::ui::style::styled;
use crate::units::Percent;

/// A filled bar.
///
/// Shows a proportion; it does not take one. A bar the user can drag is a
/// [`Slider`], and the difference is whether the unit hears about it.
///
/// [`Slider`]: crate::ui::Slider
#[derive(Debug, Clone)]
pub struct Progress {
    node: Node,
}

impl Progress {
    pub fn new(filled: Percent) -> Self {
        Self {
            node: Node::new("progress").fraction("value", filled.fraction()),
        }
    }
}

styled!(Progress);
