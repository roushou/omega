use crate::{Chord, KeyEvent};

/// An ordered collection of nonoverlapping bindings. Scoping is a host concern.
#[derive(Debug, Clone)]
pub struct Keymap<A> {
    bindings: Vec<(Chord, A)>,
}

impl<A> Default for Keymap<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A> Keymap<A> {
    pub const fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }
    /// Construct a map with one binding; no conflicts are possible.
    pub fn single(chord: Chord, action: A) -> Self {
        Self {
            bindings: vec![(chord, action)],
        }
    }

    /// Add a binding, rejecting any chord that can match the same event.
    pub fn bind(mut self, chord: Chord, action: A) -> Result<Self, Conflict> {
        if self
            .bindings
            .iter()
            .any(|(existing, _)| existing.overlaps(chord))
        {
            return Err(Conflict { chord });
        }
        self.bindings.push((chord, action));
        Ok(self)
    }
    /// Compose maps without silently overriding either map's bindings.
    pub fn merge(mut self, other: Self) -> Result<Self, Conflict> {
        for (chord, action) in other.bindings {
            self = self.bind(chord, action)?;
        }
        Ok(self)
    }
    /// Adapt actions without changing keyboard policy.
    pub fn map<B>(self, mut map: impl FnMut(A) -> B) -> Keymap<B> {
        Keymap {
            bindings: self
                .bindings
                .into_iter()
                .map(|(c, a)| (c, map(a)))
                .collect(),
        }
    }
    pub fn resolve(&self, event: &KeyEvent) -> Option<&A> {
        self.bindings
            .iter()
            .find(|(c, _)| c.matches(event))
            .map(|(_, a)| a)
    }
    pub fn into_bindings(self) -> impl Iterator<Item = (Chord, A)> {
        self.bindings.into_iter()
    }
}

/// Two declarations match the same initial event, including case-equivalent keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Conflict {
    pub chord: Chord,
}

impl std::fmt::Display for Conflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "overlapping keyboard binding: {:?}", self.chord)
    }
}

impl std::error::Error for Conflict {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Key, Modifiers, Phase};
    #[test]
    fn modifiers_phase_repeat_and_case() {
        let chord = Chord::new(Key::Character('k')).ctrl();
        let mut event = KeyEvent::pressed(Key::Character('K'), Modifiers::CONTROL);
        assert!(chord.matches(&event));
        event.modifiers = event.modifiers | Modifiers::SHIFT;
        assert!(!chord.matches(&event));
        event.modifiers = Modifiers::CONTROL;
        event.repeat = true;
        assert!(!chord.matches(&event));
        assert!(chord.repeat().matches(&event));
        event.repeat = false;
        event.phase = Phase::Release;
        assert!(!chord.matches(&event));
        assert!(chord.on_release().matches(&event));
    }
    #[test]
    fn composition_rejects_overlaps_and_maps_actions() {
        let c = Chord::new(Key::Character('k')).ctrl();
        let keys = Keymap::new().bind(c, 1).unwrap();
        assert!(keys.clone().bind(c.repeat(), 2).is_err());
        assert!(
            keys.clone()
                .merge(
                    Keymap::new()
                        .bind(Chord::new(Key::Character('K')).ctrl(), 2)
                        .unwrap()
                )
                .is_err()
        );
        let keys = keys.map(|n| n.to_string());
        assert_eq!(
            keys.resolve(&KeyEvent::pressed(Key::Character('k'), Modifiers::CONTROL))
                .map(String::as_str),
            Some("1")
        );
        assert!(!c.matches(&KeyEvent::pressed(
            Key::Character('k'),
            Modifiers::ALT_GRAPH
        )));
        assert!(Modifiers::from_bits(32).is_none());
    }
}
