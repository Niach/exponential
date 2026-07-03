//! Pure unified-patch parsing + side-by-side row alignment (masterplan-v3
//! §7.8 bullet 1). gpui-free by design — unit-tested against real captured
//! patches in `diff/fixtures/`.
//!
//! Input is the GitHub "list PR files" `patch` string (hunks only, starting at
//! `@@`), but the parser also tolerates full `git diff` output (the
//! `diff --git`/`index`/`---`/`+++` preamble is skipped) so local fixtures and
//! future local-diff sources parse identically.
//!
//! Output layers:
//! 1. [`parse_patch`] → `Vec<Hunk>` with the spec-named row model
//!    (`Context { old_ln, new_ln, text }` / `Removed { old_ln, text }` /
//!    `Added { new_ln, text }`).
//! 2. [`align_file`] → flat `Vec<DisplayRow>` for the side-by-side view: hunk
//!    header rows + line rows of paired `Option<Cell>` (left = old, right =
//!    new; a removal/addition run is paired index-wise, the shorter side gets
//!    `None` filler so both columns stay row-aligned).
//!
//! Every cell carries an [`Anchor`] `{ filename, side, line }` NOW (§7.8:
//! read-only v1, but anchored so the future "comment on this line → GitHub
//! review comment" write-back is a pure addition, not a re-architecture).

/// Which side of the diff a line/anchor belongs to (§7.8: `Side /* Old | New */`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Old,
    New,
}

/// The stable per-line anchor for future review-comment write-back (§7.8).
/// `line` is 1-based in the file version named by `side`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anchor {
    pub filename: String,
    pub side: Side,
    pub line: u32,
}

/// One parsed patch line inside a hunk (spec row model, §7.8 bullet 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffRow {
    Context { old_ln: u32, new_ln: u32, text: String },
    Removed { old_ln: u32, text: String },
    Added { new_ln: u32, text: String },
}

/// One `@@ -a,b +c,d @@` hunk with its rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    /// The full `@@ … @@ <section>` header line, rendered verbatim (web
    /// `diff-view.tsx` renders the raw `@@` line).
    pub header: String,
    pub rows: Vec<DiffRow>,
}

/// Parse a unified patch (GitHub `patch` field or `git diff` body) into hunks.
///
/// Robustness rules (all hit by the fixtures/tests):
/// * Lines before the first `@@` (git preamble, including the leading
///   `diff --git`/`index`/`---`/`+++` block) are skipped.
/// * Once this file's hunks have started, a `diff --git` line ends parsing —
///   GitHub `patch` fields are single-file; a concatenated multi-file diff
///   only yields its first file's hunks.
/// * `\ No newline at end of file` markers are dropped (they annotate the
///   previous line; they are not content).
/// * Inside a hunk, an empty line counts as an empty **context** line (GitHub
///   emits truly empty strings for empty context lines).
/// * Hunk line counts (`old_count`/`new_count`) bound consumption; trailing
///   junk after a completed hunk is ignored until the next `@@`.
pub fn parse_patch(patch: &str) -> Vec<Hunk> {
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut current: Option<HunkBuilder> = None;

    for line in patch.split('\n') {
        if line.starts_with("diff --git ") && (current.is_some() || !hunks.is_empty()) {
            break; // next file in a concatenated diff — this file is done
        }
        if let Some(header) = parse_hunk_header(line) {
            if let Some(builder) = current.take() {
                hunks.push(builder.finish());
            }
            current = Some(HunkBuilder::new(header, line.to_string()));
            continue;
        }
        let Some(builder) = current.as_mut() else {
            continue; // preamble (index/---/+++/mode lines) before any hunk
        };
        if !builder.wants_more() {
            continue; // hunk complete; skip until the next @@ header
        }
        if line.starts_with('\\') {
            continue; // "\ No newline at end of file"
        }
        builder.push_line(line);
    }
    if let Some(builder) = current.take() {
        hunks.push(builder.finish());
    }
    hunks
}

/// `@@ -old_start[,old_count] +new_start[,new_count] @@ …` → the four numbers.
/// Omitted counts default to 1 per the unified-diff format.
fn parse_hunk_header(line: &str) -> Option<(u32, u32, u32, u32)> {
    let rest = line.strip_prefix("@@ -")?;
    let (old_part, rest) = rest.split_once(" +")?;
    let (new_part, _) = rest.split_once(" @@")?;
    let (old_start, old_count) = parse_start_count(old_part)?;
    let (new_start, new_count) = parse_start_count(new_part)?;
    Some((old_start, old_count, new_start, new_count))
}

fn parse_start_count(part: &str) -> Option<(u32, u32)> {
    match part.split_once(',') {
        Some((start, count)) => Some((start.parse().ok()?, count.parse().ok()?)),
        None => Some((part.parse().ok()?, 1)),
    }
}

struct HunkBuilder {
    hunk: Hunk,
    old_left: u32,
    new_left: u32,
    old_ln: u32,
    new_ln: u32,
}

impl HunkBuilder {
    fn new((old_start, old_count, new_start, new_count): (u32, u32, u32, u32), header: String) -> Self {
        Self {
            hunk: Hunk {
                old_start,
                old_count,
                new_start,
                new_count,
                header,
                rows: Vec::new(),
            },
            old_left: old_count,
            new_left: new_count,
            old_ln: old_start,
            new_ln: new_start,
        }
    }

    fn wants_more(&self) -> bool {
        self.old_left > 0 || self.new_left > 0
    }

    fn push_line(&mut self, line: &str) {
        let (marker, text) = match line.chars().next() {
            Some(c @ ('+' | '-' | ' ')) => (c, &line[1..]),
            // Empty string inside a hunk = empty context line (GitHub form).
            None => (' ', ""),
            // Unknown marker inside an incomplete hunk: treat as context so a
            // malformed producer degrades to visible-but-unstyled, never a
            // desynced line counter.
            Some(_) => (' ', line),
        };
        // saturating_add: a pathological header like `@@ -4294967295,… @@`
        // must degrade to pinned line numbers, never a debug-build overflow
        // panic (the parser accepts arbitrary patch text).
        match marker {
            '-' => {
                self.hunk.rows.push(DiffRow::Removed {
                    old_ln: self.old_ln,
                    text: text.to_string(),
                });
                self.old_ln = self.old_ln.saturating_add(1);
                self.old_left = self.old_left.saturating_sub(1);
            }
            '+' => {
                self.hunk.rows.push(DiffRow::Added {
                    new_ln: self.new_ln,
                    text: text.to_string(),
                });
                self.new_ln = self.new_ln.saturating_add(1);
                self.new_left = self.new_left.saturating_sub(1);
            }
            _ => {
                self.hunk.rows.push(DiffRow::Context {
                    old_ln: self.old_ln,
                    new_ln: self.new_ln,
                    text: text.to_string(),
                });
                self.old_ln = self.old_ln.saturating_add(1);
                self.new_ln = self.new_ln.saturating_add(1);
                self.old_left = self.old_left.saturating_sub(1);
                self.new_left = self.new_left.saturating_sub(1);
            }
        }
    }

    fn finish(self) -> Hunk {
        self.hunk
    }
}

// ---------------------------------------------------------------------------
// Side-by-side alignment
// ---------------------------------------------------------------------------

/// What a populated cell renders as (drives the gutter + background tint).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellKind {
    Context,
    Removed,
    Added,
}

/// One populated side of an aligned row. `anchor.line` is the 1-based line
/// number in that side's file version — it doubles as the rendered gutter
/// number, so the anchor can never drift from what the user sees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub kind: CellKind,
    pub anchor: Anchor,
    pub text: String,
}

/// One visual row of the side-by-side view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayRow {
    /// The verbatim `@@ … @@` header line, spanning both columns.
    HunkHeader { header: String },
    /// An aligned pair: left = old side, right = new side. `None` is the
    /// blank filler opposite an unpaired add/remove.
    Line {
        left: Option<Cell>,
        right: Option<Cell>,
    },
}

/// Flatten parsed hunks into aligned side-by-side rows for one file.
///
/// Pairing semantics (GitHub split-view parity): within a hunk, a maximal run
/// of `Removed` lines followed by a maximal run of `Added` lines is one change
/// block; row *i* of the block shows the *i*-th removal on the left against
/// the *i*-th addition on the right, and the longer run gets `None` filler on
/// the opposite side. Context lines occupy both sides.
pub fn align_file(filename: &str, hunks: &[Hunk]) -> Vec<DisplayRow> {
    let mut rows = Vec::new();
    for hunk in hunks {
        rows.push(DisplayRow::HunkHeader {
            header: hunk.header.clone(),
        });
        let mut pending_removed: Vec<Cell> = Vec::new();
        let mut pending_added: Vec<Cell> = Vec::new();

        let flush =
            |removed: &mut Vec<Cell>, added: &mut Vec<Cell>, rows: &mut Vec<DisplayRow>| {
                let count = removed.len().max(added.len());
                let mut removed_iter = removed.drain(..);
                let mut added_iter = added.drain(..);
                for _ in 0..count {
                    rows.push(DisplayRow::Line {
                        left: removed_iter.next(),
                        right: added_iter.next(),
                    });
                }
            };

        for row in &hunk.rows {
            match row {
                DiffRow::Context { old_ln, new_ln, text } => {
                    flush(&mut pending_removed, &mut pending_added, &mut rows);
                    rows.push(DisplayRow::Line {
                        left: Some(Cell {
                            kind: CellKind::Context,
                            anchor: Anchor {
                                filename: filename.to_string(),
                                side: Side::Old,
                                line: *old_ln,
                            },
                            text: text.clone(),
                        }),
                        right: Some(Cell {
                            kind: CellKind::Context,
                            anchor: Anchor {
                                filename: filename.to_string(),
                                side: Side::New,
                                line: *new_ln,
                            },
                            text: text.clone(),
                        }),
                    });
                }
                DiffRow::Removed { old_ln, text } => {
                    // A removal after additions started closes the block —
                    // unified diffs emit `---` then `+++` per block, so a new
                    // `-` means a new block.
                    if !pending_added.is_empty() {
                        flush(&mut pending_removed, &mut pending_added, &mut rows);
                    }
                    pending_removed.push(Cell {
                        kind: CellKind::Removed,
                        anchor: Anchor {
                            filename: filename.to_string(),
                            side: Side::Old,
                            line: *old_ln,
                        },
                        text: text.clone(),
                    });
                }
                DiffRow::Added { new_ln, text } => {
                    pending_added.push(Cell {
                        kind: CellKind::Added,
                        anchor: Anchor {
                            filename: filename.to_string(),
                            side: Side::New,
                            line: *new_ln,
                        },
                        text: text.clone(),
                    });
                }
            }
        }
        flush(&mut pending_removed, &mut pending_added, &mut rows);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real captured `git diff` of apps/web/src/lib/integrations/github-pr.ts
    /// (ef0a1aa → 40912b0): two hunks — a 2-removed/3-added block and a
    /// 1-removed/1-added block, both with context.
    const TS_PATCH: &str = include_str!("fixtures/github-pr-ts.patch");
    /// Real captured `git diff` of apps/desktop/crates/theme/src/lib.rs
    /// (045bb98~1 → 045bb98): a pure-addition hunk and a 1-removed/2-added hunk.
    const RS_PATCH: &str = include_str!("fixtures/theme-lib-rs.patch");

    /// GitHub `patch` fields start at `@@` with no git preamble — derive that
    /// form from the same fixture to prove both parse identically.
    fn github_form(patch: &str) -> String {
        let start = patch.find("@@").expect("fixture has a hunk");
        patch[start..].trim_end_matches('\n').to_string()
    }

    #[test]
    fn parses_git_preamble_and_github_form_identically() {
        let from_git = parse_patch(TS_PATCH);
        let from_github = parse_patch(&github_form(TS_PATCH));
        assert_eq!(from_git, from_github);
        assert_eq!(from_git.len(), 2);
    }

    #[test]
    fn ts_fixture_hunk_headers_and_counts() {
        let hunks = parse_patch(TS_PATCH);
        let h0 = &hunks[0];
        assert_eq!(
            (h0.old_start, h0.old_count, h0.new_start, h0.new_count),
            (26, 8, 26, 9)
        );
        assert_eq!(h0.header, "@@ -26,8 +26,9 @@ export interface CreatedPull {");
        // Row budget must be fully consumed: 8 old-side + 9 new-side lines.
        let old_rows = h0
            .rows
            .iter()
            .filter(|r| !matches!(r, DiffRow::Added { .. }))
            .count();
        let new_rows = h0
            .rows
            .iter()
            .filter(|r| !matches!(r, DiffRow::Removed { .. }))
            .count();
        assert_eq!(old_rows, 8);
        assert_eq!(new_rows, 9);

        let h1 = &hunks[1];
        assert_eq!(
            (h1.old_start, h1.old_count, h1.new_start, h1.new_count),
            (97, 7, 98, 7)
        );
    }

    #[test]
    fn line_numbers_advance_per_side() {
        let hunks = parse_patch(TS_PATCH);
        let rows = &hunks[0].rows;
        // First three rows are context 26/26, 27/27, 28/28 ("  number: number", "}", "").
        assert_eq!(
            rows[0],
            DiffRow::Context {
                old_ln: 26,
                new_ln: 26,
                text: "  number: number".into()
            }
        );
        assert!(matches!(rows[2], DiffRow::Context { old_ln: 28, new_ln: 28, ref text } if text.is_empty()));
        // Then the change block: removals number the OLD side 29,30 …
        assert!(matches!(rows[3], DiffRow::Removed { old_ln: 29, .. }));
        assert!(matches!(rows[4], DiffRow::Removed { old_ln: 30, .. }));
        // … additions number the NEW side 29,30,31.
        assert!(matches!(rows[5], DiffRow::Added { new_ln: 29, .. }));
        assert!(matches!(rows[6], DiffRow::Added { new_ln: 30, .. }));
        assert!(matches!(rows[7], DiffRow::Added { new_ln: 31, .. }));
        // Context after the block continues on both counters (old 31 / new 32).
        assert!(matches!(
            rows[8],
            DiffRow::Context { old_ln: 31, new_ln: 32, .. }
        ));
    }

    #[test]
    fn rs_fixture_pure_addition_hunk() {
        let hunks = parse_patch(RS_PATCH);
        assert_eq!(hunks.len(), 2);
        let adds: Vec<_> = hunks[0]
            .rows
            .iter()
            .filter(|r| matches!(r, DiffRow::Added { .. }))
            .collect();
        assert_eq!(adds.len(), 2, "hunk 0 adds `pub mod terminal;` + blank");
        assert!(hunks[0]
            .rows
            .iter()
            .all(|r| !matches!(r, DiffRow::Removed { .. })));
        assert!(
            matches!(&hunks[0].rows[3], DiffRow::Added { new_ln: 71, text } if text == "pub mod terminal;"),
            "unexpected: {:?}",
            hunks[0].rows[3]
        );
    }

    #[test]
    fn alignment_pairs_unequal_runs_with_filler() {
        let hunks = parse_patch(TS_PATCH);
        let rows = align_file("apps/web/src/lib/integrations/github-pr.ts", &hunks);
        // Row 0 = hunk header.
        assert!(matches!(&rows[0], DisplayRow::HunkHeader { header } if header.starts_with("@@ -26,8")));
        // 3 context rows, then the 2-removed/3-added block → 3 aligned rows.
        let block: Vec<_> = rows[4..7]
            .iter()
            .map(|r| match r {
                DisplayRow::Line { left, right } => (left.is_some(), right.is_some()),
                _ => panic!("expected line rows"),
            })
            .collect();
        assert_eq!(block, vec![(true, true), (true, true), (false, true)]);
        // The filler row's populated side is the third addition, new line 31.
        if let DisplayRow::Line { left, right } = &rows[6] {
            assert!(left.is_none());
            let cell = right.as_ref().unwrap();
            assert_eq!(cell.kind, CellKind::Added);
            assert_eq!(cell.anchor.side, Side::New);
            assert_eq!(cell.anchor.line, 31);
        }
    }

    #[test]
    fn alignment_context_rows_populate_both_sides_with_anchors() {
        let hunks = parse_patch(RS_PATCH);
        let rows = align_file("apps/desktop/crates/theme/src/lib.rs", &hunks);
        let DisplayRow::Line { left, right } = &rows[1] else {
            panic!("row 1 should be the first context line");
        };
        let (l, r) = (left.as_ref().unwrap(), right.as_ref().unwrap());
        assert_eq!(l.kind, CellKind::Context);
        assert_eq!(r.kind, CellKind::Context);
        assert_eq!(l.anchor.side, Side::Old);
        assert_eq!(r.anchor.side, Side::New);
        assert_eq!(l.anchor.filename, "apps/desktop/crates/theme/src/lib.rs");
        assert_eq!(l.anchor.line, 68);
        assert_eq!(r.anchor.line, 68);
        assert_eq!(l.text, r.text);
    }

    #[test]
    fn alignment_pure_addition_left_side_is_filler() {
        let hunks = parse_patch(RS_PATCH);
        let rows = align_file("f.rs", &hunks);
        let added: Vec<_> = rows
            .iter()
            .filter_map(|r| match r {
                DisplayRow::Line { left: None, right: Some(cell) } if cell.kind == CellKind::Added => {
                    Some(cell)
                }
                _ => None,
            })
            .collect();
        assert!(added.len() >= 2, "pure additions render right-only rows");
        assert!(added.iter().all(|c| c.anchor.side == Side::New));
    }

    #[test]
    fn every_populated_cell_carries_an_anchor_matching_its_gutter() {
        // §7.8 acceptance: per-line anchors present on every rendered line.
        for (name, patch) in [("a.ts", TS_PATCH), ("b.rs", RS_PATCH)] {
            for row in align_file(name, &parse_patch(patch)) {
                if let DisplayRow::Line { left, right } = row {
                    for (cell, side) in [(left, Side::Old), (right, Side::New)] {
                        if let Some(cell) = cell {
                            assert_eq!(cell.anchor.side, side);
                            assert_eq!(cell.anchor.filename, name);
                            assert!(cell.anchor.line > 0);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn interleaved_change_blocks_do_not_cross_pair() {
        // -a +b -c +d must pair (a,b) and (c,d), never (a,b),(c,·),(·,d) drift.
        let patch = "@@ -1,2 +1,2 @@\n-a\n+b\n-c\n+d";
        let rows = align_file("x", &parse_patch(patch));
        assert_eq!(rows.len(), 3); // header + 2 paired rows
        if let DisplayRow::Line { left, right } = &rows[1] {
            assert_eq!(left.as_ref().unwrap().text, "a");
            assert_eq!(right.as_ref().unwrap().text, "b");
        }
        if let DisplayRow::Line { left, right } = &rows[2] {
            assert_eq!(left.as_ref().unwrap().text, "c");
            assert_eq!(right.as_ref().unwrap().text, "d");
        }
    }

    #[test]
    fn no_newline_marker_and_missing_counts_are_handled() {
        let patch = "@@ -1 +1 @@\n-old\n+new\n\\ No newline at end of file";
        let hunks = parse_patch(patch);
        assert_eq!(hunks.len(), 1);
        assert_eq!((hunks[0].old_count, hunks[0].new_count), (1, 1));
        assert_eq!(hunks[0].rows.len(), 2);
    }

    #[test]
    fn empty_line_inside_hunk_is_empty_context() {
        // GitHub emits truly empty strings for empty context lines.
        let patch = "@@ -1,3 +1,3 @@\n a\n\n-b\n+c";
        let hunks = parse_patch(patch);
        assert_eq!(
            hunks[0].rows[1],
            DiffRow::Context {
                old_ln: 2,
                new_ln: 2,
                text: String::new()
            }
        );
    }

    #[test]
    fn empty_or_headerless_patch_yields_no_hunks() {
        assert!(parse_patch("").is_empty());
        assert!(parse_patch("Binary files a/x and b/x differ").is_empty());
    }

    #[test]
    fn pathological_u32_max_start_does_not_overflow() {
        // Attacker-shaped header: line counters must saturate, never panic.
        let patch = "@@ -4294967295,3 +4294967295,3 @@\n a\n b\n c";
        let hunks = parse_patch(patch);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].rows.len(), 3);
        assert!(matches!(
            hunks[0].rows[2],
            DiffRow::Context { old_ln: u32::MAX, new_ln: u32::MAX, .. }
        ));
    }

    #[test]
    fn stops_at_next_file_in_concatenated_diff() {
        let patch = format!("{TS_PATCH}\ndiff --git a/other b/other\n@@ -1 +1 @@\n-x\n+y");
        // Everything after `diff --git` belongs to another file — not ours.
        assert_eq!(parse_patch(&patch), parse_patch(TS_PATCH));
    }
}
