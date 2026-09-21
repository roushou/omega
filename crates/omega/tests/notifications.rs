//! The notifications reading exposes the notifications Omega has raised.

use omega::testing::{Drawn, State};
use omega::{Surface, View, ui::Text};
use omega_proto::omega::{ActiveNotification, NotificationsState};

#[derive(omega::Surface)]
struct Raised {
    notifications: omega::platform::notification::Notifications,
}

impl Surface for Raised {
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
        Text::new(self.notifications.active().len()).into()
    }
}

#[test]
fn notifications_read_their_topic() {
    let drawn = Drawn::of::<Raised>(&State::new().with(NotificationsState {
        notifications: vec![
            ActiveNotification {
                id: 1,
                summary: "Battery low".into(),
                ..Default::default()
            },
            ActiveNotification {
                id: 2,
                summary: "Update ready".into(),
                ..Default::default()
            },
        ],
    }))
    .unwrap();
    assert_eq!(drawn.text(), "2");
}
