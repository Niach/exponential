//! EXP-792 (EXP-747 B1) — agent ACCOUNT PROFILES: several logins of one
//! agent CLI on one machine, each in its own config dir.
//!
//! claude and codex keep every bit of their state (credentials, trust flags,
//! sessions) under ONE directory the environment can relocate —
//! `CLAUDE_CONFIG_DIR` / `CODEX_HOME`. A profile is nothing more than such
//! a directory under `{data_dir}/agents/<agent>/<id>/`, plus a row in the
//! `profiles.json` index beside it. The CLI itself does every login and
//! holds every credential; the product only decides WHICH directory a run
//! (or a login, or a usage probe) sees.
//!
//! A login is identified by its EMAIL alone: profiles carry no names, and a
//! sign-in COMMITS into the profile already signed in as that address
//! ([`find_by_email`], `agent_login::commit_login`), so one email is one
//! profile. The AMBIENT login (the CLI's own default dir) is never a profile
//! and never used by a run, a usage probe or a launch: the doctor only
//! reports it as importable.
//!
//! The index also carries the device's per-agent LAST USED login
//! ([`active_profile`], EXP-1158) — the login an unnamed launch runs on and
//! the one the heartbeat flags `active`. Last used = per agent, the login a
//! PERSON last started or switched a run on, on that device
//! (`agent_accounts[agent].profiles[].active`); the last used agent =
//! `launch_defaults.defaultAgent`. Triggered runs, agent-started
//! runs and auto-rotation never move it. A launch naming no account (or the
//! retired `system`) runs on it. Only the launcher writes it
//! ([`note_last_used`]), device-locally.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::agent::CodingAgent;

/// The retired id of the ambient login. A stored launch/trigger/run
/// `account` still spelling it reads as UNPINNED ([`is_unpinned`]).
const LEGACY_SYSTEM: &str = "system";

/// The index file beside the profile dirs.
const INDEX_FILE: &str = "profiles.json";

/// The prefix of a sign-in's STAGING dir beside the profile dirs
/// ([`staging_dir`]); [`valid_id`] never takes one.
const STAGING_PREFIX: &str = ".login-";

/// One profile row. `created_at` / `last_login_at` are ISO instants. A
/// legacy `label` (or any other unknown key) is read past and dropped on
/// the next write.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfile {
    pub id: String,
    #[serde(default)]
    pub created_at: String,
    /// When a sign-in or an import last committed into this profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_login_at: Option<String>,
}

/// `profiles.json`: the profiles and the last used id. A legacy
/// `ambientHidden` key is read past and dropped on the next write.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Index {
    #[serde(skip_serializing_if = "Option::is_none")]
    active: Option<String>,
    profiles: Vec<AgentProfile>,
}

/// Whether `account` names NO profile: `None`, blank or the retired
/// `system` — the device's last used login decides.
pub fn is_unpinned(account: Option<&str>) -> bool {
    account
        .map(str::trim)
        .is_none_or(|id| id.is_empty() || id == LEGACY_SYSTEM)
}

/// EXP-909 — the PROFILE ID behind a launch's `account` slot: the trimmed
/// id, or empty for an unpinned slot ([`is_unpinned`]).
///
/// The usage cache's entry key, the heartbeat's `profiles[].id`, the synced
/// `coding_sessions.agent_account` and the live-usage registry are all keyed
/// by this string, while [`account_dir`] is keyed by the `Option`. One
/// function, so a run's config dir and the cache row its numbers land in can
/// never disagree about which login it is. A launch resolves its slot to a
/// profile before anything is stamped, so the empty id only ever names a
/// legacy record.
pub fn profile_id(account: Option<&str>) -> String {
    if is_unpinned(account) {
        return String::new();
    }
    account.map(str::trim).unwrap_or_default().to_string()
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
    // A hand-edited index never smuggles a dir-less row or a path in: only
    // rows whose directory exists are real.
    index.profiles.retain(|profile| {
        valid_id(&profile.id) && agent_root(data_dir, agent).join(&profile.id).is_dir()
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
/// anything else (a staging dir included) is refused before it can become a
/// path component.
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

/// 8 random lowercase hex chars (the same uuid v4 generator the batch ids
/// come from).
fn random_hex8() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..8].to_string()
}

/// Every profile of `agent` on this machine, in creation order. An agent
/// without a config-dir variable has none.
pub fn list(data_dir: &Path, agent: CodingAgent) -> Vec<AgentProfile> {
    if config_env_var(agent).is_none() {
        return Vec::new();
    }
    read_index(data_dir, agent).profiles
}

/// The profile `id` names, if it exists.
pub fn get(data_dir: &Path, agent: CodingAgent, id: &str) -> Option<AgentProfile> {
    list(data_dir, agent).into_iter().find(|profile| profile.id == id.trim())
}

fn unsupported(agent: CodingAgent) -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        format!("{} has no account profiles", agent.id()),
    )
}

/// A fresh, unused profile id under `root`. A collision on 32 random bits
/// is a curiosity; still, never reuse a dir.
fn fresh_id(root: &Path) -> String {
    loop {
        let candidate = random_hex8();
        if !root.join(&candidate).exists() {
            return candidate;
        }
    }
}

/// Create a fresh, empty profile dir for `agent` and index it. The dir is
/// 0700 — it will hold a credential.
pub fn create(data_dir: &Path, agent: CodingAgent) -> io::Result<AgentProfile> {
    if config_env_var(agent).is_none() {
        return Err(unsupported(agent));
    }
    let mut index = read_index(data_dir, agent);
    let root = agent_root(data_dir, agent);
    create_private_dir(&root)?;
    let id = fresh_id(&root);
    create_private_dir(&root.join(&id))?;
    let profile = AgentProfile {
        id,
        created_at: crate::agent_accounts::now_iso(),
        last_login_at: None,
    };
    index.profiles.push(profile.clone());
    write_index(data_dir, agent, &index)?;
    Ok(profile)
}

/// [`create`] under a FIXED id — tests that need a known profile id.
#[cfg(test)]
pub(crate) fn create_with_id(data_dir: &Path, agent: CodingAgent, id: &str) -> io::Result<AgentProfile> {
    assert!(valid_id(id), "{id}");
    let mut index = read_index(data_dir, agent);
    create_private_dir(&agent_root(data_dir, agent).join(id))?;
    let profile = AgentProfile {
        id: id.to_string(),
        created_at: crate::agent_accounts::now_iso(),
        last_login_at: None,
    };
    index.profiles.push(profile.clone());
    write_index(data_dir, agent, &index)?;
    Ok(profile)
}

/// A sign-in's STAGING dir: a fresh, 0700 `{agent root}/.login-<8hex>/`
/// the login runs in (its config-dir variable points here), so a sign-in
/// never touches an existing profile or the ambient login before it is
/// known WHO signed in ([`crate::agent_login::commit_login`]). Never listed:
/// [`valid_id`] refuses the name.
pub fn staging_dir(data_dir: &Path, agent: CodingAgent) -> io::Result<PathBuf> {
    if config_env_var(agent).is_none() {
        return Err(unsupported(agent));
    }
    let root = agent_root(data_dir, agent);
    create_private_dir(&root)?;
    let dir = loop {
        let candidate = root.join(format!("{STAGING_PREFIX}{}", random_hex8()));
        if !candidate.exists() {
            break candidate;
        }
    };
    create_private_dir(&dir)?;
    Ok(dir)
}

/// Index `staging` as a NEW profile: the dir is renamed into place under a
/// fresh id (the credential the CLI wrote there moves with it).
pub fn adopt_dir(data_dir: &Path, agent: CodingAgent, staging: &Path) -> io::Result<AgentProfile> {
    if config_env_var(agent).is_none() {
        return Err(unsupported(agent));
    }
    let mut index = read_index(data_dir, agent);
    let root = agent_root(data_dir, agent);
    create_private_dir(&root)?;
    let id = fresh_id(&root);
    std::fs::rename(staging, root.join(&id))?;
    let profile = AgentProfile {
        id,
        created_at: crate::agent_accounts::now_iso(),
        last_login_at: None,
    };
    index.profiles.push(profile.clone());
    write_index(data_dir, agent, &index)?;
    Ok(profile)
}

/// Delete a profile: its dir (credentials included — the CLI's own files,
/// gone with the login) and its index row. Removing the last used profile
/// clears the pointer.
pub fn remove(data_dir: &Path, agent: CodingAgent, id: &str) -> io::Result<()> {
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

/// The config dir behind `id` — `None` for an unknown id.
pub fn profile_dir(data_dir: &Path, agent: CodingAgent, id: &str) -> Option<PathBuf> {
    let id = id.trim();
    if config_env_var(agent).is_none() || !valid_id(id) {
        return None;
    }
    let dir = agent_root(data_dir, agent).join(id);
    dir.is_dir().then_some(dir)
}

/// [`profile_dir`] over a launch's `account` slot: an unpinned slot → `None`,
/// a non-builtin (external) agent → `None`.
pub fn account_dir(data_dir: &Path, agent: Option<CodingAgent>, account: Option<&str>) -> Option<PathBuf> {
    let agent = agent?;
    if is_unpinned(account) {
        return None;
    }
    profile_dir(data_dir, agent, account?)
}

/// The one env pair a profile run carries — `(CLAUDE_CONFIG_DIR|CODEX_HOME,
/// dir)` — or nothing for an unpinned slot / an unknown profile.
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

fn write_emails(data_dir: &Path, agent: CodingAgent, emails: &BTreeMap<String, String>) -> io::Result<()> {
    create_private_dir(&agent_root(data_dir, agent))?;
    let json = serde_json::to_string_pretty(emails).map_err(io::Error::other)?;
    crate::atomic_config::write_atomic(&emails_path(data_dir, agent), &json)
}

/// The email `id` was last seen signed in as (`emails.json`).
pub fn known_email(data_dir: &Path, agent: CodingAgent, id: &str) -> Option<String> {
    read_emails(data_dir, agent).remove(id.trim())
}

/// Two addresses name the same login: trimmed, case-insensitive.
pub fn same_email(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim(), b.trim());
    !a.is_empty() && a.eq_ignore_ascii_case(b)
}

/// The profile of `agent` signed in as `email` — the commit target of a
/// sign-in or an import. Each profile's address is `probe(id)` (what its CLI
/// says right now) or, when that names nobody, the one it last answered
/// with ([`known_email`]). First match in creation order.
pub fn find_by_email(
    data_dir: &Path,
    agent: CodingAgent,
    email: &str,
    probe: impl Fn(&str) -> Option<String>,
) -> Option<String> {
    let emails = read_emails(data_dir, agent);
    list(data_dir, agent).into_iter().map(|profile| profile.id).find(|id| {
        probe(id)
            .or_else(|| emails.get(id).cloned())
            .is_some_and(|seen| same_email(&seen, email))
    })
}

/// A sign-in or an import COMMITTED into `id`: stamp its `lastLoginAt` and
/// remember the address it signed in as.
pub fn stamp_login(data_dir: &Path, agent: CodingAgent, id: &str, email: Option<&str>) -> io::Result<()> {
    let mut index = read_index(data_dir, agent);
    let Some(profile) = index.profiles.iter_mut().find(|profile| profile.id == id) else {
        return Err(io::Error::new(io::ErrorKind::NotFound, "no such profile"));
    };
    profile.last_login_at = Some(crate::agent_accounts::now_iso());
    write_index(data_dir, agent, &index)?;
    if let Some(email) = email.map(str::trim).filter(|email| !email.is_empty()) {
        let mut emails = read_emails(data_dir, agent);
        if emails.get(id).map(String::as_str) != Some(email) {
            emails.insert(id.to_string(), email.to_string());
            write_emails(data_dir, agent, &emails)?;
        }
    }
    Ok(())
}

/// EXP-1013 — a login is named by its EMAIL on every client, signed in or
/// not. A signed-out CLI names nobody, so the device remembers the last
/// address each profile answered with and fills it back into a row that has
/// none. An email is identity, never a credential.
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
        let active = account
            .profiles
            .iter()
            .find(|row| row.active)
            .map(|row| row.id.clone());
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
        if let Some(active) = active {
            settle(&active, account.signed_in, account.plan.as_deref(), &mut account.email);
        }
        if emails != before {
            let _ = write_emails(data_dir, agent, &emails);
        }
    }
}

/// The device's per-agent LAST USED login (EXP-1158): the profile a launch
/// naming no account runs on, and the one the heartbeat flags `active`. The
/// recorded pointer while it names a profile, else the FIRST profile; `None`
/// only for an agent with no profile at all.
pub fn active_profile(data_dir: &Path, agent: CodingAgent) -> Option<String> {
    let index = read_index(data_dir, agent);
    index
        .active
        .filter(|id| index.profiles.iter().any(|profile| &profile.id == id))
        .or_else(|| index.profiles.first().map(|profile| profile.id.clone()))
}

/// EXP-1138 / EXP-1158 — the profile a FRESH LAUNCH (or an agent shell) of
/// `agent` runs on. A launch NEVER runs outside a profile dir:
///
/// 1. a NAMED profile that exists here → it (kept even when signed out, so
///    the doctor gate refuses it by name);
/// 2. else the LAST USED login ([`active_profile`]) unless it is provably
///    signed out (`signed_out`, the doctor's account gate);
/// 3. else the first other profile that is not;
/// 4. else nothing: [`LaunchAccount::account`] is `None` and the gate
///    refuses with "Sign in to … on this device first."
///
/// An unpinned slot (`None`, blank, the retired `system`) and a stale id
/// both start at 2.
pub fn resolve_launch_account(
    data_dir: &Path,
    agent: CodingAgent,
    account: Option<String>,
    signed_out: impl Fn(&str) -> bool,
) -> LaunchAccount {
    if let Some(named) = account
        .as_deref()
        .filter(|account| account_dir(data_dir, Some(agent), Some(account)).is_some())
    {
        return LaunchAccount {
            account: Some(named.trim().to_string()),
            named: true,
        };
    }
    let active = active_profile(data_dir, agent);
    let mut candidates: Vec<String> = active.clone().into_iter().collect();
    candidates.extend(
        list(data_dir, agent)
            .into_iter()
            .map(|profile| profile.id)
            .filter(|id| Some(id) != active.as_ref()),
    );
    LaunchAccount {
        account: candidates.into_iter().find(|id| !signed_out(id)),
        named: false,
    }
}

/// What [`resolve_launch_account`] settled on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchAccount {
    /// The profile the launch runs on; `None` = no profile can take it.
    pub account: Option<String>,
    /// Whether the CALLER named that profile. `false` = resolved from last
    /// used — never a pin for the start-time rotation.
    pub named: bool,
}

/// [`resolve_launch_account`]'s account alone, with no sign-in probe.
pub fn launch_account(data_dir: &Path, agent: CodingAgent, account: Option<String>) -> Option<String> {
    resolve_launch_account(data_dir, agent, account, |_| false).account
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

/// EXP-1158 — record `profile` as `agent`'s LAST USED login (an unpinned
/// slot clears the pointer). Called ONLY where a PERSON started or switched
/// a run (`launcher::prepare`'s stamp, the desktop's mid-run switch):
/// triggered runs, agent-started runs and auto-rotation never move it.
/// Writes only on a change; an unknown profile is refused.
pub fn note_last_used(data_dir: &Path, agent: CodingAgent, profile: &str) -> io::Result<()> {
    let mut index = read_index(data_dir, agent);
    let next = if is_unpinned(Some(profile)) {
        None
    } else if index.profiles.iter().any(|row| row.id == profile.trim()) {
        Some(profile.trim().to_string())
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

    /// EXP-909: the launch slot and the usage side name the same login; the
    /// retired `system` is just an unpinned slot.
    #[test]
    fn a_launchs_account_slot_and_its_profile_id_name_the_same_login() {
        for unpinned in [None, Some(""), Some("  "), Some("system"), Some(" system ")] {
            assert_eq!(profile_id(unpinned), "", "{unpinned:?}");
            assert!(is_unpinned(unpinned));
        }
        assert_eq!(profile_id(Some(" 0a1b2c3d ")), "0a1b2c3d");
        assert!(!is_unpinned(Some("0a1b2c3d")));
        let dir = temp_dir("profile-id");
        assert_eq!(account_dir(&dir, Some(CodingAgent::Claude), None), None);
        assert_eq!(account_dir(&dir, Some(CodingAgent::Claude), Some("system")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn create_list_remove_round_trip_with_no_names() {
        let dir = temp_dir("crud");
        assert!(list(&dir, CodingAgent::Claude).is_empty(), "the ambient login is never a profile");
        assert_eq!(active_profile(&dir, CodingAgent::Claude), None);

        let work = create(&dir, CodingAgent::Claude).unwrap();
        assert!(valid_id(&work.id), "{}", work.id);
        assert!(!work.created_at.is_empty());
        assert_eq!(work.last_login_at, None);
        let home = create(&dir, CodingAgent::Claude).unwrap();
        assert_ne!(work.id, home.id);

        let listed = list(&dir, CodingAgent::Claude);
        assert_eq!(
            listed.iter().map(|profile| profile.id.as_str()).collect::<Vec<_>>(),
            vec![work.id.as_str(), home.id.as_str()]
        );
        let work_dir = profile_dir(&dir, CodingAgent::Claude, &work.id).unwrap();
        assert!(work_dir.is_dir());
        assert_eq!(work_dir, dir.join("agents").join("claude").join(&work.id));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&work_dir).unwrap().permissions().mode() & 0o777, 0o700);
        }
        assert!(get(&dir, CodingAgent::Claude, &home.id).is_some());
        // Codex has its own index — nothing leaks across agents.
        assert!(list(&dir, CodingAgent::Codex).is_empty());
        // Nothing name-shaped is written.
        let raw = std::fs::read_to_string(index_path(&dir, CodingAgent::Claude)).unwrap();
        assert!(!raw.contains("label"), "{raw}");

        remove(&dir, CodingAgent::Claude, &work.id).unwrap();
        assert!(!work_dir.exists());
        assert_eq!(list(&dir, CodingAgent::Claude).len(), 1);
        assert_eq!(profile_dir(&dir, CodingAgent::Claude, &work.id), None);
        assert!(remove(&dir, CodingAgent::Claude, &work.id).is_err(), "already gone");
        // A path-shaped id never becomes a directory lookup.
        assert_eq!(profile_dir(&dir, CodingAgent::Claude, "../etc"), None);
        assert_eq!(account_dir(&dir, None, Some("0a1b2c3d")), None, "external agent");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A legacy index (labels, a `system` row, `ambientHidden`) reads as its
    /// real profile dirs, and the next write drops every legacy key.
    #[test]
    fn a_legacy_index_reads_its_dirs_and_drops_labels_on_the_next_write() {
        let dir = temp_dir("legacy");
        let real = create(&dir, CodingAgent::Claude).unwrap();
        let path = index_path(&dir, CodingAgent::Claude);
        let raw = format!(
            r#"{{"ambientHidden":true,"profiles":[{{"id":"system","label":"Evil","createdAt":""}},{{"id":"{}","label":"Real","createdAt":"x"}},{{"id":"ffffffff","label":"Ghost","createdAt":"x"}},{{"id":"../../x","label":"Path","createdAt":"x"}}]}}"#,
            real.id
        );
        std::fs::write(&path, raw).unwrap();
        let listed = list(&dir, CodingAgent::Claude);
        assert_eq!(listed.len(), 1, "{listed:?}");
        assert_eq!(listed[0].id, real.id);
        note_last_used(&dir, CodingAgent::Claude, &real.id).unwrap();
        let rewritten = std::fs::read_to_string(&path).unwrap();
        assert!(!rewritten.contains("label"), "{rewritten}");
        assert!(!rewritten.contains("ambientHidden"), "{rewritten}");
        assert!(!rewritten.contains("system"), "{rewritten}");
        // A corrupt file reads as "no profiles", never a panic.
        std::fs::write(&path, "{{{").unwrap();
        assert!(list(&dir, CodingAgent::Claude).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn active_profile_falls_back_to_the_first_and_follows_removal() {
        let dir = temp_dir("active");
        assert!(note_last_used(&dir, CodingAgent::Codex, "deadbeef").is_err());
        let work = create(&dir, CodingAgent::Codex).unwrap();
        let home = create(&dir, CodingAgent::Codex).unwrap();
        assert_eq!(active_profile(&dir, CodingAgent::Codex), Some(work.id.clone()));
        note_last_used(&dir, CodingAgent::Codex, &home.id).unwrap();
        assert_eq!(active_profile(&dir, CodingAgent::Codex), Some(home.id.clone()));
        assert_eq!(
            config_env(&dir, CodingAgent::Codex, Some(&home.id)).unwrap().0,
            "CODEX_HOME"
        );
        assert_eq!(config_env(&dir, CodingAgent::Codex, Some("system")), None);
        assert_eq!(config_env(&dir, CodingAgent::Codex, None), None);
        remove(&dir, CodingAgent::Codex, &home.id).unwrap();
        assert_eq!(active_profile(&dir, CodingAgent::Codex), Some(work.id.clone()));
        // An unpinned id clears the pointer.
        note_last_used(&dir, CodingAgent::Codex, "system").unwrap();
        assert_eq!(active_profile(&dir, CodingAgent::Codex), Some(work.id.clone()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A sign-in's staging dir sits beside the profiles, never listed, and is
    /// adopted as a NEW profile under a fresh id with its files.
    #[test]
    fn a_staging_dir_is_never_listed_and_is_adopted_whole() {
        let dir = temp_dir("staging");
        let staging = staging_dir(&dir, CodingAgent::Claude).unwrap();
        assert!(staging
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(STAGING_PREFIX));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&staging).unwrap().permissions().mode() & 0o777, 0o700);
        }
        std::fs::write(staging.join(".credentials.json"), "{}").unwrap();
        assert!(list(&dir, CodingAgent::Claude).is_empty());
        let adopted = adopt_dir(&dir, CodingAgent::Claude, &staging).unwrap();
        assert!(!staging.exists());
        let home = profile_dir(&dir, CodingAgent::Claude, &adopted.id).unwrap();
        assert!(home.join(".credentials.json").is_file());
        assert_eq!(list(&dir, CodingAgent::Claude).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Commit-by-email: the probe wins, `emails.json` fills in for a
    /// signed-out profile, the compare is trimmed and case-insensitive, and a
    /// commit stamps `lastLoginAt` + remembers the address.
    #[test]
    fn a_login_finds_its_profile_by_email_and_stamps_it() {
        let dir = temp_dir("by-email");
        let agent = CodingAgent::Claude;
        let a = create(&dir, agent).unwrap();
        let b = create(&dir, agent).unwrap();
        assert_eq!(find_by_email(&dir, agent, "a@acme.test", |_| None), None);
        stamp_login(&dir, agent, &a.id, Some("a@acme.test")).unwrap();
        assert!(get(&dir, agent, &a.id).unwrap().last_login_at.is_some());
        assert_eq!(get(&dir, agent, &b.id).unwrap().last_login_at, None);
        assert_eq!(known_email(&dir, agent, &a.id).as_deref(), Some("a@acme.test"));
        assert_eq!(
            find_by_email(&dir, agent, "  A@Acme.Test ", |_| None),
            Some(a.id.clone())
        );
        // What the CLI says now beats the memory.
        let b_id = b.id.clone();
        let probe = |id: &str| (id == b_id).then(|| "a@acme.test".to_string());
        let moved = |id: &str| (id == a.id).then(|| "c@acme.test".to_string()).or_else(|| probe(id));
        assert_eq!(find_by_email(&dir, agent, "a@acme.test", moved), Some(b.id.clone()));
        assert!(stamp_login(&dir, agent, "deadbeef", None).is_err());
        assert!(!same_email("", ""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_signed_out_login_keeps_the_email_it_last_answered_with() {
        use crate::agent_accounts::{AgentAccount, AgentAccounts, AgentProfileEntry};
        let dir = temp_dir("emails");
        let home = create(&dir, CodingAgent::Claude).unwrap();
        let work = create(&dir, CodingAgent::Claude).unwrap();
        let build = |signed_in: bool, email: Option<&str>| {
            let mut accounts = AgentAccounts::new();
            accounts.insert(
                "claude".into(),
                AgentAccount {
                    signed_in,
                    email: email.map(str::to_string),
                    profiles: vec![
                        AgentProfileEntry {
                            id: home.id.clone(),
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

    /// A launch NEVER runs outside a profile: a named one is kept (signed in
    /// or not), else the last used one unless signed out, else the first
    /// other that is not, else none. An unpinned or stale slot is unnamed.
    #[test]
    fn a_launch_lands_on_a_profile_or_on_nothing() {
        let dir = temp_dir("launch");
        let agent = CodingAgent::Claude;
        assert_eq!(launch_account(&dir, agent, None), None, "no profile, no launch");
        let work = create(&dir, agent).unwrap();
        let home = create(&dir, agent).unwrap();
        // Nothing used yet: the first profile.
        assert_eq!(launch_account(&dir, agent, None), Some(work.id.clone()));
        assert_eq!(launch_account(&dir, agent, Some("system".into())), Some(work.id.clone()));
        assert_eq!(launch_account(&dir, agent, Some("deadbeef".into())), Some(work.id.clone()));
        note_last_used(&dir, agent, &home.id).unwrap();
        assert_eq!(
            resolve_launch_account(&dir, agent, None, |_| false),
            LaunchAccount { account: Some(home.id.clone()), named: false }
        );
        // Named: kept and pinned, whatever its sign-in state.
        assert_eq!(
            resolve_launch_account(&dir, agent, Some(work.id.clone()), |_| true),
            LaunchAccount { account: Some(work.id.clone()), named: true }
        );
        // The last used one signed out: the next one that is not.
        let home_out = |id: &str| id == home.id;
        assert_eq!(
            resolve_launch_account(&dir, agent, None, home_out).account,
            Some(work.id.clone())
        );
        // Every profile signed out: nothing (the gate refuses).
        assert_eq!(resolve_launch_account(&dir, agent, None, |_| true).account, None);
        // A sign-out forgets the pointer only when it named that profile.
        forget_last_used(&dir, agent, &work.id).unwrap();
        assert_eq!(launch_account(&dir, agent, None), Some(home.id.clone()));
        forget_last_used(&dir, agent, &home.id).unwrap();
        assert_eq!(launch_account(&dir, agent, None), Some(work.id.clone()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
