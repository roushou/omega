use omega::{Surface, View, ui::Text};
use omega_document::Document;
use omega_omarchy::{
    DocumentValidation,
    shell::{Bar, PluginWidget, Shell},
};
use omega_proto::{PluginName, omega::Manifest};

#[derive(omega::Surface)]
struct Indicator;
impl Surface for Indicator {
    type Model = ();
    type Message = std::convert::Infallible;
    type Effects = ();
    fn update(
        &self,
        _: &mut (),
        message: Self::Message,
        _: &(),
    ) -> omega::surface::Task<Self::Message> {
        match message {}
    }
    fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> View {
        Text::new("Ready").into()
    }
}

#[derive(omega::Surface)]
#[omega(name = "panel")]
struct Details;
impl Surface for Details {
    type Model = ();
    type Message = std::convert::Infallible;
    type Effects = ();
    fn update(
        &self,
        _: &mut (),
        message: Self::Message,
        _: &(),
    ) -> omega::surface::Task<Self::Message> {
        match message {}
    }
    fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> View {
        Text::new("Details").into()
    }
}

struct Fixture;
impl Fixture {
    fn document() -> Document {
        Document::new()
            .with(Shell::new().bar(
                Bar::top().right([PluginWidget::new("main", Indicator).panel(Details).into()]),
            ))
            .unwrap()
    }
    fn built(panel: bool) -> std::collections::BTreeMap<PluginName, Manifest> {
        let plugin = omega::plugin!().surface(Indicator);
        let plugin = if panel {
            plugin.surface(Details)
        } else {
            plugin
        };
        [(
            PluginName::try_from(env!("CARGO_PKG_NAME")).unwrap(),
            plugin.manifest().unwrap(),
        )]
        .into()
    }
}

#[test]
fn placement_uses_the_same_names_as_registration() {
    let placement = PluginWidget::new("main", Indicator).panel(Details);
    let json = serde_json::to_value(placement).unwrap();
    assert_eq!(json["plugin"], env!("CARGO_PKG_NAME"));
    assert_eq!(json["surface"], "indicator");
    assert_eq!(json["panel"], "panel");
}

#[test]
fn a_panel_from_another_plugin_is_rejected_before_compilation() {
    assert!(
        PluginWidget::named("main", "another-plugin")
            .try_panel(Details)
            .is_err()
    );
}

#[test]
fn projected_bar_placements_obey_the_runtime_identifier_limit() {
    let built = Fixture::built(true);
    for length in [128, 129] {
        let document = Document::new()
            .with(
                Shell::new().bar(
                    Bar::top().right([PluginWidget::new(&"a".repeat(length), Indicator)
                        .panel(Details)
                        .into()]),
                ),
            )
            .unwrap()
            .into_inner();
        let result = DocumentValidation::validate(&document, built.values());
        assert_eq!(result.is_ok(), length == 128);
        if length == 129 {
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("placement id exceeds 128 bytes")
            );
        }
    }
}

#[test]
fn typed_placements_still_require_registered_surfaces() {
    let document = Fixture::document().into_inner();
    let built = Fixture::built(true);
    DocumentValidation::validate(&document, built.values()).unwrap();
    let missing = Fixture::built(false);
    let error = DocumentValidation::validate(&document, missing.values()).unwrap_err();
    assert!(error.to_string().contains("panel"), "{error}");
}
