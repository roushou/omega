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
