use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

/// A hidden sibling of a final path, distinct for every allocation in a process, used as
/// the write-then-rename staging spot.
#[derive(Debug)]
pub struct TempPath;

impl TempPath {
    pub fn sibling(final_path: &Path, suffix: &str) -> PathBuf {
        Self::at(final_path, suffix, SystemTime::now())
    }

    fn at(final_path: &Path, suffix: &str, now: SystemTime) -> PathBuf {
        // A clock reading can repeat across threads or move backwards. Only
        // allocation identity is shared; this counter publishes no file contents.
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let sequence = NEXT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .expect("temporary path sequence exhausted");
        let name = final_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "stage".to_string());
        let nanos = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        final_path.with_file_name(format!(
            ".{name}.{suffix}-{}-{nanos}-{sequence}",
            std::process::id()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn concurrent_paths_are_distinct_even_when_the_clock_never_moves() {
        let threads: Vec<_> = (0..8)
            .map(|_| {
                std::thread::spawn(|| {
                    (0..100)
                        .map(|_| TempPath::at(Path::new("output"), "stage", SystemTime::UNIX_EPOCH))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let paths: BTreeSet<_> = threads
            .into_iter()
            .flat_map(|thread| thread.join().unwrap())
            .collect();
        assert_eq!(paths.len(), 800);
    }
}
