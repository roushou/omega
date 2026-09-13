use omega::{
    View, Widget,
    ui::{Text, WidgetRef},
};

#[derive(omega::Widget)]
struct Indicator {
    battery: omega::power::Battery,
}
impl Widget for Indicator {
    fn render(&self) -> View {
        Text::new(self.battery.charge()).into()
    }
}

#[derive(omega::Widget)]
#[omega(name = "panel")]
struct RenamedPanel;
impl Widget for RenamedPanel {
    fn render(&self) -> View {
        Text::new("Panel").into()
    }
}

#[test]
fn references_and_registration_share_identity_without_constructing_a_widget() {
    let reference: WidgetRef<Indicator> = Indicator;
    assert_eq!(reference.unit(), env!("CARGO_PKG_NAME"));
    assert_eq!(reference.surface(), "indicator");
    let manifest = omega::plugin!()
        .widget(reference)
        .widget(RenamedPanel)
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
            .widget(Indicator)
            .widget(Indicator)
            .manifest()
            .is_err()
    );
    assert!(
        omega::plugin!()
            .widget(RenamedPanel)
            .widget_as::<Indicator>("panel")
            .manifest()
            .is_err()
    );
}

#[test]
fn typed_registration_cannot_silently_change_the_owning_unit() {
    let error = omega::Plugin::named("other-unit", "0.1.0")
        .widget(Indicator)
        .manifest()
        .unwrap_err();
    assert!(error.to_string().contains("belongs to"));
}
