//! #EDE-162: a space's apps present at most once per tick of its clock.
//! Frame callbacks go out only at ticks, which holds every app that waits
//! for them (FIFO, a swap interval of 1, a browser's animation frames). An
//! app that does not (vsync off, MAILBOX or IMMEDIATE, a game's own loop)
//! would otherwise render as fast as its GPU lets it, on the person's GPU:
//! so the second new buffer a surface commits within one tick waits as a
//! blocker until the next tick, one let through per tick, at most
//! [`HELD_MAX`] waiting (a newer one cancels the oldest held, whose buffer
//! goes back: newest wins). The app runs out of buffers and is held to the
//! clock. Only while the space's clock is armed (it never holds what no
//! tick would release). The space's Xwayland is not paced.

use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc, Mutex,
    },
};

use smithay::{
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Resource},
    wayland::compositor::{add_blocker, add_pre_commit_hook, is_sync_subsurface, with_states, Blocker, BlockerState, BufferAssignment, CompositorHandler, SurfaceAttributes},
};

use crate::{desk::DeskId, state::Ede};

/// At most this many commits of a surface wait for a tick.
pub const HELD_MAX: usize = 2;

const PENDING: u8 = 0;
const RELEASED: u8 = 1;
const CANCELLED: u8 = 2;

#[derive(Clone)]
struct Pace(Arc<AtomicU8>);

impl Blocker for Pace {
    fn state(&self) -> BlockerState {
        match self.0.load(Ordering::Acquire) {
            PENDING => BlockerState::Pending,
            RELEASED => BlockerState::Released,
            _ => BlockerState::Cancelled,
        }
    }
}

/// A surface's commits of the current tick window, and those that wait.
#[derive(Default)]
struct Paced {
    window: u64,
    admitted: u32,
    held: VecDeque<Pace>,
}

/// On every surface of a space's Wayland client (not its Xwayland's).
pub(crate) fn install(surface: &WlSurface) {
    add_pre_commit_hook::<Ede, _>(surface, |state, _dh, surface| state.space_pace_commit(surface));
}

impl Ede {
    fn space_pace_commit(&mut self, surface: &WlSurface) {
        let Some(desk) = crate::spaces::space_of_surface(surface) else { return };
        // A synchronized subsurface's commit is its parent's.
        if is_sync_subsurface(surface) {
            return;
        }
        let new_buffer = with_states(surface, |s| matches!(s.cached_state.get::<SurfaceAttributes>().pending().buffer, Some(BufferAssignment::NewBuffer(_))));
        let Some((window, ticking)) = self.spaces.get(desk).map(|s| (s.ticks, s.scheduled)) else { return };
        if !new_buffer {
            return;
        }
        // #EDE-162: its launch drew something ([`super::net`]).
        self.space_net_drew(desk, surface.client());
        let held = with_states(surface, |s| {
            s.data_map.insert_if_missing_threadsafe(|| Mutex::new(Paced::default()));
            let mut p = s.data_map.get::<Mutex<Paced>>().unwrap().lock().unwrap();
            if p.window != window {
                (p.window, p.admitted) = (window, 0);
            }
            p.admitted += 1;
            if p.admitted == 1 || !ticking {
                return None;
            }
            let pace = Pace(Arc::new(AtomicU8::new(PENDING)));
            p.held.push_back(pace.clone());
            let cancelled = (p.held.len() > HELD_MAX).then(|| p.held.pop_front()).flatten();
            Some((pace, cancelled))
        });
        let Some((pace, cancelled)) = held else { return };
        add_blocker(surface, pace);
        let paced = self.spaces.frames.pacing.entry(desk).or_default();
        if !paced.iter().any(|s| s == surface) {
            paced.push(surface.clone());
        }
        self.spaces.frames.stats.paced();
        if let Some(old) = cancelled {
            old.0.store(CANCELLED, Ordering::Release);
            // Not inside this commit's own hook: on the next turn.
            if let Some(client) = surface.client() {
                self.loop_handle.insert_idle(move |state| {
                    let dh = state.display_handle.clone();
                    state.client_compositor_state(&client).blocker_cleared(state, &dh);
                });
            }
        }
    }

    /// At each tick of space `desk` (its window count already moved), before
    /// its frame callbacks: each paced surface's oldest held commit goes
    /// through and counts for the new window.
    pub(super) fn space_pace_tick(&mut self, desk: DeskId) {
        let Some(window) = self.spaces.get(desk).map(|s| s.ticks) else { return };
        // Spaces that closed while a commit waited.
        let live: Vec<DeskId> = self.spaces.iter().map(|s| s.desk.id).collect();
        self.spaces.frames.pacing.retain(|d, _| live.contains(d));
        let Some(surfaces) = self.spaces.frames.pacing.get(&desk) else { return };
        let surfaces: Vec<WlSurface> = surfaces.iter().filter(|s| s.is_alive()).cloned().collect();
        let mut clients = Vec::new();
        for surface in &surfaces {
            let released = with_states(surface, |s| {
                let mut p = s.data_map.get::<Mutex<Paced>>()?.lock().unwrap();
                let next = p.held.pop_front()?;
                next.0.store(RELEASED, Ordering::Release);
                (p.window, p.admitted) = (window, 1);
                Some(())
            });
            if released.is_some() {
                if let Some(client) = surface.client() {
                    if !clients.contains(&client) {
                        clients.push(client);
                    }
                }
            }
        }
        // Only surfaces that still hold a commit stay listed.
        let holding: Vec<WlSurface> = surfaces.into_iter().filter(|s| with_states(s, |st| st.data_map.get::<Mutex<Paced>>().is_some_and(|p| !p.lock().unwrap().held.is_empty()))).collect();
        if holding.is_empty() {
            self.spaces.frames.pacing.remove(&desk);
        } else {
            self.spaces.frames.pacing.insert(desk, holding);
        }
        let dh = self.display_handle.clone();
        for client in clients {
            self.client_compositor_state(&client).blocker_cleared(self, &dh);
        }
    }

    /// How many surfaces of all spaces are listed as holding a commit
    /// (tests: closed spaces and settled surfaces are forgotten).
    pub fn spaces_paced_surfaces(&self) -> usize {
        self.spaces.frames.pacing.values().map(Vec::len).sum()
    }

    /// How many of space `desk`'s commits wait for a tick now (tests).
    pub fn space_held_commits(&self, desk: DeskId) -> usize {
        self.spaces.frames.pacing.get(&desk).map_or(0, |surfaces| {
            surfaces.iter().filter(|s| s.is_alive()).map(|s| with_states(s, |st| st.data_map.get::<Mutex<Paced>>().map_or(0, |p| p.lock().unwrap().held.len()))).sum()
        })
    }
}
