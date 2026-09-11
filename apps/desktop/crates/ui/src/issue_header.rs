//! The issue-detail HEADER state (EXP-417 — was the right properties
//! sidebar): everything above the scrolling body except the title input.
//!
//! It is an entity for its state (calendars, picker queries, busy flags, the
//! ~12 collection subscriptions) but NOT a view — the detail view interleaves
//! its rows with the title block it owns itself, so this renders through
//! three builders called from the host's render: [`IssueHeader::top_row`]
//! (switcher · copy-link · `…` · delete), [`IssueHeader::chip_row`]
//! (Status · Priority · Assignee · Labels · Due date · Board · Origin) and
//! [`IssueHeader::agent_row`] (the coding-now card, with Merge PR and Fix
//! conflicts). The host observes this entity so a builder's `cx.notify()`
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
use sync::Store;

use domain::board::format_short_date;
use domain::options::get_issue_priority_config;
use domain::rows::{Issue, Label, Board, User};

use crate::coding_flow::{LocalSessions, StartCodingControl};
use crate::icons::{option_icon, registry, ExpIcon};
use crate::pickers::{chip_button, PICKER_MENU_MIN_WIDTH, PICKER_SEARCH_WIDTH};
use crate::issue_detail::{issue_web_url, set_duplicate_of, DETAIL_GUTTER};
use crate::navigation::go_back;
use crate::queries;
use crate::surface::{glass_pill, glass_pill_button, PillMode, PillSize};

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
        // The Agent group's coding-now card follows the synced sessions; its
        // skip-while-local guard follows the local registry.
        let local_sessions = LocalSessions::global(cx);
        let merge_state = crate::pr_merge::MergeState::global(cx);
        for subscription in [
            cx.observe(&collections.labels, |_, _, cx| cx.notify()),
            cx.observe(&collections.issue_labels, |_, _, cx| cx.notify()),
            cx.observe(&collections.users, |_, _, cx| cx.notify()),
            cx.observe(&collections.team_members, |_, _, cx| cx.notify()),
            cx.observe(&collections.boards, |_, _, cx| cx.notify()),
            cx.observe(&collections.coding_sessions, |_, _, cx| cx.notify()),
            // EXP-549/550: the card's machine name + paused state come from
            // the devices rows — heartbeats and renames re-render it.
            cx.observe(&collections.devices, |_, _, cx| cx.notify()),
            // EXP-314: a status rename/recolor re-renders the status control.
            cx.observe(&collections.issue_statuses, |_, _, cx| cx.notify()),
            cx.observe(&local_sessions, |_, _, cx| cx.notify()),
            // EXP-325: the Merge button's arm/spinner/error live in the
            // shared app-global merge state (any surface can drive them).
            cx.observe(&merge_state, |_, _, cx| cx.notify()),
        ] {
            subscriptions.push(subscription);
        }
        // EXP-698: the coding-now card's Watch pill is gated on the steer
        // config, which is fetched ONCE and lands after this header mounts —
        // without an observer the header renders on the "no relay" default
        // and never asks again, so the pill never appears.
        let steer_config = crate::queries::steer_config(cx);
        subscriptions.push(cx.observe(&steer_config, |_, _, cx| cx.notify()));

        Self {
            issue_id: None,
            due_calendar,
            label_query,
            board_query,
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

    /// The agent row (EXP-256/EXP-417, web `issue-coding-rows.tsx`): the
    /// synced coding-now CARD on its OWN full-width line — its EXP-309
    /// ellipsis chain needs one — above a wrapping row of the merge error and
    /// the fix-conflicts offer. The Start-coding control itself moved into the
    /// chip row (EXP-426). The card is skipped while a LOCAL session runs —
    /// the control already shows the live indicator, and the synced row would
    /// double it as soon as the Electric echo lands.
    ///
    /// EXP-698: the Merge button (EXP-268) is now a trailing action INSIDE the
    /// card whenever both are showing — one tray holds the run and everything
    /// to do about it — and only falls back to its own row when the PR is open
    /// with no live session.
    ///
    /// `None` when the board has no repository, and when there is neither a
    /// card nor an open PR — an empty row would only add padding.
    pub(crate) fn agent_row(
        &mut self,
        issue: &Issue,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if !self.start_coding.read(cx).is_visible(cx) {
            return None;
        }
        // EXP-818: the coding-now CARD is gone — the tray's coding slot
        // (`coding_now_slot`) carries Watch or the teammate caption. What is
        // left here is the merge-error caption.
        let pr_open = issue.pr_state.as_deref() == Some("open");
        if !pr_open {
            return None;
        }
        let mut column = v_flex().w_full().gap_2().px(px(DETAIL_GUTTER)).pb_2();
        let has_card = false;

        let mut controls = h_flex().w_full().flex_wrap().gap_2().items_center();
        // EXP-760: Merge moved UP into the property tray beside Start coding
        // (`chip_row`) — one place for the two actions, where the eye already
        // is. EXP-799: the fix-conflicts offer followed it there — on a real
        // conflict the tray's Merge pill SWAPS to "Fix conflicts" + "Retry
        // merge" (`merge_button`). What is left here is the merge ERROR
        // caption, so a refusal that offers no run (offline, stale base, no
        // App) still has a visible message; empty, the row would render as
        // bare padding.
        let mut has_controls = false;
        if pr_open {
            let error = crate::pr_merge::MergeState::global(cx).read(cx).error(&issue.id);
            if let Some(error) = error {
                controls = controls.child(
                    div()
                        .min_w_0()
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(error),
                );
                has_controls = true;
            }
        }
        if has_controls {
            column = column.child(controls);
        }
        // EXP-760: an open PR alone no longer earns this row — its Merge pill
        // is in the tray above. Nothing to show means nothing rendered.
        if !has_card && !has_controls {
            return None;
        }
        Some(column.into_any_element())
    }

    /// The "Fix conflicts" button (EXP-313): opens the Agent page composer
    /// with the fix-conflicts builtin and this issue's PR preselected.
    /// EXP-799: it wears the PRIMARY paint — while it is up it holds the
    /// tray's Merge slot (see [`merge_slot_swapped`]), so it is the header's
    /// one emphasised action and Merge stands down beside it.
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
        // EXP-698: the ONE capsule at the `Sm` rung — it shares its slot with
        // the merge pill, so the two read as one pair of chips.
        let mut button = crate::surface::glass_pill_button_primary("header-fix-conflicts", PillSize::Sm)
            .icon(
                Icon::from(ExpIcon::GitBranch)
                    .with_size(px(PillSize::Sm.glyph()))
                    .text_color(cx.theme().primary_foreground),
            )
            .label(if fixing { "Fixing…" } else { "Fix conflicts" })
            .tooltip(
                no_agent
                    .clone()
                    .unwrap_or_else(|| "Run the fix-conflicts action on this pull request".into()),
            )
            .on_click(cx.listener(move |_, _, window, cx| {
                if Store::global(cx)
                    .collections()
                    .boards
                    .read(cx)
                    .get(&board_id)
                    .is_none()
                {
                    return;
                }
                // EXP-825: the composer with the fix-conflicts builtin and
                // this PR preselected.
                crate::navigation::navigate_to_chat(
                    window,
                    cx,
                    crate::navigation::ChatSeed::fix_conflicts(issue_id.clone()),
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
    ///
    /// EXP-799: on a REAL conflict the slot swaps, the Reviews / PR-diff
    /// pattern (`reviews_view.rs`, `pr_diff.rs`): "Fix conflicts" takes the
    /// primary paint and Merge steps down to a glass "Retry merge" beside
    /// it — never a dead end, the conflict may have been resolved outside
    /// that run (a teammate rebased and pushed). The swap is short-lived by
    /// construction: `MergeState` drops a failure whose row re-synced
    /// (`superseded_by`), so the next echo restores the plain Merge pill.
    fn merge_button(&self, issue: &Issue, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let (armed, merging, failed_op, is_conflict) = {
            let state = crate::pr_merge::MergeState::global(cx);
            let state = state.read(cx);
            (
                state.armed(&issue.id),
                state.merging(&issue.id),
                state.failed_op(&issue.id),
                state.is_conflict(&issue.id),
            )
        };
        let swapped = merge_slot_swapped(
            issue.pr_state.as_deref() == Some("open"),
            failed_op == Some(crate::pr_merge::FailedOp::Merge),
            is_conflict,
            issue.branch.is_some(),
        );
        let issue_id = issue.id.clone();
        // EXP-698: the ONE capsule at the `Sm` rung — it rides inside the
        // coding-now tray beside the state badge, so it wears the tray's own
        // chip scale rather than the 32px control box it had as a lone row.
        // EXP-760: the PRIMARY (white) pill of the header. With a PR open,
        // merging is what the reader came to do — Start coding stands down to
        // the glass paint beside it (`header_action_styles`).
        let mut button = if swapped {
            glass_pill_button("header-merge-pr", PillSize::Sm, cx)
        } else {
            crate::surface::glass_pill_button_primary("header-merge-pr", PillSize::Sm)
        }
        .icon(
            Icon::from(ExpIcon::GitMerge)
                .with_size(px(PillSize::Sm.glyph()))
                .text_color(if armed {
                    cx.theme().danger
                } else if swapped {
                    cx.theme().muted_foreground
                } else {
                    cx.theme().primary_foreground
                }),
        )
        .label(if merging {
            "Merging…"
        } else if armed {
            "Confirm merge"
        } else if swapped {
            "Retry merge"
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
        if !swapped {
            return button.into_any_element();
        }
        h_flex()
            .items_center()
            .gap_1()
            .child(self.fix_conflicts_button(issue, cx))
            .child(button)
            .into_any_element()
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
    /// relation ▸ · Unmark duplicate (only when it IS one) · Delete issue.
    fn render_actions_menu(
        &mut self,
        issue: &Issue,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let url = issue_web_url(issue, cx);
        let issue_id = issue.id.clone();
        let identifier = issue.identifier.clone();
        let is_duplicate = issue.duplicate_of_id.is_some();
        crate::controls::glass_icon_button("issue-actions", Icon::new(registry::UI_MORE), cx)
            .tooltip("Issue actions")
            .dropdown_menu(move |mut menu, window, cx| {
                {
                    let url = url.clone();
                    menu = menu.item(
                        PopupMenuItem::new("Copy link")
                            .icon(Icon::from(ExpIcon::Link))
                            .disabled(url.is_none())
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
                        PopupMenuItem::new("Unmark duplicate")
                            .icon(Icon::new(registry::UI_UNDO))
                            .on_click(move |_, _, cx| {
                                set_duplicate_of(issue_id.clone(), None, cx);
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

    /// EXP-277/EXP-417: the header's top row — the `…` actions menu, right.
    /// (EXP-723 retired the Subscribe toggle on every client;
    /// auto-subscription and the `issue_subscribers` shape stay. EXP-760
    /// folded copy-link and delete into the menu. EXP-791 retired the
    /// prev/next switcher on the left; the row's leading slot is the detail
    /// view's — it puts the "back to issue" control there while a run is
    /// slid in over the issue.)
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
            .child(div().flex_1().min_w_0())
            // EXP-778: the personal pin toggle — a pinned issue lands in the
            // rail's Pinned section. Needs the team (the board's) to address
            // the toggle; a not-yet-synced board hides it for a repaint.
            .children(self.team_id_of(issue, cx).map(|team_id| {
                crate::pins::pin_toggle_button(
                    "issue-pin",
                    team_id,
                    domain::contract::PIN_KIND_ISSUE,
                    issue.id.clone(),
                    cx,
                )
            }))
            .child(self.render_actions_menu(issue, cx))
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
        //
        // EXP-698 round 5: it also stands down while the coding-now CARD is
        // up — a live run already carries its own state and Watch, so the
        // launcher beside it is noise (web `issue-coding-rows.tsx` returns
        // null for the start variant on a live session). A LOCAL run is the
        // exception: `agent_row` suppresses the card for those because this
        // very control turns into "Coding… / Stop" — hiding it there would
        // strand the run with no way to stop it.
        let local_running = LocalSessions::global_ref(cx)
            .map(|sessions| sessions.read(cx).get(&issue.id).is_some())
            .unwrap_or(false);
        let coding_visible = self.start_coding.read(cx).is_visible(cx);
        let live = crate::issue_detail::has_live_coding_session(&issue.id, cx);
        let start_coding = coding_visible && (local_running || !live);
        // EXP-818: a live run this process does NOT host takes the control's
        // slot instead — the caller's own run as a Watch pill, a teammate's
        // as a muted caption.
        let coding_slot = (coding_visible && !start_coding && live)
            .then(|| crate::issue_detail::coding_now_slot(&issue.id, cx))
            .flatten();
        // EXP-760: which of the two trailing actions show, and which one is
        // the emphasised (white) one.
        let styles = header_action_styles(
            start_coding,
            issue.pr_state.as_deref() == Some("open"),
        );
        self.start_coding
            .update(cx, |control, cx| control.set_demoted(styles.demote_start, cx));

        // EXP-568/EXP-601: everything lives in ONE glass tray — the property
        // chips grow from the left, Start coding floats on the right edge of
        // the same card (`ml_auto`; an action, so it keeps its visual
        // distance from the chips even after wrapping).
        let properties = crate::surface::glass_tray()
            .child(self.status_control(issue, cx))
            .child(self.priority_control(issue, cx))
            .when(!solo_team, |row| {
                row.child(self.assignee_control(issue, cx))
            })
            .child(self.labels_control(issue, cx))
            .child(self.due_control(issue, cx))
            .children(self.board_chip(issue, cx))
            .children(self.origin_chip(issue, cx))
            // EXP-760: BOTH header actions live at the tray's right edge now
            // — Start coding and, while the PR is open, Merge. The stale
            // merge card below the tray is gone.
            .when(styles.any() || coding_slot.is_some(), |tray| {
                tray.child(
                    h_flex()
                        .ml_auto()
                        .flex_shrink_0()
                        .items_center()
                        .gap_1()
                        .when(styles.start_coding, |row| {
                            row.child(self.start_coding.clone())
                        })
                        .children(coding_slot)
                        .when(styles.merge, |row| row.child(self.merge_button(issue, cx))),
                )
            });

        h_flex()
            .w_full()
            .items_center()
            .px(px(DETAIL_GUTTER))
            // EXP-601: a small gap between the prop bar and the description.
            .pb_3()
            // flex_1 + min_w_0: the tray takes the full column width, which
            // is what gives its own `flex_wrap` a definite width to wrap the
            // chips against (a shrink-to-fit tray would size to max-content
            // and paint past the reading column instead).
            .child(properties.flex_1().min_w_0())
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

/// EXP-760: which trailing actions the header's property tray shows, and
/// which of them wears the emphasised (white) paint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HeaderActionStyles {
    pub(crate) start_coding: bool,
    pub(crate) merge: bool,
    /// Start coding drops to the glass pill — an open PR means Merge is the
    /// action, and two white pills side by side name neither.
    pub(crate) demote_start: bool,
}

impl HeaderActionStyles {
    /// Nothing to trail: the tray keeps its chips and no action cluster.
    pub(crate) fn any(&self) -> bool {
        self.start_coding || self.merge
    }
}

/// The pure rule behind [`IssueHeader::chip_row`]'s trailing cluster.
pub(crate) fn header_action_styles(start_visible: bool, pr_open: bool) -> HeaderActionStyles {
    HeaderActionStyles {
        start_coding: start_visible,
        merge: pr_open,
        demote_start: start_visible && pr_open,
    }
}

/// EXP-799: whether the tray's Merge slot swaps to a "Fix conflicts" primary
/// beside a glass "Retry merge". MERGE failures only: the fix run ends in a
/// merge, so a failed CLOSE must never offer it; and only a REAL content
/// conflict (EXP-533) — an offline or policy-refused merge has nothing to
/// rebase; and only with the PR's recorded branch, which the run rebases.
pub(crate) fn merge_slot_swapped(
    pr_open: bool,
    merge_failed: bool,
    is_conflict: bool,
    has_branch: bool,
) -> bool {
    pr_open && merge_failed && is_conflict && has_branch
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-760: exactly ONE emphasised pill in the tray. An open PR makes it
    /// Merge and demotes Start coding; without one, Start coding keeps it.
    #[test]
    fn only_one_header_action_is_emphasised() {
        // The ordinary issue: Start coding, white, no Merge.
        let plain = header_action_styles(true, false);
        assert!(plain.start_coding && !plain.merge && !plain.demote_start);
        assert!(plain.any());

        // PR open with a startable issue: both show, Merge takes the white.
        let both = header_action_styles(true, true);
        assert!(both.start_coding && both.merge && both.demote_start);

        // PR open on a repo-less/live-session issue: Merge alone, and there
        // is nothing to demote.
        let merge_only = header_action_styles(false, true);
        assert!(!merge_only.start_coding && merge_only.merge && !merge_only.demote_start);
        assert!(merge_only.any());

        // Neither: the tray trails nothing at all.
        let neither = header_action_styles(false, false);
        assert!(!neither.any());
        assert!(!neither.demote_start);
    }

    /// EXP-799: the Merge slot swaps to Fix conflicts + Retry merge on a
    /// real conflict of a failed MERGE with a recorded branch — and on
    /// nothing less.
    #[test]
    fn merge_slot_swaps_only_on_a_real_merge_conflict() {
        // A conflict-classified merge failure on an open PR with a branch.
        assert!(merge_slot_swapped(true, true, true, true));
        // A merge refused for another reason (offline, stale base, no App).
        assert!(!merge_slot_swapped(true, true, false, true));
        // A failed CLOSE, even one the server called a conflict.
        assert!(!merge_slot_swapped(true, false, true, true));
        // No recorded branch: nothing for the fix run to rebase.
        assert!(!merge_slot_swapped(true, true, true, false));
        // The PR is no longer open: there is no slot to swap.
        assert!(!merge_slot_swapped(false, true, true, true));
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
