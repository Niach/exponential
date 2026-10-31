//! The per-coding-session publisher (masterplan-v3 §8.4–§8.7, steering v2 =
//! EXP-249): record the scrubbed activity stream (the in-memory journal and
//! the EXP-773 journal file), serve it to eNet viewers (`enet_steer.rs`),
//! inject their `input` and semantic `answer` frames, and honor kill.
//!
//! VAPP-43: the steer relay carries no session content any more — the
//! publisher never dials it. Viewers reach the run over eNet only; the
//! relay's session-scoped control commands (`kill`, `interrupt`,
//! `compact_request`) arrive on the device's CONTROL socket and are routed
//! here by session id ([`control`]).
//!
//! **Best-effort and non-blocking** (§8.4): the coding session runs fine
//! with no viewer at all — the publisher never gates the agent. Control
//! commands and activity events ride ONE unbounded channel that is never
//! dropped or reordered. Remote `input` frames ride a second ordered channel
//! to a dedicated task (EXP-514), so their choreography — sleeps, EXP-511
//! image downloads — never stalls the loop.
//!
//! Steering is seamless (EXP-312): there is no operator claim and no perm
//! tier, and the LOCAL user is never gated.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use api::trpc::TrpcClient;

use crate::activity::{
    AnswerLink, CommandLink, ConfigChange, ConfigLink, RemoteAnswer, SessionAgent,
};
use crate::commands::parse_command;
use crate::frames::{ActivityEvent, CompactRefusal, CompactVerdict, ServerFrame};
use crate::history::JournalWriter;
use crate::journal::ActivityJournal;
use crate::SteerRuntime;

// ---------------------------------------------------------------------------
// Spec, hooks
// ---------------------------------------------------------------------------

/// What to publish: the `sessions` row id (EXP-1292) keys the eNet room and
/// the journal file.
#[derive(Clone, Debug, Default)]
pub struct PublishSpec {
    pub session_id: String,
    pub issue_id: Option<String>,
    /// EXP-773: the app/daemon data dir whose `journal/` subdirectory keeps
    /// this session's DURABLE transcript ([`crate::history`]). Every event
    /// this publisher sends is appended there, so the run can be replayed
    /// from disk long after the process is gone. `None` = don't record (the
    /// examples and the tests).
    pub journal_dir: Option<PathBuf>,
    /// EXP-825: the session's image-embed restore map, SHARED with the
    /// engine — a start prompt's images are localized before the publisher
    /// exists ([`localize_message`] on the host), and the agent's echoes of
    /// those paths must restore to tokens exactly like a steered image's.
    /// `Default` = a fresh map (tests, the PTY-less examples).
    pub embeds: ImageEmbeds,
    /// VAPP-33: who may steer the run over eNet (`enet_steer.rs`); `None` =
    /// no eNet room (tests, the examples).
    pub audience: Option<coding::SteerAudience>,
}

/// Why the publisher asked the coding flow to tear the session down.
///
/// EXP-283: an explicit `kill` (a viewer's over eNet, or VAPP-43 the
/// relay's down the control socket) is the ONLY remote signal that may end
/// the local session. Real session ends always also flip the synced
/// `sessions` row to `ended`, which the pickup pass's `Kill` action handles
/// durably (EXP-1292).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KillSignal {
    /// A `kill` frame (steer.killSession via the control socket / a
    /// steerer's kill over eNet).
    RemoteKill,
}

/// Remote `input` bytes → the shared PTY writer (§6.5). Aliased so the
/// `&[u8]`-taking `Fn` type stays under clippy's `type_complexity` bar.
pub type InputHook = Arc<dyn Fn(&[u8]) + Send + Sync>;

/// EXP-511: attachment id → the local file the agent should read. BLOCKING
/// (HTTP + a disk write); the input task calls it from `spawn_blocking`.
/// `Err` is a human-readable reason for the log — the embed then stays as it
/// arrived. Wave D: the second argument is the FILE's name for a
/// `[<name>](/api/attachments/<id>)` file line, `None` for an image embed.
pub type AttachmentHook = Arc<dyn Fn(&str, Option<&str>) -> Result<PathBuf, String> + Send + Sync>;

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
    /// Remote teardown: kill the agent child; the exit hook then reports the
    /// `sessions` row's terminal status (idempotent server-side).
    pub kill: Arc<dyn Fn(KillSignal) + Send + Sync>,
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
    /// EXP-790/EXP-861: a viewer's Stop (`interrupt`) → the engine cancels
    /// the running turn and drops its queued messages. `None` (tests) is a
    /// documented no-op.
    pub interrupt: Option<Arc<dyn Fn() + Send + Sync>>,
    /// EXP-861: a viewer revoking ONE queued message (`unqueue {id}`) → the
    /// engine drops it and republishes the `queue` slot. `None` (tests) is a
    /// documented no-op.
    pub unqueue: Option<Arc<dyn Fn(String) + Send + Sync>>,
    /// EXP-936: the run's OWN `exponential_sessions_compact`, relayed as a
    /// `compact_request` down the control socket (VAPP-43). The hook DECIDES
    /// (it holds the context meter, the turns since the last compaction and
    /// the open-fold state) and, when it accepts, arranges the agent's
    /// `/compact <keep>` at the next turn boundary; the publisher only hands
    /// the verdict back up the control socket, where it is the web server's
    /// awaiting tool result. Synchronous and cheap by contract: the relay
    /// times the ask out after 4s. `None` (tests, a run with no engine)
    /// refuses as `unsupported_agent` rather than leaving the relay to time
    /// out.
    pub compact: Option<CompactHook>,
}

/// EXP-936: `keep` (what the summary must preserve) → the host's verdict.
pub type CompactHook = Arc<dyn Fn(Option<String>) -> CompactVerdict + Send + Sync>;

/// [`PublisherHooks::attachments`] over an account's tRPC client (EXP-511):
/// downloads into `dest_dir` — `<worktree>/.exp-steer-images`, which the
/// launcher git-excludes so a steerer's screenshots never reach the PR diff.
///
/// Wave D: a FILE line (any content type) lands in the same dir (the name is
/// load-bearing — existing worktrees' git excludes name it) as
/// `<id>/<filename>`.
pub fn image_localizer(trpc: Arc<TrpcClient>, dest_dir: PathBuf) -> AttachmentHook {
    Arc::new(move |attachment_id, file_name| {
        match file_name {
            // FEED-180: an image embed may name a non-image (a sealed
            // message's clear ids, embedded by the device): it lands as a file.
            None => api::attachments::download_attachment(&trpc, attachment_id, &dest_dir),
            Some(name) => api::attachments::download_file(&trpc, attachment_id, name, &dest_dir),
        }
        .map_err(|err| err.to_string())
    })
}

/// The embed token a steering client sends for an attached image (the shared
/// `buildSteerImageMessage` template, byte-identical on web/iOS/Android). The
/// id is UUID-shaped so nothing else in a message can be mistaken for one.
/// FEED-180: any alt text, so the line the server appends to a plaintext
/// body (`![<filename>](/api/attachments/<id>)`) localizes too.
fn image_embed_pattern() -> &'static regex::Regex {
    static PATTERN: OnceLock<regex::Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        regex::Regex::new(
            r"(?i)!\[[^\]\n]*\]\(/api/attachments/([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\)",
        )
        .expect("the image embed pattern is a valid regex")
    })
}

/// Wave D: the FILE line a steering client sends for an attached non-image
/// (`buildSteerMessage`'s file block): `[<name>](/api/attachments/<uuid>)`
/// NOT preceded by `!` (the caller checks that — the regex crate has no
/// look-behind). The link text may carry the builder's `\]`/`\\` escapes.
/// Unanchored on purpose, like [`image_embed_pattern`]: a link to one of
/// OUR attachments anywhere in the prose is localized for the agent too
/// (the viewers keep showing it as a link); the parsers' tail-only rule
/// decides what the composer STRIP shows, not what the agent may read.
fn file_embed_pattern() -> &'static regex::Regex {
    static PATTERN: OnceLock<regex::Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        regex::Regex::new(
            r"(?i)\[((?:[^\]\\\n]|\\.)*)\]\(/api/attachments/([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\)",
        )
        .expect("the file embed pattern is a valid regex")
    })
}

/// Every file line in `text`: `(token, id, unescaped name)`, skipping the
/// `[image](…)` half of an image embed.
fn file_embeds(text: &str) -> Vec<(String, String, String)> {
    file_embed_pattern()
        .captures_iter(text)
        .filter_map(|captures| {
            let whole = captures.get(0)?;
            if text[..whole.start()].ends_with('!') {
                return None;
            }
            Some((
                whole.as_str().to_string(),
                captures[2].to_string(),
                domain::image_message::unescape_file_name(&captures[1]),
            ))
        })
        .collect()
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
///
/// EXP-825: a shareable handle — the engine creates it once per session
/// ([`PublishSpec::embeds`]), so the START prompt's images (localized on the
/// host before any publisher exists) and every steered image land in the
/// same map.
#[derive(Clone, Debug, Default)]
pub struct ImageEmbeds(Arc<Mutex<Vec<(String, String)>>>);

impl ImageEmbeds {
    /// Record `(needle, token)` pairs; a needle already known is kept as is.
    fn record(&self, restores: Vec<(String, String)>) {
        if let Ok(mut embeds) = self.0.lock() {
            for (needle, token) in restores {
                if !embeds.iter().any(|(known, _)| *known == needle) {
                    embeds.push((needle, token));
                }
            }
        }
    }

    /// The recorded `(needle, token)` pairs, in push order (tests).
    pub fn snapshot(&self) -> Vec<(String, String)> {
        self.0
            .lock()
            .map(|embeds| embeds.clone())
            .unwrap_or_default()
    }
}

type ImageEmbedMap = ImageEmbeds;

/// EXP-825: does `text` carry at least one steer image embed? The host asks
/// before spending a download task on a prompt (the overwhelmingly common
/// prompt has none).
pub fn has_image_embed(text: &str) -> bool {
    image_embed_pattern().is_match(text)
}

/// Wave D: an image embed OR a file line — what the host checks before
/// localizing a prompt.
pub fn has_attachment_embed(text: &str) -> bool {
    has_image_embed(text) || !file_embeds(text).is_empty()
}

/// EXP-825: [`localize_image_embeds`] for the host's own prompts — the start
/// prompt's images (and a local composer's) go through the same download +
/// manifest + restore-map path a steered message takes.
pub async fn localize_message(text: String, hook: &AttachmentHook, embeds: &ImageEmbeds) -> String {
    localize_image_embeds(text, hook, embeds).await
}

/// The prefix every manifest-line needle starts with — how
/// [`restore_image_embeds`] tells a line needle from a bare path.
const MANIFEST_PREFIX: &str = "Image #";

/// Wave D: the file manifest lines' prefix (numbered apart from the images).
const FILE_MANIFEST_PREFIX: &str = "File #";

/// One image's manifest line, as the agent reads it.
fn image_manifest_line(index: usize, target: &str) -> String {
    format!("{MANIFEST_PREFIX}{index}: {target}")
}

/// One file's manifest line, as the agent reads it.
fn file_manifest_line(index: usize, target: &str) -> String {
    format!("{FILE_MANIFEST_PREFIX}{index}: {target}")
}

/// Is `needle` a whole manifest LINE (vs a bare path)?
fn is_manifest_needle(needle: &str) -> bool {
    needle.starts_with(MANIFEST_PREFIX) || needle.starts_with(FILE_MANIFEST_PREFIX)
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
///
/// Wave D: FILE lines (`[<name>](/api/attachments/<uuid>)`) localize the same
/// way into `File #N: <path>` lines AFTER the `Image #N:` ones, numbered on
/// their own (files carry no positional marker), through the same restore map.
async fn localize_image_embeds(
    data: String,
    hook: &AttachmentHook,
    embeds: &ImageEmbedMap,
) -> String {
    // (token, id, file name — None for an image)
    let mut tokens: Vec<(String, String, Option<String>)> = Vec::new();
    for captures in image_embed_pattern().captures_iter(&data) {
        let token = captures[0].to_string();
        if !tokens.iter().any(|(existing, _, _)| existing == &token) {
            tokens.push((token, captures[1].to_string(), None));
        }
    }
    for (token, id, name) in file_embeds(&data) {
        if !tokens.iter().any(|(existing, _, _)| existing == &token) {
            tokens.push((token, id, Some(name)));
        }
    }
    if tokens.is_empty() {
        return data;
    }
    let mut manifest: Vec<String> = Vec::with_capacity(tokens.len());
    let mut restores: Vec<(String, String)> = Vec::new();
    let (mut images, mut files) = (0usize, 0usize);
    for (token, id, name) in tokens.iter() {
        let line_for: fn(usize, &str) -> String = if name.is_some() {
            files += 1;
            file_manifest_line
        } else {
            images += 1;
            image_manifest_line
        };
        let number = if name.is_some() { files } else { images };
        let hook = hook.clone();
        let requested = id.clone();
        let requested_name = name.clone();
        match tokio::task::spawn_blocking(move || hook(&requested, requested_name.as_deref())).await
        {
            Ok(Ok(path)) => {
                let path = path.display().to_string();
                let line = line_for(number, &path);
                restores.push((line.clone(), token.clone()));
                restores.push((path, token.clone()));
                manifest.push(line);
            }
            Ok(Err(reason)) => {
                log::warn!("steer publisher: attachment {id} not localized ({reason})");
                // No local file — the manifest carries the URL itself, and
                // the line restores to the token like any other.
                let line = line_for(number, token);
                restores.push((line.clone(), token.clone()));
                manifest.push(line);
            }
            Err(join_err) => {
                log::warn!("steer publisher: attachment {id} download panicked: {join_err}");
                let line = line_for(number, token);
                restores.push((line.clone(), token.clone()));
                manifest.push(line);
            }
        }
    }
    embeds.record(restores);
    // Strip the embed block out of the prose, then hang the manifest off it.
    // A line that held NOTHING but an embed goes entirely (the composer puts
    // them on their own lines); an inline one leaves its line tidied, so the
    // agent never reads a double space where a token used to be.
    let mut kept: Vec<String> = Vec::new();
    for line in data.split('\n') {
        let mut next = line.to_string();
        let mut touched = false;
        for (token, _, _) in &tokens {
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
    let Ok(embeds) = embeds.0.lock() else { return };
    if embeds.is_empty() {
        return; // the overwhelmingly common case — no allocation, no walk
    }
    for field in event.text_fields_mut() {
        for lines_pass in [true, false] {
            for (needle, token) in embeds.iter() {
                if is_manifest_needle(needle) != lines_pass {
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
    /// §P7: one PUBLIC activity event (already redacted): recorded, then fed
    /// live to the eNet viewers. Low-rate (per assistant turn / debounced
    /// diff), so it never backs up.
    Activity(ActivityEvent),
    /// Clean end: `bye {outcome}` to the eNet viewers (child exit, local
    /// stop, kill-watch).
    Shutdown { outcome: Option<String> },
    /// VAPP-33: an eNet viewer sent `join` (an allowed one: `enet_steer`
    /// checked the access rule): reset + replay + synced, then the live feed.
    EnetJoin(crate::enet_steer::EnetPeer),
    /// VAPP-33: an eNet viewer sent `leave`.
    EnetLeave(enet_core::EndpointId),
    /// VAPP-33: one client frame from an eNet viewer (JSON text, the relay's
    /// shape).
    EnetFrame { from: enet_core::EndpointId, frame: String },
    /// VAPP-43: a session-scoped command the relay sent down the CONTROL
    /// socket ([`control`]).
    Control(ControlCmd),
    /// Tests only: a raw viewer queue (no eNet node), joined like a viewer.
    #[cfg(test)]
    Tap(enet_core::EndpointId, TapTx),
}

/// Tests only: [`PublisherCmd::Tap`]'s queue.
#[cfg(test)]
#[derive(Clone, Debug)]
pub(crate) struct TapTx(pub flume::Sender<String>);

#[cfg(test)]
impl PartialEq for TapTx {
    fn eq(&self, other: &Self) -> bool {
        self.0.same_channel(&other.0)
    }
}

/// VAPP-43: what the relay may ask of a live run, down the device's CONTROL
/// socket. Routed by session id to the run's publisher ([`control`]).
#[derive(Clone, Debug, PartialEq)]
pub enum ControlCmd {
    /// End the run (`steer.killSession`): exactly an eNet viewer's `kill`.
    Kill,
    /// Stop the running turn (`sessions.stop`): exactly a viewer's Stop.
    Interrupt,
    /// EXP-936: the run's own compaction ask. The verdict goes back through
    /// `reply` (up the control socket the ask came on).
    Compact { keep: Option<String>, reply: CompactReply },
}

/// VAPP-43: where a [`ControlCmd::Compact`] verdict goes (the control
/// socket's outbound queue). Called once.
#[derive(Clone)]
pub struct CompactReply(pub Arc<dyn Fn(CompactVerdict) + Send + Sync>);

impl std::fmt::Debug for CompactReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CompactReply")
    }
}

impl PartialEq for CompactReply {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
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
/// stopped is a harmless no-op (the loop drops the receiver).
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

/// EXP-1426 part 4: the agent runtime's event seam. The engine core sends
/// every mapped event through it; for Exponential that is the publisher.
impl exponential_agent_runtime::engine::EventSink for ActivitySender {
    fn send(&self, event: ActivityEvent) {
        ActivitySender::send(self, event)
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

    /// Clean end (§8.4 End): `bye {outcome}` to the eNet viewers. Outcome
    /// format per spec: `exit:<code>` from the child-exit hook, `killed` for
    /// a kill-path teardown. Idempotent.
    pub fn shutdown(&self, outcome: Option<String>) {
        self.running.store(false, Ordering::SeqCst);
        let _ = self.cmd_tx.send(PublisherCmd::Shutdown { outcome });
    }

    /// §8.8 kill-switch entry: the synced `sessions` row flipped to
    /// `ended` (or the local session was torn down out-of-band). Equivalent
    /// to `shutdown(Some("killed"))`.
    pub fn session_ended(&self) {
        self.shutdown(Some("killed".to_string()));
    }

    /// False once the publisher stopped for good (clean end, kill).
    pub fn is_active(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

/// Start publishing a coding session (§8.4). Non-blocking: spawns the task
/// onto the steer runtime and returns the handle immediately. Wire-up
/// contract (the coding-flow seam):
///
/// 1. `spawn_activity_emitter` with `handle.activity_sender()` — the events
///    this task journals and serves;
/// 2. exit hook → `handle.shutdown(Some(format!("exit:{code}")))`;
/// 3. the pickup pass's `Kill` action (the synced `sessions` row went
///    `ended`, EXP-1292) → `handle.session_ended()` (after killing the child).
pub fn publish(runtime: &SteerRuntime, spec: PublishSpec, hooks: PublisherHooks) -> PublisherHandle {
    let (cmd_tx, cmd_rx) = flume::unbounded();
    let running = Arc::new(AtomicBool::new(true));
    let handle = PublisherHandle {
        cmd_tx,
        running: running.clone(),
    };
    let live = LivePublisher::claim(&spec.session_id, handle.cmd_tx.downgrade());
    let weak_tx = handle.cmd_tx.downgrade();
    runtime.handle().spawn(async move {
        run_publisher_loop(spec, hooks, cmd_rx, weak_tx, running).await;
        drop(live);
    });
    handle
}

/// The sessions THIS PROCESS is publishing live, by id: the newest
/// publisher's command channel (VAPP-43: where [`control`] routes) under a
/// generation, so an older publisher's teardown never clears a newer one.
fn live_publishers() -> &'static Mutex<HashMap<String, (u64, flume::WeakSender<PublisherCmd>)>> {
    static LIVE: OnceLock<Mutex<HashMap<String, (u64, flume::WeakSender<PublisherCmd>)>>> = OnceLock::new();
    LIVE.get_or_init(Default::default)
}

fn live_lock() -> std::sync::MutexGuard<'static, HashMap<String, (u64, flume::WeakSender<PublisherCmd>)>> {
    live_publishers().lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

static LIVE_GENERATION: AtomicU64 = AtomicU64::new(1);

/// One live publisher's entry in [`live_publishers`], released on drop so
/// every exit path of the task clears it — including a panic.
struct LivePublisher {
    session_id: String,
    generation: u64,
}

impl LivePublisher {
    fn claim(session_id: &str, cmd_tx: flume::WeakSender<PublisherCmd>) -> Self {
        let generation = LIVE_GENERATION.fetch_add(1, Ordering::Relaxed);
        live_lock().insert(session_id.to_string(), (generation, cmd_tx));
        Self { session_id: session_id.to_string(), generation }
    }
}

impl Drop for LivePublisher {
    fn drop(&mut self) {
        let mut live = live_lock();
        if live.get(&self.session_id).is_some_and(|(generation, _)| *generation == self.generation) {
            live.remove(&self.session_id);
        }
    }
}

/// Is this process publishing that session RIGHT NOW?
pub fn is_publishing(session_id: &str) -> bool {
    live_lock().get(session_id).is_some_and(|(_, tx)| tx.upgrade().is_some())
}

/// VAPP-43: hand a control-socket command to the LIVE publisher of
/// `session_id` on this process. `false` = no such run here (the caller
/// ignores the command).
pub fn control(session_id: &str, cmd: ControlCmd) -> bool {
    let Some(tx) = live_lock().get(session_id).and_then(|(_, tx)| tx.upgrade()) else {
        return false;
    };
    tx.send(PublisherCmd::Control(cmd)).is_ok()
}

// ---------------------------------------------------------------------------
// The task
// ---------------------------------------------------------------------------

/// The session's transcript in BOTH places it lives: the in-memory replay
/// journal (EXP-249, replayed to every joining viewer) and the durable
/// per-session file (EXP-773, read back long after the process is gone). One
/// `push` keeps them in step, and the file is optional — a publisher without
/// a `journal_dir` keeps the memory journal alone.
struct Recorder {
    journal: ActivityJournal,
    file: Option<JournalWriter>,
    /// EXP-783: the next wire sequence. Seeded from the journal FILE's line
    /// count, so a resumed run keeps counting where its predecessor stopped
    /// and a viewer's `history_page` asks line up with what it already holds.
    next_seq: u64,
    /// EXP-906: the cap notice the last `push` minted, until the loop sends
    /// it. Already in the memory journal (so a join replays it); the file is
    /// stopped by definition, so it is never on disk.
    notice: Option<(u64, ActivityEvent)>,
}

impl Recorder {
    /// Record one already-prepared event (redacted upstream, stamped by
    /// `prepare_for_journal`): disk first, then memory, which takes it.
    /// Returns the wire sequence the event was numbered with.
    fn push(&mut self, event: ActivityEvent) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        let mut capped = false;
        if let Some(file) = self.file.as_mut() {
            file.append(&event);
            capped = file.take_cap_notice();
        }
        self.journal.push_seq(seq, event);
        if capped {
            // EXP-906: the moment the file stops, the run says so — one row
            // every viewer sees, instead of a device log line nobody reads.
            let notice_seq = self.next_seq;
            self.next_seq += 1;
            let notice = ActivityEvent::narration(crate::history::JOURNAL_CAP_NOTICE);
            self.journal.push_seq(notice_seq, notice.clone());
            self.notice = Some((notice_seq, notice));
        }
        seq
    }

    /// EXP-906: the notice `push` minted, once.
    fn take_notice(&mut self) -> Option<(u64, ActivityEvent)> {
        self.notice.take()
    }
}

/// The run's whole life: record activity, serve the eNet viewers, dispatch
/// their frames and the control socket's commands, until the run ends.
async fn run_publisher_loop(
    spec: PublishSpec,
    hooks: PublisherHooks,
    cmd_rx: flume::Receiver<PublisherCmd>,
    cmd_tx: flume::WeakSender<PublisherCmd>,
    running: Arc<AtomicBool>,
) {
    let hooks = Arc::new(hooks);
    // VAPP-33: the run's eNet room, for its whole lifetime. Dropping `feeds`
    // (every exit below) says `bye` to its viewers.
    let room = spec
        .audience
        .clone()
        .map(|audience| crate::enet_steer::register_room(&spec.session_id, audience, cmd_tx));
    let mut feeds = crate::enet_steer::Feeds::new(&spec.session_id, room);
    // EXP-511/EXP-825: the session's localized image embeds — filled by the
    // input path (and the engine's start prompt), read by the activity path.
    let embeds: ImageEmbedMap = spec.embeds.clone();
    // EXP-514: remote input is handled on its own session-lived task, fed in
    // order over this channel — the loop must never await a download.
    let input_tx = spawn_input_pump(hooks.clone(), embeds.clone());
    let file = spec
        .journal_dir
        .as_deref()
        .and_then(|dir| JournalWriter::open(dir, &spec.session_id));
    // EXP-783: a resumed run's journal file already holds N lines, and the
    // viewer that reads it back numbers them 0..N — so this run's first event
    // must be N, never 0.
    let next_seq = file.as_ref().map_or(0, JournalWriter::lines);
    let mut recorder = Recorder {
        journal: ActivityJournal::new(),
        file,
        next_seq,
        notice: None,
    };
    let mut lp = Loop {
        spec: &spec,
        hooks: &hooks,
        input_tx: &input_tx,
        recorder: &mut recorder,
        embeds: &embeds,
        running: &running,
        feeds: &mut feeds,
    };
    while let Ok(cmd) = cmd_rx.recv_async().await {
        match cmd {
            PublisherCmd::Activity(event) => {
                lp.record(event);
            }
            PublisherCmd::Shutdown { outcome } => {
                lp.feeds.set_outcome(outcome);
                break;
            }
            cmd => {
                if let Some(Steered::Kill) = lp.command(cmd) {
                    lp.kill();
                    break;
                }
            }
        }
    }
    running.store(false, Ordering::SeqCst);
}

// ---------------------------------------------------------------------------
// Remote input (EXP-514: its own ordered task, off the pump loop)
// ---------------------------------------------------------------------------

// EXP-383: composer-message chunk tracking. A message arrives as ≤4 KiB
// text chunks closed by a bare `\r`; the FIRST chunk is where codex's
// leading-sigil guard applies and where the text sink's buffer opens. A chunk
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

/// The state the publisher loop threads through its commands.
struct Loop<'a> {
    spec: &'a PublishSpec,
    hooks: &'a PublisherHooks,
    input_tx: &'a flume::Sender<String>,
    recorder: &'a mut Recorder,
    embeds: &'a ImageEmbedMap,
    running: &'a Arc<AtomicBool>,
    feeds: &'a mut crate::enet_steer::Feeds,
}

impl Loop<'_> {
    /// Record one event (stamped, embeds restored) and fan it out live to
    /// the eNet viewers, then (EXP-906) the cap notice it may have minted.
    fn record(&mut self, mut event: ActivityEvent) {
        prepare_for_journal(&mut event, self.embeds);
        let seq = self.recorder.push(event.clone());
        self.feeds.activity(seq, &event);
        if let Some((seq, notice)) = self.recorder.take_notice() {
            self.feeds.activity(seq, &notice);
        }
    }

    /// The eNet and control commands. `Some(Steered::Kill)` when the run is
    /// to end.
    fn command(&mut self, cmd: PublisherCmd) -> Option<Steered> {
        match cmd {
            PublisherCmd::EnetJoin(peer) => self.feeds.join(peer, &self.recorder.journal),
            PublisherCmd::EnetLeave(id) => self.feeds.leave(&id),
            PublisherCmd::EnetFrame { from, frame } => {
                let Some(frame) = ServerFrame::parse(&frame) else {
                    log::debug!("steer publisher: unparseable eNet frame ignored");
                    return None;
                };
                match steer_frame(frame, self.spec, self.hooks, self.input_tx) {
                    Steered::Reply(reply) => self.feeds.send_to(&from, reply),
                    Steered::Kill => return Some(Steered::Kill),
                    Steered::Done => {}
                }
            }
            PublisherCmd::Control(ControlCmd::Kill) => return Some(Steered::Kill),
            PublisherCmd::Control(ControlCmd::Interrupt) => {
                if let Some(interrupt) = &self.hooks.interrupt {
                    interrupt();
                }
            }
            // EXP-936: the hook decides off in-memory state; the verdict goes
            // straight back up the control socket (the relay holds the web
            // server's HTTP call on it) — never through `input_tx`, whose
            // choreography sleeps.
            PublisherCmd::Control(ControlCmd::Compact { keep, reply }) => {
                let verdict = match &self.hooks.compact {
                    Some(compact) => compact(keep),
                    None => CompactVerdict::Refused(CompactRefusal::UnsupportedAgent),
                };
                log::info!("steer publisher: compact_request for {} → {verdict:?}", self.spec.session_id);
                (reply.0)(verdict);
            }
            #[cfg(test)]
            PublisherCmd::Tap(id, tx) => self.feeds.tap(id, tx.0, &self.recorder.journal),
            PublisherCmd::Activity(_) | PublisherCmd::Shutdown { .. } => {}
        }
        None
    }

    /// A kill (an eNet viewer's or the control socket's): the hook ends the
    /// child; the viewers hear `bye killed`.
    fn kill(&mut self) {
        log::info!("steer publisher: kill received");
        self.running.store(false, Ordering::SeqCst);
        self.feeds.set_outcome(Some("killed".to_string()));
        (self.hooks.kill)(KillSignal::RemoteKill);
    }
}

/// What one steer client frame asks of the loop beyond its hooks.
enum Steered {
    Done,
    /// End the run (`kill`).
    Kill,
    /// A frame for the asker only (a `history_chunk`).
    Reply(String),
}

/// The client frames an eNet viewer sends: input → the input task, answer,
/// set_config, set_mode, interrupt, unqueue, kill, history_page from the
/// disk journal. Anything else (frames only a relay ever sent) is ignored.
fn steer_frame(
    frame: ServerFrame,
    spec: &PublishSpec,
    hooks: &PublisherHooks,
    input_tx: &flume::Sender<String>,
) -> Steered {
    match frame {
        ServerFrame::Input { data } => {
            // EXP-514: input choreography sleeps (Enter separation, picker
            // revalidation) and downloads (EXP-511 embed localization — up
            // to 60s per blob) run on the input task, strictly in order; a
            // send after teardown is a harmless no-op.
            let _ = input_tx.send(data);
        }
        // EXP-249: the semantic answer path — the emitter owns question
        // identity + the TUI key choreography, so the publisher only routes.
        ServerFrame::Answer { question_id, ask_id, keys, text } => {
            let answer = RemoteAnswer { question_id, ask_id, keys, text };
            // EXP-1316/EXP-1317: the synced card this question was posted as
            // closes too — but NOT from here: the engine's `answer_ask` is
            // the choke point every answer path shares (this frame, the
            // IDE's local card, the CLI's TUI) and fires the `answer_tap`
            // for all of them. A tap here would double the write-back.
            if let Some(answers) = &hooks.answers {
                answers.submit(answer);
            }
        }
        // EXP-746: live config. Like `answer`, never through `input_tx` — a
        // model switch has no keystroke form, and only the engine holds the
        // ACP session that can apply it. `None` (the PTY path) is a
        // documented no-op: such a run advertises no chips to change.
        ServerFrame::SetConfig { id, value } => {
            if let Some(config) = &hooks.config {
                config.submit(ConfigChange::Option { id, value });
            }
        }
        ServerFrame::SetMode { id } => {
            if let Some(config) = &hooks.config {
                config.submit(ConfigChange::Mode { id });
            }
        }
        // EXP-790/EXP-861: a viewer's Stop. Never through `input_tx` — the
        // ESC keystroke path is the TUI's, and this frame is what every
        // client sends.
        ServerFrame::Interrupt { .. } => {
            if let Some(interrupt) = &hooks.interrupt {
                interrupt();
            }
        }
        ServerFrame::Unqueue { id } => {
            if let Some(unqueue) = &hooks.unqueue {
                unqueue(id);
            }
        }
        // §8.4: kill → end the session. The kill hook kills the child (whose
        // exit hook ends the synced row).
        ServerFrame::Kill { .. } => return Steered::Kill,
        // EXP-783: a viewer scrolled past the top of what it holds. The page
        // comes off the DISK journal, not the in-memory one — the memory
        // journal is bounded and the file is the whole run. Answered inline:
        // one page is a bounded read and a bounded frame.
        ServerFrame::HistoryPage { request_id, before_seq, limit, .. } => {
            let page = spec
                .journal_dir
                .as_deref()
                .and_then(|dir| {
                    crate::history::read_journal_page(
                        dir,
                        &spec.session_id,
                        before_seq,
                        limit.min(crate::history::HISTORY_PAGE_MAX) as usize,
                    )
                })
                .unwrap_or_default();
            return Steered::Reply(crate::history::history_chunk_frame(&request_id, page));
        }
        _ => {}
    }
    Steered::Done
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frames::QuestionOption;

    /// EXP-906: the append that stops the file mints ONE notice row, numbered
    /// right behind the event, held in the memory journal for the reconnect
    /// replay and handed to the loop once.
    #[test]
    fn the_recorder_mints_one_cap_notice_when_the_file_stops() {
        let data_dir = std::env::temp_dir().join(format!(
            "exp-publisher-cap-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = crate::history::journal_path(&data_dir, "sess-cap").unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, vec![b'\n'; crate::history::JOURNAL_FILE_CAP as usize + 1])
            .unwrap();
        let file = JournalWriter::open(&data_dir, "sess-cap").unwrap();
        let next_seq = file.lines();
        let mut recorder = Recorder {
            journal: ActivityJournal::new(),
            file: Some(file),
            next_seq,
            notice: None,
        };

        let seq = recorder.push(ActivityEvent::narration("over the cap"));
        let (notice_seq, notice) = recorder.take_notice().expect("a notice");
        assert_eq!(notice_seq, seq + 1);
        assert_eq!(
            notice,
            ActivityEvent::narration(crate::history::JOURNAL_CAP_NOTICE)
        );
        assert!(recorder.take_notice().is_none(), "handed out once");
        recorder.push(ActivityEvent::narration("later"));
        assert!(recorder.take_notice().is_none(), "never minted twice");
        let replay: Vec<u64> = recorder.journal.replay_seq().map(|(seq, _)| seq).collect();
        assert_eq!(replay, vec![seq, notice_seq, seq + 2]);
        let _ = std::fs::remove_dir_all(&data_dir);
    }
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

    /// A session is "publishing here" (and [`control`] reaches it) for as
    /// long as its newest publisher holds it; an older one letting go never
    /// clears the newer one.
    #[test]
    fn a_live_publisher_is_registered_until_it_lets_go() {
        let (first_tx, first_rx) = flume::unbounded();
        let (second_tx, second_rx) = flume::unbounded();
        assert!(!is_publishing("sess-live"));
        assert!(!control("sess-live", ControlCmd::Interrupt));
        let first = LivePublisher::claim("sess-live", first_tx.downgrade());
        assert!(is_publishing("sess-live"));
        let second = LivePublisher::claim("sess-live", second_tx.downgrade());
        drop(first);
        assert!(control("sess-live", ControlCmd::Interrupt), "the newer publisher still owns it");
        assert_eq!(second_rx.try_recv().unwrap(), PublisherCmd::Control(ControlCmd::Interrupt));
        assert!(first_rx.try_recv().is_err());
        drop(second);
        assert!(!is_publishing("sess-live"));
        drop(first_tx);
    }

    // ── Full-task tests: a tapped viewer queue stands in for an eNet viewer

    use tokio_tungstenite::tungstenite::Message;

    /// The endpoint the tap and the injected frames come from.
    fn tap_peer() -> enet_core::EndpointId {
        enet_core::Identity::generate().endpoint_id()
    }

    /// What the tests inject: an eNet viewer's frames, from the tap's peer.
    struct Inject {
        cmd_tx: flume::Sender<PublisherCmd>,
        from: enet_core::EndpointId,
    }

    impl Inject {
        fn send(&self, message: Message) -> Result<(), ()> {
            let Message::Text(frame) = message else { return Err(()) };
            self.cmd_tx.send(PublisherCmd::EnetFrame { from: self.from, frame }).map_err(|_| ())
        }
    }

    /// Joins a raw queue to `handle`'s run as a viewer: everything the run
    /// sends that viewer lands on the receiver (its join's `activity_reset`
    /// and `activity_synced` already consumed); the [`Inject`] sends frames
    /// as that viewer.
    fn tap(handle: &PublisherHandle) -> (flume::Receiver<String>, Inject) {
        let (tx, rx) = flume::unbounded();
        let from = tap_peer();
        handle.cmd_tx.send(PublisherCmd::Tap(from, TapTx(tx))).unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), r#"{"t":"activity_reset"}"#);
        loop {
            let frame = rx.recv_timeout(Duration::from_secs(5)).unwrap();
            if frame.contains(r#""t":"activity_synced""#) {
                break;
            }
        }
        (rx, Inject { cmd_tx: handle.cmd_tx.clone(), from })
    }

    #[derive(Default)]
    struct Recorded {
        inputs: Mutex<Vec<Vec<u8>>>,
        kills: Mutex<Vec<KillSignal>>,
    }

    fn recording_hooks(recorded: Arc<Recorded>) -> PublisherHooks {
        recording_hooks_with(recorded, None)
    }

    fn recording_hooks_with(
        recorded: Arc<Recorded>,
        answers: Option<Arc<AnswerLink>>,
    ) -> PublisherHooks {
        let r1 = recorded.clone();
        let r2 = recorded;
        PublisherHooks {
            write_input: Arc::new(move |bytes| {
                r1.inputs.lock().unwrap().push(bytes.to_vec());
            }),
            kill: Arc::new(move |signal| {
                r2.kills.lock().unwrap().push(signal);
            }),
            answers,
            agent: SessionAgent::Claude,
            text_sink: None,
            attachments: None,
            commands: None,
            config: None,
            interrupt: None,
            unqueue: None,
            compact: None,
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

    /// EXP-773: every published event also lands in the durable per-session
    /// journal file, so the run has a transcript after the process is gone.
    #[test]
    fn published_events_are_appended_to_the_session_journal_file() {
        let runtime = SteerRuntime::new().unwrap();
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
                embeds: ImageEmbeds::default(),
                audience: None,
            },
            recording_hooks(recorded),
        );
        let (seen_rx, _inject_tx) = tap(&handle);

        handle
            .activity_sender()
            .send(ActivityEvent::narration("on disk"));
        seen_rx.recv_timeout(Duration::from_secs(5)).unwrap(); // the live copy
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
    fn a_viewer_gets_activity_steers_and_kills() {
        let runtime = SteerRuntime::new().unwrap();

        let recorded = Arc::new(Recorded::default());
        let (link, answers_rx) = AnswerLink::new();
        let handle = publish(
            &runtime,
            PublishSpec {
                session_id: "sess-t".to_string(),
                issue_id: Some("issue-t".to_string()),
                journal_dir: None,
                embeds: ImageEmbeds::default(),
                audience: None,
            },
            recording_hooks_with(recorded.clone(), Some(link.clone())),
        );
        // A run serves until it ends, viewer or not (VAPP-43: no relay to
        // be disabled).
        let (seen_rx, inject_tx) = tap(&handle);

        // 1) activity events go out stamped, live, to the joined viewer.
        let sender = handle.activity_sender();
        sender.send(ActivityEvent::narration("Session started"));
        let narration = seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            narration.starts_with(r#"{"t":"activity","event":{"kind":"narration","text":"Session started","at":"#),
            "{narration}"
        );

        // 2) remote input → the PTY-writer hook, byte-identical.
        inject_tx
            .send(Message::Text(r#"{"t":"input","data":"ls\r"}"#.to_string()))
            .unwrap();
        wait_for(|| !recorded.inputs.lock().unwrap().is_empty());
        assert_eq!(recorded.inputs.lock().unwrap()[0], b"ls\r");

        // 3) a semantic answer routes to the emitter's link, NOT the PTY.
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

        // 4) an unknown frame parses to None and is ignored — the loop keeps
        // running (the kill below still lands).
        inject_tx
            .send(Message::Text(
                r#"{"t":"presence","viewers":[],"steererId":null}"#.to_string(),
            ))
            .unwrap();

        // 5) kill → kill hook fires, the viewers hear `bye killed`, the task
        // stops.
        inject_tx
            .send(Message::Text(r#"{"t":"kill"}"#.to_string()))
            .unwrap();
        wait_for(|| !recorded.kills.lock().unwrap().is_empty());
        assert_eq!(recorded.kills.lock().unwrap()[0], KillSignal::RemoteKill);
        let bye = seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(bye, r#"{"t":"bye","outcome":"killed"}"#);
        wait_for(|| !handle.is_active());
    }

    /// VAPP-43: the relay's session-scoped commands, down the CONTROL
    /// socket, reach the run by id: interrupt → the hook, kill → the kill
    /// hook and `bye killed`; an unknown session is not routed.
    #[test]
    fn control_commands_reach_the_live_run_by_session_id() {
        let runtime = SteerRuntime::new().unwrap();
        let recorded = Arc::new(Recorded::default());
        let interrupts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut hooks = recording_hooks(recorded.clone());
        let counted = interrupts.clone();
        hooks.interrupt = Some(Arc::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
        }));
        let handle = publish(
            &runtime,
            PublishSpec { session_id: "sess-ctl".to_string(), ..Default::default() },
            hooks,
        );
        let (seen_rx, _inject_tx) = tap(&handle);
        assert!(!control("sess-elsewhere", ControlCmd::Kill));
        assert!(control("sess-ctl", ControlCmd::Interrupt));
        wait_for(|| interrupts.load(Ordering::SeqCst) == 1);
        assert!(recorded.kills.lock().unwrap().is_empty(), "a Stop never ends the run");
        assert!(control("sess-ctl", ControlCmd::Kill));
        wait_for(|| !recorded.kills.lock().unwrap().is_empty());
        assert_eq!(
            seen_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            r#"{"t":"bye","outcome":"killed"}"#
        );
        wait_for(|| !handle.is_active() && !is_publishing("sess-ctl"));
    }

    // ── EXP-724: remote slash commands ─────────────────────────────────────

    /// EXP-746: without a config link the two live-config frames are a
    /// documented NO-OP — never an answer, never a surfaced error.
    #[test]
    fn set_config_and_set_mode_are_no_ops_without_a_config_link() {
        let runtime = SteerRuntime::new().unwrap();
        let recorded = Arc::new(Recorded::default());
        let (answer_link, answers_rx) = AnswerLink::new();
        let handle = publish(
            &runtime,
            PublishSpec {
                session_id: "sess-cfg-none".to_string(),
                issue_id: None,
                journal_dir: None,
                embeds: ImageEmbeds::default(),
                audience: None,
            },
            recording_hooks_with(recorded.clone(), Some(answer_link)),
        );
        let (_seen_rx, inject_tx) = tap(&handle);

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
        handle.shutdown(None);
    }

    /// The positive twin: with a link wired (the ACP path) both frames cross
    /// to the engine verbatim, blank value included, and still never touch
    /// the PTY writer.
    #[test]
    fn set_config_and_set_mode_cross_to_the_config_link() {
        let runtime = SteerRuntime::new().unwrap();
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
                embeds: ImageEmbeds::default(),
                audience: None,
            },
            hooks,
        );
        let (_seen_rx, inject_tx) = tap(&handle);

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

    /// The verdict a control-socket compaction ask gets back.
    fn compact(session_id: &str, keep: Option<&str>) -> CompactVerdict {
        let (tx, rx) = flume::bounded(1);
        let reply = CompactReply(Arc::new(move |verdict| {
            let _ = tx.send(verdict);
        }));
        assert!(control(session_id, ControlCmd::Compact { keep: keep.map(str::to_string), reply }));
        rx.recv_timeout(Duration::from_secs(5)).unwrap()
    }

    /// EXP-936/VAPP-43: a `compact_request` down the control socket is
    /// answered through its reply with the hook's verdict, and the `keep`
    /// text reaches the hook verbatim.
    #[test]
    fn a_compact_request_is_answered_with_the_hooks_verdict() {
        let runtime = SteerRuntime::new().unwrap();
        let recorded = Arc::new(Recorded::default());
        let asked: Arc<Mutex<Vec<Option<String>>>> = Arc::new(Mutex::new(Vec::new()));
        let mut hooks = recording_hooks(recorded.clone());
        let asked_in_hook = asked.clone();
        hooks.compact = Some(Arc::new(move |keep: Option<String>| {
            let verdict = match keep.as_deref() {
                Some("open threads") => CompactVerdict::Accepted,
                _ => CompactVerdict::Refused(CompactRefusal::TooEarly),
            };
            asked_in_hook.lock().unwrap().push(keep);
            verdict
        }));
        let handle = publish(
            &runtime,
            PublishSpec { session_id: "sess-compact".to_string(), ..Default::default() },
            hooks,
        );
        assert_eq!(compact("sess-compact", Some("open threads")), CompactVerdict::Accepted);
        assert_eq!(compact("sess-compact", None), CompactVerdict::Refused(CompactRefusal::TooEarly));
        assert_eq!(
            asked.lock().unwrap().clone(),
            vec![Some("open threads".to_string()), None]
        );
        assert!(
            recorded.inputs.lock().unwrap().is_empty(),
            "a compaction ask never reaches the input path"
        );
        handle.shutdown(None);
    }

    /// EXP-936: no hook (a run with no engine behind it) still answers, as
    /// `unsupported_agent`, instead of leaving the relay to time out.
    #[test]
    fn a_compact_request_without_a_hook_is_refused_as_unsupported() {
        let runtime = SteerRuntime::new().unwrap();
        let recorded = Arc::new(Recorded::default());
        let handle = publish(
            &runtime,
            PublishSpec { session_id: "sess-nohook".to_string(), ..Default::default() },
            recording_hooks(recorded),
        );
        assert_eq!(
            compact("sess-nohook", None),
            CompactVerdict::Refused(CompactRefusal::UnsupportedAgent)
        );
        handle.shutdown(None);
    }

    /// A catalog command is buffered, never written, and crosses whole to the
    /// emitter on its `\r` — the publisher cannot type one (no grid, no turn
    /// state) and must not hand it to the agent as prose either.
    #[test]
    fn a_catalog_command_crosses_to_the_command_link_instead_of_the_pty() {
        let runtime = SteerRuntime::new().unwrap();
        let recorded = Arc::new(Recorded::default());
        let link = CommandLink::new();
        let mut hooks = recording_hooks(recorded.clone());
        hooks.commands = Some(link.clone());
        let handle = publish(
            &runtime,
            PublishSpec {
                session_id: "sess-cmd".to_string(),
                issue_id: None,
                journal_dir: None,
                embeds: ImageEmbeds::default(),
                audience: None,
            },
            hooks,
        );
        let (_seen_rx, inject_tx) = tap(&handle);

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
        // A command name that is not in claude's catalog is prose too
        // (thread 58cb32df: `/new` IS one now; `/cost` never was).
        recorded.inputs.lock().unwrap().clear();
        send("/cost");
        send("\r");
        wait_for(|| recorded.inputs.lock().unwrap().len() >= 2);
        assert_eq!(
            recorded.inputs.lock().unwrap().as_slice(),
            &[b"/cost".to_vec(), b"\r".to_vec()]
        );
        assert!(link.try_recv().is_none());
        handle.shutdown(None);
    }

    /// A sunk session's messages route to the engine's text sink; its
    /// COMMANDS must not — a `/compact` sent as text is prose to the model.
    #[test]
    fn a_catalog_command_bypasses_the_text_sink() {
        let runtime = SteerRuntime::new().unwrap();
        let recorded = Arc::new(Recorded::default());
        let sunk: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let link = CommandLink::new();
        let mut hooks = recording_hooks(recorded.clone());
        hooks.agent = SessionAgent::Claude;
        hooks.commands = Some(link.clone());
        let sink = sunk.clone();
        hooks.text_sink = Some(Arc::new(move |text| sink.lock().unwrap().push(text)));
        let handle = publish(
            &runtime,
            PublishSpec {
                session_id: "sess-cmd-sink".to_string(),
                issue_id: None,
                journal_dir: None,
                embeds: ImageEmbeds::default(),
                audience: None,
            },
            hooks,
        );
        let (_seen_rx, inject_tx) = tap(&handle);

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

    /// EXP-825: the host localizes a START prompt through the exported
    /// [`localize_message`] with a shared [`ImageEmbeds`] — the agent reads
    /// the numbered manifest, and an echo of the line OR the bare path
    /// restores to the token through the same map the publisher uses.
    #[test]
    fn localize_message_shares_the_restore_map_with_the_publisher() {
        let runtime = SteerRuntime::new().unwrap();
        let embeds = ImageEmbeds::default();
        let local = image_dir("start").join("11111111-2222-3333-4444-555555555555.png");
        let localized = local.clone();
        let hook: AttachmentHook = Arc::new(move |id, _name| {
            assert_eq!(id, "11111111-2222-3333-4444-555555555555");
            Ok(localized.clone())
        });
        assert!(has_image_embed(&format!("fix [Image #1]\n\n{EMBED}")));
        assert!(!has_image_embed("fix the login flicker"));
        let text = format!("fix [Image #1]\n\n{EMBED}");
        let agent_text = runtime
            .handle()
            .block_on(localize_message(text, &hook, &embeds));
        let path = local.display().to_string();
        assert_eq!(agent_text, format!("fix [Image #1]\n\nImage #1: {path}"));
        assert_eq!(
            embeds.snapshot(),
            vec![
                (format!("Image #1: {path}"), EMBED.to_string()),
                (path.clone(), EMBED.to_string()),
            ]
        );
        // The round trip: an agent echo restores to the steerer's token.
        let mut echo = ActivityEvent::user_message(format!("fix [Image #1]\n\nImage #1: {path}"));
        restore_image_embeds(&mut echo, &embeds);
        assert_eq!(echo, ActivityEvent::user_message(format!("fix [Image #1]\n\n{EMBED}")));
        let mut tool = ActivityEvent::tool("Read", Some(path.clone()));
        restore_image_embeds(&mut tool, &embeds);
        assert_eq!(tool, ActivityEvent::tool("Read", Some(EMBED.to_string())));
        // No embed = no download, text untouched.
        let plain = runtime
            .handle()
            .block_on(localize_message("plain".to_string(), &hook, &embeds));
        assert_eq!(plain, "plain");
    }

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
        let recorded = Arc::new(Recorded::default());
        let local = image_dir("rewrite").join("11111111-2222-3333-4444-555555555555.png");
        let localized = local.clone();
        let mut hooks = recording_hooks(recorded.clone());
        hooks.attachments = Some(Arc::new(move |id, _name| {
            assert_eq!(id, "11111111-2222-3333-4444-555555555555");
            Ok(localized.clone())
        }));
        let handle = publish(
            &runtime,
            PublishSpec {
                session_id: "sess-img".to_string(),
                issue_id: None,
                journal_dir: None,
                embeds: ImageEmbeds::default(),
                audience: None,
            },
            hooks,
        );
        let (seen_rx, inject_tx) = tap(&handle);

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
        let recorded = Arc::new(Recorded::default());
        let mut hooks = recording_hooks(recorded.clone());
        hooks.attachments = Some(Arc::new(|_id, _name| Err("offline".to_string())));
        let handle = publish(
            &runtime,
            PublishSpec {
                session_id: "sess-img-err".to_string(),
                issue_id: None,
                journal_dir: None,
                embeds: ImageEmbeds::default(),
                audience: None,
            },
            hooks,
        );
        let (_seen_rx, inject_tx) = tap(&handle);

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
        let recorded = Arc::new(Recorded::default());
        let release = Arc::new(AtomicBool::new(false));
        let gate = release.clone();
        let local = image_dir("slow").join("11111111-2222-3333-4444-555555555555.png");
        let localized = local.clone();
        let mut hooks = recording_hooks(recorded.clone());
        hooks.attachments = Some(Arc::new(move |_id, _name| {
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
                embeds: ImageEmbeds::default(),
                audience: None,
            },
            hooks,
        );
        let (seen_rx, inject_tx) = tap(&handle);

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
    fn a_text_sink_receives_the_localized_message() {
        // The rewrite happens BEFORE the sink buffer opens, so a sunk session
        // gets the injected paths too.
        let runtime = SteerRuntime::new().unwrap();
        let recorded = Arc::new(Recorded::default());
        let sunk: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let local = image_dir("sink").join("11111111-2222-3333-4444-555555555555.png");
        let localized = local.clone();
        let mut hooks = recording_hooks(recorded);
        hooks.agent = SessionAgent::Claude;
        let sink = sunk.clone();
        hooks.text_sink = Some(Arc::new(move |text| sink.lock().unwrap().push(text)));
        hooks.attachments = Some(Arc::new(move |_id, _name| Ok(localized.clone())));
        let handle = publish(
            &runtime,
            PublishSpec {
                session_id: "sess-img-sink".to_string(),
                issue_id: None,
                journal_dir: None,
                embeds: ImageEmbeds::default(),
                audience: None,
            },
            hooks,
        );
        let (_seen_rx, inject_tx) = tap(&handle);

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
        let embeds = ImageEmbeds::default();
        embeds.record(vec![
            (
                "Image #1: /tmp/w/.exp-steer-images/img.png".to_string(),
                EMBED.to_string(),
            ),
            (
                "/tmp/w/.exp-steer-images/img.png".to_string(),
                EMBED.to_string(),
            ),
        ]);
        let mut tool = ActivityEvent::tool("Read", Some("/tmp/w/.exp-steer-images/img.png".into()));
        restore_image_embeds(&mut tool, &embeds);
        assert_eq!(tool, ActivityEvent::tool("Read", Some(EMBED.to_string())));
        let mut question = ActivityEvent::Question {
            text: "Use /tmp/w/.exp-steer-images/img.png?".into(),
            options: vec![QuestionOption::new("Yes", "1")],
            multi_select: None,
            plan_mode: None,
            id: "/tmp/w/.exp-steer-images/img.png".into(),
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
                assert_eq!(id, "/tmp/w/.exp-steer-images/img.png");
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
        let hook: AttachmentHook = Arc::new(move |_id, _name| {
            let n = counter.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(PathBuf::from(format!("/img/{n}.png")))
        });
        let embeds = ImageEmbeds::default();
        let runtime = SteerRuntime::new().unwrap();
        let out = runtime.handle().block_on(localize_image_embeds(
            message.to_string(),
            &hook,
            &embeds,
        ));
        (out, embeds)
    }

    // ── Wave D: steered FILE lines → `File #N:` manifest lines ─────────────

    const FILE: &str = "[notes.pdf](/api/attachments/33333333-4444-5555-6666-777777777777)";

    /// Like [`localize`], but the hook answers a file's NAME so the
    /// assertions can see which kind each call was.
    fn localize_named(message: &str) -> (String, ImageEmbedMap, Arc<Mutex<Vec<Option<String>>>>) {
        let seen: Arc<Mutex<Vec<Option<String>>>> = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        let hook: AttachmentHook = Arc::new(move |id, name| {
            log.lock().unwrap().push(name.map(str::to_string));
            Ok(match name {
                Some(name) => PathBuf::from(format!("/w/{id}/{name}")),
                None => PathBuf::from(format!("/w/{id}.png")),
            })
        });
        let embeds = ImageEmbeds::default();
        let runtime = SteerRuntime::new().unwrap();
        let out = runtime.handle().block_on(localize_image_embeds(
            message.to_string(),
            &hook,
            &embeds,
        ));
        (out, embeds, seen)
    }

    #[test]
    fn file_lines_become_a_file_manifest_after_the_images() {
        let message = format!("read these [Image #1]\n\n{EMBED}\n{FILE}\n[a\\]b.txt](/api/attachments/44444444-5555-6666-7777-888888888888)");
        let (out, embeds, seen) = localize_named(&message);
        assert_eq!(
            out,
            "read these [Image #1]\n\n\
             Image #1: /w/11111111-2222-3333-4444-555555555555.png\n\
             File #1: /w/33333333-4444-5555-6666-777777777777/notes.pdf\n\
             File #2: /w/44444444-5555-6666-7777-888888888888/a]b.txt"
        );
        // The image embed's `[image](…)` half is never mistaken for a file.
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            &[None, Some("notes.pdf".to_string()), Some("a]b.txt".to_string())]
        );
        assert!(has_attachment_embed(FILE));
        assert!(!has_attachment_embed("[notes.pdf](https://example.com/x)"));
        // An echo of the file line (and of the bare path) restores the token.
        let mut echo = ActivityEvent::user_message(
            "File #1: /w/33333333-4444-5555-6666-777777777777/notes.pdf".to_string(),
        );
        restore_image_embeds(&mut echo, &embeds);
        assert_eq!(echo, ActivityEvent::user_message(FILE.to_string()));
        let mut tool = ActivityEvent::tool(
            "Read",
            Some("/w/33333333-4444-5555-6666-777777777777/notes.pdf".into()),
        );
        restore_image_embeds(&mut tool, &embeds);
        assert_eq!(tool, ActivityEvent::tool("Read", Some(FILE.to_string())));
    }

    #[test]
    fn a_failed_file_download_keeps_the_url_in_its_line() {
        let hook: AttachmentHook = Arc::new(|_id, _name| Err("offline".to_string()));
        let embeds = ImageEmbeds::default();
        let runtime = SteerRuntime::new().unwrap();
        let out = runtime.handle().block_on(localize_image_embeds(
            format!("look\n\n{FILE}"),
            &hook,
            &embeds,
        ));
        assert_eq!(out, format!("look\n\nFile #1: {FILE}"));
        let mut echo = ActivityEvent::user_message(out);
        restore_image_embeds(&mut echo, &embeds);
        assert_eq!(echo, ActivityEvent::user_message(format!("look\n\n{FILE}")));
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
        let hook: AttachmentHook = Arc::new(|id, _name| {
            // Stable per id: the same attachment localizes to the same file
            // however many messages carry it.
            Ok(PathBuf::from(format!("/img/{id}.png")))
        });
        let embeds = ImageEmbeds::default();
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

    /// A (re-)join gets `activity_reset` + the whole journal + synced.
    #[test]
    fn a_join_resets_and_replays_the_journal() {
        let runtime = SteerRuntime::new().unwrap();
        let recorded = Arc::new(Recorded::default());
        let handle = publish(
            &runtime,
            PublishSpec { session_id: "sess-replay".to_string(), ..Default::default() },
            recording_hooks(recorded),
        );
        let sender = handle.activity_sender();
        sender.send(ActivityEvent::narration("first"));
        sender.send(ActivityEvent::tool("Edit", Some("a.rs".into())));
        let (tx, rx) = flume::unbounded();
        handle.cmd_tx.send(PublisherCmd::Tap(tap_peer(), TapTx(tx))).unwrap();
        let frames: Vec<String> = (0..4).map(|_| rx.recv_timeout(Duration::from_secs(5)).unwrap()).collect();
        assert_eq!(frames[0], r#"{"t":"activity_reset"}"#);
        assert!(frames[1].contains(r#""text":"first""#), "{:?}", frames[1]);
        assert!(frames[2].contains(r#""name":"Edit""#), "{:?}", frames[2]);
        assert_eq!(frames[3], r#"{"t":"activity_synced","firstSeq":0,"lastSeq":1}"#);
        handle.shutdown(None);
    }

    #[test]
    fn clean_shutdown_sends_bye_with_exit_outcome() {
        let runtime = SteerRuntime::new().unwrap();
        let recorded = Arc::new(Recorded::default());
        let handle = publish(
            &runtime,
            PublishSpec { session_id: "sess-x".to_string(), ..Default::default() },
            recording_hooks(recorded),
        );
        let (seen_rx, _inject_tx) = tap(&handle);
        handle.shutdown(Some("exit:0".to_string()));
        let bye = seen_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(bye, r#"{"t":"bye","outcome":"exit:0"}"#);
        wait_for(|| !handle.is_active() && !is_publishing("sess-x"));
    }
}
