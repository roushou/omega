//! Explicit development cases using Omega's production renderer and isolated runtime.
//!
//! Register a normal Rust test called `previews::preview` in a plugin or library.
//! `cargo test` validates the cases; `omega preview <package>` opens them visually.
//! Keep this crate in `[dev-dependencies]` so private components remain accessible
//! without preview registrations or dependencies in a production binary.
//!
//! ```no_run
//! #![allow(clippy::test_attr_in_doctest)]
//! #[cfg(test)]
//! mod previews {
//!     #[test]
//!     fn preview() {
//!         omega_preview::Cases::new()
//!             .component("ready", || omega::ui::Text::new("Ready"))
//!             .component("long-label", || omega::ui::Text::new("A deliberately long label"))
//!             .run().unwrap();
//!     }
//! }
//! ```
mod scene;
mod session;
use omega::{Surface, View, testing::State};
use omega_proto::preview::CaseId;
use scene::{Component, Scene};
use std::{collections::BTreeMap, rc::Rc};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Omega(#[from] omega::Error),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Transport(#[from] omega_proto::preview::PreviewError),
    #[error("{0}")]
    Invalid(String),
}

type Factory = Box<dyn Fn() -> omega::Result<Box<dyn Scene>>>;
/// Named factories: selecting or resetting a case always constructs fresh state.
#[derive(Default)]
pub struct Cases {
    factories: BTreeMap<CaseId, Factory>,
    error: Option<String>,
}
impl std::fmt::Debug for Cases {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cases")
            .field("cases", &self.factories.keys())
            .finish()
    }
}
impl Cases {
    pub fn new() -> Self {
        Self::default()
    }
    /// Register a component or composed view, including a private component.
    pub fn component<V: Into<View>, F: Fn() -> V + 'static>(
        mut self,
        name: &str,
        render: F,
    ) -> Self {
        let render = Rc::new(render);
        self.insert(
            name,
            Box::new(move || {
                let render = render.clone();
                Ok(Box::new(Component(move || render().into())))
            }),
        );
        self
    }
    /// Register a production surface with synthetic readings.
    /// Every external effect waits for explicit success/refusal in the preview.
    pub fn surface<S: Surface + 'static>(mut self, name: &str, state: State) -> Self {
        self.insert(
            name,
            Box::new(move || Ok(Box::new(scene::Surface::<S>::new(&state)?))),
        );
        self
    }
    /// Reuse a fixture factory that supplies settings, initial messages, or custom
    /// behavior dependencies through `SurfaceHarness`. The factory also serves tests.
    pub fn surface_with<S: Surface + 'static>(
        mut self,
        name: &str,
        build: impl Fn() -> omega::Result<omega::testing::SurfaceHarness<S>> + 'static,
    ) -> Self {
        self.insert(
            name,
            Box::new(move || Ok(Box::new(scene::Surface::<S>::from_harness(build()?)?))),
        );
        self
    }
    fn insert(&mut self, name: &str, factory: Factory) {
        if self.factories.len() >= 128 {
            self.error = Some("preview catalogue exceeds 128 cases".into());
            return;
        }
        match CaseId::parse(name) {
            Ok(id) if !self.factories.contains_key(&id) => {
                self.factories.insert(id, factory);
            }
            Ok(_) => self.error = Some(format!("duplicate preview case: {name}")),
            Err(e) => self.error = Some(e.to_string()),
        }
    }
    /// Render a named case for structural assertions using the same fixture factory.
    /// Surface cases require a Tokio runtime when their initialization starts tasks.
    ///
    /// ```
    /// let cases = omega_preview::Cases::new().component("ready", || omega::ui::Text::new("Ready"));
    /// assert_eq!(cases.draw("ready").unwrap().text(), "Ready");
    /// ```
    pub fn draw(&self, name: &str) -> Result<omega::testing::Drawn, Error> {
        if let Some(error) = &self.error {
            return Err(Error::Invalid(error.clone()));
        }
        let id = CaseId::parse(name).map_err(|e| Error::Invalid(e.to_string()))?;
        let factory = self
            .factories
            .get(&id)
            .ok_or_else(|| Error::Invalid(format!("unknown preview case: {name}")))?;
        Ok(factory()?.draw())
    }
    /// Validate every initial render in ordinary tests; connect to the CLI's
    /// private session only when invoked by `omega preview`.
    pub fn run(self) -> Result<(), Error> {
        if let Some(error) = &self.error {
            return Err(Error::Invalid(error.clone()));
        }
        if self.factories.is_empty() {
            return Err(Error::Invalid("register at least one preview case".into()));
        }
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                for factory in self.factories.values() {
                    factory()?.draw();
                }
                if let Some(socket) = std::env::var_os("OMEGA_PREVIEW_SOCKET") {
                    session::Session::run(self, socket).await?;
                }
                Ok(())
            })
    }
}

#[cfg(test)]
mod previews {
    use super::*;
    use omega::{
        surface::{Events, Task},
        ui::{Button, Column, Text},
    };
    struct Badge(&'static str);
    impl Badge {
        fn view(&self) -> View {
            Text::new(self.0).into()
        }
    }
    #[derive(omega::Surface)]
    struct Counter {}
    impl Surface for Counter {
        type Model = u32;
        type Message = ();
        type Effects = ();
        fn render(&self, count: &u32, events: &Events<()>) -> View {
            Column::new()
                .child(Text::new(format!("Count: {count}")))
                .child(
                    Button::new("Increment")
                        .key("increment")
                        .on_press(events.send(())),
                )
                .into()
        }
        fn update(&self, count: &mut u32, _: (), _: &()) -> Task<()> {
            *count += 1;
            Task::none()
        }
    }
    #[test]
    fn preview() {
        Cases::new()
            .component("ready", || Badge("Ready").view())
            .component("long-label", || {
                Badge("A deliberately long component label that wraps in a narrow viewport").view()
            })
            .component("disabled", || Button::new("Unavailable").disabled())
            .surface::<Counter>("counter", State::new())
            .run()
            .unwrap();
    }
    #[derive(omega::Surface)]
    struct Charge {
        battery: omega::platform::power::Battery,
    }
    impl Surface for Charge {
        type Model = ();
        type Message = std::convert::Infallible;
        type Effects = ();
        fn render(&self, _: &(), _: &Events<Self::Message>) -> View {
            if self.battery.has_reading() {
                Text::new(self.battery.charge()).into()
            } else {
                Text::new("Unavailable").into()
            }
        }
        fn update(&self, _: &mut (), message: Self::Message, _: &()) -> Task<Self::Message> {
            match message {}
        }
    }
    #[test]
    fn message_free_surface_cases_obey_readiness_without_tokio() {
        let cases = Cases::new()
            .surface::<Charge>("pending", State::new())
            .surface::<Charge>("ready", State::new().battery(0.5, false))
            .surface::<Charge>(
                "absent",
                State::new().absent(omega::testing::SystemTopic::Battery),
            );
        assert!(cases.draw("pending").unwrap().is_empty());
        assert_eq!(cases.draw("ready").unwrap().text(), "50%");
        assert_eq!(cases.draw("absent").unwrap().text(), "Unavailable");
    }

    #[test]
    fn duplicate_registration_fails_loudly() {
        assert!(
            Cases::new()
                .component("same", || Text::new("one"))
                .component("same", || Text::new("two"))
                .run()
                .is_err()
        );
    }
}
