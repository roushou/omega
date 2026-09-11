//! The backlight broker, in both directions.
//!
//! The raw scale is per device — 255 on one panel, 96000 on the next — so the
//! arithmetic between raw and percent is the part worth pinning: a writer that
//! rounded differently from the reader would set 40% and report 39%.

use std::path::PathBuf;

use omega_brokers::{Backlight, Broker};
use omega_proto::omega::{SetBacklight, action, set_backlight, state_topic};

struct Fixture(PathBuf);

impl Fixture {
    /// A device whose raw scale is `max`, currently at `raw`.
    fn new(tag: &str, raw: u32, max: u32) -> Self {
        let fixture = Self(Self::dir(tag));
        let device = fixture.0.join("intel_backlight");
        std::fs::create_dir_all(&device).unwrap();
        std::fs::write(device.join("max_brightness"), format!("{max}\n")).unwrap();
        std::fs::write(device.join("brightness"), format!("{raw}\n")).unwrap();
        // Deliberately disagrees with the setpoint. `actual_brightness` lags
        // a write while the panel fades, so reading it would make a step
        // that was just applied read back as the old value — and the next
        // relative step would then start from a number the user already
        // moved away from.
        std::fs::write(device.join("actual_brightness"), b"1\n").unwrap();
        fixture
    }

    /// A machine with no backlight.
    fn empty(tag: &str) -> Self {
        Self(Self::dir(tag))
    }

    fn dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("omega-backlight-{tag}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn raw(&self) -> u32 {
        std::fs::read_to_string(self.0.join("intel_backlight/brightness"))
            .unwrap()
            .trim()
            .parse()
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn percent(patch: &omega_proto::omega::StatePatch) -> Option<u32> {
    match patch.topics[0].value.as_ref()? {
        state_topic::Value::Backlight(backlight) => Some(backlight.percent),
        other => panic!("expected a backlight, got {other:?}"),
    }
}

fn set(change: set_backlight::Change) -> action::Kind {
    action::Kind::SetBacklight(SetBacklight {
        change: Some(change),
    })
}

#[tokio::test]
async fn a_raw_scale_is_reported_as_a_percentage() {
    let fixture = Fixture::new("read", 48, 96);

    assert_eq!(percent(&Backlight::at(&fixture.0).patch()), Some(50));
}

#[tokio::test]
async fn setting_it_writes_the_raw_scale_back() {
    let fixture = Fixture::new("write", 0, 255);

    let mut backlight = Backlight::at(&fixture.0);
    let changed = backlight
        .act(&set(set_backlight::Change::AbsolutePercent(40)))
        .await
        .expect("the device accepted it");

    assert_eq!(fixture.raw(), 102, "40% of 255, rounded to nearest");

    // The broker that just set it knows the new value. Making the caller wait
    // for the next poll is a slider that lags its own drag.
    assert_eq!(
        percent(&changed.expect("the write reported what it changed")),
        Some(40)
    );
}

#[tokio::test]
async fn a_step_moves_from_where_the_screen_is_now() {
    let fixture = Fixture::new("step", 50, 100);

    let mut backlight = Backlight::at(&fixture.0);
    backlight
        .act(&set(set_backlight::Change::DeltaPercent(10)))
        .await
        .unwrap();
    assert_eq!(fixture.raw(), 60);

    // Both ends are reachable and neither wraps: stepping down from 5% by ten
    // points lands on off, not on 95%.
    backlight
        .act(&set(set_backlight::Change::DeltaPercent(-95)))
        .await
        .unwrap();
    assert_eq!(fixture.raw(), 0);

    backlight
        .act(&set(set_backlight::Change::DeltaPercent(-10)))
        .await
        .unwrap();
    assert_eq!(fixture.raw(), 0);
}

#[tokio::test]
async fn a_machine_with_no_backlight_says_so_and_refuses_to_set_one() {
    let fixture = Fixture::empty("absent");

    let mut backlight = Backlight::at(&fixture.0);

    let patch = backlight.patch();
    assert_eq!(patch.topics.len(), 1, "the topic is published either way");
    assert!(
        patch.topics[0].value.is_none(),
        "a desktop reports no reading, not a reading of zero"
    );

    // Refused, not silently ignored: a unit that asked for a change and got
    // nothing has no way to tell that from a change that did not stick.
    assert!(
        backlight
            .act(&set(set_backlight::Change::AbsolutePercent(50)))
            .await
            .is_err()
    );
}

#[test]
fn separate_backlight_roots_do_not_share_configuration() {
    let first = Fixture::new("first", 25, 100);
    let second = Fixture::new("second", 75, 100);
    let mut a = Backlight::at(&first.0);
    let mut b = Backlight::at(&second.0);
    assert_eq!(percent(&a.patch()), Some(25));
    assert_eq!(percent(&b.patch()), Some(75));
    assert_eq!(percent(&a.patch()), Some(25));
}
