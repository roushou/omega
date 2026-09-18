use omega::{
    Args, Command, Input,
    config::IntoValue,
    platform::desktop::{WorkspaceControl, WorkspaceIndex, WorkspaceName},
    testing::{Called, State, manifest_of},
};
use omega_proto::omega::{Capability, Direction, action, invoke, switch_workspace};

#[derive(omega::Command)]
struct Select {
    control: WorkspaceControl,
}
impl Command for Select {
    type Input = WorkspaceIndex;
    type Output = ();
    async fn call(&self, index: WorkspaceIndex) -> omega::Result<()> {
        self.control.switch_to(index).await
    }
}

#[derive(omega::Command)]
struct Named {
    control: WorkspaceControl,
}
impl Command for Named {
    type Input = WorkspaceName;
    type Output = ();
    async fn call(&self, name: WorkspaceName) -> omega::Result<()> {
        self.control.switch_to_named(&name).await
    }
}

#[derive(omega::Command)]
struct Cycle {
    control: WorkspaceControl,
}
impl Command for Cycle {
    type Input = bool;
    type Output = ();
    async fn call(&self, next: bool) -> omega::Result<()> {
        if next {
            self.control.next().await
        } else {
            self.control.previous().await
        }
    }
}

struct Fixture;
impl Fixture {
    fn target(called: Called) -> switch_workspace::Target {
        assert!(called.answer.is_ok());
        let [invoke::Op::Act(act)] = called.effects.as_slice() else {
            panic!("expected one action")
        };
        let Some(action::Kind::SwitchWorkspace(switch)) =
            act.action.as_ref().and_then(|a| a.kind.as_ref())
        else {
            panic!("expected workspace switch")
        };
        switch.target.clone().unwrap()
    }
}

#[tokio::test]
async fn switching_needs_no_reading_and_preserves_the_target_kind() {
    let state = State::new();
    let index = WorkspaceIndex::new(7).unwrap();
    for _ in 0..2 {
        assert_eq!(
            Fixture::target(Called::of::<Select>(&state, index).await),
            switch_workspace::Target::Index(7)
        );
    }
    for name in ["7", "next", "Work notes", "仕事"] {
        assert_eq!(
            Fixture::target(
                Called::of::<Named>(&state, WorkspaceName::try_from(name).unwrap()).await
            ),
            switch_workspace::Target::Name(name.into())
        );
    }
    for (next, direction) in [(true, Direction::Next), (false, Direction::Previous)] {
        assert_eq!(
            Fixture::target(Called::of::<Cycle>(&state, next).await),
            switch_workspace::Target::Direction(direction as i32)
        );
    }
}

#[tokio::test]
async fn command_inputs_reject_invalid_indices_before_effects() {
    for value in [
        0_u64.into_value(),
        (u32::MAX as u64).into_value(),
        (-1_i64).into_value(),
        1.5_f64.into_value(),
        "next".into_value(),
        true.into_value(),
    ] {
        let called = Called::raw::<Select>(&State::new(), vec![value]).await;
        assert!(called.answer.is_err());
        assert!(called.effects.is_empty());
    }
    assert_eq!(
        WorkspaceIndex::decode(Args::new(vec!["10".into_value()]))
            .unwrap()
            .get(),
        10
    );
}

#[test]
fn switching_adds_no_reading_or_process_execution_grants() {
    let manifest =
        manifest_of(&omega::Plugin::named(env!("CARGO_PKG_NAME"), "1").command::<Select>());
    assert!(!manifest.capabilities.contains(&(Capability::Spawn as i32)));
    assert!(manifest.state_topics.is_empty());
}
