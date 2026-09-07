//! The battery source reads the machine, not a memory of it.

use std::path::PathBuf;

use omega_daemon::source::StateSource;
use omega_daemon::sources::Battery;
use omega_proto::omega::state_topic;

struct Fixture(PathBuf);

impl Fixture {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("omega-battery-{tag}-{nanos}"));
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

#[tokio::test]
async fn each_poll_reads_the_current_charge() {
    let fixture = Fixture::new("reread");
    // SAFETY: single-threaded test, before any other thread reads the env.
    unsafe { std::env::set_var("OMEGA_POWER_SUPPLY", &fixture.0) };

    let mut battery = Battery::new();
    assert_eq!(level(&battery.poll().await.unwrap()), 0.42);

    fixture.set(17, "Discharging");
    assert_eq!(
        level(&battery.poll().await.unwrap()),
        0.17,
        "a source that caches its first reading reports a battery that never moves"
    );

    unsafe { std::env::remove_var("OMEGA_POWER_SUPPLY") };
}
