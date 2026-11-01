//! NEW-58: the voice device's SCREEN MEMORY (`fixtures/screen-memory.json`
//! is the spec): the last 10 minutes of the screen as deduplicated frames,
//! and the `get_screenshots` host tool the voice agent's session reaches over
//! a loopback MCP endpoint (`screen`), like the `computer` grant.
//!
//! The IDE turns it on and off (`start` / `stop`) from the tray's Screen
//! memory switch on the member's voice device, hears paused/active through
//! [`set_status_observer`], and hands the voice agent's run a [`Grant`].

mod capture;
mod engine;
mod fixture;
mod hash;
mod ring;
mod server;
mod shrink;
mod store;
mod tool;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};

pub const SERVER_NAME: &str = "screen";
/// The env var the run's token rides in (`Bearer ${EXP_SCREEN_TOKEN}`).
pub const TOKEN_ENV: &str = "EXP_SCREEN_TOKEN";

/// What a run needs to reach the endpoint.
#[derive(Clone, PartialEq, Eq)]
pub struct Grant {
    pub url: String,
    pub token: String,
}

impl std::fmt::Debug for Grant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Grant({}, <redacted>)", self.url)
    }
}

/// What the tray item shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Off,
    Active,
    /// A denylisted window is focused, or the pause hotkey holds it.
    Paused,
}

/// The running screen memory, `None` while off.
static RUNNING: Mutex<Option<engine::Memory>> = Mutex::new(None);
/// The data dir of the last start: where `stop(true)` deletes.
static LAST_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);
static STATUS: Mutex<Status> = Mutex::new(Status::Off);
type Observer = Arc<dyn Fn(Status) + Send + Sync>;
static OBSERVER: Mutex<Option<Observer>> = Mutex::new(None);
static HOTKEY: LazyLock<Arc<AtomicBool>> = LazyLock::new(|| Arc::new(AtomicBool::new(false)));

fn publish(status: Status) {
    let changed = std::mem::replace(&mut *STATUS.lock().unwrap(), status) != status;
    if changed {
        let observer = OBSERVER.lock().unwrap().clone();
        if let Some(observer) = observer {
            observer(status);
        }
    }
}

struct Live {
    server: emcp::Server,
}

/// The process-wide endpoint, started on the first grant; an `Err` (no
/// free port) is remembered.
static LIVE: OnceLock<Result<Live, String>> = OnceLock::new();

fn live() -> Result<&'static Live, String> {
    LIVE.get_or_init(|| {
        let call: server::Call = Arc::new(|name, arguments| {
            let store = RUNNING.lock().unwrap().as_ref().map(|memory| memory.store.clone());
            // Only frames of a display that is a work display NOW go out.
            let work = match store {
                Some(_) => epilot::screen_view().map(|view| view.work).unwrap_or_default(),
                None => Vec::new(),
            };
            tool::call(store.as_deref(), name, arguments, engine::now_ms(), &work)
        });
        let server = server::serve(call)?;
        log::info!("[screen] serving screen memory on {}", server.url());
        Ok(Live { server })
    })
    .as_ref()
    .map_err(Clone::clone)
}

/// Whether screen memory can ever capture here: it needs the computer
/// crate's window list (PAUSE fails closed without one), which Wayland and
/// a display-less session lack, so the Screen memory row hides there.
pub fn available() -> bool {
    cfg!(any(target_os = "macos", target_os = "linux", target_os = "windows")) && epilot::visible::window_list_supported()
}

/// Bumped by every stop: a start asked for before it is cancelled.
static STOPS: AtomicU64 = AtomicU64::new(0);
/// One capturer opens at a time (a second start waits for the first's
/// portal dialog, then finds it running or cancelled).
static OPENING: Mutex<()> = Mutex::new(());

/// What a cancelled start answers.
pub const CANCELLED: &str = "Screen memory was switched off while it started.";

/// A start as asked for: a [`stop`] (or [`forget`]) after [`ticket`]
/// cancels it, even while it waits on the Linux portal's dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket(u64);

/// Take when the start is ASKED for (in order with the stops), then hand to
/// [`start_with`] on any thread.
pub fn ticket() -> Ticket {
    Ticket(STOPS.load(Ordering::Acquire))
}

/// Start capturing into `data_dir/screen` (frames) + `data_dir/screen.sqlite`
/// (index); a no-op while running. `Err` = this machine cannot capture.
pub fn start(data_dir: PathBuf) -> Result<(), String> {
    start_with(data_dir, ticket())
}

/// [`start`] for a start asked for at `ticket`: a stop since then cancels it
/// ([`CANCELLED`]); one that comes while the capturer opens (the portal's
/// dialog) does not wait for it: the capturer, once open, is closed at once.
pub fn start_with(data_dir: PathBuf, ticket: Ticket) -> Result<(), String> {
    let state_dir = data_dir.clone();
    start_inner(data_dir, ticket, move || capture::open(&state_dir), Box::new(epilot::visible::screen_look), Box::new(epilot::visible::display_epoch))
}

pub(crate) fn start_inner(
    data_dir: PathBuf,
    ticket: Ticket,
    open: impl FnOnce() -> Result<Box<dyn capture::Capturer>, String>,
    view: engine::ViewSource,
    epoch: engine::EpochSource,
) -> Result<(), String> {
    let cancelled = || STOPS.load(Ordering::Acquire) != ticket.0;
    let _opening = OPENING.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if cancelled() {
        return Err(CANCELLED.to_string());
    }
    if RUNNING.lock().unwrap().is_some() {
        return Ok(());
    }
    let capturer = open()?;
    let mut running = RUNNING.lock().unwrap();
    // Under the lock: a stop either came before (cancelled here) or waits
    // for this start and stops it.
    if cancelled() {
        drop(running);
        drop(capturer);
        return Err(CANCELLED.to_string());
    }
    if running.is_some() {
        return Ok(());
    }
    let memory = engine::Memory::start(&data_dir, capturer, view, epoch, engine::Timing::from_fixture(), HOTKEY.clone(), Arc::new(publish))?;
    *LAST_DIR.lock().unwrap() = Some(data_dir);
    *running = Some(memory);
    Ok(())
}

/// Stop capturing; `forget` also deletes every frame and index row (sign
/// out, another voice device or account). Cancels every start asked for
/// before it, without waiting for one still opening.
pub fn stop(forget: bool) {
    STOPS.fetch_add(1, Ordering::AcqRel);
    let memory = RUNNING.lock().unwrap().take();
    match memory {
        Some(memory) => memory.stop(forget),
        None if forget => {
            if let Some(dir) = LAST_DIR.lock().unwrap().clone() {
                store::forget(&dir);
            }
        }
        None => {}
    }
    publish(Status::Off);
}

/// Stop and delete every frame and index row under `data_dir`, whether or
/// not this process started there (sign-out before any start).
pub fn forget(data_dir: &Path) {
    stop(true);
    store::forget(data_dir);
}

/// Whether `data_dir` holds a buffer (frames or the index): at start-up
/// with the switch off one left there is deleted.
pub fn leftover(data_dir: &Path) -> bool {
    data_dir.join(store::FRAMES_DIR).exists() || data_dir.join(store::INDEX_FILE).exists()
}

pub fn status() -> Status {
    *STATUS.lock().unwrap()
}

/// Who hears Off / Active / Paused changes (the tray). Called at once.
/// A later observer replaces an earlier one.
pub fn set_status_observer(observer: impl Fn(Status) + Send + Sync + 'static) {
    let observer: Observer = Arc::new(observer);
    *OBSERVER.lock().unwrap() = Some(observer.clone());
    observer(status());
}

/// NEW-57's pause hotkey: holds capture paused while `true` (the loop
/// reads it on its next focus poll).
pub fn set_hotkey_paused(paused: bool) {
    HOTKEY.store(paused, Ordering::Release);
}

/// Admit `session_id` (the voice agent's conversation run) to
/// `get_screenshots`; `Err` while screen memory is off. A grant held over
/// a stop answers `copy.off` until screen memory is on again.
pub fn grant(session_id: &str) -> Result<Grant, String> {
    if RUNNING.lock().unwrap().is_none() {
        return Err(fixture::copy("off").to_string());
    }
    let live = live()?;
    Ok(Grant { url: live.server.url().to_string(), token: live.server.grant(session_id, session_id) })
}

/// The run ended: its token is dead.
pub fn revoke(session_id: &str) {
    if let Some(Ok(live)) = LIVE.get() {
        live.server.revoke(session_id);
    }
}

#[cfg(test)]
mod tests;
