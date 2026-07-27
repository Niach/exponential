//! Settings → General + Danger Zone (masterplan-v3 §4.2).
//!
//! Web parity: `components/team/general-section.tsx` (name `Input`,
//! dirty-gated Save; teams are always private — there is no visibility
//! setting) and the Danger Zone card of
//! `routes/t/$teamSlug/settings/general.tsx` (type-the-name-to-confirm
//! delete, gated owner + team-only).
//!
//! Local state mirrors the web's `useState` + resync-on-team-change
//! `useEffect`: an Electric echo that changes the synced row overwrites the
//! local draft (which is exactly how the post-save echo clears `dirty`).

use gpui::{
    div, prelude::FluentBuilder as _, App, AppContext as _, Entity, IntoElement, ParentElement,
    Render, SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    v_flex, ActiveTheme as _, Disableable as _, Sizable as _,
};
use sync::Store;

use crate::native_dialog::{self, AlertSpec};
use crate::navigation::Navigation;

use super::{
    active_team, card_header, error_notice, is_owner, open_url, row_stroke, section,
    show_team_chrome, team_delete_error_message,
};

/// Server fetch state for the read-only billing summary (EXP-288).
enum BillingLoad {
    Idle,
    Loading,
    Ready(api::billing::TeamPlanOut),
    /// Errors hide the section (logged) — billing must never block the
    /// name/danger-zone cards.
    Failed,
}

/// Snapshot of the synced fields the pane mirrors — resync happens whenever
/// this differs from the live row (the web `useEffect` dep list).
#[derive(Clone, PartialEq, Eq)]
struct Snapshot {
    team_id: String,
    name: String,
}

pub struct GeneralPane {
    nav: Entity<Navigation>,
    name_input: Entity<InputState>,
    delete_input: Entity<InputState>,
    snapshot: Option<Snapshot>,
    saving: bool,
    error: Option<SharedString>,
    /// REV2-55: the server can REFUSE a delete (a live subscription, a lost
    /// ownership race), so the Danger Zone shows why instead of leaving the
    /// confirm dialog looking like it worked.
    delete_error: Option<SharedString>,
    /// EXP-288: the read-only plan/usage summary between the name card and
    /// the Danger Zone. Refetched on team/account change; hidden while
    /// loading/failed and entirely on self-hosted (`plan == "unlimited"`).
    billing: BillingLoad,
    billing_team: Option<String>,
    billing_account: Option<String>,
    billing_generation: u64,
    _subscriptions: Vec<Subscription>,
}

impl GeneralPane {
    pub fn new(
        nav: Entity<Navigation>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Team name"));
        let delete_input = cx.new(|cx| InputState::new(window, cx));

        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            // Resync needs the window (set_value) — window-aware observers.
            cx.observe_in(&nav, window, |this, _, window, cx| {
                this.resync(window, cx);
            }),
            cx.observe_in(&collections.teams, window, |this, _, window, cx| {
                this.resync(window, cx);
            }),
            cx.observe(&collections.team_members, |_, _, cx| cx.notify()),
            cx.observe(&collections.users, |_, _, cx| cx.notify()),
            // Live dirty tracking: typing enables/disables Save.
            cx.subscribe(&name_input, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];

        let mut this = Self {
            nav,
            name_input,
            delete_input,
            snapshot: None,
            saving: false,
            error: None,
            delete_error: None,
            billing: BillingLoad::Idle,
            billing_team: None,
            billing_account: None,
            billing_generation: 0,
            _subscriptions: subscriptions,
        };
        this.resync(window, cx);
        this
    }

    /// Kick the `billing.teamPlan` fetch when the pane is shown or the
    /// team/account changed (render-time — a hidden pane never fetches;
    /// generation-guarded like the boards repo cache).
    fn ensure_billing(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) {
        let account_id = Store::global(cx)
            .session(cx)
            .account_id()
            .map(str::to_string);
        if account_id != self.billing_account {
            self.billing_account = account_id;
            self.billing = BillingLoad::Idle;
        }
        if self.billing_team.as_deref() == Some(team_id)
            && !matches!(self.billing, BillingLoad::Idle)
        {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            return;
        };
        self.billing = BillingLoad::Loading;
        self.billing_team = Some(team_id.to_string());
        self.billing_generation += 1;
        let generation = self.billing_generation;
        let team_id = team_id.to_string();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::billing::billing_team_plan(&trpc, &team_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.billing_generation != generation {
                    return; // superseded
                }
                this.billing = match result {
                    Ok(plan) => BillingLoad::Ready(plan),
                    Err(err) => {
                        log::warn!("[ui] billing.teamPlan failed: {err}");
                        BillingLoad::Failed
                    }
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// Mirror the web `useEffect`: whenever the synced row (or the active
    /// team) changes, replace the local draft.
    fn resync(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(team) = active_team(cx, &self.nav) else {
            return;
        };
        let snapshot = Snapshot {
            team_id: team.id.clone(),
            name: team.name.clone(),
        };
        if self.snapshot.as_ref() == Some(&snapshot) {
            return;
        }
        self.name_input.update(cx, |state, cx| {
            state.set_value(snapshot.name.clone(), window, cx);
        });
        self.snapshot = Some(snapshot);
        // A refused delete belonged to the team that was selected then.
        self.delete_error = None;
        cx.notify();
    }

    fn dirty(&self, cx: &App) -> bool {
        let Some(snapshot) = &self.snapshot else {
            return false;
        };
        self.name_input.read(cx).value().as_ref() != snapshot.name
    }

    fn save(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(snapshot) = self.snapshot.clone() else {
            return;
        };
        if !self.dirty(cx) || self.saving {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            return;
        };

        let typed = self.name_input.read(cx).value().trim().to_string();
        // Web: `name.trim() || team.name` — an emptied field falls back.
        let name = if typed.is_empty() {
            snapshot.name.clone()
        } else {
            typed
        };
        let mut input = api::teams::TeamsUpdateInput::new(snapshot.team_id.clone());
        input.name = Some(name);

        self.saving = true;
        self.error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::teams::teams_update(&trpc, &input) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.saving = false;
                if let Err(err) = result {
                    this.error = Some(format!("Failed to save changes: {err}").into());
                }
                // Success needs no action: the Electric echo resyncs the
                // snapshot, which clears `dirty`.
                cx.notify();
            });
        })
        .detach();
    }

    /// Web `formatStorage`: MB under a GB, one-decimal GB above.
    fn format_storage(mb: f64) -> String {
        if mb >= 1024. {
            let gb = mb / 1024.;
            if (gb - gb.round()).abs() < 0.05 {
                format!("{} GB", gb.round() as i64)
            } else {
                format!("{gb:.1} GB")
            }
        } else if (mb - mb.round()).abs() < 0.05 {
            format!("{} MB", mb.round() as i64)
        } else {
            format!("{mb:.1} MB")
        }
    }

    /// One usage row: label left, "current / limit" right, a thin progress
    /// track underneath (no track for unlimited).
    fn usage_bar(
        label: &'static str,
        current: String,
        limit: Option<String>,
        fraction: Option<f64>,
        cx: &App,
    ) -> impl IntoElement {
        let amount = match limit {
            Some(limit) => format!("{current} / {limit}"),
            None => format!("{current} / unlimited"),
        };
        let mut row = v_flex()
            .gap_1()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(div().text_sm().child(label))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(SharedString::from(amount)),
                    ),
            );
        if let Some(fraction) = fraction {
            let fraction = fraction.clamp(0., 1.) as f32;
            row = row.child(
                div()
                    .h(gpui::px(6.))
                    .w_full()
                    .rounded_full()
                    .bg(cx.theme().muted.opacity(0.35))
                    .child(
                        div()
                            .h_full()
                            .rounded_full()
                            .w(gpui::relative(fraction))
                            .bg(cx.theme().primary),
                    ),
            );
        }
        row
    }

    /// The EXP-288 read-only billing summary: plan chip + usage bars + a
    /// "Manage on the web" hand-off. Returns `None` while loading/failed,
    /// on self-hosted (`unlimited`), or without a synced team slug.
    fn render_billing_section(
        &self,
        team: &domain::rows::Team,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::Div> {
        let BillingLoad::Ready(plan) = &self.billing else {
            return None;
        };
        if plan.plan == "unlimited" {
            return None; // self-hosted: no billing surface at all (web parity)
        }
        let plan_label: SharedString = match plan.plan.as_str() {
            "free" => "Free".into(),
            "pro" => "Pro".into(),
            "business" => "Business".into(),
            other => other.to_string().into(),
        };
        let plan_chip = div()
            .px_2()
            .py_0p5()
            .rounded_full()
            .border_1()
            .border_color(cx.theme().primary.opacity(0.4))
            .bg(cx.theme().primary.opacity(0.1))
            .text_xs()
            .child(plan_label);

        let fraction = |current: f64, limit: Option<f64>| {
            limit.filter(|limit| *limit > 0.).map(|limit| current / limit)
        };
        let seats = Self::usage_bar(
            "Seats",
            format!("{}", plan.usage.members.round() as i64),
            plan.limits
                .seats
                .map(|seats| format!("{}", seats.round() as i64)),
            fraction(plan.usage.members, plan.limits.seats),
            cx,
        );
        let storage = Self::usage_bar(
            "Attachment storage",
            Self::format_storage(plan.usage.storage_mb),
            plan.limits.storage_mb.map(Self::format_storage),
            fraction(plan.usage.storage_mb, plan.limits.storage_mb),
            cx,
        );
        let widgets = Self::usage_bar(
            "Feedback widgets",
            format!("{}", plan.usage.widget_configs.round() as i64),
            plan.limits
                .widget_configs
                .map(|widgets| format!("{}", widgets.round() as i64)),
            fraction(plan.usage.widget_configs, plan.limits.widget_configs),
            cx,
        );

        let mut body = section(cx)
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(card_header(
                        "Plan & Billing",
                        "Plan, seats, and usage — managed on the web.",
                        cx,
                    ))
                    .child(plan_chip),
            )
            .child(
                v_flex()
                    .gap_3()
                    .p_3()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_color(row_stroke(cx))
                    .child(seats)
                    .child(storage)
                    .child(widgets),
            );

        // Checkout/portal/seat changes are web flows (Creem runs in the
        // browser) — hand off to the team's billing settings page.
        if let Some(slug) = team.slug.clone() {
            if let Some(account) = crate::queries::active_account(cx) {
                let url = format!(
                    "{}/t/{slug}/settings/billing",
                    account.instance_url.trim_end_matches('/')
                );
                body = body.child(
                    h_flex().child(
                        Button::new("billing-manage")
                            .outline()
                            .small()
                            .icon(gpui_component::IconName::ExternalLink)
                            .label("Manage billing on the web")
                            .on_click(cx.listener(move |_, _, _, cx| {
                                open_url(cx, url.clone());
                            })),
                    ),
                );
            }
        }
        Some(body)
    }

    fn open_delete_dialog(
        &mut self,
        team_id: String,
        team_name: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.delete_input.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        // Re-opening the confirm clears the previous refusal.
        self.delete_error = None;
        cx.notify();
        let pane = cx.entity().downgrade();
        let content_input = self.delete_input.clone();
        let ok_input = self.delete_input.clone();
        let confirm_name = team_name.clone();
        let prompt = format!("Type {team_name} to confirm");
        // The typed-confirm block rides as extra content between the
        // description and the ok/cancel footer.
        let spec = AlertSpec::new(
            "Delete team",
            format!(
                "This will permanently delete {team_name} and all its boards, \
                 issues, and data. This cannot be undone."
            ),
            "Delete team",
        )
        .ok_variant(ButtonVariant::Danger)
        .height(gpui::px(320.))
        .content(move |_, cx| {
            v_flex()
                .gap_1()
                .mt_2()
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(SharedString::from(prompt.clone())),
                )
                .child(Input::new(&content_input).small())
                .into_any_element()
        })
        .on_ok(move |_, cx| {
            let typed = ok_input.read(cx).value().trim().to_string();
            if typed != confirm_name {
                // Mismatch keeps the dialog open (web disables the button
                // until it matches).
                return false;
            }
            let Some(trpc) = crate::queries::trpc_client(cx) else {
                log::warn!("[ui] teams.delete skipped: no signed-in account");
                return true;
            };
            let team_id = team_id.clone();
            let pane = pane.clone();
            // Not fire-and-forget like the other Danger Zone mutations: the
            // server REFUSES a team whose subscription is still live (REV2-55,
            // PRECONDITION_FAILED), so the failure has to land on the screen.
            cx.spawn(async move |cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move { api::teams::teams_delete(&trpc, &team_id) })
                    .await;
                let _ = pane.update(cx, |this, cx| {
                    if let Err(err) = &result {
                        log::warn!("[ui] teams.delete failed: {err}");
                        this.delete_error = Some(team_delete_error_message(err));
                        cx.notify();
                    }
                    // Success needs no action: the Electric echo drops the
                    // team and navigation re-scopes.
                });
            })
            .detach();
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }
}

impl Render for GeneralPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(team) = active_team(cx, &self.nav) else {
            return v_flex().child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No team selected."),
            );
        };
        let solo = !show_team_chrome(cx, &team.id);
        // Web parity: `if (solo) return null` — solo users don't see the
        // "team" concept, so the name card (a name nobody else sees) is
        // hidden. Visibility is deliberately not configurable (v6).
        if solo {
            return v_flex();
        }
        let owner = is_owner(cx, &team.id);
        let dirty = self.dirty(cx);
        let saving = self.saving;

        let mut general = section(cx)
            .child(card_header("General", "Team name", cx))
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Name"),
                    )
                    .child(Input::new(&self.name_input).small().disabled(!owner)),
            );

        if let Some(error) = &self.error {
            general = general.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            );
        }

        general = general.child(
            h_flex().justify_end().child(
                Button::new("team-save")
                    .primary()
                    .small()
                    .label(if saving { "Saving…" } else { "Save changes" })
                    .disabled(!owner || !dirty || saving)
                    .loading(saving)
                    .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
            ),
        );

        let mut pane = v_flex().gap_4().child(general);

        // EXP-288: read-only Plan & Billing between the name card and the
        // Danger Zone (fetch kicked at render like the boards repo cache).
        self.ensure_billing(&team.id, cx);
        if let Some(billing) = self.render_billing_section(&team, cx) {
            pane = pane.child(billing);
        }

        // Danger Zone (web settings/index.tsx): owner + team-only (the solo
        // case already returned above).
        if owner {
            let team_id = team.id.clone();
            let team_name = team.name.clone();
            pane = pane.child(
                v_flex()
                    .w_full()
                    .gap_3()
                    .p_4()
                    .border_1()
                    .border_color(cx.theme().danger.opacity(0.5))
                    .rounded(cx.theme().radius_lg)
                    // EXP-285 made `list_head` transparent, which silently
                    // flattened this card into the page gradient. The Danger
                    // Zone must stay a distinct surface, so take the same
                    // danger tint the sibling `error_notice` card uses.
                    .bg(cx.theme().danger.opacity(0.1))
                    .child(
                        v_flex()
                            .gap_0p5()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(cx.theme().danger)
                                    .child("Danger Zone"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Permanently delete this team and all its data."),
                            ),
                    )
                    .child(
                        h_flex().child(
                            Button::new("team-delete")
                                .danger()
                                .small()
                                .label("Delete team")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_delete_dialog(
                                        team_id.clone(),
                                        team_name.clone(),
                                        window,
                                        cx,
                                    );
                                })),
                        ),
                    )
                    // Web parity (settings/general.tsx): a refused delete —
                    // the REV2-55 billing gate above all — is shown, never
                    // swallowed.
                    .when_some(self.delete_error.clone(), |zone, message| {
                        zone.child(error_notice(message, cx))
                    }),
            );
        }

        pane.when(!owner, |pane| {
            pane.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Only team owners can change these settings."),
            )
        })
    }
}
