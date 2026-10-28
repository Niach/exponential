//! EXP-895/EXP-916 — THE Changes vocabulary a desktop diff surface shares:
//! the file row model ([`PaneFile`], the Guide's coverage + the transcript's
//! per-turn file card read it), the ONE counts renderer ([`counts`]) and the
//! tree builder's input ([`domain::diff_tree::diff_file_tree`], hand-mirrored
//! ×4). S-3 took the thread's Guide (and with it the PR's review pane) away.
//!
//! The counts are the contract's ([`domain::diff::summary_label`],
//! [`additions_label`] / [`deletions_label`] — U+2212, never a hyphen).

use gpui::{div, ParentElement as _, SharedString, Styled};
use gpui_component::h_flex;

use domain::diff::{additions_label, deletions_label, DiffStatus};

/// One row of the pane's file list — the shared file-row anatomy (status
/// letter · basename · dimmed dir · `+N −M`), which the transcript's per-turn
/// file card renders too.
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

#[cfg(test)]
mod tests {
    use super::*;
    use domain::diff::summary_label;
    use domain::diff_tree::{diff_file_tree, DiffTreeKind};

    fn pane_file(path: &str, additions: u32, deletions: u32) -> PaneFile {
        PaneFile::from_parts(path, DiffStatus::Modified, additions, deletions)
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
