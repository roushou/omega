use omega_document::{Bars, Document, DocumentValidation, Modules, Plugins};
use omega_proto::omega::SurfaceKind;
use omega_proto::{Manifest, PluginName, Surface, SurfaceId};

struct Fixture;
impl Fixture {
    fn manifest() -> Manifest {
        Manifest::new(&"clock".parse::<PluginName>().unwrap(), "1").exposing([
            Surface::new(&"time".parse::<SurfaceId>().unwrap(), SurfaceKind::Widget),
            Surface::new(
                &"calendar".parse::<SurfaceId>().unwrap(),
                SurfaceKind::Widget,
            ),
        ])
    }
}

#[test]
fn plugin_references_and_duplicate_declarations_are_validated() {
    let manifest = Fixture::manifest();
    let unknown = Document::new()
        .plugin(Plugins::enabled("missing"))
        .into_inner();
    assert!(DocumentValidation::validate(&unknown, [&manifest]).is_err());
    let duplicate = Document::new()
        .plugin(Plugins::enabled("clock"))
        .plugin(Plugins::disabled("clock"))
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
fn bar_placement_ids_obey_the_embedded_presentation_limit() {
    let manifest = Fixture::manifest();
    for length in [128, 129] {
        let id = "a".repeat(length);
        let document = Document::new()
            .bar(Bars::top(
                "main",
                vec![Modules::panel(
                    Modules::surface(Modules::plain_widget(id.clone(), "clock"), "time"),
                    "calendar",
                )],
            ))
            .into_inner();
        let embedded =
            omega_proto::instance::PresentationSpec::try_from(omega_proto::omega::Presentation {
                kind: Some(omega_proto::omega::presentation::Kind::Embedded(
                    omega_proto::omega::EmbeddedPresentation {
                        placement: id.clone(),
                    },
                )),
            });
        let validation = DocumentValidation::validate(&document, [&manifest]);
        assert_eq!(validation.is_ok(), embedded.is_ok(), "length {length}");
        if length == 129 {
            assert!(
                validation
                    .unwrap_err()
                    .to_string()
                    .contains("placement id exceeds 128 bytes")
            );
        }
        assert!(omega_proto::ModuleId::try_from(id).is_ok());
    }
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
fn scheduled_commands_must_name_a_built_plugins_command_surface() {
    use omega_proto::omega::{Action, InvokePlugin, Schedule, action};
    let manifest = Fixture::manifest()
        .exposing([Surface::new(
            &"time".parse::<SurfaceId>().unwrap(),
            SurfaceKind::Widget,
        )])
        .serving([omega_proto::omega::CommandEndpoint {
            input: Some(Default::default()),
            output: Some(Default::default()),
            description: String::new(),
            id: "refresh".into(),
        }]);
    for (plugin, command, valid) in [
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
                    kind: Some(action::Kind::InvokePlugin(InvokePlugin {
                        signature: Vec::new(),
                        plugin: plugin.into(),
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
            "{plugin}.{command}"
        );
    }
}

#[test]
fn a_service_surface_is_not_placeable() {
    let manifest = Manifest::new(&"clock".parse::<PluginName>().unwrap(), "1").exposing([
        Surface::new(&"time".parse::<SurfaceId>().unwrap(), SurfaceKind::Widget),
        Surface::new(
            &"worker".parse::<SurfaceId>().unwrap(),
            SurfaceKind::Service,
        ),
    ]);
    // A service resolves to nothing placeable: naming it is a missing widget.
    assert!(matches!(
        DocumentValidation::surface(&manifest, "worker"),
        Err(omega_document::ValidationError::MissingSurface { .. })
    ));
    // The single placeable surface is the widget, not the service.
    assert_eq!(
        DocumentValidation::surface(&manifest, "").unwrap().as_str(),
        "time"
    );
}
