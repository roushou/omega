//! Signals `tokio::process` does not send.

use std::io;

/// Who this process is.
#[derive(Debug)]
pub struct Identity;

impl Identity {
    /// The effective user the daemon runs as. A peer with the same uid can
    /// already signal the daemon and rewrite its state dir, so it is the
    /// operator, not a stranger.
    pub fn uid() -> u32 {
        // SAFETY: `geteuid` reads a process attribute and cannot fail.
        unsafe { libc::geteuid() }
    }
}

/// A polite request to a child process. `Child::kill` is `SIGKILL`, which is
/// the wrong first word to a unit that may have a socket to flush.
#[derive(Debug)]
pub struct Signal;

impl Signal {
    /// Ask the process to exit.
    pub fn terminate(pid: i32) -> io::Result<()> {
        // SAFETY: `kill` with a pid we spawned and a valid signal number. A
        // pid that has already been reaped returns ESRCH rather than acting
        // on an unrelated process, because the child is not reaped until the
        // supervisor waits on it.
        match unsafe { libc::kill(pid, libc::SIGTERM) } {
            0 => Ok(()),
            _ => Err(io::Error::last_os_error()),
        }
    }
}
