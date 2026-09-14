//! Stream-ID allocation: daemon requests use even IDs; peer requests use odd IDs.

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
