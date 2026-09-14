//! Typed placement of plugin surfaces outside the bar.
use ::omega::surface::{SurfaceIdentity, SurfaceRef};
use omega_proto::{
    Fields,
    omega::{self, presentation},
};

/// Independent presentations use the same typed surface references as bars.
///
/// ```
/// use omega::{Surface, View, ui::Text};
/// use omega_document::{Document, Presentations};
/// #[derive(omega::Surface)]
/// struct Example {}
/// impl Surface for Example {
///     type Model = ();
///     type Message = std::convert::Infallible;
///     type Effects = ();
///     fn update(&self, _: &mut (), message: Self::Message, _: &()) -> omega::surface::Task<Self::Message> {
///         match message {}
///     }
///     fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> View { Text::new("Hello").into() } }
/// let document = Document::new().presentation(
///     Presentations::window("example", Example).title("Example").size(640, 480)
/// );
/// assert_eq!(document.into_inner().presentations.len(), 1);
/// ```
#[derive(Debug)]
pub struct Presentations;
impl Presentations {
    pub fn window<W: SurfaceIdentity>(id: impl Into<String>, surface: SurfaceRef<W>) -> Window {
        Window {
            entry: Self::entry(id, surface),
            spec: omega::WindowPresentation {
                title: surface.surface().into(),
                app_id: format!("org.omega.{}", surface.unit()),
                width: 480,
                height: 320,
                min_width: 1,
                min_height: 1,
            },
        }
    }
    pub fn overlay<W: SurfaceIdentity>(id: impl Into<String>, surface: SurfaceRef<W>) -> Overlay {
        Overlay {
            entry: Self::entry(id, surface),
            spec: omega::OverlayPresentation {
                dismiss_on_outside: false,
                width: 480,
                height: 320,
                output: String::new(),
                keyboard: omega::KeyboardPolicy::OnDemand as i32,
            },
        }
    }
    fn entry<W: SurfaceIdentity>(
        id: impl Into<String>,
        surface: SurfaceRef<W>,
    ) -> omega::ConfiguredPresentation {
        omega::ConfiguredPresentation {
            id: id.into(),
            unit: surface.unit().into(),
            surface: surface.surface().into(),
            ..Default::default()
        }
    }
}

/// A normal, independently resizable desktop window.
#[derive(Debug, Clone)]
pub struct Window {
    entry: omega::ConfiguredPresentation,
    spec: omega::WindowPresentation,
}
impl Window {
    pub fn configured(mut self, settings: &impl Fields) -> Self {
        self.entry.config = settings.write().into_map();
        self
    }
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.spec.title = title.into();
        self
    }
    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.spec.width = width;
        self.spec.height = height;
        self
    }
    pub fn minimum_size(mut self, width: u32, height: u32) -> Self {
        self.spec.min_width = width;
        self.spec.min_height = height;
        self
    }
}
impl From<Window> for omega::ConfiguredPresentation {
    fn from(mut window: Window) -> Self {
        window.entry.presentation = Some(omega::Presentation {
            kind: Some(presentation::Kind::Window(window.spec)),
        });
        window.entry
    }
}

/// A layer-shell surface with explicit keyboard and output selection.
#[derive(Debug, Clone)]
pub struct Overlay {
    entry: omega::ConfiguredPresentation,
    spec: omega::OverlayPresentation,
}
impl Overlay {
    pub fn configured(mut self, settings: &impl Fields) -> Self {
        self.entry.config = settings.write().into_map();
        self
    }
    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.spec.width = width;
        self.spec.height = height;
        self
    }
    pub fn keyboard(mut self, policy: Keyboard) -> Self {
        self.spec.keyboard = match policy {
            Keyboard::None => omega::KeyboardPolicy::None,
            Keyboard::OnDemand => omega::KeyboardPolicy::OnDemand,
            Keyboard::Exclusive => omega::KeyboardPolicy::Exclusive,
        } as i32;
        self
    }
    /// Capture clicks outside the content and dismiss this overlay.
    pub fn dismiss_on_outside(mut self) -> Self {
        self.spec.dismiss_on_outside = true;
        self
    }
    pub fn output(mut self, name: impl Into<String>) -> Self {
        self.spec.output = name.into();
        self
    }
}
impl From<Overlay> for omega::ConfiguredPresentation {
    fn from(mut overlay: Overlay) -> Self {
        overlay.entry.presentation = Some(omega::Presentation {
            kind: Some(presentation::Kind::Overlay(overlay.spec)),
        });
        overlay.entry
    }
}

/// A compositor may refuse focus; this selects the layer-shell request policy.
#[derive(Debug, Clone, Copy)]
pub enum Keyboard {
    None,
    OnDemand,
    Exclusive,
}
