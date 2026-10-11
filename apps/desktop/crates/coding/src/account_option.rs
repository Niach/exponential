//! EXP-872 — the ONE account option model every launch surface offers, the
//! desktop mirror of `apps/web/src/lib/accounts/account-option.ts`.
//!
//! One flattened list REPLACES the agent picker + the account picker: there
//! is no separate agent pick any more, the list is every signed-in login the
//! machine reports across both contract agents, and picking one IMPLIES its
//! agent. The rules (locked by the ×4 fixture in this file's tests, the same
//! names as `account-option.test.ts`):
//!
//! * one option per signed-in login (`agent_accounts[agent].profiles`, the
//!   ONLY rows: an agent that reports no profiles yields none); retired
//!   agent ids (`pi`) never produce one;
//! * the label is ALWAYS the brand mark + the EMAIL. A login reported
//!   without an address shows its plan; with neither, its profile id;
//! * EXP-1158: the LAST USED login leads, marked by ORDER (it is first) and
//!   by a check, not by a label: `defaultAgent`'s active login, else the
//!   first contract agent's active login, else the first row. The rest follow
//!   in `sort_device_logins` order (`ui::usage_bar`, web `sortDeviceLogins`),
//!   ties in the order the device sent them;
//! * `limits` are FRACTIONS 0..1 off the usage windows (`percent / 100`):
//!   `five_hour` = the `session` window, `week` = the `weekly` window,
//!   `model` = the FIRST per-model window. A login with no usage report has
//!   no limits at all.

use crate::agent::CodingAgent;
use crate::agent_accounts::{AgentAccounts, Health};
use crate::agent_usage::{AgentUsage, AgentUsageMap};

/// The ≥ threshold (as a percent) a login has to reach for its row to lead
/// the healthy ones — `ui::usage_bar::DANGER_PERCENT`'s twin, kept here so
/// the ordering stays a pure function of this crate.
const DANGER_PERCENT: u8 = 95;

/// The per-model window of a login's limits: which model, and how much of
/// its own window is spent (0..1).
#[derive(Clone, Debug, PartialEq)]
pub struct AccountModelLimit {
    /// The window's own name as the agent reported it (`Opus`); the picker
    /// lower-cases it for the bar label.
    pub label: String,
    pub used: f32,
}

/// One login's three usage fractions. Absent entirely for a login the device
/// reported no windows for.
#[derive(Clone, Debug, PartialEq)]
pub struct AccountLimits {
    pub five_hour: f32,
    pub week: f32,
    pub model: Option<AccountModelLimit>,
}

/// One signed-in login, as every launch surface offers it.
#[derive(Clone, Debug, PartialEq)]
pub struct AccountOption {
    /// The profile id (`agent_profiles`), what a launch passes as `account`.
    pub id: String,
    /// Picking the option picks this agent: there is no separate agent pick.
    pub agent: CodingAgent,
    /// What the row SAYS beside the brand mark — email, else plan, else the
    /// profile id.
    pub email: String,
    /// Exactly one option per device is the last used login; it is also
    /// listed first.
    pub is_last_used: bool,
    /// EXP-849: the device's verdict on the credential.
    pub health: Health,
    pub limits: Option<AccountLimits>,
}

impl AccountOption {
    /// `<agent>:<profileId>` — the ONE string a picker can carry for an
    /// option, the agent naming which index the id is from.
    pub fn account_option_key(&self) -> String {
        format!("{}:{}", self.agent.id(), self.id)
    }

    /// What a launch puts on the wire: the id VERBATIM (an absent account
    /// means "the last used login"). Always `Some`; the `Option` is the
    /// launch slot's own type.
    pub fn wire_account(&self) -> Option<String> {
        Some(self.id.clone())
    }
}

/// Read an [`AccountOption::account_option_key`] back. `None` for anything
/// that is not `<agent>:<id>` with both halves non-empty.
pub fn parse_account_option_key(key: &str) -> Option<(String, String)> {
    let at = key.find(':')?;
    if at == 0 || at == key.len() - 1 {
        return None;
    }
    Some((key[..at].to_string(), key[at + 1..].to_string()))
}

/// The option a launch surface should START on: the last used login, else
/// the first option. `None` for a device that reports no login at all.
pub fn last_used_account_option(options: &[AccountOption]) -> Option<&AccountOption> {
    options
        .iter()
        .find(|option| option.is_last_used)
        .or_else(|| options.first())
}

/// EXP-1278: the new device's option for the login picked on the previous
/// one — same agent, same email address (case-insensitive). Profile ids are
/// per machine, so the address is the identity; a row with no address (a
/// plan, "No email", the agent-named ambient fallback) never carries. `None`
/// = the new device settles on its own last used login.
pub fn carried_account_option<'a>(
    options: &'a [AccountOption],
    previous: Option<&AccountOption>,
) -> Option<&'a AccountOption> {
    let previous = previous.filter(|option| option.email.contains('@'))?;
    let email = previous.email.trim().to_lowercase();
    options.iter().find(|option| {
        option.agent == previous.agent && option.email.trim().to_lowercase() == email
    })
}

/// One login row before it becomes an option — the desktop twin of the web's
/// `AgentProfileUsageRow`, trimmed to what the ordering and the option need.
#[derive(Clone, Debug)]
struct LoginRow {
    agent: CodingAgent,
    profile_id: String,
    active: bool,
    health: Health,
    email: Option<String>,
    plan: Option<String>,
    usage: Option<AgentUsage>,
}

/// `Some(trimmed)` for a non-blank string — the wire uses "absent" and "empty
/// string" interchangeably.
fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn fraction(percent: Option<u8>) -> f32 {
    f32::from(percent.unwrap_or(0).min(100)) / 100.
}

fn peak_percent(usage: Option<&AgentUsage>) -> u8 {
    usage
        .map(|usage| usage.windows.iter().map(|window| window.percent).max().unwrap_or(0))
        .unwrap_or(0)
}

/// Attention-first ordering (web `attentionRank`): a dead credential leads,
/// then a login at or over [`DANGER_PERCENT`], then the rest. Every row here
/// is signed in, so the signed-out bucket never fires.
fn attention_rank(row: &LoginRow) -> u8 {
    if row.health == Health::NeedsRelogin {
        return 0;
    }
    if peak_percent(row.usage.as_ref()) >= DANGER_PERCENT {
        return 1;
    }
    2
}

fn agent_rank(agent: CodingAgent) -> usize {
    CodingAgent::ALL
        .iter()
        .position(|known| *known == agent)
        .unwrap_or(usize::MAX)
}

/// Every SIGNED-IN login one machine reports, in `sort_device_logins` order:
/// contract agent order, that agent's ACTIVE login, [`attention_rank`], then
/// the order the device sent them (a stable sort).
fn login_rows(accounts: &AgentAccounts, usage: &AgentUsageMap) -> Vec<LoginRow> {
    let mut rows: Vec<LoginRow> = Vec::new();
    // The union of "has an account" and "reported usage", minus every id
    // outside the contract (EXP-849: a retired `pi` row must never produce
    // an option).
    let mut agent_ids: std::collections::BTreeSet<&str> =
        accounts.keys().map(String::as_str).collect();
    agent_ids.extend(usage.keys().map(String::as_str));
    for agent_id in agent_ids {
        let Some(agent) = CodingAgent::parse(agent_id) else {
            continue;
        };
        let profiles = accounts
            .get(agent_id)
            .map(|account| account.profiles.as_slice())
            .unwrap_or(&[]);
        for profile in profiles {
            if !profile.signed_in {
                continue;
            }
            // The active profile's numbers ride BOTH the profile entry and
            // the pre-profile `agentUsage[agent]` slot; prefer its own.
            let profile_usage = profile
                .usage
                .clone()
                .or_else(|| profile.active.then(|| usage.get(agent_id).cloned()).flatten());
            rows.push(LoginRow {
                agent,
                profile_id: profile.id.clone(),
                active: profile.active,
                health: profile.health(),
                email: non_empty(profile.email.as_deref()),
                plan: non_empty(profile.plan.as_deref()),
                usage: profile_usage,
            });
        }
    }
    rows.sort_by(|a, b| {
        agent_rank(a.agent)
            .cmp(&agent_rank(b.agent))
            .then_with(|| b.active.cmp(&a.active))
            .then_with(|| attention_rank(a).cmp(&attention_rank(b)))
    });
    rows
}

fn option_limits(row: &LoginRow) -> Option<AccountLimits> {
    let usage = row.usage.as_ref()?;
    if usage.windows.is_empty() {
        return None;
    }
    let find = |key: &str| {
        usage
            .windows
            .iter()
            .find(|window| window.key == key)
            .map(|window| window.percent)
    };
    let model = usage
        .windows
        .iter()
        .find(|window| window.key.starts_with("model:"))
        .map(|window| AccountModelLimit {
            label: window.label.clone(),
            used: fraction(Some(window.percent)),
        });
    Some(AccountLimits {
        five_hour: fraction(find("session")),
        week: fraction(find("weekly")),
        model,
    })
}

/// THE flattener: one machine's reported logins as the options every launch
/// surface offers, the last used login first. See the module header for the
/// rules; `default_agent` is the machine's stored `launch_defaults
/// .defaultAgent` (the last used agent).
pub fn flatten_accounts(
    accounts: &AgentAccounts,
    usage: &AgentUsageMap,
    default_agent: Option<&str>,
) -> Vec<AccountOption> {
    let rows = login_rows(accounts, usage);
    if rows.is_empty() {
        return Vec::new();
    }
    let configured = default_agent.and_then(CodingAgent::parse);
    let active_of = |agent: Option<CodingAgent>| {
        agent.and_then(|agent| rows.iter().position(|row| row.agent == agent && row.active))
    };
    let last_used_at = active_of(configured)
        .or_else(|| CodingAgent::ALL.iter().find_map(|agent| active_of(Some(*agent))))
        .unwrap_or(0);

    let order =
        std::iter::once(last_used_at).chain((0..rows.len()).filter(|at| *at != last_used_at));
    order
        .map(|at| {
            let row = &rows[at];
            AccountOption {
                id: row.profile_id.clone(),
                agent: row.agent,
                // EXP-1013: `accountName` — never the profile's id.
                email: crate::agent_accounts::account_name(
                    row.email.as_deref(),
                    row.plan.as_deref(),
                ),
                is_last_used: at == last_used_at,
                health: row.health,
                limits: option_limits(row),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The fixture ×4 (`account-option.test.ts` `ACCOUNT_FIXTURE`): two
    /// claude logins (work = active, home = a dead credential), one codex
    /// login, and a second codex profile that is signed out. The last used
    /// agent is codex.
    fn fixture() -> AgentAccounts {
        serde_json::from_value(json!({
            "claude": {
                "signedIn": true,
                "email": "work@x.test",
                "profiles": [
                    {
                        "id": "work",
                        "signedIn": true,
                        "email": "work@x.test",
                        "active": true,
                        "health": "ok",
                        "usage": {
                            "fetchedAt": "2026-09-19T10:00:00Z",
                            "windows": [
                                { "key": "session", "label": "5h", "percent": 40 },
                                { "key": "weekly", "label": "Week", "percent": 85 },
                                { "key": "model:opus", "label": "Opus", "percent": 10 }
                            ]
                        }
                    },
                    {
                        "id": "home",
                        "signedIn": true,
                        "email": "home@x.test",
                        "health": "needs_relogin"
                    }
                ]
            },
            "codex": {
                "signedIn": true,
                "profiles": [
                    {
                        "id": "main",
                        "signedIn": true,
                        "email": "codex@x.test",
                        "active": true,
                        "usage": {
                            "windows": [
                                { "key": "session", "label": "5h", "percent": 5 },
                                { "key": "weekly", "label": "Week", "percent": 50 }
                            ]
                        }
                    },
                    { "id": "spare", "signedIn": false, "active": false }
                ]
            },
            // A retired agent still sitting in a synced row (EXP-849).
            "pi": { "signedIn": true, "email": "pi@x.test" }
        }))
        .expect("the fixture parses")
    }

    fn keys(options: &[AccountOption]) -> Vec<String> {
        options
            .iter()
            .map(|option| option.account_option_key())
            .collect()
    }

    fn flatten(default_agent: Option<&str>) -> Vec<AccountOption> {
        flatten_accounts(&fixture(), &AgentUsageMap::new(), default_agent)
    }

    #[test]
    fn yields_one_option_per_signed_in_login_across_both_agents() {
        assert_eq!(
            keys(&flatten(Some("codex"))),
            vec!["codex:main", "claude:work", "claude:home"]
        );
    }

    #[test]
    fn labels_every_option_by_email() {
        let options = flatten(Some("codex"));
        assert_eq!(
            options
                .iter()
                .map(|option| option.email.as_str())
                .collect::<Vec<_>>(),
            vec!["codex@x.test", "work@x.test", "home@x.test"]
        );
    }

    #[test]
    fn puts_the_last_used_login_first_and_marks_exactly_one_option() {
        // defaultAgent = codex with an active codex login → that login is
        // options[0] and the only `is_last_used`.
        let options = flatten(Some("codex"));
        assert_eq!(options[0].agent, CodingAgent::Codex);
        assert_eq!(options[0].id, "main");
        assert!(options[0].is_last_used);
        assert_eq!(options.iter().filter(|option| option.is_last_used).count(), 1);
        assert_eq!(last_used_account_option(&options), Some(&options[0]));
    }

    #[test]
    fn falls_back_to_the_first_contract_agents_active_login_when_no_last_used_agent_is_set() {
        let options = flatten(None);
        assert_eq!(options[0].agent, CodingAgent::Claude);
        assert_eq!(options[0].id, "work");
        assert!(options[0].is_last_used);
        assert_eq!(options.iter().filter(|option| option.is_last_used).count(), 1);
        // A last used agent with no active login falls back the same way.
        let stale = flatten(Some("pi"));
        assert_eq!(stale[0].agent, CodingAgent::Claude);
        assert_eq!(stale[0].id, "work");
    }

    #[test]
    fn carries_the_agent_on_the_option_so_a_pick_implies_it() {
        let options = flatten(Some("codex"));
        let by_id = |id: &str| {
            options
                .iter()
                .find(|option| option.id == id)
                .expect("the option is offered")
        };
        assert_eq!(by_id("main").agent, CodingAgent::Codex);
        assert_eq!(by_id("home").agent, CodingAgent::Claude);
        assert_eq!(by_id("home").health, Health::NeedsRelogin);
        assert_eq!(
            parse_account_option_key(&options[0].account_option_key()),
            Some(("codex".to_string(), "main".to_string()))
        );
        assert_eq!(parse_account_option_key("nope"), None);
        // The option's id rides the wire verbatim.
        assert_eq!(by_id("main").wire_account(), Some("main".to_string()));
    }

    #[test]
    fn derives_limits_as_0_1_fractions_from_the_session_weekly_and_first_model_windows() {
        let options = flatten(Some("codex"));
        let by_id = |id: &str| {
            options
                .iter()
                .find(|option| option.id == id)
                .expect("the option is offered")
        };
        assert_eq!(
            by_id("work").limits,
            Some(AccountLimits {
                five_hour: 0.4,
                week: 0.85,
                model: Some(AccountModelLimit {
                    label: "Opus".to_string(),
                    used: 0.1,
                }),
            })
        );
        // Codex reports no per-model window: no `model` at all.
        assert_eq!(
            by_id("main").limits,
            Some(AccountLimits {
                five_hour: 0.05,
                week: 0.5,
                model: None,
            })
        );
    }

    #[test]
    fn omits_limits_for_a_login_with_no_usage_report() {
        let options = flatten(Some("codex"));
        assert_eq!(
            options
                .iter()
                .find(|option| option.id == "home")
                .and_then(|option| option.limits.clone()),
            None
        );
    }

    #[test]
    fn shows_the_plan_for_a_login_the_device_reports_without_an_address() {
        let accounts: AgentAccounts = serde_json::from_value(json!({
            "claude": {
                "signedIn": true,
                "profiles": [
                    { "id": "p1", "signedIn": true, "plan": "Max", "active": true },
                    { "id": "p2", "signedIn": true }
                ]
            }
        }))
        .unwrap();
        let options = flatten_accounts(&accounts, &AgentUsageMap::new(), None);
        assert_eq!(
            options
                .iter()
                .map(|option| option.email.as_str())
                .collect::<Vec<_>>(),
            vec!["Max", "No email"]
        );
    }

    /// The ambient login is never an option: an agent that reports no
    /// profiles offers nothing, whatever its top-level fields say.
    #[test]
    fn yields_nothing_for_a_device_that_reports_no_profiles() {
        let accounts: AgentAccounts =
            serde_json::from_value(json!({ "claude": { "signedIn": true, "email": "solo@x.test" } }))
                .unwrap();
        let usage: AgentUsageMap = serde_json::from_value(json!({
            "claude": { "windows": [{ "key": "session", "label": "5h", "percent": 20 }] }
        }))
        .unwrap();
        assert!(flatten_accounts(&accounts, &usage, None).is_empty());
    }

    #[test]
    fn skips_signed_out_logins_and_retired_agents() {
        let options = flatten(Some("codex"));
        assert!(!options.iter().any(|option| option.id == "spare"));
        assert!(!options
            .iter()
            .any(|option| option.account_option_key().starts_with("pi:")));
        assert!(flatten_accounts(
            &AgentAccounts::new(),
            &AgentUsageMap::new(),
            None
        )
        .is_empty());
        assert_eq!(last_used_account_option(&[]), None);
    }

    #[test]
    fn carries_a_pick_to_another_device_by_agent_and_email() {
        let options = flatten(Some("codex"));
        let previous = |agent: CodingAgent, email: &str| AccountOption {
            id: "elsewhere".into(),
            agent,
            email: email.into(),
            is_last_used: false,
            health: Health::Unknown,
            limits: None,
        };
        let home = previous(CodingAgent::Claude, " HOME@x.test");
        assert_eq!(
            carried_account_option(&options, Some(&home)).map(|option| option.id.as_str()),
            Some("home")
        );
        // Same address, other agent: a different login.
        let codex = previous(CodingAgent::Codex, "work@x.test");
        assert_eq!(carried_account_option(&options, Some(&codex)), None);
        // No address, nothing to match on.
        let bare = previous(CodingAgent::Claude, "No email");
        assert_eq!(carried_account_option(&options, Some(&bare)), None);
        assert_eq!(carried_account_option(&options, None), None);
    }
}
