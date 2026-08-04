//! First-run onboarding wizard (EXP-367) — the desktop analog of mobile's
//! server-gated flow, rendered by the Shell INSTEAD of the rail+dock whenever
//! a step applies (like the login surface; the rail is the trunk-sync
//! lifecycle driver, so no clone is attempted until the wizard finishes).
//!
//! Steps, each independently gated:
//!
//! 1. **Team** — the account has no team and `onboardingCompletedAt` is null:
//!    create one (embedded [`CreateTeamDialogView`]) or join via invite link
//!    (embedded [`JoinTeamView`]; the server stamps onboarding on accept).
//! 2. **Board** — the first team has no board yet: the embedded
//!    [`CreateBoardDialogView`] (repo picker + inline GitHub App connect ride
//!    along untouched).
//! 3. **Tools** — EVERY fresh install, even with a fully-onboarded account
//!    and an all-green toolchain: the shared
//!    [`crate::settings::doctor_section::DoctorPanel`] confirms git + agent
//!    CLIs (per-device `tools_setup_seen` in the coding settings).
//!
//! The account steps are skippable — skipping them marks onboarding complete
//! (server + the local mirror in accounts.json) so the wizard never nags; the
//! zero-team empty state in [`crate::screens`] stays as the fallback surface
//! behind a skipped team step. The tools step is the ONE hard gate (EXP-369):
//! without git its button is disabled and the wizard cannot be left; a
//! missing agent CLI still skips. `EXP_SKIP_ONBOARDING=1` bypasses the whole
//! wizard (dev/CI machines without agent CLIs).

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, App, AppContext as _, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription,
    Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme as _, Disableable as _, Sizable as _,
};
use sync::Store;

use crate::coding_flow::CodingHub;
use crate::create_board_dialog::{BoardCreated, CreateBoardDialogView};
use crate::create_team_dialog::{CreateTeamDialogView, TeamCreated};
use crate::join_team::{InviteAccepted, JoinTeamView};
use crate::queries;
use crate::session::AuthContext;
use crate::settings::doctor_section::DoctorPanel;

/// Mirror the server's onboarding stamp into accounts.json (one-way; warm
/// starts never re-fetch the session, so this is the only local source of
/// the gate between logins). Safe to call repeatedly.
pub(crate) fn stamp_local_onboarding(cx: &mut App) {
    let Some(account) = queries::active_account(cx) else {
        return;
    };
    if account.onboarding_completed_at.is_some() {
        return;
    }
    let now = chrono::Utc::now().to_rfc3339();
    if let Some(auth) = cx.try_global::<AuthContext>() {
        auth.auth.set_onboarding_completed(&account.id, &now);
    }
}

/// The wizard's current step. `Syncing` = account steps apply but the teams
/// shape hasn't landed — render a quiet loading surface, NEVER a flash of
/// "no team" (§4.1: empty-because-loading is not empty).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WizardStep {
    Syncing,
    Team,
    Board { team_id: String },
    Tools,
}

/// Pure step resolution (unit-tested below). `first_team` is
/// `(team_id, has_boards)` of the oldest team, `None` when the account has
/// no team. A first team WITH boards satisfies the account steps — the
/// caller completes them as a side effect and falls through to Tools.
pub(crate) fn resolve_step(
    account_pending: bool,
    teams_ready: bool,
    first_team: Option<(String, bool)>,
    tools_pending: bool,
) -> Option<WizardStep> {
    if account_pending {
        if !teams_ready {
            return Some(WizardStep::Syncing);
        }
        match first_team {
            None => return Some(WizardStep::Team),
            Some((team_id, false)) => return Some(WizardStep::Board { team_id }),
            Some((_, true)) => {}
        }
    }
    if tools_pending {
        return Some(WizardStep::Tools);
    }
    None
}

pub struct OnboardingView {
    /// The account the session latches belong to — reset on account switch.
    account_id: Option<String>,
    /// Session latch: the account steps finished (created/joined/skipped) —
    /// advance immediately without waiting for the persistence round-trips.
    account_steps_done: bool,
    /// Session latch for the tools step (persisted as `tools_setup_seen`).
    tools_step_done: bool,
    create_team: Option<Entity<CreateTeamDialogView>>,
    join_team: Option<Entity<JoinTeamView>>,
    /// Keyed by team id — a different first team rebuilds the form.
    create_board: Option<(String, Entity<CreateBoardDialogView>)>,
    doctor: Option<Entity<DoctorPanel>>,
    /// Base subscriptions (session/collections/hub); page-event
    /// subscriptions are pushed here as pages are lazily created.
    _subscriptions: Vec<Subscription>,
}

impl OnboardingView {
    pub fn new(_window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let store = Store::global(cx).clone();
        let mut subscriptions = store.observe_collections(cx);
        // Session flips (sign-in/out, account switch) re-evaluate the gate;
        // the account-latch reset rides the same observer.
        subscriptions.push(cx.observe(&store.state(), |this: &mut Self, _, cx| {
            this.sync_account(cx);
            cx.notify();
        }));
        // Settings/doctor changes (tools_setup_seen, report landing).
        let hub = CodingHub::global(cx);
        subscriptions.push(cx.observe(&hub, |_, _, cx| cx.notify()));

        let mut this = Self {
            account_id: None,
            account_steps_done: false,
            tools_step_done: false,
            create_team: None,
            join_team: None,
            create_board: None,
            doctor: None,
            _subscriptions: subscriptions,
        };
        this.sync_account(cx);
        this
    }

    /// Reset the per-account latches (and the embedded pages) when the
    /// signed-in account changes.
    fn sync_account(&mut self, cx: &mut gpui::Context<Self>) {
        let current = queries::active_account(cx).map(|account| account.id);
        if current != self.account_id {
            self.account_id = current;
            self.account_steps_done = false;
            self.tools_step_done = false;
            self.create_team = None;
            self.join_team = None;
            self.create_board = None;
        }
    }

    /// Whether the Shell must render the wizard instead of the app.
    /// Read-only — safe from the Shell's render path.
    pub fn is_active(&self, cx: &App) -> bool {
        if std::env::var("EXP_SKIP_ONBOARDING").as_deref() == Ok("1") {
            return false;
        }
        self.desired_step(cx).is_some()
    }

    fn account_pending(&self, cx: &App) -> bool {
        !self.account_steps_done
            && queries::active_account(cx)
                .is_some_and(|account| account.onboarding_completed_at.is_none())
    }

    fn tools_pending(&self, cx: &App) -> bool {
        if self.tools_step_done {
            return false;
        }
        let Some(hub) = CodingHub::global_ref(cx) else {
            return false;
        };
        let hub = hub.read(cx);
        if !hub.settings.tools_setup_seen {
            return true;
        }
        // EXP-409: even after the first-run latch, the tools step comes BACK
        // at startup while coding is fully unusable — no git, or no runnable
        // agent (not installed OR installed-but-signed-out both count).
        // Gated on a LANDED report so the wizard never flashes while the
        // first probe is still running; "Set up later" dismisses it for the
        // session.
        hub.doctor
            .report
            .as_ref()
            .is_some_and(|report| !report.git.ok || !report.any_agent_ok())
    }

    fn desired_step(&self, cx: &App) -> Option<WizardStep> {
        // Not signed in → the Shell renders the login surface, not us.
        queries::active_account(cx)?;
        let collections = Store::global(cx).collections();
        let teams_ready = collections.teams.read(cx).is_ready();
        let first_team = collections.teams_sorted(cx).first().map(|team| {
            let has_boards = !collections.boards_in_team(&team.id, cx).is_empty();
            (team.id.clone(), has_boards)
        });
        resolve_step(
            self.account_pending(cx),
            teams_ready,
            first_team,
            self.tools_pending(cx),
        )
    }

    /// The account steps are satisfied (board created / invite accepted /
    /// user skipped / the account turned out to already have a boarded
    /// team): latch, mirror the stamp locally, and fire the idempotent
    /// server mutation best-effort.
    fn complete_account_steps(&mut self, cx: &mut gpui::Context<Self>) {
        if self.account_steps_done {
            return;
        }
        self.account_steps_done = true;
        stamp_local_onboarding(cx);
        if let Some(trpc) = queries::trpc_client(cx) {
            cx.background_executor()
                .spawn(async move {
                    if let Err(err) = api::onboarding::complete(&trpc) {
                        log::warn!("[ui] onboarding.complete failed (best-effort): {err}");
                    }
                })
                .detach();
        }
        cx.notify();
    }

    /// Continue / "Set up later" on the tools step: latch + persist the
    /// per-device flag through the hub (merge-preserving save).
    fn complete_tools_step(&mut self, cx: &mut gpui::Context<Self>) {
        if self.tools_step_done {
            return;
        }
        self.tools_step_done = true;
        let hub = CodingHub::global(cx);
        let mut settings = hub.read(cx).settings.clone();
        if !settings.tools_setup_seen {
            settings.tools_setup_seen = true;
            let _ = CodingHub::save_settings(&hub, settings, cx);
        }
        cx.notify();
    }

    // -- step pages (lazily created; events subscribed once) ----------------

    fn team_page(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        if self.create_team.is_none() {
            let view = cx.new(|cx| CreateTeamDialogView::new(true, window, cx));
            self._subscriptions.push(cx.subscribe(
                &view,
                |_, _, _: &TeamCreated, cx| cx.notify(), // step advances via collections
            ));
            self.create_team = Some(view);
        }
        if self.join_team.is_none() {
            let view = cx.new(|cx| JoinTeamView::new(None, true, window, cx));
            self._subscriptions.push(cx.subscribe(
                &view,
                |this: &mut Self, _, _: &InviteAccepted, cx| {
                    // The server stamped onboarding on accept; latch so the
                    // board step never flashes for a joined boardless team.
                    this.complete_account_steps(cx);
                },
            ));
            self.join_team = Some(view);
        }
        let muted = cx.theme().muted_foreground;
        v_flex()
            .gap_4()
            .child(self.create_team.clone().expect("created above"))
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(div().flex_1().h(px(1.)).bg(cx.theme().border))
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child("or join an existing team"),
                    )
                    .child(div().flex_1().h(px(1.)).bg(cx.theme().border)),
            )
            .child(self.join_team.clone().expect("created above"))
            .into_any_element()
    }

    fn board_page(
        &mut self,
        team_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let stale = self
            .create_board
            .as_ref()
            .is_some_and(|(id, _)| id != team_id);
        if stale {
            self.create_board = None;
        }
        if self.create_board.is_none() {
            let view = cx.new(|cx| CreateBoardDialogView::new(team_id.to_string(), true, window, cx));
            self._subscriptions.push(cx.subscribe(
                &view,
                |this: &mut Self, _, _: &BoardCreated, cx| {
                    this.complete_account_steps(cx);
                },
            ));
            self.create_board = Some((team_id.to_string(), view));
        }
        self.create_board
            .clone()
            .expect("created above")
            .1
            .into_any_element()
    }

    fn tools_page(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        if self.doctor.is_none() {
            self.doctor = Some(cx.new(|cx| DoctorPanel::new(window, cx)));
        }
        self.doctor.clone().expect("created above").into_any_element()
    }
}

impl Render for OnboardingView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.sync_account(cx);

        // A pending account that already owns a boarded team (joined on the
        // web, second device…) needs no wizard pages — complete silently and
        // fall through to whatever is next.
        if self.account_pending(cx) {
            let collections = Store::global(cx).collections();
            if collections.teams.read(cx).is_ready() {
                let boarded = collections
                    .teams_sorted(cx)
                    .first()
                    .is_some_and(|team| !collections.boards_in_team(&team.id, cx).is_empty());
                if boarded {
                    self.complete_account_steps(cx);
                }
            }
        }

        let Some(step) = self.desired_step(cx) else {
            // Inactive — the Shell should not be rendering us; stay blank
            // for the one frame until it re-evaluates.
            return div().into_any_element();
        };

        let muted = cx.theme().muted_foreground;
        // Which steps apply this run (for the "Step x of y" line): the
        // account pair counts as two entries while pending.
        let (title, subtitle, position): (&'static str, &'static str, Option<(usize, usize)>) = {
            let total = if self.account_pending(cx) { 2 } else { 0 }
                + if self.tools_pending(cx) { 1 } else { 0 };
            match &step {
                WizardStep::Syncing => ("Welcome to Exponential", "Syncing your account…", None),
                WizardStep::Team => (
                    "Welcome to Exponential",
                    "Create a team to get started, or join one with an invite link.",
                    Some((1, total)),
                ),
                WizardStep::Board { .. } => (
                    "Create your first board",
                    "Boards hold your issues. Connect a GitHub repository to unlock \
                     coding features. You can also do this later.",
                    Some((2, total)),
                ),
                WizardStep::Tools => (
                    "Set up your tools",
                    "Exponential runs coding sessions with git and an agent CLI (Claude \
                     Code, Codex, or pi). Git is required; one agent is enough. \
                     Everything else works without them.",
                    Some((total.max(1), total.max(1))),
                ),
            }
        };

        let body: gpui::AnyElement = match &step {
            WizardStep::Syncing => v_flex()
                .gap_2()
                .items_center()
                .text_sm()
                .text_color(muted)
                .child("Loading your teams…")
                .into_any_element(),
            WizardStep::Team => self.team_page(window, cx),
            WizardStep::Board { team_id } => {
                let team_id = team_id.clone();
                self.board_page(&team_id, window, cx)
            }
            WizardStep::Tools => self.tools_page(window, cx),
        };

        // Footer: every step is skippable; the tools step's primary Continue
        // appears once the report is fully green (both dismiss it).
        let footer: Option<gpui::AnyElement> = match &step {
            WizardStep::Syncing => None,
            WizardStep::Team => Some(
                h_flex()
                    .justify_end()
                    .child(
                        Button::new("onboarding-skip-team")
                            .ghost()
                            .small()
                            .label("Set up later")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.complete_account_steps(cx);
                            })),
                    )
                    .into_any_element(),
            ),
            WizardStep::Board { .. } => Some(
                h_flex()
                    .justify_end()
                    .child(
                        Button::new("onboarding-skip-board")
                            .ghost()
                            .small()
                            .label("Skip for now")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.complete_account_steps(cx);
                            })),
                    )
                    .into_any_element(),
            ),
            WizardStep::Tools => {
                let report = CodingHub::global_ref(cx)
                    .and_then(|hub| hub.read(cx).doctor.report.clone());
                // EXP-369: git is a HARD gate — nothing the IDE does with a
                // repository works without it, so there is no forward path
                // out of this step (a still-running probe reads as not-ok and
                // re-renders when it lands; `EXP_SKIP_ONBOARDING=1` is the
                // dev/CI bypass for the whole wizard). A missing agent CLI
                // only blocks coding, so it keeps the "Set up later" escape.
                let git_ok = report.as_ref().is_some_and(|report| report.git.ok);
                let all_green = report.is_some_and(|report| report.git.ok && report.any_agent_ok());
                let mut row = h_flex().items_center().justify_end().gap_3();
                if !git_ok {
                    row = row.child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .text_color(muted)
                            .child("git is required. You cannot start coding without an agent CLI."),
                    );
                }
                Some(
                    row.child(if all_green {
                        Button::new("onboarding-tools-continue")
                            .primary()
                            .small()
                            .label("Continue")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.complete_tools_step(cx);
                            }))
                    } else {
                        Button::new("onboarding-tools-continue")
                            .outline()
                            .small()
                            .label("Set up later")
                            .disabled(!git_ok)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.complete_tools_step(cx);
                            }))
                    })
                    .into_any_element(),
                )
            }
        };

        // Full-window surface (login parity: floats on the Shell's page
        // gradient); the column scrolls when a step (the board form) runs
        // taller than the window.
        div()
            .id("onboarding-scroll")
            .size_full()
            .overflow_y_scroll()
            .text_color(cx.theme().foreground)
            .child(
                div().size_full().flex().items_center().justify_center().child(
                    v_flex()
                        .w(px(480.))
                        .my_8()
                        .gap_4()
                        .child(
                            v_flex()
                                .gap_1()
                                .when_some(position, |this, (current, total)| {
                                    // A one-step run reads awkwardly as
                                    // "Step 1 of 1" — drop the line.
                                    this.when(total > 1, |this| {
                                        this.child(div().text_xs().text_color(muted).child(
                                            SharedString::from(format!(
                                                "Step {current} of {total}"
                                            )),
                                        ))
                                    })
                                })
                                .child(
                                    div()
                                        .text_xl()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(title),
                                )
                                .child(div().text_sm().text_color(muted).child(subtitle)),
                        )
                        .child(crate::surface::glass_card().p_6().child(body))
                        .children(footer),
                ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn team(id: &str, has_boards: bool) -> Option<(String, bool)> {
        Some((id.to_string(), has_boards))
    }

    /// EXP-367: the step machine — account steps first (skeleton while the
    /// teams shape loads, NEVER a "no team" flash), then the per-device
    /// tools step, independent of the account's state.
    #[test]
    fn step_resolution() {
        // Fresh account, teams still syncing → hold.
        assert_eq!(
            resolve_step(true, false, None, true),
            Some(WizardStep::Syncing)
        );
        // Fresh account, no team → Team.
        assert_eq!(resolve_step(true, true, None, true), Some(WizardStep::Team));
        // Team exists but has no board → Board.
        assert_eq!(
            resolve_step(true, true, team("t1", false), true),
            Some(WizardStep::Board { team_id: "t1".into() })
        );
        // Boarded team → account steps satisfied → Tools.
        assert_eq!(
            resolve_step(true, true, team("t1", true), true),
            Some(WizardStep::Tools)
        );
        // Onboarded account, fresh install → ONLY the tools step (teams
        // readiness is irrelevant — nothing account-shaped is pending).
        assert_eq!(resolve_step(false, false, None, true), Some(WizardStep::Tools));
        // Everything satisfied → inactive.
        assert_eq!(resolve_step(false, true, team("t1", true), false), None);
        // Account pending but tools seen: the account steps still run…
        assert_eq!(resolve_step(true, true, None, false), Some(WizardStep::Team));
        // …and a boarded team with tools seen means no wizard at all.
        assert_eq!(resolve_step(true, true, team("t1", true), false), None);
    }
}
