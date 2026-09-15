use super::*;
use std::time::Duration;
use tokio::net::UnixListener;

struct Fixture {
    dir: PathBuf,
    listener: UnixListener,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("omega-hypr-{}-{serial}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let listener = UnixListener::bind(dir.join(".socket.sock")).unwrap();
        Self { dir, listener }
    }

    fn broker(&self) -> (Hyprland, UnixStream) {
        let (source, events) = UnixStream::pair().unwrap();
        (
            Hyprland {
                link: Some(Link {
                    dir: self.dir.clone(),
                    events: BufReader::new(events).lines(),
                }),
                affected: Hyprland::EVERYTHING,
            },
            source,
        )
    }

    async fn reply(&self, expected: &str, response: &str) {
        let (mut socket, _) = self.listener.accept().await.unwrap();
        let mut request = String::new();
        socket.read_to_string(&mut request).await.unwrap();
        assert_eq!(request, expected);
        socket.write_all(response.as_bytes()).await.unwrap();
        socket.shutdown().await.unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[tokio::test]
async fn monitor_focus_events_refresh_workspace_and_window_readings() {
    for event in ["focusedmon", "focusedmonv2"] {
        let fixture = Fixture::new();
        let (mut broker, mut source) = fixture.broker();
        source
            .write_all(format!("{event}>>DP-1,2\n").as_bytes())
            .await
            .unwrap();
        broker.wake().await.unwrap();
        let server = async {
            fixture.reply("j/monitors", "[]").await;
            fixture
                .reply("j/activeworkspace", r#"{"id":2,"name":"new name"}"#)
                .await;
            fixture
                .reply(
                    "j/workspaces",
                    r#"[
                {"id":1,"name":"1","monitor":"eDP-1","windows":1},
                {"id":2,"name":"old name","monitor":"DP-1","windows":0}
            ]"#,
                )
                .await;
            fixture.reply("j/activewindow", "{}").await;
        };
        let (patch, ()) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(broker.read(), server)
        })
        .await
        .expect("focus refresh must complete every expected query");
        let patch = patch.unwrap();
        assert_eq!(patch.topics.len(), 3);
        let Some(state_topic::Value::Workspaces(state)) = &patch.topics[1].value else {
            panic!("workspace reading missing")
        };
        assert!(!state.workspaces[0].active);
        assert!(state.workspaces[1].active);
        assert_eq!(state.workspaces[1].monitor_id, "DP-1");
        let Some(state_topic::Value::Window(window)) = &patch.topics[2].value else {
            panic!("window reading missing")
        };
        assert!(window.focused.is_none());
    }
}

#[test]
fn lifecycle_and_occupancy_events_invalidate_their_readings() {
    for event in [
        "workspace",
        "workspacev2",
        "openwindow",
        "closewindow",
        "movewindowv2",
    ] {
        assert_eq!(
            Link::affected(event),
            &[SystemTopic::Workspaces, SystemTopic::Window]
        );
    }
    for event in ["monitoraddedv2", "monitorremovedv2", "configreloaded"] {
        assert_eq!(Link::affected(event), Hyprland::EVERYTHING);
    }
    for event in [
        "createworkspacev2",
        "destroyworkspacev2",
        "renameworkspace",
        "moveworkspacev2",
    ] {
        assert_eq!(Link::affected(event), &[SystemTopic::Workspaces]);
    }
}

#[tokio::test]
async fn dispatch_selects_the_provider_before_sending_one_action_and_propagates_refusal() {
    use omega_proto::omega::{SwitchWorkspace, switch_workspace};
    for (status, dispatch, answer) in [
        (
            r#"{"configProvider":"lua"}"#,
            "dispatch hl.dsp.focus({ workspace = \"3\" })",
            "ok",
        ),
        ("unknown request", "dispatch workspace 3", "ok"),
        (
            r#"{"configProvider":"lua"}"#,
            "dispatch hl.dsp.focus({ workspace = \"3\" })",
            "workspace refused",
        ),
    ] {
        let fixture = Fixture::new();
        let (mut broker, _source) = fixture.broker();
        let action = action::Kind::SwitchWorkspace(SwitchWorkspace {
            target: Some(switch_workspace::Target::Index(3)),
        });
        let server = async {
            fixture.reply("j/status", status).await;
            fixture.reply(dispatch, answer).await;
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(5), async {
            tokio::join!(broker.act(&action), server)
        })
        .await
        .expect("an action must select a provider and dispatch once");
        assert_eq!(result.is_ok(), answer == "ok");
        if let Err(error) = result {
            assert!(error.to_string().contains(answer));
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(20), fixture.listener.accept())
                .await
                .is_err()
        );
    }
}

#[test]
fn malformed_active_workspace_replies_are_not_silently_empty() {
    assert_eq!(Session::active_id("{}").unwrap(), None);
    assert_eq!(Session::active_id(r#"{"id":-99}"#).unwrap(), Some(-99));
    for reply in [
        "bad json",
        "null",
        r#"{"id":"broken"}"#,
        r#"{"name":"missing identity"}"#,
    ] {
        assert!(Session::active_id(reply).is_err());
    }
}

#[tokio::test(start_paused = true)]
async fn event_wait_is_idle_until_a_complete_relevant_event_and_survives_cancellation() {
    let (mut source, receiver) = UnixStream::pair().unwrap();
    let mut link = Link {
        dir: PathBuf::new(),
        events: BufReader::new(receiver).lines(),
    };
    let pause = Duration::from_millis(500);
    assert!(tokio::time::timeout(pause, link.wait()).await.is_err());
    source
        .write_all(b"unrelated>>ignored\nactivewin")
        .await
        .unwrap();
    assert!(tokio::time::timeout(pause, link.wait()).await.is_err());
    source.write_all(b"dow>>terminal,title\n").await.unwrap();
    assert_eq!(link.wait().await.unwrap(), &[SystemTopic::Window]);
    assert!(tokio::time::timeout(pause, link.wait()).await.is_err());
}
