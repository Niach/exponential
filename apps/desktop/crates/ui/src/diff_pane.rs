//! EXP-895/EXP-916 — THE Changes layout, on every desktop surface that shows
//! a diff.
//!
//! One anatomy, three hosts (the review screen [`crate::pr_diff`], the run's
//! Changes face [`crate::steer_viewer`], and anything else that grows one):
//!
//! ```text
//! ┌───────────────────┐  ┌──────────────────────────────────────────────┐
//! │ 3 files +82 −6    │  │ M feature/…/strings.xml            +2 −0  ⌄  │
//! │ [ Filter files  ] │  │              15 unchanged lines              │
//! │ ▾ app/src         │  │ @@ -16,4 +16,6 @@                            │
//! │   M strings.xml   │  │                                              │
//! └───────────────────┘  └──────────────────────────────────────────────┘
//! ```
//!
//! The web route (`@exp/ui` `FileDiffNav` + `FileDiffTree`) is the reference
//! look and the natives mirror it, so a review reads identically ×4.
//!
//! EXP-916 took the pane's own `Changes +N −M · K files · branch · state` bar
//! away: the pane IS the changes, and everything that bar said about the PR
//! (its branch, its state, the merge and the GitHub link) belongs to the
//! header ABOVE the pane — the work header on a run or an issue, the review's
//! own header row on the Reviews detail. What is left here is the diff and
//! the way around it: the file TREE
//! ([`domain::diff_tree::diff_file_tree`], hand-mirrored ×4) over a
//! `Filter files` field.
//!
//! The counts are the contract's ([`domain::diff::summary_label`],
//! [`additions_label`] / [`deletions_label`] — U+2212, never a hyphen).
//!
//! EXP-877 made the pane a FULL PAGE under the run's header rather than a
//! right-hand split: the diff and the transcript are two things you read, not
//! one thing you read while glancing at the other. Its diff column is centred
//! in the same [`crate::work_header::WORK_COLUMN_W`] column the run's header
//! and transcript use, so nothing shifts sideways when you toggle it; the
//! tree sits beside that column, and only where the window has room for both.

use std::collections::HashSet;

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
use domain::diff::{additions_label, deletions_label, summary_label, DiffStatus, Totals};
use domain::diff_tree::{diff_file_tree, DiffTreeKind, DiffTreeNode};

use crate::changes_bar::MergeTarget;
use crate::controls::{glass_input, WebControl as _, WebText as _};
use crate::diff::{status_color, status_letter};
use crate::icons::registry;

/// The file tree's own column.
pub(crate) const FILE_LIST_WIDTH: f32 = 216.;

/// EXP-916: one level of nesting in the file tree.
const TREE_INDENT: f32 = 12.;

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

/// A file row was picked: its index into the pane's `files`.
pub(crate) type PickFile<V> = std::rc::Rc<dyn Fn(&mut V, usize, &mut Context<V>) + 'static>;

/// A folder row was clicked: its path.
pub(crate) type ToggleDir<V> = std::rc::Rc<dyn Fn(&mut V, String, &mut Context<V>) + 'static>;

/// Everything one painting of the pane needs. Generic over the hosting view,
/// like [`crate::changes_bar`] was, so the pane's selection and fold state
/// stays the caller's.
pub(crate) struct DiffPaneSpec<V: Render> {
    /// EXP-916: the surface's OWN header row above the diff — the review's
    /// identifier/branch/state/merge cluster. `None` wherever a work header
    /// already sits above the pane (a run's Changes face, an embedded issue
    /// tab), which is the only header those surfaces get.
    pub(crate) header: Option<AnyElement>,
    pub(crate) files: Vec<PaneFile>,
    /// The file the tree highlights (an index into `files`).
    pub(crate) selected: usize,
    /// The `Filter files` field's state. `None` = no filter on this surface.
    pub(crate) filter: Option<Entity<InputState>>,
    /// The directories the reader FOLDED. Every directory is open by
    /// default — a tree that opens closed is a list of folders, not a
    /// review — so this set is what is shut, keyed by the node's path.
    pub(crate) folded_dirs: HashSet<String>,
    /// A failure line under the header (the review's merge/close refusal).
    pub(crate) caption: Option<SharedString>,
    pub(crate) diff: Entity<crate::diff::DiffView>,
    pub(crate) on_pick: PickFile<V>,
    pub(crate) on_toggle_dir: ToggleDir<V>,
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

/// The [`DiffFile`]s the shared tree builder reads — the pane holds only the
/// four fields a row shows, and a tree needs nothing more.
fn tree_files(files: &[PaneFile]) -> Vec<DiffFile> {
    files
        .iter()
        .map(|file| DiffFile {
            path: file.path.to_string(),
            previous_path: None,
            status: file.status,
            additions: file.additions,
            deletions: file.deletions,
            binary: false,
            hunks: Vec::new(),
        })
        .collect()
}

/// EXP-916 — the file TREE column: the summary, the `Filter files` field and
/// the tree itself ([`domain::diff_tree::diff_file_tree`], the ×4 mirror).
///
/// Directories carry their subtree's counts and are OPEN unless the reader
/// folded them (`folded`, keyed by the node's path); a non-blank query turns
/// the tree into the FLAT list of matching files, because a filtered tree is
/// a search result, not a smaller tree. Picking a file row hands its index
/// back — the host scrolls the diff to it and expands it.
#[allow(clippy::too_many_arguments)]
pub(crate) fn file_tree<V: Render>(
    files: &[PaneFile],
    selected: usize,
    filter: Option<&Entity<InputState>>,
    folded: &HashSet<String>,
    on_pick: PickFile<V>,
    on_toggle_dir: ToggleDir<V>,
    window: &Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let totals = Totals {
        files: files.len(),
        additions: files.iter().map(|file| file.additions).sum(),
        deletions: files.iter().map(|file| file.deletions).sum(),
    };
    let query = filter
        .map(|state| state.read(cx).value().to_string())
        .unwrap_or_default();
    let nodes = diff_file_tree(&tree_files(files), &query);
    let mut card = crate::surface::glass_card()
        .id("diff-file-tree")
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
                        // The ONE summary sentence, the contract's:
                        // `3 files +82 −6` / `No changes`.
                        .child(SharedString::from(summary_label(
                            totals.files,
                            totals.additions,
                            totals.deletions,
                        ))),
                ),
        );
    if let Some(state) = filter {
        card = card.child(
            div().px_2().pb_1p5().child(
                div()
                    .w_full()
                    .child(glass_input(state, window, cx).small().cleanable(true)),
            ),
        );
    }
    let mut painted = 0usize;
    let mut out: Vec<AnyElement> = Vec::new();
    walk_tree(
        &nodes,
        0,
        files,
        selected,
        folded,
        &on_pick,
        &on_toggle_dir,
        &mut painted,
        &mut out,
        cx,
    );
    let rows = v_flex()
        .id("diff-file-tree-rows")
        .w_full()
        .min_w_0()
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .px_1()
        .pb_1()
        .gap_0p5()
        .children(out);
    card.child(rows).into_any_element()
}

/// One level of the tree, depth-first — directories then files, the order the
/// builder already put them in.
#[allow(clippy::too_many_arguments)]
fn walk_tree<V: Render>(
    nodes: &[DiffTreeNode],
    depth: usize,
    files: &[PaneFile],
    selected: usize,
    folded: &HashSet<String>,
    on_pick: &PickFile<V>,
    on_toggle_dir: &ToggleDir<V>,
    painted: &mut usize,
    out: &mut Vec<AnyElement>,
    cx: &mut Context<V>,
) {
    for node in nodes {
        let key = *painted;
        *painted += 1;
        let indent = px(TREE_INDENT * depth as f32);
        match node.kind {
            DiffTreeKind::File => {
                let Some(index) = node.index.filter(|index| *index < files.len()) else {
                    continue;
                };
                let on_pick = on_pick.clone();
                let row = file_row(("diff-tree-file", key), &files[index], index == selected, cx)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        cx.stop_propagation();
                        on_pick(this, index, cx);
                    }));
                out.push(div().w_full().min_w_0().pl(indent).child(row).into_any_element());
            }
            DiffTreeKind::Dir => {
                let open = !folded.contains(&node.path);
                let path = node.path.clone();
                let toggle = on_toggle_dir.clone();
                let row = dir_row(("diff-tree-dir", key), node, open, cx).on_click(cx.listener(
                    move |this, _: &ClickEvent, _window, cx| {
                        cx.stop_propagation();
                        toggle(this, path.clone(), cx);
                    },
                ));
                out.push(div().w_full().min_w_0().pl(indent).child(row).into_any_element());
                if open {
                    walk_tree(
                        &node.children,
                        depth + 1,
                        files,
                        selected,
                        folded,
                        on_pick,
                        on_toggle_dir,
                        painted,
                        out,
                        cx,
                    );
                }
            }
        }
    }
}

/// ONE folder row: `chevron · folder glyph · name · +N −M`.
fn dir_row(
    id: impl Into<gpui::ElementId>,
    node: &DiffTreeNode,
    open: bool,
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
        .hover(|this| this.bg(theme::tokens::glass::FILL_ROW.to_hsla()))
        .child(
            Icon::new(if open {
                registry::UI_FOLDER_OPEN
            } else {
                registry::UI_FOLDER
            })
            .xsmall()
            .text_color(muted),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(cx.theme().foreground.opacity(0.9))
                .child(SharedString::from(node.name.clone())),
        )
        .child(counts(node.additions, node.deletions, cx))
        .child(
            Icon::new(if open {
                registry::UI_CHEVRON_DOWN
            } else {
                registry::UI_CHEVRON_RIGHT
            })
            .xsmall()
            .text_color(muted),
        )
}

/// The merge slot — exactly one primary control, plus the ghost "Retry merge"
/// the swap leaves standing. EXP-916: the surface's own header hosts it (the
/// pane has no bar of its own any more).
pub(crate) fn render_merge_slot<V: Render>(merge: MergeSlot, cx: &mut Context<V>) -> AnyElement {
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

/// EXP-877/EXP-895/EXP-916 — the pane: the surface's own header (when it has
/// one) over the file TREE and the shared [`crate::diff::DiffView`], whose
/// column is centred at [`crate::work_header::WORK_COLUMN_W`].
///
/// The tree only appears where the window has room for it BESIDE that column
/// — on a narrow window the diff keeps the full width and the file list would
/// have taken it, so a phone-width desktop window reads like the phone does.
pub(crate) fn render<V: Render>(
    spec: DiffPaneSpec<V>,
    window: &Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let DiffPaneSpec {
        header,
        files,
        selected,
        filter,
        folded_dirs,
        caption,
        diff,
        on_pick,
        on_toggle_dir,
    } = spec;
    let tree = fits_tree(window).then(|| {
        file_tree(
            &files,
            selected,
            filter.as_ref(),
            &folded_dirs,
            on_pick,
            on_toggle_dir,
            window,
            cx,
        )
    });
    let column = v_flex()
        .w_full()
        .max_w(px(crate::work_header::WORK_COLUMN_W))
        .h_full()
        .min_w_0()
        .overflow_hidden()
        .children(header)
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
        .child(div().flex_1().min_h_0().w_full().min_w_0().child(diff));
    v_flex()
        .size_full()
        .min_w_0()
        .items_center()
        .overflow_hidden()
        .child(
            h_flex()
                .w_full()
                .h_full()
                .min_w_0()
                .justify_center()
                .gap_2()
                .items_start()
                .children(tree)
                .child(column),
        )
        .into_any_element()
}

/// Whether the window is wide enough for the work column AND the tree beside
/// it. Below that the diff keeps the whole width.
fn fits_tree(window: &Window) -> bool {
    f32::from(window.viewport_size().width)
        >= crate::work_header::WORK_COLUMN_W + FILE_LIST_WIDTH + 2. * TREE_INDENT
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

    /// EXP-916: the tree column is the shared builder's
    /// ([`domain::diff_tree::diff_file_tree`], byte-locked ×4) — the pane
    /// only paints it. A query flattens it to the matching FILES, in input
    /// order, case-insensitively.
    #[test]
    fn the_tree_is_the_contracts_and_a_query_flattens_it() {
        let files = [
            pane_file("apps/web/src/diff.tsx", 1, 0),
            pane_file("apps/desktop/crates/ui/src/diff.rs", 2, 1),
            pane_file("README.md", 0, 3),
        ];
        let tree = diff_file_tree(&tree_files(&files), "");
        assert_eq!(
            domain::diff_tree::render_diff_tree(&tree),
            vec![
                "apps +3 -1 (2)",
                "  desktop/crates/ui/src +2 -1 (1)",
                "    diff.rs +2 -1",
                "  web/src +1 -0 (1)",
                "    diff.tsx +1 -0",
                "README.md +0 -3",
            ]
        );
        let hits = diff_file_tree(&tree_files(&files), "DESKTOP");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, DiffTreeKind::File);
        assert_eq!(hits[0].index, Some(1), "a file node names its input row");
        assert!(diff_file_tree(&tree_files(&files), "nothing here").is_empty());
    }

    /// EXP-916: the tree needs the work column PLUS its own beside it — a
    /// narrower window gives the diff the whole width instead.
    #[test]
    fn the_tree_column_needs_room_beside_the_work_column() {
        assert_eq!(FILE_LIST_WIDTH, 216.);
        assert_eq!(TREE_INDENT, 12.);
        assert!(FILE_LIST_WIDTH + crate::work_header::WORK_COLUMN_W > crate::work_header::WORK_COLUMN_W);
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
        // EXP-916: the tree's one summary sentence is the contract's.
        assert_eq!(summary_label(1, 2, 0), "1 file +2 \u{2212}0");
        assert_eq!(summary_label(3, 82, 6), "3 files +82 \u{2212}6");
        assert_eq!(summary_label(0, 0, 0), domain::contract::DIFF_UI_NO_CHANGES);
    }
}
