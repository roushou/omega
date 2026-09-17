//! A minimal widget. Run its tests with `cargo test`.
use omega::surface::{Events, Task};
use omega::ui::Text;
use omega::{Plugin, Surface, View};
use std::convert::Infallible;

pub const PLUGIN: &str = env!("CARGO_PKG_NAME");

#[derive(Debug, omega::Surface)]
pub struct Hello;

impl Surface for Hello {
    type Model = ();
    type Message = Infallible;
    type Effects = ();
    fn render(&self, _: &(), _: &Events<Infallible>) -> View {
        Text::new("Hello from Omega").into()
    }

    fn update(&self, _: &mut (), message: Infallible, _: &()) -> Task<Infallible> {
        match message {}
    }
}

pub fn plugin() -> Plugin {
    omega::plugin!().surface(Hello)
}

#[cfg(test)]
mod tests {
    use super::*;
    use omega::testing::{Drawn, State};

    #[test]
    fn it_shows_a_greeting() {
        assert_eq!(
            Drawn::of::<Hello>(&State::new()).unwrap().text(),
            "Hello from Omega"
        );
    }
}
