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

use gpui::{
    div, AnyElement, App, IntoElement, ParentElement, SharedString, Styled as _, Window,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::notification::Notification;
use gpui_component::{
    h_flex, v_flex, ActiveTheme as _, Disableable as _, Sizable as _, WindowExt as _,
};

use crate::controls::WebText as _;

use coding::agent_accounts::Health;
use coding::CodingAgent;

// ---------------------------------------------------------------------------
// EXP-849 — the copy, byte-identical ×4
// ---------------------------------------------------------------------------
//
// Hand-mirrored with `SessionAccountSwitch` (Android
// `domain/SessionAccountSwitch.kt`, iOS `ExpCore/Domain/SessionAccountSwitch`)
// and the web `session-account-switch.tsx`: same section title, same control
// labels, same refusal sentences. A refusal always names the thing the person
// can change, and it is shown ON the disabled control rather than hiding it, so
// the switch never silently disappears mid-run.

/// The account block's title.
pub(crate) const SECTION_TITLE: &str = "Accounts";

/// The primary control on an account row.
pub(crate) const SWITCH_LABEL: &str = "Switch to this account";

/// The rate-limit wall's PRIMARY button.
pub(crate) const WALL_SWITCH_LABEL: &str = "Switch account";

/// What a switch costs, said ONCE where the switch is offered.
pub(crate) const COST_NOTE: &str =
    "The run continues under the other account. Re-reading the transcript once costs tokens.";

/// The continuation byline a resumed run's screen carries.
pub(crate) const CONTINUATION_NOTE: &str = "Continues an earlier run";

/// What that continuation cost, said ONCE on the new run.
pub(crate) const CONTINUATION_COST_NOTE: &str =
    "The agent re-read the transcript once to pick it up — a one-time cost.";

const REASON_AGENT: &str = "Only claude can switch accounts during a run.";
const REASON_OFFLINE: &str = "The machine is offline.";
const REASON_NO_CAP: &str = "Update the app on that machine to switch accounts.";
const REASON_BUSY: &str = "The agent is working — switching waits for the turn to finish.";
const REASON_SIGNED_OUT: &str = "Sign in to this account on that machine first.";
const REASON_NEEDS_RELOGIN: &str = "This account needs a re-login on that machine.";

/// Why a run cannot move accounts right now — the note under the rows, when
/// not one of them can be taken.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SwitchBlocker {
    /// Not claude.
    Agent,
    /// The host machine is not reporting.
    Offline,
    /// The host runs a build that does not honour `account` on a resume
    /// (EXP-849's `account-switch` cap).
    NoCap,
    /// A turn is in flight.
    Working,
    /// Nothing to switch TO (one login on the machine, or only broken ones).
    NoOtherAccount,
}

impl SwitchBlocker {
    pub(crate) fn message(&self) -> &'static str {
        match self {
            SwitchBlocker::Agent => REASON_AGENT,
            SwitchBlocker::Offline => REASON_OFFLINE,
            SwitchBlocker::NoCap => REASON_NO_CAP,
            SwitchBlocker::Working => REASON_BUSY,
            // Desktop-only state: the ×4 set has no sentence for it (the other
            // clients simply list nothing), and "there is no other account" is
            // about the machine, not about this run.
            SwitchBlocker::NoOtherAccount => {
                "This machine has no other signed-in account for this agent."
            }
        }
    }
}

/// One account a run could move to.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SwitchTarget {
    pub profile_id: String,
    pub label: String,
    /// The identity caption — the email, else the plan, else the label.
    pub caption: String,
    pub health: Health,
    pub usage: Option<coding::agent_usage::AgentUsage>,
    /// This is the account the run is ON.
    pub current: bool,
    /// `None` = offerable; `Some(reason)` = rendered disabled with the reason.
    pub blocked: Option<String>,
}

impl SwitchTarget {
    /// EXP-863 — the refusal that belongs UNDER this row and nowhere else:
    /// the account's own state (signed out, needs a re-login). Every other
    /// reason is about the RUN (agent, host, build, turn) and is said once,
    /// in the sheet's footer ([`footer_note`]) — a row must never repeat it.
    pub(crate) fn row_refusal(&self) -> Option<&str> {
        self.blocked
            .as_deref()
            .filter(|reason| *reason == REASON_SIGNED_OUT || *reason == REASON_NEEDS_RELOGIN)
    }
}

/// EXP-863 — the ONE note under the sheet: the global blocker when a switch
/// is refused for every other account, else the one-time cost of taking one.
/// `None` when there is no other account at all (the Accounts section is
/// hidden then, and a cost note for a switch nobody can take is noise).
pub(crate) fn footer_note(
    targets: &[SwitchTarget],
    blocker: Option<&SwitchBlocker>,
) -> Option<&'static str> {
    if !targets.iter().any(|target| !target.current) {
        return None;
    }
    Some(match blocker {
        Some(blocker) => blocker.message(),
        None => COST_NOTE,
    })
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
    can_switch: bool,
    working: bool,
) -> (Vec<SwitchTarget>, Option<SwitchBlocker>) {
    let current_account = current_account
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .unwrap_or(coding::SYSTEM_PROFILE);
    // One machine's rows all carry its own liveness; an empty set says nothing
    // about the host, so it falls through to "nothing to switch to".
    let host_online = rows
        .iter()
        .filter(|row| row.device_id == device_id)
        .all(|row| row.online);
    let mut targets: Vec<SwitchTarget> = rows
        .iter()
        .filter(|row| row.device_id == device_id && row.agent == agent.id())
        .map(|row| {
            let current = row.profile_id == current_account;
            // The refusal ORDER is the ×4 one (`SessionAccountSwitch.refusal`):
            // the run's own facts first (agent, host, build, turn), the
            // account's own last. A broken login is still LISTED — it is an
            // account on this machine, and the reader wants to know — just
            // never offered: resuming into it would fail at the first request.
            let blocked = (agent != CodingAgent::Claude)
                .then_some(REASON_AGENT)
                .or_else(|| (!row.online).then_some(REASON_OFFLINE))
                .or_else(|| (!can_switch).then_some(REASON_NO_CAP))
                .or_else(|| working.then_some(REASON_BUSY))
                .or_else(|| (row.health == Health::NeedsRelogin).then_some(REASON_NEEDS_RELOGIN))
                .or_else(|| {
                    (!row.signed_in || row.health == Health::SignedOut)
                        .then_some(REASON_SIGNED_OUT)
                });
            SwitchTarget {
                profile_id: row.profile_id.clone(),
                label: row.profile_label.clone(),
                caption: caption_of(row),
                health: row.health,
                usage: row.usage.clone(),
                current,
                // The account the run is already on is never a target: the row
                // says "in use" instead of refusing a switch nobody asked for.
                blocked: if current {
                    None
                } else {
                    blocked.map(str::to_string)
                },
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
    // The note under the rows names the FIRST thing standing in the way, in the
    // same order the rows themselves refuse.
    let others = targets.iter().any(|target| !target.current);
    let blocker = if agent != CodingAgent::Claude {
        Some(SwitchBlocker::Agent)
    } else if targets
        .iter()
        .any(|target| !target.current && target.blocked.is_none())
    {
        None
    } else if others && !host_online {
        Some(SwitchBlocker::Offline)
    } else if others && !can_switch {
        Some(SwitchBlocker::NoCap)
    } else if others && working {
        Some(SwitchBlocker::Working)
    } else {
        Some(SwitchBlocker::NoOtherAccount)
    };
    (targets, blocker)
}

/// EXP-862 — the account's identity, never its status: the email, else the
/// plan (an agent may report a provider, never an address), else the login's
/// own label. A signed-out account used to title itself "Not signed in", which
/// said what the refusal under the rows already says and buried the only
/// identifying thing the row had (iOS `SessionAccountSwitch.caption`, Android
/// `SessionAccountOption.caption`, desktop `accounts_section::group_caption`).
fn caption_of(row: &crate::usage_bar::AgentProfileUsageRow) -> String {
    row.email
        .clone()
        .or_else(|| row.plan.clone())
        .unwrap_or_else(|| row.profile_label.clone())
}

/// EXP-849 — what the usage sheet needs to offer an account switch for ONE
/// run. Plain data: the sheet renders inside a popover closure, which has no
/// view to ask.
#[derive(Clone, Debug)]
pub(crate) struct SwitchContext {
    pub session_id: String,
    pub device_id: Option<String>,
    pub local: bool,
    pub agent: coding::CodingAgent,
    /// A turn is in flight — every target is disabled with the reason.
    pub working: bool,
}

impl SwitchContext {
    /// Whether the HOST could take the switch at all: this machine always can
    /// (the launcher runs right here), another of mine only on a build that
    /// honours `account` on a live-run resume (EXP-849's `account-switch`) —
    /// the server refuses the start otherwise.
    fn can_switch(&self, cx: &App) -> bool {
        if self.local {
            return true;
        }
        let Some(device_id) = self.device_id.as_deref() else {
            return false;
        };
        crate::queries::device_caps(cx, device_id)
            .iter()
            .any(|cap| cap == coding::doctor::ACCOUNT_SWITCH_CAP)
    }
}

impl SwitchContext {
    /// The run's recorded account, for a run THIS machine hosts. A remote run
    /// is assumed to be on its machine's default login: the row carries no
    /// account, and guessing one would mis-label the current row.
    fn current_account(&self, cx: &App) -> Option<String> {
        self.local
            .then(|| {
                coding::run_registry::get(
                    &crate::coding_flow::coding_data_dir(cx),
                    &self.session_id,
                )
            })
            .flatten()
            .and_then(|record| record.account())
    }

    /// EXP-863 — the switch decision for this run: every account its host
    /// holds for the agent (the current one included, flagged) and the global
    /// blocker. `None` when the machine reported no account at all for the
    /// agent (nothing to say, and nothing to switch), or the row has not
    /// synced its device yet.
    pub(crate) fn resolve(&self, cx: &App) -> Option<(Vec<SwitchTarget>, Option<SwitchBlocker>)> {
        let device_id = self.device_id.as_deref()?;
        let rows = crate::usage_bar::device_profile_rows(cx);
        let (targets, blocker) = switch_targets(
            &rows,
            device_id,
            self.agent,
            self.current_account(cx).as_deref(),
            self.can_switch(cx),
            self.working,
        );
        (!targets.is_empty()).then_some((targets, blocker))
    }

    /// EXP-863 — the rows of the sheet's "Accounts" section: ONLY the accounts
    /// the run is NOT on (the active one is the sheet's header), each with its
    /// caption + health, the Switch control, its dense usage cards and — for
    /// an account-level refusal only — the reason. `None` when there is no
    /// other account, so the caller omits the section.
    pub(crate) fn render_account_rows(
        &self,
        targets: &[SwitchTarget],
        cx: &App,
    ) -> Option<AnyElement> {
        let device_id = self.device_id.clone()?;
        let others: Vec<&SwitchTarget> = targets.iter().filter(|target| !target.current).collect();
        if others.is_empty() {
            return None;
        }
        let muted = cx.theme().muted_foreground;
        let mut block = v_flex().w_full().min_w_0().gap_2p5();
        for (index, target) in others.into_iter().enumerate() {
            let session_id = self.session_id.clone();
            let device = device_id.clone();
            let local = self.local;
            let profile_id = target.profile_id.clone();
            let offerable = target.blocked.is_none();
            let mut row = v_flex().w_full().min_w_0().gap_1().child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_1p5()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .child(SharedString::from(target.caption.clone())),
                    )
                    .children(crate::usage_bar::health_badge(target.health, cx))
                    // EXP-849: the control is DISABLED with its reason, never
                    // hidden — a switch that silently disappears mid-run
                    // reads as a bug, and the reason is the whole point.
                    .child(
                        Button::new(("session-use-account", index))
                            .ghost()
                            .cursor_pointer()
                            .xsmall()
                            .label(SWITCH_LABEL)
                            .disabled(!offerable)
                            .on_click(move |_, window, cx| {
                                switch_to(
                                    session_id.clone(),
                                    Some(device.clone()),
                                    local,
                                    profile_id.clone(),
                                    window,
                                    cx,
                                );
                            }),
                    ),
            );
            if let Some(usage) = target.usage.as_ref().filter(|usage| !usage.windows.is_empty()) {
                row = row.child(crate::usage_bar::render_usage_cards_dense(
                    self.agent,
                    usage,
                    chrono::Utc::now().timestamp(),
                    cx,
                ));
            }
            // EXP-863: only the ACCOUNT's own refusal sits on its row; a
            // run-level one (busy, offline, agent, build) is the footer's,
            // said once for the whole sheet.
            if let Some(reason) = target.row_refusal() {
                row = row.child(
                    div()
                        .text_2xs()
                        .text_color(muted)
                        .child(SharedString::from(reason.to_string())),
                );
            }
            block = block.child(row);
        }
        Some(block.into_any_element())
    }
}

impl SwitchContext {
    /// EXP-849 — the account block on its own (the rate-limit wall's "Switch
    /// account" popover, `steer_viewer`): the "Accounts" title, the OTHER
    /// accounts' rows and the one footer note ([`footer_note`]). `None` when
    /// the machine reported no account for the agent, or holds no other one
    /// — a popover that can only explain itself is worse than none. The
    /// session header's usage sheet composes the same pieces itself, with
    /// the active account and the context meter above them.
    pub(crate) fn render(&self, cx: &App) -> Option<AnyElement> {
        let (targets, blocker) = self.resolve(cx)?;
        let rows = self.render_account_rows(&targets, cx)?;
        let mut block = v_flex()
            .w_full()
            .min_w_0()
            .gap_2()
            .child(crate::usage_bar::sheet_section_title(SECTION_TITLE, cx))
            .child(rows);
        if let Some(note) = footer_note(&targets, blocker.as_ref()) {
            block = block.child(
                div()
                    .text_2xs()
                    .text_color(cx.theme().muted_foreground.opacity(0.8))
                    .child(note),
            );
        }
        Some(block.into_any_element())
    }
}

/// EXP-849 — start the switch: a RESUME of `session_id` on `profile_id`.
///
/// A live run is STOPPED first (the agent cannot be in two processes in one
/// worktree) and the resume fires once its row is gone; an already-ended run
/// resumes straight away. A run this machine does not host goes over the relay
/// as a resume naming the account, exactly like a remote start.
///
/// `profile_id` rides VERBATIM, `system` included: a start omits the ambient
/// login (there, `system` IS the absence of an account), but a switch may not —
/// the server reads the PRESENCE of `account` as "this resume is a switch" and
/// that is the only thing that lets a resume ride a LIVE run (×4
/// `SessionAccountSwitch.wireAccount`).
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
    let target = Some(window.window_handle());
    if !end_then_resume_on_account(session_id, profile_id, target, coding::LaunchOrigin::Local, cx)
    {
        window.push_notification(Notification::error(SharedString::from(REASON_BUSY)), cx);
    }
}

/// EXP-849 — the DEVICE side of a mid-run switch: continue `session_id` on
/// `profile_id` on THIS machine, whether or not the run is still live here.
/// The one body behind the local control AND a relay resume frame that names an
/// account (`steer_wiring`), so both paths end the run the same way.
///
/// A live run is ended FIRST — the agent cannot be in two processes in one
/// worktree — through the engine's own end sequence, which flips the row to
/// `ended_by: client` (never `merge`/`system`: nothing merged, and the person
/// asked). The resume waits for the registration to go rather than racing it: a
/// resume that started first would hit the one-session-per-issue guard against
/// the very run it replaces.
///
/// `false` = refused because the agent is MID-TURN; a switch then would truncate
/// exactly the output the reader is waiting for. The caller says so ([
/// `REASON_BUSY`] is the ×4 sentence).
pub(crate) fn end_then_resume_on_account(
    session_id: String,
    profile_id: String,
    target: Option<gpui::AnyWindowHandle>,
    origin: coding::LaunchOrigin,
    cx: &mut App,
) -> bool {
    let live = crate::coding_flow::LocalSessions::global(cx)
        .read(cx)
        .session_for_id(&session_id)
        .map(|session| session.host.session.clone());
    let Some(session) = live else {
        crate::action_run::resume_run_on_account(
            session_id,
            target,
            false,
            origin,
            Some(profile_id),
            cx,
        );
        return true;
    };
    // The turn slot is the authority on both sides of the wire (EXP-848); the
    // engine's own signal is the local half of it.
    if !session.turn_signal().is_idle() {
        return false;
    }
    session.kill("ended");
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
                let _ = cx.update(|cx| {
                    crate::action_run::resume_run_on_account(
                        session_id.clone(),
                        target,
                        false,
                        origin,
                        Some(profile_id.clone()),
                        cx,
                    );
                });
                return;
            }
        }
        let _ = cx.update(|cx| {
            crate::action_run::notify_target_error(
                target,
                "The run did not stop in time — switch accounts again once it has.",
                cx,
            );
        });
    })
    .detach();
    true
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
    /// only, and something to switch TO — plus the host facts (offline, a
    /// build that cannot resume), in the ×4 refusal order.
    #[test]
    fn switch_targets_enforce_the_three_rules() {
        let rows = vec![
            row(coding::SYSTEM_PROFILE, true, Health::Ok),
            row("0a1b2c3d", true, Health::Ok),
            row("deadbeef", false, Health::SignedOut),
            row("badc0ffe", true, Health::NeedsRelogin),
        ];

        // Idle claude on the ambient login: the other healthy account is
        // offerable, the broken ones are listed with their own sentence.
        let (targets, blocker) =
            switch_targets(&rows, "dev-1", CodingAgent::Claude, None, true, false);
        assert_eq!(blocker, None);
        assert_eq!(targets.len(), 4);
        assert!(targets[0].current, "the run's own account leads");
        assert_eq!(targets[0].profile_id, coding::SYSTEM_PROFILE);
        let by_id = |id: &str| targets.iter().find(|t| t.profile_id == id).unwrap().clone();
        assert_eq!(by_id("0a1b2c3d").blocked, None);
        assert_eq!(
            by_id("deadbeef").blocked.as_deref(),
            Some("Sign in to this account on that machine first.")
        );
        assert_eq!(
            by_id("badc0ffe").blocked.as_deref(),
            Some("This account needs a re-login on that machine.")
        );

        // Mid-turn: every target is disabled and the sheet says why, in the
        // ×4 sentence.
        let (targets, blocker) =
            switch_targets(&rows, "dev-1", CodingAgent::Claude, None, true, true);
        assert_eq!(blocker, Some(SwitchBlocker::Working));
        assert_eq!(
            SwitchBlocker::Working.message(),
            "The agent is working — switching waits for the turn to finish."
        );
        assert!(targets
            .iter()
            .filter(|target| !target.current)
            .all(|target| target.blocked.as_deref() == Some(SwitchBlocker::Working.message())));

        // Codex: refused outright, whatever it holds.
        let codex: Vec<AgentProfileUsageRow> = rows
            .iter()
            .cloned()
            .map(|mut row| {
                row.agent = "codex".into();
                row
            })
            .collect();
        let (targets, blocker) =
            switch_targets(&codex, "dev-1", CodingAgent::Codex, None, true, false);
        assert_eq!(blocker, Some(SwitchBlocker::Agent));
        assert_eq!(
            SwitchBlocker::Agent.message(),
            "Only claude can switch accounts during a run."
        );
        assert!(targets
            .iter()
            .filter(|target| !target.current)
            .all(|target| target.blocked.is_some()));

        // An offline host, and a host whose build cannot take the switch: the
        // run's own facts outrank the accounts' (the ×4 order).
        let offline: Vec<AgentProfileUsageRow> = rows
            .iter()
            .cloned()
            .map(|mut row| {
                row.online = false;
                row
            })
            .collect();
        let (targets, blocker) =
            switch_targets(&offline, "dev-1", CodingAgent::Claude, None, true, false);
        assert_eq!(blocker, Some(SwitchBlocker::Offline));
        assert_eq!(SwitchBlocker::Offline.message(), "The machine is offline.");
        assert_eq!(
            targets
                .iter()
                .find(|target| target.profile_id == "0a1b2c3d")
                .and_then(|target| target.blocked.as_deref()),
            Some("The machine is offline.")
        );
        let (_, blocker) = switch_targets(&rows, "dev-1", CodingAgent::Claude, None, false, false);
        assert_eq!(blocker, Some(SwitchBlocker::NoCap));
        assert_eq!(
            SwitchBlocker::NoCap.message(),
            "Update the app on that machine to switch accounts."
        );

        // One login only: nothing to switch to.
        let (_, blocker) =
            switch_targets(&rows[..1], "dev-1", CodingAgent::Claude, None, true, false);
        assert_eq!(blocker, Some(SwitchBlocker::NoOtherAccount));

        // Another machine's rows are never offered here.
        let (targets, _) = switch_targets(&rows, "dev-2", CodingAgent::Claude, None, true, false);
        assert!(targets.is_empty());

        // The run's CURRENT account is the one it records, not the default.
        let (targets, _) = switch_targets(
            &rows,
            "dev-1",
            CodingAgent::Claude,
            Some("0a1b2c3d"),
            true,
            false,
        );
        assert!(targets[0].current && targets[0].profile_id == "0a1b2c3d");
    }

    /// EXP-863: a refusal is said ONCE. An account-level reason (signed out,
    /// re-login) belongs on its row; a run-level one (busy, offline, agent,
    /// build) is the footer's and never repeats under a row. No other account
    /// at all: no footer — the section is hidden, and so is the cost note.
    #[test]
    fn refusals_are_said_once_row_or_footer() {
        let rows = vec![
            row(coding::SYSTEM_PROFILE, true, Health::Ok),
            row("0a1b2c3d", true, Health::Ok),
            row("deadbeef", false, Health::SignedOut),
            row("badc0ffe", true, Health::NeedsRelogin),
        ];
        let by_id = |targets: &[SwitchTarget], id: &str| {
            targets.iter().find(|t| t.profile_id == id).unwrap().clone()
        };

        // Idle: the broken accounts carry their own sentence, the healthy one
        // none, and the footer is the cost note.
        let (targets, blocker) =
            switch_targets(&rows, "dev-1", CodingAgent::Claude, None, true, false);
        assert_eq!(
            by_id(&targets, "deadbeef").row_refusal(),
            Some("Sign in to this account on that machine first.")
        );
        assert_eq!(
            by_id(&targets, "badc0ffe").row_refusal(),
            Some("This account needs a re-login on that machine.")
        );
        assert_eq!(by_id(&targets, "0a1b2c3d").row_refusal(), None);
        assert_eq!(footer_note(&targets, blocker.as_ref()), Some(COST_NOTE));

        // Mid-turn: the footer says busy ONCE and no row repeats it.
        let (targets, blocker) =
            switch_targets(&rows, "dev-1", CodingAgent::Claude, None, true, true);
        assert_eq!(
            footer_note(&targets, blocker.as_ref()),
            Some("The agent is working — switching waits for the turn to finish.")
        );
        assert!(targets.iter().all(|target| target.row_refusal().is_none()));

        // One login only: nothing to switch to, nothing to say.
        let (targets, blocker) =
            switch_targets(&rows[..1], "dev-1", CodingAgent::Claude, None, true, false);
        assert_eq!(footer_note(&targets, blocker.as_ref()), None);

        // Only broken other accounts: their rows say why, the footer names
        // the machine's state — two different sentences.
        let broken = vec![rows[0].clone(), rows[2].clone()];
        let (targets, blocker) =
            switch_targets(&broken, "dev-1", CodingAgent::Claude, None, true, false);
        let footer = footer_note(&targets, blocker.as_ref()).unwrap();
        assert_eq!(footer, SwitchBlocker::NoOtherAccount.message());
        assert_ne!(Some(footer), by_id(&targets, "deadbeef").row_refusal());
    }

    /// The copy is the ×4 copy, byte for byte (Android
    /// `SessionAccountSwitch`, iOS `SessionAccountSwitch`, web
    /// `session-account-switch.tsx`).
    #[test]
    fn the_copy_is_byte_identical_across_clients() {
        assert_eq!(SECTION_TITLE, "Accounts");
        assert_eq!(SWITCH_LABEL, "Switch to this account");
        assert_eq!(WALL_SWITCH_LABEL, "Switch account");
        assert_eq!(
            COST_NOTE,
            "The run continues under the other account. Re-reading the transcript once costs tokens."
        );
        assert_eq!(CONTINUATION_NOTE, "Continues an earlier run");
        assert_eq!(
            CONTINUATION_COST_NOTE,
            "The agent re-read the transcript once to pick it up — a one-time cost."
        );
    }

    /// EXP-862: the row's caption is the account's IDENTITY, never its status
    /// — the refusal under the row already says a signed-out account cannot
    /// be switched to, and "Not signed in" as a title threw away the only
    /// identifying thing the row had (iOS + Android `caption`, desktop
    /// `accounts_section::group_caption`).
    #[test]
    fn a_signed_out_account_still_says_who_it_is() {
        let mut signed_out = row("deadbeef", false, Health::SignedOut);
        assert_eq!(caption_of(&signed_out), "deadbeef@acme.test");

        // No email: the plan, then the login's own label.
        signed_out.email = None;
        signed_out.plan = Some("Pro".to_string());
        assert_eq!(caption_of(&signed_out), "Pro");
        signed_out.plan = None;
        assert_eq!(caption_of(&signed_out), signed_out.profile_label);
    }
}
