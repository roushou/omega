use crate::{Cases, Error, scene::Scene};
use omega::testing::{CapturedEffect, Drawn};
use omega_proto::{
    ActionKind,
    omega::{PreviewEffect, PreviewRequest, PreviewSnapshot, invoke, preview_request::Command},
    preview::{CaseId, EffectId, Reader, VERSION, Writer},
};
use std::{
    collections::{BTreeMap, VecDeque},
    ffi::OsString,
    time::Duration,
};

pub(crate) struct Session {
    cases: Cases,
    selected: CaseId,
    scene: Box<dyn Scene>,
    drawn: Drawn,
    revision: u64,
    epoch: u64,
    presentation: omega_proto::omega::PresentationState,
    next_effect: u64,
    effects: BTreeMap<EffectId, CapturedEffect>,
    events: VecDeque<String>,
}
impl Session {
    fn new(cases: Cases, generation: u32) -> Result<Self, Error> {
        let selected = cases
            .factories
            .keys()
            .next()
            .ok_or_else(|| Error::Invalid("no preview cases".into()))?
            .clone();
        let mut scene = cases.factories[&selected]()?;
        let drawn = scene.draw();
        Ok(Self {
            cases,
            selected,
            scene,
            drawn,
            revision: (generation as u64) << 32,
            epoch: (generation as u64) << 32,
            presentation: omega_proto::omega::PresentationState::Visible,
            next_effect: (generation as u64) << 32,
            effects: BTreeMap::new(),
            events: VecDeque::new(),
        })
    }
    pub(crate) async fn run(cases: Cases, socket: OsString) -> Result<(), Error> {
        let generation = std::env::var("OMEGA_PREVIEW_GENERATION")
            .unwrap_or_else(|_| "1".into())
            .parse::<u32>()
            .map_err(|e| Error::Invalid(e.to_string()))?;
        let mut session = Self::new(cases, generation)?;
        let stream = tokio::net::UnixStream::connect(socket).await?;
        let (read, write) = stream.into_split();
        let mut reader = Reader::new(read);
        let mut writer = Writer::new(write);
        writer.send(&session.snapshot(0, String::new())).await?;
        let mut tick = tokio::time::interval(Duration::from_millis(16));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            let mut answered = 0;
            let mut error = String::new();
            let mut changed = false;
            tokio::select! {
                request = reader.receive::<PreviewRequest>() => {
                    let Some(request) = request? else { break; };
                    answered = request.id;
                    if let Err(e) = session.request(request) { error = e.to_string(); }
                    changed = true;
                }
                result = std::future::poll_fn(|cx| session.scene.poll(cx)) => {
                    if let Err(e) = result { error = e.to_string(); }
                    session.redraw();
                    changed = true;
                }
                _ = tick.tick() => {}
            }
            while let Some(effect) = session.scene.effect() {
                session.capture(effect)?;
                changed = true;
            }
            if changed {
                writer.send(&session.snapshot(answered, error)).await?;
            }
        }
        Ok(())
    }
    fn capture(&mut self, effect: CapturedEffect) -> Result<EffectId, Error> {
        self.next_effect = self
            .next_effect
            .checked_add(1)
            .ok_or_else(|| Error::Invalid("preview effect identities exhausted".into()))?;
        let id = EffectId::parse(self.next_effect)?;
        self.effects.insert(id, effect);
        Ok(id)
    }
    fn redraw(&mut self) {
        self.drawn = self.scene.draw();
        self.revision += 1;
    }
    fn request(&mut self, request: PreviewRequest) -> Result<(), Error> {
        match request
            .command
            .ok_or_else(|| Error::Invalid("missing preview command".into()))?
        {
            Command::Select(name) => {
                self.reset(CaseId::parse(name).map_err(|e| Error::Invalid(e.to_string()))?)?
            }
            Command::Reset(_) => self.reset(self.selected.clone())?,
            Command::Interact(event) => {
                if self.presentation != omega_proto::omega::PresentationState::Visible {
                    return Err(Error::Invalid(
                        "presentation is not visible; reset to reopen".into(),
                    ));
                }
                if event.revision != self.revision {
                    return Err(Error::Invalid("obsolete preview render".into()));
                }
                // Only event kinds are retained: form values and arguments may contain secrets.
                if event.event.len() > 64 {
                    return Err(Error::Invalid("preview event kind exceeds 64 bytes".into()));
                }
                let kind = event.event.clone();
                self.scene.interact(&self.drawn, event)?;
                self.record(format!("interaction: {kind}"));
                self.redraw();
            }
            Command::Resolve(resolve) => {
                let effect = self
                    .effects
                    .remove(&EffectId::parse(resolve.effect)?)
                    .ok_or_else(|| Error::Invalid("unknown or already resolved effect".into()))?;
                self.record(
                    if resolve.success {
                        "effect: simulated success"
                    } else {
                        "effect: simulated refusal"
                    }
                    .into(),
                );
                if resolve.success
                    && let invoke::Op::ChangePresentation(change) = effect.operation()
                {
                    use omega_proto::omega::{PresentationAction, PresentationState};
                    let (state, lifecycle) = match PresentationAction::try_from(change.action) {
                        Ok(PresentationAction::Close) => {
                            (PresentationState::Closed, omega::surface::Lifecycle::Closed)
                        }
                        Ok(PresentationAction::Hide) => {
                            (PresentationState::Hidden, omega::surface::Lifecycle::Hidden)
                        }
                        _ => {
                            return Err(Error::Invalid(
                                "unsupported fixture presentation action".into(),
                            ));
                        }
                    };
                    self.scene.lifecycle(lifecycle)?;
                    self.presentation = state;
                    self.redraw();
                }
                effect
                    .complete(if resolve.success {
                        Ok(None)
                    } else {
                        Err(omega::effect::EffectError::Refused(
                            omega_proto::Refusal::unavailable(
                                "preview fixture refused the operation",
                            ),
                        ))
                    })
                    .map_err(|e| Error::Invalid(e.to_string()))?;
            }
            Command::Captured(_) => {
                return Err(Error::Invalid(
                    "capture completion belongs to the preview host".into(),
                ));
            }
        }
        Ok(())
    }
    fn reset(&mut self, selected: CaseId) -> Result<(), Error> {
        let factory = self
            .cases
            .factories
            .get(&selected)
            .ok_or_else(|| Error::Invalid("unknown preview case".into()))?;
        let scene = factory()?;
        self.scene = scene;
        self.selected = selected;
        self.epoch += 1;
        self.presentation = omega_proto::omega::PresentationState::Visible;
        self.effects.clear();
        self.events.clear();
        self.redraw();
        Ok(())
    }
    fn record(&mut self, event: String) {
        if self.events.len() == 64 {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }
    fn snapshot(&self, answered: u64, error: String) -> PreviewSnapshot {
        let mut view = self.drawn.tree().clone();
        view.revision = self.revision;
        PreviewSnapshot {
            version: VERSION,
            epoch: self.epoch,
            presentation: self.presentation as i32,
            answered,
            cases: self
                .cases
                .factories
                .keys()
                .map(ToString::to_string)
                .collect(),
            selected: self.selected.to_string(),
            view: Some(view),
            error,
            effects: self
                .effects
                .iter()
                .map(|(id, capture)| PreviewEffect {
                    id: id.get(),
                    operation: Self::operation(capture.operation()),
                })
                .collect(),
            events: self.events.iter().cloned().collect(),
            ..Default::default()
        }
    }
    fn operation(op: &invoke::Op) -> String {
        match op {
            invoke::Op::Act(act) => act
                .action
                .as_ref()
                .and_then(|a| a.kind.as_ref())
                .map(|a| format!("action: {:?}", ActionKind::of(a)))
                .unwrap_or_else(|| "action".into()),
            invoke::Op::ChangePresentation(_) => "presentation".into(),
            invoke::Op::SetState(_) => "record write".into(),
            invoke::Op::CallCommand(_) => "command".into(),
            _ => "operation".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omega::{
        Surface, View,
        platform::applications::{ApplicationId, Launcher},
        surface::{Events, Task},
        testing::State,
        ui::{Button, Column, Text},
    };
    use omega_proto::omega::{PreviewInteraction, PreviewResolution};
    #[derive(omega::Surface)]
    struct Launch {}
    #[derive(omega::Effects)]
    struct Effects {
        launch: Launcher,
        presentation: omega::surface::Presentation,
    }
    enum Message {
        Launch,
        Finished(omega::Result<()>),
        Dismissed(omega::Result<()>),
    }
    impl Surface for Launch {
        type Model = String;
        type Message = Message;
        type Effects = Effects;
        fn render(&self, model: &String, events: &Events<Message>) -> View {
            Column::new()
                .child(Text::new(model))
                .child(
                    Button::new("Launch")
                        .key("launch")
                        .on_press(events.on(|(): ()| Message::Launch)),
                )
                .into()
        }
        fn update(&self, model: &mut String, message: Message, effects: &Effects) -> Task<Message> {
            match message {
                Message::Launch => {
                    *model = "waiting".into();
                    return Task::perform(
                        effects
                            .launch
                            .launch(&ApplicationId::parse("never-run.desktop").unwrap()),
                        Message::Finished,
                    );
                }
                Message::Finished(result) => {
                    *model = if result.is_ok() { "success" } else { "refused" }.into();
                    if result.is_ok() {
                        return Task::perform(effects.presentation.close(), Message::Dismissed);
                    }
                }
                Message::Dismissed(result) => {
                    if result.is_err() {
                        *model = "close refused".into();
                    }
                }
            }
            Task::none()
        }
        fn lifecycle(
            &self,
            model: &mut String,
            event: omega::surface::Lifecycle,
            _: &Effects,
        ) -> Task<Message> {
            if matches!(event, omega::surface::Lifecycle::Closed) {
                Self::closed(model);
            }
            Task::none()
        }
    }
    impl Launch {
        fn closed(model: &mut String) {
            *model = "closed".into();
        }
    }
    struct Fixture;
    impl Fixture {
        fn session() -> Session {
            Session::new(Cases::new().surface::<Launch>("launch", State::new()), 1).unwrap()
        }
        fn request(session: &mut Session, command: Command) -> Result<(), Error> {
            session.request(PreviewRequest {
                id: 1,
                command: Some(command),
            })
        }
        fn press(session: &Session) -> Command {
            Command::Interact(PreviewInteraction {
                revision: session.revision,
                node: "launch".into(),
                event: "press".into(),
                value: None,
            })
        }
        async fn queued(session: &mut Session) -> CapturedEffect {
            tokio::time::timeout(Duration::from_secs(1), async {
                loop {
                    if let Some(effect) = session.scene.effect() {
                        return effect;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap()
        }
    }
    #[tokio::test]
    async fn effects_wait_for_an_explicit_outcome_and_cannot_be_resolved_twice() {
        let mut session = Fixture::session();
        let press = Fixture::press(&session);
        Fixture::request(&mut session, press).unwrap();
        assert!(session.drawn.text().contains("waiting"));
        let effect = Fixture::queued(&mut session).await;
        assert_eq!(Session::operation(effect.operation()), "action: LaunchApp");
        session.effects.insert(EffectId::parse(1).unwrap(), effect);
        let command = Command::Resolve(PreviewResolution {
            effect: 1,
            success: false,
        });
        Fixture::request(&mut session, command.clone()).unwrap();
        assert!(Fixture::request(&mut session, command).is_err());
        std::future::poll_fn(|cx| session.scene.poll(cx))
            .await
            .unwrap();
        session.redraw();
        assert!(session.drawn.text().contains("refused"));
        assert!(session.events.iter().all(|e| !e.contains("never-run")));
        let press = Fixture::press(&session);
        Fixture::request(&mut session, press).unwrap();
        let effect = Fixture::queued(&mut session).await;
        effect.complete(Ok(None)).unwrap();
        std::future::poll_fn(|cx| session.scene.poll(cx))
            .await
            .unwrap();
        session.redraw();
        assert!(session.drawn.text().contains("success"));
        let close = Fixture::queued(&mut session).await;
        session.effects.insert(EffectId::parse(2).unwrap(), close);
        Fixture::request(
            &mut session,
            Command::Resolve(PreviewResolution {
                effect: 2,
                success: true,
            }),
        )
        .unwrap();
        assert_eq!(
            session.presentation,
            omega_proto::omega::PresentationState::Closed
        );
        assert!(session.drawn.text().contains("closed"));
        let press = Fixture::press(&session);
        assert!(Fixture::request(&mut session, press).is_err());
    }
    #[tokio::test]
    async fn reset_drops_pending_work_and_rejects_previous_bindings() {
        let mut session = Fixture::session();
        let obsolete = Fixture::press(&session);
        Fixture::request(&mut session, obsolete.clone()).unwrap();
        let effect = Fixture::queued(&mut session).await;
        session.effects.insert(EffectId::parse(1).unwrap(), effect);
        Fixture::request(&mut session, Command::Reset(true)).unwrap();
        assert!(session.effects.is_empty());
        assert!(!session.drawn.text().contains("waiting"));
        assert!(Fixture::request(&mut session, obsolete).is_err());
    }
    #[tokio::test]
    async fn a_late_resolution_cannot_complete_a_rebuilt_cases_effect() {
        let mut first = Fixture::session();
        let press = Fixture::press(&first);
        Fixture::request(&mut first, press).unwrap();
        let effect = Fixture::queued(&mut first).await;
        let old = first.capture(effect).unwrap();
        let mut replacement =
            Session::new(Cases::new().surface::<Launch>("launch", State::new()), 2).unwrap();
        let press = Fixture::press(&replacement);
        Fixture::request(&mut replacement, press).unwrap();
        let effect = Fixture::queued(&mut replacement).await;
        let current = replacement.capture(effect).unwrap();
        assert!(
            Fixture::request(
                &mut replacement,
                Command::Resolve(PreviewResolution {
                    effect: old.get(),
                    success: true
                })
            )
            .is_err()
        );
        assert!(replacement.effects.contains_key(&current));
        assert!(replacement.drawn.text().contains("waiting"));
    }
}
