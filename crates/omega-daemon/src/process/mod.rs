//! Signals `tokio::process` does not send.

use std::io;

/// Who this process is.
#[derive(Debug)]
pub struct Identity;

impl Identity {
    /// Effective daemon uid used to identify the operator.
    pub fn uid() -> u32 {
        // SAFETY: `geteuid` reads a process attribute and cannot fail.
        unsafe { libc::geteuid() }
    }
}

/// Send SIGTERM before escalating to SIGKILL at the shutdown deadline.
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

mod child;
pub mod session;
mod token;
pub(crate) use child::ManagedChild;
pub use token::{SpawnToken, TokenError};

pub(crate) use token::SpawnIdentity;
