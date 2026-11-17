//! Tier B, the compositor's half (#EDE-17): `exponential-sealed`, a separate
//! binary with compiled-in genUI documents, draws the lock screen, the
//! polkit prompt and consent. The compositor trusts it by identity, never
//! by what a document says:
//!
//! - **The lock**: with no `[lock] command` the compositor spawns
//!   `exponential-sealed lock` itself and remembers the child's pid; while
//!   [`LockState::Sealed`] every output shows only that process's layer
//!   surfaces (black until one arrives), only they get input, bindings are
//!   off but VT switching, and the session unlocks only through the
//!   `unlock` op with the grant minted for that child, written to its
//!   stdin (`EDE_TOKEN_FD=0`; never its environment, which `/proc` shows).
//!   A lock client that dies, or shows nothing, keeps the session locked
//!   and is started again; it ends with the lock and with the compositor.
//! - **Sealed surfaces**: a layer surface from a process whose executable
//!   is `exponential-sealed` (the polkit prompt, consent) is protected: black in
//!   captures, never driven by an agent, no input method on it.

use std::{
    io::Write,
    process::{Child, Stdio},
    time::{Duration, Instant},
};

use smithay::{
    desktop::{layer_map_for_output, LayerSurface},
    output::Output,
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Resource},
    utils::{IsAlive, SERIAL_COUNTER},
};

use crate::{lock::LockState, state::Ede};

/// The grant's session name for the lock child.
pub const SEALED_SESSION: &str = "exponential-sealed";
/// After this many quick deaths the lock client is started again only
/// after a pause.
pub const MAX_RESPAWNS: u32 = 5;
/// A lock client that lived this long was healthy: its deaths count anew.
pub const HEALTHY: Duration = Duration::from_secs(30);
/// A running lock client with no surface for this long is started again.
pub const NO_SURFACE: Duration = Duration::from_secs(10);

/// A layer surface's origin, decided once when it maps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerOrigin {
    pub pid: u32,
    /// The process's executable is `exponential-sealed`.
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
    /// What starts the lock client; empty = a test's own process.
    pub command: Vec<String>,
    /// When a lock surface was last mapped (or the client started).
    pub seen: Instant,
    /// A lock client that keeps dying is not started again before this.
    pub retry_at: Instant,
    /// The outputs that showed a locked frame.
    pub shown_on: Vec<Output>,
    /// Lock surfaces whose output went away, until the next output comes.
    pub homeless: Vec<LayerSurface>,
}

impl Drop for Sealed {
    /// The lock client never outlives its lock (unlocking takes it out
    /// first: it exits on its own then).
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Where `exponential-sealed` is: next to this binary, else on `PATH`.
pub fn sealed_binary() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| Some(p.parent()?.join("exponential-sealed")))
        .filter(|p| p.exists())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "exponential-sealed".into())
}

pub fn origin_of(ede: &Ede, surface: &WlSurface) -> LayerOrigin {
    let pid = ede.display_handle.get_client(surface.id()).ok().and_then(|c| c.get_credentials(&ede.display_handle).ok()).map(|c| c.pid as u32).unwrap_or(0);
    let sealed = pid != 0 && std::fs::read_link(format!("/proc/{pid}/exe")).ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned())).is_some_and(|n| n == "exponential-sealed");
    LayerOrigin { pid, sealed }
}

pub fn layer_origin(layer: &LayerSurface) -> LayerOrigin {
    layer.user_data().get::<LayerOrigin>().copied().unwrap_or(LayerOrigin { pid: 0, sealed: false })
}

impl Ede {
    /// Starts the sealed lock: a grant for the child, the child, the locked state.
    pub fn begin_sealed_lock(&mut self) {
        self.begin_sealed_lock_with(vec![sealed_binary(), "lock".to_string()]);
    }

    /// [`Self::begin_sealed_lock`] with `argv` as the lock client.
    pub fn begin_sealed_lock_with(&mut self, argv: Vec<String>) {
        let grant = self.ipc.mint_for(SEALED_SESSION, vec![ede_protocol::Capability::System, ede_protocol::Capability::Ui]);
        match self.spawn_lock_client(&argv, &grant.token) {
            Some(child) => {
                let pid = child.id();
                tracing::info!(pid, "sealed lock client started");
                self.lock = LockState::Sealed(Sealed { child: Some(child), pid, token: grant.token, respawns: 0, started: Instant::now(), command: argv, seen: Instant::now(), retry_at: Instant::now(), shown_on: Vec::new(), homeless: Vec::new() });
            }
            None => {
                tracing::error!("exponential-sealed could not start: the session stays unlocked");
                self.ipc.grants.retain(|g| g.token != grant.token);
                return;
            }
        }
        self.enter_lock();
        self.emit(ede_protocol::events::SESSION_LOCKED, serde_json::Value::Null);
    }

    /// Starts the lock client with the unlock grant on its stdin.
    fn spawn_lock_client(&self, argv: &[String], token: &ede_protocol::Token) -> Option<Child> {
        let mut command = self.client_command(argv)?;
        let mut child = command.env("EDE_TOKEN_FD", "0").stdin(Stdio::piped()).spawn().map_err(|e| tracing::warn!("{}: {e}", argv[0])).ok()?;
        if let Some(mut stdin) = child.stdin.take() {
            // A client that already ended reads nothing: the tick sees it.
            let _ = writeln!(stdin, "{}", token.as_str());
        }
        Some(child)
    }

    /// A test's lock: the test's own process plays the lock client.
    pub fn begin_sealed_lock_as(&mut self, pid: u32) -> ede_protocol::Token {
        let grant = self.ipc.mint_for(SEALED_SESSION, vec![ede_protocol::Capability::System, ede_protocol::Capability::Ui]);
        self.lock = LockState::Sealed(Sealed { child: None, pid, token: grant.token.clone(), respawns: 0, started: Instant::now(), command: Vec::new(), seen: Instant::now(), retry_at: Instant::now(), shown_on: Vec::new(), homeless: Vec::new() });
        self.enter_lock();
        self.emit(ede_protocol::events::SESSION_LOCKED, serde_json::Value::Null);
        grant.token
    }

    /// `unlock` over ede-protocol: only the lock child's grant opens it,
    /// and only on a connection of that child (`peer` = the caller's pid):
    /// a grant read from its stdin or its memory opens nothing.
    pub fn sealed_unlock(&mut self, token: &ede_protocol::Token, peer: Option<u32>) -> Result<(), &'static str> {
        let LockState::Sealed(s) = &mut self.lock else { return Err("the session is not sealed-locked") };
        if &s.token != token {
            return Err("only the lock client's grant unlocks");
        }
        if !s.command.is_empty() && peer != Some(s.pid) {
            return Err("only the lock client's own connection unlocks");
        }
        let (token, pid) = (s.token.clone(), s.pid);
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
        self.focus_after_unlock(Some(pid));
        let pos = self.desk.seat.get_pointer().unwrap().current_location();
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

    /// The lock's output went away: its surfaces move to an output that is
    /// left, or wait for the next one.
    pub fn sealed_output_removed(&mut self, output: &Output) {
        let LockState::Sealed(s) = &mut self.lock else { return };
        s.shown_on.retain(|o| o != output);
        let mut map = layer_map_for_output(output);
        let layers: Vec<LayerSurface> = map.layers().filter(|l| layer_origin(l).pid == s.pid).cloned().collect();
        for layer in &layers {
            map.unmap_layer(layer);
        }
        drop(map);
        s.homeless.extend(layers);
        self.sealed_rehome();
    }

    /// Lock surfaces without an output go onto the first one there is.
    pub fn sealed_rehome(&mut self) {
        let Some(output) = self.space.outputs().next().cloned() else { return };
        let LockState::Sealed(s) = &mut self.lock else { return };
        let layers = std::mem::take(&mut s.homeless);
        if layers.is_empty() {
            return;
        }
        let mut map = layer_map_for_output(&output);
        for layer in layers.iter().filter(|l| l.alive()) {
            if let Err(e) = map.map_layer(layer) {
                tracing::warn!("the lock surface did not move to {}: {e}", output.name());
            }
        }
        drop(map);
        self.focus_sealed_lock();
    }

    /// Once a second: a lock client that died, or runs with no surface for
    /// [`NO_SURFACE`], is started again (the session stays locked
    /// meanwhile), and one that could not start is tried again. After
    /// [`MAX_RESPAWNS`] deaths in a row, none of them after [`HEALTHY`],
    /// the next starts wait: 5 s, doubling up to a minute.
    pub fn sealed_tick(&mut self) {
        // With no output there is nowhere to show: nothing is missing.
        let shows = self.space.outputs().next().is_none() || !self.sealed_lock_surfaces().is_empty();
        let LockState::Sealed(s) = &mut self.lock else { return };
        if s.command.is_empty() {
            return;
        }
        if shows {
            s.seen = Instant::now();
        }
        if let Some(child) = s.child.as_mut() {
            match child.try_wait() {
                Ok(Some(status)) => tracing::warn!(%status, "the lock client ended while locked"),
                _ if s.seen.elapsed() < NO_SURFACE => return,
                _ => {
                    tracing::warn!("the lock client shows nothing; starting it again");
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
            s.child = None;
            if s.started.elapsed() >= HEALTHY {
                s.respawns = 0;
            }
            s.respawns += 1;
            if s.respawns > MAX_RESPAWNS {
                let pause = Duration::from_secs(60.min(5 << (s.respawns - MAX_RESPAWNS - 1).min(4)));
                tracing::error!(?pause, "the lock client keeps dying; the session stays locked");
                s.retry_at = Instant::now() + pause;
            }
        }
        if s.respawns > MAX_RESPAWNS && Instant::now() < s.retry_at {
            return;
        }
        let (argv, token) = (s.command.clone(), s.token.clone());
        if let Some(child) = self.spawn_lock_client(&argv, &token) {
            if let LockState::Sealed(s) = &mut self.lock {
                s.pid = child.id();
                s.child = Some(child);
                s.started = Instant::now();
                s.seen = s.started;
            }
        }
    }

    /// Focus the sealed lock surface under the pointer (else any).
    pub fn focus_sealed_lock(&mut self) {
        let pointer = self.desk.seat.get_pointer().unwrap().current_location();
        let output = self.output_at(pointer);
        let surfaces = self.sealed_lock_surfaces();
        let target = surfaces.iter().find(|(o, _)| Some(o) == output.as_ref()).or_else(|| surfaces.first()).map(|(_, s)| s.clone());
        let keyboard = self.desk.seat.get_keyboard().unwrap();
        if keyboard.current_focus() != target {
            keyboard.set_focus(self, target, SERIAL_COUNTER.next_serial());
        }
    }
}
