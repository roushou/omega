use std::{fs::File, io, os::unix::fs::OpenOptionsExt, path::Path};

/// Creation and durability of directory entries.
#[derive(Debug)]
pub struct Directory;

impl Directory {
    /// Create a directory tree and flush its ancestor chain, including existing
    /// directories. Retrying after a failed flush must establish durability even
    /// when the preceding attempt already created every directory.
    pub fn create_all(path: &Path) -> io::Result<()> {
        Self::create_with(path, Self::sync)
    }

    fn create_with(path: &Path, mut sync: impl FnMut(&Path) -> io::Result<()>) -> io::Result<()> {
        let path = std::path::absolute(Self::nonempty(path))?;
        std::fs::create_dir_all(&path)?;
        for ancestor in path.ancestors() {
            sync(Self::nonempty(ancestor))?;
        }
        Ok(())
    }

    /// Flush directory entries. Missing directories and unsupported flushes are
    /// errors: neither establishes durability.
    pub fn sync(path: &Path) -> io::Result<()> {
        File::options()
            .read(true)
            .custom_flags(libc::O_DIRECTORY)
            .open(Self::nonempty(path))?
            .sync_all()
    }

    fn nonempty(path: &Path) -> &Path {
        if path.as_os_str().is_empty() {
            Path::new(".")
        } else {
            path
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Directory;
    use crate::TempPath;
    use std::{io, path::Path};

    #[test]
    fn retry_flushes_the_whole_chain_even_after_creation_succeeded() {
        let root = TempPath::sibling(&std::env::temp_dir().join("omega-directory"), "test");
        let path = root.join("one/two");
        let ancestors = path.ancestors().collect::<Vec<_>>();
        for failure in 0..ancestors.len() {
            let mut calls = Vec::new();
            let error = Directory::create_with(&path, |directory| {
                calls.push(directory.to_path_buf());
                if calls.len() - 1 == failure {
                    Err(io::Error::from_raw_os_error(libc::EIO))
                } else {
                    Directory::sync(directory)
                }
            })
            .unwrap_err();
            assert_eq!(error.raw_os_error(), Some(libc::EIO));
            assert!(path.is_dir());
            assert_eq!(calls, ancestors[..=failure]);
            let mut retried = Vec::new();
            Directory::create_with(&path, |directory| {
                retried.push(directory.to_path_buf());
                Directory::sync(directory)
            })
            .unwrap();
            assert_eq!(retried, ancestors);
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn relative_paths_include_the_current_directory() {
        let mut calls = Vec::new();
        Directory::create_with(Path::new(""), |directory| {
            calls.push(directory.to_path_buf());
            Ok(())
        })
        .unwrap();
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(calls, cwd.ancestors().collect::<Vec<_>>());
    }

    #[test]
    fn a_file_cannot_be_used_as_a_directory() {
        let path = TempPath::sibling(&std::env::temp_dir().join("omega-directory"), "file");
        std::fs::write(&path, b"file").unwrap();
        assert!(Directory::create_all(&path.join("nested")).is_err());
        assert!(Directory::sync(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
