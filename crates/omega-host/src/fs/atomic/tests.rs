use super::{AtomicFile, WriteStep};
use crate::TempPath;
use std::path::PathBuf;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        Self(TempPath::sibling(
            &std::env::temp_dir().join("omega-atomic"),
            "test",
        ))
    }

    fn exercise(&self, old: Option<&[u8]>, fault: WriteStep) {
        let path = self.0.join("one/two/document");
        if let Some(old) = old {
            AtomicFile::at(&path).write(old).unwrap();
        }
        let mut file = AtomicFile::at(&path);
        file.fault = Some(fault);
        assert_eq!(
            file.write(b"new").unwrap_err().raw_os_error(),
            Some(libc::EIO)
        );

        let expected = if fault == WriteStep::FlushDirectory {
            Some(b"new".as_slice())
        } else {
            old
        };
        match expected {
            Some(bytes) => assert_eq!(std::fs::read(&path).unwrap(), bytes),
            None => assert!(!path.exists()),
        }
        let entries = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(
            entries,
            expected
                .map(|_| path.clone())
                .into_iter()
                .collect::<Vec<_>>()
        );

        // A caller can retry without knowing whether the failed write renamed.
        AtomicFile::at(&path).write(b"retried").unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"retried");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn failed_first_writes_preserve_absence_until_rename() {
    for fault in [
        WriteStep::Write,
        WriteStep::FlushFile,
        WriteStep::Rename,
        WriteStep::FlushDirectory,
    ] {
        Fixture::new().exercise(None, fault);
    }
}

#[test]
fn failed_replacements_preserve_old_contents_until_rename() {
    for fault in [
        WriteStep::Write,
        WriteStep::FlushFile,
        WriteStep::Rename,
        WriteStep::FlushDirectory,
    ] {
        Fixture::new().exercise(Some(b"old"), fault);
    }
}
