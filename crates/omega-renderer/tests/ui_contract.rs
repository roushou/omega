use omega::{Args, Input, surface::TextEdit, ui::ViewportGesture};
use omega_proto::omega::Value;
use omega_proto::{IntoValue, NodeKind, Values, ui::Payload};
use serde_json::{Value as Json, json};
use std::path::Path;

struct Cases;
impl Cases {
    fn sample(payload: Payload) -> (Json, Option<Value>) {
        match payload {
            Payload::None => (Json::Null, None),
            Payload::Text => (json!("chosen"), Some("chosen".into_value())),
            Payload::Flag => (json!(true), Some(true.into_value())),
            Payload::Fraction | Payload::Real | Payload::Percent => {
                (json!(1.0), Some(1.0_f64.into_value()))
            }
            Payload::Unsigned => (json!(u32::MAX), Some(u32::MAX.into_value())),
            Payload::FormFields => (
                json!({"name":"Ada", "count":"42"}),
                Some(
                    Values::new()
                        .with("name", "Ada")
                        .with("count", "42")
                        .into_value(),
                ),
            ),
            Payload::TextEdit | Payload::Gesture => {
                let mut input = serde_json::Map::new();
                let mut wire = Values::new();
                for field in payload.fields() {
                    let (plain, encoded) = Self::sample(field.payload);
                    input.insert(field.name.into(), plain);
                    wire = wire.with(field.name, encoded.unwrap());
                }
                (Json::Object(input), Some(wire.into_value()))
            }
        }
    }

    fn generate() -> String {
        let mut cases = Vec::new();
        for kind in NodeKind::ALL {
            for event in kind.events() {
                let (input, encoded) = Self::sample(event.payload);
                assert!(event.payload.accepts(encoded.as_ref()));
                cases.push(
                    json!({"kind":kind.name(), "event":event.name, "input": input,
                    "absent": encoded.is_none(), "encoded": encoded}),
                );
            }
        }
        let nodes: Vec<_> = NodeKind::ALL.iter().map(|kind| kind.name()).collect();
        format!(
            ".pragma library\n\n// Generated from the shared event contracts and canonical protobuf JSON.\nvar nodes = {}\nvar cases = {}\n",
            serde_json::to_string(&nodes).unwrap(),
            serde_json::to_string_pretty(&cases).unwrap()
        )
    }
}

#[test]
fn qml_event_vectors_match_the_protocol_contract_and_json_serializer() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("shell/tests/fixtures/UiContract.js");
    let expected = Cases::generate();
    if std::env::var_os("OMEGA_REGENERATE").is_some() {
        std::fs::write(path, expected).unwrap();
    } else {
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            expected,
            "regenerate with OMEGA_REGENERATE=1 cargo test -p omega-renderer --test ui_contract"
        );
    }
}

#[derive(omega::Form)]
struct FormValues {
    name: String,
    count: String,
}

#[test]
fn canonical_renderer_vectors_decode_through_sdk_inputs() {
    let source = include_str!("../shell/tests/fixtures/UiContract.js");
    let (_, json) = source.split_once("var cases = ").unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(json).unwrap();
    for case in cases {
        let kind = NodeKind::from_name(case["kind"].as_str().unwrap()).unwrap();
        let event = kind
            .events()
            .iter()
            .find(|event| event.name == case["event"])
            .unwrap();
        let value: Option<omega_proto::omega::Value> =
            serde_json::from_value(case["encoded"].clone()).unwrap();
        let args = Args::new(value.into_iter().collect());
        match event.payload {
            Payload::None => <()>::decode(args).unwrap(),
            Payload::Text => assert_eq!(String::decode(args).unwrap(), "chosen"),
            Payload::Flag => assert!(bool::decode(args).unwrap()),
            Payload::Fraction | Payload::Real => assert_eq!(f64::decode(args).unwrap(), 1.0),
            Payload::Percent => assert_eq!(
                omega::Percent::decode(args).unwrap(),
                omega::Percent::whole(100)
            ),
            Payload::Unsigned => assert_eq!(args.get::<u32>(0).unwrap(), u32::MAX),
            Payload::TextEdit => {
                let edit = TextEdit::decode(args).unwrap();
                assert_eq!(edit.text, "chosen");
                assert_eq!(edit.revision, u32::MAX);
            }
            Payload::Gesture => {
                let gesture = ViewportGesture::decode(args).unwrap();
                assert_eq!(gesture.zoom, 1.0);
                assert_eq!(gesture.dx, 1.0);
            }
            Payload::FormFields => {
                let values = FormValues::decode(args).unwrap();
                assert_eq!(values.name, "Ada");
                assert_eq!(values.count, "42");
            }
        }
    }
}
