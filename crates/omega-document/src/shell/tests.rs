use super::*;
use omega_proto::omega::{StateDocument, module};
#[test]
fn placement_is_the_source_of_both_outputs() {
    let shell = Shell::new().bar(
        Bar::top().right([
            Native::tray().into(),
            PluginWidget::new("audio-main", "audio")
                .surface("indicator")
                .panel("panel")
                .into(),
            PluginWidget::new("audio-other", "audio")
                .surface("indicator")
                .into(),
        ]),
    );
    let compiled = shell.compile().unwrap();
    let entries = compiled.config()["bar"]["layout"]["right"]
        .as_array()
        .unwrap();
    assert_eq!(entries[0]["id"], "omarchy.tray");
    for (entry, module) in entries[1..].iter().zip(&compiled.bar().modules) {
        let module::Kind::Widget(widget) = module.kind.as_ref().unwrap() else {
            panic!()
        };
        assert_eq!(entry["unit"], widget.unit);
        assert_eq!(entry["module"], module.id);
        assert_eq!(entry["surface"], widget.surface);
    }
}
#[test]
fn duplicate_placements_and_competing_bars_are_rejected() {
    let shell = Shell::new().bar(Bar::top().right([
        PluginWidget::new("same", "audio").into(),
        PluginWidget::new("same", "audio").into(),
    ]));
    assert!(shell.compile().is_err());
    let document = StateDocument {
        shell_json: Shell::new().encode().unwrap(),
        bars: vec![Default::default()],
        ..Default::default()
    };
    assert!(CompiledShell::of(&document).is_err());
}
#[test]
fn import_preserves_native_options_and_unknown_fields() {
    let value = json!({
        "version":1,
        "bar":{"position":"top","transparent":false,"layout":{
            "left":[{"id":"omarchy.clock","format":"HH:mm","nested":{"unicode":"界\u{1}"}}],
            "center":[],"right":[{"id":"omega.view","unit":"audio","module":"audio","surface":"indicator","panel":"panel"}]
        },"custom":17},
        "idle":{"screensaver":150,"lock":300,"extra":true},
        "plugins":[{"id":"custom.service","enabled":true}],
        "other":[1,2,3]
    });
    let shell = Shell::from_omarchy(&value.to_string()).unwrap();
    assert_eq!(shell.compile().unwrap().config(), &value);
    let source = shell.rust_source().unwrap();
    assert!(source.contains("PluginWidget::try_new"));
    assert!(source.contains("Native::new"));
    assert!(!source.contains("from_omarchy"));
}
#[test]
fn extensions_cannot_overwrite_typed_fields() {
    assert!(Shell::new().extension("bar", json!({})).is_err());
    assert!(
        Native::new("omarchy.clock")
            .options(json!({"id":"other"}))
            .is_err()
    );
    assert!(Bar::top().extension("layout", json!({})).is_err());
    assert!(Idle::new().extension("lock", 0).is_err());
}
#[test]
fn unsupported_versions_and_ambiguous_imports_fail_loudly() {
    assert!(Shell::from_omarchy(r#"{"version":2}"#).is_err());
    assert!(
        Shell::from_omarchy(r#"{"version":1,"bar":{"layout":{"right":[{"id":"omega.view"}]}}}"#)
            .is_err()
    );
}

#[test]
fn document_validation_checks_shell_surfaces_against_manifests() {
    let document = crate::Document::new()
        .shell(Shell::new().bar(Bar::top().right([PluginWidget::new("missing", "missing").into()])))
        .unwrap()
        .into_inner();
    assert!(
        crate::DocumentValidation::validate(&document, [])
            .unwrap_err()
            .to_string()
            .contains("unknown widget unit")
    );
}

#[test]
fn clock_settings_are_typed_and_fractional_deadlines_are_rejected() {
    let shell = Shell::new().bar(
        Bar::top().center([Native::clock()
            .format("HH:mm")
            .alternate_format("dddd")
            .into()]),
    );
    let compiled = shell.compile().unwrap();
    assert_eq!(
        compiled.config()["bar"]["layout"]["center"][0]["formatAlt"],
        "dddd"
    );
    assert!(
        Shell::new()
            .idle(Idle::new().lock_after(Duration::from_millis(1)))
            .compile()
            .is_err()
    );
}

#[test]
fn string_plugin_references_are_imported_without_losing_identity() {
    let shell = Shell::from_omarchy(
        r#"{"version":1,"bar":{"layout":{"left":["omarchy.menu"]}},"plugins":["custom.service"]}"#,
    )
    .unwrap();
    let compiled = shell.compile().unwrap();
    assert_eq!(
        compiled.config()["bar"]["layout"]["left"][0]["id"],
        "omarchy.menu"
    );
    assert_eq!(compiled.config()["plugins"][0]["id"], "custom.service");
}
