//! Stream ids, split between the two ends of a connection.
//!
//! Both peers start requests on the same connection, and a `Result` is
//! answered by whoever allocated the stream it arrives on — so the two ends
//! must never pick the same id. They are split by parity: the daemon's
//! requests are even, a unit's or an operator's are odd. Nothing enforces
//! this on the wire, so it is enforced here, where both sides get it from.

/// The daemon's half: even ids.
#[derive(Debug, Default)]
pub struct DaemonStreams {
    next: u64,
}

impl DaemonStreams {
    pub fn new() -> Self {
        Self::default()
    }

    /// The id for the daemon's next request on this connection.
    pub fn allocate(&mut self) -> u64 {
        self.next += 2;
        self.next
    }

    /// Whether a `Result` on this stream answers something the daemon asked.
    pub fn is_ours(stream_id: u64) -> bool {
        stream_id != 0 && stream_id.is_multiple_of(2)
    }
}

/// The other end's half: odd ids.
#[derive(Debug, Default)]
pub struct PeerStreams {
    next: u64,
}

impl PeerStreams {
    pub fn new() -> Self {
        Self::default()
    }

    /// The id for this peer's next request.
    pub fn allocate(&mut self) -> u64 {
        self.next += 2;
        self.next - 1
    }

    /// Whether a `Result` on this stream answers something this peer asked.
    pub fn is_ours(stream_id: u64) -> bool {
        stream_id % 2 == 1
    }
}
