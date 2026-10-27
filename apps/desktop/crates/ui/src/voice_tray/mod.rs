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

use gpui::{App, AppContext as _, Context, Entity, Global};

use api::voice::VoiceUpdate;
use domain::voice::{tray_menu, TrayContext, TrayItem, TrayItemId, VoiceSettings};

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
/// like the hicolor icon `desktop_integration` installs).
fn icon_svg() -> std::borrow::Cow<'static, [u8]> {
    if cfg!(target_os = "windows") {
        std::borrow::Cow::Owned(ringed(ICON_SVG))
    } else {
        std::borrow::Cow::Borrowed(ICON_SVG)
    }
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

fn icon_frames(sizes: &[u32]) -> Vec<IconFrame> {
    let Ok(tree) = resvg::usvg::Tree::from_data(&icon_svg(), &resvg::usvg::Options::default()) else {
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
        VoiceTray { muted: false, pending: None, generation: 0, device_id, menu: None, backend: backend::Backend::new(tx) }
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
        let menu = tray_menu(
            &settings,
            &TrayContext {
                agent: agent.as_deref(),
                muted: self.muted,
                // NEW-55: Call shows while a call can start here.
                call_available: crate::voice_call::unavailable(cx).is_none(),
                mic_available,
                device_is_voice_device: settings.voice_device_id.as_deref() == Some(self.device_id.as_str()),
                shortcut_unavailable: status.reason().is_some(),
                shortcut_label: status.trigger(),
            },
        );
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
    use domain::voice::{TrayItem, TrayItemId};
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
            #[cfg(target_os = "windows")]
            tray_icon::TrayIconEvent::set_event_handler(Some(move |event: tray_icon::TrayIconEvent| {
                use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};
                let open = matches!(
                    event,
                    TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }
                        | TrayIconEvent::DoubleClick { button: MouseButton::Left, .. }
                );
                if open {
                    let _ = clicks.send(super::TrayEvent::Activate);
                }
            }));
            #[cfg(not(target_os = "windows"))]
            let _ = clicks;
            Self { tray: None, rows: Vec::new(), shape: Vec::new() }
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
            if let Some(frame) = super::icon_frames(&[64]).pop() {
                if let Ok(icon) = Icon::from_rgba(frame.rgba, frame.width, frame.height) {
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
    use domain::voice::TrayItem;

    pub(super) struct Backend;

    impl Backend {
        pub(super) fn new(_clicks: flume::Sender<super::TrayEvent>) -> Self {
            Self
        }

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
        let frames = icon_frames(&[22, 64]);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[1].rgba.len(), 64 * 64 * 4);
        assert!(frames[1].rgba.chunks_exact(4).any(|pixel| pixel[3] > 0), "not blank");
    }
}
