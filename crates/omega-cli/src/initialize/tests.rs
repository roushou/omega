use super::*;
use super::{
    pipeline::Steps,
    preflight::{Host, HostPlan, Inspected, PrepareWorkspace},
    steps::*,
    summary::Changes,
};
use crate::{build::steps as build, renderer::Snapshot, service::DaemonService};
use omega_base::execution::{
    Carry, FailureCause, Operation, Outcome, Progress,
    testing::{Event, Pass, Pending, Recorder, Stub},
};
use omega_host::{
    TempPath,
    recovery::{RecoveryStore, Replacement, Snapshot as FileSnapshot},
};
use std::{
    future::Future,
    path::PathBuf,
    task::{Context, Waker},
};

struct Fixture {
    root: PathBuf,
    request: Initialize,
}

impl Fixture {
    fn new(bare: bool) -> Self {
        let root = TempPath::sibling(&std::env::temp_dir().join("omega-pipeline"), "test");
        let request = Initialize {
            layout: Layout::at(root.join("config"), root.join("state"), root.join("cache")),
            socket: Socket::at(root.join("control.sock")),
            bare,
            profile: Profile::Debug,
        };
        Self { root, request }
    }

    fn steps(&self) -> Steps {
        let mut steps = Steps::isolated();
        let host = (!self.request.bare).then(|| HostPlan {
            host: Host {
                service: omega_host::systemd::Service::new(
                    omega_host::systemd::Manager::new(omega_host::systemd::Scope::User),
                    DaemonService::name(),
                    self.root.join("omega.service"),
                )
                .unwrap(),
                definition: DaemonService::definition(&self.root.join("omega")).unwrap(),
                shell: omega_omarchy::HostShell::Omarchy,
            },
            service: Replacement::prepare(
                &self.root.join("omega.service"),
                FileSnapshot::file(b"plugin"),
            )
            .unwrap(),
            renderers: vec![
                Replacement::prepare(&self.root.join("renderer"), FileSnapshot::file(b"renderer"))
                    .unwrap(),
            ],
        });
        steps.preflight.replace(Stub::returning(Inspected {
            request: self.request.clone(),
            source: None,
            host,
        }));
        steps.prepare_workspace.replace(PrepareWorkspace);
        steps.adopt_shell.replace(AdoptShell(Changes::default()));
        steps
            .write_workspace
            .replace(WriteWorkspace(Changes::default()));
        steps
            .dependencies
            .replace(ConfigureDependencies(Changes::default()));
        steps.finish_bare.replace(FinishBare);
        steps.prepare_build.replace(PrepareBuild);
        steps.compile.replace(Carry(FakeCompile));
        steps.describe.replace(Carry(build::DescribePlugins));
        steps.validate.replace(Carry(FakeDocument));
        steps
            .install_renderer
            .replace(InstallRenderer(Changes::default()));
        steps
            .install_service
            .replace(InstallService(Changes::default()));
        steps.start_daemon.replace(Pass::default());
        steps.verify_daemon.replace(Pass::default());
        steps.publish.replace(Carry(build::Publish));
        steps.verify_application.replace(FakeApplication);
        steps.restart_shell.replace(Pass::default());
        steps.verify_renderer.replace(FakeRenderer);
        steps
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct FakeCompile;

impl Operation<build::Sources> for FakeCompile {
    type Output = build::Compiled;
    type Error = anyhow::Error;

    async fn execute(
        self,
        sources: build::Sources,
        _: &mut Progress<'_>,
    ) -> anyhow::Result<Self::Output> {
        build::Compiled::fixture(sources)
    }
}

struct FakeDocument;

impl Operation<build::Described> for FakeDocument {
    type Output = build::Validated;
    type Error = anyhow::Error;

    async fn execute(
        self,
        described: build::Described,
        _: &mut Progress<'_>,
    ) -> anyhow::Result<Self::Output> {
        described.validate(omega_document::StateDocument::default())
    }
}

struct FakeApplication;

impl Operation<(Runtime, build::Published)> for FakeApplication {
    type Output = Applied;
    type Error = anyhow::Error;

    async fn execute(
        self,
        (runtime, published): (Runtime, build::Published),
        _: &mut Progress<'_>,
    ) -> anyhow::Result<Applied> {
        Ok(Applied {
            runtime,
            published,
            before: Snapshot {
                active: Vec::new(),
                placements: Vec::new(),
            },
        })
    }
}

struct FakeRenderer;

impl Operation<Applied> for FakeRenderer {
    type Output = InitReport;
    type Error = anyhow::Error;

    async fn execute(self, applied: Applied, _: &mut Progress<'_>) -> anyhow::Result<InitReport> {
        Ok(InitReport {
            layout: applied.published.layout,
            renderer: Some(Verification::NoPlacements),
        })
    }
}

#[tokio::test]
async fn isolated_initialization_never_falls_back_to_host_effects() {
    let f = Fixture::new(false);
    let run = Steps::isolated()
        .compose(false)
        .run(f.request.clone(), &mut ())
        .await;
    let failure = run.result.err().unwrap();
    assert_eq!(failure.step.id.0, "init.preflight");
    assert!(matches!(failure.cause, FailureCause::Unconfigured));
    assert!(!f.root.exists());
}

#[tokio::test]
async fn the_declared_pipeline_runs_real_file_recovery_with_replaced_external_steps() {
    let f = Fixture::new(false);
    let plan = f.steps().compose(false);
    let declared = plan
        .steps()
        .iter()
        .map(|step| step.id.0)
        .collect::<Vec<_>>();
    assert!(
        !f.request.layout.workspace_manifest().exists(),
        "composition must be inert"
    );
    let run = plan.run(f.request.clone(), &mut ()).await;
    assert!(run.result.is_ok());
    assert_eq!(
        run.reports
            .iter()
            .map(|report| report.description.id.0)
            .collect::<Vec<_>>(),
        declared
    );
    assert!(f.request.layout.active_build().is_file());
    assert!(
        RecoveryStore::new(&f.request.layout)
            .receipts()
            .unwrap()
            .len()
            >= 5
    );
    assert!(!f.request.socket.is_live());
}

#[tokio::test]
async fn every_step_can_fail_without_redeclaring_the_pipeline_or_running_successors() {
    for failed in 0..17 {
        let f = Fixture::new(false);
        let mut steps = f.steps();
        let error = || anyhow::anyhow!("injected failure");
        match failed {
            0 => steps.preflight.replace(Stub::failing(error())),
            1 => steps.prepare_workspace.replace(Stub::failing(error())),
            2 => steps.adopt_shell.replace(Stub::failing(error())),
            3 => steps.write_workspace.replace(Stub::failing(error())),
            4 => steps.dependencies.replace(Stub::failing(error())),
            5 => steps.prepare_build.replace(Stub::failing(error())),
            6 => steps.compile.replace(Stub::failing(error())),
            7 => steps.describe.replace(Stub::failing(error())),
            8 => steps.validate.replace(Stub::failing(error())),
            9 => steps.install_renderer.replace(Stub::failing(error())),
            10 => steps.install_service.replace(Stub::failing(error())),
            11 => steps.start_daemon.replace(Stub::failing(error())),
            12 => steps.verify_daemon.replace(Stub::failing(error())),
            13 => steps.publish.replace(Stub::failing(error())),
            14 => steps.verify_application.replace(Stub::failing(error())),
            15 => steps.restart_shell.replace(Stub::failing(error())),
            16 => steps.verify_renderer.replace(Stub::failing(error())),
            _ => unreachable!(),
        }
        let plan = steps.compose(false);
        let id = plan.steps()[failed].id;
        let run = plan.run(f.request.clone(), &mut ()).await;
        let failure = run.result.err().unwrap();
        assert_eq!(failure.step.id, id);
        assert_eq!(run.reports.len(), failed + 1);
        assert!(matches!(run.reports[failed].outcome, Outcome::Failed(_)));
        assert_eq!(f.request.layout.active_build().exists(), failed > 13);
    }
}

#[tokio::test]
async fn bare_mode_uses_the_same_workspace_prefix_without_desktop_steps() {
    let f = Fixture::new(true);
    let plan = f.steps().compose(true);
    assert_eq!(plan.steps().last().unwrap().id.0, "init.bare");
    let run = plan.run(f.request.clone(), &mut ()).await;
    assert!(run.result.is_ok());
    assert!(f.request.layout.workspace_manifest().exists());
    assert!(!f.request.layout.active_build().exists());
    assert!(!f.root.join("omega.service").exists());
}

#[tokio::test]
async fn interrupted_pipeline_retains_file_receipts_and_never_publishes() {
    let f = Fixture::new(false);
    let mut steps = f.steps();
    steps.start_daemon.replace(Pending::default());
    let mut recorder = Recorder::default();
    let mut run = Box::pin(steps.compose(false).run(f.request.clone(), &mut recorder));
    assert!(
        run.as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    drop(run);
    assert!(
        matches!(recorder.events.last(), Some(Event::Outcome(report)) if report.description.id.0 == "init.daemon.activate" && report.outcome == Outcome::Interrupted)
    );
    assert!(f.root.join("omega.service").exists());
    assert!(!f.request.layout.active_build().exists());
    assert!(
        RecoveryStore::new(&f.request.layout)
            .receipts()
            .unwrap()
            .len()
            >= 5
    );
    // Reopening would block if the cancelled run retained its source lock.
    drop(crate::workspace::ConfigWorkspace::open(f.request.layout.clone()).unwrap());
}
