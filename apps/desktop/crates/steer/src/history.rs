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
//! The read path is the mirror image: when a viewer asks the relay for a room
//! that is not up, the relay routes a `history_request` down the device's
//! control socket, the device reads the file back and republishes it through
//! [`publish_history`] as a short-lived publisher — hello, the events, then
//! `bye {outcome:"history"}`. The server never stores a transcript.
//!
//! Three rules the file inherits from the in-memory journal:
//!
//! - The three latest-wins kinds (`config_state`, `usage`, `diff`) are
//!   APPENDED like anything else (an append-only file cannot rewrite a slot);
//!   [`read_journal`] folds them back into one slot each, replayed at the end
//!   in the relay's own `LATEST_REPLAY_ORDER`.
//! - A file stops growing at [`JOURNAL_FILE_CAP`] (one log line, then
//!   silence): a runaway run must never fill the disk, and the head of a
//!   transcript is the part worth keeping.
//! - Files older than [`JOURNAL_MAX_AGE`] are pruned at daemon/app boot
//!   ([`prune_journals`]).
//!
//! Redaction is NOT done here. Events reach the writer already scrubbed by
//! the emitter and stamped by the publisher's `prepare_for_journal`, exactly
//! as they go on the wire — the file holds what the relay held, never more.

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use futures_util::SinkExt;
use tokio_tungstenite::tungstenite::Message;

use crate::frames::{ActivityEvent, ClientFrame};
use crate::publisher::RELAY_MAX_PAYLOAD_BYTES;
use crate::{dial, DialError};

/// A journal file stops growing here. Well past any real run (the in-memory
/// journal's own budget is 4 MiB) and small enough that a thousand of them
/// cannot surprise a laptop.
pub const JOURNAL_FILE_CAP: u64 = 16 * 1024 * 1024;

/// Journals older than this are removed at boot (60 days).
pub const JOURNAL_MAX_AGE: Duration = Duration::from_secs(60 * 24 * 60 * 60);

/// `bye` outcome that tells the relay this publisher was a history replay,
/// not a live session ending — it answers the parked viewers with
/// `activity_synced` before closing the room.
pub const HISTORY_OUTCOME: &str = "history";

/// Frames [`publish_history`] sends between yields, and how long it yields.
///
/// EXP-781: a replay pushes a whole journal as fast as the socket accepts it,
/// while the relay fans every frame out into a per-viewer buffer it caps at
/// 512 KiB (`hub.ts` `VIEWER_HIGH_WATER`) and evicts the viewer over. A tight
/// send loop is the burst that trips that cap; a short yield every batch gives
/// the fan-out room to drain. Cheap either way — a replay is a background
/// task and nothing waits on it.
const HISTORY_BATCH_FRAMES: usize = 64;
const HISTORY_BATCH_PAUSE: Duration = Duration::from_millis(20);

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

/// The append-only writer the live publisher feeds. Every event is flushed on
/// the spot: a run that is killed (or a machine that loses power) still has
/// everything it published on disk.
pub struct JournalWriter {
    path: PathBuf,
    file: BufWriter<File>,
    /// Bytes on disk, including whatever a resumed run's file already held.
    written: u64,
    /// EXP-783: lines on disk — the wire sequence the next event gets.
    lines: u64,
    /// Past the cap (or after a write error): every further append is a no-op.
    stopped: bool,
}

impl JournalWriter {
    /// Open (creating the directory and the file) for append. `None` when the
    /// id is unusable or the filesystem refuses — journaling is best-effort
    /// and never gates publishing.
    pub fn open(data_dir: &Path, session_id: &str) -> Option<Self> {
        let path = journal_path(data_dir, session_id)?;
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
            file: BufWriter::new(file),
            written,
            lines,
            stopped: false,
        })
    }

    /// Append one already-redacted event. Best-effort by design: a full disk
    /// must never stall the publisher's pump loop.
    pub fn append(&mut self, event: &ActivityEvent) {
        if self.stopped {
            return;
        }
        if self.written >= JOURNAL_FILE_CAP {
            log::warn!(
                "steer history: {} reached {JOURNAL_FILE_CAP} bytes — no longer recording",
                self.path.display()
            );
            self.stopped = true;
            return;
        }
        let Ok(mut line) = serde_json::to_string(event) else {
            return;
        };
        line.push('\n');
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
/// (`config_state`, `usage`, `rate_limit`, `diff`; EXP-784 added the third).
const SLOT_COUNT: usize = 4;

fn slot_of(event: &ActivityEvent) -> Option<usize> {
    match event {
        ActivityEvent::ConfigState { .. } => Some(0),
        ActivityEvent::Usage { .. } => Some(1),
        ActivityEvent::RateLimit { .. } => Some(2),
        ActivityEvent::Diff { .. } => Some(3),
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
    let mut slots: [Option<(u64, ActivityEvent)>; SLOT_COUNT] = [None, None, None, None];
    for (seq, line) in BufReader::new(file).lines().enumerate() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(event) = serde_json::from_str::<ActivityEvent>(&line) else {
            continue;
        };
        let seq = seq as u64;
        match slot_of(&event) {
            Some(slot) => slots[slot] = Some((seq, event)),
            None => events.push((seq, event)),
        }
    }
    events.extend(slots.into_iter().flatten());
    Some(events)
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
        .filter(|(seq, event)| *seq < before_seq && slot_of(event).is_none())
        .collect();
    if page.len() > limit {
        page.drain(..page.len() - limit);
    }
    Some(page)
}

/// EXP-783: how many events one `history_page` answer carries — the
/// contract's `steerFeed.historyPageMax`, which the relay's zod enforces.
pub const HISTORY_PAGE_MAX: u32 = domain::contract::STEER_FEED_HISTORY_PAGE_MAX;

/// ONE `history_chunk` answering a page ask, serialized — the shape BOTH
/// routes send: the live publisher (`session_id: None`, its socket already
/// belongs to the room) and, EXP-796, the control socket (`Some`, one socket
/// serves every session this machine ran). The page is already bounded to
/// [`HISTORY_PAGE_MAX`] events, so it is one frame with `done: true`; an
/// oversize frame is dropped down to an EMPTY done chunk rather than severing
/// the socket, which leaves the asking viewer with "nothing older" instead of
/// a dead connection.
pub fn history_chunk_frame(
    session_id: Option<&str>,
    request_id: &str,
    page: Vec<(u64, ActivityEvent)>,
) -> String {
    let (seqs, events): (Vec<u64>, Vec<ActivityEvent>) = page.into_iter().unzip();
    let frame = ClientFrame::HistoryChunk {
        session_id: session_id.map(str::to_string),
        request_id: request_id.to_string(),
        events,
        seqs,
        done: true,
    }
    .to_json();
    if frame.len() < RELAY_MAX_PAYLOAD_BYTES {
        return frame;
    }
    log::warn!("steer history: page {request_id} too large — answering empty");
    ClientFrame::HistoryChunk {
        session_id: session_id.map(str::to_string),
        request_id: request_id.to_string(),
        events: Vec::new(),
        seqs: Vec::new(),
        done: true,
    }
    .to_json()
}

/// EXP-796: the page a control-socket ask gets — [`read_journal_page`]
/// bounded to [`HISTORY_PAGE_MAX`], or an EMPTY page when this machine has no
/// journal for the session (the live publisher answers the same: `nothing
/// older`, never silence, so the viewer's spinner ends).
pub fn history_page_for(data_dir: &Path, ask: &crate::control_channel::HistoryPageAsk) -> Vec<(u64, ActivityEvent)> {
    read_journal_page(
        data_dir,
        &ask.session_id,
        ask.before_seq,
        ask.limit.min(HISTORY_PAGE_MAX) as usize,
    )
    .unwrap_or_default()
}

/// EXP-764: drop ONE session's journal — the hosts call it when a repo-less
/// run is purged whole. Returns whether a file went; a missing file is not
/// an error, anything else is logged. The writer keeps one open append
/// handle, so unlinking after the run ended never loses it a write.
pub fn remove_journal(data_dir: &Path, session_id: &str) -> bool {
    let Some(path) = journal_path(data_dir, session_id) else {
        return false;
    };
    match fs::remove_file(&path) {
        Ok(()) => true,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
        Err(err) => {
            log::warn!("steer history: remove {}: {err}", path.display());
            false
        }
    }
}

/// Remove journal files whose mtime is older than `max_age`. Called once at
/// daemon/app boot; returns how many files went.
pub fn prune_journals(data_dir: &Path, max_age: Duration) -> usize {
    let dir = journal_dir(data_dir);
    let Ok(entries) = fs::read_dir(&dir) else {
        return 0;
    };
    let now = SystemTime::now();
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
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

/// Republish a stored transcript to the relay as a one-shot publisher.
///
/// `dial_url` is the mint's `url` field, used AS-IS (§8.2 — the ticket rides
/// its query string and the URL is never reconstructed). The socket sends
/// `hello`, the events, then `bye {outcome:"history"}` and closes; inbound
/// frames are ignored throughout (no steering, no kill, no reconnect — the
/// session this replays is over). Deliberately NOT the live
/// [`crate::publisher`]: none of its state machine applies to a replay.
pub async fn publish_history(
    dial_url: &str,
    session_id: &str,
    events: &[(u64, ActivityEvent)],
) -> Result<(), String> {
    let mut ws = match dial(dial_url).await {
        Ok(stream) => stream,
        Err(DialError::Unauthorized) => return Err("relay rejected the ticket".to_string()),
        Err(DialError::Other(reason)) => return Err(reason),
    };
    let hello = ClientFrame::Hello {
        session_id,
        issue_id: None,
        // EXP-90: always the explicit opt-out (an absent key means "public"
        // to legacy relays).
        activity_public: Some(false),
    }
    .to_json();
    ws.send(Message::Text(hello))
        .await
        .map_err(|err| format!("hello failed: {err}"))?;
    // The same frame the live publisher opens with: this replay IS the whole
    // transcript, so anything a previous replay left in the room's log would
    // be duplicated in front of it.
    ws.send(Message::Text(ClientFrame::ActivityReset.to_json()))
        .await
        .map_err(|err| format!("activity_reset failed: {err}"))?;
    for (sent, (seq, event)) in events.iter().enumerate() {
        if sent > 0 && sent % HISTORY_BATCH_FRAMES == 0 {
            tokio::time::sleep(HISTORY_BATCH_PAUSE).await;
        }
        let framed = ClientFrame::Activity {
            event: event.clone(),
            // EXP-783: the replay carries the ORIGINAL sequence, so a viewer
            // that already holds part of this run splices instead of losing it.
            seq: Some(*seq),
        }
        .to_json();
        if framed.len() >= RELAY_MAX_PAYLOAD_BYTES {
            // Same rule as the live publisher: an oversize frame would make
            // the relay sever the socket, so it is a skip, not a failure.
            continue;
        }
        ws.send(Message::Text(framed))
            .await
            .map_err(|err| format!("activity failed: {err}"))?;
    }
    let bye = ClientFrame::Bye {
        outcome: Some(HISTORY_OUTCOME),
    }
    .to_json();
    ws.send(Message::Text(bye))
        .await
        .map_err(|err| format!("bye failed: {err}"))?;
    let _ = ws.close(None).await;
    Ok(())
}

// ---------------------------------------------------------------------------
// The host seam: one relay `history_request`, served end to end
// ---------------------------------------------------------------------------

/// Sessions this device is replaying right now. A viewer that redials while
/// the replay is in flight makes the relay ask again, and a SECOND publisher
/// for the same room would evict the first (`CLOSE_REPLACED`) and restart the
/// transcript from zero — so an ask for a session already in flight is
/// dropped. Cheap to clone; one per host.
#[derive(Clone, Default)]
pub struct HistoryInFlight(std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>>);

impl HistoryInFlight {
    pub fn new() -> Self {
        Self::default()
    }

    fn claim(&self, session_id: &str) -> bool {
        match self.0.lock() {
            Ok(mut set) => set.insert(session_id.to_string()),
            Err(_) => false,
        }
    }

    fn release(&self, session_id: &str) {
        if let Ok(mut set) = self.0.lock() {
            set.remove(session_id);
        }
    }
}

/// The device half of EXP-773, for both hosts (desktop app + CLI daemon):
/// read the session's journal, mint a publisher ticket for it, and republish
/// it to the waiting room. Non-blocking — everything happens on the steer
/// runtime, so the control socket's select loop is never held up.
///
/// Silent no-ops, all of them normal: no journal for that id (the run
/// happened on another machine, or the file was pruned), an empty journal, a
/// relay that reports disabled, or a replay already in flight. The relay's
/// 20s timer tells the viewer; there is no failure frame to send.
pub fn serve_history_request(
    runtime: &crate::SteerRuntime,
    tickets: std::sync::Arc<dyn crate::publisher::PublisherTickets>,
    data_dir: PathBuf,
    session_id: String,
    in_flight: HistoryInFlight,
) {
    // Never republish over a run this machine is still hosting: the relay
    // would hand the room to the replay and close the live publisher as
    // REPLACED, which is terminal for it. Reachable whenever the row reads
    // `ended` while the run is alive (see [`crate::publisher::is_publishing`]).
    if crate::publisher::is_publishing(&session_id) {
        log::info!("steer history: {session_id} is running here; not replaying over its publisher");
        return;
    }
    if !in_flight.claim(&session_id) {
        log::debug!("steer history: replay for {session_id} already in flight");
        return;
    }
    runtime.handle().spawn(async move {
        serve_history_task(tickets, data_dir, &session_id).await;
        in_flight.release(&session_id);
    });
}

/// EXP-796, the other half for both hosts: answer a control-socket
/// `history_page` with ONE `history_chunk` naming the session. The journal
/// read runs on a blocking task of the steer runtime; `reply` queues the
/// frame on the control socket the ask arrived on. No ticket, no publisher —
/// the relay routes by session id, and the room that asked is a lingering
/// one this device already replayed into.
pub fn serve_history_page(
    runtime: &crate::SteerRuntime,
    data_dir: PathBuf,
    ask: crate::control_channel::HistoryPageAsk,
    reply: crate::control_channel::HistoryPageReply,
) {
    runtime.handle().spawn(async move {
        let read_ask = ask.clone();
        let page =
            match tokio::task::spawn_blocking(move || history_page_for(&data_dir, &read_ask)).await
        {
            Ok(page) => page,
            Err(err) => {
                log::warn!("steer history: page read panicked: {err}");
                Vec::new()
            }
        };
        log::info!(
            "steer history: page of {} event(s) for {} below {}",
            page.len(),
            ask.session_id,
            ask.before_seq
        );
        let (seqs, events): (Vec<u64>, Vec<ActivityEvent>) = page.into_iter().unzip();
        reply(ClientFrame::HistoryChunk {
            session_id: Some(ask.session_id),
            request_id: ask.request_id,
            events,
            seqs,
            done: true,
        });
    });
}

async fn serve_history_task(
    tickets: std::sync::Arc<dyn crate::publisher::PublisherTickets>,
    data_dir: PathBuf,
    session_id: &str,
) {
    let read_dir = data_dir.clone();
    let read_id = session_id.to_string();
    let events =
        match tokio::task::spawn_blocking(move || read_journal_seq(&read_dir, &read_id)).await
    {
        Ok(Some(events)) if !events.is_empty() => events,
        Ok(_) => {
            log::info!("steer history: no stored transcript for {session_id}");
            return;
        }
        Err(err) => {
            log::warn!("steer history: journal read panicked: {err}");
            return;
        }
    };
    let minted = match tokio::task::spawn_blocking(move || tickets.mint()).await {
        Ok(Ok(Some(ticket))) => ticket,
        Ok(Ok(None)) => {
            log::info!("steer history: relay disabled; not replaying {session_id}");
            return;
        }
        Ok(Err(err)) => {
            log::warn!("steer history: ticket mint for {session_id} failed: {err}");
            return;
        }
        Err(err) => {
            log::warn!("steer history: mint task panicked: {err}");
            return;
        }
    };
    match publish_history(&minted.url, session_id, &events).await {
        Ok(()) => log::info!(
            "steer history: replayed {} event(s) for {session_id}",
            events.len()
        ),
        Err(reason) => log::warn!("steer history: replay of {session_id} failed: {reason}"),
    }
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
        drop(writer);

        let events = read_journal(&dir, "sess-1").unwrap();
        assert_eq!(events.len(), 6, "two rows + four folded slots");
        assert_eq!(events[0], ActivityEvent::narration("prose"));
        assert_eq!(events[1], ActivityEvent::tool_update("tc-1", None, None));
        // LATEST_REPLAY_ORDER: config_state, usage, rate_limit, diff — newest
        // of each.
        assert!(matches!(
            &events[2],
            ActivityEvent::ConfigState { current_mode: Some(mode), .. } if mode == "bypassPermissions"
        ));
        assert!(matches!(&events[3], ActivityEvent::Usage { .. }));
        assert_eq!(events[4], ActivityEvent::rate_limit("rejected", Some(9), None));
        assert_eq!(events[5], ActivityEvent::diff("new diff"));
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

    /// EXP-796: a control-socket page ask is answered from the same file the
    /// publisher route reads, bounded, oldest first, naming the session —
    /// and a session this machine never ran gets an EMPTY done chunk.
    #[test]
    fn a_control_page_ask_is_answered_with_one_named_chunk() {
        use crate::control_channel::HistoryPageAsk;
        let dir = temp_dir("control-page");
        let mut writer = JournalWriter::open(&dir, "sess-1").unwrap();
        for i in 0..6 {
            writer.append(&ActivityEvent::narration(&format!("row {i}")));
        }
        writer.append(&ActivityEvent::diff("state, never a page row"));
        drop(writer);

        let ask = HistoryPageAsk {
            session_id: "sess-1".to_string(),
            request_id: "h7".to_string(),
            before_seq: 5,
            limit: 2,
        };
        let page = history_page_for(&dir, &ask);
        assert_eq!(
            page,
            vec![
                (3, ActivityEvent::narration("row 3")),
                (4, ActivityEvent::narration("row 4")),
            ]
        );
        assert_eq!(
            history_chunk_frame(Some("sess-1"), "h7", page),
            r#"{"t":"history_chunk","sessionId":"sess-1","requestId":"h7","events":[{"kind":"narration","text":"row 3"},{"kind":"narration","text":"row 4"}],"seqs":[3,4],"done":true}"#
        );
        // The limit is capped at the contract's page max.
        let wide = HistoryPageAsk { limit: u32::MAX, before_seq: u64::MAX, ..ask.clone() };
        assert_eq!(history_page_for(&dir, &wide).len(), 6.min(HISTORY_PAGE_MAX as usize));
        // No journal here: an empty done page, never silence.
        let missing = HistoryPageAsk { session_id: "sess-elsewhere".to_string(), ..ask };
        assert_eq!(history_page_for(&dir, &missing), Vec::new());
        assert_eq!(
            history_chunk_frame(Some("sess-elsewhere"), "h8", Vec::new()),
            r#"{"t":"history_chunk","sessionId":"sess-elsewhere","requestId":"h8","events":[],"seqs":[],"done":true}"#
        );

        // The whole seam: the reply lands ONE frame, off the runtime.
        let runtime = crate::SteerRuntime::new().unwrap();
        let (tx, rx) = flume::bounded::<String>(1);
        serve_history_page(
            &runtime,
            dir.clone(),
            HistoryPageAsk {
                session_id: "sess-1".to_string(),
                request_id: "h9".to_string(),
                before_seq: 1,
                limit: 10,
            },
            Box::new(move |frame| {
                let _ = tx.send(frame.to_json());
            }),
        );
        let frame = rx.recv_timeout(Duration::from_secs(10)).expect("one chunk");
        assert_eq!(
            frame,
            r#"{"t":"history_chunk","sessionId":"sess-1","requestId":"h9","events":[{"kind":"narration","text":"row 0"}],"seqs":[0],"done":true}"#
        );
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

        assert_eq!(prune_journals(&dir, Duration::from_secs(600)), 1);
        assert!(read_journal(&dir, "fresh").is_some());
        assert!(read_journal(&dir, "old").is_none());
        // A missing directory is a no-op, not an error.
        assert_eq!(prune_journals(Path::new("/nonexistent-exp"), JOURNAL_MAX_AGE), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    /// Set a file's mtime without pulling in a crate for it.
    fn filetime_set(path: &Path, when: SystemTime) {
        let file = OpenOptions::new().write(true).open(path).unwrap();
        file.set_times(fs::FileTimes::new().set_modified(when)).unwrap();
    }
}
