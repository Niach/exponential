//! EXP-895 — THE Changes layout, on every desktop surface that shows a diff.
//!
//! One anatomy, three hosts (the review screen [`crate::pr_diff`], the run's
//! Changes face [`crate::steer_viewer`], and anything else that grows one):
//!
//! ```text
//! Changes +82 −6 · 3 files · exp/APP-14 · Open        [×] [GitHub] [Merge PR]
//! ┌───────────────────┐  ┌──────────────────────────────────────────────┐
//! │ 3 files +82 −6    │  │ M feature/…/strings.xml            +2 −0  ⌄  │
//! │ [ Filter files  ] │  │              15 unchanged lines              │
//! │ M strings.xml f…  │  │ @@ -16,4 +16,6 @@                            │
//! └───────────────────┘  └──────────────────────────────────────────────┘
//! ```
//!
//! The web route (`@exp/ui` `FileDiffNav` + `FileDiffList`) is the reference
//! look and the natives mirror it, so a review reads identically ×4.
//!
//! Two rules the bar exists to hold:
//!
//! * **the bar OWNS the merge control** while the Changes face is up — one
//!   [`MergeSlot`], never two. A conflict-classified failure SWAPS it for the
//!   recovery run (merging is exactly what is blocked) and offers Merge again
//!   as a ghost "Retry merge" beside it, never a dead end;
//! * the counts are the contract's ([`domain::diff::totals`],
//!   [`additions_label`] / [`deletions_label`] — U+2212, never a hyphen).
//!
//! EXP-877 made the pane a FULL PAGE under the run's header rather than a
//! right-hand split: the diff and the transcript are two things you read, not
//! one thing you read while glancing at the other. Its content is centred in
//! the same [`crate::work_header::WORK_COLUMN_W`] column the run's header and
//! transcript use, so nothing shifts sideways when you toggle it.

use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, ClickEvent, Context, Entity,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::InputState,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};

use coding::scm::DiffFile;
use domain::diff::{additions_label, deletions_label, DiffStatus, Totals};

use crate::changes_bar::MergeTarget;
use crate::controls::{glass_input, WebControl as _, WebText as _};
use crate::diff::{status_color, status_letter};
use crate::icons::registry;

/// The file list's own column.
pub(crate) const FILE_LIST_WIDTH: f32 = 216.;

/// One row of the pane's file list — the shared file-row anatomy (status
/// letter · basename · dimmed dir · `+N −M`), which the transcript's per-turn
/// file card renders too ([`file_row`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PaneFile {
    pub(crate) path: SharedString,
    /// The basename, at full weight.
    pub(crate) name: SharedString,
    /// The rest of the path, dimmed.
    pub(crate) dir: SharedString,
    pub(crate) status: DiffStatus,
    pub(crate) additions: u32,
    pub(crate) deletions: u32,
}

impl PaneFile {
    /// Split one [`DiffFile`] into the row's parts.
    pub(crate) fn new(file: &DiffFile) -> Self {
        Self::from_parts(&file.path, file.status, file.additions, file.deletions)
    }

    /// The same, for a producer that carries the four values loose (the
    /// transcript's `FileEdit`).
    pub(crate) fn from_parts(
        path: &str,
        status: DiffStatus,
        additions: u32,
        deletions: u32,
    ) -> Self {
        let (dir, name) = crate::diff::split_path(path);
        Self {
            path: SharedString::from(path.to_string()),
            name: SharedString::from(name),
            dir: SharedString::from(dir),
            status,
            additions,
            deletions,
        }
    }
}

/// What the bar's merge slot holds. EXACTLY one control: a diff surface never
/// shows two ways to merge the same PR.
pub(crate) enum MergeSlot {
    /// The plain control — the shared [`crate::work_header::merge_pill`] at
    /// `PillSize::Sm`, two-click armed like every other merge on the app.
    Merge(MergeTarget),
    /// EXP-533/EXP-799: a REAL content conflict on a failed MERGE hands the
    /// slot to the recovery run (the builtin "Fix merge conflicts" action on
    /// this PR). `retry` keeps Merge reachable beside it as a ghost — the
    /// conflict may be resolved outside that run (a teammate rebases and
    /// pushes), and the swap must never be a dead end.
    FixConflicts {
        /// The issue whose open PR the recovery run rebases.
        issue_id: String,
        /// A fix run for this branch is already going.
        fixing: bool,
        /// EXP-367: no agent CLI on this machine — the reason, verbatim.
        blocked: Option<SharedString>,
        retry: Option<MergeTarget>,
    },
}

/// The Changes BAR: `Changes +N −M · K files · branch · state` and the
/// surface's own controls.
pub(crate) struct DiffBarSpec {
    pub(crate) totals: Totals,
    /// The PR state as prose ("Open", "No Pull Request"); `None` while the
    /// diff is still loading.
    pub(crate) state: Option<SharedString>,
    pub(crate) branch: Option<SharedString>,
    pub(crate) merge: Option<MergeSlot>,
    /// The surface's own glyphs, in order, BEFORE the merge slot: the
    /// review's Close-PR ×, its GitHub link and undock, the pane's close.
    /// The merge control anchors the bar's right edge — it is the one thing
    /// on the bar a reader came to press.
    pub(crate) trailing: Vec<AnyElement>,
}

/// Everything one painting of the pane needs. Generic over the hosting view,
/// like [`crate::changes_bar`] was, so the pane's open/selected state stays
/// the caller's.
pub(crate) struct DiffPaneSpec<V: Render> {
    pub(crate) bar: DiffBarSpec,
    pub(crate) files: Vec<PaneFile>,
    /// The file the list highlights (an index into `files`).
    pub(crate) selected: usize,
    /// Whether the left file list is unfolded.
    pub(crate) list_open: bool,
    /// The `Filter files` field's state. `None` = no filter on this surface.
    pub(crate) filter: Option<Entity<InputState>>,
    /// EXP-862: what the pane is SHOWING, when it is not the whole branch —
    /// "This turn: 3 files" / "This edit". `None` = the session scope, where
    /// the header's Diff pill already says it.
    pub(crate) scope_label: Option<SharedString>,
    /// A failure line under the bar (the review's merge/close refusal).
    pub(crate) caption: Option<SharedString>,
    pub(crate) diff: Entity<crate::diff::DiffView>,
    pub(crate) on_toggle_list: Box<dyn Fn(&mut V, &mut Context<V>) + 'static>,
    /// The scope chip's click: back to the whole branch.
    pub(crate) on_show_session: Box<dyn Fn(&mut V, &mut Context<V>) + 'static>,
    pub(crate) on_pick: std::rc::Rc<dyn Fn(&mut V, usize, &mut Context<V>) + 'static>,
}

/// `+N` / `−M`, mono, the contract's labels and the shared tints. The ONE
/// place a desktop diff prints its counts.
pub(crate) fn counts(additions: u32, deletions: u32, _cx: &gpui::App) -> gpui::Div {
    h_flex()
        .flex_shrink_0()
        .gap_1p5()
        .font_family(theme::terminal::FONT_FAMILY)
        .child(
            div()
                .text_color(theme::tokens::diff::ADD_FG.to_hsla())
                .child(SharedString::from(additions_label(additions))),
        )
        .child(
            div()
                .text_color(theme::tokens::diff::DEL_FG.to_hsla())
                .child(SharedString::from(deletions_label(deletions))),
        )
}

/// ONE file row: `letter · name · dimmed dir · +N −M`. Shared by the pane's
/// file list and the transcript's per-turn file card, so a file reads the
/// same wherever it is listed. Layout only — the caller hangs the click on
/// the returned row.
pub(crate) fn file_row(
    id: impl Into<gpui::ElementId>,
    file: &PaneFile,
    active: bool,
    cx: &gpui::App,
) -> gpui::Stateful<gpui::Div> {
    let muted = cx.theme().muted_foreground;
    crate::surface::flat_row()
        .id(id)
        .flex()
        .w_full()
        .min_w_0()
        .gap_1p5()
        .items_center()
        .px_1p5()
        .py_1()
        .cursor_pointer()
        .text_2xs()
        .font_family(theme::terminal::FONT_FAMILY)
        .when(active, |this| {
            this.bg(theme::tokens::glass::FILL_ACTIVE.to_hsla())
        })
        .hover(|this| this.bg(theme::tokens::glass::FILL_ROW.to_hsla()))
        .child(
            div()
                .flex_shrink_0()
                .w(px(10.))
                .text_center()
                .text_color(status_color(file.status, cx))
                .child(status_letter(file.status)),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_color(if active {
                    cx.theme().foreground
                } else {
                    cx.theme().foreground.opacity(0.9)
                })
                .child(file.name.clone()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(muted)
                .child(file.dir.clone()),
        )
        .child(counts(file.additions, file.deletions, cx))
}

/// The file list's own filter (pure): the indices of `files` whose PATH holds
/// `query`, case-insensitively. An empty query keeps everything, in order.
pub(crate) fn filter_files(files: &[PaneFile], query: &str) -> Vec<usize> {
    let needle = query.trim().to_lowercase();
    files
        .iter()
        .enumerate()
        .filter(|(_, file)| needle.is_empty() || file.path.to_lowercase().contains(needle.as_str()))
        .map(|(index, _)| index)
        .collect()
}

/// `1 file` / `7 files`.
fn file_count_label(files: usize) -> String {
    if files == 1 {
        "1 file".to_string()
    } else {
        format!("{files} files")
    }
}

/// The bar. `Changes +N −M · K files · branch · state` on the left, the
/// surface's glyphs and the ONE merge control on the right.
fn render_bar<V: Render>(
    spec: DiffBarSpec,
    scope: Option<(SharedString, Box<dyn Fn(&mut V, &mut Context<V>) + 'static>)>,
    cx: &mut Context<V>,
) -> gpui::Div {
    let DiffBarSpec {
        totals,
        state,
        branch,
        merge,
        trailing,
    } = spec;
    let muted = cx.theme().muted_foreground;
    let mut bar = h_flex()
        .w_full()
        .flex_shrink_0()
        .h(px(36.))
        .px_1()
        .gap_2()
        .items_center()
        .text_xs()
        .child(div().flex_shrink_0().child("Changes"))
        // Nothing is counted until the files are in hand: a "0 files +0 −0"
        // flash while a review loads reads as an empty PR.
        .when(totals.files > 0, |bar| {
            bar.child(counts(totals.additions, totals.deletions, cx))
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(muted)
                        .child(SharedString::from(file_count_label(totals.files))),
                )
        })
        .children(branch.map(|branch| {
            div()
                .min_w_0()
                .truncate()
                .text_color(muted)
                .font_family(theme::terminal::FONT_FAMILY)
                .child(branch)
        }))
        .children(state.map(|state| div().flex_shrink_0().text_color(muted).child(state)));

    // EXP-862 — the SCOPE chip: what the pane is showing when it is not the
    // whole branch, and the click that goes back to it.
    if let Some((label, on_show_session)) = scope {
        bar = bar.child(
            crate::surface::glass_pill(
                "session-diff-scope",
                crate::surface::PillSize::Sm,
                crate::surface::PillMode::Action,
                cx,
            )
            .flex_shrink_0()
            .tooltip(|window, cx| {
                gpui_component::tooltip::Tooltip::new("Show all changes").build(window, cx)
            })
            .child(div().text_2xs().child(label))
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                cx.stop_propagation();
                on_show_session(this, cx);
            })),
        );
    }

    bar.child(
        h_flex()
            .ml_auto()
            .flex_shrink_0()
            .items_center()
            .gap_1()
            .children(trailing)
            .children(merge.map(|merge| render_merge_slot(merge, cx))),
    )
}

/// The merge slot — exactly one primary control, plus the ghost "Retry merge"
/// the swap leaves standing.
fn render_merge_slot<V: Render>(merge: MergeSlot, cx: &mut Context<V>) -> AnyElement {
    match merge {
        // EXP-917: the shared SLOT — a conflict-refused ISSUE target swaps
        // here too, so a run's Changes bar (which never builds the explicit
        // arm below) offers the recovery run like the review page's does.
        MergeSlot::Merge(target) => {
            crate::work_header::merge_slot("diff-bar-merge", &target, true, cx)
        }
        MergeSlot::FixConflicts {
            issue_id,
            fixing,
            blocked,
            retry,
        } => {
            // Merge steps down to a ghost beside the recovery run rather than
            // vanishing until the PR closes.
            // The label stays the pill's own ("Merge PR"); only the paint
            // changes, so the two controls read as primary + secondary.
            let retry = retry
                .map(|target| crate::work_header::merge_pill("diff-bar-merge", &target, false, cx));
            let mut fix = Button::new("diff-bar-fix").primary().web_sm();
            if fixing {
                fix = fix.label("Fixing…").disabled(true);
            } else if let Some(reason) = blocked {
                fix = fix.label("Fix conflicts").tooltip(reason).disabled(true);
            } else {
                fix = fix.label("Fix conflicts");
            }
            let fix = fix.on_click(move |_: &ClickEvent, window, cx| {
                crate::navigation::navigate_to_chat(
                    window,
                    cx,
                    crate::navigation::ChatSeed::fix_conflicts(issue_id.clone()),
                );
            });
            h_flex()
                .flex_shrink_0()
                .items_center()
                .gap_1()
                .children(retry)
                .child(fix)
                .into_any_element()
        }
    }
}

/// §11/EXP-877/EXP-895 — the pane: the Changes bar over the file list and the
/// shared [`crate::diff::DiffView`], centred in the work column.
pub(crate) fn render<V: Render>(
    spec: DiffPaneSpec<V>,
    window: &Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let DiffPaneSpec {
        bar,
        files,
        selected,
        list_open,
        filter,
        scope_label,
        caption,
        diff,
        on_toggle_list,
        on_show_session,
        on_pick,
    } = spec;
    let multiple = files.len() > 1;
    let totals = bar.totals;

    // The file-list toggle only exists when there is more than one file to
    // list; EXP-862: a glyph on a row is a GHOST button.
    let mut bar = bar;
    if multiple {
        let mut trailing = Vec::with_capacity(bar.trailing.len() + 1);
        trailing.push(
            crate::controls::ghost_icon_button(
                "session-diff-files",
                Icon::new(registry::NAV_FILES),
                cx,
            )
            .tooltip(if list_open {
                "Hide file list"
            } else {
                "Show file list"
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                cx.stop_propagation();
                on_toggle_list(this, cx);
            }))
            .into_any_element(),
        );
        trailing.append(&mut bar.trailing);
        bar.trailing = trailing;
    }
    let scope = scope_label.map(|label| (label, on_show_session));
    let bar = render_bar(bar, scope, cx);

    let query = filter
        .as_ref()
        .map(|state| state.read(cx).value().to_string())
        .unwrap_or_default();
    let visible = filter_files(&files, &query);

    let list = (multiple && list_open).then(|| {
        let mut card = crate::surface::glass_card()
            .id("session-diff-file-list")
            .w(px(FILE_LIST_WIDTH))
            .flex_shrink_0()
            .overflow_hidden()
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_1p5()
                    .px_2p5()
                    .py_2()
                    .text_xs()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(SharedString::from(file_count_label(totals.files))),
                    )
                    .child(counts(totals.additions, totals.deletions, cx)),
            );
        // The `Filter files` field — the issue picker's search-row recipe.
        if let Some(state) = filter.as_ref() {
            card = card.child(
                div().px_2().pb_1p5().child(
                    div()
                        .w_full()
                        .child(glass_input(state, window, cx).small().cleanable(true)),
                ),
            );
        }
        let mut rows = v_flex()
            .id("session-diff-file-rows")
            .w_full()
            .min_w_0()
            .max_h(px(360.))
            .overflow_y_scroll()
            .px_1()
            .pb_1()
            .gap_0p5();
        for index in visible {
            let Some(file) = files.get(index) else {
                continue;
            };
            let on_pick = on_pick.clone();
            rows = rows.child(
                file_row(("session-diff-file", index), file, index == selected, cx).on_click(
                    cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        cx.stop_propagation();
                        on_pick(this, index, cx);
                    }),
                ),
            );
        }
        card.child(rows)
    });

    // EXP-877: the page is full-bleed, its CONTENT is the work column — the
    // same block the header and the transcript sit in, so toggling the diff
    // never shifts the run sideways.
    v_flex()
        .size_full()
        .min_w_0()
        .items_center()
        .overflow_hidden()
        .child(
            v_flex()
                .w_full()
                .max_w(px(crate::work_header::WORK_COLUMN_W))
                .h_full()
                .min_w_0()
                .overflow_hidden()
                .child(bar)
                .children(caption.map(|message| {
                    div()
                        .w_full()
                        .flex_shrink_0()
                        .px_1()
                        .pb_1()
                        .text_xs()
                        .truncate()
                        .text_color(cx.theme().danger)
                        .child(message)
                }))
                .child(
                    h_flex()
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .min_w_0()
                        .gap_2()
                        .items_start()
                        .children(list)
                        .child(div().flex_1().min_w_0().h_full().child(diff)),
                ),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane_file(path: &str, additions: u32, deletions: u32) -> PaneFile {
        PaneFile::from_parts(path, DiffStatus::Modified, additions, deletions)
    }

    /// EXP-877: the pane is a PAGE, so it has exactly one width rule left —
    /// the shared work column. (The 45 % share, the 360px floor, the
    /// transcript's 320px guard and the drag that moved between them are all
    /// gone with the split.)
    #[test]
    fn the_pane_is_the_work_column_wide() {
        assert_eq!(crate::work_header::WORK_COLUMN_W, 896.);
        // The file list still fits inside it with room for a diff.
        assert!(FILE_LIST_WIDTH * 2. < crate::work_header::WORK_COLUMN_W);
    }

    /// EXP-895: the list's filter matches on the whole PATH, not the
    /// basename — a reader types `web/src` as readily as `diff.rs` — and it
    /// is case-insensitive. An empty query is the identity.
    #[test]
    fn the_file_filter_matches_on_path() {
        let files = [
            pane_file("apps/web/src/diff.tsx", 1, 0),
            pane_file("apps/desktop/crates/ui/src/diff.rs", 2, 1),
            pane_file("README.md", 0, 3),
        ];
        assert_eq!(filter_files(&files, ""), vec![0, 1, 2]);
        assert_eq!(filter_files(&files, "   "), vec![0, 1, 2]);
        assert_eq!(filter_files(&files, "diff"), vec![0, 1]);
        assert_eq!(filter_files(&files, "DESKTOP"), vec![1]);
        assert_eq!(filter_files(&files, "readme"), vec![2]);
        assert!(filter_files(&files, "nothing here").is_empty());
    }

    /// A file row splits its path the way the diff card's header does: the
    /// basename at weight, the directory dimmed behind it.
    #[test]
    fn a_pane_file_splits_its_path() {
        let file = pane_file("apps/web/src/diff.tsx", 1, 0);
        assert_eq!(file.name.as_ref(), "diff.tsx");
        assert_eq!(file.dir.as_ref(), "apps/web/src/");
        let root = pane_file("README.md", 0, 0);
        assert_eq!(root.name.as_ref(), "README.md");
        assert_eq!(root.dir.as_ref(), "");
    }

    /// EXP-895: the bar holds EXACTLY one merge control. `Merge` is the
    /// plain pill; the conflict swap replaces it (and only then offers Merge
    /// again, as the secondary "retry").
    #[test]
    fn the_merge_slot_holds_exactly_one_control() {
        let target = MergeTarget::Issue {
            issue_id: "i-1".to_string(),
        };
        let plain = MergeSlot::Merge(target.clone());
        assert!(matches!(plain, MergeSlot::Merge(_)));
        let swapped = MergeSlot::FixConflicts {
            issue_id: "i-1".to_string(),
            fixing: false,
            blocked: None,
            retry: Some(target),
        };
        match swapped {
            MergeSlot::FixConflicts { retry, .. } => {
                assert!(retry.is_some(), "Merge stays reachable beside the swap")
            }
            MergeSlot::Merge(_) => panic!("the swap took the slot"),
        }
    }

    /// The counts on a file row are the CONTRACT's labels — U+2212, never an
    /// ASCII hyphen (the desktop's one deletion label).
    #[test]
    fn file_rows_use_the_minus_sign() {
        assert_eq!(deletions_label(6), "\u{2212}6");
        assert!(!deletions_label(6).contains('-'));
        assert_eq!(additions_label(82), "+82");
        assert_eq!(file_count_label(1), "1 file");
        assert_eq!(file_count_label(3), "3 files");
    }
}
