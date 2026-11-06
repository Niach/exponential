//! `ede-compositor doctor`: what this machine needs for eDE, and what is
//! missing: the render nodes, the KMS driver and its connected displays,
//! the seat (logind or seatd), what EGL renders with, the keyboard layout,
//! Xwayland, NVIDIA. The checks are pure functions over what was read from the
//! system, so they are tested without the hardware.

use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::Command,
};

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

/// Where the .deb puts its own Xwayland (#NEW-120): Ubuntu noble ships 23.2.
pub const BUNDLED_XWAYLAND_DIR: &str = "/usr/lib/ede/bin";

/// The `PATH` Xwayland is spawned with: the bundled directory first when it
/// holds an Xwayland, so it wins over the distro's.
pub fn xwayland_search_path(path: Option<&OsStr>, bundled_dir: &Path) -> OsString {
    let path = path.unwrap_or_default();
    if !bundled_dir.join("Xwayland").is_file() {
        return path.to_os_string();
    }
    let dirs = std::iter::once(bundled_dir.to_path_buf()).chain(std::env::split_paths(path).filter(|d| !d.as_os_str().is_empty() && d != bundled_dir));
    std::env::join_paths(dirs).unwrap_or_else(|_| path.to_os_string())
}

/// The Xwayland a spawn with `path` runs: the first one on it.
pub fn xwayland_binary(path: &OsStr) -> Option<PathBuf> {
    std::env::split_paths(path).map(|d| d.join("Xwayland")).find(|b| b.is_file())
}

pub fn check_xwayland(binary: Option<&Path>, version_output: Option<&str>) -> Check {
    let name = "Xwayland";
    let (Some(binary), Some(output)) = (binary, version_output) else {
        return Check { name, level: Level::Warn, detail: "not installed: X11 apps and most games won't run (apt install xwayland)".into() };
    };
    let binary = binary.display();
    match parse_xwayland_version(output) {
        None => Check { name, level: Level::Warn, detail: format!("{binary}, version unreadable") },
        Some((major, minor, patch)) if (major, minor) >= XWAYLAND_MIN => {
            Check { name, level: Level::Ok, detail: format!("{binary} {major}.{minor}.{patch}, explicit sync") }
        }
        Some((major, minor, patch)) => Check {
            name,
            level: Level::Warn,
            detail: format!(
                "{binary} {major}.{minor}.{patch} has no explicit sync: X11 games may flicker on NVIDIA; needs >= {}.{}",
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

/// What logind (or seatd) says about the seat the real session takes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Seat {
    /// `/run/systemd/seats/seat0`, when systemd-logind runs.
    pub logind_seat0: Option<String>,
    /// `/run/seatd.sock` exists.
    pub seatd: bool,
}

/// The udev backend takes the GPU and the input devices through libseat:
/// logind's seat0 with a graphical device on it, or a seatd socket.
pub fn check_seat(seat: &Seat) -> Check {
    let name = "seat";
    if let Some(info) = &seat.logind_seat0 {
        let flag = |key: &str| info.lines().any(|l| l.trim() == format!("{key}=1"));
        if !flag("CAN_GRAPHICAL") {
            return Check { name, level: Level::Fail, detail: "logind seat0 has no GPU attached (CAN_GRAPHICAL=0): the session can't take a display".into() };
        }
        let vt = if flag("CAN_TTY") { "" } else { ", no VT" };
        return Check { name, level: Level::Ok, detail: format!("logind seat0, graphical{vt}") };
    }
    if seat.seatd {
        return Check { name, level: Level::Ok, detail: "seatd at /run/seatd.sock".into() };
    }
    Check { name, level: Level::Fail, detail: "neither systemd-logind (seat0) nor seatd runs: the session can't take the GPU and the input devices".into() }
}

/// One `/dev/dri/cardN` as sysfs describes it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DrmCard {
    pub name: String,
    /// `DRIVER=` from `device/uevent`.
    pub driver: Option<String>,
    /// Connectors whose `status` is `connected`.
    pub connected: Vec<String>,
}

/// Reads `sys` (`/sys/class/drm`): every `cardN` with its driver and its
/// connected connectors (`cardN-<connector>/status`).
pub fn read_drm_cards(sys: &Path) -> Vec<DrmCard> {
    let Ok(entries) = std::fs::read_dir(sys) else { return Vec::new() };
    let names: Vec<String> = entries.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    let mut cards: Vec<DrmCard> = names
        .iter()
        .filter(|n| n.strip_prefix("card").is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())))
        .map(|card| {
            let uevent = std::fs::read_to_string(sys.join(card).join("device/uevent")).unwrap_or_default();
            let driver = uevent.lines().find_map(|l| l.strip_prefix("DRIVER=")).map(|d| d.trim().to_string());
            let prefix = format!("{card}-");
            let mut connected: Vec<String> = names
                .iter()
                .filter_map(|n| n.strip_prefix(&prefix))
                .filter(|c| std::fs::read_to_string(sys.join(format!("{prefix}{c}")).join("status")).is_ok_and(|s| s.trim() == "connected"))
                .map(str::to_string)
                .collect();
            connected.sort();
            DrmCard { name: card.clone(), driver, connected }
        })
        .collect();
    cards.sort_by(|a, b| a.name.cmp(&b.name));
    cards
}

/// Firmware framebuffers: KMS works on them, nothing accelerates.
const FRAMEBUFFER_DRIVERS: [&str; 3] = ["simpledrm", "efifb", "vesafb"];

/// Every display the real session can drive comes from a KMS driver; the
/// known virtual ones get a word on what to expect.
pub fn check_drm(cards: &[DrmCard]) -> Check {
    let name = "DRM";
    if cards.is_empty() {
        return Check { name, level: Level::Fail, detail: "no /dev/dri/card*: no KMS driver loaded, eDE can't drive a display".into() };
    }
    let mut level = Level::Ok;
    let parts: Vec<String> = cards
        .iter()
        .map(|c| {
            let driver = c.driver.as_deref().unwrap_or("unknown driver");
            let note = match driver {
                "vmwgfx" => " (VMware SVGA / VirtualBox VMSVGA: software rendering unless the VM has 3D acceleration)",
                "vboxvideo" => " (VirtualBox VBoxVGA/VBoxSVGA: software rendering; VMSVGA is the supported controller)",
                "virtio_gpu" | "virtio-pci" => " (virtio-gpu)",
                d if FRAMEBUFFER_DRIVERS.contains(&d) => " (firmware framebuffer, no GPU driver: works, slowly)",
                _ => "",
            };
            let connectors = if c.connected.is_empty() { "nothing connected".to_string() } else { c.connected.join(", ") };
            format!("{} {driver}{note}: {connectors}", c.name)
        })
        .collect();
    if cards.iter().all(|c| c.driver.as_deref().is_some_and(|d| FRAMEBUFFER_DRIVERS.contains(&d))) {
        level = Level::Warn;
    }
    let mut detail = parts.join("; ");
    if cards.iter().all(|c| c.connected.is_empty()) {
        level = Level::Warn;
        detail.push_str(" (no display connected: the session would start with no output)");
    }
    Check { name, level, detail }
}

/// What the real session renders with: EGL + GLES on `render_node`, the
/// same way the udev backend opens it (no seat needed, so it runs from
/// inside another desktop). The GL renderer and version strings.
#[cfg(feature = "udev")]
pub fn probe_renderer(render_node: &Path) -> Result<(String, String), String> {
    use smithay::{
        backend::{
            allocator::gbm::GbmDevice,
            drm::DrmDeviceFd,
            egl::{EGLContext, EGLDisplay},
            renderer::gles::{ffi, GlesRenderer},
        },
        utils::DeviceFd,
    };
    use std::os::fd::OwnedFd;

    unsafe fn gl_string(gl: &ffi::Gles2, which: ffi::types::GLenum) -> String {
        let p = gl.GetString(which) as *const std::ffi::c_char;
        if p.is_null() { String::new() } else { std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned() }
    }

    let file = std::fs::OpenOptions::new().read(true).write(true).open(render_node).map_err(|e| format!("{}: {e}", render_node.display()))?;
    let fd = DrmDeviceFd::new(DeviceFd::from(OwnedFd::from(file)));
    let gbm = GbmDevice::new(fd).map_err(|e| format!("gbm: {e}"))?;
    let display = unsafe { EGLDisplay::new(gbm) }.map_err(|e| format!("EGL display: {e}"))?;
    let context = EGLContext::new(&display).map_err(|e| format!("EGL context: {e}"))?;
    let mut renderer = unsafe { GlesRenderer::new(context) }.map_err(|e| format!("GLES: {e}"))?;
    renderer.with_context(|gl| unsafe { (gl_string(gl, ffi::RENDERER), gl_string(gl, ffi::VERSION)) }).map_err(|e| format!("GL: {e}"))
}

/// Software renderers Mesa falls back to (a VM without 3D, no GPU driver).
const SOFTWARE_RENDERERS: [&str; 3] = ["llvmpipe", "softpipe", "swrast"];

/// `None` = this binary has no udev backend, so nothing was probed.
pub fn check_renderer(probe: Option<Result<(String, String), String>>) -> Check {
    let name = "render";
    match probe {
        None => Check { name, level: Level::Warn, detail: "built without the udev backend: nested and headless only (cargo build --features udev)".into() },
        Some(Err(e)) => Check { name, level: Level::Fail, detail: format!("EGL/GLES on the render node failed: {e}; the real session can't draw") },
        Some(Ok((renderer, version))) => {
            let software = SOFTWARE_RENDERERS.iter().any(|s| renderer.contains(s));
            let note = if software { " (software rendering: expected in a VM, slow on a real GPU)" } else { "" };
            Check { name, level: Level::Ok, detail: format!("{renderer}, {version}{note}") }
        }
    }
}

/// The layout the session starts with when `[keyboard]` is unset: the
/// system's file, else xkb's `us`, which types the wrong password on any
/// other keyboard.
pub fn check_keyboard(system: Option<&crate::config::Keyboard>) -> Check {
    let name = "keyboard";
    match system {
        Some(kb) => {
            let variant = if kb.variant.is_empty() { String::new() } else { format!(" ({})", kb.variant) };
            Check { name, level: Level::Ok, detail: format!("layout {}{variant} from {}", kb.layout, crate::config::SYSTEM_KEYBOARD) }
        }
        None => Check {
            name,
            level: Level::Warn,
            detail: format!("no layout in {}: eDE starts with `us` unless ~/.config/ede/compositor.toml sets [keyboard] layout", crate::config::SYSTEM_KEYBOARD),
        },
    }
}

/// Reads the system and runs every check.
pub fn run_checks() -> Vec<Check> {
    let path = xwayland_search_path(std::env::var_os("PATH").as_deref(), Path::new(BUNDLED_XWAYLAND_DIR));
    let binary = xwayland_binary(&path);
    let xwayland = binary.as_ref().and_then(|b| Command::new(b).arg("-version").output().ok()).map(|o| {
        let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
        s.push_str(&String::from_utf8_lossy(&o.stderr));
        s
    });
    let read = |p: &str| std::fs::read_to_string(p).ok();
    let mut nodes: Vec<String> = std::fs::read_dir(Path::new("/dev/dri"))
        .map(|d| d.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with("renderD")).collect())
        .unwrap_or_default();
    nodes.sort();
    let seat = Seat { logind_seat0: read("/run/systemd/seats/seat0"), seatd: Path::new("/run/seatd.sock").exists() };
    let cards = read_drm_cards(Path::new("/sys/class/drm"));
    #[cfg(feature = "udev")]
    let probe = nodes.first().map(|n| probe_renderer(&Path::new("/dev/dri").join(n)));
    #[cfg(not(feature = "udev"))]
    let probe: Option<Result<(String, String), String>> = None;
    let system_keyboard = read(crate::config::SYSTEM_KEYBOARD).and_then(|t| crate::config::parse_system_keyboard(&t));
    let mut checks = vec![
        check_render_nodes(&nodes),
        check_drm(&cards),
        check_seat(&seat),
        check_renderer(probe),
        check_keyboard(system_keyboard.as_ref()),
        check_xwayland(binary.as_deref(), xwayland.as_deref()),
    ];
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
        let bin = Some(Path::new("/usr/bin/Xwayland"));
        assert_eq!(check_xwayland(bin, Some("Xwayland Version 23.2.6")).level, Level::Warn);
        assert_eq!(check_xwayland(bin, Some("Xwayland Version 24.1.4")).level, Level::Ok);
        assert_eq!(check_xwayland(bin, Some("Xwayland Version 25.0.0")).level, Level::Ok);
        assert!(check_xwayland(None, None).detail.contains("apt install xwayland"));
    }

    #[test]
    fn doctor_names_the_xwayland_it_checked() {
        let check = check_xwayland(Some(Path::new("/usr/lib/ede/bin/Xwayland")), Some("Xwayland Version 24.1.14"));
        assert_eq!(check.detail, "/usr/lib/ede/bin/Xwayland 24.1.14, explicit sync");
    }

    #[test]
    fn the_bundled_xwayland_goes_first_on_path_only_when_it_exists() {
        let root = std::env::temp_dir().join(format!("ede-xwl-{}", std::process::id()));
        let (bundled, system) = (root.join("ede-bin"), root.join("usr-bin"));
        std::fs::create_dir_all(&bundled).unwrap();
        std::fs::create_dir_all(&system).unwrap();
        std::fs::write(system.join("Xwayland"), "").unwrap();
        let path = std::env::join_paths([&system, &bundled]).unwrap();

        let without = xwayland_search_path(Some(&path), &bundled);
        assert_eq!(without, path);
        assert_eq!(xwayland_binary(&without), Some(system.join("Xwayland")));

        std::fs::write(bundled.join("Xwayland"), "").unwrap();
        let with = xwayland_search_path(Some(&path), &bundled);
        assert_eq!(std::env::split_paths(&with).collect::<Vec<_>>(), [bundled.clone(), system.clone()]);
        assert_eq!(xwayland_binary(&with), Some(bundled.join("Xwayland")));
        assert_eq!(xwayland_search_path(None, &bundled), bundled.as_os_str());
        std::fs::remove_dir_all(&root).unwrap();
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

    #[test]
    fn the_seat_comes_from_logind_or_seatd() {
        let logind = |body: &str| Seat { logind_seat0: Some(body.into()), seatd: false };
        let ok = check_seat(&logind("# private\nIS_SEAT0=1\nCAN_TTY=1\nCAN_GRAPHICAL=1\nACTIVE=2\n"));
        assert_eq!((ok.level, ok.detail.as_str()), (Level::Ok, "logind seat0, graphical"));
        assert_eq!(check_seat(&logind("IS_SEAT0=1\nCAN_GRAPHICAL=0\n")).level, Level::Fail, "no GPU on the seat");
        assert_eq!(check_seat(&Seat { logind_seat0: None, seatd: true }).level, Level::Ok);
        assert_eq!(check_seat(&Seat::default()).level, Level::Fail);
    }

    #[test]
    fn drm_cards_are_read_from_sysfs() {
        let sys = std::env::temp_dir().join(format!("ede-drm-{}", std::process::id()));
        for (path, body) in [
            ("card0/device/uevent", "DRIVER=vmwgfx\nPCI_CLASS=30000\n"),
            ("card0-Virtual-1/status", "connected\n"),
            ("card0-Virtual-2/status", "disconnected\n"),
            ("card1/device/uevent", "DRIVER=simpledrm\n"),
            ("renderD128/dev", "226:128\n"),
        ] {
            let p = sys.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }
        let cards = read_drm_cards(&sys);
        assert_eq!(
            cards,
            [
                DrmCard { name: "card0".into(), driver: Some("vmwgfx".into()), connected: vec!["Virtual-1".into()] },
                DrmCard { name: "card1".into(), driver: Some("simpledrm".into()), connected: vec![] },
            ]
        );
        std::fs::remove_dir_all(&sys).unwrap();
        assert_eq!(read_drm_cards(&sys), []);
    }

    #[test]
    fn drm_check_names_the_virtual_gpus_and_warns_without_a_display() {
        let vm = check_drm(&[DrmCard { name: "card0".into(), driver: Some("vmwgfx".into()), connected: vec!["Virtual-1".into()] }]);
        assert_eq!(vm.level, Level::Ok);
        assert!(vm.detail.contains("card0 vmwgfx") && vm.detail.contains("VirtualBox VMSVGA") && vm.detail.ends_with("Virtual-1"), "{}", vm.detail);
        let fb = check_drm(&[DrmCard { name: "card0".into(), driver: Some("simpledrm".into()), connected: vec!["Unknown-1".into()] }]);
        assert_eq!(fb.level, Level::Warn, "a firmware framebuffer only");
        let dark = check_drm(&[DrmCard { name: "card0".into(), driver: Some("amdgpu".into()), connected: vec![] }]);
        assert_eq!(dark.level, Level::Warn);
        assert!(dark.detail.contains("no display connected"));
        assert_eq!(check_drm(&[]).level, Level::Fail);
    }

    #[test]
    fn the_keyboard_check_names_the_system_layout() {
        let de = crate::config::Keyboard { layout: "de".into(), variant: "nodeadkeys".into(), ..Default::default() };
        let check = check_keyboard(Some(&de));
        assert_eq!(check.level, Level::Ok);
        assert_eq!(check.detail, "layout de (nodeadkeys) from /etc/default/keyboard");
        assert_eq!(check_keyboard(None).level, Level::Warn);
    }

    #[test]
    fn the_renderer_check_tells_software_from_hardware() {
        let sw = check_renderer(Some(Ok(("llvmpipe (LLVM 20.1.8, 256 bits)".into(), "OpenGL ES 3.2 Mesa 26.0.8".into()))));
        assert_eq!(sw.level, Level::Ok);
        assert!(sw.detail.contains("software rendering"), "{}", sw.detail);
        let hw = check_renderer(Some(Ok(("AMD Radeon RX 7800 XT".into(), "OpenGL ES 3.2 Mesa 26.0.8".into()))));
        assert!(!hw.detail.contains("software"));
        assert_eq!(check_renderer(Some(Err("EGL display: no EGL".into()))).level, Level::Fail);
        assert_eq!(check_renderer(None).level, Level::Warn, "a winit-only build");
    }
}
