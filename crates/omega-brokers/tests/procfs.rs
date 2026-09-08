//! Reading `/proc`.
//!
//! Fixed text formats, so the whole translation is testable against captured
//! strings — including the part that needs two readings to mean anything.

use omega_brokers::procfs::{Cpu, Jiffies, Load, Memory};

const STAT: &str = "cpu  100 0 100 800 0 0 0 0 0 0
cpu0 50 0 50 400 0 0 0 0 0 0
cpu1 50 0 50 400 0 0 0 0 0 0
intr 12345 0 0
ctxt 999";

const MEMINFO: &str = "MemTotal:       11995036 kB
MemFree:         1004532 kB
MemAvailable:    8672320 kB
Buffers:              20 kB
SwapTotal:      23989152 kB
SwapFree:       22856524 kB";

#[test]
fn the_aggregate_comes_first_and_the_cores_follow() {
    let samples = Cpu::parse(STAT);

    // `/proc/stat` has more after the cpu lines, and a parser that kept
    // reading would count interrupt counters as a core.
    assert_eq!(samples.len(), 3);
    assert_eq!(samples[0].total, 1000);
    assert_eq!(samples[1].total, 500);
}

#[test]
fn waiting_on_a_disk_is_not_doing_work() {
    // Idle is idle plus iowait. Counting iowait as busy makes a machine
    // waiting on a slow disk look pegged.
    let waiting = Cpu::parse("cpu  100 0 100 700 100 0 0 0 0 0");
    assert_eq!(waiting[0].idle, 800);
}

#[test]
fn utilisation_is_the_change_between_two_readings() {
    // Jiffies are counters. A single reading says how busy the machine has
    // been since boot, which is not what anybody means by CPU usage.
    let before = Jiffies {
        total: 1000,
        idle: 800,
    };
    let after = Jiffies {
        total: 2000,
        idle: 1400,
    };

    // Six hundred of the thousand that passed were idle.
    assert_eq!(Jiffies::between(before, after), 40);
}

#[test]
fn counters_going_backwards_are_not_a_negative_percentage() {
    // A suspend, or a core coming online, and the numbers do not line up.
    // Zero is a better answer than a divide by nothing.
    let after = Jiffies {
        total: 500,
        idle: 400,
    };
    let before = Jiffies {
        total: 1000,
        idle: 800,
    };

    assert_eq!(Jiffies::between(before, after), 0);
    assert_eq!(Jiffies::between(before, before), 0, "no time passed");
}

#[test]
fn kibibytes_become_bytes() {
    // Every value in `/proc/meminfo` is in kibibytes whatever its suffix
    // says, and reporting the raw number would be off by a factor of a
    // thousand.
    let memory = Memory::parse(MEMINFO);

    assert_eq!(memory.total, 11_995_036 * 1024);
    assert_eq!(memory.available, 8_672_320 * 1024);
    assert_eq!(memory.swap_free, 22_856_524 * 1024);
}

#[test]
fn a_machine_with_no_swap_reports_none_rather_than_failing() {
    let memory = Memory::parse("MemTotal: 100 kB\nMemAvailable: 50 kB");
    assert_eq!(memory.swap_total, 0);
    assert_eq!(memory.total, 100 * 1024);
}

#[test]
fn the_load_averages_are_the_first_three_numbers() {
    let (one, five, fifteen) = Load::parse("1.84 1.95 1.91 1/950 1666027");
    assert_eq!((one, five, fifteen), (1.84, 1.95, 1.91));

    // A kernel that does not carry the file reads as no load rather than
    // losing the memory figures alongside it.
    assert_eq!(Load::parse(""), (0.0, 0.0, 0.0));
}

#[test]
fn uptime_is_whole_seconds() {
    // `/proc/uptime` carries hundredths, and a bar showing "up 3 days" does
    // not.
    assert_eq!(Load::uptime("81480.96 623804.62"), 81_480);
    assert_eq!(Load::uptime(""), 0);
}

// ---- against the machine this is running on ----

use omega_brokers::Procfs;

#[test]
fn it_reads_the_machine_it_is_running_on() {
    // No live-test gate: every Linux has `/proc`, and a broker that could not
    // read it here could not read it anywhere.
    let mut procfs = Procfs::new();

    let first = procfs.reading();
    assert!(first.memory_total_bytes > 0, "the machine has memory");
    assert!(first.uptime_seconds > 0, "and has been up for a while");
    assert_eq!(
        first.cpu_percent, 0,
        "the first reading has nothing to compare against"
    );

    // The second has. It may legitimately be zero on an idle machine, so the
    // claim is that it is a percentage rather than that it is busy.
    let second = procfs.reading();
    assert!(second.cpu_percent <= 100);
    assert_eq!(
        second.core_percent.len(),
        first.core_percent.len(),
        "the cores do not come and go between two readings"
    );
    assert!(!second.core_percent.is_empty(), "a machine has a core");
}
