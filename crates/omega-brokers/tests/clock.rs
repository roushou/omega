//! Reading the wall clock.

use chrono::TimeZone;
use omega_brokers::Clock;

/// 2026-09-08 14:32:45, a Tuesday, in whatever zone the machine is in.
fn at(second: u32) -> chrono::DateTime<chrono::Local> {
    chrono::Local
        .with_ymd_and_hms(2026, 9, 8, 14, 32, second)
        .single()
        .expect("an unambiguous local time")
}

#[test]
fn a_moment_is_broken_down_where_the_machine_is() {
    let time = Clock::of(at(45));

    assert_eq!(time.year, 2026);
    assert_eq!(time.month, 9);
    assert_eq!(time.day, 8);
    assert_eq!(time.hour, 14);
    assert_eq!(time.minute, 32);
    // Zero is Sunday, so Tuesday is two.
    assert_eq!(time.weekday, 2);
    assert!(!time.zone.is_empty(), "the zone is named");
}

#[test]
fn every_field_is_truncated_to_the_minute() {
    // Including the timestamp. A value that moved every second would be a new
    // revision every second, and last-value-wins only helps when the value is
    // actually the same — so a bar of clocks would redraw sixty times an hour
    // to show the same two digits.
    let early = Clock::of(at(1));
    let late = Clock::of(at(59));

    assert_eq!(early, late, "the same minute is the same reading");
    assert_eq!(early.unix_seconds % 60, 0);
}
