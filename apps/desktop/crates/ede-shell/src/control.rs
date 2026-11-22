//! #EDE-172 (P4): the shell's way to the compositor. Started by the
//! compositor (`[session] helpers = "compositor"`, through its supervisor),
//! the shell holds the channel it was handed at fd 3 and opens every
//! connection it needs on it: the events, the calls, the captures, the
//! UI's own, the Engine apps' grant. No token is read then. Started by its
//! unit, it reads the token file as before. A shell without a channel in a
//! session whose compositor starts the shell itself is not the shell: it
//! says so and ends ([`take`]).

use std::sync::{Mutex, OnceLock};

use ede_protocol::{Client, Error};

/// The inherited channel: only ever used to open the others.
static BASE: OnceLock<Mutex<Option<Client>>> = OnceLock::new();

/// Takes the channel this shell was started with, if any: the first thing
/// `main` does after the crash monitor's dispatch and the arguments,
/// before any thread starts (it removes `EDE_CONTROL_FD`). `Err` = no
/// channel where there must be one, or one that does not answer.
pub fn take() -> Result<(), String> {
    ede_protocol::process::keep_inherited_to_this_process();
    let base = match Client::inherited() {
        Some(Ok(client)) => Some(client),
        Some(Err(e)) => return Err(format!("the inherited channel: {e}")),
        None if ede_protocol::process::helpers_spawned() => {
            return Err("no channel: this session's compositor starts the shell itself ([session] helpers = \"compositor\"); a shell started by hand is not it".into())
        }
        None => None,
    };
    let _ = BASE.set(Mutex::new(base));
    Ok(())
}

/// This shell holds a channel ([`take`]).
pub fn held() -> bool {
    BASE.get().is_some_and(|base| base.lock().unwrap_or_else(|p| p.into_inner()).is_some())
}

/// A new connection holding the shell's grant: a channel opened on the
/// inherited one, else (a shell its unit started) the token file's.
pub fn connect() -> Result<Client, Error> {
    if let Some(base) = BASE.get() {
        if let Some(base) = base.lock().unwrap_or_else(|p| p.into_inner()).as_mut() {
            let held = base.hello.capabilities.clone();
            return base.open_channel(None, held, None).map(|(client, _)| client);
        }
    }
    Client::connect_env()
}

/// Once the first roundtrips on the Wayland and the control connection are
/// done (the compositor took this process's executable when it took the
/// client): no other process of the person reads this one any more. Only a
/// shell the compositor started (one that holds a channel).
pub fn refuse_ptrace() {
    if held() {
        if let Err(e) = ede_protocol::process::refuse_ptrace() {
            tracing::warn!("PR_SET_DUMPABLE: {e}");
        }
    }
}
