//! EXP-484 — WHO is signed in to each agent CLI on this machine, as the
//! `devices.agentAccounts` wire map.
//!
//! The product never holds, copies, refreshes or uploads a credential: this
//! module reports the *identity* an already-signed-in CLI advertises about
//! itself (claude's `auth status` JSON, codex's app-server account, pi's
//! provider files) and nothing else. No token ever enters an [`AgentAccount`],
//! and nothing here writes to an agent's credential store.
//!
//! The vocabulary is locked across all four clients (web, iOS, Android,
//! desktop) — camelCase keys, `checkedAt` an ISO instant, `email`/`plan`
//! absent rather than null:
//!
//! ```json
//! { "claude": { "signedIn": true, "email": "a@b.c", "plan": "max",
//!               "checkedAt": "2026-08-28T10:00:00.000Z" } }
//! ```
//!
//! pi names no account (it has no login at all — only provider credentials),
//! so its caption is the PROVIDER it would run against:
//! `plan: "anthropic (oauth)"`. Codex's API-key logins report
//! `plan: "api key"` with no email.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One agent's signed-in identity. `Default` is the signed-out row.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AgentAccount {
    pub signed_in: bool,
    /// Absent (never null) when the agent names no address — pi always, and
    /// codex's API-key logins.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// The plan/provider half of the caption: claude's `subscriptionType`,
    /// codex's `planType` (or `api key`), pi's `<provider> (oauth|api key)`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    /// When this row was probed — the "as of …" fallback when a device is
    /// offline.
    pub checked_at: String,
}

/// `{ agent: account }`, keyed by the contract `codingAgent` id. `BTreeMap`
/// so the serialized wire (and the change-detection compare on the hosts) is
/// deterministic.
pub type AgentAccounts = BTreeMap<String, AgentAccount>;

/// The stamp every row of one collection pass carries.
pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// Unix seconds → the same ISO shape (codex reports resets as unix stamps).
pub fn iso_from_unix_secs(secs: i64) -> Option<String> {
    chrono::DateTime::from_timestamp(secs, 0)
        .map(|at| at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}

/// pi's account row. pi has NO login and no notion of a user: its credential
/// store (`~/.pi/agent/auth.json`) is a provider map, and
/// `~/.pi/agent/settings.json`'s `defaultProvider` names the one a run would
/// actually use. The caption is therefore the provider plus how it
/// authenticates — `anthropic (oauth)` / `openai (api key)`.
///
/// `env_credential` = any provider API key exported into the environment
/// (the doctor's [`crate::doctor::pi_auth_state`] rule); with no auth.json
/// entry to classify, that reads as an API key.
pub fn pi_account(
    auth_json: Option<&str>,
    settings_json: Option<&str>,
    env_credential: bool,
    now: &str,
) -> AgentAccount {
    let auth = auth_json
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        .and_then(|value| value.as_object().cloned());
    let default_provider = settings_json
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        .and_then(|value| {
            value
                .get("defaultProvider")
                .and_then(|found| found.as_str())
                .map(str::trim)
                .filter(|found| !found.is_empty())
                .map(str::to_string)
        });
    let provider = default_provider.or_else(|| auth.as_ref().and_then(|map| map.keys().next().cloned()));
    let signed_in = auth.as_ref().is_some_and(|map| !map.is_empty()) || env_credential;
    let plan = provider.filter(|_| signed_in).map(|name| {
        let kind = auth
            .as_ref()
            .and_then(|map| map.get(&name))
            .and_then(|entry| entry.get("type"))
            .and_then(|kind| kind.as_str())
            .map(|kind| {
                if kind.eq_ignore_ascii_case("oauth") {
                    "oauth"
                } else {
                    "api key"
                }
            })
            .unwrap_or("api key");
        format!("{name} ({kind})")
    });
    AgentAccount {
        signed_in,
        email: None,
        plan,
        checked_at: now.to_string(),
    }
}

/// The map's IDENTITY, `checked_at` excluded — a probe that finds the same
/// accounts must not look like a change to the hosts' last-sent compare (the
/// stamp moves every single probe).
pub fn accounts_key(accounts: &AgentAccounts) -> String {
    accounts
        .iter()
        .map(|(agent, account)| {
            format!(
                "{agent}:{}:{}:{}",
                account.signed_in,
                account.email.as_deref().unwrap_or_default(),
                account.plan.as_deref().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_serializes_the_locked_wire_shape() {
        let account = AgentAccount {
            signed_in: true,
            email: Some("dev@acme.test".into()),
            plan: Some("max".into()),
            checked_at: "2026-08-28T10:00:00.000Z".into(),
        };
        assert_eq!(
            serde_json::to_string(&account).unwrap(),
            r#"{"signedIn":true,"email":"dev@acme.test","plan":"max","checkedAt":"2026-08-28T10:00:00.000Z"}"#
        );
        // Absent, never null — the natives decode `email?`/`plan?`.
        let signed_out = AgentAccount {
            checked_at: "2026-08-28T10:00:00.000Z".into(),
            ..AgentAccount::default()
        };
        assert_eq!(
            serde_json::to_string(&signed_out).unwrap(),
            r#"{"signedIn":false,"checkedAt":"2026-08-28T10:00:00.000Z"}"#
        );
        // And a narrow/older row still decodes.
        let decoded: AgentAccount = serde_json::from_str(r#"{"signedIn":true}"#).unwrap();
        assert!(decoded.signed_in);
        assert_eq!(decoded.checked_at, "");
    }

    #[test]
    fn pi_account_captions_the_provider_and_its_kind() {
        let auth = r#"{"anthropic":{"type":"oauth","access":"secret"},"openai":{"type":"api"}}"#;
        let settings = r#"{"defaultProvider":"anthropic","model":"fable"}"#;
        let account = pi_account(Some(auth), Some(settings), false, "NOW");
        assert!(account.signed_in);
        assert_eq!(account.email, None, "pi never names a user");
        assert_eq!(account.plan.as_deref(), Some("anthropic (oauth)"));
        assert_eq!(account.checked_at, "NOW");

        // defaultProvider WINS over the first credential in the file.
        let account = pi_account(Some(auth), Some(r#"{"defaultProvider":"openai"}"#), false, "NOW");
        assert_eq!(account.plan.as_deref(), Some("openai (api key)"));

        // No settings file: the credential map alone names the provider.
        let account = pi_account(Some(r#"{"anthropic":{"type":"oauth"}}"#), None, false, "NOW");
        assert_eq!(account.plan.as_deref(), Some("anthropic (oauth)"));

        // An env key alone is a signed-in API-key run with no named
        // provider (nothing on disk to classify).
        let account = pi_account(None, None, true, "NOW");
        assert!(account.signed_in);
        assert_eq!(account.plan, None);

        // Nothing anywhere = signed out, and never a stale caption.
        let account = pi_account(Some("{}"), Some(r#"{"defaultProvider":"anthropic"}"#), false, "NOW");
        assert!(!account.signed_in);
        assert_eq!(account.plan, None);

        // Unparseable files degrade instead of panicking.
        let account = pi_account(Some("not json"), Some("also not json"), false, "NOW");
        assert!(!account.signed_in);
    }

    #[test]
    fn accounts_key_ignores_the_probe_stamp() {
        let mut first = AgentAccounts::new();
        first.insert(
            "claude".into(),
            AgentAccount {
                signed_in: true,
                email: Some("dev@acme.test".into()),
                plan: Some("max".into()),
                checked_at: "2026-08-28T10:00:00.000Z".into(),
            },
        );
        let mut second = first.clone();
        second.get_mut("claude").unwrap().checked_at = "2026-08-28T10:05:00.000Z".into();
        assert_eq!(accounts_key(&first), accounts_key(&second));

        // A real identity change DOES move the key.
        second.get_mut("claude").unwrap().email = Some("other@acme.test".into());
        assert_ne!(accounts_key(&first), accounts_key(&second));
    }

    #[test]
    fn iso_helpers_render_the_locked_shape() {
        assert_eq!(
            iso_from_unix_secs(1_756_000_000).as_deref(),
            Some("2025-08-24T01:46:40.000Z")
        );
        let now = now_iso();
        assert!(now.ends_with('Z'), "{now}");
        assert_eq!(now.len(), 24, "{now}");
    }
}
