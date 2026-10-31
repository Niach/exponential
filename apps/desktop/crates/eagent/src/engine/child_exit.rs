//! The child a stdio adapter owns (EXP-746; EXP-1426 part 4 moved it out of
//! `engine::host`, which re-exports it): the one-shot exit, the pid and the
//! transport's drop count, recorded by the adapter and read by the host's end
//! sequence.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::spawn::ChildExit;

/// How an adapter reports its CHILD's exit to the engine core.
///
/// The adapter owns the process (`crate::transport::ChildLines`), the engine
/// owns the outcome vocabulary — `exit:<code>` is the publisher `bye` every
/// client already reads. An adapter records the exit here the moment its wait
/// thread reaps; the engine reads it in the end sequence and, absent an
/// explicit kill, ends the row with that code.
///
/// It reaches the adapters as a field on `AdapterSpec` (claude and codex
/// record into it; an adapter that owns no child never touches it, and the
/// run then ends as `ended`).
#[derive(Clone)]
pub struct ChildExitLink {
    slot: Arc<Mutex<Option<ChildExit>>>,
    /// EXP-758: the child's pid, recorded at spawn. The lifecycle writes it
    /// onto the run record so a host that died without its end sequence
    /// (Cmd-Q, a crash) leaves a pid the next start can reap
    /// (Exponential's `coding::reaper::reap_recorded`) — a codex child carries no
    /// `claude-hooks` anchor, so this is the only handle on it.
    pid: Arc<Mutex<Option<u32>>>,
    /// EXP-784: stdout lines the transport DROPPED over the child's life
    /// (`transport::STDOUT_LINES_CAP`), written by the exit forwarder just
    /// before the exit. Nonzero is folded into the ended banner
    /// (the host's ended banner): a session that lost frames is broken
    /// in ways the transcript cannot show.
    dropped_lines: Arc<AtomicU64>,
    /// Held until the exit is recorded; dropping it is the signal
    /// [`ChildExitLink::reaped`] waits on. A flume receiver whose senders are
    /// all gone resolves immediately and KEEPS resolving, so the edge is
    /// memoryful: a waiter that arrives after the child died sees it too.
    gate: Arc<Mutex<Option<flume::Sender<()>>>>,
    signal: flume::Receiver<()>,
}

impl Default for ChildExitLink {
    fn default() -> ChildExitLink {
        ChildExitLink::new()
    }
}

/// How long a closing adapter waits for its child's exit CODE.
///
/// The child's stdout ends on one thread and `wait()` reaps it on another, so
/// the two race by microseconds — but the end sequence reads the code once,
/// right after the connection closes. Without this grace a crashed CLI would
/// end the run as a bare `ended` instead of `exit:<code>`.
pub const CHILD_EXIT_GRACE: Duration = Duration::from_secs(2);

impl ChildExitLink {
    pub fn new() -> ChildExitLink {
        let (gate, signal) = flume::bounded(0);
        ChildExitLink {
            slot: Arc::new(Mutex::new(None)),
            pid: Arc::new(Mutex::new(None)),
            dropped_lines: Arc::new(AtomicU64::new(0)),
            gate: Arc::new(Mutex::new(Some(gate))),
            signal,
        }
    }

    /// Adapter side (EXP-784): the transport's final stdout drop count.
    pub fn record_dropped_lines(&self, dropped: u64) {
        self.dropped_lines.store(dropped, Ordering::SeqCst);
    }

    /// EXP-784: stdout lines the transport dropped, once recorded.
    pub fn dropped_lines(&self) -> u64 {
        self.dropped_lines.load(Ordering::SeqCst)
    }

    /// Adapter side: the child was reaped.
    pub fn record(&self, exit: ChildExit) {
        if let Ok(mut slot) = self.slot.lock() {
            slot.get_or_insert(exit);
        }
        // Dropped last: a waiter woken by this must find the code already in
        // the slot.
        if let Ok(mut gate) = self.gate.lock() {
            gate.take();
        }
    }

    /// Adapter side: the child was spawned as `pid`.
    pub fn record_pid(&self, pid: u32) {
        if let Ok(mut slot) = self.pid.lock() {
            *slot = Some(pid);
        }
    }

    /// The spawned child's pid, once an adapter recorded it.
    pub fn pid(&self) -> Option<u32> {
        self.pid.lock().ok().and_then(|slot| *slot)
    }

    /// Engine side: what the child exited with, if it did.
    pub fn get(&self) -> Option<ChildExit> {
        self.slot.lock().ok().and_then(|slot| slot.clone())
    }

    /// Resolves once [`ChildExitLink::record`] has run — immediately, if it
    /// already has. An adapter awaits this (bounded by [`CHILD_EXIT_GRACE`])
    /// between its child's stdout EOF and closing the connection.
    pub async fn reaped(&self) {
        let _ = self.signal.recv_async().await;
    }
}

/// EXP-784: the banner fragment for a transport that dropped stdout lines.
pub fn dropped_lines_note(dropped: u64) -> String {
    let (lines, verb) = if dropped == 1 { ("line", "was") } else { ("lines", "were") };
    format!("{dropped} stdout {lines} {verb} dropped")
}

/// The `main_fn` every stdio adapter runs: hold the connection open until
/// EITHER half goes away, then return so the connection shuts down.
///
/// The child half is the load-bearing one. The SDK's own `connect_to` waits
/// only on `incoming_closed`, and the host's loop waits only on the adapter,
/// so an agent CLI that crashed (OOM, `kill -9`, the reaper) would leave the
/// two halves waiting on each other forever: no `on_exit`, no `exit:<code>`,
/// a row still reading `running` and a desktop tab still Live
/// (EXP-746 review E1). On the child's edge the exit code gets
/// [`CHILD_EXIT_GRACE`] to land so the run ends as `exit:<code>`.
pub async fn until_either_closes(
    cx: &agent_client_protocol::ConnectionTo<agent_client_protocol::Client>,
    exit: &ChildExitLink,
    child_gone: impl std::future::Future<Output = ()>,
) {
    tokio::select! {
        () = cx.incoming_closed() => {}
        () = child_gone => {
            let _ = tokio::time::timeout(CHILD_EXIT_GRACE, exit.reaped()).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_child_exit_becomes_the_bye_outcome() {
        let link = ChildExitLink::new();
        assert_eq!(link.get(), None);
        link.record(ChildExit {
            code: 3,
            success: false,
            signal: None,
        });
        // A second report never overwrites the first.
        link.record(ChildExit {
            code: 0,
            success: true,
            signal: None,
        });
        assert_eq!(link.get().map(|exit| exit.code), Some(3));
    }

    #[tokio::test]
    async fn a_recorded_child_exit_wakes_waiters_before_and_after_it() {
        let link = ChildExitLink::new();
        // Nothing yet: an adapter parked here keeps the connection open.
        assert!(
            tokio::time::timeout(Duration::from_millis(50), link.reaped())
                .await
                .is_err()
        );
        let waiting = link.clone();
        let parked = tokio::spawn(async move { waiting.reaped().await });
        link.record(ChildExit {
            code: 3,
            success: false,
            signal: None,
        });
        tokio::time::timeout(Duration::from_secs(5), parked)
            .await
            .expect("the waiter parked before the exit is woken")
            .expect("the waiter did not panic");
        // And the edge is memoryful: an adapter that asks AFTER the child died
        // must not park forever on an event it missed.
        tokio::time::timeout(Duration::from_secs(5), link.reaped())
            .await
            .expect("a late waiter resolves immediately");
    }
}
