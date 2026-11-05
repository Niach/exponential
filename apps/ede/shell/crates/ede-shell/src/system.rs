//! What the shell does to the system for the menu and the media keys
//! (#EDE-12): sound through WirePlumber (`wpctl`), the backlight through
//! sysfs or `brightnessctl`, power through logind (`systemctl`), the
//! terminal and the screenshot. Everything shells out to tools the
//! distribution ships; a missing tool is a warning, never a crash.

use std::{path::PathBuf, process::Command};

/// A level the OSD shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Level {
    /// The icon concept.
    pub icon: &'static str,
    /// 0..=100.
    pub percent: u32,
    pub muted: bool,
}

impl Level {
    pub fn label(&self) -> String {
        if self.muted {
            "muted".into()
        } else {
            format!("{}%", self.percent)
        }
    }
}

fn run(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    if !out.status.success() {
        tracing::warn!(program, ?args, status = %out.status, "failed");
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `wpctl get-volume` prints `Volume: 0.45 [MUTED]`.
pub fn parse_wpctl_volume(text: &str) -> Option<(u32, bool)> {
    let rest = text.trim().strip_prefix("Volume:")?.trim();
    let mut parts = rest.split_whitespace();
    let volume: f64 = parts.next()?.parse().ok()?;
    Some(((volume * 100.0).round().clamp(0.0, 150.0) as u32, rest.contains("[MUTED]")))
}

const SINK: &str = "@DEFAULT_AUDIO_SINK@";
const SOURCE: &str = "@DEFAULT_AUDIO_SOURCE@";

pub fn volume_step(up: bool) -> Option<Level> {
    run("wpctl", &["set-volume", "-l", "1.0", SINK, if up { "5%+" } else { "5%-" }])?;
    volume_level()
}

pub fn volume_toggle_mute() -> Option<Level> {
    run("wpctl", &["set-mute", SINK, "toggle"])?;
    volume_level()
}

pub fn volume_level() -> Option<Level> {
    let (percent, muted) = parse_wpctl_volume(&run("wpctl", &["get-volume", SINK])?)?;
    Some(Level { icon: if muted { "media-volume-mute" } else if percent == 0 { "media-volume-off" } else if percent < 50 { "media-volume-down" } else { "media-volume-up" }, percent, muted })
}

pub fn mic_toggle_mute() -> Option<Level> {
    run("wpctl", &["set-mute", SOURCE, "toggle"])?;
    let (percent, muted) = parse_wpctl_volume(&run("wpctl", &["get-volume", SOURCE])?)?;
    Some(Level { icon: if muted { "call-unmute" } else { "call-mute" }, percent, muted })
}

/// The first backlight in sysfs.
fn backlight() -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir("/sys/class/backlight").ok()?.filter_map(|e| e.ok().map(|e| e.path())).collect();
    dirs.sort();
    dirs.into_iter().next()
}

fn read_u32(path: &std::path::Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Up or down by 5 % of the maximum; `brightnessctl` first (it has the
/// permissions), sysfs when the file is writable.
pub fn brightness_step(up: bool) -> Option<Level> {
    let delta = if up { "5%+" } else { "5%-" };
    if Command::new("brightnessctl").args(["-q", "set", delta]).status().is_ok_and(|s| s.success()) {
        return brightness_level();
    }
    let dir = backlight()?;
    let max = read_u32(&dir.join("max_brightness"))?.max(1);
    let now = read_u32(&dir.join("brightness"))?;
    let step = (max / 20).max(1);
    let next = if up { (now + step).min(max) } else { now.saturating_sub(step) };
    if std::fs::write(dir.join("brightness"), next.to_string()).is_err() {
        tracing::warn!("no brightnessctl and {} is not writable", dir.display());
        return None;
    }
    brightness_level()
}

pub fn brightness_level() -> Option<Level> {
    let dir = backlight()?;
    let max = read_u32(&dir.join("max_brightness"))?.max(1);
    let now = read_u32(&dir.join("brightness"))?;
    Some(Level { icon: "ede-brightness", percent: (now * 100 / max).min(100), muted: false })
}

/// `suspend`, `reboot` or `poweroff` through logind; the active session is
/// allowed by polkit's defaults.
pub fn power(action: &str) -> bool {
    if !matches!(action, "suspend" | "reboot" | "poweroff" | "hibernate") {
        tracing::warn!(action, "unknown power action");
        return false;
    }
    Command::new("systemctl").arg(action).spawn().map(|_| true).unwrap_or_else(|e| {
        tracing::warn!("systemctl {action}: {e}");
        false
    })
}

/// The person's terminal: the alternatives link, else the first installed.
pub fn terminal_command() -> Option<&'static str> {
    ["x-terminal-emulator", "gnome-terminal", "foot", "alacritty", "kitty", "xterm"].into_iter().find(|t| {
        std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(t).exists()))
    })
}

pub fn launch(command: &str) {
    match Command::new("sh").arg("-c").arg(command).spawn() {
        Ok(_) => tracing::info!(command, "launched"),
        Err(e) => tracing::warn!(command, "launch failed: {e}"),
    }
}

/// `playerctl` when it is installed.
pub fn media(action: &str) {
    let verb = match action {
        "media.play" => "play-pause",
        "media.next" => "next",
        "media.prev" => "previous",
        _ => return,
    };
    if let Err(e) = Command::new("playerctl").arg(verb).spawn() {
        tracing::info!("playerctl {verb}: {e}");
    }
}

/// `~/Pictures/Screenshots/Screenshot <date time>.png`.
pub fn screenshot_path() -> PathBuf {
    let base = std::env::var_os("XDG_PICTURES_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("Pictures"));
    let dir = base.join("Screenshots");
    let _ = std::fs::create_dir_all(&dir);
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: time() with a null pointer only returns; localtime_r writes the tm we own.
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        libc::localtime_r(&now, &mut tm);
    }
    dir.join(format!("Screenshot {:04}-{:02}-{:02} {:02}-{:02}-{:02}.png", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min, tm.tm_sec))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_wpctl_volume_lines() {
        assert_eq!(parse_wpctl_volume("Volume: 0.45\n"), Some((45, false)));
        assert_eq!(parse_wpctl_volume("Volume: 1.00 [MUTED]\n"), Some((100, true)));
        assert_eq!(parse_wpctl_volume("nothing"), None);
        assert_eq!(Level { icon: "x", percent: 30, muted: true }.label(), "muted");
        assert_eq!(Level { icon: "x", percent: 30, muted: false }.label(), "30%");
    }

    #[test]
    fn only_known_power_actions_run() {
        assert!(!power("explode"));
    }
}
