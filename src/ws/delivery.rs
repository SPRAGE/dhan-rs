//! Bounded delivery for the supervised standard market feed.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::SystemTime;

use tokio::sync::{Notify, broadcast, mpsc};

use super::market_feed::MarketFeedEvent;

/// Selects the authoritative delivery path. Raw frames are optional observers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MarketFeedDelivery {
    /// Broadcast individual events through managed receivers.
    #[default]
    Events,
    /// Deliver each fully validated binary message to one bounded consumer.
    Batches,
}

#[derive(Default)]
pub(crate) struct ConsumerLag {
    pub(crate) dropped: AtomicU64,
    pub(crate) notify: Notify,
}

impl ConsumerLag {
    fn report(&self, dropped: u64) {
        self.dropped.fetch_add(dropped, Ordering::Relaxed);
        self.notify.notify_one();
    }
}

/// Broadcast receiver that reports actual overflow to the connection owner.
/// The error and its exact dropped count are also returned to the caller.
pub struct ManagedMarketFeedReceiver<T: Clone> {
    pub(crate) rx: broadcast::Receiver<T>,
    pub(crate) lag: Option<Arc<ConsumerLag>>,
}

impl<T: Clone> ManagedMarketFeedReceiver<T> {
    /// Receive one event, preserving Tokio's cancellation-safe behavior.
    pub async fn recv(&mut self) -> Result<T, broadcast::error::RecvError> {
        let result = self.rx.recv().await;
        if let Err(broadcast::error::RecvError::Lagged(dropped)) = &result {
            if let Some(lag) = &self.lag {
                lag.report(*dropped);
            }
        }
        result
    }

    /// Receive without waiting and report any actual overflow.
    pub fn try_recv(&mut self) -> Result<T, broadcast::error::TryRecvError> {
        let result = self.rx.try_recv();
        if let Err(broadcast::error::TryRecvError::Lagged(dropped)) = &result {
            if let Some(lag) = &self.lag {
                lag.report(*dropped);
            }
        }
        result
    }

    /// Subscribe at the current tail with the same health-reporting behavior.
    pub fn resubscribe(&self) -> Self {
        Self {
            rx: self.rx.resubscribe(),
            lag: self.lag.clone(),
        }
    }

    /// Number of events available to this receiver.
    pub fn len(&self) -> usize {
        self.rx.len()
    }

    /// Whether this receiver currently has no queued events.
    pub fn is_empty(&self) -> bool {
        self.rx.is_empty()
    }
}

pub(crate) struct BatchState {
    bytes: AtomicUsize,
    pub(crate) claimed: AtomicBool,
    dropped: AtomicU64,
    notify: Notify,
}

impl Default for BatchState {
    fn default() -> Self {
        Self {
            bytes: AtomicUsize::new(0),
            claimed: AtomicBool::new(false),
            dropped: AtomicU64::new(0),
            notify: Notify::new(),
        }
    }
}

/// One completely validated WebSocket binary message, in packet order.
/// Dropping the batch releases its byte reservation and recycles its buffer.
pub struct MarketFeedBatch {
    events: Vec<MarketFeedEvent>,
    /// Local receipt time; this is not an exchange timestamp.
    pub received_at: SystemTime,
    state: Arc<BatchState>,
    reserved_bytes: usize,
    recycle: mpsc::Sender<Vec<MarketFeedEvent>>,
}

impl MarketFeedBatch {
    /// Borrow the complete ordered packet sequence.
    pub fn events(&self) -> &[MarketFeedEvent] {
        &self.events
    }

    /// Consume events without cloning them. Keep one runtime lease for the drain.
    pub fn drain_events(&mut self) -> std::vec::Drain<'_, MarketFeedEvent> {
        self.events.drain(..)
    }
}

impl Drop for MarketFeedBatch {
    fn drop(&mut self) {
        self.state
            .bytes
            .fetch_sub(self.reserved_bytes, Ordering::AcqRel);
        self.events.clear();
        // One spare buffer per connection, rather than a second unbounded pool.
        let _ = self.recycle.try_send(std::mem::take(&mut self.events));
    }
}

/// Single authoritative batch consumer. Overflow is explicit and recoverable;
/// only channel closure terminates the stream.
pub struct MarketFeedBatchReceiver {
    pub(crate) rx: mpsc::Receiver<MarketFeedBatch>,
    pub(crate) state: Arc<BatchState>,
}

impl MarketFeedBatchReceiver {
    /// Receive the next batch or the exact number of packets lost to overflow.
    pub async fn recv(&mut self) -> Result<MarketFeedBatch, broadcast::error::RecvError> {
        loop {
            let dropped = self.state.dropped.swap(0, Ordering::AcqRel);
            if dropped > 0 {
                return Err(broadcast::error::RecvError::Lagged(dropped));
            }
            tokio::select! {
                batch = self.rx.recv() => return batch.ok_or(broadcast::error::RecvError::Closed),
                _ = self.state.notify.notified() => {}
            }
        }
    }

    /// Bytes reserved by queued batches and batches still held by the consumer.
    pub fn reserved_bytes(&self) -> usize {
        self.state.bytes.load(Ordering::Acquire)
    }
}

#[derive(Clone)]
pub(crate) struct BatchSender {
    tx: mpsc::Sender<MarketFeedBatch>,
    pub(crate) state: Arc<BatchState>,
    byte_capacity: usize,
    recycle: mpsc::Sender<Vec<MarketFeedEvent>>,
}

pub(crate) enum BatchSendError {
    NoReceiver,
    Overflow(u64),
}

impl BatchSender {
    pub(crate) fn channel(
        capacity: usize,
        byte_capacity: usize,
    ) -> (
        Self,
        MarketFeedBatchReceiver,
        mpsc::Receiver<Vec<MarketFeedEvent>>,
    ) {
        let (tx, rx) = mpsc::channel(capacity.max(1));
        let (recycle, recycled) = mpsc::channel(1);
        let state = Arc::new(BatchState::default());
        (
            Self {
                tx,
                state: state.clone(),
                byte_capacity,
                recycle,
            },
            MarketFeedBatchReceiver { rx, state },
            recycled,
        )
    }

    pub(crate) fn send(
        &self,
        events: &mut Vec<MarketFeedEvent>,
        received_at: SystemTime,
    ) -> Result<(), BatchSendError> {
        if !self.state.claimed.load(Ordering::Acquire) || self.tx.is_closed() {
            return Err(BatchSendError::NoReceiver);
        }
        let count = events.len() as u64;
        let bytes = events.capacity() * std::mem::size_of::<MarketFeedEvent>()
            + events
                .iter()
                .map(|event| match event {
                    MarketFeedEvent::Index { raw, .. }
                    | MarketFeedEvent::MarketStatus { raw, .. } => raw.capacity(),
                    _ => 0,
                })
                .sum::<usize>();
        if bytes > self.byte_capacity {
            // An oversized allocation must not poison future small messages.
            // Temporary pressure from consumer-held batches may retain reusable
            // scratch, but an intrinsically oversized vector must be released.
            *events = Vec::new();
            self.overflow(count);
            return Err(BatchSendError::Overflow(count));
        }
        if self
            .state
            .bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(bytes)
                    .filter(|total| *total <= self.byte_capacity)
            })
            .is_err()
        {
            self.overflow(count);
            return Err(BatchSendError::Overflow(count));
        }
        let batch = MarketFeedBatch {
            events: std::mem::take(events),
            received_at,
            state: self.state.clone(),
            reserved_bytes: bytes,
            recycle: self.recycle.clone(),
        };
        match self.tx.try_send(batch) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.overflow(count);
                Err(BatchSendError::Overflow(count))
            }
            Err(mpsc::error::TrySendError::Closed(_)) => Err(BatchSendError::NoReceiver),
        }
    }

    fn overflow(&self, count: u64) {
        self.state.dropped.fetch_add(count, Ordering::Relaxed);
        self.state.notify.notify_one();
    }
}
