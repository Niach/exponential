//! The per-coding-session publisher (masterplan-v3 §8.4–§8.7, steering v2 =
//! EXP-249): publish the scrubbed activity stream, replay the session's full
//! journal on every (re)connect, inject remote `input` and semantic `answer`
//! frames, honor kill, and auto-reconnect resuming the room.
//!
//! EXP-249 removed the binary PTY mirror it used to carry (no client ever
//! joined `channel:'pty'`): there is no read-loop tee, no ring, no `resync`
//! and no geometry here anymore. What remains on the socket is TEXT only.
//!
//! **Best-effort and non-blocking** (§8.4): if the relay is disabled or
//! unreachable the coding session runs fine locally — the publisher never
//! gates the terminal. Control frames and activity events ride ONE unbounded
//! channel that is never dropped or reordered. Remote `input` frames ride a
//! second ordered channel to a dedicated task (EXP-514), so their
//! choreography — sleeps, EXP-511 image downloads — never stalls the pump
//! loop's ping tick past the relay's idle-publisher timeout.
//!
//! Steering is seamless and owner-only (EXP-312): there is no operator claim
//! and no perm tier, viewer tickets are minted only for the session owner,
//! and the LOCAL user is never gated — their keystrokes go straight to the
//! PTY while the relay forwards the joined viewer's input.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use api::error::ApiError;
use api::steer::MintedTicket;
use api::trpc::TrpcClient;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::Message;

use crate::activity::{
    AnswerLink, CommandLink, ConfigChange, ConfigLink, RemoteAnswer, SessionAgent,
};
use crate::commands::parse_command;
use crate::frames::{
    ActivityEvent, ClientFrame, ServerFrame, CLOSE_REPLACED, CLOSE_UNAUTHORIZED,
};
use crate::history::JournalWriter;
use crate::journal::ActivityJournal;
use crate::{dial, Backoff, DialError, SteerRuntime, WsStream, BACKOFF_RESET_AFTER};

/// The relay's WebSocket `maxPayloadLength` (bytes). A text frame at or past
/// this makes the relay sever the connection — killing the whole activity
/// stream — so oversize activity frames are dropped client-side instead.
pub(crate) const RELAY_MAX_PAYLOAD_BYTES: usize = 1024 * 1024;

/// §8.7: surfaced after two consecutive fresh-ticket 401s (never silently
/// retry a skewed clock — the native failure mode this fixes).
const CLOCK_SKEW_ERROR: &str = "Steer relay rejected the connection (ticket expired on \
     arrival) — check that this machine's clock is in sync (NTP).";

// ---------------------------------------------------------------------------
// Spec, hooks, tickets
// ---------------------------------------------------------------------------

/// What to publish (§8.4 handshake): the `coding_sessions` row id keys the
/// relay room; the issue id rides along for the phone's session list.
#[derive(Clone, Debug, Default)]
pub struct PublishSpec {
    pub session_id: String,
    pub issue_id: Option<String>,
    /// EXP-773: the app/daemon data dir whose `journal/` subdirectory keeps
    /// this session's DURABLE transcript ([`crate::history`]). Every event
    /// this publisher sends is appended there, so the run can be replayed
    /// from disk long after the process is gone. `None` = don't record (the
    /// examples and the relay integration tests).
    pub journal_dir: Option<PathBuf>,
}

/// Publisher-ticket source, injectable for tests. Blocking (reqwest) — the loop
/// wraps calls in `spawn_blocking`. `Ok(None)` = instance reports disabled ⇒
/// skip publishing entirely (§8.4 #1).
pub trait PublisherTickets: Send + Sync + 'static {
    fn mint(&self) -> Result<Option<MintedTicket>, ApiError>;
}

/// Production tickets over the account's tRPC client:
/// `steer.mintTicket({kind:"publisher", codingSessionId})`.
pub struct TrpcPublisherTickets {
    pub trpc: Arc<TrpcClient>,
    pub coding_session_id: String,
}

impl PublisherTickets for TrpcPublisherTickets {
    fn mint(&self) -> Result<Option<MintedTicket>, ApiError> {
        Ok(api::steer::mint_publisher_ticket(&self.trpc, &self.coding_session_id)?.into_ticket())
    }
}

/// Why the publisher asked the coding flow to tear the session down.
///
/// EXP-283: an explicit relay `kill` frame is the ONLY relay signal that may
/// end the local session. A relay-initiated socket CLOSE — whatever its code —
/// is transport-level and must never kill live local work: the relay's
/// idle-publisher detector closes quiet sockets with `CLOSE_SESSION_ENDED`
/// (a laptop sleeping >90s reads that close on wake), and treating it as a
/// kill was tearing down live agents mid-session. Real session ends always
/// also flip the synced `coding_sessions` row, which the §8.8 kill-watch
/// handles durably.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KillSignal {
    /// A relay `kill` frame (steer.killSession fan-out / a steerer's kill).
    RemoteKill,
}

/// Remote `input` bytes → the shared PTY writer (§6.5). Aliased so the
/// `&[u8]`-taking `Fn` type stays under clippy's `type_complexity` bar.
pub type InputHook = Arc<dyn Fn(&[u8]) + Send + Sync>;

/// EXP-511: attachment id → the local file the agent should read. BLOCKING
/// (HTTP + a disk write); the input task calls it from `spawn_blocking`.
/// `Err` is a human-readable reason for the log — the embed then stays as it
/// arrived.
pub type AttachmentHook = Arc<dyn Fn(&str) -> Result<PathBuf, String> + Send + Sync>;

/// The seam back into the app (§8.9). Every hook is invoked on the steer
/// runtime — implementations marshal to the gpui foreground themselves where
/// needed. Cheap-and-non-blocking applies to all of them EXCEPT
/// [`PublisherHooks::attachments`], which runs on a blocking task off the
/// dedicated input task (EXP-514) — never on the pump loop.
pub struct PublisherHooks {
    /// Remote `input` frames that are NOT whole composer messages (a bare
    /// `\r`, an `\x1b` interrupt). EXP-773: the engine's
    /// `line_buffered_input` is the only implementation left.
    pub write_input: InputHook,
    /// Relay-initiated teardown: kill the `claude` child; the exit hook then
    /// ends the `coding_sessions` row (idempotent server-side).
    pub kill: Arc<dyn Fn(KillSignal) + Send + Sync>,
    /// Terminal-state errors worth surfacing (clock skew, repeated rejects).
    pub error: Arc<dyn Fn(String) + Send + Sync>,
    /// EXP-249: semantic `answer` frames → the ACP engine, which owns the
    /// live question state. `None` makes `answer` a no-op.
    pub answers: Option<Arc<AnswerLink>>,
    /// EXP-383: which agent CLI the session runs — the slash-command catalog
    /// a composer message is matched against.
    pub agent: SessionAgent,
    /// Whole composer messages → the engine (which turns them into a
    /// `session/prompt`). `None` writes them through
    /// [`Self::write_input`] instead.
    pub text_sink: Option<Arc<dyn Fn(String) + Send + Sync>>,
    /// EXP-511: localizes the image embeds of a steered message — every
    /// `![image](/api/attachments/{id})` token is downloaded with this
    /// device's own API auth and replaced by the local file path, so the
    /// agent just reads the file. `None` (no account, tests) leaves the
    /// tokens alone: the agent then sees the URL and can still fetch it over
    /// MCP. Build with [`image_localizer`].
    pub attachments: Option<AttachmentHook>,
    /// EXP-724: the remote slash-command seam. A composer message whose
    /// first token is a catalog `/name` for [`Self::agent`] crosses here
    /// instead of becoming prose. `None` = commands ride the ordinary
    /// message path (the pre-EXP-724 behaviour).
    pub commands: Option<Arc<CommandLink>>,
    /// EXP-746: the live-config seam. `set_config`/`set_mode` cross to the
    /// ACP engine here — never through [`Self::write_input`], because there
    /// are no keystrokes that could express them. `None` (tests) is a
    /// documented no-op.
    pub config: Option<Arc<ConfigLink>>,
}

/// [`PublisherHooks::attachments`] over an account's tRPC client (EXP-511):
/// downloads into `dest_dir` — `<worktree>/.exp-steer-images`, which the
/// launcher git-excludes so a steerer's screenshots never reach the PR diff.
pub fn image_localizer(trpc: Arc<TrpcClient>, dest_dir: PathBuf) -> AttachmentHook {
    Arc::new(move |attachment_id| {
        api::attachments::download_image(&trpc, attachment_id, &dest_dir)
            .map_err(|err| err.to_string())
    })
}

/// The embed token a steering client sends for an attached image (the shared
/// `buildSteerImageMessage` template, byte-identical on web/iOS/Android). The
/// id is UUID-shaped so nothing else in a message can be mistaken for one.
fn image_embed_pattern() -> &'static regex::Regex {
    static PATTERN: OnceLock<regex::Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        regex::Regex::new(
            r"(?i)!\[image\]\(/api/attachments/([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\)",
        )
        .expect("the image embed pattern is a valid regex")
    })
}

/// EXP-511: `(needle, the embed token it replaced)` for every image localized
/// on this session — what [`restore_image_embeds`] rewrites back out. Created
/// in [`publish`] and shared across reconnects — `pump_connection` is
/// re-entered per connection, but the journal it replays outlives them.
///
/// EXP-698: each image contributes TWO kinds of entry — the whole
/// `Image #N: <path>` MANIFEST LINE, and the bare `<path>` — so a transcript
/// echo of the line collapses to one embed token and a stray mention of the
/// path elsewhere still resolves.
///
/// Push order does NOT encode the precedence: one attachment can be sent
/// twice under different numbers (`Image #1:` in one message, `Image #3:` in
/// the next), and by the second message the bare path is already in the map
/// AHEAD of the new line — replacing it first would strand the `Image #3: `
/// prefix in the feed. [`restore_image_embeds`] therefore runs the LINE
/// needles in a first pass and the bare paths in a second.
type ImageEmbedMap = Arc<Mutex<Vec<(String, String)>>>;

/// The prefix every manifest-line needle starts with — how
/// [`restore_image_embeds`] tells a line needle from a bare path.
const MANIFEST_PREFIX: &str = "Image #";

/// One image's manifest line, as the agent reads it.
fn image_manifest_line(index: usize, target: &str) -> String {
    format!("{MANIFEST_PREFIX}{index}: {target}")
}

/// Localize a steered message's image embeds for the agent.
///
/// EXP-698 changed the SHAPE (not the wire — the composer's message is still
/// `buildSteerImageMessage`'s): the embed block is REMOVED from the prose and
/// replaced by a numbered manifest appended after a blank line —
///
/// ```text
/// crop [Image #1] please
///
/// Image #1: /…/.exp-steer-images/a.png
/// Image #2: /…/.exp-steer-images/b.png
/// ```
///
/// The numbers are the EMBED ORDER, which is the image order the composer's
/// `[Image #N]` markers count in, so a marker in the prose and a line in the
/// manifest name the same file. Every image gets a line whether or not a
/// marker references it. Substituting each embed in place (the pre-EXP-698
/// behaviour) left the agent a bare path with nothing tying it to the
/// sentence that meant it.
///
/// A download that fails keeps its URL in the manifest line — the agent can
/// still fetch it over MCP, which is the pre-EXP-511 behaviour.
async fn localize_image_embeds(
    data: String,
    hook: &AttachmentHook,
    embeds: &ImageEmbedMap,
) -> String {
    let mut tokens: Vec<(String, String)> = Vec::new();
    for captures in image_embed_pattern().captures_iter(&data) {
        let token = captures[0].to_string();
        if !tokens.iter().any(|(existing, _)| existing == &token) {
            tokens.push((token, captures[1].to_string()));
        }
    }
    if tokens.is_empty() {
        return data;
    }
    let mut manifest: Vec<String> = Vec::with_capacity(tokens.len());
    let mut restores: Vec<(String, String)> = Vec::new();
    for (index, (token, id)) in tokens.iter().enumerate() {
        let number = index + 1;
        let hook = hook.clone();
        let requested = id.clone();
        match tokio::task::spawn_blocking(move || hook(&requested)).await {
            Ok(Ok(path)) => {
                let path = path.display().to_string();
                let line = image_manifest_line(number, &path);
                restores.push((line.clone(), token.clone()));
                restores.push((path, token.clone()));
                manifest.push(line);
            }
            Ok(Err(reason)) => {
                log::warn!("steer publisher: attachment {id} not localized ({reason})");
                // No local file — the manifest carries the URL itself, and
                // the line restores to the token like any other.
                let line = image_manifest_line(number, token);
                restores.push((line.clone(), token.clone()));
                manifest.push(line);
            }
            Err(join_err) => {
                log::warn!("steer publisher: attachment {id} download panicked: {join_err}");
                let line = image_manifest_line(number, token);
                restores.push((line.clone(), token.clone()));
                manifest.push(line);
            }
        }
    }
    if let Ok(mut embeds) = embeds.lock() {
        for (needle, token) in restores {
            if !embeds.iter().any(|(known, _)| *known == needle) {
                embeds.push((needle, token));
            }
        }
    }
    // Strip the embed block out of the prose, then hang the manifest off it.
    // A line that held NOTHING but an embed goes entirely (the composer puts
    // them on their own lines); an inline one leaves its line tidied, so the
    // agent never reads a double space where a token used to be.
    let mut kept: Vec<String> = Vec::new();
    for line in data.split('\n') {
        let mut next = line.to_string();
        let mut touched = false;
        for (token, _) in &tokens {
            if next.contains(token.as_str()) {
                next = next.replace(token.as_str(), "");
                touched = true;
            }
        }
        if touched {
            if next.trim().is_empty() {
                continue;
            }
            next = tidy_gaps(&next);
        }
        kept.push(next);
    }
    let prose = kept.join("\n");
    let prose = prose.trim_end();
    let manifest = manifest.join("\n");
    if prose.is_empty() {
        manifest
    } else {
        format!("{prose}\n\n{manifest}")
    }
}

/// Collapse the gap a removed embed token left: runs of 2+ spaces/tabs become
/// one, trailing ones go. Mirrors the tidy `renumber_image_markers` applies on
/// the client when a marker is dropped.
fn tidy_gaps(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut run = false;
    for ch in line.chars() {
        if ch == ' ' || ch == '\t' {
            run = true;
            continue;
        }
        if run {
            out.push(' ');
            run = false;
        }
        out.push(ch);
    }
    out
}

/// The reverse of [`localize_image_embeds`], applied to every text field of an
/// activity event before it is journaled or published: the agent's transcript
/// echoes the manifest LINE (and sometimes just the local path) we
/// substituted, and a viewer's echo-dedupe (and its image rendering) only work
/// against the embed token they sent. Local paths must never reach the
/// published feed at all, hence every field, not just `user_message`.
///
/// TWO passes (EXP-698): every manifest-LINE needle first, then the bare
/// paths. An echoed `Image #1: /…/a.png` must collapse to ONE `![image](…)`
/// token rather than leaving the `Image #1: ` prefix stranded in front of it,
/// and a single map order cannot guarantee that — resending the same
/// attachment under a different number puts its bare path in the map ahead of
/// the new line. The pass split makes the precedence structural.
fn restore_image_embeds(event: &mut ActivityEvent, embeds: &ImageEmbedMap) {
    let Ok(embeds) = embeds.lock() else { return };
    if embeds.is_empty() {
        return; // the overwhelmingly common case — no allocation, no walk
    }
    for field in event.text_fields_mut() {
        for lines_pass in [true, false] {
            for (needle, token) in embeds.iter() {
                if needle.starts_with(MANIFEST_PREFIX) != lines_pass {
                    continue;
                }
                if field.contains(needle.as_str()) {
                    *field = field.replace(needle.as_str(), token);
                }
            }
        }
    }
}

/// Everything an activity event needs before it enters the journal: the §8.4
/// stamp (a re-publish replays the ORIGINAL timeline) and the EXP-511 reverse
/// rewrite.
fn prepare_for_journal(event: &mut ActivityEvent, embeds: &ImageEmbedMap) {
    if event.at_mut().is_none() {
        *event.at_mut() = Some(now_millis());
    }
    restore_image_embeds(event, embeds);
}

// ---------------------------------------------------------------------------
// Handle + commands
// ---------------------------------------------------------------------------

/// Control-path commands (§8.4 backpressure rule: these ride an UNBOUNDED
/// channel — never dropped, never reordered against each other).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PublisherCmd {
    /// §P7: one PUBLIC activity event (already redacted) → `activity` text
    /// frame. Rides the unbounded control channel like the others; low-rate
    /// (per assistant turn / debounced diff), so it never backs up.
    Activity(ActivityEvent),
    /// Clean end: send `bye {outcome}` and close (child exit, local stop,
    /// kill-watch).
    Shutdown { outcome: Option<String> },
}

/// The coding-flow's handle onto a running publisher task.
pub struct PublisherHandle {
    cmd_tx: flume::Sender<PublisherCmd>,
    running: Arc<AtomicBool>,
}

/// A cheap `Send + Sync` clone of the publisher's control sender, dedicated to
/// §P7 activity events (the member-only activity channel). The activity
/// emitter thread holds one and pushes already-redacted narration/tool/diff
/// events; sending is fire-and-forget and never blocks the emitter (unbounded
/// flume). Sending after the publisher has
/// stopped is a harmless no-op (the pump drops the receiver).
#[derive(Clone)]
pub struct ActivitySender {
    cmd_tx: flume::Sender<PublisherCmd>,
}

impl ActivitySender {
    /// Publish one already-redacted activity event (best-effort).
    pub fn send(&self, event: ActivityEvent) {
        let _ = self.cmd_tx.send(PublisherCmd::Activity(event));
    }
}

impl PublisherHandle {
    /// A cheap [`ActivitySender`] for the §P7 activity emitter thread — pushes
    /// activity events onto the same unbounded control channel without
    /// coupling the emitter to the whole handle.
    pub fn activity_sender(&self) -> ActivitySender {
        ActivitySender {
            cmd_tx: self.cmd_tx.clone(),
        }
    }

    /// Clean end (§8.4 End): `bye {outcome}` + close(1000). Outcome format
    /// per spec: `exit:<code>` from the child-exit hook, `killed` for a
    /// kill-path teardown. Idempotent; stops any reconnect loop.
    pub fn shutdown(&self, outcome: Option<String>) {
        self.running.store(false, Ordering::SeqCst);
        let _ = self.cmd_tx.send(PublisherCmd::Shutdown { outcome });
    }

    /// §8.8 kill-switch entry: the synced `coding_sessions` row flipped to
    /// `ended` (or the local session was torn down out-of-band). Equivalent
    /// to `shutdown(Some("killed"))`.
    pub fn session_ended(&self) {
        self.shutdown(Some("killed".to_string()));
    }

    /// False once the publisher stopped for good (clean end, kill frame,
    /// 4002 replaced, skew give-up).
    pub fn is_active(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

/// Start publishing a coding session (§8.4). Non-blocking: spawns the task
/// onto the steer runtime and returns the handle immediately. Wire-up
/// contract (the coding-flow seam):
///
/// 1. `spawn_activity_emitter` with `handle.activity_sender()` — the events
///    this task journals and republishes;
/// 2. exit hook → `handle.shutdown(Some(format!("exit:{code}")))`;
/// 3. `sync::kill_watch` on_ended → `handle.session_ended()` (after killing
///    the child).
pub fn publish(
    runtime: &SteerRuntime,
    spec: PublishSpec,
    tickets: Arc<dyn PublisherTickets>,
    hooks: PublisherHooks,
) -> PublisherHandle {
    let (cmd_tx, cmd_rx) = flume::unbounded();
    let running = Arc::new(AtomicBool::new(true));
    let handle = PublisherHandle {
        cmd_tx,
        running: running.clone(),
    };
    runtime
        .handle()
        .spawn(run_publisher_loop(spec, tickets, hooks, cmd_rx, running));
    handle
}

// ---------------------------------------------------------------------------
// The task
// ---------------------------------------------------------------------------

/// The session's transcript in BOTH places it lives: the in-memory replay
/// journal (EXP-249, rebuilt on the wire after every reconnect) and the
/// durable per-session file (EXP-773, read back long after the process is
/// gone). One `push` keeps them in step, and the file is optional — a
/// publisher without a `journal_dir` behaves exactly as it did before.
struct Recorder {
    journal: ActivityJournal,
    file: Option<JournalWriter>,
}

impl Recorder {
    /// Record one already-prepared event (redacted upstream, stamped by
    /// `prepare_for_journal`): disk first, then memory, which takes it.
    fn push(&mut self, event: ActivityEvent) {
        if let Some(file) = self.file.as_mut() {
            file.append(&event);
        }
        self.journal.push(event);
    }
}

/// How one connection ended.
enum LoopEnd {
    /// `bye` sent, socket closed — the task is done.
    Clean,
    /// Relay closed with a code (§8.6 semantics apply).
    Closed(Option<u16>),
    /// Unexpected drop (socket error / EOF without close).
    Dropped,
}

async fn run_publisher_loop(
    spec: PublishSpec,
    tickets: Arc<dyn PublisherTickets>,
    hooks: PublisherHooks,
    cmd_rx: flume::Receiver<PublisherCmd>,
    running: Arc<AtomicBool>,
) {
    let hooks = Arc::new(hooks);
    // EXP-511: the session's localized image embeds — filled by the input
    // path, read by the activity path, and shared across reconnects.
    let embeds: ImageEmbedMap = Arc::new(Mutex::new(Vec::new()));
    // EXP-514: remote input is handled on its own session-lived task, fed in
    // order over this channel — the pump loop must never await a download.
    let input_tx = spawn_input_pump(hooks.clone(), embeds.clone());
    // EXP-249: the session's full published history. Every connection starts
    // with `activity_reset` + this journal, so a viewer joining a resumed room
    // (or after a relay restart) sees the session from its first event.
    // EXP-773 pairs it with the durable file the same events land in.
    let mut recorder = Recorder {
        journal: ActivityJournal::new(),
        file: spec
            .journal_dir
            .as_deref()
            .and_then(|dir| JournalWriter::open(dir, &spec.session_id)),
    };
    let mut backoff = Backoff::publisher();
    // §8.7: one immediate re-mint is allowed after a fresh-ticket 401; a
    // second consecutive 401 surfaces the clock-skew error and stops.
    let mut unauthorized_once = false;

    'reconnect: while running.load(Ordering::SeqCst) {
        // Mint (blocking reqwest off the reactor), then dial IMMEDIATELY (§8.7).
        let tickets_for_mint = tickets.clone();
        let minted = match tokio::task::spawn_blocking(move || tickets_for_mint.mint()).await {
            Ok(result) => result,
            Err(join_err) => {
                log::warn!("steer publisher: mint task panicked: {join_err}");
                running.store(false, Ordering::SeqCst);
                return;
            }
        };
        let url = match minted {
            Ok(Some(ticket)) => ticket.url,
            Ok(None) => {
                // Relay disabled: a normal state — no remote mirror (§8.4).
                log::info!("steer publisher: relay disabled; not publishing");
                running.store(false, Ordering::SeqCst);
                return;
            }
            Err(ApiError::Unauthorized) => {
                log::info!("steer publisher: session token dead; not publishing");
                running.store(false, Ordering::SeqCst);
                return;
            }
            Err(ApiError::Http { status: 403, message }) => {
                // Not the session owner — authorization is mint-time (§8.0).
                (hooks.error)(format!("Steer publish refused: {message}"));
                running.store(false, Ordering::SeqCst);
                return;
            }
            Err(err) => {
                log::debug!("steer publisher: mint failed: {err}");
                if sleep_or_shutdown(backoff.next_delay(), &cmd_rx, &mut recorder, &embeds, &running)
                    .await
                    .is_break()
                {
                    return;
                }
                continue 'reconnect;
            }
        };

        let mut ws = match dial(&url).await {
            Ok(stream) => stream,
            Err(DialError::Unauthorized) => {
                if unauthorized_once {
                    // Two consecutive fresh tickets rejected: clock skew.
                    (hooks.error)(CLOCK_SKEW_ERROR.to_string());
                    running.store(false, Ordering::SeqCst);
                    return;
                }
                unauthorized_once = true;
                // Re-mint once and retry immediately (§8.7) — no backoff.
                continue 'reconnect;
            }
            Err(DialError::Other(reason)) => {
                log::debug!("steer publisher: connect failed: {reason}");
                if sleep_or_shutdown(backoff.next_delay(), &cmd_rx, &mut recorder, &embeds, &running)
                    .await
                    .is_break()
                {
                    return;
                }
                continue 'reconnect;
            }
        };
        unauthorized_once = false;

        // §8.4 #2 — hello creates the room (or resumes it on re-hello,
        // evicting any stale publisher with CLOSE_REPLACED).
        let hello = ClientFrame::Hello {
            session_id: &spec.session_id,
            issue_id: spec.issue_id.as_deref(),
            // EXP-90: the anonymous public-activity audience is removed —
            // always send the explicit opt-out, because an ABSENT key means
            // "public" to legacy relays (manual, independent deploys).
            activity_public: Some(false),
        }
        .to_json();
        if let Err(err) = ws.send(Message::Text(hello)).await {
            log::debug!("steer publisher: hello failed: {err}");
            if sleep_or_shutdown(backoff.next_delay(), &cmd_rx, &mut recorder, &embeds, &running)
                .await
                .is_break()
            {
                return;
            }
            continue 'reconnect;
        }
        log::info!("steer publisher: room {} live", spec.session_id);
        let established = Instant::now();

        // EXP-249 full-history re-publish: clear whatever the room (and its
        // viewers) still hold, then replay the journal in order. A resumed
        // `--continue` transcript seeds the journal from byte 0, so this is
        // the ONE deliberate place a feed is rebuilt.
        if !republish_history(&mut ws, &recorder.journal).await {
            log::debug!("steer publisher: history replay failed; reconnecting");
            if sleep_or_shutdown(backoff.next_delay(), &cmd_rx, &mut recorder, &embeds, &running)
                .await
                .is_break()
            {
                return;
            }
            continue 'reconnect;
        }

        let end =
            pump_connection(&mut ws, &hooks, &input_tx, &cmd_rx, &mut recorder, &embeds, &running)
                .await;

        match end {
            LoopEnd::Clean => {
                running.store(false, Ordering::SeqCst);
                return;
            }
            LoopEnd::Closed(Some(CLOSE_REPLACED)) => {
                // 4002: a newer publisher socket owns the room — this socket
                // must not fight it (expected during our own reconnect race).
                log::info!("steer publisher: replaced by a newer socket");
                running.store(false, Ordering::SeqCst);
                return;
            }
            LoopEnd::Closed(Some(CLOSE_UNAUTHORIZED)) => {
                if unauthorized_once {
                    (hooks.error)(CLOCK_SKEW_ERROR.to_string());
                    running.store(false, Ordering::SeqCst);
                    return;
                }
                unauthorized_once = true;
                continue 'reconnect; // re-mint once, immediately (§8.6/§8.7)
            }
            LoopEnd::Closed(_) | LoopEnd::Dropped => {
                // Any other close or unexpected drop: reconnect while the
                // session is still running, resuming the same room (§8.6).
                // That INCLUDES 4001 (EXP-283): the relay's idle-publisher
                // detector closes quiet sockets with CLOSE_SESSION_ENDED —
                // e.g. read on wake after a laptop sleep >90s — and a relay
                // close must never kill live local work. Real kills arrive as
                // the explicit `kill` frame or the §8.8 own-row Electric flip;
                // re-hello resets the relay's idle timer and resumes the room.
                // Reconnect promptly — the relay's staleTimer bounds the
                // grace window.
                if established.elapsed() >= BACKOFF_RESET_AFTER {
                    backoff.reset();
                }
                if !running.load(Ordering::SeqCst) {
                    return;
                }
                log::debug!("steer publisher: dropped; reconnecting");
                if sleep_or_shutdown(backoff.next_delay(), &cmd_rx, &mut recorder, &embeds, &running)
                    .await
                    .is_break()
                {
                    return;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Remote input (EXP-514: its own ordered task, off the pump loop)
// ---------------------------------------------------------------------------

// EXP-383: composer-message chunk tracking. A message arrives as ≤4 KiB
// text chunks closed by a bare `\r`; the FIRST chunk is where codex's
// leading-sigil guard applies and where pi's sink buffer opens. A chunk
// this stale without its `\r` is a dead message (client gone mid-send) —
// the next text chunk counts as a fresh composer open again.
const MESSAGE_STALENESS: Duration = Duration::from_secs(5);

/// The composer state one remote `input` frame threads to the next.
/// Session-lived (like the journal): a reconnect changes the socket, not the
/// composer the steerer is typing into.
#[derive(Default)]
struct InputState {
    message_chunk_at: Option<Instant>,
    /// Text chunks buffered for `text_sink` until the `\r`.
    sink_buffer: String,
    /// EXP-724: the composer-opening chunk was a catalog slash command — the
    /// whole message is buffered here until its `\r` hands it to the
    /// [`CommandLink`]. Same staleness rule as `sink_buffer`: a buffer this
    /// old without its `\r` is a dead message.
    command_buffer: Option<String>,
}

/// EXP-514: remote `input` frames are handled on this dedicated session-lived
/// task, fed strictly in order over an unbounded channel. EXP-511 embed
/// localization downloads sequentially with a 60s per-blob timeout, so a
/// multi-image message against a slow app server can stall for minutes —
/// awaited inline in the pump's select loop (which also owns the 30s ping
/// tick) that silence tripped the relay's 90s idle-publisher detector and
/// closed the room. Off-loop, pings and activity keep flowing while the
/// downloads run, and the single-task ordering preserves the composer
/// choreography (text chunks, then the submitting `\r`). The task drains what
/// was queued and ends when the last sender drops (publisher teardown).
fn spawn_input_pump(hooks: Arc<PublisherHooks>, embeds: ImageEmbedMap) -> flume::Sender<String> {
    let (input_tx, input_rx) = flume::unbounded::<String>();
    tokio::spawn(async move {
        let mut state = InputState::default();
        while let Ok(data) = input_rx.recv_async().await {
            handle_input(data, &hooks, &embeds, &mut state).await;
        }
    });
    input_tx
}

/// One remote `input` frame. Runs only on the input task (see
/// [`spawn_input_pump`]) — free to download attachments.
async fn handle_input(
    mut data: String,
    hooks: &PublisherHooks,
    embeds: &ImageEmbedMap,
    state: &mut InputState,
) {
    // EXP-511: localize the message's image embeds FIRST, so every consumer
    // below sees the paths the agent can actually read. EXP-698: the paths
    // ride a trailing `Image #N: <path>` manifest, so a localized message
    // starts with the sender's prose — or, when it was images only, with
    // `Image #1:`.
    if is_message_text(&data) {
        if let Some(localize) = &hooks.attachments {
            data = localize_image_embeds(data, localize, embeds).await;
        }
    }
    // EXP-724: a composer message whose first token is a catalog `/name` for
    // this agent is a COMMAND, not prose: it is buffered here and handed
    // whole to the engine on its `\r`.
    if let Some(commands) = &hooks.commands {
        let stale = state
            .message_chunk_at
            .is_none_or(|at| at.elapsed() >= MESSAGE_STALENESS);
        if stale {
            state.command_buffer = None;
        }
        if is_message_text(&data) {
            if let Some(buffer) = &mut state.command_buffer {
                buffer.push_str(&data);
                state.message_chunk_at = Some(Instant::now());
                return;
            }
            // Only the chunk that OPENS the composer can start a command
            // — `/compact` in the middle of a pasted paragraph is prose.
            if parse_command(&data, hooks.agent).is_some() {
                state.command_buffer = Some(data.clone());
                state.message_chunk_at = Some(Instant::now());
                return;
            }
        }
        if data == "\r" {
            if let Some(buffer) = state.command_buffer.take() {
                state.message_chunk_at = None;
                match parse_command(&buffer, hooks.agent) {
                    Some(command) => commands.submit(command),
                    // Unreachable (only the first token decides), but never
                    // drop a steerer's text on the floor: write it as prose.
                    None => (hooks.write_input)(buffer.as_bytes()),
                }
                return;
            }
        }
    }
    // Whole composer messages route to the engine, which submits them as one
    // `session/prompt` — so the trailing `\r` is swallowed too.
    if let Some(sink) = &hooks.text_sink {
        let stale = state
            .message_chunk_at
            .is_none_or(|at| at.elapsed() >= MESSAGE_STALENESS);
        if stale && !state.sink_buffer.is_empty() {
            state.sink_buffer.clear();
        }
        if is_message_text(&data) {
            state.sink_buffer.push_str(&data);
            state.message_chunk_at = Some(Instant::now());
            return;
        }
        if data == "\r" && !state.sink_buffer.is_empty() {
            sink(std::mem::take(&mut state.sink_buffer));
            state.message_chunk_at = None;
            return;
        }
    }
    if is_message_text(&data) {
        state.message_chunk_at = Some(Instant::now());
    } else if data == "\r" {
        state.message_chunk_at = None;
    }
    (hooks.write_input)(data.as_bytes())
}

/// One connection's select loop (§8.4's pseudocode, made real). Remote
/// `input` frames are FORWARDED to the session's input task (EXP-514), never
/// handled here — their choreography sleeps and downloads, and this loop owns
/// the ping tick the relay's idle detector watches.
async fn pump_connection(
    ws: &mut WsStream,
    hooks: &PublisherHooks,
    input_tx: &flume::Sender<String>,
    cmd_rx: &flume::Receiver<PublisherCmd>,
    recorder: &mut Recorder,
    embeds: &ImageEmbedMap,
    running: &Arc<AtomicBool>,
) -> LoopEnd {
    // REV2-X: Send periodic pings so the relay can detect dead publishers.
    // During plan mode (or any idle period), the desktop sends no activity,
    // and without pings, a dropped connection can sit undetected while UIs retry
    // endlessly against a stale `no_such_session`.
    const PING_INTERVAL: Duration = Duration::from_secs(30);
    // REV-41, mirroring control_channel's EXP-414: no inbound traffic (relay
    // pongs included) for this long means the path is dead — `send()` alone
    // can buffer into TCP retransmit for ~15min without erroring, so the
    // ping arm's send-side check can't detect a silently dead path (NAT
    // expiry, AP roam, VPN flap). The relay's idle detector closes the room
    // after 90s of publisher silence; without this watchdog the session
    // looks live locally while remote view/steer is dead until TCP gives up.
    // Dropping here reconnects promptly: re-mint, re-hello, journal replay.
    const LIVENESS_TIMEOUT: Duration = Duration::from_secs(90);
    let mut ping_interval = tokio::time::interval(PING_INTERVAL);
    ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // REV-41: any inbound frame (the relay's pongs to our pings included —
    // they surface through `ws.next()`) proves the path; silence past
    // LIVENESS_TIMEOUT means it is gone.
    let mut last_rx = Instant::now();
    loop {
        tokio::select! {
            // 1) local control commands (unbounded — never dropped).
            cmd = cmd_rx.recv_async() => {
                let Ok(cmd) = cmd else { return LoopEnd::Dropped };
                match cmd {
                    PublisherCmd::Activity(mut event) => {
                        // §P7: publish one already-redacted activity event.
                        // The relay fans it to the member activity audience.
                        // Stamp it first (the journal replays the ORIGINAL
                        // timeline after a reconnect) and put any localized
                        // image path back to the token the steerer sent.
                        prepare_for_journal(&mut event, embeds);
                        recorder.push(event.clone());
                        if !send_activity(ws, event).await {
                            return LoopEnd::Dropped;
                        }
                    }
                    PublisherCmd::Shutdown { outcome } => {
                        let bye = ClientFrame::Bye { outcome: outcome.as_deref() }.to_json();
                        let _ = ws.send(Message::Text(bye)).await;
                        let _ = ws.close(None).await; // 1000 normal closure
                        return LoopEnd::Clean;
                    }
                }
            }
            // 2) relay → publisher control frames.
            msg = ws.next() => {
                if matches!(msg, Some(Ok(_))) {
                    last_rx = Instant::now();
                }
                match msg {
                    Some(Ok(Message::Text(text))) => match ServerFrame::parse(&text) {
                        Some(ServerFrame::Input { data }) => {
                            // EXP-514: input choreography sleeps (Enter
                            // separation, picker revalidation) and downloads
                            // (EXP-511 embed localization — up to 60s per
                            // blob) — inline it starved the ping tick and the
                            // relay's 90s idle detector closed the room. The
                            // input task processes frames strictly in order;
                            // a send after teardown is a harmless no-op.
                            let _ = input_tx.send(data);
                        }
                        // EXP-249: the semantic answer path — the emitter owns
                        // question identity + the TUI key choreography, so the
                        // publisher only routes.
                        Some(ServerFrame::Answer { question_id, ask_id, keys, text }) => {
                            if let Some(answers) = &hooks.answers {
                                answers.submit(RemoteAnswer { question_id, ask_id, keys, text });
                            }
                        }
                        // EXP-746: live config. Like `answer`, never through
                        // `input_tx` — a model switch has no keystroke form,
                        // and only the engine holds the ACP session that can
                        // apply it. `None` (the PTY path) is a documented
                        // no-op: such a run advertises no chips to change.
                        Some(ServerFrame::SetConfig { id, value }) => {
                            if let Some(config) = &hooks.config {
                                config.submit(ConfigChange::Option { id, value });
                            }
                        }
                        Some(ServerFrame::SetMode { id }) => {
                            if let Some(config) = &hooks.config {
                                config.submit(ConfigChange::Mode { id });
                            }
                        }
                        Some(ServerFrame::Kill) => {
                            // §8.4: relay kill → end the session. The kill hook
                            // kills the child (whose exit hook ends the synced
                            // row); we close the room cleanly right away.
                            log::info!("steer publisher: kill received");
                            running.store(false, Ordering::SeqCst);
                            (hooks.kill)(KillSignal::RemoteKill);
                            let bye = ClientFrame::Bye { outcome: Some("killed") }.to_json();
                            let _ = ws.send(Message::Text(bye)).await;
                            let _ = ws.close(None).await;
                            return LoopEnd::Clean;
                        }
                        Some(ServerFrame::Bye { outcome }) => {
                            log::debug!("steer publisher: relay bye ({outcome:?})");
                            return LoopEnd::Dropped;
                        }
                        Some(ServerFrame::Error { code, message }) => {
                            log::debug!("steer publisher: relay error {code} ({message:?})");
                            return LoopEnd::Dropped;
                        }
                        Some(ServerFrame::StartSession { .. })
                        | Some(ServerFrame::CheckIn)
                        | Some(ServerFrame::HistoryRequest { .. }) => {
                            // Control-socket frames; never valid here. Ignore.
                        }
                        None => log::debug!("steer publisher: unparseable frame ignored"),
                    },
                    Some(Ok(Message::Close(frame))) => {
                        return LoopEnd::Closed(close_code(&frame));
                    }
                    Some(Ok(_binary_or_ping)) => {
                        // The relay speaks TEXT only since EXP-249; pings are
                        // answered by tungstenite internally, and the relay's
                        // pongs to OUR pings land here — bumping `last_rx`
                        // above is their whole job (REV-41).
                    }
                    Some(Err(err)) => {
                        log::debug!("steer publisher: socket error: {err}");
                        return LoopEnd::Dropped;
                    }
                    None => return LoopEnd::Dropped,
                }
            }
            // 3) periodic ping to keep the connection alive and let the relay
            // detect dead publishers (REV2-X: plan mode can idle for minutes),
            // and reconnect when the path has been silent past the watchdog
            // window (REV-41).
            _ = ping_interval.tick() => {
                if last_rx.elapsed() > LIVENESS_TIMEOUT {
                    log::debug!(
                        "steer publisher: no traffic for {:?} — reconnecting",
                        last_rx.elapsed()
                    );
                    return LoopEnd::Dropped;
                }
                if ws.send(Message::Ping(vec![])).await.is_err() {
                    return LoopEnd::Dropped;
                }
            }
        }
    }
}

/// `activity_reset` + the whole journal, in order. `false` on a socket error
/// (the caller reconnects and tries again from the top).
async fn republish_history(ws: &mut WsStream, journal: &ActivityJournal) -> bool {
    if ws
        .send(Message::Text(ClientFrame::ActivityReset.to_json()))
        .await
        .is_err()
    {
        return false;
    }
    for event in journal.replay() {
        if !send_activity(ws, event.clone()).await {
            return false;
        }
    }
    true
}

/// Send one activity event. The emitter caps event strings in UTF-8 bytes, but
/// JSON escaping can still inflate pathological content past the relay's frame
/// limit — dropping the event beats letting the relay close the socket, so an
/// oversize frame is a skip, not a failure.
async fn send_activity(ws: &mut WsStream, event: ActivityEvent) -> bool {
    let frame = ClientFrame::Activity { event }.to_json();
    if frame.len() >= RELAY_MAX_PAYLOAD_BYTES {
        log::warn!(
            "steer publisher: dropping oversize activity frame ({} bytes)",
            frame.len()
        );
        return true;
    }
    ws.send(Message::Text(frame)).await.is_ok()
}

/// Whether an `input` frame carries MESSAGE text rather than a keystroke
/// (EXP-334). Steering clients only ever send two shapes: whole composer
/// messages (chunked text + a separate `\r`) and single keystrokes — legacy
/// answer digits, Tab, Esc, and Esc-prefixed sequences. Multi-byte non-escape
/// data is therefore a message.
fn is_message_text(data: &str) -> bool {
    data.len() > 1 && !data.starts_with('\u{1b}')
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as i64)
        .unwrap_or(0)
}

fn close_code(frame: &Option<CloseFrame<'_>>) -> Option<u16> {
    frame.as_ref().map(|f| u16::from(f.code))
}

/// Interruptible backoff sleep: `Break` on `Shutdown` (we're disconnected —
/// nothing to `bye`) or when `running` flipped. Activity published while the
/// socket is down still goes into the journal, so the reconnect's re-publish
/// carries it; a take-over is momentary UI state, safely superseded by the
/// re-`hello`.
async fn sleep_or_shutdown(
    delay: Duration,
    cmd_rx: &flume::Receiver<PublisherCmd>,
    recorder: &mut Recorder,
    embeds: &ImageEmbedMap,
    running: &Arc<AtomicBool>,
) -> std::ops::ControlFlow<()> {
    let deadline = tokio::time::Instant::now() + delay;
    loop {
        if !running.load(Ordering::SeqCst) {
            return std::ops::ControlFlow::Break(());
        }
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => return std::ops::ControlFlow::Continue(()),
            cmd = cmd_rx.recv_async() => match cmd {
                Ok(PublisherCmd::Shutdown { .. }) | Err(_) => {
                    running.store(false, Ordering::SeqCst);
                    return std::ops::ControlFlow::Break(());
                }
                Ok(PublisherCmd::Activity(mut event)) => {
                    prepare_for_journal(&mut event, embeds);
                    recorder.push(event);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frames::QuestionOption;
    use std::sync::Mutex;

    // ── Control path (§8.4): never dropped, never reordered ────────────────

    #[test]
    fn control_channel_is_unbounded_and_lossless() {
        // §8.4: control frames are NEVER dropped — activity/kill/bye ride an
        // unbounded path however chatty the session gets.
        let (cmd_tx, cmd_rx) = flume::unbounded();
        for i in 0..10_000u16 {
            cmd_tx
                .send(PublisherCmd::Activity(ActivityEvent::narration(format!(
                    "line {i}"
                ))))
                .expect("unbounded send never fails");
        }
        cmd_tx
            .send(PublisherCmd::Shutdown { outcome: None })
            .unwrap();
        assert_eq!(cmd_rx.len(), 10_001);
    }

    // ── Handle semantics ────────────────────────────────────────────────────

    #[test]
    fn shutdown_flips_running_and_queues_bye() {
        let (cmd_tx, cmd_rx) = flume::unbounded();
        let handle = PublisherHandle {
            cmd_tx,
            running: Arc::new(AtomicBool::new(true)),
        };
        assert!(handle.is_active());
        handle.shutdown(Some("exit:0".to_string()));
        assert!(!handle.is_active());
        assert_eq!(
            cmd_rx.try_recv().unwrap(),
            PublisherCmd::Shutdown {
                outcome: Some("exit:0".to_string())
            }
        );
        handle.session_ended();
        assert_eq!(
            cmd_rx.try_recv().unwrap(),
            PublisherCmd::Shutdown {
                outcome: Some("killed".to_string())
            }
        );
    }

    // ── Full-task test against a local fake relay (tokio-tungstenite server)

    struct FakeTickets {
        url: String,
    }

    impl PublisherTickets for FakeTickets {
        fn mint(&self) -> Result<Option<MintedTicket>, ApiError> {
            Ok(Some(MintedTicket {
                ticket: "fake.fake".to_string(),
                url: self.url.clone(),
            }))
        }
    }

    #[derive(Default)]
    struct Recorded {
        inputs: Mutex<Vec<Vec<u8>>>,
        kills: Mutex<Vec<KillSignal>>,
        errors: Mutex<Vec<String>>,
    }

    fn recording_hooks(recorded: Arc<Recorded>) -> PublisherHooks {
        recording_hooks_with(recorded, None)
    }

    fn recording_hooks_with(
        recorded: Arc<Recorded>,
        answers: Option<Arc<AnswerLink>>,
    ) -> PublisherHooks {
        let r1 = recorded.clone();
        let r2 = recorded.clone();
        let r3 = recorded;
        PublisherHooks {
            write_input: Arc::new(move |bytes| {
                r1.inputs.lock().unwrap().push(bytes.to_vec());
            }),
            kill: Arc::new(move |signal| {
                r2.kills.lock().unwrap().push(signal);
            }),
            error: Arc::new(move |message| {
                r3.errors.lock().unwrap().push(message);
            }),
            answers,
            agent: SessionAgent::Claude,
            text_sink: None,
            attachments: None,
            commands: None,
            config: None,
        }
    }

    /// Wait (bounded) until `predicate` holds — hooks fire on runtime threads.
    fn wait_for(predicate: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !predicate() {
            assert!(Instant::now() < deadline, "timed out waiting");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// A fake relay socket: everything the publisher sends lands on `seen`,
    /// everything the test injects goes back down the wire.
    fn fake_relay(
        runtime: &SteerRuntime,
    ) -> (u16, flume::Receiver<String>, flume::Sender<Message>) {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (seen_tx, seen_rx) = flume::unbounded::<String>();
        let (inject_tx, inject_rx) = flume::unbounded::<Message>();
        runtime.handle().spawn(async move {
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            let (stream, _addr) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            loop {
                tokio::select! {
                    inject = inject_rx.recv_async() => {
                        let Ok(message) = inject else { break };
                        if ws.send(message).await.is_err() { break; }
                    }
                    msg = ws.next() => match msg {
                        Some(Ok(Message::Text(text))) => { let _ = seen_tx.send(text); }
                        Some(Ok(_)) => {}
                        _ => break,
                    }
                }
            }
        });
        (port, seen_rx, inject_tx)
    }

    /// EXP-773: every published event also lands in the durable per-session
    /// journal file, so the run has a transcript after the process is gone.
    #[test]
    fn published_events_are_appended_to_the_session_journal_file() {
        let runtime = SteerRuntime::new().unwrap();
        let (port, seen_rx, _inject_tx) = fake_relay(&runtime);
        let data_dir = std::env::temp_dir().join(format!(
            "exp-publisher-journal-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));

        let recorded = Arc::new(Recorded::default());
        let handle = publish(
            &runtime,
            PublishSpec {
                session_id: "sess-j".to_string(),
                issue_id: None,
                journal_dir: Some(data_dir.clone()),
            },
            Arc::new(FakeTickets {
                url: format!("ws://127.0.0.1:{port}/ws?ticket=fake.fake"),
            }),
            recording_hooks(recorded),
        );
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap(); // hello
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap(); // activity_reset

        handle
            .activity_sender()
            .send(ActivityEvent::narration("on disk"));
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap(); // the wire copy
        wait_for(|| {
            crate::history::read_journal(&data_dir, "sess-j")
                .is_some_and(|events| !events.is_empty())
        });
        let events = crate::history::read_journal(&data_dir, "sess-j").unwrap();
        assert!(
            matches!(&events[0], ActivityEvent::Narration { text, .. } if text == "on disk"),
            "{events:?}"
        );
        handle.shutdown(None);
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[test]
    fn publisher_hellos_publishes_activity_steers_and_kills_against_a_fake_relay() {
        let runtime = SteerRuntime::new().unwrap();
        let (port, seen_rx, inject_tx) = fake_relay(&runtime);

        let recorded = Arc::new(Recorded::default());
        let (link, answers_rx) = AnswerLink::new();
        let handle = publish(
            &runtime,
            PublishSpec {
                session_id: "sess-t".to_string(),
                issue_id: Some("issue-t".to_string()),
                journal_dir: None,
            },
            Arc::new(FakeTickets {
                url: format!("ws://127.0.0.1:{port}/ws?ticket=fake.fake"),
            }),
            recording_hooks_with(recorded.clone(), Some(link.clone())),
        );

        // 1) hello — no geometry since EXP-249. EXP-90: every hello carries
        // the explicit activityPublic:false.
        let hello = seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            hello,
            r#"{"t":"hello","sessionId":"sess-t","issueId":"issue-t","activityPublic":false}"#
        );
        // 2) …immediately followed by the (empty) history re-publish.
        let reset = seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(reset, r#"{"t":"activity_reset"}"#);

        // 3) activity events go out stamped and land in the journal.
        let sender = handle.activity_sender();
        sender.send(ActivityEvent::narration("Session started"));
        let narration = seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            narration.starts_with(r#"{"t":"activity","event":{"kind":"narration","text":"Session started","at":"#),
            "{narration}"
        );

        // 4) remote input → the PTY-writer hook, byte-identical.
        inject_tx
            .send(Message::Text(r#"{"t":"input","data":"ls\r"}"#.to_string()))
            .unwrap();
        wait_for(|| !recorded.inputs.lock().unwrap().is_empty());
        assert_eq!(recorded.inputs.lock().unwrap()[0], b"ls\r");

        // 5) a semantic answer routes to the emitter's link, NOT the PTY.
        inject_tx
            .send(Message::Text(
                r#"{"t":"answer","questionId":"toolu_1#0","askId":"toolu_1","keys":["2"]}"#
                    .to_string(),
            ))
            .unwrap();
        let answer = answers_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            answer,
            RemoteAnswer {
                question_id: "toolu_1#0".to_string(),
                ask_id: Some("toolu_1".to_string()),
                keys: vec!["2".to_string()],
                text: None,
            }
        );
        assert_eq!(recorded.inputs.lock().unwrap().len(), 1, "answers never keystroke");

        // 6) an unknown frame parses to None and is ignored — the pump keeps
        // running (the kill below still lands).
        inject_tx
            .send(Message::Text(
                r#"{"t":"presence","viewers":[],"steererId":null}"#.to_string(),
            ))
            .unwrap();

        // 7) relay kill → kill hook fires, clean bye goes out, task stops.
        inject_tx
            .send(Message::Text(r#"{"t":"kill"}"#.to_string()))
            .unwrap();
        wait_for(|| !recorded.kills.lock().unwrap().is_empty());
        assert_eq!(recorded.kills.lock().unwrap()[0], KillSignal::RemoteKill);
        let bye = seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(bye, r#"{"t":"bye","outcome":"killed"}"#);
        wait_for(|| !handle.is_active());
        assert!(recorded.errors.lock().unwrap().is_empty());
    }

    // ── EXP-724: remote slash commands ─────────────────────────────────────

    /// EXP-746: without a config link the two live-config frames are a
    /// documented NO-OP — never an answer, never a surfaced error.
    #[test]
    fn set_config_and_set_mode_are_no_ops_without_a_config_link() {
        let runtime = SteerRuntime::new().unwrap();
        let (port, seen_rx, inject_tx) = fake_relay(&runtime);
        let recorded = Arc::new(Recorded::default());
        let (answer_link, answers_rx) = AnswerLink::new();
        let handle = publish(
            &runtime,
            PublishSpec {
                session_id: "sess-cfg-none".to_string(),
                issue_id: None,
                journal_dir: None,
            },
            Arc::new(FakeTickets {
                url: format!("ws://127.0.0.1:{port}/ws?ticket=fake.fake"),
            }),
            recording_hooks_with(recorded.clone(), Some(answer_link)),
        );
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap(); // hello
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap(); // activity_reset

        inject_tx
            .send(Message::Text(
                r#"{"t":"set_config","id":"model","value":"opus"}"#.to_string(),
            ))
            .unwrap();
        inject_tx
            .send(Message::Text(r#"{"t":"set_mode","id":"plan"}"#.to_string()))
            .unwrap();
        // An `input` behind them proves the pump kept running and that the
        // two frames really were consumed (not merely slow).
        inject_tx
            .send(Message::Text(r#"{"t":"input","data":"ls\r"}"#.to_string()))
            .unwrap();
        wait_for(|| !recorded.inputs.lock().unwrap().is_empty());
        assert_eq!(
            recorded.inputs.lock().unwrap().as_slice(),
            &[b"ls\r".to_vec()],
            "a config change never becomes keystrokes"
        );
        assert!(answers_rx.try_recv().is_err(), "and never an answer");
        assert!(recorded.errors.lock().unwrap().is_empty());
        handle.shutdown(None);
    }

    /// The positive twin: with a link wired (the ACP path) both frames cross
    /// to the engine verbatim, blank value included, and still never touch
    /// the PTY writer.
    #[test]
    fn set_config_and_set_mode_cross_to_the_config_link() {
        let runtime = SteerRuntime::new().unwrap();
        let (port, seen_rx, inject_tx) = fake_relay(&runtime);
        let recorded = Arc::new(Recorded::default());
        let config = ConfigLink::new();
        let mut hooks = recording_hooks(recorded.clone());
        hooks.config = Some(config.clone());
        let handle = publish(
            &runtime,
            PublishSpec {
                session_id: "sess-cfg".to_string(),
                issue_id: None,
                journal_dir: None,
            },
            Arc::new(FakeTickets {
                url: format!("ws://127.0.0.1:{port}/ws?ticket=fake.fake"),
            }),
            hooks,
        );
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap(); // hello
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap(); // activity_reset

        // A BLANK value is the "CLI default" choice, not a malformed frame.
        inject_tx
            .send(Message::Text(
                r#"{"t":"set_config","id":"model","value":""}"#.to_string(),
            ))
            .unwrap();
        inject_tx
            .send(Message::Text(r#"{"t":"set_mode","id":"plan"}"#.to_string()))
            .unwrap();
        let received: Mutex<Vec<ConfigChange>> = Mutex::new(Vec::new());
        wait_for(|| {
            while let Some(change) = config.try_recv() {
                received.lock().unwrap().push(change);
            }
            received.lock().unwrap().len() >= 2
        });
        assert_eq!(
            received.into_inner().unwrap(),
            vec![
                ConfigChange::Option {
                    id: "model".to_string(),
                    value: String::new(),
                },
                ConfigChange::Mode {
                    id: "plan".to_string(),
                },
            ]
        );
        assert!(
            recorded.inputs.lock().unwrap().is_empty(),
            "live config never reaches the PTY"
        );
        handle.shutdown(None);
    }

    /// A catalog command is buffered, never written, and crosses whole to the
    /// emitter on its `\r` — the publisher cannot type one (no grid, no turn
    /// state) and must not hand it to the agent as prose either.
    #[test]
    fn a_catalog_command_crosses_to_the_command_link_instead_of_the_pty() {
        let runtime = SteerRuntime::new().unwrap();
        let (port, seen_rx, inject_tx) = fake_relay(&runtime);
        let recorded = Arc::new(Recorded::default());
        let link = CommandLink::new(None);
        let mut hooks = recording_hooks(recorded.clone());
        hooks.commands = Some(link.clone());
        let handle = publish(
            &runtime,
            PublishSpec {
                session_id: "sess-cmd".to_string(),
                issue_id: None,
                journal_dir: None,
            },
            Arc::new(FakeTickets {
                url: format!("ws://127.0.0.1:{port}/ws?ticket=fake.fake"),
            }),
            hooks,
        );
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap(); // hello
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap(); // activity_reset

        let send = |data: &str| {
            inject_tx
                .send(Message::Text(format!(
                    r#"{{"t":"input","data":{}}}"#,
                    serde_json::to_string(data).unwrap()
                )))
                .unwrap();
        };
        // Chunked exactly like a message; only the `\r` completes it.
        send("/compact keep ");
        send("the diff");
        send("\r");
        let received: Mutex<Vec<_>> = Mutex::new(Vec::new());
        wait_for(|| {
            if let Some(command) = link.try_recv() {
                received.lock().unwrap().push(command);
            }
            !received.lock().unwrap().is_empty()
        });
        let received = received.into_inner().unwrap();
        assert_eq!(received.len(), 1, "one command, not one per chunk");
        assert_eq!(received[0].text(), "/compact keep the diff");
        assert!(
            recorded.inputs.lock().unwrap().is_empty(),
            "a command never reaches the PTY from the publisher"
        );

        // Prose still rides the ordinary path, `\r` and all.
        send("just a message");
        send("\r");
        wait_for(|| recorded.inputs.lock().unwrap().len() >= 2);
        assert_eq!(
            recorded.inputs.lock().unwrap().as_slice(),
            &[b"just a message".to_vec(), b"\r".to_vec()]
        );
        // A command name that is not in claude's catalog is prose too.
        recorded.inputs.lock().unwrap().clear();
        send("/new");
        send("\r");
        wait_for(|| recorded.inputs.lock().unwrap().len() >= 2);
        assert_eq!(
            recorded.inputs.lock().unwrap().as_slice(),
            &[b"/new".to_vec(), b"\r".to_vec()]
        );
        assert!(link.try_recv().is_none());
        handle.shutdown(None);
    }

    /// Pi's messages route to the observer extension; its COMMANDS must not
    /// — `pi.sendUserMessage("/compact")` is literal text to the model.
    #[test]
    fn a_pi_catalog_command_bypasses_the_text_sink() {
        let runtime = SteerRuntime::new().unwrap();
        let (port, seen_rx, inject_tx) = fake_relay(&runtime);
        let recorded = Arc::new(Recorded::default());
        let sunk: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let link = CommandLink::new(None);
        let mut hooks = recording_hooks(recorded.clone());
        hooks.agent = SessionAgent::Pi;
        hooks.commands = Some(link.clone());
        let sink = sunk.clone();
        hooks.text_sink = Some(Arc::new(move |text| sink.lock().unwrap().push(text)));
        let handle = publish(
            &runtime,
            PublishSpec {
                session_id: "sess-cmd-pi".to_string(),
                issue_id: None,
                journal_dir: None,
            },
            Arc::new(FakeTickets {
                url: format!("ws://127.0.0.1:{port}/ws?ticket=fake.fake"),
            }),
            hooks,
        );
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap(); // hello
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap(); // activity_reset

        let send = |data: &str| {
            inject_tx
                .send(Message::Text(format!(
                    r#"{{"t":"input","data":{}}}"#,
                    serde_json::to_string(data).unwrap()
                )))
                .unwrap();
        };
        send("/compact");
        send("\r");
        let received: Mutex<Vec<_>> = Mutex::new(Vec::new());
        wait_for(|| {
            if let Some(command) = link.try_recv() {
                received.lock().unwrap().push(command);
            }
            !received.lock().unwrap().is_empty()
        });
        let received = received.into_inner().unwrap();
        assert_eq!(received[0].text(), "/compact");
        assert!(sunk.lock().unwrap().is_empty(), "never sendUserMessage");
        assert!(recorded.inputs.lock().unwrap().is_empty(), "never the PTY");

        // Ordinary prose still reaches the sink.
        send("carry on");
        send("\r");
        wait_for(|| !sunk.lock().unwrap().is_empty());
        assert_eq!(sunk.lock().unwrap().as_slice(), &["carry on".to_string()]);
        handle.shutdown(None);
    }

    // ── EXP-511: steered image embeds → local files → tokens again ─────────

    /// The exact token shape the shared `buildSteerImageMessage` template
    /// emits on web/iOS/Android.
    const EMBED: &str = "![image](/api/attachments/11111111-2222-3333-4444-555555555555)";

    /// A scratch dir for the fake localizer's paths (nothing is written — the
    /// publisher only ever handles the path string).
    fn image_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("exp-steer-images-{tag}-{}", std::process::id()))
    }

    #[test]
    fn image_embeds_become_local_paths_and_echo_back_as_the_token() {
        let runtime = SteerRuntime::new().unwrap();
        let (port, seen_rx, inject_tx) = fake_relay(&runtime);
        let recorded = Arc::new(Recorded::default());
        let local = image_dir("rewrite").join("11111111-2222-3333-4444-555555555555.png");
        let localized = local.clone();
        let mut hooks = recording_hooks(recorded.clone());
        hooks.attachments = Some(Arc::new(move |id| {
            assert_eq!(id, "11111111-2222-3333-4444-555555555555");
            Ok(localized.clone())
        }));
        let handle = publish(
            &runtime,
            PublishSpec {
                session_id: "sess-img".to_string(),
                issue_id: None,
                journal_dir: None,
            },
            Arc::new(FakeTickets {
                url: format!("ws://127.0.0.1:{port}/ws?ticket=fake.fake"),
            }),
            hooks,
        );
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap(); // hello
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap(); // activity_reset

        let path = local.display().to_string();
        inject_tx
            .send(Message::Text(format!(
                r#"{{"t":"input","data":{}}}"#,
                serde_json::to_string(&format!("look at {EMBED} please")).unwrap()
            )))
            .unwrap();
        wait_for(|| !recorded.inputs.lock().unwrap().is_empty());
        assert_eq!(
            recorded.inputs.lock().unwrap()[0],
            format!("look at please\n\nImage #1: {path}").into_bytes(),
            "the agent gets a numbered file to read, not an auth-gated URL"
        );

        // The transcript echoes the manifest LINE — the published event must
        // carry the token the steerer sent, or echo-dedupe misses and the
        // feed renders a path instead of the image.
        handle
            .activity_sender()
            .send(ActivityEvent::user_message(format!(
                "look at please\n\nImage #1: {path}"
            )));
        let event = seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(event.contains(r#""kind":"user_message""#), "{event}");
        assert!(!event.contains(&path), "a local path leaked to the feed: {event}");
        assert!(
            event.contains(r#"![image](/api/attachments/11111111-2222-3333-4444-555555555555)"#),
            "{event}"
        );
        handle.shutdown(None);
    }

    #[test]
    fn a_failed_image_download_keeps_the_url_in_the_manifest() {
        // Benign degradation: the manifest line carries the URL instead of a
        // path — the agent can still fetch it over MCP, and the line restores
        // to the embed token like any other.
        let runtime = SteerRuntime::new().unwrap();
        let (port, seen_rx, inject_tx) = fake_relay(&runtime);
        let recorded = Arc::new(Recorded::default());
        let mut hooks = recording_hooks(recorded.clone());
        hooks.attachments = Some(Arc::new(|_id| Err("offline".to_string())));
        let handle = publish(
            &runtime,
            PublishSpec {
                session_id: "sess-img-err".to_string(),
                issue_id: None,
                journal_dir: None,
            },
            Arc::new(FakeTickets {
                url: format!("ws://127.0.0.1:{port}/ws?ticket=fake.fake"),
            }),
            hooks,
        );
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap(); // hello
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap(); // activity_reset

        inject_tx
            .send(Message::Text(format!(
                r#"{{"t":"input","data":{}}}"#,
                serde_json::to_string(EMBED).unwrap()
            )))
            .unwrap();
        wait_for(|| !recorded.inputs.lock().unwrap().is_empty());
        assert_eq!(
            recorded.inputs.lock().unwrap()[0],
            format!("Image #1: {EMBED}").into_bytes()
        );
        handle.shutdown(None);
    }

    #[test]
    fn a_hung_image_download_never_stalls_the_pump_loop() {
        // EXP-514: localization runs on the dedicated input task. A slow
        // download (up to 60s per blob, sequential) used to run inline in the
        // pump's select loop — silencing the 30s ping tick and the activity
        // flush until the relay's 90s idle detector closed the room. The hook
        // below parks until released: activity published meanwhile must still
        // reach the relay, and the message (with its chased Enter) must land
        // afterwards, localized and in order.
        let runtime = SteerRuntime::new().unwrap();
        let (port, seen_rx, inject_tx) = fake_relay(&runtime);
        let recorded = Arc::new(Recorded::default());
        let release = Arc::new(AtomicBool::new(false));
        let gate = release.clone();
        let local = image_dir("slow").join("11111111-2222-3333-4444-555555555555.png");
        let localized = local.clone();
        let mut hooks = recording_hooks(recorded.clone());
        hooks.attachments = Some(Arc::new(move |_id| {
            while !gate.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(localized.clone())
        }));
        let handle = publish(
            &runtime,
            PublishSpec {
                session_id: "sess-img-slow".to_string(),
                issue_id: None,
                journal_dir: None,
            },
            Arc::new(FakeTickets {
                url: format!("ws://127.0.0.1:{port}/ws?ticket=fake.fake"),
            }),
            hooks,
        );
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap(); // hello
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap(); // activity_reset

        inject_tx
            .send(Message::Text(format!(
                r#"{{"t":"input","data":{}}}"#,
                serde_json::to_string(&format!("look at {EMBED}")).unwrap()
            )))
            .unwrap();
        inject_tx
            .send(Message::Text(r#"{"t":"input","data":"\r"}"#.to_string()))
            .unwrap();

        // The download is parked — the pump must still flush activity.
        handle
            .activity_sender()
            .send(ActivityEvent::narration("still alive"));
        let event = seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(event.contains(r#""text":"still alive""#), "{event}");
        assert!(
            recorded.inputs.lock().unwrap().is_empty(),
            "the input waits on its download"
        );

        // Released, the message lands localized, then its Enter.
        release.store(true, Ordering::SeqCst);
        wait_for(|| recorded.inputs.lock().unwrap().len() >= 2);
        assert_eq!(
            recorded.inputs.lock().unwrap().as_slice(),
            &[
                format!("look at\n\nImage #1: {}", local.display()).into_bytes(),
                b"\r".to_vec()
            ]
        );
        handle.shutdown(None);
    }

    #[test]
    fn a_pi_text_sink_receives_the_localized_message() {
        // The rewrite happens BEFORE the sink buffer opens, so pi's observer
        // extension injects paths too.
        let runtime = SteerRuntime::new().unwrap();
        let (port, seen_rx, inject_tx) = fake_relay(&runtime);
        let recorded = Arc::new(Recorded::default());
        let sunk: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let local = image_dir("pi").join("11111111-2222-3333-4444-555555555555.png");
        let localized = local.clone();
        let mut hooks = recording_hooks(recorded);
        hooks.agent = SessionAgent::Pi;
        let sink = sunk.clone();
        hooks.text_sink = Some(Arc::new(move |text| sink.lock().unwrap().push(text)));
        hooks.attachments = Some(Arc::new(move |_id| Ok(localized.clone())));
        let handle = publish(
            &runtime,
            PublishSpec {
                session_id: "sess-img-pi".to_string(),
                issue_id: None,
                journal_dir: None,
            },
            Arc::new(FakeTickets {
                url: format!("ws://127.0.0.1:{port}/ws?ticket=fake.fake"),
            }),
            hooks,
        );
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap(); // hello
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap(); // activity_reset

        inject_tx
            .send(Message::Text(format!(
                r#"{{"t":"input","data":{}}}"#,
                serde_json::to_string(&format!("crop this\n\n{EMBED}")).unwrap()
            )))
            .unwrap();
        inject_tx
            .send(Message::Text(r#"{"t":"input","data":"\r"}"#.to_string()))
            .unwrap();
        wait_for(|| !sunk.lock().unwrap().is_empty());
        assert_eq!(
            sunk.lock().unwrap().as_slice(),
            &[format!("crop this\n\nImage #1: {}", local.display())]
        );
        handle.shutdown(None);
    }

    #[test]
    fn only_uuid_embeds_are_localized_and_every_text_field_is_restored() {
        // The token contract: fixed `image` alt, an attachments URL, a
        // UUID id. Anything else in a message is left alone.
        let pattern = image_embed_pattern();
        assert!(pattern.is_match(EMBED));
        assert!(!pattern.is_match("![image](/api/attachments/not-a-uuid)"));
        assert!(!pattern.is_match("![image](https://evil.example/api/attachments/x)"));
        assert!(!pattern.is_match("/api/attachments/11111111-2222-3333-4444-555555555555"));

        // The reverse rewrite covers every text field an agent could quote a
        // path into, never the machine fields (ids, option keys).
        let embeds: ImageEmbedMap = Arc::new(Mutex::new(vec![
            (
                "Image #1: /tmp/w/.exp-steer-images/img.png".to_string(),
                EMBED.to_string(),
            ),
            (
                "/tmp/w/.exp-steer-images/img.png".to_string(),
                EMBED.to_string(),
            ),
        ]));
        let mut tool = ActivityEvent::tool("Read", Some("/tmp/w/.exp-steer-images/img.png".into()));
        restore_image_embeds(&mut tool, &embeds);
        assert_eq!(tool, ActivityEvent::tool("Read", Some(EMBED.to_string())));
        let mut question = ActivityEvent::Question {
            text: "Use /tmp/w/.exp-steer-images/img.png?".into(),
            options: vec![QuestionOption::new("Yes", "1")],
            multi_select: None,
            plan_mode: None,
            id: Some("/tmp/w/.exp-steer-images/img.png".into()),
            ask_id: None,
            index: None,
            total: None,
            header: None,
            at: None,
        };
        restore_image_embeds(&mut question, &embeds);
        match question {
            ActivityEvent::Question { text, id, .. } => {
                assert_eq!(text, format!("Use {EMBED}?"));
                assert_eq!(id.as_deref(), Some("/tmp/w/.exp-steer-images/img.png"));
            }
            other => panic!("expected Question, got {other:?}"),
        }
    }

    // ── EXP-698: the numbered manifest ─────────────────────────────────────

    const EMBED_B: &str = "![image](/api/attachments/22222222-3333-4444-5555-666666666666)";

    /// Localize with a hook that answers `/img/<n>.png` per call, so the
    /// manifest's numbering is readable in the assertions.
    fn localize(message: &str) -> (String, ImageEmbedMap) {
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let hook: AttachmentHook = Arc::new(move |_id| {
            let n = counter.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(PathBuf::from(format!("/img/{n}.png")))
        });
        let embeds: ImageEmbedMap = Arc::new(Mutex::new(Vec::new()));
        let runtime = SteerRuntime::new().unwrap();
        let out = runtime.handle().block_on(localize_image_embeds(
            message.to_string(),
            &hook,
            &embeds,
        ));
        (out, embeds)
    }

    #[test]
    fn the_manifest_numbers_every_image_in_embed_order() {
        // A marker in the prose and a line in the manifest name the same
        // file: image order IS embed order, which is the whole contract.
        let (out, _) = localize(&format!("crop [Image #2]\n\n{EMBED}\n{EMBED_B}"));
        assert_eq!(
            out,
            "crop [Image #2]\n\nImage #1: /img/1.png\nImage #2: /img/2.png"
        );
    }

    #[test]
    fn an_unreferenced_image_still_gets_its_line() {
        // "every image gets a line whether or not a marker references it" —
        // an agent handed two images must be able to find both.
        let (out, _) = localize(&format!("have a look\n\n{EMBED}\n{EMBED_B}"));
        assert_eq!(
            out,
            "have a look\n\nImage #1: /img/1.png\nImage #2: /img/2.png"
        );
    }

    #[test]
    fn embeds_alone_become_the_manifest_alone() {
        let (out, _) = localize(EMBED);
        assert_eq!(out, "Image #1: /img/1.png");
    }

    #[test]
    fn an_inline_embed_leaves_no_gap_behind() {
        let (out, _) = localize(&format!("look at {EMBED} please"));
        assert_eq!(out, "look at please\n\nImage #1: /img/1.png");
    }

    #[test]
    fn one_attachment_resent_under_a_new_number_still_restores_whole() {
        // The two-pass restore's reason to exist. The SAME image is sent
        // twice — image #1 of one message, image #2 of the next — so by the
        // second message its bare path is already in the map, pushed ahead of
        // the `Image #2:` line that now needs to win. A single-pass walk
        // would replace the path first and strand `Image #2: ` in the feed.
        let hook: AttachmentHook = Arc::new(|id| {
            // Stable per id: the same attachment localizes to the same file
            // however many messages carry it.
            Ok(PathBuf::from(format!("/img/{id}.png")))
        });
        let embeds: ImageEmbedMap = Arc::new(Mutex::new(Vec::new()));
        let runtime = SteerRuntime::new().unwrap();
        let first = runtime.handle().block_on(localize_image_embeds(
            format!("look [Image #1]\n\n{EMBED}"),
            &hook,
            &embeds,
        ));
        let second = runtime.handle().block_on(localize_image_embeds(
            format!("again [Image #2]\n\n{EMBED_B}\n{EMBED}"),
            &hook,
            &embeds,
        ));
        // In the second message the shared image is #2, not #1.
        assert!(second.contains("Image #2: /img/11111111-2222-3333-4444-555555555555.png"));

        for (localized, original) in [
            (first, format!("look [Image #1]\n\n{EMBED}")),
            (
                second,
                format!("again [Image #2]\n\n{EMBED_B}\n{EMBED}"),
            ),
        ] {
            let mut echo = ActivityEvent::user_message(localized);
            restore_image_embeds(&mut echo, &embeds);
            match echo {
                ActivityEvent::UserMessage { text, .. } => {
                    assert_eq!(text, original);
                    // The prose keeps its `[Image #N]` MARKERS; what must not
                    // survive is a manifest prefix left in front of a token.
                    assert!(!text.contains(": !["), "a prefix was stranded: {text}");
                    assert!(!text.contains("/img/"), "a local path leaked: {text}");
                }
                other => panic!("expected UserMessage, got {other:?}"),
            }
        }
    }

    #[test]
    fn the_manifest_line_maps_back_to_the_embed_token() {
        // The round trip viewers depend on: the agent's transcript echoes the
        // LINE, and the published event must carry the token again — prefix
        // and all, or `Image #1: ` would be stranded in the feed.
        let (out, embeds) = localize(&format!("crop [Image #1]\n\n{EMBED}"));
        let mut echo = ActivityEvent::user_message(out);
        restore_image_embeds(&mut echo, &embeds);
        match echo {
            ActivityEvent::UserMessage { text, .. } => {
                assert_eq!(text, format!("crop [Image #1]\n\n{EMBED}"));
            }
            other => panic!("expected UserMessage, got {other:?}"),
        }
        // A bare path quoted somewhere else still resolves (the second map
        // entry per image).
        let mut tool = ActivityEvent::tool("Read", Some("/img/1.png".into()));
        restore_image_embeds(&mut tool, &embeds);
        assert_eq!(tool, ActivityEvent::tool("Read", Some(EMBED.to_string())));
    }

    #[test]
    fn a_reconnect_resets_and_replays_the_journal() {
        let runtime = SteerRuntime::new().unwrap();
        // Two sequential relay sockets on ONE port: the first is dropped after
        // the publisher hellos, forcing the reconnect path.
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (seen_tx, seen_rx) = flume::unbounded::<(usize, String)>();
        let (drop_tx, drop_rx) = flume::unbounded::<()>();
        runtime.handle().spawn(async move {
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            for connection in 0..2usize {
                let Ok((stream, _addr)) = listener.accept().await else { return };
                let Ok(mut ws) = tokio_tungstenite::accept_async(stream).await else { return };
                loop {
                    tokio::select! {
                        _ = drop_rx.recv_async() => break, // sever this socket
                        msg = ws.next() => match msg {
                            Some(Ok(Message::Text(text))) => {
                                let _ = seen_tx.send((connection, text));
                            }
                            Some(Ok(_)) => {}
                            _ => break,
                        }
                    }
                }
                drop(ws);
            }
        });

        let recorded = Arc::new(Recorded::default());
        let handle = publish(
            &runtime,
            PublishSpec {
                session_id: "sess-j".to_string(),
                issue_id: None,
                journal_dir: None,
            },
            Arc::new(FakeTickets {
                url: format!("ws://127.0.0.1:{port}/ws?ticket=fake.fake"),
            }),
            recording_hooks(recorded.clone()),
        );
        assert!(seen_rx.recv_timeout(Duration::from_secs(5)).unwrap().1.contains("hello"));
        assert_eq!(seen_rx.recv_timeout(Duration::from_secs(5)).unwrap().1, r#"{"t":"activity_reset"}"#);

        let sender = handle.activity_sender();
        sender.send(ActivityEvent::narration("first"));
        sender.send(ActivityEvent::tool("Edit", Some("a.rs".into())));
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();

        // Sever the socket; the publisher re-mints, re-hellos, and rebuilds
        // the whole feed on the fresh connection.
        drop_tx.send(()).unwrap();
        let mut second: Vec<String> = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        while second.len() < 4 {
            assert!(Instant::now() < deadline, "no replay after reconnect: {second:?}");
            if let Ok((connection, text)) = seen_rx.recv_timeout(Duration::from_millis(250)) {
                if connection == 1 {
                    second.push(text);
                }
            }
        }
        assert!(second[0].contains(r#""t":"hello""#), "{:?}", second[0]);
        assert_eq!(second[1], r#"{"t":"activity_reset"}"#);
        assert!(second[2].contains(r#""text":"first""#), "{:?}", second[2]);
        assert!(second[3].contains(r#""name":"Edit""#), "{:?}", second[3]);
        handle.shutdown(None);
    }

    #[test]
    fn clean_shutdown_sends_bye_with_exit_outcome() {
        let runtime = SteerRuntime::new().unwrap();
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (seen_tx, seen_rx) = flume::unbounded::<String>();

        runtime.handle().spawn(async move {
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            let (stream, _addr) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            while let Some(Ok(msg)) = ws.next().await {
                if let Message::Text(text) = msg {
                    let _ = seen_tx.send(text);
                }
            }
        });

        let recorded = Arc::new(Recorded::default());
        let handle = publish(
            &runtime,
            PublishSpec {
                session_id: "sess-x".to_string(),
                issue_id: None,
                journal_dir: None,
            },
            Arc::new(FakeTickets {
                url: format!("ws://127.0.0.1:{port}/ws?ticket=fake.fake"),
            }),
            recording_hooks(recorded),
        );
        let hello = seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            hello,
            r#"{"t":"hello","sessionId":"sess-x","activityPublic":false}"#
        );
        assert_eq!(
            seen_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            r#"{"t":"activity_reset"}"#
        );

        handle.shutdown(Some("exit:0".to_string()));
        let bye = seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(bye, r#"{"t":"bye","outcome":"exit:0"}"#);
        wait_for(|| !handle.is_active());
    }

    #[test]
    fn disabled_relay_ends_the_task_without_noise() {
        struct DisabledTickets;
        impl PublisherTickets for DisabledTickets {
            fn mint(&self) -> Result<Option<MintedTicket>, ApiError> {
                Ok(None)
            }
        }
        let runtime = SteerRuntime::new().unwrap();
        let recorded = Arc::new(Recorded::default());
        let handle = publish(
            &runtime,
            PublishSpec {
                session_id: "sess-d".to_string(),
                issue_id: None,
                journal_dir: None,
            },
            Arc::new(DisabledTickets),
            recording_hooks(recorded.clone()),
        );
        wait_for(|| !handle.is_active());
        assert!(recorded.errors.lock().unwrap().is_empty(), "EXP-4: no noise");
    }
}
