/// Presentation intent delivered to the instance's serialized behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifecycle {
    Presented,
    Hidden,
    Closed,
}
