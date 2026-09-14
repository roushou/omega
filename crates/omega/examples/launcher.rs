//! A standalone launcher. The same surface is tested with injected catalogues and effects.
use omega::{
    Surface, View,
    platform::applications::{Application, ApplicationId, Applications, Launcher as Activation},
    surface::{Events, Lifecycle, Optional, Presentation, Task, TextEdit, TextValue},
    ui::{Column, Field, Image, List, Row, Text},
};

#[derive(Debug, Default)]
pub struct Model {
    query: TextValue,
    selected: Option<ApplicationId>,
    launching: bool,
    error: String,
}
#[derive(Debug)]
pub enum Message {
    Edited(TextEdit),
    Selected(ApplicationId),
    Activate(ApplicationId),
    Launched(omega::Result<()>),
    Dismissed(omega::Result<()>),
}
#[derive(Debug, omega::Effects)]
pub struct Effects {
    launcher: Activation,
    presentation: Presentation,
}
#[derive(Debug, omega::Surface)]
pub struct Launcher {
    applications: Optional<Applications>,
}
impl Launcher {
    fn results(&self, query: &str) -> Option<Vec<Application>> {
        let entries = self.applications.entries()?;
        let query: String = query.chars().take(256).collect::<String>().to_lowercase();
        let terms: Vec<_> = query.split_whitespace().take(16).collect();
        let mut matches = Vec::new();
        for app in entries {
            let name = app.name().to_lowercase();
            let other = format!(
                "{} {} {}",
                app.generic_name(),
                app.keywords().join(" "),
                app.description()
            )
            .to_lowercase();
            let mut score = 0;
            let mut matched = true;
            for term in &terms {
                score += if name == *term {
                    0
                } else if name.starts_with(term) {
                    10
                } else if name.split_whitespace().any(|word| word.starts_with(term)) {
                    20
                } else if name.contains(term) {
                    30
                } else if other.contains(term) {
                    50
                } else {
                    matched = false;
                    break;
                };
            }
            if matched {
                matches.push((score, name, app));
            }
        }
        matches.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then(a.1.cmp(&b.1))
                .then(a.2.id().cmp(b.2.id()))
        });
        Some(
            matches
                .into_iter()
                .take(20)
                .map(|(_, _, app)| app)
                .collect(),
        )
    }
}
impl Surface for Launcher {
    type Model = Model;
    type Message = Message;
    type Effects = Effects;
    fn render(&self, model: &Model, events: &Events<Message>) -> View {
        let results = self.results(model.query.text());
        let entries = results.as_deref().unwrap_or_default();
        let selected = model
            .selected
            .as_ref()
            .filter(|id| entries.iter().any(|app| app.id() == *id))
            .or_else(|| entries.first().map(Application::id))
            .map(ToString::to_string)
            .unwrap_or_default();
        let mut list = List::new()
            .key("results")
            .height(420)
            .selected(selected)
            .on_select(events.on(Message::Selected))
            .on_activate(events.on(Message::Activate))
            .children(entries.iter().map(|app| {
                Row::new()
                    .gap(12)
                    .key(app.id().to_string())
                    .child(
                        Image::icon(if app.icon().is_empty() {
                            "application-x-executable"
                        } else {
                            app.icon()
                        })
                        .width(32)
                        .height(32),
                    )
                    .child(
                        Column::new()
                            .gap(2)
                            .child(Text::new(app.name()))
                            .child(Text::new(app.description()).muted()),
                    )
            }));
        if model.launching {
            list = list.busy();
        }
        let status = if !model.error.is_empty() {
            &model.error
        } else if model.launching {
            "Opening…"
        } else if self.applications.is_pending() {
            "Loading applications…"
        } else if results.is_none() {
            "Application service unavailable"
        } else if entries.is_empty() {
            "No matching applications"
        } else {
            "↑ ↓ to select · Enter to open · Esc to close"
        };
        Column::new()
            .gap(12)
            .child(
                Field::new("Search applications")
                    .key("query")
                    .autofocus()
                    .controlled(&model.query)
                    .on_change(events.on(Message::Edited))
                    .navigate("results"),
            )
            .child(list)
            .child(Text::new(status).key("status").muted())
            .into()
    }
    fn update(&self, model: &mut Model, message: Message, effects: &Effects) -> Task<Message> {
        match message {
            Message::Edited(edit) => {
                if model.query.apply(edit) {
                    model.selected = None;
                    model.error.clear();
                }
            }
            Message::Selected(id) => model.selected = Some(id),
            Message::Activate(id) => {
                if model.launching {
                    return Task::none();
                }
                if !self
                    .results(model.query.text())
                    .is_some_and(|apps| apps.iter().any(|app| app.id() == &id))
                {
                    model.error = "This application is no longer in the results".into();
                    return Task::none();
                }
                model.launching = true;
                model.error.clear();
                return Task::perform(effects.launcher.launch(&id), Message::Launched);
            }
            Message::Launched(Ok(())) => {
                return Task::perform(effects.presentation.close(), Message::Dismissed);
            }
            Message::Launched(Err(error)) | Message::Dismissed(Err(error)) => {
                model.launching = false;
                model.error = error.to_string();
            }
            Message::Dismissed(Ok(())) => {}
        }
        Task::none()
    }
    fn lifecycle(&self, model: &mut Model, event: Lifecycle, _: &Effects) -> Task<Message> {
        if matches!(event, Lifecycle::Presented | Lifecycle::Closed) {
            model.query.reset("");
            model.selected = None;
            model.error.clear();
            model.launching = false;
        }
        Task::none()
    }
}
fn main() -> omega::Result<()> {
    omega::plugin!().surface(Launcher).run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use omega::testing::{
        operation::{Action, Operation, PresentationAction, Refusal},
        topic::{Application as Entry, ApplicationsState},
    };
    use omega::{
        effect::EffectError,
        testing::{State, SurfaceHarness},
    };
    struct Fixture;
    impl Fixture {
        fn state() -> State {
            State::new().with(ApplicationsState {
                applications: vec![
                    Entry {
                        id: "terminal.desktop".into(),
                        name: "Terminal".into(),
                        ..Default::default()
                    },
                    Entry {
                        id: "files.desktop".into(),
                        name: "Files".into(),
                        keywords: vec!["folders".into()],
                        ..Default::default()
                    },
                ],
            })
        }
        fn id(name: &str) -> ApplicationId {
            ApplicationId::parse(format!("{name}.desktop")).unwrap()
        }
    }
    #[tokio::test]
    async fn activation_failure_is_retryable_and_success_dismisses_once() {
        let mut surface = SurfaceHarness::<Launcher>::new(&Fixture::state()).unwrap();
        surface
            .send(Message::Activate(Fixture::id("terminal")))
            .unwrap();
        surface
            .send(Message::Activate(Fixture::id("files")))
            .unwrap();
        assert!(surface.model().launching);
        let refused = Refusal::unavailable("fixture refused activation");
        let request = surface
            .complete_effect(Err(EffectError::Refused(refused)))
            .await
            .unwrap();
        assert!(
            matches!(request, Operation::Act(act) if matches!(act.action.as_ref().unwrap().kind.as_ref(), Some(Action::LaunchApp(app)) if app.desktop_id == "terminal.desktop"))
        );
        surface.complete().await.unwrap();
        assert!(!surface.model().launching);
        assert!(surface.model().error.contains("fixture refused"));
        surface
            .send(Message::Activate(Fixture::id("files")))
            .unwrap();
        surface.complete_effect(Ok(None)).await.unwrap();
        surface.complete().await.unwrap();
        assert!(
            matches!(surface.complete_effect(Ok(None)).await.unwrap(), Operation::ChangePresentation(change) if change.action == PresentationAction::Close as i32)
        );
        surface.lifecycle(Lifecycle::Closed).unwrap();
        surface.lifecycle(Lifecycle::Presented).unwrap();
        assert!(surface.model().query.text().is_empty());
        assert!(!surface.model().launching);
    }
    #[tokio::test]
    async fn instances_search_independently_and_catalogue_updates_invalidate_targets() {
        let mut first = SurfaceHarness::<Launcher>::new(&Fixture::state()).unwrap();
        let second = SurfaceHarness::<Launcher>::new(&Fixture::state()).unwrap();
        first
            .send(Message::Edited(TextEdit {
                text: "folders".into(),
                revision: 1,
                reset: 0,
            }))
            .unwrap();
        assert!(second.model().query.text().is_empty());
        let drawn = first.draw();
        first
            .interact(&drawn, "results", "select", Fixture::id("files"))
            .unwrap();
        first.state(&State::new().with(ApplicationsState::default()));
        first.send(Message::Activate(Fixture::id("files"))).unwrap();
        assert!(!first.model().launching);
        assert!(first.model().error.contains("no longer"));
    }
}
