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
//! it. A run's calls use a grant of the run's own (#EXP-1925: session id =
//! the run's, an expiry renewed while the run calls), revoked at the run's
//! end. `EDE_TOKEN` (a grant handed to this process) is used as it is when
//! it cannot mint.

use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ede_protocol::{Capability, CaptureTarget, ClipboardItem, Client, ErrorCode, InputEvent, Request, SessionId, SpaceInfo, Token, WindowId, CLIPBOARD_SECRET_HINT, CLIPBOARD_TEXT_MIMES};

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

/// #EXP-1925: how long a run's grant lives; a call with less than
/// [`RUN_GRANT_RENEW`] of it left mints the next one. The ones before
/// expire on their own (a call in flight keeps its connection).
const RUN_GRANT_TTL: Duration = Duration::from_secs(15 * 60);
const RUN_GRANT_RENEW: Duration = Duration::from_secs(5 * 60);

/// #EDE-126: how often a space run's keeper looks at the clock.
const KEEPER_TICK: Duration = Duration::from_secs(10);

/// The lifetimes a run's grants are minted and renewed by (the tests'
/// are short, or on a clock that jumps).
#[derive(Clone, Copy)]
struct Timing {
    ttl: Duration,
    renew: Duration,
    tick: Duration,
    /// Unix milliseconds: the wall clock.
    now: fn() -> i64,
}

const RUN_TIMING: Timing = Timing { ttl: RUN_GRANT_TTL, renew: RUN_GRANT_RENEW, tick: KEEPER_TICK, now: now_ms };

/// The space runs with a keeper thread, per compositor socket, and which
/// one (a run granted again gets a new one; the old one sees it and ends).
static KEEPERS: LazyLock<Mutex<HashMap<(PathBuf, String), u64>>> = LazyLock::new(Default::default);

/// One grant minted for a run.
#[derive(Clone)]
struct RunGrant {
    socket: PathBuf,
    token: Token,
    /// Unix milliseconds.
    expires_at: i64,
}

/// Per run: the grants minted for it and not yet expired, newest last.
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

/// Per space run: the compositor it runs on and the space it made there.
static SPACES: LazyLock<Mutex<HashMap<String, (PathBuf, SpaceInfo)>>> = LazyLock::new(Default::default);

const NO_SPACES: &str = "This eDE makes no agent spaces (update eDE); nothing was done.";
const NO_SPACE_GRANT: &str = "eDE gave this run no space grant of its own (the token here cannot mint one); nothing was done.";
const SPACE_ENDED: &str = "The agent space had ended, so nothing was done. A new one was made: it is new and empty, whatever ran in the old one is gone. Take a screenshot and start again.";

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
/// every run's too (#EXP-1925).
pub(crate) fn revoke_grant() {
    if let Some(Ok(session)) = session() {
        session.revoke();
        let runs: Vec<String> = RUN_GRANTS.lock().unwrap_or_else(|p| p.into_inner()).keys().cloned().collect();
        for run in runs {
            session.end_run(&run);
        }
    }
}

/// #EXP-1925: `run` ended: the grants minted for it end. Touches nothing
/// when none was.
pub(crate) fn end_run(run: &str) {
    if !RUN_GRANTS.lock().unwrap_or_else(|p| p.into_inner()).contains_key(run) {
        return;
    }
    if let Some(Ok(session)) = session() {
        session.end_run(run);
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
        let Some(run) = run else { return self.connect() };
        match self.connect_run(run, &CAPABILITIES, RUN_TIMING)? {
            Some(client) => Ok(client),
            None => self.connect(),
        }
    }

    /// #EDE-126: a connection under a space run's own grant. The
    /// invariant: its hello says `[space]` exactly, the run as its
    /// session, and a compositor that makes spaces; anything else is an
    /// error before a single op goes out. Never [`Session::connect`]: a
    /// space run has no hold, no gate and no Esc, so this process's person
    /// grant there would type on the person's seat.
    fn connect_space(&self, run: &str, timing: Timing) -> Result<Client, String> {
        let client = self.connect_run(run, &SPACE_CAPABILITIES, timing)?.ok_or(NO_SPACE_GRANT)?;
        if !client.hello.offers_spaces() {
            return Err(NO_SPACES.to_string());
        }
        // Exact: a capability this build does not know is more than `[space]`.
        if client.hello.capabilities != SPACE_CAPABILITIES || !client.hello.unknown_capabilities.is_empty() || client.hello.session_id.as_str() != run {
            return Err(NO_SPACE_GRANT.to_string());
        }
        Ok(client)
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
        // Read before the mint: an older compositor would serve a `[space]`
        // grant the capability-free ops on the person's desk.
        if capabilities == SPACE_CAPABILITIES && !root.hello.offers_spaces() {
            return Err(NO_SPACES.to_string());
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

    /// #EDE-126: a thread that keeps the space run `run`'s grant alive
    /// while no call does: a renewal runs only inside a call, and the
    /// compositor closes a space whose grant stayed expired. It wakes every
    /// [`Timing::tick`] and compares the wall clock, so the first tick
    /// after a suspend renews too (inside the compositor's grace). It ends
    /// with the run ([`Session::end_run`]). One per run; a person run has none.
    fn keep(&self, run: &str, timing: Timing) {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let key = (self.socket.clone(), run.to_string());
        let mine = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        match KEEPERS.lock().unwrap_or_else(|p| p.into_inner()).entry(key.clone()) {
            std::collections::hash_map::Entry::Occupied(_) => return,
            std::collections::hash_map::Entry::Vacant(slot) => drop(slot.insert(mine)),
        }
        let session = self.clone();
        let keeper = Some(mine);
        let _ = std::thread::Builder::new().name("computer-use-space-keeper".into()).spawn(move || loop {
            std::thread::sleep(timing.tick);
            let now = || KEEPERS.lock().unwrap_or_else(|p| p.into_inner()).get(&key).copied();
            if now() != keeper {
                return;
            }
            let fresh = (timing.now)() + timing.renew.as_millis() as i64;
            let newest = RUN_GRANTS.lock().unwrap_or_else(|p| p.into_inner()).get(&key.1).and_then(|grants| grants.iter().rev().find(|grant| grant.socket == key.0).map(|grant| grant.expires_at));
            if newest.is_some_and(|expires_at| expires_at > fresh) {
                continue;
            }
            if let Err(err) = session.connect_space(&key.1, timing) {
                log::debug!("computer use: a space run's eDE grant was not renewed: {err}");
            }
            // The run ended while this minted: what was minted ends too.
            match now() {
                None => return session.end_run(&key.1),
                other if other != keeper => return,
                _ => {}
            }
        });
    }

    /// `run`'s grants on this compositor end; the compositor forgets them.
    fn end_run(&self, run: &str) {
        KEEPERS.lock().unwrap_or_else(|p| p.into_inner()).remove(&(self.socket.clone(), run.to_string()));
        // A space run's space ends with its grant (the compositor closes it).
        SPACES.lock().unwrap_or_else(|p| p.into_inner()).retain(|of, (socket, _)| of != run || *socket != self.socket);
        let grants = {
            let mut runs = RUN_GRANTS.lock().unwrap_or_else(|p| p.into_inner());
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
        let revoked = self.root_token().and_then(|root| self.connect_token(root)).and_then(|mut root| {
            live.into_iter().try_for_each(|token| root.call(Request::RevokeGrant { token }).map(|_| ()).map_err(e("eDE grant")))
        });
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
/// its space, made on its first call).
pub(crate) fn open_as(session: &Session, run: Option<&str>, target: Target) -> Result<Box<dyn Backend>, String> {
    let ede = |client, space| Ede { client: Mutex::new(client), session: session.clone(), run: run.map(str::to_string), target, space, notice: Mutex::new(None) };
    if target == Target::Person {
        return Ok(Box::new(ede(session.connect_as(run)?, None)));
    }
    open_space(session, run.ok_or("An agent space belongs to a run.")?, target, RUN_TIMING)
}

fn open_space(session: &Session, run: &str, target: Target, timing: Timing) -> Result<Box<dyn Backend>, String> {
    let ede = |client, space| Ede { client: Mutex::new(client), session: session.clone(), run: Some(run.to_string()), target, space, notice: Mutex::new(None) };
    let mut client = session.connect_space(run, timing)?;
    session.keep(run, timing);
    let made = SPACES.lock().unwrap_or_else(|p| p.into_inner()).get(run).filter(|(socket, _)| *socket == session.socket).map(|(_, space)| space.clone());
    let space = match made {
        Some(space) => space,
        None => create_space(&mut client, &session.socket, run)?,
    };
    Ok(Box::new(ede(client, Some(space))))
}

/// A space for `run`, remembered for its next calls.
fn create_space(client: &mut Client, socket: &Path, run: &str) -> Result<SpaceInfo, String> {
    let space = client.create_space(SPACE_SIZE.0, SPACE_SIZE.1).map_err(e("eDE space"))?;
    SPACES.lock().unwrap_or_else(|p| p.into_inner()).insert(run.to_string(), (socket.to_path_buf(), space.clone()));
    Ok(space)
}

/// #EDE-126: does this compositor make agent spaces? Its hello says so;
/// `Err` = why not (an older eDE).
pub(crate) fn spaces_offered(session: &Session) -> Result<(), String> {
    let root = session.connect_token(session.root_token()?)?;
    if root.hello.offers_spaces() { Ok(()) } else { Err(NO_SPACES.to_string()) }
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
        let mut client = self.client.lock().unwrap_or_else(|p| p.into_inner());
        call(&mut client).map_err(|err| self.space_ended(&mut client, &err).unwrap_or_else(|| format!("{what}: {err}")))
    }

    fn space_ended(&self, client: &mut Client, err: &ede_protocol::Error) -> Option<String> {
        let (space, run) = (self.space.as_ref()?, self.run.as_deref()?);
        if !matches!(err, ede_protocol::Error::Failed(failure) if failure.code == ede_protocol::ErrorCode::NotFound) {
            return None;
        }
        if client.spaces().ok()?.iter().any(|live| live.id == space.id) {
            return None;
        }
        SPACES.lock().unwrap_or_else(|p| p.into_inner()).remove(run);
        Some(match create_space(client, &self.session.socket, run) {
            Ok(_) => SPACE_ENDED.to_string(),
            Err(err) => format!("The agent space ended and a new one could not be made ({err}); nothing was done."),
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
        self.ask("eDE launch", |client| client.launch(argv.to_vec(), Default::default(), cwd.map(str::to_string)))
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
    use std::sync::atomic::{AtomicBool, Ordering};
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
    }

    /// #EXP-1925: a fake compositor on a socket of its own: the root token
    /// mints, every mint and revoke is logged.
    fn fake_socket() -> (Session, Arc<Mutex<Vec<String>>>, PathBuf) {
        let faked = fake_socket_of(Fake { spaces: false, root_system: true });
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
        let minted = Arc::new(Mutex::new(HashMap::<String, (SessionId, Vec<Capability>)>::new()));
        let forgotten = Arc::new(Mutex::new(Vec::<String>::new()));
        let spaces = Arc::new(Mutex::new((0u32, HashMap::<String, SpaceInfo>::new())));
        let (seen, end, forgets) = (log.clone(), end_spaces.clone(), forget.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let (log, minted, spaces, end, root) = (seen.clone(), minted.clone(), spaces.clone(), end.clone(), root.clone());
                let (forgets, forgotten) = (forgets.clone(), forgotten.clone());
                std::thread::spawn(move || {
                    let mut me = SessionId::new("s").unwrap();
                    loop {
                        let Ok(frame) = read_frame(&mut stream) else { return };
                        let call: Call = serde_json::from_slice(&frame).unwrap();
                        if end.swap(false, Ordering::SeqCst) {
                            spaces.lock().unwrap().1.clear();
                        }
                        if forgets.swap(false, Ordering::SeqCst) {
                            let gone: Vec<String> = minted.lock().unwrap().drain().map(|(token, _)| token).collect();
                            forgotten.lock().unwrap().extend(gone);
                        }
                        let op = serde_json::to_value(&call.request).unwrap()["op"].as_str().unwrap().to_string();
                        let space = spaces.lock().unwrap().1.get(me.as_str()).cloned();
                        let result = match call.request {
                            Request::Hello { token, .. } if forgotten.lock().unwrap().contains(&token.as_str().to_string()) => {
                                Err(Failure::new(ErrorCode::Unauthorized, "this grant means nothing to me"))
                            }
                            Request::Hello { token, .. } => {
                                let mut capabilities = vec![Capability::Capture, Capability::InputPerson, Capability::Ui];
                                if token.as_str() == root && fake.root_system {
                                    capabilities.push(Capability::System);
                                }
                                if let Some((session, held)) = minted.lock().unwrap().get(token.as_str()).cloned() {
                                    (me, capabilities) = (session, held);
                                }
                                let features: &[&str] = if fake.spaces { &["spaces"] } else { &[] };
                                Ok(serde_json::json!({"version": 1, "capabilities": capabilities, "session_id": me, "features": features}))
                            }
                            Request::MintGrant { session_id, expires_at, capabilities, .. } => {
                                log.lock().unwrap().push(format!("mint {session_id} {}", expires_at.is_some()));
                                // Numbered past the forgotten ones: a new grant is never an old one.
                                let past = forgotten.lock().unwrap().len();
                                let token = Token::new(format!("{:064x}", minted.lock().unwrap().len() + past + 1)).unwrap();
                                minted.lock().unwrap().insert(token.as_str().to_string(), (session_id.clone(), capabilities.clone()));
                                Ok(serde_json::to_value(ede_protocol::Grant { token, session_id, run_id: None, capabilities, expires_at }).unwrap())
                            }
                            Request::RevokeGrant { token } => {
                                log.lock().unwrap().push(format!("revoke {}", token.as_str()));
                                Ok(serde_json::Value::Null)
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
        Faked { session: Session { socket, token_file, token: None }, log, dir, end_spaces, forget }
    }

    fn logged(faked: &Faked) -> Vec<String> {
        faked.log.lock().unwrap().clone()
    }

    /// The fake itself: a minted grant says hello with what it was minted,
    /// and a space lives until the switch ends it.
    #[test]
    fn the_fake_compositor_speaks_spaces() {
        let faked = fake_socket_of(Fake { spaces: true, root_system: true });
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
        let narrow = fake_socket_of(Fake { spaces: false, root_system: false });
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
        let faked = fake_socket_of(Fake { spaces: true, root_system: true });
        let run = "epilot-test-space-a";
        let client = faked.session.connect_space(run, RUN_TIMING).unwrap();
        assert_eq!((client.hello.capabilities.clone(), client.hello.session_id.as_str()), (vec![Capability::Space], run));
        drop(client);
        let first = open_as(&faked.session, Some(run), Target::Space(7)).unwrap();
        assert_eq!(first.target(), Target::Space(7));
        first.mouse_move(10, 10).unwrap();
        drop(first);
        open_as(&faked.session, Some(run), Target::Space(7)).unwrap().mouse_move(20, 20).unwrap();
        assert_eq!(logged(&faked), [format!("mint {run} true"), "create_space 1280x800".to_string(), "input".to_string(), "input".to_string()]);
        // The run's end revokes its grant (the compositor closes the space) and forgets the space.
        faked.session.end_run(run);
        assert!(logged(&faked).last().unwrap().starts_with("revoke ") && !SPACES.lock().unwrap().contains_key(run));
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// #EDE-126: an older compositor would serve a `[space]` grant the
    /// capability-free ops on the person's desk: its hello is read before
    /// anything is minted or asked, and there is no fallback to the seat.
    #[test]
    fn an_older_compositor_makes_no_space_and_is_asked_nothing() {
        let faked = fake_socket_of(Fake { spaces: false, root_system: true });
        let refused = open_as(&faked.session, Some("epilot-test-space-old"), Target::Space(1)).err().unwrap();
        assert!(refused.contains("no agent spaces"), "{refused}");
        assert!(spaces_offered(&faked.session).is_err());
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
        let mut handed = fake_socket_of(Fake { spaces: true, root_system: true });
        handed.session.token = Some("e".repeat(64));
        // The token file's grant holds no `system`.
        let narrow = fake_socket_of(Fake { spaces: true, root_system: false });
        for faked in [&handed, &narrow] {
            let refused = open_as(&faked.session, Some("epilot-test-space-p"), Target::Space(1)).err().unwrap();
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
        let faked = fake_socket_of(Fake { spaces: true, root_system: true });
        let run = "epilot-test-space-e";
        let ede = open_as(&faked.session, Some(run), Target::Space(1)).unwrap();
        let missing = ede.activate_window(999).unwrap_err();
        assert!(missing.contains("no such window"), "{missing}");
        faked.end_spaces.store(true, Ordering::SeqCst);
        let told = ede.mouse_move(10, 10).unwrap_err();
        assert!(told.contains("new and empty"), "{told}");
        drop(ede);
        open_as(&faked.session, Some(run), Target::Space(1)).unwrap().mouse_move(10, 10).unwrap();
        assert_eq!(logged(&faked)[1..], ["create_space 1280x800", "focus_window", "list_spaces", "input", "list_spaces", "create_space 1280x800", "input"]);
        assert_eq!(SPACES.lock().unwrap().get(run).unwrap().1.id.as_str(), "space-2");
        faked.session.end_run(run);
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// #EDE-126: a space has no hold; its backend never asks for the
    /// person's (that would take a person grant).
    #[test]
    fn a_space_backend_never_takes_the_persons_hold() {
        let faked = fake_socket_of(Fake { spaces: true, root_system: true });
        let run = "epilot-test-space-h";
        let ede = open_as(&faked.session, Some(run), Target::Space(1)).unwrap();
        assert!(ede.lock(Box::new(|| {})).is_err());
        assert_eq!(logged(&faked), [format!("mint {run} true"), "create_space 1280x800".to_string()]);
        faked.session.end_run(run);
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// #EDE-126: a space run's screen is its space's output and its
    /// windows are the space's; it asks for no workspaces (the person's)
    /// and its output never enters the person's display sizes.
    #[test]
    fn a_space_backend_lists_its_space_and_nothing_of_the_persons_desk() {
        let _one = OUTPUTS.lock().unwrap_or_else(|p| p.into_inner());
        let faked = fake_socket_of(Fake { spaces: true, root_system: true });
        let run = "epilot-test-space-l";
        let before = PIXELS.lock().unwrap().clone();
        let ede = open_as(&faked.session, Some(run), Target::Space(1)).unwrap();
        let displays = ede.displays().unwrap();
        assert_eq!(displays.iter().map(|display| (display.name.as_str(), display.width, display.height, display.primary)).collect::<Vec<_>>(), [("space-1", 1280, 800, true)]);
        assert_eq!(ede.list_windows().unwrap().iter().map(|window| (window.id, window.active)).collect::<Vec<_>>(), [(5, true)]);
        assert_eq!(*PIXELS.lock().unwrap(), before);
        assert_eq!(logged(&faked)[1..], ["create_space 1280x800", "list_outputs", "list_windows"]);
        faked.session.end_run(run);
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// #EDE-126: `launch_app` is `launch` with the argv and the cwd and no
    /// environment of ePilot's: the compositor decides what a launch gets.
    /// Only a space backend sends it.
    #[test]
    fn a_launch_goes_to_the_compositor_with_no_environment_of_ours() {
        let faked = fake_socket_of(Fake { spaces: true, root_system: true });
        let run = "epilot-test-space-x";
        let ede = open_as(&faked.session, Some(run), Target::Space(1)).unwrap();
        assert_eq!(ede.launch(&["foot".to_string()], Some("/tmp")).unwrap(), 77);
        assert_eq!(logged(&faked)[2..], [r#"launch ["foot"] {} Some("/tmp")"#]);
        let person = open_as(&faked.session, Some("epilot-test-person-x"), Target::Person).unwrap();
        assert!(person.launch(&["foot".to_string()], None).is_err());
        assert_eq!(logged(&faked).iter().filter(|line| line.starts_with("launch")).count(), 1);
        for run in [run, "epilot-test-person-x"] {
            faked.session.end_run(run);
        }
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    fn mints(faked: &Faked) -> usize {
        logged(faked).iter().filter(|line| line.starts_with("mint ")).count()
    }

    fn until(what: &str, mut ok: impl FnMut() -> bool) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !ok() {
            assert!(std::time::Instant::now() < deadline, "{what}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn kept(faked: &Faked, run: &str) -> bool {
        KEEPERS.lock().unwrap().contains_key(&(faked.session.socket.clone(), run.to_string()))
    }

    /// #EDE-126: a renewal runs only inside a call, and an idle space run
    /// makes none: its keeper mints the next grant before the last one
    /// runs out (the compositor closes a space whose grant expired). It
    /// ends with the run.
    #[test]
    fn an_idle_space_run_keeps_its_grant_until_its_end() {
        let faked = fake_socket_of(Fake { spaces: true, root_system: true });
        let run = "epilot-test-keeper-a";
        let short = Timing { ttl: Duration::from_millis(600), renew: Duration::from_millis(300), tick: Duration::from_millis(20), now: now_ms };
        drop(open_space(&faked.session, run, Target::Space(1), short).unwrap());
        assert!(kept(&faked, run));
        until("the keeper renewed no grant", || mints(&faked) >= 3);
        assert_eq!(logged(&faked).iter().filter(|line| line.starts_with("create_space")).count(), 1, "a renewal makes no space");
        faked.session.end_run(run);
        assert!(!kept(&faked, run));
        std::thread::sleep(Duration::from_millis(100));
        let ended = mints(&faked);
        std::thread::sleep(Duration::from_millis(700));
        assert_eq!(mints(&faked), ended, "the keeper outlived its run");
        assert!(!RUN_GRANTS.lock().unwrap().contains_key(run));
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// #EDE-126: the keeper compares the wall clock, so a suspend longer
    /// than the grant's life is renewed at its first tick after it.
    #[test]
    fn the_keeper_renews_after_a_wall_clock_jump() {
        static SKEW: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
        fn skewed() -> i64 {
            now_ms() + SKEW.load(Ordering::SeqCst)
        }
        let faked = fake_socket_of(Fake { spaces: true, root_system: true });
        let run = "epilot-test-keeper-j";
        let long = Timing { tick: Duration::from_millis(20), now: skewed, ..RUN_TIMING };
        drop(open_space(&faked.session, run, Target::Space(1), long).unwrap());
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(mints(&faked), 1, "a fresh grant is left alone");
        SKEW.store(2 * 60 * 60 * 1000, Ordering::SeqCst);
        until("no renewal after the jump", || mints(&faked) == 2);
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(mints(&faked), 2, "one renewal, by the clock");
        faked.session.end_run(run);
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// A person run renews inside its calls and takes the hold: no keeper.
    #[test]
    fn a_person_run_has_no_keeper() {
        let faked = fake_socket_of(Fake { spaces: true, root_system: true });
        let run = "epilot-test-keeper-p";
        drop(open_as(&faked.session, Some(run), Target::Person).unwrap());
        assert!(!kept(&faked, run) && RUN_GRANTS.lock().unwrap().contains_key(run));
        faked.session.end_run(run);
        let _ = std::fs::remove_dir_all(&faked.dir);
    }

    /// #EXP-1994: a compositor that forgot the run's grant (a restart)
    /// refuses it `unauthorized`, in whatever words: a new grant is minted
    /// and the call goes on.
    #[test]
    fn a_forgotten_grant_is_minted_again_by_its_code_not_its_words() {
        let faked = fake_socket_of(Fake { spaces: false, root_system: true });
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

    /// A headless `exponential-compositor` on a runtime directory of its
    /// own, ended and removed with the guard.
    struct Kill {
        child: std::process::Child,
        runtime: PathBuf,
        binary: std::ffi::OsString,
    }

    impl Drop for Kill {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let _ = std::fs::remove_dir_all(&self.runtime);
        }
    }

    impl Kill {
        fn spawn(binary: &std::ffi::OsStr, runtime: &Path) -> std::process::Child {
            std::process::Command::new(binary)
                .args(["--headless", "640x480", "--no-xwayland", "--config"])
                .arg(runtime.join("compositor.toml"))
                .env("XDG_RUNTIME_DIR", runtime)
                .env_remove("DISPLAY")
                .env_remove("WAYLAND_DISPLAY")
                .env("RUST_LOG", "warn")
                .spawn()
                .unwrap()
        }

        /// The session of the compositor running on the runtime directory,
        /// once it answers its own token (after a kill the dead one's
        /// socket and token file are still there).
        fn session(&self) -> Session {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            loop {
                let sockets = std::fs::read_dir(self.runtime.join("exponential-desktop")).into_iter().flatten().flatten().map(|entry| entry.path());
                for socket in sockets.filter(|path| path.extension().is_some_and(|ext| ext == "sock")) {
                    let display = socket.file_stem().unwrap().to_string_lossy().into_owned();
                    let session = session_of(env(&[("XDG_CURRENT_DESKTOP", "Exponential"), ("XDG_RUNTIME_DIR", self.runtime.to_str().unwrap()), ("WAYLAND_DISPLAY", &display)]))
                        .unwrap()
                        .unwrap();
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
            self.child = Kill::spawn(&self.binary, &self.runtime);
            self.session()
        }
    }

    /// `None` = `EDE_COMPOSITOR` is unset: the test is skipped, unless
    /// `EDE_COMPOSITOR_REQUIRED` is set (CI, #EDE-140): then it fails.
    fn headless_compositor() -> Option<(Kill, Session)> {
        let Some(binary) = std::env::var_os("EDE_COMPOSITOR") else {
            assert!(std::env::var_os("EDE_COMPOSITOR_REQUIRED").is_none(), "EDE_COMPOSITOR_REQUIRED is set but EDE_COMPOSITOR is not");
            eprintln!("EDE_COMPOSITOR unset: skipping the headless eDE run");
            return None;
        };
        use std::os::unix::fs::PermissionsExt;
        let runtime = std::env::temp_dir().join(format!("epilot-ede-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&runtime).unwrap();
        std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(runtime.join("compositor.toml"), "background = \"#102030\"\n").unwrap();
        let kill = Kill { child: Kill::spawn(&binary, &runtime), runtime, binary };
        let session = kill.session();
        Some((kill, session))
    }

    /// #EDE-126 against a real headless `exponential-compositor` that makes
    /// spaces (`EDE_COMPOSITOR=<binary> cargo test -p epilot headless_ede`;
    /// skipped without it): a space run's backend and tools end to end.
    /// Its grant is `[space]`, its screen is its space's, its keys, clicks
    /// and clipboard stay there, `launch_app` starts a process the space's
    /// end takes along, and the person's desk hears nothing of it.
    /// `EDE_SPACE_APP=<a Wayland program>` also checks its window.
    #[test]
    fn headless_ede_runs_a_space_run() {
        let Some((_kill, session)) = headless_compositor() else { return };
        use serde_json::json;
        let run = "epilot-live-space";
        spaces_offered(&session).expect("this compositor makes spaces");
        let hello = session.connect_space(run, RUN_TIMING).unwrap().hello;
        assert_eq!((hello.capabilities, hello.session_id.as_str()), (vec![Capability::Space], run));
        let mut root = session.connect_token(session.root_token().unwrap()).unwrap();
        let person = |root: &mut Client| (root.pointer().unwrap(), root.clipboard_serial().unwrap(), root.windows().unwrap().len());
        root.clipboard_set(text_items("persons", false), false).unwrap();
        let before = person(&mut root);

        let ede = open_as(&session, Some(run), Target::Space(1)).unwrap();
        assert!(KEEPERS.lock().unwrap().contains_key(&(session.socket.clone(), run.to_string())), "a space run has a keeper");
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
        if let Ok(app) = std::env::var("EDE_SPACE_APP") {
            ok(tool("launch_app", json!({ "argv": [app] })));
            until("the app's window never opened in the space", || tool("list_windows", json!({}))["structuredContent"]["windows"].as_array().is_some_and(|w| !w.is_empty()));
            ok(tool("screenshot", json!({})));
        }

        // The person's desk heard nothing of all that.
        assert_eq!(person(&mut root), before, "the person's pointer, clipboard or windows moved");
        assert_eq!(ede_protocol::persons(root.outputs().unwrap()).len(), 1);
        assert!(ede.launch(&["no-such-program-epilot".to_string()], None).unwrap_err().contains("no such program"));

        // The run ends: its grant is revoked, the compositor closes the space and ends what it launched.
        drop(ede);
        session.end_run(run);
        until("the space outlived its run", || root.spaces().unwrap().is_empty());
        let stat = || std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
        until("the launched program outlived its space", || stat().is_empty() || stat().contains(") Z "));
        assert!(!KEEPERS.lock().unwrap().contains_key(&(session.socket.clone(), run.to_string())));
        // A space run after it gets a new, empty space.
        let again = open_as(&session, Some(run), Target::Space(2)).unwrap();
        assert_eq!(again.displays().unwrap()[0].name, "space-2");
        drop(again);
        session.end_run(run);
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
        assert!(ede.type_char('\u{2603}').is_err(), "no key types a snowman on the default layout");
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
