//! Pause while busy (#EXP-2174, the gaming hook moved into the runner): with
//! `pauseWhenBusy` on, the runner claims no job while a game or a full-screen
//! window runs. A running job goes on (it was claimed when the machine was
//! free); the next one waits for the game to end.

/// Why the machine counts as busy right now, or `None`.
pub fn busy_reason() -> Option<&'static str> {
    if game_running() {
        return Some("a game is running");
    }
    if fullscreen_window() {
        return Some("a full-screen app is running");
    }
    None
}

/// One process's `/proc/<pid>/cmdline` (NUL-separated) names a game launch:
/// Steam's `reaper SteamLaunch AppId=…`, Lutris' wrapper or gamescope.
pub fn cmdline_is_game(cmdline: &[u8]) -> bool {
    cmdline.split(|b| *b == 0).any(|arg| {
        let name = arg.rsplit(|b| *b == b'/').next().unwrap_or(arg);
        arg == b"SteamLaunch" || name == b"lutris-wrapper" || name == b"gamescope"
    })
}

#[cfg(target_os = "linux")]
fn game_running() -> bool {
    let Ok(entries) = std::fs::read_dir("/proc") else { return false };
    entries.flatten().any(|entry| {
        let name = entry.file_name();
        name.to_str().is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()))
            && std::fs::read(entry.path().join("cmdline")).is_ok_and(|cmdline| cmdline_is_game(&cmdline))
    })
}

#[cfg(not(target_os = "linux"))]
fn game_running() -> bool {
    false
}

/// The active X11 window (XWayland too) is full screen (EWMH
/// `_NET_WM_STATE_FULLSCREEN`). No `DISPLAY` (a server, a pure Wayland
/// session) = not known, so not busy.
#[cfg(target_os = "linux")]
fn fullscreen_window() -> bool {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};
    if std::env::var_os("DISPLAY").is_none() {
        return false;
    }
    let probe = || -> Option<bool> {
        let (conn, screen) = x11rb::connect(None).ok()?;
        let root = conn.setup().roots.get(screen)?.root;
        let atom = |name: &str| conn.intern_atom(false, name.as_bytes()).ok()?.reply().ok().map(|r| r.atom);
        let (active, state, fullscreen) = (atom("_NET_ACTIVE_WINDOW")?, atom("_NET_WM_STATE")?, atom("_NET_WM_STATE_FULLSCREEN")?);
        let window = conn.get_property(false, root, active, AtomEnum::WINDOW, 0, 1).ok()?.reply().ok()?.value32()?.next()?;
        if window == 0 {
            return Some(false);
        }
        let states = conn.get_property(false, window, state, AtomEnum::ATOM, 0, 64).ok()?.reply().ok()?;
        let fullscreen_now = states.value32()?.any(|a| a == fullscreen);
        Some(fullscreen_now)
    };
    probe().unwrap_or(false)
}

#[cfg(not(target_os = "linux"))]
fn fullscreen_window() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::cmdline_is_game;

    #[test]
    fn steam_lutris_and_gamescope_launches_are_games() {
        assert!(cmdline_is_game(b"/home/u/.steam/ubuntu12_32/reaper\0SteamLaunch\0AppId=570\0--\0dota2\0"));
        assert!(cmdline_is_game(b"/usr/bin/lutris-wrapper\0Game\0"));
        assert!(cmdline_is_game(b"gamescope\0-f\0--\0game\0"));
    }

    #[test]
    fn the_steam_client_and_other_processes_are_not() {
        assert!(!cmdline_is_game(b"/home/u/.steam/steam/ubuntu12_32/steam\0-silent\0"));
        assert!(!cmdline_is_game(b"cargo\0test\0SteamLaunchpad\0"));
        assert!(!cmdline_is_game(b""));
    }
}
