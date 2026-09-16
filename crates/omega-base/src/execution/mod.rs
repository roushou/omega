//! Declare a typed pipeline, replace individual implementations, then execute it.
//!
//! Composition performs no effects. Every step owns its input and produces the
//! next input. The runner records outcomes and stops on the first failure; domain
//! operations own persistence and recovery. Dropping a polled run reports the
//! active step as interrupted, without promising that external effects stopped.

mod pipeline;
mod step;
pub mod testing;

pub use pipeline::{Failure, FailureCause, Pipeline, Run};
use std::path::PathBuf;
pub use step::{Carry, Operation, Step};

/// Stable identity of one step within a pipeline, including test replacements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepId(pub &'static str);

/// A declared step's identity and purpose, available before execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Description {
    pub id: StepId,
    pub title: String,
}

impl Description {
    pub fn new(id: &'static str, title: impl Into<String>) -> Self {
        Self {
            id: StepId(id),
            title: title.into(),
        }
    }
}

/// A structured progress detail. Observers decide how to display paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detail {
    pub message: String,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Running,
    Completed,
    Skipped(String),
    Failed(String),
    Interrupted,
}

/// One attempted step. Unreached steps have no report; inspect the pipeline's
/// declaration to see the entire sequence. Details survive a later step failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub description: Description,
    pub outcome: Outcome,
    pub details: Vec<Detail>,
}

/// Receives start/terminal updates and details as they happen. Implementations
/// must not panic. Notifications are not durable commit acknowledgments.
pub trait Observer {
    fn observe(&mut self, report: &Report);

    fn detail(&mut self, _step: &Description, _detail: &Detail) {}
}

impl Observer for () {
    fn observe(&mut self, _: &Report) {}
}

/// Reporting scoped to the executing step. Operations cannot access the runner,
/// execute another step, change their identity, or reach the concrete observer.
pub struct Progress<'a> {
    report: &'a mut Report,
    observer: &'a mut dyn Observer,
    skipped: &'a mut Option<String>,
}

impl Progress<'_> {
    pub fn message(&mut self, message: impl Into<String>) {
        self.emit(Detail {
            message: message.into(),
            path: None,
        });
    }

    pub fn path(&mut self, message: impl Into<String>, path: impl Into<PathBuf>) {
        self.emit(Detail {
            message: message.into(),
            path: Some(path.into()),
        });
    }

    /// Mark a successful no-op. An error or interruption still takes precedence.
    pub fn skip(&mut self, reason: impl Into<String>) {
        *self.skipped = Some(reason.into());
    }

    fn emit(&mut self, detail: Detail) {
        self.report.details.push(detail);
        self.observer.detail(
            &self.report.description,
            self.report.details.last().expect("inserted detail"),
        );
    }
}

struct Runner<'a> {
    observer: &'a mut dyn Observer,
    reports: Vec<Report>,
}

impl Runner<'_> {
    fn begin(&mut self, description: Description) -> Attempt<'_> {
        self.reports.push(Report {
            description,
            outcome: Outcome::Running,
            details: Vec::new(),
        });

        let report = self.reports.last_mut().expect("inserted report");
        self.observer.observe(report);

        Attempt {
            report,
            observer: self.observer,
            skipped: None,
            finished: false,
        }
    }
}

struct Attempt<'a> {
    report: &'a mut Report,
    observer: &'a mut dyn Observer,
    skipped: Option<String>,
    finished: bool,
}

impl Attempt<'_> {
    fn progress(&mut self) -> Progress<'_> {
        Progress {
            report: self.report,
            observer: self.observer,
            skipped: &mut self.skipped,
        }
    }

    fn finish<T, E: std::fmt::Display>(mut self, result: &Result<T, E>) {
        self.report.outcome = match result {
            Ok(_) => self
                .skipped
                .take()
                .map(Outcome::Skipped)
                .unwrap_or(Outcome::Completed),
            Err(error) => Outcome::Failed(error.to_string()),
        };

        self.finished = true;
        self.observer.observe(self.report);
    }
}

impl Drop for Attempt<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.report.outcome = Outcome::Interrupted;
            self.observer.observe(self.report);
        }
    }
}

impl std::fmt::Debug for Progress<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Progress")
            .field("report", &self.report)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    use testing::{Event, Pending, Recorder, Stub};

    struct PollOnce;

    impl PollOnce {
        fn ready<F: Future>(future: F) -> F::Output {
            let mut future = Box::pin(future);
            match future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
            {
                Poll::Ready(value) => value,
                Poll::Pending => panic!("expected an immediately ready fixture"),
            }
        }
    }

    #[test]
    fn composition_is_inert_replacements_keep_identity_and_failures_stop_the_sequence() {
        let mut first = Step::<(), usize, &str>::new("first", "first operation");
        first.replace(Stub::returning(3));
        let plan = Pipeline::new()
            .then(first)
            .then(Step::new("fail", "reject").using(Stub::<usize, (), _>::failing("rejected")))
            .then(Step::<(), (), _>::new("unreached", "must not run"));
        assert_eq!(
            plan.steps().iter().map(|s| s.id.0).collect::<Vec<_>>(),
            ["first", "fail", "unreached"]
        );

        let run = PollOnce::ready(plan.run((), &mut ()));
        assert_eq!(run.reports.len(), 2);

        let failure = run.result.unwrap_err();
        assert_eq!(failure.step.id.0, "fail");
        assert!(matches!(failure.cause, FailureCause::Operation("rejected")));
    }

    #[test]
    fn unconfigured_steps_fail_without_any_production_fallback() {
        let plan = Pipeline::new().then(Step::<(), (), &str>::new("missing", "missing operation"));

        let run = PollOnce::ready(plan.run((), &mut ()));
        assert!(matches!(
            run.result.unwrap_err().cause,
            FailureCause::Unconfigured
        ));
    }

    #[test]
    fn cancellation_reports_only_the_active_step_and_unpolled_runs_do_nothing() {
        let mut recorder = Recorder::default();
        let plan = Pipeline::new()
            .then(Step::<(), (), &str>::new("wait", "wait").using(Pending::default()))
            .then(Step::new("later", "later").using(Stub::returning(())));
        let mut future = Box::pin(plan.run((), &mut recorder));
        assert!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );

        drop(future);
        assert_eq!(recorder.events.len(), 2);
        assert!(matches!(
            &recorder.events[1],
            Event::Outcome(Report {
                outcome: Outcome::Interrupted,
                ..
            })
        ));

        let plan = Pipeline::new().then(Step::<(), (), &str>::new("unpolled", "unused"));
        drop(plan.run((), &mut recorder));
        assert_eq!(recorder.events.len(), 2);
    }

    #[test]
    fn owned_inputs_pass_through_gate_replacements_and_are_captured_by_the_next_operation() {
        use std::{cell::RefCell, rc::Rc};
        use testing::{Gate, Pass};
        struct Capture(Rc<RefCell<Vec<String>>>);

        impl Operation<String> for Capture {
            type Output = usize;
            type Error = &'static str;

            async fn execute(
                self,
                input: String,
                progress: &mut Progress<'_>,
            ) -> Result<usize, Self::Error> {
                progress.message("captured input");
                let length = input.len();
                self.0.borrow_mut().push(input);
                Ok(length)
            }
        }

        let captured = Rc::new(RefCell::new(Vec::new()));
        let (gate, release) = Gate::new(Pass::<&str>::default());
        let pipeline = Pipeline::new()
            .then(Step::new("gate", "wait for test").using(gate))
            .then(Step::new("capture", "capture input").using(Capture(captured.clone())));
        assert!(captured.borrow().is_empty());
        let mut observer = ();
        let mut future = Box::pin(pipeline.run(String::from("hello"), &mut observer));
        let mut context = Context::from_waker(Waker::noop());
        assert!(future.as_mut().poll(&mut context).is_pending());
        assert!(captured.borrow().is_empty());

        release.release();
        let Poll::Ready(run) = future.as_mut().poll(&mut context) else {
            panic!("released gate must finish");
        };
        assert_eq!(run.result.unwrap(), 5);
        assert_eq!(*captured.borrow(), ["hello"]);
        assert_eq!(run.reports[1].details[0].message, "captured input");
    }

    #[test]
    fn panics_report_interruption_and_skip_is_only_terminal_after_success() {
        struct PanicAfterSkip;

        impl Operation<()> for PanicAfterSkip {
            type Output = ();
            type Error = &'static str;

            async fn execute(self, _: (), progress: &mut Progress<'_>) -> Result<(), Self::Error> {
                progress.skip("not finished yet");
                panic!("fixture panic");
            }
        }
        let mut recorder = Recorder::default();
        let plan = Pipeline::new().then(Step::new("panic", "panic").using(PanicAfterSkip));
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            PollOnce::ready(plan.run((), &mut recorder))
        }));
        assert!(outcome.is_err());
        assert!(matches!(
            recorder.events.last(),
            Some(Event::Outcome(Report {
                outcome: Outcome::Interrupted,
                ..
            }))
        ));
    }

    #[test]
    fn carried_context_and_operation_errors_keep_their_types() {
        let step = Step::<u32, String, &str>::new("convert", "convert input")
            .using(Stub::returning("converted".into()))
            .carrying::<u64>();
        let plan = Pipeline::new().then(step);

        let run = PollOnce::ready(plan.run((42, 7), &mut ()));
        assert_eq!(run.result.unwrap(), (42, "converted".into()));
    }

    #[test]
    #[should_panic(expected = "pipeline step IDs must be unique")]
    fn duplicate_identities_are_refused_during_composition() {
        let _ = Pipeline::new()
            .then(Step::<(), (), &str>::new("duplicate", "first"))
            .then(Step::<(), (), &str>::new("duplicate", "second"));
    }
}
