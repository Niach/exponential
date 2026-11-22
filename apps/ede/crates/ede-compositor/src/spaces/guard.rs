//! #EDE-146..148: the compositor's side of the space guard.
//!
//! - **Fail closed.** When the guard is installed on this machine (its
//!   socket unit is there), every launch into a space asks it from inside
//!   its scope (`space-enter --guard`) and runs only on its `ok`: a guard
//!   that is down, busy or broken means the launch does not run, never a
//!   space without the guard. Nothing about that depends on what the guard
//!   answered at the session's start.
//! - **What the session decided** goes to a state file beside the
//!   compositor's socket (`<wayland-N>.guard`), which the doctor reads:
//!   also whether the guard refuses the person's Wayland socket (#EDE-145:
//!   a guard from before leaves it open, and the doctor says so).
//! - **Readiness** (the flip's `space-readiness`, [`super::readiness`]):
//!   ready only with the guard installed, the guard's probe answered ok,
//!   and polkit refusing a space's process (the package's rule is read).

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use ede_space_guard::client::Probed;

use super::slice;

/// Where the package installs the guard's socket unit: there = installed.
const GUARD_UNITS: [&str; 2] = ["/usr/lib/systemd/system/exponential-space-guard.socket", "/etc/systemd/system/exponential-space-guard.socket"];

/// What this session decided about the guard.
#[derive(Default)]
pub struct GuardState {
    /// The guard's socket when it is installed: every launch asks it.
    pub required: Option<PathBuf>,
    /// The guard's answer to the session's probe (`None` = still asking).
    pub probe: Arc<Mutex<Option<Result<Probed, String>>>>,
    /// polkit's answer to the session's probe (`None` = still asking).
    pub polkit: Arc<Mutex<Option<Result<(), String>>>>,
    /// The state file the doctor reads.
    pub file: Option<PathBuf>,
}

/// The guard's socket when the guard is installed (a debug build may name
/// another in `EDE_SPACE_GUARD_SOCKET`, or none with it empty: the real
/// tests on a machine with eDE installed, #EDE-176; a release build never
/// does).
pub fn installed() -> Option<PathBuf> {
    #[cfg(debug_assertions)]
    if let Some(socket) = std::env::var_os("EDE_SPACE_GUARD_SOCKET") {
        return (!socket.is_empty()).then(|| socket.into());
    }
    let socket = PathBuf::from(ede_space_guard::SOCKET);
    (socket.exists() || GUARD_UNITS.iter().any(|unit| Path::new(unit).exists())).then_some(socket)
}

/// The state file of the compositor whose spaces live in `spaces_dir`
/// (`…/exponential-desktop/<wayland-N>.spaces` → `…/<wayland-N>.guard`).
pub fn state_file(spaces_dir: &Path) -> PathBuf {
    spaces_dir.with_extension("guard")
}

fn answer<T>(probe: Option<&Result<T, String>>) -> String {
    match probe {
        None => "pending".to_string(),
        Some(Ok(_)) => "ok".to_string(),
        Some(Err(reason)) => reason.replace('\n', " "),
    }
}

/// The state file's text: `guard required|absent`, `probe ok|pending|<why>`,
/// `user-manager yes|no`, `polkit ok|pending|<why>`, `wayland closed|open`
/// (`closed` = the guard answered that it refuses the person's Wayland
/// socket, #EDE-145).
pub fn state_text(required: bool, probe: Option<&Result<Probed, String>>, units: bool, polkit: Option<&Result<(), String>>) -> String {
    let wayland = if matches!(probe, Some(Ok(Probed::Wayland))) { "closed" } else { "open" };
    format!("guard {}\nprobe {}\nuser-manager {}\npolkit {}\nwayland {wayland}\n", if required { "required" } else { "absent" }, answer(probe), if units { "yes" } else { "no" }, answer(polkit))
}

/// Does the guard guard agent spaces here: installed, its probe answered
/// ok, polkit refusing a space's process. `Err` says why not ([`super::readiness`]'s
/// `guard`). A guard from before #EDE-145 counts: the doctor names what it
/// leaves open.
pub fn verdict(required: bool, probe: Option<&Result<Probed, String>>, polkit: Option<&Result<(), String>>) -> Result<(), String> {
    if !required {
        return Err("the space guard is not installed (exponential-space-guard.socket)".into());
    }
    match probe {
        None => return Err("the space guard has not answered the session's probe yet".into()),
        Some(Err(reason)) => return Err(reason.clone()),
        Some(Ok(_)) => {}
    }
    match polkit {
        None => Err("polkit has not answered the session's probe yet".into()),
        Some(Err(reason)) => Err(format!("polkit does not refuse agent spaces: {reason}")),
        Some(Ok(())) => Ok(()),
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
                Ok(Probed::Wayland) => tracing::info!("the space guard answers ({})", socket.display()),
                Ok(Probed::Older) => tracing::warn!("the space guard answers ({}), but it is an older one: a program in an agent space that names your Wayland socket opens its windows on your desk", socket.display()),
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

    /// What the guard and polkit decided so far ([`verdict`]).
    pub fn space_guard_verdict(&self) -> Result<(), String> {
        let probe = self.spaces.guard.probe.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let polkit = self.spaces.guard.polkit.lock().unwrap_or_else(|p| p.into_inner()).clone();
        verdict(self.spaces.guard.required.is_some(), probe.as_ref(), polkit.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_guard_guards_only_installed_answered_and_with_polkit_refusing() {
        let (ok, older, yes) = (Ok(Probed::Wayland), Ok(Probed::Older), Ok(()));
        let no = Err::<Probed, String>("the space guard: busy".into());
        let polkit_no = Err::<(), String>("pkcheck=0".into());
        assert_eq!(verdict(true, Some(&ok), Some(&yes)), Ok(()));
        assert_eq!(verdict(true, Some(&older), Some(&yes)), Ok(()), "a guard from before #EDE-145");
        assert_eq!(verdict(false, None, Some(&yes)), Err("the space guard is not installed (exponential-space-guard.socket)".into()));
        assert_eq!(verdict(true, None, Some(&yes)), Err("the space guard has not answered the session's probe yet".into()), "still asking");
        assert_eq!(verdict(true, Some(&no), Some(&yes)), Err("the space guard: busy".into()));
        assert_eq!(verdict(true, Some(&ok), None), Err("polkit has not answered the session's probe yet".into()), "polkit still asked");
        assert_eq!(verdict(true, Some(&ok), Some(&polkit_no)), Err("polkit does not refuse agent spaces: pkcheck=0".into()));
    }

    #[test]
    fn the_state_file_says_what_the_session_decided() {
        assert_eq!(state_text(true, Some(&Ok(Probed::Wayland)), true, Some(&Ok(()))), "guard required\nprobe ok\nuser-manager yes\npolkit ok\nwayland closed\n");
        assert_eq!(state_text(true, Some(&Ok(Probed::Older)), true, Some(&Ok(()))), "guard required\nprobe ok\nuser-manager yes\npolkit ok\nwayland open\n");
        assert_eq!(state_text(true, None, true, None), "guard required\nprobe pending\nuser-manager yes\npolkit pending\nwayland open\n");
        assert_eq!(state_text(false, None, false, None), "guard absent\nprobe pending\nuser-manager no\npolkit pending\nwayland open\n");
        assert_eq!(state_text(true, Some(&Err("the space guard: busy\nagain".into())), true, Some(&Ok(()))), "guard required\nprobe the space guard: busy again\nuser-manager yes\npolkit ok\nwayland open\n");
        assert_eq!(state_file(Path::new("/run/user/1000/exponential-desktop/wayland-1.spaces")), Path::new("/run/user/1000/exponential-desktop/wayland-1.guard"));
    }
}
