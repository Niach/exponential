//! #EXP-1616: git's `exp://` remote helper inside the CLI and the desktop
//! app. Both binaries answer as `git-remote-exp` when started under that
//! name (argv[0]); at start they link the name to themselves so git finds
//! it, and the helper signs in with the app's own account.

use std::path::Path;

/// Makes `git-remote-exp` findable for git: a link to this executable next
/// to it, else in `<data_dir>/bin`; the link's dir is then put on this
/// process's PATH when it is not there yet (git, the CI runner's checkouts
/// and the sessions it starts inherit it). #EXP-2188: a daemon started
/// without its bin dir on PATH (mint-ci's) linked the helper beside itself
/// and git never found it, so every CI checkout of an `exp://` repo failed.
/// Never inside a macOS app bundle (a write there breaks its signature);
/// not on Windows yet (a link there is a copy of the whole binary).
pub fn install_git_helper(data_dir: &Path) {
    if cfg!(windows) {
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    let in_bundle = exe.to_string_lossy().contains(".app/Contents/");
    let beside = exe.parent().map(Path::to_path_buf).filter(|_| !in_bundle);
    let linked = beside
        .filter(|dir| git_remote_exp::install_link(&exe, dir).is_ok())
        .or_else(|| {
            let dir = data_dir.join("bin");
            match git_remote_exp::install_link(&exe, &dir) {
                Ok(_) => Some(dir),
                Err(err) => {
                    log::debug!("git-remote-exp: could not link the helper: {err}");
                    None
                }
            }
        });
    if let Some(path) = linked.and_then(|dir| path_with(&dir, std::env::var_os("PATH"))) {
        std::env::set_var("PATH", path);
    }
}

/// `path` with `dir` in front, or `None` when `dir` is already on it.
fn path_with(dir: &Path, path: Option<std::ffi::OsString>) -> Option<std::ffi::OsString> {
    let path = path.unwrap_or_default();
    if std::env::split_paths(&path).any(|entry| entry == dir) {
        return None;
    }
    let mut parts = vec![dir.to_path_buf()];
    parts.extend(std::env::split_paths(&path).filter(|entry| !entry.as_os_str().is_empty()));
    std::env::join_paths(parts).ok()
}

/// #EXP-2187: the data dir the helper signs in from: the app's, as the
/// launcher passed it to a session whose own XDG dirs hide it, else this
/// process's default.
fn helper_data_dir(passed: Option<std::ffi::OsString>) -> std::path::PathBuf {
    passed.filter(|dir| !dir.is_empty()).map(std::path::PathBuf::from).unwrap_or_else(api::default_data_dir)
}

/// The helper's credential when this binary runs AS `git-remote-exp`: the
/// signed-in account mints a repo token for the URL.
pub fn helper_auth(url: &str) -> Result<git_remote_exp::Auth, String> {
    if let Ok(auth) = git_remote_exp::auth_from_env(url) {
        return Ok(auth);
    }
    let store = api::accounts::AuthStore::load(helper_data_dir(std::env::var_os(crate::session_env::GIT_DATA_DIR_VAR)));
    let accounts = store.signed_in_accounts();
    let selector = std::env::var("EXP_ACCOUNT").ok().filter(|v| !v.is_empty());
    let account = match selector {
        Some(selector) => accounts.iter().find(|a| a.id == selector || a.email == selector),
        None if accounts.len() == 1 => accounts.first(),
        None => None,
    }
    .ok_or("not signed in to exactly one Exponential account: run `exponential login` (or set EXP_ACCOUNT)")?;
    let bearer = store.token(&account.id).ok_or("the Exponential account has no session: run `exponential login`")?;
    git_remote_exp::auth_from_session(&account.instance_url, &bearer, url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_helper_signs_in_from_the_dir_the_launcher_passed() {
        assert_eq!(helper_data_dir(Some("/real/exponential".into())), std::path::PathBuf::from("/real/exponential"));
        assert_eq!(helper_data_dir(Some("".into())), api::default_data_dir());
        assert_eq!(helper_data_dir(None), api::default_data_dir());
    }

    #[test]
    fn the_helper_dir_goes_on_path_once() {
        let dir = std::path::Path::new("/opt/exp/bin");
        assert_eq!(path_with(dir, Some("/usr/bin:/bin".into())), Some("/opt/exp/bin:/usr/bin:/bin".into()));
        assert_eq!(path_with(dir, Some("/usr/bin:/opt/exp/bin".into())), None);
        assert_eq!(path_with(dir, None), Some("/opt/exp/bin".into()));
    }
}
