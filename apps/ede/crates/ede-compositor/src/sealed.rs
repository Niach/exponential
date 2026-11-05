//! Tier B, the compositor's half (#EDE-17): `ede-sealed`, a separate
//! binary with compiled-in genUI documents, draws the lock screen, the
//! polkit prompt and consent. The compositor trusts it by identity, never
//! by what a document says:
//!
//! - **The lock**: with no `[lock] command` the compositor spawns
//!   `ede-sealed lock` itself and remembers the child's pid; while
//!   [`LockState::Sealed`] every output shows only that process's layer
//!   surfaces (black until one arrives), only they get input, bindings are
//!   off but VT switching, and the session unlocks only through the
//!   `unlock` op with the grant minted for that child (`EDE_TOKEN`). A lock
//!   client that dies keeps the session locked and is started again.
//! - **Sealed surfaces**: a layer surface from a process whose executable
//!   is `ede-sealed` (the polkit prompt, consent) is protected: black in
//!   captures, never driven by an agent, no input method on it.

use std::{process::Child, time::Instant};

use smithay::{
    desktop::{layer_map_for_output, LayerSurface},
    output::Output,
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Resource},
    utils::SERIAL_COUNTER,
};

use crate::{lock::LockState, state::Ede};

/// The grant's session name for the lock child.
pub const SEALED_SESSION: &str = "ede-sealed";
/// Give up starting the lock client after this many quick deaths.
pub const MAX_RESPAWNS: u32 = 5;

/// A layer surface's origin, decided once when it maps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerOrigin {
    pub pid: u32,
    /// The process's executable is `ede-sealed`.
    pub sealed: bool,
}

#[derive(Debug)]
pub struct Sealed {
    /// The lock client the compositor spawned; `None` = a test's own process.
    pub child: Option<Child>,
    pub pid: u32,
    pub token: ede_protocol::Token,
    pub respawns: u32,
    pub started: Instant,
}

/// Where `ede-sealed` is: next to this binary, else on `PATH`.
pub fn sealed_binary() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| Some(p.parent()?.join("ede-sealed")))
        .filter(|p| p.exists())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "ede-sealed".into())
}

pub fn origin_of(ede: &Ede, surface: &WlSurface) -> LayerOrigin {
    let pid = ede.display_handle.get_client(surface.id()).ok().and_then(|c| c.get_credentials(&ede.display_handle).ok()).map(|c| c.pid as u32).unwrap_or(0);
    let sealed = pid != 0 && std::fs::read_link(format!("/proc/{pid}/exe")).ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())).is_some_and(|n| n == "ede-sealed");
    LayerOrigin { pid, sealed }
}

pub fn layer_origin(layer: &LayerSurface) -> LayerOrigin {
    layer.user_data().get::<LayerOrigin>().copied().unwrap_or(LayerOrigin { pid: 0, sealed: false })
}

impl Ede {
    /// Starts the sealed lock: a grant for the child, the child, the locked state.
    pub fn begin_sealed_lock(&mut self) {
        let grant = self.ipc.mint_for(SEALED_SESSION, vec![ede_protocol::Capability::System, ede_protocol::Capability::Ui]);
        let argv = [sealed_binary(), "lock".to_string()];
        match self.spawn_with_env(&argv, &[("EDE_TOKEN", grant.token.as_str())]) {
            Some(child) => {
                let pid = child.id();
                tracing::info!(pid, "sealed lock client started");
                self.lock = LockState::Sealed(Sealed { child: Some(child), pid, token: grant.token, respawns: 0, started: Instant::now() });
            }
            None => {
                tracing::error!("ede-sealed could not start: the session stays unlocked");
                self.ipc.grants.retain(|g| g.token != grant.token);
                return;
            }
        }
        self.enter_lock();
        self.emit(ede_protocol::events::SESSION_LOCKED, serde_json::Value::Null);
    }

    /// A test's lock: the test's own process plays the lock client.
    pub fn begin_sealed_lock_as(&mut self, pid: u32) -> ede_protocol::Token {
        let grant = self.ipc.mint_for(SEALED_SESSION, vec![ede_protocol::Capability::System, ede_protocol::Capability::Ui]);
        self.lock = LockState::Sealed(Sealed { child: None, pid, token: grant.token.clone(), respawns: 0, started: Instant::now() });
        self.enter_lock();
        self.emit(ede_protocol::events::SESSION_LOCKED, serde_json::Value::Null);
        grant.token
    }

    /// `unlock` over ede-protocol: only the lock child's grant opens it.
    pub fn sealed_unlock(&mut self, token: &ede_protocol::Token) -> Result<(), &'static str> {
        let LockState::Sealed(s) = &mut self.lock else { return Err("the session is not sealed-locked") };
        if &s.token != token {
            return Err("only the lock client's grant unlocks");
        }
        let token = s.token.clone();
        if let Some(mut child) = s.child.take() {
            // The client exits on its own after unlocking; reap it either way.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        self.ipc.grants.retain(|g| g.token != token);
        self.lock = LockState::Unlocked;
        self.idle.activity();
        tracing::info!("session unlocked (sealed)");
        self.emit(ede_protocol::events::SESSION_UNLOCKED, serde_json::Value::Null);
        let top = self.space.elements().last().cloned();
        self.focus(top);
        let pos = self.seat.get_pointer().unwrap().current_location();
        self.pointer_motion_to(pos, 0);
        Ok(())
    }

    /// The sealed lock client's layer surface on `output`, if it drew one.
    pub fn sealed_lock_surface_on(&self, output: &Output) -> Option<WlSurface> {
        let LockState::Sealed(s) = &self.lock else { return None };
        let map = layer_map_for_output(output);
        let found = map.layers().rev().find(|l| layer_origin(l).pid == s.pid).map(|l| l.wl_surface().clone());
        found
    }

    /// Every sealed lock surface (any output).
    pub fn sealed_lock_surfaces(&self) -> Vec<(Output, WlSurface)> {
        let LockState::Sealed(s) = &self.lock else { return vec![] };
        self.space
            .outputs()
            .flat_map(|o| {
                let map = layer_map_for_output(o);
                map.layers().filter(|l| layer_origin(l).pid == s.pid).map(|l| (o.clone(), l.wl_surface().clone())).collect::<Vec<_>>()
            })
            .collect()
    }

    /// Is `surface` a sealed surface (the lock's, a prompt's)?
    pub fn is_sealed_surface(&self, surface: &WlSurface) -> bool {
        self.space.outputs().any(|o| {
            let map = layer_map_for_output(o);
            let hit = map.layers().any(|l| l.wl_surface() == surface && (layer_origin(l).sealed || matches!(&self.lock, LockState::Sealed(s) if s.pid == layer_origin(l).pid)));
            hit
        })
    }

    /// Once a second: a dead lock client is started again (the session
    /// stays locked meanwhile); after [`MAX_RESPAWNS`] quick deaths the
    /// screen stays black until the next login.
    pub fn sealed_tick(&mut self) {
        let LockState::Sealed(s) = &mut self.lock else { return };
        let Some(child) = s.child.as_mut() else { return };
        let Ok(Some(status)) = child.try_wait() else { return };
        tracing::warn!(%status, "the lock client ended while locked");
        s.child = None;
        if s.respawns >= MAX_RESPAWNS {
            tracing::error!("the lock client keeps dying; the session stays locked");
            return;
        }
        s.respawns += 1;
        let token = s.token.clone();
        let argv = [sealed_binary(), "lock".to_string()];
        if let Some(child) = self.spawn_with_env(&argv, &[("EDE_TOKEN", token.as_str())]) {
            if let LockState::Sealed(s) = &mut self.lock {
                s.pid = child.id();
                s.child = Some(child);
                s.started = Instant::now();
            }
        }
    }

    /// Focus the sealed lock surface under the pointer (else any).
    pub fn focus_sealed_lock(&mut self) {
        let pointer = self.seat.get_pointer().unwrap().current_location();
        let output = self.output_at(pointer);
        let surfaces = self.sealed_lock_surfaces();
        let target = surfaces.iter().find(|(o, _)| Some(o) == output.as_ref()).or_else(|| surfaces.first()).map(|(_, s)| s.clone());
        let keyboard = self.seat.get_keyboard().unwrap();
        if keyboard.current_focus() != target {
            keyboard.set_focus(self, target, SERIAL_COUNTER.next_serial());
        }
    }
}
