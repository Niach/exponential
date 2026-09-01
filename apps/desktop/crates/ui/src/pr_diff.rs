//! PR diff center screen (EXP-181): the Reviews rows open this instead of the
//! issue detail — the shared side-by-side [`DiffView`] over `issues.prFiles`,
//! under a sticky header.
//!
//! EXP-706 reshaped that header into the review DETAIL bar the web route
//! grew: two lines on the left (identifier link + branch, then the PR state,
//! the file count and the `+`/`−` totals) and the merge cluster on the right —
//! close, Merge, open-on-GitHub, undock. The `#N` sub is gone (PR numbers are
//! leaving the surfaces), and the diff below renders per-file COLLAPSED cards
//! in a centered column instead of one endless flat list.
//!
//! Merge/close drive the SAME [`crate::pr_merge`] two-click machinery the
//! Reviews list does, so an arm/spinner/failure started on either surface
//! renders identically on both. "Fix conflicts replaces Merge": a
//! conflict-classified merge failure swaps the Merge pill for the recovery
//! run's button — merging is exactly what is blocked.
//!
//! One instance per window, re-pointed by the screens panel on tab switches
//! (the issue-detail / file-viewer model). Same-id re-points are no-ops —
//! `sync_tabs` re-fires on every navigation observer tick, and the fetch must
//! not re-run per tick; the diff is a snapshot of the PR at open time.

use std::sync::Arc;

use gpui::{
    div, prelude::FluentBuilder as _, px, App, AppContext as _, ClickEvent, Entity, FocusHandle,
    Focusable, InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};
use sync::Store;

use crate::controls::WebControl as _;
use crate::diff::DiffView;
use crate::icons::{registry, ExpIcon};
use crate::navigation::{active_team_id, nav_for_window, navigate, Navigation, Screen};
use crate::pr_merge::{close_pr_key, MergeOp, MergeState};
use crate::queries;

/// The diff column's cap — the same 768px the issue detail centers its body
/// to, so a review and an issue read at one width.
const DIFF_COLUMN_W: f32 = 768.;

/// The read-only PR diff center screen.
pub struct PrDiffView {
    focus_handle: FocusHandle,
    nav: Entity<Navigation>,
    diff: Entity<DiffView>,
    issue_id: Option<String>,
    /// EXP-525: the diff lost its tab chip (and with it the chip's undock
    /// button), so the ScreensPanel-owned instance offers "open in new
    /// window" in its own header. Stays `false` on the instances
    /// `build_screen_content` creates — an undocked window must not offer
    /// undocking itself.
    pub(crate) show_undock: bool,
    _subscriptions: Vec<gpui::Subscription>,
}

impl PrDiffView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let diff = cx.new(|cx| {
            let mut diff = DiffView::new(window, cx);
            // EXP-706: the review diff opens as a stack of per-file cards.
            diff.set_collapsible(true);
            diff
        });
        // The header's merge/close cluster mirrors the shared two-click state
        // (EXP-325), and its identity/state line rides the synced issue row.
        let merge_state = MergeState::global(cx);
        let mut subscriptions = vec![cx.observe(&merge_state, |_, _, cx| cx.notify())];
        if let Some(store) = Store::try_global(cx) {
            let issues = store.collections().issues.clone();
            subscriptions.push(cx.observe(&issues, |_, _, cx| cx.notify()));
        }
        // The file counts + `+`/`−` totals come off the diff's own summaries.
        subscriptions.push(cx.observe(&diff, |_, _, cx| cx.notify()));
        Self {
            focus_handle: cx.focus_handle(),
            nav,
            diff,
            issue_id: None,
            show_undock: false,
            _subscriptions: subscriptions,
        }
    }

    /// Re-point at `issue_id` and fetch its PR files (no-op on the same id).
    pub fn set_issue(&mut self, issue_id: String, cx: &mut gpui::Context<Self>) {
        if self.issue_id.as_deref() == Some(issue_id.as_str()) {
            return;
        }
        // The screens panel drives this from its CONSTRUCTOR, which runs
        // while the session is still validating on a background thread — so a
        // cold start into a PR deep link finds no client yet. Latching an
        // error here would be terminal (same-id calls no-op, and the panel
        // only re-drives on a screen CHANGE); stay Loading and leave
        // `issue_id` unrecorded so the Synced re-drive actually re-attempts.
        let Some(client) = queries::trpc_client(cx) else {
            self.diff.update(cx, |diff, cx| diff.set_loading(cx));
            return;
        };
        self.issue_id = Some(issue_id.clone());
        self.diff
            .update(cx, |diff, cx| diff.fetch(Arc::new(client), issue_id, cx));
    }

    /// The "Fix conflicts" recovery run (EXP-259): the Start coding dialog with
    /// the builtin "Fix merge conflicts" action and this PR preselected.
    fn on_fix_conflicts_click(
        &mut self,
        issue_id: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return;
        };
        crate::start_coding_dialog::open_for_fix_conflicts(window, cx, team_id, issue_id);
    }
}

impl Focusable for PrDiffView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for PrDiffView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme_colors = cx.theme();
        let muted = theme_colors.muted_foreground;
        let fg = theme_colors.foreground;
        let danger = theme_colors.danger;
        // EXP-277: content headers use the faint glass row stroke, not the
        // heavier chrome border (fewer/softer section lines).
        let border = theme::tokens::glass::STROKE_ROW.to_hsla();
        let green = theme::tokens::GREEN.to_hsla();

        // Header off the live synced issue row (identifier/branch/PR fields
        // stay fresh); a deleted issue degrades to the bare diff.
        let issue = self.issue_id.as_ref().and_then(|id| {
            Store::global(cx)
                .collections()
                .issues
                .read(cx)
                .get(id)
                .cloned()
        });

        // Totals off the loaded diff — nothing renders while it is still
        // loading (a "0 files" flash would read as an empty PR).
        let totals = {
            let diff = self.diff.read(cx);
            let files = diff.files();
            (!files.is_empty()).then(|| {
                let additions: u32 = files.iter().map(|file| file.additions).sum();
                let deletions: u32 = files.iter().map(|file| file.deletions).sum();
                (files.len(), additions, deletions)
            })
        };

        let mut error_caption: Option<SharedString> = None;
        let header = issue.map(|issue| {
            let nav_id = issue.id.clone();
            let is_open = issue.pr_state.as_deref() == Some("open");
            let close_key = close_pr_key(&issue.id);
            let (merging, armed, closing, close_armed, error, failed_op, is_conflict) = {
                let state = MergeState::global(cx);
                let state = state.read(cx);
                (
                    state.merging(&issue.id),
                    state.armed(&issue.id),
                    state.merging(&close_key),
                    state.armed(&close_key),
                    state.error(&issue.id),
                    state.failed_op(&issue.id),
                    state.is_conflict(&issue.id),
                )
            };
            error_caption = error.clone();

            // EXP-533 + EXP-706: only a REAL content conflict on a failed
            // MERGE offers the recovery run — and when it does, it TAKES the
            // Merge slot (merging is the blocked action).
            let fixing = issue.branch.as_deref().is_some_and(|branch| {
                crate::coding_flow::LocalSessions::global_ref(cx)
                    .is_some_and(|sessions| sessions.read(cx).is_branch_fixing(branch))
            });
            let fix_button = error
                .as_ref()
                .filter(|_| is_open)
                .filter(|_| failed_op == Some(crate::pr_merge::FailedOp::Merge))
                .filter(|_| is_conflict)
                .filter(|_| issue.branch.is_some())
                .map(|_| {
                    let mut button = Button::new("pr-diff-fix").primary().web_sm();
                    if fixing {
                        button = button.label("Fixing…").disabled(true);
                    } else if let Some(reason) = crate::coding_flow::no_agent_reason(cx) {
                        // EXP-367: no agent CLI → disabled with the reason.
                        button = button.label("Fix conflicts").tooltip(reason).disabled(true);
                    } else {
                        button = button.label("Fix conflicts");
                    }
                    let click_id = issue.id.clone();
                    button
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.on_fix_conflicts_click(click_id.clone(), window, cx);
                        }))
                        .into_any_element()
                });

            // The reject path — a quiet round ghost `×` that only grows into a
            // labeled danger confirm once armed (EXP-100).
            let close_button = is_open.then(|| {
                let mut button = Button::new("pr-diff-close");
                if closing {
                    button = button
                        .ghost()
                        .web_icon_sm()
                        .icon(Icon::new(registry::UI_CLOSE))
                        .loading(true)
                        .disabled(true);
                } else if close_armed {
                    button = button.web_sm().label("Close PR").danger();
                } else {
                    button = button
                        .ghost()
                        .web_icon_sm()
                        .icon(Icon::new(registry::UI_CLOSE).text_color(muted))
                        .tooltip("Close PR without merging")
                        .disabled(merging);
                }
                let click_id = issue.id.clone();
                button
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                        crate::pr_merge::two_click(
                            MergeOp::CloseIssuePr {
                                issue_id: click_id.clone(),
                            },
                            None,
                            None,
                            cx,
                        );
                    }))
                    .into_any_element()
            });

            let merge_button = is_open.then(|| {
                let mut button = Button::new("pr-diff-merge").primary().web_sm();
                if merging {
                    button = button.label("Merging…").loading(true).disabled(true);
                } else if armed {
                    button = button.label("Confirm merge").danger();
                } else {
                    button = button
                        .icon(Icon::new(registry::PR_MERGED))
                        .label("Merge")
                        .disabled(closing);
                }
                let click_id = issue.id.clone();
                button
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                        crate::pr_merge::two_click(
                            MergeOp::MergeIssuePr {
                                issue_id: click_id.clone(),
                            },
                            None,
                            None,
                            cx,
                        );
                    }))
                    .into_any_element()
            });
            // The swap: a conflict-classified merge failure replaces Merge in
            // its own slot instead of trailing the error caption.
            let primary = match (fix_button, merge_button) {
                (Some(fix), _) => Some(fix),
                (None, merge) => merge,
            };

            let external = issue.pr_url.clone().map(|url| {
                Button::new("pr-diff-open-github")
                    .ghost()
                    .web_icon_sm()
                    .icon(Icon::new(registry::UI_EXTERNAL_LINK).text_color(muted))
                    .tooltip("Open pull request on GitHub")
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                        crate::settings::open_url(cx, url.clone());
                    }))
                    .into_any_element()
            });

            // Line 2: PR state as plain muted text (no chip), the file count,
            // then the totals — absent entirely while the diff loads.
            let state_text = issue.pr_state.as_deref().map(capitalize);
            let mut meta = h_flex().min_w_0().items_center().gap_2().text_xs();
            if let Some(state) = state_text {
                meta = meta.child(div().flex_shrink_0().text_color(muted).child(state));
            }
            if let Some((files, additions, deletions)) = totals {
                meta = meta
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(muted)
                            .child(SharedString::from(if files == 1 {
                                "1 file".to_string()
                            } else {
                                format!("{files} files")
                            })),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(green)
                            .child(SharedString::from(format!("+{additions}"))),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(danger)
                            .child(SharedString::from(format!("\u{2212}{deletions}"))),
                    );
            }

            h_flex()
                .w_full()
                .min_w_0()
                .flex_shrink_0()
                .px_3()
                .py_2()
                .gap_3()
                .items_center()
                .border_b_1()
                .border_color(border)
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_0p5()
                        .child(
                            h_flex()
                                .min_w_0()
                                .items_center()
                                .gap_2()
                                .child(
                                    Icon::from(ExpIcon::GitPullRequest)
                                        .xsmall()
                                        .flex_shrink_0()
                                        .text_color(green),
                                )
                                // The identifier opens the issue detail — the
                                // row click no longer does (it lands here), so
                                // this is the way back.
                                .child(
                                    div()
                                        .id("pr-diff-open-issue")
                                        .flex_shrink_0()
                                        .text_sm()
                                        .text_color(fg)
                                        .font_family(theme::terminal::FONT_FAMILY)
                                        .hover(|this| this.text_color(theme::tokens::PRIMARY.to_hsla()))
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |_, _, window, cx| {
                                            navigate(
                                                window,
                                                cx,
                                                Screen::IssueDetail {
                                                    issue_id: nav_id.clone(),
                                                },
                                            );
                                        }))
                                        .child(SharedString::from(issue.identifier.clone())),
                                )
                                .children(issue.branch.clone().map(|branch| {
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_xs()
                                        .truncate()
                                        .font_family(theme::terminal::FONT_FAMILY)
                                        .text_color(muted)
                                        .child(SharedString::from(branch))
                                })),
                        )
                        .child(meta),
                )
                .child(
                    h_flex()
                        .flex_shrink_0()
                        .items_center()
                        .gap_1()
                        .children(close_button)
                        .children(primary)
                        .children(external)
                        .when(self.show_undock, |row| {
                            let undock_id = issue.id.clone();
                            row.child(
                                // The undock glyph is `UI_UNDOCK`, not the
                                // ExternalLink every other undock button
                                // wears: this is the ONE place it would sit
                                // beside an actual external link (the GitHub
                                // button), and two identical icons in one
                                // cluster read as a duplicate control.
                                Button::new("pr-diff-undock")
                                    .ghost()
                                    .web_icon_sm()
                                    .icon(Icon::new(registry::UI_UNDOCK).text_color(muted))
                                    .tooltip("Open in new window")
                                    .on_click(cx.listener(
                                        move |_, _: &ClickEvent, window, cx| {
                                            crate::undock::open_undocked_screen(
                                                Screen::PrDiff {
                                                    issue_id: undock_id.clone(),
                                                },
                                                window.window_handle(),
                                                cx,
                                            );
                                            crate::navigation::set_screen(window, cx, None);
                                        },
                                    )),
                            )
                        }),
                )
        });

        // The failure caption is its own thin row under the header — message
        // only (EXP-706: the recovery button lives in the Merge slot above).
        let caption = error_caption.map(|message| {
            div()
                .w_full()
                .flex_shrink_0()
                .px_3()
                .py_1()
                .text_xs()
                .truncate()
                .text_color(danger)
                .child(message)
        });

        // EXP-282: no fill — the screen floats on the page gradient.
        v_flex()
            .size_full()
            .min_w_0()
            .children(header)
            .children(caption)
            .child(
                // The diff body centers to the same column the issue detail
                // uses — a display-BLOCK wrapper + `mx_auto` (the EXP-179-safe
                // recipe), `min_w_0` on every flex hop below it.
                div().flex_1().min_h_0().w_full().min_w_0().child(
                    div()
                        .w_full()
                        .h_full()
                        .max_w(px(DIFF_COLUMN_W))
                        .mx_auto()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .child(self.diff.clone()),
                ),
            )
    }
}

/// `open` → `Open` (the PR state reads as prose next to the counts, not as a
/// wire value). ASCII-safe: the `pr_state` vocabulary is `open`/`closed`/
/// `merged`.
fn capitalize(value: &str) -> SharedString {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => {
            SharedString::from(first.to_uppercase().collect::<String>() + chars.as_str())
        }
        None => SharedString::default(),
    }
}
