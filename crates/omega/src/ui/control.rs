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

use crate::ui::node::Node;
use crate::ui::style::styled;
use crate::units::Percent;
use crate::{Command, Input};
use crate::{command::CommandRef, ui::Bind};

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

    /// Add a named glyph beside the button's label.
    ///
    /// ```
    /// use omega::ui::{Button, Glyph};
    /// let pause = Button::new("Pause").icon(Glyph::Pause);
    /// ```
    pub fn icon(mut self, glyph: super::Glyph) -> Self {
        self.node = self.node.text_prop("icon", glyph.name());
        self
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
    /// Route Up/Down and Enter to a list in the same component/instance scope.
    pub fn navigate(mut self, list: impl Display) -> Self {
        self.node = self.node.text_prop("navigation", list.to_string());
        self
    }

    /// Model-owned text with edit/reset revisions; use with `on_change`.
    pub fn controlled(mut self, value: &crate::surface::TextValue) -> Self {
        self.node = self
            .node
            .text_prop("value", value.text())
            .number("edit_revision", value.revision())
            .number("reset_revision", value.reset_revision())
            .flag("controlled", true);
        self
    }
    /// Notify local behavior of committed edits while typing remains immediate.
    pub fn on_change(mut self, change: impl Into<Bind<crate::surface::TextEdit>>) -> Self {
        self.node = self.node.on("change", change);
        self
    }
    /// Give this field initial keyboard focus when its instance appears.
    pub fn autofocus(mut self) -> Self {
        self.node = self.node.flag("autofocus", true);
        self
    }

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
/// through. Arrows change selection and Enter activates it. Selection can be
/// controlled with `selected` and `on_select`; activation delivers the row key
/// to the binding.
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
    /// Control selection by the row's stable domain key.
    pub fn selected(mut self, key: impl Display) -> Self {
        self.node = self.node.text_prop("selected", key.to_string());
        self
    }
    /// Receive the selected row key. The input must decode a string, such as
    /// `String` or `applications::ApplicationId`.
    pub fn on_select<I: crate::Input>(mut self, select: impl Into<Bind<I>>) -> Self {
        self.node = self.node.on("select", select);
        self
    }

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

    pub fn child(mut self, child: impl Into<crate::View>) -> Self {
        self.node = self.node.child(child);
        self
    }

    pub fn children<C: Into<crate::View>>(mut self, children: impl IntoIterator<Item = C>) -> Self {
        for child in children {
            self.node = self.node.child(child);
        }
        self
    }

    /// What to call when a row is activated, by Enter or by clicking it. The
    /// row's key is appended to the binding's arguments. The input must decode
    /// a string, such as `String` or `applications::ApplicationId`.
    pub fn on_activate<I: crate::Input>(mut self, activate: impl Into<Bind<I>>) -> Self {
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

/// A choice value encoded as a stable string key.
/// Implementations must decode `key()` as the same value through [`Input`].
/// Labels may change independently of keys.
pub trait ChoiceValue: Input {
    fn key(&self) -> &str;
}

impl ChoiceValue for String {
    fn key(&self) -> &str {
        self.as_str()
    }
}
impl ChoiceValue for omega_proto::PlayerId {
    fn key(&self) -> &str {
        self.as_str()
    }
}
impl ChoiceValue for omega_proto::omega::PowerProfile {
    fn key(&self) -> &str {
        self.as_str_name()
    }
}

/// A row of mutually exclusive options with typed values and separate labels.
///
/// ```
/// use omega::{Command, platform::power::{PowerProfile, SetProfile}, ui::{Choice, Text}};
/// #[derive(omega::Command)]
/// struct SetPowerProfile { profiles: SetProfile }
/// impl Command for SetPowerProfile {
///     type Input = PowerProfile;
///     type Output = ();
///     async fn call(&self, profile: PowerProfile) -> omega::Result<()> {
///         self.profiles.set(profile).await
///     }
/// }
/// let picker = Choice::new()
///     .option(PowerProfile::Balanced, Text::new("Balanced"))
///     .option(PowerProfile::Saver, Text::new("Save power"))
///     .selected(Some(PowerProfile::Balanced))
///     .on_select(SetPowerProfile);
/// ```
///
/// A choice cannot invoke a command expecting a different value type:
///
/// ```compile_fail
/// use omega::{Command, platform::power::PowerProfile, ui::{Choice, Text}};
/// #[derive(omega::Command)]
/// struct Rename {}
/// impl Command for Rename {
///     type Input = String;
///     type Output = ();
///     async fn call(&self, _: String) -> omega::Result<()> { Ok(()) }
/// }
/// Choice::new().option(PowerProfile::Balanced, Text::new("Balanced")).on_select(Rename);
/// ```
#[derive(Debug, Clone)]
pub struct Choice<T: ChoiceValue = String> {
    node: Node,
    value: std::marker::PhantomData<fn() -> T>,
}

impl<T: ChoiceValue> Choice<T> {
    pub fn new() -> Self {
        Self {
            node: Node::new("group"),
            value: std::marker::PhantomData,
        }
    }

    /// Add a value and its presentation. The value supplies the option's key.
    pub fn option(mut self, value: T, label: impl Into<crate::View>) -> Self {
        self.node = self.node.child(label.into().key(value.key()));
        self
    }

    /// Add options from an iterator of values and labels.
    pub fn options<N: Into<crate::View>>(
        mut self,
        options: impl IntoIterator<Item = (T, N)>,
    ) -> Self {
        for (value, label) in options {
            self = self.option(value, label);
        }
        self
    }

    /// Select a value. `None` or a value absent from the options selects nothing.
    pub fn selected(mut self, value: Option<T>) -> Self {
        self.node = self.node.text_prop(
            "selected",
            value.as_ref().map(ChoiceValue::key).unwrap_or_default(),
        );
        self
    }

    /// Invoke a command accepting this choice's value type.
    pub fn on_select(mut self, select: impl Into<Bind<T>>) -> Self {
        self.node = self.node.on("select", select);
        self
    }
}

impl<T: ChoiceValue> Default for Choice<T> {
    fn default() -> Self {
        Self::new()
    }
}

styled!(Choice<T: ChoiceValue>);

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
/// struct Connect { wifi: omega::platform::network::WifiControl }
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
