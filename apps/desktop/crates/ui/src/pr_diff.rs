//! PR diff center screen (EXP-181): the Reviews rows open this instead of the
//! issue detail — the shared unified [`DiffView`] over `issues.prFiles`.
//!
//! EXP-895: the screen is no longer a header of its own. It IS the shared
//! Changes layout ([`crate::diff_pane`]) — `Changes +N −M · K files · branch
//! · state` over the file list and the per-file cards — the same thing the
//! run's Changes face renders, so a review and a run read identically. All
//! this module still owns is WHAT to put in the bar's slots: the issue's PR
//! state, the merge/close cluster and the way back to the issue.
//!
//! Merge/close drive the SAME [`crate::pr_merge`] two-click machinery the
//! Reviews list does, so an arm/spinner/failure started on either surface
//! renders identically on both. "Fix conflicts replaces Merge": a
//! conflict-classified merge failure hands the PRIMARY pill to the recovery
//! run — merging is exactly what is blocked. Merge itself steps down to a
//! ghost "Retry merge" beside it rather than disappearing: the conflict may
//! be resolved outside that run (a teammate rebases and pushes), and
//! [`crate::pr_merge::MergeState`] retires the failure on a re-synced row or
//! a re-point of this screen.
//!
//! One instance per window, re-pointed by the screens panel on tab switches
//! (the issue-detail / file-viewer model). Same-id re-points are no-ops —
//! `sync_tabs` re-fires on every navigation observer tick, and the fetch must
//! not re-run per tick; the diff is a snapshot of the PR at open time.

use std::sync::Arc;

use gpui::{
    div, App, AppContext as _, ClickEvent, Entity, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    ActiveTheme as _, Disableable as _, Icon,
};
use sync::Store;

use crate::controls::WebControl as _;
use crate::diff::DiffView;
use crate::icons::registry;
use crate::navigation::{active_team_id, nav_for_window, navigate, Navigation, Screen};
use crate::pr_merge::{close_pr_key, MergeOp, MergeState};
use crate::queries;

/// EXP-895: the diff column is the shared WORK column
/// ([`crate::work_header::WORK_COLUMN_W`], applied by [`crate::diff_pane`]) —
/// a review, a run and an issue all read at one width.

/// The read-only PR diff center screen.
pub struct PrDiffView {
    focus_handle: FocusHandle,
    nav: Entity<Navigation>,
    diff: Entity<DiffView>,
    issue_id: Option<String>,
    /// EXP-895: the file list's state — which row the list highlights,
    /// whether it is unfolded, and the `Filter files` field.
    selected: usize,
    list_open: bool,
    filter: Entity<gpui_component::input::InputState>,
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
            // EXP-706/EXP-895: the review diff is a stack of per-file cards.
            diff.set_options(crate::diff::DiffOptions::review());
            diff
        });
        let filter = cx.new(|cx| {
            gpui_component::input::InputState::new(window, cx).placeholder("Filter files")
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
        subscriptions.push(cx.subscribe(
            &filter,
            |_, _, event: &gpui_component::input::InputEvent, cx| {
                if matches!(event, gpui_component::input::InputEvent::Change) {
                    cx.notify();
                }
            },
        ));
        Self {
            focus_handle: cx.focus_handle(),
            nav,
            diff,
            issue_id: None,
            selected: 0,
            list_open: true,
            filter,
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
        self.selected = 0;
        // Re-pointing is a refetch: a refusal captioned on the PREVIOUS review
        // describes a snapshot that is no longer on screen, and leaving it
        // standing would keep "Fix conflicts" parked in the Merge slot.
        MergeState::clear_error(cx);
        self.diff
            .update(cx, |diff, cx| diff.fetch(Arc::new(client), issue_id, cx));
    }

    /// Name `index` in the file list and scroll the diff to it.
    fn select_file(&mut self, index: usize, cx: &mut gpui::Context<Self>) {
        self.selected = index;
        self.diff
            .update(cx, |diff, cx| diff.scroll_to_file(index, cx));
        cx.notify();
    }
}

impl Focusable for PrDiffView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for PrDiffView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
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

        // The files (and with them the counts) come off the diff's own
        // summaries — nothing is counted until they are in hand.
        let files: Vec<crate::diff_pane::PaneFile> = self
            .diff
            .read(cx)
            .files()
            .iter()
            .map(|file| {
                crate::diff_pane::PaneFile::from_parts(
                    &file.filename,
                    file.status,
                    file.additions,
                    file.deletions,
                )
            })
            .collect();
        let totals = domain::diff::Totals {
            files: files.len(),
            additions: files.iter().map(|file| file.additions).sum(),
            deletions: files.iter().map(|file| file.deletions).sum(),
        };

        let mut caption: Option<SharedString> = None;
        let mut trailing: Vec<gpui::AnyElement> = Vec::with_capacity(4);
        let mut merge: Option<crate::diff_pane::MergeSlot> = None;
        let mut branch: Option<SharedString> = None;
        let mut state: Option<SharedString> = None;

        if let Some(issue) = issue.as_ref() {
            let is_open = issue.pr_state.as_deref() == Some("open");
            let close_key = close_pr_key(&issue.id);
            let (merging, closing, close_armed, error, failed_op, is_conflict) = {
                let merge_state = MergeState::global(cx);
                let merge_state = merge_state.read(cx);
                (
                    merge_state.merging(&issue.id),
                    merge_state.merging(&close_key),
                    merge_state.armed(&close_key),
                    merge_state.error(&issue.id),
                    merge_state.failed_op(&issue.id),
                    merge_state.is_conflict(&issue.id),
                )
            };
            caption = error.clone();
            branch = issue.branch.clone().map(SharedString::from);
            state = issue.pr_state.as_deref().map(capitalize);

            // The way back: the row click lands HERE now, so the identifier
            // is what reopens the issue.
            let nav_id = issue.id.clone();
            trailing.push(
                div()
                    .id("pr-diff-open-issue")
                    .flex_shrink_0()
                    .px_1()
                    .text_xs()
                    .cursor_pointer()
                    .font_family(theme::terminal::FONT_FAMILY)
                    .text_color(cx.theme().muted_foreground)
                    .hover(|this| this.text_color(theme::tokens::PRIMARY.to_hsla()))
                    .on_click(cx.listener(move |_, _, window, cx| {
                        navigate(
                            window,
                            cx,
                            Screen::IssueDetail {
                                issue_id: nav_id.clone(),
                            },
                        );
                    }))
                    .child(SharedString::from(issue.identifier.clone()))
                    .into_any_element(),
            );

            // The reject path — a quiet round `×` that only grows into a
            // labeled danger confirm once armed (EXP-100). EXP-862: the glyph
            // is a GHOST icon button — a circle means a primary action, and
            // closing a PR without merging is not one.
            if is_open {
                let mut button = if close_armed && !closing {
                    Button::new("pr-diff-close")
                        .web_sm()
                        .label("Close PR")
                        .danger()
                } else {
                    crate::controls::ghost_icon_button(
                        "pr-diff-close",
                        Icon::new(registry::UI_CLOSE),
                        cx,
                    )
                };
                if closing {
                    button = button.loading(true).disabled(true);
                } else if !close_armed {
                    button = button.tooltip("Close PR without merging").disabled(merging);
                }
                let click_id = issue.id.clone();
                trailing.push(
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
                        .into_any_element(),
                );
            }

            if let Some(url) = issue.pr_url.clone() {
                trailing.push(
                    crate::controls::ghost_icon_button(
                        "pr-diff-open-github",
                        Icon::new(registry::UI_EXTERNAL_LINK),
                        cx,
                    )
                    .tooltip("Open pull request on GitHub")
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                        crate::settings::open_url(cx, url.clone());
                    }))
                    .into_any_element(),
                );
            }

            if self.show_undock {
                let undock_id = issue.id.clone();
                trailing.push(
                    // The undock glyph is `UI_UNDOCK`, not the ExternalLink
                    // every other undock button wears: this is the ONE place
                    // it would sit beside an actual external link (the GitHub
                    // button), and two identical icons in one cluster read as
                    // a duplicate control.
                    crate::controls::ghost_icon_button(
                        "pr-diff-undock",
                        Icon::new(registry::UI_UNDOCK),
                        cx,
                    )
                    .tooltip("Open in new window")
                    .on_click(cx.listener(move |_, _: &ClickEvent, window, cx| {
                        crate::undock::open_undocked_screen(
                            Screen::PrDiff {
                                issue_id: undock_id.clone(),
                            },
                            window.window_handle(),
                            cx,
                        );
                        crate::navigation::set_screen(window, cx, None);
                    }))
                    .into_any_element(),
                );
            }

            // EXP-533 + EXP-706 + EXP-895: the merge SLOT. A real content
            // conflict on a failed MERGE hands it to the recovery run — and
            // leaves Merge beside it as the secondary "retry".
            let target = crate::changes_bar::MergeTarget::Issue {
                issue_id: issue.id.clone(),
            };
            let conflicted = error.is_some()
                && failed_op == Some(crate::pr_merge::FailedOp::Merge)
                && is_conflict
                && issue.branch.is_some()
                && active_team_id(&self.nav, cx).is_some();
            if is_open {
                merge = Some(if conflicted {
                    let fixing = issue.branch.as_deref().is_some_and(|branch| {
                        crate::coding_flow::LocalSessions::global_ref(cx)
                            .is_some_and(|sessions| sessions.read(cx).is_branch_fixing(branch))
                    });
                    crate::diff_pane::MergeSlot::FixConflicts {
                        issue_id: issue.id.clone(),
                        fixing,
                        blocked: crate::coding_flow::no_agent_reason(cx).map(SharedString::from),
                        retry: Some(target),
                    }
                } else {
                    crate::diff_pane::MergeSlot::Merge(target)
                });
            }
        }

        crate::diff_pane::render(
            crate::diff_pane::DiffPaneSpec {
                bar: crate::diff_pane::DiffBarSpec {
                    totals,
                    state,
                    branch,
                    merge,
                    trailing,
                },
                files,
                selected: self.selected,
                list_open: self.list_open,
                filter: Some(self.filter.clone()),
                scope_label: None,
                caption,
                diff: self.diff.clone(),
                on_toggle_list: Box::new(|this: &mut Self, cx| {
                    this.list_open = !this.list_open;
                    cx.notify();
                }),
                on_show_session: Box::new(|_, _| {}),
                on_pick: std::rc::Rc::new(|this: &mut Self, index, cx| {
                    this.select_file(index, cx);
                }),
            },
            window,
            cx,
        )
    }
}

/// `open` → `Open` (the PR state reads as prose next to the counts, not as a
/// wire value). ASCII-safe: the `pr_state` vocabulary is `open`/`closed`/
/// `merged`.
pub(crate) fn capitalize(value: &str) -> SharedString {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => {
            SharedString::from(first.to_uppercase().collect::<String>() + chars.as_str())
        }
        None => SharedString::default(),
    }
}
