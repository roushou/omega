use omega::Command;
use omega::platform::power::{PowerProfile, SetProfile};
use omega::testing::{Called, Drawn, State};
use omega::ui::{Button, Choice, Text};
use omega_proto::{
    IntoValue,
    omega::{SetPowerProfile as WireProfile, action},
};

#[derive(omega::Command)]
struct SetPowerProfile {
    profiles: SetProfile,
}

impl Command for SetPowerProfile {
    const ID: &'static str = "set-power-profile";

    type Input = PowerProfile;
    type Output = ();
    async fn call(&self, profile: PowerProfile) -> omega::Result<()> {
        self.profiles.set(profile).await
    }
}

#[tokio::test]
async fn choices_submit_typed_values_independently_of_labels_and_order() {
    for profiles in [
        [PowerProfile::Balanced, PowerProfile::Saver],
        [PowerProfile::Saver, PowerProfile::Balanced],
    ] {
        let drawn = Drawn::of_ui(
            Choice::new()
                .options(
                    profiles
                        .into_iter()
                        .map(|profile| (profile, Text::new("A presentation label"))),
                )
                .selected(Some(PowerProfile::Saver))
                .on_select(SetPowerProfile)
                .into(),
        );
        let root = drawn.node("root").unwrap();
        let selected = root.props["selected"].clone();
        assert_eq!(selected, PowerProfile::Saver.as_str_name().into_value());
        let option = root
            .children
            .iter()
            .find(|node| node.key == PowerProfile::Saver.as_str_name())
            .unwrap();
        let mut args = root.events["select"].args.clone();
        args.push(option.key.clone().into_value());
        let called = Called::raw::<SetPowerProfile>(&State::new(), args).await;
        assert!(called.answer.is_ok());
        assert!(called.did(&action::Kind::SetPowerProfile(WireProfile {
            profile: PowerProfile::Saver as i32
        })));
    }
}

#[tokio::test]
async fn invalid_external_profile_values_are_refused_before_execution() {
    for input in [
        "unknown-profile".into_value(),
        PowerProfile::Unspecified.into_value(),
        1_i64.into_value(),
    ] {
        let called = Called::raw::<SetPowerProfile>(&State::new(), vec![input]).await;
        assert!(called.answer.is_err());
        assert!(called.effects.is_empty());
    }
}

#[test]
fn no_selection_and_conditional_disabling_can_clear_previous_builder_values() {
    let drawn = Drawn::of_ui(
        Choice::new()
            .option(PowerProfile::Balanced, Text::new("Balanced"))
            .selected(Some(PowerProfile::Balanced))
            .selected(None)
            .disabled_if(true)
            .into(),
    );
    let root = drawn.node("root").unwrap();
    assert_eq!(root.props["selected"], "".into_value());
    assert_eq!(root.props["disabled"], true.into_value());
    let drawn = Drawn::of_ui(Button::new("Next").disabled().disabled_if(false).into());
    assert_eq!(
        drawn.node("root").unwrap().props["disabled"],
        false.into_value()
    );
}
