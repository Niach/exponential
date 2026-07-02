//! `steer` — the relay publisher (masterplan-v3 §3.1 / §08).
//!
//! Phase 6 lands: `control_channel.rs` (device presence + inbound
//! `start_session`) and `publisher.rs` (tee out, inject, resize, claim/kill,
//! ring replay, auto-reconnect) over tokio-tungstenite + rustls (ws and wss).
//! Wire protocol and ticket format are frozen (`packages/steer-ticket`,
//! `apps/steer-relay`).
