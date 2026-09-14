use std::convert::Infallible;

use omega::{
    Surface, View,
    platform::power::Battery,
    surface::{Events, Lifecycle, Task},
    testing::{Drawn, State, SurfaceHarness, SystemTopic},
    ui::Text,
};

#[derive(omega::Surface)]
struct Reading {
    battery: Battery,
}

impl Surface for Reading {
    type Model = u32;
    type Message = Infallible;
    type Effects = ();

    fn render(&self, mounts: &u32, _: &Events<Infallible>) -> View {
        Text::new(format!("{mounts}: {}", self.battery.charge())).into()
    }

    fn update(&self, _: &mut u32, message: Infallible, _: &()) -> Task<Infallible> {
        match message {}
    }

    fn mounted(&self, mounts: &mut u32, _: &()) -> Task<Infallible> {
        *mounts += 1;
        Task::none()
    }
}

#[test]
fn message_free_surfaces_initialize_once_and_obey_readiness_without_tokio() {
    let mut instance = SurfaceHarness::<Reading>::new(&State::new()).unwrap();
    assert_eq!(*instance.model(), 1);
    assert!(instance.draw().is_empty());
    assert!(Drawn::of::<Reading>(&State::new()).unwrap().is_empty());

    let reported = State::new().battery(0.5, false);
    instance.state(&reported);
    assert_eq!(instance.draw().text(), "1: 50%");
    assert_eq!(Drawn::of::<Reading>(&reported).unwrap().text(), "1: 50%");
    instance.lifecycle(Lifecycle::Closed).unwrap();
    instance.lifecycle(Lifecycle::Presented).unwrap();
    assert_eq!(*instance.model(), 1);

    let absent = State::new().absent(SystemTopic::Battery);
    instance.state(&absent);
    assert_eq!(instance.draw().text(), "1: 0%");
    assert_eq!(Drawn::of::<Reading>(&absent).unwrap().text(), "1: 0%");
}

#[derive(omega::Surface)]
struct InvalidInitialization;

impl Surface for InvalidInitialization {
    type Model = ();
    type Message = ();
    type Effects = ();

    fn render(&self, _: &(), _: &Events<()>) -> View {
        Text::new("Must not render").into()
    }

    fn update(&self, _: &mut (), _: (), _: &()) -> Task<()> {
        Task::none()
    }

    fn mounted(&self, _: &mut (), _: &()) -> Task<()> {
        Task::batch((0..17).map(|_| Task::perform(async { Ok(()) }, |_| ())))
    }
}

#[test]
fn one_shot_rendering_propagates_initialization_errors() {
    let error = Drawn::of::<InvalidInitialization>(&State::new()).unwrap_err();
    assert!(error.to_string().contains("at most 16 tasks"));
}
