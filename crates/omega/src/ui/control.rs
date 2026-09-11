//! Things the user acts on.
//!
//! What separates these from the rest of the vocabulary is that the unit
//! hears about them. A [`Progress`] shows a proportion; a [`Slider`] reports
//! one. A typed [`Bind`] connects the interaction to a command accepting
//! that value. Decoding happens before the command executes.
//!
//! Interaction state is not on the wire. Which row is expanded, where a drag
//! is right now, what is half-typed — the shell owns all of it, because a
//! half-finished gesture is not a fact about the machine. The unit hears the
//! value when there is one to hear.
//!
//! [`Progress`]: crate::ui::Progress

use std::fmt::Display;

use crate::ui::bind::{Bind, CommandRef};
use crate::ui::node::Node;
use crate::ui::style::styled;
use crate::units::Percent;
use crate::{Command, Input};

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
    /// Accepts a command taking `()`, or a command reference with its input bound.
    pub fn on_press(mut self, press: impl Into<Bind<()>>) -> Self {
        self.node = self.node.on("press", press);
        self
    }
}

styled!(Button);

/// A proportion the user can drag.
///
/// Submits a [`Percent`] to the bound command.
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

    /// A command accepting the percentage selected by the user.
    pub fn on_change(mut self, change: impl Into<Bind<Percent>>) -> Self {
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
    pub fn on_change(mut self, change: impl Into<Bind<bool>>) -> Self {
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
///
/// A standalone field submits a `String`; a form submits all its fields together.
#[derive(Debug, Clone)]
pub struct Field {
    node: Node,
}

impl Field {
    /// A field with a persistent label.
    pub fn new(label: impl Display) -> Self {
        Self {
            node: Node::new("field").text_prop("label", label.to_string()),
        }
    }
    /// An example shown only while the field is empty.
    pub fn placeholder(mut self, text: impl Display) -> Self {
        self.node = self.node.text_prop("placeholder", text.to_string());
        self
    }
    /// Instructions that remain visible while editing.
    pub fn help(mut self, text: impl Display) -> Self {
        self.node = self.node.text_prop("help", text.to_string());
        self
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
    pub fn on_submit(mut self, submit: impl Into<Bind<String>>) -> Self {
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
    pub fn on_activate(mut self, activate: impl Into<Bind<String>>) -> Self {
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
///
/// [`key`]: crate::ui::Text::key
#[derive(Debug, Clone)]
pub struct Choice {
    node: Node,
}

impl Choice {
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
    pub fn on_select(mut self, select: impl Into<Bind<String>>) -> Self {
        self.node = self.node.on("select", select);
        self
    }
}

impl Default for Choice {
    fn default() -> Self {
        Self::new()
    }
}

styled!(Choice);

/// An input whose text fields describe a form. Derive `omega::Form`.
pub trait FormInput: Input {
    fn fields() -> Vec<(&'static str, Field)>;
}

/// A form built from its command's input type.
/// Labels, placeholders and help are declared on the input fields.
///
/// ```
/// use omega::{Command, ui::Form};
/// #[derive(omega::Form)]
/// struct Credentials {
///     #[omega(label = "Network", placeholder = "Home")]
///     ssid: String,
///     #[omega(label = "Password", help = "Leave blank for saved networks", secret)]
///     password: String,
/// }
/// #[derive(omega::Command)]
/// struct Connect { wifi: omega::network::WifiControl }
/// impl Command for Connect {
///     type Input = Credentials;
///     type Output = ();
///     async fn call(&self, input: Credentials) -> omega::Result<()> {
///         self.wifi.connect(input.ssid, input.password).await
///     }
/// }
/// let form = Form::new(Connect).submit_label("Connect");
/// ```
#[derive(Debug, Clone)]
pub struct Form {
    node: Node,
}
impl Form {
    pub fn new<C: Command>(command: impl Into<CommandRef<C>>) -> Self
    where
        C::Input: FormInput,
    {
        let mut node = Node::new("form")
            .text_prop("label", "Submit")
            .on("submit", command.into());
        for (name, mut field) in C::Input::fields() {
            field.node = field.node.text_prop("name", name);
            node = node.child(field);
        }
        Self { node }
    }
    /// The action described by the submit button, independently of field labels.
    pub fn submit_label(mut self, label: impl Display) -> Self {
        self.node = self.node.text_prop("label", label.to_string());
        self
    }
}
styled!(Form);
