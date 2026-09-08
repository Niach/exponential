//! EXP-786 — the per-call unified diff a `tool_update` carries.
//!
//! Two pure helpers: [`unified_diff`] builds a patch from ACP's
//! `ToolCallContent::Diff` (a path plus old/new text — ACP hands over no
//! patch), and [`truncate_unified_diff`] cuts it to the contract's
//! `toolDiffMaxLines`/`toolDiffMaxBytes` on LINE boundaries so every client
//! renders the same prefix and never a torn line. No diff crate in the
//! workspace: the line diff is a bounded LCS, and past the bound the patch is
//! one whole-replacement hunk — the wire truncates it to 200 lines anyway,
//! and a viewer's diff card is a glance, not a review.

/// The contract's per-call diff caps (`steerFeed.toolDiffMaxLines` /
/// `toolDiffMaxBytes`) — what the publisher truncates to, and what the
/// relay's zod admits (plus the one-line truncation marker).
pub const TOOL_DIFF_MAX_LINES: usize = domain::contract::STEER_FEED_TOOL_DIFF_MAX_LINES;
pub const TOOL_DIFF_MAX_BYTES: usize = domain::contract::STEER_FEED_TOOL_DIFF_MAX_BYTES;

/// Context lines around a change, git's default.
const CONTEXT: usize = 3;
/// The LCS table's cell budget (`u32` cells, ~16 MiB): above it the diff is
/// a whole-replacement hunk instead of an exact edit script.
const LCS_CELL_BUDGET: usize = 4_000_000;

/// A git-style unified diff of `old` → `new` at `path` (already display-form:
/// worktree-relative). `None` old = a new file (`--- /dev/null`). Empty when
/// nothing changed, so the caller publishes no `diff` at all.
pub fn unified_diff(path: &str, old: Option<&str>, new: &str) -> String {
    let old_lines = lines(old.unwrap_or(""));
    let new_lines = lines(new);
    if old_lines == new_lines {
        return String::new();
    }
    let ops = edit_script(&old_lines, &new_lines);
    let mut out = String::new();
    if old.is_some() {
        out.push_str("--- a/");
        out.push_str(path);
    } else {
        out.push_str("--- /dev/null");
    }
    out.push('\n');
    out.push_str("+++ b/");
    out.push_str(path);
    out.push('\n');
    for (start, end) in hunk_ranges(&ops) {
        // Line numbers: count the old/new lines consumed before the hunk.
        let (mut old_at, mut new_at) = (0usize, 0usize);
        for op in &ops[..start] {
            match op {
                Op::Eq(_) => {
                    old_at += 1;
                    new_at += 1;
                }
                Op::Del(_) => old_at += 1,
                Op::Ins(_) => new_at += 1,
            }
        }
        let (mut old_count, mut new_count) = (0usize, 0usize);
        for op in &ops[start..end] {
            match op {
                Op::Eq(_) => {
                    old_count += 1;
                    new_count += 1;
                }
                Op::Del(_) => old_count += 1,
                Op::Ins(_) => new_count += 1,
            }
        }
        out.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            hunk_start(old_at, old_count),
            old_count,
            hunk_start(new_at, new_count),
            new_count
        ));
        for op in &ops[start..end] {
            let (prefix, line) = match op {
                Op::Eq(line) => (' ', line),
                Op::Del(line) => ('-', line),
                Op::Ins(line) => ('+', line),
            };
            out.push(prefix);
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Cut `patch` to at most `max_lines` lines and `max_bytes` bytes, on line
/// boundaries only — a kept line is always whole. Returns the kept prefix
/// (its newlines intact) and how many lines were dropped; the caller decides
/// how to mark the cut. `(patch, 0)` when it already fits.
pub fn truncate_unified_diff(patch: &str, max_lines: usize, max_bytes: usize) -> (String, usize) {
    let mut kept = String::new();
    let mut kept_lines = 0usize;
    let mut omitted = 0usize;
    for line in patch.split_inclusive('\n') {
        // A PREFIX: the first line that does not fit ends the kept part, and
        // every line after it is omitted even when it would fit on its own.
        if omitted == 0 && kept_lines < max_lines && kept.len() + line.len() <= max_bytes {
            kept.push_str(line);
            kept_lines += 1;
        } else {
            omitted += 1;
        }
    }
    (kept, omitted)
}

/// Split into lines without a phantom trailing entry for a final newline.
fn lines(text: &str) -> Vec<&str> {
    let mut out: Vec<&str> = text.split('\n').collect();
    if text.ends_with('\n') || text.is_empty() {
        out.pop();
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op<'a> {
    Eq(&'a str),
    Del(&'a str),
    Ins(&'a str),
}

/// The edit script, common prefix/suffix stripped before the LCS so a
/// one-line change in a long file costs nothing.
fn edit_script<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<Op<'a>> {
    let prefix = old
        .iter()
        .zip(new.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let (old_mid, new_mid) = (
        &old[prefix..old.len() - suffix],
        &new[prefix..new.len() - suffix],
    );
    let mut ops: Vec<Op<'a>> = old[..prefix].iter().map(|line| Op::Eq(line)).collect();
    let cells = (old_mid.len() + 1).saturating_mul(new_mid.len() + 1);
    if cells > LCS_CELL_BUDGET {
        ops.extend(old_mid.iter().map(|line| Op::Del(line)));
        ops.extend(new_mid.iter().map(|line| Op::Ins(line)));
    } else {
        ops.extend(lcs_ops(old_mid, new_mid));
    }
    ops.extend(old[old.len() - suffix..].iter().map(|line| Op::Eq(line)));
    ops
}

/// Classic LCS table + backtrack; deletions before insertions at a boundary,
/// which is what `diff` prints too.
fn lcs_ops<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<Op<'a>> {
    let (n, m) = (old.len(), new.len());
    let width = m + 1;
    let mut table = vec![0u32; (n + 1) * width];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i * width + j] = if old[i] == new[j] {
                table[(i + 1) * width + j + 1] + 1
            } else {
                table[(i + 1) * width + j].max(table[i * width + j + 1])
            };
        }
    }
    let mut ops = Vec::with_capacity(n + m);
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && old[i] == new[j] {
            ops.push(Op::Eq(old[i]));
            i += 1;
            j += 1;
        } else if i < n && (j == m || table[(i + 1) * width + j] >= table[i * width + j + 1]) {
            ops.push(Op::Del(old[i]));
            i += 1;
        } else {
            ops.push(Op::Ins(new[j]));
            j += 1;
        }
    }
    ops
}

/// `[start, end)` op ranges per hunk: every change with [`CONTEXT`] equal
/// lines around it, adjacent/overlapping windows merged.
fn hunk_ranges(ops: &[Op<'_>]) -> Vec<(usize, usize)> {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for (at, op) in ops.iter().enumerate() {
        if matches!(op, Op::Eq(_)) {
            continue;
        }
        let start = at.saturating_sub(CONTEXT);
        let end = (at + CONTEXT + 1).min(ops.len());
        match ranges.last_mut() {
            Some(last) if start <= last.1 => last.1 = end,
            _ => ranges.push((start, end)),
        }
    }
    ranges
}

/// git prints a 1-based start, and the line BEFORE for an empty side.
fn hunk_start(at: usize, count: usize) -> usize {
    if count == 0 {
        at
    } else {
        at + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_one_line_change_is_one_hunk_with_three_lines_of_context() {
        let old = "a\nb\nc\nd\ne\nf\ng\nh\n";
        let new = "a\nb\nc\nd\nE\nf\ng\nh\n";
        assert_eq!(
            unified_diff("src/x.rs", Some(old), new),
            "--- a/src/x.rs\n+++ b/src/x.rs\n@@ -2,7 +2,7 @@\n b\n c\n d\n-e\n+E\n f\n g\n h\n"
        );
    }

    #[test]
    fn a_new_file_diffs_against_dev_null_and_identical_text_is_empty() {
        assert_eq!(
            unified_diff("new.txt", None, "one\ntwo\n"),
            "--- /dev/null\n+++ b/new.txt\n@@ -0,0 +1,2 @@\n+one\n+two\n"
        );
        assert_eq!(unified_diff("same.txt", Some("x\n"), "x\n"), "");
    }

    #[test]
    fn distant_changes_become_separate_hunks_and_near_ones_merge() {
        let old: String = (1..=20).map(|n| format!("l{n}\n")).collect();
        let mut far = old.replace("l2\n", "L2\n");
        far = far.replace("l18\n", "L18\n");
        let patch = unified_diff("f", Some(&old), &far);
        assert_eq!(patch.matches("@@ -").count(), 2, "{patch}");
        let near = old.replace("l5\n", "L5\n").replace("l9\n", "L9\n");
        let patch = unified_diff("f", Some(&old), &near);
        assert_eq!(patch.matches("@@ -").count(), 1, "{patch}");
        assert!(patch.contains("-l5\n+L5\n l6\n l7\n l8\n-l9\n+L9\n"), "{patch}");
    }

    #[test]
    fn a_deletion_at_the_end_keeps_its_leading_context() {
        assert_eq!(
            unified_diff("f", Some("a\nb\nc\n"), "a\n"),
            "--- a/f\n+++ b/f\n@@ -1,3 +1,1 @@\n a\n-b\n-c\n"
        );
    }

    #[test]
    fn over_the_lcs_budget_the_middle_is_one_replacement_hunk() {
        let old: String = (0..3000).map(|n| format!("o{n}\n")).collect();
        let new: String = (0..3000).map(|n| format!("n{n}\n")).collect();
        let patch = unified_diff("big", Some(&old), &new);
        assert_eq!(patch.matches("@@ -").count(), 1);
        assert!(patch.contains("@@ -1,3000 +1,3000 @@\n-o0\n"));
        assert!(patch.ends_with("+n2999\n"));
    }

    #[test]
    fn truncation_cuts_on_line_boundaries_and_counts_the_rest() {
        let patch = "--- a/f\n+++ b/f\n@@ -1,3 +1,3 @@\n-a\n+b\n c\n";
        assert_eq!(truncate_unified_diff(patch, 200, 16_384), (patch.to_string(), 0));
        // Line cap: the first three lines, three dropped.
        assert_eq!(
            truncate_unified_diff(patch, 3, 16_384),
            ("--- a/f\n+++ b/f\n@@ -1,3 +1,3 @@\n".to_string(), 3)
        );
        // Byte cap: a line that would cross it is dropped WHOLE, never split.
        assert_eq!(
            truncate_unified_diff(patch, 200, 20),
            ("--- a/f\n+++ b/f\n".to_string(), 4)
        );
        // A cap smaller than the first line keeps nothing.
        assert_eq!(truncate_unified_diff(patch, 200, 3), (String::new(), 6));
        // No trailing newline: the last partial line still counts as one.
        assert_eq!(truncate_unified_diff("x\ny", 1, 100), ("x\n".to_string(), 1));
        assert_eq!(truncate_unified_diff("", 1, 100), (String::new(), 0));
    }
}
