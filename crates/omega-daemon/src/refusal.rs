//! Map domain errors to protocol refusal codes once per error type.

use omega_document::DocumentError;
use omega_host::TomlError;
use omega_proto::IdentError;
use omega_proto::ManifestError;
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

impl Refusable for omega_document::ValidationError {
    fn refusal(&self) -> Refusal {
        Refusal::invalid(self.to_string())
    }
}

impl Refusable for omega_proto::instance::RendererFingerprintError {
    fn refusal(&self) -> Refusal {
        Refusal::invalid(self.to_string())
    }
}

impl Refusable for omega_proto::instance::PresentationError {
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
            // there yet. Availability is distinct from malformed arguments.
            Self::Absent(_) => Refusal::unavailable(self.to_string()),
            Self::Timeout(_) => Refusal::deadline(self.to_string()),
            Self::Full(_) => Refusal::exhausted(self.to_string()),
            Self::TooLarge(_) => Refusal::too_large(self.to_string()),
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

impl Refusable for omega_platform::BrokerError {
    fn refusal(&self) -> Refusal {
        match self {
            Self::Unserved(_) | Self::Unsupported(_) => Refusal::unimplemented(self.to_string()),
            Self::Io(_) | Self::Unreadable(_) => Refusal::unavailable(self.to_string()),
            Self::Timeout => Refusal::deadline(self.to_string()),
            Self::Full => Refusal::exhausted(self.to_string()),
            Self::TooLarge => Refusal::too_large(self.to_string()),
        }
    }
}

impl Refusable for crate::state::StateError {
    fn refusal(&self) -> Refusal {
        match self {
            Self::Address(error) => error.refusal(),
            Self::TooLarge => Refusal::too_large(self.to_string()),
            Self::Full | Self::RevisionExhausted => Refusal::exhausted(self.to_string()),
        }
    }
}

impl Refusable for crate::hub::PublishError {
    fn refusal(&self) -> Refusal {
        match self {
            Self::Readiness(error) => Refusal::invalid(error.to_string()),
            Self::State(error) => error.refusal(),
            Self::TooLarge => Refusal::too_large(self.to_string()),
            Self::Full | Self::RevisionExhausted => Refusal::exhausted(self.to_string()),
        }
    }
}

impl Refusable for omega_proto::action::ActionError {
    fn refusal(&self) -> Refusal {
        Refusal::invalid(self.to_string())
    }
}

impl Refusable for crate::reconcile::shell::ShellApplyError {
    fn refusal(&self) -> Refusal {
        use crate::reconcile::shell::ShellApplyError;
        match self {
            ShellApplyError::Io(_) => Refusal::unavailable(self.to_string()),
            _ => Refusal::precondition(self.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn broker_operation_failures_are_not_missing_handlers() {
        use omega_platform::BrokerError;
        use omega_proto::omega::ErrorCode;
        for (error, code) in [
            (BrokerError::Timeout, ErrorCode::DeadlineExceeded),
            (BrokerError::Full, ErrorCode::ResourceExhausted),
            (BrokerError::TooLarge, ErrorCode::PayloadTooLarge),
            (BrokerError::gone(), ErrorCode::Unavailable),
        ] {
            assert_eq!(error.refusal().code, code);
        }
        assert_eq!(
            BrokerError::Unserved(omega_proto::ActionKind::Lock)
                .refusal()
                .code,
            ErrorCode::Unimplemented
        );
    }
}
