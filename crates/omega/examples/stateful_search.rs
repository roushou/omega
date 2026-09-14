//! A fixture catalogue exercises local UI behavior without launching desktop apps.
const RESULTS: omega::ui::ListTarget = omega::ui::ListTarget::new("results");

use omega::{
    Surface, View,
    keyboard::{Chord, Key, Keymap},
    surface::{Events, Lifecycle, Task, TextEdit, TextValue},
    ui::{Button, Column, Field, List, Text},
};

#[derive(Debug, Default)]
pub struct Model {
    query: TextValue,
    results: Vec<String>,
    selected: String,
    status: String,
}
#[derive(Debug)]
pub enum Message {
    Edited(TextEdit),
    Found(omega::Result<Vec<String>>),
    Selected(String),
    Activate(String),
    Clear,
}
#[derive(Debug, omega::Surface)]
pub struct Search {}
impl Search {
    fn search(query: String) -> Task<Message> {
        Task::replace(
            "search",
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(80)).await;
                Ok(["Browser", "Files", "Terminal", "Settings", "Text Editor"]
                    .into_iter()
                    .filter(|name| name.to_lowercase().contains(&query.to_lowercase()))
                    .map(str::to_string)
                    .collect())
            },
            Message::Found,
        )
    }
}
impl Surface for Search {
    type Model = Model;
    type Message = Message;
    type Effects = ();
    fn mounted(&self, _: &mut Model, _: &()) -> Task<Message> {
        Self::search(String::new())
    }
    fn render(&self, model: &Model, events: &Events<Message>) -> View {
        Column::new()
            .shortcuts(Keymap::single(
                Chord::new(Key::Escape),
                events.on(|()| Message::Clear),
            ))
            .gap(12)
            .child(
                Field::new("Search fixture applications")
                    .key("query")
                    .autofocus()
                    .controlled(&model.query)
                    .on_change(events.on(Message::Edited))
                    .navigate(RESULTS),
            )
            .child(
                List::new()
                    .target(RESULTS)
                    .selected(&model.selected)
                    .children(model.results.iter().map(|name| Text::new(name).key(name)))
                    .on_select(events.on(Message::Selected))
                    .on_activate(events.on(Message::Activate)),
            )
            .child(Text::new(&model.status).key("status"))
            .child(
                Button::new("Clear")
                    .key("clear")
                    .on_press(events.on(|()| Message::Clear)),
            )
            .into()
    }
    fn update(&self, model: &mut Model, message: Message, _: &()) -> Task<Message> {
        match message {
            Message::Edited(edit) => {
                if model.query.apply(edit) {
                    model.status = "Searching…".into();
                    return Self::search(model.query.text().into());
                }
            }
            Message::Found(Ok(results)) => {
                if !results.contains(&model.selected) {
                    model.selected = results.first().cloned().unwrap_or_default();
                }
                model.results = results;
                model.status = if model.results.is_empty() {
                    "No matches"
                } else {
                    "Fixture results"
                }
                .into();
            }
            Message::Found(Err(error)) => model.status = error.to_string(),
            Message::Selected(key) => model.selected = key,
            Message::Activate(key) => model.status = format!("Selected {key} (fixture only)"),
            Message::Clear => {
                model.query.reset("");
                return Self::search(String::new());
            }
        }
        Task::none()
    }
    fn lifecycle(&self, model: &mut Model, event: Lifecycle, _: &()) -> Task<Message> {
        match event {
            Lifecycle::Presented => Self::search(model.query.text().into()),
            Lifecycle::Closed => {
                model.status = "Closed".into();
                Task::none()
            }
            Lifecycle::Hidden => Task::none(),
        }
    }
}
fn main() -> omega::Result<()> {
    omega::plugin!().surface(Search).run()
}
