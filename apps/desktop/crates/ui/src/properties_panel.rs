//! The issue-detail properties sidebar (masterplan-v3 §4.2; web parity
//! target: `apps/web/src/components/issue-properties-panel.tsx` in `sidebar`
//! layout — desktop always renders the non-mobile branch, §4.9).
//!
//! Groups top-to-bottom exactly like web: Status · Priority · Assignee ·
//! Labels · Due date · Board. Every
//! control mutates immediately through tRPC (`issues.update` /
//! `issueLabels.add|remove`) in the §4.1 un-gated form — the Electric echo
//! re-renders. `completed_at` is server-managed and never set here.
//!
//! Due-date control (§4.2, web `DueDateControl` sidebar layout): a ghost
//! trigger labeled **"Due date" when empty**, icon + short date once set
//! (the icon-only-when-empty rule applies to the board ROW's
//! `due-date-dropdown.tsx`, not this panel); the popover hosts the
//! gpui-component `Calendar` plus a Clear action. Clearing the date
//! cascade-nulls `due_time`/`end_time` (web `onDueDateSelect`). The synced
//! `issues` shape deliberately drops `due_time`/`end_time` (§5.4), so the
//! desktop shows no time inputs — date edits leave any server-side times
//! untouched except through that cascade.

use chrono::NaiveDate;

use gpui::{
    div, px, App, AppContext as _, ClipboardItem, Entity, FontWeight, IntoElement, ParentElement,
    Render, SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    calendar::{Calendar, CalendarEvent, CalendarState, Date},
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    popover::Popover,
    v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, Side,
};
use serde::Serialize;
use sync::Store;

use domain::board::format_short_date;
use domain::options::{
    get_issue_priority_config, get_issue_status_config, IssueOption, ISSUE_PRIORITY_OPTIONS,
    ISSUE_STATUS_OPTIONS,
};
use domain::rows::{Issue, Label, Board, User};

use crate::coding_flow::{LocalSessions, StartCodingControl};
use crate::icons::{option_icon, ExpIcon};
use crate::issue_detail::{is_subscribed, issue_web_url, set_duplicate_of};
use crate::issue_list::IssueQuery;
use crate::navigation::{go_back, replace_screen, Screen};
use crate::queries;

/// Detail sidebar width — narrower than the web's `w-72` (288px): the
/// desktop panel holds compact chip controls, so 288px left it mostly
/// empty (EXP-144).
const PANEL_WIDTH: f32 = 240.;

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

pub struct PropertiesPanel {
    issue_id: Option<String>,
    due_calendar: Entity<CalendarState>,
    /// The window's shared rail state — the EXP-48 switcher reads the active
    /// issue board's query + filters from it (EXP-277: the switcher lives in
    /// this panel's toolbar row now).
    rail_shared: Entity<crate::sidebar::RailShared>,
    /// Subscribe-toggle in-flight flag (web `busy`).
    subscribe_busy: bool,
    /// Copy-link feedback: the toolbar button shows a check for ~1.5s after a
    /// copy (web `linkCopied`). The seq guards the disarm timer against a
    /// re-click racing an older timer (the sidebar's merge-confirm pattern).
    link_copied: bool,
    link_copied_seq: u64,
    /// The detail view's Start-coding control, rendered here as the "Agent"
    /// group (EXP-256, web parity — the entity stays owned by the detail
    /// view, which also reads its `resolved_repo` for the actions menu).
    start_coding: Entity<StartCodingControl>,
    /// Merge button state (EXP-268 — the reviews-rail two-click arm pattern):
    /// the armed issue id, its auto-disarm sequence, the in-flight issue id
    /// (spinner held until the Electric echo flips `pr_state`), the last
    /// failure caption.
    merge_arm: Option<String>,
    merge_arm_seq: u64,
    merging: Option<String>,
    merge_error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl PropertiesPanel {
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
        // The Agent group's coding-now pill follows the synced sessions; its
        // skip-while-local guard follows the local registry. The toolbar's
        // subscribe toggle follows the issue_subscribers shape (EXP-277).
        let local_sessions = LocalSessions::global(cx);
        for subscription in [
            cx.observe(&collections.labels, |_, _, cx| cx.notify()),
            cx.observe(&collections.issue_labels, |_, _, cx| cx.notify()),
            cx.observe(&collections.users, |_, _, cx| cx.notify()),
            cx.observe(&collections.team_members, |_, _, cx| cx.notify()),
            cx.observe(&collections.boards, |_, _, cx| cx.notify()),
            cx.observe(&collections.coding_sessions, |_, _, cx| cx.notify()),
            cx.observe(&collections.issue_subscribers, |_, _, cx| cx.notify()),
            cx.observe(&local_sessions, |_, _, cx| cx.notify()),
        ] {
            subscriptions.push(subscription);
        }
        // EXP-48 switcher (EXP-277: now in this panel's toolbar): the counter
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
            rail_shared,
            subscribe_busy: false,
            link_copied: false,
            link_copied_seq: 0,
            start_coding,
            merge_arm: None,
            merge_arm_seq: 0,
            merging: None,
            merge_error: None,
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

        let trigger = match issue.assignee_id.as_deref() {
            // Assigned — render the member's name, falling back to `Member
            // <LAST4>` when the co-member's user row didn't sync.
            Some(id) => Button::new("prop-assignee").ghost().xsmall().label(
                SharedString::from(crate::comments::user_label(id, selected.as_ref())),
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
            // Member lists grow with the team — cap + scroll (EXP-46a).
            let mut menu = menu
                .check_side(Side::Right)
                .scrollable(true)
                .max_h(px(320.));
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

    /// Web `LabelPicker`: toggle menu over the team's labels (colored
    /// dot + name + check). Label creation stays in team settings on
    /// desktop v1.
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

        Button::new("prop-labels")
            .ghost()
            .xsmall()
            .icon(Icon::from(ExpIcon::Tag).text_color(cx.theme().muted_foreground))
            .label(SharedString::from(trigger_label))
            .dropdown_menu(move |menu, _, _| {
                // Label lists grow with the team — cap + scroll (EXP-46a).
                let mut menu = menu
                    .check_side(Side::Right)
                    .scrollable(true)
                    .max_h(px(320.));
                if labels.is_empty() {
                    return menu.item(PopupMenuItem::label("No labels in this team"));
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

    /// Origin chip for widget-filed issues (web keys a "Feedback widget"
    /// origin off `issues.source`). Widget rows carry a null creator, so this
    /// is the only author/origin signal; renders NOTHING for `user`/None.
    fn origin_chip(&self, issue: &Issue, cx: &App) -> Option<impl IntoElement> {
        if issue.source.as_deref() != Some(domain::contract::ISSUE_SOURCE_WIDGET) {
            return None;
        }
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
                .child(
                    Icon::from(ExpIcon::MessageSquare)
                        .xsmall()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(SharedString::from("Feedback widget")),
        )
    }

    /// The "Agent" group body (EXP-256, web `issue-coding-rows.tsx` sidebar
    /// variant): the synced coding-now pill above the full-width
    /// Start-coding/Stop control, plus a Merge button while the linked PR is
    /// open (EXP-268 — sidebar merge, web parity). The pill is skipped while
    /// a LOCAL session runs — the control already shows the live indicator,
    /// and the synced pill would double it as soon as the Electric echo
    /// lands.
    fn agent_control(
        &self,
        issue: &Issue,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let local_running = LocalSessions::global_ref(cx)
            .map(|sessions| sessions.read(cx).get(&issue.id).is_some())
            .unwrap_or(false);
        let mut column = v_flex().w_full().gap_2();
        if !local_running {
            if let Some(pill) = crate::issue_detail::coding_now_pill(&issue.id, cx) {
                column = column.child(pill);
            }
        }
        column = column.child(self.start_coding.clone());
        if issue.pr_state.as_deref() == Some("open") {
            column = column.child(self.merge_button(issue, cx));
            if let Some(error) = self.merge_error.clone() {
                column = column.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(error),
                );
            }
        }
        column
    }

    /// The sidebar Merge button (EXP-268): two-click arm ("Merge" →
    /// "Confirm merge", auto-disarm ~5s — the reviews-rail pattern), then
    /// `issues.mergePr` on the background executor. The spinner is held
    /// until the Electric echo flips `pr_state` away from `open` (which
    /// also drops the whole button); the server ends the issue's live
    /// coding session on merge, so the terminal tears down on its own.
    fn merge_button(&self, issue: &Issue, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let armed = self.merge_arm.as_deref() == Some(issue.id.as_str());
        let merging = self.merging.as_deref() == Some(issue.id.as_str());
        let issue_id = issue.id.clone();
        let mut button = Button::new("sidebar-merge-pr")
            .outline()
            .small()
            .w_full()
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
            .tooltip("Merge the pull request — completes every linked issue and ends its coding session")
            .on_click(cx.listener(move |this, _, _, cx| {
                this.on_merge_click(issue_id.clone(), cx);
            }));
        if merging {
            button = button.disabled(true);
        }
        button
    }

    fn on_merge_click(&mut self, issue_id: String, cx: &mut gpui::Context<Self>) {
        if self.merging.is_some() {
            return;
        }
        if self.merge_arm.as_deref() != Some(issue_id.as_str()) {
            // First click arms; auto-disarm after ~5s (seq-guarded).
            self.merge_arm = Some(issue_id);
            self.merge_arm_seq += 1;
            let seq = self.merge_arm_seq;
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(5))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if this.merge_arm_seq == seq && this.merge_arm.is_some() {
                        this.merge_arm = None;
                        cx.notify();
                    }
                });
            })
            .detach();
            cx.notify();
            return;
        }

        // Confirmed — fire the server-side squash merge.
        self.merge_arm = None;
        self.merge_arm_seq += 1;
        self.merge_error = None;
        let Some(trpc) = queries::trpc_client(cx) else {
            log::warn!("[ui] issues.mergePr skipped: no active account");
            cx.notify();
            return;
        };
        self.merging = Some(issue_id.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let call_id = issue_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move { api::issues::merge_pr(&trpc, &call_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Err(err) = result {
                    log::warn!("[ui] issues.mergePr({issue_id}) failed: {err}");
                    let message = match err {
                        api::ApiError::Http { message, .. } => message,
                        other => other.to_string(),
                    };
                    this.merging = None;
                    this.merge_error = Some(SharedString::from(message));
                    cx.notify();
                }
                // Success: the Electric echo flips `pr_state` and the whole
                // button leaves the panel (the issues observer re-renders).
            });
        })
        .detach();
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

    /// Where this issue sits in the ACTIVE issue list's flattened visible
    /// ordering (the sidebar's My Issues board while that tool is active,
    /// the active board's list otherwise) — same grouping, same EXP-38
    /// comparator, same filters the list applies. `None` (hide the switcher)
    /// when no list scope resolves or the issue isn't in the filtered list.
    fn switcher_state(&self, issue: &Issue, cx: &App) -> Option<SwitcherState> {
        let (query, filters) = {
            let board = self.rail_shared.read(cx).active_issue_board().read(cx);
            (board.query().clone(), board.filters().clone())
        };
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
        let ids = domain::board::flatten_group_issue_ids(&data.groups);
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
                        .ghost()
                        .xsmall()
                        .icon(
                            Icon::new(IconName::ChevronUp)
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
                        .ghost()
                        .xsmall()
                        .icon(
                            Icon::new(IconName::ChevronDown)
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
            .ghost()
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

    /// Web `SubscribeToggle`, icon-only in the 240px panel (Bell/BellOff +
    /// tooltip), live off the `issue_subscribers` shape.
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
            .ghost()
            .xsmall()
            .icon(Icon::from(icon).text_color(tint))
            .disabled(self.subscribe_busy || account.is_none())
            .tooltip(if subscribed {
                "Subscribed — click to unsubscribe"
            } else {
                "Subscribe to this issue"
            })
            .on_click(cx.listener(|this, _, window, cx| this.toggle_subscription(window, cx)))
    }

    /// The `…` actions menu (web L361-398): always present (EXP-59) with the
    /// Move-to-board submenu (EXP-57 — hidden without a move target) and the
    /// destructive Delete-issue confirm submenu, plus Unmark duplicate for a
    /// duplicate issue. After the delete fires, the tabbed analog of the
    /// web's back-navigation is popping the back stack.
    fn render_actions_menu(
        &mut self,
        issue: &Issue,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let issue_id = issue.id.clone();
        let board_id = issue.board_id.clone();
        let is_duplicate = issue.duplicate_of_id.is_some();
        let can_move = !crate::issue_list::move_target_boards(cx, &board_id).is_empty();
        Button::new("issue-actions")
            .ghost()
            .xsmall()
            .icon(Icon::new(IconName::Ellipsis).text_color(cx.theme().muted_foreground))
            .dropdown_menu(move |mut menu, window, cx| {
                if is_duplicate {
                    let issue_id = issue_id.clone();
                    menu = menu
                        .item(
                            PopupMenuItem::new("Unmark duplicate")
                                .icon(Icon::new(IconName::Undo2))
                                .on_click(move |_, _, cx| {
                                    set_duplicate_of(issue_id.clone(), None, cx);
                                }),
                        )
                        .separator();
                }
                if can_move {
                    let issue_id = issue_id.clone();
                    let board_id = board_id.clone();
                    menu = menu.submenu_with_icon(
                        Some(Icon::from(ExpIcon::SquareKanban)),
                        "Move to board",
                        window,
                        cx,
                        move |menu, _, cx| {
                            crate::issue_list::move_to_board_menu(
                                menu,
                                &issue_id,
                                &board_id,
                                cx,
                            )
                        },
                    );
                }
                let issue_id = issue_id.clone();
                menu.submenu_with_icon(
                    Some(Icon::new(IconName::Delete)),
                    "Delete issue",
                    window,
                    cx,
                    move |menu, _, _| {
                        let issue_id = issue_id.clone();
                        menu.item(
                            PopupMenuItem::new("Confirm delete")
                                .icon(Icon::new(IconName::Delete))
                                .on_click(move |_, window, cx| {
                                    crate::issue_list::spawn_issue_delete(cx, issue_id.clone());
                                    go_back(window, cx);
                                }),
                        )
                    },
                )
            })
    }

    /// EXP-277: the panel's compact toolbar row — the former issue-detail
    /// header cluster. Switcher left, copy-link · subscribe · actions right.
    fn render_toolbar(&mut self, issue: &Issue, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let mut row = h_flex().w_full().gap_0p5().items_center().min_w_0();
        row = row.children(self.render_switcher(issue, cx));
        row = row.child(div().flex_1().min_w_0());
        row = row.child(self.render_copy_link(issue, cx));
        row = row.child(self.render_subscribe_toggle(issue, cx));
        row = row.child(self.render_actions_menu(issue, cx));
        row
    }

    fn board_chip(&self, issue: &Issue, cx: &App) -> Option<impl IntoElement> {
        let board: Board = Store::global(cx)
            .collections()
            .boards
            .read(cx)
            .get(&issue.board_id)
            .cloned()?;
        let color = board
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
                .child(SharedString::from(board.name)),
        )
    }
}

impl Render for PropertiesPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // EXP-277: no left hairline — whitespace separates the sidebar from
        // the centered detail column (blended chrome).
        let base = v_flex()
            .w(px(PANEL_WIDTH))
            .flex_shrink_0()
            .h_full()
            .px_3()
            .py_3()
            .gap_3()
            .text_sm();

        let Some(issue) = self.issue(cx) else {
            return base;
        };

        // EXP-50: a team with exactly one human member has no assignment
        // choice — hide the assignee control entirely (server-side default
        // assignment keeps the data correct). Multi-member (and the not-yet-
        // synced 0-member snapshot) keeps the picker.
        let solo_team = self.member_users(&issue, cx).len() == 1;

        base.child(self.render_toolbar(&issue, cx))
            .child(property_group("Status", self.status_control(&issue, cx), cx))
            .child(property_group(
                "Priority",
                self.priority_control(&issue, cx),
                cx,
            ))
            .when(!solo_team, |panel| {
                panel.child(property_group(
                    "Assignee",
                    self.assignee_control(&issue, cx),
                    cx,
                ))
            })
            .child(property_group("Labels", self.labels_control(&issue, cx), cx))
            .child(property_group(
                "Due date",
                self.due_control(&issue, cx),
                cx,
            ))
            .when_some(self.board_chip(&issue, cx), |panel, chip| {
                panel.child(property_group("Board", chip, cx))
            })
            .when_some(self.origin_chip(&issue, cx), |panel, chip| {
                panel.child(property_group("Origin", chip, cx))
            })
            // Web places Agent last; gate on the control's own visibility so
            // a repo-less board never shows an orphaned group label.
            .when(self.start_coding.read(cx).is_visible(cx), |panel| {
                panel.child(property_group("Agent", self.agent_control(&issue, cx), cx))
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
/// `pub(crate)` — the support-thread and action-detail sidebars reuse it
/// (EXP-277).
pub(crate) fn property_group(
    label: &'static str,
    control: impl IntoElement,
    cx: &App,
) -> impl IntoElement {
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
/// check.
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
