//! Input methods (ibus, fcitx5): input-method-v2 for the IME, text-input-v3
//! for apps, and the virtual keyboard the IME forwards keys through.
//! Protected surfaces and the lock screen stay out of reach of all three
//! (see [`crate::protected`]).
//!
//! #EDE-172 (P3, design 3.5): the virtual keyboard and the input-method
//! manager type into whatever has the person's focus, so only an installed
//! input method sees them. A client is one when the process that connected
//! the person's Wayland socket ([`crate::peer::Peer`], taken before its
//! first request) runs one of the root-owned files of [`DEFAULTS`] or of
//! the root-owned [`LIST`], in the compositor's user namespace
//! ([`crate::peer::Installed`]). A copy or a rename anywhere else, `wtype`,
//! a flatpak (another user namespace) and the compositor's own clients see
//! neither. text-input-v3 stays every app's.
//!
//! What this does not stop (design, left open 1): the genuine input method
//! run with code of someone else's (`LD_PRELOAD`, its own plugins), and
//! what an input method types on another process's word over its own
//! interfaces (an ibus engine on ibus's bus).

use std::{
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

use smithay::{
    backend::input::{Event, InputEvent, KeyboardKeyEvent},
    desktop::{PopupKind, PopupManager},
    input::keyboard::XkbConfig,
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Client},
    utils::{Logical, Rectangle, SERIAL_COUNTER},
    wayland::{
        input_method::{InputMethodHandler, InputMethodSeat, PopupSurface},
        text_input::{TextInputActivation, TextInputSeat},
        virtual_keyboard::{VirtualKeyboardBackend, VirtualKeyboardDevice, VirtualKeyboardHandler, VirtualKeyboardSpecialEvent},
    },
};

use crate::{
    desk::{on_person, DeskId},
    peer::{root_owned, Installed, Peer},
    state::{ClientState, Ede},
};

/// The input methods beyond [`DEFAULTS`]: `paths = ["/usr/bin/…"]`, each a
/// file root owns. The list counts only while it is a file root owns that
/// no one else may write, in directories the same holds for.
pub const LIST: &str = "/etc/exponential-desktop/input-methods.toml";

/// The processes that bind the two globals, where the packages put them.
pub const DEFAULTS: [&str; 8] = [
    // fcitx5: its Wayland frontend (waylandim) runs inside the daemon.
    "/usr/bin/fcitx5",
    // ibus: `ibus start` looks for the input-method manager before it
    // starts the panel with `--enable-wayland-im` (1.5.30 on), which binds
    // both; ibus-wayland is its own frontend. Arch keeps both in /usr/lib/ibus.
    "/usr/bin/ibus",
    "/usr/libexec/ibus-ui-gtk3",
    "/usr/libexec/ibus-wayland",
    "/usr/lib/ibus/ibus-ui-gtk3",
    "/usr/lib/ibus/ibus-wayland",
    // The on-screen keyboards. maliit-server's Wayland frontend speaks
    // input-method-v1 today, which eDE does not offer.
    "/usr/bin/squeekboard",
    "/usr/bin/maliit-server",
];

/// Every input method has this one name in [`Installed`].
const NAME: &str = "input-method";

/// How [`LIST`] was when it was read: whether it counted, then its
/// `(st_dev, st_ino, mtime, mtime_nsec, size, uid, mode)`.
type Stamp = (bool, u64, u64, i64, i64, u64, u32, u32);

/// The installed input methods ([`DEFAULTS`] and [`LIST`]).
#[derive(Debug, Clone)]
pub struct InputMethods {
    defaults: Vec<PathBuf>,
    /// The list it reads again whenever the file changed; `None` = none.
    list: Option<PathBuf>,
    stamp: Option<Stamp>,
    /// The defaults and what the list named.
    paths: Vec<PathBuf>,
    known: Installed,
}

impl InputMethods {
    /// [`DEFAULTS`] and [`LIST`]: the session's.
    pub fn system() -> InputMethods {
        InputMethods::of(&DEFAULTS.map(PathBuf::from)).with_list(PathBuf::from(LIST))
    }

    /// Exactly `paths`, no list (a test's).
    pub fn of(paths: &[PathBuf]) -> InputMethods {
        InputMethods { defaults: paths.to_vec(), list: None, stamp: None, paths: paths.to_vec(), known: installed(paths) }
    }

    /// The same, with the further input methods `list` names.
    pub fn with_list(mut self, list: PathBuf) -> InputMethods {
        self.list = Some(list);
        self.stamp = None;
        self.set(Vec::new());
        self.refresh();
        self
    }

    /// Is `peer` an installed input method? A process named like one that
    /// runs another file is not, and the log says so.
    pub fn admits(&mut self, peer: &Peer) -> bool {
        self.refresh();
        let path = peer.exe_path().unwrap_or_default();
        if self.known.is(peer, NAME) {
            tracing::info!("input method {} (pid {}): the virtual keyboard and the input-method manager", path.display(), peer.pid);
            return true;
        }
        if path.file_name().is_some_and(|name| self.paths.iter().any(|known| known.file_name() == Some(name))) {
            tracing::warn!("{} (pid {}) is not an installed input method: no virtual keyboard, no input-method manager (#EDE-172)", path.display(), peer.pid);
        }
        false
    }

    /// Reads [`LIST`] again when it changed (or began or ceased to count).
    fn refresh(&mut self) {
        let Some(list) = self.list.clone() else { return };
        let stamp = stamp_of(&list);
        if stamp == self.stamp {
            return;
        }
        self.stamp = stamp;
        let listed = match stamp {
            None => Vec::new(),
            Some((false, ..)) => {
                tracing::warn!("{}: ignored; it counts only as a file root owns and only root may write (#EDE-172)", list.display());
                Vec::new()
            }
            Some((true, ..)) => match std::fs::read_to_string(&list).map_err(|e| e.to_string()).and_then(|text| parse_list(&text)) {
                Ok(paths) => {
                    tracing::info!("input methods from {}: {paths:?}", list.display());
                    paths
                }
                Err(e) => {
                    tracing::warn!("{}: ignored: {e}", list.display());
                    Vec::new()
                }
            },
        };
        self.set(listed);
    }

    fn set(&mut self, listed: Vec<PathBuf>) {
        self.paths = self.defaults.iter().cloned().chain(listed).collect();
        self.known = installed(&self.paths);
    }
}

fn installed(paths: &[PathBuf]) -> Installed {
    Installed::of(&paths.iter().map(|p| (NAME.to_string(), p.clone())).collect::<Vec<_>>())
}

fn stamp_of(path: &Path) -> Option<Stamp> {
    let m = std::fs::metadata(path).ok()?;
    Some((root_owned(path).is_some(), m.dev(), m.ino(), m.mtime(), m.mtime_nsec(), m.size(), m.uid(), m.mode()))
}

/// [`LIST`]'s text: `paths`, each absolute; anything else in it voids it.
pub fn parse_list(text: &str) -> Result<Vec<PathBuf>, String> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct List {
        #[serde(default)]
        paths: Vec<PathBuf>,
    }
    let list: List = toml::from_str(text).map_err(|e| e.to_string())?;
    match list.paths.iter().find(|p| !p.is_absolute()) {
        Some(p) => Err(format!("{} is not an absolute path", p.display())),
        None => Ok(list.paths),
    }
}

/// The filter of the virtual keyboard and input-method managers: a client
/// of the person's socket that connected as an installed input method
/// ([`InputMethods::admits`], decided in [`Ede::insert_person_client`]).
pub(crate) fn on_person_input_method(client: &Client) -> bool {
    on_person(client) && client.get_data::<ClientState>().is_some_and(|state| state.input_method)
}

impl InputMethodHandler for Ede {
    fn new_popup(&mut self, surface: PopupSurface) {
        if let Err(e) = self.popups.track_popup(PopupKind::from(surface)) {
            tracing::warn!("input method popup: {e}");
        }
    }

    fn popup_repositioned(&mut self, _surface: PopupSurface) {}

    fn dismiss_popup(&mut self, surface: PopupSurface) {
        if let Some(parent) = surface.get_parent().map(|p| p.surface.clone()) {
            let _ = PopupManager::dismiss_popup(&parent, &PopupKind::from(surface));
        }
    }

    fn parent_geometry(&self, parent: &WlSurface) -> Rectangle<i32, Logical> {
        self.window_for_surface(parent).and_then(|w| self.desk.space.element_geometry(&w)).unwrap_or_default()
    }
}

// text-input-v3 through smithay, except that a protected surface (or the
// lock screen) never turns the input method on: the IME neither sees its
// text nor commits into it. smithay drops text-input requests from a client
// whose surface is not the text-input focus, so a protected keyboard focus
// takes that focus away (see `focus_changed`).
impl TextInputActivation for Ede {}

impl Ede {
    pub(crate) fn keep_text_input_off_protected(&mut self, focused: Option<&WlSurface>) {
        if !(self.is_locked() || focused.is_some_and(|s| self.is_protected_surface(s))) {
            return;
        }
        let text_input = self.desk.seat.text_input();
        if self.desk.seat.input_method().has_instance() || text_input.compositor_input_method() {
            text_input.leave();
        }
        text_input.set_focus(None);
    }

    /// A real key goes under the configured keymap again after a virtual
    /// keyboard swapped in its own.
    pub(crate) fn restore_seat_keymap(&mut self) {
        if self.desk.virtual_keymap.take().is_none() {
            return;
        }
        let kb = self.config.keyboard.clone();
        let xkb = XkbConfig { layout: &kb.layout, variant: &kb.variant, options: Some(kb.options.clone()).filter(|o| !o.is_empty()), ..Default::default() };
        if let Err(e) = self.desk.seat.get_keyboard().unwrap().set_xkb_config(self, xkb) {
            tracing::warn!("keyboard layout {:?} not restored: {e:?}", kb.layout);
        }
    }

    /// The device's keycodes only mean anything under its own keymap (none =
    /// it uses the seat's).
    fn use_virtual_keymap(&mut self, device: &VirtualKeyboardDevice) {
        match device.keymap() {
            Some(keymap) if self.desk.virtual_keymap != Some(device.source()) => {
                let keyboard = self.desk.seat.get_keyboard().unwrap();
                match keyboard.set_keymap_from_keymap_file(self, keymap) {
                    Ok(()) => self.desk.virtual_keymap = Some(device.source()),
                    Err(e) => tracing::warn!("virtual keyboard keymap: {e:?}"),
                }
            }
            Some(_) => {}
            None => self.restore_seat_keymap(),
        }
    }
}

// The virtual keyboard types straight into whatever has focus (no compositor
// bindings, no input-method grab, not person activity), so its keys stop at
// protected surfaces and the lock screen.
impl VirtualKeyboardHandler for Ede {
    fn process_virtual_keyboard_event(&mut self, event: InputEvent<VirtualKeyboardBackend>) {
        let keyboard = self.desk.seat.get_keyboard().unwrap();
        match event {
            InputEvent::Keyboard { event } => {
                if self.keyboard_focus_protected(DeskId::PERSON) {
                    return;
                }
                let device = event.device();
                self.use_virtual_keymap(&device);
                let (code, state) = (event.key_code(), event.state());
                // #EDE-172 (P3): the input method that holds the keyboard
                // grab hands back a key the grab took (fcitx5, ibus), often
                // while the person still holds it. As another source of the
                // same key it was swallowed, and so was the person's release,
                // which then never reached the input method: it repeated the
                // key for ever. It goes to the focus as it is (smithay's way).
                let grab = self.desk.seat.input_method().keyboard_grab_client();
                if grab.is_some() && grab == device.client() {
                    keyboard.input_forward_bypassing_grab(self, code, state, SERIAL_COUNTER.next_serial(), event.time(), true);
                    return;
                }
                let Some(((), mods_changed)) = keyboard.input_intercept(device.source(), self, code, state, |_, _, _| ()) else { return };
                keyboard.input_forward_bypassing_grab(self, code, state, SERIAL_COUNTER.next_serial(), event.time(), mods_changed);
            }
            InputEvent::Special(VirtualKeyboardSpecialEvent::Modifiers { device, mods_depressed, mods_latched, mods_locked, group }) => {
                if self.keyboard_focus_protected(DeskId::PERSON) {
                    return;
                }
                self.use_virtual_keymap(&device);
                keyboard.with_xkb_state(self, |mut xkb| xkb.set_modifier_mask(mods_depressed, mods_latched, mods_locked, group));
            }
            InputEvent::Special(VirtualKeyboardSpecialEvent::KeymapChanged { device }) => {
                if self.desk.virtual_keymap == Some(device.source()) {
                    self.desk.virtual_keymap = None;
                }
            }
            InputEvent::DeviceRemoved { device } => {
                keyboard.release_source(self, device.source());
                if self.desk.virtual_keymap == Some(device.source()) {
                    self.restore_seat_keymap();
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_names_absolute_paths_and_nothing_else() {
        assert_eq!(parse_list("paths = [\"/usr/bin/wvkbd-mobintl\", \"/opt/ime/bin/ime\"]\n"), Ok(vec![PathBuf::from("/usr/bin/wvkbd-mobintl"), PathBuf::from("/opt/ime/bin/ime")]));
        assert_eq!(parse_list("# none\n"), Ok(vec![]));
        assert!(parse_list("paths = [\"bin/ime\"]").is_err(), "a relative path voids the list");
        assert!(parse_list("path = [\"/usr/bin/ime\"]").is_err(), "a key it does not know voids the list");
        assert!(parse_list("paths = \"/usr/bin/ime\"").is_err());
    }

    #[test]
    fn a_missing_list_or_one_the_person_writes_adds_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let defaults = DEFAULTS.map(PathBuf::from).to_vec();
        let list = dir.path().join("input-methods.toml");
        let mut methods = InputMethods::system().with_list(list.clone());
        assert_eq!(methods.paths, defaults, "no list");
        std::fs::write(&list, "paths = [\"/usr/bin/sleep\"]\n").unwrap();
        methods.refresh();
        assert_eq!(methods.paths, defaults, "a list the person owns counts for nothing");
    }
}
