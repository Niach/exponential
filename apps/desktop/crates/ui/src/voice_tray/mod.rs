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
//! reverts and toasts). Mute microphone is IDE-local: it shows while push to
//! talk listens here and the speech worker is up (`micAvailable`), and push
//! to talk never opens the microphone while it is on (#NEW-54). A shortcut
//! that can't be held system-wide says so on the Push to talk item.
//! While the tray is up, closing the last window keeps the IDE running
//! ([`keeps_running`], read by the shell) so the speech worker stays resident.
//! #NEW-56: while the wake word holds the microphone open the icon carries a
//! dot (`domain::voice::tray_icon`), and Mute takes the wake word's
//! microphone away too.

use gpui::{App, AppContext as _, Context, Entity, Global};

use api::voice::VoiceUpdate;
use domain::voice::{tray_icon, tray_menu, TrayContext, TrayIcon, TrayItem, TrayItemId, VoiceSettings};

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
/// adds the always-on microphone dot.
fn icon_svg(icon: TrayIcon) -> std::borrow::Cow<'static, [u8]> {
    let svg = if cfg!(target_os = "windows") { std::borrow::Cow::Owned(ringed(ICON_SVG)) } else { std::borrow::Cow::Borrowed(ICON_SVG) };
    match icon {
        TrayIcon::App => svg,
        TrayIcon::Listening => std::borrow::Cow::Owned(dotted(&svg)),
    }
}

/// The icon with the microphone dot in its lower right corner: the
/// design tokens' `destructive` red, cut out of the disc by a dark ring.
fn dotted(svg: &[u8]) -> Vec<u8> {
    String::from_utf8_lossy(svg)
        .replace(
            "</svg>",
            "  <circle cx=\"86\" cy=\"86\" r=\"21\" fill=\"#ff6467\" stroke=\"#27272a\" stroke-width=\"7\"></circle>\n</svg>",
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
        TrayItemId::SpeakReplies => "voice-tray:speak-replies",
        TrayItemId::PushToTalk => "voice-tray:push-to-talk",
        TrayItemId::WakeWord => "voice-tray:wake-word",
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
        TrayItemId::SpeakReplies,
        TrayItemId::PushToTalk,
        TrayItemId::WakeWord,
        TrayItemId::Mute,
        TrayItemId::Settings,
        TrayItemId::Quit,
    ]
    .into_iter()
    .find(|id| item_key(*id) == key)
}

pub(crate) struct VoiceTray {
    /// Mute microphone: IDE-local, never stored.
    muted: bool,
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
    backend: backend::Backend,
}

struct VoiceTrayGlobal(Entity<VoiceTray>);

impl Global for VoiceTrayGlobal {}

/// Install the tray ONCE, after `speak_replies::install` (it follows the
/// settings that watcher holds).
pub fn install(cx: &mut App) {
    let Some(speaker) = crate::speak_replies::entity(cx) else { return };
    let device_id = steer::persistent_device_id(&crate::session::AuthContext::global(cx).data_dir);
    let (tx, rx) = flume::unbounded::<TrayEvent>();
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
        VoiceTray { muted: false, pending: None, generation: 0, device_id, menu: None, icon: None, backend: backend::Backend::new(tx) }
    });
    cx.set_global(VoiceTrayGlobal(entity.clone()));
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
            });
        }
    })
    .detach();
}

fn global(cx: &App) -> Option<Entity<VoiceTray>> {
    cx.try_global::<VoiceTrayGlobal>().map(|global| global.0.clone())
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
        entity.update(cx, |this, cx| {
            this.muted = muted;
            crate::push_to_talk::mute_changed(muted, cx);
            this.sync(cx);
        });
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

    fn sync(&mut self, cx: &mut Context<Self>) {
        let settings = self.settings(cx);
        let agent = agent_label(&settings, cx);
        // #NEW-54: Mute shows once push to talk listens here and the worker
        // is up; the grab's status names an unavailable shortcut or the
        // portal's trigger.
        let mic_available = crate::speak_replies::listens_here(&settings, &self.device_id) && crate::speak_replies::worker_ready(cx);
        let status = crate::push_to_talk::status(cx);
        let device_is_voice_device = settings.voice_device_id.as_deref() == Some(self.device_id.as_str());
        let icon = tray_icon(device_is_voice_device, crate::push_to_talk::wake_listening(cx), self.muted);
        let menu = tray_menu(
            &settings,
            &TrayContext {
                agent: agent.as_deref(),
                muted: self.muted,
                // NEW-55: Call shows while a call can start here.
                call_available: crate::voice_call::unavailable(cx).is_none(),
                mic_available,
                device_is_voice_device,
                shortcut_unavailable: status.reason().is_some(),
                shortcut_label: status.trigger(),
            },
        );
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
            TrayItemId::SpeakReplies | TrayItemId::PushToTalk | TrayItemId::WakeWord => self.toggle(id, cx),
            TrayItemId::Mute => {
                self.muted = !self.muted;
                crate::push_to_talk::mute_changed(self.muted, cx);
                self.sync(cx);
            }
            TrayItemId::Open => open_ide(cx),
            TrayItemId::Settings => crate::os_notifications::open_settings(crate::settings::SettingsSection::Voice, cx),
            TrayItemId::Quit => cx.quit(),
            // NEW-55: call the voice agent (deferred: the call stops this
            // tray's speaker and redraws it).
            TrayItemId::Call => {
                if let Some(agent_id) = self.settings(cx).agent_id {
                    cx.defer(move |cx| crate::voice_call::start(agent_id, cx));
                }
            }
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
            TrayItemId::SpeakReplies => {
                next.speak_replies = !current.speak_replies;
                update.speak_replies = Some(next.speak_replies);
            }
            TrayItemId::PushToTalk => {
                next.push_to_talk = !current.push_to_talk;
                update.push_to_talk = Some(next.push_to_talk);
            }
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
            let set = tray.set_icon_templated(icon(state));
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
                // The menu bar tints a template image for light and dark.
                #[cfg(target_os = "macos")]
                {
                    builder = builder.with_icon_templated(icon);
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
            TrayItemId::SpeakReplies,
            TrayItemId::PushToTalk,
            TrayItemId::WakeWord,
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

    #[test]
    fn a_double_click_activates_once() {
        let start = std::time::Instant::now();
        let mut last = None;
        assert!(activation_counts(&mut last, start));
        assert!(!activation_counts(&mut last, start + std::time::Duration::from_millis(250)), "the double-click");
        assert!(activation_counts(&mut last, start + ACTIVATE_DEBOUNCE), "a later click");
    }
}
