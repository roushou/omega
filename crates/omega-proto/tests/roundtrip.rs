use bytes::BytesMut;
use tokio_util::codec::{Decoder, Encoder};

use omega_proto::CodecError;
use omega_proto::codec::FrameCodec;
use omega_proto::omega::{
    Act, Action, BatteryState, Frame, Hello, Invoke, StatePatch, StateTopic, SwitchWorkspace,
    action, frame, invoke, state_topic, switch_workspace,
};

fn battery_frame() -> Frame {
    Frame {
        stream_id: 1,
        body: Some(frame::Body::StatePatch(StatePatch {
            topics: vec![StateTopic {
                topic: "battery".into(),
                revision: 42,
                value: Some(state_topic::Value::Battery(BatteryState {
                    level: 0.87,
                    charging: true,
                    seconds_to_empty: 0,
                    seconds_to_full: 1800,
                })),
            }],
        })),
    }
}

fn hello_frame() -> Frame {
    Frame {
        stream_id: 0,
        body: Some(frame::Body::Hello(Hello {
            protocol_version: 1,
            manifest_hash: "abc123".into(),
            token: "d34db33f".into(),
        })),
    }
}

fn invoke_frame() -> Frame {
    Frame {
        stream_id: 7,
        body: Some(frame::Body::Invoke(Invoke {
            op: Some(invoke::Op::Act(Act {
                action: Some(Action {
                    kind: Some(action::Kind::SwitchWorkspace(SwitchWorkspace {
                        target: Some(switch_workspace::Target::Index(2)),
                    })),
                }),
            })),
        })),
    }
}

#[test]
fn codec_roundtrip() {
    let mut codec = FrameCodec;
    for frame in [battery_frame(), invoke_frame(), hello_frame()] {
        let mut buf = BytesMut::new();
        codec.encode(frame.clone(), &mut buf).unwrap();
        assert_eq!(codec.decode(&mut buf).unwrap(), Some(frame));
        assert!(buf.is_empty());
    }
}

#[test]
fn codec_decodes_incrementally() {
    let mut codec = FrameCodec;
    let mut buf = BytesMut::new();
    codec.encode(battery_frame(), &mut buf).unwrap();

    let mut partial = BytesMut::new();
    for (i, b) in buf.iter().copied().enumerate() {
        partial.extend_from_slice(&[b]);
        if i + 1 < buf.len() {
            assert!(
                codec.decode(&mut partial).unwrap().is_none(),
                "decoded early at byte {i}"
            );
        }
    }
    assert_eq!(codec.decode(&mut partial).unwrap(), Some(battery_frame()));
    assert!(partial.is_empty());
}

#[test]
fn codec_two_frames_back_to_back() {
    let mut codec = FrameCodec;
    let mut buf = BytesMut::new();
    codec.encode(battery_frame(), &mut buf).unwrap();
    codec.encode(hello_frame(), &mut buf).unwrap();

    assert_eq!(codec.decode(&mut buf).unwrap(), Some(battery_frame()));
    assert_eq!(codec.decode(&mut buf).unwrap(), Some(hello_frame()));
    assert!(buf.is_empty());
}

#[test]
fn truncated_frame_is_an_error_on_eof() {
    let mut codec = FrameCodec;
    let mut buf = BytesMut::new();
    codec.encode(battery_frame(), &mut buf).unwrap();
    buf.truncate(buf.len() - 1);

    assert!(matches!(
        codec.decode_eof(&mut buf),
        Err(CodecError::Truncated)
    ));
}

#[test]
fn json_roundtrip() {
    for frame in [battery_frame(), invoke_frame(), hello_frame()] {
        let json = serde_json::to_string(&frame).unwrap();
        assert_eq!(serde_json::from_str::<Frame>(&json).unwrap(), frame);
    }
}

#[test]
fn identifiers_validate_deserialized_strings() {
    use omega_proto::{ModuleId, SurfaceId, UnitName};
    for value in ["", "../outside", "/absolute", "9name", "Upper"] {
        let value = serde_json::to_string(value).unwrap();
        assert!(serde_json::from_str::<UnitName>(&value).is_err());
        assert!(serde_json::from_str::<SurfaceId>(&value).is_err());
        assert!(serde_json::from_str::<ModuleId>(&value).is_err());
    }
    let name: UnitName = serde_json::from_str("\"valid-name\"").unwrap();
    assert_eq!(name.as_str(), "valid-name");
}

#[test]
fn encoding_rejects_oversized_frames_without_touching_the_output_buffer() {
    let mut frame = hello_frame();
    let Some(frame::Body::Hello(hello)) = &mut frame.body else {
        panic!()
    };
    hello.token = "x".repeat(omega_proto::MAX_FRAME_LEN);
    let mut bytes = BytesMut::from(b"existing".as_slice());
    let capacity = bytes.capacity();
    assert!(matches!(
        FrameCodec.encode(frame, &mut bytes),
        Err(CodecError::FrameTooLong(_))
    ));
    assert_eq!(&bytes[..], b"existing");
    assert_eq!(bytes.capacity(), capacity);
}

#[test]
fn an_oversized_reply_is_a_terminal_refusal_on_the_original_stream() {
    let answer = Frame::reply(
        13,
        omega_proto::omega::result::Outcome::Value(omega_proto::IntoValue::into_value(
            "x".repeat(omega_proto::MAX_FRAME_LEN),
        )),
    );
    assert_eq!(answer.stream_id, 13);
    let refusal = omega_proto::Refusal::of(&answer).unwrap();
    assert_eq!(refusal.code, omega_proto::omega::ErrorCode::PayloadTooLarge);
    let Some(frame::Body::Result(result)) = &answer.body else {
        panic!()
    };
    assert!(result.done);
    let mut bytes = BytesMut::new();
    FrameCodec.encode(answer, &mut bytes).unwrap();
    assert!(bytes.len() < 100);
}

#[test]
fn resource_refusals_preserve_distinct_codes_in_protobuf_and_json() {
    use omega_proto::omega::ErrorCode;
    use prost::Message;
    for code in [
        ErrorCode::Unavailable,
        ErrorCode::ResourceExhausted,
        ErrorCode::PayloadTooLarge,
        ErrorCode::DeadlineExceeded,
    ] {
        let frame = omega_proto::Refusal::new(code, "reason").frame(17);
        let decoded = Frame::decode(frame.encode_to_vec().as_slice()).unwrap();
        assert_eq!(omega_proto::Refusal::of(&decoded).unwrap().code, code);
        let json = serde_json::to_string(&frame).unwrap();
        let decoded: Frame = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.stream_id, 17);
        assert_eq!(omega_proto::Refusal::of(&decoded).unwrap().code, code);
    }
}
