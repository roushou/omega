use omega::{View, Widget, ui::Text};
use omega_document::{
    Document, DocumentValidation,
    shell::{Bar, PluginWidget, Shell},
};
use omega_proto::{UnitName, omega::Manifest};

#[derive(omega::Widget)]
struct Indicator;
impl Widget for Indicator {
    fn render(&self) -> View {
        Text::new("Ready").into()
    }
}

#[derive(omega::Widget)]
#[omega(name = "panel")]
struct Details;
impl Widget for Details {
    fn render(&self) -> View {
        Text::new("Details").into()
    }
}

struct Fixture;
impl Fixture {
    fn document() -> Document {
        Document::new()
            .shell(Shell::new().bar(
                Bar::top().right([PluginWidget::new("main", Indicator).panel(Details).into()]),
            ))
            .unwrap()
    }
    fn built(panel: bool) -> std::collections::BTreeMap<UnitName, Manifest> {
        let plugin = omega::plugin!().widget(Indicator);
        let plugin = if panel {
            plugin.widget(Details)
        } else {
            plugin
        };
        [(
            UnitName::parse(env!("CARGO_PKG_NAME")).unwrap(),
            plugin.manifest().unwrap(),
        )]
        .into()
    }
}

#[test]
fn placement_uses_the_same_names_as_registration() {
    let placement = PluginWidget::new("main", Indicator).panel(Details);
    let json = serde_json::to_value(placement).unwrap();
    assert_eq!(json["unit"], env!("CARGO_PKG_NAME"));
    assert_eq!(json["surface"], "indicator");
    assert_eq!(json["panel"], "panel");
}

#[test]
fn a_panel_from_another_unit_is_rejected_before_compilation() {
    assert!(
        PluginWidget::named("main", "another-unit")
            .try_panel(Details)
            .is_err()
    );
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
