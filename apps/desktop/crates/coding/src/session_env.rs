//! #EXP-1875: what every command a session runs must NOT inherit from the
//! machine's real desktop state.
//!
//! On 2026-11-15 a branch IDE started inside a session (its own
//! `EXP_DATA_DIR`) still rewrote the user's `exponential://` handler
//! (`$XDG_DATA_HOME/applications`, `$XDG_CONFIG_HOME/mimeapps.list`), and the
//! `APPIMAGE` it inherited from the AppImage IDE that ran the session made it
//! self-update the user's AppImage. So a session's spawn:
//!
//! - drops the AppImage runtime's variables ([`APPIMAGE_VARS`]);
//! - on Linux/BSD gets its OWN `XDG_DATA_HOME` / `XDG_CONFIG_HOME`
//!   ([`prepare_xdg`]): per-session dirs that MIRROR the real ones entry by
//!   entry (symlinks: `gh`, git, the lane config keep working) EXCEPT the
//!   user-wide desktop integration points ([`CONFIG_KEPT_OUT`],
//!   [`DATA_KEPT_OUT`]) and the real Exponential data dir. Recorded in the
//!   session's manifest ([`crate::session_manifest`]), so they go when the
//!   session ends (a symlink's target is never followed).

use std::path::{Path, PathBuf};

use eagent::spawn::SpawnSpec;

/// What the AppImage runtime exports into the process it starts.
pub const APPIMAGE_VARS: &[&str] = &["APPIMAGE", "APPDIR", "ARGV0", "OWD"];

/// `$XDG_CONFIG_HOME` entries a session never sees: the default-handler
/// list, login autostart, user systemd units.
pub const CONFIG_KEPT_OUT: &[&str] = &["mimeapps.list", "autostart", "systemd"];

/// `$XDG_DATA_HOME` entries a session never sees: `.desktop` handlers, the
/// mime database, icons, and the user's real Exponential data dir (accounts,
/// sessions, talk/).
pub const DATA_KEPT_OUT: &[&str] = &["applications", "mime", "icons", "exponential"];

/// The session's own XDG root: `<tmp>/exp-session-xdg/<session>`.
pub fn xdg_root(temp: &Path, session: &str) -> PathBuf {
    temp.join("exp-session-xdg").join(session)
}

/// A session id that is safe as one path component.
fn valid_session(session: &str) -> bool {
    !session.is_empty() && session.len() <= 64 && session.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// The user's real XDG dirs as THIS process sees them.
pub fn real_xdg_dirs() -> Option<(PathBuf, PathBuf)> {
    let home = std::env::var_os("HOME").filter(|home| !home.is_empty()).map(PathBuf::from)?;
    let pick = |key: &str, fallback: &str| {
        std::env::var_os(key).filter(|dir| !dir.is_empty()).map(PathBuf::from).unwrap_or_else(|| home.join(fallback))
    };
    Some((pick("XDG_CONFIG_HOME", ".config"), pick("XDG_DATA_HOME", ".local/share")))
}

/// Mirror `real`'s entries into `into` as symlinks, minus `kept_out`.
/// Idempotent: existing entries stay; a kept-out name is never linked.
#[cfg(unix)]
fn mirror(real: &Path, into: &Path, kept_out: &[&str]) -> std::io::Result<()> {
    std::fs::create_dir_all(into)?;
    let Ok(entries) = std::fs::read_dir(real) else { return Ok(()) };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        if name.to_str().is_some_and(|name| kept_out.contains(&name)) {
            continue;
        }
        let link = into.join(&name);
        if link.symlink_metadata().is_ok() {
            continue;
        }
        std::os::unix::fs::symlink(entry.path(), &link)?;
    }
    Ok(())
}

/// Create (or refresh) `root/config` + `root/data` mirroring `real_config` /
/// `real_data`. Returns `(config, data)`.
#[cfg(unix)]
pub fn prepare_xdg(root: &Path, real_config: &Path, real_data: &Path) -> std::io::Result<(PathBuf, PathBuf)> {
    use std::os::unix::fs::DirBuilderExt as _;
    // 0700, and never through a symlink someone planted in a shared /tmp.
    if let Some(parent) = root.parent() {
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(parent)?;
    }
    match root.symlink_metadata() {
        Ok(meta) if !meta.is_dir() => {
            return Err(std::io::Error::other(format!("{} is not a directory", root.display())))
        }
        Ok(_) => {}
        Err(_) => std::fs::DirBuilder::new().mode(0o700).create(root)?,
    }
    let (config, data) = (root.join("config"), root.join("data"));
    mirror(real_config, &config, CONFIG_KEPT_OUT)?;
    mirror(real_data, &data, DATA_KEPT_OUT)?;
    Ok((config, data))
}

/// Where [`apply_in`] makes a session's XDG dirs and records them.
pub struct XdgContext {
    pub temp: PathBuf,
    pub real_config: PathBuf,
    pub real_data: PathBuf,
    pub manifests: Option<PathBuf>,
}

impl XdgContext {
    /// This machine's (Linux/BSD only; `None` on macOS/Windows, where the
    /// app's dirs are not XDG's, and in unit tests, which must never write
    /// the real manifest dir).
    pub fn current() -> Option<Self> {
        if cfg!(test) || !cfg!(all(unix, not(target_os = "macos"))) {
            return None;
        }
        let (real_config, real_data) = real_xdg_dirs()?;
        Some(Self { temp: std::env::temp_dir(), real_config, real_data, manifests: crate::session_manifest::manifest_dir() })
    }
}

/// Strip the AppImage variables and, with a session on Linux/BSD, point the
/// XDG dirs at the session's own mirror ([`apply_in`]).
pub fn apply(spawn: SpawnSpec, session: Option<&str>) -> SpawnSpec {
    apply_in(spawn, session, XdgContext::current().as_ref())
}

/// [`apply`] against an explicit context. Best-effort: a mirror that cannot
/// be made leaves the XDG vars alone (logged), never fails the launch.
pub fn apply_in(spawn: SpawnSpec, session: Option<&str>, xdg: Option<&XdgContext>) -> SpawnSpec {
    let mut spawn = APPIMAGE_VARS.iter().fold(spawn, |spawn, key| spawn.env_remove(*key));
    let (Some(session), Some(xdg)) = (session.filter(|s| valid_session(s)), xdg) else { return spawn };
    let root = xdg_root(&xdg.temp, session);
    #[cfg(unix)]
    match prepare_xdg(&root, &xdg.real_config, &xdg.real_data) {
        Ok((config, data)) => {
            if let Some(manifests) = &xdg.manifests {
                crate::session_manifest::record_dir(manifests, session, &root);
            }
            spawn = spawn
                .env("XDG_CONFIG_HOME", config.to_string_lossy().into_owned())
                .env("XDG_DATA_HOME", data.to_string_lossy().into_owned());
        }
        Err(err) => log::warn!("session {session}: own XDG dirs not made ({err:#}); the real ones stay"),
    }
    #[cfg(not(unix))]
    let _ = root;
    spawn
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("exp-1875-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn appimage_vars_are_removed_from_every_session_spawn() {
        let spawn = apply(SpawnSpec::new("agent").env("KEEP", "1"), None);
        for key in ["APPIMAGE", "APPDIR"] {
            assert!(spawn.env_remove.iter().any(|removed| removed == key), "{key}: {:?}", spawn.env_remove);
        }
        assert!(!spawn.env.iter().any(|(key, _)| key.starts_with("XDG_")), "no session, no XDG");
    }

    #[test]
    fn a_session_spawn_gets_its_own_xdg_dirs_recorded_in_its_manifest() {
        let dir = scratch("apply");
        std::fs::create_dir_all(dir.join("real/config/gh")).unwrap();
        std::fs::create_dir_all(dir.join("real/data/applications")).unwrap();
        let xdg = XdgContext {
            temp: dir.join("tmp"),
            real_config: dir.join("real/config"),
            real_data: dir.join("real/data"),
            manifests: Some(dir.join("manifests")),
        };
        let spawn = apply_in(SpawnSpec::new("agent"), Some("sess-1"), Some(&xdg));
        let root = xdg_root(&xdg.temp, "sess-1");
        let get = |key: &str| spawn.env.iter().find(|(k, _)| k == key).map(|(_, v)| PathBuf::from(v));
        assert_eq!(get("XDG_CONFIG_HOME"), Some(root.join("config")));
        assert_eq!(get("XDG_DATA_HOME"), Some(root.join("data")));
        assert!(root.join("config/gh").exists() && !root.join("data/applications").exists());
        assert!(spawn.env_remove.iter().any(|key| key == "APPIMAGE"));
        let manifest = std::fs::read_to_string(dir.join("manifests/sess-1")).unwrap();
        assert_eq!(manifest.trim(), format!("dir\t{}", root.display()));
        // A session id that is no path component gets no dirs at all.
        let odd = apply_in(SpawnSpec::new("agent"), Some("../x"), Some(&xdg));
        assert!(!odd.env.iter().any(|(key, _)| key.starts_with("XDG_")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_mirror_keeps_tools_config_but_not_the_desktop_integration() {
        let dir = scratch("mirror");
        let (real_config, real_data) = (dir.join("real/config"), dir.join("real/data"));
        for sub in ["gh", "git", "exponential", "autostart", "systemd"] {
            std::fs::create_dir_all(real_config.join(sub)).unwrap();
        }
        std::fs::write(real_config.join("mimeapps.list"), "x-scheme-handler/exponential=at.exponential.desktop\n").unwrap();
        for sub in ["applications", "exponential", "icons", "mime", "fish"] {
            std::fs::create_dir_all(real_data.join(sub)).unwrap();
        }
        let root = xdg_root(&dir, "abcd-1234");
        let (config, data) = prepare_xdg(&root, &real_config, &real_data).unwrap();
        for kept in ["gh", "git", "exponential"] {
            assert!(config.join(kept).symlink_metadata().unwrap().file_type().is_symlink(), "{kept}");
        }
        for out in CONFIG_KEPT_OUT {
            assert!(config.join(out).symlink_metadata().is_err(), "{out} leaked into the session");
        }
        assert!(data.join("fish").exists());
        for out in DATA_KEPT_OUT {
            assert!(data.join(out).symlink_metadata().is_err(), "{out} leaked into the session");
        }
        // What a test build writes lands in the session, never the user's dirs.
        std::fs::create_dir_all(data.join("applications")).unwrap();
        std::fs::write(data.join("applications/at.exponential.desktop"), "Exec=/tmp/branch-build %U").unwrap();
        assert!(std::fs::read_dir(real_data.join("applications")).unwrap().next().is_none());
        // Idempotent (a resume prepares again), and removing the root never follows a link.
        prepare_xdg(&root, &real_config, &real_data).unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        assert!(real_config.join("gh").is_dir() && real_config.join("mimeapps.list").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_planted_file_at_the_root_is_refused() {
        let dir = scratch("planted");
        let root = xdg_root(&dir, "s1");
        std::fs::create_dir_all(root.parent().unwrap()).unwrap();
        std::fs::write(&root, "").unwrap();
        assert!(prepare_xdg(&root, &dir.join("c"), &dir.join("d")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
