//! The dispatcher — a threaded port of `apps/companion/src/dispatcher.ts`. It
//! turns Electric `IssueEvent`s into pipeline runs: a bounded-concurrency queue
//! with per-issue dedup, unassign-cancel, an `updated`-event re-entry allowlist,
//! and boot-time recovery of in-flight issues.
//!
//! Threaded (no tokio): each pipeline run is a `std::thread`; `State` is shared
//! as `Arc<Mutex<State>>` (rusqlite Connection is Send, so the Mutex makes it
//! usable across the worker threads — the same one-connection-serialized model
//! as the Zig sync engine).

use crate::electric::{IssueEvent, IssueEventType};
use crate::state::{IssueRow, IssueSeed, State};
use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Statuses that count as "in flight" (re-enqueued on boot; cancelled on unassign).
/// Single source of truth: packages/domain-contract/contract.json (agentPipeline).
const NON_TERMINAL: &[&str] = crate::domain_contract::AGENT_PIPELINE_NON_TERMINAL_STATUSES;

/// Statuses the dispatcher may re-enter the pipeline from on an `updated` event.
const REENTRY: &[&str] = crate::domain_contract::AGENT_PIPELINE_REENTRY_STATUSES;

/// The per-issue pipeline. Captures whatever it needs (state, config, mcp); the
/// dispatcher just invokes it on a worker thread.
pub type PipelineFn = Arc<dyn Fn(IssueRow) + Send + Sync>;

struct Inner {
    state: Arc<Mutex<State>>,
    max_concurrent: usize,
    pipeline: PipelineFn,
    queue: Mutex<VecDeque<String>>,
    running: Mutex<HashSet<String>>,
    stopped: AtomicBool,
}

impl Inner {
    fn with_state<R>(&self, f: impl FnOnce(&State) -> R) -> R {
        let guard = self.state.lock().expect("state poisoned");
        f(&guard)
    }

    fn enqueue(self: &Arc<Self>, event: IssueEvent) {
        let is_own = event.assignee_id.is_some();
        if matches!(event.event_type, IssueEventType::Unassigned) || !is_own {
            // No longer ours — cancel any in-flight work.
            self.with_state(|s| {
                if let Ok(Some(issue)) = s.get_issue(&event.issue_id) {
                    if NON_TERMINAL.contains(&issue.status.as_str()) {
                        let _ = s.set_issue_status(&event.issue_id, "cancelled", Some("unassigned"));
                    }
                }
            });
            return;
        }

        match event.event_type {
            IssueEventType::Assigned => {
                // Explicit user signal — always honour it: reset + re-queue.
                self.with_state(|s| {
                    let _ = s.upsert_issue(&IssueSeed {
                        id: &event.issue_id,
                        identifier: &event.identifier,
                        title: &event.title,
                        project_id: &event.project_id,
                        status: "queued",
                    });
                    let _ = s.reset_for_assignment(&event.issue_id);
                });
                self.enqueue_id(&event.issue_id);
            }
            IssueEventType::Updated => {
                let existing = self.with_state(|s| s.get_issue(&event.issue_id).ok().flatten());
                if let Some(ex) = &existing {
                    if ex.interactive_owned != 0 {
                        // A desktop interactive session owns this issue (e.g. the
                        // user is about to approve-and-continue in the embedded
                        // terminal). Suppress the automatic background code run so
                        // the two don't both code.
                        return;
                    }
                    if !REENTRY.contains(&ex.status.as_str()) {
                        return; // updated events fire constantly; only re-enter from the allowlist
                    }
                }
                let status = match &existing {
                    Some(ex) if ex.status == "awaiting_approval" => "awaiting_approval",
                    _ => "queued",
                };
                self.with_state(|s| {
                    let _ = s.upsert_issue(&IssueSeed {
                        id: &event.issue_id,
                        identifier: &event.identifier,
                        title: &event.title,
                        project_id: &event.project_id,
                        status,
                    });
                });
                self.enqueue_id(&event.issue_id);
            }
            IssueEventType::Unassigned => {}
        }
    }

    fn enqueue_id(self: &Arc<Self>, id: &str) {
        {
            let mut q = self.queue.lock().unwrap();
            let r = self.running.lock().unwrap();
            if q.iter().any(|x| x == id) || r.contains(id) {
                return;
            }
            q.push_back(id.to_string());
        }
        self.drain();
    }

    fn drain(self: &Arc<Self>) {
        if self.stopped.load(Ordering::Acquire) {
            return;
        }
        // Reserve slots under the locks (queue → running order), then fetch +
        // spawn outside them so the pipeline/finish never contend on these.
        let mut ids: Vec<String> = Vec::new();
        {
            let mut q = self.queue.lock().unwrap();
            let mut r = self.running.lock().unwrap();
            while r.len() < self.max_concurrent {
                let Some(id) = q.pop_front() else { break };
                r.insert(id.clone());
                ids.push(id);
            }
        }
        for id in ids {
            let issue = self.with_state(|s| s.get_issue(&id).ok().flatten());
            match issue {
                Some(issue) => {
                    let inner = Arc::clone(self);
                    std::thread::spawn(move || {
                        (inner.pipeline)(issue.clone());
                        inner.finish(&issue.id);
                    });
                }
                None => {
                    // Vanished from state — free the slot.
                    self.running.lock().unwrap().remove(&id);
                }
            }
        }
    }

    fn finish(self: &Arc<Self>, id: &str) {
        self.running.lock().unwrap().remove(id);
        if !self.stopped.load(Ordering::Acquire) {
            self.drain();
        }
    }

    fn recover(self: &Arc<Self>) {
        let stuck = self.with_state(|s| s.list_issues(NON_TERMINAL).unwrap_or_default());
        for issue in &stuck {
            if issue.status == "coding" || issue.status == "planning" {
                self.with_state(|s| {
                    let _ = s.set_issue_status(&issue.id, "claimed", Some("resumed after restart"));
                });
            }
            self.enqueue_id(&issue.id);
        }
    }

    fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if self.running.lock().unwrap().is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

#[derive(Clone)]
pub struct Dispatcher {
    inner: Arc<Inner>,
}

impl Dispatcher {
    /// Start the dispatcher (runs boot recovery immediately).
    pub fn start(state: Arc<Mutex<State>>, max_concurrent: usize, pipeline: PipelineFn) -> Dispatcher {
        let inner = Arc::new(Inner {
            state,
            max_concurrent: max_concurrent.max(1),
            pipeline,
            queue: Mutex::new(VecDeque::new()),
            running: Mutex::new(HashSet::new()),
            stopped: AtomicBool::new(false),
        });
        inner.recover();
        Dispatcher { inner }
    }

    pub fn enqueue(&self, event: IssueEvent) {
        self.inner.enqueue(event);
    }

    /// Stop accepting new work and wait (≤5s) for in-flight runs to settle.
    pub fn stop(&self) {
        self.inner.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Condvar;

    fn state() -> Arc<Mutex<State>> {
        Arc::new(Mutex::new(State::open(":memory:").unwrap()))
    }

    fn ev(id: &str, assignee: Option<&str>) -> IssueEvent {
        IssueEvent {
            event_type: if assignee.is_some() { IssueEventType::Assigned } else { IssueEventType::Unassigned },
            issue_id: id.to_string(),
            identifier: format!("EXP-{id}"),
            title: format!("Test {id}"),
            project_id: format!("proj-{id}"),
            assignee_id: assignee.map(|s| s.to_string()),
        }
    }

    /// A release-once gate the test pipelines block on.
    #[derive(Default)]
    struct Gate {
        open: Mutex<bool>,
        cv: Condvar,
    }
    impl Gate {
        fn wait(&self) {
            let mut g = self.open.lock().unwrap();
            while !*g {
                g = self.cv.wait(g).unwrap();
            }
        }
        fn release(&self) {
            *self.open.lock().unwrap() = true;
            self.cv.notify_all();
        }
    }

    fn status_of(state: &Arc<Mutex<State>>, id: &str) -> Option<String> {
        state.lock().unwrap().get_issue(id).unwrap().map(|i| i.status)
    }

    #[test]
    fn runs_once_per_issue_and_reaches_done() {
        let st = state();
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let (st2, seen2) = (Arc::clone(&st), Arc::clone(&seen));
        let pipeline: PipelineFn = Arc::new(move |issue: IssueRow| {
            seen2.lock().unwrap().push(issue.id.clone());
            st2.lock().unwrap().set_issue_status(&issue.id, "done", None).unwrap();
        });
        let d = Dispatcher::start(Arc::clone(&st), 2, pipeline);
        d.enqueue(ev("a", Some("bot")));
        std::thread::sleep(Duration::from_millis(80));
        d.stop();
        assert_eq!(&*seen.lock().unwrap(), &["a".to_string()]);
        assert_eq!(status_of(&st, "a").as_deref(), Some("done"));
    }

    #[test]
    fn dedupes_in_flight_assigned() {
        let st = state();
        let calls = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new(Gate::default());
        let (st2, calls2, gate2) = (Arc::clone(&st), Arc::clone(&calls), Arc::clone(&gate));
        let pipeline: PipelineFn = Arc::new(move |issue: IssueRow| {
            calls2.fetch_add(1, Ordering::SeqCst);
            gate2.wait();
            st2.lock().unwrap().set_issue_status(&issue.id, "done", None).unwrap();
        });
        let d = Dispatcher::start(Arc::clone(&st), 2, pipeline);
        d.enqueue(ev("a", Some("bot")));
        d.enqueue(ev("a", Some("bot")));
        d.enqueue(ev("a", Some("bot")));
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        gate.release();
        std::thread::sleep(Duration::from_millis(50));
        d.stop();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn respects_max_concurrent() {
        let st = state();
        let active = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new(Gate::default());
        let (active2, max2, gate2) = (Arc::clone(&active), Arc::clone(&max_seen), Arc::clone(&gate));
        let pipeline: PipelineFn = Arc::new(move |_issue: IssueRow| {
            let a = active2.fetch_add(1, Ordering::SeqCst) + 1;
            max2.fetch_max(a, Ordering::SeqCst);
            gate2.wait();
            active2.fetch_sub(1, Ordering::SeqCst);
        });
        let d = Dispatcher::start(Arc::clone(&st), 2, pipeline);
        for id in ["a", "b", "c", "d"] {
            d.enqueue(ev(id, Some("bot")));
        }
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(max_seen.load(Ordering::SeqCst), 2);
        gate.release();
        std::thread::sleep(Duration::from_millis(80));
        d.stop();
    }

    #[test]
    fn cancels_unassigned_in_flight() {
        let st = state();
        let gate = Arc::new(Gate::default());
        let gate2 = Arc::clone(&gate);
        let pipeline: PipelineFn = Arc::new(move |_issue: IssueRow| {
            gate2.wait();
        });
        let d = Dispatcher::start(Arc::clone(&st), 2, pipeline);
        d.enqueue(ev("a", Some("bot")));
        std::thread::sleep(Duration::from_millis(40));
        d.enqueue(ev("a", None)); // unassigned
        assert_eq!(status_of(&st, "a").as_deref(), Some("cancelled"));
        gate.release();
        d.stop();
    }

    #[test]
    fn updated_event_gated_by_reentry_allowlist() {
        let st = state();
        let calls = Arc::new(AtomicUsize::new(0));
        let (st2, calls2) = (Arc::clone(&st), Arc::clone(&calls));
        let pipeline: PipelineFn = Arc::new(move |issue: IssueRow| {
            calls2.fetch_add(1, Ordering::SeqCst);
            st2.lock().unwrap().set_issue_status(&issue.id, "in_review", None).unwrap();
        });
        let d = Dispatcher::start(Arc::clone(&st), 2, pipeline);
        // Put the issue in a non-reentry terminal-ish state.
        st.lock().unwrap().upsert_issue(&IssueSeed { id: "a", identifier: "EXP-a", title: "t", project_id: "p", status: "in_review" }).unwrap();
        let mut e = ev("a", Some("bot"));
        e.event_type = IssueEventType::Updated;
        d.enqueue(e);
        std::thread::sleep(Duration::from_millis(40));
        d.stop();
        // in_review is not in REENTRY → pipeline must NOT have run.
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(status_of(&st, "a").as_deref(), Some("in_review"));
    }

    #[test]
    fn interactive_owned_suppresses_reentry() {
        let st = state();
        let calls = Arc::new(AtomicUsize::new(0));
        let (st2, calls2) = (Arc::clone(&st), Arc::clone(&calls));
        let pipeline: PipelineFn = Arc::new(move |issue: IssueRow| {
            calls2.fetch_add(1, Ordering::SeqCst);
            st2.lock().unwrap().set_issue_status(&issue.id, "in_review", None).unwrap();
        });
        let d = Dispatcher::start(Arc::clone(&st), 2, pipeline);
        // awaiting_approval IS in REENTRY, but an interactive session owns it →
        // the background code stage must NOT run (the desktop continues it).
        st.lock().unwrap().upsert_issue(&IssueSeed { id: "a", identifier: "EXP-a", title: "t", project_id: "p", status: "awaiting_approval" }).unwrap();
        st.lock().unwrap().patch_issue("a", &crate::state::IssuePatch { interactive_owned: Some(1), ..Default::default() }).unwrap();
        let mut e = ev("a", Some("bot"));
        e.event_type = IssueEventType::Updated;
        d.enqueue(e);
        std::thread::sleep(Duration::from_millis(40));
        d.stop();
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
