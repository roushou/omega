use omega::{Args, Command, Input};
use omega_document::{Actions, Key, Keybinds, Schedules};
use omega_proto::{Cadence, omega::action};

#[derive(omega::Command)]
#[omega(name = "refresh-now")]
struct Refresh;
impl Command for Refresh {
    type Input = ();
    type Output = ();
    async fn call(&self, _: ()) -> omega::Result<()> {
        Ok(())
    }
}

#[derive(Debug, PartialEq, omega::Input)]
struct Selection {
    name: String,
    count: u64,
}
#[derive(omega::Command)]
struct Select {}
impl Command for Select {
    type Input = Selection;
    type Output = ();
    async fn call(&self, _: Selection) -> omega::Result<()> {
        Ok(())
    }
}

#[test]
fn schedule_and_keybind_share_typed_command_identity() {
    let action = Actions::invoke(Refresh);
    let Some(action::Kind::InvokePlugin(call)) = &action.kind else {
        panic!("expected invocation")
    };
    assert_eq!(call.plugin, env!("CARGO_PKG_NAME"));
    assert_eq!(call.command, "refresh-now");
    assert!(call.args.is_empty());
    let schedule = Schedules::every("refresh", Cadence::seconds(10), action.clone());
    let keybind = Keybinds::on("refresh", Key::A, [], action.clone());
    assert_eq!(schedule.action, Some(action.clone()));
    assert_eq!(keybind.action, Some(action));
}

#[test]
fn structured_input_round_trips_through_the_command_decoder() {
    let action = Actions::invoke_with(
        Select,
        Selection {
            name: "wifi".into(),
            count: 2,
        },
    );
    let Some(action::Kind::InvokePlugin(call)) = action.kind else {
        panic!("expected invocation")
    };
    assert_eq!(call.plugin, env!("CARGO_PKG_NAME"));
    assert_eq!(call.command, "select");
    assert_eq!(
        Selection::decode(Args::new(call.args)).unwrap(),
        Selection {
            name: "wifi".into(),
            count: 2
        }
    );
}
