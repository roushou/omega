//! Things the user acts on.
//!
//! What separates these from the rest of the vocabulary is that the unit
//! hears about them. A [`Progress`] shows a proportion; a [`Slider`] reports
//! one. The difference is a [`Bind`], and the value the control carries is
//! appended to that binding's arguments — so a unit reads its own arguments
//! by position and the user's value last.
//!
//! Interaction state is not on the wire. Which row is expanded, where a drag
//! is right now, what is half-typed — the shell owns all of it, because a
//! half-finished gesture is not a fact about the machine. The unit hears the
//! value when there is one to hear.
//!
//! [`Progress`]: crate::ui::Progress

use std::fmt::Display;

use crate::ui::bind::Bind;
use crate::ui::node::Node;
use crate::ui::style::styled;
use crate::units::Percent;

/// Something to press.
///
/// Pressing it calls one of this unit's own commands — the same command
/// `omega run` calls, and the same one a keybind would. A button is not a way
/// to reach past the unit that drew it.
#[derive(Debug, Clone)]
pub struct Button {
    node: Node,
}

impl Button {
    pub fn new(label: impl Display) -> Self {
        Self {
            node: Node::new("button").text_prop("label", label.to_string()),
        }
    }

    /// What to call when it is pressed. Must be a command this unit
    /// registered, or the daemon refuses the call.
    ///
    /// Takes a bare command name for a press with nothing to say, or a
    /// [`Bind`] carrying the arguments the command needs — which is what
    /// makes one command serve a list of rows instead of one per row.
    pub fn on_press(mut self, press: impl Into<Bind>) -> Self {
        self.node = self.node.on("press", press);
        self
    }
}

styled!(Button);

/// A proportion the user can drag.
///
/// The value it lands on is appended to the binding's arguments as a fraction
/// between zero and one:
///
/// ```
/// # use omega::{Bind, Percent, Slider};
/// // `set` is called with ("output", 0.42) when dragged to 42%.
/// Slider::new(Percent::whole(60)).on_change(Bind::call("set").arg("output"));
/// ```
///
/// The drag itself is the shell's business. A unit hears where it landed, not
/// every pixel on the way — a render round trip per frame would make the
/// control lag the finger doing it.
#[derive(Debug, Clone)]
pub struct Slider {
    node: Node,
}

impl Slider {
    pub fn new(value: Percent) -> Self {
        Self {
            node: Node::new("slider").fraction("value", value.fraction()),
        }
    }

    /// What to call when the user moves it. The fraction it landed on is
    /// appended to the binding's arguments.
    pub fn on_change(mut self, change: impl Into<Bind>) -> Self {
        self.node = self.node.on("change", change);
        self
    }
}

styled!(Slider);

/// Something with two states.
///
/// The state it lands in is appended to the binding's arguments as a boolean.
/// It flips as soon as it is pressed rather than waiting to be told: the
/// round trip is short, but not so short that a switch which hesitates reads
/// as a switch that did not work. The next render says what is actually true.
#[derive(Debug, Clone)]
pub struct Toggle {
    node: Node,
}

impl Toggle {
    pub fn new(on: bool) -> Self {
        Self {
            node: Node::new("toggle").flag("on", on),
        }
    }

    /// What to call when the user flips it. The state it landed in is
    /// appended to the binding's arguments.
    pub fn on_change(mut self, change: impl Into<Bind>) -> Self {
        self.node = self.node.on("change", change);
        self
    }
}

styled!(Toggle);
