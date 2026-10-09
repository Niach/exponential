//! EXP-895/EXP-916 — THE Changes layout, on every desktop surface that shows
//! a diff.
//!
//! One anatomy, every host (the issue's Changes face [`crate::pr_diff`], the
//! run's Changes face [`crate::steer_viewer`], and anything else that grows one):
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
//! The web route (`@exp/ui` `file-diff-tree.tsx` `FileDiffTree`) is the reference
//! look and the natives mirror it, so a review reads identically ×4.
//!
//! EXP-916 took the pane's own `Changes +N −M · K files · branch · state` bar
//! away: the pane IS the changes, and everything that bar said about the PR
//! (its branch, its state, the merge and the GitHub link) belongs to the
//! header ABOVE the pane — the work header on a run or an issue (EXP-1154:
//! the review of a PR is the issue's Changes face). What is left here is the diff and
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
use gpui_component::{h_flex, input::InputState, v_flex, ActiveTheme as _, Icon, Sizable as _};

use coding::scm::DiffFile;
use domain::diff::{additions_label, deletions_label, summary_label, DiffStatus, Totals};
use domain::diff_tree::{diff_file_tree, DiffTreeKind, DiffTreeNode};

use crate::controls::{search_field, SearchFieldSize, WebText as _};
use crate::diff::{status_color, status_letter};
use crate::icons::registry;

/// The file tree's own column.
pub(crate) const FILE_LIST_WIDTH: f32 = 216.;

/// EXP-916: one level of nesting in the file tree.
const TREE_INDENT: f32 = 12.;

/// The gap between the tree column and the diff column.
const TREE_GAP: f32 = 8.;

/// The narrowest diff column the tree may leave beside itself: under it the
/// tree goes and the diff takes the whole pane.
const MIN_DIFF_COLUMN_W: f32 = 640.;

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
    /// EXP-1251: a rename's source — the Guide's coverage matches a listed
    /// path against it too.
    pub(crate) previous_path: Option<SharedString>,
}

impl PaneFile {
    /// Split one [`DiffFile`] into the row's parts.
    pub(crate) fn new(file: &DiffFile) -> Self {
        Self::from_parts(&file.path, file.status, file.additions, file.deletions)
            .with_previous_path(file.previous_path.as_deref())
    }

    /// EXP-1251: carry a rename's source path (blank = none).
    pub(crate) fn with_previous_path(mut self, previous: Option<&str>) -> Self {
        self.previous_path = previous
            .filter(|previous| !previous.is_empty())
            .map(|previous| SharedString::from(previous.to_string()));
        self
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
            previous_path: None,
        }
    }
}

/// A file row was picked: its index into the pane's `files`.
pub(crate) type PickFile<V> = std::rc::Rc<dyn Fn(&mut V, usize, &mut Context<V>) + 'static>;

/// A folder row was clicked: its path.
pub(crate) type ToggleDir<V> = std::rc::Rc<dyn Fn(&mut V, String, &mut Context<V>) + 'static>;

/// Everything one painting of the pane needs. Generic over the hosting view,
/// like [`crate::changes_bar`] was, so the pane's selection and fold state
/// stays the caller's.
pub(crate) struct DiffPaneSpec<V: Render> {
    pub(crate) files: Vec<PaneFile>,
    /// The file the tree highlights (an index into `files`).
    pub(crate) selected: usize,
    /// The `Filter files` field's state. `None` = no filter on this surface.
    pub(crate) filter: Option<Entity<InputState>>,
    /// The directories the reader FOLDED. Every directory is open by
    /// default — a tree that opens closed is a list of folders, not a
    /// review — so this set is what is shut, keyed by the node's path.
    pub(crate) folded_dirs: HashSet<String>,
    /// A failure line over the diff (the issue's merge/close refusal).
    pub(crate) caption: Option<SharedString>,
    pub(crate) diff: Entity<crate::diff::DiffView>,
    /// The host's probe of the pane's own painted width (0 before the first
    /// paint). The pane writes it and reads it back for [`fits_tree`]: the
    /// tree is gated on the room the PANE has, never the window's — a second
    /// sidebar beside it takes its share first (EXP-1192).
    pub(crate) pane_width: std::rc::Rc<std::cell::Cell<f32>>,
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
        // EXP-916: the name alone — the tree above it IS the directory, so a
        // dimmed dir crumb would say it twice (web `FileDiffTree` parity).
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(if active {
                    cx.theme().foreground
                } else {
                    cx.theme().foreground.opacity(0.9)
                })
                .child(file.name.clone()),
        )
        .child(counts(file.additions, file.deletions, cx))
}

/// The pane's rows ARE the tree builder's input — it reads a path and two
/// counts, all of which a [`PaneFile`] already holds. (Rebuilding a
/// `Vec<DiffFile>` here cloned every path once per frame.)
impl domain::diff_tree::DiffTreeRow for PaneFile {
    fn path(&self) -> &str {
        &self.path
    }
    fn additions(&self) -> u32 {
        self.additions
    }
    fn deletions(&self) -> u32 {
        self.deletions
    }
}

/// EXP-916 — the file TREE column, a glass card [`FILE_LIST_WIDTH`] wide
/// beside the diff column: the summary, the `Filter files` field and the tree
/// itself ([`domain::diff_tree::diff_file_tree`], the ×4 mirror).
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
    let nodes = diff_file_tree(files, &query);
    let mut card = crate::surface::glass_card()
        .w(px(FILE_LIST_WIDTH))
        .flex_shrink_0()
        .id("diff-file-tree")
        .min_w_0()
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
                    .child(search_field(state, SearchFieldSize::Sm, window, cx)),
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
        // EXP-916: the ONE disclosure chevron ×4 — down when closed, flipped
        // while open, exactly like the file card header above the diff.
        .child({
            let chevron = Icon::new(registry::UI_CHEVRON_DOWN).xsmall().text_color(muted);
            if open {
                chevron
                    .transform(gpui::Transformation::rotate(gpui::percentage(0.5)))
                    .into_any_element()
            } else {
                chevron.into_any_element()
            }
        })
}

/// EXP-877/EXP-895/EXP-916 — the pane: the file TREE and the shared [`crate::diff::DiffView`], whose
/// column is centred at [`crate::work_header::WORK_COLUMN_W`].
///
/// The tree only appears where the PANE has room for it beside a diff column
/// of at least [`MIN_DIFF_COLUMN_W`] — on a narrow pane (a small window, or a
/// second sidebar beside it) the diff keeps the full width the file list
/// would have taken, so a phone-width pane reads like the phone does.
pub(crate) fn render<V: Render>(
    spec: DiffPaneSpec<V>,
    window: &Window,
    cx: &mut Context<V>,
) -> AnyElement {
    let DiffPaneSpec {
        files,
        selected,
        filter,
        folded_dirs,
        caption,
        diff,
        pane_width,
        on_pick,
        on_toggle_dir,
    } = spec;
    let recorded = pane_width.get();
    // Unmeasured first frame: the card's width with no second sidebar. The
    // probe below corrects it one frame later.
    let available = if recorded > 1. {
        recorded
    } else {
        crate::shell::window_extent(window)
            - crate::shell::left_column_width_for(
                crate::shell::LeftOccupant::Rail,
                crate::shell::window_extent(window),
            )
            - 2. * crate::shell::PANEL_MARGIN
    };
    let shows_tree = fits_tree(available);
    let tree = shows_tree.then(|| {
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
        // EXP-1191: the file cards' outer borders on the work column's
        // content box (the diff list pads itself by `LIST_PAD`).
        .px(px(crate::work_header::WORK_GUTTER - crate::diff::LIST_PAD))
        .h_full()
        .min_w_0()
        .overflow_hidden()
        .children(caption.map(|message| {
            div()
                .w_full()
                .flex_shrink_0()
                .px(px(crate::diff::LIST_PAD))
                .pb_1()
                .text_xs()
                .truncate()
                .text_color(cx.theme().danger)
                .child(message)
        }))
        .child(div().flex_1().min_h_0().w_full().min_w_0().child(diff));
    let host = cx.entity_id();
    v_flex()
        .size_full()
        .min_w_0()
        .items_center()
        .overflow_hidden()
        // The pane's own painted width (its one `w_full` child), read back
        // by the next frame's tree gate. Only a width that FLIPS the gate
        // repaints the host.
        .on_children_prepainted(move |bounds: Vec<gpui::Bounds<gpui::Pixels>>, _, cx| {
            let Some(first) = bounds.first() else {
                return;
            };
            let width = f32::from(first.size.width);
            pane_width.set(width);
            if width > 1. && fits_tree(width) != shows_tree {
                cx.notify(host);
            }
        })
        .child(
            h_flex()
                .w_full()
                .h_full()
                .min_w_0()
                .justify_center()
                .gap(px(TREE_GAP))
                .items_start()
                .children(tree)
                .child(column),
        )
        .into_any_element()
}

/// Whether a pane `pane_width` wide holds the tree AND a diff column of at
/// least [`MIN_DIFF_COLUMN_W`] beside it. Below that the diff keeps the whole
/// width. The width is the PANE's own (the card less any second sidebar),
/// never the window's (EXP-1192).
fn fits_tree(pane_width: f32) -> bool {
    pane_width >= FILE_LIST_WIDTH + TREE_GAP + MIN_DIFF_COLUMN_W
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
        let tree = diff_file_tree(&files, "");
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
        let hits = diff_file_tree(&files, "DESKTOP");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, DiffTreeKind::File);
        assert_eq!(hits[0].index, Some(1), "a file node names its input row");
        assert!(diff_file_tree(&files, "nothing here").is_empty());
    }

    /// EXP-916/EXP-1192: the tree needs its own column PLUS a sane diff
    /// column beside it, measured on the PANE — a narrower pane gives the
    /// diff the whole width instead.
    #[test]
    fn the_tree_column_needs_room_beside_the_diff_column() {
        assert_eq!(FILE_LIST_WIDTH, 216.);
        assert_eq!(TREE_INDENT, 12.);
        let threshold = FILE_LIST_WIDTH + TREE_GAP + MIN_DIFF_COLUMN_W;
        assert_eq!(threshold, 864.);
        assert!(fits_tree(threshold));
        assert!(!fits_tree(threshold - 1.));
        assert!(!fits_tree(0.));
        // The card of a window `w` wide: less the fixed rail and the panel
        // margins, less a second sidebar when one is open.
        let rail = crate::resize_edge::SidebarPanel::Main.default_width();
        let card = |w: f32| w - rail - 2. * crate::shell::PANEL_MARGIN;
        let side = crate::resize_edge::SidebarPanel::List.default_width();
        // A 1280 window: the tree beside a full card, never beside a second
        // sidebar (the diff column would be left ~400px).
        assert!(fits_tree(card(1280.)));
        assert!(!fits_tree(card(1280.) - side));
        assert!(!fits_tree(card(1440.) - side));
        // A wide window holds the sidebar, the tree and the diff.
        assert!(fits_tree(card(1728.) - side));
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
