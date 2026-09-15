use omega::{
    Surface, View,
    surface::{Events, Lifecycle, Task, TextEdit, TextValue},
    testing::{State, SurfaceHarness},
    ui::{Button, Text},
};

#[derive(omega::Surface)]
struct Counter {}
#[derive(Default)]
struct Model {
    value: u32,
}
enum Message {
    Add(u32),
    Work(u32, u64),
    Done(omega::Result<u32>),
}
impl Surface for Counter {
    type Model = Model;
    type Message = Message;
    type Effects = ();
    fn render(&self, model: &Model, events: &Events<Message>) -> View {
        let captured = model.value + 1;
        Button::new(model.value)
            .key("add")
            .on_press(events.on(move |()| Message::Add(captured)))
            .into()
    }
    fn update(&self, model: &mut Model, message: Message, _: &()) -> Task<Message> {
        match message {
            Message::Add(value) => model.value += value,
            Message::Work(value, delay) => {
                return Task::replace(
                    "search",
                    async move {
                        tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                        Ok(value)
                    },
                    Message::Done,
                );
            }
            Message::Done(value) => model.value = value.unwrap(),
        }
        Task::none()
    }
}
#[tokio::test]
async fn models_and_bindings_belong_to_one_instance_and_one_render() {
    let mut first = SurfaceHarness::<Counter>::new(&State::new()).unwrap();
    let mut second = SurfaceHarness::<Counter>::new(&State::new()).unwrap();
    let old = first.draw();
    let other = second.draw();
    assert!(first.interact(&other, "add", "press", ()).is_err());
    first.interact(&old, "add", "press", ()).unwrap();
    let fresh = first.draw();
    assert!(first.interact(&old, "add", "press", ()).is_err());
    first.interact(&fresh, "add", "press", ()).unwrap();
    assert_eq!(first.model().value, 3);
    assert_eq!(second.model().value, 0);
}
#[tokio::test(start_paused = true)]
async fn replacement_and_close_invalidate_task_delivery() {
    let mut instance = SurfaceHarness::<Counter>::new(&State::new()).unwrap();
    instance.send(Message::Work(1, 10)).unwrap();
    tokio::task::yield_now().await;
    instance.send(Message::Work(2, 1)).unwrap();
    instance.complete().await.unwrap();
    assert_eq!(instance.model().value, 2);
    tokio::time::advance(std::time::Duration::from_secs(20)).await;
    instance.send(Message::Work(3, 1)).unwrap();
    instance.lifecycle(Lifecycle::Closed).unwrap();
    tokio::time::advance(std::time::Duration::from_secs(20)).await;
    assert_eq!(instance.model().value, 2);
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(1), instance.complete())
            .await
            .is_err()
    );
}

#[derive(omega::Surface)]
struct Blocked;
impl Surface for Blocked {
    type Model = bool;
    type Message = bool;
    type Effects = ();
    fn render(&self, busy: &bool, events: &Events<bool>) -> View {
        let parent = omega::ui::Column::new().child(
            Button::new("Blocked")
                .key("blocked")
                .on_press(events.on(|()| false)),
        );
        if *busy {
            parent.busy().into()
        } else {
            parent.disabled().into()
        }
    }
    fn update(&self, busy: &mut bool, message: bool, _: &()) -> Task<bool> {
        *busy = message;
        Task::none()
    }
}

#[test]
fn harness_respects_disabled_and_busy_ancestors() {
    let mut harness = SurfaceHarness::<Blocked>::new(&State::new()).unwrap();
    for busy in [false, true] {
        harness.send(busy).unwrap();
        let drawn = harness.draw();
        let error = harness
            .interact(&drawn, "blocked", "press", ())
            .unwrap_err();
        assert!(matches!(error, omega::Error::Refused(refusal)
            if refusal.code == omega_proto::omega::ErrorCode::FailedPrecondition));
        assert_eq!(*harness.model(), busy);
    }
}
#[test]
fn text_resets_reject_old_edits_and_reordered_updates() {
    let mut value = TextValue::default();
    assert!(value.apply(TextEdit {
        text: "new".into(),
        revision: 2,
        reset: 0
    }));
    assert!(!value.apply(TextEdit {
        text: "old".into(),
        revision: 1,
        reset: 0
    }));
    value.reset("reset");
    assert!(!value.apply(TextEdit {
        text: "late".into(),
        revision: 3,
        reset: 0
    }));
    assert_eq!(value.text(), "reset");
}
#[derive(omega::Surface)]
struct Loading {
    battery: omega::surface::Optional<omega::platform::power::Battery>,
}
impl omega::Surface for Loading {
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
    fn render(&self, _: &(), _: &omega::surface::Events<Self::Message>) -> View {
        Text::new(if self.battery.is_pending() {
            "pending"
        } else if self.battery.has_reading() {
            "available"
        } else {
            "absent"
        })
        .into()
    }
}
#[test]
fn optional_readings_distinguish_pending_absent_and_available() {
    use omega::testing::{Drawn, SystemTopic};
    assert_eq!(
        Drawn::of::<Loading>(&State::new()).unwrap().text(),
        "pending"
    );
    assert_eq!(
        Drawn::of::<Loading>(&State::new().absent(SystemTopic::Battery))
            .unwrap()
            .text(),
        "absent"
    );
    assert_eq!(
        Drawn::of::<Loading>(&State::new().battery(0.5, false))
            .unwrap()
            .text(),
        "available"
    );
}
