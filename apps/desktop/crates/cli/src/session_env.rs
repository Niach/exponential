//! #EXP-1918: the installed unit (`exponential daemon install`) starts at
//! boot under lingering, before anyone signs in at the screen. The desktop
//! hands its display to the user manager at sign-in (`systemctl --user
//! import-environment`, `dbus-update-activation-environment --systemd`;
//! eDE: `exponential-session --ready`), which never reaches a process that
//! already runs: computer use read "No display session" on a machine whose
//! person sat at the desktop. The daemon asks the manager for the session
//! and takes over what its own environment lacks; the agents it starts
//! inherit it.

/// What a session's processes read to find the screen and its bus.
const SESSION_ENV: &[&str] = &[
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "XAUTHORITY",
    "XDG_CURRENT_DESKTOP",
    "XDG_SESSION_DESKTOP",
    "XDG_SESSION_TYPE",
    "XDG_RUNTIME_DIR",
    "DBUS_SESSION_BUS_ADDRESS",
];

/// Whether this process is the unit and still has no screen: only then is
/// the manager asked. A daemon started by hand keeps the environment of
/// the shell that started it.
fn lacks_a_session(own: &impl Fn(&str) -> Option<String>) -> bool {
    own("INVOCATION_ID").is_some() && own("DISPLAY").is_none() && own("WAYLAND_DISPLAY").is_none()
}

/// The variables to take over from the user manager's environment
/// (`systemctl --user show-environment`: one `NAME=value` a line) into a
/// process whose own reads `own`: nothing until the manager names a
/// display, never one the process already has.
pub(crate) fn to_adopt(manager: &str, own: impl Fn(&str) -> Option<String>) -> Vec<(String, String)> {
    if !lacks_a_session(&own) {
        return Vec::new();
    }
    // A value systemd quoted (`$'…'`: a space, a control character) is
    // left alone rather than unescaped by hand.
    let shown = |name: &str| {
        manager
            .lines()
            .filter_map(|line| line.split_once('='))
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value)
            .filter(|value| !value.is_empty() && !value.starts_with("$'"))
    };
    if shown("DISPLAY").is_none() && shown("WAYLAND_DISPLAY").is_none() {
        return Vec::new();
    }
    SESSION_ENV
        .iter()
        .filter(|name| own(name).is_none())
        .filter_map(|name| Some((name.to_string(), shown(name)?.to_string())))
        .collect()
}

/// Take the session's environment over once it exists; `true` = this
/// process has a screen it did not have before.
pub(crate) fn adopt() -> bool {
    let own = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
    if !cfg!(target_os = "linux") || !lacks_a_session(&own) {
        return false;
    }
    let shown = std::process::Command::new("systemctl")
        .args(["--user", "show-environment"])
        .stderr(std::process::Stdio::null())
        .output();
    let Ok(shown) = shown.map(|output| String::from_utf8_lossy(&output.stdout).into_owned()) else {
        return false;
    };
    let adopted = to_adopt(&shown, own);
    for (name, value) in &adopted {
        std::env::set_var(name, value);
    }
    if !adopted.is_empty() {
        let names: Vec<&str> = adopted.iter().map(|(name, _)| name.as_str()).collect();
        log::info!("session environment: took over {} from the user manager", names.join(", "));
    }
    !adopted.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| vars.iter().find(|(key, _)| *key == name).map(|(_, value)| value.to_string())
    }

    const UNIT: &[(&str, &str)] = &[("INVOCATION_ID", "0f3a"), ("XDG_RUNTIME_DIR", "/run/user/1000"), ("HOME", "/home/a")];

    fn names(adopted: &[(String, String)]) -> Vec<(&str, &str)> {
        adopted.iter().map(|(name, value)| (name.as_str(), value.as_str())).collect()
    }

    /// The unit started at boot; the person signed in since (X11, GDM).
    #[test]
    fn the_unit_takes_over_the_session_that_started_after_it() {
        let manager = "HOME=/home/a\nLANG=de_DE.UTF-8\nDISPLAY=:0\nXAUTHORITY=/run/user/1000/gdm/Xauthority\nXDG_CURRENT_DESKTOP=X-Cinnamon\nXDG_SESSION_TYPE=x11\nXDG_RUNTIME_DIR=/run/user/1000\nDBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus\n";
        assert_eq!(
            names(&to_adopt(manager, env(UNIT))),
            [
                ("DISPLAY", ":0"),
                ("XAUTHORITY", "/run/user/1000/gdm/Xauthority"),
                ("XDG_CURRENT_DESKTOP", "X-Cinnamon"),
                ("XDG_SESSION_TYPE", "x11"),
                ("DBUS_SESSION_BUS_ADDRESS", "unix:path=/run/user/1000/bus"),
            ]
        );
    }

    /// eDE's `--ready` exports these (`ede_session::EXPORTED_ENV`); the
    /// computer backend reads the three it needs from them.
    #[test]
    fn the_unit_takes_over_an_exponential_desktop_session() {
        let manager = "WAYLAND_DISPLAY=wayland-1\nDISPLAY=:1\nXDG_CURRENT_DESKTOP=Exponential\nXDG_SESSION_DESKTOP=exponential\nXDG_SESSION_TYPE=wayland\n";
        let adopted = to_adopt(manager, env(UNIT));
        for wanted in [("WAYLAND_DISPLAY", "wayland-1"), ("XDG_CURRENT_DESKTOP", "Exponential"), ("XDG_SESSION_TYPE", "wayland")] {
            assert!(names(&adopted).contains(&wanted), "{wanted:?} in {adopted:?}");
        }
    }

    #[test]
    fn nothing_is_taken_before_a_session_exists_or_by_a_process_that_has_one() {
        // Boot, nobody signed in: the manager names no display yet.
        assert!(to_adopt("HOME=/home/a\nXDG_SESSION_TYPE=tty\nDBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus\n", env(UNIT)).is_empty());
        let manager = "DISPLAY=:0\nXDG_CURRENT_DESKTOP=X-Cinnamon\n";
        // Started inside the session: its own display stays.
        let inside = [("INVOCATION_ID", "0f3a"), ("DISPLAY", ":7")];
        assert!(to_adopt(manager, env(&inside)).is_empty());
        // Started by hand (a shell over SSH): not the unit.
        assert!(to_adopt(manager, env(&[("HOME", "/home/a")])).is_empty());
        // A value systemd had to quote is left alone, the rest still comes.
        let quoted = "DISPLAY=:0\nXAUTHORITY=$'/home/a b/.Xauthority'\n";
        assert_eq!(names(&to_adopt(quoted, env(UNIT))), [("DISPLAY", ":0")]);
    }
}
