//! Content hierarchy composed from ordinary UI nodes.
use super::style::styled;
use super::{Bind, Column, Component, Field, List, Node, Row, Size, Slider, Spacer, Text, View};
use crate::surface::{TextEdit, TextValue};
use crate::ui::ChoiceValue;
use crate::units::Percent;
use std::fmt::Display;

/// A group of arbitrary views with an optional heading.
///
/// ```
/// use omega::ui::{Header, Section, Text};
/// let section = Section::new().heading(Header::new("Devices"))
///     .gap(8).children([Text::new("Keyboard"), Text::new("Mouse")]);
/// ```
#[derive(Debug, Clone, Default)]
pub struct Section {
    heading: View,
    children: Vec<View>,
    gap: u32,
}

impl Section {
    pub fn new() -> Self {
        Self {
            gap: 14,
            ..Self::default()
        }
    }

    /// An optional heading drawn above the children.
    pub fn heading(mut self, view: impl Into<View>) -> Self {
        self.heading = view.into();
        self
    }

    /// A title heading using the theme's title style.
    pub fn title(self, title: impl Display) -> Self {
        self.heading(Text::new(title).size(Size::Title).bold())
    }

    pub fn gap(mut self, pixels: u32) -> Self {
        self.gap = pixels;
        self
    }

    pub fn child(mut self, view: impl Into<View>) -> Self {
        self.children.push(view.into());
        self
    }

    pub fn children<V: Into<View>>(mut self, views: impl IntoIterator<Item = V>) -> Self {
        self.children.extend(views.into_iter().map(Into::into));
        self
    }
}

impl Component for Section {
    fn render(&self) -> View {
        Column::new()
            .gap(self.gap)
            .child(self.heading.clone())
            .children(self.children.iter().cloned())
            .into()
    }
}

/// A prominent reading and an optional supporting label.
///
/// ```
/// use omega::ui::Metric;
/// let remaining = Metric::new("25:00").label("Ready to focus");
/// ```
#[derive(Debug, Clone)]
pub struct Metric {
    node: Node,
    value: String,
    label: Option<String>,
}

impl Metric {
    pub fn new(value: impl Display) -> Self {
        Self {
            node: Column::new().gap(6).into(),
            value: value.to_string(),
            label: None,
        }
    }
    pub fn label(mut self, label: impl Display) -> Self {
        self.label = Some(label.to_string());
        self
    }
}
styled!(Metric, |metric: Metric| {
    let mut node = metric
        .node
        .child(Text::new(metric.value).size(Size::Display));
    if let Some(label) = metric.label {
        node = node.child(Text::new(label).muted());
    }
    node
});

/// A label and trailing content separated by flexible space. Either slot
/// accepts a view, including a control.
///
/// ```
/// use omega::ui::{Detail, Text, Toggle};
/// let row = Detail::new(Text::new("Enabled"), Toggle::new(true)).gap(8);
/// ```
#[derive(Debug, Clone)]
pub struct Detail {
    label: View,
    value: View,
    gap: u32,
}

impl Detail {
    pub fn new(label: impl Into<View>, value: impl Into<View>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            gap: 12,
        }
    }

    /// Set the horizontal gap between label and trailing content.
    pub fn gap(mut self, pixels: u32) -> Self {
        self.gap = pixels;
        self
    }

    /// A muted label and a text value in one row.
    pub fn row(label: impl Display, value: impl Display) -> View {
        Self::new(Text::new(label).muted(), Text::new(value)).render()
    }

    /// A muted caption above a text value, separated by a small gap.
    pub fn tile(label: &str, value: impl Display) -> View {
        Column::new()
            .gap(2)
            .child(Text::new(label).size(Size::Caption).muted())
            .child(Text::new(value))
            .into()
    }
}

impl Component for Detail {
    fn render(&self) -> View {
        Row::new()
            .gap(self.gap)
            .child(self.label.clone())
            .child(Spacer::new())
            .child(self.value.clone())
            .into()
    }
}

/// A leading view, a growing title/subtitle column, and a trailing view.
/// Slots accept arbitrary views and retain their bindings; the row adds no
/// selection or click behavior.
///
/// ```
/// use omega::ui::{Glyph, Icon, ItemRow, Text};
/// let row = ItemRow::new(Text::new("Keyboard").bold())
///     .leading(Icon::new(Glyph::Bluetooth))
///     .subtitle(Text::new("Connected").muted())
///     .trailing(Text::new("76%"));
/// ```
#[derive(Debug, Clone)]
pub struct ItemRow {
    title: View,
    subtitle: View,
    leading: View,
    trailing: View,
    gap: u32,
    content_gap: u32,
}

impl ItemRow {
    pub fn new(title: impl Into<View>) -> Self {
        Self {
            title: title.into(),
            subtitle: View::empty(),
            leading: View::empty(),
            trailing: View::empty(),
            gap: 12,
            content_gap: 2,
        }
    }

    pub fn subtitle(mut self, view: impl Into<View>) -> Self {
        self.subtitle = view.into();
        self
    }

    pub fn leading(mut self, view: impl Into<View>) -> Self {
        self.leading = view.into();
        self
    }

    pub fn trailing(mut self, view: impl Into<View>) -> Self {
        self.trailing = view.into();
        self
    }

    pub fn gap(mut self, pixels: u32) -> Self {
        self.gap = pixels;
        self
    }

    pub fn content_gap(mut self, pixels: u32) -> Self {
        self.content_gap = pixels;
        self
    }
}

impl Component for ItemRow {
    fn render(&self) -> View {
        Row::new()
            .gap(self.gap)
            .child(self.leading.clone())
            .child(
                Column::new()
                    .fill_width()
                    .gap(self.content_gap)
                    .child(self.title.clone())
                    .child(self.subtitle.clone()),
            )
            .child(self.trailing.clone())
            .into()
    }
}

/// A heading with optional leading, subtitle, and trailing views.
///
/// ```
/// use omega::ui::{PanelHeader, Text};
/// let heading = PanelHeader::new(Text::new("Applications").bold())
///     .subtitle(Text::new("Choose an application").muted());
/// ```
#[derive(Debug, Clone)]
pub struct PanelHeader {
    title: View,
    leading: View,
    subtitle: View,
    trailing: View,
    gap: u32,
    content_gap: u32,
}

impl PanelHeader {
    pub fn new(title: impl Into<View>) -> Self {
        Self {
            title: title.into(),
            leading: View::empty(),
            subtitle: View::empty(),
            trailing: View::empty(),
            gap: 14,
            content_gap: 2,
        }
    }

    /// A title with an uppercase, muted status caption as its subtitle.
    pub fn labelled(title: impl Display, status: impl Display) -> Self {
        Self::new(Text::new(title).size(Size::Title).bold()).subtitle(
            Text::new(status.to_string().to_uppercase())
                .size(Size::Caption)
                .bold()
                .muted(),
        )
    }

    pub fn leading(mut self, view: impl Into<View>) -> Self {
        self.leading = view.into();
        self
    }

    pub fn subtitle(mut self, view: impl Into<View>) -> Self {
        self.subtitle = view.into();
        self
    }

    pub fn trailing(mut self, view: impl Into<View>) -> Self {
        self.trailing = view.into();
        self
    }

    pub fn gap(mut self, pixels: u32) -> Self {
        self.gap = pixels;
        self
    }

    pub fn content_gap(mut self, pixels: u32) -> Self {
        self.content_gap = pixels;
        self
    }
}

impl Component for PanelHeader {
    fn render(&self) -> View {
        ItemRow::new(self.title.clone())
            .leading(self.leading.clone())
            .subtitle(self.subtitle.clone())
            .trailing(self.trailing.clone())
            .gap(self.gap)
            .content_gap(self.content_gap)
            .render()
    }
}

/// A label/value row above an arbitrary control or meter.
///
/// ```
/// use omega::ui::{Labelled, Progress, Text};
/// use omega::Percent;
/// let meter = Labelled::new(
///     Text::new("Used").muted(), Text::new(Percent::whole(40)), Progress::new(Percent::whole(40)),
/// );
/// ```
#[derive(Debug, Clone)]
pub struct Labelled {
    detail: Detail,
    control: View,
    gap: u32,
}

impl Labelled {
    pub fn new(label: impl Into<View>, value: impl Into<View>, control: impl Into<View>) -> Self {
        Self {
            detail: Detail::new(label, value),
            control: control.into(),
            gap: 8,
        }
    }

    /// Set the gap between the row and the control.
    pub fn gap(mut self, pixels: u32) -> Self {
        self.gap = pixels;
        self
    }

    /// Set the gap between the label and the value.
    pub fn label_gap(mut self, pixels: u32) -> Self {
        self.detail = self.detail.gap(pixels);
        self
    }
}

impl Component for Labelled {
    fn render(&self) -> View {
        Column::new()
            .gap(self.gap)
            .child(self.detail.render())
            .child(self.control.clone())
            .into()
    }
}

/// A labelled percentage slider with the desktop's caption styling.
/// The caller owns the typed change binding; the slider's local key is `slider`.
///
/// ```
/// use omega::{Percent, ui::LevelControl};
/// #[derive(omega::Command)]
/// struct SetVolume {}
/// impl omega::Command for SetVolume {
///     const ID: &'static str = "set-volume";
///     type Input = Percent;
///     type Output = ();
///     async fn call(&self, _: Percent) -> omega::Result<()> { Ok(()) }
/// }
/// let level = LevelControl { label: "Volume", level: Percent::whole(60), change: SetVolume.into() };
/// ```
#[derive(Debug, Clone)]
pub struct LevelControl<'a> {
    pub label: &'a str,
    pub level: Percent,
    pub change: Bind<Percent>,
}

impl Component for LevelControl<'_> {
    fn render(&self) -> View {
        Labelled::new(
            Text::new(self.label.to_uppercase())
                .size(Size::Caption)
                .bold()
                .muted(),
            Text::new(self.level).size(Size::Caption).muted(),
            Slider::new(self.level)
                .key("slider")
                .on_change(self.change.clone()),
        )
        .label_gap(0)
        .render()
    }
}

/// A search field wired to a keyed result list in the same component scope.
/// The parent owns the [`TextValue`] query and supplies typed bindings; rows are
/// keyed by their value, so selection and activation decode that value.
///
/// ```
/// use omega::{View, surface::{Events, Task, TextEdit, TextValue}, ui::{Column, SearchSelect, Text}};
///
/// #[derive(omega::Surface)]
/// struct Picker {}
/// #[derive(Debug, Default)]
/// struct Model { query: TextValue }
/// enum Message { Edited(TextEdit), Activate(String) }
///
/// impl omega::Surface for Picker {
///     type Model = Model;
///     type Message = Message;
///     type Effects = ();
///     fn update(&self, _: &mut Model, _: Message, _: &()) -> Task<Message> { Task::none() }
///     fn render(&self, model: &Model, events: &Events<Message>) -> View {
///         let search = SearchSelect::new(&model.query)
///             .placeholder("Search")
///             .on_change(events.on(Message::Edited))
///             .row("alpha".to_string(), Text::new("Alpha"))
///             .row("beta".to_string(), Text::new("Beta"))
///             .on_activate(events.on(Message::Activate));
///         Column::new().child(search).into()
///     }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct SearchSelect<'a, K: ChoiceValue> {
    query: &'a TextValue,
    placeholder: String,
    change: Option<Bind<TextEdit>>,
    select: Option<Bind<K>>,
    activate: Option<Bind<K>>,
    rows: Vec<(K, View)>,
    autofocus: bool,
}

impl<'a, K: ChoiceValue + Clone> SearchSelect<'a, K> {
    pub fn new(query: &'a TextValue) -> Self {
        Self {
            query,
            placeholder: String::new(),
            change: None,
            select: None,
            activate: None,
            rows: Vec::new(),
            autofocus: false,
        }
    }

    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        self.placeholder = text.into();
        self
    }

    /// Notify local behavior of committed edits.
    pub fn on_change(mut self, change: impl Into<Bind<TextEdit>>) -> Self {
        self.change = Some(change.into());
        self
    }

    /// Report the row whose selection changed. Optional; activation alone works.
    pub fn on_select(mut self, select: impl Into<Bind<K>>) -> Self {
        self.select = Some(select.into());
        self
    }

    /// Report the activated row's key, decoded as `K`.
    pub fn on_activate(mut self, activate: impl Into<Bind<K>>) -> Self {
        self.activate = Some(activate.into());
        self
    }

    /// Add one result row, keyed by its value.
    pub fn row(mut self, value: K, label: impl Into<View>) -> Self {
        self.rows.push((value, label.into()));
        self
    }

    /// Give the field initial keyboard focus when its instance appears.
    pub fn autofocus(mut self) -> Self {
        self.autofocus = true;
        self
    }
}

impl<K: ChoiceValue + Clone> Component for SearchSelect<'_, K> {
    fn render(&self) -> View {
        let mut field = Field::new("")
            .placeholder(&self.placeholder)
            .controlled(self.query)
            .navigate("results");
        if let Some(change) = self.change.clone() {
            field = field.on_change(change);
        }
        if self.autofocus {
            field = field.autofocus();
        }

        let mut list = List::new().id("results");
        if let Some(select) = self.select.clone() {
            list = list.on_select(select);
        }
        if let Some(activate) = self.activate.clone() {
            list = list.on_activate(activate);
        }
        for (value, label) in &self.rows {
            list = list.child(label.clone().key(value.key()));
        }

        Column::new().child(field).child(list).into()
    }
}
