//! #EXP-1875: is THIS process the user's installed app? Only that one may
//! touch per-USER state outside its data dir: the `exponential://` handler
//! (`.desktop` + `mimeapps.list`), the icon, and the self-update swap.
//!
//! On 2026-11-15 a branch build started as a test instance with its own
//! `EXP_DATA_DIR` re-registered itself as the user's link handler on every
//! start (links then opened the BRANCH binary on the user's REAL data dir),
//! and, having inherited `APPIMAGE` from the session's shell, self-updated the
//! user's AppImage. [`classify`] is the one decision; it is pure.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// `EXP_USER_INSTALL=1`: a developer's explicit "treat this build as my
/// install" (a dev build that must receive `exponential://` links). Never
/// overrides `EXP_DATA_DIR`.
pub const FORCE_ENV: &str = "EXP_USER_INSTALL";

/// Where a non-AppImage build counts as installed (a distro package).
const INSTALLED_PREFIXES: &[&str] = &["/usr/", "/opt/"];

/// What [`classify`] reads, captured once ([`LaunchEnv::current`]).
#[derive(Clone, Debug, Default)]
pub struct LaunchEnv {
    pub data_dir_override: Option<OsString>,
    pub debug_build: bool,
    pub appimage: Option<PathBuf>,
    pub appdir: Option<PathBuf>,
    pub exe: Option<PathBuf>,
    pub force: bool,
}

impl LaunchEnv {
    pub fn current() -> Self {
        let path = |key: &str| std::env::var_os(key).filter(|value| !value.is_empty()).map(PathBuf::from);
        Self {
            data_dir_override: std::env::var_os("EXP_DATA_DIR"),
            debug_build: cfg!(debug_assertions),
            appimage: path("APPIMAGE"),
            appdir: path("APPDIR"),
            exe: std::env::current_exe().ok(),
            force: std::env::var_os(FORCE_ENV).is_some_and(|value| value == "1"),
        }
    }
}

/// The answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Install {
    /// The user's AppImage, running from its own mount: handler + self-update.
    AppImage(PathBuf),
    /// An installed (packaged) binary: handler, no self-update.
    Binary(PathBuf),
    /// A test or dev instance: touches nothing user-wide.
    NotTheUsers(NotTheUsers),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotTheUsers {
    /// `EXP_DATA_DIR` names another data dir: a test instance.
    DataDirOverride,
    /// A debug build.
    DebugBuild,
    /// `APPIMAGE` is set but this binary does not run from that AppImage's
    /// mount: it was inherited (a terminal or session started by the AppImage).
    ForeignAppImage,
    /// A bare binary outside the installed paths (a `target/` build, `/tmp`).
    OutsideInstallPaths,
}

impl NotTheUsers {
    pub fn reason(self) -> &'static str {
        match self {
            Self::DataDirOverride => "this instance runs on its own data dir (EXP_DATA_DIR), so it is a test instance",
            Self::DebugBuild => "this is a debug build",
            Self::ForeignAppImage => "$APPIMAGE was inherited: this binary isn't running from that AppImage",
            Self::OutsideInstallPaths => "this binary isn't an installed one (not an AppImage, not under /usr or /opt)",
        }
    }
}

/// Does `exe` run from inside the AppImage's mount? `APPDIR` is the mount the
/// AppImage runtime exports; without it, the runtime's `.mount_*` dir.
fn runs_from_appimage(exe: &Path, appdir: Option<&Path>) -> bool {
    match appdir {
        Some(appdir) => exe.starts_with(appdir),
        None => exe.ancestors().any(|dir| dir.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.starts_with(".mount_"))),
    }
}

pub fn classify(env: &LaunchEnv) -> Install {
    if env.data_dir_override.as_ref().is_some_and(|dir| !dir.is_empty()) {
        return Install::NotTheUsers(NotTheUsers::DataDirOverride);
    }
    if env.force {
        return match (&env.appimage, &env.exe) {
            (Some(appimage), _) => Install::AppImage(appimage.clone()),
            (None, Some(exe)) => Install::Binary(exe.clone()),
            (None, None) => Install::NotTheUsers(NotTheUsers::OutsideInstallPaths),
        };
    }
    if env.debug_build {
        return Install::NotTheUsers(NotTheUsers::DebugBuild);
    }
    let Some(exe) = &env.exe else { return Install::NotTheUsers(NotTheUsers::OutsideInstallPaths) };
    if let Some(appimage) = &env.appimage {
        return if runs_from_appimage(exe, env.appdir.as_deref()) {
            Install::AppImage(appimage.clone())
        } else {
            Install::NotTheUsers(NotTheUsers::ForeignAppImage)
        };
    }
    let exe_text = exe.to_string_lossy();
    if INSTALLED_PREFIXES.iter().any(|prefix| exe_text.starts_with(prefix)) {
        Install::Binary(exe.clone())
    } else {
        Install::NotTheUsers(NotTheUsers::OutsideInstallPaths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const USERS_APPIMAGE: &str = "/home/u/Software/Exponential-production-x86_64.AppImage";

    fn the_users_appimage() -> LaunchEnv {
        LaunchEnv {
            appimage: Some(USERS_APPIMAGE.into()),
            appdir: Some("/tmp/.mount_ExponeAbC123".into()),
            exe: Some("/tmp/.mount_ExponeAbC123/usr/bin/exp-desktop".into()),
            ..LaunchEnv::default()
        }
    }

    #[test]
    fn the_users_appimage_is_the_users_install() {
        assert_eq!(classify(&the_users_appimage()), Install::AppImage(USERS_APPIMAGE.into()));
        // No APPDIR: the runtime's mount dir still says so.
        let env = LaunchEnv { appdir: None, ..the_users_appimage() };
        assert_eq!(classify(&env), Install::AppImage(USERS_APPIMAGE.into()));
        // `EXP_DATA_DIR=` (empty) means "the real dir".
        let env = LaunchEnv { data_dir_override: Some(OsString::new()), ..the_users_appimage() };
        assert_eq!(classify(&env), Install::AppImage(USERS_APPIMAGE.into()));
    }

    /// The 2026-11-15 incident: a branch build with its own data dir.
    #[test]
    fn a_data_dir_override_is_a_test_instance_even_from_an_appimage_and_forced() {
        for force in [false, true] {
            let env = LaunchEnv { data_dir_override: Some("/tmp/ide-mint-1857".into()), force, ..the_users_appimage() };
            assert_eq!(classify(&env), Install::NotTheUsers(NotTheUsers::DataDirOverride));
        }
    }

    /// The other half: `APPIMAGE` inherited from the session's shell.
    #[test]
    fn an_inherited_appimage_is_not_this_binarys() {
        let env = LaunchEnv {
            exe: Some("/home/u/repo/apps/desktop/target/release/exp-desktop".into()),
            ..the_users_appimage()
        };
        assert_eq!(classify(&env), Install::NotTheUsers(NotTheUsers::ForeignAppImage));
        let env = LaunchEnv { appdir: None, ..env };
        assert_eq!(classify(&env), Install::NotTheUsers(NotTheUsers::ForeignAppImage));
    }

    #[test]
    fn debug_builds_and_bare_binaries_are_not_the_users() {
        let env = LaunchEnv { debug_build: true, ..the_users_appimage() };
        assert_eq!(classify(&env), Install::NotTheUsers(NotTheUsers::DebugBuild));
        let bare = LaunchEnv { exe: Some("/home/u/repo/target/release/exp-desktop".into()), ..LaunchEnv::default() };
        assert_eq!(classify(&bare), Install::NotTheUsers(NotTheUsers::OutsideInstallPaths));
        let packaged = LaunchEnv { exe: Some("/usr/bin/exp-desktop".into()), ..LaunchEnv::default() };
        assert_eq!(classify(&packaged), Install::Binary("/usr/bin/exp-desktop".into()));
    }

    #[test]
    fn the_explicit_opt_in_makes_a_dev_build_the_install() {
        let env = LaunchEnv { debug_build: true, force: true, exe: Some("/r/target/debug/exp-desktop".into()), ..LaunchEnv::default() };
        assert_eq!(classify(&env), Install::Binary("/r/target/debug/exp-desktop".into()));
    }
}
