//! A minimal widget. Run its tests with `cargo test`.
use omega::ui::Text;
use omega::{Plugin, Surface, View};

pub const UNIT: &str = env!("CARGO_PKG_NAME");

#[derive(Debug, omega::Surface)]
pub struct Hello;

impl Surface for Hello {
    fn render(&self) -> View {
        Text::new("Hello from Omega").into()
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
        assert_eq!(Drawn::of::<Hello>(&State::new()).text(), "Hello from Omega");
    }
}
