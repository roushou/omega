//! Screen capture and clipboard handles read state and declare capabilities.

use omega::testing::{Drawn, State, manifest_of};
use omega::{Command, Surface, View, ui::Text};
use omega_proto::omega::Capability;

#[derive(omega::Surface)]
struct Clip {
    clipboard: omega::platform::clipboard::Clipboard,
}

impl Surface for Clip {
    type Model = ();
    type Message = std::convert::Infallible;
    type Effects = ();
    fn update(
        &self,
        _: &mut (),
        message: Self::Message,
        _: &(),
    ) -> omega::surface::Task<Self::Message> {
        match message {}
    }
    fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> View {
        Text::new(self.clipboard.text().unwrap_or_default()).into()
    }
}

#[test]
fn clipboard_reads_its_topic_and_reports_empty_as_none() {
    let drawn = Drawn::of::<Clip>(&State::new().clipboard("hello")).unwrap();
    assert_eq!(drawn.text(), "hello");
    // A reading is required before rendering; with none reported the surface
    // has not rendered yet, so `Drawn` returns the empty pre-render state.
    let absent = Drawn::of::<Clip>(&State::new()).unwrap();
    assert_eq!(absent.text(), "");
}

#[derive(omega::Command)]
struct Snap {
    capture: omega::platform::capture::Capture,
}
impl Command for Snap {
    const ID: &'static str = "snap";

    type Input = ();
    type Output = ();
    async fn call(&self, _: ()) -> omega::Result<()> {
        let _ = &self.capture;
        Ok(())
    }
}

#[derive(omega::Command)]
struct Paste {
    write: omega::platform::clipboard::Write,
}
impl Command for Paste {
    const ID: &'static str = "paste";

    type Input = ();
    type Output = ();
    async fn call(&self, _: ()) -> omega::Result<()> {
        let _ = &self.write;
        Ok(())
    }
}

#[test]
fn capture_and_clipboard_controls_declare_their_capabilities() {
    let manifest = manifest_of(
        &omega::Plugin::new("capture", "0.1.0")
            .command::<Snap>()
            .command::<Paste>(),
    );
    let granted = manifest.granted().unwrap();
    assert!(granted.contains(&Capability::Screenshot));
    assert!(granted.contains(&Capability::Clipboard));
}
