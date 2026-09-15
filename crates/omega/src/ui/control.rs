//! Interactive controls with typed command and local-message bindings.
//! Controls submit values through [`Bind`]; input decoding occurs before dispatch.
//! The renderer retains focus, drag state, and unsubmitted text across updates.
//! Use controlled fields and lists to synchronize editing or selection with a model.

use std::fmt::Display;

use crate::ui::node::Node;
use crate::ui::style::styled;
use crate::units::Percent;
use crate::{Command, Input};
use crate::{command::CommandRef, ui::Bind};

/// A button that submits a command or local message when activated.
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

    /// Hide the idle background and border, retaining hover and keyboard focus feedback.
    /// Padding and dimensions are unchanged. Useful for compact toolbar actions.
    ///
    /// ```
    /// use omega::ui::Button;
    /// let workspace = Button::new("1").flat().width(20).height(24);
    /// ```
    pub fn flat(mut self) -> Self {
        self.node = self.node.flag("flat", true);
        self
    }

    /// Bind activation to a local message or a command registered by this plugin.
    /// Accepts a binding with no input or with all command input already bound.
    pub fn on_press(mut self, press: impl Into<Bind<()>>) -> Self {
        self.node = self.node.on("press", press);
        self
    }
}

styled!(Button);

/// An adjustable percentage slider.
/// Submits a [`Percent`] when a drag is released or a keyboard adjustment is made.
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

    /// Bind percentage changes to a command or local message.
    pub fn on_change(mut self, change: impl Into<Bind<Percent>>) -> Self {
        self.node = self.node.on("change", change);
        self
    }
}

styled!(Slider);

/// A boolean switch.
/// Submits the selected boolean value. The renderer displays the change
/// immediately; subsequent views supply the authoritative state.
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

    /// Bind changes to a command or local message accepting `bool`.
    pub fn on_change(mut self, change: impl Into<Bind<bool>>) -> Self {
        self.node = self.node.on("change", change);
        self
    }
}

styled!(Toggle);

/// A text input with a persistent label.
/// Standalone submission sends a `String`. Inside a form, fields submit together.
/// Use `controlled` and `on_change` to synchronize edits with a surface model.
#[derive(Debug, Clone)]
pub struct Field {
    node: Node,
}

impl Field {
    /// Route Up/Down and Enter to the list with this ID in the same component scope.
    ///
    /// The list may be nested inside layouts. Missing IDs and non-list targets
    /// are rejected when the complete view is finalized.
    ///
    /// ```
    /// use omega::{View, ui::{Column, Field, List}};
    /// let view: View = Column::new()
    ///     .child(Field::new("Search").navigate("results"))
    ///     .child(List::new().id("results"))
    ///     .into();
    /// assert!(view.try_into_tree().is_ok());
    /// ```
    pub fn navigate(mut self, list: impl Into<String>) -> Self {
        self.node.navigation = Some(list.into());
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

    /// Mask displayed input. Submitted values still contain the original text.
    pub fn secret(mut self) -> Self {
        self.node = self.node.flag("secret", true);
        self
    }

    /// Set the field's model value. A changed value replaces the local draft;
    /// repeated unchanged values preserve edits. Use [`Self::controlled`] for
    /// revision-aware updates and explicit resets.
    pub fn value(mut self, value: impl Display) -> Self {
        self.node = self.node.text_prop("value", value.to_string());
        self
    }

    /// Bind text submission to a command or local message accepting `String`.
    pub fn on_submit(mut self, submit: impl Into<Bind<String>>) -> Self {
        self.node = self.node.on("submit", submit);
        self
    }
}

styled!(Field);

/// A selectable list with keyboard and pointer activation.
/// Arrow keys change selection; Enter or a click activates a row. Assign each
/// child a stable [`key`](crate::ui::Text::key); selection and activation submit
/// that key. Use [`Self::selected`] and [`Self::on_select`] for model-owned selection.
///
/// Set [`Self::height`] to constrain the viewport and enable scrolling.
/// Without it, the list sizes to its rows.
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

    /// Bind row activation by Enter or click. Submits the row's key; the input
    /// must decode a string, such as `String` or `ApplicationId`.
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
