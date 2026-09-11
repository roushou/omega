//! A minimal widget. Run its tests with `cargo test`.
use omega::ui::Text;
use omega::{Plugin, Ui, Widget};

pub const UNIT: &str = env!("CARGO_PKG_NAME");

#[derive(omega::Widget)]
pub struct Hello;

impl Widget for Hello {
    fn render(&self) -> Ui {
        Text::new("Hello from Omega").into()
    }
}

pub fn plugin() -> Plugin {
    omega::plugin!().widget::<Hello>()
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
