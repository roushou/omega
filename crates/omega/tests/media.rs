use omega::platform::audio::{Media, MediaControl, PlayerId};
use omega::record::PluginState;
use omega::testing::{Called, State};
use omega::{Command, Input};
use omega_proto::{
    Fields, Values,
    omega::{MediaKey, MediaState, Playback, PlayerInfo, action, media_key},
};

#[derive(omega::Command)]
struct Pause {
    media: MediaControl,
}
impl Command for Pause {
    type Input = PlayerId;
    type Output = ();
    async fn call(&self, id: PlayerId) -> omega::Result<()> {
        self.media.player(&id).pause().await
    }
}

#[derive(omega::Command)]
struct Toggle {
    media: MediaControl,
}
impl Command for Toggle {
    type Input = ();
    type Output = ();
    async fn call(&self, _: ()) -> omega::Result<()> {
        self.media.active().play_pause().await
    }
}

#[derive(omega::PluginState, Default, Clone)]
struct Selection {
    player: Option<PlayerId>,
}

#[tokio::test]
async fn a_player_binding_carries_the_explicit_target_without_a_local_reading() {
    let id = "vlc.instance123".parse::<PlayerId>().unwrap();
    let called = Called::of::<Pause>(&State::new(), id.clone()).await;
    assert!(called.answer.is_ok());
    assert!(called.did(&action::Kind::MediaKey(MediaKey {
        key: media_key::Key::MediaPause as i32,
        player_id: Some(id.to_string())
    })));
    let called = Called::of::<Toggle>(&State::new(), ()).await;
    assert!(called.answer.is_ok());
    assert!(called.did(&action::Kind::MediaKey(MediaKey {
        key: media_key::Key::MediaPlayPause as i32,
        player_id: None
    })));
}

#[tokio::test]
async fn malformed_player_input_is_refused_before_an_effect_is_submitted() {
    use omega_proto::IntoValue;
    let called =
        Called::raw::<Pause>(&State::new(), vec!["/org/mpris".to_string().into_value()]).await;
    assert!(called.answer.is_err());
    assert!(called.effects.is_empty());
}

#[test]
fn player_identity_round_trips_through_bindings_and_records() {
    let id = "chromium.instance123".parse::<PlayerId>().unwrap();
    assert_eq!(
        PlayerId::decode(omega::Args::new(id.clone().encode())).unwrap(),
        id
    );
    let selected = Selection {
        player: Some(id.clone()),
    };
    assert_eq!(Selection::read(&selected.write()).player, Some(id));
    assert_eq!(Selection::read(&Selection::default().write()).player, None);
    assert_eq!(Selection::read(&Values::new()).player, None);
    assert!(Selection::address().contains("selection"));
}

#[derive(omega::Command)]
struct Inspect {
    media: Media,
}
impl Command for Inspect {
    type Input = ();
    type Output = ();
    async fn call(&self, _: ()) -> omega::Result<()> {
        let player = self.media.active().unwrap();
        assert_eq!(player.id().as_str(), "vlc");
        assert!(player.is_playing());
        assert!(!player.can_control());
        assert!(!player.can_play());
        assert!(!player.can_pause());
        assert!(!player.can_go_next());
        assert!(!player.can_go_previous());
        Ok(())
    }
}

#[tokio::test]
async fn player_abilities_require_control_support() {
    let state = State::new().with(MediaState {
        players: vec![PlayerInfo {
            id: "vlc".into(),
            active: true,
            playback: Playback::Playing as i32,
            can_control: false,
            can_play: true,
            can_pause: true,
            can_go_next: true,
            can_go_previous: true,
            ..Default::default()
        }],
    });
    assert!(Called::of::<Inspect>(&state, ()).await.answer.is_ok());
}

#[test]
fn playback_commands_declare_media_permission_without_a_read_subscription() {
    let manifest = omega::testing::manifest_of(
        &omega::Plugin::named(env!("CARGO_PKG_NAME"), "0.1.0").command::<Pause>(),
    );
    assert_eq!(
        manifest.granted().unwrap(),
        vec![omega_proto::omega::Capability::Media]
    );
    assert!(manifest.state_topics.is_empty());
}
