//! A widget's identity, without constructing its readings or settings.

use std::marker::PhantomData;

/// Identity emitted by `derive(Surface)` and shared by registration and placement.
#[doc(hidden)]
pub trait SurfaceIdentity {
    const UNIT: &'static str;
    const SURFACE: &'static str;
}

/// A typed reference to a widget surface. `derive(Surface)` supplies a value with
/// the widget's name, following the same convention as typed command references.
/// The unit comes from the defining crate; the surface defaults to the type's
/// kebab-case name. `#[omega(name = "indicator")]` pins it across type renames.
///
/// ```
/// use omega::{View, Surface, surface::SurfaceRef, ui::Text};
/// #[derive(omega::Surface)]
/// #[omega(name = "indicator")]
/// struct Charge { battery: omega::platform::power::Battery }
/// impl Surface for Charge {
///     type Model = ();
///     type Message = std::convert::Infallible;
///     type Effects = ();
///     fn update(&self, _: &mut (), message: Self::Message, _: &()) -> omega::surface::Task<Self::Message> {
///         match message {}
///     }
///
///     fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> View { Text::new(self.battery.charge()).into() }
/// }
/// let reference: SurfaceRef<Charge> = Charge;
/// assert_eq!(reference.surface(), "indicator");
/// let plugin = omega::plugin!().surface(Charge);
/// assert_eq!(plugin.manifest().unwrap().surfaces[0].id, "indicator");
/// ```
pub struct SurfaceRef<W>(PhantomData<fn() -> W>);
impl<W> Copy for SurfaceRef<W> {}
impl<W> Clone for SurfaceRef<W> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<W> std::fmt::Debug for SurfaceRef<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SurfaceRef")
    }
}
impl<W> SurfaceRef<W> {
    #[doc(hidden)]
    pub const INSTANCE: Self = Self(PhantomData);
}
impl<W: SurfaceIdentity> SurfaceRef<W> {
    /// The package that defines this widget.
    pub fn unit(self) -> &'static str {
        W::UNIT
    }
    /// The surface name used by typed registration.
    pub fn surface(self) -> &'static str {
        W::SURFACE
    }
}
