//! The issue-detail properties sidebar (masterplan-v3 §4.2; web parity
//! target: `apps/web/src/components/issue-properties-panel.tsx` in `sidebar`
//! layout — desktop always renders the non-mobile branch, §4.9).
//!
//! Groups top-to-bottom exactly like web: Status · Priority · Assignee ·
//! Labels · Due date (+ the recurrence control beneath it) · Project. Every
//! control mutates immediately through tRPC (`issues.update` /
//! `issueLabels.add|remove`) in the §4.1 un-gated form — the Electric echo
//! re-renders. `completed_at` is server-managed and never set here.
//!
//! Due-date control (§4.2, web `DueDateControl` sidebar layout): a ghost
//! trigger labeled **"Due date" when empty**, icon + short date once set
//! (the EXP-3 icon-only-when-empty rule applies to the board ROW's
//! `due-date-dropdown.tsx`, not this panel); the popover hosts the
//! gpui-component `Calendar` plus a Clear action. Clearing the date
//! cascade-nulls `due_time`/`end_time` (web `onDueDateSelect`). The synced
//! `issues` shape deliberately drops `due_time`/`end_time` (§5.4), so the
//! desktop shows no time inputs — date edits leave any server-side times
//! untouched except through that cascade.
//!
//! Recurrence control: a `Repeat` trigger whose popover mirrors
//! `recurrence-editor.tsx` — first-due `Calendar` + interval/unit selectors
//! (values from the domain contract) + "Stop recurring". Every change
//! commits immediately (web `RecurrenceControl` calls
//! `onRecurrenceChange` per edit): `issues.update({ recurrence_interval,
//! recurrence_unit, due_date: first_due })`; stop clears both to null.

use chrono::NaiveDate;

use gpui::{
    div, px, App, AppContext as _, Entity, FontWeight, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    calendar::{Calendar, CalendarEvent, CalendarState, Date},
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    popover::Popover,
    v_flex, ActiveTheme as _, Icon, Sizable as _, Side,
};
use sync::Store;

use domain::board::format_short_date;
use domain::options::{
    get_issue_priority_config, get_issue_status_config, IssueOption, ISSUE_PRIORITY_OPTIONS,
    ISSUE_STATUS_OPTIONS,
};
use domain::rows::{Issue, Label, Project, User};

use crate::icons::{option_icon, ExpIcon};
use crate::queries;

/// Web `w-72` sidebar width.
const PANEL_WIDTH: f32 = 288.;

pub struct PropertiesPanel {
    issue_id: Option<String>,
    due_calendar: Entity<CalendarState>,
    recur_calendar: Entity<CalendarState>,
    _subscriptions: Vec<Subscription>,
}

impl PropertiesPanel {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let due_calendar = cx.new(|cx| CalendarState::new(window, cx));
        let recur_calendar = cx.new(|cx| CalendarState::new(window, cx));

        let mut subscriptions = Vec::new();
        // User picked a due date in the popover → immediate mutation (the
        // popover stays open, web parity — shadcn's Calendar doesn't
        // auto-close either).
        subscriptions.push(cx.subscribe(
            &due_calendar,
            |this, _, event: &CalendarEvent, cx| {
                let CalendarEvent::Selected(Date::Single(Some(date))) = event else {
                    return;
                };
                this.commit_due_date(Some(*date), cx);
            },
        ));
        // First-due picked in the recurrence editor → commit the whole
        // recurrence value (web RecurrenceEditor onChange).
        subscriptions.push(cx.subscribe(
            &recur_calendar,
            |this, _, event: &CalendarEvent, cx| {
                let CalendarEvent::Selected(Date::Single(Some(date))) = event else {
                    return;
                };
                let (interval, unit) = this.effective_recurrence(cx);
                this.commit_recurrence(interval, unit, Some(*date), cx);
            },
        ));
        // Re-render on every collection this panel reads; keep the calendars
        // mirroring the synced due date (remote edits included).
        let collections = Store::global(cx).collections().clone();
        subscriptions.push(cx.observe_in(
            &collections.issues,
            window,
            |this, _, window, cx| {
                this.sync_calendars(window, cx);
                cx.notify();
            },
        ));
        for subscription in [
            cx.observe(&collections.labels, |_, _, cx| cx.notify()),
            cx.observe(&collections.issue_labels, |_, _, cx| cx.notify()),
            cx.observe(&collections.users, |_, _, cx| cx.notify()),
            cx.observe(&collections.workspace_members, |_, _, cx| cx.notify()),
            cx.observe(&collections.projects, |_, _, cx| cx.notify()),
        ] {
            subscriptions.push(subscription);
        }

        Self {
            issue_id: None,
            due_calendar,
            recur_calendar,
            _subscriptions: subscriptions,
        }
    }

    /// Point the panel at another issue.
    pub fn set_issue(
        &mut self,
        issue_id: Option<String>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.issue_id == issue_id {
            return;
        }
        self.issue_id = issue_id;
        self.sync_calendars(window, cx);
        cx.notify();
    }

    fn issue(&self, cx: &App) -> Option<Issue> {
        let issue_id = self.issue_id.as_deref()?;
        Store::global(cx)
            .collections()
            .issues
            .read(cx)
            .get(issue_id)
            .cloned()
    }

    /// Push the synced due date into both calendar states (idempotent —
    /// `set_date` does not emit `Selected`; only user clicks do).
    fn sync_calendars(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let due = self
            .issue(cx)
            .and_then(|issue| issue.due_date)
            .and_then(|date| NaiveDate::parse_from_str(&date, "%Y-%m-%d").ok());
        self.due_calendar.update(cx, |calendar, cx| {
            calendar.set_date(Date::Single(due), window, cx);
        });
        let first_due = due.unwrap_or_else(today);
        self.recur_calendar.update(cx, |calendar, cx| {
            calendar.set_date(Date::Single(Some(first_due)), window, cx);
        });
    }

    // -- mutations -------------------------------------------------------------

    /// Web `onDueDateSelect`: set/clear the date; clearing cascade-nulls
    /// `due_time` + `end_time`.
    pub(crate) fn commit_due_date(
        &mut self,
        date: Option<NaiveDate>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(issue_id) = self.issue_id.clone() else {
            return;
        };
        let mut input = api::issues::IssuesUpdateInput::new(issue_id);
        match date {
            Some(date) => {
                input.due_date = api::Patch::Set(format_mutation_date(date));
            }
            None => {
                input.due_date = api::Patch::Null;
                input.due_time = api::Patch::Null;
                input.end_time = api::Patch::Null;
            }
        }
        spawn_issue_update(cx, input);
    }

    /// The recurrence the editor shows: the issue's when set, else the web
    /// default draft `{ interval: 1, unit: week }`.
    fn effective_recurrence(&self, cx: &App) -> (i64, String) {
        let issue = self.issue(cx);
        let interval = issue
            .as_ref()
            .and_then(|issue| issue.recurrence_interval)
            .unwrap_or(1);
        let unit = issue
            .and_then(|issue| issue.recurrence_unit)
            .unwrap_or_else(|| "week".to_string());
        (interval, unit)
    }

    /// Web `handleRecurrenceChange(next)`: commit interval + unit + first-due
    /// in one mutation.
    pub(crate) fn commit_recurrence(
        &mut self,
        interval: i64,
        unit: String,
        first_due: Option<NaiveDate>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(issue_id) = self.issue_id.clone() else {
            return;
        };
        let first_due = first_due.or_else(|| {
            self.issue(cx)
                .and_then(|issue| issue.due_date)
                .and_then(|date| NaiveDate::parse_from_str(&date, "%Y-%m-%d").ok())
        });
        let mut input = api::issues::IssuesUpdateInput::new(issue_id);
        input.recurrence_interval = api::Patch::Set(interval);
        input.recurrence_unit = api::Patch::Set(unit);
        if let Some(first_due) = first_due {
            input.due_date = api::Patch::Set(format_mutation_date(first_due));
        }
        spawn_issue_update(cx, input);
    }

    /// Web `handleRecurrenceChange(null)`: clear both fields.
    pub(crate) fn stop_recurrence(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(issue_id) = self.issue_id.clone() else {
            return;
        };
        let mut input = api::issues::IssuesUpdateInput::new(issue_id);
        input.recurrence_interval = api::Patch::Null;
        input.recurrence_unit = api::Patch::Null;
        spawn_issue_update(cx, input);
    }

    // -- derived reads ----------------------------------------------------------

    /// Workspace members eligible as assignees (web passes the workspace's
    /// member users; synthetic agent users are excluded).
    fn member_users(&self, issue: &Issue, cx: &App) -> Vec<User> {
        let collections = Store::global(cx).collections();
        let Some(project) = collections.projects.read(cx).get(&issue.project_id).cloned()
        else {
            return Vec::new();
        };
        let member_ids: std::collections::HashSet<String> = collections
            .workspace_members
            .read(cx)
            .iter()
            .filter(|member| member.workspace_id == project.workspace_id)
            .map(|member| member.user_id.clone())
            .collect();
        let mut users: Vec<User> = collections
            .users
            .read(cx)
            .iter()
            .filter(|user| {
                member_ids.contains(&user.id) && user.is_agent != Some(true)
            })
            .cloned()
            .collect();
        users.sort_by_key(|user| {
            user.name
                .clone()
                .or_else(|| user.email.clone())
                .unwrap_or_default()
                .to_lowercase()
        });
        users
    }

    /// The workspace's labels, sort-order sorted (web LabelPicker query).
    fn workspace_labels(&self, issue: &Issue, cx: &App) -> Vec<Label> {
        let collections = Store::global(cx).collections();
        let Some(project) = collections.projects.read(cx).get(&issue.project_id).cloned()
        else {
            return Vec::new();
        };
        let mut labels: Vec<Label> = collections
            .labels
            .read(cx)
            .iter()
            .filter(|label| label.workspace_id == project.workspace_id)
            .cloned()
            .collect();
        labels.sort_by(|a, b| {
            a.sort_order
                .unwrap_or(f64::MAX)
                .total_cmp(&b.sort_order.unwrap_or(f64::MAX))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        labels
    }

    fn selected_label_ids(&self, issue_id: &str, cx: &App) -> Vec<String> {
        Store::global(cx)
            .collections()
            .issue_labels
            .read(cx)
            .iter()
            .filter(|link| link.issue_id == issue_id)
            .map(|link| link.label_id.clone())
            .collect()
    }

    // -- controls ---------------------------------------------------------------

    fn status_control(&self, issue: &Issue, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let config = get_issue_status_config(issue.status);
        let current = issue.status;
        let issue_id = issue.id.clone();
        Button::new("prop-status")
            .ghost()
            .xsmall()
            .icon(option_icon(config, cx))
            .label(SharedString::from(config.label))
            .dropdown_menu(move |menu, _, cx| {
                let mut menu = menu.check_side(Side::Right);
                for option in &ISSUE_STATUS_OPTIONS {
                    menu = menu.item(option_item(option, option.value == current, cx, {
                        let issue_id = issue_id.clone();
                        let value = option.value;
                        // L27: `duplicate` opens the picker; every other status writes.
                        move |window, cx| {
                            crate::issue_detail::apply_status_selection(
                                issue_id.clone(),
                                value,
                                window,
                                cx,
                            );
                        }
                    }));
                }
                menu
            })
    }

    fn priority_control(&self, issue: &Issue, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let config = get_issue_priority_config(issue.priority);
        let current = issue.priority;
        let issue_id = issue.id.clone();
        Button::new("prop-priority")
            .ghost()
            .xsmall()
            .icon(option_icon(config, cx))
            .label(SharedString::from(config.label))
            .dropdown_menu(move |menu, _, cx| {
                let mut menu = menu.check_side(Side::Right);
                for option in &ISSUE_PRIORITY_OPTIONS {
                    menu = menu.item(option_item(option, option.value == current, cx, {
                        let issue_id = issue_id.clone();
                        let value = option.value;
                        move |_window, cx| {
                            let mut input = api::issues::IssuesUpdateInput::new(issue_id.clone());
                            input.priority = Some(value);
                            spawn_issue_update(cx, input);
                        }
                    }));
                }
                menu
            })
    }

    /// Web `AssigneePicker`: avatar + name when assigned, `User` glyph +
    /// "Assignee" otherwise; menu offers Unassign + every member.
    fn assignee_control(&self, issue: &Issue, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let users = self.member_users(issue, cx);
        let selected = issue
            .assignee_id
            .as_deref()
            .and_then(|id| users.iter().find(|user| user.id == id))
            .cloned();
        let issue_id = issue.id.clone();
        let current_id = issue.assignee_id.clone();

        let trigger = match &selected {
            Some(user) => Button::new("prop-assignee").ghost().xsmall().label(
                SharedString::from(crate::comments::author_label(Some(user))),
            ),
            None => Button::new("prop-assignee")
                .ghost()
                .xsmall()
                .icon(
                    Icon::new(gpui_component::IconName::User)
                        .text_color(cx.theme().muted_foreground),
                )
                .label("Assignee"),
        };

        trigger.dropdown_menu(move |menu, _, _| {
            let mut menu = menu.check_side(Side::Right);
            if current_id.is_some() {
                let issue_id = issue_id.clone();
                menu = menu.item(PopupMenuItem::new("Unassign").on_click(move |_, _, cx| {
                    let mut input = api::issues::IssuesUpdateInput::new(issue_id.clone());
                    input.assignee_id = api::Patch::Null;
                    spawn_issue_update(cx, input);
                }));
            }
            for user in &users {
                let name = crate::comments::author_label(Some(user));
                let checked = current_id.as_deref() == Some(user.id.as_str());
                let issue_id = issue_id.clone();
                let user_id = user.id.clone();
                menu = menu.item(
                    PopupMenuItem::new(SharedString::from(name))
                        .checked(checked)
                        .on_click(move |_, _, cx| {
                            let mut input =
                                api::issues::IssuesUpdateInput::new(issue_id.clone());
                            input.assignee_id = api::Patch::Set(user_id.clone());
                            spawn_issue_update(cx, input);
                        }),
                );
            }
            menu
        })
    }

    /// Web `LabelPicker`: toggle menu over the workspace's labels (colored
    /// dot + name + check). Label creation stays in workspace settings on
    /// desktop v1.
    fn labels_control(&self, issue: &Issue, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let labels = self.workspace_labels(issue, cx);
        let selected = self.selected_label_ids(&issue.id, cx);
        let issue_id = issue.id.clone();

        let trigger_label = if selected.is_empty() {
            "Labels".to_string()
        } else {
            let names: Vec<&str> = labels
                .iter()
                .filter(|label| selected.contains(&label.id))
                .map(|label| label.name.as_str())
                .collect();
            names.join(", ")
        };

        Button::new("prop-labels")
            .ghost()
            .xsmall()
            .icon(Icon::from(ExpIcon::Tag).text_color(cx.theme().muted_foreground))
            .label(SharedString::from(trigger_label))
            .dropdown_menu(move |menu, _, _| {
                let mut menu = menu.check_side(Side::Right);
                if labels.is_empty() {
                    return menu.item(PopupMenuItem::label("No labels in this workspace"));
                }
                for label in &labels {
                    let checked = selected.contains(&label.id);
                    let issue_id = issue_id.clone();
                    let label_id = label.id.clone();
                    let dot_color = label
                        .color
                        .as_deref()
                        .and_then(parse_hex_color)
                        .unwrap_or(gpui::opaque_grey(0.5, 1.0));
                    let name = SharedString::from(label.name.clone());
                    menu = menu.item(
                        PopupMenuItem::element(move |_, cx| {
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(
                                    div()
                                        .size_2()
                                        .rounded_full()
                                        .flex_shrink_0()
                                        .bg(dot_color),
                                )
                                .child(
                                    div()
                                        .text_color(cx.theme().popover_foreground)
                                        .child(name.clone()),
                                )
                        })
                        .checked(checked)
                        .on_click(move |_, _, cx| {
                            toggle_label(cx, issue_id.clone(), label_id.clone(), checked);
                        }),
                    );
                }
                menu
            })
    }

    /// The due-date control (web `DueDateControl`, sidebar layout): a ghost
    /// `CalendarDays` trigger labeled with the formatted short date when set,
    /// or the literal "Due date" when empty (`triggerLabel = dueDate ?
    /// formatDate(dueDate) : 'Due date'`); popover = Calendar + Clear.
    fn due_control(&self, issue: &Issue, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let due = issue.due_date.clone();
        let label: SharedString = match due.as_deref() {
            Some(date) => format_short_date(date).into(),
            None => "Due date".into(),
        };
        let trigger = Button::new("prop-due")
            .ghost()
            .xsmall()
            .icon(Icon::from(ExpIcon::CalendarDays).text_color(cx.theme().muted_foreground))
            .label(label);

        let calendar = self.due_calendar.clone();
        let panel = cx.entity();
        let has_due = due.is_some();
        Popover::new("prop-due-popover")
            .trigger(trigger)
            .content(move |_, _, cx| {
                let panel = panel.clone();
                let mut content = v_flex()
                    .p_2()
                    .gap_2()
                    .child(Calendar::new(&calendar));
                if has_due {
                    content = content.child(
                        Button::new("prop-due-clear")
                            .ghost()
                            .xsmall()
                            .label("Clear due date")
                            .text_color(cx.theme().muted_foreground)
                            .on_click(move |_, _, cx| {
                                panel.update(cx, |panel, cx| {
                                    panel.commit_due_date(None, cx);
                                });
                            }),
                    );
                }
                content.into_any_element()
            })
    }

    /// The recurrence control (web `RecurrenceControl` +
    /// `recurrence-editor.tsx`): Repeat trigger; popover hosts first-due
    /// Calendar + interval/unit selectors + Stop recurring.
    fn recurrence_control(&self, issue: &Issue, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let is_recurring =
            issue.recurrence_interval.is_some() && issue.recurrence_unit.is_some();
        let trigger_label = if is_recurring {
            format_recurrence(
                issue.recurrence_interval.unwrap_or(1),
                issue.recurrence_unit.as_deref().unwrap_or("week"),
            )
        } else {
            "Add recurrence".to_string()
        };
        let (interval, unit) = self.effective_recurrence(cx);

        let calendar = self.recur_calendar.clone();
        let panel = cx.entity();
        Popover::new("prop-recurrence-popover")
            .trigger(
                Button::new("prop-recurrence")
                    .ghost()
                    .xsmall()
                    .icon(Icon::from(ExpIcon::Repeat).text_color(cx.theme().muted_foreground))
                    .label(SharedString::from(trigger_label)),
            )
            .content(move |_, _, cx| {
                let mut content = v_flex()
                    .p_3()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("First due"),
                    )
                    .child(Calendar::new(&calendar))
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("repeats every"),
                            )
                            .child(interval_select(interval, unit.clone(), &panel))
                            .child(unit_select(interval, unit.clone(), &panel)),
                    );
                if is_recurring {
                    let panel = panel.clone();
                    content = content.child(
                        Button::new("prop-recurrence-stop")
                            .ghost()
                            .xsmall()
                            .label("Stop recurring")
                            .text_color(cx.theme().muted_foreground)
                            .on_click(move |_, _, cx| {
                                panel.update(cx, |panel, cx| panel.stop_recurrence(cx));
                            }),
                    );
                }
                content.into_any_element()
            })
    }

    fn project_chip(&self, issue: &Issue, cx: &App) -> Option<impl IntoElement> {
        let project: Project = Store::global(cx)
            .collections()
            .projects
            .read(cx)
            .get(&issue.project_id)
            .cloned()?;
        let color = project
            .color
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or(cx.theme().muted_foreground);
        Some(
            h_flex()
                .gap_1p5()
                .px_2()
                .py_1()
                .rounded_md()
                .bg(cx.theme().accent.opacity(0.4))
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .items_center()
                .child(div().size_2p5().rounded_full().flex_shrink_0().bg(color))
                .child(SharedString::from(project.name)),
        )
    }
}

impl Render for PropertiesPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let base = v_flex()
            .w(px(PANEL_WIDTH))
            .flex_shrink_0()
            .h_full()
            .border_l_1()
            .border_color(cx.theme().border)
            .px_3()
            .py_3()
            .gap_3()
            .text_sm();

        let Some(issue) = self.issue(cx) else {
            return base;
        };

        base.child(property_group("Status", self.status_control(&issue, cx), cx))
            .child(property_group(
                "Priority",
                self.priority_control(&issue, cx),
                cx,
            ))
            .child(property_group(
                "Assignee",
                self.assignee_control(&issue, cx),
                cx,
            ))
            .child(property_group("Labels", self.labels_control(&issue, cx), cx))
            .child(property_group(
                "Due date",
                v_flex()
                    .items_start()
                    .gap_1()
                    .child(self.due_control(&issue, cx))
                    .child(self.recurrence_control(&issue, cx)),
                cx,
            ))
            .when_some(self.project_chip(&issue, cx), |panel, chip| {
                panel.child(property_group("Project", chip, cx))
            })
    }
}

use gpui::prelude::FluentBuilder as _;

// ---------------------------------------------------------------------------
// Pieces
// ---------------------------------------------------------------------------

/// Web `PropertyGroup`: UPPERCASE micro-label over the control
/// (`text-[11px] font-medium uppercase tracking-wide text-muted-foreground`
/// — the CSS `uppercase` transform is baked into the string here).
fn property_group(label: &'static str, control: impl IntoElement, cx: &App) -> impl IntoElement {
    v_flex()
        .gap_1()
        .items_start()
        .child(
            div()
                .text_size(px(11.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(cx.theme().muted_foreground)
                .child(SharedString::from(label.to_uppercase())),
        )
        .child(control)
}

/// One option row (same as the board's): table icon + label + right-side
/// check (EXP-1 #5).
fn option_item<V: Copy + 'static>(
    option: &'static IssueOption<V>,
    checked: bool,
    cx: &App,
    on_select: impl Fn(&mut Window, &mut App) + 'static,
) -> PopupMenuItem {
    PopupMenuItem::new(SharedString::from(option.label))
        .icon(option_icon(option, cx))
        .checked(checked)
        .on_click(move |_, window, cx| on_select(window, cx))
}

/// Interval dropdown (contract `recurrenceIntervals`).
fn interval_select(
    current: i64,
    unit: String,
    panel: &Entity<PropertiesPanel>,
) -> impl IntoElement {
    let panel = panel.clone();
    Button::new("prop-recur-interval")
        .outline()
        .xsmall()
        .label(SharedString::from(current.to_string()))
        .dropdown_menu(move |menu, _, _| {
            let mut menu = menu.check_side(Side::Right);
            for value in domain::contract::RECURRENCE_INTERVALS {
                let value = *value as i64;
                let panel = panel.clone();
                let unit = unit.clone();
                menu = menu.item(
                    PopupMenuItem::new(SharedString::from(value.to_string()))
                        .checked(value == current)
                        .on_click(move |_, _, cx| {
                            let unit = unit.clone();
                            panel.update(cx, |panel, cx| {
                                panel.commit_recurrence(value, unit, None, cx);
                            });
                        }),
                );
            }
            menu
        })
}

/// Unit dropdown (contract `recurrenceUnitValues`); plural label when the
/// interval is > 1 (web parity: "week" vs "weeks").
fn unit_select(interval: i64, current: String, panel: &Entity<PropertiesPanel>) -> impl IntoElement {
    let panel = panel.clone();
    let label = if interval == 1 {
        current.clone()
    } else {
        format!("{current}s")
    };
    Button::new("prop-recur-unit")
        .outline()
        .xsmall()
        .label(SharedString::from(label))
        .dropdown_menu(move |menu, _, _| {
            let mut menu = menu.check_side(Side::Right);
            for unit in domain::contract::RECURRENCE_UNIT_VALUES {
                let panel = panel.clone();
                let item_label = if interval == 1 {
                    (*unit).to_string()
                } else {
                    format!("{unit}s")
                };
                let checked = *unit == current;
                menu = menu.item(
                    PopupMenuItem::new(SharedString::from(item_label))
                        .checked(checked)
                        .on_click(move |_, _, cx| {
                            let unit = (*unit).to_string();
                            panel.update(cx, |panel, cx| {
                                panel.commit_recurrence(interval, unit, None, cx);
                            });
                        }),
                );
            }
            menu
        })
}

/// Web `issueLabels.add` / `issueLabels.remove` toggle. `pub(crate)` — shared
/// with the issue-row context menu's Labels submenu (§4.2).
pub(crate) fn toggle_label(
    cx: &mut App,
    issue_id: String,
    label_id: String,
    currently_selected: bool,
) {
    let Some(trpc) = queries::trpc_client(cx) else {
        log::warn!("[ui] issueLabels toggle skipped: no signed-in account");
        return;
    };
    cx.background_executor()
        .spawn(async move {
            let result = if currently_selected {
                api::labels::issue_labels_remove(&trpc, &issue_id, &label_id)
            } else {
                api::labels::issue_labels_add(&trpc, &issue_id, &label_id)
            };
            if let Err(err) = result {
                log::warn!("[ui] issueLabels toggle failed: {err}");
            }
        })
        .detach();
}

/// §4.1 un-gated `issues.update` on a background thread — the Electric echo
/// re-renders; errors log and the UI stays put (web inline behavior). Shared
/// by the properties panel, the detail header actions and the title save.
pub(crate) fn spawn_issue_update(cx: &mut App, input: api::issues::IssuesUpdateInput) {
    let Some(trpc) = queries::trpc_client(cx) else {
        log::warn!("[ui] issues.update skipped: no signed-in account");
        return;
    };
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = api::issues::issues_update(&trpc, &input) {
                log::warn!("[ui] issues.update({}) failed: {err}", input.id);
            }
        })
        .detach();
}

/// Web `formatDateForMutation`: `YYYY-MM-DD`.
fn format_mutation_date(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// Web `formatRecurrence` (`db-schema/domain.ts`): "Every week" / "Every 2
/// weeks".
fn format_recurrence(interval: i64, unit: &str) -> String {
    let noun = if interval == 1 {
        unit.to_string()
    } else {
        format!("{unit}s")
    };
    if interval == 1 {
        format!("Every {noun}")
    } else {
        format!("Every {interval} {noun}")
    }
}

fn today() -> NaiveDate {
    chrono::Utc::now().date_naive()
}

/// `#rrggbb` (leading `#` optional) → Hsla (labels/projects store hex
/// strings). Shared with the detail view's breadcrumb/banner dots.
pub(crate) fn parse_hex_color(hex: &str) -> Option<gpui::Hsla> {
    let hex = hex.trim();
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(
        gpui::Rgba {
            r: r as f32 / 255.,
            g: g as f32 / 255.,
            b: b as f32 / 255.,
            a: 1.0,
        }
        .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recurrence_label_matches_web_format_recurrence() {
        assert_eq!(format_recurrence(1, "week"), "Every week");
        assert_eq!(format_recurrence(2, "week"), "Every 2 weeks");
        assert_eq!(format_recurrence(1, "day"), "Every day");
        assert_eq!(format_recurrence(30, "day"), "Every 30 days");
    }

    #[test]
    fn mutation_date_is_iso_ymd() {
        let date = NaiveDate::from_ymd_opt(2026, 7, 3).unwrap();
        assert_eq!(format_mutation_date(date), "2026-07-03");
    }

    #[test]
    fn hex_colors_parse_and_reject_garbage() {
        assert!(parse_hex_color("#22c55e").is_some());
        assert!(parse_hex_color("22c55e").is_some());
        assert!(parse_hex_color("#nope!!").is_none());
        assert!(parse_hex_color("").is_none());
    }
}
