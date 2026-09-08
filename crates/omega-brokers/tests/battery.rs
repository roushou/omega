//! The battery broker reads the machine, not a memory of it.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use omega_brokers::Battery;
use omega_proto::omega::state_topic;

/// `OMEGA_POWER_SUPPLY` is one variable for the whole process, so two tests
/// pointing it at their own fixture at the same time would each read the
/// other's machine.
static ENV: Mutex<()> = Mutex::new(());

fn exclusive() -> MutexGuard<'static, ()> {
    ENV.lock().unwrap_or_else(|e| e.into_inner())
}

struct Fixture(PathBuf);

impl Fixture {
    /// A machine with no battery: the tree exists, nothing in it is a `BAT*`.
    fn empty(tag: &str) -> Self {
        Self(Self::dir(tag))
    }

    fn dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("omega-battery-{tag}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn new(tag: &str) -> Self {
        let dir = Self::dir(tag);
        std::fs::create_dir_all(dir.join("BAT0")).unwrap();
        let fixture = Self(dir);
        fixture.set(42, "Discharging");
        fixture
    }

    fn set(&self, capacity: u32, status: &str) {
        std::fs::write(self.0.join("BAT0/capacity"), format!("{capacity}\n")).unwrap();
        std::fs::write(self.0.join("BAT0/status"), format!("{status}\n")).unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn level(patch: &omega_proto::omega::StatePatch) -> f64 {
    match patch.topics[0].value.as_ref().unwrap() {
        state_topic::Value::Battery(battery) => battery.level,
        other => panic!("expected a battery, got {other:?}"),
    }
}

#[test]
fn each_reading_reads_the_current_charge() {
    let _guard = exclusive();
    let fixture = Fixture::new("reread");
    // SAFETY: single-threaded test, before any other thread reads the env.
    unsafe { std::env::set_var("OMEGA_POWER_SUPPLY", &fixture.0) };

    let mut battery = Battery::new();
    assert_eq!(level(&battery.patch()), 0.42);

    fixture.set(17, "Discharging");
    assert_eq!(
        level(&battery.patch()),
        0.17,
        "a broker that caches its first reading reports a battery that never moves"
    );

    unsafe { std::env::remove_var("OMEGA_POWER_SUPPLY") };
}

#[test]
fn a_machine_with_no_battery_says_so() {
    let _guard = exclusive();
    let fixture = Fixture::empty("absent");
    // SAFETY: the guard makes this the only test reading or writing it.
    unsafe { std::env::set_var("OMEGA_POWER_SUPPLY", &fixture.0) };

    let patch = Battery::new().patch();

    // Publishing nothing at all would be indistinguishable from not having
    // been asked yet, and a unit that declared the topic waits for a first
    // value before it draws. So the topic is published with no value: the
    // daemon saying there is nothing to report, which a widget can answer.
    assert_eq!(patch.topics.len(), 1, "the topic is published either way");
    assert_eq!(patch.topics[0].topic, "battery");
    assert!(
        patch.topics[0].value.is_none(),
        "a desktop reports no reading, not a reading of zero"
    );

    unsafe { std::env::remove_var("OMEGA_POWER_SUPPLY") };
}
