//! Reading `/proc`.
//!
//! Fixed text formats, so the whole translation is testable against captured
//! strings — including the part that needs two readings to mean anything.

use omega_brokers::procfs::{Cpu, Jiffies, Load, Memory, Mounts};
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
    // A machine has forty-odd mounts and four of them are disks. Listing
    // `cgroup2` and eleven `tmpfs` buries the one anybody cares about.
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

#[test]
fn one_filesystem_at_three_paths_is_one_disk() {
    // A btrfs root is often also `/home` and `/var/log`. They share the
    // space, so reporting each is reporting the same disk three times with
    // the same numbers.
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

    // The shortest path is the one a person means, and the result is ordered
    // by where things are mounted rather than by device name.
    assert_eq!(paths, vec!["/", "/boot"]);
}

#[test]
fn the_disks_are_measured_less_often_than_the_cpu() {
    // A `statvfs` per mount is real work and free space moves in minutes. The
    // first turn is a reading; the ones under it are not.
    let mut procfs = Procfs::new();

    let first = procfs.disks().expect("the first turn measures");
    assert!(
        first.mounts.iter().any(|mount| mount.path == "/"),
        "a machine has a root filesystem"
    );
    assert!(first.mounts.iter().all(|mount| mount.total_bytes > 0));

    assert!(procfs.disks().is_none(), "and the next turn does not");
}
