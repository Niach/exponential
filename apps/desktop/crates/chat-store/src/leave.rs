//! #EDE-120 SIGN-OUT (`chat-store.json` `signOut`): only a DELIBERATE
//! sign-out deletes the account's store files (the store holds chats
//! decrypted), after the caller stopped the store. A session expiry / lost
//! token or an account switch keeps them for the next sign-in. Pure: the
//! caller lists the files.

use serde::{Deserialize, Serialize};

/// Why the account is left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaveReason {
    /// The person's Sign out (desktop, web, the CLI's `logout`).
    SignOut,
    /// A session expiry or a lost token, without a sign-out.
    Expired,
    /// Another account became the active one.
    Switch,
}

/// The files leaving deletes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeavePlan {
    pub delete: Vec<String>,
}

/// The desktop store's own file names (beside the account's sync store).
pub const DESKTOP_STORE_FILES: [&str; 4] =
    ["chat-store.sqlite", "chat-store.sqlite-wal", "chat-store.sqlite-shm", "chat-store.sqlite-journal"];

/// Whether `account` is one safe path segment (`^[A-Za-z0-9_-]{1,128}$`).
pub fn safe_account(account: &str) -> bool {
    (1..=128).contains(&account.len()) && account.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// Whether `path` (relative to the data dir or the OPFS root, `/`-separated)
/// is one of `account`'s chat store files.
pub fn is_store_file(account: &str, path: &str) -> bool {
    if !safe_account(account) {
        return false;
    }
    let parts: Vec<&str> = path.split('/').collect();
    let [dir, owner, name] = parts.as_slice() else { return false };
    if *owner != account || name.is_empty() {
        return false;
    }
    match *dir {
        "accounts" => DESKTOP_STORE_FILES.contains(name),
        "chat-store" => true,
        _ => false,
    }
}

/// SIGN-OUT: what leaving `account` does, given the files there are.
pub fn plan(reason: LeaveReason, account: &str, files: &[String]) -> LeavePlan {
    let delete = match reason {
        LeaveReason::SignOut => files.iter().filter(|path| is_store_file(account, path)).cloned().collect(),
        LeaveReason::Expired | LeaveReason::Switch => Vec::new(),
    };
    LeavePlan { delete }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{from, spec};

    #[test]
    fn sign_out_cases() {
        for case in spec()["signOut"]["cases"].as_array().unwrap() {
            let got = plan(from(&case["reason"]), case["account"].as_str().unwrap(), &from::<Vec<String>>(&case["files"]));
            assert_eq!(serde_json::to_value(got).unwrap(), case["expect"], "{}", case["name"]);
        }
    }
}
