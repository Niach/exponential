//! `ede-compositor doctor`: what this machine needs for eDE, and what is
//! missing. The checks are pure functions over what was read from the
//! system, so they are tested without the hardware.

use std::{path::Path, process::Command};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub name: &'static str,
    pub level: Level,
    pub detail: String,
}

/// Xwayland from 24.1 on has explicit sync; older ones flicker with X11
/// games on NVIDIA.
pub const XWAYLAND_MIN: (u32, u32) = (24, 1);

/// `"The X.Org Foundation Xwayland Version 23.2.6 (12302006)"` → (23, 2, 6).
pub fn parse_xwayland_version(output: &str) -> Option<(u32, u32, u32)> {
    let rest = output.split("Xwayland Version ").nth(1)?;
    let mut parts = rest.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).map(|p| p.parse::<u32>().ok());
    Some((parts.next()??, parts.next()??, parts.next().flatten().unwrap_or(0)))
}

pub fn check_xwayland(version_output: Option<&str>) -> Check {
    let name = "Xwayland";
    match version_output.map(parse_xwayland_version) {
        None => Check { name, level: Level::Warn, detail: "not installed: X11 apps and most games won't run (apt install xwayland)".into() },
        Some(None) => Check { name, level: Level::Warn, detail: "installed, version unreadable".into() },
        Some(Some((major, minor, patch))) if (major, minor) >= XWAYLAND_MIN => {
            Check { name, level: Level::Ok, detail: format!("{major}.{minor}.{patch}, explicit sync") }
        }
        Some(Some((major, minor, patch))) => Check {
            name,
            level: Level::Warn,
            detail: format!(
                "{major}.{minor}.{patch} has no explicit sync: X11 games may flicker on NVIDIA; needs >= {}.{}",
                XWAYLAND_MIN.0, XWAYLAND_MIN.1
            ),
        },
    }
}

/// What we know about the NVIDIA driver; `None` when it isn't loaded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Nvidia {
    /// `/sys/module/nvidia/version`, e.g. `595.99.02`.
    pub version: Option<String>,
    /// `nvidia_drm` parameters (`Y`/`N`); root-only on most distros.
    pub modeset_param: Option<String>,
    pub fbdev_param: Option<String>,
    /// `/proc/cmdline`.
    pub cmdline: String,
}

/// A boolean kernel parameter: the readable module parameter, else the
/// command line, else the driver's default (on from 560).
fn nvidia_flag(param: Option<&str>, cmdline: &str, key: &str, version: Option<&str>) -> Option<bool> {
    if let Some(p) = param {
        return Some(p.trim() == "Y");
    }
    for word in cmdline.split_whitespace() {
        for prefix in [format!("nvidia-drm.{key}="), format!("nvidia_drm.{key}=")] {
            if let Some(v) = word.strip_prefix(&prefix) {
                return Some(matches!(v, "1" | "Y" | "y"));
            }
        }
    }
    let major: u32 = version?.split('.').next()?.parse().ok()?;
    Some(major >= 560)
}

pub fn check_nvidia(nv: Option<&Nvidia>) -> Option<Check> {
    let nv = nv?;
    let name = "NVIDIA";
    let version = nv.version.as_deref().map(str::trim);
    let driver = version.map(|v| format!("driver {v}, ")).unwrap_or_default();
    match nvidia_flag(nv.modeset_param.as_deref(), &nv.cmdline, "modeset", version) {
        Some(false) => Some(Check {
            name,
            level: Level::Fail,
            detail: format!("{driver}nvidia-drm.modeset is off: eDE can't drive the displays (add nvidia-drm.modeset=1 to the kernel command line)"),
        }),
        None => Some(Check { name, level: Level::Warn, detail: format!("{driver}can't tell whether nvidia-drm.modeset is on (run doctor as root)") }),
        Some(true) => match nvidia_flag(nv.fbdev_param.as_deref(), &nv.cmdline, "fbdev", version) {
            Some(false) => Some(Check { name, level: Level::Warn, detail: format!("{driver}modeset on; nvidia-drm.fbdev=1 is recommended for a flicker-free VT switch") }),
            _ => Some(Check { name, level: Level::Ok, detail: format!("{driver}nvidia-drm modeset on") }),
        },
    }
}

pub fn check_render_nodes(nodes: &[String]) -> Check {
    let name = "GPU";
    if nodes.is_empty() {
        Check { name, level: Level::Fail, detail: "no /dev/dri/renderD* node: no GPU driver loaded".into() }
    } else {
        Check { name, level: Level::Ok, detail: nodes.join(", ") }
    }
}

/// Reads the system and runs every check.
pub fn run_checks() -> Vec<Check> {
    let xwayland = Command::new("Xwayland").arg("-version").output().ok().map(|o| {
        let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
        s.push_str(&String::from_utf8_lossy(&o.stderr));
        s
    });
    let read = |p: &str| std::fs::read_to_string(p).ok();
    let mut nodes: Vec<String> = std::fs::read_dir(Path::new("/dev/dri"))
        .map(|d| d.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with("renderD")).collect())
        .unwrap_or_default();
    nodes.sort();
    let mut checks = vec![check_render_nodes(&nodes), check_xwayland(xwayland.as_deref())];
    let nvidia = Path::new("/sys/module/nvidia_drm").exists().then(|| Nvidia {
        version: read("/sys/module/nvidia/version"),
        modeset_param: read("/sys/module/nvidia_drm/parameters/modeset"),
        fbdev_param: read("/sys/module/nvidia_drm/parameters/fbdev"),
        cmdline: read("/proc/cmdline").unwrap_or_default(),
    });
    checks.extend(check_nvidia(nvidia.as_ref()));
    checks
}

/// Prints the checks; `false` if any failed.
pub fn report(checks: &[Check]) -> bool {
    for c in checks {
        let mark = match c.level {
            Level::Ok => "ok  ",
            Level::Warn => "warn",
            Level::Fail => "FAIL",
        };
        println!("{mark}  {:<9} {}", c.name, c.detail);
    }
    !checks.iter().any(|c| c.level == Level::Fail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_xwayland_versions() {
        assert_eq!(parse_xwayland_version("The X.Org Foundation Xwayland Version 23.2.6 (12302006)\nX Protocol"), Some((23, 2, 6)));
        assert_eq!(parse_xwayland_version("Xwayland Version 24.1.0"), Some((24, 1, 0)));
        assert_eq!(parse_xwayland_version("Xwayland Version 24.1"), Some((24, 1, 0)));
        assert_eq!(parse_xwayland_version("Xorg"), None);
    }

    #[test]
    fn xwayland_below_24_1_warns_about_explicit_sync() {
        assert_eq!(check_xwayland(Some("Xwayland Version 23.2.6")).level, Level::Warn);
        assert_eq!(check_xwayland(Some("Xwayland Version 24.1.4")).level, Level::Ok);
        assert_eq!(check_xwayland(Some("Xwayland Version 25.0.0")).level, Level::Ok);
        assert!(check_xwayland(None).detail.contains("apt install xwayland"));
    }

    #[test]
    fn nvidia_needs_modeset_and_wants_fbdev() {
        let nv = |version: Option<&str>, modeset: Option<&str>, fbdev: Option<&str>, cmdline: &str| Nvidia {
            version: version.map(Into::into),
            modeset_param: modeset.map(Into::into),
            fbdev_param: fbdev.map(Into::into),
            cmdline: cmdline.into(),
        };
        let level = |n: Nvidia| check_nvidia(Some(&n)).unwrap().level;
        assert_eq!(check_nvidia(None), None, "no NVIDIA driver, no check");
        // readable parameters win
        assert_eq!(level(nv(Some("595.99.02"), Some("N\n"), None, "")), Level::Fail);
        assert_eq!(level(nv(None, Some("Y\n"), Some("N\n"), "")), Level::Warn);
        assert_eq!(level(nv(None, Some("Y\n"), Some("Y\n"), "")), Level::Ok);
        // root-only parameters: the command line, then the driver default
        assert_eq!(level(nv(Some("550.1"), None, None, "quiet nvidia-drm.modeset=1 nvidia_drm.fbdev=1")), Level::Ok);
        assert_eq!(level(nv(Some("595.99.02"), None, None, "quiet splash")), Level::Ok, "560+ defaults to modeset + fbdev");
        assert_eq!(level(nv(Some("550.1"), None, None, "quiet splash")), Level::Fail, "older drivers default to off");
        assert_eq!(level(nv(Some("595.99.02"), None, None, "nvidia-drm.modeset=0")), Level::Fail);
        assert_eq!(level(nv(None, None, None, "")), Level::Warn, "unknown");
    }

    #[test]
    fn no_render_node_fails() {
        assert_eq!(check_render_nodes(&[]).level, Level::Fail);
        assert_eq!(check_render_nodes(&["renderD128".into()]).level, Level::Ok);
    }
}
