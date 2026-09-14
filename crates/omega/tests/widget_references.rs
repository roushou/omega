use omega::{Surface, View, surface::SurfaceRef, ui::Text};

#[derive(omega::Surface)]
struct Indicator {
    battery: omega::platform::power::Battery,
}
impl Surface for Indicator {
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
        Text::new(self.battery.charge()).into()
    }
}

#[derive(omega::Surface)]
#[omega(name = "panel")]
struct RenamedPanel;
impl Surface for RenamedPanel {
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
        Text::new("Panel").into()
    }
}

#[test]
fn references_and_registration_share_identity_without_constructing_a_widget() {
    let reference: SurfaceRef<Indicator> = Indicator;
    assert_eq!(reference.unit(), env!("CARGO_PKG_NAME"));
    assert_eq!(reference.surface(), "indicator");
    let manifest = omega::plugin!()
        .surface(reference)
        .surface(RenamedPanel)
        .manifest()
        .unwrap();
    let surfaces: Vec<_> = manifest.surfaces.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(surfaces, ["indicator", "panel"]);
    assert_eq!(manifest.state_topics, ["battery"]);
}

#[test]
fn duplicate_typed_or_explicit_registrations_are_rejected() {
    assert!(
        omega::plugin!()
            .surface(Indicator)
            .surface(Indicator)
            .manifest()
            .is_err()
    );
    assert!(
        omega::plugin!()
            .surface(RenamedPanel)
            .surface_as::<Indicator>("panel")
            .manifest()
            .is_err()
    );
}

#[test]
fn typed_registration_cannot_silently_change_the_owning_unit() {
    let error = omega::Plugin::named("other-unit", "0.1.0")
        .surface(Indicator)
        .manifest()
        .unwrap_err();
    assert!(error.to_string().contains("belongs to"));
}
