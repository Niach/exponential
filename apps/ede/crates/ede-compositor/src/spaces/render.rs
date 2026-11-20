//! #EDE-138: what a space's apps draw with. A space in software takes
//! shared memory buffers alone (no dmabuf, no explicit sync), so a frame
//! drawn on the GPU cannot reach it; what an app does about that decides
//! whether it shows anything:
//!
//! - OpenGL and EGL fall back by themselves: Mesa's EGL finds no GPU behind
//!   the space's display and takes its software driver (llvmpipe), also
//!   where NVIDIA's EGL is asked first (it gives up on a display without
//!   its buffers). Nothing is forced for them.
//! - Vulkan has no such fallback: the loader offers every device and an
//!   app takes the one it likes best, a GPU, whose swapchain then fails
//!   (`vkcube` does not start). While a space renders in software
//!   ([`super::AgentSpace::software_rendering`]) its programs see Mesa's
//!   software Vulkan (lavapipe) alone. Without lavapipe installed a Vulkan
//!   app fails as it would without this; the doctor says so.
//!
//! The switch is a space's own and on. #EDE-162 (F7) turns it off for a
//! space that draws on the GPU (`frames::net::space_gpu_allowed`), before
//! its first launch (the space's user manager keeps the environment it
//! started with); its clients then get the spaces' dmabuf and explicit
//! sync, and its programs see the Vulkan driver of the GPU spaces render on
//! and lavapipe, nothing else ([`vulkan_environment`]). A space whose GPU
//! path fails latches, which turns the switch back on for its later
//! launches.

use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

use crate::{desk::DeskId, doctor::VULKAN_ICD_DIRS, state::Ede};

/// The compositor's to set for a space's programs (in software and on the
/// GPU), refused in a caller's `env`: `VK_DRIVER_FILES`, and
/// `VK_ICD_FILENAMES` for Vulkan loaders before 1.3.207.
pub const SET: [&str; 2] = ["VK_DRIVER_FILES", "VK_ICD_FILENAMES"];

/// lavapipe's ICD manifests under `root` (`/` but in tests): in the
/// loader's directories and each of `data_dirs` (`XDG_DATA_DIRS`), one per
/// file name (the first found), sorted. More than one = one per
/// architecture (`lvp_icd.x86_64.json`, `lvp_icd.i686.json`).
pub fn lavapipe_icds(root: &Path, data_dirs: Option<&OsStr>) -> Vec<PathBuf> {
    icds_named(root, data_dirs, "lvp_icd")
}

/// #EDE-162: the Vulkan driver of a space renderer's kernel driver: its
/// ICD manifests' name, `None` for one with no known Vulkan driver.
pub fn vulkan_icd_of(kernel_driver: &str) -> Option<&'static str> {
    match kernel_driver {
        "nvidia" | "nvidia-drm" => Some("nvidia_icd"),
        "i915" | "xe" => Some("intel_icd"),
        "amdgpu" | "radeon" => Some("radeon_icd"),
        "nouveau" => Some("nouveau_icd"),
        "virtio_gpu" | "virtio-gpu" => Some("virtio_icd"),
        _ => None,
    }
}

/// The ICD manifests whose file names start with `prefix` (as
/// [`lavapipe_icds`] finds lavapipe's).
pub fn icds_named(root: &Path, data_dirs: Option<&OsStr>, prefix: &str) -> Vec<PathBuf> {
    let under = |dir: &Path| root.join(dir.strip_prefix("/").unwrap_or(dir));
    let data: Vec<PathBuf> = std::env::split_paths(data_dirs.filter(|dirs| !dirs.is_empty()).unwrap_or(OsStr::new("/usr/local/share:/usr/share"))).map(|dir| dir.join("vulkan/icd.d")).collect();
    let mut found: Vec<PathBuf> = Vec::new();
    for dir in VULKAN_ICD_DIRS.iter().map(PathBuf::from).chain(data) {
        let Ok(entries) = std::fs::read_dir(under(&dir)) else { continue };
        let mut names: Vec<OsString> = entries.flatten().map(|entry| entry.file_name()).filter(|name| name.to_str().is_some_and(|name| name.starts_with(prefix) && name.ends_with(".json"))).collect();
        names.sort();
        for name in names {
            if !found.iter().any(|known| known.file_name() == Some(&name)) {
                found.push(dir.join(&name));
            }
        }
    }
    found.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    found
}

/// What a program of a space that renders in software starts with: the
/// Vulkan loader pointed at `icds` (lavapipe's) alone; nothing without them.
pub fn software_environment(icds: &[PathBuf]) -> Vec<(&'static str, OsString)> {
    if icds.is_empty() {
        return Vec::new();
    }
    let joined = std::env::join_paths(icds).unwrap_or_default();
    SET.iter().map(|name| (*name, joined.clone())).collect()
}

/// #EDE-162: what a space's programs see of Vulkan. In software
/// (`software`): lavapipe alone. On the GPU, `gpu` = the manifests of the
/// Vulkan driver of the GPU spaces render on: exactly those and lavapipe,
/// so an app takes the GPU its space draws on (on a machine whose displays
/// are on another GPU, not the person's) and lavapipe stays its fallback;
/// lavapipe alone where that driver is not installed, never every GPU of
/// the machine. `None` (the switch off with no GPU renderer behind it, or
/// one whose kernel driver has no known Vulkan driver): nothing is set.
pub fn vulkan_environment(software: bool, gpu: Option<&[PathBuf]>, lavapipe: &[PathBuf]) -> Vec<(&'static str, OsString)> {
    match (software, gpu) {
        (true, _) => software_environment(lavapipe),
        (false, None) => Vec::new(),
        (false, Some(gpu)) => software_environment(&gpu.iter().chain(lavapipe).cloned().collect::<Vec<_>>()),
    }
}

impl Ede {
    /// What space `desk`'s next launch sees of Vulkan ([`vulkan_environment`]).
    pub(crate) fn space_vulkan_environment(&self, software: bool) -> Vec<(&'static str, OsString)> {
        let data = std::env::var_os("XDG_DATA_DIRS");
        let lavapipe = lavapipe_icds(Path::new("/"), data.as_deref());
        let driver = self.space_gpu_driver().or(self.space_pretended_driver());
        let gpu = driver.and_then(vulkan_icd_of).map(|name| icds_named(Path::new("/"), data.as_deref(), name));
        vulkan_environment(software, gpu.as_deref(), &lavapipe)
    }

    /// Whether space `desk`'s programs draw in software from its next
    /// launch on (every space starts with `true`); #EDE-162 turns it off
    /// for a space whose GPU path is ready. `false` = no such space.
    pub fn set_space_software_rendering(&mut self, desk: DeskId, on: bool) -> bool {
        let Some(space) = self.spaces.get_mut(desk) else { return false };
        space.software_rendering = on;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lavapipe_is_found_once_per_architecture_and_set_for_both_loaders() {
        let root = tempfile::tempdir().unwrap();
        let put = |dir: &str, name: &str| {
            let dir = root.path().join(dir.trim_start_matches('/'));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(name), "{}").unwrap();
        };
        put("/usr/share/vulkan/icd.d", "lvp_icd.x86_64.json");
        put("/usr/share/vulkan/icd.d", "intel_icd.x86_64.json");
        put("/usr/share/vulkan/icd.d", "nvidia_icd.json");
        put("/etc/vulkan/icd.d", "lvp_icd.x86_64.json");
        put("/opt/data/vulkan/icd.d", "lvp_icd.i686.json");
        let found = lavapipe_icds(root.path(), Some(OsStr::new("/opt/data:/usr/share")));
        assert_eq!(found, [PathBuf::from("/opt/data/vulkan/icd.d/lvp_icd.i686.json"), PathBuf::from("/usr/share/vulkan/icd.d/lvp_icd.x86_64.json")]);
        let env = software_environment(&found);
        let expected = OsString::from("/opt/data/vulkan/icd.d/lvp_icd.i686.json:/usr/share/vulkan/icd.d/lvp_icd.x86_64.json");
        assert_eq!(env, [("VK_DRIVER_FILES", expected.clone()), ("VK_ICD_FILENAMES", expected)]);
        // Without lavapipe: nothing is pointed anywhere.
        let none = tempfile::tempdir().unwrap();
        assert!(lavapipe_icds(none.path(), None).is_empty());
        assert!(software_environment(&[]).is_empty());
    }

    /// #EDE-162: in software a space's Vulkan is lavapipe alone; on the GPU
    /// exactly the space renderer's driver and lavapipe.
    #[test]
    fn a_gpu_spaces_vulkan_is_its_renderers_driver_and_lavapipe_and_a_software_ones_lavapipe() {
        let lvp = [PathBuf::from("/usr/share/vulkan/icd.d/lvp_icd.json")];
        let intel = [PathBuf::from("/usr/share/vulkan/icd.d/intel_icd.json")];
        let both = OsString::from("/usr/share/vulkan/icd.d/intel_icd.json:/usr/share/vulkan/icd.d/lvp_icd.json");
        assert_eq!(vulkan_environment(false, Some(&intel[..]), &lvp), [("VK_DRIVER_FILES", both.clone()), ("VK_ICD_FILENAMES", both)]);
        let soft = OsString::from("/usr/share/vulkan/icd.d/lvp_icd.json");
        assert_eq!(vulkan_environment(true, Some(&intel[..]), &lvp), [("VK_DRIVER_FILES", soft.clone()), ("VK_ICD_FILENAMES", soft.clone())]);
        assert_eq!(vulkan_environment(true, None, &lvp), [("VK_DRIVER_FILES", soft.clone()), ("VK_ICD_FILENAMES", soft.clone())]);
        assert_eq!(vulkan_environment(false, Some(&[][..]), &lvp), [("VK_DRIVER_FILES", soft.clone()), ("VK_ICD_FILENAMES", soft)], "its GPU's Vulkan driver not installed: lavapipe alone, never every GPU");
        assert!(vulkan_environment(false, Some(&[][..]), &[]).is_empty(), "neither installed: nothing is pointed anywhere");
        assert!(vulkan_environment(false, None, &lvp).is_empty(), "the switch off without a GPU renderer: nothing is set (F3's switch as is)");
        assert_eq!(vulkan_icd_of("i915"), Some("intel_icd"));
        assert_eq!(vulkan_icd_of("nvidia"), Some("nvidia_icd"));
        assert_eq!(vulkan_icd_of("vmwgfx"), None);
        // The intel manifests are found by name, hasvk's (Gen 7/8) is not taken.
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("usr/share/vulkan/icd.d");
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["intel_icd.x86_64.json", "intel_hasvk_icd.x86_64.json", "nvidia_icd.json"] {
            std::fs::write(dir.join(name), "{}").unwrap();
        }
        assert_eq!(icds_named(root.path(), None, "intel_icd"), [PathBuf::from("/usr/share/vulkan/icd.d/intel_icd.x86_64.json")]);
    }
}
