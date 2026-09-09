//! Turning the daemon's own errors into answers a peer can act on.
//!
//! `ErrorCode` is a closed taxonomy — a peer branches on the code and reads
//! the message only for humans — so which code an error becomes is a
//! decision worth making once per error type, in one place where the choices
//! can be compared, rather than at each call site by whoever was writing it.

use omega_document::DocumentError;
use omega_proto::IdentError;
use omega_proto::ManifestError;
use omega_proto::TomlError;
use omega_proto::{AddressError, Refusal};

use crate::units::RequestError;

/// An error this daemon knows how to answer with.
pub trait Refusable {
    fn refusal(&self) -> Refusal;
}

/// A name that is not a name is the caller's mistake, whatever it was naming.
impl Refusable for IdentError {
    fn refusal(&self) -> Refusal {
        Refusal::invalid(self.to_string())
    }
}

impl Refusable for AddressError {
    fn refusal(&self) -> Refusal {
        Refusal::invalid(self.to_string())
    }
}

/// A manifest the daemon cannot read is the daemon's problem, not the
/// caller's: nothing the peer sends would make it work.
impl Refusable for ManifestError {
    fn refusal(&self) -> Refusal {
        Refusal::precondition(format!("manifest is not loadable: {self}"))
    }
}

impl Refusable for TomlError {
    fn refusal(&self) -> Refusal {
        Refusal::precondition(self.to_string())
    }
}

impl Refusable for DocumentError {
    fn refusal(&self) -> Refusal {
        Refusal::precondition(self.to_string())
    }
}

impl Refusable for RequestError {
    fn refusal(&self) -> Refusal {
        match self {
            // The unit's own answer, kept as it was: the daemon was only the
            // messenger, and flattening its code would lose why.
            Self::Refused { source, .. } => source.clone(),
            // The caller asked for something reasonable of a unit that is not
            // there yet — a precondition, not a bad request.
            Self::Absent(_) | Self::Timeout(_) => Refusal::precondition(self.to_string()),
        }
    }
}

/// `?` on a domain error inside anything that answers a peer.
pub trait RefusableResult<T> {
    fn or_refuse(self) -> Result<T, Refusal>;
}

impl<T, E: Refusable> RefusableResult<T> for Result<T, E> {
    fn or_refuse(self) -> Result<T, Refusal> {
        self.map_err(|e| e.refusal())
    }
}
