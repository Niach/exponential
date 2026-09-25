//! Settings → Issues (EXP-630): how issues behave team-wide — the estimate
//! scale (Linear's "Estimates" setting, off by default) and the PR
//! automation card (EXP-319, moved here from the Statuses pane). Labels and
//! Statuses hang under this entry in the nav.
//!
//! Web parity: `components/team/issues-section.tsx`. Member-visible like the
//! web page: the PR automation pickers are member-managed, the scale is
//! owner-only (`teams.update`) and reads as plain text for everyone else.

use gpui::{
    div, px, App, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, Subscription, Window,
};
use gpui_component::{
    button::Button,
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    v_flex, ActiveTheme as _, Icon, Sizable as _,
};
use sync::Store;

use domain::rows::IssueStatusRow;
use domain::statuses::{resolve_pr_target, resolve_row, IssueStatusCategory, ResolvedStatus};

use crate::controls::WebControl as _;
use crate::icons::registry;
use crate::navigation::{active_team_id, Navigation};

use super::{is_owner, section};

/// EXP-328: both PR-automation pickers share this width so their right-aligned
/// edges line up (web's `w-44` = 176px on the same two rows).
const PR_PICKER_WIDTH: f32 = 176.;

/// The picker key of the row that is NOT a status: `*Automation=false`
/// (web's `none`). It can never collide with a real key — those are a row
/// uuid or `builtin:<key>`.
const PR_TARGET_NONE: &str = "none";


/// The settings picker rows, in Linear's order and wording (web
/// `ESTIMATION_TYPE_OPTIONS`).
const ESTIMATION_OPTIONS: &[(&str, &str, &str)] = &[
    (domain::contract::ISSUE_ESTIMATION_NONE, "Not in use", ""),
    (domain::contract::ISSUE_ESTIMATION_EXPONENTIAL, "Exponential", "1, 2, 4, 8, 16 points"),
    (domain::contract::ISSUE_ESTIMATION_FIBONACCI, "Fibonacci", "1, 2, 3, 5, 8 points"),
    (domain::contract::ISSUE_ESTIMATION_LINEAR, "Linear", "1, 2, 3, 4, 5 points"),
    (domain::contract::ISSUE_ESTIMATION_TSHIRT, "T-shirt", "XS, S, M, L, XL"),
];

/// "T-shirt (XS, S, M, L, XL)" / "Not in use" — the trigger and the
/// read-only text.
fn estimation_label(value: &str) -> String {
    let (_, label, hint) = ESTIMATION_OPTIONS
        .iter()
        .find(|(wire, _, _)| *wire == value)
        .copied()
        .unwrap_or(ESTIMATION_OPTIONS[0]);
    if hint.is_empty() {
        label.to_string()
    } else {
        format!("{label} ({hint})")
    }
}

pub struct IssuesPane {
    nav: Entity<Navigation>,
    /// Inline error under the Estimates card.
    estimation_error: Option<String>,
    /// Inline error under the PR-automation card (EXP-774, web parity: the
    /// card holds ONE error for its three writes).
    pr_automation_error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl IssuesPane {
    pub fn new(nav: Entity<Navigation>, cx: &mut gpui::Context<Self>) -> Self {
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&nav, |this, _, cx| {
                this.estimation_error = None;
                this.pr_automation_error = None;
                cx.notify();
            }),
            // Both cards read the synced teams row — without this the
            // mutation's Electric echo never re-renders them.
            cx.observe(&collections.teams, |_, _, cx| cx.notify()),
            cx.observe(&collections.team_members, |_, _, cx| cx.notify()),
            cx.observe(&collections.issue_statuses, |_, _, cx| cx.notify()),
        ];
        Self {
            nav,
            estimation_error: None,
            pr_automation_error: None,
            _subscriptions: subscriptions,
        }
    }

    fn scoped_statuses(&self, cx: &App) -> Vec<(IssueStatusRow, ResolvedStatus)> {
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return Vec::new();
        };
        let sorted = crate::queries::team_statuses(cx, &team_id);
        (0..sorted.len())
            .map(|index| (sorted[index].clone(), resolve_row(&sorted, index)))
            .collect()
    }

    /// EXP-630 — the Estimates card: one row, the scale picker on the right
    /// (owners) or the current scale as text (members).
    fn render_estimates(&self, cx: &mut gpui::Context<Self>) -> Option<gpui::Div> {
        let team = super::active_team(cx, &self.nav)?;
        let owner = is_owner(cx, &team.id);
        let current = team.estimation().to_string();
        let trigger_label: SharedString = estimation_label(&current).into();

        let control: gpui::AnyElement = if owner {
            let pane = cx.entity().downgrade();
            let team_id = team.id.clone();
            let current_wire = current.clone();
            Button::new("issues-estimation-scale")
                .outline()
                .cursor_pointer()
                .web_input_sm()
                .w(px(256.))
                .child(
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .gap_1p5()
                        .items_center()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(trigger_label),
                        )
                        .child(
                            Icon::new(registry::UI_CHEVRON_DOWN)
                                .size_3()
                                .flex_shrink_0()
                                .text_color(cx.theme().muted_foreground),
                        ),
                )
                .dropdown_menu(move |mut menu, _window, _cx| {
                    for (wire, label, hint) in ESTIMATION_OPTIONS {
                        let text: SharedString = if hint.is_empty() {
                            (*label).into()
                        } else {
                            format!("{label}  ({hint})").into()
                        };
                        let team_id = team_id.clone();
                        let pane = pane.clone();
                        let picked = *wire;
                        menu = menu.item(
                            PopupMenuItem::new(text)
                                .checked(current_wire == *wire)
                                .on_click(move |_, _window, cx| {
                                    set_estimation(pane.clone(), team_id.clone(), picked, cx);
                                }),
                        );
                    }
                    menu
                })
                .into_any_element()
        } else {
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(trigger_label)
                .into_any_element()
        };

        let mut card = section(cx)
            .child(crate::surface::glass_section_header("Estimates", None, cx))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .justify_between()
                    .child(
                        v_flex()
                            .min_w_0()
                            .child(div().text_sm().child("Estimate scale")),
                    )
                    .child(control),
            );
        if let Some(message) = self.estimation_error.clone() {
            card = card.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .px_1()
                    .child(SharedString::from(message)),
            );
        }
        Some(card)
    }

    fn set_estimation_error<T>(
        &mut self,
        result: Result<T, api::ApiError>,
        cx: &mut gpui::Context<Self>,
    ) {
        match result {
            Ok(_) => {
                if self.estimation_error.take().is_some() {
                    cx.notify();
                }
            }
            Err(err) => {
                log::warn!("[ui] issues estimate scale write failed: {err}");
                self.estimation_error =
                    Some(super::form_error(&err, "Failed to save changes."));
                cx.notify();
            }
        }
    }

    /// The PR-automation card's error writer (EXP-774): one slot for the
    /// card's three writes, cleared by the next success.
    fn set_pr_automation_error<T>(
        &mut self,
        result: Result<T, api::ApiError>,
        cx: &mut gpui::Context<Self>,
    ) {
        match result {
            Ok(_) => {
                if self.pr_automation_error.take().is_some() {
                    cx.notify();
                }
            }
            Err(err) => {
                log::warn!("[ui] statuses PR automation write failed: {err}");
                self.pr_automation_error =
                    Some(super::form_error(&err, "Failed to update PR automation."));
                cx.notify();
            }
        }
    }

    // -- rendering -----------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    /// EXP-319 — the "PR automation" card: where issues move when their
    /// pull request opens/merges, per event a status picker (duplicate
    /// excluded) plus "Do nothing". Reads the synced teams row; writes
    /// converge via the Electric echo (no local pending state — the picker
    /// idiom everywhere else) and a rejection lands under the card (EXP-774).
    fn render_pr_automation(
        &self,
        statuses: &[(IssueStatusRow, ResolvedStatus)],
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::Div> {
        let team = super::active_team(cx, &self.nav)?;
        let rows: Vec<IssueStatusRow> =
            statuses.iter().map(|(row, _)| row.clone()).collect();

        let mut card = section(cx)
            .child(crate::surface::glass_section_header("PR automation", None, cx));
        let pane = cx.entity().downgrade();

        let events: [(
            &'static str,
            &'static str,
            api::statuses::PrAutomationEvent,
            Option<String>,
            Option<bool>,
            &'static str,
        ); 2] = [
            (
                "pr-automation-opened",
                "When a pull request opens, move issues to",
                api::statuses::PrAutomationEvent::Opened,
                team.pr_opened_status_id.clone(),
                team.pr_opened_automation,
                "in_review",
            ),
            (
                "pr-automation-merged",
                "When a pull request merges, move issues to",
                api::statuses::PrAutomationEvent::Merged,
                team.pr_merged_status_id.clone(),
                team.pr_merged_automation,
                "done",
            ),
        ];

        for (id, label, event, status_id, automation, default_builtin) in events {
            let current =
                resolve_pr_target(&rows, status_id.as_deref(), automation, default_builtin);
            let trigger_label: SharedString = current
                .as_ref()
                .map(|status| SharedString::from(status.name.clone()))
                .unwrap_or_else(|| "Do nothing".into());
            // EXP-328: a DEFINITE width so both rows' triggers line up (web
            // `w-44` parity). The label rides a child row rather than
            // `Button::label` — that one is `flex_none` and would spill out of
            // the fixed box; here it ellipsizes inside the definite-width
            // chain (button → `size_full` inner row → `w_full` child).
            let trigger = Button::new(id)
                .outline().cursor_pointer()
                .web_input_sm()
                .w(px(PR_PICKER_WIDTH))
                .child(
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .gap_1p5()
                        .items_center()
                        .children(current.as_ref().map(|status| {
                            crate::icons::resolved_status_icon(status, cx)
                                .xsmall()
                                .flex_shrink_0()
                        }))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(trigger_label),
                        )
                        .child(
                            Icon::new(registry::UI_CHEVRON_DOWN)
                                .size_3()
                                .flex_shrink_0()
                                .text_color(cx.theme().muted_foreground),
                        ),
                );

            let current_key = current
                .as_ref()
                .map(|status| status.group_key.clone())
                .unwrap_or_else(|| PR_TARGET_NONE.to_string());
            let team_id = team.id.clone();
            // Every offered entry is a real synced row (the pane renders
            // "Loading…" while statuses are empty), so a pick always
            // carries a row uuid — the constructed `builtin:` fallback
            // vocabulary can never leak into this write.
            let candidates: Vec<ResolvedStatus> = statuses
                .iter()
                .filter(|(_, resolved)| resolved.category != IssueStatusCategory::Duplicate)
                .filter(|(_, resolved)| resolved.row_id.is_some())
                .map(|(_, resolved)| resolved.clone())
                .collect();
            // EXP-1021: the team's statuses as THE status picker's own rows
            // (glyph in its colour, `status_picker::status_items`), plus the
            // one row that is not a status: `*Automation=false` turns the
            // automation off entirely, so "Do nothing" rides the same list —
            // exactly how the web card builds it.
            let mut items = crate::picker::status_picker::status_items(&candidates);
            items.push(
                crate::picker::PickerItem::new(PR_TARGET_NONE.to_string(), "Do nothing")
                    // Web marks this row with `ban`; the registry maps that
                    // glyph to `relation-blocks` alone, and a new concept is
                    // minted in `packages/icons` for all four clients at once,
                    // never here — the neutral minus keeps the row in the same
                    // glyph column meanwhile.
                    .icon(Icon::new(registry::UI_MINUS))
                    .color(cx.theme().muted_foreground),
            );
            // `group_key` is what the rows are keyed on; the write needs the
            // row uuid behind it (and the sentinel has none, which is what
            // makes it the "off" target).
            let targets: std::collections::HashMap<String, String> = candidates
                .iter()
                .filter_map(|status| Some((status.group_key.clone(), status.row_id.clone()?)))
                .collect();
            // The picker closure moves its own handle; the card's outlives
            // the loop for the switch row below.
            let pane = pane.clone();
            let picker_id = SharedString::from(format!("{id}-picker"));
            let control = crate::picker::deferred(move |window, cx| {
                crate::picker::Picker::single(
                    items,
                    Some(current_key),
                    trigger.into_any_element(),
                    std::rc::Rc::new(move |next: Vec<String>, _window, cx: &mut App| {
                        let Some(key) = next.into_iter().next() else {
                            return;
                        };
                        let target = match targets.get(&key) {
                            Some(row_id) => {
                                api::statuses::PrAutomationTarget::Status(row_id.clone())
                            }
                            None => api::statuses::PrAutomationTarget::DoNothing,
                        };
                        set_pr_automation(pane.clone(), team_id.clone(), event, target, cx);
                    }),
                )
                .id(picker_id)
                .render(window, cx)
            });

            card = card.child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().child(label))
                    .child(control),
            );
        }

        // EXP-711: merge ends the PR's live coding sessions by default
        // (EXP-498); the switch turns that off team-wide. Web parity: the
        // third row of the same card, sub-hint included.
        let ends_sessions = team.ends_sessions_on_merge();
        let team_id = team.id.clone();
        let switch_pane = pane.clone();
        card = card.child(
            h_flex()
                .gap_2()
                .items_center()
                .justify_between()
                .child(
                    v_flex()
                        .min_w_0()
                        .child(
                            div()
                                .text_sm()
                                .child("When a pull request merges, end its coding sessions"),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(
                                    "The session that merged its own pull request always keeps running.",
                                ),
                        ),
                )
                .child(
                    crate::controls::web_switch("pr-automation-end-sessions")
                        .checked(ends_sessions)
                        .on_click(move |checked: &bool, _window, cx| {
                            let team_id = team_id.clone();
                            let enabled = *checked;
                            let pane = switch_pane.clone();
                            let Some(trpc) = crate::queries::trpc_client(cx) else {
                                return;
                            };
                            cx.spawn(async move |cx| {
                                let result = cx
                                    .background_executor()
                                    .spawn(async move {
                                        api::statuses::statuses_set_end_sessions_on_merge(
                                            &trpc, &team_id, enabled,
                                        )
                                    })
                                    .await;
                                let _ = pane.update(cx, |this, cx| {
                                    this.set_pr_automation_error(result, cx);
                                });
                            })
                            .detach();
                        }),
                ),
        );

        // EXP-774 (web parity): the card's one inline error line.
        if let Some(message) = self.pr_automation_error.clone() {
            card = card.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .px_1()
                    .child(SharedString::from(message)),
            );
        }

        Some(card)
    }
}

impl Render for IssuesPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let statuses = self.scoped_statuses(cx);
        let mut pane = v_flex().gap_6();
        if let Some(card) = self.render_estimates(cx) {
            pane = pane.child(card);
        }
        if let Some(card) = self.render_pr_automation(&statuses, cx) {
            pane = pane.child(card);
        }
        pane
    }
}

/// One estimate-scale write (`teams.update`), awaited off the foreground
/// thread; the Electric echo re-renders the trigger.
fn set_estimation(pane: gpui::WeakEntity<IssuesPane>, team_id: String, scale: &'static str, cx: &mut App) {
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move {
                let mut input = api::teams::TeamsUpdateInput::new(team_id);
                input.estimation_type = Some(scale.to_string());
                api::teams::teams_update(&trpc, &input)
            })
            .await;
        let _ = pane.update(cx, |this, cx| {
            this.set_estimation_error(result, cx);
        });
    })
    .detach();
}

/// One PR-automation write (status pick / "Do nothing"): awaited off the
/// foreground thread, result routed to the pane's card-level error slot
/// (EXP-774). Menu callbacks only see `&mut App`, hence the weak pane.
fn set_pr_automation(
    pane: gpui::WeakEntity<IssuesPane>,
    team_id: String,
    event: api::statuses::PrAutomationEvent,
    target: api::statuses::PrAutomationTarget,
    cx: &mut App,
) {
    let Some(trpc) = crate::queries::trpc_client(cx) else {
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move {
                api::statuses::statuses_set_pr_automation(&trpc, &team_id, event, &target)
            })
            .await;
        let _ = pane.update(cx, |this, cx| {
            this.set_pr_automation_error(result, cx);
        });
    })
    .detach();
}

