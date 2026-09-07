//! The on-disk contract.

use omega_core::{Layout, Profile, UnitName};

fn layout() -> Layout {
    Layout::at("/c", "/s", "/x")
}

#[test]
fn a_profile_decides_which_binaries_a_build_produced() {
    let name = UnitName::parse("battery").unwrap();

    // Two profiles, two directories: an inner loop compiling debug binaries
    // must never be confused for the release ones a machine runs.
    assert_eq!(
        layout().compiled_binary(Profile::Debug, &name),
        std::path::Path::new("/x/target/debug/battery")
    );
    assert_eq!(
        layout().compiled_binary(Profile::Release, &name),
        std::path::Path::new("/x/target/release/battery")
    );
    assert_ne!(
        layout().compiled_system(Profile::Debug),
        layout().compiled_system(Profile::Release)
    );
}

#[test]
fn debug_is_the_profile_cargo_needs_no_flag_for() {
    assert_eq!(Profile::Debug.flag(), None);
    assert_eq!(Profile::Release.flag(), Some("--release"));
    // What a build produces by default is what the machine runs.
    assert_eq!(Profile::default(), Profile::Release);
}
