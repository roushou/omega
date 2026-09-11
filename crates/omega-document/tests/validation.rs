use omega_document::{Bars, Document, DocumentValidation, Modules, Units};
use omega_proto::omega::SurfaceKind;
use omega_proto::{Manifest, Surface, SurfaceId, UnitName};

struct Fixture;
impl Fixture {
    fn manifest() -> Manifest {
        Manifest::new(&UnitName::parse("clock").unwrap(), "1").exposing([
            Surface::new(&SurfaceId::parse("time").unwrap(), SurfaceKind::Widget),
            Surface::new(&SurfaceId::parse("calendar").unwrap(), SurfaceKind::Widget),
        ])
    }
}

#[test]
fn unit_references_and_duplicate_declarations_are_validated() {
    let manifest = Fixture::manifest();
    let unknown = Document::new().unit(Units::enabled("missing")).into_inner();
    assert!(DocumentValidation::validate(&unknown, [&manifest]).is_err());
    let duplicate = Document::new()
        .unit(Units::enabled("clock"))
        .unit(Units::disabled("clock"))
        .into_inner();
    assert!(DocumentValidation::validate(&duplicate, [&manifest]).is_err());
}

#[test]
fn widget_resolution_is_identical_at_publication_and_adoption() {
    let manifest = Fixture::manifest();
    assert!(DocumentValidation::surface(&manifest, "").is_err());
    assert!(DocumentValidation::surface(&manifest, "missing").is_err());
    let document = Document::new()
        .bar(Bars::top(
            "main",
            vec![Modules::surface(
                Modules::plain_widget("slot", "clock"),
                "time",
            )],
        ))
        .into_inner();
    assert!(DocumentValidation::validate(&document, [&manifest]).is_ok());
}

#[test]
fn unsupported_domains_fail_instead_of_being_accepted_without_a_provider() {
    let mut document = omega_document::StateDocument::default();
    document.monitors.push(Default::default());
    assert!(DocumentValidation::validate(&document, []).is_err());
}

#[test]
fn environment_names_and_duplicates_are_rejected() {
    for key in ["BAD-NAME", "A;echo", "1BAD", ""] {
        let document = Document::new().env(key, "value").into_inner();
        assert!(DocumentValidation::validate(&document, []).is_err());
    }
    let duplicate = Document::new()
        .env("EDITOR", "a")
        .env("EDITOR", "b")
        .into_inner();
    assert!(DocumentValidation::validate(&duplicate, []).is_err());
}

#[test]
fn schedule_payload_errors_identify_the_schedule_and_reject_empty_envelopes() {
    use omega_proto::omega::{Action, Schedule, SetVolume, action, set_volume};
    for action in [
        Action::default(),
        Action {
            kind: Some(action::Kind::SetVolume(SetVolume {
                change: Some(set_volume::Change::Absolute(f64::NAN)),
            })),
        },
    ] {
        let document = omega_document::StateDocument {
            schedules: vec![Schedule {
                id: "refresh".into(),
                cadence: "every 1m".into(),
                action: Some(action),
            }],
            ..Default::default()
        };
        let error = DocumentValidation::validate(&document, []).unwrap_err();
        assert!(error.to_string().contains("refresh"));
    }
    let document = omega_document::StateDocument {
        schedules: vec![Schedule::announcing(
            "refresh",
            omega_proto::Cadence::minutes(1),
        )],
        ..Default::default()
    };
    DocumentValidation::validate(&document, []).unwrap();
}

#[test]
fn scheduled_commands_must_name_a_built_units_command_surface() {
    use omega_proto::omega::{Action, InvokeUnit, Schedule, action};
    let manifest = Fixture::manifest().exposing([
        Surface::new(&SurfaceId::parse("time").unwrap(), SurfaceKind::Widget),
        Surface::new(&SurfaceId::parse("refresh").unwrap(), SurfaceKind::Command),
    ]);
    for (unit, command, valid) in [
        ("clock", "refresh", true),
        ("missing", "refresh", false),
        ("clock", "time", false),
        ("clock", "absent", false),
    ] {
        let document = omega_document::StateDocument {
            schedules: vec![Schedule {
                id: "refresh".into(),
                cadence: "every 1m".into(),
                action: Some(Action {
                    kind: Some(action::Kind::InvokeUnit(InvokeUnit {
                        unit: unit.into(),
                        command: command.into(),
                        args: vec![],
                    })),
                }),
            }],
            ..Default::default()
        };
        assert_eq!(
            DocumentValidation::validate(&document, [&manifest]).is_ok(),
            valid,
            "{unit}.{command}"
        );
    }
}
