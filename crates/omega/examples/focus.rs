//! Focus timer. Schedule `tick` every second in the system document.
//!
//! Elapsed time includes suspend and ignores wall-clock adjustments. A plugin
//! restart resumes from the daemon's record; a daemon restart resets the timer.
//! Completion is notified at most once; a crash between recording completion
//! and notification may lose that notification.
use omega::config::{Fields, Values};
use omega::platform::notification::Notify;
use omega::record::{Own, PluginState, Watch};
use omega::ui::{Button, Metric, Row, Section, Text};
use omega::{Command, Plugin, Surface, Ui};

pub const PLUGIN: &str = env!("CARGO_PKG_NAME");

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Timer {
    #[default]
    Idle,
    Running {
        deadline: u64,
        observed: u64,
    },
    Paused {
        remaining: u64,
    },
    Complete,
}
impl Timer {
    const DURATION: u64 = 25 * 60 * 1000;
    fn now() -> u64 {
        let time = rustix::time::clock_gettime(rustix::time::ClockId::Boottime);
        time.tv_sec as u64 * 1000 + time.tv_nsec as u64 / 1_000_000
    }
    fn remaining(&self) -> u64 {
        match *self {
            Self::Running { deadline, observed } => deadline.saturating_sub(observed),
            Self::Paused { remaining } => remaining,
            Self::Idle => Self::DURATION,
            Self::Complete => 0,
        }
    }
    fn start(&mut self, now: u64) {
        if matches!(self, Self::Running { .. }) {
            return;
        }
        let remaining = match *self {
            Self::Paused { remaining } => remaining,
            _ => Self::DURATION,
        };
        *self = Self::Running {
            deadline: now.saturating_add(remaining),
            observed: now,
        };
    }
    fn pause(&mut self, now: u64) {
        if let Self::Running { deadline, .. } = *self
            && now < deadline
        {
            *self = Self::Paused {
                remaining: deadline - now,
            };
        }
    }
    fn tick(&mut self, now: u64) -> bool {
        if let Self::Running { deadline, .. } = *self {
            if now >= deadline {
                *self = Self::Complete;
                return true;
            }
            *self = Self::Running {
                deadline,
                observed: now,
            };
        }
        false
    }
}
impl PluginState for Timer {
    const PLUGIN: &'static str = env!("CARGO_PKG_NAME");
    const KEY: &'static str = "timer";
}
impl Fields for Timer {
    fn read(values: &Values) -> Self {
        match values.get::<String>("phase").as_deref() {
            Some("running") => match (values.get::<u64>("deadline"), values.get::<u64>("observed"))
            {
                (Some(deadline), Some(observed)) if deadline > observed => {
                    Self::Running { deadline, observed }
                }
                _ => Self::Idle,
            },
            Some("paused") => match values.get::<u64>("remaining") {
                Some(remaining) if remaining > 0 && remaining <= Self::DURATION => {
                    Self::Paused { remaining }
                }
                _ => Self::Idle,
            },
            Some("complete") => Self::Complete,
            _ => Self::Idle,
        }
    }
    fn write(&self) -> Values {
        match *self {
            Self::Idle => Values::new().with("phase", "idle"),
            Self::Running { deadline, observed } => Values::new()
                .with("phase", "running")
                .with("deadline", deadline)
                .with("observed", observed),
            Self::Paused { remaining } => Values::new()
                .with("phase", "paused")
                .with("remaining", remaining),
            Self::Complete => Values::new().with("phase", "complete"),
        }
    }
}

#[derive(omega::Surface, Debug)]
pub struct Indicator {
    timer: Watch<Timer>,
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
        let timer = self.timer.get();
        let seconds = timer.remaining().div_ceil(1000);
        Text::new(format!("Focus {:02}:{:02}", seconds / 60, seconds % 60)).into()
    }
}
#[derive(omega::Surface, Debug)]
pub struct Panel {
    timer: Watch<Timer>,
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
        let timer = self.timer.get();
        let label = match timer {
            Timer::Idle => "Ready",
            Timer::Running { .. } => "Focusing",
            Timer::Paused { .. } => "Paused",
            Timer::Complete => "Complete",
        };
        let seconds = timer.remaining().div_ceil(1000);
        let action = if matches!(timer, Timer::Running { .. }) {
            Button::new("Pause").on_press(Pause)
        } else {
            Button::new(if matches!(timer, Timer::Paused { .. }) {
                "Resume"
            } else {
                "Start focus"
            })
            .on_press(Start)
        };
        Section::new("Focus")
            .child(Metric::new(format!("{:02}:{:02}", seconds / 60, seconds % 60)).label(label))
            .child(
                Row::new()
                    .gap(8)
                    .child(action.primary().key("primary").fill_width())
                    .child(
                        Button::new("Reset")
                            .secondary()
                            .on_press(Reset)
                            .key("reset"),
                    ),
            )
            .into()
    }
}
#[derive(omega::Command, Debug)]
pub struct Start {
    timer: Own<Timer>,
}
impl Command for Start {
    const ID: &'static str = "start";

    type Input = ();
    type Output = ();
    async fn call(&self, _: ()) -> omega::Result<()> {
        self.timer.update(|timer| timer.start(Timer::now())).await
    }
}
#[derive(omega::Command, Debug)]
pub struct Pause {
    timer: Own<Timer>,
}
impl Command for Pause {
    const ID: &'static str = "pause";

    type Input = ();
    type Output = ();
    async fn call(&self, _: ()) -> omega::Result<()> {
        self.timer.update(|timer| timer.pause(Timer::now())).await
    }
}
#[derive(omega::Command, Debug)]
pub struct Reset {
    timer: Own<Timer>,
}
impl Command for Reset {
    const ID: &'static str = "reset";

    type Input = ();
    type Output = ();
    async fn call(&self, _: ()) -> omega::Result<()> {
        self.timer.set(&Timer::Idle).await
    }
}
#[derive(omega::Command, Debug)]
pub struct Tick {
    timer: Own<Timer>,
    notify: Notify,
}
impl Command for Tick {
    const ID: &'static str = "tick";

    type Input = ();
    type Output = ();
    async fn call(&self, _: ()) -> omega::Result<()> {
        if !matches!(self.timer.get(), Timer::Running { .. }) {
            return Ok(());
        }
        let mut completed = false;
        self.timer
            .update(|timer| completed = timer.tick(Timer::now()))
            .await?;
        if completed {
            self.notify.send("Focus complete — take a break").await?;
        }
        Ok(())
    }
}
pub fn plugin() -> Plugin {
    Plugin::new(PLUGIN, env!("CARGO_PKG_VERSION"))
        .surface_as::<Indicator>("indicator")
        .surface_as::<Panel>("panel")
        .command::<Start>()
        .command::<Pause>()
        .command::<Reset>()
        .command::<Tick>()
}
fn main() -> omega::Result<()> {
    plugin().run()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pause_resume_and_suspend_preserve_elapsed_time() {
        let mut timer = Timer::Idle;
        timer.start(1000);
        timer.pause(11_000);
        assert_eq!(timer.remaining(), Timer::DURATION - 10_000);
        timer.start(50_000);
        let restored = Timer::read(&timer.write());
        assert_eq!(restored, timer);
        assert!(timer.tick(50_000 + Timer::DURATION));
        assert!(!timer.tick(50_000 + Timer::DURATION + 1));
        assert_eq!(timer, Timer::Complete);
    }
    #[test]
    fn repeated_start_does_not_extend_a_running_timer() {
        let mut timer = Timer::Idle;
        timer.start(10);
        let original = timer.clone();
        timer.start(20);
        assert_eq!(timer, original);
    }
    #[test]
    fn incomplete_records_reset_to_idle() {
        assert_eq!(
            Timer::read(&Values::new().with("phase", "running")),
            Timer::Idle
        );
    }
}
