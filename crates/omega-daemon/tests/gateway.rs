//! What a shell may ask of the daemon, over the socket it already reads.

mod common;

use std::time::Duration;

use common::{TempSocket, command_manifest, unit_name, widget_manifest};
use omega_daemon::Shutdown;
use omega_daemon::hub::Hub;
use omega_daemon::manifest::ManifestStore;
use omega_daemon::shell::ShellServer;
use omega_daemon::supervisor::{Supervisor, UnitSpec};
use omega_daemon::units::UnitTable;
use omega_wire::omega::{
    Act, Action, ErrorCode, InvokeUnit, RestartUnit, action, frame, invoke, result,
};
use omega_wire::{Observation, Refusal, Socket};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// A daemon with an observation socket that serves requests, and the
/// supervisor behind it.
struct Shell {
    socket: Socket,
    supervisor: Supervisor,
    _path: TempSocket,
}

impl Shell {
    fn serving(tag: &str) -> Self {
        let path = TempSocket::new(tag);
        let socket = path.socket();
        let hub = Hub::new();
        let units = UnitTable::detached(hub.clone());
        units.adopt(&ManifestStore::from_manifests([
            widget_manifest("sleeper", "battery"),
            command_manifest("lamp", "toggle"),
        ]));
        let supervisor = Supervisor::new(socket.clone(), units.clone(), Shutdown::new());

        let server = ShellServer::bind_at(socket.clone(), hub)
            .unwrap()
            .serving(supervisor.clone(), units);
        tokio::spawn(async move {
            let _ = server.run().await;
        });

        Self {
            socket,
            supervisor,
            _path: path,
        }
    }

    async fn connect(&self) -> Observer {
        Observer {
            lines: BufReader::new(self.socket.connect_stream().await.unwrap()).lines(),
        }
    }
}

/// A shell, as far as the daemon can tell: it reads lines and writes lines.
struct Observer {
    lines: tokio::io::Lines<BufReader<UnixStream>>,
}

impl Observer {
    /// Ask for something, and take the answer — skipping the views and topics
    /// that arrive alongside it, which is what sharing one stream costs.
    async fn ask(&mut self, op: invoke::Op) -> result::Outcome {
        let request = Observation::request(1, op);
        let line = format!("{}\n", Observation::line(&request).unwrap());
        self.lines
            .get_mut()
            .get_mut()
            .write_all(line.as_bytes())
            .await
            .unwrap();

        loop {
            let line = tokio::time::timeout(Duration::from_secs(2), self.lines.next_line())
                .await
                .expect("the daemon answered nothing")
                .unwrap()
                .expect("the daemon closed the connection");

            let Some(answer) = Observation::answer(&line) else {
                continue;
            };
            assert_eq!(
                answer.stream_id, request.stream_id,
                "answered the wrong ask"
            );

            match answer.body {
                Some(frame::Body::Result(result)) => {
                    return result.outcome.expect("a Result carries an outcome");
                }
                other => panic!("expected a Result, got {other:?}"),
            }
        }
    }

    async fn refusal(&mut self, op: invoke::Op) -> Refusal {
        match self.ask(op).await {
            result::Outcome::Error(error) => Refusal {
                code: ErrorCode::try_from(error.code).unwrap_or(ErrorCode::Unspecified),
                message: error.message,
            },
            other => panic!("expected a refusal, got {other:?}"),
        }
    }
}

/// A unit that stays up until something stops it.
fn sleeper(tag: &str) -> std::path::PathBuf {
    let script = std::env::temp_dir().join(format!("omega-{tag}-{}.sh", std::process::id()));
    std::fs::write(&script, "#!/bin/sh\nsleep 30\n").unwrap();
    std::fs::set_permissions(
        &script,
        <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o755),
    )
    .unwrap();
    script
}

#[tokio::test]
async fn a_shell_asks_in_the_protocol_it_is_already_read_in() {
    let shell = Shell::serving("gateway-restart");
    let script = sleeper("gateway-sleeper");
    shell
        .supervisor
        .spawn(UnitSpec::new(unit_name("sleeper"), &script));

    let started = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::time::Instant::now() < started && shell.supervisor.running().is_empty() {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let mut observer = shell.connect().await;
    let outcome = observer
        .ask(invoke::Op::RestartUnit(RestartUnit {
            unit: "sleeper".into(),
        }))
        .await;

    // The same op, the same policy, the same answer — a JSON request is the
    // protocol in a second encoding, not a second protocol.
    assert!(matches!(outcome, result::Outcome::Ok(_)), "{outcome:?}");
    assert!(shell.supervisor.running().contains(&unit_name("sleeper")));

    let _ = std::fs::remove_file(&script);
}

/// What a button press is: the operator asking a unit to run one of its own
/// commands, which is exactly what `omega run` asks for.
fn press(unit: &str, command: &str) -> invoke::Op {
    invoke::Op::Act(Act {
        action: Some(Action {
            kind: Some(action::Kind::InvokeUnit(InvokeUnit {
                unit: unit.into(),
                command: command.into(),
                args: Vec::new(),
            })),
        }),
    })
}

#[tokio::test]
async fn pressing_a_button_reaches_the_plugin_that_drew_it() {
    let shell = Shell::serving("gateway-invoke");
    let mut observer = shell.connect().await;

    // The plugin declares the command but is not running, so the press is
    // refused for the reason it actually failed — a precondition, not a bad
    // request, which is the difference between "try again" and "stop".
    let refusal = observer.refusal(press("lamp", "toggle")).await;
    assert_eq!(refusal.code, ErrorCode::FailedPrecondition, "{refusal}");
    assert!(refusal.message.contains("lamp"), "{refusal}");

    // A button wired to a command nobody declared is answered too, rather
    // than the press vanishing into a daemon that had nowhere to send it.
    let refusal = observer.refusal(press("lamp", "explode")).await;
    assert_eq!(refusal.code, ErrorCode::InvalidArgument, "{refusal}");
    assert!(refusal.message.contains("explode"), "{refusal}");
}

#[tokio::test]
async fn a_request_the_policy_does_not_serve_an_operator_is_refused() {
    let shell = Shell::serving("gateway-denied");
    let mut observer = shell.connect().await;

    // Publishing a view is a unit's business. The observation socket does not
    // become a way to be one.
    let refusal = observer
        .refusal(invoke::Op::PublishView(omega_wire::omega::PublishView {
            surface_id: "battery".into(),
            module_id: String::new(),
            view: Some(Default::default()),
        }))
        .await;

    assert_eq!(refusal.code, ErrorCode::PermissionDenied);
}

#[tokio::test]
async fn a_line_that_is_not_a_request_is_answered_rather_than_dropped() {
    let shell = Shell::serving("gateway-garbage");
    let mut observer = shell.connect().await;

    observer
        .lines
        .get_mut()
        .get_mut()
        .write_all(b"{\"nonsense\": true}\n")
        .await
        .unwrap();

    let line = loop {
        let line = tokio::time::timeout(Duration::from_secs(2), observer.lines.next_line())
            .await
            .expect("the daemon answered nothing")
            .unwrap()
            .unwrap();
        if let Some(answer) = Observation::answer(&line) {
            break answer;
        }
    };

    let refusal = Refusal::of(&line).expect("a bad request is refused out loud");
    assert_eq!(refusal.code, ErrorCode::InvalidArgument);
}

#[tokio::test]
async fn a_server_that_only_streams_says_so() {
    let path = TempSocket::new("gateway-none");
    let socket = path.socket();
    let server = ShellServer::bind_at(socket.clone(), Hub::new()).unwrap();
    tokio::spawn(async move {
        let _ = server.run().await;
    });

    let mut observer = Observer {
        lines: BufReader::new(socket.connect_stream().await.unwrap()).lines(),
    };

    // A server nobody wired a supervisor into refuses out loud rather than
    // accepting a request it has no way to serve.
    let refusal = observer
        .refusal(invoke::Op::RestartUnit(RestartUnit {
            unit: "sleeper".into(),
        }))
        .await;
    assert_eq!(refusal.code, ErrorCode::Unimplemented);
}
