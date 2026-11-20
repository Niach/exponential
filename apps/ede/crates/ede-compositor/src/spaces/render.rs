//! #EDE-138: what a space's apps draw with while spaces have no GPU path of
//! their own (#EDE-162, F7, builds one). A space's Wayland takes shared
//! memory buffers alone (no dmabuf, no explicit sync), so a frame drawn on
//! the GPU cannot reach it; what an app does about that decides whether it
//! shows anything:
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
//! The switch is a space's own and on: F7 turns it off for a space whose
//! GPU path is ready, before its first launch (the space's user manager
//! keeps the environment it started with).

use std::{
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

use crate::{desk::DeskId, doctor::VULKAN_ICD_DIRS, state::Ede};

/// The compositor's to set while a space renders in software, refused in
/// a caller's `env`: `VK_DRIVER_FILES`, and `VK_ICD_FILENAMES` for Vulkan
/// loaders before 1.3.207.
pub const SET: [&str; 2] = ["VK_DRIVER_FILES", "VK_ICD_FILENAMES"];

/// lavapipe's ICD manifests under `root` (`/` but in tests): in the
/// loader's directories and each of `data_dirs` (`XDG_DATA_DIRS`), one per
/// file name (the first found), sorted. More than one = one per
/// architecture (`lvp_icd.x86_64.json`, `lvp_icd.i686.json`).
pub fn lavapipe_icds(root: &Path, data_dirs: Option<&OsStr>) -> Vec<PathBuf> {
    let under = |dir: &Path| root.join(dir.strip_prefix("/").unwrap_or(dir));
    let data: Vec<PathBuf> = std::env::split_paths(data_dirs.filter(|dirs| !dirs.is_empty()).unwrap_or(OsStr::new("/usr/local/share:/usr/share"))).map(|dir| dir.join("vulkan/icd.d")).collect();
    let mut found: Vec<PathBuf> = Vec::new();
    for dir in VULKAN_ICD_DIRS.iter().map(PathBuf::from).chain(data) {
        let Ok(entries) = std::fs::read_dir(under(&dir)) else { continue };
        let mut names: Vec<OsString> = entries.flatten().map(|entry| entry.file_name()).filter(|name| name.to_str().is_some_and(|name| name.starts_with("lvp_icd") && name.ends_with(".json"))).collect();
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

impl Ede {
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
}
