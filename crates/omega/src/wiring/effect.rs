/// Declares an effect handle: what it costs, and how it is built.
macro_rules! does {
    ($handle:ident, $capability:ident) => {
        impl $crate::wiring::Wiring for $handle {
            const CAPABILITIES: &'static [omega_proto::omega::Capability] =
                &[omega_proto::omega::Capability::$capability];

            fn build(context: &$crate::runtime::context::Context) -> Self {
                Self {
                    context: context.clone(),
                }
            }
        }

        impl $crate::wiring::Does for $handle {}

        impl $handle {
            /// Queue one action for the daemon.
            fn act(&self, kind: omega_proto::omega::action::Kind) -> $crate::effect::Effect {
                $crate::effect::Effect::new(self.context.act(omega_proto::omega::invoke::Op::Act(
                    omega_proto::omega::Act {
                        action: Some(omega_proto::omega::Action { kind: Some(kind) }),
                    },
                )))
            }
        }
    };
}

pub(crate) use does;
