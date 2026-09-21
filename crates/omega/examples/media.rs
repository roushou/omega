//! MPRIS playback controls. Selection survives plugin restarts while Omega runs.
use omega::platform::audio::{Media, MediaControl, Playback, Player, PlayerId};
use omega::record::{Own, Watch};
use omega::ui::{Button, Column, Glyph, Icon, Row, Section, Text};
use omega::{Command, Plugin, Surface, Ui};

pub const PLUGIN: &str = env!("CARGO_PKG_NAME");

#[derive(omega::PluginState, Default, Clone, Debug)]
pub struct Selection {
    pub player: Option<PlayerId>,
}

impl Selection {
    fn resolve<'a>(&self, players: &'a [Player]) -> Option<&'a Player> {
        players
            .iter()
            .find(|p| Some(p.id()) == self.player.as_ref())
            .or_else(|| players.iter().find(|p| p.is_active()))
            .or_else(|| players.iter().find(|p| p.is_playing()))
            .or_else(|| players.first())
    }

    fn title(player: &Player) -> &str {
        if player.title().is_empty() {
            player.identity()
        } else {
            player.title()
        }
    }

    fn status(player: &Player) -> &'static str {
        match player.playback() {
            Playback::Playing => "Playing",
            Playback::Paused => "Paused",
            Playback::Stopped => "Stopped",
            Playback::Unspecified => "Playback unavailable",
        }
    }

    fn compact(title: &str) -> String {
        let mut chars = title.chars();
        let mut label: String = chars.by_ref().take(28).collect();
        if chars.next().is_some() {
            label.push('…');
        }
        label
    }
}

#[derive(omega::Surface, Debug)]
pub struct Indicator {
    media: Media,
    selection: Watch<Selection>,
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
    fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> Ui {
        if !self.media.has_reading() {
            return Text::new("Media unavailable").muted().into();
        }
        let players = self.media.players();
        let Some(player) = self.selection.get().resolve(&players) else {
            return Text::new("No media").muted().into();
        };
        let glyph = match player.playback() {
            Playback::Playing => Glyph::Play,
            Playback::Paused => Glyph::Pause,
            Playback::Stopped => Glyph::Stop,
            Playback::Unspecified => Glyph::Music,
        };
        Row::new()
            .gap(6)
            .child(Icon::new(glyph))
            .child(Text::new(Selection::compact(Selection::title(player))))
            .tooltip(format!(
                "{} — {}\n{}",
                Selection::title(player),
                player.artist(),
                player.identity()
            ))
            .into()
    }
}

#[derive(omega::Surface, Debug)]
pub struct Panel {
    media: Media,
    selection: Watch<Selection>,
}

impl Surface for Panel {
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
    fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> Ui {
        if !self.media.has_reading() {
            return Section::new()
                .title("Media")
                .child(Text::new("Media service unavailable"))
                .into();
        }
        let players = self.media.players();
        let selected = self.selection.get();
        let Some(player) = selected.resolve(&players) else {
            return Section::new()
                .title("Media")
                .child(Text::new("No media players"))
                .child(Text::new("Start playback in a music app or browser.").muted())
                .into();
        };
        let id = player.id().to_owned();
        let mut panel = Section::new()
            .title("Media")
            .child(Text::new(Selection::title(player)).bold())
            .child(Text::new(player.artist()).muted())
            .child(Text::new(player.album()).muted())
            .child(
                Text::new(format!(
                    "{} · {}",
                    player.identity(),
                    Selection::status(player)
                ))
                .muted(),
            )
            .child(
                Row::new()
                    .gap(8)
                    .child(
                        Button::new("Previous")
                            .icon(Glyph::Previous)
                            .disabled_if(!player.can_go_previous())
                            .secondary()
                            .fill_width()
                            .key("previous")
                            .on_press(Previous.with(id.clone())),
                    )
                    .child(
                        Button::new(if player.is_playing() { "Pause" } else { "Play" })
                            .icon(if player.is_playing() {
                                Glyph::Pause
                            } else {
                                Glyph::Play
                            })
                            .disabled_if(if player.is_playing() {
                                !player.can_pause()
                            } else {
                                !player.can_play()
                            })
                            .fill_width()
                            .key("play")
                            .on_press(PlayPause.with(id.clone())),
                    )
                    .child(
                        Button::new("Next")
                            .icon(Glyph::Next)
                            .disabled_if(!player.can_go_next())
                            .secondary()
                            .fill_width()
                            .key("next")
                            .on_press(Next.with(id)),
                    ),
            );
        if players.len() > 1 || selected.player.is_some() {
            let mut choices = Column::new().gap(6).child(
                Button::new(if selected.player.is_none() {
                    "Automatic ✓"
                } else {
                    "Automatic"
                })
                .secondary()
                .fill_width()
                .key("automatic")
                .on_press(SelectPlayer.with(None)),
            );
            for option in &players {
                choices = choices.child(
                    Button::new(format!(
                        "{}{}",
                        option.identity(),
                        if option.id() == player.id() {
                            " ✓"
                        } else {
                            ""
                        }
                    ))
                    .secondary()
                    .fill_width()
                    .key(option.id().as_str())
                    .on_press(SelectPlayer.with(Some(option.id().clone()))),
                );
            }
            panel = panel.child(Section::new().title("Player").child(choices));
        }
        panel.into()
    }
}

#[derive(omega::Command, Debug)]
pub struct SelectPlayer {
    media: Media,
    selection: Own<Selection>,
}

impl Command for SelectPlayer {
    const ID: &'static str = "select-player";

    type Input = Option<PlayerId>;
    type Output = ();
    async fn call(&self, player: Option<PlayerId>) -> omega::Result<()> {
        if let Some(id) = &player
            && !self.media.players().iter().any(|p| p.id() == id)
        {
            return Err(omega::Error::invalid("That player is no longer available"));
        }
        self.selection.set(&Selection { player }).await
    }
}

#[derive(omega::Command, Debug)]
pub struct PlayPause {
    media: MediaControl,
}

impl Command for PlayPause {
    const ID: &'static str = "play-pause";

    type Input = PlayerId;
    type Output = ();
    async fn call(&self, id: PlayerId) -> omega::Result<()> {
        self.media.player(&id).play_pause().await
    }
}

#[derive(omega::Command, Debug)]
pub struct Previous {
    media: MediaControl,
}

impl Command for Previous {
    const ID: &'static str = "previous";

    type Input = PlayerId;
    type Output = ();
    async fn call(&self, id: PlayerId) -> omega::Result<()> {
        self.media.player(&id).previous().await
    }
}

#[derive(omega::Command, Debug)]
pub struct Next {
    media: MediaControl,
}

impl Command for Next {
    const ID: &'static str = "next";

    type Input = PlayerId;
    type Output = ();
    async fn call(&self, id: PlayerId) -> omega::Result<()> {
        self.media.player(&id).next().await
    }
}

fn main() -> omega::Result<()> {
    Plugin::new(PLUGIN, env!("CARGO_PKG_VERSION"))
        .surface_as::<Indicator>("indicator")
        .surface_as::<Panel>("panel")
        .command::<SelectPlayer>()
        .command::<PlayPause>()
        .command::<Previous>()
        .command::<Next>()
        .run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use omega::testing::{Called, Drawn, State, SystemTopic, topic::MediaState};

    struct Fixture;
    impl Fixture {
        fn state() -> State {
            let mut media = MediaState::default();
            for (id, playing) in [("first", false), ("second", true)] {
                media.players.push(Default::default());
                let p = media.players.last_mut().unwrap();
                p.id = id.into();
                p.identity = id.into();
                p.title = format!("{id} track");
                p.playback = if playing {
                    Playback::Playing
                } else {
                    Playback::Paused
                } as i32;
                p.active = playing;
            }
            State::new().with(media)
        }
    }

    #[test]
    fn distinguishes_unavailable_and_empty() {
        assert!(
            Drawn::of::<Panel>(&State::new().absent(SystemTopic::Media))
                .unwrap()
                .text()
                .contains("unavailable")
        );
        assert!(
            Drawn::of::<Panel>(&State::new().with(MediaState::default()))
                .unwrap()
                .text()
                .contains("No media players")
        );
    }

    #[test]
    fn automatic_prefers_playing_and_bar_title_is_bounded() {
        assert!(
            Drawn::of::<Indicator>(&Fixture::state())
                .unwrap()
                .text()
                .contains("second track")
        );
        assert_eq!(Selection::compact(&"界".repeat(50)).chars().count(), 29);
    }

    #[tokio::test]
    async fn explicit_selection_is_validated() {
        let state = Fixture::state();
        assert!(
            Called::of::<SelectPlayer>(&state, Some("gone".parse::<PlayerId>().unwrap()))
                .await
                .answer
                .is_err()
        );
        assert!(
            Called::of::<SelectPlayer>(&state, Some("first".parse::<PlayerId>().unwrap()))
                .await
                .answer
                .is_ok()
        );
        assert!(
            Called::of::<SelectPlayer>(&state, None)
                .await
                .answer
                .is_ok()
        );
    }

    #[tokio::test]
    async fn controls_submit_effects_even_when_the_local_reading_is_stale() {
        let called =
            Called::of::<PlayPause>(&Fixture::state(), "gone".parse::<PlayerId>().unwrap()).await;
        assert!(called.answer.is_ok());
        assert_eq!(called.effects.len(), 1);
    }

    #[test]
    fn explicit_selection_wins_and_missing_selection_falls_back() {
        use omega::config::Fields;
        use omega::record::PluginState;
        for (selected, expected) in [("first", "first"), ("gone", "second")] {
            let state = Fixture::state().keyspace(
                &Selection::address(),
                Selection {
                    player: Some(PlayerId::try_from(selected).unwrap()),
                }
                .write(),
            );
            assert!(
                Drawn::of::<Indicator>(&state)
                    .unwrap()
                    .text()
                    .contains(expected)
            );
        }
    }
}
