use omega_proto::omega::{Heartbeat, InstanceSnapshot, PresentationState, StateTopic, ViewTree};
use omega_proto::{Observation, Refusal};

#[test]
fn rejected_payloads_retain_unique_valid_correlation() {
    for (line, expected) in [
        (
            r#"{"streamId":"18446744073709551615","invoke":{"futureOp":{}}}"#,
            u64::MAX,
        ),
        (
            r#"{"invoke":{"subscribe":{"replace":"bad"}},"stream_id":17}"#,
            17,
        ),
        (r#"{"streamId":19,"unknown":true}"#, 19),
        (
            r#"{"streamId":"21","invoke":{"subscribe":{},"act":{}}}"#,
            21,
        ),
        (r#"{"streamId":1,"streamId":3,"unknown":true}"#, 0),
        (r#"{"streamId":1,"stream_id":3,"unknown":true}"#, 0),
        (r#"{"streamId":null,"stream_id":3,"unknown":true}"#, 0),
        (r#"{"streamId":-1,"unknown":true}"#, 0),
        (r#"{"streamId":1.5,"unknown":true}"#, 0),
        (r#"{"streamId":"18446744073709551616","unknown":true}"#, 0),
        (r#"{"streamId":23,"invoke": "#, 0),
        (r#"{"invoke":{"streamId":25}}"#, 0),
        (r#"[]"#, 0),
    ] {
        let error = Observation::decode_request(line).unwrap_err();
        assert_eq!(error.stream_id(), expected, "{line}");
        let answer = Refusal::from(&error).frame(error.stream_id());
        assert_eq!(answer.stream_id, expected);
        assert_eq!(
            Refusal::of(&answer).unwrap().code,
            omega_proto::omega::ErrorCode::InvalidArgument
        );
    }
}

#[test]
fn valid_requests_use_the_generated_frame_decoder() {
    for line in [
        r#"{"streamId":"9007199254740993","invoke":{"subscribe":{"topics":[]}}}"#,
        r#"{"stream_id":17,"invoke":{"subscribe":{"replace":true}}}"#,
    ] {
        assert_eq!(
            Observation::decode_request(line).unwrap(),
            serde_json::from_str(line).unwrap()
        );
    }
}

#[test]
fn observation_message_shapes_are_generated_and_distinguishable() {
    let snapshot = InstanceSnapshot {
        instance: Some(omega_proto::omega::InstanceRef {
            id: "one".into(),
            incarnation: "first".into(),
        }),
        plugin: "clock".into(),
        surface: "main".into(),
        module: Some("top".into()),
        requested: PresentationState::Visible as i32,
        observed: PresentationState::Hidden as i32,
        view: Some(ViewTree {
            revision: u64::MAX,
            ..Default::default()
        }),
        ..Default::default()
    };
    let json = serde_json::to_value(&snapshot).unwrap();
    assert_eq!(json["instance"]["id"], "one");
    assert_eq!(json["module"], "top");
    assert_eq!(json["requested"], "PRESENTATION_STATE_VISIBLE");
    assert_eq!(json["view"]["revision"], u64::MAX.to_string());
    assert!(json.get("destroyed").is_none());
    assert_eq!(
        serde_json::from_value::<InstanceSnapshot>(json).unwrap(),
        snapshot
    );
    let tombstone = InstanceSnapshot {
        destroyed: true,
        module: None,
        ..snapshot
    };
    let json = serde_json::to_value(&tombstone).unwrap();
    assert_eq!(json["destroyed"], true);
    assert!(json.get("module").is_none());

    for line in [
        serde_json::to_string(&tombstone).unwrap(),
        serde_json::to_string(&Heartbeat { heartbeat: true }).unwrap(),
    ] {
        assert!(Observation::topic(&line).is_none());
        assert!(Observation::answer(&line).is_none());
    }
    let topic = StateTopic {
        topic: "battery".into(),
        revision: 3,
        value: None,
    };
    assert_eq!(
        Observation::topic(&serde_json::to_string(&topic).unwrap()),
        Some(topic)
    );
}
