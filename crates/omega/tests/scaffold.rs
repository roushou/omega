// Compile the bundled libraries against the SDK, including their own tests.
#[allow(dead_code, unreachable_pub)]
#[path = "../../omega-cli/templates/minimal/src/lib.rs"]
mod minimal;

#[allow(dead_code, unreachable_pub)]
#[path = "../../omega-cli/templates/battery/src/lib.rs"]
mod battery;

#[test]
fn a_minimal_widget_needs_no_topics_or_capabilities() {
    let manifest = omega::testing::manifest_of(&minimal::plugin());
    assert!(manifest.state_topics.is_empty());
    assert!(manifest.granted().unwrap().is_empty());
}
