use omega::testing::Drawn;
use omega::ui::{Bind, Button, Choice, Column, Component, Dropdown, List, Row, Slider, Text};
use omega::{Percent, Ui, View};

#[derive(omega::Command)]
struct Change {}
impl omega::Command for Change {
    const ID: &'static str = "change";

    type Input = Percent;
    type Output = ();
    async fn call(&self, _: Percent) -> omega::Result<()> {
        Ok(())
    }
}

struct VolumeControl {
    value: Percent,
    change: Bind<Percent>,
}

impl Component for VolumeControl {
    fn render(&self) -> View {
        Row::new()
            .child(Text::new(self.value).key("label"))
            .child(
                Slider::new(self.value)
                    .on_change(self.change.clone())
                    .key("volume"),
            )
            .into()
    }
}

impl VolumeControl {
    fn new() -> Self {
        Self {
            value: Percent::whole(25),
            change: Change.into(),
        }
    }
}

#[test]
fn components_and_helpers_compose_without_layout_wrappers() {
    let helper: View = Text::new("heading").into();
    let view: View = Column::new()
        .child(helper)
        .child(VolumeControl::new().padding(8).key("left"))
        .into();
    let root = view.into_tree().root.unwrap();
    assert_eq!(root.children.len(), 2);
    assert_eq!(root.children[1].r#type, "stack");
    assert_eq!(root.children[1].children.len(), 2);
    let drawn = Drawn::of_view(VolumeControl::new().padding(8).key("volume"));
    assert_eq!(drawn.text(), "25%");
    assert!(drawn.node("volume/volume").is_some());
    assert!(drawn.node("volume").unwrap().props.contains_key("pad"));
}

#[test]
fn moving_instances_keep_scoped_controls_and_bindings() {
    let before = Drawn::of_view(
        Column::new()
            .child(VolumeControl::new().key("left"))
            .child(VolumeControl::new().key("right")),
    );
    let after = Drawn::of_view(
        Column::new()
            .child(VolumeControl::new().key("right"))
            .child(VolumeControl::new().key("left")),
    );
    for key in ["left/volume", "right/volume"] {
        assert_eq!(before.node(key), after.node(key));
        assert_eq!(before.node(key).unwrap().events["change"].command, "change");
    }
    assert_ne!(
        before.node("left/volume").unwrap().key,
        before.node("right/volume").unwrap().key
    );
}

struct Empty;
impl Component for Empty {
    fn render(&self) -> View {
        View::empty()
    }
}

#[test]
fn empty_views_and_components_have_no_nodes_or_gaps() {
    assert!(Drawn::of_view(Empty.padding(12).key("empty")).is_empty());
    let root = View::from(
        Column::new()
            .child(View::empty())
            .child(Empty)
            .child(Text::new("visible")),
    )
    .into_tree()
    .root
    .unwrap();
    assert_eq!(root.children.len(), 1);
    assert_eq!(root.children[0].key, "root.0");
}

struct Controls;
impl Component for Controls {
    fn render(&self) -> View {
        Column::new()
            .child(
                Choice::new()
                    .option("first".to_string(), Text::new("First"))
                    .selected(Some("first".into()))
                    .key("choice"),
            )
            .child(List::new().child(Text::new("Item").key("item")).key("list"))
            .into()
    }
}

#[test]
fn scoping_does_not_change_choice_or_list_values() {
    let drawn = Drawn::of_view(Controls.key("controls"));
    assert_eq!(
        drawn.prop("controls/choice", "selected").as_deref(),
        Some("first")
    );
    assert_eq!(
        drawn
            .prop("controls/choice/first", "selection_key")
            .as_deref(),
        Some("first")
    );
    assert_eq!(
        drawn.prop("controls/list/item", "selection_key").as_deref(),
        Some("item")
    );
}

struct Nested;
impl Component for Nested {
    fn render(&self) -> View {
        Column::new()
            .child(VolumeControl::new().key("inner"))
            .child(Text::new("slash").key("inner/volume"))
            .child(Text::new("tilde").key("~p0"))
            .child(Button::new("plain"))
            .into()
    }
}

#[test]
fn nested_scopes_escape_user_segments_and_remain_unique() {
    let drawn = Drawn::of_view(Nested.key("outer"));
    assert!(drawn.node("outer/inner/volume").is_some());
    assert!(drawn.node("outer/inner~1volume").is_some());
    assert!(drawn.node("outer/~0p0").is_some());
    let keys = drawn.keys();
    let unique: std::collections::BTreeSet<_> = keys.iter().collect();
    assert_eq!(keys.len(), unique.len());
}

#[test]
fn conversion_is_delayed_and_the_ui_name_remains_compatible() {
    let legacy: Ui = VolumeControl::new().into();
    let reused = View::from(Column::new().child(legacy.clone()).child(legacy))
        .into_tree()
        .root
        .unwrap();
    assert_ne!(
        reused.children[0].children[1].key,
        reused.children[1].children[1].key
    );
    assert_eq!(Drawn::of_ui(Text::new("old").into()).text(), "old");
}

#[test]
fn component_modifiers_override_the_root_without_changing_control_inputs() {
    let drawn = Drawn::of_view(
        VolumeControl::new()
            .key("component")
            .disabled()
            .tooltip("offline")
            .width(200),
    );
    assert_eq!(drawn.flag("component", "disabled"), Some(true));
    assert_eq!(
        drawn.prop("component", "tooltip").as_deref(),
        Some("offline")
    );
    assert_eq!(
        drawn.node("component/volume").unwrap().events["change"].command,
        "change"
    );
}

#[test]
fn borrowed_components_can_be_reused() {
    let volume = VolumeControl::new();
    let drawn = Drawn::of_view(
        Column::new()
            .child((&volume).key("a"))
            .child((&volume).key("b")),
    );
    assert!(drawn.node("a/volume").is_some());
    assert!(drawn.node("b/volume").is_some());
}

struct TwoChoices;
impl Component for TwoChoices {
    fn render(&self) -> View {
        Column::new()
            .child(
                Choice::new()
                    .option("same".to_string(), Text::new("One"))
                    .key("first"),
            )
            .child(
                Choice::new()
                    .option("same".to_string(), Text::new("Two"))
                    .key("second"),
            )
            .into()
    }
}

#[test]
fn sibling_controls_can_use_identical_domain_values() {
    let drawn = Drawn::of_view(TwoChoices.key("settings"));
    for key in ["settings/first/same", "settings/second/same"] {
        assert_eq!(drawn.prop(key, "selection_key").as_deref(), Some("same"));
    }
    let keys = drawn.keys();
    let unique: std::collections::BTreeSet<_> = keys.iter().collect();
    assert_eq!(keys.len(), unique.len());
}

struct Dropdowns;
impl Component for Dropdowns {
    fn render(&self) -> View {
        Column::new()
            .child(
                Dropdown::new()
                    .option("same".to_string(), Text::new("One"))
                    .selected(Some("same".into()))
                    .key("first"),
            )
            .child(
                Dropdown::new()
                    .option("same".to_string(), Text::new("Two"))
                    .key("second"),
            )
            .into()
    }
}

#[test]
fn dropdown_scoping_preserves_domain_values_like_choice() {
    let drawn = Drawn::of_view(Dropdowns.key("settings"));
    for key in ["settings/first/same", "settings/second/same"] {
        assert_eq!(drawn.prop(key, "selection_key").as_deref(), Some("same"));
    }
    let keys = drawn.keys();
    let unique: std::collections::BTreeSet<_> = keys.iter().collect();
    assert_eq!(keys.len(), unique.len());
}
