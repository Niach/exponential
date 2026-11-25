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
//!   One that cannot start is tried again every second, and logged once,
//!   then every [`STILL_FAILING_EVERY`] with its count (#EDE-210); so is
//!   one that cannot start as the lock begins: the session locks all the
//!   same, black and deaf until it shows (#EDE-212).
//! - **Sealed surfaces**: a layer surface from a process whose executable
//!   is the installed `exponential-sealed` (the polkit prompt, consent) is
//!   protected: black in captures, never driven by an agent, no input
//!   method on it. #EDE-172: the file itself (root-owned, by `(st_dev,
//!   st_ino)`, in the compositor's user namespace: [`crate::peer`]), never
//!   its name: a copy or a rename of it draws an ordinary surface.
//! - #EDE-172 (P4, `[session] helpers = "compositor"`): the lock client
//!   gets its grant as a channel at fd 3 instead of a token on its stdin,
//!   its Wayland connection from the compositor, and the pinned
//!   environment ([`crate::helpers`]); `unlock` counts on that channel
//!   alone. A client the compositor made for a helper is the shell or
//!   sealed by construction; with the switch on, a shell is nothing else.

use std::{
    io::Write,
    process::{Child, Stdio},
    time::{Duration, Instant},
};

use smithay::{
    desktop::{layer_map_for_output, LayerSurface},
    output::Output,
    reexports::wayland_server::{protocol::wl_surface::WlSurface, Client, Resource},
    utils::{IsAlive, SERIAL_COUNTER},
};

use crate::{
    helpers::{Channel, Kind},
    lock::LockState,
    state::{client_pid, ClientState, Ede},
};

/// The grant's session name for the lock child.
pub const SEALED_SESSION: &str = "exponential-sealed";
/// After this many quick deaths the lock client is started again only
/// after a pause.
pub const MAX_RESPAWNS: u32 = 5;
/// A lock client that lived this long was healthy: its deaths count anew.
pub const HEALTHY: Duration = Duration::from_secs(30);
/// A running lock client with no surface for this long is started again.
pub const NO_SURFACE: Duration = Duration::from_secs(10);
/// #EDE-210: a lock client that still cannot start is logged again (with
/// the count of tries) this often, not on every try.
pub const STILL_FAILING_EVERY: Duration = Duration::from_secs(30);
/// #EDE-212: [`Sealed::pid`] while no lock client runs (its start failed):
/// above any pid Linux hands out (`pid_max` ≤ 2^22), so no surface is the lock's.
pub const NO_CLIENT: u32 = u32::MAX;

/// #EDE-210: the lock client's starts that failed in a row while locked,
/// and when that was last logged.
#[derive(Debug, Default)]
pub struct FailedStarts {
    pub tries: u32,
    logged: Option<Instant>,
}

impl FailedStarts {
    /// One more start failed at `now`; whether to log it: the first at
    /// once, the ones after it every [`STILL_FAILING_EVERY`].
    pub fn failed(&mut self, now: Instant) -> bool {
        self.tries += 1;
        let due = self.logged.is_none_or(|at| now.saturating_duration_since(at) >= STILL_FAILING_EVERY);
        if due {
            self.logged = Some(now);
        }
        due
    }

    /// A start worked: how many failed before it.
    pub fn started(&mut self) -> u32 {
        std::mem::take(self).tries
    }
}

/// A layer surface's origin, decided once when it maps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerOrigin {
    pub pid: u32,
    /// The process is the installed `exponential-sealed` (#EDE-172).
    pub sealed: bool,
    /// #EXP-2024: the shell's pinned view of an agent space (the namespace
    /// [`ede_protocol::SPACE_VIEW_NAMESPACE`] from `exponential-shell`): black
    /// in captures of the person's output, as a sealed surface is.
    pub space_view: bool,
    /// #EDE-225: one of the shell's overlays a fullscreen window does not
    /// hide ([`ede_protocol::ABOVE_FULLSCREEN_NAMESPACES`] from
    /// `exponential-shell`): the switcher, Expo, Scale, the OSD.
    pub shell_overlay: bool,
}

#[derive(Debug)]
pub struct Sealed {
    /// The lock client the compositor spawned; `None` = a test's own process.
    pub child: Option<Child>,
    pub pid: u32,
    pub token: ede_protocol::Token,
    /// #EDE-172 (P4): the lock client's channel, the one connection that
    /// unlocks; `None` = its grant went to its stdin (or a test's process).
    pub conn: Option<u64>,
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
    /// #EDE-210: starts that failed since the client last ran.
    pub failed_starts: FailedStarts,
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

/// A process's executable name as `/proc/<pid>/exe` links it: a binary
/// replaced while it runs (a package upgrade, a rebuild) reads
/// `<name> (deleted)` there, and is still that program.
pub fn exe_name(link: &std::path::Path) -> Option<String> {
    let name = link.file_name()?.to_string_lossy();
    Some(name.strip_suffix(" (deleted)").unwrap_or(&name).to_string())
}

/// #EDE-172: `client` connected the person's socket as the installed
/// binary `name` ([`crate::peer::Installed::is`]).
fn is_installed(ede: &Ede, client: &Client, name: &str) -> bool {
    client.get_data::<crate::state::ClientState>().and_then(|c| c.peer.as_ref()).is_some_and(|peer| ede.installed.is(peer, name))
}

/// #EDE-172 (P4): what the compositor made `client` for, if it did.
fn made_for(client: &Client) -> Option<Kind> {
    client.get_data::<ClientState>().and_then(|c| c.helper)
}

/// `client` is the shell, the installed `exponential-shell` (or the
/// headless tests' stand-in for it, by pid): its space view (#EXP-2024) and
/// its Agent spaces list (#EXP-2041) are black in captures of the person's
/// output. #EDE-172 (P4): with the compositor starting the shell, the
/// client it made for it, and no other.
pub fn is_shell(ede: &Ede, client: &Client) -> bool {
    let pid = client_pid(&ede.display_handle, client);
    made_for(client) == Some(Kind::Shell) || (pid != 0 && ede.shell_stand_in == Some(pid)) || (!ede.helpers.on && is_installed(ede, client, crate::peer::SHELL))
}

pub fn origin_of(ede: &Ede, surface: &WlSurface, namespace: &str) -> LayerOrigin {
    let Ok(client) = ede.display_handle.get_client(surface.id()) else { return LayerOrigin { pid: 0, sealed: false, space_view: false, shell_overlay: false } };
    let pid = client_pid(&ede.display_handle, &client);
    // Consent and the share chooser (the shell's and the portal's) still
    // connect by the socket. #EDE-172 (P5'): the consent prompt the
    // compositor spawned is known by its pid, as the lock is.
    let sealed = made_for(&client) == Some(Kind::Sealed) || (pid != 0 && (ede.sealed_stand_in == Some(pid) || ede.consent_prompt_pid() == Some(pid))) || is_installed(ede, &client, crate::peer::SEALED);
    // Only the shell's own view hides: another client naming the namespace stays in captures.
    let shell = is_shell(ede, &client);
    let space_view = namespace == ede_protocol::SPACE_VIEW_NAMESPACE && shell;
    // #EDE-225: and only the shell's overlays show above a fullscreen window.
    let shell_overlay = ede_protocol::ABOVE_FULLSCREEN_NAMESPACES.contains(&namespace) && shell;
    LayerOrigin { pid, sealed, space_view, shell_overlay }
}

pub fn layer_origin(layer: &LayerSurface) -> LayerOrigin {
    layer.user_data().get::<LayerOrigin>().copied().unwrap_or(LayerOrigin { pid: 0, sealed: false, space_view: false, shell_overlay: false })
}

impl Ede {
    /// Starts the sealed lock: a grant for the child, the child, the locked state.
    pub fn begin_sealed_lock(&mut self) {
        self.begin_sealed_lock_with(vec![sealed_binary(), "lock".to_string()]);
    }

    /// [`Self::begin_sealed_lock`] with `argv` as the lock client.
    pub fn begin_sealed_lock_with(&mut self, argv: Vec<String>) {
        // The grant a failed start leaves for the next one: with the
        // channel each start mints its own; on stdin the client's grant
        // lives as long as the lock, as for a client that dies.
        let (started, kept) = if self.helpers.on {
            (self.spawn_lock_on_channel(&argv), crate::ipc::new_token())
        } else {
            let grant = self.ipc.mint_for(SEALED_SESSION, vec![ede_protocol::Capability::System, ede_protocol::Capability::Ui]);
            (self.spawn_lock_client(&argv, &grant.token).map(|child| (child, None, grant.token.clone())), grant.token)
        };
        let now = Instant::now();
        let sealed = match started {
            Ok((child, conn, token)) => {
                let pid = child.id();
                tracing::info!(pid, channel = conn.is_some(), "sealed lock client started");
                Sealed { child: Some(child), pid, token, conn, respawns: 0, started: now, command: argv, seen: now, retry_at: now, shown_on: Vec::new(), homeless: Vec::new(), failed_starts: FailedStarts::default() }
            }
            // #EDE-212: a lock that began stays a lock. With no client the
            // outputs show black, nothing gets input and nothing unlocks;
            // [`Self::sealed_tick`] starts the client as for one lost while
            // locked (#EDE-210).
            Err(e) => {
                tracing::error!("exponential-sealed could not start ({e}): the session is locked (black) and it is tried again every second");
                let mut failed_starts = FailedStarts::default();
                failed_starts.failed(now);
                Sealed { child: None, pid: NO_CLIENT, token: kept, conn: None, respawns: 0, started: now, command: argv, seen: now, retry_at: now, shown_on: Vec::new(), homeless: Vec::new(), failed_starts }
            }
        };
        self.lock = LockState::Sealed(sealed);
        self.enter_lock();
        self.emit(ede_protocol::events::SESSION_LOCKED, serde_json::Value::Null);
    }

    /// #EDE-172 (P4): starts the lock client with a grant of its own on a
    /// channel at fd 3 (bound: it ends with the client's connection) and a
    /// Wayland connection the compositor made, in the pinned environment;
    /// the client, its channel's connection and the grant; else why not.
    fn spawn_lock_on_channel(&mut self, argv: &[String]) -> Result<(Child, Option<u64>, ede_protocol::Token), String> {
        let channel = Channel::Child { session: SEALED_SESSION.into(), capabilities: vec![ede_protocol::Capability::System, ede_protocol::Capability::Ui] };
        let (conn, fd, grant) = self.ipc_helper_channel(&channel).ok_or("no channel for the lock client")?;
        let Some(grant) = grant else {
            self.ipc_end_channel(conn, None);
            return Err("no grant for the lock client".into());
        };
        let (client, wayland_fd) = match self.helper_wayland_client(Kind::Sealed) {
            Some((client, fd)) => (Some(client), Some(fd)),
            None => (None, None),
        };
        let started = match self.helper_command(SEALED_SESSION, argv, &[], None) {
            Some(mut command) => ede_protocol::process::hand_over(&mut command, Some(fd), wayland_fd).and_then(|()| command.spawn()),
            None => Err(std::io::Error::other("no lock client to start")),
        };
        match started {
            Ok(child) => {
                if let Some(state) = client.as_ref().and_then(|client| client.get_data::<ClientState>()) {
                    state.helper_pid.store(child.id(), std::sync::atomic::Ordering::Relaxed);
                }
                Ok((child, Some(conn), grant))
            }
            Err(e) => {
                if let Some(client) = client {
                    self.display_handle.backend_handle().kill_client(client.id(), smithay::reexports::wayland_server::backend::DisconnectReason::ConnectionClosed);
                }
                self.ipc_end_channel(conn, Some(&grant));
                Err(format!("{}: {e}", argv.first().map(String::as_str).unwrap_or("the lock client")))
            }
        }
    }

    /// Starts the lock client with the unlock grant on its stdin; else why not.
    fn spawn_lock_client(&self, argv: &[String], token: &ede_protocol::Token) -> Result<Child, String> {
        let mut command = self.client_command(argv).ok_or("no lock client to start")?;
        let mut child = command.env("EDE_TOKEN_FD", "0").stdin(Stdio::piped()).spawn().map_err(|e| format!("{}: {e}", argv[0]))?;
        if let Some(mut stdin) = child.stdin.take() {
            // A client that already ended reads nothing: the tick sees it.
            let _ = writeln!(stdin, "{}", token.as_str());
        }
        Ok(child)
    }

    /// A test's lock: the test's own process plays the lock client.
    pub fn begin_sealed_lock_as(&mut self, pid: u32) -> ede_protocol::Token {
        let grant = self.ipc.mint_for(SEALED_SESSION, vec![ede_protocol::Capability::System, ede_protocol::Capability::Ui]);
        self.lock = LockState::Sealed(Sealed { child: None, pid, token: grant.token.clone(), conn: None, respawns: 0, started: Instant::now(), command: Vec::new(), seen: Instant::now(), retry_at: Instant::now(), shown_on: Vec::new(), homeless: Vec::new(), failed_starts: FailedStarts::default() });
        self.enter_lock();
        self.emit(ede_protocol::events::SESSION_LOCKED, serde_json::Value::Null);
        grant.token
    }

    /// `unlock` over ede-protocol: only the lock child's grant opens it,
    /// and only on a connection of that child (`peer` = the caller's pid):
    /// a grant read from its stdin or its memory opens nothing. #EDE-172
    /// (P4): a lock client on its channel unlocks on that very connection
    /// (`conn`), never on another one, its own process's included.
    pub fn sealed_unlock(&mut self, token: &ede_protocol::Token, peer: Option<u32>, conn: Option<u64>) -> Result<(), &'static str> {
        let LockState::Sealed(s) = &mut self.lock else { return Err("the session is not sealed-locked") };
        if &s.token != token {
            return Err("only the lock client's grant unlocks");
        }
        match s.conn {
            Some(channel) if conn != Some(channel) => return Err("only the lock client's own channel unlocks"),
            None if !s.command.is_empty() && peer != Some(s.pid) => return Err("only the lock client's own connection unlocks"),
            _ => {}
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
        self.desk.space
            .outputs()
            .flat_map(|o| {
                let map = layer_map_for_output(o);
                map.layers().filter(|l| layer_origin(l).pid == s.pid).map(|l| (o.clone(), l.wl_surface().clone())).collect::<Vec<_>>()
            })
            .collect()
    }

    /// Is `surface` a sealed surface (the lock's, a prompt's)?
    pub fn is_sealed_surface(&self, surface: &WlSurface) -> bool {
        self.desk.space.outputs().any(|o| {
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
        let Some(output) = self.desk.space.outputs().next().cloned() else { return };
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
    /// the next starts wait: 5 s, doubling up to a minute. #EDE-210: a
    /// start that fails is logged the first time, then every
    /// [`STILL_FAILING_EVERY`] with the count of tries.
    pub fn sealed_tick(&mut self) {
        // With no output there is nowhere to show: nothing is missing.
        let shows = self.desk.space.outputs().next().is_none() || !self.sealed_lock_surfaces().is_empty();
        let LockState::Sealed(s) = &mut self.lock else { return };
        if s.command.is_empty() {
            return;
        }
        if shows {
            s.seen = Instant::now();
        }
        if let Some(child) = s.child.as_mut() {
            match child.try_wait() {
                // #ERR-29: the session stays locked with nothing to unlock
                // it until the next start: an error (a GlitchTip event).
                Ok(Some(status)) => tracing::error!(%status, "the lock client ended while locked"),
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
        let (argv, token, old) = (s.command.clone(), s.token.clone(), s.conn.take());
        // #EDE-172 (P4): the dead client's channel and grant go; the next
        // one gets its own.
        if let Some(old) = old {
            self.ipc_end_channel(old, Some(&token));
        }
        let started = if self.helpers.on { self.spawn_lock_on_channel(&argv) } else { self.spawn_lock_client(&argv, &token).map(|child| (child, None, token)) };
        let LockState::Sealed(s) = &mut self.lock else { return };
        match started {
            Ok((child, conn, token)) => {
                let failed = s.failed_starts.started();
                if failed > 0 {
                    tracing::info!(pid = child.id(), "the lock client started after {failed} failed tries");
                }
                s.pid = child.id();
                s.child = Some(child);
                s.conn = conn;
                s.token = token;
                s.started = Instant::now();
                s.seen = s.started;
            }
            // The session stays locked (black, nothing unlocks it) and the
            // next tick tries again; the journal hears it once, then every
            // STILL_FAILING_EVERY.
            Err(e) if s.failed_starts.failed(Instant::now()) => match s.failed_starts.tries {
                1 => tracing::warn!("the lock client cannot start: {e}; the session stays locked and it is tried again every second"),
                tries => tracing::warn!("the lock client still cannot start ({tries} tries): {e}"),
            },
            Err(_) => {}
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

#[cfg(test)]
mod tests {
    use super::*;

    /// #EDE-210: a start failing once a second is logged at the first try,
    /// then every 30 s with its count; one that works starts the count anew.
    #[test]
    fn a_start_that_keeps_failing_is_logged_once_then_every_thirty_seconds() {
        let mut starts = FailedStarts::default();
        let t0 = Instant::now();
        let logged: Vec<u32> = (0..95).filter_map(|s| starts.failed(t0 + Duration::from_secs(s)).then_some(starts.tries)).collect();
        assert_eq!(logged, [1, 31, 61, 91]);
        assert_eq!(starts.started(), 95);
        assert_eq!(starts.started(), 0, "a start resets the count");
        assert!(starts.failed(t0 + Duration::from_secs(96)), "the next failure after a start is logged at once");
        assert_eq!(starts.tries, 1);
    }
}
