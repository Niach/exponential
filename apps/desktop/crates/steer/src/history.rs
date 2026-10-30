//! Session history ON THE DEVICE (EXP-773) — the transcript of an ended run
//! lives on the machine that ran it and nowhere else.
//!
//! The live [`crate::journal::ActivityJournal`] is memory: it dies with the
//! process, so a finished run used to have no transcript at all once its
//! publisher closed the room. This module is its durable twin — one
//! append-only `{data_dir}/journal/<sessionId>.jsonl` file per session, one
//! JSON [`ActivityEvent`] per line, in publish order, written by the live
//! publisher as it sends.
//!
//! The read path is the mirror image: VAPP-43 STORED REPLAY — an eNet viewer
//! that joins a session no live publisher here runs is answered off this
//! file (`enet_steer.rs`, [`read_journal_seq`] + [`read_journal_page`]).
//! Neither the server nor the relay ever stores or carries a transcript.
//!
//! Three rules the file inherits from the in-memory journal:
//!
//! - The small latest-wins kinds (`config_state`, `usage`, …) are APPENDED
//!   like anything else (an append-only file cannot rewrite a slot);
//!   [`read_journal`] folds them back into one slot each, replayed at the end
//!   in the relay's own `LATEST_REPLAY_ORDER`. EXP-906: the `diff` slot is
//!   the exception — a worktree snapshot is up to 512 KiB and a run emits
//!   dozens, so appending every one filled the cap in ~32 snapshots and
//!   froze the rest of the transcript. It lives in a SIDECAR
//!   (`<sessionId>.diff.json`, overwritten in place, never appended); the
//!   `.jsonl` keeps a marker line so the wire sequence stays the line index.
//! - A file stops growing at [`JOURNAL_FILE_CAP`] (one visible notice on the
//!   run, then silence): a runaway run must never fill the disk, and the head
//!   of a transcript is the part worth keeping.
//! - Files are kept FOREVER by default (EXP-886). The per-device "Keep
//!   session history" setting (`coding::session_retention`) is the only
//!   thing that removes them: [`prune_session_history`] applies it at
//!   daemon/app boot and when the desktop setting shrinks.
//!
//! Redaction is NOT done here. Events reach the writer already scrubbed by
//! the emitter and stamped by the publisher's `prepare_for_journal`, exactly
//! as they go on the wire — the file holds what a viewer sees, never more.

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::frames::{ActivityEvent, ClientFrame};

/// A journal file stops growing here. Well past any real run (the in-memory
/// journal's own budget is 4 MiB) and small enough that a thousand of them
/// cannot surprise a laptop.
pub const JOURNAL_FILE_CAP: u64 = 16 * 1024 * 1024;

/// EXP-906: the row the publisher puts on the wire (and in its in-memory
/// journal) the moment the file stops recording — a person reading the run
/// must learn it from the run, not from a log line on the device.
pub const JOURNAL_CAP_NOTICE: &str = "[Exponential] Session history stopped recording on this device: the journal reached its size cap. The session continues, but a viewer who joins later sees the transcript only up to here.";

/// EXP-906: the line a sidecar'd `diff` leaves in the `.jsonl` — it holds the
/// diff's place in the line count (= the wire sequence) and parses as nothing
/// on every build, so a reader skips it like any unknown kind.
const DIFF_REF_LINE: &str = "{\"kind\":\"diff_ref\"}\n";

/// `bye` outcome of a STORED REPLAY of an ended run (VAPP-43): the replay
/// is the whole transcript, and no live run follows it.
pub const HISTORY_OUTCOME: &str = "history";

/// EXP-850 §3 review: how often ONE workflow id may reach the file while it
/// runs. A card republishes up to once a second and serializes to kilobytes
/// (32 phases, 64 agents, four preview strings each), so a workflow that runs
/// for an hour would write megabytes of near-identical frames and push the
/// rest of the run past [`JOURNAL_FILE_CAP`] — after which nothing is
/// recorded at all. The file is latest-wins per id on read
/// ([`read_journal`]), so dropping the frames in between costs a replay
/// nothing but freshness. A TERMINAL status and the FIRST frame of an id are
/// always written.
const WORKFLOW_JOURNAL_INTERVAL: Duration = Duration::from_secs(10);

/// Where every session's journal file lives.
pub fn journal_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("journal")
}

/// `{data_dir}/journal/{session_id}.jsonl`, or `None` when the id could
/// escape that directory. Session ids are server-minted UUIDs, but this path
/// is built from a RELAY frame (`history_request`), so the id is untrusted
/// input and is treated as one.
pub fn journal_path(data_dir: &Path, session_id: &str) -> Option<PathBuf> {
    if session_id.is_empty() || session_id.len() > 128 {
        return None;
    }
    if session_id.starts_with('.')
        || session_id
            .chars()
            .any(|c| c == '/' || c == '\\' || c == '\0' || c.is_control())
    {
        return None;
    }
    Some(journal_dir(data_dir).join(format!("{session_id}.jsonl")))
}

/// EXP-906: `{data_dir}/journal/{session_id}.diff.json` — the latest worktree
/// diff of that session, `{"seq": <line index>, "event": <diff event>}`,
/// overwritten whole on every snapshot. Same id hygiene as [`journal_path`].
pub fn diff_sidecar_path(data_dir: &Path, session_id: &str) -> Option<PathBuf> {
    journal_path(data_dir, session_id)?;
    Some(journal_dir(data_dir).join(format!("{session_id}.diff.json")))
}

/// The one on-disk shape of the sidecar.
#[derive(serde::Serialize, serde::Deserialize)]
struct DiffSidecar {
    seq: u64,
    event: ActivityEvent,
}

/// The append-only writer the live publisher feeds. Every event is flushed on
/// the spot: a run that is killed (or a machine that loses power) still has
/// everything it published on disk.
pub struct JournalWriter {
    path: PathBuf,
    /// EXP-906: where the latest `diff` goes instead of the file.
    sidecar: PathBuf,
    file: BufWriter<File>,
    /// Bytes on disk, including whatever a resumed run's file already held.
    written: u64,
    /// EXP-783: lines on disk — the wire sequence the next event gets.
    lines: u64,
    /// EXP-850 §3: when each workflow id last reached the file — the
    /// [`WORKFLOW_JOURNAL_INTERVAL`] throttle's clock. Kept in
    /// first-appearance order and capped at [`HISTORY_WORKFLOW_CAP`], like
    /// every other fold of this slot.
    workflow_writes: Vec<(String, std::time::Instant)>,
    /// Past the cap (or after a write error): every further append is a no-op.
    stopped: bool,
    /// EXP-906: set once, the moment the cap stops the file — the publisher
    /// takes it and puts [`JOURNAL_CAP_NOTICE`] on the wire.
    cap_notice: bool,
}

impl JournalWriter {
    /// Open (creating the directory and the file) for append. `None` when the
    /// id is unusable or the filesystem refuses — journaling is best-effort
    /// and never gates publishing.
    pub fn open(data_dir: &Path, session_id: &str) -> Option<Self> {
        let path = journal_path(data_dir, session_id)?;
        let sidecar = diff_sidecar_path(data_dir, session_id)?;
        let dir = path.parent()?.to_path_buf();
        if let Err(err) = fs::create_dir_all(&dir) {
            log::warn!("steer history: cannot create {}: {err}", dir.display());
            return None;
        }
        let file = match OpenOptions::new().create(true).append(true).open(&path) {
            Ok(file) => file,
            Err(err) => {
                log::warn!("steer history: cannot open {}: {err}", path.display());
                return None;
            }
        };
        let written = file.metadata().map(|meta| meta.len()).unwrap_or(0);
        // EXP-783: a resumed run continues the wire sequence its predecessor
        // stopped at, which is this file's line count. Counting newlines over
        // a capped-at-16 MiB file is a millisecond, once per publisher start.
        let lines = if written == 0 { 0 } else { count_lines(&path) };
        Some(Self {
            path,
            sidecar,
            file: BufWriter::new(file),
            written,
            lines,
            workflow_writes: Vec::new(),
            stopped: false,
            cap_notice: false,
        })
    }

    /// Append one already-redacted event. Best-effort by design: a full disk
    /// must never stall the publisher's pump loop.
    pub fn append(&mut self, event: &ActivityEvent) {
        self.append_at(event, std::time::Instant::now());
    }

    /// [`JournalWriter::append`] with the clock passed in — the throttle's
    /// seam, so its rule is testable without sleeping ten seconds.
    fn append_at(&mut self, event: &ActivityEvent, now: std::time::Instant) {
        // EXP-906: the diff slot is bounded on its own (one snapshot, ≤512
        // KiB) and keeps updating even after the file stopped — the Changes
        // face of a late viewer reads the NEWEST patch, not the one from
        // the minute the cap hit.
        let is_diff = matches!(event, ActivityEvent::Diff { .. });
        if is_diff {
            self.write_diff_sidecar(event);
        }
        if self.stopped {
            return;
        }
        if !self.record_workflow(event, now) {
            return;
        }
        if self.written >= JOURNAL_FILE_CAP {
            log::warn!(
                "steer history: {} reached {JOURNAL_FILE_CAP} bytes — no longer recording",
                self.path.display()
            );
            self.stopped = true;
            self.cap_notice = true;
            return;
        }
        let line = if is_diff {
            DIFF_REF_LINE.to_string()
        } else {
            let Ok(mut line) = serde_json::to_string(event) else {
                return;
            };
            line.push('\n');
            line
        };
        if let Err(err) = self
            .file
            .write_all(line.as_bytes())
            .and_then(|()| self.file.flush())
        {
            log::warn!(
                "steer history: write to {} failed: {err} — no longer recording",
                self.path.display()
            );
            self.stopped = true;
            return;
        }
        self.written += line.len() as u64;
        self.lines += 1;
    }

    /// EXP-906: overwrite the sidecar with this snapshot, atomically (write
    /// beside, then rename) so a reader never sees half a patch. `seq` is
    /// the line the marker takes in the file — the wire sequence the live
    /// publisher numbered the event with.
    fn write_diff_sidecar(&self, event: &ActivityEvent) {
        let record = DiffSidecar {
            seq: self.lines,
            event: event.clone(),
        };
        let Ok(json) = serde_json::to_vec(&record) else {
            return;
        };
        let tmp = self.sidecar.with_extension("json.tmp");
        if let Err(err) = fs::write(&tmp, json).and_then(|()| fs::rename(&tmp, &self.sidecar)) {
            log::warn!("steer history: diff sidecar {} failed: {err}", self.sidecar.display());
            let _ = fs::remove_file(&tmp);
        }
    }

    /// EXP-906: `true` exactly once, on the append that hit the cap — the
    /// publisher's cue to put [`JOURNAL_CAP_NOTICE`] on the wire.
    pub fn take_cap_notice(&mut self) -> bool {
        std::mem::take(&mut self.cap_notice)
    }

    /// EXP-850 §3: whether this `workflow` frame is due. Anything else is
    /// always written. `true` = write it.
    fn record_workflow(&mut self, event: &ActivityEvent, now: std::time::Instant) -> bool {
        let ActivityEvent::Workflow(workflow) = event else {
            return true;
        };
        // A card that finished is the one a replay must not be missing.
        if workflow.status.is_terminal() {
            self.workflow_writes.retain(|(id, _)| *id != workflow.id);
            return true;
        }
        match self
            .workflow_writes
            .iter_mut()
            .find(|(id, _)| *id == workflow.id)
        {
            Some((_, last)) => {
                if now.duration_since(*last) < WORKFLOW_JOURNAL_INTERVAL {
                    return false;
                }
                *last = now;
            }
            None => {
                self.workflow_writes.push((workflow.id.clone(), now));
                while self.workflow_writes.len() > HISTORY_WORKFLOW_CAP {
                    self.workflow_writes.remove(0);
                }
            }
        }
        true
    }

    /// Test/observability surface: bytes recorded so far.
    pub fn bytes(&self) -> u64 {
        self.written
    }

    /// EXP-783: lines recorded so far, INCLUDING a resumed run's inherited
    /// ones — the wire sequence the publisher seeds its counter from.
    pub fn lines(&self) -> u64 {
        self.lines
    }
}

/// Newlines in `path`, or 0 when it cannot be read.
fn count_lines(path: &Path) -> u64 {
    let Ok(file) = File::open(path) else { return 0 };
    BufReader::new(file).lines().map_while(Result::ok).count() as u64
}

/// Which latest-wins slot an event owns, if any — the file's mirror of
/// `journal::slot_of`. Replay order is the relay's `LATEST_REPLAY_ORDER`
/// (`config_state`, `usage`, `context_layout`, `rate_limit`, `turn`, `queue`,
/// `workflow`, `background_tasks`, `task_list`, `diff`; EXP-784 added
/// `rate_limit`, EXP-848 `turn`, EXP-861 `queue`, EXP-850 the keyed workflow
/// block and the background-task strip, EXP-927 the task list, EXP-1051 the
/// context breakdown right after `usage`). Nine slots in all.
const SLOT_COUNT: usize = 9;
/// The slots replayed BEFORE the keyed workflow cards (`config_state`,
/// `usage`, `context_layout`, `rate_limit`, `turn`, `queue`);
/// `background_tasks`, `task_list` and `diff` follow them.
const SLOTS_BEFORE_WORKFLOWS: usize = 6;

/// EXP-850 §3: how many workflow cards one file's fold keeps — the in-memory
/// journal's [`crate::journal::JOURNAL_WORKFLOW_CAP`], mirrored so a replay
/// off disk and a replay off memory carry the same set.
pub const HISTORY_WORKFLOW_CAP: usize = 16;

fn slot_of(event: &ActivityEvent) -> Option<usize> {
    match event {
        ActivityEvent::ConfigState { .. } => Some(0),
        ActivityEvent::Usage { .. } => Some(1),
        // EXP-1051: the context breakdown, right after `usage`.
        ActivityEvent::ContextLayout { .. } => Some(2),
        ActivityEvent::RateLimit { .. } => Some(3),
        ActivityEvent::Turn { .. } => Some(4),
        // EXP-861: the queue slot, right after `turn`.
        ActivityEvent::Queue { .. } => Some(5),
        ActivityEvent::BackgroundTasks { .. } => Some(6),
        ActivityEvent::TaskList { .. } => Some(7),
        ActivityEvent::Diff { .. } => Some(8),
        _ => None,
    }
}

/// Read a session's journal back, folded and replay-ready. `None` = no file
/// (this machine never ran that session, or the journal was pruned);
/// `Some(vec![])` = a file with nothing usable in it.
///
/// Unparsable lines are skipped, not fatal: a torn last line after a crash,
/// or an event kind a newer build wrote and this one does not know, costs one
/// row instead of the whole transcript.
pub fn read_journal(data_dir: &Path, session_id: &str) -> Option<Vec<ActivityEvent>> {
    Some(
        read_journal_seq(data_dir, session_id)?
            .into_iter()
            .map(|(_, event)| event)
            .collect(),
    )
}

/// EXP-783 — [`read_journal`] with each event's wire sequence beside it.
///
/// The sequence IS the file's line index: the publisher numbers its stream
/// from the line count it inherited ([`JournalWriter::lines`]), so a line's
/// position and its wire `seq` are the same number for the life of the run.
/// The folded latest-wins slots keep the seq of the line they last occupied,
/// which is out of order — deliberately: all three are state slots on every
/// client, never transcript rows, and nothing sorts by their seq.
pub fn read_journal_seq(
    data_dir: &Path,
    session_id: &str,
) -> Option<Vec<(u64, ActivityEvent)>> {
    let path = journal_path(data_dir, session_id)?;
    let file = File::open(&path).ok()?;
    let mut events: Vec<(u64, ActivityEvent)> = Vec::new();
    let mut slots: [Option<(u64, ActivityEvent)>; SLOT_COUNT] =
        [None, None, None, None, None, None, None, None, None];
    // EXP-850 §3: the keyed `workflow` fold — the newest line per workflow id,
    // in first-appearance order, capped like the in-memory journal.
    let mut workflows: Vec<(String, u64, ActivityEvent)> = Vec::new();
    for (seq, line) in BufReader::new(file).lines().enumerate() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(mut event) = serde_json::from_str::<ActivityEvent>(&line) else {
            continue;
        };
        let seq = seq as u64;
        backfill_tool_identity(&mut event, seq);
        if let ActivityEvent::Workflow(workflow) = &event {
            let id = workflow.id.clone();
            match workflows.iter_mut().find(|(held, _, _)| *held == id) {
                Some(held) => *held = (id, seq, event),
                None => {
                    workflows.push((id, seq, event));
                    while workflows.len() > HISTORY_WORKFLOW_CAP {
                        workflows.remove(0);
                    }
                }
            }
            continue;
        }
        match slot_of(&event) {
            Some(slot) => slots[slot] = Some((seq, event)),
            None => events.push((seq, event)),
        }
    }
    // EXP-906: the sidecar holds the newest diff of every run written since;
    // an older file that still carries in-line diffs folded them above.
    if let Some(sidecar) = read_diff_sidecar(data_dir, session_id) {
        slots[SLOT_COUNT - 1] = Some(sidecar);
    }
    let mut slots = slots.into_iter();
    events.extend(slots.by_ref().take(SLOTS_BEFORE_WORKFLOWS).flatten());
    events.extend(workflows.into_iter().map(|(_, seq, event)| (seq, event)));
    events.extend(slots.flatten());
    Some(events)
}

/// EXP-906: the sidecar's diff with the sequence it was numbered with, or
/// `None` when there is no (readable) sidecar.
fn read_diff_sidecar(data_dir: &Path, session_id: &str) -> Option<(u64, ActivityEvent)> {
    let path = diff_sidecar_path(data_dir, session_id)?;
    let bytes = fs::read(path).ok()?;
    let record: DiffSidecar = serde_json::from_slice(&bytes).ok()?;
    matches!(record.event, ActivityEvent::Diff { .. }).then_some((record.seq, record.event))
}

/// EXP-785: a `tool` row needs its call `id` and `tool_kind` on the wire —
/// the relay DROPS one without them, and a client keys its `tool_update` fold
/// on the id. A journal line written before EXP-785 (the device keeps them
/// for as long as its session history setting says) carries neither, so a replay stamps the missing pieces: the id is
/// the line's own position, which is unique within the file and stable across
/// replays, and the kind is `other`, which is what an unknown tool renders as
/// anyway. The row can never settle — nothing will ever send an update for
/// that id — but it is READ, which is the whole point of a transcript.
fn backfill_tool_identity(event: &mut ActivityEvent, seq: u64) {
    let ActivityEvent::Tool { id, tool_kind, .. } = event else {
        return;
    };
    if id.is_none() {
        *id = Some(format!("journal-{seq}"));
    }
    if tool_kind.is_none() {
        *tool_kind = Some(crate::frames::ToolKind::Other);
    }
}

/// EXP-783 — the page of a session's transcript BELOW `before_seq`.
///
/// Answers one [`crate::frames::ServerFrame::HistoryPage`]: the LAST `limit`
/// events whose sequence is under `before_seq`, oldest first, so a client
/// prepends them as one block. The latest-wins slots are excluded — they are
/// state, and the client already holds the newest of each. `None` = no
/// journal on this machine; an empty vec = nothing older exists.
pub fn read_journal_page(
    data_dir: &Path,
    session_id: &str,
    before_seq: u64,
    limit: usize,
) -> Option<Vec<(u64, ActivityEvent)>> {
    let mut page: Vec<(u64, ActivityEvent)> = read_journal_seq(data_dir, session_id)?
        .into_iter()
        // EXP-850: a `workflow` is a latest-wins slot too (keyed by id), so
        // it is excluded from a transcript page exactly like the fixed ones.
        .filter(|(seq, event)| {
            *seq < before_seq
                && slot_of(event).is_none()
                && !matches!(event, ActivityEvent::Workflow(_))
        })
        .collect();
    if page.len() > limit {
        page.drain(..page.len() - limit);
    }
    Some(page)
}

/// EXP-783: how many events one `history_page` answer carries — the
/// contract's `steerFeed.historyPageMax`, which the relay's zod enforces.
pub const HISTORY_PAGE_MAX: u32 = domain::contract::STEER_FEED_HISTORY_PAGE_MAX;

/// The largest `history_chunk` frame: what one eNet envelope can carry with
/// room to spare for the envelope itself.
const PAGE_FRAME_MAX: usize = crate::enet_steer::CHAT_MAX_BYTES - 4096;

/// ONE `history_chunk` answering a page ask, serialized — the live
/// publisher's and the STORED REPLAY's (the envelope names the session). The
/// page is already bounded to [`HISTORY_PAGE_MAX`] events, so it is one frame
/// with `done: true`; an oversize frame is dropped down to an EMPTY done
/// chunk, which leaves the asking viewer with "nothing older" instead of a
/// page that never arrives.
pub fn history_chunk_frame(request_id: &str, page: Vec<(u64, ActivityEvent)>) -> String {
    let (seqs, events): (Vec<u64>, Vec<ActivityEvent>) = page.into_iter().unzip();
    let frame = ClientFrame::HistoryChunk {
        session_id: None,
        request_id: request_id.to_string(),
        events,
        seqs,
        done: true,
    }
    .to_json();
    if frame.len() < PAGE_FRAME_MAX {
        return frame;
    }
    log::warn!("steer history: page {request_id} too large — answering empty");
    ClientFrame::HistoryChunk {
        session_id: None,
        request_id: request_id.to_string(),
        events: Vec::new(),
        seqs: Vec::new(),
        done: true,
    }
    .to_json()
}

/// A latest-wins STATE slot (the fixed kinds and `workflow`), never a
/// transcript row: what `activity_synced`'s first/last seq skip.
pub(crate) fn is_state_slot(event: &ActivityEvent) -> bool {
    slot_of(event).is_some() || matches!(event, ActivityEvent::Workflow(_))
}

/// EXP-764: drop ONE session's journal — the hosts call it when a repo-less
/// run is purged whole. Returns whether a file went; a missing file is not
/// an error, anything else is logged. The writer keeps one open append
/// handle, so unlinking after the run ended never loses it a write.
pub fn remove_journal(data_dir: &Path, session_id: &str) -> bool {
    let Some(path) = journal_path(data_dir, session_id) else {
        return false;
    };
    // EXP-906: the diff sidecar goes with its journal, quietly.
    if let Some(sidecar) = diff_sidecar_path(data_dir, session_id) {
        let _ = fs::remove_file(sidecar);
    }
    match fs::remove_file(&path) {
        Ok(()) => true,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
        Err(err) => {
            log::warn!("steer history: remove {}: {err}", path.display());
            false
        }
    }
}

/// Remove journal files whose mtime is older than `max_age`; `None` =
/// unlimited retention, which removes nothing. Returns how many files went.
pub fn prune_journals(data_dir: &Path, max_age: Option<Duration>) -> usize {
    let Some(max_age) = max_age else {
        return 0;
    };
    let dir = journal_dir(data_dir);
    let Ok(entries) = fs::read_dir(&dir) else {
        return 0;
    };
    let now = SystemTime::now();
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        // EXP-906: a journal and its diff sidecar age together.
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !(name.ends_with(".jsonl") || name.ends_with(".diff.json")) {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|meta| meta.modified()) else {
            continue;
        };
        let Ok(age) = now.duration_since(modified) else {
            continue; // a clock skew into the future is not "old"
        };
        if age > max_age && fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    if removed > 0 {
        log::info!("steer history: pruned {removed} journal file(s) older than {max_age:?}");
    }
    removed
}

/// EXP-886: apply this machine's "Keep session history" setting
/// (`coding::session_retention`, read fresh from `settings.json`) to BOTH
/// halves of a finished run's local history — the journals here and the
/// resume records in `runs.json`. Unlimited (the default) touches nothing.
/// Blocking file I/O: hosts call it off their UI/select threads, at boot and
/// whenever the desktop setting changes. Returns (journals, records) removed.
pub fn prune_session_history(data_dir: &Path) -> (usize, usize) {
    let retention = coding::session_retention::load(data_dir);
    if retention.is_none() {
        return (0, 0);
    }
    let journals = prune_journals(data_dir, retention);
    let records = coding::session_retention::prune_run_records(data_dir, retention);
    (journals, records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "exp-steer-history-{tag}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// EXP-1051: a one-segment context breakdown, keyed by its base size.
    fn context_layout(base: i64) -> ActivityEvent {
        ActivityEvent::context_layout(vec![crate::frames::ContextSegment {
            key: crate::frames::ContextSegmentKey::Base,
            tokens: base,
            source: crate::frames::ContextSegmentSource::Measured,
            detail: None,
        }])
    }

    #[test]
    fn journal_path_rejects_ids_that_escape_the_directory() {
        let dir = Path::new("/data");
        assert_eq!(
            journal_path(dir, "sess-1"),
            Some(PathBuf::from("/data/journal/sess-1.jsonl"))
        );
        assert_eq!(journal_path(dir, ""), None);
        assert_eq!(journal_path(dir, ".."), None);
        assert_eq!(journal_path(dir, "../../etc/passwd"), None);
        assert_eq!(journal_path(dir, "a/b"), None);
        assert_eq!(journal_path(dir, "a\\b"), None);
        assert_eq!(journal_path(dir, "a\nb"), None);
        assert_eq!(journal_path(dir, &"x".repeat(129)), None);
    }

    #[test]
    fn writes_and_reads_back_one_line_per_event() {
        let dir = temp_dir("roundtrip");
        let mut writer = JournalWriter::open(&dir, "sess-1").unwrap();
        writer.append(&ActivityEvent::narration("first"));
        writer.append(&ActivityEvent::UserMessage {
            text: "hi".to_string(),
            subagent_id: None,
            at: Some(7),
        });
        drop(writer);

        let raw = fs::read_to_string(journal_path(&dir, "sess-1").unwrap()).unwrap();
        assert_eq!(raw.lines().count(), 2, "one line per event");

        let events = read_journal(&dir, "sess-1").unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], ActivityEvent::narration("first"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_reopened_writer_appends_instead_of_truncating() {
        let dir = temp_dir("append");
        let mut writer = JournalWriter::open(&dir, "sess-1").unwrap();
        writer.append(&ActivityEvent::narration("first"));
        drop(writer);
        let mut writer = JournalWriter::open(&dir, "sess-1").unwrap();
        assert!(writer.bytes() > 0, "picks the existing size up");
        writer.append(&ActivityEvent::narration("second"));
        drop(writer);

        let events = read_journal(&dir, "sess-1").unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[1], ActivityEvent::narration("second"));
        let _ = fs::remove_dir_all(&dir);
    }

    /// EXP-785 (pre-cleanup train): a journal line written before the tool
    /// call carried an `id` and a `toolKind` still parses AND still reaches
    /// the wire — the relay drops a `tool` frame without them, so the reader
    /// stamps the line's own position and `other` on its way out. A row that
    /// has them keeps them.
    #[test]
    fn a_pre_exp_785_tool_line_is_given_an_id_and_a_kind() {
        let dir = temp_dir("legacy-tool");
        let path = journal_path(&dir, "sess-1").unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            concat!(
                r#"{"kind":"narration","text":"before"}"#,
                "\n",
                r#"{"kind":"tool","name":"Edit","detail":"a.rs"}"#,
                "\n",
                r#"{"kind":"tool","name":"Bash","id":"tc-9","toolKind":"execute"}"#,
                "\n",
            ),
        )
        .unwrap();

        let events = read_journal_seq(&dir, "sess-1").unwrap();
        assert_eq!(events.len(), 3);
        let (seq, legacy) = &events[1];
        assert_eq!(*seq, 1);
        assert_eq!(
            legacy,
            &ActivityEvent::Tool {
                name: "Edit".to_string(),
                detail: Some("a.rs".to_string()),
                // The line index: unique in the file, stable across replays.
                id: Some("journal-1".to_string()),
                tool_kind: Some(crate::frames::ToolKind::Other),
                subagent_id: None,
                at: None,
            }
        );
        assert_eq!(
            events[2].1,
            ActivityEvent::Tool {
                name: "Bash".to_string(),
                detail: None,
                id: Some("tc-9".to_string()),
                tool_kind: Some(crate::frames::ToolKind::Execute),
                subagent_id: None,
                at: None,
            }
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_journal_reads_as_none() {
        let dir = temp_dir("missing");
        assert_eq!(read_journal(&dir, "sess-nope"), None);
        assert_eq!(read_journal(&dir, "../escape"), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_reader_folds_latest_wins_kinds_to_the_end() {
        let dir = temp_dir("fold");
        let mut writer = JournalWriter::open(&dir, "sess-1").unwrap();
        writer.append(&ActivityEvent::ConfigState {
            options: vec![],
            current_mode: Some("plan".to_string()),
            modes: None,
            commands: None,
            at: None,
        });
        writer.append(&ActivityEvent::diff("old diff"));
        writer.append(&ActivityEvent::narration("prose"));
        writer.append(&ActivityEvent::Usage {
            context_used: 1,
            context_size: 2,
            cost_usd: None,
            at: None,
        });
        writer.append(&ActivityEvent::diff("new diff"));
        // EXP-784: the rate-limit slot folds like the other three.
        writer.append(&ActivityEvent::rate_limit("allowed_warning", None, None));
        writer.append(&ActivityEvent::ConfigState {
            options: vec![],
            current_mode: Some("bypassPermissions".to_string()),
            modes: None,
            commands: None,
            at: None,
        });
        // EXP-785: a tool_update is a ROW — it pages and replays in place.
        writer.append(&ActivityEvent::tool_update("tc-1", None, None));
        writer.append(&ActivityEvent::rate_limit("rejected", Some(9), None));
        // EXP-1051: the context breakdown folds too, right behind `usage` —
        // a `/clear` republishes it and only the newest survives.
        writer.append(&context_layout(21_000));
        writer.append(&context_layout(18_500));
        // EXP-848: the turn slot folds too — only the newest edge survives.
        writer.append(&ActivityEvent::turn(crate::frames::TurnState::Started));
        writer.append(&ActivityEvent::turn(crate::frames::TurnState::Ended));
        drop(writer);

        let events = read_journal(&dir, "sess-1").unwrap();
        assert_eq!(events.len(), 8, "two rows + six folded slots");
        assert_eq!(events[0], ActivityEvent::narration("prose"));
        assert_eq!(events[1], ActivityEvent::tool_update("tc-1", None, None));
        // LATEST_REPLAY_ORDER: config_state, usage, context_layout,
        // rate_limit, turn, diff — newest of each.
        assert!(matches!(
            &events[2],
            ActivityEvent::ConfigState { current_mode: Some(mode), .. } if mode == "bypassPermissions"
        ));
        assert!(matches!(&events[3], ActivityEvent::Usage { .. }));
        assert_eq!(events[4], context_layout(18_500));
        assert_eq!(events[5], ActivityEvent::rate_limit("rejected", Some(9), None));
        assert_eq!(events[6], ActivityEvent::turn(crate::frames::TurnState::Ended));
        assert_eq!(events[7], ActivityEvent::diff("new diff"));
        // A page carries the rows and never a slot.
        let page = read_journal_page(&dir, "sess-1", u64::MAX, 10).unwrap();
        let kinds: Vec<&ActivityEvent> = page.iter().map(|(_, event)| event).collect();
        assert_eq!(
            kinds,
            vec![
                &ActivityEvent::narration("prose"),
                &ActivityEvent::tool_update("tc-1", None, None)
            ]
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// EXP-783: a page ask is answered from the file, bounded, oldest first,
    /// as ONE done chunk — and a session this machine never ran reads empty.
    #[test]
    fn a_page_ask_is_answered_with_one_chunk() {
        let dir = temp_dir("page");
        let mut writer = JournalWriter::open(&dir, "sess-1").unwrap();
        for i in 0..6 {
            writer.append(&ActivityEvent::narration(&format!("row {i}")));
        }
        writer.append(&ActivityEvent::diff("state, never a page row"));
        drop(writer);

        let page = read_journal_page(&dir, "sess-1", 5, 2).unwrap();
        assert_eq!(
            page,
            vec![
                (3, ActivityEvent::narration("row 3")),
                (4, ActivityEvent::narration("row 4")),
            ]
        );
        assert_eq!(
            history_chunk_frame("h7", page),
            r#"{"t":"history_chunk","requestId":"h7","events":[{"kind":"narration","text":"row 3"},{"kind":"narration","text":"row 4"}],"seqs":[3,4],"done":true}"#
        );
        assert_eq!(read_journal_page(&dir, "sess-elsewhere", 5, 2), None);
        assert_eq!(
            history_chunk_frame("h8", Vec::new()),
            r#"{"t":"history_chunk","requestId":"h8","events":[],"seqs":[],"done":true}"#
        );
        assert!(is_state_slot(&ActivityEvent::diff("x")));
        assert!(!is_state_slot(&ActivityEvent::narration("x")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unparsable_lines_are_skipped() {
        let dir = temp_dir("torn");
        let mut writer = JournalWriter::open(&dir, "sess-1").unwrap();
        writer.append(&ActivityEvent::narration("kept"));
        drop(writer);
        let path = journal_path(&dir, "sess-1").unwrap();
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        // A torn tail after a crash, an unknown future kind, and a blank line.
        file.write_all(b"{\"kind\":\"narra\n\n{\"kind\":\"from_the_future\"}\n")
            .unwrap();
        drop(file);

        let events = read_journal(&dir, "sess-1").unwrap();
        assert_eq!(events, vec![ActivityEvent::narration("kept")]);
        let _ = fs::remove_dir_all(&dir);
    }

    /// EXP-850 §3 review — a running workflow republishes its whole card up
    /// to once a second. Written in full, one long workflow burns the file
    /// cap and the REST of the run is never recorded. The writer keeps at
    /// most one frame per id per [`WORKFLOW_JOURNAL_INTERVAL`], always the
    /// first and always a terminal one, and touches no other kind.
    #[test]
    fn a_running_workflow_is_coalesced_to_one_frame_per_interval() {
        let dir = temp_dir("workflow-throttle");
        let mut writer = JournalWriter::open(&dir, "sess-1").unwrap();
        let card = |id: &str, status: crate::WorkflowStatus, agents: usize| {
            ActivityEvent::workflow(crate::WorkflowState {
                id: id.to_string(),
                name: "wire-probe".to_string(),
                status,
                agents: (0..agents)
                    .map(|index| crate::WorkflowAgent {
                        index: index as u32,
                        label: format!("agent {index}"),
                        ..crate::WorkflowAgent::default()
                    })
                    .collect(),
                ..crate::WorkflowState::default()
            })
        };
        let start = std::time::Instant::now();
        // One frame a second for two minutes, two workflows at a time, with a
        // narration row between them so the ordinary kinds are visible.
        for tick in 0..120u64 {
            let now = start + Duration::from_secs(tick);
            writer.append_at(&card("wf-1", crate::WorkflowStatus::Running, 8), now);
            writer.append_at(&card("wf-2", crate::WorkflowStatus::Running, 8), now);
            writer.append_at(&ActivityEvent::narration(&format!("row {tick}")), now);
        }
        // The terminal frame is never throttled, whenever it lands.
        writer.append_at(
            &card("wf-1", crate::WorkflowStatus::Completed, 8),
            start + Duration::from_secs(120),
        );
        drop(writer);

        let events = read_journal(&dir, "sess-1").unwrap();
        let rows = events
            .iter()
            .filter(|event| matches!(event, ActivityEvent::Narration { .. }))
            .count();
        assert_eq!(rows, 120, "no ordinary kind is touched");
        // On disk: 12 frames an id (t=0, then every 10 s) plus the terminal
        // one, not 240.
        let written: Vec<String> = fs::read_to_string(journal_path(&dir, "sess-1").unwrap())
            .unwrap()
            .lines()
            .filter(|line| line.contains(r#""kind":"workflow""#))
            .map(str::to_string)
            .collect();
        assert_eq!(written.len(), 25, "{}", written.len());
        // …and the fold still replays exactly one card per id, the newest.
        let cards: Vec<&crate::WorkflowState> = events
            .iter()
            .filter_map(|event| match event {
                ActivityEvent::Workflow(workflow) => Some(workflow),
                _ => None,
            })
            .collect();
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].id, "wf-1");
        assert_eq!(cards[0].status, crate::WorkflowStatus::Completed);
        assert_eq!(cards[1].id, "wf-2");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_writer_stops_at_the_byte_cap() {
        let dir = temp_dir("cap");
        let path = journal_path(&dir, "sess-1").unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, vec![b'\n'; JOURNAL_FILE_CAP as usize + 1]).unwrap();
        let before = fs::metadata(&path).unwrap().len();

        let mut writer = JournalWriter::open(&dir, "sess-1").unwrap();
        writer.append(&ActivityEvent::narration("dropped"));
        drop(writer);

        assert_eq!(
            fs::metadata(&path).unwrap().len(),
            before,
            "nothing more is recorded past the cap"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// EXP-906: the post-mortem's failure — 31 worktree snapshots of ~540 KiB
    /// each took a 16.9 MB journal to its cap at 21:10 while the agent worked
    /// on until 22:25, and every late viewer replayed a frozen run. The diff
    /// slot now lives in an overwritten sidecar: forty full-size snapshots
    /// leave the file small, the narration after them is recorded, and the
    /// replay carries exactly the NEWEST diff.
    #[test]
    fn forty_full_size_diffs_never_fill_the_file_and_the_narration_survives() {
        let dir = temp_dir("diff-sidecar");
        let mut writer = JournalWriter::open(&dir, "sess-1").unwrap();
        let big = "+".repeat(crate::activity::DIFF_MAX);
        for i in 0..40 {
            writer.append(&ActivityEvent::narration(&format!("step {i}")));
            writer.append(&ActivityEvent::diff(format!("{i}:{big}")));
        }
        writer.append(&ActivityEvent::narration("still recording"));
        let bytes = writer.bytes();
        assert_eq!(writer.lines(), 81, "every diff still holds one line");
        drop(writer);

        assert!(
            bytes < 64 * 1024,
            "the file holds prose and markers only, not {bytes} bytes"
        );
        let sidecar = diff_sidecar_path(&dir, "sess-1").unwrap();
        assert!(sidecar.is_file(), "the newest diff sits in the sidecar");

        let events = read_journal_seq(&dir, "sess-1").unwrap();
        let rows: Vec<&ActivityEvent> = events.iter().map(|(_, event)| event).collect();
        assert_eq!(rows.len(), 42, "41 narration rows + ONE diff");
        assert_eq!(rows[40], &ActivityEvent::narration("still recording"));
        let (seq, last) = events.last().unwrap();
        assert!(
            matches!(last, ActivityEvent::Diff { diff, .. } if diff.starts_with("39:")),
            "the replay carries the newest snapshot"
        );
        // The marker of the 40th diff is line 79 (0-based): 40 narrations +
        // 39 earlier markers before it.
        assert_eq!(*seq, 79);
        // A transcript page never carries the diff.
        let page = read_journal_page(&dir, "sess-1", u64::MAX, 100).unwrap();
        assert!(page.iter().all(|(_, event)| !matches!(event, ActivityEvent::Diff { .. })));
        let _ = fs::remove_dir_all(&dir);
    }

    /// EXP-906: the sidecar keeps updating after the file stopped — the
    /// Changes face of a late viewer must read the run's LAST patch, not the
    /// one from the minute the cap hit.
    #[test]
    fn the_sidecar_outlives_the_cap_and_the_cap_is_reported_once() {
        let dir = temp_dir("cap-notice");
        let path = journal_path(&dir, "sess-1").unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, vec![b'\n'; JOURNAL_FILE_CAP as usize + 1]).unwrap();

        let mut writer = JournalWriter::open(&dir, "sess-1").unwrap();
        assert!(!writer.take_cap_notice(), "nothing to report before an append");
        writer.append(&ActivityEvent::narration("dropped"));
        assert!(writer.take_cap_notice(), "the append that hit the cap reports it");
        assert!(!writer.take_cap_notice(), "…exactly once");
        writer.append(&ActivityEvent::diff("after the cap"));
        assert!(!writer.take_cap_notice());
        drop(writer);

        let events = read_journal(&dir, "sess-1").unwrap();
        assert_eq!(events, vec![ActivityEvent::diff("after the cap")]);
        let _ = fs::remove_dir_all(&dir);
    }

    /// A journal written before EXP-906 carries its diffs IN the file and has
    /// no sidecar; it still folds to the newest one. A sidecar, when present,
    /// wins over any in-line diff.
    #[test]
    fn legacy_in_line_diffs_still_fold_and_a_sidecar_wins() {
        let dir = temp_dir("legacy-diff");
        let path = journal_path(&dir, "sess-1").unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut legacy = String::new();
        for event in [
            ActivityEvent::narration("old"),
            ActivityEvent::diff("first"),
            ActivityEvent::diff("second"),
        ] {
            legacy.push_str(&serde_json::to_string(&event).unwrap());
            legacy.push('\n');
        }
        fs::write(&path, legacy).unwrap();

        let events = read_journal(&dir, "sess-1").unwrap();
        assert_eq!(
            events,
            vec![ActivityEvent::narration("old"), ActivityEvent::diff("second")]
        );

        // The same run, resumed on this build: the sidecar takes over.
        let mut writer = JournalWriter::open(&dir, "sess-1").unwrap();
        writer.append(&ActivityEvent::diff("third"));
        drop(writer);
        let events = read_journal_seq(&dir, "sess-1").unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[1], (3, ActivityEvent::diff("third")));
        let _ = fs::remove_dir_all(&dir);
    }

    /// EXP-906: the sidecar is removed and pruned with its journal.
    #[test]
    fn remove_and_prune_take_the_sidecar_along() {
        let dir = temp_dir("sidecar-lifecycle");
        let mut writer = JournalWriter::open(&dir, "sess-1").unwrap();
        writer.append(&ActivityEvent::diff("x"));
        drop(writer);
        let sidecar = diff_sidecar_path(&dir, "sess-1").unwrap();
        assert!(sidecar.is_file());
        assert!(remove_journal(&dir, "sess-1"));
        assert!(!sidecar.exists(), "removed with the journal");

        let mut writer = JournalWriter::open(&dir, "sess-2").unwrap();
        writer.append(&ActivityEvent::diff("y"));
        drop(writer);
        let sidecar = diff_sidecar_path(&dir, "sess-2").unwrap();
        let old = SystemTime::now() - Duration::from_secs(48 * 3600);
        filetime_set(&journal_path(&dir, "sess-2").unwrap(), old);
        filetime_set(&sidecar, old);
        assert_eq!(prune_journals(&dir, Some(Duration::from_secs(3600))), 2);
        assert!(!sidecar.exists(), "pruned with the journal");
        let _ = fs::remove_dir_all(&dir);
    }

    /// EXP-764: one session's journal goes on request; a missing one, or an
    /// id that could escape the directory, is a quiet `false`.
    #[test]
    fn remove_journal_drops_exactly_one_session() {
        let dir = temp_dir("remove-journal");
        for id in ["keep", "gone"] {
            let mut writer = JournalWriter::open(&dir, id).unwrap();
            writer.append(&ActivityEvent::narration(id));
            drop(writer);
        }
        assert!(remove_journal(&dir, "gone"));
        assert!(read_journal(&dir, "gone").is_none());
        assert!(read_journal(&dir, "keep").is_some());
        assert!(!remove_journal(&dir, "gone"));
        assert!(!remove_journal(&dir, "../keep"));
        assert!(read_journal(&dir, "keep").is_some());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn prune_removes_only_old_journals() {
        let dir = temp_dir("prune");
        let mut writer = JournalWriter::open(&dir, "fresh").unwrap();
        writer.append(&ActivityEvent::narration("fresh"));
        drop(writer);
        let mut writer = JournalWriter::open(&dir, "old").unwrap();
        writer.append(&ActivityEvent::narration("old"));
        drop(writer);
        // Age the second file by rewinding its mtime an hour.
        let old = journal_path(&dir, "old").unwrap();
        let hour_ago = SystemTime::now() - Duration::from_secs(3600);
        filetime_set(&old, hour_ago);

        // EXP-886: unlimited retention removes nothing, however old.
        assert_eq!(prune_journals(&dir, None), 0);
        assert!(read_journal(&dir, "old").is_some());

        assert_eq!(prune_journals(&dir, Some(Duration::from_secs(600))), 1);
        assert!(read_journal(&dir, "fresh").is_some());
        assert!(read_journal(&dir, "old").is_none());
        // A missing directory is a no-op, not an error.
        assert_eq!(
            prune_journals(Path::new("/nonexistent-exp"), Some(Duration::from_secs(1))),
            0
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// EXP-886: the one host entry point reads the setting. No key = keep
    /// everything; a window drops journals past it (records ride the same
    /// call, tested in `coding::run_registry`).
    #[test]
    fn prune_session_history_follows_the_setting() {
        let dir = temp_dir("retention");
        let mut writer = JournalWriter::open(&dir, "ancient").unwrap();
        writer.append(&ActivityEvent::narration("ancient"));
        drop(writer);
        let ancient = journal_path(&dir, "ancient").unwrap();
        filetime_set(&ancient, SystemTime::now() - Duration::from_secs(400 * 24 * 60 * 60));

        assert_eq!(prune_session_history(&dir), (0, 0), "unlimited by default");
        assert!(read_journal(&dir, "ancient").is_some());

        fs::write(dir.join("settings.json"), r#"{"sessionRetentionDays":"soon"}"#).unwrap();
        assert_eq!(prune_session_history(&dir), (0, 0), "garbage is unlimited");

        coding::session_retention::save_days(&dir, Some(365)).unwrap();
        assert_eq!(prune_session_history(&dir).0, 1);
        assert!(read_journal(&dir, "ancient").is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    /// Set a file's mtime without pulling in a crate for it.
    fn filetime_set(path: &Path, when: SystemTime) {
        let file = OpenOptions::new().write(true).open(path).unwrap();
        file.set_times(fs::FileTimes::new().set_modified(when)).unwrap();
    }
}

#[cfg(test)]
mod exp850_tests {
    use super::*;
    use crate::frames::{BackgroundTask, BackgroundTaskKind, TurnState};
    use crate::workflow::{WorkflowState, WorkflowStatus};
    use std::time::UNIX_EPOCH;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "exp-steer-history-{tag}-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn workflow(id: &str, status: WorkflowStatus) -> ActivityEvent {
        ActivityEvent::workflow(WorkflowState {
            id: id.to_string(),
            name: "wire-probe".to_string(),
            status,
            ..WorkflowState::default()
        })
    }

    fn label(event: &ActivityEvent) -> String {
        match event {
            ActivityEvent::Workflow(workflow) => format!("workflow:{}", workflow.id),
            other => serde_json::to_value(other)
                .ok()
                .and_then(|value| {
                    value.get("kind").and_then(|kind| kind.as_str()).map(str::to_string)
                })
                .unwrap_or_default(),
        }
    }

    /// EXP-850 §2/§3: an append-only file cannot rewrite a slot, so the READ
    /// folds both new kinds back — the strip to one entry, the cards to one
    /// per id — and replays them in the relay's order.
    #[test]
    fn the_file_folds_the_new_slots_and_replays_them_in_order() {
        let dir = temp_dir("exp850-fold");
        let mut writer = JournalWriter::open(&dir, "sess-1").expect("a writer");
        writer.append(&ActivityEvent::narration("working"));
        writer.append(&ActivityEvent::diff("+ line"));
        writer.append(&workflow("wf-1", WorkflowStatus::Running));
        writer.append(&ActivityEvent::background_tasks(vec![BackgroundTask {
            id: "b1".to_string(),
            kind: BackgroundTaskKind::Shell,
            description: "sleep".to_string(),
            tool_id: None,
            started_at: None,
        }]));
        writer.append(&ActivityEvent::turn(TurnState::Started));
        writer.append(&workflow("wf-2", WorkflowStatus::Running));
        writer.append(&workflow("wf-1", WorkflowStatus::Completed));
        writer.append(&ActivityEvent::background_tasks(Vec::new()));

        let events = read_journal(&dir, "sess-1").expect("a journal");
        assert_eq!(
            events.iter().map(label).collect::<Vec<_>>(),
            vec![
                "narration",
                "turn",
                "workflow:wf-1",
                "workflow:wf-2",
                "background_tasks",
                "diff",
            ]
        );
        // Latest-wins per id and whole: the completed card and the empty strip.
        assert!(matches!(
            &events[2],
            ActivityEvent::Workflow(state) if state.status == WorkflowStatus::Completed
        ));
        assert!(matches!(
            &events[4],
            ActivityEvent::BackgroundTasks { tasks, .. } if tasks.is_empty()
        ));
        let _ = fs::remove_dir_all(&dir);
    }

    /// EXP-850 §3: the file's fold keeps the same 16 cards the in-memory
    /// journal does, so a replay off disk and one off memory agree.
    #[test]
    fn the_file_fold_caps_the_workflow_cards() {
        let dir = temp_dir("exp850-cap");
        let mut writer = JournalWriter::open(&dir, "sess-1").expect("a writer");
        for i in 0..20 {
            writer.append(&workflow(&format!("wf-{i}"), WorkflowStatus::Running));
        }
        let events = read_journal(&dir, "sess-1").expect("a journal");
        let cards: Vec<String> = events.iter().map(label).collect();
        assert_eq!(cards.len(), HISTORY_WORKFLOW_CAP);
        assert_eq!(cards.first().map(String::as_str), Some("workflow:wf-4"));
        assert_eq!(cards.last().map(String::as_str), Some("workflow:wf-19"));
        let _ = fs::remove_dir_all(&dir);
    }

    /// A `workflow` is STATE, so an older-page request never carries one — the
    /// asking client already holds the newest per id.
    #[test]
    fn a_history_page_never_carries_a_workflow_card() {
        let dir = temp_dir("exp850-page");
        let mut writer = JournalWriter::open(&dir, "sess-1").expect("a writer");
        writer.append(&ActivityEvent::narration("one"));
        writer.append(&workflow("wf-1", WorkflowStatus::Running));
        writer.append(&ActivityEvent::narration("two"));
        writer.append(&ActivityEvent::background_tasks(Vec::new()));
        let page = read_journal_page(&dir, "sess-1", 10, 50).expect("a page");
        assert_eq!(
            page.iter().map(|(_, event)| label(event)).collect::<Vec<_>>(),
            vec!["narration", "narration"]
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
