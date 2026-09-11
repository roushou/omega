//! A byte- and count-bounded broadcast log. Eviction makes slow readers lag.
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::sync::{
    broadcast::error::{RecvError, TryRecvError},
    watch,
};

#[derive(Debug)]
struct Log<T> {
    entries: VecDeque<(T, usize)>,
    bytes: usize,
    next: u64,
}

#[derive(Debug)]
pub(super) struct History<T> {
    log: Arc<Mutex<Log<T>>>,
    changed: watch::Sender<()>,
}

#[derive(Debug)]
pub struct Receiver<T> {
    log: Arc<Mutex<Log<T>>>,
    changed: watch::Receiver<()>,
    next: u64,
}

impl<T: Clone> History<T> {
    pub(super) const BYTES: usize = 8 * 1024 * 1024;
    const COUNT: usize = 64;

    pub(super) fn new() -> Self {
        let (changed, _) = watch::channel(());
        Self {
            log: Arc::new(Mutex::new(Log {
                entries: VecDeque::new(),
                bytes: 0,
                next: 0,
            })),
            changed,
        }
    }

    // Publishers validate individual sizes before committing authoritative state.
    pub(super) fn send(&self, value: T, size: usize) {
        assert!(size <= Self::BYTES);
        let mut log = self.log.lock().unwrap_or_else(|e| e.into_inner());
        while log.entries.len() >= Self::COUNT || log.bytes + size > Self::BYTES {
            let (_, size) = log
                .entries
                .pop_front()
                .expect("history contains evictable entries");
            log.bytes -= size;
        }
        log.entries.push_back((value, size));
        log.bytes += size;
        log.next = log
            .next
            .checked_add(1)
            .expect("broadcast sequence exhausted");
        self.changed.send_replace(());
    }

    pub(super) fn subscribe(&self) -> Receiver<T> {
        let log = self.log.lock().unwrap_or_else(|e| e.into_inner());
        Receiver {
            log: self.log.clone(),
            changed: self.changed.subscribe(),
            next: log.next,
        }
    }
}

impl<T: Clone> Receiver<T> {
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        let log = self.log.lock().unwrap_or_else(|e| e.into_inner());
        let oldest = log.next - log.entries.len() as u64;
        if self.next < oldest {
            let missed = oldest - self.next;
            self.next = oldest;
            return Err(TryRecvError::Lagged(missed));
        }
        if self.next < log.next {
            let value = log.entries[(self.next - oldest) as usize].0.clone();
            self.next += 1;
            return Ok(value);
        }
        if self.changed.has_changed().is_err() {
            Err(TryRecvError::Closed)
        } else {
            Err(TryRecvError::Empty)
        }
    }

    pub async fn recv(&mut self) -> Result<T, RecvError> {
        loop {
            match self.try_recv() {
                Ok(value) => return Ok(value),
                Err(TryRecvError::Closed) => return Err(RecvError::Closed),
                Err(TryRecvError::Lagged(missed)) => return Err(RecvError::Lagged(missed)),
                Err(TryRecvError::Empty) => {}
            }
            // A permit remains if publication races with the read above.
            let _ = self.changed.changed().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn lagged_receivers_drain_retained_entries_after_publication_closes() {
        let history = History::new();
        let mut receiver = history.subscribe();
        for value in 0..66 {
            history.send(value, 1);
        }
        let mut late = history.subscribe();
        drop(history);
        assert_eq!(late.recv().await, Err(RecvError::Closed));
        assert_eq!(receiver.recv().await, Err(RecvError::Lagged(2)));
        for value in 2..66 {
            assert_eq!(receiver.recv().await, Ok(value));
        }
        assert_eq!(receiver.recv().await, Err(RecvError::Closed));
    }

    #[tokio::test]
    async fn byte_eviction_reports_exact_lag_and_keeps_the_newest_values() {
        let history = History::new();
        let mut receiver = history.subscribe();
        for value in 0..4 {
            history.send(value, 3 * 1024 * 1024);
        }
        assert_eq!(history.log.lock().unwrap().bytes, 6 * 1024 * 1024);
        assert_eq!(receiver.recv().await, Err(RecvError::Lagged(2)));
        assert_eq!(receiver.recv().await, Ok(2));
        assert_eq!(receiver.recv().await, Ok(3));
        drop(history);
        assert_eq!(receiver.recv().await, Err(RecvError::Closed));
    }

    #[tokio::test]
    async fn cancelled_waits_preserve_delivery_and_count_eviction_is_bounded() {
        let history = History::new();
        let mut receiver = history.subscribe();
        tokio::select! {
            biased;
            _ = receiver.recv() => panic!("empty history answered"),
            _ = tokio::task::yield_now() => {}
        }
        for value in 0..65 {
            history.send(value, 1);
        }
        assert_eq!(receiver.recv().await, Err(RecvError::Lagged(1)));
        assert_eq!(receiver.recv().await, Ok(1));
    }
}
