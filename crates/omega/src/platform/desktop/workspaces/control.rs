use crate::{effect::Effect, runtime::context::Context, wiring::does};
use omega_proto::{
    WorkspaceIndex, WorkspaceName,
    omega::{Direction, SwitchWorkspace, action, switch_workspace},
};

/// Switch compositor workspaces through the daemon.
///
/// Completion acknowledges the compositor request; [`super::Workspaces`] reports
/// the observed focus. Destinations need not appear in the reading: the compositor
/// may create an empty workspace. Requests are not retried. Backend failures are
/// returned by the effect. No additional action capability is required.
///
/// ```no_run
/// use omega::{Command, platform::desktop::{WorkspaceControl, WorkspaceIndex}};
/// #[derive(omega::Command)]
/// struct Select { workspaces: WorkspaceControl }
/// impl Command for Select {
///     type Input = WorkspaceIndex;
///     type Output = ();
///     async fn call(&self, index: WorkspaceIndex) -> omega::Result<()> {
///         self.workspaces.switch_to(index).await
///     }
/// }
/// ```
///
/// ```compile_fail
/// #[derive(omega::Surface)]
/// struct Indicator { workspaces: omega::platform::desktop::WorkspaceControl }
/// ```
#[derive(Debug)]
pub struct WorkspaceControl {
    context: Context,
}

does!(WorkspaceControl);

impl WorkspaceControl {
    /// Focus a numbered workspace. Output assignment follows compositor rules.
    pub fn switch_to(&self, index: WorkspaceIndex) -> Effect {
        self.switch(switch_workspace::Target::Index(index.get()))
    }

    /// Focus a literal named workspace, including names that resemble selectors.
    ///
    /// ```no_run
    /// # async fn example(control: &omega::platform::desktop::WorkspaceControl) -> omega::Result<()> {
    /// use omega::platform::desktop::WorkspaceName;
    /// let name = "mail".parse::<WorkspaceName>().unwrap();
    /// control.switch_to_named(&name).await?;
    /// # Ok(()) }
    /// ```
    pub fn switch_to_named(&self, name: &WorkspaceName) -> Effect {
        self.switch(switch_workspace::Target::Name(name.to_string()))
    }

    /// Focus the next existing workspace in compositor order, wrapping at the end.
    /// Hyprland includes workspaces across outputs and skips unopened workspaces.
    pub fn next(&self) -> Effect {
        self.switch(switch_workspace::Target::Direction(Direction::Next as i32))
    }

    /// Focus the previous existing workspace in compositor order, wrapping at the start.
    /// This is reverse traversal, not focus history.
    pub fn previous(&self) -> Effect {
        self.switch(switch_workspace::Target::Direction(
            Direction::Previous as i32,
        ))
    }

    fn switch(&self, target: switch_workspace::Target) -> Effect {
        self.act(action::Kind::SwitchWorkspace(SwitchWorkspace {
            target: Some(target),
        }))
    }
}
