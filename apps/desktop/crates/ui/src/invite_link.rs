//! The onboarding wizard's invite step body (EXP-725) — mint one team invite
//! link and copy it.
//!
//! Deliberately NOT the Settings → Members invite block: this is the
//! first-run control, so it carries no email field, no pending list and no
//! revoke. The one behaviour it adds is the plan CEILING (web/mobile parity):
//! `teams.inviteCapacity` is asked before the control is offered, and a team
//! whose free tier is already full renders NOTHING at all — no disabled
//! button, no hint, no upgrade notice. Onboarding never sells.
//!
//! Capacity moves while the step is open (a teammate accepts, a second
//! invite is minted from the web), so the two collections that feed the
//! server's count are observed and a change re-asks after a short debounce —
//! Electric deltas arrive in bursts, and the answer is a round trip.

use std::time::Duration;

use gpui::{
    div, App, IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Task, Window,
};
use gpui_component::{
    button::Button, clipboard::Clipboard, h_flex, v_flex, ActiveTheme as _, Disableable as _,
};
use sync::Store;

use crate::controls::WebControl as _;
use crate::icons::registry;
use crate::onboarding::copy;
use crate::queries;

/// How long a collection burst is allowed to settle before the capacity is
/// re-asked (one round trip per burst, not one per delta).
const REFETCH_DEBOUNCE: Duration = Duration::from_millis(300);

/// What the plan allows. `Loading` renders the control optimistically — the
/// answer usually lands before the user reaches for the button, and a
/// spinner where a button belongs reads as broken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Capacity {
    Loading,
    /// `Some(n)` = n invites left, `None` = unlimited.
    Known(Option<u32>),
}

impl Capacity {
    /// The ONE state that hides the control: the plan is provably full.
    fn is_full(self) -> bool {
        matches!(self, Capacity::Known(Some(0)))
    }
}

/// Emitted once a link exists — the wizard flips its footer from "skip" to
/// "continue" on it.
pub(crate) struct InviteMinted;

pub struct InviteLinkPanel {
    team_id: String,
    capacity: Capacity,
    invite_url: Option<SharedString>,
    generating: bool,
    error: Option<SharedString>,
    /// Bumped on every counted change; a debounce task only refetches while
    /// it is still the newest one.
    refetch_generation: u64,
    /// (members, pending invites) as of the last look — the server counts
    /// both, so either moving is a reason to re-ask.
    counts: (usize, usize),
    _refetch: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl gpui::EventEmitter<InviteMinted> for InviteLinkPanel {}

impl InviteLinkPanel {
    pub(crate) fn new(team_id: String, cx: &mut gpui::Context<Self>) -> Self {
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&collections.team_members, |this: &mut Self, _, cx| {
                this.counts_changed(cx);
            }),
            cx.observe(&collections.team_invites, |this: &mut Self, _, cx| {
                this.counts_changed(cx);
            }),
        ];
        let counts = Self::counts(&team_id, cx);
        let mut this = Self {
            team_id,
            capacity: Capacity::Loading,
            invite_url: None,
            generating: false,
            error: None,
            refetch_generation: 0,
            counts,
            _refetch: None,
            _subscriptions: subscriptions,
        };
        this.fetch_capacity(cx);
        this
    }

    /// (members, pending invites) for the team — exactly what the server
    /// subtracts from the plan's seat count.
    fn counts(team_id: &str, cx: &App) -> (usize, usize) {
        let collections = Store::global(cx).collections();
        let members = collections
            .team_members
            .read(cx)
            .iter()
            .filter(|member| member.team_id == team_id)
            .count();
        let invites = collections
            .team_invites
            .read(cx)
            .iter()
            .filter(|invite| invite.team_id == team_id && invite.accepted_at.is_none())
            .count();
        (members, invites)
    }

    fn counts_changed(&mut self, cx: &mut gpui::Context<Self>) {
        let counts = Self::counts(&self.team_id, cx);
        if counts == self.counts {
            return;
        }
        self.counts = counts;
        self.refetch_generation = self.refetch_generation.wrapping_add(1);
        let generation = self.refetch_generation;
        self._refetch = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REFETCH_DEBOUNCE).await;
            let _ = this.update(cx, |this: &mut Self, cx| {
                // A newer burst already queued its own refetch.
                if this.refetch_generation == generation {
                    this.fetch_capacity(cx);
                }
            });
        }));
    }

    /// Ask the server. A FAILURE is not a ceiling — an unreachable instance
    /// must never remove the control, so it decays to "unlimited".
    fn fetch_capacity(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            self.capacity = Capacity::Known(None);
            return;
        };
        let team_id = self.team_id.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::teams::teams_invite_capacity(&trpc, &team_id) })
                .await;
            let _ = this.update(cx, |this: &mut Self, cx| {
                this.capacity = Capacity::Known(result.unwrap_or(None));
                cx.notify();
            });
        })
        .detach();
    }

    fn generate(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.generating {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let Some(account) = queries::active_account(cx) else {
            return;
        };
        let base = account.instance_url;
        let team_id = self.team_id.clone();

        self.generating = true;
        self.error = None;
        cx.notify();

        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move {
                    api::teams::team_invites_create(
                        &trpc,
                        &team_id,
                        api::teams::TeamRole::Member,
                        None,
                    )
                })
                .await;
            let _ = this.update_in(window, |this: &mut Self, _, cx| {
                this.generating = false;
                match result {
                    Ok(out) => {
                        this.invite_url = Some(format!("{base}/invite/{}", out.token).into());
                        cx.emit(InviteMinted);
                    }
                    Err(err) if crate::settings::is_plan_limit(&err) => {
                        // The ceiling moved under us — take the control away
                        // the same way a known-full capacity does, silently.
                        this.capacity = Capacity::Known(Some(0));
                        this.invite_url = None;
                    }
                    Err(err) => this.error = Some(err.user_message().into()),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for InviteLinkPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // Plan full: the control is REMOVED. No disabled button, no hint, no
        // upgrade pitch (the mobile/web rule — onboarding never sells).
        if self.capacity.is_full() {
            return div().into_any_element();
        }

        let mut column = v_flex().gap_3();
        if let Some(url) = &self.invite_url {
            column = column.child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .px_2()
                            .py_1()
                            .rounded(cx.theme().radius)
                            .border_1()
                            .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                            .text_xs()
                            .font_family(theme::terminal::FONT_FAMILY)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(url.clone()),
                    )
                    .child(
                        Clipboard::new("invite-copy")
                            .value(url.clone())
                            .tooltip(copy::INVITE_COPY),
                    ),
            );
        }
        column = column.child(
            h_flex().child(
                Button::new("invite-generate")
                    .outline()
                    .web_sm()
                    .icon(registry::UI_INVITE)
                    .label(copy::INVITE_GENERATE)
                    .loading(self.generating)
                    .disabled(self.generating)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.generate(window, cx);
                    })),
            ),
        );
        if let Some(error) = &self.error {
            column = column.child(crate::settings::error_notice(error.clone(), cx));
        }
        column.into_any_element()
    }
}
