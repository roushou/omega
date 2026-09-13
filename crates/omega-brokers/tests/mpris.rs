//! Deciding which player a media key reaches, and what a bar draws.

mod common;

use omega_brokers::Mpris;
use omega_brokers::mpris::{Player, Players};
use omega_proto::omega::{MediaKey, Playback, media_key, state_topic};

fn player(id: &str, status: &str) -> Player {
    Player {
        id: omega_proto::PlayerId::parse(id).unwrap(),
        identity: id.into(),
        status: status.into(),
        title: "Track".into(),
        artists: vec!["Someone".into()],
        album: "Album".into(),
        length_us: 210_000_000,
        can_control: true,
        can_play: true,
        can_pause: true,
        can_go_next: true,
        can_go_previous: true,
    }
}

#[test]
fn whatever_is_playing_is_the_one_a_key_reaches() {
    // A browser tab and a music app at once is the normal case. The key goes
    // to the one making noise, not to whichever the bus listed first.
    let players = vec![player("chromium", "Paused"), player("spotify", "Playing")];
    let state = Players::state(&players);

    assert_eq!(state.players[0].id, "spotify");
    assert!(state.players[0].active);
    assert!(!state.players[1].active);
    assert_eq!(
        Players::active(&players).unwrap(),
        "org.mpris.MediaPlayer2.spotify"
    );
}

#[test]
fn with_nothing_playing_the_first_by_name_is_reached() {
    // Still a decision, and a stable one: the bus answers in whatever order
    // it holds names, so a bar would otherwise swap between two paused
    // players every refresh.
    let players = vec![player("spotify", "Paused"), player("chromium", "Paused")];
    let state = Players::state(&players);

    assert_eq!(state.players[0].id, "chromium");
    assert!(state.players[0].active);
}

#[test]
fn no_players_is_a_reading_and_reaches_nothing() {
    // A desktop with nothing open is an answer. A key with nowhere to go is
    // refused rather than sent to a player that is not there.
    assert!(Players::state(&[]).players.is_empty());
    assert!(Players::active(&[]).is_none());
}

#[test]
fn several_artists_are_one_line() {
    // `xesam:artist` is a list. Joining here is what stops every widget
    // picking its own separator.
    let duet = Player {
        artists: vec!["One".into(), "Two".into()],
        ..player("spotify", "Playing")
    };
    assert_eq!(Players::state(&[duet]).players[0].artist, "One, Two");
}

#[test]
fn a_status_becomes_a_playback() {
    let of = |status| Players::state(&[player("p", status)]).players[0].playback;

    assert_eq!(of("Playing"), Playback::Playing as i32);
    assert_eq!(of("Paused"), Playback::Paused as i32);
    assert_eq!(of("Stopped"), Playback::Stopped as i32);
    // A player saying something this build has no arm for is unspecified
    // rather than guessed at as playing.
    assert_eq!(of("Buffering"), Playback::Unspecified as i32);
}

#[test]
fn a_length_a_player_does_not_know_is_not_a_negative_track() {
    // `mpris:length` is signed and occasionally negative before a player has
    // worked it out. The ontology's is unsigned, and casting would report a
    // track lasting six hundred thousand years.
    let unknown = Player {
        length_us: -1,
        ..player("spotify", "Playing")
    };
    assert_eq!(Players::state(&[unknown]).players[0].length_us, 0);
}

#[test]
fn every_media_key_has_a_method() {
    let method = |key| {
        Players::method(&MediaKey {
            key: key as i32,
            player_id: None,
        })
    };

    assert_eq!(method(media_key::Key::MediaPlayPause), Some("PlayPause"));
    assert_eq!(method(media_key::Key::MediaNext), Some("Next"));
    assert_eq!(method(media_key::Key::MediaPrevious), Some("Previous"));
    assert_eq!(method(media_key::Key::MediaStop), Some("Stop"));
    // A key nobody named is refused rather than turned into play.
    assert_eq!(method(media_key::Key::MediaKeyUnspecified), None);
}

// ---- against the machine this is running on ----

#[tokio::test]
#[ignore = "needs a session bus; run with --ignored"]
async fn it_reads_the_desktop_it_is_running_on() {
    let mut mpris = Mpris::new();

    let patch = common::first(&mut mpris).await.expect("the bus answered");
    assert_eq!(patch.topics[0].topic, "media");

    let Some(state_topic::Value::Media(media)) = patch.topics[0].value.as_ref() else {
        panic!("expected a media reading");
    };
    // A desktop with no player open is a reading of nothing, so the only
    // claim that always holds is that at most one player is the active one.
    assert!(media.players.iter().filter(|p| p.active).count() <= 1);

    // The second reading waits: for a player to say something, or for the
    // refresh that notices one starting.
    assert!(
        common::waits(&mut mpris).await,
        "a second reading should wait"
    );
}

#[test]
fn explicit_targets_never_fall_back_and_unsupported_operations_fail() {
    let players = vec![player("chromium", "Paused"), player("spotify", "Playing")];
    let mut key = MediaKey {
        key: media_key::Key::MediaPlayPause as i32,
        player_id: Some("chromium".into()),
    };
    assert_eq!(
        Players::target(&players, &key).unwrap().id.as_str(),
        "chromium"
    );
    key.player_id = Some("gone".into());
    assert!(Players::target(&players, &key).is_err());
    key.player_id = Some(String::new());
    assert!(Players::target(&players, &key).is_err());
    key.player_id = None;
    assert_eq!(
        Players::target(&players, &key).unwrap().id.as_str(),
        "spotify"
    );
    let players = vec![Player {
        can_pause: false,
        ..player("spotify", "Playing")
    }];
    assert!(matches!(
        Players::target(&players, &key),
        Err(omega_brokers::BrokerError::Unsupported(_))
    ));
    key.key = media_key::Key::MediaPlay as i32;
    assert!(Players::target(&players, &key).is_ok());
}

mod isolated {
    use super::*;
    use omega_brokers::Broker;
    use omega_proto::omega::action;
    use std::{
        collections::HashMap,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };
    use zbus::zvariant::OwnedValue;

    struct Root;
    #[zbus::interface(name = "org.mpris.MediaPlayer2")]
    impl Root {
        #[zbus(property)]
        fn identity(&self) -> &str {
            "Omega test player"
        }
    }
    struct Transport {
        calls: Arc<AtomicUsize>,
    }
    #[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
    impl Transport {
        fn play_pause(&self) {
            self.calls.fetch_add(1, Ordering::SeqCst);
        }
        fn next(&self) -> zbus::fdo::Result<()> {
            Err(zbus::fdo::Error::Failed("track unavailable".into()))
        }
        #[zbus(property)]
        fn playback_status(&self) -> &str {
            "Playing"
        }
        #[zbus(property)]
        fn metadata(&self) -> HashMap<String, OwnedValue> {
            HashMap::new()
        }
        #[zbus(property)]
        fn can_control(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn can_play(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn can_pause(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn can_go_next(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn can_go_previous(&self) -> bool {
            false
        }
    }

    #[tokio::test]
    #[ignore = "run under dbus-run-session to isolate desktop players"]
    async fn selected_player_routes_and_reports_refusal_and_disappearance() {
        let first = Arc::new(AtomicUsize::new(0));
        let second = Arc::new(AtomicUsize::new(0));
        let first_bus = zbus::connection::Builder::session()
            .unwrap()
            .name("org.mpris.MediaPlayer2.omega_first")
            .unwrap()
            .serve_at("/org/mpris/MediaPlayer2", Root)
            .unwrap()
            .serve_at(
                "/org/mpris/MediaPlayer2",
                Transport {
                    calls: first.clone(),
                },
            )
            .unwrap()
            .build()
            .await
            .unwrap();
        let _second_bus = zbus::connection::Builder::session()
            .unwrap()
            .name("org.mpris.MediaPlayer2.omega_second")
            .unwrap()
            .serve_at("/org/mpris/MediaPlayer2", Root)
            .unwrap()
            .serve_at(
                "/org/mpris/MediaPlayer2",
                Transport {
                    calls: second.clone(),
                },
            )
            .unwrap()
            .build()
            .await
            .unwrap();
        let mut broker = Mpris::new();
        broker.connect().await.unwrap();
        let patch = broker.read().await.unwrap();
        let Some(state_topic::Value::Media(media)) = &patch.topics[0].value else {
            panic!("expected media")
        };
        assert_eq!(media.players.len(), 2);
        assert!(media.players[0].can_pause);
        assert!(!media.players[0].can_go_previous);
        let mut key = MediaKey {
            key: media_key::Key::MediaPlayPause as i32,
            player_id: Some("omega_second".into()),
        };
        broker
            .act(&action::Kind::MediaKey(key.clone()))
            .await
            .unwrap();
        assert_eq!(first.load(Ordering::SeqCst), 0);
        assert_eq!(second.load(Ordering::SeqCst), 1);
        key.player_id = Some("omega_first".into());
        key.key = media_key::Key::MediaNext as i32;
        assert!(
            broker
                .act(&action::Kind::MediaKey(key.clone()))
                .await
                .unwrap_err()
                .to_string()
                .contains("track unavailable")
        );
        key.key = media_key::Key::MediaPrevious as i32;
        assert!(matches!(
            broker.act(&action::Kind::MediaKey(key.clone())).await,
            Err(omega_brokers::BrokerError::Unsupported(_))
        ));
        first_bus
            .release_name("org.mpris.MediaPlayer2.omega_first")
            .await
            .unwrap();
        key.key = media_key::Key::MediaPlayPause as i32;
        assert!(broker.act(&action::Kind::MediaKey(key)).await.is_err());
        assert_eq!(first.load(Ordering::SeqCst), 0);
        assert_eq!(second.load(Ordering::SeqCst), 1);
    }
}
