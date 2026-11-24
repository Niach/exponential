//! Linux on eDE (#EDE-23): the compositor's own control plane,
//! `ede-protocol`, instead of XTest (which Wayland does not have).
//!
//! Input is `input` on the person's seat (`input:person`): pointer motion,
//! evdev buttons, wheel notches, and keys as keysyms the compositor looks up
//! on its own layout (Shift/AltGr included). The screen is `capture` of an
//! output (protected windows black), windows are `list_windows` (the active
//! workspace's only) and `focus_window`. Displays and windows are in the
//! compositor's logical units, the ones the pointer speaks; a display's
//! pixels are its output's mode ([`pixel_size`]). The compositor refuses input on protected surfaces and
//! while the session is locked; those refusals are the tool's error.
//!
//! The lock is `hold_input` on a connection of its own: the person's keys,
//! pointer and wheel reach no window while it lives; their Esc ends it in
//! the compositor and comes back as `input.escape`, which is [`LockGuard`]'s
//! `on_esc`. Dropping the guard closes the connection, which releases the
//! hold (a crashed host never leaves the person locked out).
//!
//! The clipboard (`paste_text`, `copy_selection`, `read_clipboard`) is the
//! compositor's: `clipboard_serial` moves on every selection change,
//! `clipboard_snapshot` reads every type the selection offers,
//! `clipboard_set` makes the compositor own the selection (served to the
//! focused window only), `clipboard_taken` says whether a window read it.
//! An eDE older than these ops cannot read them
//! ([`ede_protocol::Error::Unsupported`]): the tool says to update eDE.
//!
//! eDE = `XDG_CURRENT_DESKTOP` names `Exponential` and its socket
//! (`$XDG_RUNTIME_DIR/exponential-desktop/$WAYLAND_DISPLAY.sock`) is there.
//! Grants by default (#EDE-22): the root token file only mints, once per
//! process and compositor, a grant for `epilot` holding `capture`,
//! `input:person` and `ui` (`focus_window`), for what no run asks (the
//! doctor, the screen state, the banner); turning computer use off revokes
//! it. A person run's calls use a grant of the run's own (#EXP-1925:
//! session id = the run's, an expiry renewed while the run calls), revoked
//! at the run's end. `EDE_TOKEN` (a grant handed to this process) is used
//! as it is when it cannot mint.
//!
//! #EXP-1514 (D 5): a space belongs to its SESSION and is kept by the host:
//! ONE keeper per compositor socket (a host has one eDE session) renews the
//! kept space grants of every session in [`KEPT`], apart from
//! [`RUN_GRANTS`] (the person runs'), over one root connection a round, and
//! says `space_busy` for every kept session whose turn runs. A space run's
//! end leaves its space and its grants; [`close_space`] and the switch
//! going off destroy the space, then revoke the grants. #EDE-193: a space
//! run's calls mint on the keeper's [`SPACE_TIMING`] too, so no grant of a
//! space outlives its host by more than one kept grant's life: a host that
//! dies (`kill -9`) leaves its spaces to close within 5 minutes and the
//! compositor's grace of its last renewal.
//!
//! #EXP-1514: an agent space is made only after the compositor's hello says
//! `space-readiness` and it answers `ready` ([`Session::space_ready`]):
//! eDE 0.2.29 says `spaces` and gets none. The answer is kept per
//! compositor ([`READINESS`]); hosts read it through [`space_offer`],
//! refreshed on a thread of its own.

use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ede_protocol::{Capability, CaptureTarget, ClipboardItem, Client, ErrorCode, InputEvent, Request, SessionId, SpaceInfo, Token, WindowId, CLIPBOARD_SECRET_HINT, CLIPBOARD_TEXT_MIMES};

use crate::{SpaceBlock, SpaceOffer};

use crate::backend::{finish_displays, Backend, Button, Clipboard, DisplayInfo, Image, LockGuard, Pasted, SavedClipboard, Target, WindowInfo};
use crate::keys::Key;

/// Where an eDE session's control plane lives, read from the environment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Session {
    pub socket: PathBuf,
    pub token_file: PathBuf,
    /// `EDE_TOKEN`, when set (wins over the file).
    pub token: Option<String>,
}

/// `Some` = this is an eDE session (`XDG_CURRENT_DESKTOP` names
/// `Exponential`): its paths, or why it cannot be driven.
pub(crate) fn session() -> Option<Result<Session, String>> {
    session_of(|name| std::env::var(name).ok())
}

fn session_of(env: impl Fn(&str) -> Option<String>) -> Option<Result<Session, String>> {
    if !env("XDG_CURRENT_DESKTOP")?.split(':').any(|desktop| desktop == "Exponential") {
        return None;
    }
    let (Some(runtime), Some(display)) = (env("XDG_RUNTIME_DIR"), env("WAYLAND_DISPLAY")) else {
        return Some(Err("This eDE session has no XDG_RUNTIME_DIR or WAYLAND_DISPLAY.".to_string()));
    };
    let runtime = Path::new(&runtime);
    Some(Ok(Session {
        socket: ede_protocol::socket_path(runtime, &display),
        token_file: ede_protocol::token_path(runtime, &display),
        token: env("EDE_TOKEN").filter(|token| !token.trim().is_empty()),
    }))
}

/// Why this eDE session cannot be driven (the doctor line), without
/// connecting.
pub(crate) fn unsupported(session: &Result<Session, String>) -> Option<String> {
    match session {
        Err(reason) => Some(reason.clone()),
        Ok(session) if !session.socket.exists() => {
            Some(format!("eDE's control socket is missing ({}); log in to the eDE session again.", session.socket.display()))
        }
        Ok(session) if session.token.is_none() && !session.token_file.exists() => {
            Some(format!("eDE's token file is missing ({}).", session.token_file.display()))
        }
        Ok(_) => None,
    }
}

/// The grant minted for this process, per compositor socket: one mint per
/// process, not one per tool call (the hub opens a backend per call).
static GRANT: Mutex<Option<(PathBuf, Token)>> = Mutex::new(None);

/// #EXP-1925: how long a person run's grant lives; a call with less than
/// [`RUN_GRANT_RENEW`] of it left mints the next one. The ones before
/// expire on their own (a call in flight keeps its connection). Never a
/// space's (#EDE-193): a space grant keeps its space at the compositor for
/// as long as it lives, so a space's grants all live [`SPACE_GRANT_TTL`].
const RUN_GRANT_TTL: Duration = Duration::from_secs(15 * 60);
const RUN_GRANT_RENEW: Duration = Duration::from_secs(5 * 60);

/// #EDE-126: how often the keeper looks at the clock.
const KEEPER_TICK: Duration = Duration::from_secs(10);

/// #EXP-1514 (C 12): a kept space grant lives 5 minutes and the next is
/// minted when less than 4 are left (every minute): a host that stalls for
/// up to four minutes keeps its spaces, and the compositor's grace is on top.
/// #EDE-193: the grant a space run's call mints is a kept one of the same
/// life: a host that dies keeps no space past it.
const SPACE_GRANT_TTL: Duration = Duration::from_secs(5 * 60);
const SPACE_GRANT_RENEW: Duration = Duration::from_secs(4 * 60);
/// A call takes a kept grant with at least this much left, else mints.
const SPACE_CALL_LEFT: Duration = Duration::from_secs(60);

/// The lifetimes a run's grants are minted and renewed by (the tests'
/// are short, or on a clock that jumps).
#[derive(Clone, Copy)]
pub(crate) struct Timing {
    ttl: Duration,
    renew: Duration,
    tick: Duration,
    /// Unix milliseconds: the wall clock.
    now: fn() -> i64,
}

const RUN_TIMING: Timing = Timing { ttl: RUN_GRANT_TTL, renew: RUN_GRANT_RENEW, tick: KEEPER_TICK, now: now_ms };
pub(crate) const SPACE_TIMING: Timing = Timing { ttl: SPACE_GRANT_TTL, renew: SPACE_GRANT_RENEW, tick: KEEPER_TICK, now: now_ms };

/// The keeper of each compositor socket, and which one (a keeper that
/// found nothing to keep ends; the next kept space starts a new one).
static KEEPERS: LazyLock<Mutex<HashMap<PathBuf, u64>>> = LazyLock::new(Default::default);

/// #EXP-1514 (D 5): a session's space as this host keeps it, from its
/// first `create_space` (or the host's start, [`hold_spaces`]) until
/// [`close_space`], the switch going off, or the process's end.
struct Kept {
    socket: PathBuf,
    /// Its space grants not yet expired, newest last.
    grants: Vec<RunGrant>,
    /// Its space, once it has one.
    space: Option<SpaceInfo>,
    /// Held at the host's start and not settled yet ([`settle_spaces`]).
    held: bool,
}

/// Per session: its kept space. Never a person run's grant
/// ([`RUN_GRANTS`]): `connect_run` takes the newest grant of a session,
/// and a kept `[space]` grant there would be handed to its next person run.
static KEPT: LazyLock<Mutex<HashMap<String, Kept>>> = LazyLock::new(Default::default);

/// The sessions with an admitted space run ([`admit`]): a call of any other
/// makes no grant and no space (a call that passed the host's check just
/// before its run's end, or one after a [`close_space`]).
static ADMITTED: LazyLock<Mutex<std::collections::HashSet<String>>> = LazyLock::new(Default::default);

/// The sessions whose TURN runs ([`turn`]): the keeper says `space_busy`
/// for their spaces each round (D 10).
static TURNS: LazyLock<Mutex<std::collections::HashSet<String>>> = LazyLock::new(Default::default);

/// The sessions that had a space: in this process, or by the host's run
/// record ([`admit`]). Such a session's first call that finds no space
/// says it is new and empty; any other gets its first one silently.
static HAD: LazyLock<Mutex<std::collections::HashSet<String>>> = LazyLock::new(Default::default);

/// Who hears a session's first space ([`crate::set_space_made_observer`]).
static MADE: Mutex<Option<std::sync::Arc<dyn Fn(&str) + Send + Sync>>> = Mutex::new(None);

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|p| p.into_inner())
}

/// One grant minted for a run.
#[derive(Clone)]
struct RunGrant {
    socket: PathBuf,
    token: Token,
    /// Unix milliseconds.
    expires_at: i64,
}

/// Per person run: the grants minted for it and not yet expired, newest last.
static RUN_GRANTS: LazyLock<Mutex<HashMap<String, Vec<RunGrant>>>> = LazyLock::new(Default::default);

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// What a tool call needs: the screen, the person's seat, and `ui` for
/// `focus_window` (`activate_window`, #EXP-1897).
const CAPABILITIES: [Capability; 3] = [Capability::Capture, Capability::InputPerson, Capability::Ui];

/// #EDE-126: what a space run's grant holds: its space and nothing else.
const SPACE_CAPABILITIES: [Capability; 1] = [Capability::Space];
/// A new space's output: the frame the vision models read best.
const SPACE_SIZE: (i32, i32) = (crate::tools::MAX_WIDTH as i32, crate::tools::MAX_HEIGHT as i32);

const NO_SPACES: &str = "This eDE makes no agent spaces (update eDE); nothing was done.";
const NO_SPACE_GRANT: &str = "eDE gave this run no space grant of its own (the token here cannot mint one); nothing was done.";
const SPACE_ENDED: &str = "The agent space had ended, so nothing was done. A new one was made: it is new and empty, whatever ran in the old one is gone. Take a screenshot and start again.";
/// #EXP-1514 (rows 7 and 8 of the design's state table): `create_space`
/// answered `limit`, every space in use, by the count or by memory.
const SPACES_FULL: &str = "The person can close one in the taskbar's Agent spaces list.";
const SPACES_OUT_OF_MEMORY: &str = "The agent spaces on this desktop are out of memory and every one is in use. The person can close one in the taskbar's Agent spaces list.";
/// The cause of [`SpaceBlock::Unavailable`] for a root that cannot mint.
const NO_MINT: &str = "the token here cannot mint a space grant";

/// #EXP-1514: how long the gate trusts a `ready` answer ([`Session::space_ready`]).
const READY_FOR: Duration = Duration::from_secs(30);
/// How long the gate's question may take: connect, hello and the answer.
const GATE_WITHIN: Duration = Duration::from_secs(5);
/// How long the offer's refresh may take ([`space_offer`]).
const OFFER_WITHIN: Duration = Duration::from_secs(2);
/// How old the offer's answer may get before a read refreshes it.
const OFFER_REFRESH: Duration = Duration::from_secs(10);

/// What a compositor said last about agent spaces.
#[derive(Clone, Debug)]
struct Said {
    at: Instant,
    offer: SpaceOffer,
    /// How many spaces it holds at most ([`ede_protocol::SpaceReadiness::max`]).
    max: Option<u32>,
}

/// Per compositor socket: its last answer ([`ask`]), and whether a refresh
/// of it runs now.
static READINESS: LazyLock<Mutex<HashMap<PathBuf, (Option<Said>, bool)>>> = LazyLock::new(Default::default);

/// #EXP-1514: ask the compositor whether it gives a run a space now, all
/// of it within `within`. A hello without `space-readiness` (eDE 0.2.29)
/// = [`SpaceBlock::OlderDesktop`]; a root that cannot mint =
/// [`SpaceBlock::Unavailable`]; then its answer. No socket, a refused
/// connect, no answer in time = [`SpaceOffer::Unknown`], never an older
/// desktop (#EXP-2065: eDE's consent question holds the hello).
fn ask(session: &Session, within: Duration) -> Said {
    let at = Instant::now();
    let said = |offer, max| Said { at, offer, max };
    let Ok(token) = session.root_token() else { return said(SpaceOffer::Unknown, None) };
    let Ok(mut root) = Client::connect_within(&session.socket, token, within) else { return said(SpaceOffer::Unknown, None) };
    if !root.hello.offers_spaces() || !root.hello.says_space_readiness() {
        return said(SpaceOffer::No(SpaceBlock::OlderDesktop), None);
    }
    if !root.hello.capabilities.contains(&Capability::System) {
        return said(SpaceOffer::No(SpaceBlock::Unavailable(NO_MINT.to_string())), None);
    }
    match root.space_readiness() {
        Ok(answer) if answer.ready => said(SpaceOffer::Offered, answer.max),
        Ok(answer) => said(SpaceOffer::No(block_of(answer.block.as_deref())), answer.max),
        Err(ede_protocol::Error::Unsupported(_)) => said(SpaceOffer::No(SpaceBlock::OlderDesktop), None),
        Err(_) => said(SpaceOffer::Unknown, None),
    }
}

/// The compositor's block, as a [`SpaceBlock`]: one this build does not
/// know is still a no, with its word as the cause.
fn block_of(block: Option<&str>) -> SpaceBlock {
    match block {
        Some("driver") => SpaceBlock::Driver,
        Some("no_user_manager") => SpaceBlock::Unavailable("no systemd user manager".to_string()),
        Some("session") => SpaceBlock::Unavailable("an agent space's session did not start".to_string()),
        Some("guard") => SpaceBlock::Unavailable("the space guard or polkit does not guard agent spaces".to_string()),
        Some(other) => SpaceBlock::Unavailable(other.to_string()),
        None => SpaceBlock::Unavailable("not ready".to_string()),
    }
}

fn remember(socket: &Path, said: Said) {
    READINESS.lock().unwrap_or_else(|p| p.into_inner()).entry(socket.to_path_buf()).or_default().0 = Some(said);
}

/// #EXP-1514 (row 6 of the design's state table): what a space run's call
/// answers when no space can be made here. Never the person's screen.
pub(crate) fn no_space(offer: &SpaceOffer) -> String {
    let why = match offer {
        SpaceOffer::Offered => return "This desktop offers an agent space.".to_string(),
        SpaceOffer::No(SpaceBlock::NotDesktop) => "it is not Exponential Desktop".to_string(),
        SpaceOffer::No(SpaceBlock::OlderDesktop) => "its Exponential Desktop is too old for agent spaces (the person can update it)".to_string(),
        SpaceOffer::No(SpaceBlock::Unavailable(cause)) => format!("agent spaces are not available here ({cause})"),
        SpaceOffer::No(SpaceBlock::Driver) => "apps do not start in an agent space on this graphics driver yet".to_string(),
        SpaceOffer::Unknown => "Exponential Desktop did not answer; try again in a moment".to_string(),
    };
    format!("This desktop offers no agent space: {why}. Nothing was done.")
}

/// #EXP-1514: what the compositor said last, read without waiting (the
/// heartbeat and the IDE's foreground call it); an answer older than
/// [`OFFER_REFRESH`], or none, starts a refresh on a thread of its own.
/// No answer (yet, or in time) = [`SpaceOffer::Unknown`]: never an older
/// desktop (#EXP-2065), only a hello without `space-readiness` says that.
/// A launch reads [`launch_offer`] instead.
pub(crate) fn space_offer(session: &Session) -> SpaceOffer {
    cached_offer(session)
}

/// #EXP-2065: the offer a launch places its run by: the cache's real answer
/// while it is fresh, else the compositor asked now (within [`GATE_WITHIN`]).
/// An empty cache or one holding a timeout is never read as an answer: a
/// launch right after the person allowed eDE's consent question gets the
/// space, not the screen. A real answer is remembered; no answer is not.
pub(crate) fn launch_offer(session: &Session) -> SpaceOffer {
    let fresh = READINESS
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .get(&session.socket)
        .and_then(|(said, _)| said.clone())
        .filter(|said| said.offer != SpaceOffer::Unknown && said.at.elapsed() < OFFER_REFRESH);
    if let Some(said) = fresh {
        return said.offer;
    }
    let said = ask(session, GATE_WITHIN);
    if said.offer != SpaceOffer::Unknown {
        remember(&session.socket, said.clone());
    }
    said.offer
}

/// #EXP-1514: [`space_offer`], once this process's cache holds the
/// compositor's first answer or `within` has passed: for a process that
/// asks once and ends (`exponential doctor`), never for a host's beat.
pub(crate) fn space_offer_settled(session: &Session, within: Duration) -> SpaceOffer {
    let deadline = Instant::now() + within;
    let _ = space_offer(session);
    let answered = || READINESS.lock().unwrap_or_else(|p| p.into_inner()).get(&session.socket).is_some_and(|(said, asking)| said.is_some() && !*asking);
    while !answered() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    space_offer(session)
}

/// [`space_offer`]'s cache read and refresh.
fn cached_offer(session: &Session) -> SpaceOffer {
    let (last, refresh) = {
        let mut all = READINESS.lock().unwrap_or_else(|p| p.into_inner());
        let (said, asking) = all.entry(session.socket.clone()).or_default();
        let stale = said.as_ref().is_none_or(|said| said.at.elapsed() >= OFFER_REFRESH);
        let refresh = stale && !*asking;
        *asking |= refresh;
        (said.as_ref().map(|said| said.offer.clone()), refresh)
    };
    if refresh {
        let asked = session.clone();
        let spawned = std::thread::Builder::new().name("computer-use-space-offer".into()).spawn(move || {
            let said = ask(&asked, OFFER_WITHIN);
            let mut all = READINESS.lock().unwrap_or_else(|p| p.into_inner());
            *all.entry(asked.socket.clone()).or_default() = (Some(said), false);
        });
        if spawned.is_err() {
            READINESS.lock().unwrap_or_else(|p| p.into_inner()).entry(session.socket.clone()).or_default().1 = false;
        }
    }
    last.unwrap_or(SpaceOffer::Unknown)
}

/// Each display's rect (logical units) with its size in pixels, as the last
/// [`Backend::displays`] read them (#EXP-1897).
static PIXELS: Mutex<Vec<((i32, i32, u32, u32), (u32, u32))>> = Mutex::new(Vec::new());

/// The pixel size of the display at this rect; `None` = not an eDE display.
pub(crate) fn pixel_size(x: i32, y: i32, width: u32, height: u32) -> Option<(u32, u32)> {
    PIXELS.lock().unwrap_or_else(|p| p.into_inner()).iter().find(|(rect, _)| *rect == (x, y, width, height)).map(|(_, pixels)| *pixels)
}

/// An output's pixels: its current mode (what a capture carries), else its
/// logical size times its scale.
fn pixels_of(output: &ede_protocol::OutputInfo) -> (u32, u32) {
    if let Some(mode) = output.modes.iter().find(|mode| mode.current) {
        return (mode.width.max(1) as u32, mode.height.max(1) as u32);
    }
    let scale = if output.scale.is_finite() && output.scale > 0.0 { output.scale } else { 1.0 };
    let scaled = |units: i32| (f64::from(units.max(1)) * scale).round() as u32;
    (scaled(output.width), scaled(output.height))
}

/// The switch turned off: the grants this process minted end (#EXP-1897),
/// every run's too (#EXP-1925), and every space this host keeps closes
/// (#EXP-1514, row 10: `destroy_space`, then the revokes).
pub(crate) fn revoke_grant() {
    if let Some(Ok(session)) = session() {
        let kept: Vec<String> = lock(&KEPT).iter().filter(|(_, kept)| kept.socket == session.socket).map(|(run, _)| run.clone()).collect();
        session.close_spaces(&kept);
        session.revoke();
        let runs: Vec<String> = lock(&RUN_GRANTS).keys().cloned().collect();
        for run in runs {
            session.end_run(&run);
        }
    }
}

/// #EXP-1925: the person run `run` ended: the grants minted for it end.
/// Touches nothing when none was.
pub(crate) fn end_run(run: &str) {
    if !lock(&RUN_GRANTS).contains_key(run) {
        return;
    }
    if let Some(Ok(session)) = session() {
        session.end_run(run);
    }
}

/// #EXP-1514 (row 13): the space run `run` ended (its MCP token is dead):
/// nothing at the compositor. Its space and kept grant stay with the host
/// for the session's next run; a call of this one that arrives late makes
/// nothing.
pub(crate) fn end_space_run(run: &str) {
    lock(&ADMITTED).remove(run);
}

/// #EXP-1514: the space run `run` was admitted ([`crate::grant_space`]);
/// `had_space` = the host's run record says the session had a space.
pub(crate) fn admit(run: &str, had_space: bool) {
    lock(&ADMITTED).insert(run.to_string());
    if had_space {
        lock(&HAD).insert(run.to_string());
    }
    // A space held at the host's start is this run's now.
    if let Some(kept) = lock(&KEPT).get_mut(run) {
        kept.held = false;
    }
}

/// #EXP-1514 (D 10): `run`'s turn started (`true`) or ended.
pub(crate) fn turn(run: &str, running: bool) {
    let mut turns = lock(&TURNS);
    if running {
        turns.insert(run.to_string());
    } else {
        turns.remove(run);
    }
}

pub(crate) fn set_space_made(hook: std::sync::Arc<dyn Fn(&str) + Send + Sync>) {
    *lock(&MADE) = Some(hook);
}

/// #EXP-1514: the sessions this host keeps a space for (its sweep reads them).
pub(crate) fn kept_spaces(session: &Session) -> Vec<String> {
    let mut kept: Vec<String> = lock(&KEPT).iter().filter(|(_, kept)| kept.socket == session.socket).map(|(run, _)| run.clone()).collect();
    kept.sort();
    kept
}

/// #EXP-1514 (rows 3, 14, 19): `run`'s space closes, whoever's grants keep
/// it: `destroy_space`, then this host's revokes. A run of it makes no new
/// space until its next grant.
pub(crate) fn close_space(session: &Session, run: &str) {
    session.close_spaces(&[run.to_string()]);
}

/// #EXP-1514 (row 15), step 1, at the host's start: a kept grant at once
/// for every space whose session `registry` (the host's run registry,
/// which outlives it) lists, so nothing expires while the rows load; with
/// the switch off they are destroyed instead. The others are left to
/// their own grants. Answers the sessions held.
pub(crate) fn hold_spaces(session: &Session, registry: &[String], switch_on: bool, timing: Timing) -> Vec<String> {
    let listed = session.root_token().and_then(|root| session.connect_token(root)).and_then(|mut root| {
        if !root.hello.capabilities.contains(&Capability::System) {
            return Err(NO_MINT.to_string());
        }
        let spaces = root.spaces().map_err(e("eDE spaces"))?;
        Ok((root, spaces))
    });
    let (mut root, spaces) = match listed {
        Ok(listed) => listed,
        Err(err) => {
            log::debug!("computer use: no agent space was adopted: {err}");
            return Vec::new();
        }
    };
    let ours: Vec<SpaceInfo> = spaces.into_iter().filter(|space| registry.iter().any(|run| run == space.session_id.as_str())).collect();
    if !switch_on {
        for space in ours {
            if let Err(err) = root.destroy_space(space.id.clone()) {
                log::warn!("computer use: an agent space of the switch that is off was not closed: {err}");
            }
        }
        return Vec::new();
    }
    let mut held = Vec::new();
    for space in ours {
        let run = space.session_id.as_str().to_string();
        if lock(&KEPT).contains_key(&run) {
            continue;
        }
        let expires_at = (timing.now)() + timing.ttl.as_millis() as i64;
        match root.mint(space.session_id.clone(), SPACE_CAPABILITIES.to_vec(), Some(expires_at)) {
            Ok(grant) => {
                let grants = vec![RunGrant { socket: session.socket.clone(), token: grant.token, expires_at }];
                lock(&KEPT).insert(run.clone(), Kept { socket: session.socket.clone(), grants, space: Some(space), held: true });
                lock(&HAD).insert(run.clone());
                held.push(run);
            }
            Err(err) => log::warn!("computer use: an agent space was not held: {err}"),
        }
    }
    drop(root);
    if !held.is_empty() {
        session.ensure_keeper(timing);
    }
    held
}

/// #EXP-1514 (row 15), step 2, once the host's rows are in sync: each held
/// session asked. `Live` here = kept; `Ended` = destroyed, but only while
/// `live_run` (the host-wide run registry, read right before) has no live
/// run of it (the desktop app and the daemon are two devices on one
/// machine); else, and `Unknown`, the hold is dropped and its own grants
/// decide.
pub(crate) fn settle_spaces(session: &Session, here: impl Fn(&str) -> crate::SessionHere, live_run: impl Fn(&str) -> bool) {
    let held: Vec<String> = lock(&KEPT).iter().filter(|(_, kept)| kept.held && kept.socket == session.socket).map(|(run, _)| run.clone()).collect();
    for run in held {
        match here(&run) {
            crate::SessionHere::Live => {
                if let Some(kept) = lock(&KEPT).get_mut(&run) {
                    kept.held = false;
                }
            }
            crate::SessionHere::Ended if !live_run(&run) => close_space(session, &run),
            crate::SessionHere::Ended | crate::SessionHere::Unknown => session.drop_hold(&run),
        }
    }
}

impl Session {
    fn root_token(&self) -> Result<Token, String> {
        let token = match &self.token {
            Some(token) => token.clone(),
            None => std::fs::read_to_string(&self.token_file).map_err(|err| format!("eDE token {}: {err}", self.token_file.display()))?,
        };
        Token::new(token.trim()).map_err(|err| format!("eDE token: {err}"))
    }

    fn connect_token(&self, token: Token) -> Result<Client, String> {
        self.hello(token).map_err(|err| self.said(err))
    }

    /// [`Session::connect_token`] with the compositor's answer as it gave
    /// it: a grant it no longer knows (a restart) is `unauthorized`
    /// ([`forgotten`]).
    fn hello(&self, token: Token) -> Result<Client, ede_protocol::Error> {
        Client::connect(&self.socket, token)
    }

    fn said(&self, err: ede_protocol::Error) -> String {
        format!("eDE ({}): {err}", self.socket.display())
    }

    /// A connection under `run`'s own grant (#EXP-1925), minted on its
    /// first call and again once less than [`RUN_GRANT_RENEW`] is left (or
    /// the compositor no longer knows it); `None`, or a root that cannot
    /// mint, = [`Session::connect`].
    fn connect_as(&self, run: Option<&str>) -> Result<Client, String> {
        let client = match run {
            None => self.connect()?,
            Some(run) => match self.connect_run(run, &CAPABILITIES, RUN_TIMING)? {
                Some(client) => client,
                None => self.connect()?,
            },
        };
        // #EXP-1514: a space grant is never a person run's: its scope is an
        // agent space, and the person's hold, gate and Esc would not be.
        if client.hello.capabilities.contains(&Capability::Space) || client.hello.unknown_capabilities.iter().any(|name| name == Capability::Space.as_str()) {
            return Err("eDE answered a person run with an agent space's grant; nothing was done.".to_string());
        }
        Ok(client)
    }

    /// #EDE-126: a connection under a space run's own grant. The
    /// invariant: its hello says `[space]` exactly, the run as its
    /// session, and a compositor that makes spaces; anything else is an
    /// error before a single op goes out. Never [`Session::connect`]: a
    /// space run has no hold, no gate and no Esc, so this process's person
    /// grant there would type on the person's seat. #EXP-1514: the
    /// session's kept grant ([`KEPT`]) while it has [`SPACE_CALL_LEFT`],
    /// else one minted now and kept.
    fn connect_space(&self, run: &str, timing: Timing) -> Result<Client, String> {
        let floor = (timing.now)() + SPACE_CALL_LEFT.min(timing.ttl / 2).as_millis() as i64;
        let cached = lock(&KEPT).get(run).filter(|kept| kept.socket == self.socket).and_then(|kept| kept.grants.last()).filter(|grant| grant.expires_at > floor).map(|grant| grant.token.clone());
        let client = match cached.map(|token| self.hello(token)) {
            Some(Ok(client)) => client,
            Some(Err(err)) if !forgotten(&err) => return Err(self.said(err)),
            _ => {
                let mut root = self.connect_token(self.root_token()?)?;
                if !root.hello.capabilities.contains(&Capability::System) {
                    return Err(NO_SPACE_GRANT.to_string());
                }
                // Read before the mint: an older compositor would serve a
                // `[space]` grant the capability-free ops on the person's desk.
                if !root.hello.offers_spaces() {
                    return Err(NO_SPACES.to_string());
                }
                let token = self.mint_kept(&mut root, run, timing)?;
                drop(root);
                self.connect_token(token)?
            }
        };
        if !client.hello.offers_spaces() {
            return Err(NO_SPACES.to_string());
        }
        // Exact: a capability this build does not know is more than `[space]`.
        if client.hello.capabilities != SPACE_CAPABILITIES || !client.hello.unknown_capabilities.is_empty() || client.hello.session_id.as_str() != run {
            return Err(NO_SPACE_GRANT.to_string());
        }
        Ok(client)
    }

    /// A space grant for `run` over `root`, kept ([`KEPT`]).
    /// #EXP-2098: the keeper's renewal of `run`'s kept grant. It only adds
    /// to a space still kept: one closed since the round read it
    /// ([`close_space`] took it out of [`KEPT`] before it returned) is
    /// never put back, and the grant minted for it is revoked at once.
    fn renew_kept(&self, root: &mut Client, run: &str, timing: Timing) -> Result<(), String> {
        let now = (timing.now)();
        let expires_at = now + timing.ttl.as_millis() as i64;
        let session = SessionId::new(run).map_err(|err| err.to_string())?;
        let token = root.mint(session, SPACE_CAPABILITIES.to_vec(), Some(expires_at)).map_err(e("eDE grant"))?.token;
        {
            let mut kept = lock(&KEPT);
            if let Some(entry) = kept.get_mut(run).filter(|entry| entry.socket == self.socket) {
                entry.grants.retain(|grant| grant.expires_at > now);
                entry.grants.push(RunGrant { socket: self.socket.clone(), token, expires_at });
                return Ok(());
            }
        }
        revoke_token(root, token)
    }

    fn mint_kept(&self, root: &mut Client, run: &str, timing: Timing) -> Result<Token, String> {
        let now = (timing.now)();
        let expires_at = now + timing.ttl.as_millis() as i64;
        let session = SessionId::new(run).map_err(|err| err.to_string())?;
        let token = root.mint(session, SPACE_CAPABILITIES.to_vec(), Some(expires_at)).map_err(e("eDE grant"))?.token;
        let mut kept = lock(&KEPT);
        let entry = kept.entry(run.to_string()).or_insert_with(|| Kept { socket: self.socket.clone(), grants: Vec::new(), space: None, held: false });
        if entry.socket != self.socket {
            // Another compositor's (a new login): that one's space is gone.
            *entry = Kept { socket: self.socket.clone(), grants: Vec::new(), space: None, held: false };
        }
        entry.grants.retain(|grant| grant.expires_at > now);
        entry.grants.push(RunGrant { socket: self.socket.clone(), token: token.clone(), expires_at });
        Ok(token)
    }

    /// `run`'s own grant holding `capabilities`; `None` = the root here
    /// cannot mint.
    fn connect_run(&self, run: &str, capabilities: &[Capability], timing: Timing) -> Result<Option<Client>, String> {
        let fresh = (timing.now)() + timing.renew.as_millis() as i64;
        let cached = RUN_GRANTS
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(run)
            .and_then(|grants| grants.last())
            .filter(|grant| grant.socket == self.socket && grant.expires_at > fresh)
            .map(|grant| grant.token.clone());
        if let Some(token) = cached {
            match self.hello(token) {
                Ok(client) => return Ok(Some(client)),
                Err(err) if forgotten(&err) => {}
                Err(err) => return Err(self.said(err)),
            }
        }
        let mut root = self.connect_token(self.root_token()?)?;
        if !root.hello.capabilities.contains(&Capability::System) {
            return Ok(None);
        }
        let now = (timing.now)();
        let expires_at = now + timing.ttl.as_millis() as i64;
        let session = SessionId::new(run).map_err(|err| err.to_string())?;
        let token = root.mint(session, capabilities.to_vec(), Some(expires_at)).map_err(e("eDE grant"))?.token;
        drop(root);
        {
            let mut runs = RUN_GRANTS.lock().unwrap_or_else(|p| p.into_inner());
            let grants = runs.entry(run.to_string()).or_default();
            grants.retain(|grant| grant.expires_at > now);
            grants.push(RunGrant { socket: self.socket.clone(), token: token.clone(), expires_at });
        }
        self.connect_token(token).map(Some)
    }

    /// #EXP-1514: the gate in a space run's own path, before a grant is
    /// minted or a space made: the compositor said `ready` within
    /// [`READY_FOR`], or says it now. A run admitted while the desktop did
    /// not answer, or on an eDE that says only `spaces` (0.2.29), makes no
    /// space. `Ok` = how many spaces it holds at most, when it says.
    fn space_ready(&self) -> Result<Option<u32>, String> {
        let fresh = READINESS
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&self.socket)
            .and_then(|(said, _)| said.clone())
            .filter(|said| said.offer == SpaceOffer::Offered && said.at.elapsed() < READY_FOR);
        let said = match fresh {
            Some(said) => said,
            None => {
                let said = ask(self, GATE_WITHIN);
                remember(&self.socket, said.clone());
                said
            }
        };
        match said.offer {
            SpaceOffer::Offered => Ok(said.max),
            offer => Err(no_space(&offer)),
        }
    }

    /// #EXP-1514 (rows 7 and 8): what a refused `create_space` tells the
    /// agent, by its code: `limit` with as many spaces open as the desktop
    /// holds = full, with fewer = out of memory; `unavailable` = it could
    /// not make one.
    fn refused_space(&self, err: &ede_protocol::Error) -> String {
        let ede_protocol::Error::Failed(failure) = err else { return format!("eDE space: {err}") };
        match failure.code {
            ErrorCode::Limit => {
                let max = READINESS.lock().unwrap_or_else(|p| p.into_inner()).get(&self.socket).and_then(|(said, _)| said.as_ref()?.max);
                let open = self.root_token().and_then(|root| self.connect_token(root)).ok().and_then(|mut root| root.spaces().ok()).map(|spaces| spaces.len());
                match (max, open) {
                    (Some(max), Some(open)) if open < max as usize => SPACES_OUT_OF_MEMORY.to_string(),
                    (Some(max), _) => format!("Every agent space on this desktop is in use ({max} at most). {SPACES_FULL}"),
                    (None, _) => format!("Every agent space on this desktop is in use. {SPACES_FULL}"),
                }
            }
            ErrorCode::Unavailable => format!("This desktop could not make an agent space: {}.", failure.message),
            _ => format!("eDE space: {err}"),
        }
    }

    /// #EXP-1514 (D 5, C 12): ONE keeper for this compositor: a thread that
    /// keeps every kept space's grant alive while no call does (a renewal
    /// runs only inside a call, and the compositor closes a space whose
    /// grant stayed expired), and says `space_busy` for every kept session
    /// whose turn runs (D 10). It wakes every [`Timing::tick`] and compares
    /// the wall clock, so the first round after a suspend renews them all.
    /// It ends when nothing is kept here; the next kept space starts it.
    fn ensure_keeper(&self, timing: Timing) {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let mine = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        match lock(&KEEPERS).entry(self.socket.clone()) {
            std::collections::hash_map::Entry::Occupied(_) => return,
            std::collections::hash_map::Entry::Vacant(slot) => drop(slot.insert(mine)),
        }
        let session = self.clone();
        let _ = std::thread::Builder::new().name("computer-use-space-keeper".into()).spawn(move || loop {
            std::thread::sleep(timing.tick);
            let mut keepers = lock(&KEEPERS);
            if keepers.get(&session.socket) != Some(&mine) {
                return;
            }
            if !lock(&KEPT).values().any(|kept| kept.socket == session.socket) {
                keepers.remove(&session.socket);
                return;
            }
            drop(keepers);
            session.keeper_round(timing);
        });
    }

    /// One round of the keeper: every kept grant with less than
    /// [`Timing::renew`] left is minted anew, all over ONE root connection;
    /// then `space_busy` for each kept session whose turn runs.
    fn keeper_round(&self, timing: Timing) {
        let fresh = (timing.now)() + timing.renew.as_millis() as i64;
        let due: Vec<String> = lock(&KEPT)
            .iter()
            .filter(|(_, kept)| kept.socket == self.socket && kept.space.is_some() && kept.grants.last().is_none_or(|grant| grant.expires_at <= fresh))
            .map(|(run, _)| run.clone())
            .collect();
        if !due.is_empty() {
            match self.root_token().and_then(|root| self.connect_token(root)) {
                Ok(mut root) => {
                    for run in due {
                        if let Err(err) = self.renew_kept(&mut root, &run, timing) {
                            log::debug!("computer use: a kept agent space's eDE grant was not renewed: {err}");
                        }
                    }
                }
                Err(err) => log::debug!("computer use: the kept agent spaces' eDE grants were not renewed: {err}"),
            }
        }
        let now = (timing.now)();
        let turns = lock(&TURNS).clone();
        let busy: Vec<Token> = lock(&KEPT)
            .iter()
            .filter(|(run, kept)| kept.socket == self.socket && kept.space.is_some() && turns.contains(*run))
            .filter_map(|(_, kept)| kept.grants.last().filter(|grant| grant.expires_at > now).map(|grant| grant.token.clone()))
            .collect();
        for token in busy {
            // An older compositor cannot read it: its spaces never give way.
            if let Err(err) = self.hello(token).and_then(|mut client| client.space_busy()) {
                log::debug!("computer use: space_busy was not said: {err}");
            }
        }
    }

    /// #EXP-1514: the spaces of `runs` close: each `destroy_space` (the one
    /// kept, or any the compositor lists for the session: another
    /// process's grant may keep it, a revoke alone would not close it),
    /// then this host's revokes, over one root connection. A run of them
    /// makes no new space until its next grant.
    fn close_spaces(&self, runs: &[String]) {
        if runs.is_empty() {
            return;
        }
        let mut grants = Vec::new();
        {
            let (mut kept, mut admitted) = (lock(&KEPT), lock(&ADMITTED));
            for run in runs {
                admitted.remove(run);
                if kept.get(run).is_some_and(|kept| kept.socket == self.socket) {
                    grants.extend(kept.remove(run).map(|kept| kept.grants).unwrap_or_default());
                }
            }
        }
        let closed = self.root_token().and_then(|root| self.connect_token(root)).and_then(|mut root| {
            let spaces = root.spaces().map_err(e("eDE spaces"))?;
            for space in spaces.into_iter().filter(|space| runs.iter().any(|run| run == space.session_id.as_str())) {
                match root.destroy_space(space.id) {
                    Err(err) if !err.has_code(ErrorCode::NotFound) => return Err(format!("eDE space: {err}")),
                    _ => {}
                }
            }
            let now = now_ms();
            for grant in grants.into_iter().filter(|grant| grant.expires_at > now) {
                revoke_token(&mut root, grant.token)?;
            }
            Ok(())
        });
        if let Err(err) = closed {
            log::warn!("computer use: an agent space was not closed: {err}");
        }
    }

    /// #EXP-1514 (row 15): the hold of `run`'s space at the host's start
    /// goes and its own grants decide: this host's grants are revoked, its
    /// space is not destroyed. (Also a grant minted for a space that could
    /// not be made.)
    fn drop_hold(&self, run: &str) {
        let grants = {
            let mut kept = lock(&KEPT);
            match kept.get(run) {
                Some(entry) if entry.socket == self.socket => kept.remove(run).map(|kept| kept.grants).unwrap_or_default(),
                _ => return,
            }
        };
        let now = now_ms();
        let live: Vec<Token> = grants.into_iter().filter(|grant| grant.expires_at > now).map(|grant| grant.token).collect();
        let dropped = self.root_token().and_then(|root| self.connect_token(root)).and_then(|mut root| live.into_iter().try_for_each(|token| revoke_token(&mut root, token)));
        if let Err(err) = dropped {
            log::warn!("computer use: a held agent space's eDE grant was not revoked: {err}");
        }
    }

    /// The person run `run`'s grants on this compositor end; the compositor
    /// forgets them.
    fn end_run(&self, run: &str) {
        let grants = {
            let mut runs = lock(&RUN_GRANTS);
            let Some(grants) = runs.get_mut(run) else { return };
            let ours: Vec<RunGrant> = grants.iter().filter(|grant| grant.socket == self.socket).cloned().collect();
            grants.retain(|grant| grant.socket != self.socket);
            if grants.is_empty() {
                runs.remove(run);
            }
            ours
        };
        let now = now_ms();
        let live: Vec<Token> = grants.into_iter().filter(|grant| grant.expires_at > now).map(|grant| grant.token).collect();
        if live.is_empty() {
            return;
        }
        let revoked = self.root_token().and_then(|root| self.connect_token(root)).and_then(|mut root| live.into_iter().try_for_each(|token| revoke_token(&mut root, token)));
        if let Err(err) = revoked {
            log::warn!("computer use: a run's eDE grant was not revoked: {err}");
        }
    }

    /// A connection under this process's own grant, minted on first use
    /// (or again when the compositor no longer knows it: a restart).
    fn connect(&self) -> Result<Client, String> {
        let cached = GRANT.lock().unwrap_or_else(|p| p.into_inner()).clone().filter(|(socket, _)| *socket == self.socket).map(|(_, token)| token);
        if let Some(token) = cached {
            match self.hello(token) {
                Ok(client) => return Ok(client),
                Err(err) if forgotten(&err) => {}
                Err(err) => return Err(self.said(err)),
            }
        }
        let mut root = self.connect_token(self.root_token()?)?;
        let token = if root.hello.capabilities.contains(&Capability::System) {
            let session = SessionId::new("epilot").map_err(|err| err.to_string())?;
            root.mint(session, CAPABILITIES.to_vec(), None).map_err(e("eDE grant"))?.token
        } else {
            root.token().clone()
        };
        drop(root);
        *GRANT.lock().unwrap_or_else(|p| p.into_inner()) = Some((self.socket.clone(), token.clone()));
        self.connect_token(token)
    }

    /// Forget the cached grant and have the compositor forget it too; the
    /// next connection mints a new one. A token handed to this process
    /// (`EDE_TOKEN` without `system`) is not ours to revoke.
    fn revoke(&self) {
        let cached = GRANT.lock().unwrap_or_else(|p| p.into_inner()).take_if(|(socket, _)| *socket == self.socket);
        let Some((_, token)) = cached else { return };
        let revoked = self.root_token().and_then(|root| self.connect_token(root)).and_then(|mut root| {
            if *root.token() == token {
                return Ok(());
            }
            root.call(Request::RevokeGrant { token }).map(|_| ()).map_err(e("eDE grant"))
        });
        if let Err(err) = revoked {
            log::warn!("computer use: the eDE grant was not revoked: {err}");
        }
    }
}

/// The backend under this process's own grant.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn open(session: &Session) -> Result<Box<dyn Backend>, String> {
    open_as(session, None, Target::Person)
}

/// The backend for `run`'s call on `target`, under its own grant
/// ([`Session::connect_as`]; a space run's: [`Session::connect_space`], and
/// its space, made on its first call). #EDE-193: a space run's grants are
/// the keeper's ([`SPACE_TIMING`]), never a person run's 15 minutes.
pub(crate) fn open_as(session: &Session, run: Option<&str>, target: Target) -> Result<Box<dyn Backend>, String> {
    let ede = |client, space| Ede { client: Mutex::new(client), session: session.clone(), run: run.map(str::to_string), target, space, notice: Mutex::new(None) };
    if target == Target::Person {
        return Ok(Box::new(ede(session.connect_as(run)?, None)));
    }
    open_space(session, run.ok_or("An agent space belongs to a run.")?, target, SPACE_TIMING)
}

fn open_space(session: &Session, run: &str, target: Target, timing: Timing) -> Result<Box<dyn Backend>, String> {
    let ede = |client, space| Ede { client: Mutex::new(client), session: session.clone(), run: Some(run.to_string()), target, space, notice: Mutex::new(None) };
    // #EXP-1514: a session without an admitted run (its end came first, or
    // its space was closed since its grant) makes no grant and no space.
    if !lock(&ADMITTED).contains(run) {
        return Err(crate::host::ENDED.to_string());
    }
    let kept = lock(&KEPT).get(run).filter(|kept| kept.socket == session.socket).and_then(|kept| kept.space.clone());
    // No grant is minted and no space made on a desktop that does not say
    // a space is ready.
    if kept.is_none() {
        session.space_ready()?;
    }
    let mut client = session.connect_space(run, timing)?;
    session.ensure_keeper(timing);
    let space = match kept {
        Some(space) => space,
        None => {
            // The session's space may be there already (another process of
            // this host made it, a restart before the adoption): it is
            // driven as it is, never told new.
            let listed = client.spaces().ok().and_then(|spaces| spaces.into_iter().next());
            let had = lock(&HAD).contains(run);
            let space = match listed {
                Some(space) => keep_space(session, run, space),
                None => {
                    let made = create_space(session, &mut client, run);
                    // No space, nothing to keep: the grant goes.
                    if made.is_err() {
                        session.drop_hold(run);
                    }
                    // "New and empty" only when it is true: the session had one.
                    match made? {
                        _ if had => return Err(SPACE_ENDED.to_string()),
                        space => space,
                    }
                }
            };
            space
        }
    };
    Ok(Box::new(ede(client, Some(space))))
}

/// `space` is `run`'s kept space; the first one a session ever had is told
/// to the host ([`crate::set_space_made_observer`]: its run record's
/// `had_space`).
fn keep_space(session: &Session, run: &str, space: SpaceInfo) -> SpaceInfo {
    if let Some(kept) = lock(&KEPT).get_mut(run).filter(|kept| kept.socket == session.socket) {
        kept.space = Some(space.clone());
    }
    if lock(&HAD).insert(run.to_string()) {
        let hook = lock(&MADE).clone();
        if let Some(hook) = hook {
            hook(run);
        }
    }
    space
}

/// A space for `run`, kept for its session's next calls and runs.
fn create_space(session: &Session, client: &mut Client, run: &str) -> Result<SpaceInfo, String> {
    let space = client.create_space(SPACE_SIZE.0, SPACE_SIZE.1).map_err(|err| session.refused_space(&err))?;
    Ok(keep_space(session, run, space))
}

pub(crate) struct Ede {
    client: Mutex<Client>,
    session: Session,
    /// The run whose grant this is; `None` = the process's.
    run: Option<String>,
    target: Target,
    /// A space run's space; `None` = the person's desk.
    space: Option<SpaceInfo>,
    /// #EDE-162: what the space's compositor said with the last capture
    /// (an app's GPU path failed, the space draws in software now).
    notice: Mutex<Option<String>>,
}

fn e(what: &str) -> impl Fn(ede_protocol::Error) -> String + '_ {
    move |err| format!("{what}: {err}")
}

/// `revoke_grant` of `token`; one the compositor no longer lists (it drops
/// expired grants, #EXP-1514) is gone already.
fn revoke_token(root: &mut Client, token: Token) -> Result<(), String> {
    match root.call(Request::RevokeGrant { token }) {
        Err(err) if !err.has_code(ErrorCode::NotFound) => Err(format!("eDE grant: {err}")),
        _ => Ok(()),
    }
}

/// #EXP-1994: the compositor no longer knows the grant (it restarted, or
/// the grant expired): mint a new one. Its code says so, whatever its words.
fn forgotten(err: &ede_protocol::Error) -> bool {
    err.has_code(ErrorCode::Unauthorized)
}

/// evdev `BTN_LEFT` / `BTN_RIGHT` / `BTN_MIDDLE`.
fn evdev_button(button: Button) -> u32 {
    match button {
        Button::Left => 0x110,
        Button::Right => 0x111,
        Button::Middle => 0x112,
    }
}

/// Our window ids are the compositor's `w<n>`.
fn window_number(id: &WindowId) -> Option<u64> {
    id.as_str().strip_prefix('w')?.parse().ok()
}

/// `argb8888` (little-endian `0xAARRGGBB` words, `stride` bytes a row) → RGBA8.
fn rgba_of(capture: &ede_protocol::Capture) -> Result<Image, String> {
    let info = &capture.info;
    if info.format != "argb8888" {
        return Err(format!("Unsupported eDE capture format {:?}.", info.format));
    }
    // Little-endian `0xAARRGGBB` words are B, G, R, A bytes.
    let (width, height, stride) = (info.width.max(0) as u32, info.height.max(0) as u32, info.stride.max(0) as usize);
    Image::from_bgra(&capture.pixels, width, height, stride).ok_or_else(|| "The eDE capture is short.".to_string())
}

impl Ede {
    /// One exchange. A space run's `not_found` may mean its space ended:
    /// then a new one is made and the answer says so ([`SPACE_ENDED`]).
    fn ask<T>(&self, what: &str, call: impl FnOnce(&mut Client) -> Result<T, ede_protocol::Error>) -> Result<T, String> {
        self.ask_told(call, |err| format!("{what}: {err}"))
    }

    /// [`Ede::ask`] with the answer's words of `told`.
    fn ask_told<T>(&self, call: impl FnOnce(&mut Client) -> Result<T, ede_protocol::Error>, told: impl FnOnce(&ede_protocol::Error) -> String) -> Result<T, String> {
        let mut client = self.client.lock().unwrap_or_else(|p| p.into_inner());
        call(&mut client).map_err(|err| self.space_ended(&mut client, &err).unwrap_or_else(|| told(&err)))
    }

    fn space_ended(&self, client: &mut Client, err: &ede_protocol::Error) -> Option<String> {
        let (space, run) = (self.space.as_ref()?, self.run.as_deref()?);
        if !matches!(err, ede_protocol::Error::Failed(failure) if failure.code == ede_protocol::ErrorCode::NotFound) {
            return None;
        }
        if client.spaces().ok()?.iter().any(|live| live.id == space.id) {
            return None;
        }
        if let Some(kept) = lock(&KEPT).get_mut(run) {
            kept.space = None;
        }
        Some(match self.session.space_ready().and_then(|_| create_space(&self.session, client, run)) {
            Ok(_) => SPACE_ENDED.to_string(),
            Err(err) => format!("The agent space had ended, and no new one was made. {err}"),
        })
    }

    fn input(&self, event: InputEvent) -> Result<(), String> {
        self.ask("eDE input", |client| client.input(event))
    }

    fn keysym(&self, keysym: u32, pressed: bool) -> Result<(), String> {
        self.input(InputEvent::Keysym { keysym, pressed })
    }
}

impl Backend for Ede {
    fn target(&self) -> Target {
        self.target
    }

    // #EDE-80: the compositor releases what a run drove when its hold or
    // its connection ends (and refuses a release after the person's Esc).
    fn lets_go_itself(&self) -> bool {
        true
    }

    fn displays(&self) -> Result<Vec<DisplayInfo>, String> {
        // #EDE-91: the person's desk only. #EDE-126: a space run is
        // listed its own space's output and nothing else.
        let outputs = self.ask("eDE outputs", Client::outputs)?;
        let outputs = if self.space.is_some() { outputs } else { ede_protocol::persons(outputs) };
        if outputs.is_empty() {
            return Err("eDE has no output.".to_string());
        }
        let connectors: Vec<Option<String>> = outputs.iter().map(|output| Some(output.name.clone())).collect();
        // The person's displays only: nothing reads a space's by its rect.
        if self.space.is_none() {
            *PIXELS.lock().unwrap_or_else(|p| p.into_inner()) =
                outputs.iter().map(|output| ((output.x, output.y, output.width.max(1) as u32, output.height.max(1) as u32), pixels_of(output))).collect();
        }
        let displays: Vec<DisplayInfo> = outputs
            .into_iter()
            .enumerate()
            .map(|(index, output)| DisplayInfo {
                id: format!("output:{}", output.name),
                x: output.x,
                y: output.y,
                width: output.width.max(1) as u32,
                height: output.height.max(1) as u32,
                // eDE lays outputs out left to right from the first.
                primary: index == 0,
                name: output.name,
                connector: None,
            })
            .collect();
        finish_displays(displays, &connectors).ok_or_else(|| "eDE has no output.".to_string())
    }

    fn screenshot(&self, display: &DisplayInfo) -> Result<Image, String> {
        let capture = self.ask("eDE capture", |client| client.capture(CaptureTarget::Output { name: display.name.clone() }))?;
        if let Some(notice) = capture.info.notice.clone() {
            *self.notice.lock().unwrap() = Some(notice);
        }
        rgba_of(&capture)
    }

    fn take_notice(&self) -> Option<String> {
        self.notice.lock().unwrap().take()
    }

    fn cursor_position(&self) -> Result<(i32, i32), String> {
        let at = self.ask("eDE pointer", Client::pointer)?;
        Ok((at.x.round() as i32, at.y.round() as i32))
    }

    fn list_windows(&self) -> Result<Vec<WindowInfo>, String> {
        let windows = self.ask("eDE windows", Client::windows)?;
        // An eDE without workspaces parks nothing; a space has none (the
        // op is the person's and never sent from one).
        let space = self.space.is_some();
        let active = if space { None } else { self.client.lock().unwrap().workspaces().ok().and_then(|all| all.into_iter().find(|workspace| workspace.active)).map(|workspace| workspace.index) };
        // The compositor lists bottom to top, then parked and minimized
        // ones: neither is on screen (#EXP-1897); an agent space's is not
        // the person's (#EDE-91).
        Ok(windows
            .into_iter()
            .rev()
            .filter(|window| (space || window.is_persons()) && !window.minimized && active.is_none_or(|active| window.workspace == active))
            .filter_map(|window| {
                Some(WindowInfo {
                    id: window_number(&window.id)?,
                    title: window.title.unwrap_or_default(),
                    app: window.app_id.unwrap_or_default(),
                    pid: None,
                    x: window.x,
                    y: window.y,
                    width: window.width.max(0) as u32,
                    height: window.height.max(0) as u32,
                    active: window.focused,
                    fullscreen: window.fullscreen,
                })
            })
            .collect())
    }

    fn activate_window(&self, id: u64) -> Result<(), String> {
        let window = WindowId::new(format!("w{id}")).map_err(|err| err.to_string())?;
        self.ask("eDE focus", |client| client.focus(window))
    }

    fn mouse_move(&self, x: i32, y: i32) -> Result<(), String> {
        self.input(InputEvent::PointerMotion { x: f64::from(x), y: f64::from(y) })
    }

    fn mouse_button(&self, button: Button, down: bool) -> Result<(), String> {
        self.input(InputEvent::PointerButton { button: evdev_button(button), pressed: down })
    }

    fn scroll(&self, dx: i32, dy: i32) -> Result<(), String> {
        self.input(InputEvent::PointerAxisDiscrete { dx, dy })
    }

    fn key(&self, key: Key, down: bool) -> Result<(), String> {
        self.keysym(super::linux::keysym_of(key), down)
    }

    fn char_stroke(&self, ch: char, down: bool) -> Result<(), String> {
        self.keysym(super::linux::char_keysym(ch), down)
    }

    fn lock(&self, on_esc: Box<dyn Fn() + Send + Sync>) -> Result<Box<dyn LockGuard>, String> {
        // #EDE-126: the hold is the person's; a space has none to take.
        if self.space.is_some() {
            return Err("an agent space has no input to hold".to_string());
        }
        Ok(Box::new(Hold::take(self.session.connect_as(self.run.as_deref())?, on_esc)?))
    }

    fn clipboard(&self) -> Result<Box<dyn Clipboard + '_>, String> {
        Ok(Box::new(EdeClipboard { client: &self.client, set: Mutex::new(None) }))
    }

    /// No environment and no wrapper of ours: the compositor's launch
    /// decides both (the space's socket and runtime directory, never the
    /// person's displays or bus; which profile an app gets).
    fn launch(&self, argv: &[String], cwd: Option<&str>) -> Result<u32, String> {
        if self.space.is_none() {
            return Err("Only a run in an agent space starts programs.".to_string());
        }
        // #EXP-1514 (row 8): a space that cannot start more, by its code.
        let program = argv.first().map_or("it", String::as_str);
        self.ask_told(
            |client| client.launch(argv.to_vec(), Default::default(), cwd.map(str::to_string)),
            |err| match err {
                ede_protocol::Error::Failed(failure) if matches!(failure.code, ErrorCode::Limit | ErrorCode::Unavailable) => {
                    format!("The agent space could not start {program}: {}.", failure.message)
                }
                err => format!("eDE launch: {err}"),
            },
        )
    }
}

/// How often [`Clipboard::wait_pasted`] asks `clipboard_taken`.
const TAKEN_POLL: Duration = Duration::from_millis(25);

/// The compositor's clipboard over the backend's connection.
struct EdeClipboard<'a> {
    client: &'a Mutex<Client>,
    /// The serial of the selection [`Clipboard::set_text`] set.
    set: Mutex<Option<u64>>,
}

/// An eDE without the clipboard ops cannot read them (#EXP-1994: it says
/// so by answering without the call's id, whatever its words).
fn clipboard_error(what: &'static str) -> impl Fn(ede_protocol::Error) -> String {
    move |err| match err {
        ede_protocol::Error::Unsupported(_) => "This eDE is too old for the clipboard; update eDE.".to_string(),
        err => format!("eDE {what}: {err}"),
    }
}

/// `set_text`'s types: every text type carrying the UTF-8 text, the
/// secret hint when sensitive (the compositor adds it too).
fn text_items(text: &str, sensitive: bool) -> Vec<ClipboardItem> {
    let mut items: Vec<ClipboardItem> = CLIPBOARD_TEXT_MIMES.iter().map(|mime| ClipboardItem { mime: mime.to_string(), data: text.as_bytes().to_vec() }).collect();
    if sensitive {
        items.push(ClipboardItem { mime: CLIPBOARD_SECRET_HINT.into(), data: b"secret".to_vec() });
    }
    items
}

impl EdeClipboard<'_> {
    fn client(&self) -> std::sync::MutexGuard<'_, Client> {
        self.client.lock().unwrap_or_else(|p| p.into_inner())
    }
}

impl Clipboard for EdeClipboard<'_> {
    fn change_count(&self) -> Result<u64, String> {
        self.client().clipboard_serial().map_err(clipboard_error("clipboard"))
    }

    fn save(&self) -> Result<SavedClipboard, String> {
        let snapshot = self.client().clipboard_snapshot().map_err(clipboard_error("clipboard"))?;
        // An eDE from before `unread` dropped types without saying so: partial.
        let partial = !snapshot.is_whole();
        Ok(SavedClipboard { formats: snapshot.items.into_iter().map(|item| (item.mime, item.data)).collect(), partial })
    }

    fn set_text(&self, text: &str, sensitive: bool) -> Result<(), String> {
        let serial = self.client().clipboard_set(text_items(text, sensitive), sensitive).map_err(clipboard_error("clipboard"))?;
        *self.set.lock().unwrap_or_else(|p| p.into_inner()) = Some(serial);
        Ok(())
    }

    fn wait_pasted(&self, timeout: Duration) -> Pasted {
        let Some(serial) = *self.set.lock().unwrap_or_else(|p| p.into_inner()) else { return Pasted::Missed };
        let until = std::time::Instant::now() + timeout;
        loop {
            match self.client().clipboard_taken(serial) {
                Ok(taken) if taken.taken => return Pasted::Taken,
                // Someone else took the clipboard first.
                Ok(taken) if taken.serial != serial => return Pasted::Missed,
                Ok(_) => {}
                Err(_) => return Pasted::Missed,
            }
            if std::time::Instant::now() >= until {
                return Pasted::Missed;
            }
            std::thread::sleep(TAKEN_POLL);
        }
    }

    fn restore(&self, saved: SavedClipboard) -> Result<(), String> {
        let sensitive = saved.formats.iter().any(|(mime, _)| mime == CLIPBOARD_SECRET_HINT);
        let items = saved.formats.into_iter().map(|(mime, data)| ClipboardItem { mime, data }).collect();
        self.client().clipboard_set(items, sensitive).map(|_| ()).map_err(clipboard_error("clipboard"))
    }

    fn text(&self) -> Result<Option<String>, String> {
        self.client().clipboard_text().map(|text| text.text).map_err(clipboard_error("clipboard"))
    }
}

/// The hold, on a connection of its own; a thread waits for the person's Esc.
struct Hold {
    stream: UnixStream,
    /// Set by the guard's drop: the close the thread then reads is ours.
    dropped: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Hold {
    fn take(mut client: Client, on_esc: Box<dyn Fn() + Send + Sync>) -> Result<Hold, String> {
        client.hold_input().map_err(e("eDE hold"))?;
        let stream = client.try_clone_stream().map_err(|err| format!("eDE hold: {err}"))?;
        let dropped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let ours = dropped.clone();
        std::thread::Builder::new()
            .name("computer-use-lock".into())
            .spawn(move || loop {
                match client.next_event() {
                    Ok(event) if event.kind == ede_protocol::events::INPUT_ESCAPE => {
                        on_esc();
                        return;
                    }
                    Ok(_) => {}
                    // Closed. By the guard's drop: done. Otherwise the
                    // compositor went away and the hold with it: the host
                    // must not believe the input is still held, so it
                    // hears an abort (#EXP-1893).
                    Err(_) => {
                        if !ours.load(std::sync::atomic::Ordering::SeqCst) {
                            on_esc();
                        }
                        return;
                    }
                }
            })
            .map_err(|err| format!("lock thread: {err}"))?;
        Ok(Hold { stream, dropped })
    }
}

impl LockGuard for Hold {}

impl Drop for Hold {
    /// Closing the connection ends the hold; never joins (this may run on
    /// the lock's own thread, inside `on_esc`).
    fn drop(&mut self) {
        self.dropped.store(true, std::sync::atomic::Ordering::SeqCst);
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}

#[cfg(test)]
#[path = "ede_test_client.rs"]
mod test_client;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Target;
    use ede_protocol::{read_frame, write_frame, Call, Envelope, Message};
    use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
    use std::sync::Arc;

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| vars.iter().find(|(key, _)| *key == name).map(|(_, value)| value.to_string())
    }

    #[test]
    fn ede_is_its_desktop_name_and_its_socket() {
        assert_eq!(session_of(env(&[("XDG_CURRENT_DESKTOP", "X-Cinnamon"), ("WAYLAND_DISPLAY", "wayland-0")])), None);
        assert_eq!(session_of(env(&[])), None);
        assert!(matches!(session_of(env(&[("XDG_CURRENT_DESKTOP", "Exponential")])), Some(Err(_))));
        let session = session_of(env(&[("XDG_CURRENT_DESKTOP", "GNOME:Exponential"), ("XDG_RUNTIME_DIR", "/run/user/1000"), ("WAYLAND_DISPLAY", "wayland-1")]))
            .unwrap()
            .unwrap();
        assert_eq!(session.socket, Path::new("/run/user/1000/exponential-desktop/wayland-1.sock"));
        assert_eq!(session.token_file, Path::new("/run/user/1000/exponential-desktop/wayland-1.token"));
        let reason = unsupported(&Ok(session)).unwrap();
        assert!(reason.contains("control socket is missing"), "{reason}");
    }

    #[test]
    fn captures_become_rgba_and_buttons_evdev() {
        let info = ede_protocol::CaptureInfo { width: 2, height: 1, stride: 12, format: "argb8888".into(), serial: 0, unchanged: false, notice: None };
        let pixels = [0xff10_2030u32, 0xff00_00ff, 0].iter().flat_map(|p| p.to_le_bytes()).collect();
        let image = rgba_of(&ede_protocol::Capture { info, pixels }).unwrap();
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.rgba, [0x10, 0x20, 0x30, 255, 0, 0, 0xff, 255]);
        assert_eq!(evdev_button(Button::Left), 272);
        assert_eq!(window_number(&WindowId::new("w42").unwrap()), Some(42));
    }

    /// A fake compositor on a socketpair: the hold is taken, the person's
    /// Esc reaches `on_esc`, and dropping the guard closes the connection.
    #[test]
    fn the_hold_hears_esc_and_lets_go_on_drop() {
        let (ours, theirs) = UnixStream::pair().unwrap();
        let server = std::thread::spawn(move || {
            let mut theirs = theirs;
            let mut ops = Vec::new();
            let reply = |stream: &mut UnixStream, id: u64, result: serde_json::Value| {
                write_frame(stream, &serde_json::to_vec(&Message::Reply { id, result }).unwrap()).unwrap();
            };
            for _ in 0..2 {
                let call: Call = serde_json::from_slice(&read_frame(&mut theirs).unwrap()).unwrap();
                let result = match &call.request {
                    Request::Hello { .. } => serde_json::json!({"version": 1, "capabilities": ["input:person"], "session_id": "epilot"}),
                    _ => serde_json::Value::Null,
                };
                ops.push(serde_json::to_value(&call).unwrap()["op"].as_str().unwrap().to_string());
                reply(&mut theirs, call.id, result);
            }
            let escape = Envelope::new("ev-1", ede_protocol::events::INPUT_ESCAPE, 0, ede_protocol::EVENT_SOURCE, None, serde_json::Value::Null);
            write_frame(&mut theirs, &serde_json::to_vec(&Message::Event { event: escape }).unwrap()).unwrap();
            // The guard's drop closes the stream: EOF.
            let closed = read_frame(&mut theirs).is_err();
            (ops, closed)
        });
        let client = Client::over(ours, Token::new("a".repeat(64)).unwrap()).unwrap();
        let escaped = Arc::new(AtomicBool::new(false));
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let tx = Mutex::new(tx);
        let flag = escaped.clone();
        let guard = Hold::take(
            client,
            Box::new(move || {
                flag.store(true, Ordering::SeqCst);
                let _ = tx.lock().unwrap().send(());
            }),
        )
        .unwrap();
        rx.recv_timeout(Duration::from_secs(5)).expect("on_esc");
        assert!(escaped.load(Ordering::SeqCst));
        drop(guard);
        let (ops, closed) = server.join().unwrap();
        assert_eq!(ops, ["hello", "hold_input"]);
        assert!(closed);
    }

    /// A fake compositor that answers hello and the hold, then does `then`
    /// with its end of the connection.
    fn held(then: impl FnOnce(UnixStream) + Send + 'static) -> (Client, std::thread::JoinHandle<()>) {
        let (ours, theirs) = UnixStream::pair().unwrap();
        let server = std::thread::spawn(move || {
            let mut theirs = theirs;
            for _ in 0..2 {
                let call: Call = serde_json::from_slice(&read_frame(&mut theirs).unwrap()).unwrap();
                let result = match &call.request {
                    Request::Hello { .. } => serde_json::json!({"version": 1, "capabilities": ["input:person"], "session_id": "epilot"}),
                    _ => serde_json::Value::Null,
                };
                write_frame(&mut theirs, &serde_json::to_vec(&Message::Reply { id: call.id, result }).unwrap()).unwrap();
            }
            then(theirs);
        });
        (Client::over(ours, Token::new("a".repeat(64)).unwrap()).unwrap(), server)
    }

    /// #EXP-1893: the compositor went away under the hold (a crash, a
    /// restart): the input is no longer held, so the host hears an abort.
    #[test]
    fn a_dead_hold_connection_is_an_abort() {
        let (release, released) = std::sync::mpsc::channel::<()>();
        let (client, server) = held(move |theirs| {
            let _ = released.recv();
            drop(theirs);
        });
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let tx = Mutex::new(tx);
        let guard = Hold::take(client, Box::new(move || { let _ = tx.lock().unwrap().send(()); })).unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err(), "nothing happened yet");
        release.send(()).unwrap();
        rx.recv_timeout(Duration::from_secs(5)).expect("the dead hold reaches on_esc");
        drop(guard);
        server.join().unwrap();
    }

    /// Dropping the guard closes the same connection: that is the host's
    /// own release, never an abort.
    #[test]
    fn dropping_the_hold_is_not_an_abort() {
        let (client, server) = held(|mut theirs| assert!(read_frame(&mut theirs).is_err()));
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let tx = Mutex::new(tx);
        let guard = Hold::take(client, Box::new(move || { let _ = tx.lock().unwrap().send(()); })).unwrap();
        drop(guard);
        server.join().unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(500)).is_err(), "on_esc fired on the guard's own drop");
    }

    /// A fake compositor answering the clipboard ops from a script; the
    /// ops it saw come back.
    fn fake_compositor(replies: Vec<Result<serde_json::Value, &'static str>>) -> (Ede, std::thread::JoinHandle<Vec<serde_json::Value>>) {
        let (ours, theirs) = UnixStream::pair().unwrap();
        let server = std::thread::spawn(move || {
            let mut theirs = theirs;
            let mut seen = Vec::new();
            let mut replies = replies.into_iter();
            loop {
                let Ok(frame) = read_frame(&mut theirs) else { return seen };
                let call: Call = serde_json::from_slice(&frame).unwrap();
                let message = if let Request::Hello { .. } = call.request {
                    Message::Reply { id: call.id, result: serde_json::json!({"version": 1, "capabilities": ["capture", "input:person"], "session_id": "epilot"}) }
                } else {
                    seen.push(serde_json::to_value(&call.request).unwrap());
                    match replies.next().expect("a scripted reply") {
                        Ok(result) => Message::Reply { id: call.id, result },
                        // An older compositor cannot parse the call: id 0.
                        Err(message) => Message::Error { id: 0, error: ede_protocol::Failure::new(ede_protocol::ErrorCode::Invalid, message) },
                    }
                };
                write_frame(&mut theirs, &serde_json::to_vec(&message).unwrap()).unwrap();
            }
        });
        let client = Client::over(ours, Token::new("a".repeat(64)).unwrap()).unwrap();
        let session = Session { socket: PathBuf::from("/nonexistent"), token_file: PathBuf::from("/nonexistent"), token: None };
        (Ede { client: Mutex::new(client), session, run: None, target: Target::Person, space: None, notice: Mutex::new(None) }, server)
    }

    /// What [`fake_socket_of`] plays.
    #[derive(Clone, Copy)]
    struct Fake {
        /// Its hello offers `spaces`.
        spaces: bool,
        /// The token file's grant holds `system`.
        root_system: bool,
        /// #EXP-1514: its hello says `space-readiness` and this is how it
        /// answers; `None` = eDE 0.2.29 (`spaces` and no readiness).
        ready: Option<Ready>,
        /// How many spaces it says it holds at most.
        max: u32,
        /// `create_space` refused with this code and these words.
        create: Option<(ErrorCode, &'static str)>,
        /// `launch` refused with this code and these words.
        launch: Option<(ErrorCode, &'static str)>,
        /// Spaces of other sessions the root lists beside the ones made here.
        others: usize,
        /// #EDE-193: the clock its grants expire by, as the compositor's
        /// do: a hello or a call on an expired or revoked grant is
        /// `unauthorized` (logged `refused <op>`), and [`Faked::tick`]
        /// closes a space whose session held no live space grant for
        /// [`GRACE_TICKS`] ticks. `None` = its grants never expire.
        clock: Option<fn() -> i64>,
    }

    /// #EDE-193: ede-compositor's `EXPIRY_GRACE`: a space whose session
    /// holds no live space grant closes after 30 of its 1 s ticks.
    const GRACE_TICKS: u32 = 30;

    /// How [`Fake`] answers `space_readiness`.
    #[derive(Clone, Copy)]
    enum Ready {
        Yes,
        Block(&'static str),
        /// It says the feature and cannot read the op.
        Unsupported,
        /// It never answers.
        Silent,
    }

    impl Fake {
        /// A compositor that makes spaces and says one is ready.
        const READY: Fake = Fake { spaces: true, root_system: true, ready: Some(Ready::Yes), max: 4, create: None, launch: None, others: 0, clock: None };
    }

    /// The fake's handles: the mints, revokes and every other op it saw
    /// (`mint <session> <expiry?>`, `revoke <token>`, else the op's name,
    /// `create_space WxH`), and a switch that ends every space.
    struct Faked {
        session: Session,
        log: Arc<Mutex<Vec<String>>>,
        dir: PathBuf,
        end_spaces: Arc<AtomicBool>,
        /// Forget every grant minted so far (a restart): their hello is
        /// refused `unauthorized`, in words of its own.
        forget: Arc<AtomicBool>,
        /// #EXP-1514: how many connections said hello with the root token.
        roots: Arc<std::sync::atomic::AtomicUsize>,
        /// #EDE-193: every mint as (its clock then, its session, its expiry).
        mints: Arc<Mutex<Vec<(i64, String, Option<i64>)>>>,
        /// Its grants: token → (session, capabilities, expiry).
        grants: Arc<Mutex<HashMap<String, (SessionId, Vec<Capability>, Option<i64>)>>>,
        revoked: Arc<Mutex<std::collections::HashSet<String>>>,
        /// Its spaces: how many were made, and each session's.
        spaces: Arc<Mutex<(u32, HashMap<String, ede_protocol::SpaceInfo>)>>,
        /// Per session: its ticks in a row without a live space grant.
        expired_ticks: Arc<Mutex<HashMap<String, u32>>>,
        clock: Option<fn() -> i64>,
        /// #EXP-2098: a mint is logged, then answered only once this is
        /// free: a test that holds it stops a mint half way.
        mint_gate: Arc<Mutex<()>>,
    }

    impl Faked {
        /// #EDE-193: one of the compositor's 1 s ticks (`spaces_expire`) by
        /// the fake's clock: a space whose session has held no live
        /// `[space]` grant for [`GRACE_TICKS`] ticks in a row closes
        /// (logged `closed <space>`); a live grant starts its count over.
        /// Answers the spaces it closed.
        fn tick(&self) -> Vec<String> {
            let now = (self.clock.expect("a fake with a clock"))();
            let (grants, revoked) = (self.grants.lock().unwrap(), self.revoked.lock().unwrap());
            let live = |session: &str| {
                grants.iter().any(|(token, (of, held, expires))| of.as_str() == session && held == &SPACE_CAPABILITIES && expires.is_none_or(|e| now < e) && !revoked.contains(token))
            };
            let (mut spaces, mut ticks) = (self.spaces.lock().unwrap(), self.expired_ticks.lock().unwrap());
            let mut closed = Vec::new();
            spaces.1.retain(|session, space| {
                let count = ticks.entry(session.clone()).or_default();
                *count = if live(session) { 0 } else { *count + 1 };
                let keep = *count < GRACE_TICKS;
                if !keep {
                    closed.push(space.id.as_str().to_string());
                }
                keep
            });
            ticks.retain(|session, _| spaces.1.contains_key(session));
            self.log.lock().unwrap().extend(closed.iter().map(|id| format!("closed {id}")));
            closed
        }

        /// #EDE-193: each grant minted for `session` as (its clock then,
        /// how long it lives).
        fn lives(&self, session: &str) -> Vec<(i64, i64)> {
            self.mints.lock().unwrap().iter().filter(|(_, of, _)| of == session).map(|(at, _, expires)| (*at, expires.expect("a grant that expires") - at)).collect()
        }
    }

    /// #EXP-1925: a fake compositor on a socket of its own: the root token
    /// mints, every mint and revoke is logged.
    fn fake_socket() -> (Session, Arc<Mutex<Vec<String>>>, PathBuf) {
        let faked = fake_socket_of(Fake { spaces: false, ready: None, ..Fake::READY });
        (faked.session, faked.log, faked.dir)
    }

    /// #EDE-126: the same with agent spaces: a minted grant says hello with
    /// what it was minted (its session, its capabilities), a space grant
    /// has a space once it made one, and an op in no space is `not_found`.
    fn fake_socket_of(fake: Fake) -> Faked {
        use ede_protocol::{ErrorCode, Failure, SpaceId, SpaceInfo};
        let dir = std::env::temp_dir().join(format!("epilot-grants-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let (socket, token_file) = (dir.join("ede.sock"), dir.join("token"));
        let root = "f".repeat(64);
        std::fs::write(&token_file, &root).unwrap();
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let log = Arc::new(Mutex::new(Vec::new()));
        let end_spaces = Arc::new(AtomicBool::new(false));
        let forget = Arc::new(AtomicBool::new(false));
        let roots = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let minted = Arc::new(Mutex::new(HashMap::<String, (SessionId, Vec<Capability>, Option<i64>)>::new()));
        let forgotten = Arc::new(Mutex::new(Vec::<String>::new()));
        let spaces = Arc::new(Mutex::new((0u32, HashMap::<String, SpaceInfo>::new())));
        let (mints, revoked) = (Arc::new(Mutex::new(Vec::new())), Arc::new(Mutex::new(std::collections::HashSet::new())));
        let (seen, end, forgets, rooted) = (log.clone(), end_spaces.clone(), forget.clone(), roots.clone());
        let mint_gate = Arc::new(Mutex::new(()));
        let gate = mint_gate.clone();
        let (all_minted, all_spaces, all_mints, all_revoked) = (minted.clone(), spaces.clone(), mints.clone(), revoked.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let (log, minted, spaces, end, root) = (seen.clone(), minted.clone(), spaces.clone(), end.clone(), root.clone());
                let (forgets, forgotten, rooted, mints, revoked) = (forgets.clone(), forgotten.clone(), rooted.clone(), mints.clone(), revoked.clone());
                let gate = gate.clone();
                std::thread::spawn(move || {
                    let mut me = SessionId::new("s").unwrap();
                    let mut is_root = false;
                    // #EDE-193: the grant this connection said hello with.
                    let mut mine: Option<String> = None;
                    // With a clock: expired or revoked, as the compositor no longer lets it in.
                    let gone = |token: &str| {
                        fake.clock.is_some_and(|now| {
                            let revoked = revoked.lock().unwrap().contains(token);
                            let expired = minted.lock().unwrap().get(token).and_then(|(_, _, expires)| *expires).is_some_and(|e| now() >= e);
                            revoked || expired
                        })
                    };
                    loop {
                        let Ok(frame) = read_frame(&mut stream) else { return };
                        let call: Call = serde_json::from_slice(&frame).unwrap();
                        if end.swap(false, Ordering::SeqCst) {
                            spaces.lock().unwrap().1.clear();
                        }
                        if forgets.swap(false, Ordering::SeqCst) {
                            let lost: Vec<String> = minted.lock().unwrap().drain().map(|(token, _)| token).collect();
                            forgotten.lock().unwrap().extend(lost);
                        }
                        let op = serde_json::to_value(&call.request).unwrap()["op"].as_str().unwrap().to_string();
                        let space = spaces.lock().unwrap().1.get(me.as_str()).cloned();
                        if let Request::SpaceReadiness = call.request {
                            log.lock().unwrap().push(op.clone());
                            let message = match fake.ready {
                                Some(Ready::Silent) => continue,
                                Some(Ready::Yes) => Message::Reply { id: call.id, result: serde_json::json!({"ready": true, "max": fake.max}) },
                                Some(Ready::Block(block)) => Message::Reply { id: call.id, result: serde_json::json!({"ready": false, "block": block, "max": fake.max}) },
                                // An older compositor cannot parse it: id 0.
                                Some(Ready::Unsupported) | None => Message::Error { id: 0, error: Failure::new(ErrorCode::Invalid, "unknown variant `space_readiness`") },
                            };
                            write_frame(&mut stream, &serde_json::to_vec(&message).unwrap()).unwrap();
                            continue;
                        }
                        let result = match call.request {
                            Request::Hello { token: Some(token), .. } if forgotten.lock().unwrap().contains(&token.as_str().to_string()) => {
                                Err(Failure::new(ErrorCode::Unauthorized, "this grant means nothing to me"))
                            }
                            Request::Hello { token: Some(ref token), .. } if gone(token.as_str()) => {
                                log.lock().unwrap().push(format!("refused {op}"));
                                Err(Failure::new(ErrorCode::Unauthorized, "unknown or expired token"))
                            }
                            // The compositor checks the grant on every call.
                            ref request if !matches!(request, Request::Hello { .. }) && mine.as_deref().is_some_and(gone) => {
                                log.lock().unwrap().push(format!("refused {op}"));
                                Err(Failure::new(ErrorCode::Unauthorized, "the grant expired"))
                            }
                            Request::Hello { token: Some(token), .. } => {
                                mine = Some(token.as_str().to_string());
                                is_root = token.as_str() == root;
                                if is_root {
                                    rooted.fetch_add(1, Ordering::SeqCst);
                                }
                                let mut capabilities = vec![Capability::Capture, Capability::InputPerson, Capability::Ui];
                                if token.as_str() == root && fake.root_system {
                                    capabilities.push(Capability::System);
                                }
                                if let Some((session, held, _)) = minted.lock().unwrap().get(token.as_str()).cloned() {
                                    (me, capabilities) = (session, held);
                                }
                                let features: &[&str] = match (fake.spaces, fake.ready) {
                                    (true, Some(_)) => &["spaces", "space-readiness"],
                                    (true, None) => &["spaces"],
                                    (false, _) => &[],
                                };
                                Ok(serde_json::json!({"version": 1, "capabilities": capabilities, "session_id": me, "features": features}))
                            }
                            Request::MintGrant { session_id, expires_at, capabilities, .. } => {
                                log.lock().unwrap().push(format!("mint {session_id} {}", expires_at.is_some()));
                                drop(gate.lock().unwrap_or_else(|p| p.into_inner()));
                                // Numbered past the forgotten ones: a new grant is never an old one.
                                let past = forgotten.lock().unwrap().len();
                                let token = Token::new(format!("{:064x}", minted.lock().unwrap().len() + past + 1)).unwrap();
                                minted.lock().unwrap().insert(token.as_str().to_string(), (session_id.clone(), capabilities.clone(), expires_at));
                                mints.lock().unwrap().push((fake.clock.map_or_else(now_ms, |now| now()), session_id.as_str().to_string(), expires_at));
                                Ok(serde_json::to_value(ede_protocol::Grant { token, session_id, run_id: None, capabilities, expires_at }).unwrap())
                            }
                            Request::RevokeGrant { token } => {
                                log.lock().unwrap().push(format!("revoke {}", token.as_str()));
                                revoked.lock().unwrap().insert(token.as_str().to_string());
                                Ok(serde_json::Value::Null)
                            }
                            Request::CreateSpace { width, height } if fake.create.is_some() => {
                                log.lock().unwrap().push(format!("create_space {width}x{height}"));
                                let (code, words) = fake.create.unwrap();
                                Err(Failure::new(code, words))
                            }
                            Request::Launch { argv, .. } if space.is_some() && fake.launch.is_some() => {
                                log.lock().unwrap().push(format!("launch {argv:?}"));
                                let (code, words) = fake.launch.unwrap();
                                Err(Failure::new(code, words))
                            }
                            Request::DestroySpace { space } if is_root => {
                                log.lock().unwrap().push(format!("destroy_space {}", space.as_str()));
                                spaces.lock().unwrap().1.retain(|_, info| info.id != space);
                                Ok(serde_json::Value::Null)
                            }
                            Request::SpaceBusy => {
                                log.lock().unwrap().push(format!("space_busy {me}"));
                                if space.is_some() { Ok(serde_json::Value::Null) } else { Err(Failure::new(ErrorCode::NotFound, "this grant has no space")) }
                            }
                            // A session has one space: a second call answers the same.
                            Request::CreateSpace { width, height } if space.is_some() => {
                                log.lock().unwrap().push(format!("create_space {width}x{height}"));
                                Ok(serde_json::to_value(space.clone().unwrap()).unwrap())
                            }
                            Request::ListSpaces if is_root => {
                                log.lock().unwrap().push(op);
                                let mine: Vec<SpaceInfo> = spaces.lock().unwrap().1.values().cloned().collect();
                                let others = (0..fake.others).map(|n| SpaceInfo { id: SpaceId::new(format!("space-o{n}")).unwrap(), session_id: SessionId::new(format!("other-{n}")).unwrap(), output: format!("space-o{n}"), width: 640, height: 480, wayland_display: "/nonexistent/wayland-0".into() });
                                Ok(serde_json::to_value(mine.into_iter().chain(others).collect::<Vec<_>>()).unwrap())
                            }
                            Request::CreateSpace { width, height } => {
                                log.lock().unwrap().push(format!("create_space {width}x{height}"));
                                let mut spaces = spaces.lock().unwrap();
                                spaces.0 += 1;
                                let name = format!("space-{}", spaces.0);
                                let info = SpaceInfo { id: SpaceId::new(&name).unwrap(), session_id: me.clone(), output: name, width, height, wayland_display: "/nonexistent/wayland-0".into() };
                                spaces.1.insert(me.as_str().to_string(), info.clone());
                                Ok(serde_json::to_value(info).unwrap())
                            }
                            Request::Launch { argv, env, cwd } if space.is_some() => {
                                log.lock().unwrap().push(format!("launch {argv:?} {env:?} {cwd:?}"));
                                Ok(serde_json::json!({"pid": 77}))
                            }
                            other => {
                                log.lock().unwrap().push(op);
                                match (other, space) {
                                    (Request::ListSpaces, space) => Ok(serde_json::to_value(space.into_iter().collect::<Vec<_>>()).unwrap()),
                                    (_, None) => Err(Failure::new(ErrorCode::NotFound, "this grant has no space")),
                                    (Request::ListOutputs, Some(space)) => Ok(serde_json::json!([{"name": space.output, "x": 0, "y": 0, "width": space.width, "height": space.height, "scale": 1.0, "space": space.id}])),
                                    (Request::ListWindows, Some(space)) => Ok(serde_json::json!([{"id": "w5", "title": "term", "app_id": "foot", "x": 0, "y": 0, "width": space.width, "height": space.height, "focused": true, "fullscreen": false, "protected": false, "x11": false, "space": space.id}])),
                                    (Request::FocusWindow { .. }, _) => Err(Failure::new(ErrorCode::NotFound, "no such window")),
                                    _ => Ok(serde_json::Value::Null),
                                }
                            }
                        };
                        let message = match result {
                            Ok(result) => Message::Reply { id: call.id, result },
                            Err(error) => Message::Error { id: call.id, error },
                        };
                        write_frame(&mut stream, &serde_json::to_vec(&message).unwrap()).unwrap();
                    }
                });
            }
        });
        Faked {
            session: Session { socket, token_file, token: None },
            log,
            dir,
            end_spaces,
            forget,
            roots,
            mints: all_mints,
            grants: all_minted,
            revoked: all_revoked,
            spaces: all_spaces,
            expired_ticks: Default::default(),
            clock: fake.clock,
            mint_gate,
        }
    }

    fn logged(faked: &Faked) -> Vec<String> {
        faked.log.lock().unwrap().clone()
    }

    /// `run` as an admitted space run ([`admit`]).
    fn admitted(run: &str) -> &str {
        admit(run, false);
        run
    }

    /// The fake itself: a minted grant says hello with what it was minted,
    /// and a space lives until the switch ends it.
    #[test]
    fn the_fake_compositor_speaks_spaces() {
        let faked = fake_socket_of(Fake::READY);
        let mut root = faked.session.connect_token(faked.session.root_token().unwrap()).unwrap();
        assert!(root.hello.offers_spaces() && root.hello.capabilities.contains(&Capability::System));
        let grant = root.mint(SessionId::new("fake-run").unwrap(), vec![Capability::Space], None).unwrap();
        let mut run = faked.session.connect_token(grant.token).unwrap();
        assert_eq!((run.hello.capabilities.clone(), run.hello.session_id.as_str()), (vec![Capability::Space], "fake-run"));
        assert!(run.input(InputEvent::Key { key: 30, pressed: true }).is_err(), "no space yet");
        let space = run.create_space(640, 480).unwrap();
        run.input(InputEvent::Key { key: 30, pressed: false }).unwrap();
        assert_eq!(run.spaces().unwrap(), [space]);
        faked.end_spaces.store(true, Ordering::SeqCst);
        assert!(run.spaces().unwrap().is_empty());
        assert_eq!(logged(&faked), ["mint fake-run false", "input", "create_space 640x480", "input", "list_spaces", "list_spaces"]);
        let narrow = fake_socket_of(Fake { spaces: false, root_system: false, ready: None, ..Fake::READY });
        let root = narrow.session.connect_token(narrow.session.root_token().unwrap()).unwrap();
        assert!(!root.hello.offers_spaces() && !root.hello.capabilities.contains(&Capability::System));
        for dir in [faked.dir, narrow.dir] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    /// #EDE-126: a space run's grant is `[space]` and nothing else, its
    /// space is made on its first call and kept, and its backend names its
    /// target.
    #[test]
    fn a_space_run_mints_space_only_and_creates_its_space_once() {
        let faked = fake_socket_of(Fake::READY);
        let run = "epilot-test-space-a";
        let client = faked.session.connect_space(run, SPACE_TIMING).unwrap();
        assert_eq!((client.hello.capabilities.clone(), client.hello.session_id.as_str()), (vec![Capability::Space], run));
        drop(client);
        let first = open_as(&faked.session, Some(admitted(run)), Target::Space(7)).unwrap();
        assert_eq!(first.target(), Target::Space(7));
        first.mouse_move(10, 10).unwrap();
        drop(first);
        open_as(&faked.session, Some(admitted(run)), Target::Space(7)).unwrap().mouse_move(20, 20).unwrap();
        assert_eq!(logged(&faked), [format!("mint {run} true"), "space_readiness".to_string(), "list_spaces".to_string(), "create_space 1280x800".to_string(), "input".to_string(), "input".to_string()]);
        // #EXP-1514: closing it destroys the space, then revokes the grant, and forgets it.
        close_space(&faked.session, run);
        assert_eq!(logged(&faked)[6..].iter().map(|line| line.split(' ').next().unwrap()).collect::<Vec<_>>(), ["list_spaces", "destroy_space", "revoke"]);
        assert!(!lock(&KEPT).contains_key(run));
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// #EDE-126: an older compositor would serve a `[space]` grant the
    /// capability-free ops on the person's desk: its hello is read before
    /// anything is minted or asked, and there is no fallback to the seat.
    #[test]
    fn an_older_compositor_makes_no_space_and_is_asked_nothing() {
        let faked = fake_socket_of(Fake { spaces: false, ready: None, ..Fake::READY });
        let refused = open_as(&faked.session, Some(admitted("epilot-test-space-old")), Target::Space(1)).err().unwrap();
        assert!(refused.contains("too old"), "{refused}");
        assert_eq!(ask(&faked.session, GATE_WITHIN).offer, SpaceOffer::No(SpaceBlock::OlderDesktop));
        assert_eq!(logged(&faked), Vec::<String>::new());
        assert!(open_as(&faked.session, None, Target::Space(1)).is_err(), "a space belongs to a run");
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// #EDE-126: the two ways a person run ends up on this process's own
    /// grant are errors for a space run: it has no hold, no gate and no
    /// Esc, so a person grant there would type on the person's seat.
    #[test]
    fn a_person_grant_never_stands_in_for_a_space_grant() {
        // `EDE_TOKEN` is a grant handed to this process: it cannot mint.
        let mut handed = fake_socket_of(Fake::READY);
        handed.session.token = Some("e".repeat(64));
        // The token file's grant holds no `system`.
        let narrow = fake_socket_of(Fake { root_system: false, ..Fake::READY });
        for faked in [&handed, &narrow] {
            let refused = open_as(&faked.session, Some(admitted("epilot-test-space-p")), Target::Space(1)).err().unwrap();
            assert!(refused.contains("space grant"), "{refused}");
            assert_eq!(logged(faked), Vec::<String>::new(), "nothing was minted, created or driven");
            assert!(faked.session.connect_as(Some("epilot-test-person-p")).is_ok(), "a person run still falls back");
            let _ = std::fs::remove_dir_all(&faked.dir);
        }
    }

    /// #EDE-126: the space ended under the run (its grant expired past the
    /// grace, the compositor restarted): a new one is made and the call
    /// says so, never silently. A `not_found` inside a live space is the
    /// call's own answer.
    #[test]
    fn a_space_that_ended_is_made_anew_and_told() {
        let faked = fake_socket_of(Fake::READY);
        let run = "epilot-test-space-e";
        let ede = open_as(&faked.session, Some(admitted(run)), Target::Space(1)).unwrap();
        let missing = ede.activate_window(999).unwrap_err();
        assert!(missing.contains("no such window"), "{missing}");
        faked.end_spaces.store(true, Ordering::SeqCst);
        let told = ede.mouse_move(10, 10).unwrap_err();
        assert!(told.contains("new and empty"), "{told}");
        drop(ede);
        open_as(&faked.session, Some(admitted(run)), Target::Space(1)).unwrap().mouse_move(10, 10).unwrap();
        assert_eq!(logged(&faked)[2..], ["list_spaces", "create_space 1280x800", "focus_window", "list_spaces", "input", "list_spaces", "create_space 1280x800", "input"]);
        assert_eq!(lock(&KEPT).get(run).unwrap().space.as_ref().unwrap().id.as_str(), "space-2");
        close_space(&faked.session, run);
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// #EDE-126: a space has no hold; its backend never asks for the
    /// person's (that would take a person grant).
    #[test]
    fn a_space_backend_never_takes_the_persons_hold() {
        let faked = fake_socket_of(Fake::READY);
        let run = "epilot-test-space-h";
        let ede = open_as(&faked.session, Some(admitted(run)), Target::Space(1)).unwrap();
        assert!(ede.lock(Box::new(|| {})).is_err());
        assert_eq!(logged(&faked), ["space_readiness".to_string(), format!("mint {run} true"), "list_spaces".to_string(), "create_space 1280x800".to_string()]);
        close_space(&faked.session, run);
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// #EDE-126: a space run's screen is its space's output and its
    /// windows are the space's; it asks for no workspaces (the person's)
    /// and its output never enters the person's display sizes.
    #[test]
    fn a_space_backend_lists_its_space_and_nothing_of_the_persons_desk() {
        let _one = OUTPUTS.lock().unwrap_or_else(|p| p.into_inner());
        let faked = fake_socket_of(Fake::READY);
        let run = "epilot-test-space-l";
        let before = PIXELS.lock().unwrap().clone();
        let ede = open_as(&faked.session, Some(admitted(run)), Target::Space(1)).unwrap();
        let displays = ede.displays().unwrap();
        assert_eq!(displays.iter().map(|display| (display.name.as_str(), display.width, display.height, display.primary)).collect::<Vec<_>>(), [("space-1", 1280, 800, true)]);
        assert_eq!(ede.list_windows().unwrap().iter().map(|window| (window.id, window.active)).collect::<Vec<_>>(), [(5, true)]);
        assert_eq!(*PIXELS.lock().unwrap(), before);
        assert_eq!(logged(&faked)[2..], ["list_spaces", "create_space 1280x800", "list_outputs", "list_windows"]);
        close_space(&faked.session, run);
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// #EDE-126: `launch_app` is `launch` with the argv and the cwd and no
    /// environment of ePilot's: the compositor decides what a launch gets.
    /// Only a space backend sends it.
    #[test]
    fn a_launch_goes_to_the_compositor_with_no_environment_of_ours() {
        let faked = fake_socket_of(Fake::READY);
        let run = "epilot-test-space-x";
        let ede = open_as(&faked.session, Some(admitted(run)), Target::Space(1)).unwrap();
        assert_eq!(ede.launch(&["foot".to_string()], Some("/tmp")).unwrap(), 77);
        assert_eq!(logged(&faked)[4..], [r#"launch ["foot"] {} Some("/tmp")"#]);
        let person = open_as(&faked.session, Some("epilot-test-person-x"), Target::Person).unwrap();
        assert!(person.launch(&["foot".to_string()], None).is_err());
        assert_eq!(logged(&faked).iter().filter(|line| line.starts_with("launch")).count(), 1);
        close_space(&faked.session, run);
        faked.session.end_run("epilot-test-person-x");
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    fn mints(faked: &Faked) -> usize {
        logged(faked).iter().filter(|line| line.starts_with("mint ")).count()
    }

    /// #EXP-1514: a process that asks once (`exponential doctor`) reads the
    /// compositor's first answer, not the empty cache's older desktop.
    #[test]
    fn a_one_shot_read_waits_for_the_first_answer() {
        let faked = fake_socket_of(Fake::READY);
        assert_eq!(space_offer_settled(&faked.session, Duration::from_secs(5)), SpaceOffer::Offered);
        let featureless = fake_socket_of(Fake { ready: None, ..Fake::READY });
        assert_eq!(space_offer_settled(&featureless.session, Duration::from_secs(5)), SpaceOffer::No(SpaceBlock::OlderDesktop));
        for dir in [&faked.dir, &featureless.dir] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    fn until(what: &str, mut ok: impl FnMut() -> bool) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !ok() {
            assert!(std::time::Instant::now() < deadline, "{what}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn kept(faked: &Faked, run: &str) -> bool {
        lock(&KEPT).get(run).is_some_and(|kept| kept.socket == faked.session.socket)
    }

    fn kept_token(run: &str) -> Token {
        lock(&KEPT).get(run).unwrap().grants.last().unwrap().token.clone()
    }

    fn revokes(faked: &Faked) -> Vec<String> {
        logged(faked).into_iter().filter(|line| line.starts_with("revoke ")).collect()
    }

    fn roots(faked: &Faked) -> usize {
        faked.roots.load(Ordering::SeqCst)
    }

    /// #EXP-1514 (D 5): an idle kept space keeps its grant: the keeper
    /// mints the next one before the last runs out (the compositor closes
    /// a space whose grant expired), after its run's end too. It stops once
    /// the space is closed.
    #[test]
    fn a_kept_space_keeps_its_grant_until_it_is_closed() {
        let faked = fake_socket_of(Fake::READY);
        let run = "epilot-test-keeper-a";
        let short = Timing { ttl: Duration::from_millis(600), renew: Duration::from_millis(300), tick: Duration::from_millis(20), now: now_ms };
        drop(open_space(&faked.session, admitted(run), Target::Space(1), short).unwrap());
        assert!(kept(&faked, run));
        end_space_run(run);
        until("the keeper renewed no grant", || mints(&faked) >= 3);
        assert_eq!(creates(&faked), 1, "a renewal makes no space");
        assert!(revokes(&faked).is_empty(), "the run's end revoked nothing");
        close_space(&faked.session, run);
        assert!(!kept(&faked, run));
        std::thread::sleep(Duration::from_millis(100));
        let closed = mints(&faked);
        std::thread::sleep(Duration::from_millis(700));
        assert_eq!(mints(&faked), closed, "the keeper outlived the space");
        assert!(!lock(&KEEPERS).contains_key(&faked.session.socket), "nothing kept, no keeper");
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// #EXP-2098: a space closed while the keeper's round mints its next
    /// grant stays closed: `close_space` took it out of the kept ones
    /// before it returned, the round does not put it back, and the grant
    /// it minted meanwhile is revoked. (The round read the space as due,
    /// its mint waits at the compositor, the close runs through, then the
    /// mint answers.)
    #[test]
    fn a_space_closed_while_the_keeper_mints_stays_closed() {
        static CLOCK: AtomicI64 = AtomicI64::new(0);
        fn clock() -> i64 {
            CLOCK.load(Ordering::SeqCst)
        }
        CLOCK.store(now_ms(), Ordering::SeqCst);
        let faked = fake_socket_of(Fake::READY);
        let run = "epilot-test-keeper-c";
        let timing = on_clock(clock);
        drop(open_space(&faked.session, admitted(run), Target::Space(1), timing).unwrap());
        assert!(kept(&faked, run) && mints(&faked) == 1);
        // Its grant is due, and the round's mint stops at the compositor.
        CLOCK.fetch_add(SPACE_GRANT_RENEW.as_millis() as i64, Ordering::SeqCst);
        let gate = faked.mint_gate.lock().unwrap();
        let session = faked.session.clone();
        let round = std::thread::spawn(move || session.keeper_round(timing));
        until("the round minted nothing", || mints(&faked) == 2);
        close_space(&faked.session, run);
        assert!(!kept(&faked, run), "close_space returned with the space still kept");
        drop(gate);
        round.join().unwrap();
        assert!(!kept(&faked, run), "the keeper's round kept a closed space again");
        assert!(kept_spaces(&faked.session).is_empty());
        let minted: std::collections::HashSet<String> = faked.grants.lock().unwrap().keys().cloned().collect();
        assert_eq!(*faked.revoked.lock().unwrap(), minted, "both grants of the closed space are revoked");
        assert_eq!(creates(&faked), 1);
        // The next round has nothing to renew.
        CLOCK.fetch_add(SPACE_GRANT_TTL.as_millis() as i64, Ordering::SeqCst);
        faked.session.keeper_round(timing);
        assert_eq!(mints(&faked), 2);
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// #EXP-1514 (C 12, the clock that jumps): ONE round mints every due
    /// grant over ONE root connection: each minute (less than 4 of its 5
    /// left), and at the first tick after a suspend longer than their life.
    #[test]
    fn a_round_mints_every_due_grant_over_one_connection_each_minute_and_after_a_jump() {
        static SKEW: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
        fn skewed() -> i64 {
            now_ms() + SKEW.load(Ordering::SeqCst)
        }
        let faked = fake_socket_of(Fake::READY);
        let timing = Timing { tick: Duration::from_millis(20), now: skewed, ..SPACE_TIMING };
        let runs = ["epilot-test-round-a", "epilot-test-round-b", "epilot-test-round-c"];
        for run in runs {
            drop(open_space(&faked.session, admitted(run), Target::Space(1), timing).unwrap());
        }
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(mints(&faked), 3, "fresh grants are left alone");
        for (round, ahead) in [(1, 61_000), (2, 2 * 60 * 60 * 1000)] {
            let before = roots(&faked);
            SKEW.fetch_add(ahead, Ordering::SeqCst);
            until("no round", || mints(&faked) == 3 + 3 * round);
            std::thread::sleep(Duration::from_millis(150));
            assert_eq!(mints(&faked), 3 + 3 * round, "one renewal each, by the clock");
            assert_eq!(roots(&faked) - before, 1, "round {round}: one root connection for its three grants");
        }
        for run in runs {
            close_space(&faked.session, run);
        }
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// A person run renews inside its calls and takes the hold: nothing kept.
    #[test]
    fn a_person_run_keeps_nothing() {
        let faked = fake_socket_of(Fake::READY);
        let run = "epilot-test-keeper-p";
        drop(open_as(&faked.session, Some(run), Target::Person).unwrap());
        assert!(!kept(&faked, run) && lock(&RUN_GRANTS).contains_key(run));
        faked.session.end_run(run);
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// #EXP-1514 (row 13): a space run's end revokes nothing at the
    /// compositor; a call of the ended run that arrives late makes nothing;
    /// the session's next run drives the SAME space, never told it is new.
    #[test]
    fn a_space_runs_end_sends_no_revoke_and_the_next_run_drives_the_same_space() {
        let faked = fake_socket_of(Fake::READY);
        let run = "epilot-test-life";
        let first = open_as(&faked.session, Some(admitted(run)), Target::Space(1)).unwrap();
        first.mouse_move(1, 1).unwrap();
        drop(first);
        end_space_run(run);
        assert!(revokes(&faked).is_empty(), "{:?}", logged(&faked));
        let late = open_as(&faked.session, Some(run), Target::Space(1)).err().unwrap();
        assert_eq!(late, crate::host::ENDED);
        let next = open_as(&faked.session, Some(admitted(run)), Target::Space(2)).unwrap();
        assert_eq!(next.displays().unwrap()[0].name, "space-1");
        next.mouse_move(2, 2).unwrap();
        assert_eq!((mints(&faked), creates(&faked)), (1, 1), "the same grant, the same space");
        drop(next);
        close_space(&faked.session, run);
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// #EXP-1514: a space run, then a person run of the same session: the
    /// person run never gets the space's grant, and its end revokes its
    /// own; a space hello handed to the person path is refused.
    #[test]
    fn a_space_run_then_a_person_run_of_one_session() {
        let faked = fake_socket_of(Fake::READY);
        let run = "epilot-test-both";
        drop(open_as(&faked.session, Some(admitted(run)), Target::Space(1)).unwrap());
        end_space_run(run);
        let space_token = kept_token(run);
        let person = faked.session.connect_as(Some(run)).unwrap();
        assert_ne!(person.token(), &space_token);
        assert_eq!(person.hello.capabilities, CAPABILITIES.to_vec());
        let person_token = person.token().clone();
        drop(person);
        faked.session.end_run(run);
        assert_eq!(revokes(&faked), [format!("revoke {}", person_token.as_str())], "a person run's end revokes its own");
        // The old shape: a kept `[space]` grant in the person runs' map.
        lock(&RUN_GRANTS).insert(run.to_string(), vec![RunGrant { socket: faked.session.socket.clone(), token: space_token, expires_at: now_ms() + 10 * 60_000 }]);
        let refused = faked.session.connect_as(Some(run)).err().unwrap();
        assert!(refused.contains("agent space's grant"), "{refused}");
        lock(&RUN_GRANTS).remove(run);
        close_space(&faked.session, run);
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// #EXP-1514 (row 14): `close_space` closes while another process's
    /// grant of the session is still live (the process before a restart):
    /// `destroy_space` first, then this host's revokes, never the other's;
    /// a call after it makes nothing until the session's next grant.
    #[test]
    fn close_space_closes_while_another_processs_grant_is_live() {
        let faked = fake_socket_of(Fake::READY);
        let run = "epilot-test-close";
        let mut root = faked.session.connect_token(faked.session.root_token().unwrap()).unwrap();
        let other = root.mint(SessionId::new(run).unwrap(), vec![Capability::Space], Some(now_ms() + 600_000)).unwrap();
        faked.session.connect_token(other.token.clone()).unwrap().create_space(1280, 800).unwrap();
        let ede = open_as(&faked.session, Some(admitted(run)), Target::Space(1)).unwrap();
        assert_eq!(ede.displays().unwrap()[0].name, "space-1", "the space there is driven, not told new");
        drop(ede);
        let ours = kept_token(run);
        close_space(&faked.session, run);
        let tail: Vec<String> = logged(&faked).into_iter().rev().take(2).collect();
        assert_eq!(tail, [format!("revoke {}", ours.as_str()), "destroy_space space-1".to_string()], "destroy first, then our revoke");
        assert!(!revokes(&faked).contains(&format!("revoke {}", other.token.as_str())));
        assert!(root.spaces().unwrap().is_empty(), "closed whatever grant keeps it");
        assert_eq!(open_as(&faked.session, Some(run), Target::Space(1)).err().unwrap(), crate::host::ENDED);
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// #EXP-1514 (row 15): a second process holds at once (one root
    /// connection), then keeps a live session's space, destroys an ended
    /// one's only while the host-wide registry has no live run of it, and
    /// drops an unknown one's hold; a space no registry lists is left
    /// alone. With the switch off at its start: destroyed, nothing held.
    #[test]
    fn adoption_holds_at_once_then_keeps_destroys_or_drops() {
        let faked = fake_socket_of(Fake::READY);
        let mut root = faked.session.connect_token(faked.session.root_token().unwrap()).unwrap();
        let made = |root: &mut Client, run: &str| {
            let grant = root.mint(SessionId::new(run).unwrap(), vec![Capability::Space], Some(now_ms() + 600_000)).unwrap();
            faked.session.connect_token(grant.token).unwrap().create_space(1280, 800).unwrap().id
        };
        let names = ["epilot-adopt-live", "epilot-adopt-ended", "epilot-adopt-ended-busy", "epilot-adopt-unknown"];
        let ids: Vec<_> = names.iter().map(|run| made(&mut root, run)).collect();
        made(&mut root, "epilot-adopt-other");
        let registry: Vec<String> = names.iter().map(|run| run.to_string()).collect();
        let (before, minted) = (roots(&faked), mints(&faked));
        let mut held = hold_spaces(&faked.session, &registry, true, SPACE_TIMING);
        held.sort();
        let mut expected = registry.clone();
        expected.sort();
        assert_eq!(held, expected);
        assert_eq!((roots(&faked) - before, mints(&faked) - minted), (1, 4), "held at once, over one connection");
        assert_eq!(kept_spaces(&faked.session), expected);
        let revoked = revokes(&faked).len();
        settle_spaces(
            &faked.session,
            |run| match run {
                "epilot-adopt-live" => crate::SessionHere::Live,
                "epilot-adopt-ended" | "epilot-adopt-ended-busy" => crate::SessionHere::Ended,
                _ => crate::SessionHere::Unknown,
            },
            |run| run == "epilot-adopt-ended-busy",
        );
        assert_eq!(kept_spaces(&faked.session), ["epilot-adopt-live"]);
        let destroyed: Vec<String> = logged(&faked).into_iter().filter(|line| line.starts_with("destroy_space")).collect();
        assert_eq!(destroyed, [format!("destroy_space {}", ids[1].as_str())], "only the ended one without a live run");
        assert_eq!(revokes(&faked).len() - revoked, 3, "the ended one's grant after its destroy, two dropped holds");
        let left: Vec<String> = root.spaces().unwrap().iter().map(|space| space.session_id.as_str().to_string()).collect();
        assert_eq!(left.len(), 4, "{left:?}");
        // The live one's next run drives its space as it is.
        let live = "epilot-adopt-live";
        let next = open_as(&faked.session, Some({ admit(live, true); live }), Target::Space(1)).unwrap();
        assert_eq!(next.displays().unwrap()[0].name, ids[0].as_str());
        drop(next);
        close_space(&faked.session, live);

        // The switch off at the host's start.
        let off = fake_socket_of(Fake::READY);
        let mut root = off.session.connect_token(off.session.root_token().unwrap()).unwrap();
        let made = |root: &mut Client, run: &str| {
            let grant = root.mint(SessionId::new(run).unwrap(), vec![Capability::Space], Some(now_ms() + 600_000)).unwrap();
            off.session.connect_token(grant.token).unwrap().create_space(1280, 800).unwrap().id
        };
        let ours = made(&mut root, "epilot-adopt-off");
        made(&mut root, "epilot-adopt-off-other");
        assert!(hold_spaces(&off.session, &["epilot-adopt-off".to_string()], false, SPACE_TIMING).is_empty());
        assert!(!kept(&off, "epilot-adopt-off"));
        assert_eq!(logged(&off).into_iter().filter(|line| line.starts_with("destroy_space")).collect::<Vec<_>>(), [format!("destroy_space {}", ours.as_str())]);
        assert_eq!(root.spaces().unwrap().len(), 1, "another host's space is left");
        for dir in [&faked.dir, &off.dir] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    /// #EXP-1514 (row 9, D 5-9): "new and empty" only when it is true: a
    /// session that never had a space gets its first one silently; one
    /// whose run record says it had one, and whose space is gone, is told
    /// once and drives the new one after. The host hears a first space once.
    #[test]
    fn had_space_decides_new_and_empty() {
        static HEARD: Mutex<Vec<String>> = Mutex::new(Vec::new());
        crate::backend::set_space_made(std::sync::Arc::new(|run: &str| {
            if run.starts_with("epilot-test-had") {
                HEARD.lock().unwrap().push(run.to_string());
            }
        }));
        let faked = fake_socket_of(Fake::READY);
        let fresh = "epilot-test-had-not";
        admit(fresh, false);
        assert!(open_as(&faked.session, Some(fresh), Target::Space(1)).is_ok(), "a first space is made silently");
        let had = "epilot-test-had";
        admit(had, true);
        assert_eq!(open_as(&faked.session, Some(had), Target::Space(1)).err().unwrap(), SPACE_ENDED);
        assert!(open_as(&faked.session, Some(had), Target::Space(1)).is_ok(), "told once");
        assert_eq!(creates(&faked), 2);
        assert_eq!(*HEARD.lock().unwrap(), [fresh.to_string()], "a session that had one is no news");
        for run in [fresh, had] {
            close_space(&faked.session, run);
        }
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// #EXP-1514 (D 10): `space_busy` goes out each round for a kept
    /// session whose turn runs, on its own space grant, and stops after
    /// `turn_ended`; a kept session with no turn never says it.
    #[test]
    fn space_busy_goes_out_each_round_while_the_turn_runs_and_stops_after_turn_ended() {
        let faked = fake_socket_of(Fake::READY);
        let timing = Timing { tick: Duration::from_millis(20), ..SPACE_TIMING };
        let (run, idle) = ("epilot-test-busy", "epilot-test-busy-idle");
        for one in [run, idle] {
            drop(open_space(&faked.session, admitted(one), Target::Space(1), timing).unwrap());
        }
        let said = |who: &str| logged(&faked).iter().filter(|line| **line == format!("space_busy {who}")).count();
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(said(run), 0, "no turn runs");
        crate::backend::turn(run, true);
        until("no space_busy while the turn runs", || said(run) >= 3);
        crate::backend::turn(run, false);
        std::thread::sleep(Duration::from_millis(60));
        let after = said(run);
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(said(run), after, "none after turn_ended");
        assert_eq!(said(idle), 0);
        for one in [run, idle] {
            close_space(&faked.session, one);
        }
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// #EXP-1514: an end during an arriving call (it passed the host's
    /// check just before the run's end) leaves no grant and no space.
    #[test]
    fn an_end_during_an_arriving_call_leaves_no_grant_and_no_space() {
        let faked = fake_socket_of(Fake::READY);
        let run = "epilot-test-race";
        admit(run, false);
        end_space_run(run);
        assert_eq!(open_as(&faked.session, Some(run), Target::Space(1)).err().unwrap(), crate::host::ENDED);
        assert_eq!((mints(&faked), creates(&faked)), (0, 0));
        assert!(!kept(&faked, run));
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// #EDE-193: a space run's own call mints a kept grant of the keeper's
    /// life (5 minutes), never a person run's 15: a space grant keeps its
    /// space at the compositor for as long as it lives, so a 15-minute one
    /// kept a dead host's space up to 15 min 30 s. A person run still mints
    /// its own 15.
    #[test]
    fn a_space_runs_call_mints_the_keepers_five_minutes_and_a_person_run_its_fifteen() {
        let faked = fake_socket_of(Fake::READY);
        let (space, person) = ("epilot-test-life-space", "epilot-test-life-person");
        drop(open_as(&faked.session, Some(admitted(space)), Target::Space(1)).unwrap());
        drop(open_as(&faked.session, Some(person), Target::Person).unwrap());
        let lives = |run: &str, ttl: Duration| {
            let lives = faked.lives(run);
            assert!(lives.len() == 1 && (lives[0].1 - ttl.as_millis() as i64).abs() < 2_000, "{run}: {lives:?}");
        };
        lives(space, SPACE_GRANT_TTL);
        lives(person, RUN_GRANT_TTL);
        close_space(&faked.session, space);
        faked.session.end_run(person);
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// #EDE-193: the keeper's timing on a fake clock. Its own thread sleeps
    /// an hour: the test runs its rounds ([`Life::second`]).
    fn on_clock(now: fn() -> i64) -> Timing {
        Timing { now, tick: Duration::from_secs(60 * 60), ..SPACE_TIMING }
    }

    /// #EDE-193: a host and its compositor on a fake clock. Each
    /// [`Life::second`] moves the clock a second and ticks the compositor
    /// ([`Faked::tick`]); while the host lives, its keeper runs a round
    /// every [`KEEPER_TICK`], as its thread does.
    struct Life<'a> {
        faked: &'a Faked,
        clock: &'static AtomicI64,
        timing: Timing,
        /// The host process runs (its keeper renews).
        host: bool,
        seconds: u64,
        /// Each space the compositor closed, and when by the clock.
        closed: Vec<(i64, String)>,
    }

    impl<'a> Life<'a> {
        fn new(faked: &'a Faked, clock: &'static AtomicI64, now: fn() -> i64) -> Self {
            clock.store(now_ms(), Ordering::SeqCst);
            Life { faked, clock, timing: on_clock(now), host: true, seconds: 0, closed: Vec::new() }
        }

        fn now(&self) -> i64 {
            (self.timing.now)()
        }

        fn second(&mut self) {
            self.clock.fetch_add(1000, Ordering::SeqCst);
            self.seconds += 1;
            if self.host && self.seconds % KEEPER_TICK.as_secs() == 0 {
                self.faked.session.keeper_round(self.timing);
            }
            let now = self.now();
            self.closed.extend(self.faked.tick().into_iter().map(|id| (now, id)));
        }

        fn seconds(&mut self, n: u64) {
            for _ in 0..n {
                self.second();
            }
        }

        /// One call of `run` as its host makes it: its backend, a pointer move.
        fn call(&self, run: &str) -> Result<Box<dyn Backend>, String> {
            let ede = open_space(&self.faked.session, run, Target::Space(1), self.timing)?;
            ede.mouse_move(5, 5)?;
            Ok(ede)
        }

        /// The host dies (`kill -9`): what it held goes with the process,
        /// nothing is revoked or destroyed, and no keeper renews.
        fn kill(&mut self, run: &str) {
            self.host = false;
            lock(&KEEPERS).remove(&self.faked.session.socket);
            lock(&KEPT).remove(run);
            lock(&ADMITTED).remove(run);
            lock(&HAD).remove(run);
            lock(&TURNS).remove(run);
        }

        /// It stays away until the compositor closed a space.
        fn away_until_closed(&mut self) -> (i64, String) {
            let since = self.now();
            while self.closed.is_empty() {
                assert!(self.now() < since + 20 * 60_000, "the space outlived its host by 20 minutes");
                self.second();
            }
            self.closed.remove(0)
        }

        fn refused(&self) -> Vec<String> {
            logged(self.faked).into_iter().filter(|line| line.starts_with("refused")).collect()
        }
    }

    const TTL_MS: i64 = SPACE_GRANT_TTL.as_millis() as i64;
    const GRACE_MS: i64 = GRACE_TICKS as i64 * 1000;

    /// #EDE-193 (S9): the host dies (`kill -9`) right after its run's last
    /// call. Every grant of the space lived the keeper's 5 minutes, the
    /// first call's own too, so the compositor closes the space within 5
    /// minutes and its grace of the host's last renewal (at most 5 min 30 s
    /// after the kill), and not while that grant lived.
    #[test]
    fn a_killed_hosts_space_closes_within_five_minutes_and_the_grace_of_its_last_renewal() {
        static NOW: AtomicI64 = AtomicI64::new(0);
        fn now() -> i64 {
            NOW.load(Ordering::SeqCst)
        }
        let faked = fake_socket_of(Fake { clock: Some(now), ..Fake::READY });
        let mut life = Life::new(&faked, &NOW, now);
        let run = "epilot-test-killed";
        drop(life.call(admitted(run)).unwrap());
        // It calls every 20 s for 7 minutes while the keeper renews, and
        // once more right before the kill.
        for second in 1..=7 * 60 + 13 {
            life.second();
            if second % 20 == 0 {
                drop(life.call(run).unwrap());
            }
        }
        drop(life.call(run).unwrap());
        let lives = faked.lives(run);
        assert!(lives.iter().all(|(_, life)| *life == TTL_MS), "every grant of the space lives 5 minutes: {lives:?}");
        assert_eq!(lives.len(), 8, "the first call's, then one each minute: {lives:?}");
        let (last, killed) = (lives.last().unwrap().0, life.now());
        life.kill(run);
        let (closed, space) = life.away_until_closed();
        assert_eq!(space, "space-1");
        assert!(closed >= last + TTL_MS, "closed while its last grant lived");
        assert!(closed <= last + TTL_MS + GRACE_MS, "closed {} s after the host's last renewal", (closed - last) / 1000);
        assert!(closed - killed <= TTL_MS + GRACE_MS, "closed {} s after the kill", (closed - killed) / 1000);
        assert_eq!(life.refused(), Vec::<String>::new());
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// #EDE-193: a live run is never cut off between renewals. For half an
    /// hour it calls every 7 s, one call holding its connection for 90 s
    /// across a renewal (the grant before is left to run out, never
    /// revoked under it), while the keeper renews each minute; then it
    /// idles 10 minutes on the keeper alone. Every call answers on the same
    /// space and the compositor refuses nothing.
    #[test]
    fn a_live_run_is_never_cut_off_between_renewals() {
        static NOW: AtomicI64 = AtomicI64::new(0);
        fn now() -> i64 {
            NOW.load(Ordering::SeqCst)
        }
        let faked = fake_socket_of(Fake { clock: Some(now), ..Fake::READY });
        let mut life = Life::new(&faked, &NOW, now);
        let run = "epilot-test-alive";
        drop(life.call(admitted(run)).unwrap());
        let mut held: Option<Box<dyn Backend>> = None;
        for second in 1..=30 * 60 {
            life.second();
            if second % 7 == 0 {
                drop(life.call(run).unwrap_or_else(|err| panic!("cut off at {second} s: {err}")));
            }
            match second {
                150 => held = Some(life.call(run).unwrap()),
                240 => held = None,
                _ => {}
            }
            if let Some(ede) = &held {
                ede.mouse_move(1, 1).unwrap_or_else(|err| panic!("the held call was cut off at {second} s: {err}"));
            }
        }
        life.seconds(10 * 60);
        let ede = life.call(run).unwrap();
        assert_eq!(ede.displays().unwrap()[0].name, "space-1", "the same space, never new and empty");
        drop(ede);
        assert_eq!((life.closed.clone(), creates(&faked)), (Vec::new(), 1));
        assert_eq!(life.refused(), Vec::<String>::new());
        assert!(revokes(&faked).is_empty(), "a renewal revokes nothing: {:?}", revokes(&faked));
        let lives = faked.lives(run);
        assert!(lives.iter().all(|(_, life)| *life == TTL_MS), "{lives:?}");
        assert_eq!(lives.len(), 1 + 40, "the first call's, then one each minute for 40 minutes");
        close_space(&faked.session, run);
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// #EDE-193 (row 15, S9): "new and empty" is still said once after a
    /// restart, and only when it is true. A host back inside its grants'
    /// life holds the space and drives it as it is; one away past 5 minutes
    /// and the grace finds it closed: the session's first call says so
    /// once, and the next ones drive the new space.
    #[test]
    fn new_and_empty_is_said_once_after_a_restart() {
        static NOW: AtomicI64 = AtomicI64::new(0);
        fn now() -> i64 {
            NOW.load(Ordering::SeqCst)
        }
        let faked = fake_socket_of(Fake { clock: Some(now), ..Fake::READY });
        let mut life = Life::new(&faked, &NOW, now);
        let run = "epilot-test-restart";
        let registry = [run.to_string()];
        drop(life.call(admitted(run)).unwrap());
        life.seconds(3 * 60);
        // Killed, and back 2 minutes later: held at once, driven as it is.
        life.kill(run);
        life.seconds(2 * 60);
        life.host = true;
        assert_eq!(hold_spaces(&faked.session, &registry, true, life.timing), registry);
        admit(run, true);
        let ede = life.call(run).unwrap();
        assert_eq!(ede.displays().unwrap()[0].name, "space-1", "a restart inside the grants' life keeps the space");
        drop(ede);
        life.seconds(3 * 60);
        // Killed again, and away until the space closed.
        life.kill(run);
        assert_eq!(life.away_until_closed().1, "space-1");
        // Back: nothing to hold; its record says the session had a space.
        life.host = true;
        assert!(hold_spaces(&faked.session, &registry, true, life.timing).is_empty());
        admit(run, true);
        assert_eq!(life.call(run).err().as_deref(), Some(SPACE_ENDED));
        for _ in 0..3 {
            let ede = life.call(run).unwrap();
            assert_eq!(ede.displays().unwrap()[0].name, "space-2", "told once, then the new space");
            drop(ede);
            life.seconds(90);
        }
        assert_eq!(creates(&faked), 2);
        assert_eq!(life.refused(), Vec::<String>::new());
        close_space(&faked.session, run);
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// #EXP-1514: the gate's answer for `faked`, asked now.
    fn offer_of(faked: &Faked) -> SpaceOffer {
        ask(&faked.session, GATE_WITHIN).offer
    }

    fn creates(faked: &Faked) -> usize {
        logged(faked).iter().filter(|line| line.starts_with("create_space")).count()
    }

    /// #EXP-1514: each `SpaceBlock` and its cause, and for each a space run
    /// that makes NO space: nothing minted, nothing created, the block's
    /// words in the call's answer.
    #[test]
    fn each_block_and_its_cause() {
        use crate::backend::space_offer_of;
        // Not eDE, a local fact; an eDE session that cannot be read.
        assert_eq!(space_offer_of(None), SpaceOffer::No(SpaceBlock::NotDesktop));
        assert_eq!(space_offer_of(Some(Err("no XDG_RUNTIME_DIR".into()))), SpaceOffer::No(SpaceBlock::Unavailable("no XDG_RUNTIME_DIR".into())));
        let unavailable = |cause: &str| SpaceOffer::No(SpaceBlock::Unavailable(cause.to_string()));
        let cases = [
            // eDE 0.2.29: `spaces`, no `space-readiness`.
            (Fake { ready: None, ..Fake::READY }, SpaceOffer::No(SpaceBlock::OlderDesktop), "too old"),
            // Says it and cannot read the op.
            (Fake { ready: Some(Ready::Unsupported), ..Fake::READY }, SpaceOffer::No(SpaceBlock::OlderDesktop), "too old"),
            // Older still: no spaces at all.
            (Fake { spaces: false, ready: None, ..Fake::READY }, SpaceOffer::No(SpaceBlock::OlderDesktop), "too old"),
            // A root that cannot mint: today such a run read as offered and every call failed.
            (Fake { root_system: false, ..Fake::READY }, unavailable(NO_MINT), "cannot mint a space grant"),
            (Fake { ready: Some(Ready::Block("no_user_manager")), ..Fake::READY }, unavailable("no systemd user manager"), "no systemd user manager"),
            (Fake { ready: Some(Ready::Block("session")), ..Fake::READY }, unavailable("an agent space's session did not start"), "session did not start"),
            // #EDE-146..148: the guard or polkit.
            (Fake { ready: Some(Ready::Block("guard")), ..Fake::READY }, unavailable("the space guard or polkit does not guard agent spaces"), "(the space guard or polkit"),
            // A block this build does not know is a no all the same.
            (Fake { ready: Some(Ready::Block("later")), ..Fake::READY }, unavailable("later"), "(later)"),
            (Fake { ready: Some(Ready::Block("driver")), ..Fake::READY }, SpaceOffer::No(SpaceBlock::Driver), "graphics driver"),
        ];
        for (n, (fake, offer, words)) in cases.into_iter().enumerate() {
            let faked = fake_socket_of(fake);
            assert_eq!(offer_of(&faked), offer, "case {n}");
            let run = format!("epilot-test-block-{n}");
            let refused = open_as(&faked.session, Some(admitted(&run)), Target::Space(1)).err().unwrap();
            assert!(refused.starts_with("This desktop offers no agent space: ") && refused.contains(words), "case {n}: {refused}");
            assert_eq!((mints(&faked), creates(&faked)), (0, 0), "case {n}: {:?}", logged(&faked));
            assert!(!RUN_GRANTS.lock().unwrap().contains_key(&run) && !kept(&faked, &run), "case {n}");
            let _ = std::fs::remove_dir_all(&faked.dir);
        }
        // Ready: the space is made.
        let faked = fake_socket_of(Fake::READY);
        assert_eq!(offer_of(&faked), SpaceOffer::Offered);
        let run = "epilot-test-block-ready";
        drop(open_as(&faked.session, Some(admitted(run)), Target::Space(1)).unwrap());
        assert_eq!((mints(&faked), creates(&faked)), (1, 1));
        close_space(&faked.session, run);
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// #EXP-1514: a 0.2.29-style hello (`spaces` alone) makes NO space,
    /// also for a run admitted while the desktop did not answer (the
    /// launch's `Unknown`), and a `ready` heard longer ago than the gate
    /// trusts is asked again.
    #[test]
    fn a_0229_hello_makes_no_space_also_for_a_run_admitted_on_unknown() {
        let faked = fake_socket_of(Fake { ready: None, ..Fake::READY });
        let run = "epilot-test-0229";
        let older = || open_as(&faked.session, Some(admitted(run)), Target::Space(1)).err().unwrap();
        assert!(older().contains("too old"), "{}", older());
        // The launch found the desktop silent: the run was admitted on Unknown.
        remember(&faked.session.socket, Said { at: Instant::now(), offer: SpaceOffer::Unknown, max: None });
        assert!(older().contains("too old"));
        // A `ready` from before the gate's trust ran out (the desktop was
        // replaced by an older one since).
        let long_ago = Instant::now().checked_sub(READY_FOR + Duration::from_secs(1)).unwrap();
        remember(&faked.session.socket, Said { at: long_ago, offer: SpaceOffer::Offered, max: Some(4) });
        assert!(older().contains("too old"));
        assert_eq!(logged(&faked), Vec::<String>::new(), "nothing minted, created or asked of it");
        // The same run on a desktop that says ready: its first call makes the space.
        let ready = fake_socket_of(Fake::READY);
        remember(&ready.session.socket, Said { at: Instant::now(), offer: SpaceOffer::Unknown, max: None });
        drop(open_as(&ready.session, Some(admitted(run)), Target::Space(1)).unwrap());
        assert_eq!(logged(&ready), ["space_readiness".to_string(), format!("mint {run} true"), "list_spaces".to_string(), "create_space 1280x800".to_string()]);
        close_space(&ready.session, run);
        for dir in [&faked.dir, &ready.dir] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    /// #EXP-1514: no socket, a compositor that never answers its hello and
    /// one that never answers the question are all `Unknown`, inside the
    /// deadline; a space run's call says the desktop did not answer.
    #[test]
    fn no_socket_and_a_silent_compositor_are_unknown_inside_the_deadline() {
        let dir = std::env::temp_dir().join(format!("epilot-silent-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("token"), "f".repeat(64)).unwrap();
        let at = |socket: PathBuf| Session { socket, token_file: dir.join("token"), token: None };
        let within = Duration::from_millis(300);
        // No socket.
        let gone = at(dir.join("gone.sock"));
        let started = Instant::now();
        assert_eq!(ask(&gone, within).offer, SpaceOffer::Unknown);
        assert!(started.elapsed() < within);
        // A socket nobody accepts on (a stopped compositor): the hello goes unanswered.
        let _stopped = std::os::unix::net::UnixListener::bind(dir.join("stopped.sock")).unwrap();
        let stopped = at(dir.join("stopped.sock"));
        let started = Instant::now();
        assert_eq!(ask(&stopped, within).offer, SpaceOffer::Unknown);
        assert!(started.elapsed() < within * 3, "{:?}", started.elapsed());
        // The hello answered, the question never.
        let silent = fake_socket_of(Fake { ready: Some(Ready::Silent), ..Fake::READY });
        let started = Instant::now();
        assert_eq!(ask(&silent.session, within).offer, SpaceOffer::Unknown);
        assert!(started.elapsed() < within * 3, "{:?}", started.elapsed());
        let refused = open_as(&gone, Some(admitted("epilot-test-silent")), Target::Space(1)).err().unwrap();
        assert!(refused.contains("did not answer"), "{refused}");
        assert_eq!(creates(&silent), 0);
        for dir in [&dir, &silent.dir] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    /// #EXP-1514: the offer is read from the cache and never waits on the
    /// desktop: a silent one answers at once, every time (`Unknown`); a
    /// ready one's answer arrives from the refresh's own thread.
    #[test]
    fn the_offer_is_read_from_a_cache_refreshed_on_its_own_thread() {
        let silent = fake_socket_of(Fake { ready: Some(Ready::Silent), ..Fake::READY });
        for _ in 0..3 {
            let started = Instant::now();
            assert_eq!(space_offer(&silent.session), SpaceOffer::Unknown);
            assert!(started.elapsed() < Duration::from_millis(100), "the read waited {:?}", started.elapsed());
        }
        let ready = fake_socket_of(Fake::READY);
        until("the refresh never answered", || space_offer(&ready.session) == SpaceOffer::Offered);
        let older = fake_socket_of(Fake { ready: None, ..Fake::READY });
        until("the refresh never answered", || space_offer(&older.session) == SpaceOffer::No(SpaceBlock::OlderDesktop));
        assert_eq!(logged(&ready).iter().filter(|line| *line == "space_readiness").count(), 1, "one question while the answer is fresh");
        for dir in [&silent.dir, &ready.dir, &older.dir] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    /// #EXP-2065: no answer is never an older desktop. The empty cache, no
    /// socket (eDE restarting), a hello nobody answers (eDE's consent
    /// question open, a stopped or busy compositor) all read `Unknown`, so
    /// the doctor says Not responding, never Update Exponential Desktop.
    /// Only a hello without `space-readiness` (eDE 0.2.29) is older.
    #[test]
    fn no_answer_is_never_an_older_desktop() {
        let older = SpaceOffer::No(SpaceBlock::OlderDesktop);
        let answered = |session: &Session| {
            READINESS.lock().unwrap().get(&session.socket).is_some_and(|(said, asking)| said.is_some() && !*asking)
        };
        // An eDE before the flip: its own answer says older.
        let featureless = fake_socket_of(Fake { ready: None, ..Fake::READY });
        assert_eq!(space_offer(&featureless.session), SpaceOffer::Unknown, "the empty cache");
        until("the refresh never answered", || answered(&featureless.session));
        assert_eq!(space_offer(&featureless.session), older);
        assert_eq!(launch_offer(&featureless.session), older, "a launch keeps the screen on an older eDE");
        // No socket and one nobody answers on.
        let dir = std::env::temp_dir().join(format!("epilot-dark-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("token"), "f".repeat(64)).unwrap();
        let _stopped = std::os::unix::net::UnixListener::bind(dir.join("stopped.sock")).unwrap();
        for socket in ["gone.sock", "stopped.sock"] {
            let session = Session { socket: dir.join(socket), token_file: dir.join("token"), token: None };
            assert_eq!(space_offer(&session), SpaceOffer::Unknown, "{socket}: the empty cache");
            until("the refresh never gave up", || answered(&session));
            assert_eq!(space_offer(&session), SpaceOffer::Unknown, "{socket}: a timeout is no answer");
        }
        // A compositor of the flip that falls silent.
        let silent = fake_socket_of(Fake { ready: Some(Ready::Silent), ..Fake::READY });
        assert_eq!(space_offer(&silent.session), SpaceOffer::Unknown);
        assert_eq!(ask(&silent.session, Duration::from_millis(200)).offer, SpaceOffer::Unknown);
        for dir in [&featureless.dir, &dir, &silent.dir] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    /// #EXP-2065: a launch never places its run by a cached timeout: it asks
    /// the desktop now, so the first launch after the person allowed eDE's
    /// consent question gets its space. A fresh real answer is read without
    /// asking; a timeout the launch itself hits is not remembered.
    #[test]
    fn a_launch_asks_again_over_a_cached_timeout() {
        let questions = |faked: &Faked| logged(faked).iter().filter(|line| *line == "space_readiness").count();
        // The consent question was open at the beat: its timeout is cached.
        let ready = fake_socket_of(Fake::READY);
        let at = Instant::now();
        *READINESS.lock().unwrap().entry(ready.session.socket.clone()).or_default() = (Some(Said { at, offer: SpaceOffer::Unknown, max: None }), false);
        assert_eq!(space_offer(&ready.session), SpaceOffer::Unknown, "the beat reads the timeout");
        // Allowed: the launch asks now and gets the space.
        assert_eq!(launch_offer(&ready.session), SpaceOffer::Offered);
        let asked = questions(&ready);
        assert!(asked >= 1, "the launch asked");
        // Its answer is remembered: the next launch reads it.
        assert_eq!(launch_offer(&ready.session), SpaceOffer::Offered);
        assert_eq!(questions(&ready), asked, "a fresh real answer is not asked again");
        assert_eq!(space_offer(&ready.session), SpaceOffer::Offered);
        // A stale real answer is asked again.
        *READINESS.lock().unwrap().entry(ready.session.socket.clone()).or_default() = (Some(Said { at: at - OFFER_REFRESH, offer: SpaceOffer::No(SpaceBlock::OlderDesktop), max: None }), true);
        assert_eq!(launch_offer(&ready.session), SpaceOffer::Offered, "a stale answer is not the launch's");
        // A desktop that does not answer the launch: Unknown (a space run
        // that asks again at its first call), and nothing is cached.
        let dir = std::env::temp_dir().join(format!("epilot-consent-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("token"), "f".repeat(64)).unwrap();
        let _held = std::os::unix::net::UnixListener::bind(dir.join("held.sock")).unwrap();
        let held = Session { socket: dir.join("held.sock"), token_file: dir.join("token"), token: None };
        assert_eq!(launch_offer(&held), SpaceOffer::Unknown);
        assert!(READINESS.lock().unwrap().get(&held.socket).is_none_or(|(said, _)| said.is_none()), "a timeout is never cached by the launch");
        for dir in [&ready.dir, &dir] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    /// #EXP-1514 (rows 7 and 8): a refused `create_space` by its code: a
    /// full desktop names its count, one out of memory says so, one that
    /// could not make a space says why; a launch the space refuses says it
    /// could not start the program.
    #[test]
    fn the_counts_the_memorys_and_the_unavailable_texts() {
        let refusal = |fake: Fake| {
            let faked = fake_socket_of(fake);
            let told = open_as(&faked.session, Some(admitted("epilot-test-full")), Target::Space(1)).err().unwrap();
            assert_eq!(creates(&faked), 1);
            assert!(!kept(&faked, "epilot-test-full"), "a run without a space has nothing to keep");
            close_space(&faked.session, "epilot-test-full");
            let _ = std::fs::remove_dir_all(&faked.dir);
            told
        };
        let full = Fake { create: Some((ErrorCode::Limit, "4 spaces at most")), others: 4, ..Fake::READY };
        assert_eq!(refusal(full), "Every agent space on this desktop is in use (4 at most). The person can close one in the taskbar's Agent spaces list.");
        let memory = Fake { create: Some((ErrorCode::Limit, "no memory for another space")), others: 2, ..Fake::READY };
        assert_eq!(refusal(memory), SPACES_OUT_OF_MEMORY);
        let broken = Fake { create: Some((ErrorCode::Unavailable, "no runtime directory for spaces")), ..Fake::READY };
        assert_eq!(refusal(broken), "This desktop could not make an agent space: no runtime directory for spaces.");
        for (n, (code, words)) in [(ErrorCode::Limit, "32 launched processes in this space at most"), (ErrorCode::Unavailable, "the systemd user manager is gone")].into_iter().enumerate() {
            let faked = fake_socket_of(Fake { launch: Some((code, words)), ..Fake::READY });
            let run = &format!("epilot-test-launch-refused-{n}");
            let ede = open_as(&faked.session, Some(admitted(run)), Target::Space(1)).unwrap();
            assert_eq!(ede.launch(&["firefox".to_string()], None).unwrap_err(), format!("The agent space could not start firefox: {words}."));
            drop(ede);
            close_space(&faked.session, run);
            let _ = std::fs::remove_dir_all(&faked.dir);
        }
    }

    /// #EXP-1994: a compositor that forgot the run's grant (a restart)
    /// refuses it `unauthorized`, in whatever words: a new grant is minted
    /// and the call goes on.
    #[test]
    fn a_forgotten_grant_is_minted_again_by_its_code_not_its_words() {
        let faked = fake_socket_of(Fake { spaces: false, ready: None, ..Fake::READY });
        let run = "epilot-test-run-forgotten";
        let first = faked.session.connect_as(Some(run)).unwrap().token().clone();
        assert_eq!(faked.session.connect_as(Some(run)).unwrap().token(), &first, "kept while the compositor knows it");
        faked.forget.store(true, Ordering::SeqCst);
        let refused = faked.session.hello(first.clone()).err().expect("forgotten");
        assert!(forgotten(&refused) && !refused.to_string().contains("unknown or expired token"), "{refused}");
        let again = faked.session.connect_as(Some(run)).unwrap().token().clone();
        assert_ne!(again, first, "minted again");
        assert_eq!(logged(&faked).iter().filter(|line| line.starts_with("mint ")).count(), 2);
        faked.session.end_run(run);
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// #EXP-1925: two runs mint two grants (each its own, with an expiry;
    /// a run's next call reuses its grant), and a run's end revokes
    /// exactly its own.
    #[test]
    fn each_run_mints_its_own_grant_and_its_end_revokes_only_that() {
        let (session, log, dir) = fake_socket();
        let (a, b) = ("epilot-test-run-a", "epilot-test-run-b");
        let token_a = session.connect_as(Some(a)).unwrap().token().clone();
        let token_b = session.connect_as(Some(b)).unwrap().token().clone();
        assert_ne!(token_a, token_b);
        assert_eq!(session.connect_as(Some(a)).unwrap().token(), &token_a, "a run's next call mints anew");
        assert_eq!(log.lock().unwrap().clone(), [format!("mint {a} true"), format!("mint {b} true")]);
        session.end_run(a);
        assert_eq!(log.lock().unwrap()[2..], [format!("revoke {}", token_a.as_str())]);
        assert!(RUN_GRANTS.lock().unwrap().contains_key(b) && !RUN_GRANTS.lock().unwrap().contains_key(a));
        session.end_run(b);
        assert_eq!(log.lock().unwrap()[3..], [format!("revoke {}", token_b.as_str())]);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// paste_text's round on eDE: save = snapshot, set = every text type
    /// (+ the hint when sensitive), wait = poll `clipboard_taken`, restore
    /// = set the saved types back (sensitive again when they carry the
    /// hint), text = `clipboard_text`.
    #[test]
    fn the_clipboard_is_the_compositors() {
        use serde_json::json;
        let (ede, server) = fake_compositor(vec![
            Ok(json!({"serial": 4})),
            Ok(json!({"serial": 4, "items": [{"mime": "image/png", "data": "AAE="}, {"mime": CLIPBOARD_SECRET_HINT, "data": "c2VjcmV0"}], "unread": []})),
            Ok(json!({"serial": 5})),
            Ok(json!({"serial": 5, "taken": false})),
            Ok(json!({"serial": 5, "taken": true})),
            Ok(json!({"serial": 6})),
            Ok(json!({"serial": 6, "text": "copied"})),
            Ok(json!({"serial": 7})),
            Ok(json!({"serial": 8, "taken": false})),
        ]);
        let clipboard = ede.clipboard().unwrap();
        assert_eq!(clipboard.change_count().unwrap(), 4);
        let saved = clipboard.save().unwrap();
        assert_eq!(saved.formats, vec![("image/png".to_string(), vec![0, 1]), (CLIPBOARD_SECRET_HINT.to_string(), b"secret".to_vec())]);
        assert!(!saved.partial);
        clipboard.set_text("pw", true).unwrap();
        assert_eq!(clipboard.wait_pasted(Duration::from_secs(2)), Pasted::Taken);
        clipboard.restore(saved).unwrap();
        assert_eq!(clipboard.text().unwrap().as_deref(), Some("copied"));
        clipboard.set_text("again", false).unwrap();
        // Someone else's selection (serial 8) before ours was read.
        assert_eq!(clipboard.wait_pasted(Duration::from_secs(2)), Pasted::Missed);
        drop(clipboard);
        drop(ede);
        let seen = server.join().unwrap();
        let ops: Vec<&str> = seen.iter().map(|op| op["op"].as_str().unwrap()).collect();
        assert_eq!(ops, ["clipboard_serial", "clipboard_snapshot", "clipboard_set", "clipboard_taken", "clipboard_taken", "clipboard_set", "clipboard_text", "clipboard_set", "clipboard_taken"]);
        let mimes = |set: &serde_json::Value| set["items"].as_array().unwrap().iter().map(|item| item["mime"].as_str().unwrap().to_string()).collect::<Vec<_>>();
        assert_eq!(mimes(&seen[2]), ["text/plain;charset=utf-8", "text/plain", "UTF8_STRING", "TEXT", "STRING", CLIPBOARD_SECRET_HINT]);
        assert_eq!(seen[2]["items"][0]["data"], "cHc=");
        assert_eq!(seen[2]["sensitive"], true);
        assert_eq!(seen[3]["serial"], 5);
        assert_eq!(mimes(&seen[5]), ["image/png", CLIPBOARD_SECRET_HINT]);
        assert_eq!(seen[5]["sensitive"], true, "a restored secret stays one");
        assert_eq!(mimes(&seen[7]).len(), 5);
        assert_eq!(seen[7].get("sensitive"), None);
    }

    /// #EXP-1994: an eDE without the clipboard ops answers without the
    /// call's id, in whatever words.
    #[test]
    fn an_old_ede_says_to_update() {
        let (ede, _server) = fake_compositor(vec![Err("this op means nothing to me")]);
        let err = ede.clipboard().unwrap().change_count().unwrap_err();
        assert_eq!(err, "This eDE is too old for the clipboard; update eDE.");
    }

    /// #EXP-1900: a snapshot that names a type it left out is a partial
    /// save, and so is one from an eDE that cannot say (no `unread`).
    #[test]
    fn a_snapshot_with_unread_types_or_from_an_old_ede_is_a_partial_save() {
        use serde_json::json;
        let (ede, server) = fake_compositor(vec![
            Ok(json!({"serial": 4, "items": [{"mime": "text/plain", "data": "YQ=="}], "unread": ["image/png"]})),
            Ok(json!({"serial": 4, "items": [{"mime": "text/plain", "data": "YQ=="}]})),
            Ok(json!({"serial": 4, "items": [], "unread": []})),
        ]);
        let clipboard = ede.clipboard().unwrap();
        let named = clipboard.save().unwrap();
        assert!(named.partial);
        assert_eq!(named.formats.len(), 1);
        assert!(clipboard.save().unwrap().partial, "an old eDE cannot say it read everything");
        assert!(!clipboard.save().unwrap().partial, "an empty clipboard is whole");
        drop(clipboard);
        drop(ede);
        server.join().unwrap();
    }

    #[test]
    fn an_empty_save_restores_as_a_clear() {
        let (ede, server) = fake_compositor(vec![Ok(serde_json::json!({"serial": 2}))]);
        ede.clipboard().unwrap().restore(SavedClipboard::default()).unwrap();
        drop(ede);
        let seen = server.join().unwrap();
        assert_eq!(seen, [serde_json::json!({"op": "clipboard_set", "items": []})]);
    }

    /// #EXP-1897: `activate_window` is `focus_window`, which the compositor
    /// admits only with `ui`: the grant ePilot mints has to hold it.
    #[test]
    fn the_grant_admits_focus_window() {
        let needs = Request::FocusWindow { window: WindowId::new("w1").unwrap() }.capability().unwrap();
        assert!(CAPABILITIES.contains(&needs), "the grant lacks {}", needs.as_str());
    }

    /// #EXP-1897: a window parked on another workspace is not on screen: a
    /// fullscreen one there must not make its display a gaming display.
    #[test]
    fn parked_windows_are_not_on_screen() {
        use serde_json::json;
        let window = |id: &str, workspace: usize, focused: bool, fullscreen: bool, minimized: bool| {
            json!({"id": id, "title": id, "app_id": "app", "x": 0, "y": 0, "width": 640, "height": 480, "focused": focused, "fullscreen": fullscreen, "protected": false, "x11": false, "workspace": workspace, "minimized": minimized})
        };
        let (ede, _server) = fake_compositor(vec![
            Ok(json!([window("w1", 0, false, false, false), window("w2", 0, true, false, false), window("w3", 1, false, true, false), window("w4", 0, false, false, true)])),
            Ok(json!([{"index": 0, "active": true, "windows": ["w1", "w2", "w4"]}, {"index": 1, "active": false, "windows": ["w3"]}])),
        ]);
        let listed: Vec<(u64, bool, bool)> = ede.list_windows().unwrap().into_iter().map(|window| (window.id, window.active, window.fullscreen)).collect();
        assert_eq!(listed, [(2, true, false), (1, false, false)], "top first, the active workspace only");
    }

    /// #EDE-91: the person's run (and screen memory, which reads through
    /// this backend) sees the person's desk only: no window or output of
    /// an agent space.
    #[test]
    fn a_space_is_not_the_persons_desk() {
        use serde_json::json;
        let _one = OUTPUTS.lock().unwrap_or_else(|p| p.into_inner());
        let window = |id: &str, space: Option<&str>| {
            let mut window = json!({"id": id, "title": id, "app_id": "app", "x": 0, "y": 0, "width": 640, "height": 480, "focused": false, "fullscreen": false, "protected": false, "x11": false, "workspace": 0, "minimized": false});
            if let Some(space) = space {
                window["space"] = json!(space);
            }
            window
        };
        let (ede, _server) = fake_compositor(vec![
            Ok(json!([window("w1", None), window("w2", Some("sp1"))])),
            Ok(json!([{"index": 0, "active": true, "windows": ["w1", "w2"]}])),
            Ok(json!([{"name": "SPACE-1", "x": 0, "y": 0, "width": 1024, "height": 768, "scale": 1.0, "space": "sp1"}, {"name": "DP-1", "x": 0, "y": 0, "width": 1280, "height": 800, "scale": 1.0}])),
        ]);
        assert_eq!(ede.list_windows().unwrap().into_iter().map(|window| window.id).collect::<Vec<_>>(), [1]);
        let displays = ede.displays().unwrap();
        assert_eq!(displays.iter().map(|display| (display.name.as_str(), display.primary)).collect::<Vec<_>>(), [("DP-1", true)]);
    }

    /// The displays cache [`pixel_size`] reads is one per process.
    static OUTPUTS: Mutex<()> = Mutex::new(());

    /// #EXP-1897: eDE speaks logical units (like macOS points); the pixel
    /// size is the output's mode, which is what a capture carries.
    #[test]
    fn a_scaled_output_reports_physical_pixels() {
        use serde_json::json;
        let _one = OUTPUTS.lock().unwrap_or_else(|p| p.into_inner());
        let (ede, _server) = fake_compositor(vec![Ok(json!([
            {"name": "DP-1", "x": 0, "y": 0, "width": 1707, "height": 960, "scale": 1.5, "modes": [{"width": 1920, "height": 1080, "refresh": 60000, "preferred": false, "current": false}, {"width": 2560, "height": 1440, "refresh": 60000, "preferred": true, "current": true}]},
            {"name": "DP-2", "x": 1707, "y": 0, "width": 1280, "height": 800, "scale": 1.25},
        ]))]);
        let displays = ede.displays().unwrap();
        let pixels: Vec<(u32, u32)> = displays.iter().map(|display| crate::backend::pixel_size(display.x, display.y, display.width, display.height)).collect();
        assert_eq!((displays[0].width, displays[0].height), (1707, 960), "the pointer's units stay logical");
        assert_eq!(pixels, [(2560, 1440), (1600, 1000)], "the current mode, else the scaled size");
    }

    /// A headless `exponential-compositor`, ended with the guard: on a
    /// runtime directory of its own (removed with it), or, live
    /// ([`live_compositor`]), on this machine's (`XDG_RUNTIME_DIR`), where
    /// the user manager, the space guard and polkit are real.
    struct Kill {
        child: std::process::Child,
        runtime: PathBuf,
        binary: std::ffi::OsString,
        /// The test's own directory: the config, and the runtime directory
        /// unless the run is live. Removed at the end.
        own: PathBuf,
        /// #EXP-1514 (live): the file its first program writes its
        /// `WAYLAND_DISPLAY` into (the machine's runtime directory has other
        /// compositors' sockets too).
        display_file: Option<PathBuf>,
    }

    impl Drop for Kill {
        fn drop(&mut self) {
            match self.display_file {
                // Live: ended as a session ends it (SIGTERM, then its own
                // exit stops the slice of its spaces), never left to the
                // lingering user manager.
                Some(_) => {
                    let pid = self.child.id();
                    let _ = std::process::Command::new("kill").args(["-TERM", &pid.to_string()]).status();
                    let deadline = std::time::Instant::now() + Duration::from_secs(10);
                    while matches!(self.child.try_wait(), Ok(None)) && std::time::Instant::now() < deadline {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    let _ = std::process::Command::new(&self.binary).args(["space-end", &pid.to_string()]).status();
                }
                None => {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                }
            }
            let _ = std::fs::remove_dir_all(&self.own);
        }
    }

    impl Kill {
        fn spawn(binary: &std::ffi::OsStr, runtime: &Path, own: &Path, display_file: Option<&Path>) -> std::process::Child {
            let mut command = std::process::Command::new(binary);
            command
                .args(["--headless", "640x480", "--no-xwayland", "--config"])
                .arg(own.join("compositor.toml"))
                .env("XDG_RUNTIME_DIR", runtime)
                .env_remove("DISPLAY")
                .env_remove("WAYLAND_DISPLAY")
                .env("RUST_LOG", "warn");
            if let Some(file) = display_file {
                // Its first program says where it listens and lives as long as it does.
                let line = format!("printf %s \"$WAYLAND_DISPLAY\" > '{0}.tmp' && mv '{0}.tmp' '{0}'; while kill -0 $PPID 2>/dev/null; do sleep 1; done", file.display());
                command.args(["--", "sh", "-c", &line]);
            }
            command.spawn().unwrap()
        }

        /// The session of the compositor running on the runtime directory,
        /// once it answers its own token (after a kill the dead one's
        /// socket and token file are still there).
        fn session(&self) -> Session {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let of = |display: &str| session_of(env(&[("XDG_CURRENT_DESKTOP", "Exponential"), ("XDG_RUNTIME_DIR", self.runtime.to_str().unwrap()), ("WAYLAND_DISPLAY", display)])).unwrap().unwrap();
            loop {
                let displays: Vec<String> = match &self.display_file {
                    Some(file) => std::fs::read_to_string(file).into_iter().collect(),
                    None => std::fs::read_dir(self.runtime.join("exponential-desktop"))
                        .into_iter()
                        .flatten()
                        .flatten()
                        .map(|entry| entry.path())
                        .filter(|path| path.extension().is_some_and(|ext| ext == "sock"))
                        .map(|socket| socket.file_stem().unwrap().to_string_lossy().into_owned())
                        .collect(),
                };
                for display in displays {
                    let session = of(&display);
                    if session.root_token().and_then(|root| session.connect_token(root)).is_ok() {
                        return session;
                    }
                }
                assert!(std::time::Instant::now() < deadline, "exponential-compositor did not open its socket");
                std::thread::sleep(Duration::from_millis(20));
            }
        }

        /// The compositor dies as in a crash (its sockets and token file
        /// stay behind) and a new one starts on the same runtime directory.
        fn restart(&mut self) -> Session {
            let _ = self.child.kill();
            let _ = self.child.wait();
            if let Some(file) = &self.display_file {
                let _ = std::fs::remove_file(file);
            }
            self.child = Kill::spawn(&self.binary, &self.runtime, &self.own, self.display_file.as_deref());
            self.session()
        }
    }

    fn compositor_binary() -> Option<std::ffi::OsString> {
        let binary = std::env::var_os("EDE_COMPOSITOR");
        if binary.is_none() {
            assert!(std::env::var_os("EDE_COMPOSITOR_REQUIRED").is_none(), "EDE_COMPOSITOR_REQUIRED is set but EDE_COMPOSITOR is not");
            eprintln!("EDE_COMPOSITOR unset: skipping the headless eDE run");
        }
        binary
    }

    /// The test's own directory, with the compositor's config.
    fn own_dir() -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let own = std::env::temp_dir().join(format!("epilot-ede-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&own).unwrap();
        std::fs::set_permissions(&own, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(own.join("compositor.toml"), "background = \"#102030\"\n").unwrap();
        own
    }

    /// `None` = `EDE_COMPOSITOR` is unset: the test is skipped, unless
    /// `EDE_COMPOSITOR_REQUIRED` is set (CI, #EDE-140): then it fails. On a
    /// runtime directory of its own: no user manager, so no agent space.
    fn headless_compositor() -> Option<(Kill, Session)> {
        let binary = compositor_binary()?;
        let own = own_dir();
        let kill = Kill { child: Kill::spawn(&binary, &own, &own, None), runtime: own.clone(), binary, own, display_file: None };
        let session = kill.session();
        Some((kill, session))
    }

    /// #EXP-1514 (#EXP-2011): a compositor on this machine's runtime
    /// directory, so that its readiness is real: the person's user manager,
    /// the space guard and polkit. Only with `EDE_SPACE_LIVE=1` (the release
    /// suite's `epilot-space-live` job, a hosted runner with all three);
    /// skipped elsewhere: the gate's container has no user manager.
    fn live_compositor() -> Option<(Kill, Session)> {
        if std::env::var_os("EDE_SPACE_LIVE").is_none_or(|live| live != "1") {
            eprintln!("EDE_SPACE_LIVE unset: an agent space needs a desktop that is ready for one (the release suite's epilot-space-live job runs this)");
            return None;
        }
        let binary = std::env::var_os("EDE_COMPOSITOR").expect("EDE_SPACE_LIVE=1 needs EDE_COMPOSITOR");
        let runtime = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").expect("EDE_SPACE_LIVE=1 needs this machine's XDG_RUNTIME_DIR"));
        let own = own_dir();
        let display_file = own.join("display");
        let kill = Kill { child: Kill::spawn(&binary, &runtime, &own, Some(&display_file)), runtime, binary, own, display_file: Some(display_file) };
        let session = kill.session();
        Some((kill, session))
    }

    /// #EDE-126 against a real headless `exponential-compositor` that makes
    /// spaces: a space run's backend and tools end to end. #EXP-1514: through
    /// the gate, on a desktop whose readiness is real (`EDE_SPACE_LIVE=1`,
    /// [`live_compositor`]): the space run waits for the answer the session's
    /// probes give and makes its space only once it says ready. Its grant is
    /// `[space]`, its screen is its space's, its keys, clicks and clipboard
    /// stay there, `launch_app` starts a process in the space's slice that
    /// the space's end takes along, and the person's desk hears nothing of
    /// it. `EDE_SPACE_APP=<a Wayland program>` also checks its window.
    #[test]
    fn headless_ede_runs_a_space_run() {
        let Some((kill, session)) = live_compositor() else { return };
        use serde_json::json;
        let run = "epilot-live-space";
        // The guard's and polkit's probes run at the session's start.
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        let offer = loop {
            let offer = ask(&session, GATE_WITHIN).offer;
            if offer == SpaceOffer::Offered || std::time::Instant::now() > deadline {
                break offer;
            }
            std::thread::sleep(Duration::from_millis(200));
        };
        assert_eq!(offer, SpaceOffer::Offered, "this desktop says a space is ready (its user manager, the space guard, polkit)");
        let hello = session.connect_space(run, SPACE_TIMING).unwrap().hello;
        assert_eq!((hello.capabilities, hello.session_id.as_str()), (vec![Capability::Space], run));
        let mut root = session.connect_token(session.root_token().unwrap()).unwrap();
        let person = |root: &mut Client| (root.pointer().unwrap(), root.clipboard_serial().unwrap(), root.windows().unwrap().len());
        root.clipboard_set(text_items("persons", false), false).unwrap();
        let before = person(&mut root);

        let ede = open_as(&session, Some(admitted(run)), Target::Space(1)).unwrap();
        assert!(lock(&KEEPERS).contains_key(&session.socket) && lock(&KEPT).contains_key(run), "a kept space, and its keeper");
        let spaces = root.spaces().unwrap();
        assert_eq!((spaces.len(), spaces[0].session_id.as_str(), spaces[0].width, spaces[0].height), (1, run, 1280, 800));
        let displays = ede.displays().unwrap();
        assert_eq!(displays.iter().map(|d| (d.name.as_str(), d.width, d.height)).collect::<Vec<_>>(), [("space-1", 1280, 800)]);
        assert!(ede.lock(Box::new(|| {})).is_err(), "a space has no hold");

        // The tools, as a run calls them (no gate, no hold: `engage` is the host's no-op).
        let display = std::cell::Cell::new(None);
        let tool = |name: &str, args: serde_json::Value| {
            let (mut shown, mut mark) = (display.get(), None);
            let reply = crate::tools::call(&*ede, name, &args, &mut crate::tools::CallContext { interrupted: &|| false, display: &mut shown, copy_mark: &mut mark, picked: None, engage: &|| Ok(()) });
            display.set(shown);
            reply
        };
        let ok = |reply: serde_json::Value| {
            assert_eq!(reply["isError"], serde_json::Value::Null, "{}", reply["content"][0]["text"]);
            reply
        };
        let shot = ok(tool("screenshot", json!({})));
        assert_eq!((shot["structuredContent"]["width"].as_u64(), shot["structuredContent"]["height"].as_u64()), (Some(1280), Some(800)));
        assert_eq!(ede.screenshot(&displays[0]).unwrap().rgba[..4], [0x10, 0x20, 0x30, 255], "the space shows the background, not the person's screen");
        ok(tool("mouse_move", json!({ "x": 300, "y": 200 })));
        assert_eq!(ok(tool("cursor_position", json!({})))["structuredContent"], json!({ "x": 300, "y": 200 }));
        ok(tool("left_click", json!({ "x": 310, "y": 210 })));
        ok(tool("key", json!({ "combo": "ctrl+a" })));
        ok(tool("type", json!({ "text": "aB" })));
        ok(tool("scroll", json!({ "x": 10, "y": 10, "direction": "down" })));
        ok(tool("drag", json!({ "from_x": 10, "from_y": 10, "to_x": 50, "to_y": 50 })));
        assert_eq!(ok(tool("list_windows", json!({})))["structuredContent"]["windows"], json!([]));
        ok(tool("wait", json!({ "seconds": 0 })));
        // The space's own clipboard: nobody is focused, so a paste is missed, but what it set stays (no restore).
        assert_eq!(tool("paste_text", json!({ "text": "agents" }))["isError"], true);
        assert_eq!(ok(tool("read_clipboard", json!({})))["content"][0]["text"], "agents");
        assert_eq!(root.clipboard_text().unwrap().text.as_deref(), Some("persons"), "the person's clipboard is theirs");

        // launch_app: a process of the space's, with the space's environment.
        let seen = std::env::temp_dir().join(format!("epilot-live-space-{}", uuid::Uuid::new_v4()));
        let line = format!("env > {0}.tmp; mv {0}.tmp {0}; exec sleep 60", seen.display());
        let pid = ok(tool("launch_app", json!({ "argv": ["sh", "-c", line], "cwd": "/tmp" })))["structuredContent"]["pid"].as_u64().unwrap();
        until("the launched program never ran", || seen.exists());
        let env = std::fs::read_to_string(&seen).unwrap();
        let _ = std::fs::remove_file(&seen);
        let var = |name: &str| env.lines().find_map(|line| line.strip_prefix(&format!("{name}="))).map(str::to_string);
        assert_eq!(var("WAYLAND_DISPLAY"), Some(spaces[0].wayland_display.clone()));
        // #EDE-138: on disk beside the other spaces' homes of this compositor
        // (`$XDG_CACHE_HOME/exponential-desktop/spaces/p<pid>/space-1/config`),
        // or in the space's own directory where the compositor has no cache
        // directory to put it in (a test's bare environment).
        assert!(var("XDG_CONFIG_HOME").is_some_and(|home| home.contains("/space-1/") && home.ends_with("/config")), "a profile of the space's own: {:?}", var("XDG_CONFIG_HOME"));
        assert_eq!((var("DISPLAY"), var("EDE_TOKEN")), (None, None));
        assert!(Path::new(&format!("/proc/{pid}")).exists());
        // #EXP-1514: in a scope of the space's own slice, never the person's session.
        let compositor = kill.child.id();
        let cgroup = std::fs::read_to_string(format!("/proc/{pid}/cgroup")).unwrap();
        assert!(cgroup.contains(&format!("/app-exponential_p{compositor}.slice/app-exponential_p{compositor}-s1.slice/")), "{cgroup}");
        if let Ok(app) = std::env::var("EDE_SPACE_APP") {
            ok(tool("launch_app", json!({ "argv": [app] })));
            until("the app's window never opened in the space", || tool("list_windows", json!({}))["structuredContent"]["windows"].as_array().is_some_and(|w| !w.is_empty()));
            ok(tool("screenshot", json!({})));
        }

        // The person's desk heard nothing of all that.
        assert_eq!(person(&mut root), before, "the person's pointer, clipboard or windows moved");
        assert_eq!(ede_protocol::persons(root.outputs().unwrap()).len(), 1);
        assert!(ede.launch(&["no-such-program-epilot".to_string()], None).unwrap_err().contains("no such program"));

        // #EXP-1514 (D 5): the run ends; its space lives on with its session,
        // and the session's next run drives it with what ran in it.
        drop(ede);
        end_space_run(run);
        assert_eq!(root.spaces().unwrap().len(), 1, "the space ended with its run");
        assert!(Path::new(&format!("/proc/{pid}")).exists());
        let next = open_as(&session, Some(admitted(run)), Target::Space(2)).unwrap();
        assert_eq!(next.displays().unwrap()[0].name, "space-1");
        drop(next);
        // Closed: `destroy_space`, then the revokes; what it launched ends.
        close_space(&session, run);
        until("the space outlived its close", || root.spaces().unwrap().is_empty());
        let stat = || std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
        until("the launched program outlived its space", || stat().is_empty() || stat().contains(") Z "));
        assert!(!lock(&KEPT).contains_key(run));
        // The session's next run: told once that its space is new and empty, then the new one.
        assert_eq!(open_as(&session, Some(admitted(run)), Target::Space(2)).err().unwrap(), SPACE_ENDED);
        let again = open_as(&session, Some(run), Target::Space(2)).unwrap();
        assert_eq!(again.displays().unwrap()[0].name, "space-2");
        drop(again);
        close_space(&session, run);
    }

    /// #EXP-1514 against the real compositor in a place without a user
    /// manager (its own runtime directory; the gate's container has none
    /// either): it says `space-readiness` (#EXP-2011, the flip) and answers
    /// why not, so ePilot makes NO space there, and a space run's first call
    /// says why. Never the person's screen.
    #[test]
    fn headless_ede_says_why_it_makes_no_space() {
        let Some((_kill, session)) = headless_compositor() else { return };
        let run = "epilot-live-unready";
        let mut root = session.connect_token(session.root_token().unwrap()).unwrap();
        assert!(root.hello.offers_spaces() && root.hello.says_space_readiness(), "{:?}", root.hello.features);
        let answer = root.space_readiness().unwrap();
        assert_eq!((answer.ready, answer.block.as_deref()), (false, Some("no_user_manager")), "{answer:?}");
        let offer = SpaceOffer::No(SpaceBlock::Unavailable("no systemd user manager".into()));
        assert_eq!(ask(&session, GATE_WITHIN).offer, offer);
        let refused = open_as(&session, Some(admitted(run)), Target::Space(1)).err().unwrap();
        assert_eq!(refused, no_space(&offer));
        assert!(refused.contains("not available here (no systemd user manager)"), "{refused}");
        assert!(root.spaces().unwrap().is_empty(), "a space was made");
        assert!(!RUN_GRANTS.lock().unwrap().contains_key(run), "a grant was minted");
        assert!(root.windows().is_ok());
    }

    /// #EDE-23 against a real headless `exponential-compositor` (pixman, no
    /// seat, no GPU): `EDE_COMPOSITOR=<path to the binary> cargo test -p
    /// epilot headless_ede`. Skipped without it (the gate's `ede` job builds
    /// the compositor and sets it, #EDE-140; its desktop job builds no
    /// eDE). Checks the grant too: the connection holds `capture`,
    /// `input:person` and `ui` only, never the root's `system`. #EDE-140:
    /// with windows of its own (listed, activated, typed into, a protected
    /// one refused), the person's Esc during a hold, and a compositor that
    /// restarted.
    #[test]
    fn headless_ede_drives_and_captures() {
        let Some((mut compositor, session)) = headless_compositor() else { return };
        assert_eq!(unsupported(&Ok(session.clone())), None);
        let ede = open(&session).unwrap();
        let held = session.connect().unwrap().hello.capabilities;
        assert_eq!(held, CAPABILITIES.to_vec(), "a grant of its own, not the root token");
        // #EXP-1897: focusing is admitted (there is no such window here).
        let refused = ede.activate_window(999).unwrap_err();
        assert!(refused.contains("no such window"), "{refused}");

        let displays = ede.displays().unwrap();
        assert_eq!(displays.len(), 1);
        let screen = &displays[0];
        assert_eq!((screen.id.as_str(), screen.width, screen.height, screen.primary), ("output:HEADLESS-1", 640, 480, true));
        let shot = ede.screenshot(screen).unwrap();
        assert_eq!((shot.width, shot.height), (640, 480));
        assert_eq!(&shot.rgba[..4], [0x10, 0x20, 0x30, 255]);

        ede.mouse_move(100, 50).unwrap();
        assert_eq!(ede.cursor_position().unwrap(), (100, 50));
        ede.mouse_button(Button::Left, true).unwrap();
        ede.mouse_button(Button::Left, false).unwrap();
        ede.scroll(0, 2).unwrap();
        ede.key(Key::Named(crate::keys::Named::Return), true).unwrap();
        ede.key(Key::Named(crate::keys::Named::Return), false).unwrap();
        ede.type_char('A').unwrap();
        // #EDE-85: no key on the default layout makes a snowman; a throwaway
        // keymap types it all the same.
        ede.type_char('\u{2603}').unwrap();
        assert_eq!(ede.list_windows().unwrap(), vec![]);

        // The hold is taken and given back, again and again.
        for _ in 0..2 {
            let guard = ede.lock(Box::new(|| {})).unwrap();
            ede.mouse_move(10, 10).unwrap();
            drop(guard);
        }
        assert_eq!(ede.cursor_position().unwrap(), (10, 10));

        // #EDE-140: real windows (a Wayland client of the test's own). The
        // list, top to bottom; a second window of the app in front takes
        // the keyboard.
        const KEYS: [u32; 4] = [30, 28, 48, 1]; // evdev: A, Enter, B, Esc
        let wayland = session.socket.parent().and_then(Path::parent).unwrap().join(session.socket.file_stem().unwrap());
        let mut app = test_client::App::connect(&wayland);
        let notes = app.window("notes", "epilot.notes", (200, 150), 0xff20_60a0);
        let other = app.window("other", "epilot.other", (200, 150), 0xffa0_6020);
        until("the second window never took the keyboard", || app.focused(&other));
        let listed = ede.list_windows().unwrap();
        let seen: Vec<_> = listed.iter().map(|window| (window.title.as_str(), window.app.as_str(), window.active)).collect();
        assert_eq!(seen, [("other", "epilot.other", true), ("notes", "epilot.notes", false)]);

        // activate_window brings the one behind forward, with the keyboard;
        // the agent's keys reach it and a capture shows it as drawn.
        let behind = listed[1].id;
        ede.activate_window(behind).unwrap();
        until("the activated window never took the keyboard", || app.focused(&notes));
        let listed = ede.list_windows().unwrap();
        assert_eq!((listed[0].id, listed[0].active, listed[1].active), (behind, true, false));
        ede.type_char('a').unwrap();
        ede.key(Key::Named(crate::keys::Named::Return), true).unwrap();
        ede.key(Key::Named(crate::keys::Named::Return), false).unwrap();
        until("the agent's keys never reached the window", || app.keys() == KEYS[..2]);
        let centre = |shot: &Image, window: &WindowInfo| {
            let (x, y) = (window.x + window.width as i32 / 2, window.y + window.height as i32 / 2);
            let at = (y as usize * shot.width as usize + x as usize) * 4;
            shot.rgba[at..at + 4].to_vec()
        };
        assert_eq!(centre(&ede.screenshot(screen).unwrap(), &listed[0]), [0x20, 0x60, 0xa0, 255], "the window in front, as drawn");

        // The hold: the person's keys stay out while the agent's land;
        // their Esc (their keyboard: through the compositor's bindings)
        // ends it, the host hears it and no window does.
        let mut person = session.connect_token(session.root_token().unwrap()).unwrap();
        let tap = |person: &mut Client, key: u32| {
            for pressed in [true, false] {
                person.input_as_person(InputEvent::Key { key, pressed }).unwrap();
            }
        };
        let (esc, escaped) = std::sync::mpsc::channel();
        let guard = ede
            .lock(Box::new(move || {
                let _ = esc.send(());
            }))
            .unwrap();
        tap(&mut person, KEYS[2]);
        ede.type_char('a').unwrap();
        until("the agent's key never landed during the hold", || app.keys() == [KEYS[0], KEYS[1], KEYS[0]]);
        tap(&mut person, KEYS[3]);
        escaped.recv_timeout(Duration::from_secs(5)).expect("the person's Esc never reached the hold");
        drop(guard);
        tap(&mut person, KEYS[2]);
        let typed = [KEYS[0], KEYS[1], KEYS[0], KEYS[2]];
        until("the person's key never landed after the hold", || app.keys() == typed);
        drop(person);

        // A protected window (a password manager) is never driven, never
        // named and black in a capture.
        let vault = app.window("My passwords", "org.keepassxc.KeePassXC", (200, 150), 0xffc0_2020);
        until("the password manager never took the keyboard", || app.focused(&vault));
        let refused = ede.type_char('a').unwrap_err();
        assert!(refused.contains("protected"), "{refused}");
        let listed = ede.list_windows().unwrap();
        assert_eq!((listed[0].title.as_str(), listed[0].app.as_str(), listed[0].active), ("", "org.keepassxc.KeePassXC", true));
        let refused = ede.mouse_move(listed[0].x + 10, listed[0].y + 10).unwrap_err();
        assert!(refused.contains("protected"), "{refused}");
        assert_eq!(centre(&ede.screenshot(screen).unwrap(), &listed[0]), [0, 0, 0, 255], "a protected window is black in a capture");
        assert_eq!(app.keys(), typed, "a key reached the password manager");
        for window in [vault, other, notes] {
            app.close(window);
        }
        until("the windows never closed", || ede.list_windows().unwrap().is_empty());
        drop(app);

        // The clipboard: set, read back, nobody focused takes it, restore.
        let clipboard = ede.clipboard().unwrap();
        let before = clipboard.change_count().unwrap();
        let saved = clipboard.save().unwrap();
        assert!(saved.formats.is_empty());
        clipboard.set_text("hello", false).unwrap();
        assert!(clipboard.change_count().unwrap() > before);
        assert_eq!(clipboard.text().unwrap().as_deref(), Some("hello"));
        assert_eq!(clipboard.wait_pasted(Duration::from_millis(100)), Pasted::Missed);
        clipboard.restore(saved).unwrap();
        assert_eq!(clipboard.text().unwrap(), None);
        drop(clipboard);

        // #EXP-1897: at scale 1.5 the display is logical (what the pointer
        // speaks) and its pixel size is the frame's.
        let _one = OUTPUTS.lock().unwrap_or_else(|p| p.into_inner());
        let mut root = session.connect_token(session.root_token().unwrap()).unwrap();
        root.set_output("HEADLESS-1", None, Some(1.5), None).unwrap();
        let displays = ede.displays().unwrap();
        let screen = &displays[0];
        assert_eq!((screen.width, screen.height), (427, 320));
        let pixels = crate::backend::pixel_size(screen.x, screen.y, screen.width, screen.height);
        let shot = ede.screenshot(screen).unwrap();
        assert_eq!((pixels, (shot.width, shot.height)), ((640, 480), (640, 480)));

        // #EXP-1897: turning computer use off ends the grant; the next call
        // mints a new one.
        let (_, token) = GRANT.lock().unwrap().clone().unwrap();
        session.connect_token(token.clone()).expect("the grant lives until it is revoked");
        session.revoke();
        let gone = session.connect_token(token).err().expect("a revoked grant connects no more");
        assert!(gone.contains("unknown or expired token"), "{gone}");
        assert_eq!(session.connect().unwrap().hello.capabilities, CAPABILITIES.to_vec());

        // #EDE-140: the compositor crashes and comes back on the same
        // runtime directory. It knows nothing of the grant this process
        // still holds: the next backend mints a new one and works.
        let (_, stale) = GRANT.lock().unwrap().clone().unwrap();
        drop((root, ede));
        let session = compositor.restart();
        let gone = session.connect_token(stale.clone()).err().expect("a new compositor knows no old grant");
        assert!(gone.contains("unknown or expired token"), "{gone}");
        let ede = open(&session).expect("the grant is minted again after a restart");
        assert!(GRANT.lock().unwrap().clone().unwrap().1 != stale, "the stale grant is not kept");
        assert_eq!(session.connect().unwrap().hello.capabilities, CAPABILITIES.to_vec());
        let displays = ede.displays().unwrap();
        assert_eq!((displays[0].width, displays[0].height), (640, 480), "the new compositor's own output, at its own scale");
        assert_eq!(ede.screenshot(&displays[0]).unwrap().rgba[..4], [0x10, 0x20, 0x30, 255]);
        ede.mouse_move(20, 30).unwrap();
        assert_eq!(ede.cursor_position().unwrap(), (20, 30));
    }
}
