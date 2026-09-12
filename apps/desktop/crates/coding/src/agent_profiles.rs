//! EXP-792 (EXP-747 B1) — agent ACCOUNT PROFILES: several logins of one
//! agent CLI on one machine, each in its own config dir.
//!
//! claude and codex keep every bit of their state (credentials, trust flags,
//! sessions) under ONE directory the environment can relocate —
//! `CLAUDE_CONFIG_DIR` / `CODEX_HOME`. A profile is nothing more than such
//! a directory under `{data_dir}/agents/<agent>/<id>/`, plus a label in the
//! `profiles.json` index beside it. The CLI itself does every login and
//! holds every credential; the product only decides WHICH directory a run
//! (or a login, or a usage probe) sees.
//!
//! `system` is the reserved id of the ambient login (the CLI's own default
//! dir, whatever the user's shell has it at): it is never a directory here,
//! never removable, and always listed first. An agent with no config-dir
//! variable has no profiles at all.
//!
//! The index also carries the device's per-agent DEFAULT account
//! ([`active_profile`]) — the profile the local Start-coding dialog and the
//! heartbeat's `active` flag key on. It lives here, device-locally, rather
//! than on the launch-defaults wire (see the module note in `remote_admin`).

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::agent::CodingAgent;

/// The ambient login's reserved id.
pub const SYSTEM_PROFILE: &str = "system";

/// The ambient login's label — byte-identical on every client's picker.
pub const SYSTEM_LABEL: &str = "Default";

/// Label bound (the server clamps `label` at 64 as well).
pub const MAX_LABEL: usize = 64;

/// The index file beside the profile dirs.
const INDEX_FILE: &str = "profiles.json";

/// One profile row. `created_at` is an ISO instant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfile {
    pub id: String,
    pub label: String,
    pub created_at: String,
}

impl AgentProfile {
    /// The row every list starts with.
    pub fn system() -> Self {
        Self {
            id: SYSTEM_PROFILE.to_string(),
            label: SYSTEM_LABEL.to_string(),
            created_at: String::new(),
        }
    }

    pub fn is_system(&self) -> bool {
        self.id == SYSTEM_PROFILE
    }
}

/// `profiles.json`: the custom profiles (never `system`) and the active id.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Index {
    #[serde(skip_serializing_if = "Option::is_none")]
    active: Option<String>,
    profiles: Vec<AgentProfile>,
}

/// Whether `account` names the ambient login (`None`, blank or `system`).
pub fn is_system(account: Option<&str>) -> bool {
    account
        .map(str::trim)
        .is_none_or(|id| id.is_empty() || id == SYSTEM_PROFILE)
}

/// The env var that relocates `agent`'s config dir; `None` for an agent with
/// no profiles.
pub fn config_env_var(agent: CodingAgent) -> Option<&'static str> {
    match agent {
        CodingAgent::Claude => Some("CLAUDE_CONFIG_DIR"),
        CodingAgent::Codex => Some("CODEX_HOME"),
    }
}

fn agent_root(data_dir: &Path, agent: CodingAgent) -> PathBuf {
    data_dir.join("agents").join(agent.id())
}

fn index_path(data_dir: &Path, agent: CodingAgent) -> PathBuf {
    agent_root(data_dir, agent).join(INDEX_FILE)
}

fn read_index(data_dir: &Path, agent: CodingAgent) -> Index {
    let Ok(raw) = std::fs::read_to_string(index_path(data_dir, agent)) else {
        return Index::default();
    };
    let mut index: Index = serde_json::from_str(&raw).unwrap_or_default();
    // A hand-edited index never smuggles the reserved id or a dir-less row
    // in: only rows whose directory exists are real.
    index.profiles.retain(|profile| {
        !profile.is_system()
            && valid_id(&profile.id)
            && agent_root(data_dir, agent).join(&profile.id).is_dir()
    });
    index
}

fn write_index(data_dir: &Path, agent: CodingAgent, index: &Index) -> io::Result<()> {
    let root = agent_root(data_dir, agent);
    create_private_dir(&root)?;
    let json = serde_json::to_string_pretty(index).map_err(io::Error::other)?;
    crate::atomic_config::write_atomic(&index_path(data_dir, agent), &json)
}

/// A profile id is exactly 8 lowercase hex chars ([`create`] mints them);
/// anything else is refused before it can become a path component.
fn valid_id(id: &str) -> bool {
    id.len() == 8 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn create_private_dir(dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Every profile of `agent` on this machine — `system` first, then the
/// custom ones in creation order. A profile-less agent lists only `system`.
pub fn list(data_dir: &Path, agent: CodingAgent) -> Vec<AgentProfile> {
    let mut out = vec![AgentProfile::system()];
    if config_env_var(agent).is_some() {
        out.extend(read_index(data_dir, agent).profiles);
    }
    out
}

/// The profile `id` names, if it exists (`system` always does).
pub fn get(data_dir: &Path, agent: CodingAgent, id: &str) -> Option<AgentProfile> {
    list(data_dir, agent).into_iter().find(|profile| profile.id == id)
}

/// Create a fresh, empty profile dir for `agent` and index it. The id is 8
/// lowercase hex chars from a random source (the same uuid v4 generator the
/// batch ids come from); the dir is 0700 — it will hold a credential.
pub fn create(data_dir: &Path, agent: CodingAgent, label: &str) -> io::Result<AgentProfile> {
    if config_env_var(agent).is_none() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("{} has no account profiles", agent.id()),
        ));
    }
    let label: String = label.trim().chars().take(MAX_LABEL).collect();
    if label.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "a profile needs a label"));
    }
    let mut index = read_index(data_dir, agent);
    let root = agent_root(data_dir, agent);
    create_private_dir(&root)?;
    // A collision on 32 random bits is a curiosity; still, never reuse a dir.
    let id = loop {
        let candidate = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();
        if !root.join(&candidate).exists() {
            break candidate;
        }
    };
    create_private_dir(&root.join(&id))?;
    let profile = AgentProfile {
        id,
        label,
        created_at: crate::agent_accounts::now_iso(),
    };
    index.profiles.push(profile.clone());
    write_index(data_dir, agent, &index)?;
    Ok(profile)
}

/// Rename a custom profile. `system` keeps its label.
pub fn rename(data_dir: &Path, agent: CodingAgent, id: &str, label: &str) -> io::Result<()> {
    if id == SYSTEM_PROFILE {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "the default profile cannot be renamed"));
    }
    let label: String = label.trim().chars().take(MAX_LABEL).collect();
    if label.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "a profile needs a label"));
    }
    let mut index = read_index(data_dir, agent);
    let Some(profile) = index.profiles.iter_mut().find(|profile| profile.id == id) else {
        return Err(io::Error::new(io::ErrorKind::NotFound, "no such profile"));
    };
    profile.label = label;
    write_index(data_dir, agent, &index)
}

/// Delete a custom profile: its dir (credentials included — the CLI's own
/// files, gone with the login) and its index row. Never `system`. Removing
/// the active profile falls the default back to `system`.
pub fn remove(data_dir: &Path, agent: CodingAgent, id: &str) -> io::Result<()> {
    if id == SYSTEM_PROFILE {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "the default profile cannot be removed"));
    }
    let mut index = read_index(data_dir, agent);
    let before = index.profiles.len();
    index.profiles.retain(|profile| profile.id != id);
    if index.profiles.len() == before {
        return Err(io::Error::new(io::ErrorKind::NotFound, "no such profile"));
    }
    if index.active.as_deref() == Some(id) {
        index.active = None;
    }
    let dir = agent_root(data_dir, agent).join(id);
    if dir.is_dir() {
        std::fs::remove_dir_all(&dir)?;
    }
    write_index(data_dir, agent, &index)
}

/// The config dir behind `id` — `None` for `system` (the CLI's own default
/// dir, wherever the shell has it) and for an unknown id.
pub fn profile_dir(data_dir: &Path, agent: CodingAgent, id: &str) -> Option<PathBuf> {
    if id == SYSTEM_PROFILE || config_env_var(agent).is_none() || !valid_id(id) {
        return None;
    }
    let dir = agent_root(data_dir, agent).join(id);
    dir.is_dir().then_some(dir)
}

/// [`profile_dir`] over a launch's `account` slot: `None`/blank/`system` →
/// `None`, a non-builtin (external) agent → `None`.
pub fn account_dir(data_dir: &Path, agent: Option<CodingAgent>, account: Option<&str>) -> Option<PathBuf> {
    let agent = agent?;
    if is_system(account) {
        return None;
    }
    profile_dir(data_dir, agent, account?.trim())
}

/// The one env pair a profile run carries — `(CLAUDE_CONFIG_DIR|CODEX_HOME,
/// dir)` — or nothing for the ambient login / an unknown profile.
pub fn config_env(data_dir: &Path, agent: CodingAgent, account: Option<&str>) -> Option<(String, String)> {
    let var = config_env_var(agent)?;
    let dir = account_dir(data_dir, Some(agent), account)?;
    Some((var.to_string(), dir.to_string_lossy().into_owned()))
}

/// The device's per-agent DEFAULT account: the profile a local start picks
/// when nothing else is said, and the one the heartbeat flags `active`.
/// `system` unless set to an existing custom profile.
pub fn active_profile(data_dir: &Path, agent: CodingAgent) -> String {
    let index = read_index(data_dir, agent);
    index
        .active
        .filter(|id| index.profiles.iter().any(|profile| &profile.id == id))
        .unwrap_or_else(|| SYSTEM_PROFILE.to_string())
}

/// Set the per-agent default account (`system` clears it).
pub fn set_active_profile(data_dir: &Path, agent: CodingAgent, id: &str) -> io::Result<()> {
    let mut index = read_index(data_dir, agent);
    if id == SYSTEM_PROFILE {
        index.active = None;
    } else if index.profiles.iter().any(|profile| profile.id == id) {
        index.active = Some(id.to_string());
    } else {
        return Err(io::Error::new(io::ErrorKind::NotFound, "no such profile"));
    }
    write_index(data_dir, agent, &index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-agent-profiles-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn list_starts_with_system_and_create_remove_round_trip() {
        let dir = temp_dir("crud");
        let listed = list(&dir, CodingAgent::Claude);
        assert_eq!(listed.len(), 1);
        assert!(listed[0].is_system());
        assert_eq!(listed[0].label, "Default");
        assert_eq!(profile_dir(&dir, CodingAgent::Claude, SYSTEM_PROFILE), None);

        let work = create(&dir, CodingAgent::Claude, "  Work  ").unwrap();
        assert!(valid_id(&work.id), "{}", work.id);
        assert_eq!(work.label, "Work");
        assert!(!work.created_at.is_empty());
        let home = create(&dir, CodingAgent::Claude, "Home").unwrap();
        assert_ne!(work.id, home.id);

        let listed = list(&dir, CodingAgent::Claude);
        assert_eq!(
            listed.iter().map(|profile| profile.id.as_str()).collect::<Vec<_>>(),
            vec![SYSTEM_PROFILE, work.id.as_str(), home.id.as_str()]
        );
        let work_dir = profile_dir(&dir, CodingAgent::Claude, &work.id).unwrap();
        assert!(work_dir.is_dir());
        assert_eq!(work_dir, dir.join("agents").join("claude").join(&work.id));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&work_dir).unwrap().permissions().mode() & 0o777, 0o700);
        }
        assert_eq!(get(&dir, CodingAgent::Claude, &home.id).unwrap().label, "Home");
        // Codex has its own index — nothing leaks across agents.
        assert_eq!(list(&dir, CodingAgent::Codex).len(), 1);

        remove(&dir, CodingAgent::Claude, &work.id).unwrap();
        assert!(!work_dir.exists());
        assert_eq!(list(&dir, CodingAgent::Claude).len(), 2);
        assert_eq!(profile_dir(&dir, CodingAgent::Claude, &work.id), None);
        assert!(remove(&dir, CodingAgent::Claude, &work.id).is_err(), "already gone");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn system_is_reserved_and_blank_labels_are_refused() {
        let dir = temp_dir("reserved");
        assert!(remove(&dir, CodingAgent::Claude, SYSTEM_PROFILE).is_err());
        assert!(rename(&dir, CodingAgent::Claude, SYSTEM_PROFILE, "x").is_err());
        assert!(create(&dir, CodingAgent::Claude, "   ").is_err(), "blank label");
        assert_eq!(config_env_var(CodingAgent::Claude), Some("CLAUDE_CONFIG_DIR"));
        assert_eq!(config_env_var(CodingAgent::Codex), Some("CODEX_HOME"));
        assert!(is_system(None));
        assert!(is_system(Some("")));
        assert!(is_system(Some(" system ")));
        assert!(!is_system(Some("0a1b2c3d")));
        // A path-shaped id never becomes a directory lookup.
        assert_eq!(profile_dir(&dir, CodingAgent::Claude, "../etc"), None);
        assert_eq!(account_dir(&dir, None, Some("0a1b2c3d")), None, "external agent");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn active_profile_defaults_to_system_and_follows_removal() {
        let dir = temp_dir("active");
        assert_eq!(active_profile(&dir, CodingAgent::Codex), SYSTEM_PROFILE);
        assert!(set_active_profile(&dir, CodingAgent::Codex, "deadbeef").is_err());
        let work = create(&dir, CodingAgent::Codex, "Work").unwrap();
        set_active_profile(&dir, CodingAgent::Codex, &work.id).unwrap();
        assert_eq!(active_profile(&dir, CodingAgent::Codex), work.id);
        assert_eq!(
            config_env(&dir, CodingAgent::Codex, Some(&work.id)).unwrap().0,
            "CODEX_HOME"
        );
        assert_eq!(config_env(&dir, CodingAgent::Codex, Some(SYSTEM_PROFILE)), None);
        assert_eq!(config_env(&dir, CodingAgent::Codex, None), None);
        rename(&dir, CodingAgent::Codex, &work.id, "Client").unwrap();
        assert_eq!(get(&dir, CodingAgent::Codex, &work.id).unwrap().label, "Client");
        remove(&dir, CodingAgent::Codex, &work.id).unwrap();
        assert_eq!(active_profile(&dir, CodingAgent::Codex), SYSTEM_PROFILE);
        set_active_profile(&dir, CodingAgent::Codex, SYSTEM_PROFILE).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_hand_edited_index_never_lists_a_dirless_or_reserved_row() {
        let dir = temp_dir("tamper");
        let real = create(&dir, CodingAgent::Claude, "Real").unwrap();
        let path = index_path(&dir, CodingAgent::Claude);
        let raw = format!(
            r#"{{"profiles":[{{"id":"system","label":"Evil","createdAt":""}},{{"id":"{}","label":"Real","createdAt":"x"}},{{"id":"ffffffff","label":"Ghost","createdAt":"x"}},{{"id":"../../x","label":"Path","createdAt":"x"}}]}}"#,
            real.id
        );
        std::fs::write(&path, raw).unwrap();
        let listed = list(&dir, CodingAgent::Claude);
        assert_eq!(listed.len(), 2, "{listed:?}");
        assert_eq!(listed[0].label, "Default");
        assert_eq!(listed[1].id, real.id);
        // A corrupt file reads as "no custom profiles", never a panic.
        std::fs::write(&path, "{{{").unwrap();
        assert_eq!(list(&dir, CodingAgent::Claude).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
