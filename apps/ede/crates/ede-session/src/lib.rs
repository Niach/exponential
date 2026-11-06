//! `exponential-session`: what the login manager (GDM, LightDM) runs for the
//! "Exponential" session.
//!
//! It sets the session environment, starts `exponential-compositor` on the seat,
//! and once the compositor is up hands the environment to systemd and
//! starts `exponential-session.target` (autostart apps, polkit agent, keyring).
//! When the compositor dies it restarts it; three quick crashes in a row
//! and it comes back in safe mode (the stock settings), so a broken config
//! never locks the person out. A compositor that stopped itself because it
//! cannot drive the display (exit code [`FATAL_EXIT`], the reason in
//! [`failure_file`]) gets one try in safe mode, then the login screen.
//! Before every fallback the reason shows on the displays
//! (`exponential-compositor recovery`, no GPU needed), never plain black (#EDE-40).
//! SIGTERM (the login manager ending the session) stops the compositor and
//! the session units, so the shell supervisor never outlives the display.

use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Variables every eDE session sets before the compositor starts.
pub const SESSION_ENV: &[(&str, &str)] = &[
    ("XDG_CURRENT_DESKTOP", "Exponential"),
    ("XDG_SESSION_DESKTOP", "exponential"),
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
/// display, and the same start would fail the same way (exponential-compositor's
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

/// What `exponential-session --device` runs (exponential-device.service) so
/// this machine stays an online Exponential device for the whole session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceStart {
    /// `exponential daemon install` made its own user unit
    /// (`exponential-daemon.service`, WantedBy=default.target): it runs
    /// already, nothing to start twice.
    DaemonUnit,
    /// The Exponential app's launcher entry (the AppImage writes
    /// `at.exponential.desktop` for the path it runs from): its command.
    App(Vec<String>),
    /// Neither is installed.
    Nothing,
}

/// The launcher entries that make a machine a device, in order.
pub const DEVICE_DESKTOP_ENTRIES: &[&str] = &["at.exponential.desktop", "at.exponential.staging.desktop"];
/// The CLI daemon's unit, under `$XDG_CONFIG_HOME/systemd/user`.
pub const DAEMON_UNIT: &str = "exponential-daemon.service";

/// Picks what to start: `daemon_unit` = the CLI daemon's unit exists;
/// `entries` = the launcher entries found, in [`DEVICE_DESKTOP_ENTRIES`]
/// order, each one's text.
pub fn device_start(daemon_unit: bool, entries: &[String]) -> DeviceStart {
    if daemon_unit {
        return DeviceStart::DaemonUnit;
    }
    entries
        .iter()
        .find_map(|entry| desktop_exec(entry))
        .filter(|argv| !argv.is_empty())
        .map(DeviceStart::App)
        .unwrap_or(DeviceStart::Nothing)
}

/// The `Exec=` command of a desktop entry's `[Desktop Entry]` group as
/// argv, field codes (`%U`, `%f`, …) dropped, per the Desktop Entry spec:
/// key-file escapes first (`\s`, `\n`, `\t`, `\\`), then the quoting
/// rules (double quotes group; inside them `\"`, `` \` ``, `\$` and `\\`
/// stand for the character).
pub fn desktop_exec(entry: &str) -> Option<Vec<String>> {
    let mut in_group = false;
    let mut exec = None;
    for line in entry.lines() {
        let line = line.trim_start();
        if line.starts_with('[') {
            in_group = line.trim() == "[Desktop Entry]";
            continue;
        }
        if in_group {
            if let Some(value) = line.strip_prefix("Exec=") {
                exec = Some(value.trim().to_string());
                break;
            }
        }
    }
    let value = exec?;
    // Key-file layer.
    let mut unescaped = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            unescaped.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => unescaped.push(' '),
            Some('n') => unescaped.push('\n'),
            Some('t') => unescaped.push('\t'),
            Some('r') => unescaped.push('\r'),
            Some('\\') => unescaped.push('\\'),
            Some(other) => {
                unescaped.push('\\');
                unescaped.push(other);
            }
            None => unescaped.push('\\'),
        }
    }
    // Quoting layer.
    let mut argv: Vec<String> = Vec::new();
    let mut arg = String::new();
    let mut have_arg = false;
    let mut quoted = false;
    let mut chars = unescaped.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                have_arg = true;
            }
            '\\' if quoted => match chars.next() {
                Some(e @ ('"' | '`' | '$' | '\\')) => arg.push(e),
                Some(other) => {
                    arg.push('\\');
                    arg.push(other);
                }
                None => arg.push('\\'),
            },
            c if c.is_whitespace() && !quoted => {
                if have_arg {
                    argv.push(std::mem::take(&mut arg));
                    have_arg = false;
                }
            }
            '%' if !quoted => match chars.next() {
                Some('%') => {
                    arg.push('%');
                    have_arg = true;
                }
                // A field code stands alone as an argument: dropped.
                Some(_) => {
                    if !arg.is_empty() {
                        have_arg = true;
                    } else {
                        have_arg = false;
                    }
                }
                None => {}
            },
            c => {
                arg.push(c);
                have_arg = true;
            }
        }
    }
    if have_arg {
        argv.push(arg);
    }
    Some(argv)
}

/// Where the launcher entries live: `$XDG_DATA_HOME/applications` (default
/// `~/.local/share/applications`, where the AppImage writes its own), then
/// each `$XDG_DATA_DIRS` entry (default `/usr/local/share:/usr/share`).
pub fn application_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    match std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        Some(home) => dirs.push(PathBuf::from(home)),
        None => {
            if let Some(home) = std::env::var_os("HOME") {
                dirs.push(Path::new(&home).join(".local/share"));
            }
        }
    }
    let data_dirs = std::env::var("XDG_DATA_DIRS").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    dirs.extend(data_dirs.split(':').filter(|d| !d.is_empty()).map(PathBuf::from));
    dirs.into_iter().map(|d| d.join("applications")).collect()
}

/// The directory under the XDG base directories (config, state, data, runtime).
pub const DIR: &str = "exponential-desktop";
/// What the 0.1.x `ede` package called it.
pub const LEGACY_DIR: &str = "ede";

/// The XDG config, state and data homes (`$XDG_*_HOME`, else under `$HOME`).
pub fn xdg_homes() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    [("XDG_CONFIG_HOME", ".config"), ("XDG_STATE_HOME", ".local/state"), ("XDG_DATA_HOME", ".local/share")]
        .iter()
        .filter_map(|(var, rel)| std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from).or_else(|| home.as_ref().map(|h| h.join(rel))))
        .collect()
}

/// The upgrade from the `ede` package: each home's `ede` directory (the
/// shell's documents with their git history, compositor.toml, the crash
/// count, the MCP audit) becomes `exponential-desktop`, once, and only where
/// the new one does not exist yet. Returns the directories that moved.
pub fn migrate_legacy_dirs(homes: &[PathBuf]) -> Vec<PathBuf> {
    homes
        .iter()
        .filter_map(|home| {
            let (old, new) = (home.join(LEGACY_DIR), home.join(DIR));
            (old.is_dir() && !new.exists() && std::fs::rename(&old, &new).is_ok()).then_some(new)
        })
        .collect()
}

/// `$XDG_STATE_HOME/exponential-desktop/crashes`: the quick-crash count carried across
/// logins, so a compositor that dies at every login also lands in safe mode.
pub fn state_file() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
    Some(base.join("exponential-desktop").join("crashes"))
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

/// `$XDG_RUNTIME_DIR/exponential-desktop/failure`: the compositor's reason for a
/// [`FATAL_EXIT`] (it writes the file, [`take_failure`] reads and removes it).
pub fn failure_file() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?).join("exponential-desktop").join("failure"))
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
/// `exponential-compositor recovery` command line. `detail` is the compositor's
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

    #[test]
    fn the_ede_directories_move_once() {
        let root = std::env::temp_dir().join(format!("exponential-session-migrate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (config, state, data) = (root.join("config"), root.join("state"), root.join("data"));
        std::fs::create_dir_all(config.join("ede/ui")).unwrap();
        std::fs::write(config.join("ede/ui/panel.json"), "{}").unwrap();
        std::fs::create_dir_all(state.join("ede")).unwrap();
        std::fs::create_dir_all(data.join("exponential-desktop")).unwrap();
        std::fs::create_dir_all(data.join("ede")).unwrap();
        let moved = migrate_legacy_dirs(&[config.clone(), state.clone(), data.clone()]);
        assert_eq!(moved, [config.join(DIR), state.join(DIR)]);
        assert!(config.join("exponential-desktop/ui/panel.json").exists() && !config.join("ede").exists());
        assert!(data.join("ede").exists(), "an existing new directory wins; the old one stays put");
        assert!(migrate_legacy_dirs(&[config.clone(), state, data]).is_empty(), "once");
        let _ = std::fs::remove_dir_all(&root);
    }

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
        assert_eq!(compositor_args(false, "/usr/bin/exponential-session"), ["--udev", "--", "/usr/bin/exponential-session", "--ready"]);
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
        let out = recovery_args(LATE, Next::Logout, "exponential-compositor ended: signal 11");
        assert_eq!(out[2], "The Exponential session stopped");
        assert!(out[6].contains("login screen"));
        assert_eq!(out[8], SHOW_BEFORE_LOGOUT.as_secs().to_string());
    }

    #[test]
    fn the_device_is_the_daemons_unit_else_the_apps_launcher_entry_else_nothing() {
        let app = "[Desktop Entry]\nType=Application\nName=Exponential\nExec=/home/u/Downloads/Exponential-production-x86_64.AppImage %U\nIcon=at.exponential\n";
        assert_eq!(device_start(true, &[app.into()]), DeviceStart::DaemonUnit, "the daemon's own unit already starts at login");
        assert_eq!(device_start(false, &[app.into()]), DeviceStart::App(vec!["/home/u/Downloads/Exponential-production-x86_64.AppImage".into()]));
        assert_eq!(device_start(false, &[]), DeviceStart::Nothing);
        assert_eq!(device_start(false, &["[Desktop Entry]\nName=x\n".into()]), DeviceStart::Nothing, "an entry without Exec starts nothing");
        // Production before staging.
        let staging = "[Desktop Entry]\nExec=/opt/staging.AppImage %U\n";
        assert_eq!(device_start(false, &[app.into(), staging.into()]), DeviceStart::App(vec!["/home/u/Downloads/Exponential-production-x86_64.AppImage".into()]));
        assert_eq!(device_start(false, &[staging.into()]), DeviceStart::App(vec!["/opt/staging.AppImage".into()]));
    }

    #[test]
    fn exec_lines_are_read_the_way_the_app_writes_them() {
        // Plain, with a field code.
        assert_eq!(desktop_exec("[Desktop Entry]\nExec=/usr/local/bin/exponential %U\n").unwrap(), vec!["/usr/local/bin/exponential"]);
        // A path with spaces, quoted (the app's exec_escape).
        assert_eq!(desktop_exec("[Desktop Entry]\nExec=\"/home/u/My Apps/Exponential.AppImage\" %U\n").unwrap(), vec!["/home/u/My Apps/Exponential.AppImage"]);
        // `%%` is a percent; `$` and `"` double-escaped inside quotes.
        assert_eq!(desktop_exec("[Desktop Entry]\nExec=/opt/100%%/app --flag\n").unwrap(), vec!["/opt/100%/app", "--flag"]);
        assert_eq!(desktop_exec("[Desktop Entry]\nExec=\"/a/b\\\\\"c\\\\$d\"\n").unwrap(), vec!["/a/b\"c$d"]);
        assert_eq!(desktop_exec("[Desktop Entry]\nExec=\"/a/b\\\\\\\\c d\"\n").unwrap(), vec!["/a/b\\c d"]);
        // Key-file escapes and other groups.
        // `\s` is a space at the key-file layer: a separator unquoted, part of the argument quoted.
        assert_eq!(desktop_exec("[Desktop Entry]\nExec=/a/b\\sc %f %F %u\n[Desktop Action x]\nExec=/other\n").unwrap(), vec!["/a/b", "c"]);
        assert_eq!(desktop_exec("[Desktop Entry]\nExec=\"/a/b\\sc\" %U\n").unwrap(), vec!["/a/b c"]);
        assert_eq!(desktop_exec("[Desktop Action x]\nExec=/other\n"), None);
        // `--icon %i` style: the code drops, the option stays (as the spec reads it).
        assert_eq!(desktop_exec("[Desktop Entry]\nExec=app %i --name %c\n").unwrap(), vec!["app", "--name"]);
    }

    #[test]
    fn launcher_entries_are_looked_up_in_the_xdg_data_dirs() {
        let dirs = application_dirs();
        assert!(dirs.iter().any(|d| d.ends_with("applications")));
        assert!(dirs.len() >= 2, "the data home and at least one data dir: {dirs:?}");
    }

    /// The files the .deb installs say what this crate expects.
    #[test]
    fn packaging_files_agree_with_the_session() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging");
        let desktop = std::fs::read_to_string(root.join("wayland-sessions/exponential.desktop")).unwrap();
        assert!(desktop.contains("Exec=exponential-session\n"));
        assert!(desktop.contains("DesktopNames=Exponential\n"), "matches XDG_CURRENT_DESKTOP");
        let safe = std::fs::read_to_string(root.join("wayland-sessions/exponential-safe.desktop")).unwrap();
        assert!(safe.contains("Exec=exponential-session --safe-mode\n"));
        let target = std::fs::read_to_string(root.join("systemd/exponential-session.target")).unwrap();
        for unit in ["graphical-session.target", "xdg-desktop-autostart.target", "exponential-services.service", "exponential-shell.service", "exponential-polkit-agent.service", "exponential-keyring.service", "exponential-device.service"] {
            assert!(target.contains(unit), "{unit}");
            if unit.starts_with("exponential-") {
                assert!(root.join("systemd").join(unit).exists(), "{unit} ships");
                // #EDE-40: stopping the session target stops them too.
                assert!(std::fs::read_to_string(root.join("systemd").join(unit)).unwrap().contains("PartOf=graphical-session.target exponential-session.target\n"), "{unit} is part of the session");
            }
        }
        assert_eq!(SESSION_ENV.iter().find(|(k, _)| *k == "XDG_CURRENT_DESKTOP").unwrap().1, "Exponential");
        // #EDE-2: the shell runs under the supervisor, and only once the
        // compositor's display reached systemd (`exponential-session --ready`).
        let shell = std::fs::read_to_string(root.join("systemd/exponential-shell.service")).unwrap();
        assert!(shell.contains("ExecStart=/usr/bin/exponential-ui supervise -- /usr/bin/exponential-shell\n"));
        assert!(shell.contains("ConditionEnvironment=WAYLAND_DISPLAY\n"));
        assert!(EXPORTED_ENV.contains(&"WAYLAND_DISPLAY") && EXPORTED_ENV.contains(&"EDE_SAFE_MODE"));
        let deb = [root.join("stage.sh"), root.join("pack-deb.sh")].iter().map(|p| std::fs::read_to_string(p).unwrap()).collect::<String>();
        for bin in ["exponential-compositor", "exponential-session", "exponential-ui", "exponential-shell", "exponential-sealed", "exponential-services", "exponential-theme"] {
            assert!(deb.contains(&format!("{bin} ")) || deb.contains(&format!("{bin}\"")) || deb.contains(&format!("-p {bin}")), "{bin} is packaged");
        }
        assert!(deb.contains("/usr/share/exponential-desktop/ui/"), "the factory documents ship");
        // #EDE-40: the icon theme is whatever is installed (exponential-theme picks); a 10-minute install is not recommended.
        assert!(!deb.contains("papirus"), "no icon theme in Recommends");
        // #EDE-17: the lock authenticates through its own PAM service, never the `login` fallback.
        assert!(deb.contains("/etc/pam.d/exponential-desktop") && root.join("pam/exponential-desktop").exists(), "the lock's PAM service ships");
        let polkit = std::fs::read_to_string(root.join("systemd/exponential-polkit-agent.service")).unwrap();
        assert!(polkit.contains("ExecStart=/usr/bin/exponential-sealed polkit\n"), "#EDE-17: the sealed agent");
        // #EDE-19: the bridge runs before the shell, so the launcher's icon theme is set.
        let theme = std::fs::read_to_string(root.join("systemd/exponential-theme.service")).unwrap();
        assert!(theme.contains("ExecStart=/usr/bin/exponential-theme sync\n") && theme.contains("Before=exponential-shell.service\n"));
        assert!(std::fs::read_to_string(root.join("systemd/exponential-session.target")).unwrap().contains("exponential-theme.service"));
        // The device unit: the app (or the daemon) stays up for the whole session, restarted when it crashes.
        let device = std::fs::read_to_string(root.join("systemd/exponential-device.service")).unwrap();
        assert!(device.contains("ExecStart=/usr/bin/exponential-session --device\n") && device.contains("Restart=on-failure\n"));
        assert!(device.contains("After=graphical-session.target exponential-shell.service\n"), "the panel comes first");
    }
}
