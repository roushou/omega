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
            /// Whether a current reading is available. Returns `false` before the first
            /// reading, after reported absence, or while the service is unavailable.
            /// Required topics delay the first render until reported;
            /// [`Optional`](crate::surface::Optional) allows rendering before that report.
            pub fn has_reading(&self) -> bool {
                self.read().is_some()
            }

            /// Return the raw protocol reading, or `None` if unavailable.
            /// Prefer the typed accessors for unit conversions and status interpretation.
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
