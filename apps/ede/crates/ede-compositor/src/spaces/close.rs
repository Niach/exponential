//! #EDE-125: a space closes and leaves nothing: no client, no socket, no
//! directory, no global, and once nothing refers to it any more, no seat.

use std::{
    path::PathBuf,
    process::Child,
    time::{Duration, Instant},
};

use smithay::{
    backend::input::{ButtonState, InputTime, KeyState},
    input::{keyboard::FilterResult, pointer::ButtonEvent, Seat},
    reexports::wayland_server::backend::DisconnectReason,
    utils::SERIAL_COUNTER,
};

use super::{scope::Scope, slice, AgentSpace, Spaces};
use crate::{
    desk::{Desk, DeskId},
    state::Ede,
};

/// A grant lives 15 minutes by the wall clock and the ticks stand still
/// during a suspend: without a grace the first tick after a long one
/// would close every space before its owner could renew.
pub const EXPIRY_GRACE: u32 = 30;

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

/// The slice of a closed space (or of a compositor that is gone) while
/// it ends (#EDE-138). The user manager stops it: SIGTERM to every process
/// of every unit in it, SIGKILL two seconds later. A scope that asked to
/// be left alone at a stop (`KillMode=none`) keeps running, and a launch
/// that was still on its way into its scope arrives after the stop: what
/// is in the slice's cgroup after [`STOP_GRACE`] is ended by the kernel,
/// and the slice is stopped once more.
pub(crate) struct Stopping {
    unit: String,
    /// `systemctl stop`, until it is reaped; never waited for.
    stop: Option<Child>,
    /// The slice's cgroup directory.
    cgroup: PathBuf,
    /// It was there when the stop was asked for.
    seen: bool,
    since: Instant,
    /// The grace is over (or was not needed): nothing more is done.
    done: bool,
}

/// After a space closed, how long its slice's processes get to end by the
/// user manager's stop before the kernel ends what is left.
const STOP_GRACE: Duration = Duration::from_millis(2500);

impl Spaces {
    /// Sets where the spaces' directories live, first removing what a
    /// crashed compositor of the same name left there.
    pub fn open(&mut self, dir: std::path::PathBuf) {
        let _ = std::fs::remove_dir_all(&dir);
        self.dir = Some(dir);
    }

    /// No closed space's seat is left to remove.
    pub fn settled(&self) -> bool {
        self.closing.is_empty() && self.leftovers.is_empty() && self.stopping.is_empty()
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
        self.kill_launches();
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
        let AgentSpace { id: name, desk, output, output_global, dir, listener, clients, sliced, .. } = self.spaces.list.remove(at);

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
        // What was launched is told to end first: its home is in `dir`.
        self.space_launches_closed(id);
        if sliced {
            self.space_slice_closed(id.number());
        }
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

    /// #EDE-138, at the session's start: finds the person's user manager,
    /// so launches run in scopes of their spaces' slices, and stops the
    /// slices a compositor that is gone left behind (their spaces'
    /// processes would run on). Without a user manager the reason is
    /// logged once and launches run as before, for the whole run.
    pub fn start_space_units(&mut self) {
        let units = match slice::Units::probe() {
            Ok(units) => units,
            Err(reason) => {
                tracing::warn!("agent spaces run in the login session, without limits: {reason} (a systemd user manager from {} on is needed)", slice::SYSTEMD_FLOOR);
                return;
            }
        };
        let own = ede_protocol::process::spaces_slice(units.compositor);
        for unit in units.stale() {
            tracing::warn!("{unit} is a gone compositor's: its spaces' processes are stopped");
            let cgroup = units.app_slice.join(&unit);
            if unit == own {
                // An earlier compositor had this process id: gone before
                // this one makes a slice of that name.
                slice::kill_cgroup(&cgroup);
                units.stop_now(&unit, Duration::from_secs(10));
            } else {
                let (stop, seen) = (units.stop(&unit), cgroup.is_dir());
                self.spaces.stopping.push(Stopping { unit, stop, cgroup, seen, since: Instant::now(), done: false });
            }
        }
        tracing::info!("agent spaces run in systemd scopes (systemd {})", units.version);
        self.spaces.units = Some(units);
    }

    /// Space number `space` closed and had launches in scopes: its slice
    /// is stopped with all in it. Nothing here waits for the user manager.
    fn space_slice_closed(&mut self, space: u32) {
        let Some(units) = &self.spaces.units else { return };
        let unit = ede_protocol::process::space_slice(units.compositor, space);
        let cgroup = units.space_cgroup(space);
        let (stop, seen) = (units.stop(&unit), cgroup.is_dir());
        self.spaces.stopping.push(Stopping { unit, stop, cgroup, seen, since: Instant::now(), done: false });
    }

    /// Once a second: what a stopped slice still holds after its grace is
    /// ended by the kernel, and the slice is stopped once more.
    pub(crate) fn space_slices_tick(&mut self) {
        let Some(units) = &self.spaces.units else { return };
        self.spaces.stopping.retain_mut(|stopping| {
            if stopping.stop.as_mut().is_some_and(|stop| !matches!(stop.try_wait(), Ok(None))) {
                stopping.stop = None;
            }
            if stopping.done {
                return stopping.stop.is_some();
            }
            // The stop is done and the slice that was there is gone: no
            // need to wait for the grace. (One that was not there yet may
            // still come: a launch on its way into its scope.)
            if stopping.seen && stopping.stop.is_none() && !stopping.cgroup.is_dir() {
                return false;
            }
            if stopping.since.elapsed() > STOP_GRACE {
                stopping.done = true;
                if slice::kill_cgroup(&stopping.cgroup) {
                    tracing::info!("{} outlived its stop: the kernel ended what was left in it", stopping.unit);
                    if stopping.stop.is_none() {
                        stopping.stop = units.stop(&stopping.unit);
                    }
                }
            }
            true
        });
    }

    /// Does `session` hold a `[space]` grant that has not expired?
    fn has_space_grant(&self, session: &ede_protocol::SessionId) -> bool {
        let now = crate::ipc::now_ms();
        self.ipc.grants.iter().any(|g| g.expires_at.is_none_or(|e| now < e) && Scope::of(g) == Scope::Space(session.clone()))
    }

    /// A grant of `session` was revoked: its space closes at once, unless
    /// the session still holds another live space grant.
    pub(crate) fn space_grant_revoked(&mut self, session: &ede_protocol::SessionId) {
        if self.has_space_grant(session) {
            return;
        }
        let desk = self.spaces.iter().find(|s| &s.owner == session).map(|s| s.desk.id);
        if let Some(desk) = desk {
            self.close_space(desk);
        }
    }

    /// A space whose owner has had no live space grant for
    /// `expiry_grace` ticks in a row closes; a grant in time starts the
    /// count over.
    fn spaces_expire(&mut self) {
        let owners: Vec<_> = self.spaces.iter().map(|s| (s.desk.id, s.owner.clone())).collect();
        for (desk, owner) in owners {
            let granted = self.has_space_grant(&owner);
            let grace = self.spaces.expiry_grace;
            let Some(space) = self.spaces.get_mut(desk) else { continue };
            space.expired_ticks = if granted { 0 } else { space.expired_ticks + 1 };
            if space.expired_ticks >= grace {
                tracing::info!("{}: no grant for {grace} ticks", space.id.as_str());
                self.close_space(desk);
            }
        }
    }

    /// Once a second: a closed space's seat leaves smithay's list as soon
    /// as nothing but that list and we hold it. One that stays shared is
    /// kept: a leaked seat is cheap, a handle without its seat panics.
    pub fn spaces_tick(&mut self) {
        self.spaces_expire();
        self.space_launches_tick();
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
