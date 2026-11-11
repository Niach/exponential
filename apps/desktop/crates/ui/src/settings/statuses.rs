//! Settings → Statuses (EXP-314; EXP-771 took the web's shorter name).
//!
//! Web parity: `components/team/statuses-section.tsx` — one section per
//! `IssueStatusCategory::DISPLAY_ORDER` entry, rows carrying the tinted
//! status glyph + name + live issue count, a lock slot (builtins) and ONE
//! "..." menu (Move up / Move down / Delete), and a per-category "+" band add with
//! the labels pane's inline create form (name input + `ColorSwatchGrid`).
//!
//! The 7 BUILTIN rows (`builtin_key != None`) are locked: never renamed,
//! recolored or deleted — only MOVED within their category (reordering the
//! started statuses is how a team shapes the pie-clock fills). Custom rows get
//! the full labels treatment: inline rename on blur/Enter, a swatch popover on
//! the glyph, move, and delete.
//!
//! #EXP-1629: the pane is an eUI document on the native binding. A custom
//! row's glyph is genUI's `ColorPicker` with the glyph as its child trigger
//! (the 24 px round ghost button, `STATUS_COLORS`); the create form's grid
//! is the same picker `inline`. The row's name is a `bare` Input whose edits
//! arrive as `rename` actions and persist on blur / Enter.
//!
//! Deleting a custom status ALWAYS opens one confirm dialog (EXP-320, web
//! parity with `ReassignDialog`): it confirms the deletion AND picks the
//! destination status (Backlog builtin preselected, duplicate + the deletee
//! excluded) in one step, regardless of the visible count — the server's
//! count includes trashed-board issues, so the synced count here can
//! undershoot and a "0-count" status may still hold issues. The dialog shows
//! the server-authoritative `statuses.referencingCount` once it arrives, and
//! always sends `reassignToId`, so the delete can never bounce
//! `PRECONDITION_FAILED`. The alert stays native; #EXP-1629: its body
//! (`DeleteStatusContent`) is an eUI document, web's inline target list.
//!
//! Writes are member-level (`mutate_resources`, like labels — NOT owner-only).
//! Reads come from the synced `issue_statuses` collection.

use std::collections::HashMap;
use std::rc::Rc;

use eui_gpui::{binding::Binding, view::SurfaceView};
use gpui::{
    div, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Subscription, Window,
};
use gpui_component::{button::ButtonVariant, ActiveTheme as _};
use serde_json::{json, Value};
use sync::Store;

use domain::contract::ISSUE_STATUS_STARTED_MAX;
use domain::rows::IssueStatusRow;
use domain::statuses::{resolve_row, resolve_status_sorted, IssueStatusCategory, ResolvedStatus};

use crate::native_dialog::{self, AlertSpec};
use crate::navigation::{active_team_id, Navigation};

use super::doc::{empty_state, hex, mount, not, path, IdeToasts, Ink, PillPaint, SM, XS};
use super::labels::{LABEL_COLORS, STATUS_COLORS};

/// Web parity with the labels pane's duplicate message (the server's unique is
/// `(team_id, lower(name))` across ALL statuses, builtins included).
const DUPLICATE_NAME_MESSAGE: &str = "A status with this name already exists.";

/// The data-model path the pane's projection is written at.
const PATH: &str = "/pane";

/// A custom row's rename as typed: the field's text, the synced name it
/// started from (what "unchanged" compares against: a teammate's rename
/// must not be undone by a click in and out), and whether its
/// `statuses.update` is in flight.
struct Draft {
    typed: String,
    base: String,
    sent: bool,
}

pub struct StatusesPane {
    nav: Entity<Navigation>,
    /// CUSTOM status id → its rename while the field differs from the row.
    drafts: HashMap<String, Draft>,
    /// The category whose inline create form is open.
    creating: Option<IssueStatusCategory>,
    new_name: String,
    new_color: String,
    submitting: bool,
    /// Inline error under the create form (server reject).
    create_error: Option<String>,
    /// (status id, message) — inline error under a row.
    row_error: Option<(String, String)>,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    /// The bench's state (headless has no team, statuses or issues).
    #[cfg(test)]
    shot: Option<StatusesShot>,
    /// The bench's node presses still to make, and the frames to wait first.
    #[cfg(test)]
    shot_presses: (std::collections::VecDeque<String>, u8),
    _subscriptions: Vec<Subscription>,
}

/// A state the pictures show (`settings_bench`).
#[cfg(test)]
pub(crate) struct StatusesShot {
    /// The team's statuses (any order: they are sorted as the synced ones).
    pub rows: Vec<IssueStatusRow>,
    /// Status id -> its issue count.
    pub counts: HashMap<String, usize>,
    /// The statuses shape has synced (no rows + ready = the empty state).
    pub ready: bool,
    /// The category whose create form is open, and its typed name.
    pub creating: Option<(IssueStatusCategory, String)>,
    /// (status id, message) under a row.
    pub row_error: Option<(String, String)>,
    /// Node ids the bench presses in turn once laid out (`|` = the first
    /// of these that exists).
    pub press: Vec<String>,
}

/// A pane's handler of a document action: its name, context and payload.
type Act = fn(&mut StatusesPane, &str, &Value, &Value, &mut Window, &mut gpui::Context<StatusesPane>);

impl StatusesPane {
    pub fn new(nav: Entity<Navigation>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&nav, |this, _, cx| {
                // Team switch: the inline state belongs to the old scope.
                this.creating = None;
                this.create_error = None;
                this.row_error = None;
                this.drafts.clear();
                cx.notify();
            }),
            cx.observe(&collections.issue_statuses, |_, _, cx| cx.notify()),
            // The live per-status issue counts move with the issues.
            cx.observe(&collections.issues, |_, _, cx| cx.notify()),
            // EXP-319's pin notice in the delete dialog reads the synced teams row.
            cx.observe(&collections.teams, |_, _, cx| cx.notify()),
        ];
        // Every action leaves the binding for the pane, deferred (the
        // binding is mid-update and the pane writes it; Delete needs the
        // window).
        let pane = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            let act: Act = |pane, name, context, payload, window, cx| pane.action(name, context, payload, window, cx);
            for name in ["add", "newName", "newColor", "create", "cancel", "rename", "commit", "color", "menu"] {
                let pane = pane.clone();
                binding.on_action(name, move |_, event, cx| {
                    let context = event.context.clone();
                    let payload = event.payload.clone().unwrap_or(Value::Null);
                    let pane = pane.clone();
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| {
                            let _ = pane.update(cx, |pane, cx| act(pane, name, &context, &payload, window, cx));
                        });
                    });
                    None
                });
            }
            binding
        });
        let view = mount("settings-statuses", document(&Ink::of(cx), &Chrome::of(cx)), &binding, window, cx);
        Self {
            nav,
            drafts: HashMap::new(),
            creating: None,
            new_name: String::new(),
            new_color: LABEL_COLORS[6].to_string(),
            submitting: false,
            create_error: None,
            row_error: None,
            binding,
            view,
            #[cfg(test)]
            shot: None,
            #[cfg(test)]
            shot_presses: Default::default(),
            _subscriptions: subscriptions,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = crate::navigation::nav_for_window(window, cx);
        Self::new(nav, window, cx)
    }

    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: StatusesShot, cx: &mut gpui::Context<Self>) {
        self.creating = shot.creating.as_ref().map(|(category, _)| *category);
        self.new_name = shot.creating.as_ref().map(|(_, name)| name.clone()).unwrap_or_default();
        self.row_error = shot.row_error.clone();
        self.shot_presses = (shot.press.iter().cloned().collect(), 2);
        self.shot = Some(shot);
        cx.notify();
    }

    /// The bench's next press once its target is laid out (two painted
    /// frames in); the headless clock stands still, so an open popup's
    /// enter fade is run out by hand (as `BoardDetailPane`'s).
    #[cfg(test)]
    fn shot_press(&mut self, cx: &mut gpui::Context<Self>) {
        let (queue, wait) = &mut self.shot_presses;
        if queue.is_empty() {
            if !self.view.read(cx).layers().is_empty() {
                self.view.update(cx, |view, cx| view.advance_clock(std::time::Duration::from_millis(500), cx));
            }
            return;
        }
        if *wait > 0 {
            *wait -= 1;
            cx.notify();
            return;
        }
        let ids = queue.pop_front().unwrap_or_default();
        *wait = 2;
        self.view.update(cx, |view, cx| {
            if let Some(index) = ids.split('|').find_map(|id| view.index_of(id)) {
                view.fire(index, "press", None, cx);
            } else {
                eprintln!("[statuses bench] no node {ids}");
            }
        });
        cx.notify();
    }

    fn team_id(&self, cx: &App) -> Option<String> {
        #[cfg(test)]
        if self.shot.is_some() {
            return Some("team-shots".to_string());
        }
        active_team_id(&self.nav, cx)
    }

    /// The team's statuses in the canonical order.
    fn sorted_rows(&self, team_id: &str, cx: &App) -> Vec<IssueStatusRow> {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return domain::statuses::sort_team_statuses(&shot.rows);
        }
        crate::queries::team_statuses(cx, team_id)
    }

    /// The statuses shape has synced.
    fn statuses_ready(&self, cx: &App) -> bool {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.ready;
        }
        Store::global(cx).collections().issue_statuses.read(cx).is_ready()
    }

    /// The team's statuses in the canonical order, each paired with its
    /// resolved presentation (glyph + tint — the SAME resolution the board
    /// list renders, so the pane can never show a different icon).
    fn scoped_statuses(&self, cx: &App) -> Vec<(IssueStatusRow, ResolvedStatus)> {
        let Some(team_id) = self.team_id(cx) else {
            return Vec::new();
        };
        let sorted = self.sorted_rows(&team_id, cx);
        (0..sorted.len())
            .map(|index| (sorted[index].clone(), resolve_row(&sorted, index)))
            .collect()
    }

    /// group key → how many of the team's synced issues sit in it. Counts
    /// pre-backfill `status_id = NULL` rows correctly (they resolve through
    /// their anchor to the matching builtin).
    fn issue_counts(&self, cx: &App) -> HashMap<String, usize> {
        let mut counts: HashMap<String, usize> = HashMap::new();
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.counts.clone();
        }
        let Some(team_id) = self.team_id(cx) else {
            return counts;
        };
        let sorted = self.sorted_rows(&team_id, cx);
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

    /// A document action.
    fn action(&mut self, name: &str, context: &Value, payload: &Value, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let key = context.get("key").and_then(Value::as_str).unwrap_or_default().to_string();
        let value = payload.get("value").and_then(Value::as_str).map(str::to_string);
        match name {
            // A category band's "+": its create form, empty (web: the
            // placeholder names the category).
            "add" => {
                let category = context.get("category").and_then(Value::as_str).map(IssueStatusCategory::from_wire);
                if let Some(category) = category {
                    self.creating = Some(category);
                    self.create_error = None;
                    self.new_name.clear();
                }
            }
            "newName" => {
                self.new_name = value.unwrap_or_default();
                self.create_error = None;
            }
            "newColor" => {
                if let Some(color) = value {
                    self.new_color = color;
                }
            }
            // The Create button, or Enter in the name field.
            "create" => {
                if let Some(typed) = value.filter(|_| payload.get("value").is_some()) {
                    self.new_name = typed;
                }
                self.create(cx);
            }
            "cancel" => {
                self.creating = None;
                self.new_name.clear();
                self.reset_form();
            }
            // A row's name as typed.
            "rename" if !key.is_empty() => {
                let typed = value.unwrap_or_default();
                let base = self.scoped_statuses(cx).into_iter().find(|(row, _)| row.id == key).map(|(row, _)| row.name);
                let Some(base) = base else { return };
                let draft = self.drafts.entry(key.clone()).or_insert(Draft { typed: String::new(), base, sent: false });
                draft.typed = typed;
                draft.sent = false;
                if self.row_error.as_ref().is_some_and(|(id, _)| id == &key) {
                    self.row_error = None;
                }
            }
            // Blur (`final`) or Enter: the rename persists.
            "commit" if !key.is_empty() => {
                if let (Some(typed), Some(draft)) = (value, self.drafts.get_mut(&key)) {
                    if draft.typed != typed {
                        draft.typed = typed;
                        draft.sent = false;
                    }
                }
                let blur = context.get("blur").and_then(Value::as_bool).unwrap_or(false);
                self.persist_name(&key, blur, cx);
            }
            "color" if !key.is_empty() => {
                if let Some(color) = value {
                    self.set_color(key, color, cx);
                }
            }
            "menu" if !key.is_empty() => {
                let statuses = self.scoped_statuses(cx);
                let Some((row, resolved)) = statuses.iter().find(|(row, _)| row.id == key).cloned() else { return };
                match payload.get("value").and_then(Value::as_str) {
                    Some("up") => self.move_status(&row, api::statuses::MoveDirection::Up, cx),
                    Some("down") => self.move_status(&row, api::statuses::MoveDirection::Down, cx),
                    Some("delete") if row.builtin_key.is_none() => {
                        // Delete ALWAYS opens the one confirm-and-reassign
                        // dialog (EXP-320). EXP-1045: the RESOLVED rows
                        // travel, so the target picker draws each status by
                        // its own glyph and tint.
                        let candidates: Vec<ResolvedStatus> = statuses
                            .iter()
                            .filter(|(candidate, resolved)| candidate.id != row.id && resolved.category != IssueStatusCategory::Duplicate)
                            .map(|(_, resolved)| resolved.clone())
                            .collect();
                        let preselected = statuses
                            .iter()
                            .find(|(candidate, _)| candidate.builtin_key.as_deref() == Some("backlog"))
                            .map(|(candidate, _)| candidate.id.clone())
                            .or_else(|| candidates.first().map(|status| status.group_key.clone()));
                        let Some(preselected) = preselected else { return };
                        let count = self.issue_counts(cx).get(&resolved.group_key).copied().unwrap_or(0);
                        self.open_delete_dialog(row.id.clone(), row.name.clone(), count, candidates, preselected, window, cx);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        cx.notify();
    }

    /// Labels-pane `persistName`: trim; empty or unchanged resets to the
    /// synced name (on blur: a focused field keeps its text), otherwise
    /// `statuses.update`. "Unchanged" means unchanged against the name the
    /// edit started from, not against the live row: a teammate's rename
    /// must not be undone by a click in and out.
    fn persist_name(&mut self, status_id: &str, blur: bool, cx: &mut gpui::Context<Self>) {
        let Some((row, _)) = self.scoped_statuses(cx).into_iter().find(|(row, _)| row.id == status_id) else {
            return;
        };
        let Some(draft) = self.drafts.get(status_id) else {
            return;
        };
        if draft.sent {
            return;
        }
        let typed = draft.typed.trim().to_string();
        if typed.is_empty() || typed == draft.base {
            if blur {
                self.drafts.remove(status_id);
            }
            self.row_error = None;
            return;
        }
        if self.is_duplicate_name(&typed, Some(status_id), cx) {
            self.row_error = Some((status_id.to_string(), DUPLICATE_NAME_MESSAGE.to_string()));
            return;
        }
        self.row_error = None;
        let team_id = row.team_id.clone();
        let status_id = status_id.to_string();
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            return;
        };
        if let Some(draft) = self.drafts.get_mut(&status_id) {
            draft.sent = true;
        }
        // Web `RenameRow`: a rejected rename says so under the row instead of
        // vanishing into the log.
        cx.spawn(async move |this, cx| {
            let call_id = status_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::statuses::statuses_update(&trpc, &team_id, &call_id, Some(&typed), None)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Err(err) = &result {
                    log::warn!("[ui] statuses.update(name) failed: {err}");
                    if let Some(draft) = this.drafts.get_mut(&status_id) {
                        draft.sent = false;
                    }
                    // FEED-79: a name already taken is INVALID INPUT (under
                    // the row); anything else is the error toast.
                    if crate::action_error::is_name_conflict(err) {
                        this.row_error = Some((
                            status_id.clone(),
                            super::form_error(err, "Failed to rename status."),
                        ));
                    } else {
                        crate::action_error::failed_action_err("rename", "the status", err, cx);
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// A custom row's colour pick (`statuses.update`); a rejection toasts.
    fn set_color(&mut self, status_id: String, color: String, cx: &mut gpui::Context<Self>) {
        let Some(team_id) = self.team_id(cx) else {
            return;
        };
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let call_id = status_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::statuses::statuses_update(&trpc, &team_id, &call_id, None, Some(&color))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.set_row_error(&status_id, result, "update", cx);
            });
        })
        .detach();
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
        let name = self.new_name.trim().to_string();
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
                    // The duplicate-name CONFLICT is INVALID INPUT (inline);
                    // the started cap and everything else is the error toast
                    // (FEED-79). A create can duplicate: no Retry.
                    if crate::action_error::is_name_conflict(err) {
                        this.create_error =
                            Some(super::form_error(err, "Failed to create status."));
                    } else {
                        crate::action_error::failed_action_err("create", "the status", err, cx);
                    }
                } else {
                    this.creating = None;
                    this.new_name.clear();
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
            let _ = this.update(cx, |_, cx| {
                if let Err(err) = &result {
                    log::warn!("[ui] statuses.delete failed: {err}");
                    crate::action_error::failed_action_err("delete", "the status", err, cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn pr_pin_notice(&self, status_id: &str, cx: &App) -> Option<SharedString> {
        let team = super::active_team(cx, &self.nav)?;
        let rows = self.scoped_statuses(cx);
        let events: [(&str, Option<&str>, Option<bool>, &str); 2] = [
            (
                "opens",
                team.pr_opened_status_id.as_deref(),
                team.pr_opened_automation,
                "in_review",
            ),
            (
                "merges",
                team.pr_merged_status_id.as_deref(),
                team.pr_merged_automation,
                "done",
            ),
        ];
        let hits: Vec<(&str, &str)> = events
            .iter()
            .filter(|(_, pinned, automation, _)| {
                *automation != Some(false) && *pinned == Some(status_id)
            })
            .map(|(verb, _, _, default_key)| (*verb, *default_key))
            .collect();
        if hits.is_empty() {
            return None;
        }
        let fallbacks: Vec<Option<String>> = hits
            .iter()
            .map(|(_, default_key)| {
                rows.iter()
                    .find(|(row, _)| row.builtin_key.as_deref() == Some(*default_key))
                    .map(|(row, _)| row.name.clone())
            })
            .collect();
        let targets = if fallbacks.iter().all(Option::is_some) {
            fallbacks
                .iter()
                .flatten()
                .cloned()
                .collect::<Vec<_>>()
                .join(" and ")
        } else if hits.len() > 1 {
            "the default statuses".to_string()
        } else {
            "the default status".to_string()
        };
        Some(if hits.len() > 1 {
            format!(
                "This status is where issues move when a pull request opens \
                 or merges. Those automations will fall back to {targets}."
            )
            .into()
        } else {
            format!(
                "This status is where issues move when a pull request {}. \
                 That automation will fall back to {targets}.",
                hits[0].0
            )
            .into()
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn open_delete_dialog(
        &mut self,
        status_id: String,
        status_name: String,
        synced_count: usize,
        candidates: Vec<ResolvedStatus>,
        preselected: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(team_id) = self.team_id(cx) else {
            return;
        };
        self.row_error = None;
        cx.notify();

        let pr_pin_notice = self.pr_pin_notice(&status_id, cx);
        let content = cx.new(|cx| DeleteStatusContent::new(candidates, preselected, None, synced_count, pr_pin_notice, window, cx));

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
            "Issues using this status (including any on trashed boards) will \
             move to the status you pick.",
            "Delete status",
        )
        .ok_variant(ButtonVariant::Danger)
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
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            return;
        };
        // EXP-774 (web parity): a rejected move says so.
        cx.spawn(async move |this, cx| {
            let call_id = status_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::statuses::statuses_move(&trpc, &team_id, &call_id, direction)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.set_row_error(&status_id, result, "move", cx);
            });
        })
        .detach();
    }

    /// The failure writer for the awaited row mutations (EXP-774): a
    /// rejection is the error toast `Couldn't <verb> the status` (FEED-79).
    fn set_row_error<T>(
        &mut self,
        status_id: &str,
        result: Result<T, api::ApiError>,
        verb: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Err(err) = result {
            log::warn!("[ui] status row mutation failed ({status_id}): {err}");
            crate::action_error::failed_action_err(verb, "the status", &err, cx);
        }
    }

    /// The state as the document binds it.
    fn project(&mut self, cx: &App) -> Value {
        let statuses = self.scoped_statuses(cx);
        let counts = self.issue_counts(cx);
        // A rename the synced row caught up with (or a row that went) is
        // no longer a draft.
        self.drafts.retain(|id, draft| {
            statuses.iter().any(|(row, _)| &row.id == id && !(draft.sent && row.name.trim() == draft.typed.trim()))
        });
        let ready = self.statuses_ready(cx);
        let mut cats = serde_json::Map::new();
        for category in IssueStatusCategory::DISPLAY_ORDER {
            let rows: Vec<&(IssueStatusRow, ResolvedStatus)> = statuses.iter().filter(|(_, resolved)| resolved.category == category).collect();
            let last = rows.len().saturating_sub(1);
            // The Duplicate category is fixed at exactly one status — no "+".
            // The pie-clock fill tables are defined only up to
            // ISSUE_STATUS_STARTED_MAX started statuses, so Started caps out.
            let can_add = category != IssueStatusCategory::Duplicate;
            let capped = category == IssueStatusCategory::Started && rows.len() >= ISSUE_STATUS_STARTED_MAX;
            let creating = can_add && self.creating == Some(category);
            let rows: Vec<Value> = rows
                .iter()
                .enumerate()
                .map(|(index, (row, resolved))| {
                    let custom = row.builtin_key.is_none();
                    let count = counts.get(&resolved.group_key).copied().unwrap_or(0);
                    let error = self.row_error.as_ref().filter(|(id, _)| id == &row.id).map(|(_, message)| message.clone());
                    // ONE "..." menu: Move up / Move down — on BUILTINS too
                    // (only name/color and delete are locked), disabled at
                    // the category edges — then Delete on custom rows.
                    let mut items = vec![
                        json!({"label": "Move up", "value": "up", "icon": "ui-chevron-up", "disabled": index == 0}),
                        json!({"label": "Move down", "value": "down", "icon": "ui-chevron-down", "disabled": index == last}),
                    ];
                    if custom {
                        items.push(json!({"label": "Delete", "value": "delete", "icon": "ui-delete", "destructive": true}));
                    }
                    json!({
                        "key": row.id,
                        "custom": custom,
                        "builtin": !custom,
                        "name": self.drafts.get(&row.id).map(|draft| draft.typed.clone()).unwrap_or_else(|| row.name.clone()),
                        "label": row.name,
                        "glyph": resolved.glyph.file_name(),
                        "tint": hex(crate::icons::status_tint_color(&resolved.tint, cx)),
                        "color": row.color.clone().unwrap_or_default(),
                        // The backlog builtin is where new issues land.
                        "isDefault": row.builtin_key.as_deref() == Some("backlog"),
                        "count": format!("{count} issue{}", if count == 1 { "" } else { "s" }),
                        "hasError": error.is_some(),
                        "error": error.unwrap_or_default(),
                        "menuLabel": format!("Status actions for {}", row.name),
                        "items": items,
                    })
                })
                .collect();
            let wire = category.as_wire().unwrap_or("unknown");
            cats.insert(wire.to_string(), json!({
                // A category can be EMPTY (since EXP-685 retired the builtin
                // Todo, `unstarted` starts out with no rows at all).
                "empty": rows.is_empty(),
                "hasRows": !rows.is_empty(),
                "rows": rows,
                "add": can_add && !creating && !capped,
                "capped": can_add && !creating && capped,
                "creating": creating,
            }));
        }
        let name = self.new_name.trim();
        let duplicate = !name.is_empty() && self.is_duplicate_name(name, None, cx);
        let form_error = if duplicate { Some(DUPLICATE_NAME_MESSAGE.to_string()) } else { self.create_error.clone() };
        json!({
            "list": !statuses.is_empty(),
            "loading": statuses.is_empty() && !ready,
            "empty": statuses.is_empty() && ready,
            "cats": cats,
            "form": {
                "name": self.new_name,
                // Web `CreateStatusForm`: "New {category} status".
                "placeholder": self.creating.map(create_placeholder).unwrap_or_default(),
                "color": self.new_color,
                "hasError": form_error.is_some(),
                "error": form_error.unwrap_or_default(),
                "disabled": name.is_empty() || self.submitting || duplicate,
                "submitting": self.submitting,
                "label": if self.submitting { "Creating…" } else { "Create status" },
            },
        })
    }
}

impl Render for StatusesPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "StatusesPane").entered();
        let projection = self.project(cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        #[cfg(test)]
        self.shot_press(cx);
        // EXP-630: the PR automation card moved to Settings → Issues.
        div().w_full().min_w_0().child(self.view.clone())
    }
}

/// The colours the document needs beyond [`Ink`].
struct Chrome {
    /// The list hover wash (the ghost buttons').
    list_hover: String,
    /// `feedback::skeleton_fill`.
    skeleton: String,
    /// `action_error::field_error`'s ink.
    danger: String,
    /// A gpui-component Button's corner (the primary Create).
    radius: f32,
}

impl Chrome {
    fn of(cx: &App) -> Self {
        let theme = cx.theme();
        Self {
            list_hover: hex(theme.list_hover),
            skeleton: hex(theme.muted_foreground.opacity(0.18)),
            danger: hex(theme.danger),
            radius: f32::from(theme.radius),
        }
    }
}

/// `action_error::field_error`: the 13 px line in the danger ink, `px`
/// inset.
fn field_error(chrome: &Chrome, id: &str, text: Value, visible: Value, px: f32) -> Value {
    let line = json!({"id": format!("{id}_text"), "component": "Text", "props": {"text": text}, "style": {"fontSize": crate::action_error::FIELD_ERROR_PX, "lineHeight": 20, "color": chrome.danger}});
    json!({"id": id, "component": "Box", "visible": visible, "style": {"display": "flex", "flexDirection": "column", "paddingHorizontal": px}, "children": [line]})
}

/// A 24 px square that centres its child (the glyph and lock slots).
fn slot(id: &str, visible: Value, child: Value) -> Value {
    json!({"id": id, "component": "Box", "visible": visible, "style": {"display": "flex", "alignItems": "center", "justifyContent": "center", "width": 24, "height": 24, "flexShrink": 0}, "children": [child]})
}

/// One status row (`flat_row`: `px_3` / `py_1p5`, `gap_3`): the glyph
/// (a custom's is the ColorPicker's child trigger; a builtin's is inert in
/// the same 24 px slot), the name (a custom's types in place), the Default
/// pill, the count, the lock slot and the ⋯ menu; the row's error under it.
fn row(ink: &Ink, chrome: &Chrome, id: &str) -> Value {
    let glyph = |id: String| json!({"id": id, "component": "Icon", "props": {"name": path("glyph")}, "style": {"width": ink.r(SM), "height": ink.r(SM), "color": path("tint"), "flexShrink": 0}});
    let inert = slot(&format!("{id}_glyph"), path("builtin"), glyph(format!("{id}_glyph_icon")));
    let picker = json!({"id": format!("{id}_color"), "component": "ColorPicker", "visible": path("custom"),
        "props": {"value": path("color"), "colors": STATUS_COLORS},
        "on": {"change": {"event": {"name": "color", "context": {"key": path("key")}}}},
        "children": [glyph(format!("{id}_color_icon"))]});
    let mut label = ink.text(&format!("{id}_label"), path("label"), SM, &ink.foreground);
    label["visible"] = path("builtin");
    label["style"]["flexGrow"] = json!(1);
    label["style"]["flexShrink"] = json!(1);
    label["style"]["flexBasis"] = json!(0);
    label["style"]["minWidth"] = json!(0);
    // `glass_input` at `web_input_sm` without its chrome: the 32 px field,
    // its text 10 px in (gpui-component's Medium input padding).
    let input = json!({"id": format!("{id}_input"), "component": "Input",
        "props": {"variant": "bare", "label": "Name", "name": "name", "value": path("name")},
        "on": {
            "change": {"event": {"name": "rename", "context": {"key": path("key")}}},
            "blur": {"event": {"name": "commit", "context": {"key": path("key"), "blur": true}}},
            "submit": {"event": {"name": "commit", "context": {"key": path("key"), "blur": false}}},
        },
        "style": {"width": "100%"}});
    let field = json!({"id": format!("{id}_field"), "component": "Box", "visible": path("custom"),
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "height": 32, "paddingHorizontal": 10, "flexGrow": 1, "flexShrink": 1, "flexBasis": 0, "minWidth": 0},
        "children": [input]});
    let mut pill = ink.pill(&format!("{id}_default"), json!("Default"), false, None);
    pill["visible"] = path("isDefault");
    let count = ink.text(&format!("{id}_count"), path("count"), XS, &ink.muted);
    // The lock slot: a builtin wears the lock glyph, a custom row keeps the
    // width so the counts, the Default pill and the names line up.
    let lock = json!({"id": format!("{id}_lock_tip"), "component": "Tooltip", "visible": path("builtin"),
        "props": {"content": "Built-in status: reorderable, but not renamable, recolorable or deletable."},
        "children": [{"id": format!("{id}_lock_icon"), "component": "Icon", "props": {"name": "ui-private"}, "style": {"width": ink.r(XS), "height": ink.r(XS), "color": ink.muted, "flexShrink": 0}}]});
    let lock = json!({"id": format!("{id}_lock"), "component": "Box", "style": {"display": "flex", "alignItems": "center", "justifyContent": "center", "width": 24, "height": 24, "flexShrink": 0}, "children": [lock]});
    // a11y: named by the caller ("Status actions for {name}", the node's accessibility label).
    let mut more = ink.ghost_icon_button(&format!("{id}_more"), "ui-more", path("menuLabel"), Value::Null, &chrome.list_hover);
    more.as_object_mut().expect("the trigger").remove("on");
    let menu = json!({"id": format!("{id}_menu"), "component": "Menu", "props": {"items": path("items"), "align": "start"},
        "on": {"select": {"event": {"name": "menu", "context": {"key": path("key")}}}}, "children": [more]});
    let line = json!({"id": format!("{id}_line"), "component": "Box",
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0, "gap": ink.r(0.75), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.375), "borderRadius": ink.radius_md},
        "children": [inert, picker, label, field, pill, count, lock, menu]});
    json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "gap": ink.r(0.25)},
        "children": [line, field_error(chrome, &format!("{id}_error"), path("error"), path("hasError"), ink.r(0.25))]})
}

/// The inline create form (the labels pane's) under `category`'s rows: the
/// boxed name field, its error, "Color" over the inline ColorPicker, then
/// Create status (primary) and Cancel. `flat_row`: `p_3`, `gap_3`.
fn create_form(ink: &Ink, chrome: &Chrome, id: &str, visible: Value) -> Value {
    let input = json!({"id": format!("{id}_name"), "component": "Input",
        "props": {"variant": "box", "size": "sm", "label": "Status name", "name": "newName", "placeholder": path("/pane/form/placeholder"), "value": path("/pane/form/name")},
        "on": {"change": {"event": {"name": "newName"}}, "submit": {"event": {"name": "create"}}},
        "style": {"width": "100%"}});
    let color_label = ink.text(&format!("{id}_color_label"), json!("Color"), XS, &ink.muted);
    let grid = json!({"id": format!("{id}_grid"), "component": "ColorPicker", "props": {"value": path("/pane/form/color"), "colors": STATUS_COLORS, "inline": true},
        "on": {"change": {"event": {"name": "newColor"}}}});
    let color = json!({"id": format!("{id}_color"), "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.375)}, "children": [color_label, grid]});
    let [mut submit, mut submit_dim] = ink.pill_button_with(&format!("{id}_submit"), PillPaint::Primary, None, None, path("/pane/form/label"), path("/pane/form/disabled"), None, json!({"name": "create"}));
    // A gpui-component Button keeps the theme's corner (not the capsule).
    for face in [&mut submit, &mut submit_dim] {
        face["style"]["borderRadius"] = json!(chrome.radius);
        for child in face["children"].as_array_mut().expect("the button's children") {
            if child["style"]["position"] == "absolute" {
                child["style"]["borderRadius"] = json!(chrome.radius);
            }
        }
    }
    // `loading`: the spinner leads the label on the disabled face.
    let glyph = ink.r(0.75);
    let spin = json!({"id": format!("{id}_submit_spin"), "component": "Spinner", "visible": path("/pane/form/submitting"), "style": {"width": glyph, "height": glyph, "color": ink.primary_button_fg, "flexShrink": 0}});
    let children = submit_dim["children"].as_array_mut().expect("the button's children");
    children.insert(children.len() - 1, spin);
    let [cancel, cancel_dim] = ink.pill_button(&format!("{id}_cancel"), None, json!("Cancel"), path("/pane/form/submitting"), json!({"name": "cancel"}));
    let buttons = json!({"id": format!("{id}_buttons"), "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.5)}, "children": [submit, submit_dim, cancel, cancel_dim]});
    json!({"id": id, "component": "Box", "visible": visible,
        "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.75), "padding": ink.r(0.75), "borderRadius": ink.radius_md},
        "children": [input, field_error(chrome, &format!("{id}_error"), path("/pane/form/error"), path("/pane/form/hasError"), 0.), color, buttons]})
}

/// One category: its band (the ghost "+" trailing, disabled with the
/// reason at the started cap), the empty state, the gapless rows, then the
/// create form or the cap line (`gap_2`).
fn category(ink: &Ink, chrome: &Chrome, category: IssueStatusCategory) -> Value {
    let wire = category.as_wire().unwrap_or("unknown");
    let at = |key: &str| path(&format!("/pane/cats/{wire}/{key}"));
    let cap = format!("A team can have at most {ISSUE_STATUS_STARTED_MAX} started statuses.");
    let trailing = (category != IssueStatusCategory::Duplicate).then(|| {
        let add = ink.ghost_icon_button(&format!("{wire}_add"), "ui-add", json!(format!("Add {} status", category.label())), json!({"name": "add", "context": {"category": wire}}), &chrome.list_hover);
        let add = json!({"id": format!("{wire}_add_tip"), "component": "Tooltip", "visible": at("add"), "props": {"content": format!("Add {} status", category.label())}, "children": [add]});
        // gpui-component's disabled ghost: no wash, the glyph muted at half.
        // a11y: named by the caller (the cap sentence, the node's accessibility label).
        let mut capped = ink.ghost_icon_button(&format!("{wire}_capped"), "ui-add", json!(cap), Value::Null, &chrome.list_hover);
        for key in ["on", "props"] {
            capped.as_object_mut().expect("the button").remove(key);
        }
        for key in [":hover", "cursor"] {
            capped["style"].as_object_mut().expect("the button's style").remove(key);
        }
        capped["children"][0]["style"]["color"] = json!(ink.muted);
        capped["children"][0]["style"]["opacity"] = json!(0.5);
        let capped = json!({"id": format!("{wire}_capped_tip"), "component": "Tooltip", "visible": at("capped"), "props": {"content": cap}, "children": [capped]});
        json!({"id": format!("{wire}_trailing"), "component": "Box", "style": {"display": "flex", "flexShrink": 0}, "children": [add, capped]})
    });
    let band = ink.band(&format!("{wire}_band"), category.label(), trailing);
    let rows = json!({"id": format!("{wire}_rows"), "component": "List", "visible": at("hasRows"), "props": {"gap": "none"},
        "template": {"component": format!("{wire}_row"), "path": format!("/pane/cats/{wire}/rows"), "key": "key"},
        "children": [row(ink, chrome, &format!("{wire}_row"))]});
    let mut cap_line = ink.text(&format!("{wire}_cap"), json!(cap), XS, &ink.muted);
    cap_line["visible"] = at("capped");
    json!({"id": wire, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "gap": ink.r(0.5)},
        "children": [
            band,
            empty_state(ink, &format!("{wire}_empty"), "settings-statuses", crate::empty_state_copy::STATUSES, at("empty")),
            rows,
            create_form(ink, chrome, &format!("{wire}_form"), at("creating")),
            cap_line,
        ]})
}

/// The pane's tree: one category after another (`gap_6`, no pane band:
/// every category carries its own); with no statuses yet, the "Statuses"
/// band over the skeleton or the empty state.
fn document(ink: &Ink, chrome: &Chrome) -> Value {
    let column = |id: &str, visible: Value, children: Vec<Value>| {
        json!({"id": id, "component": "Box", "visible": visible, "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "gap": ink.r(1.5)}, "children": children})
    };
    let categories = IssueStatusCategory::DISPLAY_ORDER.iter().map(|c| category(ink, chrome, *c)).collect();
    let none = column("none", not(path("/pane/list")), vec![
        ink.band("none_band", "Statuses", None),
        ink.skeleton_rows("skeleton", &chrome.skeleton, path("/pane/loading")),
        empty_state(ink, "empty", "settings-statuses", crate::empty_state_copy::STATUSES, path("/pane/empty")),
    ]);
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0},
        "children": [column("categories", path("/pane/list"), categories), none]})
}

/// The delete dialog's body (EXP-320, #EXP-1629: an eUI document): the live
/// referencing count + the destination picker. Its own entity so the async
/// `referencingCount` fetch can re-render just this block (the surrounding
/// native alert never re-renders).
pub(crate) struct DeleteStatusContent {
    /// The offered targets, the duplicate category and the deletee
    /// excluded. Every one is SYNCED, so its `group_key` IS its row id (what
    /// `selected_id` holds and what `reassignToId` sends).
    candidates: Vec<ResolvedStatus>,
    selected_id: String,
    /// Server-authoritative count (trashed-board issues included); `None`
    /// while loading or after a failed fetch: the copy stays hedged then.
    server_count: Option<i64>,
    /// The client-visible count: flags when part of the total is on
    /// trashed boards the client can't see.
    synced_count: usize,
    /// Web `prPinNotice`: set only when a live PR automation points here.
    pr_pin_notice: Option<SharedString>,
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
}

impl DeleteStatusContent {
    #[allow(clippy::too_many_arguments)]
    fn new(candidates: Vec<ResolvedStatus>, selected_id: String, server_count: Option<i64>, synced_count: usize, pr_pin_notice: Option<SharedString>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let me = cx.weak_entity();
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            binding.on_action("target", move |_, event, cx| {
                let key = event.context.get("key").and_then(Value::as_str).map(str::to_string);
                let me = me.clone();
                cx.defer(move |cx| {
                    let _ = me.update(cx, |state, cx| {
                        if let Some(key) = key {
                            // Every candidate is a synced row, so its
                            // `group_key` IS the id `reassignToId` wants.
                            state.selected_id = key;
                            cx.notify();
                        }
                    });
                });
                None
            });
            binding
        });
        let view = mount("status-delete", delete_document(&Ink::of(cx), &DeleteChrome::of(cx)), &binding, window, cx);
        Self {
            candidates,
            selected_id,
            server_count,
            synced_count,
            pr_pin_notice,
            binding,
            view,
        }
    }

    /// The bench's dialog body: `rows` minus the deletee and the duplicate
    /// as the targets, Backlog picked.
    #[cfg(test)]
    pub(crate) fn for_shots(rows: &[IssueStatusRow], deletee: &str, server_count: Option<i64>, pr_pin_notice: Option<&str>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let sorted = domain::statuses::sort_team_statuses(rows);
        let candidates: Vec<ResolvedStatus> = (0..sorted.len())
            .map(|index| resolve_row(&sorted, index))
            .filter(|status| status.group_key != deletee && status.category != IssueStatusCategory::Duplicate)
            .collect();
        let selected_id = sorted.iter().find(|row| row.builtin_key.as_deref() == Some("backlog")).map(|row| row.id.clone()).unwrap_or_default();
        Self::new(candidates, selected_id, server_count, 2, pr_pin_notice.map(|notice| SharedString::from(notice.to_string())), window, cx)
    }

    /// The bench's press on the row of `key`, as a click makes it.
    #[cfg(test)]
    pub(crate) fn press_target(&mut self, key: &str, cx: &mut gpui::Context<Self>) {
        self.view.update(cx, |view, cx| {
            let index = view.index_of(&format!("target_row.{key}")).expect("the target's row");
            view.fire(index, "press", None, cx);
        });
    }

    /// The state as the document binds it.
    fn project(&self, cx: &App) -> Value {
        // Web copy: the alert's own description carries the hedged sentence
        // while the server count is in flight, so this line only speaks once
        // the real number lands.
        let count_line = match self.server_count {
            None => None,
            Some(0) => Some("No issues use this status right now. Anything referencing it when you confirm will move to the status you pick.".to_string()),
            Some(n) => {
                let trashed = if n as usize > self.synced_count { " (some on trashed boards)" } else { "" };
                Some(format!("{n} issue{}{trashed} will move to the status you pick.", if n == 1 { "" } else { "s" }))
            }
        };
        let theme = cx.theme();
        let accent = hex(theme.accent);
        // Web `space-y-3` under whatever leads the list (the column's own
        // gap is `gap_2`).
        let list_top = if self.pr_pin_notice.is_some() || count_line.is_some() { f32::from(theme.font_size) * 0.25 } else { 0. };
        let targets: Vec<Value> = self
            .candidates
            .iter()
            .map(|status| {
                let selected = status.group_key == self.selected_id;
                json!({
                    "key": status.group_key,
                    "label": status.name,
                    "glyph": status.glyph.file_name(),
                    "tint": hex(crate::icons::status_tint_color(&status.tint, cx)),
                    "fill": if selected { accent.as_str() } else { "#00000000" },
                    "state": if selected { "Selected" } else { "" },
                })
            })
            .collect();
        json!({
            "hasNotice": self.pr_pin_notice.is_some(),
            "notice": self.pr_pin_notice.as_ref().map(|notice| notice.to_string()).unwrap_or_default(),
            "hasCount": count_line.is_some(),
            "count": count_line.unwrap_or_default(),
            "listTop": list_top,
            "targets": targets,
        })
    }
}

impl Render for DeleteStatusContent {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "DeleteStatusContent").entered();
        let projection = self.project(cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        div().w_full().min_w_0().child(self.view.clone())
    }
}

/// The colours the delete body needs beyond [`Ink`].
struct DeleteChrome {
    radius: f32,
    accent: String,
    notice_stroke: String,
}

impl DeleteChrome {
    fn of(cx: &App) -> Self {
        let theme = cx.theme();
        Self { radius: f32::from(theme.radius), accent: hex(theme.accent), notice_stroke: hex(super::row_stroke(cx)) }
    }
}

/// The body's tree (`gap_2`): the PR-pin notice, the count line, then web's
/// inline target list (`statuses-section.tsx`, `space-y-1`): a pressable row
/// per target (`px-2 py-2`, `gap-2`, `rounded-md`, the `h-4` glyph in its
/// tint, the `text-sm` name truncated), the chosen one and the hovered one
/// on `bg-accent`. The alert's body scrolls past its cap (#EXP-1588).
fn delete_document(ink: &Ink, chrome: &DeleteChrome) -> Value {
    let at = |key: &str| path(&format!("/pane/{key}"));
    let notice_line = ink.text("notice_text", at("notice"), XS, &ink.muted);
    let notice = json!({"id": "notice", "component": "Box", "visible": at("hasNotice"),
        "style": {"display": "flex", "flexDirection": "column", "paddingHorizontal": ink.r(0.625), "paddingVertical": ink.r(0.5), "borderRadius": chrome.radius, "borderWidth": 1, "borderColor": chrome.notice_stroke},
        "children": [notice_line]});
    let mut count = ink.text("count", at("count"), SM, &ink.muted);
    count["visible"] = at("hasCount");
    let glyph = json!({"id": "target_glyph", "component": "Icon", "props": {"name": path("glyph")}, "style": {"width": ink.r(1.), "height": ink.r(1.), "color": path("tint"), "flexShrink": 0}});
    let mut name = ink.text("target_name", path("label"), SM, &ink.foreground);
    name["props"]["lines"] = json!(1);
    name["style"]["lineHeight"] = json!(ink.r(1.25));
    name["style"]["flexGrow"] = json!(1);
    name["style"]["flexShrink"] = json!(1);
    name["style"]["minWidth"] = json!(0);
    let row = json!({"id": "target_row", "component": "Box", "props": {"pressable": true}, "accessibility": {"label": path("label"), "description": path("state")},
        "on": {"press": {"event": {"name": "target", "context": {"key": path("key")}}}},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0, "gap": ink.r(0.5), "paddingHorizontal": ink.r(0.5), "paddingVertical": ink.r(0.5), "borderRadius": ink.radius_md, "backgroundColor": path("fill"), "cursor": "pointer", ":hover": {"backgroundColor": chrome.accent}},
        "children": [glyph, name]});
    let list = json!({"id": "targets", "component": "List", "props": {"gap": "none"}, "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "gap": ink.r(0.25), "marginTop": at("listTop")},
        "template": {"component": "target_row", "path": "/pane/targets", "key": "key"},
        "children": [row]});
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0, "gap": ink.r(0.5)}, "children": [notice, count, list]})
}

/// Web `CreateStatusForm`'s placeholder: "New {category} status", the category
/// LOWERCASED (`statuses-section.tsx`).
fn create_placeholder(category: IssueStatusCategory) -> String {
    format!("New {} status", category.label().to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The document raises no issue a debug build's mount check refuses.
    #[test]
    fn the_document_raises_no_unexpected_issue() {
        let ink = Ink {
            rem: 14.,
            primary: "#fafafaff".into(),
            foreground: "#ffffffff".into(),
            hint: "#ffffffff".into(),
            value: "#ffffffff".into(),
            muted: "#a1a1aaff".into(),
            muted_glyph: "#a1a1aaff".into(),
            fill_row: "#ffffff0a".into(),
            stroke_row: "#ffffff14".into(),
            fill_card: "#ffffff0f".into(),
            stroke_card: "#ffffff1a".into(),
            code: "#27272aff".into(),
            radius_lg: 12.,
            radius_md: 8.,
            fill_section: "#ffffff0a".into(),
            fill_active: "#ffffff1a".into(),
            mono: "$type.family.mono".into(),
            pill_button_fill: "#ffffff0f".into(),
            pill_button_fill_disabled: "#ffffff0b".into(),
            primary_button_fill: "#f4f4f5ff".into(),
            primary_button_hover: "#e4e4e7ff".into(),
            primary_button_fill_disabled: "#f4f4f580".into(),
            primary_button_fg: "#18181bff".into(),
        };
        let chrome = Chrome { list_hover: "#ffffff10".into(), skeleton: "#a1a1aa2e".into(), danger: "#fb7185ff".into(), radius: 10. };
        let delete = DeleteChrome { radius: 10., accent: "#27272aff".into(), notice_stroke: "#ffffff14".into() };
        for tree in [document(&ink, &chrome), delete_document(&ink, &delete)] {
            let bound = super::super::doc::bound_values(&tree);
            let mut surface = eui_core::surface::Surface::new("settings-statuses", Default::default());
            let outcome = surface.set_nested(serde_json::from_value(tree).expect("parses"));
            let issues = super::super::doc::unexpected_issues(&bound, &outcome.issues);
            assert!(issues.is_empty(), "{issues:?}");
        }
    }

    #[test]
    fn the_create_placeholder_names_the_category() {
        assert_eq!(create_placeholder(IssueStatusCategory::Started), "New started status");
    }
}
