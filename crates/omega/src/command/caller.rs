//! Declared command dependencies and typed remote invocation.
use super::{Command, CommandRef, CommandValue, Input};
use crate::{
    effect::{Effect, EffectError},
    runtime::context::Context,
    wiring::{Does, Wiring},
};
use omega_proto::omega::{Act, Action, InvokePlugin, action, invoke};
use std::marker::PhantomData;

/// Permission to invoke one command. Declare this field in behavior dependencies;
/// the manifest records the target and expected signature. Calls are admitted
/// immediately and are never replayed after timeout or target replacement.
///
/// ```no_run
/// # use omega::{Command, Percent};
/// # #[derive(omega::Command)] struct SetVolume { volume: omega::platform::audio::Volume }
/// # impl Command for SetVolume { type Input = Percent; type Output = (); async fn call(&self, p: Percent) -> omega::Result<()> { self.volume.set(p).await } }
/// # async fn example(caller: &omega::command::Caller<SetVolume>) -> omega::Result<()> {
/// caller.call(Percent::whole(30)).await?;
/// # Ok(()) }
/// ```
pub struct Caller<C> {
    context: Context,
    command: PhantomData<fn() -> C>,
}
impl<C> std::fmt::Debug for Caller<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Caller").finish_non_exhaustive()
    }
}
impl<C> Clone for Caller<C> {
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
            command: PhantomData,
        }
    }
}
impl<C: Command> Wiring for Caller<C> {
    fn commands() -> Vec<omega_proto::omega::CommandDependency> {
        vec![CommandRef::<C>::INSTANCE.descriptor().dependency(C::PLUGIN)]
    }
    fn build(context: &Context) -> Self {
        Self {
            context: context.clone(),
            command: PhantomData,
        }
    }
}
impl<C: Command> Does for Caller<C> {}
impl<C: Command> Caller<C> {
    pub fn call(&self, input: C::Input) -> Effect<C::Output> {
        self.invoke(CommandRef::<C>::INSTANCE.with(input))
    }
    pub fn invoke(&self, invocation: super::Invocation<C>) -> Effect<C::Output> {
        Effect::decoded(self.context.act(invocation.operation()), |value| {
            let value = match value {
                Some(value) => value,
                None if C::Output::shape().kind
                    == omega_proto::omega::command_type::Kind::Unit as i32 =>
                {
                    Default::default()
                }
                None => return Err(EffectError::UnexpectedResponse),
            };
            C::Output::decode_value(&value).map_err(|_| EffectError::UnexpectedResponse)
        })
    }
}

/// A completely bound command invocation. Constructing one performs no work.
pub struct Invocation<C: Command> {
    pub(crate) args: Vec<omega_proto::omega::Value>,
    command: PhantomData<fn() -> C>,
}
impl<C: Command> std::fmt::Debug for Invocation<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Invocation")
            .field("plugin", &C::PLUGIN)
            .field("command", &C::NAME)
            .finish_non_exhaustive()
    }
}
impl<C: Command> Invocation<C> {
    pub(crate) fn new(input: C::Input) -> Self {
        Self {
            args: input.encode(),
            command: PhantomData,
        }
    }
    pub fn into_wire(self) -> InvokePlugin {
        InvokePlugin {
            plugin: C::PLUGIN.into(),
            command: C::NAME.into(),
            signature: CommandRef::<C>::INSTANCE.descriptor().signature(C::PLUGIN),
            args: self.args,
        }
    }
    pub(crate) fn operation(self) -> invoke::Op {
        invoke::Op::Act(Act {
            action: Some(Action {
                kind: Some(action::Kind::InvokePlugin(self.into_wire())),
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CommandName;
    use crate::wiring::Wired;
    use omega_proto::IntoValue;

    #[derive(crate::Command)]
    struct Echo {}
    impl Command for Echo {
        type Input = String;
        type Output = Option<String>;
        async fn call(&self, input: String) -> crate::Result<Self::Output> {
            Ok(Some(input))
        }
    }
    #[derive(crate::Effects)]
    struct Dependencies {
        echo: Caller<Echo>,
    }

    #[tokio::test]
    async fn declared_caller_preserves_address_signature_and_optional_result() {
        let dependencies = Dependencies::commands();
        assert_eq!(
            dependencies,
            vec![
                CommandRef::<Echo>::INSTANCE
                    .descriptor()
                    .dependency(Echo::PLUGIN)
            ]
        );
        let (sender, mut queue) = crate::effect::queue::Effects::channel();
        let context = Context::new(&Default::default(), sender);
        let caller = Dependencies::build(&context, &Default::default()).echo;
        let answer = caller.call("hello".into());
        let capture = crate::testing::CapturedEffect::from_request(queue.try_recv().unwrap());
        let call = capture.command::<Echo>().unwrap();
        assert_eq!(call.input(), "hello");
        call.complete(Ok(None)).unwrap();
        assert_eq!(answer.await.unwrap(), None);
        let answer = caller.call("hello".into());
        queue
            .try_recv()
            .unwrap()
            .complete(Ok(Some(42i64.into_value())))
            .unwrap();
        assert!(answer.await.is_err());
    }

    #[test]
    fn bound_invocation_retains_owner_and_is_pure() {
        let binding: crate::ui::Bind<()> = CommandRef::<Echo>::INSTANCE.with("hello".into()).into();
        let binding = binding.into_wire();
        assert_eq!(binding.plugin, Echo::PLUGIN);
        assert_eq!(binding.command, Echo::NAME);
        assert_eq!(
            binding.signature,
            CommandRef::<Echo>::INSTANCE
                .descriptor()
                .signature(Echo::PLUGIN)
        );
    }
}
