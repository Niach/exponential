//! NEW-68 (voice plan v3, V2): the IDE in the system tray
//! (`fixtures/voice.json` `trayMenu`).
//!
//! The tray shows ONLY while this IDE is the member's voice device. Its menu
//! is OS-native: a StatusNotifierItem through `ksni` on Linux (pure Rust over
//! D-Bus, no GTK; GNOME without the AppIndicator extension shows nothing),
//! `tray-icon` + `muda` on macOS and Windows (created on gpui's main thread).
//! The rows come from [`domain::voice::tray_menu`]; a toggle writes
//! `voice.update` optimistically like Settings → Voice (the menu flips at
//! once, the stored value goes to `speak_replies::set_settings`, a refusal
//! reverts and toasts). Mute microphone is IDE-local (V3 consumes it).
//! While the tray is up, closing the last window keeps the IDE running
//! ([`keeps_running`], read by the shell) so the speech worker stays resident.

use gpui::{App, AppContext as _, Context, Entity, Global};

use api::voice::VoiceUpdate;
use domain::voice::{tray_menu, TrayContext, TrayItem, TrayItemId, VoiceSettings};

/// The app icon (the one `desktop_integration` installs on Linux).
const ICON_SVG: &[u8] = include_bytes!("../../../assets/icons/logo-white.svg");

/// The tray's title / tooltip.
const TITLE: &str = "Exponential";

/// One rasterized icon frame, straight (not premultiplied) RGBA.
struct IconFrame {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn icon_frames(sizes: &[u32]) -> Vec<IconFrame> {
    let Ok(tree) = resvg::usvg::Tree::from_data(ICON_SVG, &resvg::usvg::Options::default()) else {
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

/// A menu entry's stable id across the OS boundary.
#[cfg_attr(target_os = "linux", allow(dead_code))]
fn item_key(id: TrayItemId) -> &'static str {
    match id {
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
    let (tx, rx) = flume::unbounded::<TrayItemId>();
    let entity = cx.new(|cx: &mut Context<VoiceTray>| {
        cx.observe(&speaker, |this: &mut VoiceTray, _, cx| this.sync(cx)).detach();
        // The agent's name labels Call and the wake phrase's placeholder.
        if let Some(agents) = sync::Store::try_global(cx).map(|store| store.collections().agents.clone()) {
            cx.observe(&agents, |this: &mut VoiceTray, _, cx| this.sync(cx)).detach();
        }
        VoiceTray { muted: false, pending: None, generation: 0, device_id, menu: None, backend: backend::Backend::new(tx) }
    });
    cx.set_global(VoiceTrayGlobal(entity.clone()));
    let weak = entity.downgrade();
    cx.spawn(async move |cx| {
        while let Ok(id) = rx.recv_async().await {
            let Some(entity) = weak.upgrade() else { break };
            cx.update(|cx| entity.update(cx, |this, cx| this.on_click(id, cx)));
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

/// Mute microphone (V3 stops listening while it is on).
#[allow(dead_code)]
pub(crate) fn muted(cx: &App) -> bool {
    global(cx).is_some_and(|entity| entity.read(cx).muted)
}

/// The voice agent's label as Settings → Voice shows it.
fn agent_label(settings: &VoiceSettings, cx: &App) -> Option<String> {
    let agent_id = settings.agent_id.as_deref()?;
    let (_, agents) = crate::settings::voice_prefs::voice_choices(cx);
    agents.into_iter().find(|option| option.id.as_deref() == Some(agent_id)).map(|option| option.label)
}

impl VoiceTray {
    fn settings(&self, cx: &App) -> VoiceSettings {
        self.pending.clone().unwrap_or_else(|| crate::speak_replies::settings(cx))
    }

    fn sync(&mut self, cx: &mut Context<Self>) {
        let settings = self.settings(cx);
        let agent = agent_label(&settings, cx);
        let menu = tray_menu(
            &settings,
            &TrayContext {
                agent: agent.as_deref(),
                muted: self.muted,
                // V4 brings the call.
                call_available: false,
                device_is_voice_device: settings.voice_device_id.as_deref() == Some(self.device_id.as_str()),
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
            TrayItemId::Settings => crate::os_notifications::open_settings(crate::settings::SettingsSection::Voice, cx),
            TrayItemId::Quit => cx.quit(),
            // V4: the menu never offers Call before then.
            TrayItemId::Call => {}
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

/// Linux: a StatusNotifierItem on its own thread (D-Bus calls block).
#[cfg(target_os = "linux")]
mod backend {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use domain::voice::{TrayItem, TrayItemId};
    use ksni::blocking::TrayMethods as _;

    pub(super) struct Backend {
        tx: flume::Sender<Option<Vec<TrayItem>>>,
        registered: Arc<AtomicBool>,
    }

    impl Backend {
        pub(super) fn new(clicks: flume::Sender<TrayItemId>) -> Self {
            let (tx, rx) = flume::unbounded();
            let registered = Arc::new(AtomicBool::new(false));
            let flag = registered.clone();
            if let Err(err) = std::thread::Builder::new().name("voice-tray".into()).spawn(move || run(rx, clicks, flag)) {
                log::warn!("[ui] voice tray: thread failed to start: {err}");
            }
            Self { tx, registered }
        }

        pub(super) fn set(&mut self, menu: Option<Vec<TrayItem>>) {
            let _ = self.tx.send(menu);
        }

        pub(super) fn registered(&self) -> bool {
            self.registered.load(Ordering::SeqCst)
        }
    }

    fn run(rx: flume::Receiver<Option<Vec<TrayItem>>>, clicks: flume::Sender<TrayItemId>, registered: Arc<AtomicBool>) {
        // ARGB32 in network byte order.
        let icon: Vec<ksni::Icon> = super::icon_frames(&[22, 32, 48, 64])
            .into_iter()
            .map(|frame| ksni::Icon {
                width: frame.width as i32,
                height: frame.height as i32,
                data: frame.rgba.chunks_exact(4).flat_map(|p| [p[3], p[0], p[1], p[2]]).collect(),
            })
            .collect();
        let mut handle: Option<ksni::blocking::Handle<SniTray>> = None;
        while let Ok(mut menu) = rx.recv() {
            // Only the newest menu matters.
            while let Ok(newer) = rx.try_recv() {
                menu = newer;
            }
            match menu {
                Some(menu) => match handle.as_ref().filter(|handle| !handle.is_closed()) {
                    Some(handle) => {
                        handle.update(|tray| tray.menu = menu);
                    }
                    None => match (SniTray { menu, icon: icon.clone(), clicks: clicks.clone() }).spawn() {
                        Ok(spawned) => {
                            handle = Some(spawned);
                            registered.store(true, Ordering::SeqCst);
                        }
                        Err(err) => {
                            // No StatusNotifierWatcher (GNOME without the
                            // AppIndicator extension): no tray, and closing
                            // the last window quits as before.
                            log::info!("[ui] voice tray: no tray host: {err}");
                            registered.store(false, Ordering::SeqCst);
                        }
                    },
                },
                None => {
                    if let Some(handle) = handle.take() {
                        handle.shutdown().wait();
                    }
                    registered.store(false, Ordering::SeqCst);
                }
            }
        }
    }

    struct SniTray {
        menu: Vec<TrayItem>,
        icon: Vec<ksni::Icon>,
        clicks: flume::Sender<TrayItemId>,
    }

    /// dbusmenu reads `_` as a mnemonic marker.
    fn label(text: &str) -> String {
        text.replace('_', "__")
    }

    fn click(id: TrayItemId) -> Box<dyn Fn(&mut SniTray) + Send> {
        Box::new(move |tray: &mut SniTray| {
            let _ = tray.clicks.send(id);
        })
    }

    impl ksni::Tray for SniTray {
        // A left click opens the menu, as on macOS and Windows.
        const MENU_ON_ACTIVATE: bool = true;

        fn id(&self) -> String {
            "exponential-voice".into()
        }

        fn title(&self) -> String {
            super::TITLE.into()
        }

        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            self.icon.clone()
        }

        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            self.menu
                .iter()
                .map(|item| match item {
                    TrayItem::Separator => ksni::MenuItem::Separator,
                    TrayItem::Action { id, label: text, enabled } => ksni::menu::StandardItem {
                        label: label(text),
                        enabled: *enabled,
                        activate: click(*id),
                        ..Default::default()
                    }
                    .into(),
                    TrayItem::Check { id, label: text, checked, enabled } => ksni::menu::CheckmarkItem {
                        label: label(text),
                        enabled: *enabled,
                        checked: *checked,
                        activate: click(*id),
                        ..Default::default()
                    }
                    .into(),
                })
                .collect()
        }
    }
}

/// macOS and Windows: the native tray, on gpui's main thread.
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod backend {
    use domain::voice::{TrayItem, TrayItemId};
    use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

    pub(super) struct Backend {
        tray: Option<TrayIcon>,
    }

    impl Backend {
        pub(super) fn new(clicks: flume::Sender<TrayItemId>) -> Self {
            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                if let Some(id) = super::item_of(event.id.0.as_str()) {
                    let _ = clicks.send(id);
                }
            }));
            Self { tray: None }
        }

        pub(super) fn set(&mut self, menu: Option<Vec<TrayItem>>) {
            let Some(items) = menu else {
                // Dropping the icon removes it.
                self.tray = None;
                return;
            };
            let menu = build(&items);
            if let Some(tray) = self.tray.as_ref() {
                tray.set_menu(Some(Box::new(menu)));
                return;
            }
            let mut builder = TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip(super::TITLE);
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

    fn build(items: &[TrayItem]) -> Menu {
        let menu = Menu::new();
        for item in items {
            let appended = match item {
                TrayItem::Separator => menu.append(&PredefinedMenuItem::separator()),
                TrayItem::Action { id, label: text, enabled } => {
                    menu.append(&MenuItem::with_id(super::item_key(*id), label(text), *enabled, None))
                }
                TrayItem::Check { id, label: text, checked, enabled } => {
                    menu.append(&CheckMenuItem::with_id(super::item_key(*id), label(text), *enabled, *checked, None))
                }
            };
            if let Err(err) = appended {
                log::warn!("[ui] voice tray: menu item: {err}");
            }
        }
        menu
    }
}

/// Elsewhere: no tray.
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod backend {
    use domain::voice::{TrayItem, TrayItemId};

    pub(super) struct Backend;

    impl Backend {
        pub(super) fn new(_clicks: flume::Sender<TrayItemId>) -> Self {
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
    fn the_app_icon_rasterizes() {
        let frames = icon_frames(&[22, 64]);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[1].rgba.len(), 64 * 64 * 4);
        assert!(frames[1].rgba.chunks_exact(4).any(|pixel| pixel[3] > 0), "not blank");
    }
}
