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
        wayland_protocols::ext::session_lock::v1::server::ext_session_lock_v1,
        wayland_server::{protocol::wl_output::WlOutput, Resource},
    },
    utils::SERIAL_COUNTER,
    wayland::session_lock::{LockSurface, SessionLockHandler, SessionLockManagerState, SessionLocker},
};

use crate::state::Ede;

#[derive(Debug, Default)]
pub enum LockState {
    #[default]
    Unlocked,
    /// An ext-session-lock client (swaylock and the like).
    Locked(Locked),
    /// The sealed lock client ([`crate::sealed`]).
    Sealed(crate::sealed::Sealed),
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
        matches!(self.lock, LockState::Locked(_) | LockState::Sealed(_))
    }

    /// The lock was confirmed to the client (every output showed it).
    pub fn lock_confirmed(&self) -> bool {
        matches!(&self.lock, LockState::Locked(l) if l.pending.is_none())
    }

    /// The surface the lock shows on `output`: the ext-session-lock
    /// client's, or the sealed client's layer surface.
    pub fn lock_surface_on(&self, output: &Output) -> Option<smithay::reexports::wayland_server::protocol::wl_surface::WlSurface> {
        match &self.lock {
            LockState::Locked(l) => l.surfaces.iter().find(|(o, s)| o == output && s.alive()).map(|(_, s)| s.wl_surface().clone()),
            LockState::Sealed(_) => self.sealed_lock_surface_on(output),
            LockState::Unlocked => None,
        }
    }

    pub fn is_lock_surface(&self, surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface) -> bool {
        match &self.lock {
            LockState::Locked(l) => l.surfaces.iter().any(|(_, s)| s.wl_surface() == surface),
            LockState::Sealed(_) => self.sealed_lock_surfaces().iter().any(|(_, s)| s == surface),
            LockState::Unlocked => false,
        }
    }

    /// Starts the lock (Super+L, idle, suspend): `[lock] command` when
    /// configured (an ext-session-lock client), else the sealed lock
    /// client (#EDE-17). Nothing when already locked.
    pub fn request_lock(&mut self) {
        if self.is_locked() {
            return;
        }
        if self.config.lock.command.is_empty() {
            self.begin_sealed_lock();
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
            self.emit(ede_protocol::events::SESSION_LOCKED, serde_json::Value::Null);
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
        if matches!(self.lock, LockState::Sealed(_)) {
            self.focus_sealed_lock();
            return;
        }
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

    pub(crate) fn enter_lock(&mut self) {
        let serial = SERIAL_COUNTER.next_serial();
        let pointer = self.seat.get_pointer().unwrap();
        // A drag in progress ends; nothing behind the lock keeps input.
        if pointer.is_grabbed() {
            pointer.unset_grab(self, serial, smithay::backend::input::InputTime::from_millis(0));
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
            LockState::Sealed(_) => {
                tracing::warn!("an ext-session-lock client was refused: the sealed lock holds the session");
            }
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
        // smithay only calls it for the lock object holding the
        // (confirmed) lock.
        self.lock = LockState::Unlocked;
        self.idle.activity();
        tracing::info!("session unlocked");
        self.emit(ede_protocol::events::SESSION_UNLOCKED, serde_json::Value::Null);
        let top = self.space.elements().last().cloned();
        self.focus(top);
        let pos = self.seat.get_pointer().unwrap().current_location();
        self.pointer_motion_to(pos, 0);
    }

    fn new_surface(&mut self, surface: LockSurface, wl_output: WlOutput) {
        let Some(output) = Output::from_resource(&wl_output) else { return };
        let LockState::Locked(l) = &mut self.lock else { return };
        if surface.ext_session_lock() == &l.lock {
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

