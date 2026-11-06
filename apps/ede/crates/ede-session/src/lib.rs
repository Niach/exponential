//! `ede-session`: what the login manager (GDM, LightDM) runs for the
//! "Exponential" session.
//!
//! It sets the session environment, starts `ede-compositor` on the seat,
//! and once the compositor is up hands the environment to systemd and
//! starts `ede-session.target` (autostart apps, polkit agent, keyring).
//! When the compositor dies it restarts it; three quick crashes in a row
//! and it comes back in safe mode (the stock settings), so a broken config
//! never locks the person out. A compositor that stopped itself because it
//! cannot drive the display (exit code [`FATAL_EXIT`], the reason in
//! [`failure_file`]) gets one try in safe mode, then the login screen.
//! Before every fallback the reason shows on the displays
//! (`ede-compositor recovery`, no GPU needed), never plain black (#EDE-40).
//! SIGTERM (the login manager ending the session) stops the compositor and
//! the session units, so the shell supervisor never outlives the display.

use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Variables every eDE session sets before the compositor starts.
pub const SESSION_ENV: &[(&str, &str)] = &[
    ("XDG_CURRENT_DESKTOP", "eDE"),
    ("XDG_SESSION_DESKTOP", "ede"),
    ("XDG_SESSION_TYPE", "wayland"),
    // Toolkits that default to X11 unless told.
    ("MOZ_ENABLE_WAYLAND", "1"),
    ("QT_QPA_PLATFORM", "wayland;xcb"),
    ("ELECTRON_OZONE_PLATFORM_HINT", "auto"),
    ("_JAVA_AWT_WM_NONREPARENTING", "1"),
];

/// What systemd and D-Bus activation need to know once the compositor runs.
pub const EXPORTED_ENV: &[&str] =
    &["WAYLAND_DISPLAY", "DISPLAY", "XDG_CURRENT_DESKTOP", "XDG_SESSION_DESKTOP", "XDG_SESSION_TYPE", "EDE_SAFE_MODE"];

/// A crash this soon after start counts toward safe mode.
pub const QUICK_CRASH: Duration = Duration::from_secs(60);
/// This many quick crashes in a row → safe mode.
pub const CRASHES_FOR_SAFE_MODE: usize = 3;
/// Never restart more often than this in one login.
pub const MAX_RESTARTS: usize = 6;
/// The compositor's exit code when it stopped itself: it cannot drive the
/// display, and the same start would fail the same way (ede-compositor's
/// `FATAL_EXIT`).
pub const FATAL_EXIT: i32 = 3;
/// How long the reason shows before a restart, and before the login screen
/// (a key ends either wait).
pub const SHOW_BEFORE_RESTART: Duration = Duration::from_secs(8);
pub const SHOW_BEFORE_LOGOUT: Duration = Duration::from_secs(45);

/// How a compositor run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// Exit status 0: the person logged out.
    Clean,
    /// Crashed or failed after running for `ran`.
    Crashed { ran: Duration },
    /// Stopped itself ([`FATAL_EXIT`]): no display could be driven.
    Fatal { ran: Duration },
}

/// The next step after a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    /// End the session (back to the login screen).
    Logout,
    Restart { safe_mode: bool },
}

/// The crash streak of this login (seeded from the state file, so it spans
/// logins) and whether safe mode is on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Policy {
    pub quick_crashes_in_a_row: usize,
    pub restarts: usize,
    pub safe_mode: bool,
}

impl Policy {
    /// Decides what follows `exit` and records it.
    pub fn after(&mut self, exit: Exit) -> Next {
        match exit {
            Exit::Clean => {
                self.quick_crashes_in_a_row = 0;
                Next::Logout
            }
            Exit::Crashed { ran } => {
                if ran < QUICK_CRASH {
                    self.quick_crashes_in_a_row += 1;
                } else {
                    self.quick_crashes_in_a_row = 1;
                }
                self.restarts += 1;
                if self.restarts > MAX_RESTARTS {
                    return Next::Logout;
                }
                if self.quick_crashes_in_a_row >= CRASHES_FOR_SAFE_MODE {
                    self.safe_mode = true;
                }
                Next::Restart { safe_mode: self.safe_mode }
            }
            // The same start fails the same way: the stock settings once
            // (an `[[outputs]]` mode or GPU pick may be at fault), then out.
            Exit::Fatal { .. } => {
                self.quick_crashes_in_a_row += 1;
                self.restarts += 1;
                if self.safe_mode || self.restarts > MAX_RESTARTS {
                    return Next::Logout;
                }
                self.safe_mode = true;
                Next::Restart { safe_mode: true }
            }
        }
    }
}

/// `$XDG_STATE_HOME/ede/crashes`: the quick-crash count carried across
/// logins, so a compositor that dies at every login also lands in safe mode.
pub fn state_file() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
    Some(base.join("ede").join("crashes"))
}

pub fn load_crashes(path: &Path) -> usize {
    std::fs::read_to_string(path).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0)
}

pub fn store_crashes(path: &Path, count: usize) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, format!("{count}\n"));
}

/// `$XDG_RUNTIME_DIR/ede/failure`: the compositor's reason for a
/// [`FATAL_EXIT`] (it writes the file, [`take_failure`] reads and removes it).
pub fn failure_file() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?).join("ede").join("failure"))
}

pub fn take_failure(path: &Path) -> Option<String> {
    let reason = std::fs::read_to_string(path).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let _ = std::fs::remove_file(path);
    reason
}

/// The compositor's command line for one run.
pub fn compositor_args(safe_mode: bool, ready_hook: &str) -> Vec<String> {
    let mut args = vec!["--udev".to_string()];
    if safe_mode {
        args.push("--safe-mode".into());
    }
    args.push("--".into());
    args.push(ready_hook.into());
    args.push("--ready".into());
    args
}

/// What the displays say after `exit`, before `next` happens: the
/// `ede-compositor recovery` command line. `detail` is the compositor's
/// reason (the failure file) or how it ended.
pub fn recovery_args(exit: Exit, next: Next, detail: &str) -> Vec<String> {
    let title = match exit {
        Exit::Crashed { ran } if ran >= QUICK_CRASH => "The Exponential session stopped",
        _ => "Exponential could not start",
    };
    let (what_next, timeout) = match next {
        Next::Restart { safe_mode: false } => ("Starting again", SHOW_BEFORE_RESTART),
        Next::Restart { safe_mode: true } => ("Starting again with the stock settings (safe mode)", SHOW_BEFORE_RESTART),
        Next::Logout => ("Back to the login screen; pick another session there to keep working", SHOW_BEFORE_LOGOUT),
    };
    vec!["recovery".into(), "--title".into(), title.into(), "--reason".into(), detail.into(), "--next".into(), what_next.into(), "--timeout".into(), timeout.as_secs().to_string()]
}

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUICK: Exit = Exit::Crashed { ran: Duration::from_secs(5) };
    const LATE: Exit = Exit::Crashed { ran: Duration::from_secs(3600) };
    const FATAL: Exit = Exit::Fatal { ran: Duration::from_secs(2) };

    #[test]
    fn logging_out_ends_the_session_and_forgives_crashes() {
        let mut p = Policy { quick_crashes_in_a_row: 2, ..Default::default() };
        assert_eq!(p.after(Exit::Clean), Next::Logout);
        assert_eq!(p.quick_crashes_in_a_row, 0);
    }

    #[test]
    fn three_quick_crashes_in_a_row_restart_in_safe_mode() {
        let mut p = Policy::default();
        assert_eq!(p.after(QUICK), Next::Restart { safe_mode: false });
        assert_eq!(p.after(QUICK), Next::Restart { safe_mode: false });
        assert_eq!(p.after(QUICK), Next::Restart { safe_mode: true });
        // Safe mode sticks for the rest of the login.
        assert_eq!(p.after(LATE), Next::Restart { safe_mode: true });
    }

    #[test]
    fn a_run_that_held_resets_the_streak() {
        let mut p = Policy::default();
        p.after(QUICK);
        p.after(QUICK);
        assert_eq!(p.after(LATE), Next::Restart { safe_mode: false });
        assert_eq!(p.quick_crashes_in_a_row, 1);
        assert_eq!(p.after(QUICK), Next::Restart { safe_mode: false });
    }

    #[test]
    fn a_crash_loop_ends_at_the_login_screen() {
        let mut p = Policy::default();
        for _ in 0..MAX_RESTARTS {
            assert!(matches!(p.after(LATE), Next::Restart { .. }));
        }
        assert_eq!(p.after(LATE), Next::Logout);
    }

    /// #EDE-40: a display that cannot be driven gets the stock settings
    /// once, then the login screen; it counts as a quick crash across logins.
    #[test]
    fn a_fatal_start_tries_safe_mode_once_then_logs_out() {
        let mut p = Policy::default();
        assert_eq!(p.after(FATAL), Next::Restart { safe_mode: true });
        assert_eq!(p.after(FATAL), Next::Logout);
        assert_eq!(p.quick_crashes_in_a_row, 2);
        let mut safe = Policy { safe_mode: true, ..Default::default() };
        assert_eq!(safe.after(FATAL), Next::Logout, "already in safe mode: nothing left to try");
    }

    #[test]
    fn crashes_carry_across_logins() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ede/crashes");
        assert_eq!(load_crashes(&path), 0);
        store_crashes(&path, 2);
        assert_eq!(load_crashes(&path), 2);
    }

    #[test]
    fn the_failure_file_is_read_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("failure");
        assert_eq!(take_failure(&path), None);
        std::fs::write(&path, "Virtual-1 never drew a frame\n").unwrap();
        assert_eq!(take_failure(&path).as_deref(), Some("Virtual-1 never drew a frame"));
        assert_eq!(take_failure(&path), None, "removed");
    }

    #[test]
    fn the_compositor_runs_on_the_seat_and_calls_back_when_ready() {
        assert_eq!(compositor_args(false, "/usr/bin/ede-session"), ["--udev", "--", "/usr/bin/ede-session", "--ready"]);
        assert_eq!(compositor_args(true, "x"), ["--udev", "--safe-mode", "--", "x", "--ready"]);
    }

    #[test]
    fn the_recovery_message_says_what_failed_and_what_comes_next() {
        let args = recovery_args(FATAL, Next::Restart { safe_mode: true }, "Virtual-1 never drew a frame");
        assert_eq!(args[0], "recovery");
        assert_eq!(args[1..3], ["--title", "Exponential could not start"]);
        assert_eq!(args[3..5], ["--reason", "Virtual-1 never drew a frame"]);
        assert!(args[6].contains("safe mode"));
        assert_eq!(args[7..9], ["--timeout".to_string(), SHOW_BEFORE_RESTART.as_secs().to_string()]);
        let out = recovery_args(LATE, Next::Logout, "ede-compositor ended: signal 11");
        assert_eq!(out[2], "The Exponential session stopped");
        assert!(out[6].contains("login screen"));
        assert_eq!(out[8], SHOW_BEFORE_LOGOUT.as_secs().to_string());
    }

    /// The files the .deb installs say what this crate expects.
    #[test]
    fn packaging_files_agree_with_the_session() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging");
        let desktop = std::fs::read_to_string(root.join("wayland-sessions/ede.desktop")).unwrap();
        assert!(desktop.contains("Exec=ede-session\n"));
        assert!(desktop.contains("DesktopNames=eDE\n"), "matches XDG_CURRENT_DESKTOP");
        let safe = std::fs::read_to_string(root.join("wayland-sessions/ede-safe.desktop")).unwrap();
        assert!(safe.contains("Exec=ede-session --safe-mode\n"));
        let target = std::fs::read_to_string(root.join("systemd/ede-session.target")).unwrap();
        for unit in ["graphical-session.target", "xdg-desktop-autostart.target", "ede-services.service", "ede-shell.service", "ede-polkit-agent.service", "ede-keyring.service"] {
            assert!(target.contains(unit), "{unit}");
            if unit.starts_with("ede-") {
                assert!(root.join("systemd").join(unit).exists(), "{unit} ships");
                // #EDE-40: stopping the session target stops them too.
                assert!(std::fs::read_to_string(root.join("systemd").join(unit)).unwrap().contains("PartOf=graphical-session.target ede-session.target\n"), "{unit} is part of the session");
            }
        }
        assert_eq!(SESSION_ENV.iter().find(|(k, _)| *k == "XDG_CURRENT_DESKTOP").unwrap().1, "eDE");
        // #EDE-2: the shell runs under the supervisor, and only once the
        // compositor's display reached systemd (`ede-session --ready`).
        let shell = std::fs::read_to_string(root.join("systemd/ede-shell.service")).unwrap();
        assert!(shell.contains("ExecStart=/usr/bin/ede-ui supervise -- /usr/bin/ede-shell\n"));
        assert!(shell.contains("ConditionEnvironment=WAYLAND_DISPLAY\n"));
        assert!(EXPORTED_ENV.contains(&"WAYLAND_DISPLAY") && EXPORTED_ENV.contains(&"EDE_SAFE_MODE"));
        let deb = std::fs::read_to_string(root.join("build-deb.sh")).unwrap();
        for bin in ["ede-compositor", "ede-session", "ede-ui", "ede-shell", "ede-sealed", "ede-services", "ede-theme"] {
            assert!(deb.contains(&format!("{bin} ")) || deb.contains(&format!("{bin}\"")) || deb.contains(&format!("-p {bin}")), "{bin} is packaged");
        }
        assert!(deb.contains("/usr/share/ede/ui/"), "the factory documents ship");
        // #EDE-40: the icon theme is whatever is installed (ede-theme picks); a 10-minute install is not recommended.
        assert!(!deb.contains("papirus"), "no icon theme in Recommends");
        // #EDE-17: the lock authenticates through its own PAM service, never the `login` fallback.
        assert!(deb.contains("/etc/pam.d/ede") && root.join("pam/ede").exists(), "the lock's PAM service ships");
        let polkit = std::fs::read_to_string(root.join("systemd/ede-polkit-agent.service")).unwrap();
        assert!(polkit.contains("ExecStart=/usr/bin/ede-sealed polkit\n"), "#EDE-17: the sealed agent");
        // #EDE-19: the bridge runs before the shell, so the launcher's icon theme is set.
        let theme = std::fs::read_to_string(root.join("systemd/ede-theme.service")).unwrap();
        assert!(theme.contains("ExecStart=/usr/bin/ede-theme sync\n") && theme.contains("Before=ede-shell.service\n"));
        assert!(std::fs::read_to_string(root.join("systemd/ede-session.target")).unwrap().contains("ede-theme.service"));
    }
}
