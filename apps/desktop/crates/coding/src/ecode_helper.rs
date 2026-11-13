//! #EXP-1616: git's `exp://` remote helper inside the CLI and the desktop
//! app. Both binaries answer as `git-remote-exp` when started under that
//! name (argv[0]); at start they link the name to themselves so git finds
//! it, and the helper signs in with the app's own account.

use std::path::Path;

/// Makes `git-remote-exp` findable for git: a link to this executable next
/// to it (the CLI's bin dir is on PATH), else in `<data_dir>/bin`, which is
/// then prepended to this process's PATH (sessions it starts inherit it).
/// Never inside a macOS app bundle (a write there breaks its signature);
/// not on Windows yet (a link there is a copy of the whole binary).
pub fn install_git_helper(data_dir: &Path) {
    if cfg!(windows) {
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    let in_bundle = exe.to_string_lossy().contains(".app/Contents/");
    let beside = exe.parent().map(Path::to_path_buf).filter(|_| !in_bundle);
    if let Some(dir) = beside {
        if git_remote_exp::install_link(&exe, &dir).is_ok() {
            return;
        }
    }
    let dir = data_dir.join("bin");
    match git_remote_exp::install_link(&exe, &dir) {
        Ok(_) => {
            let path = std::env::var_os("PATH").unwrap_or_default();
            let mut parts = vec![dir];
            parts.extend(std::env::split_paths(&path));
            if let Ok(joined) = std::env::join_paths(parts) {
                std::env::set_var("PATH", joined);
            }
        }
        Err(err) => log::debug!("git-remote-exp: could not link the helper: {err}"),
    }
}

/// The helper's credential when this binary runs AS `git-remote-exp`: the
/// signed-in account mints a repo token for the URL.
pub fn helper_auth(url: &str) -> Result<git_remote_exp::Auth, String> {
    if let Ok(auth) = git_remote_exp::auth_from_env(url) {
        return Ok(auth);
    }
    let data_dir = api::default_data_dir();
    let store = api::accounts::AuthStore::load(data_dir);
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
