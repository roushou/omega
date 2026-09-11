//! `omega clean`: remove what a build can make again.

use std::path::Path;

use omega_host::{Generations, Layout};

use crate::ui::{Paint, Step, Ui};

/// Remove the build cache.
///
/// Safe while the desktop is running: the daemon runs the binaries staged in
/// the state dir, not the ones cargo left in `target/`. The state dir is
/// retained unless generation cleanup proves a directory has no references or leases.
#[derive(Debug, clap::Args)]
pub struct CleanCmd {
    /// Also remove the unit logs.
    #[arg(long)]
    pub logs: bool,

    /// Also remove unreferenced build generations that no running process leases.
    #[arg(long)]
    pub generations: bool,
}

impl CleanCmd {
    pub fn run(self, ui: &mut Ui) -> anyhow::Result<()> {
        let layout = Layout::resolve();

        if self.generations {
            let removed = Generations::new(&layout).clean()?;
            if removed.is_empty() {
                ui.step(Step::Done, "no unused build generations");
            }
            for id in removed {
                ui.step(Step::Removed, format!("build generation {id}"));
            }
        }

        let mut targets = vec![layout.target_dir()];
        if self.logs {
            targets.push(layout.logs_dir());
        }

        let mut freed = 0;
        for path in &targets {
            match Self::remove(path)? {
                None => ui.step(
                    Step::Done,
                    format!("{} was already gone", Paint::path(path)),
                ),
                Some(bytes) => {
                    freed += bytes;
                    ui.step(
                        Step::Removed,
                        format!("{} ({})", Paint::path(path), Paint::size(bytes)),
                    );
                }
            }
        }

        if targets.len() > 1 && freed > 0 {
            ui.step(Step::Done, format!("{} freed", Paint::size(freed)));
        }
        Ok(())
    }

    /// Measure before removing, because afterwards there is nothing to ask.
    /// `None` when there was nothing there — not an error, since the point of
    /// the command is that the directory should not exist.
    fn remove(path: &Path) -> anyhow::Result<Option<u64>> {
        if !path.exists() {
            return Ok(None);
        }
        let bytes = Self::size(path);
        std::fs::remove_dir_all(path)?;
        Ok(Some(bytes))
    }

    /// Bytes under a directory. Symlinks are measured, never followed: a
    /// linked plugin's tree is somebody else's disk.
    fn size(path: &Path) -> u64 {
        let Ok(entries) = std::fs::read_dir(path) else {
            return 0;
        };
        entries
            .flatten()
            .map(|entry| match entry.file_type() {
                Ok(kind) if kind.is_dir() => Self::size(&entry.path()),
                Ok(kind) if kind.is_symlink() => 0,
                _ => entry.metadata().map(|meta| meta.len()).unwrap_or(0),
            })
            .sum()
    }
}
