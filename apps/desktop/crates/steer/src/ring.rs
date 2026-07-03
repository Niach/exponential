//! The 256 KiB replay ring (masterplan-v3 §8.4) — a mirror of the relay's
//! `RingBuffer` (`apps/steer-relay/src/hub.ts`, `RING_CAP_BYTES`). The
//! publisher pushes every teed chunk; on `resync` (the slow-consumer recovery
//! path — NOT viewer join, NOT reconnect) it resends the ring as `0x01`
//! frames.

use std::collections::VecDeque;

/// Mirror of the relay's `RING_CAP_BYTES = 256 * 1024`.
pub const RING_CAP_BYTES: usize = 256 * 1024;

/// Chunk-granular byte ring: evicts whole oldest chunks once the total
/// exceeds the cap, but always keeps at least one chunk (hub.ts:
/// `while (total > cap && chunks.length > 1)`).
pub struct RingBuffer {
    chunks: VecDeque<Vec<u8>>,
    total: usize,
    cap: usize,
}

impl Default for RingBuffer {
    fn default() -> Self {
        Self::new(RING_CAP_BYTES)
    }
}

impl RingBuffer {
    pub fn new(cap: usize) -> Self {
        Self {
            chunks: VecDeque::new(),
            total: 0,
            cap,
        }
    }

    pub fn push(&mut self, chunk: &[u8]) {
        self.chunks.push_back(chunk.to_vec());
        self.total += chunk.len();
        while self.total > self.cap && self.chunks.len() > 1 {
            if let Some(evicted) = self.chunks.pop_front() {
                self.total -= evicted.len();
            }
        }
    }

    /// Oldest → newest chunks for replay.
    pub fn replay(&self) -> impl Iterator<Item = &[u8]> {
        self.chunks.iter().map(|chunk| chunk.as_slice())
    }

    pub fn bytes(&self) -> usize {
        self.total
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_oldest_past_the_cap() {
        // Port of hub.test.ts `RingBuffer › evicts oldest past the cap`.
        let mut ring = RingBuffer::new(10);
        ring.push(&[0u8; 6]);
        ring.push(&[1u8; 6]);
        assert_eq!(ring.replay().count(), 1);
        assert_eq!(ring.bytes(), 6);
        assert_eq!(ring.replay().next().unwrap(), &[1u8; 6]);
    }

    #[test]
    fn keeps_a_single_oversized_chunk() {
        // hub.ts keeps ≥ 1 chunk even when it alone exceeds the cap.
        let mut ring = RingBuffer::new(4);
        ring.push(&[7u8; 9]);
        assert_eq!(ring.replay().count(), 1);
        assert_eq!(ring.bytes(), 9);
    }

    #[test]
    fn replays_in_push_order() {
        let mut ring = RingBuffer::default();
        ring.push(b"one");
        ring.push(b"two");
        ring.push(b"three");
        let replayed: Vec<&[u8]> = ring.replay().collect();
        assert_eq!(replayed, vec![b"one".as_ref(), b"two".as_ref(), b"three".as_ref()]);
        assert_eq!(ring.bytes(), 11);
    }

    #[test]
    fn default_cap_is_256_kib() {
        let mut ring = RingBuffer::default();
        for _ in 0..300 {
            ring.push(&[0u8; 1024]);
        }
        assert!(ring.bytes() <= RING_CAP_BYTES);
        assert!(ring.bytes() > RING_CAP_BYTES - 1024);
    }
}
