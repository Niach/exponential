//! `exponential-compositor doctor`: what this machine needs for eDE, and what is
//! missing: the render nodes, the KMS driver and its connected displays,
//! what the kernel's KMS offers on the GPU that drives them (atomic, a
//! cursor plane, modifiers, explicit sync), the seat (logind or seatd),
//! what EGL renders with, the Vulkan GPU the shell takes, the keyboard
//! layout, Xwayland, and NVIDIA's driver (#EDE-59: the kernel module, open
//! or proprietary, `nvidia-drm.modeset` and `fbdev` with where each value
//! comes from, the GBM backend, the EGL GBM platform, video memory across
//! suspend). The checks are pure functions over what was read from the
//! system (`Nvidia::read(root)`, `read_drm_cards(sys)`, `parse_vulkaninfo`),
//! so they are tested without the hardware: `tests/fixtures/doctor/` holds
//! a copy of what a Mint 22.3 box with an RTX 4090 on driver 595 showed.

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
pub const BUNDLED_XWAYLAND_DIR: &str = "/usr/lib/exponential-desktop/bin";

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
                "{binary} {major}.{minor}.{patch} has no explicit sync: X11 games may flicker on NVIDIA; needs >= {}.{} (the .deb bundles 24.1 in {BUNDLED_XWAYLAND_DIR})",
                XWAYLAND_MIN.0, XWAYLAND_MIN.1
            ),
        },
    }
}

/// `root.join(absolute)`: the system's files under another root in tests.
fn under(root: &Path, absolute: &str) -> PathBuf {
    root.join(absolute.trim_start_matches('/'))
}

/// What we know about the NVIDIA driver; `None` when it isn't loaded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Nvidia {
    /// `/sys/module/nvidia/version`, e.g. `595.99.02`.
    pub version: Option<String>,
    /// `/proc/driver/nvidia/version`: the NVRM banner, which names the open
    /// kernel module ("NVIDIA UNIX Open Kernel Module for x86_64 …").
    pub proc_version: Option<String>,
    /// `/proc/driver/nvidia/params`: the `nvidia` module's parameters
    /// (`PreserveVideoMemoryAllocations: 1`), readable by everyone.
    pub params: Option<String>,
    /// `nvidia_drm` parameters (`Y`/`N`); root-only (0400) on most distros.
    pub modeset_param: Option<String>,
    pub fbdev_param: Option<String>,
    /// `/proc/cmdline`.
    pub cmdline: String,
    /// `/etc/modprobe.d/*.conf`, then `/lib/modprobe.d/*.conf` and
    /// `/usr/lib/modprobe.d/*.conf`: (path, text) of every file that names
    /// nvidia. The driver's own package writes `options nvidia_drm
    /// modeset=1` there, nvidia-prime `options nvidia-drm modeset=1`.
    pub modprobe: Vec<(String, String)>,
    /// Whether the driver's GBM backend (`nvidia-drm_gbm.so`) is installed;
    /// without it the compositor can't allocate buffers on the GPU.
    pub gbm_backend: bool,
    /// Whether libEGL_nvidia has its GBM platform (`15_nvidia_gbm.json` in
    /// `egl_external_platform.d`); without it `EGLDisplay::new(gbm)` fails.
    pub egl_gbm_platform: bool,
    /// `systemctl is-enabled nvidia-suspend.service nvidia-resume.service`,
    /// one word per line; `None` = systemctl unavailable.
    pub suspend_units: Option<String>,
}

/// The driver that has explicit sync (no flicker in Xwayland games).
pub const NVIDIA_EXPLICIT_SYNC: u32 = 555;
/// The oldest driver the session runs on: GBM with modifiers, EGL device
/// render nodes (`EGL_EXT_device_drm_render_node`) and linux-dmabuf v4 all
/// need it. Set from NVIDIA's release notes, not from a run on hardware.
pub const NVIDIA_MIN: u32 = 525;
/// From 560 on `nvidia-drm.modeset` and `fbdev` default to on.
pub const NVIDIA_MODESET_DEFAULT: u32 = 560;
/// Where Mesa's libgbm looks for backends (`$GBM_BACKENDS_PATH` first).
pub const GBM_BACKEND_DIRS: &[&str] = &["/usr/lib/x86_64-linux-gnu/gbm", "/usr/lib/aarch64-linux-gnu/gbm", "/usr/lib64/gbm", "/usr/lib/gbm"];
/// Where libEGL_nvidia finds its platform libraries.
pub const EGL_EXTERNAL_PLATFORM_DIRS: &[&str] = &["/usr/share/egl/egl_external_platform.d", "/etc/egl/egl_external_platform.d"];
pub const MODPROBE_DIRS: &[&str] = &["/etc/modprobe.d", "/lib/modprobe.d", "/usr/lib/modprobe.d"];

impl Nvidia {
    /// Reads the driver's state from the system under `root` (`/`);
    /// `None` when `nvidia_drm` isn't loaded. `suspend_units` is left for
    /// the caller (a command, not a file).
    pub fn read(root: &Path) -> Option<Nvidia> {
        if !under(root, "/sys/module/nvidia_drm").exists() {
            return None;
        }
        let read = |p: &str| std::fs::read_to_string(under(root, p)).ok();
        let mut modprobe = Vec::new();
        for dir in MODPROBE_DIRS {
            let Ok(entries) = std::fs::read_dir(under(root, dir)) else { continue };
            let mut files: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "conf")).collect();
            files.sort();
            for file in files {
                if let Ok(text) = std::fs::read_to_string(&file) {
                    if text.contains("nvidia") {
                        modprobe.push((format!("{dir}/{}", file.file_name().unwrap_or_default().to_string_lossy()), text));
                    }
                }
            }
        }
        let gbm_dirs = std::env::var_os("GBM_BACKENDS_PATH")
            .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .chain(GBM_BACKEND_DIRS.iter().map(|d| under(root, d)));
        let egl_gbm_platform = EGL_EXTERNAL_PLATFORM_DIRS.iter().any(|dir| {
            std::fs::read_dir(under(root, dir)).is_ok_and(|entries| {
                entries.filter_map(|e| e.ok()).any(|e| {
                    let name = e.file_name().to_string_lossy().to_lowercase();
                    name.contains("nvidia") && name.contains("gbm") && name.ends_with(".json")
                })
            })
        });
        Some(Nvidia {
            version: read("/sys/module/nvidia/version"),
            proc_version: read("/proc/driver/nvidia/version"),
            params: read("/proc/driver/nvidia/params"),
            modeset_param: read("/sys/module/nvidia_drm/parameters/modeset"),
            fbdev_param: read("/sys/module/nvidia_drm/parameters/fbdev"),
            cmdline: read("/proc/cmdline").unwrap_or_default(),
            modprobe,
            gbm_backend: gbm_dirs.into_iter().any(|dir| dir.join("nvidia-drm_gbm.so").exists()),
            egl_gbm_platform,
            suspend_units: None,
        })
    }

    /// `595.99.02` → 595.
    pub fn major(&self) -> Option<u32> {
        self.version.as_deref()?.trim().split('.').next()?.parse().ok()
    }

    /// Open (`nvidia-open`, "Dual MIT/GPL") or proprietary kernel module,
    /// from the NVRM banner; `None` when it can't be read.
    pub fn kernel_module(&self) -> Option<KernelModule> {
        let banner = self.proc_version.as_deref()?;
        Some(if banner.contains("Open Kernel Module") { KernelModule::Open } else { KernelModule::Proprietary })
    }

    /// A parameter of the `nvidia` module from `/proc/driver/nvidia/params`
    /// (`PreserveVideoMemoryAllocations: 1` → `"1"`, `TemporaryFilePath:
    /// "/var"` → `"/var"`).
    pub fn param(&self, key: &str) -> Option<String> {
        let params = self.params.as_deref()?;
        params.lines().find_map(|l| l.trim().strip_prefix(key)?.strip_prefix(':')).map(|v| v.trim().trim_matches('"').to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelModule {
    /// `nvidia-open` (`nvidia-dkms-NNN-open`, license "Dual MIT/GPL"), the
    /// one NVIDIA recommends from 560 on for Turing and newer.
    Open,
    Proprietary,
}

impl KernelModule {
    pub fn as_str(self) -> &'static str {
        match self {
            KernelModule::Open => "open kernel module",
            KernelModule::Proprietary => "proprietary kernel module",
        }
    }
}

/// A boolean `nvidia_drm` parameter and where its value came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Flag {
    pub on: bool,
    pub source: String,
}

/// `nvidia_drm.<key>`: the readable module parameter, else the kernel
/// command line, else `options nvidia_drm <key>=` in modprobe.d (the
/// driver's package writes it there), else the driver's default (on from
/// 560). `None` = none of these say.
pub fn nvidia_flag(nv: &Nvidia, key: &str) -> Option<Flag> {
    let param = match key {
        "modeset" => nv.modeset_param.as_deref(),
        "fbdev" => nv.fbdev_param.as_deref(),
        _ => None,
    };
    if let Some(p) = param {
        return Some(Flag { on: p.trim() == "Y", source: "the module parameter".into() });
    }
    let value_of = |word: &str| -> Option<bool> {
        for prefix in [format!("nvidia-drm.{key}="), format!("nvidia_drm.{key}=")] {
            if let Some(v) = word.strip_prefix(&prefix) {
                return Some(matches!(v, "1" | "Y" | "y"));
            }
        }
        None
    };
    if let Some(on) = nv.cmdline.split_whitespace().find_map(value_of) {
        return Some(Flag { on, source: "the kernel command line".into() });
    }
    for (path, text) in &nv.modprobe {
        for line in text.lines().map(str::trim).filter(|l| l.starts_with("options")) {
            let mut words = line.split_whitespace();
            words.next();
            let Some(module) = words.next() else { continue };
            if module != "nvidia_drm" && module != "nvidia-drm" {
                continue;
            }
            if let Some(v) = words.find_map(|w| w.strip_prefix(&format!("{key}="))) {
                return Some(Flag { on: matches!(v, "1" | "Y" | "y"), source: path.clone() });
            }
        }
    }
    let major = nv.major()?;
    Some(if major >= NVIDIA_MODESET_DEFAULT {
        Flag { on: true, source: format!("the driver's default from {NVIDIA_MODESET_DEFAULT}") }
    } else {
        Flag { on: false, source: format!("the driver's default before {NVIDIA_MODESET_DEFAULT}") }
    })
}

/// The NVIDIA driver: its version against the minimum, the kernel module
/// it runs, and `nvidia-drm.modeset` (required) + `fbdev` (recommended),
/// each with the place its value comes from.
pub fn check_nvidia(nv: Option<&Nvidia>) -> Option<Check> {
    let nv = nv?;
    let name = "NVIDIA";
    let version = nv.version.as_deref().map(str::trim);
    let module = nv.kernel_module().map(|m| format!(", {}", m.as_str())).unwrap_or_default();
    let driver = match version {
        Some(v) => format!("driver {v}{module}"),
        None => format!("driver of unknown version{module}"),
    };
    if let Some(major) = nv.major() {
        if major < NVIDIA_MIN {
            return Some(Check {
                name,
                level: Level::Fail,
                detail: format!("{driver} is too old: Exponential Desktop needs {NVIDIA_MIN} or newer (GBM with modifiers, EGL render nodes), {NVIDIA_EXPLICIT_SYNC}+ for explicit sync"),
            });
        }
    }
    let modeset = nvidia_flag(nv, "modeset");
    let fbdev = nvidia_flag(nv, "fbdev");
    match modeset {
        Some(Flag { on: false, source }) => Some(Check {
            name,
            level: Level::Fail,
            detail: format!("{driver}; nvidia-drm.modeset is off ({source}): no atomic KMS, Exponential Desktop can't drive the displays (write `options nvidia_drm modeset=1` to /etc/modprobe.d/nvidia-graphics-drivers-kms.conf, run update-initramfs -u, reboot)"),
        }),
        None => Some(Check { name, level: Level::Warn, detail: format!("{driver}; can't tell whether nvidia-drm.modeset is on (the parameter is root-only: run doctor as root, or check /etc/modprobe.d)") }),
        Some(Flag { on: true, source }) => {
            let mut detail = format!("{driver}; modeset on ({source})");
            let mut level = Level::Ok;
            match fbdev {
                Some(Flag { on: true, source }) => detail.push_str(&format!(", fbdev on ({source})")),
                Some(Flag { on: false, source }) => {
                    level = Level::Warn;
                    detail.push_str(&format!("; nvidia-drm.fbdev is off ({source}): set fbdev=1 too for a flicker-free VT switch and a console after a crash"));
                }
                None => detail.push_str(", fbdev unknown"),
            }
            if nv.major().is_some_and(|m| m < NVIDIA_EXPLICIT_SYNC) {
                level = Level::Warn;
                detail.push_str(&format!("; no explicit sync before driver {NVIDIA_EXPLICIT_SYNC}: X11 games and Vulkan apps may flicker"));
            }
            Some(Check { name, level, detail })
        }
    }
}

/// The user-space half the compositor's GBM + EGL path needs on NVIDIA:
/// Mesa's libgbm loads `nvidia-drm_gbm.so` for the `nvidia-drm` KMS
/// driver, and libEGL_nvidia needs its GBM platform library registered.
pub fn check_nvidia_stack(nv: Option<&Nvidia>) -> Option<Check> {
    let nv = nv?;
    let name = "NVIDIA GBM";
    Some(if !nv.gbm_backend {
        Check {
            name,
            level: Level::Fail,
            detail: "the driver's GBM backend (nvidia-drm_gbm.so) is missing: the compositor can't allocate on the GPU; it ships with the driver's GL libraries (Ubuntu, Mint: libnvidia-gl-<version>, older: libnvidia-egl-gbm1; Fedora (RPM Fusion): xorg-x11-drv-nvidia-libs; Arch: egl-gbm)".into(),
        }
    } else if !nv.egl_gbm_platform {
        Check {
            name,
            level: Level::Fail,
            detail: "libEGL_nvidia has no GBM platform (no 15_nvidia_gbm.json in /usr/share/egl/egl_external_platform.d): EGL on GBM fails; install the driver's EGL GBM library (Ubuntu, Mint: libnvidia-gl-<version>; Arch: egl-gbm)".into(),
        }
    } else {
        Check { name, level: Level::Ok, detail: "GBM backend (nvidia-drm_gbm.so) and the EGL GBM platform installed".into() }
    })
}

/// Video memory across suspend: without `NVreg_PreserveVideoMemoryAllocations=1`
/// and the driver's systemd sleep hooks the GPU forgets every allocation,
/// and the desktop (and every GL/Vulkan app) comes back garbled after a resume.
pub fn check_nvidia_suspend(nv: Option<&Nvidia>) -> Option<Check> {
    let nv = nv?;
    let name = "NVIDIA sleep";
    let Some(preserve) = nv.param("PreserveVideoMemoryAllocations") else {
        return Some(Check { name, level: Level::Warn, detail: "can't read /proc/driver/nvidia/params: unknown whether video memory survives suspend".into() });
    };
    let fix = "write `options nvidia NVreg_PreserveVideoMemoryAllocations=1` to /etc/modprobe.d/nvidia-power-management.conf, then `systemctl enable nvidia-suspend nvidia-resume nvidia-hibernate`, update-initramfs -u and reboot";
    if preserve != "1" {
        return Some(Check { name, level: Level::Warn, detail: format!("video memory is lost on suspend (NVreg_PreserveVideoMemoryAllocations={preserve}): the desktop comes back garbled after a resume; {fix}") });
    }
    let path = nv.param("TemporaryFilePath").filter(|p| !p.is_empty()).unwrap_or_else(|| "/tmp".into());
    match nv.suspend_units.as_deref() {
        None => Some(Check { name, level: Level::Warn, detail: format!("video memory kept across suspend (to {path}), but systemctl is missing: can't tell whether nvidia-suspend.service and nvidia-resume.service are enabled") }),
        Some(units) => {
            let states: Vec<&str> = units.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
            if states.len() >= 2 && states.iter().all(|s| *s == "enabled" || *s == "static" || *s == "alias") {
                Some(Check { name, level: Level::Ok, detail: format!("video memory kept across suspend (NVreg_PreserveVideoMemoryAllocations=1 to {path}; nvidia-suspend and nvidia-resume enabled)") })
            } else {
                Some(Check { name, level: Level::Warn, detail: format!("NVreg_PreserveVideoMemoryAllocations=1, but nvidia-suspend.service / nvidia-resume.service are not both enabled ({}): the memory isn't saved; {fix}", states.join(", ")) })
            }
        }
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
    /// `DRIVER=` from `device/uevent` (the PCI driver: `nvidia` for
    /// nvidia-drm's card, `i915`, `amdgpu`, `vmwgfx`).
    pub driver: Option<String>,
    /// `PCI_ID=` from `device/uevent`, lower case (`10de:2684`).
    pub pci_id: Option<String>,
    /// `device/boot_vga` = 1: the GPU the firmware booted on, udev's
    /// primary.
    pub boot_vga: bool,
    /// `device/drm/renderD*`: the card's own render node.
    pub render_node: Option<String>,
    /// Connectors whose `status` is `connected`.
    pub connected: Vec<String>,
}

/// Reads `sys` (`/sys/class/drm`): every `cardN` with its driver, PCI id,
/// boot_vga flag, render node and connected connectors
/// (`cardN-<connector>/status`).
pub fn read_drm_cards(sys: &Path) -> Vec<DrmCard> {
    let Ok(entries) = std::fs::read_dir(sys) else { return Vec::new() };
    let names: Vec<String> = entries.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    let mut cards: Vec<DrmCard> = names
        .iter()
        .filter(|n| n.strip_prefix("card").is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())))
        .map(|card| {
            let device = sys.join(card).join("device");
            let uevent = std::fs::read_to_string(device.join("uevent")).unwrap_or_default();
            let driver = uevent.lines().find_map(|l| l.strip_prefix("DRIVER=")).map(|d| d.trim().to_string());
            let pci_id = uevent.lines().find_map(|l| l.strip_prefix("PCI_ID=")).map(|d| d.trim().to_lowercase());
            let boot_vga = std::fs::read_to_string(device.join("boot_vga")).is_ok_and(|v| v.trim() == "1");
            let mut render_nodes: Vec<String> = std::fs::read_dir(device.join("drm"))
                .map(|d| d.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with("renderD")).collect())
                .unwrap_or_default();
            render_nodes.sort();
            let prefix = format!("{card}-");
            let mut connected: Vec<String> = names
                .iter()
                .filter_map(|n| n.strip_prefix(&prefix))
                .filter(|c| std::fs::read_to_string(sys.join(format!("{prefix}{c}")).join("status")).is_ok_and(|s| s.trim() == "connected"))
                .map(str::to_string)
                .collect();
            connected.sort();
            DrmCard { name: card.clone(), driver, pci_id, boot_vga, render_node: render_nodes.into_iter().next(), connected }
        })
        .collect();
    cards.sort_by(|a, b| a.name.cmp(&b.name));
    cards
}

/// The card the session renders on and drives its displays from: the boot
/// GPU when a display is on it, else any card with a display (a desktop
/// whose monitors hang off the discrete card while the firmware booted on
/// the iGPU: rendering on the iGPU and copying every frame across would be
/// the slow path), else the boot GPU, else the first card.
pub fn primary_card(cards: &[DrmCard]) -> Option<&DrmCard> {
    cards
        .iter()
        .find(|c| c.boot_vga && !c.connected.is_empty())
        .or_else(|| cards.iter().find(|c| !c.connected.is_empty()))
        .or_else(|| cards.iter().find(|c| c.boot_vga))
        .or_else(|| cards.first())
}

/// Firmware framebuffers: KMS works on them, nothing accelerates.
const FRAMEBUFFER_DRIVERS: [&str; 3] = ["simpledrm", "efifb", "vesafb"];

/// Drivers of real GPUs: software rendering on one of them means its GL
/// stack is broken or missing, never what the person wants.
pub const GPU_DRIVERS: &[&str] = &["nvidia", "nvidia-drm", "amdgpu", "radeon", "i915", "xe", "nouveau", "msm", "panfrost", "panthor", "v3d", "vc4", "tegra", "etnaviv", "lima"];

pub fn is_gpu_driver(driver: &str) -> bool {
    GPU_DRIVERS.contains(&driver)
}

/// Software rendering turned up on `driver`'s card (Mesa's llvmpipe EGL
/// device). `Ok(note)` = run with it (a VM, a firmware framebuffer, or safe
/// mode on anything); `Err(reason)` = refuse: a real GPU whose GL stack is
/// broken must not become a silent 2 fps desktop (#EDE-59), the session
/// shows the reason and the safe-mode entry still gets in.
pub fn software_rendering(driver: &str, safe_mode: bool) -> Result<String, String> {
    if !is_gpu_driver(driver) {
        return Ok(format!("software EGL device (llvmpipe) on {driver}: expected on a virtual GPU or a firmware framebuffer"));
    }
    if safe_mode {
        return Ok(format!("safe mode: rendering in software on {driver} (its OpenGL stack doesn't work; run `exponential-compositor doctor`)"));
    }
    let hint = if driver.contains("nvidia") { " (the driver's GBM backend nvidia-drm_gbm.so or its EGL GBM platform is missing)" } else { "" };
    Err(format!(
        "{driver} renders in software only (Mesa's llvmpipe): the GPU's OpenGL/EGL stack is broken or missing{hint}; Exponential Desktop refuses to run a real GPU in software. Run `exponential-compositor doctor`; the safe-mode session runs in software anyway"
    ))
}

/// Every display the real session can drive comes from a KMS driver; the
/// known virtual ones get a word on what to expect, and the card the
/// session renders on is marked.
pub fn check_drm(cards: &[DrmCard]) -> Check {
    let name = "DRM";
    if cards.is_empty() {
        return Check { name, level: Level::Fail, detail: "no /dev/dri/card*: no KMS driver loaded, Exponential Desktop can't drive a display".into() };
    }
    let primary = primary_card(cards).map(|c| c.name.clone());
    let mut level = Level::Ok;
    let parts: Vec<String> = cards
        .iter()
        .map(|c| {
            let driver = c.driver.as_deref().unwrap_or("unknown driver");
            let note = match driver {
                "vmwgfx" => " (VMware SVGA / VirtualBox VMSVGA: software rendering unless the VM has 3D acceleration)",
                "vboxvideo" => " (VirtualBox VBoxVGA/VBoxSVGA: software rendering; VMSVGA is the supported controller)",
                "virtio_gpu" | "virtio-pci" => " (virtio-gpu)",
                "nvidia" => " (nvidia-drm)",
                d if FRAMEBUFFER_DRIVERS.contains(&d) => " (firmware framebuffer, no GPU driver: works, slowly)",
                _ => "",
            };
            let marks = match (primary.as_deref() == Some(&c.name), c.boot_vga, cards.len() > 1) {
                (true, true, true) => ", boot GPU, renders",
                (true, false, true) => ", renders",
                (false, true, true) => ", boot GPU",
                _ => "",
            };
            let connectors = if c.connected.is_empty() { "nothing connected".to_string() } else { c.connected.join(", ") };
            format!("{} {driver}{note}{marks}: {connectors}", c.name)
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

/// What the kernel's KMS offers on one card, read without DRM master
/// (capabilities and plane properties are open to every client), so the
/// doctor sees it from inside another desktop.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KmsProbe {
    /// `drmGetVersion`'s name: `nvidia-drm`, `amdgpu`, `i915`, `vmwgfx`.
    pub driver: String,
    /// DRM_CLIENT_CAP_ATOMIC was taken: atomic modesetting (planes, a
    /// cursor plane, direct scanout). Off = legacy KMS.
    pub atomic: bool,
    /// DRM_CAP_CURSOR_WIDTH × HEIGHT.
    pub cursor_size: Option<(u64, u64)>,
    pub primary_planes: usize,
    pub overlay_planes: usize,
    pub cursor_planes: usize,
    /// DRM_CAP_ADDFB2_MODIFIERS: framebuffers with format modifiers
    /// (tiled GPU buffers on the plane; NVIDIA's block-linear layouts).
    pub modifiers: bool,
    /// DRM_CAP_SYNCOBJ and DRM_CAP_SYNCOBJ_TIMELINE: explicit sync
    /// (linux-drm-syncobj-v1 for Wayland clients and Xwayland).
    pub syncobj: bool,
    pub syncobj_timeline: bool,
    /// A primary plane has IN_FENCE_FD: the kernel waits for the
    /// renderer's fence before scanning out; without it the compositor
    /// waits itself (vmwgfx, virtio-gpu).
    pub in_fence_fd: bool,
}

impl KmsProbe {
    /// One line for the log and the doctor.
    pub fn describe(&self) -> String {
        let mut parts = vec![if self.atomic { "atomic".to_string() } else { "legacy KMS".to_string() }];
        match (self.cursor_planes, self.cursor_size) {
            (0, _) => parts.push("no cursor plane".into()),
            (n, Some((w, h))) => parts.push(format!("{n} cursor plane{} {w}x{h}", if n == 1 { "" } else { "s" })),
            (n, None) => parts.push(format!("{n} cursor plane{}", if n == 1 { "" } else { "s" })),
        }
        parts.push(format!("{} primary, {} overlay", self.primary_planes, self.overlay_planes));
        parts.push(if self.modifiers { "modifiers".into() } else { "no modifiers".into() });
        parts.push(match (self.syncobj, self.syncobj_timeline) {
            (true, true) => "syncobj timeline".into(),
            (true, false) => "syncobj without timeline".into(),
            _ => "no syncobj".into(),
        });
        parts.push(if self.in_fence_fd { "IN_FENCE_FD".into() } else { "no IN_FENCE_FD".into() });
        format!("{}: {}", self.driver, parts.join(", "))
    }

    /// Reads the probe from an open DRM node (a card, no master needed).
    #[cfg(feature = "udev")]
    pub fn read(fd: &smithay::backend::drm::DrmDeviceFd) -> Result<KmsProbe, String> {
        use smithay::reexports::drm::{
            control::{plane, property, Device as _, PlaneType},
            ClientCapability, Device as _, DriverCapability,
        };
        let driver = fd.get_driver().map(|d| d.name().to_string_lossy().into_owned()).map_err(|e| format!("drmGetVersion: {e}"))?;
        let atomic = fd.set_client_capability(ClientCapability::Atomic, true).is_ok();
        let _ = fd.set_client_capability(ClientCapability::UniversalPlanes, true);
        // What the session takes too: a virtual GPU shows its cursor plane
        // only to clients that set the hotspot (udev.rs); real GPUs refuse
        // the cap and keep theirs.
        let _ = fd.set_client_capability(ClientCapability::CursorPlaneHotspot, true);
        let cap = |c: DriverCapability| fd.get_driver_capability(c).ok();
        let cursor_size = cap(DriverCapability::CursorWidth).zip(cap(DriverCapability::CursorHeight));
        let mut probe = KmsProbe {
            driver,
            atomic,
            cursor_size,
            modifiers: cap(DriverCapability::AddFB2Modifiers) == Some(1),
            syncobj: cap(DriverCapability::SyncObj) == Some(1),
            syncobj_timeline: cap(DriverCapability::TimelineSyncObj) == Some(1),
            ..KmsProbe::default()
        };
        let planes: Vec<plane::Handle> = fd.plane_handles().unwrap_or_default();
        for handle in planes {
            let Ok(props) = fd.get_properties(handle) else { continue };
            let mut kind = PlaneType::Overlay;
            for (prop, value) in props.iter() {
                let Ok(info) = fd.get_property(*prop) else { continue };
                match info.name().to_bytes() {
                    b"type" => {
                        if let property::Value::Enum(Some(e)) = info.value_type().convert_value(*value) {
                            kind = match e.name().to_bytes() {
                                b"Primary" => PlaneType::Primary,
                                b"Cursor" => PlaneType::Cursor,
                                _ => PlaneType::Overlay,
                            };
                        }
                    }
                    b"IN_FENCE_FD" => probe.in_fence_fd = true,
                    _ => {}
                }
            }
            match kind {
                PlaneType::Primary => probe.primary_planes += 1,
                PlaneType::Cursor => probe.cursor_planes += 1,
                PlaneType::Overlay => probe.overlay_planes += 1,
            }
        }
        Ok(probe)
    }
}

/// Opens `card` read-write without taking master and reads its [`KmsProbe`].
#[cfg(feature = "udev")]
pub fn probe_kms(card: &Path) -> Result<KmsProbe, String> {
    use smithay::{backend::drm::DrmDeviceFd, utils::DeviceFd};
    use std::os::fd::OwnedFd;
    let file = std::fs::OpenOptions::new().read(true).write(true).open(card).map_err(|e| format!("{}: {e}", card.display()))?;
    KmsProbe::read(&DrmDeviceFd::new(DeviceFd::from(OwnedFd::from(file))))
}

/// What the KMS of the card the session drives offers. `None` = not probed
/// (no udev backend in this binary, or no card).
pub fn check_kms(probe: Option<Result<KmsProbe, String>>) -> Option<Check> {
    let name = "KMS";
    let probe = match probe? {
        Ok(p) => p,
        Err(e) => return Some(Check { name, level: Level::Warn, detail: format!("the card could not be read: {e}") }),
    };
    let nvidia = probe.driver.contains("nvidia");
    let mut level = Level::Ok;
    let mut notes: Vec<String> = Vec::new();
    if !probe.atomic {
        if nvidia {
            level = Level::Fail;
            notes.push("no atomic modesetting: nvidia-drm.modeset=1 is not in effect, the displays can't be driven".into());
        } else {
            level = Level::Warn;
            notes.push("legacy KMS only: no cursor plane, no direct scanout".into());
        }
    } else if probe.cursor_planes == 0 {
        level = Level::Warn;
        notes.push("no cursor plane: the cursor is drawn into every frame".into());
    }
    if !probe.modifiers {
        level = Level::Warn;
        notes.push("no framebuffer modifiers: linear buffers only, no direct scanout of tiled buffers".into());
    }
    if !probe.syncobj_timeline {
        if nvidia {
            level = Level::Warn;
            notes.push("no syncobj timeline: explicit sync stays off, Vulkan apps and X11 games may flicker".into());
        } else {
            notes.push("no syncobj timeline: explicit sync off".into());
        }
    }
    if !probe.in_fence_fd {
        notes.push("no IN_FENCE_FD: frames wait for the renderer before the flip".into());
    }
    if nvidia && probe.overlay_planes > 0 {
        notes.push(format!("{} overlay planes, unused on NVIDIA", probe.overlay_planes));
    }
    let mut detail = probe.describe();
    if !notes.is_empty() {
        detail.push_str(" (");
        detail.push_str(&notes.join("; "));
        detail.push(')');
    }
    Some(Check { name, level, detail })
}

/// What EGL and GLES say about a render node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RendererProbe {
    pub renderer: String,
    pub version: String,
    /// The render node EGL's device names; `None` = a software device
    /// without one (Mesa on vmwgfx without 3D, a firmware framebuffer),
    /// where the session renders through the card's own node (#EDE-40).
    pub egl_node: Option<String>,
}

/// What the real session renders with: EGL + GLES on `render_node`, the
/// same way the udev backend opens it (no seat needed, so it runs from
/// inside another desktop). The GL renderer and version strings.
#[cfg(feature = "udev")]
pub fn probe_renderer(render_node: &Path) -> Result<RendererProbe, String> {
    use smithay::{
        backend::{
            allocator::gbm::GbmDevice,
            drm::DrmDeviceFd,
            egl::{EGLContext, EGLDevice, EGLDisplay},
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
    let egl_node = EGLDevice::device_for_display(&display).ok().and_then(|d| d.try_get_render_node().ok().flatten()).map(|n| n.to_string());
    let context = EGLContext::new(&display).map_err(|e| format!("EGL context: {e}"))?;
    let mut renderer = unsafe { GlesRenderer::new(context) }.map_err(|e| format!("GLES: {e}"))?;
    let (renderer, version) = renderer.with_context(|gl| unsafe { (gl_string(gl, ffi::RENDERER), gl_string(gl, ffi::VERSION)) }).map_err(|e| format!("GL: {e}"))?;
    Ok(RendererProbe { renderer, version, egl_node })
}

/// Software renderers Mesa falls back to (a VM without 3D, no GPU driver).
const SOFTWARE_RENDERERS: [&str; 3] = ["llvmpipe", "softpipe", "swrast"];

/// `None` = this binary has no udev backend, so nothing was probed.
/// `gpu_driver` = the probed node belongs to a real GPU's driver, where
/// software rendering is a failure, not a VM's normal state.
pub fn check_renderer(probe: Option<Result<RendererProbe, String>>, gpu_driver: Option<&str>) -> Check {
    let name = "render";
    match probe {
        None => Check { name, level: Level::Warn, detail: "built without the udev backend: nested and headless only (cargo build --features udev)".into() },
        Some(Err(e)) => {
            let hint = if gpu_driver.is_some_and(|d| d.contains("nvidia")) { " (on NVIDIA: the GBM backend or the EGL GBM platform, see the NVIDIA GBM row)" } else { "" };
            Check { name, level: Level::Fail, detail: format!("EGL/GLES on the render node failed: {e}; the real session can't draw{hint}") }
        }
        Some(Ok(RendererProbe { renderer, version, egl_node })) => {
            let software = SOFTWARE_RENDERERS.iter().any(|s| renderer.contains(s));
            let device = match egl_node {
                Some(node) => format!(", EGL device {node}"),
                None => ", EGL device names no render node (the session renders through the card's own)".into(),
            };
            match (software, gpu_driver) {
                (true, Some(driver)) if is_gpu_driver(driver) => Check {
                    name,
                    level: Level::Fail,
                    detail: format!("{renderer}, {version}{device}: software rendering on {driver}, a real GPU; the session refuses to start like this (safe mode runs in software; the GL stack of {driver} is broken or missing)"),
                },
                (true, _) => Check { name, level: Level::Ok, detail: format!("{renderer}, {version} (software rendering: expected in a VM, slow on a real GPU){device}") },
                (false, _) => Check { name, level: Level::Ok, detail: format!("{renderer}, {version}{device}") },
            }
        }
    }
}

/// One GPU as `vulkaninfo --summary` lists it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VulkanGpu {
    pub index: u32,
    pub name: String,
    /// `DISCRETE_GPU`, `INTEGRATED_GPU`, `CPU` (lavapipe), `VIRTUAL_GPU`.
    pub kind: String,
    /// `10de:2684`, as a DRM card's `PCI_ID`.
    pub pci_id: String,
    /// `driverName driverInfo`: `NVIDIA 595.99.02`, `Intel open-source Mesa driver Mesa 25.2.8`.
    pub driver: String,
}

/// Parses `vulkaninfo --summary`'s `Devices:` section: `GPUn:` headers
/// followed by `key = value` lines. The loader's order, which is the order
/// a Vulkan app without a device filter sees (GPU0 = what gpui's blade
/// takes for the shell).
pub fn parse_vulkaninfo(summary: &str) -> Vec<VulkanGpu> {
    let mut gpus: Vec<VulkanGpu> = Vec::new();
    let (mut vendor, mut device) = (String::new(), String::new());
    let (mut driver_name, mut driver_info) = (String::new(), String::new());
    let finish = |gpu: &mut VulkanGpu, vendor: &mut String, device: &mut String, driver_name: &mut String, driver_info: &mut String| {
        gpu.pci_id = format!("{}:{}", vendor.trim_start_matches("0x"), device.trim_start_matches("0x"));
        gpu.driver = format!("{driver_name} {driver_info}").trim().to_string();
        vendor.clear();
        device.clear();
        driver_name.clear();
        driver_info.clear();
    };
    for line in summary.lines() {
        let trimmed = line.trim();
        if let Some(index) = trimmed.strip_prefix("GPU").and_then(|r| r.strip_suffix(':')).and_then(|n| n.parse::<u32>().ok()) {
            if let Some(last) = gpus.last_mut() {
                finish(last, &mut vendor, &mut device, &mut driver_name, &mut driver_info);
            }
            gpus.push(VulkanGpu { index, ..VulkanGpu::default() });
            continue;
        }
        let Some(gpu) = gpus.last_mut() else { continue };
        let Some((key, value)) = trimmed.split_once('=') else { continue };
        let (key, value) = (key.trim(), value.trim());
        match key {
            "deviceName" => gpu.name = value.to_string(),
            "deviceType" => gpu.kind = value.trim_start_matches("PHYSICAL_DEVICE_TYPE_").to_string(),
            "vendorID" => vendor = value.to_lowercase(),
            "deviceID" => device = value.to_lowercase(),
            "driverName" => driver_name = value.to_string(),
            "driverInfo" => driver_info = value.to_string(),
            _ => {}
        }
    }
    if let Some(last) = gpus.last_mut() {
        finish(last, &mut vendor, &mut device, &mut driver_name, &mut driver_info);
    }
    gpus
}

/// Where the Vulkan loader finds drivers (ICD manifests).
pub const VULKAN_ICD_DIRS: &[&str] = &["/usr/share/vulkan/icd.d", "/etc/vulkan/icd.d", "/usr/local/share/vulkan/icd.d", "/usr/local/etc/vulkan/icd.d"];

/// The ICD manifest names under `root`.
pub fn read_vulkan_icds(root: &Path) -> Vec<String> {
    let mut icds: Vec<String> = VULKAN_ICD_DIRS
        .iter()
        .filter_map(|dir| std::fs::read_dir(under(root, dir)).ok())
        .flat_map(|entries| entries.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()))
        .filter(|n| n.ends_with(".json"))
        .collect();
    icds.sort();
    icds.dedup();
    icds
}

/// The GPU the shell draws on: `exponential-shell` is gpui on Vulkan
/// (blade), which takes the loader's first device. `summary` =
/// `vulkaninfo --summary` (None = not installed), `icds` = the manifests
/// found, `compositor` = the PCI id of the card the compositor renders on
/// and whether that card is a real GPU.
pub fn check_vulkan(summary: Option<&str>, icds: &[String], compositor: Option<(&str, bool)>) -> Check {
    let name = "Vulkan";
    let (compositor_pci, real_gpu) = compositor.map(|(p, r)| (Some(p), r)).unwrap_or((None, false));
    let Some(summary) = summary else {
        if icds.is_empty() {
            return Check { name, level: Level::Fail, detail: "no Vulkan driver (no ICD manifest in /usr/share/vulkan/icd.d): the shell can't draw; install mesa-vulkan-drivers, or NVIDIA's driver brings its own".into() };
        }
        let software_only = icds.iter().all(|i| i.starts_with("lvp") || i.contains("lavapipe"));
        return if software_only {
            Check { name, level: if real_gpu { Level::Fail } else { Level::Warn }, detail: format!("only lavapipe (software Vulkan) is installed ({}): the shell would draw on the CPU", icds.join(", ")) }
        } else {
            Check { name, level: Level::Warn, detail: format!("drivers installed ({}); install vulkan-tools to see which GPU the shell takes", icds.join(", ")) }
        };
    };
    let gpus = parse_vulkaninfo(summary);
    let Some(first) = gpus.first() else {
        return Check { name, level: Level::Fail, detail: "vulkaninfo lists no GPU: the shell can't draw".into() };
    };
    let others: Vec<String> = gpus.iter().skip(1).map(|g| format!("GPU{} {} ({})", g.index, g.name, g.kind)).collect();
    let rest = if others.is_empty() { String::new() } else { format!("; then {}", others.join(", ")) };
    let head = format!("GPU0 {} ({}, {}, {})", first.name, first.pci_id, first.kind, first.driver);
    if first.kind == "CPU" {
        let on_hardware = gpus.iter().any(|g| g.kind != "CPU");
        let hint = if on_hardware { format!("; a GPU is listed after it: put it first with MESA_VK_DEVICE_SELECT={}!", gpus.iter().find(|g| g.kind != "CPU").map(|g| g.pci_id.as_str()).unwrap_or_default()) } else { String::new() };
        return Check { name, level: if real_gpu { Level::Fail } else { Level::Warn }, detail: format!("{head}: the shell would draw in software (lavapipe){hint}{rest}") };
    }
    match compositor_pci {
        Some(pci) if pci == first.pci_id => Check { name, level: Level::Ok, detail: format!("{head}: the shell draws on the compositor's GPU{rest}") },
        Some(pci) => Check {
            name,
            level: Level::Warn,
            detail: format!("{head} is not the compositor's GPU ({pci}): the shell's frames would cross GPUs; put the compositor's first with MESA_VK_DEVICE_SELECT={pci}!{rest}"),
        },
        None => Check { name, level: Level::Ok, detail: format!("{head}{rest}") },
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
            detail: format!("no layout in {}: Exponential Desktop starts with `us` unless ~/.config/exponential-desktop/compositor.toml sets [keyboard] layout", crate::config::SYSTEM_KEYBOARD),
        },
    }
}

/// Runs `program args` and returns stdout + stderr, `None` when it can't run.
fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let o = Command::new(program).args(args).output().ok()?;
    let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
    s.push_str(&String::from_utf8_lossy(&o.stderr));
    Some(s)
}

/// Reads the system and runs every check.
pub fn run_checks() -> Vec<Check> {
    let root = Path::new("/");
    let path = xwayland_search_path(std::env::var_os("PATH").as_deref(), Path::new(BUNDLED_XWAYLAND_DIR));
    let binary = xwayland_binary(&path);
    let xwayland = binary.as_ref().and_then(|b| command_output(&b.to_string_lossy(), &["-version"]));
    let read = |p: &str| std::fs::read_to_string(p).ok();
    let mut nodes: Vec<String> = std::fs::read_dir(Path::new("/dev/dri"))
        .map(|d| d.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.starts_with("renderD")).collect())
        .unwrap_or_default();
    nodes.sort();
    let seat = Seat { logind_seat0: read("/run/systemd/seats/seat0"), seatd: Path::new("/run/seatd.sock").exists() };
    let cards = read_drm_cards(Path::new("/sys/class/drm"));
    let primary = primary_card(&cards);
    let primary_driver = primary.and_then(|c| c.driver.as_deref());
    // The node the session renders on: the primary card's own render node,
    // else the first one there is.
    let render_node = primary.and_then(|c| c.render_node.clone()).or_else(|| nodes.first().cloned());
    #[cfg(feature = "udev")]
    let probe = render_node.as_ref().map(|n| probe_renderer(&Path::new("/dev/dri").join(n)));
    #[cfg(not(feature = "udev"))]
    let probe: Option<Result<RendererProbe, String>> = {
        let _ = &render_node;
        None
    };
    #[cfg(feature = "udev")]
    let kms = primary.map(|c| probe_kms(&Path::new("/dev/dri").join(&c.name)));
    #[cfg(not(feature = "udev"))]
    let kms: Option<Result<KmsProbe, String>> = None;
    let vulkan = command_output("vulkaninfo", &["--summary"]);
    let icds = read_vulkan_icds(root);
    let compositor_gpu = primary.and_then(|c| Some((c.pci_id.as_deref()?, c.driver.as_deref().is_some_and(is_gpu_driver))));
    let system_keyboard = read(crate::config::SYSTEM_KEYBOARD).and_then(|t| crate::config::parse_system_keyboard(&t));
    let mut checks = vec![check_render_nodes(&nodes), check_drm(&cards)];
    checks.extend(check_kms(kms));
    checks.extend([
        check_seat(&seat),
        check_renderer(probe, primary_driver),
        check_vulkan(vulkan.as_deref(), &icds, compositor_gpu),
        check_keyboard(system_keyboard.as_ref()),
        check_xwayland(binary.as_deref(), xwayland.as_deref()),
    ]);
    let nvidia = Nvidia::read(root).map(|mut nv| {
        nv.suspend_units = command_output("systemctl", &["is-enabled", "nvidia-suspend.service", "nvidia-resume.service"]);
        nv
    });
    checks.extend(check_nvidia(nvidia.as_ref()));
    checks.extend(check_nvidia_stack(nvidia.as_ref()));
    checks.extend(check_nvidia_suspend(nvidia.as_ref()));
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
        println!("{mark}  {:<12} {}", c.name, c.detail);
    }
    !checks.iter().any(|c| c.level == Level::Fail)
}

/// The checks as JSON for Settings: `[{name, level: ok|warn|fail, detail}]`.
pub fn to_json(checks: &[Check]) -> String {
    let rows: Vec<serde_json::Value> = checks
        .iter()
        .map(|c| {
            let level = match c.level {
                Level::Ok => "ok",
                Level::Warn => "warn",
                Level::Fail => "fail",
            };
            serde_json::json!({"name": c.name, "level": level, "detail": c.detail})
        })
        .collect();
    serde_json::Value::Array(rows).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a Mint 22.3 box with an RTX 4090 (driver 595.99.02, open kernel
    /// module) next to an Intel iGPU showed on 2026-11-07, as files.
    fn mint() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/doctor/mint-rtx4090")
    }

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(mint().join(name)).unwrap()
    }

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
        let check = check_xwayland(Some(Path::new("/usr/lib/exponential-desktop/bin/Xwayland")), Some("Xwayland Version 24.1.14"));
        assert_eq!(check.detail, "/usr/lib/exponential-desktop/bin/Xwayland 24.1.14, explicit sync");
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

    fn nv(version: Option<&str>, modeset: Option<&str>, fbdev: Option<&str>, cmdline: &str) -> Nvidia {
        Nvidia {
            version: version.map(Into::into),
            modeset_param: modeset.map(Into::into),
            fbdev_param: fbdev.map(Into::into),
            cmdline: cmdline.into(),
            gbm_backend: true,
            egl_gbm_platform: true,
            ..Nvidia::default()
        }
    }

    #[test]
    fn nvidia_needs_modeset_and_wants_fbdev() {
        let level = |n: Nvidia| check_nvidia(Some(&n)).unwrap().level;
        assert_eq!(check_nvidia(None), None, "no NVIDIA driver, no check");
        // readable parameters win
        assert_eq!(level(nv(Some("595.99.02"), Some("N\n"), None, "")), Level::Fail);
        assert_eq!(level(nv(None, Some("Y\n"), Some("N\n"), "")), Level::Warn);
        assert_eq!(level(nv(None, Some("Y\n"), Some("Y\n"), "")), Level::Ok);
        // root-only parameters: the command line, then the driver default
        assert_eq!(level(nv(Some("560.1"), None, None, "quiet nvidia-drm.modeset=1 nvidia_drm.fbdev=1")), Level::Ok);
        assert_eq!(level(nv(Some("595.99.02"), None, None, "quiet splash")), Level::Ok, "560+ defaults to modeset + fbdev");
        assert_eq!(level(nv(Some("550.1"), None, None, "quiet splash")), Level::Fail, "older drivers default to off");
        assert_eq!(level(nv(Some("595.99.02"), None, None, "nvidia-drm.modeset=0")), Level::Fail);
        assert_eq!(level(nv(None, None, None, "")), Level::Warn, "unknown");
    }

    #[test]
    fn modeset_from_modprobe_d_names_its_file() {
        let mut n = nv(Some("595.99.02"), None, None, "quiet splash");
        n.modprobe = vec![
            ("/etc/modprobe.d/nvidia-graphics-drivers-kms.conf".into(), "# comment\noptions nvidia_drm modeset=1\noptions nvidia NVreg_PreserveVideoMemoryAllocations=1\n".into()),
            ("/lib/modprobe.d/nvidia-graphics-drivers.conf".into(), "blacklist nouveau\noptions nvidia-drm modeset=1\n".into()),
        ];
        assert_eq!(nvidia_flag(&n, "modeset"), Some(Flag { on: true, source: "/etc/modprobe.d/nvidia-graphics-drivers-kms.conf".into() }));
        assert_eq!(nvidia_flag(&n, "fbdev"), Some(Flag { on: true, source: "the driver's default from 560".into() }), "nothing sets fbdev: the default");
        n.modprobe[0].1 = "options nvidia-drm modeset=0 fbdev=0\n".into();
        assert_eq!(nvidia_flag(&n, "modeset").unwrap().on, false);
        assert_eq!(nvidia_flag(&n, "fbdev"), Some(Flag { on: false, source: "/etc/modprobe.d/nvidia-graphics-drivers-kms.conf".into() }));
        let check = check_nvidia(Some(&n)).unwrap();
        assert_eq!(check.level, Level::Fail);
        assert!(check.detail.contains("nvidia-drm.modeset is off (/etc/modprobe.d/nvidia-graphics-drivers-kms.conf)"), "{}", check.detail);
        // the command line beats modprobe.d
        n.cmdline = "nvidia-drm.modeset=1".into();
        assert_eq!(nvidia_flag(&n, "modeset"), Some(Flag { on: true, source: "the kernel command line".into() }));
    }

    #[test]
    fn drivers_below_the_minimum_fail_and_below_explicit_sync_warn() {
        let level = |n: Nvidia| check_nvidia(Some(&n)).unwrap().level;
        let too_old = check_nvidia(Some(&nv(Some("470.256.02"), Some("Y\n"), Some("Y\n"), ""))).unwrap();
        assert_eq!(too_old.level, Level::Fail);
        assert!(too_old.detail.contains("needs 525 or newer"), "{}", too_old.detail);
        let no_sync = check_nvidia(Some(&nv(Some("535.183.01"), Some("Y\n"), Some("Y\n"), ""))).unwrap();
        assert_eq!(no_sync.level, Level::Warn);
        assert!(no_sync.detail.contains("no explicit sync before driver 555"), "{}", no_sync.detail);
        assert_eq!(level(nv(Some("555.42.02"), Some("Y\n"), Some("Y\n"), "")), Level::Ok);
    }

    #[test]
    fn the_kernel_module_is_read_from_the_nvrm_banner() {
        let open = Nvidia { proc_version: Some("NVRM version: NVIDIA UNIX Open Kernel Module for x86_64  595.99.02  Release Build\nGCC version: gcc\n".into()), ..Nvidia::default() };
        let closed = Nvidia { proc_version: Some("NVRM version: NVIDIA UNIX x86_64 Kernel Module  550.120  Fri Sep 13 10:10:01 UTC 2024\n".into()), ..Nvidia::default() };
        assert_eq!(open.kernel_module(), Some(KernelModule::Open));
        assert_eq!(closed.kernel_module(), Some(KernelModule::Proprietary));
        assert_eq!(Nvidia::default().kernel_module(), None);
        let mut n = nv(Some("595.99.02"), Some("Y\n"), Some("Y\n"), "");
        n.proc_version = open.proc_version.clone();
        assert!(check_nvidia(Some(&n)).unwrap().detail.starts_with("driver 595.99.02, open kernel module; modeset on (the module parameter), fbdev on (the module parameter)"), "{}", check_nvidia(Some(&n)).unwrap().detail);
    }

    #[test]
    fn nvidia_needs_its_gbm_backend_and_the_egl_gbm_platform() {
        let stack = |gbm_backend: bool, egl_gbm_platform: bool| check_nvidia_stack(Some(&Nvidia { version: Some("595.99.02".into()), gbm_backend, egl_gbm_platform, ..Nvidia::default() })).unwrap();
        assert_eq!(check_nvidia_stack(None), None);
        assert_eq!(stack(false, true).level, Level::Fail);
        assert!(stack(false, true).detail.contains("nvidia-drm_gbm.so"));
        assert_eq!(stack(true, false).level, Level::Fail);
        assert!(stack(true, false).detail.contains("15_nvidia_gbm.json"));
        assert_eq!(stack(true, true).level, Level::Ok);
    }

    #[test]
    fn video_memory_across_suspend_needs_the_parameter_and_the_units() {
        let sleep = |params: Option<&str>, units: Option<&str>| check_nvidia_suspend(Some(&Nvidia { params: params.map(Into::into), suspend_units: units.map(Into::into), ..Nvidia::default() })).unwrap();
        assert_eq!(check_nvidia_suspend(None), None);
        assert_eq!(sleep(None, None).level, Level::Warn);
        let off = sleep(Some("PreserveVideoMemoryAllocations: 0\nTemporaryFilePath: \"\"\n"), Some("enabled\nenabled\n"));
        assert_eq!(off.level, Level::Warn);
        assert!(off.detail.contains("lost on suspend") && off.detail.contains("nvidia-power-management.conf"), "{}", off.detail);
        let no_units = sleep(Some("PreserveVideoMemoryAllocations: 1\n"), Some("disabled\nenabled\n"));
        assert_eq!(no_units.level, Level::Warn);
        assert!(no_units.detail.contains("not both enabled (disabled, enabled)"), "{}", no_units.detail);
        let ok = sleep(Some("PreserveVideoMemoryAllocations: 1\nTemporaryFilePath: \"/var\"\n"), Some("enabled\nenabled\n"));
        assert_eq!(ok.level, Level::Ok);
        assert!(ok.detail.contains("to /var"), "{}", ok.detail);
        assert_eq!(sleep(Some("PreserveVideoMemoryAllocations: 1\n"), None).level, Level::Warn, "no systemctl");
    }

    #[test]
    fn the_checks_as_json() {
        let json = to_json(&[Check { name: "Xwayland", level: Level::Warn, detail: "23.2 \"old\"".into() }]);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, serde_json::json!([{"name": "Xwayland", "level": "warn", "detail": "23.2 \"old\""}]));
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
            ("card0/device/uevent", "DRIVER=vmwgfx\nPCI_CLASS=30000\nPCI_ID=15AD:0405\n"),
            ("card0/device/boot_vga", "1\n"),
            ("card0/device/drm/renderD128/dev", "226:128\n"),
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
                DrmCard { name: "card0".into(), driver: Some("vmwgfx".into()), pci_id: Some("15ad:0405".into()), boot_vga: true, render_node: Some("renderD128".into()), connected: vec!["Virtual-1".into()] },
                DrmCard { name: "card1".into(), driver: Some("simpledrm".into()), ..DrmCard::default() },
            ]
        );
        std::fs::remove_dir_all(&sys).unwrap();
        assert_eq!(read_drm_cards(&sys), []);
    }

    #[test]
    fn drm_check_names_the_virtual_gpus_and_warns_without_a_display() {
        let vm = check_drm(&[DrmCard { name: "card0".into(), driver: Some("vmwgfx".into()), connected: vec!["Virtual-1".into()], ..DrmCard::default() }]);
        assert_eq!(vm.level, Level::Ok);
        assert!(vm.detail.contains("card0 vmwgfx") && vm.detail.contains("VirtualBox VMSVGA") && vm.detail.ends_with("Virtual-1"), "{}", vm.detail);
        let fb = check_drm(&[DrmCard { name: "card0".into(), driver: Some("simpledrm".into()), connected: vec!["Unknown-1".into()], ..DrmCard::default() }]);
        assert_eq!(fb.level, Level::Warn, "a firmware framebuffer only");
        let dark = check_drm(&[DrmCard { name: "card0".into(), driver: Some("amdgpu".into()), ..DrmCard::default() }]);
        assert_eq!(dark.level, Level::Warn);
        assert!(dark.detail.contains("no display connected"));
        assert_eq!(check_drm(&[]).level, Level::Fail);
    }

    #[test]
    fn the_primary_card_is_the_one_with_the_displays() {
        let igpu = DrmCard { name: "card1".into(), driver: Some("i915".into()), boot_vga: true, ..DrmCard::default() };
        let dgpu = DrmCard { name: "card2".into(), driver: Some("nvidia".into()), connected: vec!["DP-2".into(), "HDMI-A-3".into()], ..DrmCard::default() };
        // The firmware booted on the iGPU, the monitors hang off the NVIDIA card: render there.
        assert_eq!(primary_card(&[igpu.clone(), dgpu.clone()]).map(|c| c.name.as_str()), Some("card2"));
        let detail = check_drm(&[igpu.clone(), dgpu.clone()]).detail;
        assert!(detail.contains("card1 i915, boot GPU: nothing connected; card2 nvidia (nvidia-drm), renders: DP-2, HDMI-A-3"), "{detail}");
        // Both have displays: the boot GPU.
        let mut igpu_with_display = igpu.clone();
        igpu_with_display.connected = vec!["HDMI-A-1".into()];
        assert_eq!(primary_card(&[igpu_with_display, dgpu.clone()]).map(|c| c.name.as_str()), Some("card1"));
        // Nothing connected anywhere: the boot GPU, else the first.
        let mut dark = dgpu.clone();
        dark.connected.clear();
        assert_eq!(primary_card(&[dark.clone(), igpu.clone()]).map(|c| c.name.as_str()), Some("card1"));
        let mut nobody = igpu.clone();
        nobody.boot_vga = false;
        assert_eq!(primary_card(&[dark, nobody]).map(|c| c.name.as_str()), Some("card2"));
        assert_eq!(primary_card(&[]), None);
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
    fn the_renderer_check_tells_software_from_hardware_and_refuses_it_on_a_real_gpu() {
        let probe = |renderer: &str, egl_node: Option<&str>| RendererProbe { renderer: renderer.into(), version: "OpenGL ES 3.2 Mesa 26.0.8".into(), egl_node: egl_node.map(Into::into) };
        let sw = check_renderer(Some(Ok(probe("llvmpipe (LLVM 20.1.8, 256 bits)", None))), Some("vmwgfx"));
        assert_eq!(sw.level, Level::Ok);
        assert!(sw.detail.contains("software rendering") && sw.detail.contains("names no render node"), "{}", sw.detail);
        let hw = check_renderer(Some(Ok(probe("AMD Radeon RX 7800 XT", Some("renderD128")))), Some("amdgpu"));
        assert!(!hw.detail.contains("software") && hw.detail.ends_with("EGL device renderD128"), "{}", hw.detail);
        let broken = check_renderer(Some(Ok(probe("llvmpipe (LLVM 20.1.8, 256 bits)", None))), Some("nvidia"));
        assert_eq!(broken.level, Level::Fail, "llvmpipe on an RTX 4090 is a broken driver, not a VM");
        assert!(broken.detail.contains("refuses to start") && broken.detail.contains("safe mode"), "{}", broken.detail);
        let failed = check_renderer(Some(Err("EGL display: no EGL".into())), Some("nvidia"));
        assert_eq!(failed.level, Level::Fail);
        assert!(failed.detail.contains("NVIDIA GBM row"), "{}", failed.detail);
        assert_eq!(check_renderer(None, None).level, Level::Warn, "a winit-only build");
    }

    #[test]
    fn software_rendering_is_refused_on_a_real_gpu_unless_safe_mode() {
        assert!(software_rendering("vmwgfx", false).is_ok());
        assert!(software_rendering("simpledrm", false).is_ok());
        let refused = software_rendering("nvidia-drm", false).unwrap_err();
        assert!(refused.contains("nvidia-drm_gbm.so") && refused.contains("refuses"), "{refused}");
        assert!(software_rendering("amdgpu", false).is_err());
        assert!(software_rendering("nvidia-drm", true).unwrap().starts_with("safe mode"));
    }

    #[test]
    fn the_kms_check_reads_atomic_cursor_modifiers_and_explicit_sync() {
        let full = KmsProbe { driver: "nvidia-drm".into(), atomic: true, cursor_size: Some((256, 256)), primary_planes: 4, overlay_planes: 0, cursor_planes: 4, modifiers: true, syncobj: true, syncobj_timeline: true, in_fence_fd: true };
        let check = check_kms(Some(Ok(full.clone()))).unwrap();
        assert_eq!(check.level, Level::Ok);
        assert_eq!(check.detail, "nvidia-drm: atomic, 4 cursor planes 256x256, 4 primary, 0 overlay, modifiers, syncobj timeline, IN_FENCE_FD");
        let legacy = check_kms(Some(Ok(KmsProbe { atomic: false, cursor_planes: 0, ..full.clone() }))).unwrap();
        assert_eq!(legacy.level, Level::Fail, "nvidia without atomic = modeset off");
        assert!(legacy.detail.contains("nvidia-drm.modeset=1 is not in effect"), "{}", legacy.detail);
        let vm = check_kms(Some(Ok(KmsProbe { driver: "vmwgfx".into(), atomic: true, cursor_size: Some((64, 64)), primary_planes: 1, cursor_planes: 1, modifiers: false, syncobj: false, syncobj_timeline: false, in_fence_fd: false, ..KmsProbe::default() }))).unwrap();
        assert_eq!(vm.level, Level::Warn);
        assert!(vm.detail.contains("no framebuffer modifiers") && vm.detail.contains("no IN_FENCE_FD: frames wait"), "{}", vm.detail);
        let no_sync = check_kms(Some(Ok(KmsProbe { syncobj_timeline: false, ..full.clone() }))).unwrap();
        assert_eq!(no_sync.level, Level::Warn);
        assert!(no_sync.detail.contains("explicit sync stays off"), "{}", no_sync.detail);
        let no_cursor = check_kms(Some(Ok(KmsProbe { cursor_planes: 0, ..full }))).unwrap();
        assert_eq!(no_cursor.level, Level::Warn);
        assert!(no_cursor.detail.contains("no cursor plane: the cursor is drawn into every frame"), "{}", no_cursor.detail);
        assert_eq!(check_kms(Some(Err("/dev/dri/card2: Permission denied".into()))).unwrap().level, Level::Warn);
        assert_eq!(check_kms(None), None);
    }

    #[test]
    fn vulkaninfo_summary_lists_the_gpus_in_the_loaders_order() {
        let gpus = parse_vulkaninfo(&fixture("vulkaninfo-summary.txt"));
        assert_eq!(gpus.len(), 3);
        assert_eq!(gpus[0], VulkanGpu { index: 0, name: "NVIDIA GeForce RTX 4090".into(), kind: "DISCRETE_GPU".into(), pci_id: "10de:2684".into(), driver: "NVIDIA 595.99.02".into() });
        assert_eq!((gpus[1].kind.as_str(), gpus[1].pci_id.as_str()), ("INTEGRATED_GPU", "8086:a780"));
        assert_eq!((gpus[2].kind.as_str(), gpus[2].name.as_str()), ("CPU", "llvmpipe (LLVM 20.1.8, 256 bits)"));
        assert_eq!(parse_vulkaninfo("ERROR: [Loader Message] ..."), []);
    }

    #[test]
    fn the_shell_must_draw_on_the_compositors_gpu_not_on_the_cpu() {
        let summary = fixture("vulkaninfo-summary.txt");
        let ok = check_vulkan(Some(&summary), &[], Some(("10de:2684", true)));
        assert_eq!(ok.level, Level::Ok);
        assert!(ok.detail.starts_with("GPU0 NVIDIA GeForce RTX 4090 (10de:2684, DISCRETE_GPU, NVIDIA 595.99.02): the shell draws on the compositor's GPU; then GPU1"), "{}", ok.detail);
        let other = check_vulkan(Some(&summary), &[], Some(("8086:a780", true)));
        assert_eq!(other.level, Level::Warn);
        assert!(other.detail.contains("MESA_VK_DEVICE_SELECT=8086:a780!"), "{}", other.detail);
        let cpu_first = "GPU0:\n\tdeviceType = PHYSICAL_DEVICE_TYPE_CPU\n\tdeviceName = llvmpipe\n\tvendorID = 0x10005\n\tdeviceID = 0x0000\nGPU1:\n\tdeviceType = PHYSICAL_DEVICE_TYPE_DISCRETE_GPU\n\tdeviceName = NVIDIA GeForce RTX 4090\n\tvendorID = 0x10de\n\tdeviceID = 0x2684\n";
        let sw = check_vulkan(Some(cpu_first), &[], Some(("10de:2684", true)));
        assert_eq!(sw.level, Level::Fail, "lavapipe first on a real GPU: the shell would be software");
        assert!(sw.detail.contains("MESA_VK_DEVICE_SELECT=10de:2684!"), "{}", sw.detail);
        assert_eq!(check_vulkan(Some(cpu_first), &[], Some(("15ad:0405", false))).level, Level::Warn, "in a VM software Vulkan is what there is");
        // no vulkaninfo: the manifests
        assert_eq!(check_vulkan(None, &[], None).level, Level::Fail);
        assert_eq!(check_vulkan(None, &["lvp_icd.x86_64.json".into()], Some(("10de:2684", true))).level, Level::Fail);
        assert_eq!(check_vulkan(None, &["lvp_icd.x86_64.json".into()], None).level, Level::Warn);
        let unknown = check_vulkan(None, &["nvidia_icd.json".into(), "lvp_icd.x86_64.json".into()], None);
        assert_eq!(unknown.level, Level::Warn);
        assert!(unknown.detail.contains("vulkan-tools"));
    }

    #[test]
    fn the_mint_box_as_the_child_read_it() {
        let root = mint();
        let icds = read_vulkan_icds(&root);
        assert_eq!(icds, ["intel_icd.x86_64.json", "lvp_icd.x86_64.json", "nvidia_icd.json"]);
        let cards = read_drm_cards(&root.join("sys/class/drm"));
        assert_eq!(cards.len(), 2);
        let primary = primary_card(&cards).unwrap();
        assert_eq!((primary.name.as_str(), primary.driver.as_deref(), primary.pci_id.as_deref(), primary.render_node.as_deref()), ("card2", Some("nvidia"), Some("10de:2684"), Some("renderD129")));
        assert_eq!(primary.connected, ["DP-2", "HDMI-A-3"]);
        let drm = check_drm(&cards);
        assert_eq!(drm.level, Level::Ok);
        assert!(drm.detail.contains("card2 nvidia (nvidia-drm), boot GPU, renders: DP-2, HDMI-A-3"), "{}", drm.detail);

        let mut nvidia = Nvidia::read(&root).expect("nvidia_drm is loaded");
        nvidia.suspend_units = Some(fixture("systemctl-is-enabled.txt"));
        assert_eq!(nvidia.version.as_deref().map(str::trim), Some("595.99.02"));
        assert_eq!(nvidia.kernel_module(), Some(KernelModule::Open));
        assert_eq!((nvidia.modeset_param.clone(), nvidia.fbdev_param.clone()), (None, None), "the parameters are root-only (0400)");
        assert_eq!(nvidia.modprobe.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(), ["/etc/modprobe.d/nvidia-graphics-drivers-kms.conf", "/lib/modprobe.d/nvidia-graphics-drivers.conf"]);
        assert!(nvidia.gbm_backend && nvidia.egl_gbm_platform);
        assert_eq!(nvidia.param("PreserveVideoMemoryAllocations").as_deref(), Some("1"));
        assert_eq!(nvidia.param("TemporaryFilePath").as_deref(), Some("/var"));
        assert_eq!(nvidia.param("EnableGpuFirmware").as_deref(), Some("18"));

        let driver = check_nvidia(Some(&nvidia)).unwrap();
        assert_eq!(driver.level, Level::Ok, "{}", driver.detail);
        assert_eq!(driver.detail, "driver 595.99.02, open kernel module; modeset on (/etc/modprobe.d/nvidia-graphics-drivers-kms.conf), fbdev on (the driver's default from 560)");
        assert_eq!(check_nvidia_stack(Some(&nvidia)).unwrap().level, Level::Ok);
        let sleep = check_nvidia_suspend(Some(&nvidia)).unwrap();
        assert_eq!(sleep.level, Level::Ok, "{}", sleep.detail);
        let vulkan = check_vulkan(Some(&fixture("vulkaninfo-summary.txt")), &icds, Some((primary.pci_id.as_deref().unwrap(), true)));
        assert_eq!(vulkan.level, Level::Ok, "{}", vulkan.detail);
        // Mint 22.3's own Xwayland is 23.2.6: the .deb's bundled 24.1 takes over.
        let xwayland = check_xwayland(Some(Path::new("/usr/bin/Xwayland")), Some(&fixture("xwayland-version.txt")));
        assert_eq!(xwayland.level, Level::Warn);
        assert!(xwayland.detail.contains("23.2.6") && xwayland.detail.contains(BUNDLED_XWAYLAND_DIR), "{}", xwayland.detail);
        assert_eq!(check_seat(&Seat { logind_seat0: Some(fixture("run/systemd/seats/seat0")), seatd: false }).detail, "logind seat0, graphical");
        assert_eq!(Nvidia::read(Path::new("/nonexistent")), None);
    }
}
