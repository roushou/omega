//! A widget's identity, without constructing its readings or settings.
use crate::Widget;
use std::marker::PhantomData;

/// Identity emitted by `derive(Widget)` and shared by registration and placement.
#[doc(hidden)]
pub trait WidgetIdentity {
    const UNIT: &'static str;
    const SURFACE: &'static str;
}

/// A typed reference to a widget surface. `derive(Widget)` supplies a value with
/// the widget's name, following the same convention as typed command references.
/// The unit comes from the defining crate; the surface defaults to the type's
/// kebab-case name. `#[omega(name = "indicator")]` pins it across type renames.
///
/// ```
/// use omega::{View, Widget, ui::{Text, WidgetRef}};
/// #[derive(omega::Widget)]
/// #[omega(name = "indicator")]
/// struct Charge { battery: omega::power::Battery }
/// impl Widget for Charge {
///     fn render(&self) -> View { Text::new(self.battery.charge()).into() }
/// }
/// let reference: WidgetRef<Charge> = Charge;
/// assert_eq!(reference.surface(), "indicator");
/// let plugin = omega::plugin!().widget(Charge);
/// assert_eq!(plugin.manifest().unwrap().surfaces[0].id, "indicator");
/// ```
pub struct WidgetRef<W>(PhantomData<fn() -> W>);
impl<W> Copy for WidgetRef<W> {}
impl<W> Clone for WidgetRef<W> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<W> std::fmt::Debug for WidgetRef<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WidgetRef")
    }
}
impl<W> WidgetRef<W> {
    #[doc(hidden)]
    pub const INSTANCE: Self = Self(PhantomData);
}
impl<W: Widget + WidgetIdentity> WidgetRef<W> {
    /// The package that defines this widget.
    pub fn unit(self) -> &'static str {
        W::UNIT
    }
    /// The surface name used by typed registration.
    pub fn surface(self) -> &'static str {
        W::SURFACE
    }
}
