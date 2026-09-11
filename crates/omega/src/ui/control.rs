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
/// # use omega::ui::{Bind, Slider};
/// # use omega::Percent;
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

/// Something to type into.
///
/// The buffer lives in the shell. A half-typed passphrase is not a fact about
/// the machine, and a render round trip per keystroke would put a Unix socket
/// in the path of every character — so the unit hears the value once, when
/// the user commits it:
///
/// ```
/// # use omega::ui::{Bind, Field};
/// # let ssid = "home";
/// Field::new("Passphrase")
///     .secret()
///     .on_submit(Bind::call("connect").arg(ssid));
/// ```
///
/// The submitted text is appended to the binding's arguments, so that command
/// is called with `(ssid, passphrase)`.
#[derive(Debug, Clone)]
pub struct Field {
    node: Node,
}

impl Field {
    /// Name this field in its enclosing form's submitted map.
    ///
    /// ```
    /// use omega::ui::Field;
    /// let network = Field::new("Network").name("ssid");
    /// ```
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.node = self.node.text_prop("name", name);
        self
    }

    /// An empty field, with the given placeholder.
    pub fn new(placeholder: impl Display) -> Self {
        Self {
            node: Node::new("field").text_prop("placeholder", placeholder.to_string()),
        }
    }

    /// Draw what is typed as dots.
    ///
    /// Only how it draws. It is the same text on the wire when submitted, and
    /// this is not a claim about how the value is handled after that.
    pub fn secret(mut self) -> Self {
        self.node = self.node.flag("secret", true);
        self
    }

    /// What the field starts with, and what it goes back to whenever the unit
    /// says so.
    ///
    /// The exception to the buffer being the shell's: giving a value takes it
    /// over, so a unit can clear a field after acting on it. Without one the
    /// field keeps what the user typed across a re-render, which is what
    /// stops a list refreshing underneath somebody mid-passphrase.
    pub fn value(mut self, value: impl Display) -> Self {
        self.node = self.node.text_prop("value", value.to_string());
        self
    }

    /// What to call when the user commits it. The text is appended to the
    /// binding's arguments.
    pub fn on_submit(mut self, submit: impl Into<Bind>) -> Self {
        self.node = self.node.on("submit", submit);
        self
    }
}

styled!(Field);

/// Rows to choose from.
///
/// A [`Stack`] draws children in a line; a list is the one the user moves
/// through. Arrows change the selection and Enter activates it, and neither
/// reaches the unit — where the cursor is right now is what the user is
/// doing, not something the machine knows. The unit hears the key of the row
/// that was activated, appended to the binding's arguments.
///
/// Give every child a [`key`]: it is the identity the selection is kept
/// against and the one handed back on activation, so a list of networks
/// should key by SSID rather than let position decide.
///
/// [`height`] is what it scrolls at. Unset, it is as tall as its rows.
///
/// [`height`]: List::height
///
/// [`Stack`]: crate::ui::Stack
/// [`key`]: crate::ui::Text::key
#[derive(Debug, Clone)]
pub struct List {
    node: Node,
}

impl List {
    pub fn new() -> Self {
        Self {
            node: Node::new("list"),
        }
    }

    /// Space between rows.
    pub fn gap(mut self, gap: u32) -> Self {
        self.node = self.node.number("gap", gap);
        self
    }

    pub fn child(mut self, child: impl Into<Node>) -> Self {
        self.node = self.node.child(child);
        self
    }

    pub fn children<C: Into<Node>>(mut self, children: impl IntoIterator<Item = C>) -> Self {
        for child in children {
            self.node = self.node.child(child);
        }
        self
    }

    /// What to call when a row is activated, by Enter or by clicking it. The
    /// row's key is appended to the binding's arguments.
    pub fn on_activate(mut self, activate: impl Into<Bind>) -> Self {
        self.node = self.node.on("activate", activate);
        self
    }
}

impl Default for List {
    fn default() -> Self {
        Self::new()
    }
}

styled!(List);

/// One of a few, chosen.
///
/// A row of options where exactly one is on — a band picker, a mode switch.
/// Drawn joined, so it reads as one control with several settings rather than
/// several controls.
///
/// Every option carries a [`key`], which is what identifies it and what is
/// handed back when it is chosen:
///
/// ```
/// # use omega::ui::{Bind, Group, Text};
/// Group::new()
///     .option(Text::new("Auto").key("auto"))
///     .option(Text::new("5 GHz").key("5"))
///     .selected("auto")
///     .on_select(Bind::call("band"));
/// ```
///
/// [`key`]: crate::ui::Text::key
#[derive(Debug, Clone)]
pub struct Group {
    node: Node,
}

impl Group {
    pub fn new() -> Self {
        Self {
            node: Node::new("group"),
        }
    }

    pub fn option(mut self, option: impl Into<Node>) -> Self {
        self.node = self.node.child(option);
        self
    }

    pub fn options<C: Into<Node>>(mut self, options: impl IntoIterator<Item = C>) -> Self {
        for option in options {
            self.node = self.node.child(option);
        }
        self
    }

    /// Which one is on, by key. A key no option carries selects nothing,
    /// which is what a group whose value came from somewhere else should
    /// show rather than guessing at the first.
    pub fn selected(mut self, key: impl Into<String>) -> Self {
        self.node = self.node.text_prop("selected", key);
        self
    }

    /// What to call when one is chosen. Its key is appended to the binding's
    /// arguments.
    pub fn on_select(mut self, select: impl Into<Bind>) -> Self {
        self.node = self.node.on("select", select);
        self
    }
}

impl Default for Group {
    fn default() -> Self {
        Self::new()
    }
}

styled!(Group);

/// Submit named fields together. Drafts remain in the shell until submission.
///
/// ```
/// use omega::ui::{Form, Field};
/// let form = Form::new("Connect")
///     .field(Field::new("Network").name("ssid"))
///     .field(Field::new("Password").name("password").secret())
///     .on_submit("connect");
/// ```
#[derive(Debug, Clone)]
pub struct Form {
    node: Node,
}
impl Form {
    pub fn new(label: impl Display) -> Self {
        Self {
            node: Node::new("form").text_prop("label", label.to_string()),
        }
    }
    pub fn field(mut self, field: Field) -> Self {
        self.node = self.node.child(field);
        self
    }
    pub fn on_submit(mut self, command: impl Into<Bind>) -> Self {
        self.node = self.node.on("submit", command);
        self
    }
}
styled!(Form);
