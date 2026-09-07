use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// A hidden sibling of a final path, unique per process and instant, used as
/// the write-then-rename staging spot.
pub(crate) struct TempPath;

impl TempPath {
    pub(crate) fn sibling(final_path: &Path, suffix: &str) -> PathBuf {
        let name = final_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "stage".to_string());
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        final_path.with_file_name(format!(".{name}.{suffix}-{}-{nanos}", std::process::id()))
    }
}
