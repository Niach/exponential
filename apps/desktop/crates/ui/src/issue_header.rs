//! The issue-detail HEADER state (EXP-417 — was the right properties
//! sidebar): everything above the scrolling body except the title input.
//!
//! It is an entity for its state (calendars, picker queries, busy flags, the
//! ~12 collection subscriptions) but NOT a view — the detail view interleaves
//! its rows with the title block it owns itself, so this renders through
//! builders called from the host's render and assembled into the shared
//! `work_header::WorkHeader` (EXP-877): [`IssueHeader::right_cluster`]
//! (EXP-1294: related-work badge · Start · pin · `…`) and
//! [`IssueHeader::chip_row`] (Status · Priority · Assignee · Labels · Due
//! date · Estimate · Board · Origin · Threads). The host observes this
//! entity so a builder's `cx.notify()` reaches it.
//!
//! Every control mutates immediately through tRPC (`issues.update` /
//! `issueLabels.add|remove`) in the §4.1 un-gated form — the Electric echo
//! re-renders. `completed_at` is server-managed and never set here.
//!
//! Due-date control (§4.2, web `DueDateControl`): a chip labeled **"Due date"
//! when empty**, icon + short date once set; the popover hosts the
//! gpui-component `Calendar` plus a Clear action. The due date is a DATE with
//! no time-of-day component anywhere in the product (REV2-49 deleted the
//! `due_time`/`end_time` columns), so there is nothing to cascade on clear.

use crate::controls::PointerMenuButtonExt as _;
use std::rc::Rc;

use chrono::NaiveDate;

use gpui::{
    div, px, App, AppContext as _, ClipboardItem, Entity, IntoElement, ParentElement,
    SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    calendar::{CalendarEvent, CalendarState, Date},
    ActiveTheme as _, Icon, Sizable as _,
};
use sync::Store;

use domain::board::format_short_date;
use domain::issue_estimate::estimate_short_label;
use domain::options::get_issue_priority_config;
use domain::rows::{Issue, Label, Board, User};

use crate::coding_flow::StartCodingControl;
use crate::icons::{option_icon, registry, ExpIcon};
use crate::pickers::{chip_button, PICKER_MENU_MIN_WIDTH};
use crate::issue_detail::{issue_web_url, set_duplicate_of};
use crate::navigation::go_back;
use crate::queries;
use crate::surface::{glass_pill, PillMode, PillSize};

pub struct IssueHeader {
    issue_id: Option<String>,
    due_calendar: Entity<CalendarState>,
    // EXP-1021: the label and move-to-board pickers used to make this view
    // hold their query fields and their keyboard cursors. The shared picker
    // primitive owns both now, so the header holds no picker state at all —
    // only the due-date calendar, which is not one of the ten.
    /// EXP-1294: the detail view's ONE Start button, rendered in the right
    /// cluster (the entity stays owned by the detail view).
    start_coding: Entity<StartCodingControl>,
    _subscriptions: Vec<Subscription>,
}

impl IssueHeader {
    pub fn new(
        start_coding: Entity<StartCodingControl>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let due_calendar = cx.new(|cx| CalendarState::new(window, cx));
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
        // Re-render on every collection this header reads; keep the calendars
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
            cx.observe(&collections.team_members, |_, _, cx| cx.notify()),
            cx.observe(&collections.boards, |_, _, cx| cx.notify()),
            // EXP-1294: the tray's Threads row (roots linked through
            // `session_issues`) and the Close PR / related-work reads.
            cx.observe(&collections.sessions, |_, _, cx| cx.notify()),
            cx.observe(&collections.session_issues, |_, _, cx| cx.notify()),
            cx.observe(&collections.pull_requests, |_, _, cx| cx.notify()),
            cx.observe(&collections.pull_request_issues, |_, _, cx| cx.notify()),
            // EXP-314: a status rename/recolor re-renders the status control.
            cx.observe(&collections.issue_statuses, |_, _, cx| cx.notify()),
            // EXP-778: the pin toggle's glyph reads the per-user pins rows.
            cx.observe(&collections.pins, |_, _, cx| cx.notify()),
        ] {
            subscriptions.push(subscription);
        }
        Self {
            issue_id: None,
            due_calendar,
            start_coding,
            _subscriptions: subscriptions,
        }
    }

    /// Point the header at another issue.
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
    }

    // -- mutations -------------------------------------------------------------

    /// Web `onDueDateSelect`: set or clear the due date (date only — REV2-49
    /// deleted the time-of-day fields, so there is nothing left to cascade).
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
            }
        }
        spawn_issue_update(cx, input);
    }

    // -- derived reads ----------------------------------------------------------

    /// Team members eligible as assignees (web passes the team's
    /// member users; synthetic agent users are excluded). Resolves the
    /// issue's team, then delegates to the shared
    /// [`queries::team_users`] (EXP-50: one agent-excluding rule).
    fn member_users(&self, issue: &Issue, cx: &App) -> Vec<User> {
        let Some(board) = Store::global(cx)
            .collections()
            .boards
            .read(cx)
            .get(&issue.board_id)
            .cloned()
        else {
            return Vec::new();
        };
        queries::team_users(cx, &board.team_id)
    }

    /// #EXP-1383: the issue's team's agents (the assignee picker's rows after
    /// the members).
    fn team_agents(&self, issue: &Issue, cx: &App) -> Vec<domain::agent_settings::AgentActorRow> {
        let Some(team_id) = Store::global(cx)
            .collections()
            .boards
            .read(cx)
            .get(&issue.board_id)
            .map(|board| board.team_id.clone())
        else {
            return Vec::new();
        };
        queries::team_agents(cx, &team_id)
    }

    /// The team's labels, sort-order sorted (web LabelPicker query).
    fn team_labels(&self, issue: &Issue, cx: &App) -> Vec<Label> {
        let collections = Store::global(cx).collections();
        let Some(board) = collections.boards.read(cx).get(&issue.board_id).cloned()
        else {
            return Vec::new();
        };
        let mut labels: Vec<Label> = collections
            .labels
            .read(cx)
            .iter()
            .filter(|label| label.team_id == board.team_id)
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
        // EXP-314: the trigger renders the issue's RESOLVED status (custom
        // rows included), and the menu lists the team's own vocabulary.
        let resolved = crate::queries::resolve_issue_status(cx, issue);
        let current_key = resolved.group_key.clone();
        let team_id = self.team_id_of(issue, cx);
        let issue_id = issue.id.clone();
        let trigger = chip_button("prop-status", cx)
            .icon(crate::icons::resolved_status_icon(&resolved, cx))
            .child(crate::pickers::chip_label(resolved.name.clone(), false, cx))
            .into_any_element();
        // EXP-1021: THE status picker, through the shared primitive.
        crate::picker::deferred(move |window, cx| {
            let statuses = match &team_id {
                Some(team_id) => crate::queries::team_status_options(cx, team_id),
                None => domain::statuses::default_resolved_statuses(),
            };
            // L27: the duplicate row IS offered here (single-issue scope) —
            // `apply_status_selection` intercepts it into the canonical
            // picker, desktop's only path to marking a duplicate.
            let offered: Vec<domain::statuses::ResolvedStatus> = crate::pickers::status_menu_options(
                &statuses,
                crate::pickers::StatusMenuScope::SingleIssue,
            )
            .into_iter()
            .cloned()
            .collect();
            let picks = offered.clone();
            crate::picker::status_picker::status_picker(
                &offered,
                crate::picker::PickerMode::Single,
                vec![current_key.clone()],
                trigger,
                Rc::new(move |keys, window, cx| {
                    let Some(pick) = keys
                        .first()
                        .and_then(|key| picks.iter().find(|status| &status.group_key == key))
                        .map(crate::pickers::StatusPick::from_resolved)
                    else {
                        return;
                    };
                    crate::issue_detail::apply_status_selection(issue_id.clone(), pick, window, cx);
                }),
            )
            .id("prop-status-picker")
            .render(window, cx)
        })
    }

    /// The team behind an issue (via its board) — scopes the status menu.
    fn team_id_of(&self, issue: &Issue, cx: &gpui::App) -> Option<String> {
        Store::global(cx)
            .collections()
            .boards
            .read(cx)
            .get(&issue.board_id)
            .map(|board| board.team_id.clone())
    }

    fn priority_control(&self, issue: &Issue, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let config = get_issue_priority_config(issue.priority);
        let current = issue.priority;
        let issue_id = issue.id.clone();
        let trigger = chip_button("prop-priority", cx)
            .icon(option_icon(config, cx))
            .child(crate::pickers::chip_label(config.label, false, cx))
            .into_any_element();
        crate::picker::deferred(move |window, cx| {
            crate::picker::priority_picker::priority_picker(
                crate::picker::PickerMode::Single,
                vec![current],
                trigger,
                Rc::new(move |values, _window, cx| {
                    let Some(value) = values.first().copied() else {
                        return;
                    };
                    let mut input = api::issues::IssuesUpdateInput::new(issue_id.clone());
                    input.priority = Some(value);
                    spawn_issue_update(cx, input);
                }),
            )
            .id("prop-priority-picker")
            .render(window, cx)
        })
    }

    /// Web `AssigneePicker`: avatar + name when assigned, `User` glyph +
    /// "Assignee" otherwise; menu offers Unassign + every member.
    fn assignee_control(&self, issue: &Issue, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let users = self.member_users(issue, cx);
        let agents = self.team_agents(issue, cx);
        let selected = issue
            .assignee_id
            .as_deref()
            .and_then(|id| users.iter().find(|user| user.id == id))
            .cloned();
        let issue_id = issue.id.clone();
        let current = crate::picker::assignee_picker::assignee_value(
            issue.assignee_id.as_deref(),
            issue.assignee_agent_id.as_deref(),
        );
        // #EXP-1383: an agent assignee reads like a member: THE AGENT MARK +
        // its name (web `AssigneePicker` trigger).
        let selected_agent = issue
            .assignee_agent_id
            .as_deref()
            .and_then(|id| queries::agent_actor(cx, id));

        let trigger = match (selected_agent, issue.assignee_id.as_deref()) {
            (Some(agent), _) => chip_button("prop-assignee", cx)
                .child(crate::user_avatar::agent_avatar(
                    agent.icon.as_deref(),
                    agent.color.as_deref(),
                    gpui_component::Size::XSmall,
                    cx,
                ))
                .child(crate::pickers::chip_label(agent.name.clone(), false, cx)),
            // Assigned — render the member's name, falling back to `Member
            // <LAST4>` when the co-member's user row didn't sync.
            (None, Some(id)) => chip_button("prop-assignee", cx)
                .icon(
                    Icon::new(registry::UI_ASSIGNEE)
                        .xsmall()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(crate::pickers::chip_label(
                    crate::comments::user_label(id, selected.as_ref()),
                    false,
                    cx,
                )),
            (None, None) => chip_button("prop-assignee", cx)
                .icon(
                    Icon::new(registry::UI_UNASSIGNED)
                        .xsmall()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(crate::pickers::chip_label("Assignee", true, cx)),
        };

        let trigger = trigger.into_any_element();
        crate::picker::deferred(move |window, cx| {
            crate::picker::assignee_picker::assignee_picker(
                &users,
                &agents,
                crate::picker::PickerMode::Single,
                // Unassigned is a real ROW with a real value, so an
                // unassigned issue PICKS it — an empty set would leave the
                // picker with nothing marked at all.
                vec![current.clone()],
                // `Unassigned` is a real ROW, never a hidden placeholder.
                true,
                trigger,
                Rc::new(move |picked, _window, cx| {
                    // #EXP-1383: both columns, the cleared one null.
                    let assignee = crate::picker::assignee_picker::assignee_from_value(
                        picked.first().map(String::as_str),
                    );
                    let input = api::issues::IssuesUpdateInput::new(issue_id.clone()).assign(&assignee);
                    spawn_issue_update(cx, input);
                }),
            )
            .id("prop-assignee-picker")
            .render(window, cx)
        })
    }

    /// Web `LabelPicker`, EXP-282 as a SEARCHABLE popover (the board filter
    /// popover's `labels_view` pattern): "Filter labels..." input on top, live
    /// `contains()` filtering, checkbox + color-dot rows that toggle
    /// `issueLabels.add|remove` without closing, and the empty state. Label
    /// creation stays in team settings on desktop v1.
    fn labels_control(&self, issue: &Issue, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let labels = self.team_labels(issue, cx);
        let selected = self.selected_label_ids(&issue.id, cx);
        let issue_id = issue.id.clone();

        // ×4: the empty chip reads "Label" (the New issue page's too).
        let trigger_label = if selected.is_empty() {
            "Label".to_string()
        } else {
            let names: Vec<&str> = labels
                .iter()
                .filter(|label| selected.contains(&label.id))
                .map(|label| label.name.as_str())
                .collect();
            names.join(", ")
        };
        let trigger = chip_button("prop-labels", cx)
            .icon(
                Icon::from(ExpIcon::Tag)
                    .xsmall()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(crate::pickers::chip_label(
                trigger_label,
                selected.is_empty(),
                cx,
            ));

        let trigger = trigger.into_any_element();
        crate::picker::deferred(move |window, cx| {
            let before = selected.clone();
            crate::picker::label_picker::label_picker(
                &labels,
                selected.clone(),
                trigger,
                // The primitive reports the whole new SET; the write is still
                // one add/remove, so the ONE row that changed is the diff.
                Rc::new(move |next, _window, cx| {
                    let added = next.iter().find(|id| !before.contains(id));
                    let removed = before.iter().find(|id| !next.contains(id));
                    if let Some(label_id) = added {
                        toggle_label(cx, issue_id.clone(), label_id.clone(), false);
                    } else if let Some(label_id) = removed {
                        toggle_label(cx, issue_id.clone(), label_id.clone(), true);
                    }
                }),
            )
            .id("prop-labels-picker")
            .render(window, cx)
        })
    }

    /// The due-date control (web `DueDateControl`): a `CalendarDays` chip
    /// labeled with the formatted short date when set,
    /// or the literal "Due date" when empty (`triggerLabel = dueDate ?
    /// formatDate(dueDate) : 'Due date'`); popover = Calendar + Clear.
    fn due_control(&self, issue: &Issue, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let due = issue.due_date.clone();
        let label: SharedString = match due.as_deref() {
            Some(date) => format_short_date(date).into(),
            None => "Due date".into(),
        };
        let has_due = due.is_some();
        let trigger = chip_button("prop-due", cx)
            .icon(
                Icon::from(ExpIcon::CalendarDays)
                    .xsmall()
                    .text_color(crate::issue_list::due_glyph_color(due.as_deref(), cx)),
            )
            .child(crate::pickers::chip_label(label, !has_due, cx));

        let panel = cx.entity();
        // No width pin here (unlike the other pickers): the calendar grid has
        // its own intrinsic width. The Clear button rides as the shared
        // popover's `extra` row.
        let extra: Option<crate::pickers::DueExtra> = has_due.then(|| {
            Rc::new(move |_window: &mut Window, cx: &mut App| {
                let panel = panel.clone();
                // Stays LABELLED: it is the only thing under the calendar
                // (`pickers::due_date_popover`), so a bare ✕ circle would
                // read as "close the popover".
                Button::new("prop-due-clear")
                    .ghost()
                    .cursor_pointer()
                    .xsmall()
                    .label("Clear due date")
                    .text_color(cx.theme().muted_foreground)
                    .on_click(move |_, _, cx| {
                        panel.update(cx, |panel, cx| {
                            panel.commit_due_date(None, cx);
                        });
                    })
                    .into_any_element()
            }) as crate::pickers::DueExtra
        });
        crate::pickers::due_date_popover(
            "prop-due-popover",
            trigger,
            self.due_calendar.clone(),
            None,
            extra,
        )
    }

    /// EXP-630 (web `EstimateControl`): a gauge chip reading "Estimate"
    /// while unset, else the short label ("L" / "5 pt"); the menu offers
    /// "No estimate" first, then the team scale's ladder (plus an off-ladder
    /// current value) labelled in full. `None` — no chip at all — while the
    /// team's scale is `none`: estimates are OFF, not merely empty.
    fn estimate_control(
        &self,
        issue: &Issue,
        cx: &mut gpui::Context<Self>,
    ) -> Option<impl IntoElement> {
        let team_id = self.team_id_of(issue, cx)?;
        let scale = Store::global(cx)
            .collections()
            .teams
            .read(cx)
            .get(&team_id)
            .map(|team| team.estimation().to_string())?;
        if scale == domain::contract::ISSUE_ESTIMATION_NONE {
            return None;
        }
        let current = issue.estimate;
        let issue_id = issue.id.clone();
        let label: SharedString = match current {
            Some(value) => estimate_short_label(value, &scale).into(),
            None => "Estimate".into(),
        };
        let trigger = chip_button("prop-estimate", cx)
            .icon(
                Icon::new(registry::UI_ESTIMATE)
                    .xsmall()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(crate::pickers::chip_label(label, current.is_none(), cx));
        Some(trigger.pointer_menu(move |menu, _window, _cx| {
            crate::pickers::estimate_menu(
                menu.min_w(px(PICKER_MENU_MIN_WIDTH)),
                issue_id.clone(),
                current,
                &scale,
            )
        }))
    }

    /// Origin chip for widget-filed issues (web keys a "Feedback widget"
    /// origin off `issues.source`). Widget rows carry a null creator, so this
    /// is the only author/origin signal; renders NOTHING for `user`/None.
    fn origin_chip(&self, issue: &Issue, cx: &App) -> Option<impl IntoElement> {
        let (icon, label) = match issue.source.as_deref() {
            Some(domain::contract::ISSUE_SOURCE_WIDGET) => {
                (ExpIcon::MessageSquare, "Feedback widget")
            }
            // EXP-496: bug reports filed by a coding agent over MCP.
            Some(domain::contract::ISSUE_SOURCE_AGENT) => (registry::UI_AGENT_SOURCE, "Agent"),
            _ => return None,
        };
        Some(
            glass_pill("prop-origin", PillSize::Sm, PillMode::Readonly, cx)
                .child(Icon::from(icon).with_size(px(PillSize::Sm.glyph())))
                .child(SharedString::from(label)),
        )
    }

    // -- EXP-277: the former issue-detail header cluster ---------------------
    // (EXP-791 retired the EXP-48 prev/next switcher that opened it: the
    // issue's tab is the one place it lives, and the rail's lists step it.)

    /// EXP-760: the ONE `…` menu on the issue header (web parity). The
    /// copy-link and delete icon buttons that used to sit beside it are gone
    /// — three round buttons for three rarely-used actions crowded the row —
    /// so the menu is always rendered, not just for a duplicate.
    ///
    /// Items, in web order and with NO dividers (EXP-697): Copy link · Add
    /// relation ▸ · Unmark duplicate (only when it IS one) · Close PR per
    /// OPEN pull request (EXP-1154/EXP-1294) · Delete issue.
    fn render_actions_menu(
        &mut self,
        issue: &Issue,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let url = issue_web_url(issue, cx);
        let issue_id = issue.id.clone();
        let identifier = issue.identifier.clone();
        let is_duplicate = issue.duplicate_of_id.is_some();
        // EXP-1154: closing a PR without merging lives here. Every viewer of
        // an issue is a member (nothing is anonymously readable), so an open
        // PR is the whole gate. A PR closing several issues names how many
        // others it closes for (`pull_request_issues`).
        let close_prs = close_pr_items(&issue.id, cx);
        // EXP-862: the "..." is a GHOST glyph, never a circle.
        crate::controls::PointerMenu::for_button(crate::controls::ghost_icon_button("issue-actions", Icon::new(registry::UI_MORE), cx)
            .tooltip("Issue actions"),
            move |mut menu, window, cx| {
                {
                    let url = url.clone();
                    menu = menu.item(
                        crate::controls::pointer_label_item("Copy link", url.is_none())
                            .icon(Icon::from(ExpIcon::Link))
                            .on_click(move |_, _, cx| {
                                if let Some(url) = url.clone() {
                                    cx.write_to_clipboard(ClipboardItem::new_string(url));
                                }
                            }),
                    );
                }
                {
                    let issue_id = issue_id.clone();
                    menu = menu.submenu_with_icon(
                        Some(Icon::new(registry::RELATION_SECTION)),
                        "Add relation",
                        window,
                        cx,
                        move |menu, _, _| {
                            crate::issue_relations::add_relation_submenu(menu, &issue_id)
                        },
                    );
                }
                if is_duplicate {
                    let issue_id = issue_id.clone();
                    menu = menu.item(
                        crate::controls::pointer_label_item("Unmark duplicate", false)
                            .icon(Icon::new(registry::UI_UNDO))
                            .on_click(move |_, _, cx| {
                                set_duplicate_of(issue_id.clone(), None, cx);
                            }),
                    );
                }
                for item in close_prs.iter().cloned() {
                    menu = menu.item(
                        crate::controls::danger_menu_item(item.label.clone(), Icon::new(registry::PR_CLOSED), cx)
                            .on_click(move |_, window, cx| {
                                prompt_close_pr(
                                    item.pull_request_id.clone(),
                                    item.description.clone(),
                                    window,
                                    cx,
                                );
                            }),
                    );
                }
                let issue_id = issue_id.clone();
                let identifier = identifier.clone();
                menu.item(
                    crate::controls::danger_menu_item(
                        "Delete issue",
                        Icon::new(registry::UI_DELETE),
                        cx,
                    )
                    .on_click(move |_, window, cx| {
                        // The same alert the list row's Delete opens
                        // (EXP-697) — plus the tabbed analog of the web's
                        // back-navigation, since this view is about to point
                        // at a row that no longer exists.
                        crate::issue_list::prompt_issue_delete(
                            issue_id.clone(),
                            identifier.clone(),
                            Some(Box::new(|window, cx| go_back(window, cx))),
                            window,
                            cx,
                        );
                    }),
                )
            })
    }

    /// EXP-877/EXP-1294: the work header's right cluster — `[related-work
    /// badge?] [Start] [pin] [… menu]`. The Start button is the detail
    /// view's ONE `StartCodingControl` (it hides itself on a repo-less
    /// board).
    pub(crate) fn right_cluster(
        &mut self,
        issue: &Issue,
        cx: &mut gpui::Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let mut cluster = Vec::with_capacity(4);
        // EXP-897 §4: the ONE related-work badge.
        cluster.extend(self.pr_graph_badge(issue, cx));
        cluster.push(self.start_coding.clone().into_any_element());
        // EXP-778: the personal pin toggle — a pinned issue lands in the
        // rail's Pinned section. Needs the team (the board's) to address
        // the toggle; a not-yet-synced board hides it for a repaint.
        if let Some(team_id) = self.team_id_of(issue, cx) {
            cluster.push(
                crate::pins::pin_toggle_button(
                    "issue-pin",
                    team_id,
                    domain::contract::PIN_KIND_ISSUE,
                    issue.id.clone(),
                    cx,
                )
                .into_any_element(),
            );
        }
        // EXP-1191: the `…` glyph, not its hit box, ends on the content edge.
        cluster.push(
            div()
                .mr(px(-crate::work_header::GHOST_ICON_HANG))
                .child(self.render_actions_menu(issue, cx))
                .into_any_element(),
        );
        cluster
    }

    /// EXP-897 §4 / SLOP-16: the related-work badge, an icon button
    /// (`pr-stack` / `pr-batch` / `relation-blocked-by`, `+N`) opening the
    /// "Related work" dialog. Absent when there is nothing around this issue
    /// at all.
    fn pr_graph_badge(&self, issue: &Issue, cx: &mut gpui::Context<Self>) -> Option<gpui::AnyElement> {
        let spec = crate::pr_graph::issue_spec(issue, cx);
        crate::pr_graph::badge("issue-pr-graph", spec, cx)
    }

    /// EXP-417: the mobile-style chip row under the title — Status ·
    /// Priority · Assignee · Labels · Due date · Estimate · Board · Origin, property-ish
    /// chips first and the navigation-ish Board last. Wraps inside the
    /// detail view's `centered_column`, which supplies the definite width
    /// `flex_wrap` needs.
    ///
    /// EXP-1294: the tray trails no actions (Start rides the bar).
    pub(crate) fn chip_row(
        &mut self,
        issue: &Issue,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        // EXP-50: a team with exactly one human member has no assignment
        // choice — hide the assignee chip entirely (server-side default
        // assignment keeps the data correct). Multi-member (and the not-yet-
        // synced 0-member snapshot) keeps the picker.
        // #EXP-1383: a team agent is someone to assign to as well.
        let solo_team = crate::picker::assignee_picker::hides_assignee(
            self.member_users(issue, cx).len(),
            self.team_agents(issue, cx).len(),
            issue.assignee_agent_id.is_some(),
        );

        // EXP-568/EXP-601: everything lives in ONE tray (EXP-1170:
        // `work_header::property_tray`, shared with the New issue page).
        let mut chips = vec![
            self.status_control(issue, cx).into_any_element(),
            self.priority_control(issue, cx).into_any_element(),
        ];
        if !solo_team {
            chips.push(self.assignee_control(issue, cx).into_any_element());
        }
        chips.push(self.labels_control(issue, cx).into_any_element());
        chips.push(self.due_control(issue, cx).into_any_element());
        chips.extend(self.estimate_control(issue, cx).map(IntoElement::into_any_element));
        chips.extend(self.board_chip(issue, cx).map(IntoElement::into_any_element));
        chips.extend(self.origin_chip(issue, cx).map(IntoElement::into_any_element));
        crate::work_header::property_tray(chips, Vec::new())
    }

    /// The Board chip (EXP-282): the board's own glyph tinted with its color
    /// (the rail's `rail_board_icon` treatment — the anonymous color dot is
    /// gone) + its name. With another board in the team the chip becomes a
    /// PICKER over the shared move-to-board menu (the same `issues.move` the
    /// row context menu and the `…` actions menu already offer); a
    /// single-board team keeps a static glass chip.
    fn board_chip(&self, issue: &Issue, cx: &App) -> Option<impl IntoElement> {
        let board: Board = Store::global(cx)
            .collections()
            .boards
            .read(cx)
            .get(&issue.board_id)
            .cloned()?;
        let tint = board
            .color
            .as_deref()
            .and_then(parse_hex_color)
            .unwrap_or(cx.theme().muted_foreground);
        let icon = crate::icons::board_icon(&board).text_color(tint);
        let name = SharedString::from(board.name.clone());

        if crate::issue_list::move_target_boards(cx, &issue.board_id).is_empty() {
            return Some(
                glass_pill("prop-board", PillSize::Sm, PillMode::Readonly, cx)
                    .child(icon.with_size(px(PillSize::Sm.glyph())))
                    .child(crate::pickers::chip_label(name, false, cx))
                    .into_any_element(),
            );
        }

        // EXP-316: the web `BoardPicker` recipe — a searchable "Move to
        // board..." popover — replaced the plain dropdown menu.
        let issue_id = issue.id.clone();
        let identifier = issue.identifier.clone();
        let boards = crate::issue_list::move_target_boards(cx, &issue.board_id);
        let current_board_id = issue.board_id.clone();
        let trigger = chip_button("prop-board", cx)
            .icon(icon.xsmall())
            .child(crate::pickers::chip_label(name, false, cx))
            .into_any_element();
        Some(
            crate::picker::deferred(move |window, cx| {
                crate::picker::board_picker::board_picker(
                    &boards,
                    Some(current_board_id.clone()),
                    trigger,
                    // EXP-426: the pick confirms before moving — the canonical
                    // cross-client wording (web/iOS/Android share it). Picking
                    // the issue's OWN board is a no-op, as it always was.
                    Rc::new(move |picked, window, cx| {
                        let Some(board_id) = picked
                            .into_iter()
                            .next()
                            .filter(|board_id| board_id != &current_board_id)
                        else {
                            return;
                        };
                        let target_name = Store::global(cx)
                            .collections()
                            .boards
                            .read(cx)
                            .get(&board_id)
                            .map(|board| board.name.clone())
                            .unwrap_or_else(|| "that board".to_string());
                        crate::issue_list::confirm_issue_move(
                            window,
                            cx,
                            issue_id.clone(),
                            identifier.clone(),
                            board_id,
                            target_name,
                        );
                    }),
                )
                .id("prop-board-picker")
                .render(window, cx)
            })
            .into_any_element(),
        )
    }
}

// ---------------------------------------------------------------------------
// Pieces
// ---------------------------------------------------------------------------

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
/// by the issue header's controls, the detail actions and the title save.
pub(crate) fn spawn_issue_update(cx: &mut App, input: api::issues::IssuesUpdateInput) {
    spawn_issue_update_then(cx, input, |_, _| {});
}

/// EXP-919 — [`spawn_issue_update`] whose OUTCOME is handled: `on_done` runs
/// on the main thread with the write's result (`Err` carries the logged
/// message). A write that never lands must be able to retire the local
/// bookkeeping that was waiting for its Electric echo
/// (`issue_detail::UnechoedSaves`) — a fire-and-forget log line left the
/// editor pinned to text the server never took.
///
/// With no signed-in account nothing is SENT, so `on_done` never runs: the
/// text is the user's unsaved work, not a write the server refused, and
/// dropping it the moment a signed-out client rendered would lose it. Its
/// bookkeeping expires on the TTL instead.
pub(crate) fn spawn_issue_update_then(
    cx: &mut App,
    input: api::issues::IssuesUpdateInput,
    on_done: impl FnOnce(Result<(), String>, &mut App) + 'static,
) {
    let Some(trpc) = queries::trpc_client(cx) else {
        log::warn!("[ui] issues.update skipped: no signed-in account");
        return;
    };
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move {
                let id = input.id.clone();
                api::issues::issues_update(&trpc, &input)
                    .map(|_| ())
                    .map_err(|err| {
                        log::warn!("[ui] issues.update({id}) failed: {err}");
                        err.to_string()
                    })
            })
            .await;
        let _ = cx.update(|cx| on_done(result, cx));
    })
    .detach();
}

/// Web `formatDateForMutation`: `YYYY-MM-DD`.
fn format_mutation_date(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

/// `#rrggbb` (leading `#` optional) → Hsla (labels/boards store hex
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

/// EXP-1154 — the Close PR confirm's copy (fixture `close-pr.json`, ×4).
pub(crate) const CLOSE_PR_TITLE: &str = "Close pull request?";
pub(crate) const CLOSE_PR_BODY: &str = "Closes the pull request on GitHub without merging. Use this when the issue was dropped even though the work exists. The branch is kept and the PR can be reopened on GitHub.";
pub(crate) const CLOSE_PR_BATCH_LINE: &str = "It also closes the pull request for {n} linked issues.";
pub(crate) const CLOSE_PR_CONFIRM: &str = "Close PR";

/// EXP-1154 — the confirm's body: [`CLOSE_PR_BODY`], plus the batch line
/// when the PR links `others` more issues.
pub(crate) fn close_pr_description(others: usize) -> String {
    if others == 0 {
        return CLOSE_PR_BODY.to_string();
    }
    format!(
        "{CLOSE_PR_BODY} {}",
        CLOSE_PR_BATCH_LINE.replace("{n}", &others.to_string())
    )
}

/// One Close PR item of the `…` menu: an OPEN pull request of the issue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClosePrItem {
    pub(crate) pull_request_id: String,
    pub(crate) label: SharedString,
    pub(crate) description: String,
}

/// EXP-1294 — the issue's OPEN pull requests as Close PR items: the plain
/// contract label for one, `Close PR #N` once there are several; each
/// description names the other issues the PR also closes.
pub(crate) fn close_pr_items(issue_id: &str, cx: &App) -> Vec<ClosePrItem> {
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let links = store.collections().pull_request_issues.read(cx);
    let open: Vec<_> = crate::session_data::pull_requests_of_issue(issue_id, cx)
        .into_iter()
        .filter(|pull| pull.is_open())
        .collect();
    let several = open.len() > 1;
    open.into_iter()
        .map(|pull| {
            let others = links
                .iter()
                .filter(|link| link.pull_request_id == pull.id && link.issue_id != issue_id)
                .count();
            let label = match pull.number {
                Some(number) if several => format!("{} #{number}", domain::contract::DIFF_UI_CLOSE_PR),
                _ => domain::contract::DIFF_UI_CLOSE_PR.to_string(),
            };
            ClosePrItem {
                pull_request_id: pull.id,
                label: label.into(),
                description: close_pr_description(others),
            }
        })
        .collect()
}

/// EXP-1154 — confirm, then close the pull request without merging through
/// the shared [`crate::pr_merge`] machinery (a refusal toasts, like a failed
/// merge).
pub(crate) fn prompt_close_pr(pull_request_id: String, description: String, window: &mut Window, cx: &mut App) {
    let tall = description.len() > CLOSE_PR_BODY.len();
    let spec = crate::native_dialog::AlertSpec::new(CLOSE_PR_TITLE, description, CLOSE_PR_CONFIRM)
        .ok_variant(gpui_component::button::ButtonVariant::Danger)
        .height(gpui::px(if tall { 280. } else { 250. }))
        .on_ok(move |_, cx| {
            crate::pr_merge::fire_confirmed(
                crate::pr_merge::MergeOp::Close {
                    pull_request_id: pull_request_id.clone(),
                },
                cx,
            );
            true
        });
    crate::native_dialog::open_alert(window, cx, spec);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-1154: the Close PR copy is the shared `close-pr.json` (×4), the
    /// menu item the contract's `diffUi.closePr`, and the batch line names
    /// the OTHER linked issues.
    #[test]
    fn close_pr_copy_matches_the_shared_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/close-pr.json"
        ))
        .unwrap();
        assert_eq!(fixture["menuItem"], domain::contract::DIFF_UI_CLOSE_PR);
        assert_eq!(fixture["title"], CLOSE_PR_TITLE);
        assert_eq!(fixture["body"], CLOSE_PR_BODY);
        assert_eq!(fixture["batchLine"], CLOSE_PR_BATCH_LINE);
        assert_eq!(fixture["confirm"], CLOSE_PR_CONFIRM);
        assert_eq!(close_pr_description(0), CLOSE_PR_BODY);
        assert_eq!(
            close_pr_description(2),
            format!("{CLOSE_PR_BODY} It also closes the pull request for 2 linked issues.")
        );
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
