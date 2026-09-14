use omega::{Args, Input, Percent, config::IntoValue, platform::power::PowerProfile};

struct TextInput;
impl TextInput {
    fn decode<T: Input>(text: &str) -> Result<T, omega::Error> {
        T::decode(Args::new(vec![text.into_value()]))
    }
}

#[test]
fn percentages_accept_human_and_wire_scales_without_clamping() {
    for text in ["40%", "0.4"] {
        assert_eq!(
            TextInput::decode::<Percent>(text).unwrap(),
            Percent::whole(40)
        );
    }
    for text in ["0%", "100%", "0", "1", "12.5%"] {
        assert!(TextInput::decode::<Percent>(text).is_ok(), "{text}");
    }
    for text in ["40", "101%", "-1%", "1.1", "NaN", "inf%", "", "%", "40%%"] {
        let error = TextInput::decode::<Percent>(text).unwrap_err();
        assert!(error.to_string().contains("0% to 100%"), "{error}");
    }
    assert_eq!(
        Percent::decode(Args::new(Percent::whole(40).encode())).unwrap(),
        Percent::whole(40)
    );
}

#[test]
fn scalars_parse_only_as_the_declared_type() {
    assert!(TextInput::decode::<bool>("true").unwrap());
    assert!(!TextInput::decode::<bool>("false").unwrap());
    assert!(TextInput::decode::<bool>("1").is_err());
    assert_eq!(TextInput::decode::<i64>("-42").unwrap(), -42);
    assert_eq!(
        TextInput::decode::<u64>(&u64::MAX.to_string()).unwrap(),
        u64::MAX
    );
    assert!(TextInput::decode::<u64>("-1").is_err());
    assert!(TextInput::decode::<i64>("9223372036854775808").is_err());
    assert_eq!(TextInput::decode::<f64>("0.4").unwrap(), 0.4);
    for text in ["001", "40%", "true", "null", "  spaced  ", "{\"a\":1}"] {
        assert_eq!(TextInput::decode::<String>(text).unwrap(), text);
    }
}

#[test]
fn profiles_accept_cli_names_and_existing_wire_names() {
    for (text, expected) in [
        ("saver", PowerProfile::Saver),
        ("power-saver", PowerProfile::Saver),
        ("balanced", PowerProfile::Balanced),
        ("performance", PowerProfile::Performance),
        ("POWER_PROFILE_BALANCED", PowerProfile::Balanced),
    ] {
        assert_eq!(TextInput::decode::<PowerProfile>(text).unwrap(), expected);
    }
    assert!(TextInput::decode::<PowerProfile>("POWER_PROFILE_UNSPECIFIED").is_err());
    assert!(
        TextInput::decode::<PowerProfile>("turbo")
            .unwrap_err()
            .to_string()
            .contains("saver, balanced, or performance")
    );
    assert!(Percent::decode(Args::default()).is_err());
    assert!(Percent::decode(Args::new(vec!["40%".into_value(), "50%".into_value()])).is_err());
}
