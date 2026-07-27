//! Settings → Issue statuses (EXP-314).
//!
//! Web parity: `components/team/statuses-section.tsx` — one section per
//! `IssueStatusCategory::SETTINGS_ORDER` entry, rows carrying the tinted
//! status glyph + name + live issue count, and a per-category "+" footer with
//! the labels pane's inline create form (name input + `ColorSwatchGrid`).
//!
//! The 7 BUILTIN rows (`builtin_key != None`) are locked: never renamed,
//! recolored or deleted — only MOVED within their category (reordering the
//! started statuses is how a team shapes the pie-clock fills). Custom rows get
//! the full labels treatment: inline rename on blur/Enter, a swatch popover on
//! the glyph, move, and delete.
//!
//! Deleting a custom status that still holds issues needs a REPLACEMENT: the
//! row opens an inline picker and `statuses.delete` reassigns every issue in
//! one transaction. The server's count includes trashed-board issues, so the
//! synced count here can undershoot — its `PRECONDITION_FAILED` message is
//! surfaced inline verbatim when that happens.
//!
//! Writes are member-level (`mutate_resources`, like labels — NOT owner-only).
//! Reads come from the synced `issue_statuses` collection.

use std::collections::HashMap;

use gpui::{
    div, App, AppContext as _, ElementId, Entity, IntoElement,
    ParentElement, Render, SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputEvent, InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
    popover::Popover,
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _,
};
use sync::Store;

use domain::contract::ISSUE_STATUS_STARTED_MAX;
use domain::rows::IssueStatusRow;
use domain::statuses::{
    resolve_row, resolve_status_sorted, IssueStatusCategory, ResolvedStatus,
};

use crate::navigation::{active_team_id, Navigation};

use super::labels::{swatch_grid, LABEL_COLORS};
use super::{card_header, section};

/// Web parity with the labels pane's duplicate message (the server's unique is
/// `(team_id, lower(name))` across ALL statuses, builtins included).
const DUPLICATE_NAME_MESSAGE: &str = "A status with this name already exists.";

pub struct StatusesPane {
    nav: Entity<Navigation>,
    /// CUSTOM status id → its name input (builtins render plain text).
    name_inputs: HashMap<String, Entity<InputState>>,
    input_subs: HashMap<String, Subscription>,
    /// A status with ZERO issues awaiting the inline "Delete?" confirm.
    confirming_delete: Option<String>,
    /// A status with issues awaiting a replacement pick.
    reassigning: Option<String>,
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
                this.confirming_delete = None;
                this.reassigning = None;
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
            confirming_delete: None,
            reassigning: None,
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
                    this.create_error = Some(error_message(err));
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

    /// `statuses.delete` with the row's inline error surfacing — the
    /// reassign-required `PRECONDITION_FAILED` must reach the user verbatim
    /// (the synced count can undershoot the server's).
    fn delete(&mut self, status_id: String, reassign_to: Option<String>, cx: &mut gpui::Context<Self>) {
        let Some(team_id) = self.team_id(cx) else {
            return;
        };
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            return;
        };
        self.confirming_delete = None;
        self.reassigning = None;
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
                    this.row_error = Some((status_id.clone(), error_message(err)));
                    cx.notify();
                }
            });
        })
        .detach();
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
        let confirming = self.confirming_delete.as_deref() == Some(status_id.as_str());
        let reassigning = self.reassigning.as_deref() == Some(status_id.as_str());

        let mut line = h_flex()
            .gap_3()
            .items_center()
            .px_3()
            .py_1p5()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(super::row_stroke(cx));

        // Leading glyph. A CUSTOM status' glyph doubles as its color swatch
        // trigger (the labels pane's dot popover, one control lighter).
        line = if builtin.is_some() {
            line.child(div().flex_shrink_0().child(glyph.clone().small()))
        } else {
            let current = row.color.clone().unwrap_or_default();
            let swatch_status = status_id.clone();
            let swatch_team = team_id.clone();
            line.child(
                Popover::new(row_id("status-color", &status_id))
                    .trigger(
                        Button::new(row_id("status-color-trigger", &status_id))
                            .ghost()
                            .xsmall()
                            .child(glyph.clone().small()),
                    )
                    .content(move |_, _, cx| {
                        let popover = cx.entity();
                        let status_id = swatch_status.clone();
                        let team_id = swatch_team.clone();
                        swatch_grid(
                            &format!("status-swatch-{status_id}"),
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
                line.child(Input::new(input).small().appearance(false).flex_1().min_w_0())
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
            line = line.child(
                div()
                    .px_1p5()
                    .rounded(cx.theme().radius)
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .border_1()
                    .border_color(super::row_stroke(cx))
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

        if confirming {
            let del_id = status_id.clone();
            line = line
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Delete?"),
                )
                .child(
                    Button::new(row_id("status-delete-confirm", &status_id))
                        .ghost()
                        .xsmall()
                        .icon(Icon::new(IconName::Check).text_color(cx.theme().danger))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.delete(del_id.clone(), None, cx);
                        })),
                )
                .child(
                    Button::new(row_id("status-delete-cancel", &status_id))
                        .ghost()
                        .xsmall()
                        .icon(IconName::Close)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.confirming_delete = None;
                            cx.notify();
                        })),
                );
        } else {
            // Move up / down — available on BUILTINS too (only name/color and
            // delete are locked). Disabled at the category edges, where the
            // server-side move is an idempotent no-op anyway.
            let up_row = row.clone();
            let down_row = row.clone();
            line = line
                .child(
                    Button::new(row_id("status-up", &status_id))
                        .ghost()
                        .xsmall()
                        .icon(IconName::ChevronUp)
                        .tooltip("Move up")
                        .disabled(first_in_category)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.move_status(&up_row, api::statuses::MoveDirection::Up, cx);
                        })),
                )
                .child(
                    Button::new(row_id("status-down", &status_id))
                        .ghost()
                        .xsmall()
                        .icon(IconName::ChevronDown)
                        .tooltip("Move down")
                        .disabled(last_in_category)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.move_status(&down_row, api::statuses::MoveDirection::Down, cx);
                        })),
                );
            if builtin.is_none() {
                let start_id = status_id.clone();
                let has_issues = count > 0;
                line = line.child(
                    Button::new(row_id("status-delete", &status_id))
                        .ghost()
                        .xsmall()
                        .icon(IconName::Delete)
                        .tooltip("Delete status")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if has_issues {
                                this.reassigning = Some(start_id.clone());
                            } else {
                                this.confirming_delete = Some(start_id.clone());
                            }
                            cx.notify();
                        })),
                );
            }
        }

        // The reassign picker: a status still holding issues needs somewhere
        // to put them before it can go.
        let reassign = reassigning.then(|| {
            let candidates: Vec<(String, String)> = siblings
                .iter()
                .filter(|(candidate, resolved)| {
                    candidate.id != status_id
                        && resolved.category != IssueStatusCategory::Duplicate
                })
                .map(|(candidate, _)| (candidate.id.clone(), candidate.name.clone()))
                .collect();
            let delete_id = status_id.clone();
            h_flex()
                .gap_2()
                .items_center()
                .px_3()
                .child(
                    div()
                        .flex_1()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(SharedString::from(format!(
                            "Move {count} issue{} to:",
                            if count == 1 { "" } else { "s" }
                        ))),
                )
                .child({
                    let pane = cx.entity().downgrade();
                    Button::new(row_id("status-reassign", &status_id))
                        .outline()
                        .xsmall()
                        .label("Choose replacement")
                        .dropdown_menu(move |mut menu, _window, _cx| {
                            menu = menu.scrollable(true).max_h(gpui::px(320.));
                            if candidates.is_empty() {
                                return menu.item(PopupMenuItem::label("No other status"));
                            }
                            for (candidate_id, name) in &candidates {
                                let pane = pane.clone();
                                let delete_id = delete_id.clone();
                                let candidate_id = candidate_id.clone();
                                menu = menu.item(
                                    PopupMenuItem::new(SharedString::from(name.clone()))
                                        .on_click(move |_, _, cx| {
                                            let _ = pane.update(cx, |this, cx| {
                                                this.delete(
                                                    delete_id.clone(),
                                                    Some(candidate_id.clone()),
                                                    cx,
                                                );
                                            });
                                        }),
                                );
                            }
                            menu
                        })
                })
                .child(
                    Button::new(row_id("status-reassign-cancel", &status_id))
                        .ghost()
                        .xsmall()
                        .label("Cancel")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.reassigning = None;
                            cx.notify();
                        })),
                )
        });

        let error = self
            .row_error
            .as_ref()
            .filter(|(id, _)| id == &status_id)
            .map(|(_, message)| message.clone());

        v_flex()
            .gap_1()
            .child(line)
            .children(reassign)
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
        v_flex()
            .gap_3()
            .p_3()
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(super::row_stroke(cx))
            .child(Input::new(&self.new_name).small())
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
                            .xsmall()
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
                            .xsmall()
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
        let custom = statuses
            .iter()
            .filter(|(row, _)| row.builtin_key.is_none())
            .count();

        let mut body = section(cx).child(card_header(
            "Issue statuses",
            format!(
                "{} status{} in this team ({custom} custom). Built-in statuses can be reordered but not renamed or deleted.",
                statuses.len(),
                if statuses.len() == 1 { "" } else { "es" },
            ),
            cx,
        ));

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

        for category in IssueStatusCategory::SETTINGS_ORDER {
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
                                    .xsmall()
                                    .icon(IconName::Plus)
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

        v_flex().child(body)
    }
}

/// An `ApiError`'s user-facing text: the server's clean tRPC message when it
/// has one (the duplicate-name CONFLICT, the started cap, the
/// reassign-required PRECONDITION_FAILED), else the transport error.
fn error_message(err: &api::ApiError) -> String {
    match err {
        api::ApiError::Http { message, .. } => message.clone(),
        other => other.to_string(),
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
