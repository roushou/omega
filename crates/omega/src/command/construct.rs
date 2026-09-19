use super::Dependencies;

/// Construct a fresh command handler from declared dependencies for each invocation.
///
/// `#[derive(omega::Command)]` supplies this implementation through generated
/// wiring. Implement this trait yourself for a command without the derive.
/// The same dependency type determines grants, subscriptions, and required
/// readings. Construction runs after those readings initialize; shared state
/// must live in a dependency rather than in the temporary handler.
///
/// ```
/// use omega::{Command, command::Construct, platform::session::Session};
///
/// struct Lock { session: Session }
///
/// impl Construct for Lock {
///     type Dependencies = (Session,);
///
///     fn construct((session,): Self::Dependencies) -> Self {
///         Self { session }
///     }
/// }
///
/// impl Command for Lock {
///     type Input = ();
///     type Output = ();
///     const ID: &'static str = "session.lock";
///
///     async fn call(&self, _: ()) -> omega::Result<()> {
///         self.session.lock().await
///     }
/// }
/// ```
pub trait Construct: Sized + Send + Sync + 'static {
    /// Handles supplying the command's grants and subscriptions.
    type Dependencies: Dependencies;

    /// Assemble the handler from dependencies supplied by Omega.
    fn construct(dependencies: Self::Dependencies) -> Self;
}

impl<T: Dependencies> Construct for T {
    type Dependencies = Self;

    fn construct(dependencies: Self) -> Self {
        dependencies
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::audio::{Audio, Volume};
    use crate::testing::{Called, State, TestDaemon};
    use crate::{Command, Percent, Plugin};
    use omega_proto::{IntoValue, SystemTopic};
    use std::sync::atomic::{AtomicU32, Ordering};

    #[derive(crate::Command)]
    struct Derived {
        _audio: Audio,
        volume: Volume,
    }

    impl Command for Derived {
        type Input = Percent;
        type Output = ();
        const ID: &'static str = "construction.volume";

        async fn call(&self, level: Percent) -> crate::Result<()> {
            self.volume.set(level).await
        }
    }

    struct Explicit {
        _audio: Audio,
        volume: Volume,
    }

    impl Construct for Explicit {
        type Dependencies = (Audio, Volume);

        fn construct((audio, volume): Self::Dependencies) -> Self {
            Self {
                _audio: audio,
                volume,
            }
        }
    }

    impl Command for Explicit {
        type Input = Percent;
        type Output = ();
        const ID: &'static str = Derived::ID;

        async fn call(&self, level: Percent) -> crate::Result<()> {
            self.volume.set(level).await
        }
    }

    #[tokio::test]
    async fn derived_and_explicit_construction_have_identical_grants_and_effects() {
        let derived = Plugin::new("example", "1")
            .command::<Derived>()
            .manifest()
            .unwrap();
        let explicit = Plugin::new("example", "1")
            .command::<Explicit>()
            .manifest()
            .unwrap();
        assert_eq!(derived, explicit);
        assert_eq!(derived.state_topics, ["audio"]);
        let state = State::new().absent(SystemTopic::Audio);
        let a = Called::of::<Derived>(&state, Percent::whole(40)).await;
        let b = Called::of::<Explicit>(&state, Percent::whole(40)).await;
        assert!(a.answer.is_ok());
        assert!(b.answer.is_ok());
        assert!(!a.effects.is_empty());
        assert_eq!(a.effects, b.effects);
    }

    static CONSTRUCTIONS: AtomicU32 = AtomicU32::new(0);

    struct Fresh {
        _audio: Audio,
        calls: AtomicU32,
    }

    impl Construct for Fresh {
        type Dependencies = (Audio,);

        fn construct((audio,): Self::Dependencies) -> Self {
            CONSTRUCTIONS.fetch_add(1, Ordering::SeqCst);
            Self {
                _audio: audio,
                calls: AtomicU32::new(0),
            }
        }
    }

    impl Command for Fresh {
        type Input = ();
        type Output = u32;
        const ID: &'static str = "construction.fresh";

        async fn call(&self, _: ()) -> crate::Result<u32> {
            Ok(self.calls.fetch_add(1, Ordering::SeqCst))
        }
    }

    #[tokio::test]
    async fn construction_waits_for_valid_input_and_readings_and_handlers_are_fresh() {
        CONSTRUCTIONS.store(0, Ordering::SeqCst);
        let plugin = Plugin::new("example", "1").command::<Fresh>();
        plugin.manifest().unwrap();
        assert_eq!(CONSTRUCTIONS.load(Ordering::SeqCst), 0);

        let missing = Called::of::<Fresh>(&State::new(), ()).await;
        assert!(missing.answer.is_err());
        let ready = State::new().absent(SystemTopic::Audio);
        let invalid = Called::raw::<Fresh>(&ready, vec![true.into_value()]).await;
        assert!(invalid.answer.is_err());
        assert_eq!(CONSTRUCTIONS.load(Ordering::SeqCst), 0);

        for _ in 0..2 {
            assert_eq!(Called::of::<Fresh>(&ready, ()).await.answer.unwrap(), 0);
        }
        assert_eq!(CONSTRUCTIONS.load(Ordering::SeqCst), 2);

        let mut daemon = TestDaemon::serving(plugin);
        daemon.welcome(&ready).await;
        assert!(
            daemon
                .call(Fresh::ID, vec![true.into_value()])
                .await
                .is_err()
        );
        assert_eq!(CONSTRUCTIONS.load(Ordering::SeqCst), 2);
        for _ in 0..2 {
            assert_eq!(
                daemon.call(Fresh::ID, Vec::new()).await.unwrap(),
                Some(0u32.into_value())
            );
        }
        assert_eq!(CONSTRUCTIONS.load(Ordering::SeqCst), 4);
    }
}
