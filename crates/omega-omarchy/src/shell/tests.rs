use super::*;
use omega_proto::omega::{StateDocument, module};

#[test]
fn placement_is_the_source_of_both_outputs() {
    let shell = Shell::new().bar(
        Bar::top().right([
            Native::tray().into(),
            PluginWidget::named("audio-main", "audio")
                .surface_named("indicator")
                .panel_named("panel")
                .into(),
            PluginWidget::named("audio-other", "audio")
                .surface_named("indicator")
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
        assert_eq!(entry.as_object().unwrap().len(), 2);
        assert_eq!(entry["id"], "omega.view");
        assert_eq!(entry["omega"]["plugin"], widget.plugin);
        assert_eq!(entry["omega"]["placement"], module.id);
        assert_eq!(entry["omega"]["surface"], widget.surface);
        if widget.panel.is_empty() {
            assert!(entry["omega"].get("panel").is_none());
        } else {
            assert_eq!(entry["omega"]["panel"], widget.panel);
        }
    }
}

#[test]
fn duplicate_placements_and_competing_bars_are_rejected() {
    let shell = Shell::new().bar(Bar::top().right([
        PluginWidget::named("same", "audio").into(),
        PluginWidget::named("same", "audio").into(),
    ]));
    let ShellError::DuplicatePlacement { id, first, second } = shell.compile().unwrap_err() else {
        panic!("expected duplicate placement");
    };
    assert_eq!(id.as_str(), "same");
    assert_eq!(first, "bar.layout.right[0]");
    assert_eq!(second, "bar.layout.right[1]");
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
            "center":[],"right":[{"id":"omega.view","omega":{"plugin":"audio","placement":"audio","surface":"indicator","panel":"panel"}}]
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
    let document = omega_document::Document::new()
        .with(
            Shell::new().bar(Bar::top().right([PluginWidget::named("missing", "missing").into()])),
        )
        .unwrap()
        .into_inner();
    assert!(
        crate::DocumentValidation::validate(&document, [])
            .unwrap_err()
            .to_string()
            .contains("unknown widget plugin")
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

#[test]
fn omega_import_rejects_flat_mixed_and_invalid_settings() {
    for entry in [
        json!({"id":"omega.view","plugin":"audio","module":"audio"}),
        json!({"id":"omega.view","omega":"audio"}),
        json!({"id":"omega.view","omega":{"placement":"audio"}}),
        json!({"id":"omega.view","omega":{"plugin":"audio","module":"audio"}}),
        json!({"id":"omega.view","plugin":"other","omega":{"plugin":"audio","placement":"audio"}}),
    ] {
        let source = json!({"version":1,"bar":{"layout":{"right":[entry]}}});
        assert!(
            Shell::from_omarchy(&source.to_string()).is_err(),
            "{source}"
        );
    }
}
