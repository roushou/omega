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

#[allow(dead_code, unreachable_pub, unused_imports)]
#[path = "../../omega-cli/templates/command-host/src/lib.rs"]
mod command_host;

#[tokio::test]
async fn command_host_template_echoes_typed_input_without_effects() {
    let called = omega::testing::Called::of::<command_host::Echo>(
        &omega::testing::State::new(),
        "hello".into(),
    )
    .await;
    assert_eq!(called.answer.unwrap(), "hello");
    assert!(called.effects.is_empty());
}
