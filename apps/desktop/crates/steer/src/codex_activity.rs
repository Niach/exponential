//! The codex activity emitter (EXP-383): tails the rollout JSONL an
//! interactive `codex` TUI session writes (per-line flushed, so live) and maps
//! it onto the same scrubbed [`ActivityEvent`] stream the claude emitter
//! publishes — narration, tool headlines, the user's prompts, questions, and
//! the synced needs-input flag — plus the shared debounced worktree diff.
//!
//! Placeholder module — filled in by the codex milestone.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::activity::{
    secrets_from_worktree, DiffSnapshots, EmitterConfig, NeedsInputForwarder, Redactor,
    POLL_INTERVAL,
};
use crate::frames::ActivityEvent;
use crate::publisher::ActivitySender;

pub(crate) fn run_emitter(config: EmitterConfig, sender: ActivitySender, active: Arc<AtomicBool>) {
    let mut exact_secrets = secrets_from_worktree(&config.worktree);
    exact_secrets.extend(config.extra_secrets.iter().cloned());
    let redactor = Redactor::new(exact_secrets);

    sender.send(ActivityEvent::narration("Session started"));

    let mut diffs = DiffSnapshots::new();
    let mut needs_input = NeedsInputForwarder::new();

    while active.load(Ordering::SeqCst) {
        needs_input.tick(false, &config.on_needs_input);
        diffs.tick(&config.worktree, &sender, &redactor);
        std::thread::sleep(POLL_INTERVAL);
    }

    needs_input.clear_on_teardown(&config.on_needs_input);
}
