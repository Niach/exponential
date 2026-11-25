//! #EDE-226: what eDE's shell and the login's HOLDER (the app that keeps the
//! account, the install and its keys for the session) say to each other until
//! the session bus of #EDE-221's phase 2.
//!
//! - Shell → holder: [`Command`]s as datagrams on the app's socket
//!   (`exp-desktop.sock`). They are no `exponential://` links, so nothing a
//!   web page opens ever reaches the holder as one.
//! - Holder → shell: [`InstallState`], a 0600 file beside the install's own
//!   (`accounts/<id>/install-state.json`), replaced in one step.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The shell's socket below `XDG_RUNTIME_DIR`: the app asks it to open
/// Connect ([`CONNECT_OPEN`]), the one sign-in. No socket = a shell from
/// before it: the app shows its own login.
pub const CONNECT_SOCKET: &str = "exponential-shell-connect.sock";
pub const CONNECT_OPEN: &str = "open";

/// The shell's socket below `XDG_RUNTIME_DIR` that takes `exponential://`
/// URLs: a sign-in's return the app got (it is the scheme's handler where it
/// is installed) goes there.
pub const URL_SOCKET: &str = "exponential-shell-url.sock";

/// Below `XDG_RUNTIME_DIR`, the pid of a running holder that hands a
/// sign-in's return on: with it the shell can sign in through the browser
/// though the app is the handler.
pub const HOLDER_PID: &str = "exponential-holder.pid";

const COMMAND_PREFIX: &str = "exponential-desktop:holder:";

/// What the shell asks the holder to do. A press on an install names the
/// install AND the code the person compared: the holder acts only on the row
/// its own pin store yields with both.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "kebab-case")]
pub enum Command {
    /// Settings → Account: the deliberate sign-out of the whole login.
    SignOut { account: String },
    Approve { account: String, install: String, code: String },
    Remove { account: String, install: String, code: String },
}

impl Command {
    pub fn to_datagram(&self) -> String {
        format!("{COMMAND_PREFIX}{}", serde_json::to_string(self).expect("a command serializes"))
    }

    /// `None`: no holder command (a link, another message).
    pub fn parse(datagram: &str) -> Option<Command> {
        serde_json::from_str(datagram.strip_prefix(COMMAND_PREFIX)?).ok()
    }
}

/// One install waiting for the person's approval, as the holder's pin store
/// yields it (`install-trust.json` `row`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRow {
    pub id: String,
    pub title: String,
    pub code: String,
    /// The Badge: `New install` or `Reset`.
    pub badge: String,
}

/// A press that failed; `seq` counts them, so each is told once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailedPress {
    pub seq: u64,
    pub verb: String,
    pub message: String,
}

/// The holder's install as the shell draws it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallState {
    /// This install's own code while it waits for approval.
    #[serde(default)]
    pub waiting: Option<String>,
    #[serde(default)]
    pub rows: Vec<InstallRow>,
    #[serde(default)]
    pub failed: Option<FailedPress>,
}

/// `accounts/<id>/install-state.json`.
pub fn install_state_path(data_dir: &Path, account_id: &str) -> PathBuf {
    data_dir.join("accounts").join(account_id).join("install-state.json")
}

impl InstallState {
    pub fn read(path: &Path) -> Option<InstallState> {
        serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
    }

    /// Never makes the account's directory (a sign-out removed it).
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_vec(self).map_err(std::io::Error::other)?;
        crate::token_store::write_private(path, &json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_survives_the_socket_and_a_link_is_never_one() {
        for command in [
            Command::SignOut { account: "app.exponential.at--u1".into() },
            Command::Approve { account: "a".into(), install: "i".into(), code: "HNMBN A0319".into() },
            Command::Remove { account: "a".into(), install: "i".into(), code: "HNMBN A0319".into() },
        ] {
            let datagram = command.to_datagram();
            assert!(!datagram.contains("://"), "{datagram}");
            assert_eq!(Command::parse(&datagram), Some(command));
        }
        for other in ["exponential://sign-out", "exponential-desktop:open-window", "exponential-desktop:holder:{}", "exponential://x#exponential-desktop:holder:{\"do\":\"sign-out\",\"account\":\"a\"}"] {
            assert_eq!(Command::parse(other), None, "{other}");
        }
    }

    #[test]
    fn the_install_state_is_private_and_never_brings_an_account_back() {
        let dir = std::env::temp_dir().join(format!("exp-holder-{}", uuid::Uuid::new_v4()));
        let path = install_state_path(&dir, "acct");
        let state = InstallState {
            waiting: None,
            rows: vec![InstallRow { id: "i".into(), title: "Pixel 9".into(), code: "HNMBN A0319".into(), badge: "New install".into() }],
            failed: Some(FailedPress { seq: 1, verb: "approve".into(), message: "offline".into() }),
        };
        assert!(state.write(&path).is_err(), "no account directory, no file");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        state.write(&path).unwrap();
        assert_eq!(InstallState::read(&path), Some(state));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
