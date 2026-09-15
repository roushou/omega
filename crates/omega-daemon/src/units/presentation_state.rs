//! Presentation intent and observation, independent of delivery and lifetime.

use omega_proto::{Refusal, omega};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Visibility {
    Visible,
    Hidden,
    Closed,
}

impl Visibility {
    pub(super) fn requested(action: omega::PresentationAction) -> Result<Self, Refusal> {
        match action {
            omega::PresentationAction::Present => Ok(Self::Visible),
            omega::PresentationAction::Hide => Ok(Self::Hidden),
            omega::PresentationAction::Close => Ok(Self::Closed),
            omega::PresentationAction::Destroy | omega::PresentationAction::Unspecified => {
                Err(Refusal::invalid("invalid presentation transition"))
            }
        }
    }

    pub(super) fn observed(value: i32) -> Result<Self, Refusal> {
        match omega::PresentationState::try_from(value) {
            Ok(omega::PresentationState::Visible) => Ok(Self::Visible),
            Ok(omega::PresentationState::Hidden) => Ok(Self::Hidden),
            Ok(omega::PresentationState::Closed) => Ok(Self::Closed),
            Ok(omega::PresentationState::Unspecified) | Err(_) => {
                Err(Refusal::invalid("unknown presentation state"))
            }
        }
    }

    pub(super) fn wire(self) -> i32 {
        match self {
            Self::Visible => omega::PresentationState::Visible as i32,
            Self::Hidden => omega::PresentationState::Hidden as i32,
            Self::Closed => omega::PresentationState::Closed as i32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Observation {
    Unknown,
    Known(Visibility),
}

impl Observation {
    pub(super) fn wire(self) -> i32 {
        match self {
            Self::Unknown => omega::PresentationState::Unspecified as i32,
            Self::Known(visibility) => visibility.wire(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PresentationState {
    requested: Visibility,
    observed: Observation,
}

impl PresentationState {
    pub(super) fn new(requested: Visibility, observed: Observation) -> Self {
        Self {
            requested,
            observed,
        }
    }

    pub(super) fn requested(self) -> Visibility {
        self.requested
    }

    pub(super) fn observed(self) -> Observation {
        self.observed
    }

    pub(super) fn request(self, requested: Visibility) -> Self {
        Self { requested, ..self }
    }

    pub(super) fn report(self, observed: Visibility) -> Self {
        Self {
            requested: if observed == Visibility::Closed {
                Visibility::Closed
            } else {
                self.requested
            },
            observed: Observation::Known(observed),
        }
    }

    pub(super) fn disconnected(self) -> Self {
        Self {
            observed: Observation::Unknown,
            ..self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dismissal_survives_disconnect_until_an_explicit_reopen() {
        let initial =
            PresentationState::new(Visibility::Visible, Observation::Known(Visibility::Hidden));
        let visible = initial.report(Visibility::Visible);
        let closed = visible.report(Visibility::Closed);
        assert_eq!(
            closed,
            PresentationState::new(Visibility::Closed, Observation::Known(Visibility::Closed))
        );
        assert_eq!(
            closed.request(Visibility::Visible),
            PresentationState::new(Visibility::Visible, Observation::Known(Visibility::Closed))
        );
        let disconnected = closed.disconnected();
        assert_eq!(
            disconnected,
            PresentationState::new(Visibility::Closed, Observation::Unknown)
        );
        let late = disconnected.report(Visibility::Visible);
        assert_eq!(late.requested(), Visibility::Closed);
        assert_eq!(late.observed(), Observation::Known(Visibility::Visible));
        assert_eq!(
            late.request(Visibility::Visible).requested(),
            Visibility::Visible
        );
    }

    #[test]
    fn requests_and_disconnects_are_idempotent_and_nonclosing_reports_preserve_intent() {
        let states = [Visibility::Visible, Visibility::Hidden, Visibility::Closed];
        for requested in states {
            for observed in [
                Observation::Unknown,
                Observation::Known(Visibility::Visible),
                Observation::Known(Visibility::Hidden),
                Observation::Known(Visibility::Closed),
            ] {
                let state = PresentationState::new(requested, observed);
                assert_eq!(state.disconnected().disconnected(), state.disconnected());
                assert_eq!(state.disconnected().requested(), requested);
                for next in states {
                    assert_eq!(state.request(next).request(next), state.request(next));
                    assert_eq!(state.request(next).observed(), observed);
                }
                for next in [Visibility::Visible, Visibility::Hidden] {
                    assert_eq!(state.report(next).requested(), requested);
                    assert_eq!(state.report(next).report(next), state.report(next));
                }
            }
        }
    }

    #[test]
    fn wire_boundaries_exclude_unknown_requests_observations_and_destruction() {
        for (action, state) in [
            (omega::PresentationAction::Present, Visibility::Visible),
            (omega::PresentationAction::Hide, Visibility::Hidden),
            (omega::PresentationAction::Close, Visibility::Closed),
        ] {
            assert_eq!(Visibility::requested(action), Ok(state));
            assert_eq!(Visibility::observed(state.wire()), Ok(state));
        }
        for action in [
            omega::PresentationAction::Destroy,
            omega::PresentationAction::Unspecified,
        ] {
            assert!(Visibility::requested(action).is_err());
        }
        for value in [Observation::Unknown.wire(), -1, i32::MAX] {
            assert!(Visibility::observed(value).is_err());
        }
    }
}
