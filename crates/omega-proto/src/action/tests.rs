use super::*;
use crate::omega::*;
use std::collections::BTreeSet;

struct Fixture;
impl Fixture {
    fn valid() -> Vec<action::Kind> {
        vec![
            action::Kind::ConnectWifi(ConnectWifi {
                ssid: "home".into(),
                password: String::new(),
            }),
            action::Kind::DisconnectWifi(DisconnectWifi {}),
            action::Kind::LaunchApp(LaunchApp {
                desktop_id: "org.example.App.desktop".into(),
                args: vec!["".into(), "a file".into()],
            }),
            action::Kind::RunCommand(RunCommand {
                command: "echo first\necho second".into(),
            }),
            action::Kind::SetSetting(SetSetting {
                setting_id: "theme".into(),
                value: Some(Value::default()),
            }),
            action::Kind::ToggleSetting(ToggleSetting {
                setting_id: "theme".into(),
            }),
            action::Kind::SwitchWorkspace(SwitchWorkspace {
                target: Some(switch_workspace::Target::Index(1)),
            }),
            action::Kind::MoveToWorkspace(MoveToWorkspace {
                target: Some(move_to_workspace::Target::Name("dev".into())),
                window: None,
            }),
            action::Kind::MoveToMonitor(MoveToMonitor {
                monitor_id: "DP-1".into(),
                window: None,
            }),
            action::Kind::CloseWindow(CloseWindow::default()),
            action::Kind::Lock(Lock {}),
            action::Kind::Sleep(Sleep {}),
            action::Kind::Hibernate(Hibernate {}),
            action::Kind::Reboot(Reboot {}),
            action::Kind::Shutdown(Shutdown {}),
            action::Kind::Screenshot(Screenshot {
                clipboard: true,
                ..Default::default()
            }),
            action::Kind::MediaKey(MediaKey {
                key: media_key::Key::MediaPlayPause as i32,
            }),
            action::Kind::SetVolume(SetVolume {
                change: Some(set_volume::Change::Absolute(0.0)),
            }),
            action::Kind::SetBacklight(SetBacklight {
                change: Some(set_backlight::Change::AbsolutePercent(100)),
            }),
            action::Kind::Notify(Notify {
                summary: "Summary".into(),
                ..Default::default()
            }),
            action::Kind::InvokeUnit(InvokeUnit {
                unit: "lamp".into(),
                command: "toggle".into(),
                args: vec![],
            }),
            action::Kind::ToggleFloating(ToggleFloating::default()),
            action::Kind::ToggleFullscreen(ToggleFullscreen::default()),
            action::Kind::SetPowerProfile(SetPowerProfile {
                profile: PowerProfile::Balanced as i32,
            }),
        ]
    }
}

#[test]
fn every_action_kind_has_a_valid_payload_fixture() {
    let valid = Fixture::valid();
    assert_eq!(
        valid.iter().map(ActionKind::of).collect::<BTreeSet<_>>(),
        ActionKind::ALL.iter().copied().collect()
    );
    for kind in valid {
        let action = Action { kind: Some(kind) };
        action.validate().unwrap();
    }
    assert_eq!(
        Action::default().validate(),
        Err(ActionError::MissingAction)
    );
}

#[test]
fn absent_changes_invalid_numbers_and_unknown_enums_are_refused() {
    let mut invalid = vec![
        action::Kind::SetVolume(SetVolume::default()),
        action::Kind::SetVolume(SetVolume {
            change: Some(set_volume::Change::ToggleMute(false)),
        }),
        action::Kind::SetBacklight(SetBacklight::default()),
        action::Kind::SetBacklight(SetBacklight {
            change: Some(set_backlight::Change::AbsolutePercent(101)),
        }),
        action::Kind::SwitchWorkspace(SwitchWorkspace::default()),
        action::Kind::SwitchWorkspace(SwitchWorkspace {
            target: Some(switch_workspace::Target::Index(0)),
        }),
        action::Kind::MoveToWorkspace(MoveToWorkspace::default()),
    ];
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.1, 1.1] {
        invalid.push(action::Kind::SetVolume(SetVolume {
            change: Some(set_volume::Change::Absolute(value)),
        }));
    }
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        invalid.push(action::Kind::SetVolume(SetVolume {
            change: Some(set_volume::Change::Delta(value)),
        }));
    }
    for value in [0, -1, 999] {
        invalid.push(action::Kind::MediaKey(MediaKey { key: value }));
        invalid.push(action::Kind::SetPowerProfile(SetPowerProfile {
            profile: value,
        }));
        invalid.push(action::Kind::SwitchWorkspace(SwitchWorkspace {
            target: Some(switch_workspace::Target::Direction(value)),
        }));
    }
    for action in invalid {
        assert!(action.validate().is_err(), "{action:?}");
    }
}

#[test]
fn required_strings_identifiers_selectors_and_destinations_are_checked() {
    let invalid = [
        action::Kind::LaunchApp(LaunchApp {
            desktop_id: "../other".into(),
            ..Default::default()
        }),
        action::Kind::LaunchApp(LaunchApp {
            desktop_id: "app".into(),
            args: vec!["bad\0arg".into()],
        }),
        action::Kind::RunCommand(RunCommand {
            command: " \n".into(),
        }),
        action::Kind::RunCommand(RunCommand {
            command: "echo\0bad".into(),
        }),
        action::Kind::SetSetting(SetSetting {
            setting_id: "theme".into(),
            value: None,
        }),
        action::Kind::ToggleSetting(ToggleSetting::default()),
        action::Kind::SwitchWorkspace(SwitchWorkspace {
            target: Some(switch_workspace::Target::Name(" ".into())),
        }),
        action::Kind::MoveToMonitor(MoveToMonitor::default()),
        action::Kind::CloseWindow(CloseWindow {
            window: Some(WindowSelector {
                target: Some(window_selector::Target::Focused(false)),
            }),
        }),
        action::Kind::ToggleFloating(ToggleFloating {
            window: Some(WindowSelector {
                target: Some(window_selector::Target::AppId("".into())),
            }),
        }),
        action::Kind::ToggleFullscreen(ToggleFullscreen {
            window: Some(WindowSelector {
                target: Some(window_selector::Target::Title("bad\0title".into())),
            }),
        }),
        action::Kind::Screenshot(Screenshot::default()),
        action::Kind::Screenshot(Screenshot {
            output_path: "bad\0path".into(),
            ..Default::default()
        }),
        action::Kind::Notify(Notify {
            body: "bad\0body".into(),
            ..Default::default()
        }),
        action::Kind::InvokeUnit(InvokeUnit {
            unit: "../lamp".into(),
            command: "toggle".into(),
            ..Default::default()
        }),
        action::Kind::InvokeUnit(InvokeUnit {
            unit: "lamp".into(),
            command: "bad name".into(),
            ..Default::default()
        }),
    ];
    for action in invalid {
        assert!(action.validate().is_err(), "{action:?}");
    }
}

#[test]
fn zero_values_deltas_and_implicit_focus_remain_valid() {
    for action in [
        action::Kind::Notify(Notify::default()),
        action::Kind::SetVolume(SetVolume {
            change: Some(set_volume::Change::Absolute(1.0)),
        }),
        action::Kind::SetVolume(SetVolume {
            change: Some(set_volume::Change::Delta(-0.05)),
        }),
        action::Kind::SetVolume(SetVolume {
            change: Some(set_volume::Change::Delta(0.0)),
        }),
        action::Kind::SetVolume(SetVolume {
            change: Some(set_volume::Change::ToggleMute(true)),
        }),
        action::Kind::SetBacklight(SetBacklight {
            change: Some(set_backlight::Change::AbsolutePercent(0)),
        }),
        action::Kind::SetBacklight(SetBacklight {
            change: Some(set_backlight::Change::DeltaPercent(i32::MIN)),
        }),
        action::Kind::CloseWindow(CloseWindow {
            window: Some(WindowSelector::default()),
        }),
    ] {
        action.validate().unwrap();
    }
}

#[test]
fn wifi_requests_require_network_authority_and_validate_without_echoing_secrets() {
    use crate::omega::{ConnectWifi, DisconnectWifi};
    let request = action::Kind::ConnectWifi(ConnectWifi {
        ssid: "home".into(),
        password: "private".into(),
    });
    assert_eq!(
        ActionKind::of(&request).cost(),
        Some(crate::omega::Capability::Network)
    );
    assert!(request.validate().is_ok());
    let invalid = action::Kind::ConnectWifi(ConnectWifi {
        ssid: "".into(),
        password: "private".into(),
    });
    let error = invalid.validate().unwrap_err().to_string();
    assert!(!error.contains("private"));
    let leave = action::Kind::DisconnectWifi(DisconnectWifi {});
    assert_eq!(
        ActionKind::of(&leave).cost(),
        Some(crate::omega::Capability::Network)
    );
}
