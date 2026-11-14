//! #LOGS-2: envelopes go to `POST <server>/api/monitoring` (the backend's
//! Sentry tunnel), never to the GlitchTip host itself: one background
//! thread, a bounded queue (full = dropped, never blocking a caller), the
//! DSN stamped into the envelope header (the tunnel routes by it) and a
//! final scrub pass over every item.
//!
//! #ERR-18: an event counts as delivered only on a 2xx (a 5xx, a 429 or a
//! network error is not), and a flush never blocks past its timeout on a
//! full queue: a panic's event waits on disk until [`delivered`] says so.

use std::collections::VecDeque;
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use sentry::protocol::EnvelopeItem;
use sentry::types::{Dsn, Uuid};
use sentry::{ClientOptions, Envelope, Transport};

/// The app's one HTTP client (`api::http::shared`).
pub type HttpClient = fn() -> reqwest::blocking::Client;

/// The event ids the tunnel last answered 2xx for (the newest).
static DELIVERED: Mutex<VecDeque<Uuid>> = Mutex::new(VecDeque::new());
const DELIVERED_KEPT: usize = 256;

fn record_delivered(id: Uuid) {
    let mut delivered = DELIVERED.lock().unwrap_or_else(|e| e.into_inner());
    if delivered.len() == DELIVERED_KEPT {
        delivered.pop_front();
    }
    delivered.push_back(id);
}

/// Did the tunnel take the event `id` (a 2xx)?
pub fn delivered(id: &Uuid) -> bool {
    DELIVERED.try_lock().is_ok_and(|delivered| delivered.contains(id))
}

/// Wait (bounded) until every event of `ids` was delivered.
pub fn wait_delivered(ids: &[Uuid], timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if ids.iter().all(delivered) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

enum Message {
    Envelope(Envelope),
    Flush(SyncSender<()>),
}

pub struct TunnelTransport {
    sender: Mutex<Option<SyncSender<Message>>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl TunnelTransport {
    pub fn new(tunnel_url: String, dsn: Dsn, http: HttpClient) -> Self {
        let (sender, receiver) = sync_channel(64);
        let thread = std::thread::Builder::new()
            .name("exp-diagnostics".into())
            .spawn(move || run(receiver, tunnel_url, dsn, http))
            .ok();
        Self { sender: Mutex::new(Some(sender)), thread: Mutex::new(thread) }
    }

    /// The `ClientOptions::transport` factory.
    pub fn factory(tunnel_url: String, http: HttpClient) -> Arc<dyn sentry::TransportFactory> {
        Arc::new(move |options: &ClientOptions| -> Arc<dyn Transport> {
            let dsn = options.dsn.clone().expect("a client is only built with a DSN");
            Arc::new(TunnelTransport::new(tunnel_url.clone(), dsn, http))
        })
    }
}

impl Transport for TunnelTransport {
    fn send_envelope(&self, envelope: Envelope) {
        if let Some(sender) = self.sender.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            match sender.try_send(Message::Envelope(envelope)) {
                Ok(()) | Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
            }
        }
    }

    fn flush(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let (done_tx, done_rx) = sync_channel(1);
        let Some(sender) = self.sender.lock().unwrap_or_else(|e| e.into_inner()).clone() else { return false };
        // A full queue waits for room until the deadline, never past it.
        let mut message = Message::Flush(done_tx);
        loop {
            match sender.try_send(message) {
                Ok(()) => break,
                Err(TrySendError::Full(back)) if Instant::now() < deadline => {
                    message = back;
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => return false,
            }
        }
        done_rx.recv_timeout(deadline.saturating_duration_since(Instant::now())).is_ok()
    }

    fn shutdown(&self, timeout: Duration) -> bool {
        let flushed = self.flush(timeout);
        self.sender.lock().unwrap_or_else(|e| e.into_inner()).take();
        if flushed {
            if let Some(thread) = self.thread.lock().unwrap_or_else(|e| e.into_inner()).take() {
                let _ = thread.join();
            }
        }
        flushed
    }
}

fn run(receiver: Receiver<Message>, url: String, dsn: Dsn, http: HttpClient) {
    let mut paused_until: Option<Instant> = None;
    loop {
        let message = match receiver.recv_timeout(Duration::from_secs(60)) {
            Ok(message) => message,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        let envelope = match message {
            Message::Flush(done) => {
                let _ = done.send(());
                continue;
            }
            Message::Envelope(envelope) => envelope,
        };
        // 429 / Retry-After: drop until the window passes.
        if paused_until.is_some_and(|until| Instant::now() < until) {
            continue;
        }
        let event_id = envelope.uuid().copied();
        let body = match encode(envelope, &dsn) {
            Some(body) => body,
            None => continue,
        };
        let response = http()
            .post(&url)
            .header("content-type", "application/x-sentry-envelope")
            .timeout(Duration::from_secs(15))
            .body(body)
            .send();
        if let Ok(response) = response {
            if response.status().is_success() {
                if let Some(id) = event_id {
                    record_delivered(id);
                }
            }
            if response.status().as_u16() == 429 {
                let retry = response
                    .headers()
                    .get("retry-after")
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.trim().parse::<u64>().ok())
                    .unwrap_or(60);
                paused_until = Some(Instant::now() + Duration::from_secs(retry.min(3600)));
            }
        }
    }
}

/// The envelope as it goes on the wire: scrubbed, DSN in the header.
pub fn encode(envelope: Envelope, dsn: &Dsn) -> Option<Vec<u8>> {
    let headers = envelope.headers().clone().with_dsn(dsn.clone());
    let mut scrubbed = Envelope::new().with_headers(headers);
    for item in envelope.into_items() {
        let item = match item {
            EnvelopeItem::Event(event) => EnvelopeItem::Event(Box::new(crate::scrub::scrub_event(*event))),
            EnvelopeItem::Transaction(transaction) => EnvelopeItem::Transaction(Box::new(crate::scrub::scrub_transaction(*transaction))),
            other => other,
        };
        scrubbed.add_item(item);
    }
    let mut body = Vec::new();
    scrubbed.to_writer(&mut body).ok()?;
    // #ERR-12: debug images in the shape GlitchTip symbolicates.
    Some(crash_monitor::native_debug_images(body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentry::protocol::Event;

    #[test]
    fn the_wire_envelope_names_the_dsn_and_is_scrubbed() {
        let dsn: Dsn = "https://publickey@glitchtip.example/7".parse().unwrap();
        let mut envelope = Envelope::new();
        envelope.add_item(Event { message: Some("Bearer abc".into()), server_name: Some("host".into()), ..Default::default() });
        let body = String::from_utf8(encode(envelope, &dsn).unwrap()).unwrap();
        let header = body.lines().next().unwrap();
        assert!(header.contains(r#""dsn":"https://publickey"#) && header.contains("glitchtip.example/7"), "{header}");
        assert!(body.contains("Bearer [Filtered]"));
        assert!(!body.contains("\"server_name\""));
    }

    #[test]
    fn only_recorded_ids_count_as_delivered() {
        let (taken, other) = (Uuid::new_v4(), Uuid::new_v4());
        record_delivered(taken);
        assert!(delivered(&taken));
        assert!(!delivered(&other));
        assert!(wait_delivered(&[taken], Duration::ZERO));
        assert!(!wait_delivered(&[taken, other], Duration::from_millis(30)));
    }

    #[test]
    fn a_flush_on_a_stuck_queue_returns_within_its_timeout() {
        // A sender whose thread never drains: the queue fills, then a flush
        // must give up at its deadline instead of blocking on the send.
        let (sender, _receiver) = sync_channel(1);
        sender.try_send(Message::Flush(sync_channel(1).0)).ok();
        let transport = TunnelTransport { sender: Mutex::new(Some(sender)), thread: Mutex::new(None) };
        let started = Instant::now();
        assert!(!transport.flush(Duration::from_millis(50)));
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
