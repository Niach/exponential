//! EXP-760 — the SHARED half of "compose a new issue": the property state
//! behind the chip row (web `issue-editor/chips.tsx`) and the post-create
//! pipeline every composer runs.
//!
//! Two surfaces use it: the full-window [`crate::create_issue_dialog`] and
//! the inline sub-issue composer, both presentations of the one
//! [`crate::issue_composer`]. Everything that is genuinely shared lives here
//! — the chips, the wire input the picks build, and the create →
//! image-resolution → file-upload → row-visible sequence — so the composer is
//! a form and a submit button rather than a second copy of the dialog.
//!
//! What deliberately stays with each surface: layout, the title input, the
//! description editor and what happens AFTER the row exists (the dialog
//! closes and navigates; the composer clears itself and stays open).

use std::rc::Rc;
use std::sync::Arc;

use chrono::NaiveDate;
use gpui::{
    div, px, AnyElement, App, AppContext as _, Entity, IntoElement, ParentElement as _, Render,
    SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    calendar::{CalendarEvent, CalendarState, Date},
    input::InputState,
    menu::DropdownMenu as _,
    ActiveTheme as _, Icon, Sizable as _,
};

use domain::rows::{Label, User};
use domain::{IssuePriority, IssueStatus};

use crate::icons::{option_icon, registry, ExpIcon};
use crate::markdown::image_paste::{self, StagedImage};
use crate::pickers::chip_button;
use crate::queries;

/// The properties a new issue is composed with. An entity because every chip
/// is a popover whose pick lands asynchronously — the same shape the dialog
/// always had, lifted out of it.
pub(crate) struct IssueDraft {
    pub(crate) team_id: String,
    /// EXP-314: the picked status as a wire-ready pick (a synced row id, or
    /// the enum anchor of a constructed `builtin:<key>` fallback).
    pub(crate) status: crate::pickers::StatusPick,
    pub(crate) priority: IssuePriority,
    /// EXP-50: `Some(member)` when the team has exactly one human member at
    /// open time — the assignee chip hides and `assignee_id` defaults (and
    /// resets) to that member so the created issue is optimistically correct.
    pub(crate) solo_member_id: Option<String>,
    pub(crate) assignee_id: Option<String>,
    pub(crate) selected_label_ids: Vec<String>,
    /// EXP-288: the shared label picker's search input (host-owned).
    label_query: Entity<InputState>,
    pub(crate) due_date: Option<NaiveDate>,
    due_calendar: Entity<CalendarState>,
    _subscriptions: Vec<Subscription>,
}

impl IssueDraft {
    pub(crate) fn new(team_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let due_calendar = cx.new(|cx| CalendarState::new(window, cx));
        let label_query = cx.new(|cx| InputState::new(window, cx).placeholder("Filter labels…"));

        let mut subscriptions = Vec::new();
        // Due-date picks mirror into our state (web `onDueDateSelect`).
        subscriptions.push(
            cx.subscribe(&due_calendar, |this, _, event: &CalendarEvent, cx| {
                let CalendarEvent::Selected(Date::Single(date)) = event else {
                    return;
                };
                this.due_date = *date;
                cx.notify();
            }),
        );

        // EXP-50: exactly one human member ⇒ no assignment choice (0 members
        // = membership not synced yet, keep the picker).
        let members = queries::team_users(cx, &team_id);
        let solo_member_id = match members.as_slice() {
            [only] => Some(only.id.clone()),
            _ => None,
        };

        Self {
            status: default_status(&team_id, cx),
            priority: IssuePriority::None,
            assignee_id: solo_member_id.clone(),
            solo_member_id,
            selected_label_ids: Vec::new(),
            label_query,
            due_date: None,
            due_calendar,
            team_id,
            _subscriptions: subscriptions,
        }
    }

    /// Back to the open-time defaults, KEEPING the team. The sub-issue
    /// composer stays open after a create, so the next child starts clean.
    pub(crate) fn reset(&mut self, cx: &mut gpui::Context<Self>) {
        self.status = default_status(&self.team_id, cx);
        self.priority = IssuePriority::None;
        self.assignee_id = self.solo_member_id.clone();
        self.selected_label_ids.clear();
        self.due_date = None;
        cx.notify();
    }

    /// Write the picks onto an `issues.create` input (web
    /// `create-issue-dialog.tsx` `handleSubmit`). The board, title,
    /// description and `parent_id` stay the caller's.
    pub(crate) fn apply_to_create(&self, input: &mut api::issues::IssuesCreateInput) {
        self.status.apply_to_create(input);
        input.priority = Some(self.priority);
        input.assignee_id = self.assignee_id.clone();
        input.due_date = self
            .due_date
            .map(|date| date.format("%Y-%m-%d").to_string());
        if !self.selected_label_ids.is_empty() {
            input.label_ids = Some(self.selected_label_ids.clone());
        }
    }

    /// The chip row (web `IssueEditorChips`): status · priority · assignee ·
    /// labels · due date. `prefix` namespaces the element ids, so two
    /// composers can be on screen at once.
    ///
    /// EXP-50: the assignee chip is dropped on a solo team.
    pub(crate) fn chips(
        &self,
        prefix: &'static str,
        cx: &mut gpui::Context<Self>,
    ) -> Vec<AnyElement> {
        let mut chips = vec![
            self.status_chip(prefix, cx).into_any_element(),
            self.priority_chip(prefix, cx).into_any_element(),
        ];
        if self.solo_member_id.is_none() {
            chips.push(self.assignee_chip(prefix, cx).into_any_element());
        }
        chips.push(self.labels_chip(prefix, cx).into_any_element());
        chips.push(self.due_chip(prefix, cx).into_any_element());
        chips
    }

    fn status_chip(&self, prefix: &'static str, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // EXP-314: the chip renders the picked TEAM status; the menu lists the
        // team's own vocabulary (constructed defaults before the shape syncs).
        let statuses = crate::queries::team_status_options(cx, &self.team_id);
        let current = self.status.clone();
        let resolved = statuses
            .iter()
            .find(|status| {
                status.row_id.is_some() && status.row_id == current.status_id
                    || (current.status_id.is_none() && status.anchor() == current.anchor)
            })
            .cloned()
            .unwrap_or_else(|| domain::statuses::constructed_default(current.anchor));
        let current_key = resolved.group_key.clone();
        let view = cx.entity().clone();
        chip_button(SharedString::from(format!("{prefix}-status-chip")), cx)
            .icon(crate::icons::resolved_status_icon(&resolved, cx))
            .child(crate::pickers::chip_label(resolved.name.clone(), false, cx))
            .dropdown_menu(move |menu, _window, cx| {
                let view = view.clone();
                let statuses = crate::queries::team_status_options(cx, &view.read(cx).team_id);
                crate::pickers::status_menu(
                    menu,
                    &statuses,
                    &current_key,
                    // A brand-new issue can't be a duplicate of anything yet —
                    // no duplicate row here (web `creatableStatusOptions`).
                    crate::pickers::StatusMenuScope::Assignable,
                    Rc::new(move |pick, _window, cx| {
                        view.update(cx, |this, cx| {
                            this.status = pick;
                            cx.notify();
                        });
                    }),
                    cx,
                )
            })
    }

    fn priority_chip(
        &self,
        prefix: &'static str,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let config = domain::options::get_issue_priority_config(self.priority);
        let current = self.priority;
        let view = cx.entity().clone();
        chip_button(SharedString::from(format!("{prefix}-priority-chip")), cx)
            .icon(option_icon(config, cx))
            .child(crate::pickers::chip_label(config.label, false, cx))
            .dropdown_menu(move |menu, _window, cx| {
                let view = view.clone();
                crate::pickers::priority_menu(
                    menu,
                    current,
                    Rc::new(move |value, _window, cx| {
                        view.update(cx, |this, cx| {
                            this.priority = value;
                            cx.notify();
                        });
                    }),
                    cx,
                )
            })
    }

    /// Web `AssigneePicker`: "Assignee" or the selected member's name;
    /// options = team members + Unassign.
    fn assignee_chip(
        &self,
        prefix: &'static str,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let users = queries::team_users(cx, &self.team_id);
        let selected = self
            .assignee_id
            .as_deref()
            .and_then(|id| users.iter().find(|user| user.id == id));
        let label: SharedString = selected
            .map(|user| SharedString::from(display_name(user)))
            .unwrap_or_else(|| "Assignee".into());
        let current = self.assignee_id.clone();
        let view = cx.entity().clone();

        chip_button(SharedString::from(format!("{prefix}-assignee-chip")), cx)
            .icon(
                Icon::new(registry::UI_ASSIGNEE)
                    .xsmall()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(crate::pickers::chip_label(label, selected.is_none(), cx))
            .dropdown_menu(move |menu, _window, _cx| {
                let view = view.clone();
                crate::pickers::assignee_menu(
                    menu,
                    &users,
                    current.as_deref(),
                    Rc::new(move |picked, _window, cx| {
                        view.update(cx, |this, cx| {
                            this.assignee_id = picked;
                            cx.notify();
                        });
                    }),
                )
            })
    }

    /// Web `LabelPicker` trigger: "Label" or the joined selected names.
    /// EXP-288: the shared SEARCHABLE multi-toggle popover (the properties
    /// panel recipe) — toggles no longer close/reopen the picker per pick.
    fn labels_chip(&self, prefix: &'static str, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let labels = queries::team_labels(cx, &self.team_id);
        let selected: Vec<&Label> = labels
            .iter()
            .filter(|label| self.selected_label_ids.contains(&label.id))
            .collect();
        let unset = selected.is_empty();
        let label: SharedString = if unset {
            "Label".into()
        } else {
            selected
                .iter()
                .map(|label| label.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
                .into()
        };
        let view = cx.entity().clone();

        let trigger = chip_button(SharedString::from(format!("{prefix}-labels-chip")), cx)
            .icon(
                Icon::from(ExpIcon::Tag)
                    .xsmall()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(crate::pickers::chip_label(label, unset, cx));
        crate::pickers::label_picker_popover(
            SharedString::from(format!("{prefix}-labels-popover")),
            trigger,
            crate::pickers::LabelPickerParams {
                labels,
                selected_ids: self.selected_label_ids.clone(),
                query: self.label_query.clone(),
                on_toggle: Rc::new(move |label_id, was_selected, _window, cx| {
                    let label_id = label_id.to_string();
                    view.update(cx, |this, cx| {
                        if was_selected {
                            this.selected_label_ids
                                .retain(|existing| existing != &label_id);
                        } else {
                            this.selected_label_ids.push(label_id);
                        }
                        cx.notify();
                    });
                }),
                width: Some(px(crate::pickers::PICKER_SEARCH_WIDTH)),
            },
        )
    }

    /// Web due chip: `CalendarDays` + "Jul 3" or "Due date"; the popover hosts
    /// the calendar (date only — REV2-49 deleted the time-of-day fields, §4.2).
    fn due_chip(&self, prefix: &'static str, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let label: SharedString = match self.due_date {
            Some(date) => {
                domain::board::format_short_date(&date.format("%Y-%m-%d").to_string()).into()
            }
            None => "Due date".into(),
        };

        crate::pickers::due_date_popover(
            SharedString::from(format!("{prefix}-due-popover")),
            chip_button(SharedString::from(format!("{prefix}-due-chip")), cx)
                .icon(
                    Icon::from(ExpIcon::CalendarDays)
                        .xsmall()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(crate::pickers::chip_label(
                    label,
                    self.due_date.is_none(),
                    cx,
                )),
            self.due_calendar.clone(),
            Some(px(280.)),
            None,
        )
    }
}

/// The entity never paints itself — the host arranges [`Self::chips`].
impl Render for IssueDraft {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
    }
}

/// EXP-314: a new issue starts in the team's BACKLOG BUILTIN row (web
/// parity). Before the statuses shape syncs this is the constructed
/// `builtin:backlog` fallback, which writes the enum anchor instead of a
/// `statusId`.
fn default_status(team_id: &str, cx: &App) -> crate::pickers::StatusPick {
    crate::pickers::StatusPick::from_resolved(
        &queries::team_status_options(cx, team_id)
            .into_iter()
            .find(|status| status.builtin_key.as_deref() == Some("backlog"))
            .unwrap_or_else(|| domain::statuses::constructed_default(IssueStatus::Backlog)),
    )
}

pub(crate) fn display_name(user: &User) -> String {
    // Filter blanks before each fallback: a name-less Apple-ID user has
    // `name = ""` (Better Auth stores empty, not null), which must fall through
    // to the email rather than render as an empty label (EXP-228).
    user.name
        .clone()
        .filter(|name| !name.is_empty())
        .or_else(|| user.email.clone().filter(|email| !email.is_empty()))
        .unwrap_or_else(|| domain::member_fallback_label(&user.id))
}

// ---------------------------------------------------------------------------
// The post-create pipeline
// ---------------------------------------------------------------------------

/// Everything one create needs after the form is read. Built on the UI
/// thread, consumed on the background executor.
pub(crate) struct CreateJob {
    /// The wire input, images already STRIPPED from `description`.
    pub(crate) input: api::issues::IssuesCreateInput,
    /// The description as typed, `draft://` images included — the source the
    /// post-create rewrite runs over.
    pub(crate) markdown: String,
    /// What `input.description` actually carries; a rewrite that comes back
    /// identical skips the follow-up update.
    pub(crate) stripped_description: String,
    pub(crate) staged_images: Vec<StagedImage>,
    /// EXP-335: queued non-image draft files, uploaded post-create.
    pub(crate) staged_files: Vec<(String, String, Arc<Vec<u8>>)>,
}

/// Create the issue, then resolve its staged content, then wait for the row
/// to be VISIBLE in the synced collection (§4.1) before calling `on_done`
/// with its id. Per-item upload failures are logged and tolerated — they
/// never fail the created issue (web parity).
///
/// `on_done` receives `Err(user message)` when the create itself failed; the
/// caller owns what happens next (the dialog navigates, the inline composer
/// clears itself and stays open).
pub(crate) fn spawn_create(
    job: CreateJob,
    window: &mut Window,
    cx: &mut App,
    on_done: impl FnOnce(Result<String, SharedString>, &mut Window, &mut App) + 'static,
) {
    let Some(trpc) = queries::trpc_client(cx) else {
        // Deliberately synchronous: there is nothing to await.
        on_done(Err("Not signed in.".into()), window, cx);
        return;
    };
    // `TrpcClient` is not `Clone` — a second one for the post-create
    // description update (cheap: an `Agent` + two `Arc`s, §5.7).
    let trpc_update = queries::trpc_client(cx);
    let transport = queries::attachment_transport(cx);
    let transport_files = transport.clone();
    let CreateJob {
        input,
        markdown,
        stripped_description,
        staged_images,
        staged_files,
    } = job;

    window
        .spawn(cx, async move |window| {
            let result = window
                .background_executor()
                .spawn(async move { api::issues::issues_create(&trpc, &input) })
                .await;
            let output = match result {
                Ok(output) => output,
                Err(err) => {
                    let message: SharedString = err.user_message().into();
                    let _ = window.update(|window, cx| on_done(Err(message), window, cx));
                    return;
                }
            };
            let issue_id = output.issue.id.clone();

            // Post-create image resolution (web parity: per-image failures
            // are tolerated — failed drafts drop out of the description).
            if let (false, Some(transport), Some(trpc_update)) =
                (staged_images.is_empty(), transport, trpc_update)
            {
                let upload_issue = issue_id.clone();
                window
                    .background_executor()
                    .spawn(async move {
                        let mut resolved = std::collections::HashMap::new();
                        for image in &staged_images {
                            match transport.upload(
                                &upload_issue,
                                &image.filename,
                                &image.content_type,
                                &image.bytes,
                            ) {
                                Ok(uploaded) => {
                                    resolved.insert(image.draft_url.clone(), uploaded.url);
                                }
                                Err(err) => {
                                    log::warn!("[ui] draft image upload failed: {err}");
                                }
                            }
                        }
                        // Rewrite the uploads in, drop the failures.
                        let rewritten = image_paste::rewrite_image_urls(&markdown, &resolved);
                        let final_description = image_paste::strip_draft_images(&rewritten);
                        if final_description == stripped_description {
                            return; // nothing survived — no update
                        }
                        let mut update =
                            api::issues::IssuesUpdateInput::new(upload_issue.clone());
                        update.description = if final_description.is_empty() {
                            api::Patch::Null
                        } else {
                            api::Patch::Set(final_description)
                        };
                        if let Err(err) = api::issues::issues_update(&trpc_update, &update) {
                            log::warn!("[ui] draft description update failed: {err}");
                        }
                    })
                    .await;
            }

            // EXP-335: the queued non-image files (web draftFiles parity).
            if let (false, Some(transport)) = (staged_files.is_empty(), transport_files) {
                let upload_issue = issue_id.clone();
                window
                    .background_executor()
                    .spawn(async move {
                        for (filename, content_type, bytes) in &staged_files {
                            if let Err(err) =
                                transport.upload(&upload_issue, filename, content_type, bytes)
                            {
                                log::warn!("[ui] draft file upload failed: {err}");
                            }
                        }
                    })
                    .await;
            }

            // Gated path (§4.1): hand the id back only once the row is
            // visible in the synced collection.
            let issues = window
                .update(|_, cx| sync::Store::global(cx).collections().issues.clone())
                .ok();
            if let Some(issues) = issues {
                queries::await_row_visible(&issues, &issue_id, window).await;
            }
            let _ = window.update(|window, cx| on_done(Ok(issue_id), window, cx));
        })
        .detach();
}
