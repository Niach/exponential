//! Settings → Issue statuses (EXP-314).
//!
//! Web parity: `components/team/statuses-section.tsx` — one section per
//! `IssueStatusCategory::DISPLAY_ORDER` entry, rows carrying the tinted
//! status glyph + name + live issue count, and a per-category "+" footer with
//! the labels pane's inline create form (name input + `ColorSwatchGrid`).
//!
//! The 7 BUILTIN rows (`builtin_key != None`) are locked: never renamed,
//! recolored or deleted — only MOVED within their category (reordering the
//! started statuses is how a team shapes the pie-clock fills). Custom rows get
//! the full labels treatment: inline rename on blur/Enter, a swatch popover on
//! the glyph, move, and delete.
//!
//! Deleting a custom status ALWAYS opens one confirm dialog (EXP-320, web
//! parity with `ReassignDialog`): it confirms the deletion AND picks the
//! destination status (Backlog builtin preselected, duplicate + the deletee
//! excluded) in one step, regardless of the visible count — the server's
//! count includes trashed-board issues, so the synced count here can
//! undershoot and a "0-count" status may still hold issues. The dialog shows
//! the server-authoritative `statuses.referencingCount` once it arrives, and
//! always sends `reassignToId`, so the delete can never bounce
//! `PRECONDITION_FAILED`.
//!
//! Writes are member-level (`mutate_resources`, like labels — NOT owner-only).
//! Reads come from the synced `issue_statuses` collection.

use std::collections::HashMap;

use gpui::{
    div, px, App, AppContext as _, ElementId, Entity, IntoElement,
    ParentElement, Render, SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
    popover::Popover,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};
use sync::Store;

use domain::contract::ISSUE_STATUS_STARTED_MAX;
use domain::rows::IssueStatusRow;
use domain::statuses::{
    resolve_pr_target, resolve_row, resolve_status_sorted, IssueStatusCategory,
    ResolvedStatus,
};

use crate::controls::{WebControl as _, CTL_XS_H};
use crate::native_dialog::{self, AlertSpec};
use crate::navigation::{active_team_id, Navigation};

use super::labels::{swatch_grid, LABEL_COLORS, STATUS_COLORS};
use super::{card_title, section};
use crate::icons::registry;

/// Web parity with the labels pane's duplicate message (the server's unique is
/// `(team_id, lower(name))` across ALL statuses, builtins included).
const DUPLICATE_NAME_MESSAGE: &str = "A status with this name already exists.";

/// EXP-328: both PR-automation pickers share this width so their right-aligned
/// edges line up (web's `w-44` = 176px on the same two rows).
const PR_PICKER_WIDTH: f32 = 176.;

pub struct StatusesPane {
    nav: Entity<Navigation>,
    /// CUSTOM status id → its name input (builtins render plain text).
    name_inputs: HashMap<String, Entity<InputState>>,
    input_subs: HashMap<String, Subscription>,
    /// The category whose inline create form is open.
    creating: Option<IssueStatusCategory>,
    new_name: Entity<InputState>,
    new_color: String,
    submitting: bool,
    /// Inline error under the create form (duplicate name / server reject).
    create_error: Option<String>,
    /// (status id, message) — inline error under a row.
    row_error: Option<(String, String)>,
    _subscriptions: Vec<Subscription>,
}

impl StatusesPane {
    pub fn new(nav: Entity<Navigation>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let new_name = cx.new(|cx| InputState::new(window, cx).placeholder("Status name"));

        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe_in(&nav, window, |this, _, window, cx| {
                // Team switch: per-row inputs (and any inline state) belong to
                // the old scope.
                this.creating = None;
                this.create_error = None;
                this.row_error = None;
                this.sync_inputs(window, cx);
                cx.notify();
            }),
            cx.observe_in(&collections.issue_statuses, window, |this, _, window, cx| {
                this.sync_inputs(window, cx);
                cx.notify();
            }),
            // The live per-status issue counts move with the issues.
            cx.observe(&collections.issues, |_, _, cx| cx.notify()),
            // EXP-319: the PR-automation card reads the synced teams row —
            // without this the mutation's Electric echo never re-renders it.
            cx.observe(&collections.teams, |_, _, cx| cx.notify()),
            cx.subscribe_in(&new_name, window, |this, _, event: &InputEvent, _, cx| {
                match event {
                    InputEvent::PressEnter { .. } => this.create(cx),
                    InputEvent::Change => {
                        this.create_error = None;
                        cx.notify();
                    }
                    _ => {}
                }
            }),
        ];

        let mut this = Self {
            nav,
            name_inputs: HashMap::new(),
            input_subs: HashMap::new(),
            creating: None,
            new_name,
            new_color: LABEL_COLORS[6].to_string(),
            submitting: false,
            create_error: None,
            row_error: None,
            _subscriptions: subscriptions,
        };
        this.sync_inputs(window, cx);
        this
    }

    fn team_id(&self, cx: &App) -> Option<String> {
        active_team_id(&self.nav, cx)
    }

    /// The team's statuses in the canonical order, each paired with its
    /// resolved presentation (glyph + tint — the SAME resolution the board
    /// list renders, so the pane can never show a different icon).
    fn scoped_statuses(&self, cx: &App) -> Vec<(IssueStatusRow, ResolvedStatus)> {
        let Some(team_id) = self.team_id(cx) else {
            return Vec::new();
        };
        let sorted = crate::queries::team_statuses(cx, &team_id);
        (0..sorted.len())
            .map(|index| (sorted[index].clone(), resolve_row(&sorted, index)))
            .collect()
    }

    /// group key → how many of the team's synced issues sit in it. Counts
    /// pre-backfill `status_id = NULL` rows correctly (they resolve through
    /// their anchor to the matching builtin).
    fn issue_counts(&self, cx: &App) -> HashMap<String, usize> {
        let mut counts: HashMap<String, usize> = HashMap::new();
        let Some(team_id) = self.team_id(cx) else {
            return counts;
        };
        let sorted = crate::queries::team_statuses(cx, &team_id);
        for issue in Store::global(cx).collections().issues_in_team(&team_id, cx) {
            *counts
                .entry(resolve_status_sorted(&issue, &sorted).group_key)
                .or_default() += 1;
        }
        counts
    }

    /// Another status in the team already has this name (case-insensitive,
    /// matching the server's `(team_id, lower(name))` unique — builtins
    /// included, so a custom can never shadow "Done").
    fn is_duplicate_name(&self, name: &str, exclude_id: Option<&str>, cx: &App) -> bool {
        let needle = name.trim().to_lowercase();
        self.scoped_statuses(cx).iter().any(|(row, _)| {
            Some(row.id.as_str()) != exclude_id && row.name.trim().to_lowercase() == needle
        })
    }

    /// One `InputState` per CUSTOM status; builtins never get one (their name
    /// renders as plain text).
    fn sync_inputs(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let statuses = self.scoped_statuses(cx);
        let live: std::collections::HashSet<&str> = statuses
            .iter()
            .filter(|(row, _)| row.builtin_key.is_none())
            .map(|(row, _)| row.id.as_str())
            .collect();
        self.name_inputs.retain(|id, _| live.contains(id.as_str()));
        self.input_subs.retain(|id, _| live.contains(id.as_str()));

        for (row, _) in statuses.iter().filter(|(row, _)| row.builtin_key.is_none()) {
            if self.name_inputs.contains_key(&row.id) {
                continue;
            }
            let input =
                cx.new(|cx| InputState::new(window, cx).default_value(row.name.clone()));
            let status_id = row.id.clone();
            let sub = cx.subscribe_in(
                &input,
                window,
                move |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::PressEnter { .. } | InputEvent::Blur => {
                        this.persist_name(&status_id, window, cx);
                    }
                    InputEvent::Change => {
                        if this
                            .row_error
                            .as_ref()
                            .is_some_and(|(id, _)| id == &status_id)
                        {
                            this.row_error = None;
                            cx.notify();
                        }
                    }
                    _ => {}
                },
            );
            self.name_inputs.insert(row.id.clone(), input);
            self.input_subs.insert(row.id.clone(), sub);
        }
    }

    /// Labels-pane `persistName`: trim; empty or unchanged resets to the
    /// synced name, otherwise `statuses.update`.
    fn persist_name(&mut self, status_id: &str, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(input) = self.name_inputs.get(status_id).cloned() else {
            return;
        };
        let Some((row, _)) = self
            .scoped_statuses(cx)
            .into_iter()
            .find(|(row, _)| row.id == status_id)
        else {
            return;
        };
        let typed = input.read(cx).value().trim().to_string();
        if typed.is_empty() || typed == row.name {
            let name = row.name.clone();
            input.update(cx, |state, cx| state.set_value(name, window, cx));
            self.row_error = None;
            cx.notify();
            return;
        }
        if self.is_duplicate_name(&typed, Some(status_id), cx) {
            self.row_error = Some((status_id.to_string(), DUPLICATE_NAME_MESSAGE.to_string()));
            cx.notify();
            return;
        }
        self.row_error = None;
        let team_id = row.team_id.clone();
        let status_id = status_id.to_string();
        super::spawn_trpc(cx, "statuses.update(name)", move |trpc| {
            api::statuses::statuses_update(trpc, &team_id, &status_id, Some(&typed), None)
        });
    }

    fn create(&mut self, cx: &mut gpui::Context<Self>) {
        if self.submitting {
            return;
        }
        let (Some(team_id), Some(category)) = (self.team_id(cx), self.creating) else {
            return;
        };
        let Some(wire) = category.as_wire() else {
            return;
        };
        let name = self.new_name.read(cx).value().trim().to_string();
        if name.is_empty() || self.is_duplicate_name(&name, None, cx) {
            return;
        }
        let color = self.new_color.clone();
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            return;
        };
        let category = wire.to_string();

        self.submitting = true;
        self.create_error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::statuses::statuses_create(&trpc, &team_id, &category, &name, &color)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.submitting = false;
                if let Err(err) = &result {
                    log::warn!("[ui] statuses.create failed: {err}");
                    // The started cap and the duplicate-name CONFLICT both
                    // arrive here — show their clean message inline.
                    this.create_error = Some(err.user_message());
                } else {
                    this.creating = None;
                    this.reset_form();
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn reset_form(&mut self) {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as usize)
            .unwrap_or(0);
        self.new_color = LABEL_COLORS[nanos % LABEL_COLORS.len()].to_string();
        self.create_error = None;
    }

    /// `statuses.delete` with the row's inline error surfacing. The dialog
    /// always supplies `reassign_to`, so a rejection here is a genuine server
    /// error, not the reassign-required `PRECONDITION_FAILED`.
    fn delete(&mut self, status_id: String, reassign_to: Option<String>, cx: &mut gpui::Context<Self>) {
        let Some(team_id) = self.team_id(cx) else {
            return;
        };
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            return;
        };
        self.row_error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let call_id = status_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::statuses::statuses_delete(
                        &trpc,
                        &team_id,
                        &call_id,
                        reassign_to.as_deref(),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Err(err) = &result {
                    log::warn!("[ui] statuses.delete failed: {err}");
                    this.row_error = Some((status_id.clone(), err.user_message()));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The ONE delete flow (EXP-320): a native confirm dialog that also picks
    /// the destination status — always, regardless of the visible count (a
    /// "0-count" status can still hold issues on trashed boards). The dialog
    /// fetches the server-authoritative referencing count async and always
    /// sends `reassignToId`.
    #[allow(clippy::too_many_arguments)]
    fn open_delete_dialog(
        &mut self,
        status_id: String,
        status_name: String,
        synced_count: usize,
        candidates: Vec<(String, String)>,
        preselected: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(team_id) = self.team_id(cx) else {
            return;
        };
        self.row_error = None;
        cx.notify();

        let content = cx.new(|_| DeleteStatusContent {
            candidates,
            selected_id: preselected,
            server_count: None,
            synced_count,
        });

        // The server count (trashed-board issues included) lands async and
        // re-renders the dialog body — the content is its own entity exactly
        // so the alert view never has to re-render.
        if let Some(trpc) = crate::queries::trpc_client(cx) {
            let fetch_status = status_id.clone();
            let weak_content = content.downgrade();
            cx.spawn(async move |_, cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        api::statuses::statuses_referencing_count(
                            &trpc,
                            &team_id,
                            &fetch_status,
                        )
                    })
                    .await;
                match result {
                    Ok(out) => {
                        let _ = weak_content.update(cx, |state, cx| {
                            state.server_count = Some(out.count);
                            cx.notify();
                        });
                    }
                    Err(err) => {
                        // Non-fatal: the dialog copy stays hedged.
                        log::warn!("[ui] statuses.referencingCount failed: {err}");
                    }
                }
            })
            .detach();
        }

        let pane = cx.entity().downgrade();
        let dialog_content = content.clone();
        let spec = AlertSpec::new(
            format!("Delete {status_name}?"),
            "Issues using this status, including any on trashed boards, \
             will move to the status you pick.",
            "Delete status",
        )
        .ok_variant(ButtonVariant::Danger)
        .height(gpui::px(300.))
        .content(move |_, _| dialog_content.clone().into_any_element())
        .on_ok(move |_, cx| {
            let reassign_to = content.read(cx).selected_id.clone();
            let status_id = status_id.clone();
            let _ = pane.update(cx, |this, cx| {
                this.delete(status_id, Some(reassign_to), cx);
            });
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    fn move_status(
        &mut self,
        row: &IssueStatusRow,
        direction: api::statuses::MoveDirection,
        cx: &mut gpui::Context<Self>,
    ) {
        let team_id = row.team_id.clone();
        let status_id = row.id.clone();
        super::spawn_trpc(cx, "statuses.move", move |trpc| {
            api::statuses::statuses_move(trpc, &team_id, &status_id, direction)
        });
    }

    // -- rendering -----------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    /// EXP-319 — the "PR automation" card: where issues move when their
    /// pull request opens/merges, per event a status picker (duplicate
    /// excluded) plus "Do nothing". Reads the synced teams row; writes are
    /// fire-and-forget and converge via the Electric echo (no local pending
    /// state — the picker idiom everywhere else).
    fn render_pr_automation(
        &self,
        statuses: &[(IssueStatusRow, ResolvedStatus)],
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::Div> {
        let team = super::active_team(cx, &self.nav)?;
        let rows: Vec<IssueStatusRow> =
            statuses.iter().map(|(row, _)| row.clone()).collect();

        let mut card = section(cx).child(card_title("PR automation"));

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

            let current_key = current.as_ref().map(|status| status.group_key.clone());
            let team_id = team.id.clone();
            // Every offered entry is a real synced row (the pane renders
            // "Loading…" while statuses are empty), so a pick always
            // carries a row uuid — the constructed `builtin:` fallback
            // vocabulary can never leak into this write.
            let candidates: Vec<ResolvedStatus> = statuses
                .iter()
                .filter(|(_, resolved)| resolved.category != IssueStatusCategory::Duplicate)
                .map(|(_, resolved)| resolved.clone())
                .collect();

            card = card.child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().child(label))
                    .child(trigger.dropdown_menu(move |mut menu, _window, cx| {
                        menu = menu.scrollable(true).max_h(gpui::px(240.));
                        for status in &candidates {
                            let Some(row_id) = status.row_id.clone() else {
                                continue;
                            };
                            let team_id = team_id.clone();
                            menu = menu.item(crate::pickers::option_item(
                                SharedString::from(status.name.clone()),
                                crate::icons::resolved_status_icon(status, cx),
                                current_key.as_deref() == Some(status.group_key.as_str()),
                                move |_window, cx| {
                                    let team_id = team_id.clone();
                                    let target =
                                        api::statuses::PrAutomationTarget::Status(row_id.clone());
                                    super::spawn_trpc(cx, "statuses.setPrAutomation", move |trpc| {
                                        api::statuses::statuses_set_pr_automation(
                                            trpc, &team_id, event, &target,
                                        )
                                    });
                                },
                            ));
                        }
                        let team_id = team_id.clone();
                        menu.item(
                            PopupMenuItem::new("Do nothing")
                                .checked(current_key.is_none())
                                .on_click(move |_, _window, cx| {
                                    let team_id = team_id.clone();
                                    super::spawn_trpc(cx, "statuses.setPrAutomation", move |trpc| {
                                        api::statuses::statuses_set_pr_automation(
                                            trpc,
                                            &team_id,
                                            event,
                                            &api::statuses::PrAutomationTarget::DoNothing,
                                        )
                                    });
                                }),
                        )
                    })),
            );
        }

        Some(card)
    }

    fn render_status_row(
        &self,
        row: &IssueStatusRow,
        resolved: &ResolvedStatus,
        count: usize,
        first_in_category: bool,
        last_in_category: bool,
        siblings: &[(IssueStatusRow, ResolvedStatus)],
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let builtin = row.builtin_key.clone();
        let status_id = row.id.clone();
        let team_id = row.team_id.clone();
        let tint = crate::icons::status_tint_color(&resolved.tint, cx);
        let glyph = crate::icons::glyph_icon(resolved.glyph).text_color(tint);

        // EXP-698: the shared glass ROW CARD — the surrounding category column
        // is a gapped list, so each status is its own object, not a fused
        // group row.
        let mut line = crate::surface::glass_row_card()
            .flex()
            .w_full()
            .min_w_0()
            .gap_3()
            .items_center()
            .px_3()
            .py_1p5();

        // Leading glyph. A CUSTOM status' glyph doubles as its color swatch
        // trigger (the labels pane's dot popover, one control lighter). A
        // builtin's is inert — but it takes the SAME 24px slot, or the two
        // kinds of row would step out of line by the button's padding.
        line = if builtin.is_some() {
            line.child(
                div()
                    .size(gpui::px(CTL_XS_H))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(glyph.clone().small()),
            )
        } else {
            let current = row.color.clone().unwrap_or_default();
            let swatch_status = status_id.clone();
            let swatch_team = team_id.clone();
            line.child(
                Popover::new(row_id("status-color", &status_id))
                    .trigger(
                        Button::new(row_id("status-color-trigger", &status_id))
                            .ghost()
                            .web_icon_xs()
                            .child(glyph.clone().small()),
                    )
                    .content(move |_, _, cx| {
                        let popover = cx.entity();
                        let status_id = swatch_status.clone();
                        let team_id = swatch_team.clone();
                        swatch_grid(
                            &format!("status-swatch-{status_id}"),
                            &STATUS_COLORS,
                            Some(current.as_str()),
                            move |picked, window, cx| {
                                let team_id = team_id.clone();
                                let status_id = status_id.clone();
                                let picked = picked.to_string();
                                super::spawn_trpc(cx, "statuses.update(color)", move |trpc| {
                                    api::statuses::statuses_update(
                                        trpc,
                                        &team_id,
                                        &status_id,
                                        None,
                                        Some(&picked),
                                    )
                                });
                                popover.update(cx, |state, cx| state.dismiss(window, cx));
                            },
                            cx,
                        )
                    }),
            )
        };

        // Name: locked plain text for builtins, an inline input for customs.
        line = match self.name_inputs.get(&status_id) {
            Some(input) => {
                line.child(Input::new(input).web_input_sm().appearance(false).flex_1().min_w_0())
            }
            None => line.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .child(SharedString::from(row.name.clone())),
            ),
        };

        // The backlog builtin is where new issues land — web parity badge.
        if builtin.as_deref() == Some("backlog") {
            // EXP-698: the shared non-interactive glass chip, not a bespoke
            // bordered-but-unfilled badge.
            line = line.child(
                crate::surface::glass_chip()
                    .text_color(cx.theme().muted_foreground)
                    .child("Default"),
            );
        }

        line = line.child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(SharedString::from(format!(
                    "{count} issue{}",
                    if count == 1 { "" } else { "s" }
                ))),
        );

        // Move up / down — available on BUILTINS too (only name/color and
        // delete are locked). Disabled at the category edges, where the
        // server-side move is an idempotent no-op anyway.
        let up_row = row.clone();
        let down_row = row.clone();
        line = line
            .child(
                // EXP-698: the one 32px glass chrome every row action wears.
                crate::controls::glass_icon_button(
                    row_id("status-up", &status_id),
                    Icon::new(registry::UI_CHEVRON_UP),
                    cx,
                )
                    .tooltip("Move up")
                    .disabled(first_in_category)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.move_status(&up_row, api::statuses::MoveDirection::Up, cx);
                    })),
            )
            .child(
                crate::controls::glass_icon_button(
                    row_id("status-down", &status_id),
                    Icon::new(registry::UI_CHEVRON_DOWN),
                    cx,
                )
                    .tooltip("Move down")
                    .disabled(last_in_category)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.move_status(&down_row, api::statuses::MoveDirection::Down, cx);
                    })),
            );
        if builtin.is_none() {
            // Delete ALWAYS opens the one confirm-and-reassign dialog
            // (EXP-320) — candidates and the Backlog preselect are computed
            // here, where the resolved siblings are at hand.
            let candidates: Vec<(String, String)> = siblings
                .iter()
                .filter(|(candidate, resolved)| {
                    candidate.id != status_id
                        && resolved.category != IssueStatusCategory::Duplicate
                })
                .map(|(candidate, _)| (candidate.id.clone(), candidate.name.clone()))
                .collect();
            let preselected = siblings
                .iter()
                .find(|(candidate, _)| candidate.builtin_key.as_deref() == Some("backlog"))
                .map(|(candidate, _)| candidate.id.clone())
                .or_else(|| candidates.first().map(|(id, _)| id.clone()));
            let del_id = status_id.clone();
            let del_name = row.name.clone();
            line = line.child(
                crate::controls::glass_icon_button(
                    row_id("status-delete", &status_id),
                    Icon::new(registry::UI_DELETE),
                    cx,
                )
                    .tooltip("Delete status")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let Some(preselected) = preselected.clone() else {
                            return;
                        };
                        this.open_delete_dialog(
                            del_id.clone(),
                            del_name.clone(),
                            count,
                            candidates.clone(),
                            preselected,
                            window,
                            cx,
                        );
                    })),
            );
        } else {
            // Builtins cannot be deleted — reserve the button's width so the
            // counts and badges stay column-aligned with the custom rows.
            line = line.child(
                div()
                    .size(gpui::px(crate::controls::CTL_SM_H))
                    .flex_shrink_0(),
            );
        }

        let error = self
            .row_error
            .as_ref()
            .filter(|(id, _)| id == &status_id)
            .map(|(_, message)| message.clone());

        v_flex()
            .gap_1()
            .child(line)
            .when_some(error, |col, message| {
                col.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .px_1()
                        .child(SharedString::from(message)),
                )
            })
    }

    /// The inline create form (the labels pane's, verbatim) for one category.
    fn render_create_form(
        &self,
        category: IssueStatusCategory,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let name = self.new_name.read(cx).value().trim().to_string();
        let duplicate = !name.is_empty() && self.is_duplicate_name(&name, None, cx);
        let form_error = if duplicate {
            Some(DUPLICATE_NAME_MESSAGE.to_string())
        } else {
            self.create_error.clone()
        };
        let entity = cx.entity();
        // EXP-698: the inline form is one more object in the category's gapped
        // list, so it wears the glass row card.
        crate::surface::glass_row_card()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .child(Input::new(&self.new_name).web_input_sm())
            .when_some(form_error, |col, message| {
                col.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(SharedString::from(message)),
                )
            })
            .child(
                v_flex()
                    .gap_1p5()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Color"),
                    )
                    .child(swatch_grid(
                        "new-status-swatch",
                        &STATUS_COLORS,
                        Some(self.new_color.as_str()),
                        move |picked, _, cx| {
                            let picked = picked.to_string();
                            entity.update(cx, |this, cx| {
                                this.new_color = picked;
                                cx.notify();
                            });
                        },
                        cx,
                    )),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new(category_id("status-create", category))
                            .primary()
                            .web_xs()
                            .label(if self.submitting {
                                "Creating…"
                            } else {
                                "Create status"
                            })
                            .disabled(name.is_empty() || self.submitting || duplicate)
                            .loading(self.submitting)
                            .on_click(cx.listener(|this, _, _, cx| this.create(cx))),
                    )
                    .child(
                        Button::new(category_id("status-create-cancel", category))
                            .ghost()
                            .web_xs()
                            .label("Cancel")
                            .disabled(self.submitting)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.creating = None;
                                this.new_name
                                    .update(cx, |state, cx| state.set_value("", window, cx));
                                this.reset_form();
                                cx.notify();
                            })),
                    ),
            )
    }
}

impl Render for StatusesPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let statuses = self.scoped_statuses(cx);
        let counts = self.issue_counts(cx);

        let mut body = section(cx).child(card_title("Issue statuses"));

        if statuses.is_empty() {
            return v_flex().child(body.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .py_2()
                    .child(
                        if Store::global(cx)
                            .collections()
                            .issue_statuses
                            .read(cx)
                            .is_ready()
                        {
                            "No statuses in this team."
                        } else {
                            "Loading…"
                        },
                    ),
            ));
        }

        for category in IssueStatusCategory::DISPLAY_ORDER {
            let rows: Vec<(IssueStatusRow, ResolvedStatus)> = statuses
                .iter()
                .filter(|(_, resolved)| resolved.category == category)
                .cloned()
                .collect();

            let mut group = v_flex().gap_2().child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(category.label()),
            );
            // A category can be EMPTY — since EXP-685 retired the builtin
            // Todo, `unstarted` starts out with no rows at all. Say so
            // instead of leaving the heading hanging over the "Add status".
            if rows.is_empty() {
                group = group.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("No statuses yet."),
                );
            }
            for (index, (row, resolved)) in rows.iter().enumerate() {
                group = group.child(self.render_status_row(
                    row,
                    resolved,
                    counts.get(&resolved.group_key).copied().unwrap_or(0),
                    index == 0,
                    index + 1 == rows.len(),
                    &statuses,
                    cx,
                ));
            }

            // The Duplicate category is fixed at exactly one status — no "+".
            if category != IssueStatusCategory::Duplicate {
                if self.creating == Some(category) {
                    group = group.child(self.render_create_form(category, cx));
                } else {
                    // The pie-clock fill tables are defined only up to
                    // ISSUE_STATUS_STARTED_MAX started statuses.
                    let capped = category == IssueStatusCategory::Started
                        && rows.len() >= ISSUE_STATUS_STARTED_MAX;
                    group = group.child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                Button::new(category_id("status-new", category))
                                    .outline()
                                    .web_xs()
                                    .icon(registry::UI_ADD)
                                    .label("Add status")
                                    .disabled(capped)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.creating = Some(category);
                                        this.create_error = None;
                                        this.new_name.update(cx, |state, cx| {
                                            state.set_value("", window, cx)
                                        });
                                        cx.notify();
                                    })),
                            )
                            .when(capped, |row| {
                                row.child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(SharedString::from(format!(
                                            "At most {ISSUE_STATUS_STARTED_MAX} started statuses"
                                        ))),
                                )
                            }),
                    );
                }
            }

            body = body.child(group);
        }

        let mut pane = v_flex().gap_6().child(body);
        if let Some(card) = self.render_pr_automation(&statuses, cx) {
            pane = pane.child(card);
        }
        pane
    }
}

/// The delete dialog's body (EXP-320): the live referencing count + the
/// destination picker. Its own entity so the async `referencingCount` fetch
/// can re-render just this block (the surrounding alert never re-renders).
struct DeleteStatusContent {
    /// (id, name) — the duplicate category and the deletee excluded.
    candidates: Vec<(String, String)>,
    selected_id: String,
    /// Server-authoritative count (trashed-board issues included); `None`
    /// while loading or after a failed fetch — the copy stays hedged then.
    server_count: Option<i64>,
    /// The client-visible count — flags when part of the total is on
    /// trashed boards the client can't see.
    synced_count: usize,
}

impl Render for DeleteStatusContent {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let count_line: SharedString = match self.server_count {
            None => "Counting issues that use this status…".into(),
            Some(0) => "No issues use this status right now.".into(),
            Some(n) => {
                let trashed = if n as usize > self.synced_count {
                    " (some on trashed boards)"
                } else {
                    ""
                };
                format!(
                    "{n} issue{}{trashed} will move.",
                    if n == 1 { "" } else { "s" }
                )
                .into()
            }
        };
        let selected_name: SharedString = self
            .candidates
            .iter()
            .find(|(id, _)| id == &self.selected_id)
            .map(|(_, name)| SharedString::from(name.clone()))
            .unwrap_or_else(|| "Choose status".into());
        let candidates = self.candidates.clone();
        let picker = cx.entity().downgrade();

        v_flex()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(count_line),
            )
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(div().text_sm().child("Move issues to"))
                    .child(
                        Button::new("status-delete-reassign-target")
                            .outline().cursor_pointer()
                            .web_input_sm()
                            .label(selected_name)
                            .dropdown_menu(move |mut menu, _window, _cx| {
                                menu = menu.scrollable(true).max_h(gpui::px(240.));
                                if candidates.is_empty() {
                                    return menu.item(PopupMenuItem::label("No other status"));
                                }
                                for (candidate_id, name) in &candidates {
                                    let picker = picker.clone();
                                    let candidate_id = candidate_id.clone();
                                    menu = menu.item(
                                        PopupMenuItem::new(SharedString::from(name.clone()))
                                            .on_click(move |_, _, cx| {
                                                let _ = picker.update(cx, |state, cx| {
                                                    state.selected_id = candidate_id.clone();
                                                    cx.notify();
                                                });
                                            }),
                                    );
                                }
                                menu
                            }),
                    ),
            )
    }
}

use gpui::prelude::FluentBuilder as _;

fn row_id(kind: &str, id: &str) -> ElementId {
    ElementId::Name(SharedString::from(format!("{kind}-{id}")))
}

fn category_id(kind: &str, category: IssueStatusCategory) -> ElementId {
    ElementId::Name(SharedString::from(format!(
        "{kind}-{}",
        category.as_wire().unwrap_or("unknown")
    )))
}
