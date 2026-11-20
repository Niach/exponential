//! #EDE-146..148: the compositor's side of the space guard.
//!
//! - **Fail closed.** When the guard is installed on this machine (its
//!   socket unit is there), every launch into a space asks it from inside
//!   its scope (`space-enter --guard`) and runs only on its `ok`: a guard
//!   that is down, busy or broken means the launch does not run, never a
//!   space without the guard. Nothing about that depends on what the guard
//!   answered at the session's start.
//! - **What the session decided** goes to a state file beside the
//!   compositor's socket (`<wayland-N>.guard`), which the doctor reads.
//! - **Readiness** (the flip's `space-readiness`, F5): ready only with a
//!   user manager, the guard installed, and the guard's probe answered ok.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use super::slice;

/// Where the package installs the guard's socket unit: there = installed.
const GUARD_UNITS: [&str; 2] = ["/usr/lib/systemd/system/exponential-space-guard.socket", "/etc/systemd/system/exponential-space-guard.socket"];

/// What this session decided about the guard.
#[derive(Default)]
pub struct GuardState {
    /// The guard's socket when it is installed: every launch asks it.
    pub required: Option<PathBuf>,
    /// The guard's answer to the session's probe (`None` = still asking).
    pub probe: Arc<Mutex<Option<Result<(), String>>>>,
    /// The state file the doctor reads.
    pub file: Option<PathBuf>,
}

/// The guard's socket when the guard is installed (a debug build may name
/// another in `EDE_SPACE_GUARD_SOCKET`; a release build never does).
pub fn installed() -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    if let Some(socket) = std::env::var_os("EDE_SPACE_GUARD_SOCKET") {
        return Some(socket.into());
    }
    let socket = PathBuf::from(ede_space_guard::SOCKET);
    (socket.exists() || GUARD_UNITS.iter().any(|unit| Path::new(unit).exists())).then_some(socket)
}

/// The state file of the compositor whose spaces live in `spaces_dir`
/// (`…/exponential-desktop/<wayland-N>.spaces` → `…/<wayland-N>.guard`).
pub fn state_file(spaces_dir: &Path) -> PathBuf {
    spaces_dir.with_extension("guard")
}

/// The state file's text: `guard required|absent`, `probe ok|pending|<why>`,
/// `user-manager yes|no`.
pub fn state_text(required: bool, probe: Option<&Result<(), String>>, units: bool) -> String {
    let probe = match probe {
        None => "pending".to_string(),
        Some(Ok(())) => "ok".to_string(),
        Some(Err(reason)) => reason.replace('\n', " "),
    };
    format!("guard {}\nprobe {probe}\nuser-manager {}\n", if required { "required" } else { "absent" }, if units { "yes" } else { "no" })
}

/// Can spaces be offered: `Err` = the readiness block (design §2 P4).
pub fn readiness(units: bool, required: bool, probe: Option<&Result<(), String>>) -> Result<(), &'static str> {
    if !units {
        return Err("no_user_manager");
    }
    match (required, probe) {
        (true, Some(Ok(()))) => Ok(()),
        _ => Err("guard"),
    }
}

impl crate::state::Ede {
    /// At the session's start, before the user manager is asked: whether
    /// the guard is installed (then required), and its probe in the
    /// background (it never decides a launch; it feeds readiness and the
    /// doctor).
    pub fn start_space_guard(&mut self) {
        let state = &mut self.spaces.guard;
        state.required = installed();
        state.file = self.spaces.dir.as_deref().map(state_file);
        let Some(socket) = state.required.clone() else {
            tracing::warn!("the space guard is not installed: programs in agent spaces can use the microphone, the camera, the radio switches and input devices");
            return;
        };
        let (probe, file) = (state.probe.clone(), state.file.clone());
        let units = Arc::new(Mutex::new(false));
        self.spaces.guard_units = units.clone();
        let _ = std::thread::Builder::new().name("ede-guard-probe".into()).spawn(move || {
            let result = slice::guard_probe(&socket);
            match &result {
                Ok(()) => tracing::info!("the space guard answers ({})", socket.display()),
                Err(reason) => tracing::warn!("the space guard does not answer: nothing starts in an agent space until it does: {reason}"),
            }
            if let Some(file) = &file {
                let _ = std::fs::write(file, state_text(true, Some(&result), *units.lock().unwrap_or_else(|p| p.into_inner())));
            }
            *probe.lock().unwrap_or_else(|p| p.into_inner()) = Some(result);
        });
        self.write_space_guard_state();
    }

    /// The state file, as the session has decided so far.
    pub fn write_space_guard_state(&self) {
        let state = &self.spaces.guard;
        let units = self.spaces.units.is_some();
        *self.spaces.guard_units.lock().unwrap_or_else(|p| p.into_inner()) = units;
        if let Some(file) = &state.file {
            let probe = state.probe.lock().unwrap_or_else(|p| p.into_inner()).clone();
            let _ = std::fs::write(file, state_text(state.required.is_some(), probe.as_ref(), units));
        }
    }

    /// The flip's readiness answer (F5's `space-readiness`).
    pub fn space_readiness(&self) -> Result<(), &'static str> {
        let probe = self.spaces.guard.probe.lock().unwrap_or_else(|p| p.into_inner()).clone();
        readiness(self.spaces.units.is_some(), self.spaces.guard.required.is_some(), probe.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_only_with_a_user_manager_and_a_guard_that_answered() {
        let ok = Ok(());
        let no = Err("busy".to_string());
        assert_eq!(readiness(true, true, Some(&ok)), Ok(()));
        assert_eq!(readiness(false, true, Some(&ok)), Err("no_user_manager"));
        assert_eq!(readiness(true, false, None), Err("guard"));
        assert_eq!(readiness(true, true, None), Err("guard"), "still asking");
        assert_eq!(readiness(true, true, Some(&no)), Err("guard"));
    }

    #[test]
    fn the_state_file_says_what_the_session_decided() {
        assert_eq!(state_text(true, Some(&Ok(())), true), "guard required\nprobe ok\nuser-manager yes\n");
        assert_eq!(state_text(true, None, true), "guard required\nprobe pending\nuser-manager yes\n");
        assert_eq!(state_text(false, None, false), "guard absent\nprobe pending\nuser-manager no\n");
        assert_eq!(state_text(true, Some(&Err("the space guard: busy\nagain".into())), true), "guard required\nprobe the space guard: busy again\nuser-manager yes\n");
        assert_eq!(state_file(Path::new("/run/user/1000/exponential-desktop/wayland-1.spaces")), Path::new("/run/user/1000/exponential-desktop/wayland-1.guard"));
    }
}
