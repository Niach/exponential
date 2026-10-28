//! NEW-68 (voice plan v3, V2): the IDE in the system tray
//! (`fixtures/voice.json` `trayMenu`).
//!
//! The tray shows ONLY while this IDE is the member's voice device. Its menu
//! is OS-native: on Linux our own StatusNotifierItem + dbusmenu over zbus
//! ([`linux`]; no GTK, and not `ksni`, whose Unlicense is off deny.toml's
//! allow-list; GNOME without the AppIndicator extension shows nothing),
//! `tray-icon` + `muda` on macOS and Windows (created on gpui's main thread).
//! The rows come from [`domain::voice::tray_menu`]; a toggle writes
//! `voice.update` optimistically like Settings → Voice (the menu flips at
//! once, the stored value goes to `speak_replies::set_settings`, a refusal
//! reverts and toasts). Mute microphone is device-local (#NEW-86: kept in
//! `{data_dir}/voice-tray.json` across restarts, never synced): it shows while a
//! bound key or the wake word listens here and the speech worker is up
//! (`micAvailable`), and push to talk never opens the microphone while it is
//! on (#NEW-54). S-3: Push to talk · {key} shows while a key is bound and
//! opens Settings → Voice (no toggle; a shortcut that can't be held
//! system-wide says so there); while a call runs here Call reads End call.
//! While the tray is up, closing the last window keeps the IDE running
//! ([`keeps_running`], read by the shell) so the speech worker stays resident.
//! #NEW-56: while the wake word holds the microphone open the icon carries a
//! dot (`domain::voice::tray_icon`), and Mute takes the wake word's
//! microphone away too.
//! #NEW-97: while no tray host shows the item (GNOME without AppIndicator)
//! the rail's footer carries the same rows ([`rail`], `domain::voice::rail_voice`),
//! clicked through [`click`] = the tray's own handlers.
//! NEW-58: the Screen memory switch (`fixtures/screen-memory.json`): device-
//! local and kept like Mute, default off; while on and this IDE is the voice
//! device the `screen` crate captures ([`coding::screen::start`]); a sign-out,
//! another account or another voice device turns it off and deletes the
//! buffer; a failed start toasts and flips it back off.
//! NEW-57 (`fixtures/meeting.json` TRAY): Meeting mode after Wake word
//! (checked while a meeting runs, its who after the label); checking it
//! opens an IDE window with THE consent dialog, unchecking stops at once.
//! Pause listening · Ctrl + Alt + P shows while a meeting runs or screen
//! memory is on, and the same keys are grabbed then
//! (`meeting_mode::sync_pause_hotkey`).

use gpui::{App, AppContext as _, Context, Entity, Global};

use api::voice::VoiceUpdate;
use coding::screen;
use domain::voice::{
    rail_voice, tray_icon, tray_menu, MeetingTrayState, RailVoice, ScreenMemoryState, TrayContext, TrayIcon, TrayItem, TrayItemId,
    VoiceSettings,
};

/// The app icon (the one `desktop_integration` installs on Linux).
const ICON_SVG: &[u8] = include_bytes!("../../../../assets/icons/logo-white.svg");

/// The tray's title / tooltip.
const TITLE: &str = "Exponential";

/// One rasterized icon frame, straight (not premultiplied) RGBA.
struct IconFrame {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// The icon as the platform draws it. Windows: the white disc gains a dark
/// ring, so it reads on a light taskbar as well as on the default dark one
/// (macOS tints its template image itself; Linux panels are dark by default,
/// like the hicolor icon `desktop_integration` installs). #NEW-56: `Listening`
/// adds the always-on microphone dot. #FEED-163: `Recording` adds the red
/// record dot in the same corner; macOS draws it NOT templated (a template
/// is monochrome), so there the disc carries the dark ring too and reads on
/// a light menu bar.
fn icon_svg(icon: TrayIcon) -> std::borrow::Cow<'static, [u8]> {
    let ring = cfg!(target_os = "windows") || (cfg!(target_os = "macos") && icon == TrayIcon::Recording);
    let svg = if ring { std::borrow::Cow::Owned(ringed(ICON_SVG)) } else { std::borrow::Cow::Borrowed(ICON_SVG) };
    match icon {
        TrayIcon::App => svg,
        TrayIcon::Listening => std::borrow::Cow::Owned(dotted(&svg)),
        TrayIcon::Recording => std::borrow::Cow::Owned(recording(&svg)),
    }
}

/// #FEED-163: whether the OS may tint `icon` as a template image (macOS):
/// everything but the red record dot.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn templated(icon: TrayIcon) -> bool {
    icon != TrayIcon::Recording
}

/// The dark that cuts the dots (and the Windows ring) out of the disc.
const CUT: &str = "#27272a";

/// A design token as an SVG colour.
fn svg_color(color: theme::Srgb8) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b)
}

/// The icon with the microphone dot in its lower right corner: the
/// design tokens' `destructive` red, cut out of the disc by a dark ring.
fn dotted(svg: &[u8]) -> Vec<u8> {
    let red = svg_color(theme::tokens::DESTRUCTIVE);
    String::from_utf8_lossy(svg)
        .replace(
            "</svg>",
            &format!("  <circle cx=\"86\" cy=\"86\" r=\"21\" fill=\"{red}\" stroke=\"{CUT}\" stroke-width=\"7\"></circle>\n</svg>"),
        )
        .into_bytes()
}

/// #FEED-163: the icon with the record dot in the same corner, distinct
/// from the microphone dot: a red ring around a red core (⏺), on a larger
/// dark cut-out.
fn recording(svg: &[u8]) -> Vec<u8> {
    let red = svg_color(theme::tokens::DESTRUCTIVE);
    String::from_utf8_lossy(svg)
        .replace(
            "</svg>",
            &format!(
                "  <circle cx=\"84\" cy=\"84\" r=\"28\" fill=\"{CUT}\"></circle>\n  <circle cx=\"84\" cy=\"84\" r=\"21\" fill=\"none\" stroke=\"{red}\" stroke-width=\"6\"></circle>\n  <circle cx=\"84\" cy=\"84\" r=\"12\" fill=\"{red}\"></circle>\n</svg>"
            ),
        )
        .into_bytes()
}

/// The icon with a dark ring around its disc.
fn ringed(svg: &[u8]) -> Vec<u8> {
    String::from_utf8_lossy(svg)
        .replace(
            "</svg>",
            "  <circle cx=\"50\" cy=\"50\" r=\"48\" fill=\"none\" stroke=\"#27272a\" stroke-width=\"5\"></circle>\n</svg>",
        )
        .into_bytes()
}

fn icon_frames(icon: TrayIcon, sizes: &[u32]) -> Vec<IconFrame> {
    let Ok(tree) = resvg::usvg::Tree::from_data(&icon_svg(icon), &resvg::usvg::Options::default()) else {
        return Vec::new();
    };
    sizes
        .iter()
        .filter_map(|&size| {
            let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)?;
            let transform =
                resvg::tiny_skia::Transform::from_scale(size as f32 / tree.size().width(), size as f32 / tree.size().height());
            resvg::render(&tree, transform, &mut pixmap.as_mut());
            let rgba = pixmap
                .pixels()
                .iter()
                .flat_map(|pixel| {
                    let c = pixel.demultiply();
                    [c.red(), c.green(), c.blue(), c.alpha()]
                })
                .collect();
            Some(IconFrame { width: size, height: size, rgba })
        })
        .collect()
}

/// What the OS reports back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TrayEvent {
    /// A menu entry was clicked.
    Item(TrayItemId),
    /// The icon itself was clicked (Linux `Activate`, a Windows left
    /// click): bring the IDE up.
    Activate,
    /// The tray host went away (Linux): with no window open, bring one back
    /// so the IDE never hides with no way in.
    HostLost,
    /// #NEW-97: whether a host shows the item now (Linux; every answer).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    Hosted(bool),
    /// NEW-58: screen memory went off, active or paused.
    Screen(screen::Status),
}

/// #NEW-76: two activations closer than this are one (a Windows double-click
/// sends a click and a double-click; the IDE opened twice).
const ACTIVATE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(500);

/// #NEW-76: whether an activation at `now` counts (and remember it if so).
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn activation_counts(last: &mut Option<std::time::Instant>, now: std::time::Instant) -> bool {
    if last.is_some_and(|last| now.saturating_duration_since(last) < ACTIVATE_DEBOUNCE) {
        return false;
    }
    *last = Some(now);
    true
}

/// A menu entry's stable id across the OS boundary.
#[cfg_attr(target_os = "linux", allow(dead_code))]
fn item_key(id: TrayItemId) -> &'static str {
    match id {
        TrayItemId::Open => "voice-tray:open",
        TrayItemId::Call => "voice-tray:call",
        TrayItemId::EndCall => "voice-tray:end-call",
        TrayItemId::PushToTalk => "voice-tray:push-to-talk",
        TrayItemId::WakeWord => "voice-tray:wake-word",
        TrayItemId::Meeting => "voice-tray:meeting",
        TrayItemId::Pause => "voice-tray:pause",
        TrayItemId::ScreenMemory => "voice-tray:screen-memory",
        TrayItemId::Mute => "voice-tray:mute",
        TrayItemId::Settings => "voice-tray:settings",
        TrayItemId::Quit => "voice-tray:quit",
    }
}

#[cfg_attr(target_os = "linux", allow(dead_code))]
fn item_of(key: &str) -> Option<TrayItemId> {
    [
        TrayItemId::Open,
        TrayItemId::Call,
        TrayItemId::EndCall,
        TrayItemId::PushToTalk,
        TrayItemId::WakeWord,
        TrayItemId::Meeting,
        TrayItemId::ScreenMemory,
        TrayItemId::Pause,
        TrayItemId::Mute,
        TrayItemId::Settings,
        TrayItemId::Quit,
    ]
    .into_iter()
    .find(|id| item_key(*id) == key)
}

/// #NEW-86: the device-local file Mute microphone (and NEW-58's Screen
/// memory) persists in.
const STATE_FILE: &str = "voice-tray.json";

/// #NEW-86: what the tray keeps on this device across restarts.
#[derive(Debug, Default, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct TrayState {
    #[serde(default)]
    muted: bool,
    /// NEW-58: the Screen memory switch.
    #[serde(default)]
    screen_memory: bool,
    /// NEW-58: the account that turned it on (another one turns it off).
    #[serde(default)]
    screen_account: Option<String>,
}

/// #NEW-86: the tray as this device last left it (a missing or unreadable
/// file = not muted, screen memory off).
fn load_state(data_dir: &std::path::Path) -> TrayState {
    std::fs::read(data_dir.join(STATE_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<TrayState>(&bytes).ok())
        .unwrap_or_default()
}

/// #NEW-86: keep Mute (and Screen memory) for the next start.
fn save_state(data_dir: &std::path::Path, state: &TrayState) {
    let written = serde_json::to_vec(state)
        .map_err(std::io::Error::other)
        .and_then(|bytes| std::fs::write(data_dir.join(STATE_FILE), bytes));
    if let Err(err) = written {
        log::warn!("[ui] voice tray: cannot keep its switches: {err}");
    }
}

/// NEW-58: what the tray asks of the `screen` crate. They join threads and
/// touch the disk, so ONE background task runs them, in the order asked: a
/// start never overtakes the stop before it. #FEED-162: a start runs on its
/// own thread (the Linux portal's dialog may hold it), so a later stop or
/// delete does not wait behind it: it cancels it ([`screen::ticket`]).
#[derive(Clone, Debug, PartialEq, Eq)]
enum ScreenOp {
    /// Start capturing; the answer comes back with this generation.
    Start(u64),
    /// Stop; `forget` = the switch went off: delete the buffer too.
    Stop { forget: bool },
    /// Stop and delete the buffer under the data dir (a sign-out, another
    /// account or voice device, a buffer left at start-up).
    Forget,
}

/// NEW-58: the switch, the account and the voice device → what the capture
/// must do; `changed` = the switch flipped (save it, redraw).
#[derive(Debug, Default, PartialEq, Eq)]
struct ScreenStep {
    ops: Vec<ScreenOp>,
    changed: bool,
}

/// NEW-58: one sync of the Screen memory switch: a sign-out or an account
/// change deletes the buffer whether the switch is on or not (and turns it
/// off); another voice device turns it off and deletes it; turning it off
/// stops and deletes; anything else that keeps it from running (the
/// settings not known yet) stops and keeps.
fn screen_step(screen: &mut ScreenMemory, account: Option<String>, voice_device: bool, settings_known: bool) -> ScreenStep {
    let account_changed = screen.seen_account && account != screen.last_account;
    screen.seen_account |= account.is_some();
    screen.last_account = account.clone();
    let mut step = ScreenStep::default();
    let left = screen.on
        && match (&account, &screen.account) {
            (Some(now), owner) => owner.as_ref() != Some(now),
            (None, _) => screen.seen_account,
        };
    let elsewhere = screen.on && settings_known && !voice_device;
    if left || elsewhere || account_changed {
        if screen.on {
            screen.on = false;
            screen.account = None;
            step.changed = true;
        }
        screen.running = false;
        screen.generation += 1;
        step.ops.push(ScreenOp::Forget);
        return step;
    }
    let wanted = screen.available && screen.on && voice_device && account.is_some() && account == screen.account;
    if wanted == screen.running {
        return step;
    }
    screen.running = wanted;
    screen.generation += 1;
    step.ops.push(if wanted { ScreenOp::Start(screen.generation) } else { ScreenOp::Stop { forget: !screen.on } });
    step
}

/// NEW-58: the tray's Screen memory item flipped to `on` (by `account`):
/// off = stop and delete the buffer, running or not (a stop that kept it may
/// have come first); on = owned by that account (the next sync starts it).
fn switch_step(screen: &mut ScreenMemory, on: bool, account: Option<String>) -> ScreenStep {
    let mut step = ScreenStep::default();
    if on == screen.on {
        return step;
    }
    screen.on = on;
    screen.account = if on { account } else { None };
    step.changed = true;
    if !on {
        screen.running = false;
        screen.generation += 1;
        step.ops.push(ScreenOp::Forget);
    }
    step
}

/// NEW-58: a buffer left on disk goes at install while the switch is off
/// or, #FEED-164, while this machine cannot capture (Wayland: no row, no
/// start, so nothing would ever sweep it).
fn sweeps_leftover(switch_on: bool, available: bool) -> bool {
    !switch_on || !available
}

/// NEW-58: the Screen memory switch and the capture it drives.
#[derive(Debug)]
struct ScreenMemory {
    /// #FEED-162: this machine can capture ([`screen::available`]: not on
    /// Wayland, which has no window list): else no row and no start.
    available: bool,
    /// The switch (device-local, kept).
    on: bool,
    /// The account that turned it on.
    account: Option<String>,
    /// What the screen crate last reported (the Paused label).
    status: screen::Status,
    /// `screen::start` was asked for (and not stopped since).
    running: bool,
    /// Bumped on every start and stop: a slow start that lost the race
    /// stops again.
    generation: u64,
    /// An account was signed in during this process (a later None is a
    /// sign-out, not the start-up before the first sync).
    seen_account: bool,
    /// The account of the last sync (a change deletes the buffer).
    last_account: Option<String>,
}

impl ScreenMemory {
    /// The switch as this device left it; nothing running yet.
    fn new(on: bool, account: Option<String>, available: bool) -> Self {
        ScreenMemory { available, on, account, status: screen::Status::Off, running: false, generation: 0, seen_account: false, last_account: None }
    }
}

pub(crate) struct VoiceTray {
    /// Mute microphone: device-local, kept across restarts (#NEW-86).
    muted: bool,
    /// NEW-58: the Screen memory switch, kept like Mute.
    screen: ScreenMemory,
    /// NEW-58: the background task that starts, stops and deletes, in order.
    screen_ops: flume::Sender<ScreenOp>,
    /// Where Mute is kept.
    data_dir: std::path::PathBuf,
    /// The optimistic settings while a toggle's write is in flight.
    pending: Option<VoiceSettings>,
    /// The newest toggle; an older write's answer leaves `pending` alone.
    generation: u64,
    /// This IDE's device id (the voice device test).
    device_id: String,
    /// The menu the OS shows (None = no tray).
    menu: Option<Vec<TrayItem>>,
    /// #NEW-56: the icon the OS shows.
    icon: Option<TrayIcon>,
    /// #NEW-97: whether a host shows the tray (None = not known yet: no
    /// rail button before the first answer, so a host never sees it flash).
    hosted: Option<bool>,
    /// #NEW-97: the rail's voice button (None = none).
    rail: Option<RailVoice>,
    backend: backend::Backend,
}

struct VoiceTrayGlobal(Entity<VoiceTray>);

impl Global for VoiceTrayGlobal {}

/// Install the tray ONCE, after `speak_replies::install` (it follows the
/// settings that watcher holds).
pub fn install(cx: &mut App) {
    let Some(speaker) = crate::speak_replies::entity(cx) else { return };
    let data_dir = crate::session::AuthContext::global(cx).data_dir.clone();
    let device_id = steer::persistent_device_id(&data_dir);
    // #NEW-86: Mute as this device left it; push to talk and the wake word
    // hear it before they first open the microphone.
    let state = load_state(&data_dir);
    let muted = state.muted;
    let switch_on = state.screen_memory;
    let screen = ScreenMemory::new(switch_on, state.screen_account, screen::available());
    // NEW-58: the stops and deletes join threads: one background task runs
    // them in order; a start's answer comes back to the tray.
    let (screen_ops, ops_rx) = flume::unbounded::<ScreenOp>();
    let (started_tx, started_rx) = flume::unbounded::<(u64, Result<(), String>)>();
    {
        let data_dir = data_dir.clone();
        cx.background_executor()
            .spawn(async move {
                while let Ok(op) = ops_rx.recv_async().await {
                    match op {
                        ScreenOp::Start(generation) => {
                            // The ticket in queue order: a stop queued after
                            // it cancels it, even mid-dialog.
                            let (ticket, data_dir, answer) = (screen::ticket(), data_dir.clone(), started_tx.clone());
                            let spawned = std::thread::Builder::new().name("screen-memory-start".into()).spawn(move || {
                                let _ = answer.send((generation, screen::start_with(data_dir, ticket)));
                            });
                            if let Err(err) = spawned {
                                let _ = started_tx.send((generation, Err(format!("Could not start screen memory: {err}"))));
                            }
                        }
                        ScreenOp::Stop { forget } => screen::stop(forget),
                        ScreenOp::Forget => screen::forget(&data_dir),
                    }
                }
            })
            .detach();
    }
    // A buffer an earlier run left behind with the switch off goes; #FEED-164:
    // so does one on a machine that cannot capture now (Wayland hides the
    // row, so nothing else would ever sweep an earlier X11 session's frames).
    if sweeps_leftover(switch_on, screen.available) && screen::leftover(&data_dir) {
        let _ = screen_ops.send(ScreenOp::Forget);
    }
    if muted {
        crate::push_to_talk::mute_changed(true, cx);
    }
    let (tx, rx) = flume::unbounded::<TrayEvent>();
    // NEW-58: the Paused label follows the capture.
    let screen_tx = tx.clone();
    screen::set_status_observer(move |status| {
        let _ = screen_tx.send(TrayEvent::Screen(status));
    });
    let entity = cx.new(|cx: &mut Context<VoiceTray>| {
        cx.observe(&speaker, |this: &mut VoiceTray, _, cx| this.sync(cx)).detach();
        // The agent's name labels Call and the wake phrase's placeholder.
        if let Some(agents) = sync::Store::try_global(cx).map(|store| store.collections().agents.clone()) {
            cx.observe(&agents, |this: &mut VoiceTray, _, cx| this.sync(cx)).detach();
        }
        // #NEW-54: the shortcut's grab status shapes the Push to talk item.
        if let Some(push_to_talk) = crate::push_to_talk::entity(cx) {
            cx.observe(&push_to_talk, |this: &mut VoiceTray, _, cx| this.sync(cx)).detach();
        }
        // NEW-55: a call starting or ending hides or shows Call.
        if let Some(call) = crate::voice_call::entity(cx) {
            cx.observe(&call, |this: &mut VoiceTray, _, cx| this.sync(cx)).detach();
        }
        // NEW-57: the meeting host's state (its who, the pause).
        cx.observe_global::<crate::meeting::MeetingState>(|this: &mut VoiceTray, cx| this.sync(cx)).detach();
        VoiceTray { muted, screen, screen_ops, data_dir, pending: None, generation: 0, device_id, menu: None, icon: None, hosted: None, rail: None, backend: backend::Backend::new(tx) }
    });
    cx.set_global(VoiceTrayGlobal(entity.clone()));
    let weak = entity.downgrade();
    cx.spawn(async move |cx| {
        while let Ok((generation, result)) = started_rx.recv_async().await {
            let Some(entity) = weak.upgrade() else { break };
            cx.update(|cx| entity.update(cx, |this, cx| this.screen_started(generation, result, cx)));
        }
    })
    .detach();
    let weak = entity.downgrade();
    cx.spawn(async move |cx| {
        while let Ok(event) = rx.recv_async().await {
            let Some(entity) = weak.upgrade() else { break };
            cx.update(|cx| match event {
                TrayEvent::Item(id) => entity.update(cx, |this, cx| this.on_click(id, cx)),
                TrayEvent::Activate => open_ide(cx),
                TrayEvent::HostLost => {
                    if cx.windows().is_empty() {
                        open_ide(cx);
                    }
                }
                TrayEvent::Hosted(hosted) => entity.update(cx, |this, cx| this.set_hosted(Some(hosted), cx)),
                TrayEvent::Screen(status) => entity.update(cx, |this, cx| {
                    if status != this.screen.status {
                        this.screen.status = status;
                        this.sync(cx);
                    }
                }),
            });
        }
    })
    .detach();
}

fn global(cx: &App) -> Option<Entity<VoiceTray>> {
    cx.try_global::<VoiceTrayGlobal>().map(|global| global.0.clone())
}

/// The tray (Settings → Voice follows its Mute, #NEW-86).
pub(crate) fn entity(cx: &App) -> Option<Entity<VoiceTray>> {
    global(cx)
}

/// The tray is up: the last window closing keeps the IDE running.
pub(crate) fn keeps_running(cx: &App) -> bool {
    global(cx).is_some_and(|entity| {
        let tray = entity.read(cx);
        tray.menu.is_some() && tray.backend.registered()
    })
}

/// Mute microphone: push to talk never opens the microphone while it is on.
pub(crate) fn muted(cx: &App) -> bool {
    global(cx).is_some_and(|entity| entity.read(cx).muted)
}

/// #NEW-54: the overlay's Unmute.
pub(crate) fn set_muted(muted: bool, cx: &mut App) {
    if let Some(entity) = global(cx) {
        entity.update(cx, |this, cx| this.set_muted(muted, cx));
    }
}

/// #NEW-97: the rail's voice button: this IDE is the voice device, the wake
/// word is on and no tray host shows the tray.
pub(crate) fn rail(cx: &App) -> Option<RailVoice> {
    global(cx).and_then(|entity| entity.read(cx).rail.clone())
}

/// #NEW-97: a rail menu row: the tray's own handler.
pub(crate) fn click(id: TrayItemId, cx: &mut App) {
    if let Some(entity) = global(cx) {
        entity.update(cx, |this, cx| this.on_click(id, cx));
    }
}

/// Bring a shell window forward, or open one when every window is closed
/// (the tray's Open Exponential, a click on its icon, a second launch).
pub fn open_ide(cx: &mut App) {
    cx.activate(true);
    if let Some(window) = crate::os_notifications::shell_window(cx) {
        let _ = window.update(cx, |_, window, _| window.activate_window());
        return;
    }
    crate::window_hooks::request_shell_window(cx);
}

/// The voice agent's bare name (`Studio`, never the picker's `Studio ·
/// Acme`): the tray's Call item and wake phrase, the overlay's pill.
pub(crate) fn agent_label(settings: &VoiceSettings, cx: &App) -> Option<String> {
    let agent_id = settings.agent_id.as_deref()?;
    crate::settings::voice_prefs::voice_agents(cx).into_iter().find(|agent| agent.id == agent_id).map(|agent| agent.name)
}

impl VoiceTray {
    fn settings(&self, cx: &App) -> VoiceSettings {
        self.pending.clone().unwrap_or_else(|| crate::speak_replies::settings(cx))
    }

    /// Mute microphone: push to talk and the wake word let the microphone
    /// go (or take it again), and the device keeps it (#NEW-86).
    fn set_muted(&mut self, muted: bool, cx: &mut Context<Self>) {
        if muted != self.muted {
            self.muted = muted;
            self.save();
            cx.notify();
        }
        crate::push_to_talk::mute_changed(muted, cx);
        self.sync(cx);
    }

    /// #NEW-86 / NEW-58: keep the device-local switches.
    fn save(&self) {
        let state = TrayState { muted: self.muted, screen_memory: self.screen.on, screen_account: self.screen.account.clone() };
        save_state(&self.data_dir, &state);
    }

    /// NEW-58: the Screen memory item.
    fn set_screen_memory(&mut self, on: bool, cx: &mut Context<Self>) {
        let step = switch_step(&mut self.screen, on, crate::speak_replies::session_account(cx));
        if !step.changed {
            return;
        }
        for op in step.ops {
            let _ = self.screen_ops.send(op);
        }
        self.save();
        cx.notify();
        self.sync(cx);
    }

    /// NEW-58: capture while the switch is on here; a sign-out or an
    /// account change deletes the buffer (switch on or off), another voice
    /// device turns it off and deletes it, the switch going off stops and
    /// deletes ([`screen_step`]); the work runs on the ops task, in order.
    fn sync_screen(&mut self, device_is_voice_device: bool, cx: &mut Context<Self>) {
        let account = crate::speak_replies::session_account(cx);
        let settings_known = crate::speak_replies::settings_known(cx);
        let step = screen_step(&mut self.screen, account, device_is_voice_device, settings_known);
        for op in step.ops {
            let _ = self.screen_ops.send(op);
        }
        if step.changed {
            self.save();
            cx.notify();
        }
    }

    /// NEW-58: a start's answer. A stale one is left alone: the stop or
    /// delete that made it stale is queued behind it.
    fn screen_started(&mut self, generation: u64, result: Result<(), String>, cx: &mut Context<Self>) {
        if self.screen.generation != generation || result.as_ref().is_err_and(|reason| reason == screen::CANCELLED) {
            return;
        }
        if let Err(reason) = result {
            self.screen.on = false;
            self.screen.account = None;
            self.screen.running = false;
            self.save();
            crate::action_error::failed_action("start", "screen memory", Some(reason), cx);
            self.sync(cx);
        }
    }

    /// #NEW-97: a host's answer (Linux), or the native tray's existence.
    fn set_hosted(&mut self, hosted: Option<bool>, cx: &mut Context<Self>) {
        if hosted != self.hosted {
            self.hosted = hosted;
            self.sync(cx);
        }
    }

    fn sync(&mut self, cx: &mut Context<Self>) {
        let settings = self.settings(cx);
        let agent = agent_label(&settings, cx);
        // #NEW-54: Mute shows once a bound key or the wake word listens here
        // and the worker is up; the grab's status names an unavailable
        // shortcut or the portal's trigger.
        let mic_available = crate::push_to_talk::listens_here(&settings, &self.device_id) && crate::speak_replies::worker_ready(cx);
        let status = crate::push_to_talk::status(cx);
        let device_is_voice_device = settings.voice_device_id.as_deref() == Some(self.device_id.as_str());
        let wake_listening = crate::push_to_talk::wake_listening(cx);

        self.sync_screen(device_is_voice_device, cx);
        // NEW-57: the meeting's item, and the pause while a meeting runs or
        // screen memory is on (its keys grabbed exactly then).
        let meeting_state = crate::meeting::state(cx);
        // NEW-57: a meeting holding the microphone shows the dot too.
        let meeting_listening = meeting_state.running && meeting_state.listening && !meeting_state.paused;
        // #FEED-163: a meeting running here (paused or not) records.
        let icon = tray_icon(device_is_voice_device, wake_listening, meeting_listening, meeting_state.running, self.muted);
        let meeting_on = settings.meeting();
        let who = if meeting_state.running && !meeting_state.who.is_empty() {
            meeting_state.who.clone()
        } else {
            crate::meeting_mode::who_of(&settings, cx)
        };
        let pausable = device_is_voice_device && (meeting_on || self.screen.on);
        crate::meeting_mode::sync_pause_hotkey(pausable, cx);
        let context = TrayContext {
            agent: agent.as_deref(),
            muted: self.muted,
            // NEW-55: Call shows while a call can start here.
            call_available: crate::voice_call::unavailable(cx).is_none(),
            mic_available,
            device_is_voice_device,
            shortcut_unavailable: status.reason().is_some(),
            shortcut_label: status.trigger(),
            screen_memory: self.screen.available.then_some(ScreenMemoryState {
                on: self.screen.on,
                paused: self.screen.status == screen::Status::Paused,
            }),
            meeting: Some(MeetingTrayState { on: meeting_on, who: &who }),
            pause: pausable.then_some(meeting_state.paused),
            // S-3: while a call runs here Call reads End call.
            in_call: crate::voice_call::active(cx),
        };
        let menu = tray_menu(&settings, &context);
        // #NEW-97: no answer yet = no button.
        let rail = self.hosted.and_then(|hosted| rail_voice(&settings, &context, wake_listening, meeting_listening, hosted));
        if rail != self.rail {
            self.rail = rail;
            cx.notify();
        }
        if icon != self.icon {
            self.icon = icon;
            if let Some(icon) = icon {
                self.backend.set_icon(icon);
            }
        }
        if menu == self.menu {
            return;
        }
        let hidden = self.menu.is_some() && menu.is_none();
        self.menu = menu.clone();
        self.backend.set(menu);
        cx.notify();
        // #NEW-97: a hidden tray answers again once it is wanted (Linux), the
        // native tray exists or not right away.
        #[cfg(target_os = "linux")]
        if hidden {
            self.hosted = None;
        }
        #[cfg(not(target_os = "linux"))]
        self.set_hosted(self.menu.is_some().then(|| self.backend.registered()), cx);
        // No longer the voice device with every window closed: nothing keeps
        // the IDE up any more (macOS keeps running as always).
        #[cfg(not(target_os = "macos"))]
        if hidden && cx.windows().is_empty() {
            cx.quit();
        }
        #[cfg(target_os = "macos")]
        let _ = hidden;
    }

    fn on_click(&mut self, id: TrayItemId, cx: &mut Context<Self>) {
        match id {
            TrayItemId::WakeWord => self.toggle(id, cx),
            TrayItemId::Mute => self.set_muted(!self.muted, cx),
            TrayItemId::ScreenMemory => self.set_screen_memory(!self.screen.on, cx),
            // NEW-57: on = THE consent dialog in an IDE window (the native
            // check flipped itself: draw the menu as it is again); off = stop.
            TrayItemId::Meeting => {
                let settings = self.settings(cx);
                if settings.voice_device_id.is_none() || settings.agent_id.is_none() {
                    return;
                }
                if settings.meeting() {
                    let mut next = settings;
                    next.meeting_started_at = None;
                    self.pending = Some(next);
                    self.generation += 1;
                    let generation = self.generation;
                    self.sync(cx);
                    crate::meeting_mode::stop(cx);
                    // The speaker adopts the stored settings; drop the
                    // optimistic copy once it has had the time to.
                    cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(std::time::Duration::from_secs(5)).await;
                        let _ = this.update(cx, |this, cx| {
                            if this.generation == generation {
                                this.pending = None;
                                this.sync(cx);
                            }
                        });
                    })
                    .detach();
                } else {
                    self.backend.set(self.menu.clone());
                    cx.defer(crate::meeting_mode::request_consent);
                }
            }
            TrayItemId::Pause => {
                let paused = crate::meeting::state(cx).paused;
                crate::meeting::set_paused(!paused, cx);
            }
            TrayItemId::Open => open_ide(cx),
            // S-3: Push to talk · {key} is an action: the key is recorded in
            // Settings → Voice.
            TrayItemId::Settings | TrayItemId::PushToTalk => {
                crate::os_notifications::open_settings(crate::settings::SettingsSection::Voice, cx)
            }
            TrayItemId::Quit => cx.quit(),
            // NEW-55: call the voice agent (deferred: the call stops this
            // tray's speaker and redraws it).
            TrayItemId::Call => {
                if let Some(agent_id) = self.settings(cx).agent_id {
                    cx.defer(move |cx| crate::voice_call::start(agent_id, cx));
                }
            }
            // S-3: End call (deferred like Call).
            TrayItemId::EndCall => cx.defer(crate::voice_call::end),
        }
    }

    /// Optimistic like Settings → Voice: flip at once, send the partial,
    /// hand the stored settings to the speaker; a refusal reverts and toasts.
    fn toggle(&mut self, id: TrayItemId, cx: &mut Context<Self>) {
        let current = self.settings(cx);
        if current.voice_device_id.is_none() || current.agent_id.is_none() {
            return;
        }
        let mut next = current.clone();
        let mut update = VoiceUpdate::default();
        match id {
            TrayItemId::WakeWord => {
                next.wake_word = !current.wake_word;
                update.wake_word = Some(next.wake_word);
            }
            _ => return,
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else { return };
        self.pending = Some(next);
        self.generation += 1;
        let generation = self.generation;
        self.sync(cx);
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { api::voice::voice_update(&trpc, &update) }).await;
            let _ = this.update(cx, |this, cx| {
                if this.generation == generation {
                    this.pending = None;
                }
                match result {
                    // The speaker adopts it; its notify re-syncs the menu.
                    Ok(stored) => crate::speak_replies::set_settings(stored, cx),
                    Err(err) => crate::action_error::failed_action_err("save", "the voice settings", &err, cx),
                }
                this.sync(cx);
            });
        })
        .detach();
    }
}

/// Linux: our own StatusNotifierItem + dbusmenu over zbus.
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as backend;

/// macOS and Windows: the native tray, on gpui's main thread.
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod backend {
    use domain::voice::{TrayIcon as IconState, TrayItem, TrayItemId};
    use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

    /// One live row, kept so a new menu of the same shape updates in place
    /// (rebuilding the `Menu` on every change leaks HMENUs on Windows).
    enum Row {
        Separator,
        Action(MenuItem),
        Check(CheckMenuItem),
    }

    /// A row's shape: its kind and id (separators have none).
    fn shape(item: &TrayItem) -> (u8, Option<TrayItemId>) {
        match item {
            TrayItem::Separator => (0, None),
            TrayItem::Action { id, .. } => (1, Some(*id)),
            TrayItem::Check { id, .. } => (2, Some(*id)),
        }
    }

    pub(super) struct Backend {
        tray: Option<TrayIcon>,
        rows: Vec<Row>,
        shape: Vec<(u8, Option<TrayItemId>)>,
        /// #NEW-56: the icon to draw (the microphone dot or not).
        icon: IconState,
    }

    /// The rasterized icon in `state`.
    fn icon(state: IconState) -> Option<Icon> {
        let frame = super::icon_frames(state, &[64]).pop()?;
        Icon::from_rgba(frame.rgba, frame.width, frame.height).ok()
    }

    impl Backend {
        pub(super) fn new(clicks: flume::Sender<super::TrayEvent>) -> Self {
            let menu_clicks = clicks.clone();
            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                if let Some(id) = super::item_of(event.id.0.as_str()) {
                    let _ = menu_clicks.send(super::TrayEvent::Item(id));
                }
            }));
            // Windows: a left click on the icon brings the IDE up (the menu
            // is a right click). macOS opens the menu, which leads with Open.
            // #NEW-76: a double-click (a click, then a double-click) opens once.
            #[cfg(target_os = "windows")]
            let last = std::sync::Mutex::new(None);
            #[cfg(target_os = "windows")]
            tray_icon::TrayIconEvent::set_event_handler(Some(move |event: tray_icon::TrayIconEvent| {
                use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};
                let open = matches!(
                    event,
                    TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }
                        | TrayIconEvent::DoubleClick { button: MouseButton::Left, .. }
                );
                let mut last = last.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                if open && super::activation_counts(&mut last, std::time::Instant::now()) {
                    let _ = clicks.send(super::TrayEvent::Activate);
                }
            }));
            #[cfg(not(target_os = "windows"))]
            let _ = clicks;
            Self { tray: None, rows: Vec::new(), shape: Vec::new(), icon: IconState::App }
        }

        /// #NEW-56: draw `state` (now, or when the tray appears).
        pub(super) fn set_icon(&mut self, state: IconState) {
            if state == self.icon {
                return;
            }
            self.icon = state;
            let Some(tray) = self.tray.as_ref() else { return };
            #[cfg(target_os = "macos")]
            let set = if super::templated(state) { tray.set_icon_templated(icon(state)) } else { tray.set_icon(icon(state)) };
            #[cfg(not(target_os = "macos"))]
            let set = tray.set_icon(icon(state));
            if let Err(err) = set {
                log::warn!("[ui] voice tray: icon: {err}");
            }
        }

        pub(super) fn set(&mut self, menu: Option<Vec<TrayItem>>) {
            let Some(items) = menu else {
                // Dropping the icon removes it.
                self.tray = None;
                self.rows.clear();
                self.shape.clear();
                return;
            };
            let shape: Vec<_> = items.iter().map(shape).collect();
            if self.tray.is_some() && shape == self.shape {
                for (row, item) in self.rows.iter().zip(&items) {
                    match (row, item) {
                        (Row::Action(row), TrayItem::Action { label: text, enabled, .. }) => {
                            row.set_text(label(text));
                            row.set_enabled(*enabled);
                        }
                        (Row::Check(row), TrayItem::Check { label: text, checked, enabled, .. }) => {
                            row.set_text(label(text));
                            row.set_enabled(*enabled);
                            row.set_checked(*checked);
                        }
                        _ => {}
                    }
                }
                return;
            }
            let (menu, rows) = build(&items);
            self.rows = rows;
            self.shape = shape;
            if let Some(tray) = self.tray.as_ref() {
                tray.set_menu(Some(Box::new(menu)));
                return;
            }
            let mut builder = TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip(super::TITLE);
            #[cfg(target_os = "windows")]
            {
                builder = builder.with_menu_on_left_click(false);
            }
            if let Some(icon) = icon(self.icon) {
                // The menu bar tints a template image for light and dark
                // (#FEED-163: never the red record dot).
                #[cfg(target_os = "macos")]
                {
                    builder = if super::templated(self.icon) { builder.with_icon_templated(icon) } else { builder.with_icon(icon) };
                }
                #[cfg(not(target_os = "macos"))]
                {
                    builder = builder.with_icon(icon);
                }
            }
            match builder.build() {
                Ok(tray) => self.tray = Some(tray),
                Err(err) => log::warn!("[ui] voice tray: {err}"),
            }
        }

        pub(super) fn registered(&self) -> bool {
            self.tray.is_some()
        }
    }

    /// Windows reads `&` as a mnemonic marker.
    fn label(text: &str) -> String {
        if cfg!(target_os = "windows") { text.replace('&', "&&") } else { text.to_string() }
    }

    fn build(items: &[TrayItem]) -> (Menu, Vec<Row>) {
        let menu = Menu::new();
        let mut rows = Vec::with_capacity(items.len());
        for item in items {
            let appended = match item {
                TrayItem::Separator => {
                    rows.push(Row::Separator);
                    menu.append(&PredefinedMenuItem::separator())
                }
                TrayItem::Action { id, label: text, enabled } => {
                    let row = MenuItem::with_id(super::item_key(*id), label(text), *enabled, None);
                    let appended = menu.append(&row);
                    rows.push(Row::Action(row));
                    appended
                }
                TrayItem::Check { id, label: text, checked, enabled } => {
                    let row = CheckMenuItem::with_id(super::item_key(*id), label(text), *enabled, *checked, None);
                    let appended = menu.append(&row);
                    rows.push(Row::Check(row));
                    appended
                }
            };
            if let Err(err) = appended {
                log::warn!("[ui] voice tray: menu item: {err}");
            }
        }
        (menu, rows)
    }
}

/// Elsewhere: no tray.
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod backend {
    use domain::voice::{TrayIcon, TrayItem};

    pub(super) struct Backend;

    impl Backend {
        pub(super) fn new(_clicks: flume::Sender<super::TrayEvent>) -> Self {
            Self
        }

        pub(super) fn set_icon(&mut self, _icon: TrayIcon) {}

        pub(super) fn set(&mut self, _menu: Option<Vec<TrayItem>>) {}

        pub(super) fn registered(&self) -> bool {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_item_round_trips_its_key() {
        for id in [
            TrayItemId::Open,
            TrayItemId::Call,
            TrayItemId::EndCall,
            TrayItemId::PushToTalk,
            TrayItemId::WakeWord,
            TrayItemId::ScreenMemory,
            TrayItemId::Mute,
            TrayItemId::Settings,
            TrayItemId::Quit,
        ] {
            assert_eq!(item_of(item_key(id)), Some(id));
        }
        assert_eq!(item_of("voice-tray:nope"), None);
    }

    #[test]
    fn the_windows_icon_carries_a_dark_ring() {
        let svg = ringed(ICON_SVG);
        let tree = resvg::usvg::Tree::from_data(&svg, &resvg::usvg::Options::default()).unwrap();
        let mut pixmap = resvg::tiny_skia::Pixmap::new(64, 64).unwrap();
        let scale = 64.0 / tree.size().width();
        resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
        let dark = pixmap.pixels().iter().filter(|pixel| pixel.alpha() > 200 && pixel.red() < 80).count();
        assert!(dark > 50, "the ring reads on a light taskbar ({dark} dark pixels)");
    }

    #[test]
    fn the_app_icon_rasterizes() {
        let frames = icon_frames(TrayIcon::App, &[22, 64]);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[1].rgba.len(), 64 * 64 * 4);
        assert!(frames[1].rgba.chunks_exact(4).any(|pixel| pixel[3] > 0), "not blank");
    }

    /// #NEW-56: the always-on microphone dot: red pixels in the lower right
    /// corner, none in the plain icon.
    #[test]
    fn the_listening_icon_carries_the_microphone_dot() {
        let red = |frame: &IconFrame| {
            frame
                .rgba
                .chunks_exact(4)
                .enumerate()
                .filter(|(index, pixel)| {
                    let (x, y) = (*index as u32 % frame.width, *index as u32 / frame.width);
                    x > frame.width / 2 && y > frame.height / 2 && pixel[3] > 200 && pixel[0] > 200 && pixel[1] < 140
                })
                .count()
        };
        let plain = icon_frames(TrayIcon::App, &[64]).pop().unwrap();
        let listening = icon_frames(TrayIcon::Listening, &[64]).pop().unwrap();
        assert_eq!(red(&plain), 0);
        assert!(red(&listening) > 60, "{} red pixels", red(&listening));
        // EXP_TRAY_ICON_PNG=<path>: write the listening icon for the PR.
        if let Ok(path) = std::env::var("EXP_TRAY_ICON_PNG") {
            let big = icon_frames(TrayIcon::Listening, &[256]).pop().unwrap();
            let mut pixmap = resvg::tiny_skia::Pixmap::new(big.width, big.height).unwrap();
            for (pixel, rgba) in pixmap.pixels_mut().iter_mut().zip(big.rgba.chunks_exact(4)) {
                *pixel = resvg::tiny_skia::ColorU8::from_rgba(rgba[0], rgba[1], rgba[2], rgba[3]).premultiply();
            }
            pixmap.save_png(path).unwrap();
        }
    }

    /// #FEED-163: the record dot: red in the same corner as the microphone
    /// dot, but its own shape (a ring around a core), drawn in colour on
    /// macOS (never a template) while App and Listening stay templates.
    /// EXP_TRAY_ICON_DIR=<dir>: write every state at 22 and 64 px for the PR.
    #[test]
    fn the_recording_icon_carries_the_record_dot() {
        let red_at = |frame: &IconFrame, corner: bool| {
            frame
                .rgba
                .chunks_exact(4)
                .enumerate()
                .filter(|(index, pixel)| {
                    let (x, y) = (*index as u32 % frame.width, *index as u32 / frame.width);
                    let lower_right = x > frame.width / 2 && y > frame.height / 2;
                    lower_right == corner && pixel[3] > 200 && pixel[0] > 200 && pixel[1] < 140
                })
                .count()
        };
        let listening = icon_frames(TrayIcon::Listening, &[64]).pop().unwrap();
        let recording = icon_frames(TrayIcon::Recording, &[64]).pop().unwrap();
        assert!(red_at(&recording, true) > 60, "{} red pixels", red_at(&recording, true));
        assert_eq!(red_at(&recording, false), 0, "only in the corner");
        assert_ne!(recording.rgba, listening.rgba, "distinct from the microphone dot");
        // The record dot's core and ring are red, the gap between them dark.
        let at = |frame: &IconFrame, units: f32| {
            let px = ((units + 11.0) * frame.width as f32 / 122.0) as u32;
            let i = ((px * frame.width + px) * 4) as usize;
            [frame.rgba[i], frame.rgba[i + 1], frame.rgba[i + 2]]
        };
        let red = theme::tokens::DESTRUCTIVE;
        assert_eq!(at(&recording, 84.0), [red.r, red.g, red.b], "the core");
        // 15 units out along the diagonal: between the core (12) and the ring (18..24).
        assert!(at(&recording, 84.0 + 10.6)[0] < 80, "the gap");
        assert!(templated(TrayIcon::App) && templated(TrayIcon::Listening) && !templated(TrayIcon::Recording));
        if let Ok(dir) = std::env::var("EXP_TRAY_ICON_DIR") {
            std::fs::create_dir_all(&dir).unwrap();
            for (name, icon) in [("app", TrayIcon::App), ("listening", TrayIcon::Listening), ("recording", TrayIcon::Recording)] {
                for frame in icon_frames(icon, &[22, 64]) {
                    let mut pixmap = resvg::tiny_skia::Pixmap::new(frame.width, frame.height).unwrap();
                    for (pixel, rgba) in pixmap.pixels_mut().iter_mut().zip(frame.rgba.chunks_exact(4)) {
                        *pixel = resvg::tiny_skia::ColorU8::from_rgba(rgba[0], rgba[1], rgba[2], rgba[3]).premultiply();
                    }
                    pixmap.save_png(format!("{dir}/tray-{name}-{}.png", frame.width)).unwrap();
                }
            }
        }
    }

    /// #FEED-164: a leftover buffer goes at install with the switch off, and
    /// with it on where nothing can capture (Wayland hides the row).
    #[test]
    fn a_leftover_buffer_is_swept_where_nothing_captures() {
        assert!(sweeps_leftover(false, true), "switch off");
        assert!(sweeps_leftover(true, false), "on, but Wayland");
        assert!(sweeps_leftover(false, false));
        assert!(!sweeps_leftover(true, true), "on and capturing: kept");
    }

    /// #NEW-86: Mute survives a restart on this device; no file (or a
    /// broken one) reads as not muted.
    #[test]
    fn mute_is_kept_on_the_device() {
        let dir = std::env::temp_dir().join(format!("exp-voice-tray-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _ = std::fs::remove_file(dir.join(STATE_FILE));
        assert_eq!(load_state(&dir), TrayState::default(), "no file");
        save_state(&dir, &TrayState { muted: true, ..TrayState::default() });
        assert!(load_state(&dir).muted, "kept");
        save_state(&dir, &TrayState::default());
        assert!(!load_state(&dir).muted, "unmuted");
        // NEW-58: Screen memory rides the same file, its account with it;
        // a file from before it reads as off.
        let on = TrayState { muted: false, screen_memory: true, screen_account: Some("acct".into()) };
        save_state(&dir, &on);
        assert_eq!(load_state(&dir), on, "screen memory kept");
        std::fs::write(dir.join(STATE_FILE), br#"{"muted":true}"#).unwrap();
        assert_eq!(load_state(&dir), TrayState { muted: true, ..TrayState::default() }, "an older file");
        std::fs::write(dir.join(STATE_FILE), b"{not json").unwrap();
        assert_eq!(load_state(&dir), TrayState::default(), "unreadable");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// NEW-58: the Screen memory switch's steps: running here starts; the
    /// switch going off (a not-running start-up stop keeps the buffer) is
    /// the tray's Forget; a sign-out or an account change deletes the buffer
    /// with the switch on or OFF; another voice device turns it off.
    #[test]
    fn screen_memory_steps_delete_the_buffer_on_sign_out_whatever_the_switch() {
        let acct = || Some("acct".to_string());
        let mut screen = ScreenMemory::new(true, acct(), true);
        // Before the settings are known nothing runs; then it starts.
        assert_eq!(screen_step(&mut screen, None, false, false).ops, []);
        let step = screen_step(&mut screen, acct(), true, true);
        assert_eq!(step.ops, [ScreenOp::Start(screen.generation)]);
        assert!(!step.changed);
        assert_eq!(screen_step(&mut screen, acct(), true, true).ops, [], "running already");
        // Sign-out while on: off and deleted.
        let step = screen_step(&mut screen, None, true, true);
        assert_eq!(step.ops, [ScreenOp::Forget]);
        assert!(step.changed && !screen.on && !screen.running);

        // Switch OFF, signed in as acct: an account change still deletes.
        let mut screen = ScreenMemory::new(false, None, true);
        assert_eq!(screen_step(&mut screen, acct(), true, true).ops, []);
        let step = screen_step(&mut screen, Some("other".into()), true, true);
        assert_eq!(step.ops, [ScreenOp::Forget]);
        assert!(!step.changed, "the switch was off already");
        assert_eq!(screen_step(&mut screen, None, true, true).ops, [ScreenOp::Forget], "sign-out with the switch off");
        assert_eq!(screen_step(&mut screen, None, true, true).ops, [], "once");

        // Another voice device: off and deleted.
        let mut screen = ScreenMemory::new(true, acct(), true);
        screen_step(&mut screen, acct(), true, true);
        let step = screen_step(&mut screen, acct(), false, true);
        assert_eq!(step.ops, [ScreenOp::Forget]);
        assert!(step.changed && !screen.on);
        // Running, then the settings say this is not the voice device yet
        // unknown: a stop that keeps the buffer.
        let mut screen = ScreenMemory::new(true, acct(), true);
        screen_step(&mut screen, acct(), true, true);
        assert_eq!(screen_step(&mut screen, acct(), false, false).ops, [ScreenOp::Stop { forget: false }]);
    }

    /// NEW-58 / #FEED-162: the item going off is the tray's Forget (stop
    /// and delete), running or not; on only records the account; a machine
    /// that cannot capture (Wayland) never starts.
    #[test]
    fn switching_screen_memory_off_forgets_and_unavailable_never_starts() {
        let acct = || Some("acct".to_string());
        let mut screen = ScreenMemory::new(false, None, true);
        let step = switch_step(&mut screen, true, acct());
        assert_eq!((step.ops.as_slice(), step.changed), ([].as_slice(), true));
        assert_eq!(screen.account, acct());
        assert_eq!(screen_step(&mut screen, acct(), true, true).ops, [ScreenOp::Start(screen.generation)]);
        let started = screen.generation;
        let step = switch_step(&mut screen, false, acct());
        assert_eq!((step.ops.as_slice(), step.changed), ([ScreenOp::Forget].as_slice(), true));
        assert!(!screen.running && screen.account.is_none() && screen.generation > started, "the start's answer is stale");
        assert_eq!(switch_step(&mut screen, false, acct()), ScreenStep::default(), "off again: nothing");
        assert_eq!(screen_step(&mut screen, acct(), true, true).ops, [], "stays off");

        let mut wayland = ScreenMemory::new(true, acct(), false);
        assert_eq!(screen_step(&mut wayland, acct(), true, true).ops, []);
        assert!(!wayland.running);
    }

    /// NEW-58: the tray entity's own path: switching the item off queues the
    /// Forget and keeps the switch off on disk.
    #[gpui::test]
    async fn the_tray_item_going_off_queues_the_forget(cx: &mut gpui::TestAppContext) {
        let dir = std::env::temp_dir().join(format!("voice-tray-screen-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (screen_ops, ops) = flume::unbounded();
        let (tx, _rx) = flume::unbounded();
        let tray = cx.update(|cx| {
            cx.new(|_| VoiceTray {
                muted: false,
                screen: ScreenMemory::new(false, None, true),
                screen_ops,
                data_dir: dir.clone(),
                pending: None,
                generation: 0,
                device_id: "this-device".into(),
                menu: None,
                icon: None,
                hosted: None,
                rail: None,
                backend: backend::Backend::new(tx),
            })
        });
        tray.update(cx, |tray, cx| tray.set_screen_memory(true, cx));
        assert!(load_state(&dir).screen_memory);
        assert!(ops.try_iter().all(|op| op != ScreenOp::Forget));
        tray.update(cx, |tray, cx| tray.set_screen_memory(false, cx));
        assert_eq!(ops.try_iter().collect::<Vec<_>>(), [ScreenOp::Forget]);
        assert!(!load_state(&dir).screen_memory);
        tray.update(cx, |tray, _| assert!(!tray.screen.on && !tray.screen.running));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_double_click_activates_once() {
        let start = std::time::Instant::now();
        let mut last = None;
        assert!(activation_counts(&mut last, start));
        assert!(!activation_counts(&mut last, start + std::time::Duration::from_millis(250)), "the double-click");
        assert!(activation_counts(&mut last, start + ACTIVATE_DEBOUNCE), "a later click");
    }
}
