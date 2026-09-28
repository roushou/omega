use omega_proto::omega::{
    Heartbeat, InstanceRef, InstanceSnapshot, PresentationState, ViewNode, ViewTree,
};

struct Cases;
impl Cases {
    fn generate() -> String {
        let live = InstanceSnapshot {
            instance: Some(InstanceRef {
                id: "test".into(),
                incarnation: "session".into(),
            }),
            plugin: "audio".into(),
            surface: "panel".into(),
            module: Some("slot".into()),
            requested: PresentationState::Visible as i32,
            view: Some(ViewTree {
                revision: 9007199254740993,
                root: Some(ViewNode {
                    r#type: "text".into(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let removed = InstanceSnapshot {
            destroyed: true,
            requested: PresentationState::Closed as i32,
            view: Some(ViewTree {
                revision: 9007199254740994,
                ..Default::default()
            }),
            ..live.clone()
        };
        format!(
            ".pragma library\n\n// Generated protobuf JSON observation messages.\nvar live = {}\nvar removed = {}\nvar heartbeat = {}\n",
            omega_proto::json::Json::encode(&live).unwrap(),
            omega_proto::json::Json::encode(&removed).unwrap(),
            omega_proto::json::Json::encode(&Heartbeat { heartbeat: true }).unwrap()
        )
    }
}

#[test]
fn renderer_observation_fixtures_use_the_schema_json_shapes() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("shell/tests/fixtures/Observation.js");
    let expected = Cases::generate();
    if std::env::var_os("OMEGA_REGENERATE").is_some() {
        std::fs::write(path, expected).unwrap();
    } else {
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            expected,
            "regenerate with OMEGA_REGENERATE=1 cargo test -p omega-renderer --test observation"
        );
    }
}
