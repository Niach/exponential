//! #EXP-2204 (packages/exponential-ui/docs/ci-components-contract.md
//! §1-§3, §6): the pure rules of LogView/LogLines and the CodeBlock anchors +
//! blame, the TS reference's `src/ci-log.ts`; both replay
//! `fixtures/ci-log.json`.
//!
//! [`ci_status_entry`]: THE status table (glyph, spin, colours, a11y id).
//! [`parse_line_anchor`] / [`format_line_anchor`] / [`extend_line_anchor`]:
//! `L12` / `L6-L9`. [`log_model`]: raw lines → numbered content lines and
//! their `##[group]` sections; [`log_open_default`], [`log_rows`] (the flat
//! rows a LogLines body windows), [`log_search`] (code-point offsets into the
//! escape-free text), [`log_reveal`]. [`blame_blocks`]: per line its blame
//! block.

use serde_json::{json, Value};

/// The `ciStatus` enum, in catalogue order.
pub const CI_STATUSES: [&str; 6] = ["passed", "failed", "running", "queued", "cancelled", "skipped"];

/// style.json `ciStatus.spinMs`: one turn of the running glyph.
pub const CI_STATUS_SPIN_MS: u32 = 1000;

/// One row of THE status table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CiStatusEntry {
    pub status: &'static str,
    /// The icons.json concept (16 px).
    pub icon: &'static str,
    pub spin: bool,
    /// The mark colour (the neutral theme's `when: {status}` recipe value).
    pub color: &'static str,
    /// The text colour.
    pub text: &'static str,
    /// The a11y string id.
    pub label: &'static str,
}

impl CiStatusEntry {
    pub fn to_json(&self) -> Value {
        json!({"status": self.status, "icon": self.icon, "spin": self.spin, "color": self.color, "text": self.text, "label": self.label})
    }
}

const TABLE: [CiStatusEntry; 6] = [
    CiStatusEntry { status: "passed", icon: "ci-passed", spin: false, color: "$color.success", text: "$color.foreground", label: "statusPassed" },
    CiStatusEntry { status: "failed", icon: "ci-failed", spin: false, color: "$color.destructive", text: "$color.foreground", label: "statusFailed" },
    CiStatusEntry { status: "running", icon: "ci-running", spin: true, color: "$color.info", text: "$color.foreground", label: "statusRunning" },
    CiStatusEntry { status: "queued", icon: "ci-queued", spin: false, color: "$color.mutedForeground", text: "$color.foreground", label: "statusQueued" },
    CiStatusEntry { status: "cancelled", icon: "ci-skipped", spin: false, color: "$color.mutedForeground", text: "$color.mutedForeground", label: "statusCancelled" },
    CiStatusEntry { status: "skipped", icon: "ci-skipped", spin: false, color: "$color.mutedForeground", text: "$color.mutedForeground", label: "statusSkipped" },
];

/// A status value → a `ciStatus`: unknown or missing = `queued`.
pub fn ci_status_of(value: Option<&Value>) -> &'static str {
    value.and_then(Value::as_str).and_then(|s| CI_STATUSES.iter().find(|c| **c == s).copied()).unwrap_or("queued")
}

/// THE table row of a status (unknown or missing = `queued`).
pub fn ci_status_entry(value: Option<&Value>) -> CiStatusEntry {
    let s = ci_status_of(value);
    TABLE.iter().find(|e| e.status == s).copied().unwrap_or(TABLE[3])
}

// ---------------------------------------------------------------------------
// §2 Line anchors
// ---------------------------------------------------------------------------

/// A 1-based inclusive line range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRange {
    pub start: u64,
    pub end: u64,
}

impl LineRange {
    pub fn contains(&self, line: u64) -> bool {
        line >= self.start && line <= self.end
    }
}

/// The longest line number an anchor spells (9 digits): longer = no anchor.
const ANCHOR_MAX_DIGITS: usize = 9;

fn anchor_number(text: &[u8], from: usize) -> Option<(u64, usize)> {
    let mut i = from;
    while i < text.len() && text[i].is_ascii_digit() {
        i += 1;
    }
    if i == from || i - from > ANCHOR_MAX_DIGITS {
        return None;
    }
    let value: u64 = std::str::from_utf8(&text[from..i]).ok()?.parse().ok()?;
    (value >= 1).then_some((value, i))
}

/// `L12` / `L6-L9` (an optional leading `#`) → the normalised range; else None.
pub fn parse_line_anchor(text: &str) -> Option<LineRange> {
    let b = text.as_bytes();
    let mut i = usize::from(b.first() == Some(&b'#'));
    if b.get(i) != Some(&b'L') {
        return None;
    }
    let (a, next) = anchor_number(b, i + 1)?;
    i = next;
    if i == b.len() {
        return Some(LineRange { start: a, end: a });
    }
    if b.get(i) != Some(&b'-') || b.get(i + 1) != Some(&b'L') {
        return None;
    }
    let (c, next) = anchor_number(b, i + 2)?;
    if next != b.len() {
        return None;
    }
    Some(LineRange { start: a.min(c), end: a.max(c) })
}

/// [`parse_line_anchor`] of any JSON value (a non-string = None).
pub fn parse_line_anchor_value(value: Option<&Value>) -> Option<LineRange> {
    value.and_then(Value::as_str).and_then(parse_line_anchor)
}

/// `L<start>` when equal, else `L<min>-L<max>`.
pub fn format_line_anchor(start: u64, end: u64) -> String {
    let (lo, hi) = (start.min(end), start.max(end));
    if lo == hi {
        format!("L{lo}")
    } else {
        format!("L{lo}-L{hi}")
    }
}

/// A press on line `line`: no shift or no valid current anchor → `L<line>`;
/// shift → the range from the current START to `line`, normalised.
pub fn extend_line_anchor(current: Option<&Value>, line: u64, shift: bool) -> String {
    match if shift { parse_line_anchor_value(current) } else { None } {
        Some(r) => format_line_anchor(r.start, line),
        None => format_line_anchor(line, line),
    }
}

// ---------------------------------------------------------------------------
// §3 The log model
// ---------------------------------------------------------------------------

/// One content line (marker lines get none).
#[derive(Debug, Clone, PartialEq)]
pub struct LogLine {
    /// 1-based content line number.
    pub n: usize,
    /// The open section's index, -1 outside every section.
    pub section: i64,
    /// `HH:MM:SS` of a leading timestamp, as written.
    pub time: Option<String>,
    /// `error` | `warning` | `comment` from a `##[error]`-style prefix.
    pub kind: Option<&'static str>,
    /// The line without the timestamp and the prefix, ANSI escapes kept.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogSection {
    pub index: usize,
    pub title: String,
    /// First content line number (`end + 1` when empty).
    pub start: usize,
    /// Last content line number (`start - 1` when empty).
    pub end: usize,
    pub count: usize,
    pub status: &'static str,
    pub duration: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LogModel {
    pub lines: Vec<LogLine>,
    pub sections: Vec<LogSection>,
}

impl LogModel {
    pub fn to_json(&self) -> Value {
        json!({
            "lines": self.lines.iter().map(|l| json!({"n": l.n, "section": l.section, "time": l.time, "kind": l.kind, "text": l.text})).collect::<Vec<_>>(),
            "sections": self.sections.iter().map(|s| json!({"index": s.index, "title": s.title, "start": s.start, "end": s.end, "count": s.count, "status": s.status, "duration": s.duration})).collect::<Vec<_>>(),
        })
    }

    /// The content line `n` (1-based).
    pub fn line(&self, n: usize) -> Option<&LogLine> {
        n.checked_sub(1).and_then(|i| self.lines.get(i)).filter(|l| l.n == n)
    }
}

fn is_csi_param(c: char) -> bool {
    ('\u{30}'..='\u{3f}').contains(&c)
}
fn is_csi_intermediate(c: char) -> bool {
    ('\u{20}'..='\u{2f}').contains(&c)
}
fn is_csi_final(c: char) -> bool {
    ('\u{40}'..='\u{7e}').contains(&c)
}

/// The end index of a COMPLETE CSI escape starting at `i`, else None.
fn csi_end(text: &[char], i: usize) -> Option<usize> {
    if text.get(i) != Some(&'\u{1b}') || text.get(i + 1) != Some(&'[') {
        return None;
    }
    let mut j = i + 2;
    while j < text.len() && is_csi_param(text[j]) {
        j += 1;
    }
    while j < text.len() && is_csi_intermediate(text[j]) {
        j += 1;
    }
    (j < text.len() && is_csi_final(text[j])).then_some(j + 1)
}

/// The text with every complete ANSI CSI escape removed (an incomplete one stays).
pub fn strip_ansi(text: &str) -> String {
    if !text.contains('\u{1b}') {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if let Some(end) = csi_end(&chars, i) {
            i = end;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// `text` with its first `count` escape-free characters removed, every
/// escape kept where it was.
fn cut_visible(text: &str, count: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut removed = 0;
    let mut i = 0;
    while i < chars.len() {
        if let Some(end) = csi_end(&chars, i) {
            out.extend(&chars[i..end]);
            i = end;
            continue;
        }
        if removed < count {
            removed += 1;
        } else {
            out.push(chars[i]);
        }
        i += 1;
    }
    out
}

/// `YYYY-MM-DDTHH:MM:SS(.fraction)?(Z|±HH:MM) ` at the start: (the
/// `HH:MM:SS`, the byte length including the trailing space).
fn timestamp(text: &str) -> Option<(&str, usize)> {
    let b = text.as_bytes();
    let digits = |from: usize, n: usize| b.len() >= from + n && b[from..from + n].iter().all(u8::is_ascii_digit);
    let at = |i: usize, c: u8| b.get(i) == Some(&c);
    if !(digits(0, 4) && at(4, b'-') && digits(5, 2) && at(7, b'-') && digits(8, 2) && at(10, b'T')) {
        return None;
    }
    if !(digits(11, 2) && at(13, b':') && digits(14, 2) && at(16, b':') && digits(17, 2)) {
        return None;
    }
    let mut i = 19;
    if at(i, b'.') && digits(i + 1, 1) {
        i += 1;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
    }
    if at(i, b'Z') {
        i += 1;
    } else if (at(i, b'+') || at(i, b'-')) && digits(i + 1, 2) && at(i + 3, b':') && digits(i + 4, 2) {
        i += 6;
    } else {
        return None;
    }
    at(i, b' ').then(|| (&text[11..19], i + 1))
}

const PREFIXES: [(&str, &str); 4] = [("##[error]", "error"), ("##[warning]", "warning"), ("##[notice]", "comment"), ("##[debug]", "comment")];

/// ASCII space and tab trimmed off both ends.
fn trim_spaces(text: &str) -> &str {
    text.trim_matches(|c| c == ' ' || c == '\t')
}

/// Raw log lines (+ per-section `{status, duration}`) → numbered content
/// lines and their `##[group]` sections (contract §3).
pub fn log_model(lines: Option<&Value>, sections_meta: Option<&Value>) -> LogModel {
    let mut out: Vec<LogLine> = Vec::new();
    let mut sections: Vec<LogSection> = Vec::new();
    let mut open: i64 = -1;
    let mut closed_last = true;
    let empty = Vec::new();
    let meta = sections_meta.and_then(Value::as_array).unwrap_or(&empty);
    for raw in lines.and_then(Value::as_array).unwrap_or(&empty) {
        let mut text = raw.as_str().unwrap_or("").to_string();
        let mut time = None;
        if let Some((t, len)) = timestamp(&text) {
            time = Some(t.to_string());
            text = text[len..].to_string();
        }
        let plain = strip_ansi(&text);
        if let Some(rest) = plain.strip_prefix("##[group]") {
            if open >= 0 {
                sections[open as usize].end = out.len();
            }
            open = sections.len() as i64;
            closed_last = false;
            sections.push(LogSection { index: sections.len(), title: trim_spaces(rest).to_string(), start: out.len() + 1, end: out.len(), count: 0, status: "passed", duration: None });
            continue;
        }
        if plain.starts_with("##[endgroup]") {
            if open >= 0 {
                sections[open as usize].end = out.len();
                open = -1;
                closed_last = true;
            }
            continue;
        }
        let mut kind = None;
        for (prefix, k) in PREFIXES {
            if plain.starts_with(prefix) {
                kind = Some(k);
                text = cut_visible(&text, prefix.chars().count());
                break;
            }
        }
        out.push(LogLine { n: out.len() + 1, section: open, time, kind, text });
    }
    if open >= 0 {
        sections[open as usize].end = out.len();
    }
    let last = sections.len().wrapping_sub(1);
    for s in &mut sections {
        s.count = (s.end + 1).saturating_sub(s.start);
        let given = meta.get(s.index).and_then(Value::as_object);
        let status = given.and_then(|m| m.get("status")).filter(|v| !v.is_null());
        s.status = match status {
            Some(v) => ci_status_of(Some(v)),
            None if !closed_last && s.index == last => "running",
            None => "passed",
        };
        s.duration = given.and_then(|m| m.get("duration")).and_then(Value::as_str).map(str::to_string);
    }
    LogModel { lines: out, sections }
}

/// The sections that open on their own: `failed` or `running`.
pub fn log_open_default(model: &LogModel) -> Vec<usize> {
    model.sections.iter().filter(|s| s.status == "failed" || s.status == "running").map(|s| s.index).collect()
}

/// One flat row of a LogLines body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogRow {
    Line(usize),
    Section(usize),
}

impl LogRow {
    pub fn to_json(&self) -> Value {
        match self {
            LogRow::Line(n) => json!({"kind": "line", "n": n}),
            LogRow::Section(s) => json!({"kind": "section", "section": s}),
        }
    }
}

/// Whether a JSON `open` value names section `index` (a number equal to it).
fn names(v: &Value, index: usize) -> bool {
    v.as_f64() == Some(index as f64)
}

/// The section indexes a JSON `open` array names (any other value ignored).
pub fn open_set(open: Option<&Value>, model: &LogModel) -> Vec<usize> {
    let open = open.and_then(Value::as_array);
    model.sections.iter().map(|s| s.index).filter(|i| open.is_some_and(|o| o.iter().any(|v| names(v, *i)))).collect()
}

/// The flat rows a LogLines body windows: lines outside sections, each
/// section's head row, then its lines when its index is in `open`.
pub fn log_rows(model: &LogModel, open: Option<&Value>) -> Vec<LogRow> {
    let open: Vec<usize> = open_set(open, model);
    log_rows_open(model, &open)
}

/// [`log_rows`] over an already resolved open set.
pub fn log_rows_open(model: &LogModel, open: &[usize]) -> Vec<LogRow> {
    let mut rows = Vec::with_capacity(model.lines.len() + model.sections.len());
    let mut line = 0;
    let lines = &model.lines;
    for s in &model.sections {
        while line < lines.len() && lines[line].n < s.start {
            rows.push(LogRow::Line(lines[line].n));
            line += 1;
        }
        rows.push(LogRow::Section(s.index));
        let shown = open.contains(&s.index);
        while line < lines.len() && lines[line].n <= s.end {
            if shown {
                rows.push(LogRow::Line(lines[line].n));
            }
            line += 1;
        }
    }
    while line < lines.len() {
        rows.push(LogRow::Line(lines[line].n));
        line += 1;
    }
    rows
}

/// One search match: code-point offsets into the line's escape-free text,
/// end exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogMatch {
    pub line: usize,
    pub start: usize,
    pub end: usize,
}

impl LogMatch {
    pub fn to_json(&self) -> Value {
        json!({"line": self.line, "start": self.start, "end": self.end})
    }
}

fn fold_ascii(c: char) -> char {
    c.to_ascii_lowercase()
}

/// Every non-overlapping match of `query` (ASCII case-insensitive,
/// code-point offsets) in every content line's escape-free text, folded
/// sections too.
pub fn log_search(model: &LogModel, query: &str) -> Vec<LogMatch> {
    if query.is_empty() {
        return Vec::new();
    }
    let q: Vec<char> = query.chars().map(fold_ascii).collect();
    let mut out = Vec::new();
    for line in &model.lines {
        let t: Vec<char> = strip_ansi(&line.text).chars().map(fold_ascii).collect();
        let mut i = 0;
        while i + q.len() <= t.len() {
            if t[i..i + q.len()] == q[..] {
                out.push(LogMatch { line: line.n, start: i, end: i + q.len() });
                i += q.len();
            } else {
                i += 1;
            }
        }
    }
    out
}

/// `open` plus the section holding `line` (sorted, unique, integers only).
pub fn log_reveal(model: &LogModel, open: Option<&Value>, line: usize) -> Vec<i64> {
    let mut set: Vec<i64> = Vec::new();
    for v in open.and_then(Value::as_array).into_iter().flatten() {
        let Some(f) = v.as_f64() else { continue };
        if f.fract() == 0.0 && f.is_finite() && f.abs() < 9_007_199_254_740_992.0 && !set.contains(&(f as i64)) {
            set.push(f as i64);
        }
    }
    if let Some(at) = model.line(line) {
        if at.section >= 0 && !set.contains(&at.section) {
            set.push(at.section);
        }
    }
    set.sort_unstable();
    set
}

// ---------------------------------------------------------------------------
// §6 CodeBlock blame
// ---------------------------------------------------------------------------

/// One line's blame block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlameLine {
    /// Index into the `blame` array, -1 before the first entry.
    pub block: i64,
    /// The block's first line (shows author + age).
    pub first: bool,
    /// A first line of every block but the first shown: the hairline.
    pub border: bool,
}

impl BlameLine {
    pub fn to_json(&self) -> Value {
        json!({"block": self.block, "first": self.first, "border": self.border})
    }
}

/// Per line (1…line_count) its blame block: entries sorted by `start`
/// (stable), each running to the next entry's start − 1 (the last to the
/// end); an entry whose `start` is no integer ≥ 1 is ignored.
pub fn blame_blocks(blame: Option<&Value>, line_count: usize) -> Vec<BlameLine> {
    let mut entries: Vec<(usize, f64)> = Vec::new();
    for (index, e) in blame.and_then(Value::as_array).into_iter().flatten().enumerate() {
        if let Some(start) = e.get("start").and_then(Value::as_f64) {
            if start.is_finite() && start.fract() == 0.0 && start >= 1.0 {
                entries.push((index, start));
            }
        }
    }
    entries.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut out = Vec::with_capacity(line_count);
    let mut at: i64 = -1;
    let mut first_shown = 0usize;
    for n in 1..=line_count {
        let mut first = false;
        while ((at + 1) as usize) < entries.len() && entries[(at + 1) as usize].1 <= n as f64 {
            at += 1;
            first = true;
        }
        if first && first_shown == 0 {
            first_shown = n;
        }
        out.push(BlameLine { block: if at >= 0 { entries[at as usize].0 as i64 } else { -1 }, first, border: first && n != first_shown });
    }
    out
}
