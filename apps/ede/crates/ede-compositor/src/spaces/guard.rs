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
//!   user manager, the guard installed, the guard's probe answered ok, and
//!   polkit refusing a space's process (the package's rule is read).

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
    /// polkit's answer to the session's probe (`None` = still asking).
    pub polkit: Arc<Mutex<Option<Result<(), String>>>>,
    /// The state file the doctor reads.
    pub file: Option<PathBuf>,
    /// #EXP-1514, DEV ONLY until the flip (#EXP-2011): the hello says
    /// `space-readiness` and the answer is `ready`, whatever the checks
    /// say ([`dev_readiness`], a test's [`Ede::pretend_space_readiness`]).
    /// Off, this compositor reads the op as an older one does.
    pub pretend_ready: bool,
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

/// #EXP-1514, DEV ONLY: `EDE_DEV_SPACE_READINESS=1` makes a DEBUG build
/// say `space-readiness` and answer `ready` (the headless runs of ePilot,
/// `serve --space` by hand), so a space can be made before the flip
/// (#EXP-2011) answers it for real and removes this. A release build never
/// reads it; it is no setting a person has.
pub fn dev_readiness() -> bool {
    #[cfg(debug_assertions)]
    return std::env::var_os("EDE_DEV_SPACE_READINESS").is_some_and(|on| on == "1");
    #[cfg(not(debug_assertions))]
    false
}

/// The state file of the compositor whose spaces live in `spaces_dir`
/// (`…/exponential-desktop/<wayland-N>.spaces` → `…/<wayland-N>.guard`).
pub fn state_file(spaces_dir: &Path) -> PathBuf {
    spaces_dir.with_extension("guard")
}

fn answer(probe: Option<&Result<(), String>>) -> String {
    match probe {
        None => "pending".to_string(),
        Some(Ok(())) => "ok".to_string(),
        Some(Err(reason)) => reason.replace('\n', " "),
    }
}

/// The state file's text: `guard required|absent`, `probe ok|pending|<why>`,
/// `user-manager yes|no`, `polkit ok|pending|<why>`.
pub fn state_text(required: bool, probe: Option<&Result<(), String>>, units: bool, polkit: Option<&Result<(), String>>) -> String {
    format!("guard {}\nprobe {}\nuser-manager {}\npolkit {}\n", if required { "required" } else { "absent" }, answer(probe), if units { "yes" } else { "no" }, answer(polkit))
}

/// Can spaces be offered: `Err` = the readiness block (design §2 P4).
pub fn readiness(units: bool, required: bool, probe: Option<&Result<(), String>>, polkit: Option<&Result<(), String>>) -> Result<(), &'static str> {
    if !units {
        return Err("no_user_manager");
    }
    match (required, probe, polkit) {
        (true, Some(Ok(())), Some(Ok(()))) => Ok(()),
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
        let probe = state.probe.clone();
        let _ = std::thread::Builder::new().name("ede-guard-probe".into()).spawn(move || {
            let result = slice::guard_probe(&socket);
            match &result {
                Ok(()) => tracing::info!("the space guard answers ({})", socket.display()),
                Err(reason) => tracing::warn!("the space guard does not answer: nothing starts in an agent space until it does: {reason}"),
            }
            *probe.lock().unwrap_or_else(|p| p.into_inner()) = Some(result);
        });
        self.write_space_guard_state();
    }

    /// With the user manager found: asks polkit, in the background, whether
    /// it refuses a space's process (readiness and the doctor only).
    pub(crate) fn start_polkit_probe(&mut self, units: &slice::Units) {
        let (units, polkit) = (units.clone(), self.spaces.guard.polkit.clone());
        let _ = std::thread::Builder::new().name("ede-polkit-probe".into()).spawn(move || {
            let result = slice::polkit_probe(&units);
            if let Err(reason) = &result {
                tracing::warn!("polkit does not refuse agent spaces: {reason}");
            }
            *polkit.lock().unwrap_or_else(|p| p.into_inner()) = Some(result);
        });
    }

    /// The state file, as the session has decided so far (at the start,
    /// and on the 1 s tick while a probe is still out).
    pub fn write_space_guard_state(&self) {
        let state = &self.spaces.guard;
        if let Some(file) = &state.file {
            let probe = state.probe.lock().unwrap_or_else(|p| p.into_inner()).clone();
            let polkit = state.polkit.lock().unwrap_or_else(|p| p.into_inner()).clone();
            let text = state_text(state.required.is_some(), probe.as_ref(), self.spaces.units.is_some(), polkit.as_ref());
            if std::fs::read_to_string(file).ok().as_deref() != Some(text.as_str()) {
                let _ = std::fs::write(file, text);
            }
        }
    }

    /// #EXP-1514: whether the hello says `space-readiness`. Until the flip
    /// (#EXP-2011) only the dev switch ([`GuardState::pretend_ready`]).
    pub fn says_space_readiness(&self) -> bool {
        self.spaces.dir.is_some() && self.spaces.guard.pretend_ready
    }

    /// Tests: [`dev_readiness`] without the environment.
    pub fn pretend_space_readiness(&mut self) {
        self.spaces.guard.pretend_ready = true;
    }

    /// The flip's readiness answer (F5's `space-readiness`).
    pub fn space_readiness(&self) -> Result<(), &'static str> {
        let probe = self.spaces.guard.probe.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let polkit = self.spaces.guard.polkit.lock().unwrap_or_else(|p| p.into_inner()).clone();
        readiness(self.spaces.units.is_some(), self.spaces.guard.required.is_some(), probe.as_ref(), polkit.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_only_with_a_user_manager_a_guard_that_answered_and_polkit_refusing() {
        let ok = Ok(());
        let no = Err("busy".to_string());
        assert_eq!(readiness(true, true, Some(&ok), Some(&ok)), Ok(()));
        assert_eq!(readiness(false, true, Some(&ok), Some(&ok)), Err("no_user_manager"));
        assert_eq!(readiness(true, false, None, Some(&ok)), Err("guard"));
        assert_eq!(readiness(true, true, None, Some(&ok)), Err("guard"), "still asking");
        assert_eq!(readiness(true, true, Some(&no), Some(&ok)), Err("guard"));
        assert_eq!(readiness(true, true, Some(&ok), None), Err("guard"), "polkit still asked");
        assert_eq!(readiness(true, true, Some(&ok), Some(&no)), Err("guard"), "polkit does not refuse");
    }

    #[test]
    fn the_state_file_says_what_the_session_decided() {
        assert_eq!(state_text(true, Some(&Ok(())), true, Some(&Ok(()))), "guard required\nprobe ok\nuser-manager yes\npolkit ok\n");
        assert_eq!(state_text(true, None, true, None), "guard required\nprobe pending\nuser-manager yes\npolkit pending\n");
        assert_eq!(state_text(false, None, false, None), "guard absent\nprobe pending\nuser-manager no\npolkit pending\n");
        assert_eq!(state_text(true, Some(&Err("the space guard: busy\nagain".into())), true, Some(&Ok(()))), "guard required\nprobe the space guard: busy again\nuser-manager yes\npolkit ok\n");
        assert_eq!(state_file(Path::new("/run/user/1000/exponential-desktop/wayland-1.spaces")), Path::new("/run/user/1000/exponential-desktop/wayland-1.guard"));
    }
}
