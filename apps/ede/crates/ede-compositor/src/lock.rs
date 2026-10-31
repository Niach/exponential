//! ext-session-lock: the screen lock.
//!
//! The lock screen itself is a client (the eUI shell's, later; `swaylock`
//! works today). The compositor's half is what makes it a lock: from the
//! `lock` request on, every output draws only that client's lock surface (or
//! black), only it gets keyboard and pointer input, compositor bindings are
//! off except VT switching, and the lock is confirmed only after every output
//! presented a locked frame. If the lock client dies the session stays
//! locked (black) until a new lock client takes over and unlocks.

use smithay::{
    output::Output,
    reexports::{
        wayland_protocols::ext::session_lock::v1::server::{ext_session_lock_surface_v1::ExtSessionLockSurfaceV1, ext_session_lock_v1},
        wayland_server::{protocol::wl_output::WlOutput, Client, DataInit, Dispatch, DisplayHandle, Resource},
    },
    utils::SERIAL_COUNTER,
    wayland::session_lock::{LockSurface, SessionLockHandler, SessionLockManagerState, SessionLockState, SessionLocker},
};

use crate::state::Ede;

#[derive(Debug, Default)]
pub enum LockState {
    #[default]
    Unlocked,
    Locked(Locked),
}

#[derive(Debug)]
pub struct Locked {
    /// The lock object of the client holding the lock. Dead = the lock
    /// client crashed: still locked, and the next lock client takes over.
    pub lock: ext_session_lock_v1::ExtSessionLockV1,
    /// Sent once every output presented a locked frame.
    pub pending: Option<SessionLocker>,
    pub shown_on: Vec<Output>,
    pub surfaces: Vec<(Output, LockSurface)>,
}

impl Ede {
    pub fn is_locked(&self) -> bool {
        matches!(self.lock, LockState::Locked(_))
    }

    /// The lock was confirmed to the client (every output showed it).
    pub fn lock_confirmed(&self) -> bool {
        matches!(&self.lock, LockState::Locked(l) if l.pending.is_none())
    }

    pub fn lock_surface_on(&self, output: &Output) -> Option<&LockSurface> {
        let LockState::Locked(l) = &self.lock else { return None };
        l.surfaces.iter().find(|(o, s)| o == output && s.alive()).map(|(_, s)| s)
    }

    pub fn is_lock_surface(&self, surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface) -> bool {
        let LockState::Locked(l) = &self.lock else { return false };
        l.surfaces.iter().any(|(_, s)| s.wl_surface() == surface)
    }

    /// Starts the configured lock client (Super+L, idle, suspend). Nothing
    /// happens without `[lock] command`, or when already locked.
    pub fn request_lock(&mut self) {
        if self.is_locked() {
            return;
        }
        if self.config.lock.command.is_empty() {
            tracing::info!("lock requested, but no [lock] command is configured");
            return;
        }
        let argv = self.config.lock.command.clone();
        self.spawn(&argv);
    }

    /// A frame of `output` went out: lock surfaces draw on, and once every
    /// output showed a locked frame the client hears `locked`.
    pub fn lock_frame_done(&mut self, output: &Output, now: std::time::Duration) {
        let LockState::Locked(l) = &mut self.lock else { return };
        for (o, s) in &l.surfaces {
            if o == output {
                smithay::desktop::utils::send_frames_surface_tree(s.wl_surface(), output, now, Some(std::time::Duration::ZERO), |_, _| {
                    Some(output.clone())
                });
            }
        }
        if !l.shown_on.contains(output) {
            l.shown_on.push(output.clone());
        }
        self.confirm_lock_if_shown();
    }

    fn confirm_lock_if_shown(&mut self) {
        let outputs: Vec<Output> = self.space.outputs().cloned().collect();
        let LockState::Locked(l) = &mut self.lock else { return };
        if l.pending.is_some() && outputs.iter().all(|o| l.shown_on.contains(o)) {
            l.pending.take().unwrap().lock();
            tracing::info!("session locked");
        }
    }

    /// An output went away while locked: forget its lock surface.
    pub fn lock_output_removed(&mut self, output: &Output) {
        if let LockState::Locked(l) = &mut self.lock {
            l.surfaces.retain(|(o, _)| o != output);
            l.shown_on.retain(|o| o != output);
        }
        self.confirm_lock_if_shown();
    }

    /// Keyboard focus goes to the lock surface of the output under the
    /// pointer (else any).
    fn focus_lock_surface(&mut self) {
        let pointer = self.seat.get_pointer().unwrap();
        let output = self.output_at(pointer.current_location());
        let surface = {
            let LockState::Locked(l) = &self.lock else { return };
            let alive = || l.surfaces.iter().filter(|(_, s)| s.alive());
            alive().find(|(o, _)| Some(o) == output.as_ref()).or_else(|| alive().next()).map(|(_, s)| s.wl_surface().clone())
        };
        let keyboard = self.seat.get_keyboard().unwrap();
        if keyboard.current_focus() != surface {
            keyboard.set_focus(self, surface, SERIAL_COUNTER.next_serial());
        }
    }

    /// Called on a click while locked: focus follows the pointer's output.
    pub fn lock_pointer_focus(&mut self) {
        self.focus_lock_surface();
    }

    fn enter_lock(&mut self) {
        let serial = SERIAL_COUNTER.next_serial();
        let pointer = self.seat.get_pointer().unwrap();
        // A drag in progress ends; nothing behind the lock keeps input.
        if pointer.is_grabbed() {
            pointer.unset_grab(self, serial, 0);
        }
        let keyboard = self.seat.get_keyboard().unwrap();
        if keyboard.is_grabbed() {
            // The input method's grab too: it must not read the password.
            keyboard.unset_grab(self);
        }
        #[cfg(feature = "xwayland")]
        {
            self.pending_x11_focus = None;
        }
        keyboard.set_focus(self, None, serial);
        let pos = pointer.current_location();
        self.pointer_motion_to(pos, 0);
        if self.space.outputs().next().is_none() {
            self.confirm_lock_if_shown();
        }
    }
}

impl SessionLockHandler for Ede {
    fn lock_state(&mut self) -> &mut SessionLockManagerState {
        &mut self.session_lock_state
    }

    fn lock(&mut self, confirmation: SessionLocker) {
        let lock = confirmation.ext_session_lock().clone();
        match &mut self.lock {
            LockState::Locked(l) if l.lock.is_alive() => {
                // Someone else holds the lock: dropping the locker answers
                // `finished`.
                tracing::warn!("a second lock client was refused");
            }
            LockState::Locked(l) => {
                // The lock client died; the screen is already locked, so the
                // new one takes over at once.
                tracing::info!("a new lock client took over");
                l.lock = lock;
                l.surfaces.clear();
                confirmation.lock();
            }
            LockState::Unlocked => {
                self.lock = LockState::Locked(Locked { lock, pending: Some(confirmation), shown_on: Vec::new(), surfaces: Vec::new() });
                self.enter_lock();
            }
        }
    }

    fn unlock(&mut self) {
        // Only reached for the client holding the lock (see the dispatch
        // guard below).
        self.lock = LockState::Unlocked;
        self.idle.activity();
        tracing::info!("session unlocked");
        let top = self.space.elements().last().cloned();
        self.focus(top);
        let pos = self.seat.get_pointer().unwrap().current_location();
        self.pointer_motion_to(pos, 0);
    }

    fn new_surface(&mut self, surface: LockSurface, wl_output: WlOutput) {
        let Some(output) = Output::from_resource(&wl_output) else { return };
        let LockState::Locked(l) = &mut self.lock else { return };
        if l.lock.id().same_client_as(&surface.wl_surface().id()) {
            if let Some(mode) = output.current_mode() {
                let size = mode.size.to_f64().to_logical(output.current_scale().fractional_scale()).to_i32_round::<i32>();
                surface.with_pending_state(|s| s.size = Some((size.w as u32, size.h as u32).into()));
            }
            surface.send_configure();
            l.surfaces.retain(|(o, _)| o != &output);
            l.surfaces.push((output, surface));
            self.focus_lock_surface();
        }
    }
}

// smithay's own dispatch for ext_session_lock_v1, behind one guard: smithay
// calls `unlock` even for a lock object that never got `locked` (it only
// posts a protocol error), so a refused second client could unlock the
// session. Here only the client holding the lock reaches it.
impl Dispatch<ext_session_lock_v1::ExtSessionLockV1, SessionLockState> for Ede {
    fn request(
        state: &mut Self,
        client: &Client,
        resource: &ext_session_lock_v1::ExtSessionLockV1,
        request: ext_session_lock_v1::Request,
        data: &SessionLockState,
        dh: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        let holder = matches!(&state.lock, LockState::Locked(l) if l.lock == *resource);
        let request = match request {
            ext_session_lock_v1::Request::UnlockAndDestroy if !holder || !state.lock_confirmed() => {
                resource.post_error(ext_session_lock_v1::Error::InvalidUnlock, "this lock does not hold the session");
                return;
            }
            // A refused (`finished`) lock object has nothing to draw on.
            ext_session_lock_v1::Request::GetLockSurface { id, .. } if !holder => {
                data_init.post_error(id, ext_session_lock_v1::Error::InvalidUnlock as u32, "this lock was finished");
                return;
            }
            request => request,
        };
        <SessionLockManagerState as Dispatch<ext_session_lock_v1::ExtSessionLockV1, SessionLockState, Self>>::request(
            state, client, resource, request, data, dh, data_init,
        );
    }
}

smithay::reexports::wayland_server::delegate_global_dispatch!(Ede: [
    smithay::reexports::wayland_protocols::ext::session_lock::v1::server::ext_session_lock_manager_v1::ExtSessionLockManagerV1: smithay::wayland::session_lock::SessionLockManagerGlobalData
] => SessionLockManagerState);
smithay::reexports::wayland_server::delegate_dispatch!(Ede: [
    smithay::reexports::wayland_protocols::ext::session_lock::v1::server::ext_session_lock_manager_v1::ExtSessionLockManagerV1: ()
] => SessionLockManagerState);
smithay::reexports::wayland_server::delegate_dispatch!(Ede: [
    ExtSessionLockSurfaceV1: smithay::wayland::session_lock::ExtLockSurfaceUserData
] => SessionLockManagerState);
