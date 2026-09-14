//! Application activation and admission.
use crate::{effect::Effect, runtime::context::Context, wiring::does};
use omega_proto::ApplicationId;

/// Permission to activate applications. Completion confirms admission, not
/// eventual application startup or exit. A failed/unknown outcome is never retried.
#[derive(Debug)]
pub struct Launcher {
    context: Context,
}
does!(Launcher, Spawn);
impl Launcher {
    /// Activate without file arguments or a compositor token.
    pub fn launch(&self, id: &ApplicationId) -> Effect {
        self.open(id, &[], None)
    }
    /// Open literal absolute URIs through desktop-entry field-code handling.
    /// An optional compositor token is forwarded to the launched environment;
    /// focus remains the compositor's decision. Supplying a token for a D-Bus
    /// activated entry currently returns an unsupported-operation refusal.
    ///
    /// ```no_run
    /// # async fn example(launcher: &omega::platform::applications::Launcher, id: &omega::platform::applications::ApplicationId) -> omega::Result<()> {
    /// launcher.open(id, &["file:///tmp/a%20file.txt"], None).await?;
    /// # Ok(()) }
    /// ```
    pub fn open(
        &self,
        id: &ApplicationId,
        uris: &[&str],
        activation_token: Option<&str>,
    ) -> Effect {
        self.act(omega_proto::omega::action::Kind::LaunchApp(
            omega_proto::omega::LaunchApp {
                desktop_id: id.to_string(),
                uris: uris.iter().map(|s| s.to_string()).collect(),
                activation_token: activation_token.unwrap_or_default().into(),
            },
        ))
    }
}
