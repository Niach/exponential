//! #EDE-125: a space closes and leaves nothing: no client, no socket, no
//! directory, no global, and once nothing refers to it any more, no seat.

use std::time::{Duration, Instant};

use smithay::{
    backend::input::{ButtonState, InputTime, KeyState},
    input::{keyboard::FilterResult, pointer::ButtonEvent, Seat},
    reexports::wayland_server::backend::DisconnectReason,
    utils::SERIAL_COUNTER,
};

use super::{AgentSpace, Spaces};
use crate::{
    desk::{Desk, DeskId},
    state::Ede,
};

/// The seat of a closed space. smithay's handles look their seat up and
/// panic when it is missing, so it stays until nothing shares it.
pub(crate) struct Closing {
    seat: Seat<Ede>,
    since: Instant,
    logged: bool,
}

/// The directory of a closed space that is still there: what ran in the
/// space may keep it busy for a moment after its clients are gone (GTK
/// apps leave gvfs and document-portal FUSE mounts that unmount by
/// themselves).
pub(crate) struct Leftover {
    dir: std::path::PathBuf,
    ticks: u32,
}

/// How many 1 s ticks a closed space's directory is tried again.
pub const DIR_RETRIES: u32 = 30;

impl Spaces {
    /// Sets where the spaces' directories live, first removing what a
    /// crashed compositor of the same name left there.
    pub fn open(&mut self, dir: std::path::PathBuf) {
        let _ = std::fs::remove_dir_all(&dir);
        self.dir = Some(dir);
    }

    /// No closed space's seat is left to remove.
    pub fn settled(&self) -> bool {
        self.closing.is_empty() && self.leftovers.is_empty()
    }

    /// Removes `dir`; `true` = it is gone.
    fn remove(&self, dir: &std::path::Path) -> bool {
        (self.remove_dir)(dir).is_ok() || !dir.exists()
    }
}

impl Drop for Spaces {
    /// The compositor ends: the sockets go with their sources, the
    /// directories here.
    fn drop(&mut self) {
        // Everything under the spaces' directory is a space's: the live
        // ones and what a close left behind.
        if let Some(dir) = &self.dir {
            self.remove(dir);
        }
    }
}

impl Ede {
    /// Closes space `id`; `false` = no such space (never the person's
    /// desk). Its clients' destructors run later and find no desk: their
    /// tags never fall back to the person's.
    pub fn close_space(&mut self, id: DeskId) -> bool {
        let Some(at) = self.spaces.list.iter().position(|s| s.desk.id == id && id != DeskId::PERSON) else { return false };
        let AgentSpace { id: name, desk, output, output_global, dir, listener, clients, .. } = self.spaces.list.remove(at);

        // Its seat lets go of everything: what is held down, a grab, the focus.
        let time = InputTime::from_millis(crate::ipc::now_ms() as u32);
        let (keyboard, pointer) = (desk.seat.get_keyboard().unwrap(), desk.seat.get_pointer().unwrap());
        for key in keyboard.pressed_keys() {
            keyboard.input::<(), _>(self, key, KeyState::Released, SERIAL_COUNTER.next_serial(), time, |_, _, _| FilterResult::Forward);
        }
        pointer.unset_grab(self, SERIAL_COUNTER.next_serial(), time);
        for (_, button) in desk.driven.buttons.iter().copied() {
            pointer.button(self, &ButtonEvent { button, state: ButtonState::Released, serial: SERIAL_COUNTER.next_serial(), time });
        }
        pointer.frame(self);
        if keyboard.current_focus().is_some() {
            keyboard.set_focus(self, None, SERIAL_COUNTER.next_serial());
        }
        drop((keyboard, pointer));

        // No new client, no old one, nothing on disk.
        self.loop_handle.remove(listener);
        if !self.spaces.remove(&dir) {
            self.spaces.leftovers.push(Leftover { dir, ticks: 0 });
        }
        for client in clients {
            self.display_handle.backend_handle().kill_client(client.id(), DisconnectReason::ConnectionClosed);
        }
        // The removals are told to every client, the person's too: two
        // names they never saw.
        if let Some(global) = desk.seat.global() {
            self.display_handle.remove_global::<Ede>(global);
        }
        self.display_handle.remove_global::<Ede>(output_global);
        // #EDE-126: what still read from its clients answers `unavailable`.
        let Desk { seat, clipboard, .. } = desk;
        self.clipboard_closed(clipboard);
        drop(output);
        self.spaces.closing.push(Closing { seat, since: Instant::now(), logged: false });
        tracing::info!("{} closed", name.as_str());
        true
    }

    /// Once a second: a closed space's seat leaves smithay's list as soon
    /// as nothing but that list and we hold it. One that stays shared is
    /// kept: a leaked seat is cheap, a handle without its seat panics.
    pub fn spaces_tick(&mut self) {
        let mut closing = std::mem::take(&mut self.spaces.closing);
        closing.retain_mut(|c| {
            if self.seat_state.remove_seat(&c.seat) {
                return false;
            }
            if !c.logged && c.since.elapsed() > Duration::from_secs(10) {
                c.logged = true;
                tracing::warn!("{} is still shared 10 s after its space closed; it is kept", c.seat.name());
            }
            true
        });
        closing.append(&mut self.spaces.closing);
        self.spaces.closing = closing;

        // A directory that was busy at the close: again, for a while.
        let mut leftovers = std::mem::take(&mut self.spaces.leftovers);
        leftovers.retain_mut(|l| {
            if self.spaces.remove(&l.dir) {
                return false;
            }
            l.ticks += 1;
            if l.ticks >= DIR_RETRIES {
                tracing::warn!("{} is still there {DIR_RETRIES} s after its space closed; it stays until the compositor ends", l.dir.display());
                return false;
            }
            true
        });
        leftovers.append(&mut self.spaces.leftovers);
        self.spaces.leftovers = leftovers;
    }
}
