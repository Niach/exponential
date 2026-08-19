//! The issue-detail HEADER state (EXP-417 — was the right properties
//! sidebar): everything above the scrolling body except the title input.
//!
//! It is an entity for its state (calendars, picker queries, busy flags, the
//! ~12 collection subscriptions) but NOT a view — the detail view interleaves
//! its rows with the title block it owns itself, so this renders through
//! three builders called from the host's render: [`IssueHeader::top_row`]
//! (switcher · copy-link · subscribe · `…`), [`IssueHeader::chip_row`]
//! (Status · Priority · Assignee · Labels · Due date · Board · Origin) and
//! [`IssueHeader::agent_row`] (coding-now pill · Start coding · Merge PR ·
//! Fix conflicts). The host observes this entity so a builder's `cx.notify()`
//! reaches it.
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

use std::rc::Rc;

use chrono::NaiveDate;

use gpui::{
    div, px, App, AppContext as _, ClipboardItem, Entity, IntoElement, ParentElement,
    SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    calendar::{CalendarEvent, CalendarState, Date},
    h_flex,
    input::InputState,
    menu::{DropdownMenu as _, PopupMenuItem},
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};
use serde::Serialize;
use sync::Store;

use domain::board::format_short_date;
use domain::options::get_issue_priority_config;
use domain::rows::{Issue, Label, Board, User};

use crate::controls::WebControl as _;
use crate::coding_flow::{LocalSessions, StartCodingControl};
use crate::icons::{option_icon, registry, ExpIcon};
use crate::pickers::{chip_button, PICKER_MENU_MIN_WIDTH, PICKER_SEARCH_WIDTH};
use crate::issue_detail::{is_subscribed, issue_web_url, set_duplicate_of, DETAIL_GUTTER};
use crate::issue_list::IssueQuery;
use crate::navigation::{go_back, replace_screen, Screen};
use crate::queries;
use crate::surface::glass_chip;

/// EXP-48 switcher position: where the displayed issue sits in the active
/// issue list's flattened visible ordering. (Moved here with the toolbar
/// cluster — EXP-277.)
struct SwitcherState {
    /// 0-based index in the flattened list.
    position: usize,
    total: usize,
    prev_id: Option<String>,
    next_id: Option<String>,
}

pub struct IssueHeader {
    issue_id: Option<String>,
    due_calendar: Entity<CalendarState>,
    /// Search query of the Labels popover (EXP-282 — the searchable picker
    /// follows `filter_popover::labels_view`: the OWNING view holds the
    /// `InputState`, the popover only renders it).
    label_query: Entity<InputState>,
    /// Search query of the move-to-board popover (EXP-316 — web
    /// `BoardPicker` parity, same host-owned-InputState recipe as labels).
    board_query: Entity<InputState>,
    /// The window's shared rail state — the EXP-48 switcher reads the active
    /// issue board's query + filters from it (EXP-277: the switcher lives in
    /// this header's top row now).
    rail_shared: Entity<crate::sidebar::RailShared>,
    /// The owning window (EXP-426) — resolves the screens panel so the
    /// switcher can follow the ACTIVE TAB's remembered origin list.
    window_id: gpui::WindowId,
    /// Subscribe-toggle in-flight flag (web `busy`).
    subscribe_busy: bool,
    /// Copy-link feedback: the toolbar button shows a check for ~1.5s after a
    /// copy (web `linkCopied`). The seq guards the disarm timer against a
    /// re-click racing an older timer (the merge-confirm pattern).
    link_copied: bool,
    link_copied_seq: u64,
    /// The detail view's Start-coding control, rendered here as the "Agent"
    /// group (EXP-256, web parity — the entity stays owned by the detail
    /// view, which also reads its `resolved_repo` for the actions menu).
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
        let label_query =
            cx.new(|cx| InputState::new(window, cx).placeholder("Filter labels..."));
        let board_query =
            cx.new(|cx| InputState::new(window, cx).placeholder("Move to board..."));

        let mut subscriptions = Vec::new();
        // Live label search re-filters the popover's rows (EXP-282).
        subscriptions.push(cx.observe(&label_query, |_, _, cx| cx.notify()));
        // Live board search re-filters the move-to-board popover (EXP-316).
        subscriptions.push(cx.observe(&board_query, |_, _, cx| cx.notify()));
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
        // The Agent group's coding-now pill follows the synced sessions; its
        // skip-while-local guard follows the local registry. The toolbar's
        // subscribe toggle follows the issue_subscribers shape (EXP-277).
        let local_sessions = LocalSessions::global(cx);
        let merge_state = crate::pr_merge::MergeState::global(cx);
        for subscription in [
            cx.observe(&collections.labels, |_, _, cx| cx.notify()),
            cx.observe(&collections.issue_labels, |_, _, cx| cx.notify()),
            cx.observe(&collections.users, |_, _, cx| cx.notify()),
            cx.observe(&collections.team_members, |_, _, cx| cx.notify()),
            cx.observe(&collections.boards, |_, _, cx| cx.notify()),
            cx.observe(&collections.coding_sessions, |_, _, cx| cx.notify()),
            // EXP-549/550: the pill's machine name + paused state come from
            // the devices rows — heartbeats and renames re-render it.
            cx.observe(&collections.devices, |_, _, cx| cx.notify()),
            cx.observe(&collections.issue_subscribers, |_, _, cx| cx.notify()),
            // EXP-314: a status rename/recolor re-renders the status control.
            cx.observe(&collections.issue_statuses, |_, _, cx| cx.notify()),
            cx.observe(&local_sessions, |_, _, cx| cx.notify()),
            // EXP-325: the Merge button's arm/spinner/error live in the
            // shared app-global merge state (any surface can drive them).
            cx.observe(&merge_state, |_, _, cx| cx.notify()),
        ] {
            subscriptions.push(subscription);
        }
        // EXP-48 switcher (EXP-277: now in this header's top row): the counter
        // follows the ACTIVE issue list — tool swaps notify the shared rail
        // state, filter changes notify the boards (issue reorders already
        // ride the issues observer above).
        let rail_shared = crate::sidebar::rail_shared_for_window(window, cx);
        subscriptions.push(cx.observe(&rail_shared, |_, _, cx| cx.notify()));
        let boards = rail_shared.read(cx).issue_boards().map(Clone::clone);
        for board in boards {
            subscriptions.push(cx.observe(&board, |_, _, cx| cx.notify()));
        }

        Self {
            issue_id: None,
            due_calendar,
            label_query,
            board_query,
            rail_shared,
            subscribe_busy: false,
            link_copied: false,
            link_copied_seq: 0,
            start_coding,
            window_id: window.window_handle().window_id(),
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
        // Toolbar state belongs to the PREVIOUS issue (EXP-277).
        self.subscribe_busy = false;
        self.link_copied = false;
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
            .child(crate::pickers::chip_label(resolved.name.clone(), false, cx));
        trigger.dropdown_menu(move |menu, _, cx| {
            let issue_id = issue_id.clone();
            let statuses = match &team_id {
                Some(team_id) => crate::queries::team_status_options(cx, team_id),
                None => domain::statuses::default_resolved_statuses(),
            };
            crate::pickers::status_menu(
                menu.min_w(px(PICKER_MENU_MIN_WIDTH)),
                &statuses,
                &current_key,
                // L27: a duplicate-category pick opens the picker; every other
                // status writes.
                crate::pickers::StatusMenuScope::SingleIssue,
                Rc::new(move |pick, window, cx| {
                    crate::issue_detail::apply_status_selection(
                        issue_id.clone(),
                        pick,
                        window,
                        cx,
                    );
                }),
                cx,
            )
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
            .child(crate::pickers::chip_label(config.label, false, cx));
        trigger.dropdown_menu(move |menu, _, cx| {
            let issue_id = issue_id.clone();
            crate::pickers::priority_menu(
                menu.min_w(px(PICKER_MENU_MIN_WIDTH)),
                current,
                Rc::new(move |value, _window, cx| {
                    let mut input = api::issues::IssuesUpdateInput::new(issue_id.clone());
                    input.priority = Some(value);
                    spawn_issue_update(cx, input);
                }),
                cx,
            )
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

        let trigger = match issue.assignee_id.as_deref() {
            // Assigned — render the member's name, falling back to `Member
            // <LAST4>` when the co-member's user row didn't sync.
            Some(id) => chip_button("prop-assignee", cx)
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
            None => chip_button("prop-assignee", cx)
                .icon(
                    Icon::new(registry::UI_UNASSIGNED)
                        .xsmall()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(crate::pickers::chip_label("Assignee", true, cx)),
        };

        trigger.dropdown_menu(move |menu, _, _| {
            let issue_id = issue_id.clone();
            crate::pickers::assignee_menu(
                menu.min_w(px(PICKER_MENU_MIN_WIDTH)),
                &users,
                current_id.as_deref(),
                Rc::new(move |picked, _window, cx| {
                    let mut input = api::issues::IssuesUpdateInput::new(issue_id.clone());
                    input.assignee_id = match picked {
                        Some(user_id) => api::Patch::Set(user_id),
                        None => api::Patch::Null,
                    };
                    spawn_issue_update(cx, input);
                }),
            )
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

        crate::pickers::label_picker_popover(
            "prop-labels-popover",
            trigger,
            crate::pickers::LabelPickerParams {
                labels,
                selected_ids: selected,
                query: self.label_query.clone(),
                on_toggle: Rc::new(move |label_id, was_selected, _window, cx| {
                    toggle_label(cx, issue_id.clone(), label_id.to_string(), was_selected);
                }),
                width: Some(px(PICKER_SEARCH_WIDTH)),
            },
        )
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
                    .text_color(cx.theme().muted_foreground),
            )
            .child(crate::pickers::chip_label(label, !has_due, cx));

        let panel = cx.entity();
        // No width pin here (unlike the other pickers): the calendar grid has
        // its own intrinsic width. The Clear button rides as the shared
        // popover's `extra` row.
        let extra: Option<crate::pickers::DueExtra> = has_due.then(|| {
            Rc::new(move |_window: &mut Window, cx: &mut App| {
                let panel = panel.clone();
                Button::new("prop-due-clear")
                    .ghost().cursor_pointer()
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
            glass_chip()
                .child(
                    Icon::from(icon)
                        .xsmall()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(SharedString::from(label)),
        )
    }

    /// The agent row (EXP-256/EXP-417, web `issue-coding-rows.tsx`): the
    /// synced coding-now pill on its OWN full-width line — its EXP-309
    /// ellipsis chain needs one — above a wrapping row of the Merge button
    /// while the linked PR is open (EXP-268), the merge error and the
    /// fix-conflicts offer. The Start-coding control itself moved into the
    /// chip row (EXP-426). The pill is skipped while a LOCAL session runs —
    /// the control already shows the live indicator, and the synced pill
    /// would double it as soon as the Electric echo lands.
    ///
    /// `None` when the board has no repository, and when there is neither a
    /// pill nor an open PR — an empty row would only add padding.
    pub(crate) fn agent_row(
        &mut self,
        issue: &Issue,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if !self.start_coding.read(cx).is_visible(cx) {
            return None;
        }
        let local_running = LocalSessions::global_ref(cx)
            .map(|sessions| sessions.read(cx).get(&issue.id).is_some())
            .unwrap_or(false);
        let pill = (!local_running)
            .then(|| crate::issue_detail::coding_now_pill(&issue.id, cx))
            .flatten();
        let pr_open = issue.pr_state.as_deref() == Some("open");
        if pill.is_none() && !pr_open {
            return None;
        }
        let mut column = v_flex().w_full().gap_2().px(px(DETAIL_GUTTER)).pb_2();
        if let Some(pill) = pill {
            column = column.child(pill);
        }

        let mut controls = h_flex().w_full().flex_wrap().gap_2().items_center();
        if pr_open {
            controls = controls.child(self.merge_button(issue, cx));
            let (error, failed_op) = {
                let state = crate::pr_merge::MergeState::global(cx);
                let state = state.read(cx);
                (state.error(&issue.id), state.failed_op(&issue.id))
            };
            if let Some(error) = error {
                // EXP-313: a failed merge (typically conflicts) offers the
                // builtin fix run right here — the Reviews-rail affordance,
                // routed through the Start-coding dialog with this PR
                // preselected. MERGE failures only: the run ends in a merge,
                // so a failed CLOSE (captioned on this same row from the
                // Reviews rail) must never offer it. Needs the PR's recorded
                // branch (the run rebases it); parks only while a fix run
                // already works it.
                if failed_op == Some(crate::pr_merge::FailedOp::Merge) && issue.branch.is_some() {
                    controls = controls.child(self.fix_conflicts_button(issue, cx));
                }
                controls = controls.child(
                    div()
                        .min_w_0()
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(error),
                );
            }
        }
        if pr_open {
            column = column.child(controls);
        }
        Some(column.into_any_element())
    }

    /// The "Fix conflicts" button (EXP-313): opens the Start-coding dialog
    /// with the fix-conflicts builtin and this issue's PR preselected.
    fn fix_conflicts_button(&self, issue: &Issue, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // "Fixing…" only while an ACTUAL fix run works the branch — any other
        // session still holding it is ended by the fix-run launch itself.
        let fixing = issue.branch.as_deref().is_some_and(|branch| {
            LocalSessions::global_ref(cx)
                .is_some_and(|sessions| sessions.read(cx).is_branch_fixing(branch))
        });
        let issue_id = issue.id.clone();
        let board_id = issue.board_id.clone();
        // EXP-367: no agent CLI → disabled with the reason, never hidden.
        let no_agent = crate::coding_flow::no_agent_reason(cx);
        let mut button = Button::new("header-fix-conflicts")
            .outline().cursor_pointer()
            .web_sm()
            .icon(Icon::from(ExpIcon::GitBranch).text_color(cx.theme().muted_foreground))
            .label(if fixing { "Fixing…" } else { "Fix conflicts" })
            .tooltip(
                no_agent
                    .clone()
                    .unwrap_or_else(|| "Run the fix-conflicts action on this pull request".into()),
            )
            .on_click(cx.listener(move |_, _, window, cx| {
                let Some(team_id) = Store::global(cx)
                    .collections()
                    .boards
                    .read(cx)
                    .get(&board_id)
                    .map(|board| board.team_id.clone())
                else {
                    return;
                };
                crate::start_coding_dialog::open_for_fix_conflicts(
                    window,
                    cx,
                    team_id,
                    issue_id.clone(),
                );
            }));
        if fixing || no_agent.is_some() {
            button = button.disabled(true);
        }
        button
    }

    /// The header Merge button (EXP-268): two-click arm ("Merge" →
    /// "Confirm merge", auto-disarm ~5s — the reviews-rail pattern), then
    /// `issues.mergePr` on the background executor. The spinner is held
    /// until the Electric echo flips `pr_state` away from `open` (which
    /// also drops the whole button). Merge always closes (EXP-498): the
    /// server ends every linked live coding session on merge.
    fn merge_button(&self, issue: &Issue, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let merge_state = crate::pr_merge::MergeState::global(cx);
        let armed = merge_state.read(cx).armed(&issue.id);
        let merging = merge_state.read(cx).merging(&issue.id);
        let issue_id = issue.id.clone();
        let mut button = Button::new("header-merge-pr")
            .outline().cursor_pointer()
            .web_sm()
            .icon(Icon::from(ExpIcon::GitMerge).text_color(if armed {
                cx.theme().danger
            } else {
                cx.theme().muted_foreground
            }))
            .label(if merging {
                "Merging…"
            } else if armed {
                "Confirm merge"
            } else {
                "Merge PR"
            })
            .tooltip("Merge the pull request: completes every linked issue and closes its coding sessions")
            .on_click(cx.listener(move |_, _, _, cx| {
                crate::pr_merge::two_click(
                    crate::pr_merge::MergeOp::MergeIssuePr {
                        issue_id: issue_id.clone(),
                    },
                    None,
                    None,
                    cx,
                );
            }));
        if merging {
            button = button.disabled(true);
        }
        button
    }

    // -- EXP-277: the former issue-detail header cluster ---------------------

    fn toggle_subscription(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.subscribe_busy {
            return;
        }
        let Some(issue_id) = self.issue_id.clone() else {
            return;
        };
        let Some(account) = queries::active_account(cx) else {
            return;
        };
        let subscribed = is_subscribed(&issue_id, &account.user_id, cx);
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        self.subscribe_busy = true;
        cx.notify();

        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    #[derive(Serialize)]
                    #[serde(rename_all = "camelCase")]
                    struct SubscriptionInput<'a> {
                        issue_id: &'a str,
                    }
                    let path = if subscribed {
                        "subscriptions.unsubscribe"
                    } else {
                        "subscriptions.subscribe"
                    };
                    let out: Result<api::labels::TxOutput, api::ApiError> =
                        trpc.mutation(path, &SubscriptionInput { issue_id: &issue_id });
                    out
                })
                .await;
            let _ = this.update_in(cx, |this, _, cx| {
                this.subscribe_busy = false;
                if let Err(err) = result {
                    log::warn!("[ui] subscription toggle failed: {err}");
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The (query, filters) pair the switcher steps through (EXP-426): the
    /// ACTIVE TAB's remembered origin first — an issue opened from My Issues
    /// keeps stepping My Issues even after the rail moved elsewhere — with
    /// the rail's current list as the fallback (undocked windows, tabs with
    /// no usable origin, notification-opened issues).
    fn switcher_scope(&self, cx: &App) -> (IssueQuery, domain::IssueFilters) {
        use crate::sidebar::{InboxTab, ToolWindow};
        let shared = self.rail_shared.read(cx);
        let origin = crate::screens::screens_for_window_id(self.window_id, cx)
            .and_then(|panel| panel.read(cx).active_tab_origin(cx));
        if let Some(origin) = origin {
            match (origin.tool, origin.inbox_tab, origin.board_id) {
                (ToolWindow::Inbox, Some(InboxTab::MyIssues), _) => {
                    let board = shared.board_my().read(cx);
                    return (board.query().clone(), board.filters().clone());
                }
                (ToolWindow::BoardIssues, _, Some(board_id)) => {
                    let board = shared.board_active().read(cx);
                    let query = IssueQuery::Board { board_id };
                    // The live filters carry over only while the active list
                    // still IS this board; a re-pointed rail leaves the
                    // origin board unfiltered.
                    let filters = if board.query() == &query {
                        board.filters().clone()
                    } else {
                        domain::IssueFilters::empty()
                    };
                    return (query, filters);
                }
                _ => {}
            }
        }
        let board = shared.active_issue_board().read(cx);
        (board.query().clone(), board.filters().clone())
    }

    /// Where this issue sits in the origin issue list's flattened visible
    /// ordering (see [`Self::switcher_scope`]) — same grouping, same EXP-38
    /// comparator, same filters the list applies. `None` (hide the switcher)
    /// when no list scope resolves or the issue isn't in the filtered list.
    fn switcher_state(&self, issue: &Issue, cx: &App) -> Option<SwitcherState> {
        let (query, filters) = self.switcher_scope(cx);
        let data = match &query {
            IssueQuery::None => return None,
            IssueQuery::Board { board_id } => {
                queries::board_board(cx, board_id, &filters)
            }
            IssueQuery::MyIssues {
                team_id,
                user_id,
            } => queries::my_issues(cx, team_id, user_id, &filters),
        };
        let ids = data.flatten_issue_ids();
        let position = ids.iter().position(|id| *id == issue.id)?;
        Some(SwitcherState {
            position,
            total: ids.len(),
            prev_id: position.checked_sub(1).map(|ix| ids[ix].clone()),
            next_id: ids.get(position + 1).cloned(),
        })
    }

    /// Swap the displayed issue in place: `+1` = next in list order, `-1` =
    /// previous. No wrap at the ends; a no-op when the current issue isn't
    /// in the filtered list (matching the hidden switcher).
    fn step_issue(&mut self, delta: i32, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(issue) = self.issue(cx) else {
            return;
        };
        let Some(state) = self.switcher_state(&issue, cx) else {
            return;
        };
        let target = if delta < 0 { state.prev_id } else { state.next_id };
        if let Some(issue_id) = target {
            replace_screen(window, cx, Screen::IssueDetail { issue_id });
        }
    }

    /// The "N / total" counter + up/down chevrons. `None` hides the segment.
    fn render_switcher(
        &mut self,
        issue: &Issue,
        cx: &mut gpui::Context<Self>,
    ) -> Option<impl IntoElement> {
        let state = self.switcher_state(issue, cx)?;
        Some(
            h_flex()
                .flex_shrink_0()
                .gap_0p5()
                .items_center()
                .text_xs()
                .child(
                    div()
                        .whitespace_nowrap()
                        .text_color(cx.theme().muted_foreground)
                        .child(SharedString::from(format!(
                            "{} / {}",
                            state.position + 1,
                            state.total
                        ))),
                )
                .child(
                    Button::new("issue-switch-prev")
                        .ghost().cursor_pointer()
                        .xsmall()
                        .icon(
                            Icon::new(registry::UI_CHEVRON_UP)
                                .text_color(cx.theme().muted_foreground),
                        )
                        .disabled(state.prev_id.is_none())
                        .tooltip("Previous issue")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.step_issue(-1, window, cx)
                        })),
                )
                .child(
                    Button::new("issue-switch-next")
                        .ghost().cursor_pointer()
                        .xsmall()
                        .icon(
                            Icon::new(registry::UI_CHEVRON_DOWN)
                                .text_color(cx.theme().muted_foreground),
                        )
                        .disabled(state.next_id.is_none())
                        .tooltip("Next issue")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.step_issue(1, window, cx)
                        })),
                ),
        )
    }

    /// Web `Copy link to issue`: a Link icon that copies the full web URL and
    /// flips to a check for ~1.5s.
    fn render_copy_link(&mut self, issue: &Issue, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let url = issue_web_url(issue, cx);
        let icon = if self.link_copied {
            Icon::from(ExpIcon::Check).text_color(cx.theme().primary)
        } else {
            Icon::from(ExpIcon::Link).text_color(cx.theme().muted_foreground)
        };
        Button::new("copy-issue-link")
            .ghost().cursor_pointer()
            .xsmall()
            .icon(icon)
            .disabled(url.is_none())
            .tooltip(if self.link_copied {
                "Link copied"
            } else {
                "Copy link to issue"
            })
            .on_click(cx.listener(move |this, _, _window, cx| {
                let Some(url) = url.clone() else { return };
                cx.write_to_clipboard(ClipboardItem::new_string(url));
                this.link_copied = true;
                this.link_copied_seq += 1;
                let seq = this.link_copied_seq;
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(1500))
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        if this.link_copied_seq == seq && this.link_copied {
                            this.link_copied = false;
                            cx.notify();
                        }
                    });
                })
                .detach();
                cx.notify();
            }))
    }

    /// Web `SubscribeToggle`, icon-only (Bell/BellOff + tooltip), live off
    /// the `issue_subscribers` shape.
    fn render_subscribe_toggle(
        &mut self,
        issue: &Issue,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let account = queries::active_account(cx);
        let subscribed = account
            .as_ref()
            .map(|account| is_subscribed(&issue.id, &account.user_id, cx))
            .unwrap_or(false);
        let icon = if subscribed { ExpIcon::Bell } else { ExpIcon::BellOff };
        let tint = if subscribed {
            cx.theme().primary
        } else {
            cx.theme().muted_foreground
        };
        Button::new("subscribe-toggle")
            .ghost().cursor_pointer()
            .xsmall()
            .icon(Icon::from(icon).text_color(tint))
            .disabled(self.subscribe_busy || account.is_none())
            .tooltip(if subscribed {
                "Subscribed. Click to unsubscribe."
            } else {
                "Subscribe to this issue"
            })
            .on_click(cx.listener(|this, _, window, cx| this.toggle_subscription(window, cx)))
    }

    /// The `…` actions menu, now duplicate-only (EXP-426): Move-to-board
    /// lives on the header's Board chip and Delete became the visible trash
    /// button, so the menu renders only when it still has content — the
    /// Unmark-duplicate entry.
    fn render_actions_menu(
        &mut self,
        issue: &Issue,
        cx: &mut gpui::Context<Self>,
    ) -> Option<impl IntoElement> {
        if issue.duplicate_of_id.is_none() {
            return None;
        }
        let issue_id = issue.id.clone();
        Some(
            Button::new("issue-actions")
                .ghost().cursor_pointer()
                .xsmall()
                .icon(Icon::new(registry::UI_MORE).text_color(cx.theme().muted_foreground))
                .dropdown_menu(move |menu, _window, _cx| {
                    let issue_id = issue_id.clone();
                    menu.item(
                        PopupMenuItem::new("Unmark duplicate")
                            .icon(Icon::new(registry::UI_UNDO))
                            .on_click(move |_, _, cx| {
                                set_duplicate_of(issue_id.clone(), None, cx);
                            }),
                    )
                }),
        )
    }

    /// The visible delete trigger (EXP-426): the trash icon opens the same
    /// two-step "Confirm delete" popup the row context menu uses — no modal.
    /// After the delete fires, the tabbed analog of the web's back-navigation
    /// is popping the back stack.
    fn render_delete_button(
        &mut self,
        issue: &Issue,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let issue_id = issue.id.clone();
        Button::new("issue-delete")
            .ghost().cursor_pointer()
            .xsmall()
            .icon(Icon::new(registry::UI_DELETE).text_color(cx.theme().muted_foreground))
            .tooltip("Delete issue")
            .dropdown_menu(move |menu, _window, _cx| {
                let issue_id = issue_id.clone();
                menu.item(
                    PopupMenuItem::new("Confirm delete")
                        .icon(Icon::new(registry::UI_DELETE))
                        .on_click(move |_, window, cx| {
                            crate::issue_list::spawn_issue_delete(cx, issue_id.clone());
                            go_back(window, cx);
                        }),
                )
            })
    }

    /// EXP-277/EXP-417: the header's top row — switcher left, copy-link ·
    /// subscribe · unmark-duplicate (when applicable) · delete right.
    pub(crate) fn top_row(
        &mut self,
        issue: &Issue,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        h_flex()
            .w_full()
            .gap_0p5()
            .items_center()
            .min_w_0()
            .px(px(DETAIL_GUTTER))
            .pt_2()
            .children(self.render_switcher(issue, cx))
            .child(div().flex_1().min_w_0())
            .child(self.render_copy_link(issue, cx))
            .child(self.render_subscribe_toggle(issue, cx))
            .children(self.render_actions_menu(issue, cx))
            .child(self.render_delete_button(issue, cx))
            .into_any_element()
    }

    /// EXP-417: the mobile-style chip row under the title — Start coding ·
    /// Status · Priority · Assignee · Labels · Due date · Board · Origin,
    /// the launcher first (EXP-426), property-ish chips next and the
    /// navigation-ish Board last. Wraps inside the detail view's
    /// `centered_column`, which supplies the definite width `flex_wrap` needs.
    pub(crate) fn chip_row(
        &mut self,
        issue: &Issue,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        // EXP-50: a team with exactly one human member has no assignment
        // choice — hide the assignee chip entirely (server-side default
        // assignment keeps the data correct). Multi-member (and the not-yet-
        // synced 0-member snapshot) keeps the picker.
        let solo_team = self.member_users(issue, cx).len() == 1;
        // Gated: the control renders an empty div when hidden (no repo),
        // which would still occupy a gap slot in the row.
        let start_coding = self.start_coding.read(cx).is_visible(cx);

        h_flex()
            .w_full()
            .flex_wrap()
            .gap_1()
            .items_center()
            .px(px(DETAIL_GUTTER))
            .pb_1()
            .when(start_coding, |row| row.child(self.start_coding.clone()))
            .child(self.status_control(issue, cx))
            .child(self.priority_control(issue, cx))
            .when(!solo_team, |row| {
                row.child(self.assignee_control(issue, cx))
            })
            .child(self.labels_control(issue, cx))
            .child(self.due_control(issue, cx))
            .children(self.board_chip(issue, cx))
            .children(self.origin_chip(issue, cx))
            .into_any_element()
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
                glass_chip()
                    .child(icon.xsmall())
                    .child(crate::pickers::chip_label(name, false, cx))
                    .into_any_element(),
            );
        }

        // EXP-316: the web `BoardPicker` recipe — a searchable "Move to
        // board..." popover — replaced the plain dropdown menu.
        let issue_id = issue.id.clone();
        let identifier = issue.identifier.clone();
        let boards = crate::issue_list::move_target_boards(cx, &issue.board_id);
        Some(
            crate::pickers::board_picker_popover(
                "prop-board-popover",
                chip_button("prop-board", cx)
                    .icon(icon.xsmall())
                    .child(crate::pickers::chip_label(name, false, cx)),
                crate::pickers::BoardPickerParams {
                    boards,
                    current_board_id: issue.board_id.clone(),
                    query: self.board_query.clone(),
                    // EXP-426: the pick confirms before moving — the canonical
                    // cross-client wording (web/iOS/Android share it).
                    on_pick: Rc::new(move |board_id: String, window, cx| {
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
                    width: Some(px(PICKER_SEARCH_WIDTH)),
                },
            )
            .into_any_element(),
        )
    }
}

use gpui::prelude::FluentBuilder as _;

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

#[cfg(test)]
mod tests {
    use super::*;

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
