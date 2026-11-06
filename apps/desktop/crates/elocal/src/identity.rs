//! The person at the machine: the one member and owner of every Local
//! desktop. The user id is minted once per desktop (`DesktopMeta`), the
//! email is `<login>@local.desktop` (local-parity.json `identity`), which is
//! what mentions carry and what Share as a team rewrites to the account's.

use serde::{Deserialize, Serialize};

use crate::parity;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalIdentity {
    pub user_id: String,
    pub email: String,
    pub name: String,
}

impl LocalIdentity {
    /// A fresh identity for the person at the machine: `$USER` (its gecos
    /// name from the password database, else the login) and a new id.
    pub fn for_machine() -> Self {
        let login = std::env::var("USER").ok().filter(|u| !u.is_empty()).unwrap_or_else(|| "local".to_string());
        let name = gecos_name(&login).unwrap_or_else(|| login.clone());
        Self { user_id: uuid::Uuid::new_v4().to_string(), email: parity::local_email(&login), name }
    }
}

/// The full name of `login` from `/etc/passwd` (the gecos' first field).
fn gecos_name(login: &str) -> Option<String> {
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
    passwd.lines().find_map(|line| {
        let fields: Vec<&str> = line.split(':').collect();
        if fields.first() != Some(&login) {
            return None;
        }
        fields.get(4).and_then(|gecos| gecos.split(',').next()).map(str::trim).filter(|name| !name.is_empty()).map(str::to_string)
    })
}
