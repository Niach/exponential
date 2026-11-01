//! Idle: ext-idle-notify for clients (the shell blanks screens and suspends
//! through it), zwp-idle-inhibit (a playing video, a game) and the
//! compositor's own lock after `[idle] lock_after` seconds without input.

use std::time::{Duration, Instant};

use smithay::{
    delegate_idle_inhibit, delegate_idle_notify,
    reexports::{
        calloop::{
            timer::{TimeoutAction, Timer},
            LoopHandle,
        },
        wayland_server::{protocol::wl_surface::WlSurface, Resource},
    },
    wayland::{
        compositor::get_parent,
        idle_inhibit::IdleInhibitHandler,
        idle_notify::{IdleNotifierHandler, IdleNotifierState},
    },
};

use crate::state::Ede;

#[derive(Debug)]
pub struct Idle {
    pub last_activity: Instant,
    /// The idle lock already ran for this stretch of idleness.
    fired: bool,
    /// Surfaces holding an idle inhibitor; they count while visible.
    pub inhibitors: Vec<WlSurface>,
}

impl Default for Idle {
    fn default() -> Self {
        Self { last_activity: Instant::now(), fired: false, inhibitors: Vec::new() }
    }
}

impl Idle {
    pub fn activity(&mut self) {
        self.last_activity = Instant::now();
        self.fired = false;
    }
}

/// Checks idleness once a second.
pub(crate) fn start_timer(loop_handle: &LoopHandle<'static, Ede>) {
    loop_handle
        .insert_source(Timer::from_duration(Duration::from_secs(1)), |_, _, state| {
            state.idle_tick();
            TimeoutAction::ToDuration(Duration::from_secs(1))
        })
        .expect("idle timer");
}

impl Ede {
    /// Real input from the person (never synthetic or agent input).
    pub fn note_activity(&mut self) {
        self.idle.activity();
        let seat = self.seat.clone();
        self.idle_notifier.notify_activity(&seat);
    }

    /// An inhibitor counts while its surface is on screen: part of a mapped
    /// window or layer surface (not minimized, not behind the lock).
    pub fn idle_inhibited(&self) -> bool {
        !self.is_locked()
            && self.idle.inhibitors.iter().any(|s| {
                let mut root = s.clone();
                while let Some(parent) = get_parent(&root) {
                    root = parent;
                }
                s.is_alive()
                    && (self.space.elements().any(|w| smithay::wayland::seat::WaylandFocus::wl_surface(w).is_some_and(|w| *w == root))
                        || self.space.outputs().any(|o| {
                            smithay::desktop::layer_map_for_output(o)
                                .layer_for_surface(&root, smithay::desktop::WindowSurfaceType::TOPLEVEL)
                                .is_some()
                        }))
            })
    }

    pub fn idle_tick(&mut self) {
        self.idle.inhibitors.retain(|s| s.is_alive());
        let inhibited = self.idle_inhibited();
        self.idle_notifier.set_is_inhibited(inhibited);
        if inhibited {
            self.idle.activity();
            return;
        }
        let after = self.config.idle.lock_after;
        if after > 0 && !self.idle.fired && !self.is_locked() && self.idle.last_activity.elapsed() >= Duration::from_secs(after) {
            self.idle.fired = true;
            tracing::info!("idle for {after}s: locking");
            self.request_lock();
        }
    }
}

impl IdleNotifierHandler for Ede {
    fn idle_notifier_state(&mut self) -> &mut IdleNotifierState<Self> {
        &mut self.idle_notifier
    }
}
delegate_idle_notify!(Ede);

impl IdleInhibitHandler for Ede {
    fn inhibit(&mut self, surface: WlSurface) {
        self.idle.inhibitors.push(surface);
        let inhibited = self.idle_inhibited();
        self.idle_notifier.set_is_inhibited(inhibited);
    }

    fn uninhibit(&mut self, surface: WlSurface) {
        if let Some(i) = self.idle.inhibitors.iter().position(|s| *s == surface) {
            self.idle.inhibitors.remove(i);
        }
        let inhibited = self.idle_inhibited();
        self.idle_notifier.set_is_inhibited(inhibited);
    }
}
delegate_idle_inhibit!(Ede);
