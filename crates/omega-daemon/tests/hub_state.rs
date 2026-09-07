//! The hub broadcasts what changed.

use omega_daemon::hub::Hub;
use omega_proto::omega::{BatteryState, StatePatch, StateTopic, state_topic};

fn battery(level: f64) -> StatePatch {
    StatePatch {
        topics: vec![StateTopic {
            topic: "battery".into(),
            revision: 0,
            value: Some(state_topic::Value::Battery(BatteryState {
                level,
                charging: false,
                seconds_to_empty: 0,
                seconds_to_full: 0,
            })),
        }],
    }
}

#[tokio::test]
async fn a_changed_value_reaches_subscribers() {
    let hub = Hub::new();
    let (_snapshot, mut patches) = hub.subscribe_state();

    hub.publish_state(battery(0.42));
    let first = patches.recv().await.unwrap();
    assert_eq!(first.topics[0].revision, 1);

    // The same reading again is not an update...
    hub.publish_state(battery(0.42));
    // ...but a different one is.
    hub.publish_state(battery(0.17));

    let second = patches.recv().await.unwrap();
    assert_eq!(second.topics[0].revision, 2);
    match second.topics[0].value.as_ref().unwrap() {
        state_topic::Value::Battery(battery) => assert_eq!(battery.level, 0.17),
        other => panic!("expected a battery, got {other:?}"),
    }
}
