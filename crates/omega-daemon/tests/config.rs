//! Construction settings are installed with the accepted manifest.
mod common;
use common::{Harness, widget_manifest};
use omega_daemon::manifest::ManifestStore;
use omega_proto::omega::frame;
use omega_proto::{UnitName, Values};

#[tokio::test]
async fn a_unit_is_told_its_settings_when_it_connects() {
    let harness = Harness::new(
        "welcome-config",
        ManifestStore::from_manifests([widget_manifest("test-unit", "battery")]),
    );
    harness.units.activate(
        &ManifestStore::from_manifests([widget_manifest("test-unit", "battery")]),
        [(
            UnitName::parse("test-unit").unwrap(),
            Values::new()
                .with("low-threshold", 20u8)
                .with("label", "batt")
                .into_map(),
        )]
        .into_iter()
        .collect(),
    );

    let token = harness.register_unit("test-unit");
    let hash = widget_manifest("test-unit", "battery").hash();
    let mut transport = harness.connect(&hash, token.as_str()).await;

    let welcome = match transport.recv().await.unwrap().unwrap().body {
        Some(frame::Body::Welcome(welcome)) => welcome,
        other => panic!("expected Welcome, got {other:?}"),
    };

    let settings = Values::from_map(welcome.config);
    assert_eq!(settings.get::<u8>("low-threshold"), Some(20));
    assert_eq!(settings.get::<String>("label").as_deref(), Some("batt"));
}
