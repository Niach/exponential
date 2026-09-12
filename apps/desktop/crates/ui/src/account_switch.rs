//! EXP-849 (interface C/D) — continuing a run on ANOTHER agent account.
//!
//! A "switch" is not a new concept: it is a RESUME that names a different
//! account profile. The launcher does the work (`coding::prepare`'s resume
//! path copies the claude transcript into the target profile's `projects`
//! tree, re-resolves the MCP servers and chains the new row back through
//! `resumedFromId`); this module is the decision surface over it — which
//! accounts the run could move to, whether it may move right now, and the one
//! call that starts it.
//!
//! Three rules the UI must never relax, each enforced here rather than at the
//! call sites (there are several):
//!
//! * **Claude only.** Codex keeps one login per session by contract: its
//!   conversation lives inside the login's own rollout store, and switching
//!   would mean `codex logout` on a credential other machines share
//!   (interface E). Codex rows are OFFERED — so the reader sees why — and
//!   disabled with the reason.
//! * **Between turns only.** The agent has to be stopped and restarted, so a
//!   switch mid-turn would truncate exactly the output the reader is waiting
//!   for. The turn slot (EXP-848) is the authority.
//! * **The first message after a switch is slower and costs the NEW account
//!   more**, because the other account's transcript is replayed into it. Said
//!   once, where the switch is offered.

use gpui::{App, SharedString, Window};
use gpui_component::notification::Notification;
use gpui_component::WindowExt as _;

use coding::agent_accounts::Health;
use coding::CodingAgent;

/// EXP-849 — the cost note, said once where a switch is offered. Byte-identical
/// on all four clients.
pub(crate) const SWITCH_COST_NOTE: &str =
    "The first message after a switch is slower and spends more of the new account's budget — \
     the conversation is replayed into it.";

/// Why a run cannot move accounts right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SwitchBlocker {
    /// Not claude.
    Agent(&'static str),
    /// A turn is in flight.
    Working,
    /// Nothing to switch TO (one login on the machine, or only signed-out ones).
    NoOtherAccount,
}

impl SwitchBlocker {
    pub(crate) fn message(&self) -> String {
        match self {
            SwitchBlocker::Agent(label) => format!(
                "{label} keeps one account per session — start a new run on the other account."
            ),
            SwitchBlocker::Working => {
                "Wait for the agent to finish this turn before switching accounts.".to_string()
            }
            SwitchBlocker::NoOtherAccount => {
                "This machine has no other signed-in account for this agent.".to_string()
            }
        }
    }
}

/// One account a run could move to.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SwitchTarget {
    pub profile_id: String,
    pub label: String,
    /// The identity caption — the email, else the plan, else "signed in".
    pub caption: String,
    pub health: Health,
    pub usage: Option<coding::agent_usage::AgentUsage>,
    /// This is the account the run is ON.
    pub current: bool,
    /// `None` = offerable; `Some(reason)` = rendered disabled with the reason.
    pub blocked: Option<String>,
}

/// EXP-849 — the switch decision for one run: the accounts its HOST machine
/// holds for its agent, and whether the switch may happen now.
///
/// `rows` are [`crate::usage_bar::agent_profile_usage_rows`] already filtered
/// to nothing in particular — this picks the host device's rows for `agent`
/// out of them, so the numbers shown beside each account are the same ones the
/// Accounts page shows.
pub(crate) fn switch_targets(
    rows: &[crate::usage_bar::AgentProfileUsageRow],
    device_id: &str,
    agent: CodingAgent,
    current_account: Option<&str>,
    working: bool,
) -> (Vec<SwitchTarget>, Option<SwitchBlocker>) {
    let current_account = current_account
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .unwrap_or(coding::SYSTEM_PROFILE);
    let agent_blocker = (agent != CodingAgent::Claude).then(|| SwitchBlocker::Agent(agent.label()));
    let mut targets: Vec<SwitchTarget> = rows
        .iter()
        .filter(|row| row.device_id == device_id && row.agent == agent.id())
        .map(|row| {
            let current = row.profile_id == current_account;
            // A signed-out account is listed (it is an account on this
            // machine, and the reader may want to know) but never offered:
            // resuming into it would fail at the agent's first request.
            let blocked = agent_blocker
                .as_ref()
                .map(SwitchBlocker::message)
                .or_else(|| {
                    (!row.signed_in).then(|| {
                        "Not signed in on this machine — sign in there first.".to_string()
                    })
                })
                .or_else(|| {
                    (row.health == Health::NeedsRelogin).then(|| {
                        "This login was refused the last time it was used — sign in again."
                            .to_string()
                    })
                })
                .or_else(|| working.then(|| SwitchBlocker::Working.message()));
            SwitchTarget {
                profile_id: row.profile_id.clone(),
                label: row.profile_label.clone(),
                caption: caption_of(row),
                health: row.health,
                usage: row.usage.clone(),
                current,
                // The account the run is already on is never a target.
                blocked: if current { None } else { blocked },
            }
        })
        .collect();
    // The current account first, then by label — a heartbeat cannot reorder
    // the sheet under the pointer.
    targets.sort_by(|a, b| {
        b.current
            .cmp(&a.current)
            .then_with(|| a.label.cmp(&b.label))
            .then_with(|| a.profile_id.cmp(&b.profile_id))
    });
    let blocker = agent_blocker.or_else(|| {
        if targets
            .iter()
            .any(|target| !target.current && target.blocked.is_none())
        {
            None
        } else if working
            && targets.iter().any(|target| {
                !target.current && target.blocked.as_deref() == Some(&SwitchBlocker::Working.message())
            })
        {
            Some(SwitchBlocker::Working)
        } else {
            Some(SwitchBlocker::NoOtherAccount)
        }
    });
    (targets, blocker)
}

fn caption_of(row: &crate::usage_bar::AgentProfileUsageRow) -> String {
    if !row.signed_in {
        return "Not signed in".to_string();
    }
    row.email
        .clone()
        .or_else(|| row.plan.clone())
        .unwrap_or_else(|| "signed in".to_string())
}

/// EXP-849 — start the switch: a RESUME of `session_id` on `profile_id`.
///
/// A live run is STOPPED first (the agent cannot be in two processes in one
/// worktree) and the resume fires once its row is gone; an already-ended run
/// resumes straight away. A run this machine does not host goes over the relay
/// as a resume naming the account, exactly like a remote start.
pub(crate) fn switch_to(
    session_id: String,
    device_id: Option<String>,
    local: bool,
    profile_id: String,
    window: &mut Window,
    cx: &mut App,
) {
    if !local {
        let Some(device_id) = device_id.filter(|id| !id.is_empty()) else {
            window.push_notification(
                Notification::error("This run's machine is unknown — it cannot be resumed."),
                cx,
            );
            return;
        };
        crate::session_screen::resume_remote_on_account(
            session_id,
            device_id,
            Some(profile_id),
            window,
            cx,
        );
        return;
    }
    let live = crate::coding_flow::LocalSessions::global(cx)
        .read(cx)
        .session_for_id(&session_id)
        .is_some();
    if !live {
        crate::action_run::resume_run_on_account(
            session_id,
            Some(window.window_handle()),
            false,
            coding::LaunchOrigin::Local,
            Some(profile_id),
            cx,
        );
        return;
    }
    // Stop, then resume on the other account. The stop is the engine's own end
    // sequence (it ends the row and deregisters), so the resume waits for the
    // registration to go rather than racing it — a resume that started first
    // would hit the one-session-per-issue guard against the run it replaces.
    if let Some(session) = crate::coding_flow::LocalSessions::global(cx)
        .read(cx)
        .session_for_id(&session_id)
    {
        session.host.session.kill("ended");
    }
    let handle = window.window_handle();
    cx.spawn(async move |cx| {
        // ~10 s at 100 ms: an engine teardown is sub-second; past that the
        // switch gives up and says so rather than launching into a worktree
        // something may still hold.
        for _ in 0..100 {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(100))
                .await;
            let gone = cx.update(|cx| {
                crate::coding_flow::LocalSessions::global(cx)
                    .read(cx)
                    .session_for_id(&session_id)
                    .is_none()
            });
            if gone {
                let _ = handle.update(cx, |_, window, cx| {
                    crate::action_run::resume_run_on_account(
                        session_id.clone(),
                        Some(window.window_handle()),
                        false,
                        coding::LaunchOrigin::Local,
                        Some(profile_id.clone()),
                        cx,
                    );
                });
                return;
            }
        }
        let _ = handle.update(cx, |_, window, cx| {
            window.push_notification(
                Notification::error(SharedString::from(
                    "The run did not stop in time — switch accounts again once it has.",
                )),
                cx,
            );
        });
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usage_bar::AgentProfileUsageRow;

    fn row(profile: &str, signed_in: bool, health: Health) -> AgentProfileUsageRow {
        AgentProfileUsageRow {
            key: format!("dev-1:claude:{profile}"),
            device_id: "dev-1".into(),
            device_label: "Studio".into(),
            mine: true,
            online: true,
            agent: "claude".into(),
            profile_id: profile.into(),
            profile_label: profile.into(),
            active: profile == coding::SYSTEM_PROFILE,
            signed_in,
            email: Some(format!("{profile}@acme.test")),
            plan: None,
            usage: None,
            checked_at: None,
            health,
            unmonitored: false,
        }
    }

    /// The three rules, each as its own refusal: claude-only, between turns
    /// only, and something to switch TO.
    #[test]
    fn switch_targets_enforce_the_three_rules() {
        let rows = vec![
            row(coding::SYSTEM_PROFILE, true, Health::Ok),
            row("0a1b2c3d", true, Health::Ok),
            row("deadbeef", false, Health::SignedOut),
            row("badc0ffe", true, Health::NeedsRelogin),
        ];

        // Idle claude on the ambient login: the other healthy account is
        // offerable, the broken ones are listed with their reason.
        let (targets, blocker) = switch_targets(&rows, "dev-1", CodingAgent::Claude, None, false);
        assert_eq!(blocker, None);
        assert_eq!(targets.len(), 4);
        assert!(targets[0].current, "the run's own account leads");
        assert_eq!(targets[0].profile_id, coding::SYSTEM_PROFILE);
        let by_id = |id: &str| targets.iter().find(|t| t.profile_id == id).unwrap().clone();
        assert_eq!(by_id("0a1b2c3d").blocked, None);
        assert!(by_id("deadbeef").blocked.is_some(), "signed out");
        assert!(by_id("badc0ffe").blocked.is_some(), "revoked");

        // Mid-turn: every target is disabled and the sheet says why.
        let (targets, blocker) = switch_targets(&rows, "dev-1", CodingAgent::Claude, None, true);
        assert_eq!(blocker, Some(SwitchBlocker::Working));
        assert!(targets
            .iter()
            .filter(|target| !target.current)
            .all(|target| target.blocked.is_some()));

        // Codex: refused outright, whatever it holds.
        let codex: Vec<AgentProfileUsageRow> = rows
            .iter()
            .cloned()
            .map(|mut row| {
                row.agent = "codex".into();
                row
            })
            .collect();
        let (targets, blocker) = switch_targets(&codex, "dev-1", CodingAgent::Codex, None, false);
        assert_eq!(blocker, Some(SwitchBlocker::Agent(CodingAgent::Codex.label())));
        assert!(targets
            .iter()
            .filter(|target| !target.current)
            .all(|target| target.blocked.is_some()));

        // One login only: nothing to switch to.
        let (_, blocker) = switch_targets(
            &rows[..1],
            "dev-1",
            CodingAgent::Claude,
            None,
            false,
        );
        assert_eq!(blocker, Some(SwitchBlocker::NoOtherAccount));

        // Another machine's rows are never offered here.
        let (targets, _) = switch_targets(&rows, "dev-2", CodingAgent::Claude, None, false);
        assert!(targets.is_empty());

        // The run's CURRENT account is the one it records, not the default.
        let (targets, _) =
            switch_targets(&rows, "dev-1", CodingAgent::Claude, Some("0a1b2c3d"), false);
        assert!(targets[0].current && targets[0].profile_id == "0a1b2c3d");
    }
}
