//! `ede-session`: what LightDM runs for the "Exponential" session.
//!
//! It sets the session environment, starts `ede-compositor` on the seat,
//! and once the compositor is up hands the environment to systemd and
//! starts `ede-session.target` (autostart apps, polkit agent, keyring).
//! When the compositor dies it restarts it; three quick crashes in a row
//! and it comes back in safe mode (the stock settings), so a broken config
//! never locks the person out.

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

/// How a compositor run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// Exit status 0: the person logged out.
    Clean,
    /// Crashed or failed after running for `ran`.
    Crashed { ran: Duration },
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

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const QUICK: Exit = Exit::Crashed { ran: Duration::from_secs(5) };
    const LATE: Exit = Exit::Crashed { ran: Duration::from_secs(3600) };

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

    #[test]
    fn crashes_carry_across_logins() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ede/crashes");
        assert_eq!(load_crashes(&path), 0);
        store_crashes(&path, 2);
        assert_eq!(load_crashes(&path), 2);
    }

    #[test]
    fn the_compositor_runs_on_the_seat_and_calls_back_when_ready() {
        assert_eq!(compositor_args(false, "/usr/bin/ede-session"), ["--udev", "--", "/usr/bin/ede-session", "--ready"]);
        assert_eq!(compositor_args(true, "x"), ["--udev", "--safe-mode", "--", "x", "--ready"]);
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
        for unit in ["graphical-session.target", "xdg-desktop-autostart.target", "ede-polkit-agent.service", "ede-keyring.service"] {
            assert!(target.contains(unit), "{unit}");
            if unit.starts_with("ede-") {
                assert!(root.join("systemd").join(unit).exists(), "{unit} ships");
            }
        }
        assert_eq!(SESSION_ENV.iter().find(|(k, _)| *k == "XDG_CURRENT_DESKTOP").unwrap().1, "eDE");
    }
}
