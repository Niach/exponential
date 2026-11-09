//! The one way out (#LOGS-2): every envelope is POSTed to the Exponential
//! server's Sentry tunnel (`<server>/api/monitoring`), never to the
//! collector itself. A worker thread sends them (the caller never blocks on
//! the network); a full queue drops, a failed POST is dropped too (no
//! retries: diagnostics never compete with the desktop).

use std::sync::{
    mpsc::{sync_channel, SyncSender, TrySendError},
    Arc, Condvar, Mutex,
};
use std::time::{Duration, Instant};

use crate::config::Target;

/// Envelopes queued before new ones drop.
const QUEUE: usize = 64;

pub struct Tunnel {
    tx: Mutex<Option<SyncSender<Vec<u8>>>>,
    /// Envelopes queued or in flight, for [`sentry::Transport::flush`].
    pending: Arc<(Mutex<usize>, Condvar)>,
    dsn: String,
}

impl Tunnel {
    pub fn new(target: Target) -> Self {
        let (tx, rx) = sync_channel::<Vec<u8>>(QUEUE);
        let pending = Arc::new((Mutex::new(0usize), Condvar::new()));
        let done = pending.clone();
        let url = target.url.clone();
        let spawned = std::thread::Builder::new().name("ede-sentry-tunnel".into()).spawn(move || {
            let client = reqwest::blocking::Client::builder().timeout(Duration::from_secs(15)).user_agent(crate::RELEASE).build().ok();
            for body in rx {
                if let Some(client) = &client {
                    let sent = client.post(&url).header("content-type", "application/x-sentry-envelope").body(body).send();
                    // Never through tracing: an error here would be an event
                    // about sending events.
                    if let Err(e) = sent {
                        if std::env::var_os("EXP_SENTRY_DEBUG").is_some() {
                            eprintln!("ede-observability: tunnel: {}", e.without_url());
                        }
                    }
                }
                let (count, cv) = &*done;
                let mut n = count.lock().unwrap_or_else(|e| e.into_inner());
                *n = n.saturating_sub(1);
                cv.notify_all();
            }
        });
        Self { tx: Mutex::new(spawned.ok().map(|_| tx)), pending, dsn: target.dsn }
    }
}

impl sentry::Transport for Tunnel {
    fn send_envelope(&self, envelope: sentry::Envelope) {
        let mut body = Vec::new();
        if envelope.to_writer(&mut body).is_err() {
            return;
        }
        let body = with_dsn_header(body, &self.dsn);
        let guard = self.tx.lock().unwrap_or_else(|e| e.into_inner());
        let Some(tx) = guard.as_ref() else { return };
        let (count, _) = &*self.pending;
        *count.lock().unwrap_or_else(|e| e.into_inner()) += 1;
        if let Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) = tx.try_send(body) {
            let mut n = count.lock().unwrap_or_else(|e| e.into_inner());
            *n = n.saturating_sub(1);
        }
    }

    fn flush(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let (count, cv) = &*self.pending;
        let mut n = count.lock().unwrap_or_else(|e| e.into_inner());
        while *n > 0 {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            n = cv.wait_timeout(n, left).unwrap_or_else(|e| e.into_inner()).0;
        }
        true
    }

    fn shutdown(&self, timeout: Duration) -> bool {
        let flushed = self.flush(timeout);
        // The worker ends once the queue is drained.
        self.tx.lock().unwrap_or_else(|e| e.into_inner()).take();
        flushed
    }
}

/// The envelope's first line (its JSON header) with `dsn` set, which the
/// tunnel reads to name the project; the body is left as it is.
pub fn with_dsn_header(body: Vec<u8>, dsn: &str) -> Vec<u8> {
    let split = body.iter().position(|b| *b == b'\n').unwrap_or(body.len());
    let Ok(serde_json::Value::Object(mut head)) = serde_json::from_slice::<serde_json::Value>(&body[..split]) else {
        return body;
    };
    head.entry("dsn").or_insert_with(|| serde_json::Value::String(dsn.to_owned()));
    let Ok(mut out) = serde_json::to_vec(&serde_json::Value::Object(head)) else {
        return body;
    };
    out.extend_from_slice(&body[split..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_header_names_the_dsn_and_keeps_the_items() {
        let body = b"{\"event_id\":\"9ec79c33ec9942ab8353589fcb2e04dc\"}\n{\"type\":\"event\",\"length\":2}\n{}\n".to_vec();
        let out = String::from_utf8(with_dsn_header(body, "https://k@g.example/2")).unwrap();
        let (head, rest) = out.split_once('\n').unwrap();
        let head: serde_json::Value = serde_json::from_str(head).unwrap();
        assert_eq!(head["dsn"], "https://k@g.example/2");
        assert_eq!(head["event_id"], "9ec79c33ec9942ab8353589fcb2e04dc");
        assert_eq!(rest, "{\"type\":\"event\",\"length\":2}\n{}\n");
    }

    #[test]
    fn an_existing_dsn_header_stays() {
        let out = with_dsn_header(b"{\"dsn\":\"https://a@b/1\"}\n".to_vec(), "https://k@g.example/2");
        assert!(String::from_utf8(out).unwrap().starts_with("{\"dsn\":\"https://a@b/1\"}\n"));
    }

    #[test]
    fn an_empty_header_line_is_still_an_object() {
        let out = String::from_utf8(with_dsn_header(b"{}\n".to_vec(), "https://k@g.example/2")).unwrap();
        assert!(out.starts_with("{\"dsn\":\"https://k@g.example/2\"}\n"));
    }

    #[test]
    fn a_flush_with_nothing_queued_returns_at_once() {
        // An unroutable URL: nothing is ever sent in this test.
        let t = Tunnel::new(Target { url: "http://127.0.0.1:9/api/monitoring".into(), dsn: "https://k@g.example/2".into() });
        assert!(sentry::Transport::flush(&t, Duration::from_millis(10)));
        assert!(sentry::Transport::shutdown(&t, Duration::from_millis(10)));
    }
}
