/// A list navigation target, resolved within the current component instance.
/// Attach it to a list with [`List::target`](crate::ui::List::target) and pass
/// the same value to [`Field::navigate`](crate::ui::Field::navigate).
/// The field and list must be siblings in the same component scope.
///
/// ```
/// use omega::ui::{Column, Field, List, ListTarget};
/// const RESULTS: ListTarget = ListTarget::new("results");
/// let view = Column::new()
///     .child(Field::new("query").navigate(RESULTS))
///     .child(List::new().target(RESULTS));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ListTarget(&'static str);
impl ListTarget {
    /// Declare a nonempty ASCII identifier using letters, digits, `_` and `-`.
    /// Panics for invalid identifiers; const declarations fail during compilation.
    pub const fn new(name: &'static str) -> Self {
        assert!(!name.is_empty(), "list target must not be empty");
        let bytes = name.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            let b = bytes[index];
            assert!(
                b.is_ascii_alphanumeric() || b == b'_' || b == b'-',
                "invalid list target"
            );
            index += 1;
        }
        Self(name)
    }
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}
impl std::fmt::Display for ListTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
