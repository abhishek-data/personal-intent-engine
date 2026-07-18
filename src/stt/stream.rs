use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::Duration;

/// How long `finalize` waits for the worker to flush and reply. Streaming
/// decoders may need to drain buffered audio, so this is generous.
pub const STREAM_FINALIZE_REPLY_TIMEOUT: Duration = Duration::from_secs(30);

/// Commands sent to the streaming worker thread. Audio frames and the finalize
/// request travel the same channel so FIFO ordering guarantees every fed frame
/// is processed before finalize runs.
pub enum StreamCmd {
    /// A 16 kHz mono frame to feed to the incremental decoder.
    Feed(Vec<f32>),
    /// Flush the stream and reply with the final text, or `None` if no stream
    /// was ever active (caller should fall back to batch transcription).
    Finalize(mpsc::Sender<Option<String>>),
    /// Abort the stream and discard any in-flight audio.
    Cancel,
}

/// Routes real-time audio frames to an active streaming worker.
///
/// Shared between the session owner (opens/closes the route) and the audio
/// recorder's per-frame callback (feeds frames). The recorder holds an
/// `Arc<StreamRouter>` directly, so a frame with no stream pending costs a
/// single relaxed atomic load — no mutex lock. (zero-overhead feed pattern;
/// the atomic-first check is the point, don't reorder it.)
pub struct StreamRouter {
    /// Command channel to the active streaming worker.
    tx: Mutex<Option<mpsc::Sender<StreamCmd>>>,
    /// True while a stream is pending or active (channel is open). The audio
    /// callback checks this first to avoid the mutex lock when no stream runs.
    open: AtomicBool,
}

impl Default for StreamRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamRouter {
    /// Create a closed router; call [`StreamRouter::open`] to begin a session.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tx: Mutex::new(None),
            open: AtomicBool::new(false),
        }
    }

    /// Open a fresh command channel for a new streaming session, returning the
    /// receiver the worker should drain. Caller must ensure no prior channel
    /// is still open.
    pub fn open(&self) -> mpsc::Receiver<StreamCmd> {
        let (tx, rx) = mpsc::channel::<StreamCmd>();
        *self.tx.lock().expect("stream router poisoned") = Some(tx);
        self.open.store(true, Ordering::Relaxed);
        rx
    }

    /// Take the sender out (closing the channel to new feeds). Returns the
    /// sender so the caller can send the final `Finalize`/`Cancel` command.
    pub fn take(&self) -> Option<mpsc::Sender<StreamCmd>> {
        self.open.store(false, Ordering::Relaxed);
        self.tx.lock().expect("stream router poisoned").take()
    }

    /// Drop the channel and mark closed without sending a final command (used
    /// when the worker exits without a finalize/cancel handshake).
    pub fn clear(&self) {
        self.open.store(false, Ordering::Relaxed);
        *self.tx.lock().expect("stream router poisoned") = None;
    }

    /// Forward a 16 kHz frame to the active streaming worker. Cheap no-op (a
    /// single relaxed atomic load) when no stream is pending.
    pub fn feed(&self, frame: &[f32]) {
        if !self.open.load(Ordering::Relaxed) {
            return;
        }
        if let Some(tx) = self.tx.lock().expect("stream router poisoned").as_ref() {
            let _ = tx.send(StreamCmd::Feed(frame.to_vec()));
        }
    }

    /// Whether a stream is pending or active.
    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::Relaxed)
    }

    /// Close the route and ask the worker to flush. `Ok(Some(text))` is the
    /// final transcript; `Ok(None)` means no usable stream was active and the
    /// caller should fall back to batch transcription.
    pub fn finalize(&self) -> anyhow::Result<Option<String>> {
        let Some(tx) = self.take() else {
            return Ok(None);
        };
        let (reply_tx, reply_rx) = mpsc::channel();
        if tx.send(StreamCmd::Finalize(reply_tx)).is_err() {
            return Ok(None);
        }
        match reply_rx.recv_timeout(STREAM_FINALIZE_REPLY_TIMEOUT) {
            Ok(result) => Ok(result),
            Err(mpsc::RecvTimeoutError::Disconnected) => Ok(None),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                anyhow::bail!("Timed out waiting for stream finalize")
            }
        }
    }

    /// Close the route and discard the in-flight stream.
    pub fn cancel(&self) {
        if let Some(tx) = self.take() {
            let _ = tx.send(StreamCmd::Cancel);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feed_is_noop_when_closed() {
        let router = StreamRouter::new();
        assert!(!router.is_open());
        router.feed(&[0.0; 480]); // must not panic or block
    }

    #[test]
    fn feed_delivers_frames_in_order_while_open() {
        let router = StreamRouter::new();
        let rx = router.open();
        assert!(router.is_open());

        router.feed(&[0.1; 4]);
        router.feed(&[0.2; 4]);

        for expected in [0.1f32, 0.2] {
            match rx.try_recv() {
                Ok(StreamCmd::Feed(frame)) => assert_eq!(frame[0], expected),
                _ => panic!("expected Feed command"),
            }
        }
    }

    #[test]
    fn take_closes_the_route() {
        let router = StreamRouter::new();
        let rx = router.open();
        let tx = router.take();
        assert!(tx.is_some());
        assert!(!router.is_open());

        router.feed(&[0.5; 4]); // dropped, route is closed
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn finalize_without_stream_returns_none() {
        let router = StreamRouter::new();
        assert!(matches!(router.finalize(), Ok(None)));
    }

    #[test]
    fn finalize_round_trips_worker_reply() {
        let router = StreamRouter::new();
        let rx = router.open();

        let worker = std::thread::spawn(move || {
            while let Ok(cmd) = rx.recv() {
                match cmd {
                    StreamCmd::Feed(_) => {}
                    StreamCmd::Finalize(reply) => {
                        let _ = reply.send(Some("final text".to_string()));
                        break;
                    }
                    StreamCmd::Cancel => break,
                }
            }
        });

        router.feed(&[0.1; 4]);
        let result = router.finalize().unwrap();
        assert_eq!(result.as_deref(), Some("final text"));
        worker.join().unwrap();
    }
}
