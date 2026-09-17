//! The initialization sequence and its replaceable, typed implementation slots.

use super::{
    InitReport, Initialize,
    preflight::{Inspected, Preflight, PrepareWorkspace, Prepared},
    steps::*,
    summary::Changes,
};
use crate::build::steps::{self as build, Compiled, Described, Published, Sources, Validated};
use omega_base::execution::{Carry, Pipeline, Step};

type Slot<I, O> = Step<I, O, anyhow::Error>;

pub(super) struct Steps {
    pub(super) preflight: Slot<Initialize, Inspected>,
    pub(super) prepare_workspace: Slot<Inspected, Prepared>,
    pub(super) adopt_shell: Slot<Prepared, Prepared>,
    pub(super) write_workspace: Slot<Prepared, Prepared>,
    pub(super) dependencies: Slot<Prepared, WorkspaceReady>,
    pub(super) finish_bare: Slot<WorkspaceReady, InitReport>,

    pub(super) prepare_build: Slot<WorkspaceReady, (Installation, Sources)>,
    pub(super) compile: Slot<(Installation, Sources), (Installation, Compiled)>,
    pub(super) describe: Slot<(Installation, Compiled), (Installation, Described)>,
    pub(super) validate: Slot<(Installation, Described), (Installation, Validated)>,

    pub(super) install_renderer: Slot<(Installation, Validated), (ServicePending, Validated)>,
    pub(super) install_service: Slot<(ServicePending, Validated), (Runtime, Validated)>,
    pub(super) start_daemon: Slot<(Runtime, Validated), (Runtime, Validated)>,
    pub(super) verify_daemon: Slot<(Runtime, Validated), (Runtime, Validated)>,

    pub(super) publish: Slot<(Runtime, Validated), (Runtime, Published)>,
    pub(super) verify_application: Slot<(Runtime, Published), Applied>,
    pub(super) restart_shell: Slot<Applied, Applied>,
    pub(super) verify_renderer: Slot<Applied, InitReport>,
}

impl Steps {
    pub(super) fn compose(self, bare: bool) -> Pipeline<Initialize, InitReport, anyhow::Error> {
        let workspace = Pipeline::new()
            .then(self.preflight)
            .then(self.prepare_workspace)
            .then(self.adopt_shell)
            .then(self.write_workspace)
            .then(self.dependencies);

        if bare {
            workspace.then(self.finish_bare)
        } else {
            workspace
                .then(self.prepare_build)
                .then(self.compile)
                .then(self.describe)
                .then(self.validate)
                .then(self.install_renderer)
                .then(self.install_service)
                .then(self.start_daemon)
                .then(self.verify_daemon)
                .then(self.publish)
                .then(self.verify_application)
                .then(self.restart_shell)
                .then(self.verify_renderer)
        }
    }

    /// No slot falls through to native effects. Tests explicitly select the
    /// operations they want to exercise and replace the rest with fixtures.
    pub(super) fn isolated() -> Self {
        let build = build::Steps::isolated();
        Self {
            preflight: Step::new("init.preflight", "check initialization requirements"),
            prepare_workspace: Step::new(
                "init.workspace.prepare",
                "prepare workspace files and shell import",
            ),
            adopt_shell: Step::new(
                "init.adopt",
                "back up and adopt the existing shell configuration",
            ),
            write_workspace: Step::new("init.workspace.write", "install workspace files"),
            dependencies: Step::new("init.dependencies", "configure dependency sources"),
            finish_bare: Step::new("init.bare", "finish workspace-only initialization"),
            prepare_build: Step::new("init.build.prepare", "prepare the desktop build"),
            compile: build.compile.carrying(),
            describe: build.describe.carrying(),
            validate: build.validate.carrying(),
            publish: build.publish.carrying(),
            install_renderer: Step::new("init.renderer.install", "install renderer files"),
            install_service: Step::new("init.service.install", "install daemon service file"),
            start_daemon: Step::new("init.daemon.activate", "enable and start the daemon"),
            verify_daemon: Step::new("init.daemon.verify", "verify daemon health and version"),
            verify_application: Step::new(
                "init.generation.verify",
                "wait for configuration and shell application",
            ),
            restart_shell: Step::new(
                "init.shell.restart",
                "restart the shell to load installed renderer code",
            ),
            verify_renderer: Step::new("init.renderer.verify", "check live renderer attachments"),
        }
    }

    pub(super) fn production(changes: &Changes) -> Self {
        let mut steps = Self::isolated();

        steps.preflight.replace(Preflight);
        steps.prepare_workspace.replace(PrepareWorkspace);
        steps.adopt_shell.replace(AdoptShell(changes.clone()));
        steps
            .write_workspace
            .replace(WriteWorkspace(changes.clone()));
        steps
            .dependencies
            .replace(ConfigureDependencies(changes.clone()));
        steps.finish_bare.replace(FinishBare);

        steps.prepare_build.replace(PrepareBuild);
        steps.compile.replace(Carry(build::Compile));
        steps.describe.replace(Carry(build::DescribePlugins));
        steps.validate.replace(Carry(build::Validate));
        steps.publish.replace(Carry(build::Publish));

        steps
            .install_renderer
            .replace(InstallRenderer(changes.clone()));
        steps
            .install_service
            .replace(InstallService(changes.clone()));
        steps.start_daemon.replace(StartDaemon);
        steps.verify_daemon.replace(VerifyDaemon);
        steps.verify_application.replace(VerifyApplication);
        steps.restart_shell.replace(RestartShell);
        steps.verify_renderer.replace(VerifyRenderer);

        steps
    }
}
