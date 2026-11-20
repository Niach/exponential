//! Who may have a request in the guard's hands at once: a few per user,
//! so one user (or a flood of connections) cannot take the guard from
//! another. Users without a runtime directory (system accounts, the
//! subordinate ids of a user namespace) share one bucket, so a person
//! cannot multiply their share by connecting under many ids.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

/// Requests of one bucket in the guard's hands at once.
pub const PER_BUCKET: usize = 2;
/// Requests in the guard's hands at once, all buckets.
pub const TOTAL: usize = 64;
/// The bucket of every user without a runtime directory.
pub const SHARED: u64 = u64::MAX;

#[derive(Clone, Default)]
pub struct Slots(Arc<Mutex<HashMap<u64, usize>>>);

/// A slot held; given back when dropped (also when its thread panics).
pub struct Slot {
    slots: Slots,
    bucket: u64,
}

impl Drop for Slot {
    fn drop(&mut self) {
        let mut held = self.slots.0.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(n) = held.get_mut(&self.bucket) {
            *n -= 1;
            if *n == 0 {
                held.remove(&self.bucket);
            }
        }
    }
}

/// The bucket of user `uid`: its own when it has a runtime directory
/// (`has_runtime`), else the shared one.
pub fn bucket(uid: u32, has_runtime: bool) -> u64 {
    if has_runtime {
        u64::from(uid)
    } else {
        SHARED
    }
}

impl Slots {
    /// A slot for `bucket`, or `None` when it (or the guard) is full.
    pub fn take(&self, bucket: u64) -> Option<Slot> {
        let mut held = self.0.lock().unwrap_or_else(|p| p.into_inner());
        let total: usize = held.values().sum();
        let mine = held.entry(bucket).or_default();
        if *mine >= PER_BUCKET || total >= TOTAL {
            return None;
        }
        *mine += 1;
        Some(Slot { slots: self.clone(), bucket })
    }

    /// Requests in the guard's hands now.
    pub fn held(&self) -> usize {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).values().sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 16 slow connections of user A (or of 16 ids without a runtime
    /// directory) leave user B its slots.
    #[test]
    fn one_user_cannot_take_the_guard_from_another() {
        let slots = Slots::default();
        let a: Vec<Slot> = (0..16).filter_map(|_| slots.take(bucket(1001, true))).collect();
        assert_eq!(a.len(), PER_BUCKET);
        let ids: Vec<Slot> = (0..16).filter_map(|n| slots.take(bucket(100_000 + n, false))).collect();
        assert_eq!(ids.len(), PER_BUCKET, "subordinate ids share one bucket");
        let b = slots.take(bucket(1000, true));
        assert!(b.is_some(), "user B is refused while A floods");
        drop(a);
        assert_eq!(slots.held(), PER_BUCKET + 1);
        drop(b);
        drop(ids);
        assert_eq!(slots.held(), 0);
    }

    /// A slot comes back when its thread panics.
    #[test]
    fn a_panicking_request_gives_its_slot_back() {
        let slots = Slots::default();
        let held = slots.take(7).unwrap();
        let _ = std::thread::spawn(move || {
            let _held = held;
            panic!("a reader died");
        })
        .join();
        assert_eq!(slots.held(), 0);
    }

    #[test]
    fn the_guard_as_a_whole_has_a_bound() {
        let slots = Slots::default();
        let many: Vec<Slot> = (0..1000u64).filter_map(|n| slots.take(n)).collect();
        assert_eq!(many.len(), TOTAL);
    }
}
