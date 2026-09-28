//! Bench protocol (mirrored byte-for-byte in `spike/vapp-3/web/peer.js`).
//!
//! ONE ordered reliable data channel labelled `exp`, BINARY messages, first byte = opcode,
//! integers little-endian:
//!
//! ```text
//! 0x01 PING          [seq u32][client_ms f64]   → server replies 0x02 PONG with the identical payload
//! 0x02 PONG          [seq u32][client_ms f64]
//! 0x03 BODY_START    [total u64]                sender → receiver: a body begins
//! 0x04 BODY_CHUNK    [bytes...]                 ≤ 16 KiB payload
//! 0x05 BODY_END                                 → receiver replies 0x06 BODY_ACK [received u64]
//! 0x06 BODY_ACK      [received u64]
//! 0x07 REQUEST_BODY  [total u64]                client asks the server to stream START/CHUNK/END;
//!                                               the client then replies BODY_ACK
//! 0x08 CLOSE                                    client is done; server stops serving
//! ```
//!
//! Client measurements: connect_ms = offer created → channel open; RTT = `pings` sequential
//! PING/PONG (p50/p95); up_mbps = BODY_START sent → BODY_ACK received for `body_bytes`;
//! down_mbps = REQUEST_BODY sent → BODY_END received. Senders respect backpressure (Rust: queued
//! bytes ≤ 1 MiB on top of str0m's 128 KiB SCTP buffer; browser: bufferedAmount ≤ 1 MiB, resume on
//! bufferedamountlow at 256 KiB).
use std::time::{Duration, Instant};

use crate::link::{BenchResult, LinkState, PeerError, PeerLink};

pub const PING: u8 = 0x01;
pub const PONG: u8 = 0x02;
pub const BODY_START: u8 = 0x03;
pub const BODY_CHUNK: u8 = 0x04;
pub const BODY_END: u8 = 0x05;
pub const BODY_ACK: u8 = 0x06;
pub const REQUEST_BODY: u8 = 0x07;
pub const CLOSE: u8 = 0x08;
pub const CHUNK: usize = 16 * 1024;

pub fn ping(seq: u32, client_ms: f64) -> Vec<u8> {
    let mut v = vec![PING];
    v.extend_from_slice(&seq.to_le_bytes());
    v.extend_from_slice(&client_ms.to_le_bytes());
    v
}
pub fn with_u64(op: u8, n: u64) -> Vec<u8> {
    let mut v = vec![op];
    v.extend_from_slice(&n.to_le_bytes());
    v
}
pub fn read_u64(m: &[u8]) -> Option<u64> {
    Some(u64::from_le_bytes(m.get(1..9)?.try_into().ok()?))
}
fn read_seq(m: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(m.get(1..5)?.try_into().ok()?))
}

/// Stream a body: START, CHUNKs, END (backpressure lives in `PeerLink::send`).
pub fn send_body(link: &PeerLink, total: u64) -> Result<(), PeerError> {
    link.send(&with_u64(BODY_START, total))?;
    let mut chunk = vec![0xA5u8; CHUNK + 1];
    chunk[0] = BODY_CHUNK;
    let mut left = total as usize;
    while left > 0 {
        let n = left.min(CHUNK);
        link.send(&chunk[..n + 1])?;
        left -= n;
    }
    link.send(&[BODY_END])
}

fn percentile(xs: &[f64], p: f64) -> f64 {
    if xs.is_empty() {
        return f64::NAN;
    }
    let mut s = xs.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    s[((p * (s.len() - 1) as f64) + 0.5).floor() as usize]
}

fn recv_op(link: &PeerLink, want: u8, timeout: Duration) -> Result<Vec<u8>, PeerError> {
    let deadline = Instant::now() + timeout;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(PeerError::Timeout);
        }
        match link.recv(left) {
            Some(m) if m.first() == Some(&want) => return Ok(m),
            Some(_) => continue,
            None => {
                if matches!(link.state(), LinkState::Closed | LinkState::Failed) {
                    return Err(link.failure().unwrap_or(PeerError::Closed));
                }
            }
        }
    }
}

pub(crate) fn run_client(link: &PeerLink, body_bytes: u64, pings: u32) -> Result<BenchResult, PeerError> {
    let connect_ms = link.wait_open(Duration::from_secs(20))?;
    let epoch = Instant::now();
    let mut r = BenchResult { connect_ms, body_bytes, ..Default::default() };
    // RTT
    let mut rtts = Vec::with_capacity(pings as usize);
    for seq in 0..pings {
        let t0 = Instant::now();
        link.send(&ping(seq, epoch.elapsed().as_secs_f64() * 1000.0))?;
        loop {
            let m = recv_op(link, PONG, Duration::from_secs(5))?;
            if read_seq(&m) == Some(seq) {
                break;
            }
        }
        rtts.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    r.rtt_p50_ms = percentile(&rtts, 0.5);
    r.rtt_p95_ms = percentile(&rtts, 0.95);
    // Upload
    if body_bytes > 0 {
        let t0 = Instant::now();
        send_body(link, body_bytes)?;
        let ack = recv_op(link, BODY_ACK, Duration::from_secs(60))?;
        let secs = t0.elapsed().as_secs_f64();
        if read_u64(&ack) != Some(body_bytes) {
            r.errors.push(format!("upload ack {:?} != {body_bytes}", read_u64(&ack)));
        }
        r.up_mbps = body_bytes as f64 * 8.0 / secs / 1e6;
        // Download
        let t0 = Instant::now();
        link.send(&with_u64(REQUEST_BODY, body_bytes))?;
        let mut received = 0u64;
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(PeerError::Timeout);
            }
            match link.recv(left) {
                Some(m) => match m.first() {
                    Some(&BODY_CHUNK) => received += (m.len() - 1) as u64,
                    Some(&BODY_END) => break,
                    _ => {}
                },
                None => {
                    if matches!(link.state(), LinkState::Closed | LinkState::Failed) {
                        return Err(link.failure().unwrap_or(PeerError::Closed));
                    }
                }
            }
        }
        let secs = t0.elapsed().as_secs_f64();
        link.send(&with_u64(BODY_ACK, received))?;
        if received != body_bytes {
            r.errors.push(format!("download {received} != {body_bytes}"));
        }
        r.down_mbps = received as f64 * 8.0 / secs / 1e6;
    }
    link.fill_paths(&mut r);
    Ok(r)
}

/// Round trip of one PING (used after an ICE restart). Returns milliseconds.
pub fn ping_once(link: &PeerLink, seq: u32, timeout: Duration) -> Result<f64, PeerError> {
    let t0 = Instant::now();
    link.send(&ping(seq, 0.0))?;
    loop {
        let m = recv_op(link, PONG, timeout)?;
        if read_seq(&m) == Some(seq) {
            return Ok(t0.elapsed().as_secs_f64() * 1000.0);
        }
    }
}

pub(crate) fn serve(link: &PeerLink) -> Result<(), PeerError> {
    link.wait_open(Duration::from_secs(30))?;
    let mut receiving: Option<u64> = None;
    loop {
        let Some(m) = link.recv(Duration::from_millis(500)) else {
            match link.state() {
                LinkState::Closed => return Ok(()),
                LinkState::Failed => return Err(link.failure().unwrap_or(PeerError::Closed)),
                _ => continue,
            }
        };
        match m.first().copied() {
            Some(PING) => {
                let mut p = m.clone();
                p[0] = PONG;
                link.send(&p)?;
            }
            Some(BODY_START) => receiving = Some(0),
            Some(BODY_CHUNK) => {
                if let Some(n) = receiving.as_mut() {
                    *n += (m.len() - 1) as u64;
                }
            }
            Some(BODY_END) => {
                let n = receiving.take().unwrap_or(0);
                link.send(&with_u64(BODY_ACK, n))?;
            }
            Some(REQUEST_BODY) => {
                let total = read_u64(&m).unwrap_or(0);
                send_body(link, total)?;
            }
            Some(BODY_ACK) => {}
            Some(CLOSE) => return Ok(()),
            _ => {}
        }
    }
}
