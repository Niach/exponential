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
//! never deletable, and always listed first. An agent with no config-dir
//! variable has no profiles at all. EXP-1137: "Remove account" on the ambient
//! login signs it out there and sets the per-agent [`ambient_hidden`] flag,
//! which keeps the signed-out row off the heartbeat until that login signs
//! in again ([`crate::doctor::DoctorReport::agent_accounts_detailed`] clears
//! it the moment the probe sees a login).
//!
//! The index also carries the device's per-agent LAST USED login
//! ([`active_profile`], EXP-1158) — the login an unnamed launch runs on and
//! the one the heartbeat flags `active`. Last used = per agent, the login a
//! PERSON last started or switched a run on, on that device
//! (`agent_accounts[agent].profiles[].active`); the last used agent =
//! `launch_defaults.defaultAgent`. Automations, workflow nodes, agent-started
//! runs and auto-rotation never move it. A launch naming no account runs on
//! it; `account: "system"` names the ambient login. Only the launcher writes
//! it ([`note_last_used`]), device-locally.

use std::collections::BTreeMap;
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

/// `profiles.json`: the custom profiles (never `system`), the active id and
/// (EXP-1137) whether the ambient login was removed from the account list.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Index {
    #[serde(skip_serializing_if = "Option::is_none")]
    active: Option<String>,
    profiles: Vec<AgentProfile>,
    /// EXP-1137: "Remove account" was asked for the ambient login. While set
    /// (and the ambient login is signed out) the heartbeat omits its row,
    /// and the last used login falls to the first named profile instead of it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    ambient_hidden: bool,
}

/// Whether `account` names the ambient login (`None`, blank or `system`).
pub fn is_system(account: Option<&str>) -> bool {
    account
        .map(str::trim)
        .is_none_or(|id| id.is_empty() || id == SYSTEM_PROFILE)
}

/// EXP-909 — the PROFILE ID behind a launch's `account` slot: `system` for
/// the ambient login (`None`, blank or the reserved id), the trimmed id
/// otherwise.
///
/// The twin of [`is_system`] in the vocabulary the USAGE side speaks: the
/// usage cache's entry key, the heartbeat's `profiles[].id`, the synced
/// `coding_sessions.agent_account` and the live-usage registry are all keyed
/// by this string, while [`account_dir`] is keyed by the `Option`. One
/// function, so a run's config dir and the cache row its numbers land in can
/// never disagree about which login it is. Note that `None` is the AMBIENT
/// login (no `CLAUDE_CONFIG_DIR`/`CODEX_HOME` override), never the machine's
/// active profile — those are different logins on a machine whose default
/// was moved to a named account.
pub fn profile_id(account: Option<&str>) -> String {
    if is_system(account) {
        return SYSTEM_PROFILE.to_string();
    }
    account
        .map(str::trim)
        .unwrap_or(SYSTEM_PROFILE)
        .to_string()
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
/// the active profile falls the last used login back to `system`.
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

/// EXP-1013 — the last email each login of `agent` was seen signed in as.
const EMAILS_FILE: &str = "emails.json";

fn emails_path(data_dir: &Path, agent: CodingAgent) -> PathBuf {
    agent_root(data_dir, agent).join(EMAILS_FILE)
}

fn read_emails(data_dir: &Path, agent: CodingAgent) -> BTreeMap<String, String> {
    std::fs::read_to_string(emails_path(data_dir, agent))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// EXP-1013 — a login is named by its EMAIL on every client, signed in or
/// not. A signed-out CLI names nobody, so the device remembers the last
/// address each profile (`system` included) answered with and fills it back
/// into a row that has none. An email is identity, never a credential.
///
/// Learns from every row that names an address, forgets removed profiles,
/// and writes only on a change. Best effort: an unwritable file costs the
/// memory, never the heartbeat.
pub fn remember_emails(data_dir: &Path, accounts: &mut crate::agent_accounts::AgentAccounts) {
    for agent in CodingAgent::ALL {
        let Some(account) = accounts.get_mut(agent.id()) else {
            continue;
        };
        let before = read_emails(data_dir, agent);
        let mut emails = before.clone();
        let known: Vec<String> = list(data_dir, agent).into_iter().map(|p| p.id).collect();
        emails.retain(|id, _| known.contains(id));
        let active = if account.profiles.is_empty() {
            SYSTEM_PROFILE.to_string()
        } else {
            account
                .profiles
                .iter()
                .find(|row| row.active)
                .map(|row| row.id.clone())
                .unwrap_or_default()
        };
        // A signed-IN row with no address is a real answer (codex's API-key
        // login): a plan proves it, and the memory goes. Presence-only
        // (no plan yet) says nothing either way.
        let mut settle =
            |id: &str, signed_in: bool, plan: Option<&str>, email: &mut Option<String>| {
                match email.as_deref().map(str::trim).filter(|e| !e.is_empty()) {
                    Some(seen) => {
                        emails.insert(id.to_string(), seen.to_string());
                    }
                    None if !signed_in => *email = emails.get(id).cloned(),
                    None if plan.is_some() => {
                        emails.remove(id);
                    }
                    None => {}
                }
            };
        for row in &mut account.profiles {
            settle(&row.id, row.signed_in, row.plan.as_deref(), &mut row.email);
        }
        if !active.is_empty() {
            settle(&active, account.signed_in, account.plan.as_deref(), &mut account.email);
        }
        if emails != before {
            let _ = create_private_dir(&agent_root(data_dir, agent)).and_then(|_| {
                let json = serde_json::to_string_pretty(&emails).map_err(io::Error::other)?;
                crate::atomic_config::write_atomic(&emails_path(data_dir, agent), &json)
            });
        }
    }
}

/// The device's per-agent LAST USED login (EXP-1158): the profile a launch
/// naming no account runs on, and the one the heartbeat flags `active`.
/// `system` unless set to an existing custom profile — or, EXP-1137, the
/// FIRST custom profile while the ambient login is hidden (a removed login
/// must not stay the machine's last used one: the heartbeat would mirror a
/// dead login into the top-level fields and no row would carry `active`).
pub fn active_profile(data_dir: &Path, agent: CodingAgent) -> String {
    let index = read_index(data_dir, agent);
    index
        .active
        .filter(|id| index.profiles.iter().any(|profile| &profile.id == id))
        .or_else(|| {
            index
                .ambient_hidden
                .then(|| index.profiles.first().map(|profile| profile.id.clone()))
                .flatten()
        })
        .unwrap_or_else(|| SYSTEM_PROFILE.to_string())
}

/// EXP-1138 / EXP-1158 — the account a FRESH LAUNCH (or an agent shell) of
/// `agent` actually runs on. Resumes keep their recorded account and never
/// come through here.
///
/// * A launch naming NO account (`None`/blank) runs on the LAST USED login
///   ([`active_profile`]). A last used named profile counts as pinned for
///   the start-time rotation, exactly like a composer pick (EXP-1107);
///   [`LaunchAccount::named`] only says whether the CALLER named it.
/// * An unnamed launch whose last used profile is provably SIGNED OUT
///   (`signed_out`, the doctor's account gate) falls through to the ambient
///   rules below instead of being refused: nobody picked that login, so
///   every unpinned trigger and workflow node would die on it until a person
///   started a run. A launch that NAMES a signed-out profile keeps it — the
///   gate refuses it by name.
/// * `Some("system")` names the ambient login; a named custom profile is
///   kept as given.
/// * A launch whose effective login is the ambient one (`system`, a stale id
///   — [`account_dir`] is `None` — or an unnamed launch whose last used
///   login is the ambient one) lands on the last used named profile while
///   the ambient login is HIDDEN (EXP-1137's "Remove account": a removed
///   login is never a launch target), and, with the doctor's `check` at
///   hand, on the named profile the doctor found signed in
///   ([`crate::doctor::ToolCheck::signed_in_profile`], last used first)
///   while the ambient login is provably SIGNED OUT — the doctor's account
///   gate refuses a launch ON a signed-out ambient login, and without this
///   hop every such start died on the device with "Pick another account".
///
/// Every ambient result is `None`: the ambient login rides as `None` past
/// this point (the run record, the rotation's "unpinned", the resume switch
/// detection all read it so).
pub fn resolve_launch_account(
    data_dir: &Path,
    agent: CodingAgent,
    account: Option<String>,
    check: Option<&crate::doctor::ToolCheck>,
    signed_out: impl Fn(&str) -> bool,
) -> LaunchAccount {
    let named = account
        .map(|account| account.trim().to_string())
        .filter(|account| !account.is_empty());
    let resolved = |account: Option<String>| LaunchAccount {
        account,
        named: named.is_some(),
    };
    let account = named.clone().or_else(|| Some(active_profile(data_dir, agent)));
    let is_profile = account_dir(data_dir, Some(agent), account.as_deref()).is_some();
    // The last used login nobody named, signed out: on to any signed-in one.
    let last_used_signed_out =
        is_profile && named.is_none() && account.as_deref().is_some_and(&signed_out);
    if is_profile && !last_used_signed_out {
        return resolved(account);
    }
    let doctor_profile = check
        .filter(|check| check.ambient_signed_out())
        .and_then(|check| check.signed_in_profile.clone());
    if ambient_hidden(data_dir, agent) {
        if last_used_signed_out {
            // No ambient login to fall back to: the doctor's signed-in
            // profile, else the signed-out one (the gate names it).
            return resolved(doctor_profile.or(account));
        }
        let active = active_profile(data_dir, agent);
        if active != SYSTEM_PROFILE {
            return resolved(Some(active));
        }
    }
    resolved(doctor_profile)
}

/// What [`resolve_launch_account`] settled on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchAccount {
    /// The login the launch runs on; `None` = the ambient one.
    pub account: Option<String>,
    /// Whether the CALLER named an account (`system` included). `false` =
    /// resolved from last used — never a pin for the start-time rotation.
    pub named: bool,
}

/// [`resolve_launch_account`]'s account alone, with no sign-in probe of the
/// last used profile.
pub fn launch_account(
    data_dir: &Path,
    agent: CodingAgent,
    account: Option<String>,
    check: Option<&crate::doctor::ToolCheck>,
) -> Option<String> {
    resolve_launch_account(data_dir, agent, account, check, |_| false).account
}

/// EXP-1158 — `profile` stops being `agent`'s last used login, if it was:
/// what a SIGN-OUT does to the pointer ([`remove`] clears it the same way),
/// so the next unnamed launch is not bound for a login that is gone. Writes
/// only on a change.
pub fn forget_last_used(data_dir: &Path, agent: CodingAgent, profile: &str) -> io::Result<()> {
    let mut index = read_index(data_dir, agent);
    if index.active.as_deref() != Some(profile.trim()) {
        return Ok(());
    }
    index.active = None;
    write_index(data_dir, agent, &index)
}

/// EXP-1137: whether "Remove account" hid the ambient login of `agent`.
pub fn ambient_hidden(data_dir: &Path, agent: CodingAgent) -> bool {
    config_env_var(agent).is_some() && read_index(data_dir, agent).ambient_hidden
}

/// EXP-1137: hide the ambient login of `agent` (its signed-out row leaves the
/// heartbeat) or show it again. Writes only on a change.
pub fn set_ambient_hidden(data_dir: &Path, agent: CodingAgent, hidden: bool) -> io::Result<()> {
    if config_env_var(agent).is_none() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("{} has no account profiles", agent.id()),
        ));
    }
    let mut index = read_index(data_dir, agent);
    if index.ambient_hidden == hidden {
        return Ok(());
    }
    index.ambient_hidden = hidden;
    write_index(data_dir, agent, &index)
}

/// EXP-1158 — record `profile` as `agent`'s LAST USED login (`system`
/// clears the pointer back to the ambient login). Called ONLY where a PERSON
/// started or switched a run (`launcher::prepare`'s stamp, the desktop's
/// mid-run switch): automations, workflow nodes, agent-started runs and
/// auto-rotation never move it. Writes only on a change; an unknown profile
/// is refused.
pub fn note_last_used(data_dir: &Path, agent: CodingAgent, profile: &str) -> io::Result<()> {
    let profile = profile.trim();
    let mut index = read_index(data_dir, agent);
    let next = if profile.is_empty() || profile == SYSTEM_PROFILE {
        None
    } else if index.profiles.iter().any(|row| row.id == profile) {
        Some(profile.to_string())
    } else {
        return Err(io::Error::new(io::ErrorKind::NotFound, "no such profile"));
    };
    if index.active == next {
        return Ok(());
    }
    index.active = next;
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

    /// EXP-909: the launch slot and the usage side name the same login.
    #[test]
    fn a_launchs_account_slot_and_its_profile_id_name_the_same_login() {
        for ambient in [None, Some(""), Some("  "), Some("system"), Some(" system ")] {
            assert_eq!(profile_id(ambient), SYSTEM_PROFILE, "{ambient:?}");
            assert!(is_system(ambient));
        }
        assert_eq!(profile_id(Some(" 0a1b2c3d ")), "0a1b2c3d");
        assert!(!is_system(Some("0a1b2c3d")));
        // The pairing is the point: the ambient slot has NO config dir.
        let dir = temp_dir("profile-id");
        assert_eq!(account_dir(&dir, Some(CodingAgent::Claude), None), None);
        let _ = std::fs::remove_dir_all(&dir);
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
        assert!(note_last_used(&dir, CodingAgent::Codex, "deadbeef").is_err());
        let work = create(&dir, CodingAgent::Codex, "Work").unwrap();
        note_last_used(&dir, CodingAgent::Codex, &work.id).unwrap();
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
        note_last_used(&dir, CodingAgent::Codex, SYSTEM_PROFILE).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    // EXP-1137: a hidden ambient login is never the machine's last used login while
    // a named profile exists — and the flag survives the index round trip
    // without changing the file of a machine that never set it.
    #[test]
    fn a_hidden_ambient_never_becomes_the_last_used_login() {
        let dir = temp_dir("hidden");
        assert!(!ambient_hidden(&dir, CodingAgent::Codex));
        // Hiding on a machine with only the ambient login: the flag holds,
        // the last used login stays `system` (there is nothing else to fall to).
        set_ambient_hidden(&dir, CodingAgent::Codex, true).unwrap();
        assert!(ambient_hidden(&dir, CodingAgent::Codex));
        assert_eq!(active_profile(&dir, CodingAgent::Codex), SYSTEM_PROFILE);
        let raw = std::fs::read_to_string(index_path(&dir, CodingAgent::Codex)).unwrap();
        assert!(raw.contains("\"ambientHidden\": true"), "{raw}");

        // With named profiles the FIRST one is the last used while hidden —
        // whether nothing was ever set or the set one was removed.
        let work = create(&dir, CodingAgent::Codex, "Work").unwrap();
        let home = create(&dir, CodingAgent::Codex, "Home").unwrap();
        assert_eq!(active_profile(&dir, CodingAgent::Codex), work.id);
        note_last_used(&dir, CodingAgent::Codex, &home.id).unwrap();
        assert_eq!(active_profile(&dir, CodingAgent::Codex), home.id);
        remove(&dir, CodingAgent::Codex, &home.id).unwrap();
        assert_eq!(active_profile(&dir, CodingAgent::Codex), work.id);
        // Clearing it (the ambient login signed in again) restores the old rule.
        set_ambient_hidden(&dir, CodingAgent::Codex, false).unwrap();
        assert!(!ambient_hidden(&dir, CodingAgent::Codex));
        assert_eq!(active_profile(&dir, CodingAgent::Codex), SYSTEM_PROFILE);
        let raw = std::fs::read_to_string(index_path(&dir, CodingAgent::Codex)).unwrap();
        assert!(!raw.contains("ambientHidden"), "a cleared flag leaves the file: {raw}");
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

    #[test]
    fn a_signed_out_login_keeps_the_email_it_last_answered_with() {
        use crate::agent_accounts::{AgentAccount, AgentAccounts, AgentProfileEntry};
        let dir = temp_dir("emails");
        let work = create(&dir, CodingAgent::Claude, "Work").unwrap();
        let build = |signed_in: bool, email: Option<&str>| {
            let mut accounts = AgentAccounts::new();
            accounts.insert(
                "claude".into(),
                AgentAccount {
                    signed_in,
                    email: email.map(str::to_string),
                    profiles: vec![
                        AgentProfileEntry {
                            id: SYSTEM_PROFILE.into(),
                            signed_in,
                            email: email.map(str::to_string),
                            active: true,
                            ..AgentProfileEntry::default()
                        },
                        AgentProfileEntry {
                            id: work.id.clone(),
                            signed_in,
                            email: email.map(|_| "w@acme.test".to_string()),
                            ..AgentProfileEntry::default()
                        },
                    ],
                    ..AgentAccount::default()
                },
            );
            accounts
        };
        let mut seen = build(true, Some("dev@acme.test"));
        remember_emails(&dir, &mut seen);
        let mut out = build(false, None);
        remember_emails(&dir, &mut out);
        let claude = &out["claude"];
        assert!(!claude.signed_in);
        assert_eq!(claude.email.as_deref(), Some("dev@acme.test"));
        assert_eq!(claude.profiles[0].email.as_deref(), Some("dev@acme.test"));
        assert_eq!(claude.profiles[1].email.as_deref(), Some("w@acme.test"));

        // A removed profile takes its memory along.
        remove(&dir, CodingAgent::Claude, &work.id).unwrap();
        let mut after = build(false, None);
        remember_emails(&dir, &mut after);
        assert_eq!(after["claude"].profiles[1].email, None);
        assert_eq!(after["claude"].profiles[0].email.as_deref(), Some("dev@acme.test"));
    }

    /// EXP-1158: a launch naming no account runs on the last used login; a
    /// launch naming `system` runs on the ambient login even while a named
    /// profile is the last used one; a named pick is kept. Every ambient
    /// result rides as `None`.
    #[test]
    fn an_unnamed_launch_runs_on_the_last_used_login_and_system_names_the_ambient_one() {
        let dir = temp_dir("last-used");
        let agent = CodingAgent::Claude;
        let work = create(&dir, agent, "Work").unwrap();
        let home = create(&dir, agent, "Home").unwrap();
        // Nothing used yet: the ambient login.
        assert_eq!(launch_account(&dir, agent, None, None), None);
        assert_eq!(launch_account(&dir, agent, Some("  ".into()), None), None);
        note_last_used(&dir, agent, &work.id).unwrap();
        assert_eq!(launch_account(&dir, agent, None, None), Some(work.id.clone()));
        assert_eq!(launch_account(&dir, agent, Some("system".into()), None), None);
        assert_eq!(launch_account(&dir, agent, Some(home.id.clone()), None), Some(home.id.clone()));
        // A stale id is the ambient login, never the last used one.
        assert_eq!(launch_account(&dir, agent, Some("deadbeef".into()), None), None);
        // `system` clears the pointer; an unknown profile is refused.
        note_last_used(&dir, agent, SYSTEM_PROFILE).unwrap();
        assert_eq!(launch_account(&dir, agent, None, None), None);
        assert!(note_last_used(&dir, agent, "deadbeef").is_err());
        // A sign-out forgets the pointer only when it named that profile.
        note_last_used(&dir, agent, &work.id).unwrap();
        forget_last_used(&dir, agent, &home.id).unwrap();
        assert_eq!(launch_account(&dir, agent, None, None), Some(work.id.clone()));
        forget_last_used(&dir, agent, &work.id).unwrap();
        assert_eq!(launch_account(&dir, agent, None, None), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-1138: a launch bound for the ambient login lands on the last used
    /// named login while that login is HIDDEN — and only then: an explicit
    /// named pick is kept, and a visible ambient login may be an explicit
    /// pick of the Default.
    #[test]
    fn an_ambient_launch_lands_on_the_last_used_login_while_the_ambient_login_is_hidden() {
        let dir = temp_dir("launch-account");
        let agent = CodingAgent::Codex;
        let work = create(&dir, agent, "Work").unwrap();
        let home = create(&dir, agent, "Home").unwrap();
        // Not hidden: nothing moves.
        assert_eq!(launch_account(&dir, agent, None, None), None);
        assert_eq!(launch_account(&dir, agent, Some(home.id.clone()), None), Some(home.id.clone()));
        // Hidden, no active pointer: the first named profile is the last used.
        set_ambient_hidden(&dir, agent, true).unwrap();
        assert_eq!(launch_account(&dir, agent, None, None), Some(work.id.clone()));
        assert_eq!(launch_account(&dir, agent, Some("system".into()), None), Some(work.id.clone()));
        // A stale id degrades to the ambient login, so it moves too.
        assert_eq!(launch_account(&dir, agent, Some("deadbeef".into()), None), Some(work.id.clone()));
        // An explicit named pick is the person's.
        assert_eq!(launch_account(&dir, agent, Some(home.id.clone()), None), Some(home.id.clone()));
        // The active pointer is the last used login.
        note_last_used(&dir, agent, &home.id).unwrap();
        assert_eq!(launch_account(&dir, agent, None, None), Some(home.id.clone()));
        // Hidden with no named profile at all: nowhere to go.
        let bare = temp_dir("launch-account-bare");
        set_ambient_hidden(&bare, agent, true).unwrap();
        assert_eq!(launch_account(&bare, agent, None, None), None);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&bare);
    }

    /// EXP-1138: a launch naming no account (or `system`) while the ambient
    /// login is provably signed out lands on the profile the doctor found
    /// signed in; a named pick, a signed-in ambient login and a doctor that
    /// found no signed-in profile leave the launch alone.
    #[test]
    fn an_ambient_launch_lands_on_the_doctors_signed_in_profile_while_the_ambient_login_is_signed_out() {
        let dir = temp_dir("launch-account-signed-out");
        let agent = CodingAgent::Claude;
        let work = create(&dir, agent, "Work").unwrap();
        let home = create(&dir, agent, "Home").unwrap();
        let check = |authed: Option<bool>, signed_in_profile: Option<String>| crate::doctor::ToolCheck {
            tool: crate::doctor::Tool::Claude,
            ok: true,
            version: Some("1.0.0".into()),
            error: None,
            authed,
            account: None,
            usage_eligible: false,
            acp: Some(true),
            acp_note: None,
            signed_in_profile,
        };
        let signed_out = check(Some(false), Some(work.id.clone()));
        assert_eq!(launch_account(&dir, agent, None, Some(&signed_out)), Some(work.id.clone()));
        assert_eq!(
            launch_account(&dir, agent, Some("system".into()), Some(&signed_out)),
            Some(work.id.clone())
        );
        assert_eq!(
            launch_account(&dir, agent, Some("deadbeef".into()), Some(&signed_out)),
            Some(work.id.clone())
        );
        // An explicit named pick is the person's.
        assert_eq!(
            launch_account(&dir, agent, Some(home.id.clone()), Some(&signed_out)),
            Some(home.id.clone())
        );
        // A signed-in ambient login is a legal target; so is an unknown one.
        assert_eq!(launch_account(&dir, agent, None, Some(&check(Some(true), None))), None);
        assert_eq!(launch_account(&dir, agent, None, Some(&check(None, None))), None);
        // Signed out with nowhere to go: the doctor gate says so, not this.
        assert_eq!(launch_account(&dir, agent, None, Some(&check(Some(false), None))), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-1158: an UNNAMED launch whose last used profile is signed out
    /// falls through to a signed-in login (ambient first, else the doctor's
    /// profile) and stays unnamed for the rotation; a launch NAMING that
    /// profile keeps it, so the gate refuses it by name.
    #[test]
    fn an_unnamed_launch_skips_a_signed_out_last_used_profile() {
        let dir = temp_dir("launch-account-last-used-signed-out");
        let agent = CodingAgent::Claude;
        let work = create(&dir, agent, "Work").unwrap();
        let home = create(&dir, agent, "Home").unwrap();
        note_last_used(&dir, agent, &work.id).unwrap();
        let check = |authed: Option<bool>, signed_in_profile: Option<String>| crate::doctor::ToolCheck {
            tool: crate::doctor::Tool::Claude,
            ok: true,
            version: Some("1.0.0".into()),
            error: None,
            authed,
            account: None,
            usage_eligible: false,
            acp: Some(true),
            acp_note: None,
            signed_in_profile,
        };
        let work_out = |id: &str| id == work.id;
        let resolve = |account: Option<String>, check: &crate::doctor::ToolCheck| {
            resolve_launch_account(&dir, agent, account, Some(check), work_out)
        };
        // Signed in: the last used login, unnamed.
        assert_eq!(
            resolve_launch_account(&dir, agent, None, None, |_| false),
            LaunchAccount { account: Some(work.id.clone()), named: false }
        );
        // Signed out, the ambient login signed in: the ambient login.
        assert_eq!(
            resolve(None, &check(Some(true), None)),
            LaunchAccount { account: None, named: false }
        );
        // The ambient login signed out too: the doctor's signed-in profile.
        assert_eq!(
            resolve(None, &check(Some(false), Some(home.id.clone()))),
            LaunchAccount { account: Some(home.id.clone()), named: false }
        );
        // NAMED: kept (and pinned), whatever its sign-in state.
        assert_eq!(
            resolve(Some(work.id.clone()), &check(Some(true), None)),
            LaunchAccount { account: Some(work.id.clone()), named: true }
        );
        // A removed ambient login is never the fallback.
        set_ambient_hidden(&dir, agent, true).unwrap();
        assert_eq!(resolve(None, &check(Some(true), None)).account, Some(work.id.clone()));
        assert_eq!(
            resolve(None, &check(Some(false), Some(home.id.clone()))).account,
            Some(home.id.clone())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
