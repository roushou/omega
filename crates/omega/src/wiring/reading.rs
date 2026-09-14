//! Topic-backed field construction and access. Domain modules own the handles.

macro_rules! reading {
    ($(#[$meta:meta])* $handle:ident : $value:ty) => {
        $(#[$meta])*
        #[derive(Debug)]
        pub struct $handle { context: $crate::runtime::context::Context }

        impl $crate::wiring::Wiring for $handle {
            const TOPICS: &'static [omega_proto::SystemTopic] =
                &[<$value as omega_proto::TopicValue>::TOPIC];
            const CAPABILITIES: &'static [omega_proto::omega::Capability] =
                &[omega_proto::omega::Capability::StateRead];

            fn build(context: &$crate::runtime::context::Context) -> Self {
                Self {
                    context: context.clone(),
                }
            }
        }

        impl $crate::wiring::Reads for $handle {}

        impl $handle {
            /// Whether there is a reading right now.
            ///
            /// False on a machine that has no such device, and false while a
            /// broker that reports it is down — a widget branches on having a
            /// reading, not on why it has none. Required readings gate the
            /// first render until a topic is reported, including explicit absence.
            /// `Optional<R>` removes that gate and distinguishes pending state.
            pub fn has_reading(&self) -> bool {
                self.read().is_some()
            }

            /// The whole reading, as the daemon published it.
            ///
            /// The floor under every handle. A topic with typed accessors has
            /// better ways to be asked — `charge()` gives a `Percent` where
            /// this gives the wire's `f64`.
            pub fn get(&self) -> Option<$value> {
                self.read()
            }

            fn read(&self) -> Option<$value> {
                self.context.topic::<$value>()
            }
        }
    };
}

pub(crate) use reading;
