//! hwmon parsing against filesystem fixtures.

use std::path::Path;

use omega_platform::hwmon::Hwmon;

/// A chip directory, as sysfs lays one out.
fn chip(root: &Path, dir: &str, name: &str, files: &[(&str, &str)]) {
    let chip = root.join(dir);
    std::fs::create_dir_all(&chip).unwrap();
    std::fs::write(chip.join("name"), name).unwrap();
    for (file, contents) in files {
        std::fs::write(chip.join(file), contents).unwrap();
    }
}

fn fixture(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("omega-hwmon-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn zero_and_negative_temperatures_are_valid_readings() {
    let root = fixture("nought");
    chip(
        &root,
        "hwmon0",
        "thinkpad",
        &[
            ("temp1_input", "44000"),
            ("temp1_label", "CPU"),
            ("temp2_input", "0"),
            ("temp3_input", "-5000"),
            ("temp4_input", "unreadable"),
        ],
    );

    let sensors = Hwmon::at(&root).reading().sensors;
    assert_eq!(sensors.len(), 3, "{sensors:?}");
    assert_eq!(sensors[0].label, "CPU");
    assert_eq!(sensors[0].millicelsius, 44_000);
    assert_eq!(sensors[1].millicelsius, 0);
    assert_eq!(sensors[2].millicelsius, -5000);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_fan_reading_nought_is_a_fan_that_is_stopped() {
    // The other way round from a temperature, and the interesting state on a
    // quiet machine.
    let root = fixture("stopped");
    chip(&root, "hwmon0", "thinkpad", &[("fan1_input", "0")]);
    chip(&root, "hwmon1", "acpi_fan", &[("fan1_input", "5200")]);

    let fans = Hwmon::at(&root).reading().fans;
    assert_eq!(fans.len(), 2, "{fans:?}");
    assert!(
        fans.iter()
            .any(|fan| fan.rpm == 0 && fan.chip == "thinkpad")
    );
    assert!(fans.iter().any(|fan| fan.rpm == 5200));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_sensor_with_no_label_is_named_for_its_place_in_the_chip() {
    // `temp3` is a worse thing to draw than `CPU` and a better thing than
    // nothing at all.
    let root = fixture("unlabelled");
    chip(&root, "hwmon0", "acpitz", &[("temp1_input", "44000")]);

    let sensors = Hwmon::at(&root).reading().sensors;
    assert_eq!(sensors[0].label, "temp1");
    assert_eq!(sensors[0].chip, "acpitz");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_tree_that_is_not_there_reads_as_nothing_rather_than_failing() {
    // A machine with no hwmon at all — a VM — is not a broken broker.
    let reading = Hwmon::at(Path::new("/nonexistent/hwmon")).reading();
    assert!(reading.sensors.is_empty());
    assert!(reading.fans.is_empty());
}
