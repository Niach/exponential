//! First-run onboarding wizard (EXP-367/EXP-725) — the desktop analog of
//! mobile's server-gated flow, rendered by the Shell INSTEAD of the rail+dock
//! whenever a step applies (like the login surface; the rail is the
//! trunk-sync lifecycle driver, so no clone is attempted until the wizard
//! finishes).
//!
//! EXP-725: the SAME four steps on every client, in this order —
//!
//! 1. **Team** — the account has no team and `onboardingCompletedAt` is null:
//!    create one (embedded [`CreateTeamDialogView`]) or join via invite link
//!    (embedded [`JoinTeamView`]; the server stamps onboarding on accept).
//! 2. **Board** — the first team has no board yet: the embedded
//!    [`CreateBoardDialogView`] (repo picker + inline GitHub App connect ride
//!    along untouched).
//! 3. **Invite** — mint one invite link for the team ([`InviteLinkPanel`]);
//!    skippable, and REMOVED entirely when the plan has no seat left.
//! 4. **Devices** — the shared
//!    [`crate::settings::doctor_section::DoctorPanel`] (git + agent CLIs) plus
//!    the headless-server install sub-card. LAST because it is the step that
//!    means leaving for another machine. There is no download card: this IS
//!    the desktop app.
//!
//! Steps 3 and 4 run off a session latch (`post_board`), not off account
//! state: creating the board already fires the server's `onboarding.complete`
//! (the web wizard does the same), so the wizard would otherwise end there.
//! Only finishing/skipping the devices step stamps the LOCAL mirror in
//! accounts.json.
//!
//! The devices step ALSO stands alone (`Devices { team_id: None }`) on every
//! fresh install of an already-onboarded account, and comes back at startup
//! while coding is unusable (EXP-409) — that is the per-device
//! `tools_setup_seen` gate, independent of the account.
//!
//! The account steps are skippable — skipping them marks onboarding complete
//! (server + the local mirror) so the wizard never nags; the zero-team empty
//! state in [`crate::screens`] stays as the fallback surface behind a skipped
//! team step. The devices step is the ONE hard gate (EXP-369): without git
//! its button is disabled and the wizard cannot be left; a missing agent CLI
//! still skips. `EXP_SKIP_ONBOARDING=1` bypasses the whole wizard (dev/CI
//! machines without agent CLIs).

use std::rc::Rc;

use gpui::{
    div, px, App, AppContext as _, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement, Render, StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme as _, Disableable as _, Icon,
};
use sync::Store;

use crate::coding_flow::CodingHub;
use crate::controls::WebControl as _;
use crate::create_board_dialog::{BoardCreated, CreateBoardDialogView};
use crate::create_team_dialog::{CreateTeamDialogView, TeamCreated};
use crate::icons::registry;
use crate::invite_link::{InviteLinkPanel, InviteMinted};
use crate::join_team::{InviteAccepted, JoinTeamView};
use crate::queries;
use crate::session::AuthContext;
use crate::settings::doctor_section::DoctorPanel;

/// EXP-725 — the wizard COPY, one place per platform and byte-identical
/// across all four. Source of truth:
/// `apps/web/src/components/onboarding/onboarding-copy.ts`; the web test
/// `onboarding-copy.test.ts` reads THIS file off disk and asserts every
/// literal appears here verbatim.
///
/// Rules that drift test depends on: ONE single-line literal per constant, no
/// escapes, no `"` inside a string. Step-1 (team choice/create/join) copy is
/// NOT part of the shared set and stays inline below.
pub(crate) mod copy {
    pub const BOARD_TITLE: &str = "Create your first board";
    pub const BOARD_SUBTITLE: &str = "Boards hold your issues. Connect a GitHub repository to code on them. Everything can be changed later.";
    /// The embedded [`super::CreateBoardDialogView`] owns its own submit
    /// button, so the wizard never renders this one — it lives here because
    /// the shared contract (and its drift test) covers it.
    #[allow(dead_code)]
    pub const BOARD_CREATE: &str = "Create board";

    pub const INVITE_TITLE: &str = "Invite your teammates";
    pub const INVITE_SUBTITLE: &str = "Teammates share boards, reviews and the support inbox. You can also invite people later from team settings.";
    pub const INVITE_GENERATE: &str = "Generate invite link";
    pub const INVITE_COPY: &str = "Copy link";
    /// The `Clipboard` control paints its own copied state; the literal is
    /// still part of the shared contract.
    #[allow(dead_code)]
    pub const INVITE_COPIED: &str = "Copied";

    pub const DEVICES_TITLE: &str = "Set up your devices";
    pub const DEVICES_SUBTITLE: &str = "Coding sessions run on the desktop app or on a server with the Exponential CLI. Install one and sign your agents in. You can also do this later.";
    pub const DEVICES_YOURS: &str = "Your devices";
    pub const DEVICES_NONE: &str = "No devices yet. Sign in on the desktop app or a server and it shows up here.";

    pub const SKIP: &str = "Skip for now";
    pub const CONTINUE: &str = "Continue";
}

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
    Invite { team_id: String },
    /// `Some` = the wizard's fourth step (the board's team is in hand);
    /// `None` = the standalone per-device tools gate (fresh install of an
    /// onboarded account, or the EXP-409 re-entry).
    Devices { team_id: Option<String> },
}

/// Pure step resolution (unit-tested below). `first_team` is
/// `(team_id, has_boards)` of the oldest team, `None` when the account has
/// no team. `post_board` is the session latch set when the wizard's board was
/// created — it OUTRANKS the account state, because creating the board is
/// what fires the server's `onboarding.complete`, and the invite + devices
/// steps still have to run.
pub(crate) fn resolve_step(
    account_pending: bool,
    teams_ready: bool,
    first_team: Option<(String, bool)>,
    post_board: Option<&str>,
    invite_done: bool,
    tools_pending: bool,
) -> Option<WizardStep> {
    if let Some(team_id) = post_board {
        if !invite_done {
            return Some(WizardStep::Invite {
                team_id: team_id.to_string(),
            });
        }
        return Some(WizardStep::Devices {
            team_id: Some(team_id.to_string()),
        });
    }
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
        return Some(WizardStep::Devices { team_id: None });
    }
    None
}

/// Which Team sub-page shows (EXP-470, web wizard parity): a choice screen
/// first, then a dedicated create or join page with a Back button. Purely
/// click-driven view state — deliberately NOT a [`WizardStep`] variant, so
/// `resolve_step` stays a pure derivation of account/collection state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum TeamPage {
    #[default]
    Choice,
    Create,
    Join,
}

/// DEV-ONLY (§11.4 headless verification, the EXP_DEV_* family) preset for a
/// capture run: which page the wizard opens on, without synthetic input.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct DevOnboarding {
    team_page: TeamPage,
    /// Force the post-board tail on, resolving its team from `first_team` —
    /// the starter identity owns a board-LESS team, so the wizard would sit
    /// on the Board step otherwise.
    post_board: bool,
    invite_done: bool,
}

/// `EXP_DEV_ONBOARDING=choice|create|join|invite|devices` — the first three
/// pick the Team step's sub-page, `invite` and `devices` jump to the
/// post-board tail. Anything else (and unset) keeps the ordinary defaults, so
/// a normal run can never land off the choice page. Never document for users.
fn dev_onboarding() -> DevOnboarding {
    match std::env::var("EXP_DEV_ONBOARDING").ok().as_deref().map(str::trim) {
        Some("create") => DevOnboarding {
            team_page: TeamPage::Create,
            ..DevOnboarding::default()
        },
        Some("join") => DevOnboarding {
            team_page: TeamPage::Join,
            ..DevOnboarding::default()
        },
        Some("invite") => DevOnboarding {
            post_board: true,
            ..DevOnboarding::default()
        },
        Some("devices") => DevOnboarding {
            post_board: true,
            invite_done: true,
            ..DevOnboarding::default()
        },
        _ => DevOnboarding::default(),
    }
}

pub struct OnboardingView {
    /// The account the session latches belong to — reset on account switch.
    account_id: Option<String>,
    /// Session latch: the account steps finished (created/joined/skipped) —
    /// advance immediately without waiting for the persistence round-trips.
    account_steps_done: bool,
    /// Session latch for the tools step (persisted as `tools_setup_seen`).
    tools_step_done: bool,
    /// EXP-725: the team whose first board was just created — the wizard's
    /// invite + devices tail runs against it. Set on [`BoardCreated`],
    /// cleared when the devices step finishes.
    post_board: Option<String>,
    /// The invite step was skipped or its link minted-and-continued.
    invite_done: bool,
    /// A link exists — the invite footer reads "Continue" instead of "skip".
    invite_minted: bool,
    /// The Team step's sub-page (EXP-470).
    team_page: TeamPage,
    /// DEV-ONLY page preset, re-read on every account switch.
    dev: DevOnboarding,
    create_team: Option<Entity<CreateTeamDialogView>>,
    join_team: Option<Entity<JoinTeamView>>,
    /// Keyed by team id — a different first team rebuilds the form.
    create_board: Option<(String, Entity<CreateBoardDialogView>)>,
    /// Keyed by team id, same rule as the board form.
    invite_panel: Option<(String, Entity<InviteLinkPanel>)>,
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

        let dev = dev_onboarding();
        let mut this = Self {
            account_id: None,
            account_steps_done: false,
            tools_step_done: false,
            post_board: None,
            invite_done: false,
            invite_minted: false,
            team_page: dev.team_page,
            dev,
            create_team: None,
            join_team: None,
            create_board: None,
            invite_panel: None,
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
            self.post_board = None;
            self.invite_done = false;
            self.invite_minted = false;
            self.dev = dev_onboarding();
            self.team_page = self.dev.team_page;
            self.create_team = None;
            self.join_team = None;
            self.create_board = None;
            self.invite_panel = None;
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

    /// The post-board tail's team: the session latch, or — under the DEV
    /// override — the resolved first team.
    fn post_board_team(&self, cx: &App) -> Option<String> {
        if let Some(team_id) = &self.post_board {
            return Some(team_id.clone());
        }
        if !self.dev.post_board {
            return None;
        }
        let collections = Store::global(cx).collections();
        collections
            .teams_sorted(cx)
            .first()
            .map(|team| team.id.clone())
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
            self.post_board_team(cx).as_deref(),
            self.invite_done || self.dev.invite_done,
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

    /// Continue / skip on the invite step (EXP-725) — nothing to persist, the
    /// link (if any) is already on the server.
    fn complete_invite_step(&mut self, cx: &mut gpui::Context<Self>) {
        self.invite_done = true;
        self.dev.invite_done = true;
        cx.notify();
    }

    /// Continue / skip on the devices step. `in_wizard` = the fourth step of
    /// the four-step run (as opposed to the standalone per-device gate): it
    /// also closes the account steps, which is where the LOCAL onboarding
    /// stamp finally lands.
    fn complete_devices_step(&mut self, in_wizard: bool, cx: &mut gpui::Context<Self>) {
        if in_wizard {
            self.post_board = None;
            self.dev.post_board = false;
            self.complete_account_steps(cx);
        }
        self.complete_tools_step(cx);
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

    /// One full-width choice row (web `ChoiceStep` outline-button parity).
    fn team_choice_row(
        &mut self,
        id: &'static str,
        icon: Icon,
        title: &'static str,
        subtitle: &'static str,
        target: TeamPage,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let muted = cx.theme().muted_foreground;
        div()
            .id(id)
            .w_full()
            .px_4()
            .py_3()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().border)
            .cursor_pointer()
            .hover(|style| style.bg(cx.theme().list_hover))
            .flex()
            .items_center()
            .gap_3()
            .child(icon.size_5().text_color(cx.theme().primary))
            .child(
                v_flex()
                    .min_w_0()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(title),
                    )
                    .child(div().text_xs().text_color(muted).child(subtitle)),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.team_page = target;
                cx.notify();
            }))
            .into_any_element()
    }

    /// EXP-470: the create-or-join choice (web wizard parity — two big
    /// option rows instead of both forms stacked).
    fn team_choice_page(&mut self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        v_flex()
            .gap_3()
            .child(self.team_choice_row(
                "onboarding-choice-create",
                Icon::new(registry::UI_ADD),
                "Create a team",
                "Start fresh. You'll be the owner.",
                TeamPage::Create,
                cx,
            ))
            .child(self.team_choice_row(
                "onboarding-choice-join",
                Icon::new(registry::EDITOR_LINK),
                "Join a team",
                "Use an invite link a teammate sent you",
                TeamPage::Join,
                cx,
            ))
            .into_any_element()
    }

    /// The Back that returns an embedded form's footer to the choice page
    /// (EXP-470/EXP-698 — the form owns the row, so it needs a handle back
    /// into the wizard).
    fn back_to_choice(&self, cx: &mut gpui::Context<Self>) -> Rc<dyn Fn(&mut Window, &mut App)> {
        let this = cx.entity().downgrade();
        Rc::new(move |_window, cx| {
            let _ = this.update(cx, |this: &mut Self, cx| {
                this.team_page = TeamPage::Choice;
                cx.notify();
            });
        })
    }

    fn team_create_page(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        if self.create_team.is_none() {
            let on_back = self.back_to_choice(cx);
            let view = cx.new(|cx| CreateTeamDialogView::new(true, window, cx).with_back(on_back));
            self._subscriptions.push(cx.subscribe(
                &view,
                |_, _, _: &TeamCreated, cx| cx.notify(), // step advances via collections
            ));
            self.create_team = Some(view);
        }
        self.create_team
            .clone()
            .expect("created above")
            .into_any_element()
    }

    fn team_join_page(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        if self.join_team.is_none() {
            let on_back = self.back_to_choice(cx);
            let view = cx.new(|cx| JoinTeamView::new(None, true, window, cx).with_back(on_back));
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
        self.join_team
            .clone()
            .expect("created above")
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
            let created_for = team_id.to_string();
            self._subscriptions.push(cx.subscribe(
                &view,
                move |this: &mut Self, _, _: &BoardCreated, cx| {
                    // EXP-725: the board does NOT end the wizard any more —
                    // it opens its tail (invite → devices). The server-side
                    // `onboarding.complete` already fired inside the form;
                    // the LOCAL stamp waits for the devices step.
                    this.post_board = Some(created_for.clone());
                    cx.notify();
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

    /// EXP-725 step 3 — one invite link, nothing else (no email field, no
    /// pending list, no revoke: those live in Settings → Members).
    fn invite_page(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let stale = self
            .invite_panel
            .as_ref()
            .is_some_and(|(id, _)| id != team_id);
        if stale {
            self.invite_panel = None;
            self.invite_minted = false;
        }
        if self.invite_panel.is_none() {
            let view = cx.new(|cx| InviteLinkPanel::new(team_id.to_string(), cx));
            self._subscriptions.push(cx.subscribe(
                &view,
                |this: &mut Self, _, _: &InviteMinted, cx| {
                    this.invite_minted = true;
                    cx.notify();
                },
            ));
            self.invite_panel = Some((team_id.to_string(), view));
        }
        self.invite_panel
            .clone()
            .expect("created above")
            .1
            .into_any_element()
    }

    /// This user's own machines, newest registration surface first. Purely
    /// informational: the row that matters (this install) appears once its
    /// heartbeat has landed, which is exactly the confirmation the step is
    /// asking for.
    fn own_device_rows(cx: &App) -> Vec<(String, bool)> {
        let Some(account) = queries::active_account(cx) else {
            return Vec::new();
        };
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut rows: Vec<(String, bool)> = Store::global(cx)
            .collections()
            .devices
            .read(cx)
            .iter()
            .filter(|row| row.user_id.as_deref() == Some(account.user_id.as_str()))
            .filter_map(|row| {
                let label = row
                    .label
                    .clone()
                    .filter(|label| !label.trim().is_empty())
                    .or_else(|| row.device_id.clone())
                    .filter(|label| !label.trim().is_empty())?;
                let online =
                    crate::device_settings::row_is_online(row.last_seen_at.as_deref(), now_ms);
                Some((label, online))
            })
            .collect();
        rows.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
        rows
    }

    /// EXP-725 step 4 — the doctor (git + agent CLIs), the always-on-server
    /// install sub-card, and this account's registered machines. NO download
    /// card: this IS the desktop app.
    fn devices_page(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        if self.doctor.is_none() {
            self.doctor = Some(cx.new(|cx| DoctorPanel::new(window, cx)));
        }
        let doctor = self.doctor.clone().expect("created above");
        let muted = cx.theme().muted_foreground;

        let server_card = v_flex()
            .gap_2()
            .child(
                v_flex()
                    .gap_0p5()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(crate::getting_started::copy::SERVER_TITLE),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(crate::getting_started::copy::SERVER_DESCRIPTION),
                    ),
            )
            .child(crate::machines::server_install_snippet_box(
                "onboarding-server-copy",
                cx,
            ));

        let devices = Self::own_device_rows(cx);
        let mut devices_card = v_flex().gap_2().child(
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .child(copy::DEVICES_YOURS),
        );
        if devices.is_empty() {
            devices_card = devices_card.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(copy::DEVICES_NONE),
            );
        } else {
            devices_card = devices_card.child(crate::surface::glass_group_rows(
                devices
                    .into_iter()
                    .map(|(label, online)| {
                        crate::surface::glass_row_shell()
                            .child(div().flex_1().min_w_0().text_sm().child(label))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(if online {
                                        cx.theme().success
                                    } else {
                                        muted
                                    })
                                    .child(if online { "Online" } else { "Offline" }),
                            )
                    })
                    .collect(),
            ));
        }

        v_flex()
            .gap_5()
            .child(doctor)
            .child(server_card)
            .child(devices_card)
            .into_any_element()
    }
}

impl Render for OnboardingView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.sync_account(cx);

        // A pending account that already owns a boarded team (joined on the
        // web, second device…) needs no wizard pages — complete silently and
        // fall through to whatever is next.
        // …but never while the post-board tail is open: the LOCAL stamp is
        // the devices step's to make (EXP-725).
        if self.post_board_team(cx).is_none() && self.account_pending(cx) {
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
        // The card HEAD (web `OnboardingWizard` parity): a primary-tinted
        // 48px disc, the step title, the muted blurb. No "Step x of y" —
        // the web wizard never numbered its steps, and neither do we.
        let (icon, title, subtitle): (crate::icons::ExpIcon, &'static str, &'static str) =
            match &step {
                WizardStep::Syncing => (
                    registry::SETTINGS_MEMBERS,
                    "Welcome to Exponential",
                    "Syncing your account…",
                ),
                WizardStep::Team => match self.team_page {
                    TeamPage::Choice => (
                        registry::SETTINGS_MEMBERS,
                        "Welcome to Exponential",
                        "Teams hold your boards and teammates. Create your own, or join \
                         one you've been invited to.",
                    ),
                    TeamPage::Create => (
                        registry::SETTINGS_MEMBERS,
                        "Create a team",
                        "Name your team. You can rename it and invite teammates later.",
                    ),
                    TeamPage::Join => (
                        registry::EDITOR_LINK,
                        "Join a team",
                        "Ask a teammate for an invite link (team settings → Members), \
                         then paste it below.",
                    ),
                },
                WizardStep::Board { .. } => (
                    registry::NAV_BOARDS,
                    copy::BOARD_TITLE,
                    copy::BOARD_SUBTITLE,
                ),
                WizardStep::Invite { .. } => (
                    registry::UI_INVITE,
                    copy::INVITE_TITLE,
                    copy::INVITE_SUBTITLE,
                ),
                WizardStep::Devices { .. } => (
                    registry::NAV_DEVICES,
                    copy::DEVICES_TITLE,
                    copy::DEVICES_SUBTITLE,
                ),
            };

        let body: gpui::AnyElement = match &step {
            WizardStep::Syncing => v_flex()
                .gap_2()
                .items_center()
                .text_sm()
                .text_color(muted)
                .child("Loading your teams…")
                .into_any_element(),
            WizardStep::Team => match self.team_page {
                TeamPage::Choice => self.team_choice_page(cx),
                TeamPage::Create => self.team_create_page(window, cx),
                TeamPage::Join => self.team_join_page(window, cx),
            },
            WizardStep::Board { team_id } => {
                let team_id = team_id.clone();
                self.board_page(&team_id, window, cx)
            }
            WizardStep::Invite { team_id } => {
                let team_id = team_id.clone();
                self.invite_page(&team_id, cx)
            }
            WizardStep::Devices { .. } => self.devices_page(window, cx),
        };

        // Footer: the last row INSIDE the card body. Every step is skippable;
        // the tools step's primary Continue appears once the report is fully
        // green (both dismiss it). Create/Join contribute nothing here — the
        // embedded forms render their own Back / primary row (web parity:
        // those pages carry no "Set up later", Back goes to the choice page
        // which keeps the skip).
        let footer: Option<gpui::AnyElement> = match &step {
            WizardStep::Syncing => None,
            WizardStep::Team => match self.team_page {
                TeamPage::Choice => Some(
                    h_flex()
                        .justify_end()
                        .child(
                            Button::new("onboarding-skip-team")
                                .ghost().web_sm()
                                .label("Set up later")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.complete_account_steps(cx);
                                })),
                        )
                        .into_any_element(),
                ),
                TeamPage::Create | TeamPage::Join => None,
            },
            WizardStep::Board { .. } => Some(
                h_flex()
                    .justify_end()
                    .child(
                        Button::new("onboarding-skip-board")
                            .ghost().web_sm()
                            .label(copy::SKIP)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.complete_account_steps(cx);
                            })),
                    )
                    .into_any_element(),
            ),
            // EXP-725: ONE button. It is a skip until a link exists, and the
            // primary "done with this step" once one does.
            WizardStep::Invite { .. } => {
                let minted = self.invite_minted;
                Some(
                    h_flex()
                        .justify_end()
                        .child(if minted {
                            Button::new("onboarding-invite-advance")
                                .primary().web_md().rounded_full()
                                .label(copy::CONTINUE)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.complete_invite_step(cx);
                                }))
                        } else {
                            Button::new("onboarding-invite-advance")
                                .ghost().web_sm()
                                .label(copy::SKIP)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.complete_invite_step(cx);
                                }))
                        })
                        .into_any_element(),
                )
            }
            WizardStep::Devices { team_id } => {
                let in_wizard = team_id.is_some();
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
                            .primary().web_md().rounded_full()
                            .label(copy::CONTINUE)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.complete_devices_step(in_wizard, cx);
                            }))
                    } else {
                        Button::new("onboarding-tools-continue")
                            .outline().web_sm()
                            .label(copy::SKIP)
                            .disabled(!git_ok)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.complete_devices_step(in_wizard, cx);
                            }))
                    })
                    .into_any_element(),
                )
            }
        };

        // Full-window surface (login parity: floats on the Shell's page
        // gradient); the column scrolls when a step (the board form) runs
        // taller than the window.
        //
        // EXP-698 — ONE card, byte-for-byte the web wizard's: a 672px
        // `GlassGroup` (radius 12, glass-row fill, no outer stroke) whose two
        // sections — the head and the p-6 body — are split by the group's
        // hairline. Title, blurb and footer all live INSIDE it; nothing
        // floats above or below.
        div()
            .id("onboarding-scroll")
            .size_full()
            .overflow_y_scroll()
            .text_color(cx.theme().foreground)
            .child(
                // min-height, not height: a card taller than the window
                // (the board step) must grow the scroll extent instead of
                // being centred past the top edge.
                div().w_full().min_h_full().flex().items_center().justify_center().child(
                    v_flex()
                        .w_full()
                        .max_w(px(672.))
                        .px_6()
                        .my_8()
                        .child(
                            crate::surface::glass_group()
                                .child(
                                    v_flex()
                                        .p_6()
                                        .items_center()
                                        .gap_1p5()
                                        .text_center()
                                        .child(
                                            div()
                                                .size(px(48.))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .rounded_full()
                                                .bg(cx.theme().primary.opacity(0.1))
                                                .child(
                                                    Icon::new(icon)
                                                        .size(px(24.))
                                                        .text_color(cx.theme().primary),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .text_xl()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child(title),
                                        )
                                        .child(
                                            div().text_sm().text_color(muted).child(subtitle),
                                        ),
                                )
                                .child(crate::surface::glass_row_divider(
                                    v_flex().p_6().gap_4().child(body).children(footer),
                                )),
                        ),
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

    /// EXP-367/EXP-725: the step machine — account steps first (skeleton
    /// while the teams shape loads, NEVER a "no team" flash), then the
    /// post-board tail (invite → devices), and finally the per-device
    /// devices/tools gate, independent of the account's state.
    #[test]
    fn step_resolution() {
        // Fresh account, teams still syncing → hold.
        assert_eq!(
            resolve_step(true, false, None, None, false, true),
            Some(WizardStep::Syncing)
        );
        // Fresh account, no team → Team.
        assert_eq!(
            resolve_step(true, true, None, None, false, true),
            Some(WizardStep::Team)
        );
        // Team exists but has no board → Board.
        assert_eq!(
            resolve_step(true, true, team("t1", false), None, false, true),
            Some(WizardStep::Board { team_id: "t1".into() })
        );
        // The board was just created → Invite, whatever the account says.
        assert_eq!(
            resolve_step(true, true, team("t1", true), Some("t1"), false, true),
            Some(WizardStep::Invite { team_id: "t1".into() })
        );
        // …and the LOCAL stamp landing early (the server completed onboarding
        // when the board was created) must not skip the tail.
        assert_eq!(
            resolve_step(false, true, team("t1", true), Some("t1"), false, false),
            Some(WizardStep::Invite { team_id: "t1".into() })
        );
        // Invite done → the wizard's own devices step (team in hand).
        assert_eq!(
            resolve_step(true, true, team("t1", true), Some("t1"), true, true),
            Some(WizardStep::Devices { team_id: Some("t1".into()) })
        );
        // It is step FOUR, so it runs even with the tools latch already set.
        assert_eq!(
            resolve_step(false, true, team("t1", true), Some("t1"), true, false),
            Some(WizardStep::Devices { team_id: Some("t1".into()) })
        );
        // Boarded team, no tail latch → account steps satisfied → the
        // STANDALONE devices gate.
        assert_eq!(
            resolve_step(true, true, team("t1", true), None, false, true),
            Some(WizardStep::Devices { team_id: None })
        );
        // Onboarded account, fresh install → only that gate (teams readiness
        // is irrelevant — nothing account-shaped is pending).
        assert_eq!(
            resolve_step(false, false, None, None, false, true),
            Some(WizardStep::Devices { team_id: None })
        );
        // Everything satisfied → inactive.
        assert_eq!(
            resolve_step(false, true, team("t1", true), None, false, false),
            None
        );
        // Account pending but tools seen: the account steps still run…
        assert_eq!(
            resolve_step(true, true, None, None, false, false),
            Some(WizardStep::Team)
        );
        // …and a boarded team with tools seen means no wizard at all.
        assert_eq!(
            resolve_step(true, true, team("t1", true), None, false, false),
            None
        );
    }

    /// EXP-470: the Team step always re-enters on the choice page.
    #[test]
    fn team_page_defaults_to_choice() {
        assert_eq!(TeamPage::default(), TeamPage::Choice);
    }

    /// EXP-642/EXP-725: the DEV-ONLY page override — unset/garbage keeps the
    /// ordinary defaults, so a normal run can never land off the choice page,
    /// and `invite`/`devices` force the post-board tail on (the starter
    /// identity's team is board-less, so the wizard would sit on Board).
    /// (Env-free: the parse is exercised through `dev_onboarding`'s match by
    /// setting the variable for the duration of this single-threaded test.)
    #[test]
    fn dev_onboarding_reads_the_env_override() {
        // Nothing else in this crate reads the variable, and it is restored
        // before the test returns.
        std::env::remove_var("EXP_DEV_ONBOARDING");
        assert_eq!(dev_onboarding(), DevOnboarding::default());
        for (value, expected) in [
            ("choice", DevOnboarding::default()),
            (
                "create",
                DevOnboarding {
                    team_page: TeamPage::Create,
                    ..DevOnboarding::default()
                },
            ),
            (
                "join",
                DevOnboarding {
                    team_page: TeamPage::Join,
                    ..DevOnboarding::default()
                },
            ),
            (
                "invite",
                DevOnboarding {
                    post_board: true,
                    ..DevOnboarding::default()
                },
            ),
            (
                "devices",
                DevOnboarding {
                    post_board: true,
                    invite_done: true,
                    ..DevOnboarding::default()
                },
            ),
            ("nonsense", DevOnboarding::default()),
        ] {
            std::env::set_var("EXP_DEV_ONBOARDING", value);
            assert_eq!(dev_onboarding(), expected, "{value}");
        }
        std::env::remove_var("EXP_DEV_ONBOARDING");
    }

    /// EXP-725: the shared copy is what the other three clients render — the
    /// web drift test reads these literals out of this file, so a rename or a
    /// reflow here breaks it deliberately. Guard the two rules it depends on.
    #[test]
    fn shared_copy_is_single_line_and_unescaped() {
        for (name, value) in [
            ("BOARD_TITLE", copy::BOARD_TITLE),
            ("BOARD_SUBTITLE", copy::BOARD_SUBTITLE),
            ("BOARD_CREATE", copy::BOARD_CREATE),
            ("INVITE_TITLE", copy::INVITE_TITLE),
            ("INVITE_SUBTITLE", copy::INVITE_SUBTITLE),
            ("INVITE_GENERATE", copy::INVITE_GENERATE),
            ("INVITE_COPY", copy::INVITE_COPY),
            ("INVITE_COPIED", copy::INVITE_COPIED),
            ("DEVICES_TITLE", copy::DEVICES_TITLE),
            ("DEVICES_SUBTITLE", copy::DEVICES_SUBTITLE),
            ("DEVICES_YOURS", copy::DEVICES_YOURS),
            ("DEVICES_NONE", copy::DEVICES_NONE),
            ("SKIP", copy::SKIP),
            ("CONTINUE", copy::CONTINUE),
        ] {
            assert!(!value.is_empty(), "{name} is empty");
            assert!(!value.contains('\n'), "{name} spans lines");
            assert!(!value.contains('"'), "{name} carries a quote");
            assert!(!value.contains('\\'), "{name} carries an escape");
            assert_eq!(value.trim(), value, "{name} has edge whitespace");
        }
    }
}
