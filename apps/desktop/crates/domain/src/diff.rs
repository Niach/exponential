//! EXP-895 — the ONE diff model every client renders, and the ONE parser that
//! builds it. Hand-mirrored ×4 (TS `@exp/domain-contract` `src/diff.ts`, iOS
//! `ExpCore/Sources/Domain/Diff.swift`, Android `domain/Diff.kt`, this module)
//! and byte-locked by `packages/domain-contract/fixtures/diff/cases.json` +
//! `fixtures/diff/summary.json`, which every client's test replays.
//!
//! Three producers feed the same model:
//!   1. `git diff` output from the desktop worktree (`diff --git` sections),
//!   2. the steer relay's per-call tool diffs (bare `--- a/x` / `+++ b/x`
//!      sections with no `diff --git` line, optionally cut with a trailing
//!      `\ N more lines truncated` marker),
//!   3. GitHub's PullFile `patch` (hunks only; the path and status arrive
//!      beside it, never inside it) — [`parse_patch`] /
//!      [`DiffStatus::from_pull_file`].
//!
//! The parse rules below ARE the contract; a change here is a change on all
//! four clients. The projection [`render_diff`] is what the fixture freezes,
//! so every platform can compare one array of strings instead of a whole
//! object graph.
//!
//! gpui-free, like the rest of `domain`. The serde shape is the TS model's
//! (camelCase keys, absent optionals) so a `Diff` round-trips through JSON
//! with the contract's own field names.

use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

// ── Model ───────────────────────────────────────────────────────────────────

/// Every line number and count saturates here (`i32::MAX`) — the natives carry
/// 32-bit counters and a hostile `@@` header must never wrap one.
pub const DIFF_LINE_MAX: u32 = 2147483647;

/// What happened to a file, in the contract's vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffStatus {
    Added,
    Removed,
    Modified,
    Renamed,
    Copied,
}

impl DiffStatus {
    /// The canonical wire value (what [`render_diff`] prints).
    pub fn as_str(&self) -> &'static str {
        match self {
            DiffStatus::Added => "added",
            DiffStatus::Removed => "removed",
            DiffStatus::Modified => "modified",
            DiffStatus::Renamed => "renamed",
            DiffStatus::Copied => "copied",
        }
    }

    /// GitHub's file status vocabulary → ours. `changed`, `unchanged` and
    /// anything a future API adds read as `modified`.
    pub fn from_pull_file(status: &str) -> DiffStatus {
        match status {
            "added" => DiffStatus::Added,
            "removed" => DiffStatus::Removed,
            "renamed" => DiffStatus::Renamed,
            "copied" => DiffStatus::Copied,
            _ => DiffStatus::Modified,
        }
    }
}

/// A single unified-diff line's role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffLineKind {
    Add,
    Del,
    Context,
    /// Unified-diff metadata (`\ No newline at end of file`): numbered on
    /// neither side, counted against neither side's line budget.
    Meta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub kind: DiffLineKind,
    /// 1-based line number on the old side; absent on `add`/`meta`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_no: Option<u32>,
    /// 1-based line number on the new side; absent on `del`/`meta`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_no: Option<u32>,
    /// The line's content, its one-character sign stripped.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    /// The verbatim `@@ … @@` line, section heading and all.
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffFile {
    pub path: String,
    /// Only on `renamed`/`copied`: where the file came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_path: Option<String>,
    pub status: DiffStatus,
    pub additions: u32,
    pub deletions: u32,
    pub binary: bool,
    pub hunks: Vec<DiffHunk>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diff {
    pub files: Vec<DiffFile>,
    /// Lines the PUBLISHER dropped, read back off its `\ N more lines
    /// truncated` marker (EXP-786). `None` when the diff is whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub truncated_lines: Option<u32>,
}

// ── Patterns ────────────────────────────────────────────────────────────────

/// The one marker the steer relay appends to a cut patch (EXP-786); web
/// `splitTruncatedDiff` and desktop `truncated_marker_count` spell it the
/// same way. Anchored to the END of the text: only a TRAILING marker counts.
/// `[0-9]` rather than `\d`, which is Unicode-wide in Rust and ASCII-only in
/// the JS twin.
fn truncation_marker() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?:^|\n)\\ ([0-9]+) more lines? truncated\s*$")
            .expect("the truncation marker is a valid regex")
    })
}

fn hunk_header_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"^@@ -([0-9]+)(?:,([0-9]+))? \+([0-9]+)(?:,([0-9]+))? @@")
            .expect("the hunk header is a valid regex")
    })
}

/// `diff --git "a/x" "b/y"` — the fully quoted pair, which parses exactly.
fn diff_git_quoted_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r#"^(?:"[^"]*"|\S+)\s+"([^"]*)"$"#)
            .expect("the quoted diff --git pair is a valid regex")
    })
}

// Path precedence: a higher-ranked source overwrites a lower-ranked one, and
// never the other way round. `rename to`/`copy to` name the destination
// outright, `+++` is the new side, `---` the old side (a fallback for a diff
// that never reaches its `+++`), `diff --git`'s b-side is the last resort
// because a path with a space makes that line ambiguous.
const RANK_NONE: i32 = -1;
const RANK_DIFF_GIT: i32 = 0;
const RANK_OLD: i32 = 1;
const RANK_NEW: i32 = 2;
const RANK_RENAME: i32 = 3;

struct Building {
    file: DiffFile,
    path_rank: i32,
}

fn blank(path: &str, status: DiffStatus) -> Building {
    Building {
        file: DiffFile {
            path: path.to_string(),
            previous_path: None,
            status,
            additions: 0,
            deletions: 0,
            binary: false,
            hunks: Vec::new(),
        },
        path_rank: RANK_NONE,
    }
}

/// A digit run → a counter, saturating at [`DIFF_LINE_MAX`] rather than
/// wrapping (the JS twin's `clamp` over `Number(...)`). A run too long for a
/// `u64` is by definition past the ceiling.
fn clamp_digits(raw: &str) -> u32 {
    match raw.parse::<u64>() {
        Ok(n) if n <= DIFF_LINE_MAX as u64 => n as u32,
        _ => DIFF_LINE_MAX,
    }
}

/// Advance a 1-based line counter, saturating rather than wrapping.
fn step(n: u32) -> u32 {
    if n >= DIFF_LINE_MAX {
        DIFF_LINE_MAX
    } else {
        n + 1
    }
}

/// A `---`/`+++`/`diff --git` payload → a display path: cut at the first TAB
/// (GNU diff's timestamp column), then unwrap surrounding double quotes (git
/// quotes a path carrying control or non-ASCII bytes). The `a/`/`b/` prefix
/// is stripped by [`strip_ab`] — only where git actually writes one.
fn cut_path(raw: &str) -> &str {
    let mut s = match raw.find('\t') {
        Some(tab) => &raw[..tab],
        None => raw,
    };
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s = &s[1..s.len() - 1];
    }
    s
}

/// Drop the one `a/`/`b/` prefix git puts on `---`, `+++` and `diff --git`
/// paths. NOT applied to `rename from`/`rename to`/`copy from`/`copy to`,
/// which git writes bare — stripping there would eat a real top-level `a/`
/// directory.
fn strip_ab(s: &str) -> &str {
    if s.starts_with("a/") || s.starts_with("b/") {
        &s[2..]
    } else {
        s
    }
}

/// The b-side of a `diff --git <a> <b>` line. Quoted pairs parse exactly;
/// otherwise the last ` b/` wins (git's own ambiguity — an unquoted path with
/// a space cannot be split reliably, which is why this is the lowest rank).
fn diff_git_new_path(rest: &str) -> Option<&str> {
    if let Some(caps) = diff_git_quoted_pattern().captures(rest) {
        return Some(strip_ab(
            caps.get(1).expect("group 1 always matches").as_str(),
        ));
    }
    if let Some(at) = rest.rfind(" b/") {
        return Some(&rest[at + 3..]);
    }
    let trimmed = rest.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(strip_ab(trimmed))
    }
}

struct HunkHead {
    old_start: u32,
    old_lines: u32,
    new_start: u32,
    new_lines: u32,
}

fn parse_hunk_header(line: &str) -> Option<HunkHead> {
    let caps = hunk_header_pattern().captures(line)?;
    Some(HunkHead {
        old_start: clamp_digits(caps.get(1).expect("group 1 always matches").as_str()),
        // A count the header omits is 1 — `@@ -1 +1 @@` is one line each side.
        old_lines: caps.get(2).map_or(1, |m| clamp_digits(m.as_str())),
        new_start: clamp_digits(caps.get(3).expect("group 3 always matches").as_str()),
        new_lines: caps.get(4).map_or(1, |m| clamp_digits(m.as_str())),
    })
}

// ── The parser ──────────────────────────────────────────────────────────────

fn set_path(file: &mut Building, path: &str, rank: i32) {
    if rank >= file.path_rank {
        file.file.path = path.to_string();
        file.path_rank = rank;
    }
}

/// The one state machine behind [`parse_diff`] and [`parse_patch`].
///
/// `seed` = the caller already knows the path and status (a GitHub patch), so
/// every header line is ignored (except the binary marker) and no `---`/
/// `diff --git` line may ever open a second file.
fn parse_sections(text: &str, seed: Option<(&str, DiffStatus)>) -> Vec<DiffFile> {
    let seeded = seed.is_some();
    let mut files: Vec<DiffFile> = Vec::new();
    let mut cur: Option<Building> = seed.map(|(path, status)| blank(path, status));
    // The hunk in progress, as an index into `cur`'s hunks.
    let mut hunk: Option<usize> = None;
    // Signed: a context line inside a hunk whose counts already ran out on one
    // side takes the other side below zero in the JS twin, and only `> 0` is
    // ever read back.
    let mut rem_old: i64 = 0;
    let mut rem_new: i64 = 0;
    let mut old_no: u32 = 0;
    let mut new_no: u32 = 0;

    for raw in text.split('\n') {
        // 1. `diff --git` starts the next file unconditionally — even mid-hunk,
        //    where a truncated patch can leave us.
        if !seeded && raw.starts_with("diff --git ") {
            let mut next = blank("", DiffStatus::Modified);
            if let Some(path) = diff_git_new_path(&raw["diff --git ".len()..]) {
                let path = path.to_string();
                set_path(&mut next, &path, RANK_DIFF_GIT);
            }
            if let Some(done) = cur.replace(next) {
                files.push(done.file);
            }
            hunk = None;
            rem_old = 0;
            rem_new = 0;
            continue;
        }

        // 2. A hunk header. Body lines always carry a sign, so a line literally
        //    starting with `@@` is unambiguous.
        if raw.starts_with("@@") {
            // A `@@` line, and a new file, always end the hunk in progress.
            hunk = None;
            rem_old = 0;
            rem_new = 0;
            let head = match parse_hunk_header(raw) {
                Some(head) => head,
                None => continue,
            };
            let file = cur.get_or_insert_with(|| blank("", DiffStatus::Modified));
            file.file.hunks.push(DiffHunk {
                old_start: head.old_start,
                old_lines: head.old_lines,
                new_start: head.new_start,
                new_lines: head.new_lines,
                header: raw.to_string(),
                lines: Vec::new(),
            });
            hunk = Some(file.file.hunks.len() - 1);
            rem_old = head.old_lines as i64;
            rem_new = head.new_lines as i64;
            old_no = head.old_start;
            new_no = head.new_start;
            continue;
        }

        if let Some(index) = hunk {
            let file = cur.as_mut().expect("a hunk in progress implies a file");
            // 3. `\ No newline at end of file`: unified-diff metadata. Kept as
            //    a row (a reader wants to see it), numbered on neither side,
            //    and it never consumes a count — so it may legally trail a hunk
            //    whose counts are already spent, as it does when BOTH sides
            //    lack the final newline.
            if raw.starts_with('\\') {
                let text = match raw.strip_prefix("\\ ") {
                    Some(text) => text,
                    None => raw.strip_prefix('\\').unwrap_or(raw),
                };
                file.file.hunks[index].lines.push(DiffLine {
                    kind: DiffLineKind::Meta,
                    old_no: None,
                    new_no: None,
                    text: text.to_string(),
                });
                continue;
            }

            // 4. The hunk body, bounded by the header's counts. Past them the
            //    hunk is over, whatever the next line looks like — that is what
            //    lets a bare steer diff start its next file on a plain
            //    `--- a/…`.
            if rem_old > 0 || rem_new > 0 {
                let sign = raw.as_bytes().first().copied();
                match sign {
                    Some(b'+') => {
                        file.file.additions = file.file.additions.saturating_add(1);
                        file.file.hunks[index].lines.push(DiffLine {
                            kind: DiffLineKind::Add,
                            old_no: None,
                            new_no: Some(new_no),
                            text: raw[1..].to_string(),
                        });
                        new_no = step(new_no);
                        rem_new -= 1;
                        continue;
                    }
                    Some(b'-') => {
                        file.file.deletions = file.file.deletions.saturating_add(1);
                        file.file.hunks[index].lines.push(DiffLine {
                            kind: DiffLineKind::Del,
                            old_no: Some(old_no),
                            new_no: None,
                            text: raw[1..].to_string(),
                        });
                        old_no = step(old_no);
                        rem_old -= 1;
                        continue;
                    }
                    // A context line is ` ` + content; a producer that trimmed
                    // trailing whitespace emits the empty string for an empty
                    // context line, and inside a hunk with counts left that is
                    // exactly what it means.
                    Some(b' ') | None => {
                        file.file.hunks[index].lines.push(DiffLine {
                            kind: DiffLineKind::Context,
                            old_no: Some(old_no),
                            new_no: Some(new_no),
                            text: if raw.is_empty() {
                                String::new()
                            } else {
                                raw[1..].to_string()
                            },
                        });
                        old_no = step(old_no);
                        new_no = step(new_no);
                        rem_old -= 1;
                        rem_new -= 1;
                        continue;
                    }
                    // Anything else inside a hunk means the counts lied: end
                    // the hunk and let the line be read as a header below.
                    _ => {
                        hunk = None;
                        rem_old = 0;
                        rem_new = 0;
                    }
                }
            }
        }

        if seeded {
            let file = cur.as_mut().expect("a seeded parse always has a file");
            if file.file.hunks.is_empty()
                && (raw.starts_with("Binary files ") || raw.starts_with("GIT binary patch"))
            {
                file.file.binary = true;
            }
            continue;
        }

        // 5. The header region. `---` is the one header line that may also OPEN
        //    a file: a bare steer section has no `diff --git` to announce it.
        if raw.starts_with("--- ") || raw == "---" {
            let reopen = match cur.as_ref() {
                Some(file) => !file.file.hunks.is_empty(),
                None => true,
            };
            if reopen {
                if let Some(done) = cur.replace(blank("", DiffStatus::Modified)) {
                    files.push(done.file);
                }
                hunk = None;
                rem_old = 0;
                rem_new = 0;
            }
            let payload = if raw.len() > 4 { &raw[4..] } else { "" };
            let file = cur.as_mut().expect("the `---` branch always has a file");
            if payload == "/dev/null" {
                file.file.status = DiffStatus::Added;
            } else if !payload.is_empty() {
                let path = strip_ab(cut_path(payload)).to_string();
                set_path(file, &path, RANK_OLD);
            }
            continue;
        }
        let file = match cur.as_mut() {
            Some(file) => file,
            None => continue,
        };
        // Everything below is honoured only BEFORE the first hunk of a file —
        // past it these words are just content that lost its sign.
        if !file.file.hunks.is_empty() {
            continue;
        }
        if let Some(payload) = raw.strip_prefix("+++ ") {
            if payload == "/dev/null" {
                file.file.status = DiffStatus::Removed;
            } else if !payload.is_empty() {
                let path = strip_ab(cut_path(payload)).to_string();
                set_path(file, &path, RANK_NEW);
            }
        } else if raw.starts_with("new file mode") {
            file.file.status = DiffStatus::Added;
        } else if raw.starts_with("deleted file mode") {
            file.file.status = DiffStatus::Removed;
        } else if let Some(payload) = raw.strip_prefix("rename from ") {
            file.file.previous_path = Some(cut_path(payload).to_string());
            file.file.status = DiffStatus::Renamed;
        } else if let Some(payload) = raw.strip_prefix("rename to ") {
            let path = cut_path(payload).to_string();
            set_path(file, &path, RANK_RENAME);
            file.file.status = DiffStatus::Renamed;
        } else if let Some(payload) = raw.strip_prefix("copy from ") {
            file.file.previous_path = Some(cut_path(payload).to_string());
            file.file.status = DiffStatus::Copied;
        } else if let Some(payload) = raw.strip_prefix("copy to ") {
            let path = cut_path(payload).to_string();
            set_path(file, &path, RANK_RENAME);
            file.file.status = DiffStatus::Copied;
        } else if raw.starts_with("Binary files ") || raw.starts_with("GIT binary patch") {
            file.file.binary = true;
        }
    }

    if let Some(done) = cur {
        files.push(done.file);
    }
    files
}

/// Read any of the three forms into the model, auto-detected:
///
/// - a full `git diff` (`diff --git` sections),
/// - bare steer sections that start straight at `--- a/x` / `+++ b/x`,
/// - hunks-only text (the first non-blank line is a `@@` header) → ONE file
///   with an EMPTY path and status `modified`; a caller that knows the path
///   uses [`parse_patch`] instead.
///
/// Garbage, whitespace and the empty string all yield no files.
pub fn parse_diff(text: &str) -> Diff {
    let empty = Diff {
        files: Vec::new(),
        truncated_lines: None,
    };
    if text.is_empty() {
        return empty;
    }
    let mut body = text;
    let mut truncated_lines: Option<u32> = None;
    if let Some(caps) = truncation_marker().captures(text) {
        let whole = caps.get(0).expect("group 0 always matches");
        body = &text[..whole.start()];
        truncated_lines = Some(clamp_digits(
            caps.get(1).expect("group 1 always matches").as_str(),
        ));
    }
    let files: Vec<DiffFile> = parse_sections(body, None)
        .into_iter()
        // A section that named neither a path nor a hunk is noise, not a file.
        .filter(|file| !file.path.is_empty() || !file.hunks.is_empty() || file.binary)
        .collect();
    if files.is_empty() {
        return empty;
    }
    Diff {
        files,
        truncated_lines,
    }
}

/// A hunks-only patch whose path and status the CALLER knows (GitHub's
/// PullFile, the desktop's per-file `git diff` wrappers). Nothing in `patch`
/// may change either one. A missing or empty patch is a file with no hunks —
/// binary, too large for GitHub to send, or a pure rename.
pub fn parse_patch(path: &str, status: DiffStatus, patch: Option<&str>) -> DiffFile {
    let hunkless = || DiffFile {
        path: path.to_string(),
        previous_path: None,
        status,
        additions: 0,
        deletions: 0,
        binary: false,
        hunks: Vec::new(),
    };
    let patch = match patch {
        Some(patch) if !patch.is_empty() => patch,
        _ => return hunkless(),
    };
    parse_sections(patch, Some((path, status)))
        .into_iter()
        .next()
        .unwrap_or_else(hunkless)
}

// ── Derivations ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Totals {
    pub files: usize,
    pub additions: u32,
    pub deletions: u32,
}

pub fn totals(files: &[DiffFile]) -> Totals {
    let mut additions: u32 = 0;
    let mut deletions: u32 = 0;
    for file in files {
        additions = additions.saturating_add(file.additions);
        deletions = deletions.saturating_add(file.deletions);
    }
    Totals {
        files: files.len(),
        additions,
        deletions,
    }
}

/// Fold sections that name the SAME path into one file, in order of first
/// appearance. A publisher may emit one section per edit, so the same file
/// arrives several times in one transcript; the reader wants one card.
/// Hunks concatenate in arrival order, counts sum, and the LATER section's
/// status, binary flag and (when it has one) `previous_path` win.
pub fn merge_files_by_path(files: &[DiffFile]) -> Vec<DiffFile> {
    let mut out: Vec<DiffFile> = Vec::new();
    let mut at: HashMap<&str, usize> = HashMap::new();
    for file in files {
        match at.get(file.path.as_str()) {
            None => {
                at.insert(file.path.as_str(), out.len());
                out.push(file.clone());
            }
            Some(&seen) => {
                let target = &mut out[seen];
                target.hunks.extend(file.hunks.iter().cloned());
                target.additions = target.additions.saturating_add(file.additions);
                target.deletions = target.deletions.saturating_add(file.deletions);
                target.status = file.status;
                target.binary = file.binary;
                if let Some(previous) = file
                    .previous_path
                    .as_deref()
                    .filter(|previous| !previous.is_empty())
                {
                    target.previous_path = Some(previous.to_string());
                }
            }
        }
    }
    out
}

/// Unchanged lines above a file's FIRST hunk — the count a "show more"
/// affordance offers to expand.
pub fn unchanged_before(first: &DiffHunk) -> u32 {
    first.new_start.saturating_sub(1)
}

/// Unchanged lines between two consecutive hunks of one file.
pub fn unchanged_between(prev: &DiffHunk, next: &DiffHunk) -> u32 {
    // Widened: two saturated counters sum past `u32::MAX` on their own.
    let after = prev.new_start as u64 + prev.new_lines as u64;
    (next.new_start as u64).saturating_sub(after) as u32
}

pub fn unchanged_label(n: u32) -> String {
    format!("{n} unchanged {}", if n == 1 { "line" } else { "lines" })
}

pub fn additions_label(n: u32) -> String {
    format!("+{n}")
}

/// U+2212 MINUS SIGN, not a hyphen: the deletion count sits beside `+n` in a
/// proportional font and a hyphen reads a full notch lighter.
pub fn deletions_label(n: u32) -> String {
    format!("\u{2212}{n}")
}

pub fn summary_label(files: usize, additions: u32, deletions: u32) -> String {
    if files == 0 {
        return "No changes".to_string();
    }
    format!(
        "{files} {} {} {}",
        if files == 1 { "file" } else { "files" },
        additions_label(additions),
        deletions_label(deletions)
    )
}

/// The byte-lock projection: one string per row, the whole [`Diff`] flattened.
/// `fixtures/diff/cases.json` stores exactly this, so every platform compares
/// a list of strings instead of reimplementing structural equality.
/// Deliberately ASCII (an ASCII `-` for the deletion count, unlike
/// [`deletions_label`]) and deliberately delimited (`|…|` around content) so
/// trailing whitespace in a diff line survives the round trip.
pub fn render_diff(diff: &Diff) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(truncated) = diff.truncated_lines {
        out.push(format!("truncated {truncated}"));
    }
    for file in &diff.files {
        let from = match file.previous_path.as_deref() {
            Some(previous) if !previous.is_empty() => format!(" <- {previous}"),
            _ => String::new(),
        };
        let binary = if file.binary { " binary" } else { "" };
        out.push(format!(
            "file {} {}{from}{binary} +{} -{}",
            file.status.as_str(),
            file.path,
            file.additions,
            file.deletions
        ));
        for hunk in &file.hunks {
            out.push(format!(
                "hunk {},{} {},{} |{}|",
                hunk.old_start, hunk.old_lines, hunk.new_start, hunk.new_lines, hunk.header
            ));
            for line in &hunk.lines {
                let old = line
                    .old_no
                    .map_or_else(|| "-".to_string(), |n| n.to_string());
                let next = line
                    .new_no
                    .map_or_else(|| "-".to_string(), |n| n.to_string());
                let tag = match line.kind {
                    DiffLineKind::Context => "ctx",
                    DiffLineKind::Add => "add",
                    DiffLineKind::Del => "del",
                    DiffLineKind::Meta => "meta",
                };
                out.push(format!("{tag} {old} {next} |{}|", line.text));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The contract fixtures, byte-locked ×4 (web `diff.test.ts`, iOS
    /// `DiffTests`, Android `DiffTest`) — the SAME four test names everywhere.
    const CASES: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/diff/cases.json");
    const SUMMARIES: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/diff/summary.json");

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureCase {
        name: String,
        form: String,
        input: String,
        #[serde(default)]
        path: Option<String>,
        #[serde(default)]
        status: Option<DiffStatus>,
        expected: Vec<String>,
        #[serde(default)]
        expected_merged: Option<Vec<String>>,
        summary: String,
        unchanged: Vec<u32>,
    }

    #[derive(Deserialize)]
    struct FixtureSummary {
        files: usize,
        additions: u32,
        deletions: u32,
        expected: String,
    }

    fn cases() -> Vec<FixtureCase> {
        serde_json::from_str(CASES).expect("the diff fixture parses")
    }

    fn summaries() -> Vec<FixtureSummary> {
        serde_json::from_str(SUMMARIES).expect("the summary fixture parses")
    }

    /// `form: "hunks"` WITH a `path` is the GitHub PullFile shape (the path and
    /// status arrive beside the patch); every other case auto-detects.
    fn parse_case(entry: &FixtureCase) -> Diff {
        if entry.form == "hunks" {
            if let Some(path) = entry.path.as_deref() {
                return Diff {
                    files: vec![parse_patch(
                        path,
                        entry.status.unwrap_or(DiffStatus::Modified),
                        Some(&entry.input),
                    )],
                    truncated_lines: None,
                };
            }
        }
        parse_diff(&entry.input)
    }

    /// The FIRST file's `[unchanged_before(h0), unchanged_between(h0, h1), …]`.
    fn unchanged_run(diff: &Diff) -> Vec<u32> {
        let first = match diff.files.first() {
            Some(first) => first,
            None => return Vec::new(),
        };
        first
            .hunks
            .iter()
            .enumerate()
            .map(|(index, hunk)| {
                if index == 0 {
                    unchanged_before(hunk)
                } else {
                    unchanged_between(&first.hunks[index - 1], hunk)
                }
            })
            .collect()
    }

    /// every fixture case parses byte exact
    #[test]
    fn every_fixture_case_parses_byte_exact() {
        for entry in &cases() {
            let diff = parse_case(entry);
            assert_eq!(render_diff(&diff), entry.expected, "{}", entry.name);
            if let Some(expected) = &entry.expected_merged {
                let merged = Diff {
                    files: merge_files_by_path(&diff.files),
                    truncated_lines: diff.truncated_lines,
                };
                assert_eq!(render_diff(&merged), *expected, "{} (merged)", entry.name);
            }
        }
    }

    /// the fixture covers every input form
    #[test]
    fn the_fixture_covers_every_input_form() {
        let cases = cases();
        assert!(cases.len() >= 18);
        let mut forms: Vec<&str> = cases.iter().map(|entry| entry.form.as_str()).collect();
        forms.sort_unstable();
        forms.dedup();
        assert_eq!(forms, vec!["bare", "git", "hunks", "none"]);
        // A `none` case is the empty parse; every other form yields files.
        for entry in &cases {
            let files = parse_case(entry).files;
            if entry.form == "none" {
                assert!(files.is_empty(), "{}", entry.name);
            } else {
                assert!(!files.is_empty(), "{}", entry.name);
            }
        }
    }

    /// summary label matches every fixture case
    #[test]
    fn summary_label_matches_every_fixture_case() {
        for entry in &cases() {
            let totals = totals(&parse_case(entry).files);
            assert_eq!(
                summary_label(totals.files, totals.additions, totals.deletions),
                entry.summary,
                "{}",
                entry.name
            );
        }
        let summaries = summaries();
        for row in &summaries {
            assert_eq!(
                summary_label(row.files, row.additions, row.deletions),
                row.expected
            );
        }
        assert!(summaries.len() >= 4);
    }

    /// unchanged line counts match every fixture case
    #[test]
    fn unchanged_line_counts_match_every_fixture_case() {
        for entry in &cases() {
            assert_eq!(
                unchanged_run(&parse_case(entry)),
                entry.unchanged,
                "{}",
                entry.name
            );
        }
    }

    fn file(path: &str) -> DiffFile {
        DiffFile {
            path: path.to_string(),
            previous_path: None,
            status: DiffStatus::Modified,
            additions: 1,
            deletions: 1,
            binary: false,
            hunks: Vec::new(),
        }
    }

    #[test]
    fn merge_folds_repeats_in_order_of_first_appearance() {
        let merged = merge_files_by_path(&[file("b.ts"), file("a.ts"), file("b.ts")]);
        let paths: Vec<&str> = merged.iter().map(|file| file.path.as_str()).collect();
        assert_eq!(paths, vec!["b.ts", "a.ts"]);
        assert_eq!(merged[0].additions, 2);
        assert_eq!(merged[0].deletions, 2);
    }

    #[test]
    fn merge_takes_the_later_status_binary_and_previous_path() {
        let later = DiffFile {
            status: DiffStatus::Renamed,
            binary: true,
            previous_path: Some("old.ts".to_string()),
            ..file("a.ts")
        };
        let merged = merge_files_by_path(&[file("a.ts"), later]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].status, DiffStatus::Renamed);
        assert!(merged[0].binary);
        assert_eq!(merged[0].previous_path.as_deref(), Some("old.ts"));
    }

    #[test]
    fn merge_concatenates_hunks_and_never_mutates_its_input() {
        let one = parse_patch("a.ts", DiffStatus::Modified, Some("@@ -1 +1 @@\n-a\n+A\n"));
        let two = parse_patch("a.ts", DiffStatus::Modified, Some("@@ -9 +9 @@\n-b\n+B\n"));
        let merged = merge_files_by_path(&[one.clone(), two]);
        assert_eq!(merged[0].hunks.len(), 2);
        assert_eq!(one.hunks.len(), 1);
        assert_eq!(one.additions, 1);
    }

    #[test]
    fn an_absent_patch_is_a_file_with_no_hunks() {
        for patch in [None, Some("")] {
            let file = parse_patch("logo.png", DiffStatus::Modified, patch);
            assert_eq!(
                file,
                DiffFile {
                    path: "logo.png".to_string(),
                    previous_path: None,
                    status: DiffStatus::Modified,
                    additions: 0,
                    deletions: 0,
                    binary: false,
                    hunks: Vec::new(),
                }
            );
        }
    }

    #[test]
    fn the_patch_never_renames_the_file_it_was_handed() {
        let file = parse_patch(
            "given.ts",
            DiffStatus::Added,
            Some("diff --git a/other.ts b/other.ts\n--- /dev/null\n+++ b/other.ts\n@@ -0,0 +1 @@\n+x\n"),
        );
        assert_eq!(file.path, "given.ts");
        assert_eq!(file.status, DiffStatus::Added);
        assert_eq!(file.additions, 1);
    }

    #[test]
    fn githubs_status_vocabulary_maps_onto_ours() {
        assert_eq!(DiffStatus::from_pull_file("added"), DiffStatus::Added);
        assert_eq!(DiffStatus::from_pull_file("removed"), DiffStatus::Removed);
        assert_eq!(DiffStatus::from_pull_file("renamed"), DiffStatus::Renamed);
        assert_eq!(DiffStatus::from_pull_file("copied"), DiffStatus::Copied);
        assert_eq!(DiffStatus::from_pull_file("changed"), DiffStatus::Modified);
        assert_eq!(
            DiffStatus::from_pull_file("unchanged"),
            DiffStatus::Modified
        );
        assert_eq!(
            DiffStatus::from_pull_file("something-new"),
            DiffStatus::Modified
        );
    }

    #[test]
    fn additions_deletions_and_unchanged_labels() {
        assert_eq!(additions_label(0), "+0");
        assert_eq!(additions_label(12), "+12");
        // U+2212 MINUS SIGN, never an ASCII hyphen.
        assert_eq!(deletions_label(4), "\u{2212}4");
        assert_eq!(deletions_label(4).chars().next(), Some('\u{2212}'));
        assert_eq!(unchanged_label(1), "1 unchanged line");
        assert_eq!(unchanged_label(12), "12 unchanged lines");
    }

    #[test]
    fn the_summary_label() {
        assert_eq!(summary_label(0, 0, 0), "No changes");
        assert_eq!(summary_label(0, 9, 9), "No changes");
        assert_eq!(summary_label(1, 2, 0), "1 file +2 \u{2212}0");
        assert_eq!(summary_label(3, 12, 4), "3 files +12 \u{2212}4");
    }

    #[test]
    fn unchanged_runs_never_go_negative() {
        let hunk = |new_start: u32, new_lines: u32| DiffHunk {
            old_start: new_start,
            old_lines: new_lines,
            new_start,
            new_lines,
            header: String::new(),
            lines: Vec::new(),
        };
        assert_eq!(unchanged_before(&hunk(1, 3)), 0);
        assert_eq!(unchanged_before(&hunk(0, 0)), 0);
        assert_eq!(unchanged_before(&hunk(40, 3)), 39);
        assert_eq!(unchanged_between(&hunk(1, 3), &hunk(20, 3)), 16);
        assert_eq!(unchanged_between(&hunk(1, 30), &hunk(20, 3)), 0);
        // Two saturated counters: the sum must not wrap.
        assert_eq!(
            unchanged_between(&hunk(DIFF_LINE_MAX, DIFF_LINE_MAX), &hunk(DIFF_LINE_MAX, 1)),
            0
        );
    }
}
