//! #EXP-2161 (member-space.json ISOLATION): how the device's logins reach a
//! member's space: as ACCESS TOKENS only.
//!
//! The HOST ([`collect`]) reads each signed-in profile's stored login (kept
//! fresh by its own usage pass, which alone holds the refresh grants) and
//! writes an allowlisted copy ([`claude_document`], [`codex_document`]):
//! never a refresh token, a setup token, an API key, a profile directory, a
//! transcript or a setting. It goes into the space's `host/` directory, which
//! the space can only read.
//!
//! The SPACE ([`apply`]) turns that into profiles of its own under the same
//! ids, so an account pick names the same login there; a login that went
//! away at the host goes away here. Both CLIs read a renewed token from the
//! file they keep their login in.
//!
//! A profile whose only login is a claude setup token (good for a year) or
//! an OpenAI API key has no short-lived token to give: it is not offered in
//! a space.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::agent::CodingAgent;
use crate::agent_usage::AgentStatusPayload;

/// The host's file for a space: the logins and what it knows of them.
pub const LOGINS_FILE: &str = "logins.json";

#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpaceLogins {
    /// Agent id → its logins, the host's last used first.
    pub agents: BTreeMap<String, Vec<SpaceLogin>>,
    /// `devices.agent_accounts` / `agent_usage` as the host reports them.
    pub status: AgentStatusPayload,
}

impl std::fmt::Debug for SpaceLogins {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map().entries(self.agents.iter().map(|(agent, logins)| (agent, logins.len()))).finish()
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct SpaceLogin {
    pub id: String,
    #[serde(default)]
    pub email: Option<String>,
    /// The credential file's content ([`claude_document`] / [`codex_document`]).
    pub document: Value,
}

/// claude's `.credentials.json` with the access token and what describes it.
pub fn claude_document(store: &Value) -> Option<Value> {
    let oauth = store.get("claudeAiOauth")?;
    let token = oauth.get("accessToken")?.as_str().filter(|token| !token.is_empty())?;
    // A setup token is no short-lived one, wherever it is stored.
    let expires = oauth.get("expiresAt")?.as_i64()?;
    let mut out = Map::new();
    out.insert("accessToken".into(), json!(token));
    out.insert("expiresAt".into(), json!(expires));
    for (key, plain) in [("scopes", Value::is_array as fn(&Value) -> bool), ("subscriptionType", Value::is_string), ("rateLimitTier", Value::is_string)] {
        if let Some(value) = oauth.get(key).filter(|value| plain(value)) {
            out.insert(key.into(), value.clone());
        }
    }
    Some(json!({ "claudeAiOauth": out }))
}

/// codex's `auth.json` of a ChatGPT login: the access token, the identity
/// token and the account, an EMPTY refresh token (the field is required).
pub fn codex_document(store: &Value) -> Option<Value> {
    if store.get("OPENAI_API_KEY").is_some_and(|key| key.as_str().is_some_and(|key| !key.is_empty())) {
        return None;
    }
    let tokens = store.get("tokens")?;
    let text = |key: &str| tokens.get(key).and_then(Value::as_str).filter(|text| !text.is_empty());
    let mut out = Map::new();
    out.insert("access_token".into(), json!(text("access_token")?));
    out.insert("id_token".into(), json!(text("id_token")?));
    out.insert("refresh_token".into(), json!(""));
    if let Some(account) = text("account_id") {
        out.insert("account_id".into(), json!(account));
    }
    let mut document = json!({ "OPENAI_API_KEY": null, "tokens": out });
    for key in ["auth_mode", "last_refresh"] {
        if let Some(value) = store.get(key).filter(|value| value.is_string()) {
            document[key] = value.clone();
        }
    }
    Some(document)
}

fn document_of(agent: CodingAgent, dir: &Path) -> Option<Value> {
    match agent {
        CodingAgent::Claude => match crate::claude_oauth::read_store(Some(dir)) {
            crate::claude_oauth::StoreRead::Found(store) => {
                claude_document(&json!({ "claudeAiOauth": store.oauth()? }))
            }
            _ => None,
        },
        CodingAgent::Codex => {
            let text = std::fs::read_to_string(dir.join(crate::agent_login::CODEX_AUTH_FILE)).ok()?;
            codex_document(&serde_json::from_str(&text).ok()?)
        }
    }
}

/// HOST: the logins of this device as a space may have them. BLOCKING (files).
pub fn collect(data_dir: &Path, mut status: AgentStatusPayload) -> SpaceLogins {
    let mut agents = BTreeMap::new();
    for agent in CodingAgent::ALL {
        let active = crate::agent_profiles::active_profile(data_dir, agent);
        let mut logins: Vec<SpaceLogin> = crate::agent_profiles::list(data_dir, agent)
            .into_iter()
            .filter_map(|profile| {
                let dir = crate::agent_profiles::profile_dir(data_dir, agent, &profile.id)?;
                Some(SpaceLogin {
                    document: document_of(agent, &dir)?,
                    email: crate::agent_profiles::known_email(data_dir, agent, &profile.id),
                    id: profile.id,
                })
            })
            .collect();
        logins.sort_by_key(|login| Some(&login.id) != active.as_ref());
        if !logins.is_empty() {
            agents.insert(agent.id().to_string(), logins);
        }
    }
    // Only what can be picked there is reported there.
    for (agent, account) in &mut status.accounts {
        let offered: &[SpaceLogin] = agents.get(agent).map_or(&[], Vec::as_slice);
        account.profiles.retain(|profile| offered.iter().any(|login| login.id == profile.id));
        account.signed_in &= !offered.is_empty();
    }
    SpaceLogins { agents, status }
}

/// HOST: writes them for a space (`dir` = its `host/`, the space reads it
/// only). 0600, whole-file replaced.
pub fn write(dir: &Path, logins: &SpaceLogins) -> std::io::Result<()> {
    let text = serde_json::to_string(logins).map_err(std::io::Error::other)?;
    let path = dir.join(LOGINS_FILE);
    if std::fs::read_to_string(&path).is_ok_and(|held| held == text) {
        return Ok(());
    }
    crate::atomic_config::write_atomic(&path, &text)
}

/// SPACE: what the host wrote.
pub fn read(dir: &Path) -> Option<SpaceLogins> {
    serde_json::from_str(&std::fs::read_to_string(dir.join(LOGINS_FILE)).ok()?).ok()
}

fn credential_path(agent: CodingAgent, dir: &Path) -> PathBuf {
    dir.join(match agent {
        CodingAgent::Claude => ".credentials.json",
        CodingAgent::Codex => crate::agent_login::CODEX_AUTH_FILE,
    })
}

/// SPACE: the profiles of `data_dir` become the host's logins.
pub fn apply(data_dir: &Path, logins: &SpaceLogins) {
    for agent in CodingAgent::ALL {
        let wanted: &[SpaceLogin] = logins.agents.get(agent.id()).map_or(&[], Vec::as_slice);
        let ids: Vec<String> = wanted.iter().map(|login| login.id.clone()).collect();
        let keep: BTreeSet<&str> = ids.iter().map(String::as_str).collect();
        for profile in crate::agent_profiles::list(data_dir, agent) {
            if !keep.contains(profile.id.as_str()) {
                if let Some(dir) = crate::agent_profiles::profile_dir(data_dir, agent, &profile.id) {
                    let _ = std::fs::remove_file(credential_path(agent, &dir));
                }
            }
        }
        let emails: Vec<(String, String)> =
            wanted.iter().filter_map(|login| Some((login.id.clone(), login.email.clone()?))).collect();
        if let Err(err) = crate::agent_profiles::mirror(data_dir, agent, &ids, &emails) {
            log::warn!("space logins: {} profiles: {err}", agent.id());
            continue;
        }
        for login in wanted {
            let Some(dir) = crate::agent_profiles::profile_dir(data_dir, agent, &login.id) else { continue };
            let path = credential_path(agent, &dir);
            let text = login.document.to_string();
            if std::fs::read_to_string(&path).is_ok_and(|held| held == text) {
                continue;
            }
            if let Err(err) = crate::atomic_config::write_atomic(&path, &text) {
                log::warn!("space logins: a {} token could not be stored: {err}", agent.id());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRETS: [&str; 4] = ["sk-ant-ort01-REFRESH", "rt_CODEX_REFRESH", "sk-openai-KEY", "sk-ant-oat01-SETUP"];

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("exp-space-logins-{tag}-{}-{}", std::process::id(), uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn host(dir: &Path) {
        use crate::agent_profiles::create_with_id;
        create_with_id(dir, CodingAgent::Claude, "aaaa1111").unwrap();
        create_with_id(dir, CodingAgent::Claude, "bbbb2222").unwrap();
        create_with_id(dir, CodingAgent::Codex, "cccc3333").unwrap();
        create_with_id(dir, CodingAgent::Codex, "dddd4444").unwrap();
        let claude = |id: &str| crate::agent_profiles::profile_dir(dir, CodingAgent::Claude, id).unwrap();
        let codex = |id: &str| crate::agent_profiles::profile_dir(dir, CodingAgent::Codex, id).unwrap();
        std::fs::write(
            claude("aaaa1111").join(".credentials.json"),
            json!({ "claudeAiOauth": { "accessToken": "sk-ant-oat01-ACCESS", "refreshToken": SECRETS[0], "expiresAt": 1_900_000_000_000i64,
                "scopes": ["user:inference", "user:profile"], "subscriptionType": "max", "clientId": "c", "other": SECRETS[0] },
                "mcpOAuth": { "x": { "refreshToken": SECRETS[0] } } })
            .to_string(),
        )
        .unwrap();
        // Only a setup token: nothing short-lived to give.
        std::fs::write(claude("bbbb2222").join(crate::claude_token::TOKEN_FILE), json!({ "token": SECRETS[3] }).to_string()).unwrap();
        std::fs::write(
            codex("cccc3333").join("auth.json"),
            json!({ "auth_mode": "chatgpt", "OPENAI_API_KEY": null, "last_refresh": "2026-11-25T00:00:00Z",
                "tokens": { "id_token": "a.b.c", "access_token": "ACCESS", "refresh_token": SECRETS[1], "account_id": "acc", "extra": SECRETS[1] } })
            .to_string(),
        )
        .unwrap();
        std::fs::write(codex("dddd4444").join("auth.json"), json!({ "OPENAI_API_KEY": SECRETS[2] }).to_string()).unwrap();
        crate::agent_profiles::stamp_login(dir, CodingAgent::Claude, "aaaa1111", Some("a@b.c")).unwrap();
    }

    #[test]
    fn only_access_tokens_reach_a_space() {
        let (host_dir, files, space) = (temp("host"), temp("files"), temp("space"));
        host(&host_dir);
        let logins = collect(&host_dir, AgentStatusPayload::default());
        write(&files, &logins).unwrap();
        let text = std::fs::read_to_string(files.join(LOGINS_FILE)).unwrap();
        for secret in SECRETS {
            assert!(!text.contains(secret), "{secret}");
        }
        assert!(text.contains("sk-ant-oat01-ACCESS"));
        assert!(!format!("{logins:?}").contains("ACCESS"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(std::fs::metadata(files.join(LOGINS_FILE)).unwrap().permissions().mode() & 0o777, 0o600);
        }

        apply(&space, &read(&files).unwrap());
        assert_eq!(crate::agent_profiles::active_profile(&space, CodingAgent::Claude).as_deref(), Some("aaaa1111"));
        assert_eq!(crate::agent_profiles::known_email(&space, CodingAgent::Claude, "aaaa1111").as_deref(), Some("a@b.c"));
        let ids = |agent| crate::agent_profiles::list(&space, agent).into_iter().map(|p| p.id).collect::<Vec<_>>();
        assert_eq!(ids(CodingAgent::Claude), ["aaaa1111"]);
        assert_eq!(ids(CodingAgent::Codex), ["cccc3333"]);
        let dir = crate::agent_profiles::profile_dir(&space, CodingAgent::Claude, "aaaa1111").unwrap();
        let held: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(".credentials.json")).unwrap()).unwrap();
        assert_eq!(held["claudeAiOauth"]["accessToken"], "sk-ant-oat01-ACCESS");
        assert!(held["claudeAiOauth"].get("refreshToken").is_none());
        let dir = crate::agent_profiles::profile_dir(&space, CodingAgent::Codex, "cccc3333").unwrap();
        let held: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("auth.json")).unwrap()).unwrap();
        assert_eq!(held["tokens"]["refresh_token"], "");

        // A login gone at the host is gone here, its token first.
        let mut fewer = logins.clone();
        fewer.agents.remove("claude");
        let path = crate::agent_profiles::profile_dir(&space, CodingAgent::Claude, "aaaa1111").unwrap().join(".credentials.json");
        apply(&space, &fewer);
        assert!(ids(CodingAgent::Claude).is_empty());
        assert!(!path.exists());
    }

    #[test]
    fn a_host_never_names_a_path() {
        let space = temp("path");
        let login = SpaceLogin { id: "../../evil".into(), email: None, document: json!({}) };
        apply(&space, &SpaceLogins { agents: [("claude".to_string(), vec![login])].into(), ..Default::default() });
        assert!(crate::agent_profiles::list(&space, CodingAgent::Claude).is_empty());
    }
}
