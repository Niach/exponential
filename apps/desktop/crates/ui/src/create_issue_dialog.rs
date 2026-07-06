//! Create-issue dialog (masterplan-v3 §4.2 — layout matches
//! `apps/web/src/components/create-issue-dialog.tsx` +
//! `issue-editor/dialog-shell.tsx` field-for-field).
//!
//! Structure (the web desktop branch shape: pinned header/footer,
//! scrollable body — never the old all-scrolling dialog):
//!
//! - header row: project pill (color dot + prefix) · `›` · "New issue" · ✕
//! - borderless title `Input` (text-lg, web `border-none focus-visible:ring-0`)
//! - the §4.5 [`crate::markdown::MarkdownEditor`] in a `flex_1` scroll region
//!   — clipboard-image paste stages `draft://` blocks; submit
//!   mirrors the web flow: create with the drafts **stripped**, upload each
//!   staged image, rewrite the URLs and `issues.update` the final description
//! - chip row: status / priority / assignee / labels / due-date (with
//!   `due_time`/`end_time` + "All day", §4.2) + the `…` overflow menu with
//!   **Make recurring…** (web L344-398)
//! - footer: "Create more" `Switch` (web uses a Switch, not a checkbox —
//!   L488-494) + the indigo submit button; while recurring the footer swaps
//!   to the `RecurrenceEditor` (first-due calendar + interval/unit + stop),
//!   status is forced to `todo` at submit and the due chip hides.
//!
//! Submit (§4.1): `issues.create` on a background thread; when "Create more"
//! is off the close+navigate is **gated** on the created row becoming visible
//! in the synced `issues` collection (the desktop's awaitTxId analog); when
//! on, fields reset immediately (web parity) and the Electric echo fills the
//! board.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, App, AppContext as _, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription,
    Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    calendar::{Calendar, CalendarEvent, CalendarState, Date},
    h_flex,
    input::{Input, InputEvent, InputState},
    menu::{DropdownMenu as _, PopupMenuItem},
    popover::Popover,
    switch::Switch,
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Side, Sizable as _,
    WindowExt as _,
};
use sync::Store;

use domain::options::{ISSUE_PRIORITY_OPTIONS, ISSUE_STATUS_OPTIONS};
use domain::rows::{Label, User};
use domain::{IssuePriority, IssueStatus};

use crate::actions::NewIssue;
use crate::attachments_row;
use crate::icons::{option_icon, ExpIcon};
use crate::markdown::{self, MarkdownEditor};
use crate::navigation::{active_project_id, nav_for_window, navigate, Screen};
use crate::queries;

/// Register the App-global [`NewIssue`] handler (call once from `ui::init`).
/// The action is the §3.6 unit action the filter bar dispatches; the target
/// project is the window's active project (the top-bar picker scope — the
/// All Issues tool window's list).
pub fn init(cx: &mut App) {
    cx.on_action(|_: &NewIssue, cx| {
        crate::navigation::on_active_window(cx, |window, cx| {
            let nav = nav_for_window(window, cx);
            let Some(project_id) = active_project_id(&nav, cx) else {
                return; // no project in scope — nothing to create into
            };
            open(window, cx, project_id);
        });
    });
}

/// Open the dialog. Resolves the project row (prefix, color, workspace) off
/// the synced collections; a no-op when the project is unknown (racing a
/// delete).
pub fn open(window: &mut Window, cx: &mut App, project_id: String) {
    let collections = Store::global(cx).collections();
    let Some(project) = collections.projects.read(cx).get(&project_id).cloned() else {
        log::warn!("[ui] NewIssue for unknown project {project_id}");
        return;
    };

    let view = cx.new(|cx| {
        CreateIssueDialogView::new(
            project.id.clone(),
            project.workspace_id.clone(),
            project.prefix.clone().unwrap_or_default(),
            project.color.clone(),
            window,
            cx,
        )
    });

    window.open_dialog(cx, move |dialog, window, cx| {
        let busy = view.read(cx).submitting;
        // Web: sm:max-w-[40rem] p-0 max-h-[85vh] — the dialog HUGS its
        // content (no fixed empty band); only past the cap does the editor
        // region scroll, with header/chips/footer pinned.
        let max_height = window.viewport_size().height * 0.85;
        dialog
            .w(px(640.))
            .max_h(max_height)
            .p_0()
            .close_button(false)
            .overlay_closable(!busy)
            .keyboard(!busy)
            // Enter anywhere in the dialog submits (web form submit); never
            // let the stock ConfirmDialog close an un-submitted dialog.
            .on_ok({
                let view = view.clone();
                move |_, window, cx| {
                    view.update(cx, |view, cx| view.submit(window, cx));
                    false
                }
            })
            .child(view.clone())
    });
}

/// Web `RecurrenceValue` (`recurrence-editor.tsx`).
#[derive(Clone, Debug, PartialEq)]
struct RecurrenceValue {
    first_due: Option<chrono::NaiveDate>,
    interval: i64,
    unit: &'static str,
}

pub struct CreateIssueDialogView {
    project_id: String,
    workspace_id: String,
    project_prefix: String,
    project_color: Option<String>,

    title: Entity<InputState>,
    /// The §4.5 block editor in create-dialog (staging) mode: pasted images
    /// stay `draft://` blocks until submit resolves them.
    description: Entity<MarkdownEditor>,
    status: IssueStatus,
    default_status: IssueStatus,
    priority: IssuePriority,
    assignee_id: Option<String>,
    selected_label_ids: Vec<String>,
    due_date: Option<chrono::NaiveDate>,
    due_calendar: Entity<CalendarState>,
    due_time: Entity<InputState>,
    end_time: Entity<InputState>,
    recurrence: Option<RecurrenceValue>,
    recurrence_calendar: Entity<CalendarState>,
    create_more: bool,
    submitting: bool,
    error: Option<SharedString>,
    focused_once: bool,
    _subscriptions: Vec<Subscription>,
}

impl CreateIssueDialogView {
    fn new(
        project_id: String,
        workspace_id: String,
        project_prefix: String,
        project_color: Option<String>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Issue title"));
        // Shared configured-editor constructor (§4.5): completion + pills
        // scoped to this workspace, upload staged (`upload_issue = None`).
        let description = crate::description_editor::build_editor(
            Some(workspace_id.clone()),
            None,
            "Add description...",
            "",
            window,
            cx,
        );
        let due_calendar = cx.new(|cx| CalendarState::new(window, cx));
        let recurrence_calendar = cx.new(|cx| CalendarState::new(window, cx));
        let due_time = cx.new(|cx| InputState::new(window, cx).placeholder("HH:MM"));
        let end_time = cx.new(|cx| InputState::new(window, cx).placeholder("HH:MM"));

        let mut subscriptions = Vec::new();
        // Enter in the (single-line) title submits, like the web form.
        subscriptions.push(cx.subscribe_in(
            &title,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.submit(window, cx);
                }
            },
        ));
        // Title emptiness drives the submit button's disabled state.
        subscriptions.push(cx.subscribe(&title, |_, _, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                cx.notify();
            }
        }));
        // Due-date picks mirror into our state (web `onDueDateSelect`).
        subscriptions.push(cx.subscribe(
            &due_calendar,
            |this, _, event: &CalendarEvent, cx| {
                let CalendarEvent::Selected(Date::Single(date)) = event else {
                    return;
                };
                this.due_date = *date;
                cx.notify();
            },
        ));
        // Recurrence first-due picks (web `RecurrenceEditor` calendar).
        subscriptions.push(cx.subscribe(
            &recurrence_calendar,
            |this, _, event: &CalendarEvent, cx| {
                let CalendarEvent::Selected(Date::Single(date)) = event else {
                    return;
                };
                if let Some(recurrence) = &mut this.recurrence {
                    recurrence.first_due = *date;
                    cx.notify();
                }
            },
        ));
        // The end-time input enables only once a start time exists (web
        // `disabled={!dueTime}`).
        subscriptions.push(cx.subscribe(&due_time, |_, _, event: &InputEvent, cx| {
            if let InputEvent::Change = event {
                cx.notify();
            }
        }));
        // The footer attachment rail mirrors the live description (web
        // `imageOccurrences` over the current markdown) — re-render on every
        // editor change (pastes, deletions, chip removals).
        subscriptions.push(cx.observe(&description, |_, _, cx| cx.notify()));

        Self {
            project_id,
            workspace_id,
            project_prefix,
            project_color,
            title,
            description,
            status: IssueStatus::Backlog,
            default_status: IssueStatus::Backlog,
            priority: IssuePriority::None,
            assignee_id: None,
            selected_label_ids: Vec::new(),
            due_date: None,
            due_calendar,
            due_time,
            end_time,
            recurrence: None,
            recurrence_calendar,
            create_more: false,
            submitting: false,
            error: None,
            focused_once: false,
            _subscriptions: subscriptions,
        }
    }

    /// Web `resetFields`: clear everything except "Create more" and (when
    /// recurring) the recurrence settings, whose first-due resets to today.
    fn reset_fields(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.title.update(cx, |state, cx| {
            state.set_value("", window, cx);
        });
        self.description.update(cx, |editor, cx| {
            editor.set_markdown("", window, cx);
        });
        self.status = self.default_status;
        self.priority = IssuePriority::None;
        self.assignee_id = None;
        self.selected_label_ids.clear();
        self.due_date = None;
        self.due_calendar.update(cx, |state, cx| {
            state.set_date(Date::Single(None), window, cx);
        });
        self.due_time
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.end_time
            .update(cx, |state, cx| state.set_value("", window, cx));
        if let Some(recurrence) = &mut self.recurrence {
            let today = chrono::Local::now().date_naive();
            recurrence.first_due = Some(today);
            self.recurrence_calendar.update(cx, |state, cx| {
                state.set_date(Date::Single(Some(today)), window, cx);
            });
        }
        self.error = None;
        self.submitting = false;
        self.title.update(cx, |state, cx| state.focus(window, cx));
        cx.notify();
    }

    /// Web `enableRecurrence`: first-due = due date or today, every 1 week.
    fn enable_recurrence(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let first_due = self
            .due_date
            .unwrap_or_else(|| chrono::Local::now().date_naive());
        self.recurrence = Some(RecurrenceValue {
            first_due: Some(first_due),
            interval: 1,
            unit: "week",
        });
        self.recurrence_calendar.update(cx, |state, cx| {
            state.set_date(Date::Single(Some(first_due)), window, cx);
        });
        cx.notify();
    }

    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let title = self.title.read(cx).value().trim().to_string();
        if title.is_empty() || self.submitting {
            return;
        }
        if let Some(recurrence) = &self.recurrence {
            // Web disables submit until a first-due exists.
            if recurrence.first_due.is_none() {
                return;
            }
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            self.error = Some("Not signed in.".into());
            cx.notify();
            return;
        };

        self.error = None;
        self.submitting = true;
        cx.notify();

        // Build the exact web mutation input (`create-issue-dialog.tsx`
        // handleSubmit): recurring forces status `todo` and uses the
        // recurrence first-due as the due date.
        let mut input = api::issues::IssuesCreateInput::new(self.project_id.clone(), title);
        input.status = Some(if self.recurrence.is_some() {
            IssueStatus::Todo
        } else {
            self.status
        });
        input.priority = Some(self.priority);
        input.assignee_id = self.assignee_id.clone();
        // Web submit flow (create-issue-dialog.tsx): create with the staged
        // `draft://` images STRIPPED, upload them post-create, then update
        // the description with the canonical attachment URLs.
        let markdown = self.description.read(cx).markdown(cx);
        let staged = self.description.read(cx).staged_images(cx);
        let stripped_description = strip_draft_images(&markdown);
        if !stripped_description.is_empty() {
            input.description = Some(stripped_description.clone());
        }
        let transport = queries::attachment_transport(cx);
        // `TrpcClient` is not `Clone` — a second one for the post-create
        // description update (cheap: an `Agent` + two `Arc`s, §5.7).
        let trpc_update = queries::trpc_client(cx);
        let due_date = match &self.recurrence {
            Some(recurrence) => recurrence.first_due,
            None => self.due_date,
        };
        input.due_date = due_date.map(|date| date.format("%Y-%m-%d").to_string());
        // Cascade rules (§4.2): times ride only with a date; end only with a
        // start.
        if input.due_date.is_some() && self.recurrence.is_none() {
            if let Some(due_time) = valid_time(&self.due_time.read(cx).value()) {
                input.due_time = Some(due_time);
                if let Some(end_time) = valid_time(&self.end_time.read(cx).value()) {
                    input.end_time = Some(end_time);
                }
            }
        }
        if !self.selected_label_ids.is_empty() {
            input.label_ids = Some(self.selected_label_ids.clone());
        }
        if let Some(recurrence) = &self.recurrence {
            input.recurrence_interval = Some(recurrence.interval);
            input.recurrence_unit = Some(recurrence.unit.to_string());
        }

        let create_more = self.create_more;
        cx.spawn_in(window, async move |this, window| {
            let result = window
                .background_executor()
                .spawn(async move { api::issues::issues_create(&trpc, &input) })
                .await;

            match result {
                Ok(output) => {
                    let issue_id = output.issue.id.clone();

                    // Post-create image resolution (web parity: per-image
                    // failures are tolerated — failed drafts drop out of the
                    // final description).
                    if let (false, Some(transport), Some(trpc_update)) =
                        (staged.is_empty(), transport, trpc_update)
                    {
                        let upload_issue = issue_id.clone();
                        let full_markdown = markdown.clone();
                        let stripped = stripped_description.clone();
                        let final_description = window
                            .background_executor()
                            .spawn(async move {
                                let mut resolved = std::collections::HashMap::new();
                                for image in &staged {
                                    match transport.upload(
                                        &upload_issue,
                                        &image.filename,
                                        &image.content_type,
                                        &image.bytes,
                                    ) {
                                        Ok(uploaded) => {
                                            resolved.insert(
                                                image.draft_url.clone(),
                                                uploaded.url,
                                            );
                                        }
                                        Err(err) => {
                                            log::warn!(
                                                "[ui] create-dialog image upload failed: {err}"
                                            );
                                        }
                                    }
                                }
                                // Rewrite the uploads in, drop the failures.
                                let rewritten = markdown::image_paste::rewrite_image_urls(
                                    &full_markdown,
                                    &resolved,
                                );
                                let final_description = strip_draft_images(&rewritten);
                                if final_description == stripped {
                                    return None; // nothing survived — no update
                                }
                                let mut update = api::issues::IssuesUpdateInput::new(
                                    upload_issue.clone(),
                                );
                                update.description = if final_description.is_empty() {
                                    api::Patch::Null
                                } else {
                                    api::Patch::Set(final_description.clone())
                                };
                                if let Err(err) =
                                    api::issues::issues_update(&trpc_update, &update)
                                {
                                    log::warn!(
                                        "[ui] create-dialog description update failed: {err}"
                                    );
                                }
                                Some(final_description)
                            })
                            .await;
                        let _ = final_description; // board renders off the echo
                    }
                    if create_more {
                        // Web parity: reset immediately, keep the dialog open,
                        // let the Electric echo fill the board.
                        let _ = this.update_in(window, |this, window, cx| {
                            this.reset_fields(window, cx);
                        });
                        return;
                    }
                    // Gated path (§4.1): close + navigate only once the row
                    // is visible in the synced collection.
                    let issues = window
                        .update(|_, cx| Store::global(cx).collections().issues.clone())
                        .ok();
                    if let Some(issues) = issues {
                        queries::await_row_visible(&issues, &issue_id, window).await;
                    }
                    let _ = this.update_in(window, |_, window, cx| {
                        window.close_dialog(cx);
                        navigate(window, cx, Screen::IssueDetail { issue_id });
                    });
                }
                Err(err) => {
                    let _ = this.update_in(window, |this, _, cx| {
                        this.error = Some(format!("{err}").into());
                        this.submitting = false;
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    // -- chips (web `issue-editor/chips.tsx`) --------------------------------

    fn status_chip(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let config = domain::options::get_issue_status_config(self.status);
        let current = self.status;
        let view = cx.entity().clone();
        chip_button("create-status-chip", cx)
            .icon(option_icon(config, cx))
            .label(SharedString::from(config.label))
            .dropdown_menu(move |menu, _window, cx| {
                let mut menu = menu.check_side(Side::Right);
                for option in &ISSUE_STATUS_OPTIONS {
                    let view = view.clone();
                    let value = option.value;
                    menu = menu.item(
                        PopupMenuItem::new(SharedString::from(option.label))
                            .icon(option_icon(option, cx))
                            .checked(option.value == current)
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.status = value;
                                    cx.notify();
                                });
                            }),
                    );
                }
                menu
            })
    }

    fn priority_chip(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let config = domain::options::get_issue_priority_config(self.priority);
        let current = self.priority;
        let view = cx.entity().clone();
        chip_button("create-priority-chip", cx)
            .icon(option_icon(config, cx))
            .label(SharedString::from(config.label))
            .dropdown_menu(move |menu, _window, cx| {
                let mut menu = menu.check_side(Side::Right);
                for option in &ISSUE_PRIORITY_OPTIONS {
                    let view = view.clone();
                    let value = option.value;
                    menu = menu.item(
                        PopupMenuItem::new(SharedString::from(option.label))
                            .icon(option_icon(option, cx))
                            .checked(option.value == current)
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.priority = value;
                                    cx.notify();
                                });
                            }),
                    );
                }
                menu
            })
    }

    /// Web `AssigneePicker`: "Assignee" or the selected member's name;
    /// options = workspace members + Unassign.
    fn assignee_chip(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let users = queries::workspace_users(cx, &self.workspace_id);
        let selected = self
            .assignee_id
            .as_deref()
            .and_then(|id| users.iter().find(|user| user.id == id));
        let label: SharedString = selected
            .map(|user| SharedString::from(display_name(user)))
            .unwrap_or_else(|| "Assignee".into());
        let current = self.assignee_id.clone();
        let view = cx.entity().clone();

        chip_button("create-assignee-chip", cx)
            .icon(
                Icon::new(IconName::User)
                    .xsmall()
                    .text_color(cx.theme().muted_foreground),
            )
            .label(label)
            .dropdown_menu(move |menu, _window, cx| {
                let mut menu = menu.check_side(Side::Right);
                if current.is_some() {
                    let view = view.clone();
                    menu = menu.item(
                        PopupMenuItem::new("Unassign")
                            .icon(Icon::new(IconName::Close).text_color(cx.theme().muted_foreground))
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.assignee_id = None;
                                    cx.notify();
                                });
                            }),
                    );
                }
                for user in &users {
                    let view = view.clone();
                    let id = user.id.clone();
                    menu = menu.item(
                        PopupMenuItem::new(SharedString::from(display_name(user)))
                            .icon(Icon::new(IconName::CircleUser))
                            .checked(current.as_deref() == Some(user.id.as_str()))
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.assignee_id = Some(id.clone());
                                    cx.notify();
                                });
                            }),
                    );
                }
                menu
            })
    }

    /// Web `LabelPicker` trigger: "Label" or up-to-3 color dots + joined
    /// names; the menu toggles membership (the web popover multi-toggles
    /// without closing — a dropdown reopens per toggle in v1).
    fn labels_chip(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let labels = queries::workspace_labels(cx, &self.workspace_id);
        let selected: Vec<&Label> = labels
            .iter()
            .filter(|label| self.selected_label_ids.contains(&label.id))
            .collect();
        let label: SharedString = if selected.is_empty() {
            "Label".into()
        } else {
            selected
                .iter()
                .map(|label| label.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
                .into()
        };
        let selected_ids = self.selected_label_ids.clone();
        let view = cx.entity().clone();

        chip_button("create-labels-chip", cx)
            .icon(
                Icon::from(ExpIcon::Tag)
                    .xsmall()
                    .text_color(cx.theme().muted_foreground),
            )
            .label(label)
            .dropdown_menu(move |menu, _window, cx| {
                let mut menu = menu.check_side(Side::Right);
                if labels.is_empty() {
                    return menu.label("No labels in this workspace");
                }
                for label in &labels {
                    let view = view.clone();
                    let id = label.id.clone();
                    let dot = label
                        .color
                        .as_deref()
                        .and_then(parse_hex_color)
                        .unwrap_or(cx.theme().muted_foreground);
                    menu = menu.item(
                        PopupMenuItem::new(SharedString::from(label.name.clone()))
                            .icon(Icon::from(ExpIcon::Tag).text_color(dot))
                            .checked(selected_ids.contains(&label.id))
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    if let Some(ix) = this
                                        .selected_label_ids
                                        .iter()
                                        .position(|existing| existing == &id)
                                    {
                                        this.selected_label_ids.remove(ix);
                                    } else {
                                        this.selected_label_ids.push(id.clone());
                                    }
                                    cx.notify();
                                });
                            }),
                    );
                }
                menu
            })
    }

    /// Web due chip: `CalendarDays` + "Jul 3 · HH:MM" or "Due date"; the
    /// popover hosts the calendar + the time row with "All day" (§4.2).
    fn due_chip(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let label: SharedString = match self.due_date {
            Some(date) => {
                let short = domain::board::format_short_date(&date.format("%Y-%m-%d").to_string());
                match valid_time(&self.due_time.read(cx).value()) {
                    Some(time) => format!("{short} · {time}").into(),
                    None => short.into(),
                }
            }
            None => "Due date".into(),
        };

        let calendar = self.due_calendar.clone();
        let due_time = self.due_time.clone();
        let end_time = self.end_time.clone();
        let view = cx.entity().clone();

        Popover::new("create-due-popover")
            .trigger(
                chip_button("create-due-chip", cx)
                    .icon(
                        Icon::from(ExpIcon::CalendarDays)
                            .xsmall()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .label(label),
            )
            .content(move |_, _window, cx| {
                let has_date = view.read(cx).due_date.is_some();
                let has_start = valid_time(&due_time.read(cx).value()).is_some();
                let has_any_time = has_start || !end_time.read(cx).value().trim().is_empty();
                v_flex()
                    .w(px(280.))
                    .child(Calendar::new(&calendar))
                    .when(has_date, |this| {
                        this.child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .border_t_1()
                                .border_color(cx.theme().border)
                                .px_1()
                                .pt_2()
                                .mt_2()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("Time")
                                .child(div().w(px(64.)).child(Input::new(&due_time).xsmall()))
                                .child("–")
                                .child(
                                    div().w(px(64.)).child(
                                        Input::new(&end_time).xsmall().disabled(!has_start),
                                    ),
                                )
                                .when(has_any_time, |this| {
                                    let due_time = due_time.clone();
                                    let end_time = end_time.clone();
                                    this.child(
                                        Button::new("create-due-all-day")
                                            .ghost()
                                            .xsmall()
                                            .label("All day")
                                            .on_click(move |_, window, cx| {
                                                due_time.update(cx, |state, cx| {
                                                    state.set_value("", window, cx)
                                                });
                                                end_time.update(cx, |state, cx| {
                                                    state.set_value("", window, cx)
                                                });
                                            }),
                                    )
                                }),
                        )
                    })
            })
    }

    /// Web overflow `…` menu: "Make recurring…" (disabled once recurring).
    fn overflow_menu(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let recurring = self.recurrence.is_some();
        let view = cx.entity().clone();
        Button::new("create-overflow")
            .ghost()
            .xsmall()
            .icon(
                Icon::new(IconName::Ellipsis)
                    .xsmall()
                    .text_color(cx.theme().muted_foreground),
            )
            .dropdown_menu(move |menu, _window, cx| {
                let view = view.clone();
                menu.item(
                    PopupMenuItem::new("Make recurring…")
                        .icon(
                            Icon::from(ExpIcon::Repeat)
                                .text_color(cx.theme().muted_foreground),
                        )
                        .disabled(recurring)
                        .on_click(move |_, window, cx| {
                            view.update(cx, |this, cx| this.enable_recurrence(window, cx));
                        }),
                )
            })
    }

    // -- footer ----------------------------------------------------------------

    /// Web `RecurrenceEditor` row: first-due date popover + "repeats every"
    /// interval/unit selects + stop.
    fn recurrence_editor(
        &self,
        recurrence: &RecurrenceValue,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let first_due_label: SharedString = recurrence
            .first_due
            .map(|date| domain::board::format_short_date(&date.format("%Y-%m-%d").to_string()).into())
            .unwrap_or_else(|| "Pick date".into());
        let calendar = self.recurrence_calendar.clone();
        let interval = recurrence.interval;
        let unit = recurrence.unit;
        let view = cx.entity().clone();
        let view_for_interval = view.clone();
        let view_for_unit = view;

        h_flex()
            .gap_2()
            .items_center()
            .text_sm()
            .child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child("First due"),
            )
            .child(
                Popover::new("recurrence-first-due")
                    .trigger(
                        Button::new("recurrence-first-due-trigger")
                            .outline()
                            .xsmall()
                            .icon(Icon::from(ExpIcon::CalendarDays).xsmall())
                            .label(first_due_label),
                    )
                    .content(move |_, _window, _cx| {
                        v_flex().w(px(280.)).child(Calendar::new(&calendar))
                    }),
            )
            .child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child("repeats every"),
            )
            .child(
                Button::new("recurrence-interval")
                    .outline()
                    .xsmall()
                    .label(SharedString::from(interval.to_string()))
                    .dropdown_menu(move |menu, _window, _cx| {
                        let mut menu = menu.check_side(Side::Right).scrollable(true).max_h(px(320.));
                        for value in domain::contract::RECURRENCE_INTERVALS {
                            let view = view_for_interval.clone();
                            let value = *value as i64;
                            menu = menu.item(
                                PopupMenuItem::new(SharedString::from(value.to_string()))
                                    .checked(value == interval)
                                    .on_click(move |_, _, cx| {
                                        view.update(cx, |this, cx| {
                                            if let Some(recurrence) = &mut this.recurrence {
                                                recurrence.interval = value;
                                                cx.notify();
                                            }
                                        });
                                    }),
                            );
                        }
                        menu
                    }),
            )
            .child(
                Button::new("recurrence-unit")
                    .outline()
                    .xsmall()
                    .label(SharedString::from(pluralize_unit(unit, interval)))
                    .dropdown_menu(move |menu, _window, _cx| {
                        let mut menu = menu.check_side(Side::Right);
                        for value in domain::contract::RECURRENCE_UNIT_VALUES {
                            let view = view_for_unit.clone();
                            let value: &'static str = value;
                            menu = menu.item(
                                PopupMenuItem::new(SharedString::from(pluralize_unit(
                                    value, interval,
                                )))
                                .checked(value == unit)
                                .on_click(move |_, _, cx| {
                                    view.update(cx, |this, cx| {
                                        if let Some(recurrence) = &mut this.recurrence {
                                            recurrence.unit = value;
                                            cx.notify();
                                        }
                                    });
                                }),
                            );
                        }
                        menu
                    }),
            )
            .child(
                Button::new("recurrence-stop")
                    .ghost()
                    .xsmall()
                    .icon(
                        Icon::new(IconName::Close)
                            .xsmall()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.recurrence = None;
                        cx.notify();
                    })),
            )
    }

    /// Web `IssueEditorAttachmentRail` (the dialog footer's left slot): one
    /// chip per image occurrence in the live description + the trailing
    /// "N images" count (always shown — "0 images" included); each chip
    /// carries the web's remove ✕, dropping that occurrence from the
    /// markdown (`removeMarkdownImageByOccurrence`).
    fn attachment_rail(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let markdown = self.description.read(cx).markdown(cx);
        let occurrences = attachments_row::extract_image_occurrences(&markdown);
        let count = occurrences.len();
        let editor = self.description.clone();
        let removable = !self.submitting;

        h_flex()
            .min_w_0()
            .flex_1()
            .gap_2()
            .items_center()
            .child(
                h_flex()
                    .min_w_0()
                    .flex_1()
                    .gap_1p5()
                    .overflow_hidden()
                    .children(occurrences.iter().enumerate().map(|(ix, occurrence)| {
                        let remove: Option<attachments_row::ChipRemove> =
                            removable.then(|| {
                                let editor = editor.clone();
                                let markdown = markdown.clone();
                                let on_click = Box::new(
                                    move |_: &gpui::ClickEvent,
                                          window: &mut Window,
                                          cx: &mut App| {
                                        let next = attachments_row::remove_image_occurrence(
                                            &markdown, ix,
                                        );
                                        editor.update(cx, |editor, cx| {
                                            editor.set_markdown(&next, window, cx);
                                        });
                                    },
                                )
                                    as Box<dyn Fn(&gpui::ClickEvent, &mut Window, &mut App)>;
                                (
                                    SharedString::from(format!("create-attachment-remove-{ix}")),
                                    on_click,
                                )
                            });
                        attachments_row::image_chip(
                            attachments_row::occurrence_label(occurrence, ix),
                            remove,
                            cx,
                        )
                    })),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(attachments_row::image_count_label(
                        count,
                    ))),
            )
    }

    fn footer(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let title_empty = self.title.read(cx).value().trim().is_empty();
        let recurring = self.recurrence.is_some();
        let submit_disabled = title_empty
            || self.submitting
            || self
                .recurrence
                .as_ref()
                .is_some_and(|recurrence| recurrence.first_due.is_none());
        let submit_label: &'static str = if self.submitting {
            "Creating..."
        } else if recurring {
            "Create recurring issue"
        } else {
            "Create issue"
        };
        let view = cx.entity().clone();

        let right = h_flex()
            .gap_3()
            .items_center()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Switch::new("create-more")
                            .small()
                            .checked(self.create_more)
                            .disabled(self.submitting)
                            .on_click({
                                let view = view.clone();
                                move |checked: &bool, _, cx| {
                                    let checked = *checked;
                                    view.update(cx, |this, cx| {
                                        this.create_more = checked;
                                        cx.notify();
                                    });
                                }
                            }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Create more"),
                    ),
            )
            .child(
                // Web submit: bg-indigo-600 hover:bg-indigo-700 text-white
                // h-7 text-xs — SOLID indigo (label swaps while creating, no
                // spinner, web parity).
                indigo_button("create-issue-submit", submit_disabled, cx)
                    .child(SharedString::from(submit_label))
                    .when(!submit_disabled, |button| {
                        button.on_click(cx.listener(|this, _, window, cx| {
                            this.submit(window, cx)
                        }))
                    }),
            );

        let left: gpui::AnyElement = match (&self.error, &self.recurrence) {
            (Some(error), _) => div()
                .text_xs()
                .text_color(cx.theme().danger)
                .child(error.clone())
                .into_any_element(),
            (None, Some(recurrence)) => self.recurrence_editor(recurrence, cx).into_any_element(),
            // Web `IssueEditorAttachmentRail` — always rendered ("0 images").
            (None, None) => self.attachment_rail(cx).into_any_element(),
        };

        h_flex()
            .px_4()
            .py_3()
            .gap_3()
            .items_center()
            .justify_between()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(div().min_w_0().flex_1().child(left))
            .child(right)
    }
}

impl Render for CreateIssueDialogView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // Web `autoFocus` on the title input (once, after the dialog mounts).
        if !self.focused_once {
            self.focused_once = true;
            self.title.update(cx, |state, cx| state.focus(window, cx));
        }

        let pill_color = self
            .project_color
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or(cx.theme().muted_foreground);
        let closable = !self.submitting;

        // Header: project pill · › · "New issue" · ✕ (web px-5 pt-4 pb-2).
        let header = h_flex()
            .px_5()
            .pt_4()
            .pb_2()
            .items_center()
            .justify_between()
            .child(
                h_flex()
                    .gap_1p5()
                    .items_center()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        h_flex()
                            .gap_1p5()
                            .items_center()
                            .rounded(cx.theme().radius)
                            .bg(cx.theme().accent.opacity(0.5))
                            .px_2()
                            .py_0p5()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(cx.theme().foreground)
                            .child(div().size_2p5().rounded_full().bg(pill_color))
                            .child(SharedString::from(self.project_prefix.clone())),
                    )
                    .child(Icon::new(IconName::ChevronRight).xsmall())
                    .child("New issue"),
            )
            .child(
                Button::new("create-issue-close")
                    .ghost()
                    .xsmall()
                    .icon(
                        Icon::new(IconName::Close)
                            .xsmall()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .disabled(!closable)
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.submitting {
                            return;
                        }
                        window.close_dialog(cx);
                    })),
            );

        // Chip row (web px-4 py-2 border-t): status · priority · assignee ·
        // labels · due (hidden while recurring) · overflow.
        let chips = h_flex()
            .px_4()
            .py_2()
            .gap_1()
            .items_center()
            .flex_wrap()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(self.status_chip(cx))
            .child(self.priority_chip(cx))
            .child(self.assignee_chip(cx))
            .child(self.labels_chip(cx))
            .when(self.recurrence.is_none(), |this| {
                this.child(self.due_chip(cx))
            })
            .child(self.overflow_menu(cx));

        v_flex()
            .size_full()
            .child(header)
            .child(
                // Borderless title (web text-lg font-medium px-5).
                div().px_3().child(
                    Input::new(&self.title)
                        .appearance(false)
                        .text_lg()
                        .font_weight(FontWeight::MEDIUM),
                ),
            )
            .child(
                // Only this region scrolls; header/chips/footer
                // pinned. The 56px floor mirrors web's `.tiptap-content
                // { min-height: 3.5rem }` so an empty dialog still shows a
                // description area (content-hugging above that).
                div()
                    .id("create-issue-description")
                    .flex_1()
                    .min_h(px(56.))
                    .px_3()
                    .overflow_y_scroll()
                    .child(self.description.clone()),
            )
            .child(chips)
            .child(self.footer(cx))
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// The web chip trigger: `Button variant="ghost" size="xs"` in
/// muted-foreground.
fn chip_button(id: &'static str, cx: &App) -> Button {
    let _ = cx;
    Button::new(id).ghost().xsmall()
}

/// Drop `![alt](draft://…)` image paragraphs from canonical GFM (the web's
/// `removeMarkdownImagesByUrl` for the create-then-upload flow). Staged
/// images are standalone paragraphs in the canonical form, so line filtering
/// + re-canonicalization is exact.
fn strip_draft_images(markdown: &str) -> String {
    let kept: Vec<&str> = markdown
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !(trimmed.starts_with("![")
                && trimmed.contains(&format!("]({}", markdown::image_paste::DRAFT_SCHEME)))
        })
        .collect();
    let joined = kept.join("\n");
    let canonical = markdown::canonicalize(&joined);
    canonical.trim().to_string()
}

/// Web `bg-indigo-600` (a literal Tailwind color on web, not a theme token).
fn indigo_600() -> gpui::Hsla {
    rgb_hsla(0x4f, 0x46, 0xe5)
}

/// Web `hover:bg-indigo-700`.
fn indigo_700() -> gpui::Hsla {
    rgb_hsla(0x43, 0x38, 0xca)
}

/// Pressed state: indigo-800.
fn indigo_800() -> gpui::Hsla {
    rgb_hsla(0x37, 0x30, 0xa3)
}

/// The web's solid primary-action button (`bg-indigo-600 hover:bg-indigo-700
/// text-white text-xs font-medium rounded-md`, size xs) — used by the board
/// "New Issue" button and the create-dialog submit.
///
/// Hand-rolled `div` on purpose: the pinned gpui-component
/// `ButtonCustomVariant` ignores its `.foreground()` (labels render in the
/// fill color) and washes the fill toward transparent
/// (`mix_oklab(transparent, 0.2..0.4)` in `button.rs`), so it cannot produce
/// this solid fill. Callers add label/icon children and — when not disabled —
/// an `.on_click`.
pub(crate) fn indigo_button(
    id: impl Into<gpui::ElementId>,
    disabled: bool,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    let base = div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .gap_1()
        .h_6()
        .px_2p5()
        .rounded(cx.theme().radius)
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(gpui::white())
        .bg(indigo_600())
        .cursor_default();
    if disabled {
        // Web `disabled:opacity-50 disabled:pointer-events-none` (callers
        // skip `.on_click` while disabled).
        base.opacity(0.5)
    } else {
        base.hover(|style| style.bg(indigo_700()))
            .active(|style| style.bg(indigo_800()))
    }
}

fn rgb_hsla(r: u8, g: u8, b: u8) -> gpui::Hsla {
    gpui::Rgba {
        r: r as f32 / 255.,
        g: g as f32 / 255.,
        b: b as f32 / 255.,
        a: 1.0,
    }
    .into()
}

/// `#rrggbb` → Hsla (project/label colors are hex strings).
pub(crate) fn parse_hex_color(hex: &str) -> Option<gpui::Hsla> {
    let hex = hex.trim().strip_prefix('#')?;
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(rgb_hsla(r, g, b))
}

/// A user's display name (name, else email, else the `Member <LAST4>` fallback
/// — web shows `user.name`; a row with neither field is a co-member whose PII
/// didn't sync).
fn display_name(user: &User) -> String {
    user.name
        .clone()
        .or_else(|| user.email.clone())
        .unwrap_or_else(|| domain::member_fallback_label(&user.id))
}

/// Validate an `HH:MM` time input (the server's zod shape). Empty/invalid →
/// `None` (treated as unset, mirroring the web TimeInput's nulling).
pub(crate) fn valid_time(value: &str) -> Option<String> {
    let value = value.trim();
    let (hours, minutes) = value.split_once(':')?;
    if hours.len() != 2 || minutes.len() != 2 {
        return None;
    }
    let hour: u8 = hours.parse().ok()?;
    let minute: u8 = minutes.parse().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some(format!("{hour:02}:{minute:02}"))
}

/// Web `RecurrenceEditor` unit labels: singular for 1, plural otherwise.
fn pluralize_unit(unit: &str, interval: i64) -> String {
    if interval == 1 {
        unit.to_string()
    } else {
        format!("{unit}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_time_accepts_hh_mm_and_rejects_garbage() {
        assert_eq!(valid_time("09:30"), Some("09:30".to_string()));
        assert_eq!(valid_time(" 23:59 "), Some("23:59".to_string()));
        assert_eq!(valid_time("24:00"), None);
        assert_eq!(valid_time("9:30"), None);
        assert_eq!(valid_time("09:60"), None);
        assert_eq!(valid_time(""), None);
        assert_eq!(valid_time("late"), None);
    }

    #[test]
    fn hex_colors_parse_and_reject_bad_input() {
        assert!(parse_hex_color("#6366f1").is_some());
        assert!(parse_hex_color("6366f1").is_none());
        assert!(parse_hex_color("#66f1").is_none());
        assert!(parse_hex_color("#zzzzzz").is_none());
    }

    #[test]
    fn recurrence_units_pluralize_like_web() {
        assert_eq!(pluralize_unit("week", 1), "week");
        assert_eq!(pluralize_unit("week", 2), "weeks");
        assert_eq!(pluralize_unit("day", 30), "days");
    }

    #[test]
    fn strip_draft_images_removes_only_staged_images() {
        let markdown = "Intro text\n\n![shot](draft://abc-1)\n\n![kept](/api/attachments/xyz)\n\nOutro";
        assert_eq!(
            strip_draft_images(markdown),
            "Intro text\n\n![kept](/api/attachments/xyz)\n\nOutro"
        );
        assert_eq!(strip_draft_images("![only](draft://a)"), "");
        assert_eq!(strip_draft_images(""), "");
    }
}
