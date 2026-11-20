//! The OS backend behind the tools: what one desktop can do, in the input
//! units of its OS. One implementation per OS; the hub opens one per tool
//! call ([`open`]: an X connection, a no-op elsewhere), so a connection that
//! died takes no state with it.
//!
//! Everything here is FOREGROUND: the pointer the person sees moves, the
//! active window is the one that gets the keys. Coordinates are the OS's
//! own (X11 and Windows: physical pixels of the virtual screen; macOS:
//! points of the main display); the frame the agent sees is `tools.rs`'s.

#[cfg(target_os = "linux")]
mod ede;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

use crate::keys::Key;

/// One RGBA8 frame of the screen, the OS's full resolution.
pub(crate) struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    /// #EXP-1994: a capture of 32-bit pixels in B, G, R, A (or X) byte
    /// order, rows `stride` bytes apart (macOS, Windows, X11's common
    /// visual, eDE's `argb8888`), as opaque RGBA8: the one conversion of
    /// every backend. Bytes past a row's pixels are padding; the last row
    /// needs none. `None` = a row is missing, or wider than `stride`.
    pub(crate) fn from_bgra(bytes: &[u8], width: u32, height: u32, stride: usize) -> Option<Image> {
        let row = usize::try_from(width).ok()?.checked_mul(4)?;
        if stride < row {
            return None;
        }
        let rows = usize::try_from(height).ok()?;
        let mut rgba = Vec::with_capacity(row.checked_mul(rows)?);
        for y in 0..rows {
            let start = y.checked_mul(stride)?;
            for pixel in bytes.get(start..start.checked_add(row)?)?.chunks_exact(4) {
                rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
            }
        }
        Some(Image { width, height, rgba })
    }
}

/// One display (monitor) of the screen, in screen units.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DisplayInfo {
    /// #FEED-161: stable across reconnects and reorders, per OS
    /// (`fixtures/work-screen.json` DISPLAYS: macOS the display UUID,
    /// Windows the monitor's device interface path, X11 `edid:`/`output:`);
    /// unique within one list ([`finish_ids`]), at most [`ID_MAX`] chars.
    pub id: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
    pub name: String,
    /// #EXP-1919: what tells this display from a twin (set by
    /// [`finish_ids`]: its connector, whether a twin is connected or not);
    /// `None` = the OS named none, or its twins are told apart by order.
    pub connector: Option<String>,
}

impl DisplayInfo {
    /// The area (screen units²) a rect shares with this display.
    pub(crate) fn overlap(&self, x: i32, y: i32, width: u32, height: u32) -> u64 {
        let span = |start: i32, len: u32, own: i32, own_len: u32| {
            let lo = i64::from(start.max(own));
            let hi = (i64::from(start) + i64::from(len)).min(i64::from(own) + i64::from(own_len));
            (hi - lo).max(0) as u64
        };
        span(x, width, self.x, self.width) * span(y, height, self.y, self.height)
    }
}

/// The longest display id (`work-screen.json` DISPLAYS).
pub(crate) const ID_MAX: usize = 128;

/// #FEED-161: every id at most [`ID_MAX`] chars (code points). #FEED-164
/// TWINS (`work-screen.json` `idCases`): displays sharing one id (monitors
/// without a serial) get `@` + their `connectors[i]` (macOS the unit
/// number, Windows the GDI device name, X11 the RandR output name), so a
/// reorder never swaps them; only when a twin's connector is unknown or
/// shared do that id's twins fall back to `#2`, `#3`, … in the OS's order.
/// A display without a twin keeps its bare id.
pub(crate) fn finish_ids(displays: &mut [DisplayInfo], connectors: &[Option<String>]) {
    let clip = |id: &str, room: usize| id.chars().take(room).collect::<String>();
    let suffixed = |base: &str, suffix: &str| format!("{}{suffix}", clip(base, ID_MAX.saturating_sub(suffix.chars().count())));
    let bases: Vec<String> = displays.iter().map(|display| clip(&display.id, ID_MAX)).collect();
    let connector = |index: usize| connectors.get(index).cloned().flatten().filter(|connector| !connector.is_empty());
    let mut seen: Vec<String> = Vec::with_capacity(displays.len());
    for (index, display) in displays.iter_mut().enumerate() {
        let base = &bases[index];
        let twins: Vec<usize> = (0..bases.len()).filter(|other| bases[*other] == *base).collect();
        let by_connector = twins.len() > 1
            && twins.iter().all(|twin| connector(*twin).is_some())
            && twins.iter().all(|twin| twins.iter().filter(|other| connector(**other) == connector(*twin)).count() == 1);
        let mut id = if by_connector { twin_id(base, &connector(index).unwrap_or_default()) } else { base.clone() };
        display.connector = connector(index).filter(|_| by_connector || twins.len() == 1);
        // OS order: the fallback, and the last guard against a collision.
        let mut n = 1;
        while seen.contains(&id) {
            n += 1;
            id = suffixed(base, &format!("#{n}"));
        }
        seen.push(id.clone());
        display.id = id;
    }
}

/// #EXP-1919: the id a display `base` carries beside a twin: `@` + its
/// connector, cut so the whole fits [`ID_MAX`].
pub(crate) fn twin_id(base: &str, connector: &str) -> String {
    let suffix = format!("@{connector}");
    let room = ID_MAX.saturating_sub(suffix.chars().count());
    format!("{}{suffix}", base.chars().take(room).collect::<String>())
}

/// #EXP-1994: the tail of every backend's [`Backend::displays`]: nobody
/// marked primary makes the first one primary, then [`finish_ids`] with
/// `connectors[i]` beside `displays[i]`. `None` = no display, which each
/// backend answers its own way (an error; X11 the whole root).
pub(crate) fn finish_displays(mut displays: Vec<DisplayInfo>, connectors: &[Option<String>]) -> Option<Vec<DisplayInfo>> {
    if displays.is_empty() {
        return None;
    }
    if !displays.iter().any(|display| display.primary) {
        displays[0].primary = true;
    }
    finish_ids(&mut displays, connectors);
    Some(displays)
}

/// FNV-1a 64: a hash that stays the same across builds (X11's `edid:` id).
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
pub(crate) fn stable_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, &byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3))
}

/// One top-level window, in screen units.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WindowInfo {
    pub id: u64,
    pub title: String,
    /// The owning application (WM_CLASS, the bundle's name, the exe stem).
    pub app: String,
    pub pid: Option<u32>,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub active: bool,
    /// NEW-59: a fullscreen app's window, by the OS's rule
    /// (`fixtures/work-screen.json` FULLSCREEN): macOS = an app's layer-0
    /// window (not the Dock's, Finder's or the Window Server's) whose bounds
    /// equal a display's; Windows = a non-tool window (not the desktop or
    /// the taskbar) covering its monitor; X11 = `_NET_WM_STATE_FULLSCREEN`.
    pub fullscreen: bool,
}

/// How long an accepted activation may take to put the window in front
/// (X11, macOS; Windows waits [`ACTIVATE_WAIT_WINDOWS`]).
#[cfg_attr(not(any(target_os = "linux", target_os = "macos", test)), allow(dead_code))]
pub(crate) const ACTIVATE_WAIT: std::time::Duration = std::time::Duration::from_millis(600);
/// Windows' own wait, as it was.
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
pub(crate) const ACTIVATE_WAIT_WINDOWS: std::time::Duration = std::time::Duration::from_millis(500);

/// #EXP-1994: the end of every `activate_window` that waits for the OS (X11,
/// macOS, Windows; eDE's compositor answers once it focused the window):
/// `is_active` is read at once and every 20 ms after, until it holds
/// (`true`) or `wait` ran out (`false`: the agent's keys would go to the
/// window in front, so the call fails).
#[cfg_attr(not(any(target_os = "linux", target_os = "macos", target_os = "windows", test)), allow(dead_code))]
pub(crate) fn wait_active(wait: std::time::Duration, mut is_active: impl FnMut() -> bool) -> bool {
    let until = std::time::Instant::now() + wait;
    loop {
        if is_active() {
            return true;
        }
        if std::time::Instant::now() >= until {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Button {
    Left,
    Middle,
    Right,
}

/// #EXP-1994: the strokes that press (`down`) or release `key` with the
/// modifiers its character needs held around it (X11's Shift and AltGr,
/// Windows' Shift): a press is the modifiers in order, then the key; a
/// release is the key, then the modifiers in reverse. eDE's compositor
/// does the same for a keysym; macOS puts the Shift flag on the key's own
/// event.
#[cfg_attr(not(any(target_os = "linux", target_os = "windows", test)), allow(dead_code))]
pub(crate) fn with_modifiers<T: Copy>(modifiers: &[T], key: T, down: bool) -> Vec<(T, bool)> {
    if down {
        modifiers.iter().map(|&modifier| (modifier, true)).chain([(key, true)]).collect()
    } else {
        [(key, false)].into_iter().chain(modifiers.iter().rev().map(|&modifier| (modifier, false))).collect()
    }
}

/// #EXP-1921: the keys and buttons the person held when a lock engaged,
/// by the backend's own index (Windows a VK, macOS a keycode or a button).
/// A lock swallows all of the person's input except each one's release,
/// once: a button swallowed on its way up would stay down, and the agent's
/// next move would drag what the person was dragging. #EXP-1979: unless the
/// agent pressed or released that one itself meanwhile ([`Self::ours`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(any(target_os = "macos", target_os = "windows", test)), allow(dead_code))]
pub(crate) struct HeldAtEngage<const N: usize>(pub [bool; N]);

#[cfg_attr(not(any(target_os = "macos", target_os = "windows", test)), allow(dead_code))]
impl<const N: usize> HeldAtEngage<N> {
    pub(crate) const NONE: Self = Self([false; N]);

    /// Whether the person's press (`down`) or release of `index` passes
    /// the lock: only the first release of one held at engage.
    pub(crate) fn passes(&mut self, index: usize, down: bool) -> bool {
        match self.0.get_mut(index) {
            Some(slot) if *slot && !down => {
                *slot = false;
                true
            }
            _ => false,
        }
    }

    /// #EXP-1979: the agent's own press or release of `index`. The agent
    /// holds or let go of it now, so the person's release no longer passes:
    /// it would cut into the agent's drag or combo.
    pub(crate) fn ours(&mut self, index: usize) {
        if let Some(slot) = self.0.get_mut(index) {
            *slot = false;
        }
    }

    /// Of the slots `indexes` (the mouse buttons), the ones still held, as
    /// bits in that order ([`PERSON_BUTTONS`]).
    pub(crate) fn armed(&self, indexes: impl IntoIterator<Item = usize>) -> u32 {
        indexes.into_iter().take(32).enumerate().filter(|&(_, index)| self.0.get(index).copied().unwrap_or(false)).fold(0, |bits, (bit, _)| bits | 1 << bit)
    }
}

/// #EXP-1979: the mouse buttons the person held when their lock engaged and
/// has not let go of yet ([`HeldAtEngage::armed`]; 0 = none, or no lock).
/// The lock keeps it; while it is set the agent's pointer waits
/// ([`wait_for_person_buttons`]): moving or clicking would drag what the
/// person holds, and a release of ours would drop it.
#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
pub(crate) static PERSON_BUTTONS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// How long the agent's pointer waits for the person to let go.
pub(crate) const PERSON_BUTTON_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// What a wait for the person's buttons ended with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ButtonWait {
    /// None held (any more): the pointer may act.
    Free,
    /// The run was interrupted (Esc, Stop) meanwhile.
    Interrupted,
    /// Still held after the wait.
    Held,
}

/// #EXP-1979: before the agent's pointer acts: wait while `held` (the
/// person still holds a button they held when the lock engaged), at most
/// `wait`, unless `interrupted`.
pub(crate) fn wait_for_person_buttons(held: impl Fn() -> bool, interrupted: impl Fn() -> bool, wait: std::time::Duration) -> ButtonWait {
    let until = std::time::Instant::now() + wait;
    loop {
        if interrupted() {
            return ButtonWait::Interrupted;
        }
        if !held() {
            return ButtonWait::Free;
        }
        if std::time::Instant::now() >= until {
            return ButtonWait::Held;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// #EXP-1921: the start of a lock on its own thread, both ends. `take`
/// waits a while for the thread's answer; a thread that gets there after
/// `take` gave up must tear its lock down at once, since no guard will
/// ever release it. [`LockThread::start`] is both ends at once.
#[cfg_attr(not(any(target_os = "linux", target_os = "macos", target_os = "windows", test)), allow(dead_code))]
pub(crate) fn lock_start<T>() -> (LockReady<T>, LockStart<T>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let abandoned = std::sync::Arc::new(std::sync::Mutex::new(false));
    (LockReady { tx, abandoned: abandoned.clone() }, LockStart { rx, abandoned })
}

/// The lock thread's end of [`lock_start`].
pub(crate) struct LockReady<T> {
    tx: std::sync::mpsc::Sender<T>,
    abandoned: std::sync::Arc<std::sync::Mutex<bool>>,
}

#[cfg_attr(not(any(target_os = "linux", target_os = "macos", target_os = "windows", test)), allow(dead_code))]
impl<T> LockReady<T> {
    /// Hand `value` to `take`. `false` = `take` gave up: tear the lock down.
    pub(crate) fn send(self, value: T) -> bool {
        let abandoned = self.abandoned.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        !*abandoned && self.tx.send(value).is_ok()
    }
}

/// `take`'s end of [`lock_start`].
pub(crate) struct LockStart<T> {
    rx: std::sync::mpsc::Receiver<T>,
    abandoned: std::sync::Arc<std::sync::Mutex<bool>>,
}

#[cfg_attr(not(any(target_os = "linux", target_os = "macos", target_os = "windows", test)), allow(dead_code))]
impl<T> LockStart<T> {
    /// The thread's answer within `timeout`. On a timeout the thread hears
    /// it was abandoned; an answer sent before it heard is handed to `late`,
    /// which tears that lock down.
    pub(crate) fn wait(self, timeout: std::time::Duration, late: impl FnOnce(T)) -> Result<T, std::sync::mpsc::RecvTimeoutError> {
        let answer = self.rx.recv_timeout(timeout);
        if let Err(std::sync::mpsc::RecvTimeoutError::Timeout) = answer {
            *self.abandoned.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
            if let Ok(value) = self.rx.try_recv() {
                late(value);
            }
        }
        answer
    }
}

/// How long a lock waits for its thread to say the lock is in place.
pub(crate) const LOCK_START_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// #EXP-1994: the thread a lock lives on, every OS's (macOS' event tap,
/// Windows' hooks, X11's grabs): named `computer-use-lock`, its start
/// answered through [`lock_start`] within [`LOCK_START_WAIT`], and joined
/// when its guard drops, unless the drop runs on it (`on_esc` dropping the
/// guard: a thread never joins itself).
#[cfg_attr(not(any(target_os = "linux", target_os = "macos", target_os = "windows", test)), allow(dead_code))]
pub(crate) struct LockThread(Option<std::thread::JoinHandle<()>>);

/// Why a lock's thread did not say it started.
#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(not(any(target_os = "linux", target_os = "macos", target_os = "windows", test)), allow(dead_code))]
pub(crate) enum LockStartError {
    /// The OS started no thread (the reason).
    Spawn(String),
    /// No answer within the wait; one that comes later is torn down.
    Timeout,
    /// The thread ended without an answer.
    Died,
}

#[cfg_attr(not(any(target_os = "linux", target_os = "macos", target_os = "windows", test)), allow(dead_code))]
impl LockThread {
    /// Run `body` on a new lock thread and wait for what it hands
    /// [`LockReady::send`], at most [`LOCK_START_WAIT`]. An answer sent
    /// just as the wait gave up goes to `late`, which tears that lock down.
    pub(crate) fn start<T: Send + 'static>(body: impl FnOnce(LockReady<T>) + Send + 'static, late: impl FnOnce(T)) -> Result<(T, LockThread), LockStartError> {
        Self::start_within(LOCK_START_WAIT, body, late)
    }

    fn start_within<T: Send + 'static>(wait: std::time::Duration, body: impl FnOnce(LockReady<T>) + Send + 'static, late: impl FnOnce(T)) -> Result<(T, LockThread), LockStartError> {
        let (ready, start) = lock_start();
        let thread = std::thread::Builder::new()
            .name("computer-use-lock".into())
            .spawn(move || body(ready))
            .map_err(|err| LockStartError::Spawn(format!("lock thread: {err}")))?;
        // Without an answer the thread is left to end on its own.
        match start.wait(wait, late) {
            Ok(answer) => Ok((answer, LockThread(Some(thread)))),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(LockStartError::Timeout),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(LockStartError::Died),
        }
    }

    /// Wait for the thread to end (its lock torn down), unless this is it.
    pub(crate) fn join(&mut self) {
        if let Some(thread) = self.0.take() {
            if thread.thread().id() != std::thread::current().id() {
                let _ = thread.join();
            }
        }
    }

    /// Let the thread go without waiting for it (it never heard its stop).
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub(crate) fn detach(&mut self) {
        self.0.take();
    }
}

impl Drop for LockThread {
    fn drop(&mut self) {
        self.join();
    }
}

/// Released on drop. Dropping from the lock's own thread (inside `on_esc`)
/// must not join that thread ([`LockThread::join`]).
pub(crate) trait LockGuard: Send {}

/// #EXP-1994: a lock's decision on one of the person's events (macOS' tap,
/// Windows' hooks; X11's grab takes whole devices).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
pub(crate) enum Verdict {
    /// On to its window: ours, or the one release of what the person held
    /// at engage ([`HeldAtEngage`]).
    Pass,
    Swallow,
    /// The person's Esc: swallowed, and the lock ends.
    Esc,
}

/// What the clipboard held before a `paste_text` / `copy_selection` took
/// it: every format the OS let us read, as (the OS's name of the format,
/// its bytes), in the OS's order. Each backend reads back only its own
/// names (X11 atom names, Wayland mime types, macOS pasteboard types,
/// Windows `CF:<n>` or a registered name). Empty = the clipboard was empty
/// (restoring clears it). Never logged: it is the person's clipboard.
#[derive(Default)]
pub(crate) struct SavedClipboard {
    pub formats: Vec<(String, Vec<u8>)>,
    /// The clipboard held more than `formats` (#EXP-1900): a format too
    /// large or one that could not be read was left out, so restoring this
    /// would lose it. The tool layer (`tools.rs`) then refuses before it
    /// touches the clipboard; a backend only reports.
    pub partial: bool,
}

/// Whether the focused window took the text a paste offered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pasted {
    /// The OS saw the focused window read it (X11, eDE).
    Taken,
    /// The OS cannot tell (macOS, Windows): a fixed settle passed.
    #[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
    Settled,
    /// Nobody took it within the timeout (or another program took the
    /// clipboard first).
    Missed,
}

/// #EXP-1994: the most of the person's clipboard a save keeps, every format
/// together (macOS, Windows, X11; eDE's compositor caps its own snapshot,
/// `ede_protocol::CLIPBOARD_TOTAL_MAX`). A format past it is left out: the
/// save is partial.
pub(crate) const CLIPBOARD_SAVE_MAX: usize = 32 << 20;

/// Where the OS cannot tell when the focused window read the clipboard
/// (macOS, Windows): how long a paste settles after its key.
pub(crate) const PASTE_SETTLE: std::time::Duration = std::time::Duration::from_millis(400);

/// #EXP-1994: [`Clipboard::wait_pasted`] where the OS cannot tell (macOS,
/// Windows): after the settle (at most `timeout`), `Settled` while the
/// clipboard's change count (`now`) is still the one our `set_text` left
/// (`set`); another program's copy since, or no `set_text` at all, is
/// `Missed`.
#[cfg_attr(not(any(target_os = "macos", target_os = "windows", test)), allow(dead_code))]
pub(crate) fn settle_paste(timeout: std::time::Duration, set: Option<u64>, now: impl FnOnce() -> u64) -> Pasted {
    std::thread::sleep(timeout.min(PASTE_SETTLE));
    match set {
        Some(set) if now() == set => Pasted::Settled,
        _ => Pasted::Missed,
    }
}

/// The clipboard, for the agent's own copy and paste only (`tools.rs`):
/// never an ambient read of the person's clipboard.
pub(crate) trait Clipboard {
    /// A number that moves on every change of the clipboard (macOS
    /// `changeCount`, Windows `GetClipboardSequenceNumber`, X11 an XFixes
    /// owner-change count, eDE the compositor's selection serial).
    fn change_count(&self) -> Result<u64, String>;
    /// Everything the clipboard holds now, to [`Clipboard::restore`] later;
    /// a format that cannot be read (too large, an OS handle) is left out
    /// and the save says so ([`SavedClipboard::partial`]).
    fn save(&self) -> Result<SavedClipboard, String>;
    /// Put `text` on the clipboard. `sensitive` = a secret: marked so
    /// clipboard managers and the OS history skip it (X11: served only to
    /// the focused window's client + `x-kde-passwordManagerHint: secret`;
    /// macOS `org.nspasteboard.ConcealedType`; Windows
    /// `ExcludeClipboardContentFromMonitorProcessing` +
    /// `CanIncludeInClipboardHistory` = 0).
    fn set_text(&self, text: &str, sensitive: bool) -> Result<(), String>;
    /// After the paste key: wait until the focused window took the text, at
    /// most `timeout`.
    fn wait_pasted(&self, timeout: std::time::Duration) -> Pasted;
    /// Put `saved` back (empty = clear the clipboard).
    fn restore(&self, saved: SavedClipboard) -> Result<(), String>;
    /// The clipboard's text now (`None` = no text on it).
    fn text(&self) -> Result<Option<String>, String>;
}

/// Whose screen and input a run drives (#EXP-1924): the person's seat, or
/// an agent space of the run's own (#EDE-126, eDE). Every lock, Esc set,
/// call queue, Work screen pick and banner is one target's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum Target {
    /// The person's own seat: their screen, pointer and keyboard.
    #[default]
    Person,
    /// The run's own agent space: a number of this process's, one per run
    /// (`Hosts::admit_space`). Never the person's screen or input.
    Space(u32),
}

/// How long a typed character's key stays down ([`type_with`]).
pub(crate) const CHAR_HOLD: std::time::Duration = std::time::Duration::from_millis(6);

/// #EXP-1994: typing one character, every backend's: `stroke`'s press,
/// [`CHAR_HOLD`], its release. A press that failed is not released.
pub(crate) fn type_with(stroke: impl Fn(bool) -> Result<(), String>) -> Result<(), String> {
    stroke(true)?;
    std::thread::sleep(CHAR_HOLD);
    stroke(false)
}

pub(crate) trait Backend: Send {
    /// The target this backend drives ([`open`]'s). A backend for another
    /// target than the person's must override it.
    fn target(&self) -> Target {
        Target::Person
    }
    /// Whether what this backend holds down is let go without it: `true`
    /// where the compositor releases a run's keys and buttons itself once
    /// its hold or connection ends (eDE), so a release it refuses leaves
    /// nothing pressed (#ERR-29).
    fn lets_go_itself(&self) -> bool {
        false
    }
    /// Every display, the primary marked; never empty.
    fn displays(&self) -> Result<Vec<DisplayInfo>, String>;
    /// `display`'s pixels now; any resolution, the frame fits it.
    fn screenshot(&self, display: &DisplayInfo) -> Result<Image, String>;
    fn cursor_position(&self) -> Result<(i32, i32), String>;
    /// Top first, the focused one `active`, each marked `fullscreen` (one
    /// window-list read: the work-screen gate reads it on every call).
    fn list_windows(&self) -> Result<Vec<WindowInfo>, String>;
    /// Raise + focus `id` (a `WindowInfo::id`).
    fn activate_window(&self, id: u64) -> Result<(), String>;
    fn mouse_move(&self, x: i32, y: i32) -> Result<(), String>;
    fn mouse_button(&self, button: Button, down: bool) -> Result<(), String>;
    /// Wheel notches at the pointer: `dy > 0` scrolls the content down (the
    /// wheel towards the person), `dx > 0` scrolls right.
    fn scroll(&self, dx: i32, dy: i32) -> Result<(), String>;
    /// Press or release one key. A character that needs Shift on this
    /// layout gets it from the backend.
    fn key(&self, key: Key, down: bool) -> Result<(), String>;
    /// Press or release one character, whatever the layout: a half of
    /// [`Backend::type_char`].
    fn char_stroke(&self, ch: char, down: bool) -> Result<(), String>;
    /// Type one character (press + release), whatever the layout.
    fn type_char(&self, ch: char) -> Result<(), String> {
        type_with(|down| self.char_stroke(ch, down))
    }
    /// #EXP-1979: whether the person still holds a mouse button they held
    /// when the lock engaged ([`PERSON_BUTTONS`]).
    fn person_holds_button(&self) -> bool {
        false
    }
    /// Take the person's keyboard and pointer: nothing they do reaches any
    /// window, our own input still lands; `on_esc` fires on their Esc.
    /// `Err` = the OS refused (another grab, a missing permission).
    fn lock(&self, on_esc: Box<dyn Fn() + Send + Sync>) -> Result<Box<dyn LockGuard>, String>;
    /// #EDE-126: start `argv` in the run's agent space (`launch_app`): its
    /// pid. The desktop decides the environment. Only a space backend can.
    fn launch(&self, _argv: &[String], _cwd: Option<&str>) -> Result<u32, String> {
        Err("Only a run in an agent space starts programs.".to_string())
    }
    /// The clipboard (`paste_text`, `copy_selection`, `read_clipboard`).
    fn clipboard(&self) -> Result<Box<dyn Clipboard + '_>, String> {
        Err("This desktop has no clipboard computer use can drive.".to_string())
    }
}

/// #ERR-29: the input lock lost its hold on the person's input (its
/// connection gone, a release that never reached its thread): an error, a
/// GlitchTip event, its text fixed so they group; the detail (the OS
/// error, nothing of the person's) in the warning before it.
#[cfg_attr(not(any(target_os = "linux", target_os = "windows")), allow(dead_code))]
pub(crate) fn lock_lost(message: &'static str, detail: &dyn std::fmt::Display) {
    log::warn!("{message}: {detail}");
    log::error!("{message}");
}

/// Why this machine cannot drive anything, if so (no display, Wayland):
/// read without connecting (the doctor line).
pub(crate) fn unsupported() -> Option<String> {
    #[cfg(target_os = "linux")]
    return match ede::session() {
        Some(session) => ede::unsupported(&session),
        None => linux::unsupported(),
    };
    #[cfg(target_os = "macos")]
    return macos::unsupported();
    #[cfg(target_os = "windows")]
    return windows::unsupported();
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    Some("Computer use is not available on this OS.".to_string())
}

/// Every OS permission computer use needs here and whether it is granted,
/// read WITHOUT prompting.
pub(crate) fn permissions() -> Vec<(crate::Permission, bool)> {
    #[cfg(target_os = "macos")]
    return macos::permissions();
    #[cfg(not(target_os = "macos"))]
    Vec::new()
}

/// Ask the OS for every permission NOW (macOS: Screen Recording +
/// Accessibility dialogs); returns the state afterwards.
pub(crate) fn request_permissions() -> Vec<(crate::Permission, bool)> {
    #[cfg(target_os = "macos")]
    return macos::request_permissions();
    #[cfg(not(target_os = "macos"))]
    Vec::new()
}

/// NEW-58: the app id of process `pid` where the OS names it cheaply (the
/// macOS bundle id); `None` elsewhere.
pub(crate) fn app_id(pid: u32) -> Option<String> {
    #[cfg(target_os = "macos")]
    return macos::app_id(pid);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = pid;
        None
    }
}

/// NEW-59: the native pixel size of the display at this rect (screen
/// units: macOS points, eDE logical units); the rect's own size where the
/// OS already counts pixels.
pub(crate) fn pixel_size(x: i32, y: i32, width: u32, height: u32) -> (u32, u32) {
    #[cfg(target_os = "macos")]
    return macos::pixel_size(x, y, width, height);
    #[cfg(target_os = "linux")]
    return ede::pixel_size(x, y, width, height).unwrap_or((width, height));
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (x, y);
        (width, height)
    }
}

/// [`crate::before_exit`].
pub(crate) fn before_exit() {
    #[cfg(target_os = "linux")]
    if ede::session().is_none() {
        linux::before_exit();
    }
}

/// #EXP-1925: `run` ended: what was granted at the desktop for it ends
/// (eDE).
pub(crate) fn end_run(target: Target, run: &str) {
    // A space run's grant ends the same way, and its space with it.
    let _ = target;
    #[cfg(target_os = "linux")]
    ede::end_run(run);
    #[cfg(not(target_os = "linux"))]
    let _ = run;
}

/// #EDE-126: can a run get an agent space here? `Err` = why not (not eDE,
/// or an eDE that makes none).
pub(crate) fn spaces_offered() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    if let Some(session) = ede::session() {
        return ede::spaces_offered(&session?);
    }
    Err("Agent spaces need Exponential Desktop.".to_string())
}

/// The switch turned off: a grant minted at the desktop ends (eDE).
pub(crate) fn revoke_grant() {
    #[cfg(target_os = "linux")]
    ede::revoke_grant();
}

/// [`crate::screen_layout`]: X11 only, where the toolkit's display is the
/// whole screen.
#[cfg(target_os = "linux")]
pub(crate) fn screen_layout() -> Option<crate::ScreenLayout> {
    if ede::session().is_some() {
        return None;
    }
    linux::screen_layout()
}

/// What drives this machine's screen: `eDE`, `X11`, `macOS`, `Windows`.
pub(crate) fn name() -> &'static str {
    #[cfg(target_os = "linux")]
    return if ede::session().is_some() { "eDE" } else { "X11" };
    #[cfg(target_os = "macos")]
    return "macOS";
    #[cfg(target_os = "windows")]
    return "Windows";
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    "none"
}

/// Open the backend driving `target` for one call of `run` (`None` = no
/// run's: the doctor, the screen state, the banner).
pub(crate) fn open(target: Target, run: Option<&str>) -> Result<Box<dyn Backend>, String> {
    if let Some(reason) = unsupported() {
        return Err(reason);
    }
    #[cfg(target_os = "linux")]
    if let Some(session) = ede::session() {
        return ede::open_as(&session?, run, target);
    }
    // Only eDE makes spaces; a space run never drives the person's seat.
    if target != Target::Person {
        return Err("Agent spaces need Exponential Desktop.".to_string());
    }
    #[cfg(target_os = "linux")]
    return linux::open();
    #[cfg(not(target_os = "linux"))]
    let _ = run;
    #[cfg(target_os = "macos")]
    return macos::open();
    #[cfg(target_os = "windows")]
    return windows::open();
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    Err("Computer use is not available on this OS.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display(id: &str) -> DisplayInfo {
        DisplayInfo { id: id.into(), x: 0, y: 0, width: 100, height: 100, primary: false, name: String::new(), connector: None }
    }

    /// #FEED-161: without connectors twins get `#2`, `#3` in the OS's
    /// order; every id fits 128 chars.
    #[test]
    fn display_ids_are_unique_and_at_most_128_chars() {
        let long = "x".repeat(200);
        let mut displays = [display("edid:a"), display("edid:a"), display("output:DP-1"), display("edid:a"), display(&long), display(&long)];
        finish_ids(&mut displays, &[]);
        let ids: Vec<&str> = displays.iter().map(|display| display.id.as_str()).collect();
        assert_eq!(&ids[..4], ["edid:a", "edid:a#2", "output:DP-1", "edid:a#3"]);
        assert_eq!(ids[4], "x".repeat(128));
        assert_eq!(ids[5], format!("{}#2", "x".repeat(126)));

        let mut displays = [display(&long), display(&long)];
        finish_ids(&mut displays, &[Some("HDMI-1".into()), Some("DP-1".into())]);
        assert_eq!(displays[0].id, format!("{}@HDMI-1", "x".repeat(121)));
        assert_eq!(displays[1].id, format!("{}@DP-1", "x".repeat(123)));
        assert!(displays.iter().all(|display| display.id.chars().count() == 128));
    }

    /// #EXP-1994: a typed character is its press, a hold, its release, on
    /// every backend; a press that failed is never released.
    #[test]
    fn a_typed_character_is_press_hold_release() {
        let strokes = std::cell::RefCell::new(Vec::new());
        let started = std::time::Instant::now();
        let stroke = |down| {
            strokes.borrow_mut().push((down, started.elapsed()));
            Ok(())
        };
        assert_eq!(type_with(stroke), Ok(()));
        let strokes = strokes.into_inner();
        assert_eq!(strokes.iter().map(|(down, _)| *down).collect::<Vec<_>>(), [true, false]);
        assert!(strokes[1].1 - strokes[0].1 >= CHAR_HOLD);

        let tried = std::cell::RefCell::new(Vec::new());
        let refused = type_with(|down| {
            tried.borrow_mut().push(down);
            Err("no key".to_string())
        });
        assert_eq!((refused, tried.into_inner()), (Err("no key".to_string()), vec![true]));
    }

    /// #FEED-164: `work-screen.json` `idCases`, and a reorder of twins keeps
    /// each one's id.
    #[test]
    fn twins_are_told_apart_by_connector() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!("../../../../../../packages/domain-contract/fixtures/work-screen.json")).unwrap();
        for case in fixture["idCases"].as_array().unwrap() {
            let rows = case["displays"].as_array().unwrap();
            let mut displays: Vec<DisplayInfo> = rows.iter().map(|row| display(row["id"].as_str().unwrap())).collect();
            let connectors: Vec<Option<String>> = rows.iter().map(|row| row["connector"].as_str().map(str::to_string)).collect();
            finish_ids(&mut displays, &connectors);
            let ids: Vec<&str> = displays.iter().map(|display| display.id.as_str()).collect();
            let expect: Vec<&str> = case["expect"].as_array().unwrap().iter().map(|id| id.as_str().unwrap()).collect();
            assert_eq!(ids, expect, "{}", case["name"]);
        }
        let mut before = [display("uuid"), display("uuid")];
        finish_ids(&mut before, &[Some("1".into()), Some("2".into())]);
        let mut after = [display("uuid"), display("uuid")];
        finish_ids(&mut after, &[Some("2".into()), Some("1".into())]);
        assert_eq!((before[0].id.as_str(), before[1].id.as_str()), (after[1].id.as_str(), after[0].id.as_str()));
    }

    /// #EXP-1994: every backend's display list ends the same way: none is
    /// `None`, the OS's primary stays, nobody primary makes the first one,
    /// and twins carry their connector.
    #[test]
    fn every_display_list_ends_alike() {
        assert_eq!(finish_displays(Vec::new(), &[]), None);
        let primary = |displays: &[DisplayInfo]| displays.iter().map(|display| display.primary).collect::<Vec<_>>();
        let second = vec![display("a"), DisplayInfo { primary: true, ..display("b") }];
        assert_eq!(primary(&finish_displays(second, &[]).unwrap()), [false, true], "the OS's primary stays");
        let twins = finish_displays(vec![display("uuid"), display("uuid")], &[Some("1".into()), Some("2".into())]).unwrap();
        assert_eq!(primary(&twins), [true, false], "nobody primary: the first");
        assert_eq!(twins.iter().map(|display| (display.id.as_str(), display.connector.as_deref())).collect::<Vec<_>>(), [("uuid@1", Some("1")), ("uuid@2", Some("2"))]);
    }

    #[test]
    fn what_was_held_at_engage_passes_its_release_once() {
        let mut held = HeldAtEngage::<4>::NONE;
        held.0[1] = true;
        assert!(!held.passes(1, true), "a press never passes");
        assert!(held.passes(1, false));
        assert!(!held.passes(1, false), "only once");
        assert!(!held.passes(2, false), "not held at engage");
        assert!(!held.passes(9, false), "out of range");
        held.0[2] = true;
        held.ours(2);
        assert!(!held.passes(2, false), "the agent pressed or released it since");
        held.ours(9);
    }

    /// #EXP-1994: a shifted character's key goes inside its modifiers on
    /// every backend: pressed after them, released before them.
    #[test]
    fn a_key_is_pressed_inside_its_modifiers() {
        assert_eq!(with_modifiers(&["shift", "altgr"], "q", true), [("shift", true), ("altgr", true), ("q", true)]);
        assert_eq!(with_modifiers(&["shift", "altgr"], "q", false), [("q", false), ("altgr", false), ("shift", false)]);
        assert_eq!(with_modifiers(&[], 'a', true), [('a', true)]);
        assert_eq!(with_modifiers(&[], 'a', false), [('a', false)]);
    }

    /// #EXP-1979: the pointer waits while the person holds a button, frees
    /// on its release, gives up after the wait, and an interrupt ends it.
    #[test]
    fn the_pointer_waits_for_the_persons_buttons_at_most_the_wait() {
        let mut held = HeldAtEngage::<8>::NONE;
        held.0[1] = true;
        held.0[5] = true;
        assert_eq!(held.armed([1, 2, 5]), 0b101);
        held.ours(5);
        assert_eq!(held.armed([1, 2, 5]), 0b001);
        let short = std::time::Duration::from_millis(60);
        assert_eq!(wait_for_person_buttons(|| true, || false, short), ButtonWait::Held);
        assert_eq!(wait_for_person_buttons(|| false, || false, short), ButtonWait::Free);
        assert_eq!(wait_for_person_buttons(|| true, || true, short), ButtonWait::Interrupted);
        let polls = std::cell::Cell::new(0);
        let held_twice = || {
            polls.set(polls.get() + 1);
            polls.get() <= 2
        };
        assert_eq!(wait_for_person_buttons(held_twice, || false, std::time::Duration::from_secs(5)), ButtonWait::Free);
    }

    /// #EXP-1994: an activation counts once the window is active, read at
    /// once and again until the wait runs out, on every OS that waits.
    #[test]
    fn an_activation_waits_until_the_window_is_active_at_most_the_wait() {
        let reads = std::cell::Cell::new(0);
        assert!(wait_active(std::time::Duration::ZERO, || {
            reads.set(reads.get() + 1);
            true
        }));
        assert_eq!(reads.get(), 1, "read at once, even without a wait");
        let started = std::time::Instant::now();
        assert!(!wait_active(std::time::Duration::from_millis(60), || false));
        assert!(started.elapsed() >= std::time::Duration::from_millis(60));
        reads.set(0);
        let third = || {
            reads.set(reads.get() + 1);
            reads.get() >= 3
        };
        assert!(wait_active(std::time::Duration::from_secs(5), third));
        assert_eq!(reads.get(), 3);
        assert!(ACTIVATE_WAIT > ACTIVATE_WAIT_WINDOWS);
    }

    /// #EXP-1921: a lock thread that answers after `take` gave up hears so
    /// and tears its lock down; one that answers in time is taken.
    #[test]
    fn a_lock_that_started_after_take_gave_up_is_torn_down() {
        let (ready, start) = lock_start::<u32>();
        let (sent_tx, sent_rx) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            sent_tx.send(ready.send(7)).unwrap();
        });
        let mut late = None;
        let answer = start.wait(std::time::Duration::from_millis(20), |value| late = Some(value));
        assert_eq!(answer, Err(std::sync::mpsc::RecvTimeoutError::Timeout));
        assert!(!sent_rx.recv().unwrap(), "the late thread must tear down");
        assert_eq!(late, None);
        thread.join().unwrap();

        let (ready, start) = lock_start::<u32>();
        assert!(ready.send(7));
        assert_eq!(start.wait(std::time::Duration::from_secs(5), |_| panic!("not late")), Ok(7));
    }

    /// #EXP-1994: every OS's lock thread: named, its answer taken, one that
    /// never comes or comes late refused (the late one hears it), joined on
    /// the guard's drop, and never joined by itself.
    #[test]
    fn a_lock_thread_answers_once_and_is_joined_unless_it_drops_itself() {
        use std::sync::mpsc::channel;
        let (answer, thread) = LockThread::start(|ready| {
            ready.send(std::thread::current().name().map(str::to_string));
        }, |_| panic!("not late"))
        .unwrap();
        assert_eq!(answer.as_deref(), Some("computer-use-lock"));
        drop(thread);

        assert_eq!(LockThread::start(|_: LockReady<()>| {}, |_| panic!("not late")).err(), Some(LockStartError::Died));

        let (heard_tx, heard) = channel();
        let short = std::time::Duration::from_millis(20);
        let late = LockThread::start_within(short, move |ready| {
            std::thread::sleep(std::time::Duration::from_millis(200));
            heard_tx.send(ready.send(())).unwrap();
        }, |_| {});
        assert_eq!(late.err(), Some(LockStartError::Timeout));
        assert!(!heard.recv().unwrap(), "the late thread hears it was given up and tears down");

        // The guard's drop waits for the thread to end its lock.
        let (stop_tx, stop) = channel::<()>();
        let (ended_tx, ended) = channel();
        let ((), thread) = LockThread::start(move |ready| {
            ready.send(());
            let _ = stop.recv();
            std::thread::sleep(std::time::Duration::from_millis(50));
            ended_tx.send(()).unwrap();
        }, |_| panic!("not late"))
        .unwrap();
        stop_tx.send(()).unwrap();
        drop(thread);
        assert!(ended.try_recv().is_ok(), "joined: the thread had ended");

        // `on_esc` drops the guard on the lock's own thread: no self-join.
        let (guard_tx, guard) = channel::<LockThread>();
        let (dropped_tx, dropped) = channel();
        let ((), thread) = LockThread::start(move |ready| {
            ready.send(());
            drop(guard.recv().unwrap());
            dropped_tx.send(()).unwrap();
        }, |_| panic!("not late"))
        .unwrap();
        guard_tx.send(thread).unwrap();
        dropped.recv_timeout(std::time::Duration::from_secs(5)).expect("the thread dropped its own guard and went on");
    }

    #[test]
    fn the_overlap_is_the_shared_area() {
        let display = DisplayInfo { x: 100, ..display("a") };
        assert_eq!(display.overlap(150, 50, 100, 100), 50 * 50);
        assert_eq!(display.overlap(0, 0, 100, 100), 0);
        assert_eq!(display.overlap(-1000, -1000, 5000, 5000), 100 * 100);
    }

    /// #EXP-1994: every backend's capture becomes RGBA8 by one rule: opaque,
    /// the row padding skipped, a short capture `None` whatever the backend.
    #[test]
    fn bgra_rows_become_opaque_rgba_and_skip_the_row_padding() {
        // 2x2, stride 12 (4 bytes of padding per row).
        let data = [1, 2, 3, 0, 4, 5, 6, 9, 0xee, 0xee, 0xee, 0xee, 7, 8, 9, 0, 10, 11, 12, 0, 0xee, 0xee, 0xee, 0xee];
        let image = Image::from_bgra(&data, 2, 2, 12).unwrap();
        assert_eq!((image.width, image.height), (2, 2));
        assert_eq!(image.rgba, [3, 2, 1, 255, 6, 5, 4, 255, 9, 8, 7, 255, 12, 11, 10, 255]);
        // The last row needs no padding: 20 bytes hold both rows, 19 do not.
        assert!(Image::from_bgra(&data[..20], 2, 2, 12).is_some());
        assert!(Image::from_bgra(&data[..19], 2, 2, 12).is_none());
        // Packed rows (Windows, X11): the alpha byte never shows.
        assert_eq!(Image::from_bgra(&[1, 2, 3, 0, 10, 20, 30, 7], 2, 1, 8).unwrap().rgba, [3, 2, 1, 255, 30, 20, 10, 255]);
        // eDE's `argb8888` words, little-endian.
        let words: Vec<u8> = [0xff10_2030u32, 0xff00_00ff, 0].iter().flat_map(|word| word.to_le_bytes()).collect();
        assert_eq!(Image::from_bgra(&words, 2, 1, 12).unwrap().rgba, [0x10, 0x20, 0x30, 255, 0, 0, 0xff, 255]);
        assert!(Image::from_bgra(&words, 4, 1, 12).is_none(), "a row wider than its stride");
        assert!(Image::from_bgra(&[], 1, 1, 4).is_none());
        assert_eq!(Image::from_bgra(&[], 0, 0, 0).unwrap().rgba, Vec::<u8>::new());
    }

    #[test]
    fn the_stable_hash_is_fnv1a() {
        assert_eq!(stable_hash(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(stable_hash(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    /// #EXP-1994: where the OS cannot see the paste (macOS, Windows) it has
    /// settled while the clipboard still holds our text; another copy since,
    /// or nothing set, missed; the settle never outlasts the timeout.
    #[test]
    fn a_paste_settles_while_the_clipboard_is_still_ours() {
        let no_wait = std::time::Duration::ZERO;
        assert_eq!(settle_paste(no_wait, Some(7), || 7), Pasted::Settled);
        assert_eq!(settle_paste(no_wait, Some(7), || 8), Pasted::Missed, "another program copied since");
        assert_eq!(settle_paste(no_wait, None, || panic!("nothing set: nothing to read")), Pasted::Missed);
        let started = std::time::Instant::now();
        settle_paste(std::time::Duration::from_millis(20), Some(1), || 1);
        let settled = started.elapsed();
        assert!(settled >= std::time::Duration::from_millis(20) && settled < PASTE_SETTLE, "{settled:?}");
    }
}
