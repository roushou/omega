//! procfs parsing and counter-delta tests using captured input.

use omega_platform::procfs::{Counters, Cpu, Interfaces, Jiffies, Load, Memory, Mounts};
use omega_proto::omega::Mount;

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
    let samples = Cpu::parse(STAT).unwrap();

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
    let waiting = Cpu::parse("cpu  100 0 100 700 100 0 0 0 0 0").unwrap();
    assert_eq!(waiting[0].idle, 800);
}

#[test]
fn utilisation_is_the_change_between_two_readings() {
    // CPU utilization requires the delta between two counter samples.
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
    // Convert meminfo values from kibibytes to bytes.
    let memory = MEMINFO.parse::<Memory>().unwrap();

    assert_eq!(memory.total, 11_995_036 * 1024);
    assert_eq!(memory.available, 8_672_320 * 1024);
    assert_eq!(memory.swap_free, 22_856_524 * 1024);
}

#[test]
fn a_machine_with_no_swap_reports_none_rather_than_failing() {
    let memory = "MemTotal: 100 kB\nMemAvailable: 50 kB"
        .parse::<Memory>()
        .unwrap();
    assert_eq!(memory.swap_total, 0);
    assert_eq!(memory.total, 100 * 1024);
}

#[test]
fn the_load_averages_are_the_first_three_numbers() {
    let (one, five, fifteen) = Load::parse("1.84 1.95 1.91 1/950 1666027");
    assert_eq!((one, five, fifteen), (1.84, 1.95, 1.91));

    // Missing load averages must not discard other resource readings.
    assert_eq!(Load::parse(""), (0.0, 0.0, 0.0));
}

#[test]
fn uptime_is_whole_seconds() {
    // `/proc/uptime` carries hundredths, and a bar showing "up 3 days" does
    // not.
    assert_eq!(Load::uptime("81480.96 623804.62"), 81_480);
    assert_eq!(Load::uptime(""), 0);
}

const MOUNTS: &str = "proc /proc proc rw,nosuid 0 0
sys /sys sysfs rw,nosuid 0 0
dev /dev devtmpfs rw,nosuid 0 0
run /run tmpfs rw,nosuid 0 0
/dev/nvme0n1p2 / ext4 rw,relatime 0 0
/dev/nvme0n1p1 /boot vfat rw,relatime 0 0
/dev/sda1 /media/My\\040Disk ext4 rw 0 0
cgroup2 /sys/fs/cgroup cgroup2 rw 0 0";

#[test]
fn the_kernels_own_furniture_is_not_a_disk() {
    // Exclude virtual filesystems from disk usage.
    let mounted = Mounts::parse(MOUNTS);
    let paths: Vec<&str> = mounted.iter().map(|m| m.path.as_str()).collect();

    assert_eq!(paths.len(), 3);
    assert!(paths.contains(&"/"));
    assert!(paths.contains(&"/boot"));
}

#[test]
fn a_mount_point_with_a_space_is_one_mount_point() {
    // `/proc/mounts` escapes a space as `\\040`, and a parser that split on
    // whitespace without unescaping would report two mounts and find neither.
    let mounted = Mounts::parse(MOUNTS);
    let disk = mounted.iter().find(|m| m.device == "/dev/sda1").unwrap();
    assert_eq!(disk.path, "/media/My Disk");
}

#[test]
fn a_mount_keeps_what_it_is_and_what_it_is_on() {
    let root = Mounts::parse(MOUNTS)
        .into_iter()
        .find(|m| m.path == "/")
        .unwrap();
    assert_eq!(root.device, "/dev/nvme0n1p2");
    assert_eq!(root.filesystem, "ext4");
}

// ---- against the machine this is running on ----

use omega_platform::Procfs;

#[test]
fn it_reads_the_machine_it_is_running_on() {
    // No live-test gate: every Linux has `/proc`, and a broker that could not
    // read it here could not read it anywhere.
    let mut procfs = Procfs::new();

    assert!(
        procfs.reading().unwrap().is_none(),
        "the first sample is a baseline"
    );
    let second = procfs.reading().unwrap().unwrap();
    assert!(second.memory_total_bytes > 0);
    assert!(second.uptime_seconds > 0);
    assert!(second.cpu_percent <= 100);
    assert!(!second.core_percent.is_empty());
}

#[test]
fn one_filesystem_at_three_paths_is_one_disk() {
    // Deduplicate shared filesystem mounts before reporting usage.
    let measured = vec![
        Mount {
            path: "/var/log".into(),
            device: "/dev/nvme0n1p2".into(),
            filesystem: "btrfs".into(),
            total_bytes: 500,
            available_bytes: 100,
        },
        Mount {
            path: "/".into(),
            device: "/dev/nvme0n1p2".into(),
            filesystem: "btrfs".into(),
            total_bytes: 500,
            available_bytes: 100,
        },
        Mount {
            path: "/boot".into(),
            device: "/dev/nvme0n1p1".into(),
            filesystem: "vfat".into(),
            total_bytes: 100,
            available_bytes: 40,
        },
    ];

    let kept = Mounts::by_device(measured);
    let paths: Vec<&str> = kept.iter().map(|m| m.path.as_str()).collect();

    // Deduplicate at the shortest mount point, then sort by path.
    assert_eq!(paths, vec!["/", "/boot"]);
}

#[test]
fn a_rate_is_the_change_between_two_samples() {
    // Initial counters cannot establish a transfer rate.
    let before = Counters { rx: 1_000, tx: 500 };
    let after = Counters {
        rx: 3_000,
        tx: 1_500,
    };

    assert_eq!(Counters::between(before, after, 2), (1_000, 500));
    assert_eq!(Counters::between(before, before, 2), (0, 0));
}

#[test]
fn counters_that_went_backwards_are_a_gap_not_a_spike() {
    // Counter resets must not produce underflow spikes.
    let before = Counters {
        rx: 9_000_000,
        tx: 9_000_000,
    };
    let after = Counters { rx: 1_000, tx: 500 };

    assert_eq!(Counters::between(before, after, 2), (0, 0));
    // And no time passing is not an infinite rate.
    assert_eq!(Counters::between(before, after, 0), (0, 0));
}

#[test]
fn every_interface_in_proc_net_dev_is_read() {
    let dev = "\
Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo:  318804    3924    0    0    0     0          0         0   318804    3924    0    0    0     0       0          0
 wlan0: 1000000    1000    0    0    0     0          0         0   250000     900    0    0    0     0       0          0
";
    let links = Interfaces::parse(dev);

    assert_eq!(links.len(), 2, "{links:?}");
    assert_eq!(
        links.get("wlan0").copied(),
        Some(Counters {
            rx: 1_000_000,
            tx: 250_000
        }),
        "receive has eight columns before transmit begins"
    );
}

#[test]
fn guest_time_is_not_counted_twice() {
    let cpu = Cpu::parse("cpu 100 20 30 850 0 0 0 0 60 10").unwrap();
    assert_eq!(cpu[0].total, 1000);
}

#[test]
fn incomplete_and_malformed_samples_fail() {
    for input in ["", "cpu 1 2", "cpu 1 wrong 3 4", "cpuX 1 2 3 4"] {
        assert!(Cpu::parse(input).is_err(), "{input}");
    }
    for input in [
        "",
        "MemTotal: 100 kB",
        "MemTotal: 0 kB\nMemAvailable: 0 kB",
        "MemTotal: 100 kB\nMemAvailable: 101 kB",
    ] {
        assert!(input.parse::<Memory>().is_err(), "{input}");
    }
}

#[test]
fn an_idle_counter_reset_is_not_full_utilisation() {
    assert_eq!(
        Jiffies::between(
            Jiffies {
                total: 100,
                idle: 90
            },
            Jiffies {
                total: 110,
                idle: 1
            }
        ),
        0
    );
}
