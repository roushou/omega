use crate::ui::{Paint, Step, Ui};
use omega_base::execution::{Outcome, Report};
use omega_host::{
    Layout,
    recovery::{InstalledReplacement, ReplacementOutcome},
};
use std::{cell::RefCell, path::PathBuf, rc::Rc};

/// Confirmed changes survive a later step failure. The runner remains agnostic
/// to installation facts; only the operations that observe them record them.
#[derive(Clone, Default)]
pub(super) struct Changes(Rc<RefCell<Vec<Entry>>>);

enum Entry {
    File(InstalledReplacement),
    Unverified(PathBuf),
    ShellBackup { backup: PathBuf, target: PathBuf },
}

impl Changes {
    pub(super) fn installed(&self, installed: InstalledReplacement) {
        let mut entries = self.0.borrow_mut();
        if installed.outcome == ReplacementOutcome::Unchanged
            && entries.iter().any(|entry| {
                matches!(entry, Entry::File(previous) if previous.target == installed.target)
            })
        {
            return;
        }
        entries.push(Entry::File(installed));
    }

    pub(super) fn unverified(&self, target: PathBuf) {
        self.0.borrow_mut().push(Entry::Unverified(target));
    }

    pub(super) fn shell_backup(&self, backup: PathBuf, target: PathBuf) {
        self.0
            .borrow_mut()
            .push(Entry::ShellBackup { backup, target });
    }

    pub(super) fn show(&self, ui: &mut Ui, reports: &[Report], layout: &Layout) {
        let entries = self.0.borrow();
        if entries.is_empty() {
            return;
        }

        ui.blank();
        ui.step(Step::Checked, "setup changes");

        let mut recoverable = false;
        for entry in entries.iter() {
            match entry {
                Entry::File(installed) => {
                    let verb = match installed.outcome {
                        ReplacementOutcome::Created => Step::Created,
                        ReplacementOutcome::Updated => Step::Changed,
                        ReplacementOutcome::Unchanged => Step::Kept,
                    };
                    ui.step(verb, Paint::path(&installed.target));

                    if let Some(recovery) = &installed.recovery {
                        recoverable = true;
                        ui.detail(format!(
                            "Recovery record: {}",
                            Paint::path(&recovery.record)
                        ));
                        ui.detail(format!(
                            "Undo: {}",
                            Paint::command(format!(
                                "omega recovery restore {}",
                                recovery.receipt.id
                            )),
                        ));
                    }
                }
                Entry::Unverified(target) => {
                    ui.warn(format!(
                        "Could not confirm the final state of {}",
                        Paint::path(target)
                    ));
                }
                Entry::ShellBackup { backup, target } => {
                    ui.step(
                        Step::Checked,
                        format!("original shell backup: {}", Paint::path(backup)),
                    );
                    ui.detail(format!("Restore destination: {}", Paint::path(target)));
                    ui.detail("To restore: run systemctl --user stop omega.service, or stop the foreground daemon.");
                    ui.detail(
                        "Copy this backup over the destination, then run omarchy restart shell.",
                    );
                    ui.detail(
                        "Keep Omega stopped until your Rust layout agrees with the restored shell.",
                    );
                }
            }
        }

        for report in reports {
            if report.outcome != Outcome::Completed {
                continue;
            }
            match report.description.id.0 {
                "init.daemon.activate" => {
                    ui.step(Step::Enabled, "daemon service started and enabled at login")
                }
                "build.publish" => ui.step(Step::Built, "new configuration generation published"),
                "init.generation.verify" => ui.step(
                    Step::Checked,
                    format!(
                        "configuration applied; shell file: {}",
                        Paint::path(&layout.shell_config)
                    ),
                ),
                "init.shell.restart" => ui.step(Step::Restarted, "Omarchy shell"),
                _ => {}
            }
        }

        if recoverable {
            ui.detail("Recovery records preserve the previous contents, or record that a target was absent.");
            ui.detail("Before undoing changes: run systemctl --user stop omega.service, or stop the foreground daemon.");
            ui.detail("Restore files in reverse order.");
            for report in reports {
                if report.outcome != Outcome::Completed {
                    continue;
                }
                match report.description.id.0 {
                    "init.service.install" => ui.detail(
                        "After restoring the service file: systemctl --user daemon-reload.",
                    ),
                    "init.renderer.install" => {
                        ui.detail("After restoring renderer files: omarchy restart shell.")
                    }
                    "init.daemon.activate" => ui.detail(
                        "Restoring files does not undo service enablement or running processes.",
                    ),
                    _ => {}
                }
            }
            if entries.iter().any(|entry| {
                matches!(entry, Entry::File(file) if file.recovery.is_some() && file.target.starts_with(&layout.config))
            }) {
                ui.detail("Rebuild restored sources before starting Omega.");
            }
            ui.next("omega recovery list");
        }
    }
}
