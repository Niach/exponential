//! #EDE-172 (P4): sealed's way to the compositor. The lock client and the
//! polkit agent the compositor starts (`[session] helpers = "compositor"`)
//! hold the channel they were handed at fd 3 and ask everything on it: the
//! outputs, `unlock` (which counts on that connection alone),
//! `space_of_process`. Without one (their unit, consent, the share
//! chooser) they connect as before.

use std::sync::{Mutex, OnceLock};

use ede_protocol::{Client, Error};

static BASE: OnceLock<Mutex<Option<Client>>> = OnceLock::new();

/// Takes the channel this process was started with, if any: in `main`
/// after the crash monitor's dispatch, before any thread starts.
pub fn take() {
    ede_protocol::process::keep_inherited_to_this_process();
    let base = match Client::inherited() {
        Some(Ok(client)) => Some(client),
        Some(Err(e)) => {
            eprintln!("exponential-sealed: the inherited channel: {e}");
            None
        }
        None => None,
    };
    let _ = BASE.set(Mutex::new(base));
}

/// This process holds a channel ([`take`]).
pub fn held() -> bool {
    BASE.get().is_some_and(|base| base.lock().unwrap_or_else(|p| p.into_inner()).is_some())
}

/// `ask` on the channel, else on a connection `connect` makes.
pub fn with<R>(ask: impl FnOnce(&mut Client) -> Result<R, Error>, connect: impl FnOnce() -> Result<Client, Error>) -> Result<R, Error> {
    if let Some(base) = BASE.get() {
        if let Some(client) = base.lock().unwrap_or_else(|p| p.into_inner()).as_mut() {
            return ask(client);
        }
    }
    ask(&mut connect()?)
}

/// After gpui's first roundtrip (and the channel's hello): no other process
/// of the person reads this one. Only one the compositor started.
pub fn refuse_ptrace() {
    if held() {
        if let Err(e) = ede_protocol::process::refuse_ptrace() {
            tracing::warn!("PR_SET_DUMPABLE: {e}");
        }
    }
}
