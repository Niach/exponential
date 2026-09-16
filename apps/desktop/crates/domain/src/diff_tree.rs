//! EXP-916 — the file TREE a Changes surface lists beside its file cards: the
//! sidebar on web (≥md) and the desktop, the file sheet on a phone. One pure
//! builder over the shared [`DiffFile`] model, hand-mirrored ×4 (web `@exp/ui`
//! `FileDiffTree`, TS `@exp/domain-contract` `src/diff-tree.ts`, iOS
//! `ExpCore/Sources/Domain/DiffTree.swift`, Android `domain/DiffTree.kt`) and
//! byte-locked by `packages/domain-contract/fixtures/diff/tree.json`.
//!
//! Rules:
//! - a path splits on `/`; every segment but the last is a directory;
//! - per level, directories come before files, each group sorted by the
//!   lower-cased name (never a locale compare — it is not portable), ties
//!   broken by the raw name the same way;
//! - a directory with exactly ONE child, a directory, and no files of its own
//!   compacts into that child — `apps/web/src` is one node (VS Code's compact
//!   folders), and the compaction repeats down the chain;
//! - a directory's `additions`/`deletions`/`files` are its subtree sums;
//! - a non-blank `query` returns a FLAT list of the FILE nodes whose path
//!   contains it, case-insensitively, in INPUT order — the filter is a search
//!   result, not a pruned tree.
//!
//! gpui-free, like the rest of `domain`.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::diff::DiffFile;

// ── Model ───────────────────────────────────────────────────────────────────

/// A tree row: a directory (possibly a compacted chain) or a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffTreeKind {
    Dir,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffTreeNode {
    pub kind: DiffTreeKind,
    /// The full path from the root (`apps/web/src` for a compacted dir).
    pub path: String,
    /// The label: the last segment, or the compacted chain `a/b/c`.
    pub name: String,
    pub additions: u32,
    pub deletions: u32,
    /// Files in the subtree (1 for a file).
    pub files: usize,
    /// The index into the input `files`; `None` for a directory (the TS twin's
    /// `-1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
    pub children: Vec<DiffTreeNode>,
}

/// A directory under construction. `dirs` is an ORDER-PRESERVING map (the JS
/// twin's `Map`): first-seen insertion order, which the level sort then
/// replaces — but the compaction chain reads `dirs[0]`, so the order must be
/// the twin's.
struct Building {
    node: DiffTreeNode,
    dirs: Vec<(String, Building)>,
    leaves: Vec<DiffTreeNode>,
}

fn new_dir(path: &str, name: &str) -> Building {
    Building {
        node: DiffTreeNode {
            kind: DiffTreeKind::Dir,
            path: path.to_string(),
            name: name.to_string(),
            additions: 0,
            deletions: 0,
            files: 0,
            index: None,
            children: Vec::new(),
        },
        dirs: Vec::new(),
        leaves: Vec::new(),
    }
}

/// What the builder reads off one row: a path and its two counts. A caller
/// whose list is NOT a `Vec<DiffFile>` (the desktop pane holds four fields per
/// row) implements this instead of rebuilding a `DiffFile` per frame.
pub trait DiffTreeRow {
    fn path(&self) -> &str;
    fn additions(&self) -> u32;
    fn deletions(&self) -> u32;
}

impl DiffTreeRow for DiffFile {
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

impl<T: DiffTreeRow> DiffTreeRow for &T {
    fn path(&self) -> &str {
        (*self).path()
    }
    fn additions(&self) -> u32 {
        (*self).additions()
    }
    fn deletions(&self) -> u32 {
        (*self).deletions()
    }
}

fn leaf(file: &impl DiffTreeRow, index: usize, name: &str) -> DiffTreeNode {
    DiffTreeNode {
        kind: DiffTreeKind::File,
        path: file.path().to_string(),
        name: name.to_string(),
        additions: file.additions(),
        deletions: file.deletions(),
        files: 1,
        index: Some(index),
        children: Vec::new(),
    }
}

/// Lower-cased order, then the raw name — in UTF-16 CODE UNITS, because the
/// contract's order is JavaScript's `localeCompare`-free `<` on strings. Rust
/// compares code points, which disagrees above the BMP (a surrogate pair
/// starts at U+D800, below every BMP char from U+E000 up), so the comparison
/// walks `encode_utf16` instead. The fixture locks the agreement.
fn utf16_cmp(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

fn by_name(a: &DiffTreeNode, b: &DiffTreeNode) -> Ordering {
    match utf16_cmp(&a.name.to_lowercase(), &b.name.to_lowercase()) {
        Ordering::Equal => utf16_cmp(&a.name, &b.name),
        other => other,
    }
}

fn finish(dir: Building) -> DiffTreeNode {
    let Building {
        mut node,
        dirs,
        mut leaves,
    } = dir;
    let mut dirs: Vec<Building> = dirs.into_iter().map(|(_, child)| child).collect();
    // Compact a lone child directory into this one, down the chain.
    while dirs.len() == 1 && leaves.is_empty() && !node.path.is_empty() {
        let Building {
            node: only,
            dirs: only_dirs,
            leaves: only_leaves,
        } = dirs.remove(0);
        node.path = only.path;
        node.name = format!("{}/{}", node.name, only.name);
        dirs = only_dirs.into_iter().map(|(_, child)| child).collect();
        leaves = only_leaves;
    }
    let mut children: Vec<DiffTreeNode> = dirs.into_iter().map(finish).collect();
    children.sort_by(by_name);
    leaves.sort_by(by_name);
    children.append(&mut leaves);
    let mut additions: u32 = 0;
    let mut deletions: u32 = 0;
    let mut files: usize = 0;
    for child in &children {
        additions = additions.saturating_add(child.additions);
        deletions = deletions.saturating_add(child.deletions);
        files = files.saturating_add(child.files);
    }
    node.additions = additions;
    node.deletions = deletions;
    node.files = files;
    node.children = children;
    node
}

/// The tree (or, with a non-blank `query`, the flat search result) for one
/// list of files. Pass `""` for no query.
pub fn diff_file_tree<T: DiffTreeRow>(files: &[T], query: &str) -> Vec<DiffTreeNode> {
    let needle = query.trim().to_lowercase();
    if !needle.is_empty() {
        let mut out: Vec<DiffTreeNode> = Vec::new();
        for (index, file) in files.iter().enumerate() {
            if !file.path().to_lowercase().contains(&needle) {
                continue;
            }
            let path = file.path().to_string();
            out.push(leaf(file, index, &path));
        }
        return out;
    }
    let mut root = new_dir("", "");
    for (index, file) in files.iter().enumerate() {
        let path = file.path();
        let segments: Vec<&str> = path.split('/').collect();
        let mut at = &mut root;
        for i in 0..segments.len() - 1 {
            let name = segments[i];
            let path = segments[..=i].join("/");
            let next = match at.dirs.iter().position(|(key, _)| key == name) {
                Some(next) => next,
                None => {
                    at.dirs.push((name.to_string(), new_dir(&path, name)));
                    at.dirs.len() - 1
                }
            };
            at = &mut at.dirs[next].1;
        }
        let name = segments[segments.len() - 1];
        at.leaves.push(leaf(file, index, name));
    }
    finish(root).children
}

/// The byte-lock projection: one line per node, two spaces per depth, a
/// directory as `name +a -d (files)` and a file as `name +a -d`. ASCII `-`,
/// like [`crate::diff::render_diff`].
pub fn render_diff_tree(nodes: &[DiffTreeNode]) -> Vec<String> {
    fn walk(list: &[DiffTreeNode], depth: usize, out: &mut Vec<String>) {
        for node in list {
            let indent = "  ".repeat(depth);
            let counts = format!("+{} -{}", node.additions, node.deletions);
            out.push(match node.kind {
                DiffTreeKind::Dir => format!("{indent}{} {counts} ({})", node.name, node.files),
                DiffTreeKind::File => format!("{indent}{} {counts}", node.name),
            });
            walk(&node.children, depth + 1, out);
        }
    }
    let mut out: Vec<String> = Vec::new();
    walk(nodes, 0, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::DiffStatus;

    /// The contract fixture, byte-locked ×4 (TS `diff-tree.test.ts`, iOS
    /// `DiffTreeTests`, Android `DiffTreeTest`) — the SAME four test names
    /// everywhere.
    const CASES: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/diff/tree.json");

    #[derive(Deserialize)]
    struct FixtureCase {
        name: String,
        files: Vec<FixtureFile>,
        #[serde(default)]
        query: String,
        expected: Vec<String>,
    }

    /// Status is irrelevant to the tree, so the fixture carries path + counts.
    #[derive(Deserialize)]
    struct FixtureFile {
        path: String,
        additions: u32,
        deletions: u32,
    }

    fn cases() -> Vec<FixtureCase> {
        serde_json::from_str(CASES).expect("the diff tree fixture parses")
    }

    fn to_file(path: &str, additions: u32, deletions: u32) -> DiffFile {
        DiffFile {
            path: path.to_string(),
            previous_path: None,
            status: DiffStatus::Modified,
            additions,
            deletions,
            binary: false,
            hunks: Vec::new(),
        }
    }

    /// every fixture case renders byte-exact
    #[test]
    fn every_fixture_case_renders_byte_exact() {
        for entry in &cases() {
            let files: Vec<DiffFile> = entry
                .files
                .iter()
                .map(|file| to_file(&file.path, file.additions, file.deletions))
                .collect();
            let tree = diff_file_tree(&files, &entry.query);
            assert_eq!(render_diff_tree(&tree), entry.expected, "{}", entry.name);
        }
    }

    /// the fixture covers a compaction, a query and an empty input
    #[test]
    fn the_fixture_covers_a_compaction_a_query_and_an_empty_input() {
        let names = cases()
            .iter()
            .map(|entry| entry.name.clone())
            .collect::<Vec<String>>()
            .join("\n");
        assert!(names.contains("compacts"));
        assert!(names.contains("query"));
        assert!(names.contains("no files"));
    }

    /// a file node keeps its input index and full path
    #[test]
    fn a_file_node_keeps_its_input_index_and_full_path() {
        let files: Vec<DiffFile> = ["src/b.ts", "src/a.ts", "top.ts"]
            .iter()
            .map(|path| to_file(path, 1, 0))
            .collect();
        let tree = diff_file_tree(&files, "");
        let top: Vec<(DiffTreeKind, &str, Option<usize>)> = tree
            .iter()
            .map(|node| (node.kind, node.path.as_str(), node.index))
            .collect();
        assert_eq!(
            top,
            vec![
                (DiffTreeKind::Dir, "src", None),
                (DiffTreeKind::File, "top.ts", Some(2)),
            ]
        );
        let children: Vec<(&str, Option<usize>)> = tree[0]
            .children
            .iter()
            .map(|node| (node.path.as_str(), node.index))
            .collect();
        assert_eq!(children, vec![("src/a.ts", Some(1)), ("src/b.ts", Some(0))]);
    }

    /// a compacted directory's path is the deepest segment's
    #[test]
    fn a_compacted_directorys_path_is_the_deepest_segments() {
        let tree = diff_file_tree(&[to_file("apps/web/src/a.ts", 1, 0)], "");
        assert_eq!(tree[0].name, "apps/web/src");
        assert_eq!(tree[0].path, "apps/web/src");
    }
}
