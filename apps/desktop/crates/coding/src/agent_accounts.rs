//! EXP-484 — WHO is signed in to each agent CLI on this machine, as the
//! `devices.agentAccounts` wire map.
//!
//! The product never holds, copies, refreshes or uploads a credential: this
//! module reports the *identity* an already-signed-in CLI advertises about
//! itself (claude's `auth status` JSON, codex's app-server account) and
//! nothing else. No token ever enters an [`AgentAccount`],
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
//! Codex's API-key logins report `plan: "api key"` with no email.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// EXP-849 — one login's HEALTH, the four-value vocabulary every client
/// badges (`health` on both the account and each `profiles[]` row; the server
/// clamp keeps nothing else).
///
/// It is NOT the same question as `signed_in`: `auth status` is identity only
/// (the CLI reports an email for a login whose refresh token the provider has
/// since revoked), so health is derived from the USAGE PROBE — the one call
/// that actually spends the credential:
///
/// * [`Health::Ok`] — the last probe answered.
/// * [`Health::NeedsRelogin`] — the last probe came back 401/403: the CLI
///   still names an account, but it can no longer act as it. The fix is a
///   login, not a wait.
/// * [`Health::SignedOut`] — the CLI names nobody.
/// * [`Health::Unknown`] — signed in, never probed (or only ever failed for
///   transport reasons), and what an unreadable token degrades to. A row with
///   no `health` at all derives one from `signedIn` instead
///   ([`Health::from_signed_in`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Health {
    Ok,
    NeedsRelogin,
    SignedOut,
    Unknown,
}

impl Health {
    /// The wire token — snake_case, byte-identical on all four clients.
    pub fn as_str(self) -> &'static str {
        match self {
            Health::Ok => "ok",
            Health::NeedsRelogin => "needs_relogin",
            Health::SignedOut => "signed_out",
            Health::Unknown => "unknown",
        }
    }

    /// A wire token back into the enum; anything else (a newer build's value,
    /// junk) is [`Health::Unknown`].
    pub fn parse(raw: &str) -> Health {
        match raw.trim() {
            "ok" => Health::Ok,
            "needs_relogin" => Health::NeedsRelogin,
            "signed_out" => Health::SignedOut,
            _ => Health::Unknown,
        }
    }

    /// The fallback every reader takes when `health` is absent (an older
    /// device, a row written before EXP-849): the only thing such a payload
    /// says is whether the CLI was signed in, and a signed-in CLI reads as
    /// healthy — ×4 (`AgentHealthRules.derived`, web `agentHealth`). Claiming
    /// `Unknown` here would badge every pre-EXP-849 machine as unverified.
    pub fn from_signed_in(signed_in: bool) -> Health {
        if signed_in {
            Health::Ok
        } else {
            Health::SignedOut
        }
    }

    /// Worst-first rank — a device row badges the WORST health among its
    /// accounts, and "needs re-login" must outrank "signed out" (one is a
    /// broken machine, the other a machine nobody set up).
    pub fn severity(self) -> u8 {
        match self {
            Health::NeedsRelogin => 3,
            Health::SignedOut => 2,
            Health::Unknown => 1,
            Health::Ok => 0,
        }
    }

    /// The worse of two healths.
    pub fn worse(self, other: Health) -> Health {
        if other.severity() > self.severity() {
            other
        } else {
            self
        }
    }

    /// The badge an account row or chip carries, or `None` when there is
    /// nothing to say: `ok` needs no badge, and `unknown` ("signed in, never
    /// probed") is not a problem. The two negatives are DISTINCT on purpose
    /// and byte-identical ×4 (`AgentHealthRules.badgeLabel`, web
    /// `healthBadgeLabel`, iOS `badgeLabel`): "Signed out" is a login nobody
    /// made, "Needs re-login" one that expired under you.
    pub fn badge_label(self) -> Option<&'static str> {
        match self {
            Health::NeedsRelogin => Some("Needs re-login"),
            Health::SignedOut => Some("Signed out"),
            Health::Ok | Health::Unknown => None,
        }
    }
}

/// One agent's signed-in identity. `Default` is the signed-out row.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AgentAccount {
    pub signed_in: bool,
    /// Absent (never null) when the agent names no address — codex's
    /// API-key logins.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// The plan/provider half of the caption: claude's `subscriptionType`,
    /// codex's `planType` (or `api key`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    /// When this row was probed — the "as of …" fallback when a device is
    /// offline.
    pub checked_at: String,
    /// EXP-849: this login's [`Health`], as its wire token. Absent = derive
    /// from `signed_in` ([`Health::from_signed_in`]), which is what every
    /// pre-EXP-849 device's row means. Mirrors the ACTIVE profile's, like
    /// every other top-level field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<String>,
    /// EXP-792 (EXP-747 B5): every account PROFILE of this agent on the
    /// machine, `system` first, when there is more than the ambient login.
    /// Absent (never `[]`) on a single-login machine, so the pre-profile
    /// payload stays byte-identical. The top-level fields above mirror the
    /// ACTIVE profile.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<AgentProfileEntry>,
}

/// One profile's row inside [`AgentAccount::profiles`] — the account
/// fields again, plus the profile's identity and its OWN usage windows
/// (the top-level `agentUsage` map carries only the active profile's).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AgentProfileEntry {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub signed_in: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    /// The device's default account for this agent (exactly one is).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub active: bool,
    pub checked_at: String,
    /// EXP-849: see [`AgentAccount::health`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<crate::agent_usage::AgentUsage>,
    /// EXP-849: this login is past [`crate::agent_usage::MAX_USAGE_PROFILES`]
    /// — its identity ships, its usage numbers are NOT collected (the cap
    /// exists so one beat cannot fan out into a probe per login). Absent =
    /// monitored, so the common payload is unchanged.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unmonitored: bool,
}

impl AgentProfileEntry {
    /// The account half of this row, as the top-level shape.
    pub fn account(&self) -> AgentAccount {
        AgentAccount {
            signed_in: self.signed_in,
            email: self.email.clone(),
            plan: self.plan.clone(),
            checked_at: self.checked_at.clone(),
            health: self.health.clone(),
            profiles: Vec::new(),
        }
    }

    /// This row's [`Health`], with the absent-field fallback.
    pub fn health(&self) -> Health {
        match &self.health {
            Some(raw) => Health::parse(raw),
            None => Health::from_signed_in(self.signed_in),
        }
    }
}

impl AgentAccount {
    /// This account's [`Health`], with the absent-field fallback.
    pub fn health(&self) -> Health {
        match &self.health {
            Some(raw) => Health::parse(raw),
            None => Health::from_signed_in(self.signed_in),
        }
    }

    /// The WORST health this account reports — its own, folded with every
    /// profile row's. The device row's badge (interface A) is this, folded
    /// across agents.
    pub fn worst_health(&self) -> Health {
        self.profiles
            .iter()
            .fold(self.health(), |worst, row| worst.worse(row.health()))
    }
}

/// The worst health across a whole [`AgentAccounts`] map — what a DEVICE row
/// badges. An empty map (a device that reported no agents at all) is
/// [`Health::Unknown`], never `Ok`.
pub fn worst_health(accounts: &AgentAccounts) -> Health {
    let mut worst: Option<Health> = None;
    for account in accounts.values() {
        let health = account.worst_health();
        worst = Some(match worst {
            Some(seen) => seen.worse(health),
            // Never fold over a seeded `Unknown`: it outranks `Ok`, so a
            // machine whose single login is healthy would badge "unknown".
            None => health,
        });
    }
    worst.unwrap_or(Health::Unknown)
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

/// The inverse, for the one place that has to COMPARE a stored stamp against
/// the wall clock (EXP-831: a synced rate-limit wall expires by its own
/// `resets_at`). It lives beside the formatter for the same reason the
/// formatter is shared: one module owns the ISO<->unix conversion, so a
/// stamp this repo wrote always parses back. An unparseable stamp is `None`,
/// never an epoch.
pub fn unix_millis_from_iso(iso: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(iso)
        .ok()
        .map(|at| at.timestamp_millis())
}

/// The map's IDENTITY, `checked_at` excluded — a probe that finds the same
/// accounts must not look like a change to the hosts' last-sent compare (the
/// stamp moves every single probe).
///
/// EXP-792: the profiles fold in (id, identity, `active`), so adding,
/// removing or re-defaulting a profile is a change the heartbeat ships.
///
/// EXP-808: and so do their USAGE numbers. They used to be excluded as
/// having "their own change detector" — but that detector is the top-level
/// `agentUsage` map, which only ever carries the ACTIVE login. A secondary
/// profile's windows ride this map and nothing else, so leaving them out
/// meant a second account's bars only ever moved when its identity did.
pub fn accounts_key(accounts: &AgentAccounts) -> String {
    accounts
        .iter()
        .map(|(agent, account)| {
            let profiles = account
                .profiles
                .iter()
                .map(|profile| {
                    format!(
                        "{}={}:{}:{}:{}:{}:{}:{}",
                        profile.id,
                        profile.signed_in,
                        profile.email.as_deref().unwrap_or_default(),
                        profile.plan.as_deref().unwrap_or_default(),
                        profile.active,
                        // EXP-849: a login going `ok` → `needs_relogin` is
                        // the one change a reader most needs shipped, and it
                        // moves no other field.
                        profile.health().as_str(),
                        profile.unmonitored,
                        usage_fingerprint(profile.usage.as_ref())
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{agent}:{}:{}:{}:{}:{profiles}",
                account.signed_in,
                account.email.as_deref().unwrap_or_default(),
                account.plan.as_deref().unwrap_or_default(),
                account.health().as_str()
            )
        })
        .collect::<Vec<_>>()
        .join("|")
}

/// EXP-808 — one profile's usage, as the [`accounts_key`] fragment. It
/// covers everything a reader can see (the stamp, the dimming and the
/// windows), which is the same bar the top-level map's JSON compare sets.
fn usage_fingerprint(usage: Option<&crate::agent_usage::AgentUsage>) -> String {
    let Some(usage) = usage else {
        return String::new();
    };
    let windows = usage
        .windows
        .iter()
        .map(|window| {
            format!(
                "{}={}@{}",
                window.key,
                window.percent,
                window.resets_at.as_deref().unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join(";");
    format!("{}/{}/{windows}", usage.fetched_at, usage.stale)
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
            ..AgentAccount::default()
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
    fn accounts_key_ignores_the_probe_stamp() {
        let mut first = AgentAccounts::new();
        first.insert(
            "claude".into(),
            AgentAccount {
                signed_in: true,
                email: Some("dev@acme.test".into()),
                plan: Some("max".into()),
                checked_at: "2026-08-28T10:00:00.000Z".into(),
                ..AgentAccount::default()
            },
        );
        let mut second = first.clone();
        second.get_mut("claude").unwrap().checked_at = "2026-08-28T10:05:00.000Z".into();
        assert_eq!(accounts_key(&first), accounts_key(&second));

        // A real identity change DOES move the key.
        second.get_mut("claude").unwrap().email = Some("other@acme.test".into());
        assert_ne!(accounts_key(&first), accounts_key(&second));
    }

    /// EXP-792: a profile appearing, vanishing or becoming the default is a
    /// change; its usage numbers and probe stamp are not.
    #[test]
    fn accounts_key_moves_when_a_profile_is_added_or_removed() {
        let mut base = AgentAccounts::new();
        base.insert(
            "claude".into(),
            AgentAccount {
                signed_in: true,
                email: Some("dev@acme.test".into()),
                plan: Some("max".into()),
                checked_at: "T0".into(),
                ..AgentAccount::default()
            },
        );
        let mut with_profile = base.clone();
        with_profile.get_mut("claude").unwrap().profiles = vec![
            AgentProfileEntry {
                id: "system".into(),
                signed_in: true,
                email: Some("dev@acme.test".into()),
                active: true,
                checked_at: "T0".into(),
                ..AgentProfileEntry::default()
            },
            AgentProfileEntry {
                id: "0a1b2c3d".into(),
                label: Some("Work".into()),
                signed_in: false,
                checked_at: "T0".into(),
                ..AgentProfileEntry::default()
            },
        ];
        assert_ne!(accounts_key(&base), accounts_key(&with_profile));

        // Only the probe stamp moved: same key.
        let mut restamped = with_profile.clone();
        for profile in &mut restamped.get_mut("claude").unwrap().profiles {
            profile.checked_at = "T1".into();
        }
        assert_eq!(accounts_key(&with_profile), accounts_key(&restamped));

        // EXP-808: a profile's own NUMBERS do move it — for a non-active
        // login this map is the only place they ride, so a key that ignored
        // them would pin a second account's bars to its identity.
        let mut repolled = with_profile.clone();
        repolled.get_mut("claude").unwrap().profiles[1].usage =
            Some(crate::agent_usage::AgentUsage {
                fetched_at: "T1".into(),
                stale: false,
                windows: vec![crate::agent_usage::UsageWindow {
                    key: "session".into(),
                    label: "5h".into(),
                    percent: 42,
                    resets_at: None,
                }],
            });
        assert_ne!(accounts_key(&with_profile), accounts_key(&repolled));
        let mut dimmed = repolled.clone();
        dimmed.get_mut("claude").unwrap().profiles[1]
            .usage
            .as_mut()
            .unwrap()
            .stale = true;
        assert_ne!(accounts_key(&repolled), accounts_key(&dimmed));

        // Re-defaulting moves it; removing the profile moves it back.
        let mut redefaulted = with_profile.clone();
        {
            let profiles = &mut redefaulted.get_mut("claude").unwrap().profiles;
            profiles[0].active = false;
            profiles[1].active = true;
        }
        assert_ne!(accounts_key(&with_profile), accounts_key(&redefaulted));
        let mut removed = with_profile.clone();
        removed.get_mut("claude").unwrap().profiles.clear();
        assert_eq!(accounts_key(&base), accounts_key(&removed));
    }

    /// The wire shape of a profile row: camelCase, `active` only when true,
    /// nothing null.
    #[test]
    fn profile_entry_serializes_the_locked_wire_shape() {
        let entry = AgentProfileEntry {
            id: "0a1b2c3d".into(),
            label: Some("Work".into()),
            signed_in: true,
            email: Some("w@acme.test".into()),
            plan: None,
            active: true,
            checked_at: "2026-08-28T10:00:00.000Z".into(),
            ..AgentProfileEntry::default()
        };
        assert_eq!(
            serde_json::to_string(&entry).unwrap(),
            r#"{"id":"0a1b2c3d","label":"Work","signedIn":true,"email":"w@acme.test","active":true,"checkedAt":"2026-08-28T10:00:00.000Z"}"#
        );
        let signed_out = AgentProfileEntry {
            id: "system".into(),
            checked_at: "T".into(),
            ..AgentProfileEntry::default()
        };
        assert_eq!(
            serde_json::to_string(&signed_out).unwrap(),
            r#"{"id":"system","signedIn":false,"checkedAt":"T"}"#
        );
        // An account with profiles nests them; without, the key is absent.
        let mut account = entry.account();
        assert!(!serde_json::to_string(&account).unwrap().contains("profiles"));
        account.profiles = vec![signed_out];
        assert!(serde_json::to_string(&account).unwrap().contains(r#""profiles":[{"id":"system""#));
        let decoded: AgentAccount = serde_json::from_str(r#"{"signedIn":true}"#).unwrap();
        assert!(decoded.profiles.is_empty());
    }

    /// EXP-849 — the four-value health vocabulary: the wire tokens, the
    /// absent-field fallback, the worst-wins fold a device row badges, and
    /// the fact that it moves `accounts_key` (a revoked login must not wait
    /// for an identity change to reach the clients).
    #[test]
    fn health_tokens_fall_back_and_fold_worst_first() {
        assert_eq!(Health::Ok.as_str(), "ok");
        assert_eq!(Health::NeedsRelogin.as_str(), "needs_relogin");
        assert_eq!(Health::SignedOut.as_str(), "signed_out");
        assert_eq!(Health::Unknown.as_str(), "unknown");
        for health in [
            Health::Ok,
            Health::NeedsRelogin,
            Health::SignedOut,
            Health::Unknown,
        ] {
            assert_eq!(Health::parse(health.as_str()), health);
        }
        // A newer build's value, or junk, degrades — never panics.
        assert_eq!(Health::parse("on_fire"), Health::Unknown);
        assert_eq!(Health::parse(""), Health::Unknown);
        // Absent = derived from `signedIn`: signed in reads HEALTHY (×4), so
        // a pre-EXP-849 machine is never badged as unverified.
        assert_eq!(Health::from_signed_in(true), Health::Ok);
        assert_eq!(Health::from_signed_in(false), Health::SignedOut);
        let legacy: AgentAccount = serde_json::from_str(r#"{"signedIn":true}"#).unwrap();
        assert_eq!(legacy.health(), Health::Ok);
        let legacy_out: AgentAccount = serde_json::from_str(r#"{"signedIn":false}"#).unwrap();
        assert_eq!(legacy_out.health(), Health::SignedOut);
        // "needs re-login" is the loudest: a broken machine outranks an
        // unconfigured one.
        assert_eq!(
            Health::SignedOut.worse(Health::NeedsRelogin),
            Health::NeedsRelogin
        );
        assert_eq!(Health::Ok.worse(Health::SignedOut), Health::SignedOut);
        assert_eq!(Health::NeedsRelogin.worse(Health::Ok), Health::NeedsRelogin);
        assert_eq!(Health::Ok.worse(Health::Ok), Health::Ok);
        // Only the two negatives badge; `ok`/`unknown` say nothing.
        assert_eq!(Health::NeedsRelogin.badge_label(), Some("Needs re-login"));
        assert_eq!(Health::SignedOut.badge_label(), Some("Signed out"));
        assert_eq!(Health::Ok.badge_label(), None);
        assert_eq!(Health::Unknown.badge_label(), None);

        // Absent on the wire when unset; present as its token when set.
        let mut account = AgentAccount {
            signed_in: true,
            checked_at: "T".into(),
            ..AgentAccount::default()
        };
        assert!(!serde_json::to_string(&account).unwrap().contains("health"));
        account.health = Some(Health::Ok.as_str().to_string());
        assert!(serde_json::to_string(&account).unwrap().contains(r#""health":"ok""#));

        // A device with one healthy login badges `ok`, not the seeded
        // `unknown`; one broken profile makes the whole device loud.
        let mut map = AgentAccounts::new();
        map.insert("claude".into(), account.clone());
        assert_eq!(worst_health(&map), Health::Ok);
        map.get_mut("claude").unwrap().profiles = vec![
            AgentProfileEntry {
                id: "system".into(),
                signed_in: true,
                active: true,
                health: Some(Health::Ok.as_str().into()),
                ..AgentProfileEntry::default()
            },
            AgentProfileEntry {
                id: "0a1b2c3d".into(),
                signed_in: true,
                health: Some(Health::NeedsRelogin.as_str().into()),
                ..AgentProfileEntry::default()
            },
        ];
        assert_eq!(worst_health(&map), Health::NeedsRelogin);
        assert_eq!(worst_health(&AgentAccounts::new()), Health::Unknown);

        // And the heartbeat ships it: health alone moves the key.
        let healthy = map.clone();
        let mut broken = map.clone();
        broken.get_mut("claude").unwrap().health = Some(Health::NeedsRelogin.as_str().into());
        assert_ne!(accounts_key(&healthy), accounts_key(&broken));
        let mut unmonitored = map.clone();
        unmonitored.get_mut("claude").unwrap().profiles[1].unmonitored = true;
        assert_ne!(accounts_key(&map), accounts_key(&unmonitored));
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
