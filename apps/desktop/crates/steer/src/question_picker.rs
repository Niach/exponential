//! AskUserQuestion picker detection on the live terminal grid (EXP-197).
//!
//! Same problem as the plan picker (EXP-150): `claude` flushes a turn's
//! transcript entries — including the `AskUserQuestion` tool_use — only once
//! the picker is answered, so the transcript-derived `question` used to reach
//! viewers only AFTER the questions were answered, and then looked freshly
//! answerable. The picker is on screen exactly while it is pending, so the
//! emitter watches the grid (captured against claude v2.1.215):
//!
//! ```text
//! ←  ☐ Toppings  ☐ Size  ✔ Submit  →          <- tab bar (☐/☒ per question)
//!
//! Which toppings do you want?                  <- question text (wraps)
//!
//! ❯ 1. [ ] Cheese                              <- ❯ cursor; [ ]/[✔] = multiSelect
//!   2. [✔] Ham
//!      A short description                     <- optional per-option line
//!   3. [ ] Type something
//!      Next
//! ────────────────────────────────────────────
//!   4. Chat about this                         <- below the rule: never offered
//!
//! Enter to select · Tab/Arrow keys to navigate · Esc to cancel
//! ```
//!
//! [`detect`] recognizes that shape — a `❯`-marked consecutively-numbered
//! option run, a tab-bar line (`☐`/`☒`/`☑`/`✔`) above it, and an
//! "Enter to select" footer below it — and parses the REAL option labels/keys
//! off the rows, skipping interleaved description lines and stopping at the
//! rule (so the synthetic "Chat about this" is never offered — re-verified
//! live on v2.1.233, where its digit INSTANTLY cancels the whole ask; the
//! label match keeps it out of any future above-the-rule layout too). The
//! single-select "Type something." row is published flagged `free_text`
//! (EXP-513): its digit only moves the cursor, typed characters fill the row
//! in place, and Enter submits them as that question's answer. The one
//! footer-less screen accepted is the REVIEW step as claude ≥2.1.220 paints
//! it (no footer, no rule, no "Chat about this" — EXP-374): there a fully
//! answered multi-tab bar is the anchor instead. Plan-approval
//! screens are explicitly excluded ([`plan_picker`] owns those).
//! [`QuestionPickerWatcher`] debounces detections and re-fires when the
//! visible question changes (the multi-question tab flow advances in place).
//!
//! EXP-249 also reads the tab bar itself: which tab is CURRENT (claude marks
//! answered tabs `☒`/`☑` and advances left to right, so the first unanswered
//! tab is the visible one) and whether the picker is sitting on the final
//! `✔ Submit` review step. Remote answering needs both — an answer is only
//! injectable while its own tab is up, and the submit step is a question of
//! its own. Colour is what the TUI actually highlights with, and
//! [`terminal::screen_lines`] is plain text, so the glyphs are the signal.

use crate::frames::QuestionOption;
use crate::plan_picker;

/// The selection cursor `claude` renders on the highlighted option row.
const SELECTION_MARKER: char = '❯';

/// Tab-bar glyphs — one of these must sit on a line above the options (the
/// per-question `☐`/`☒` markers or the `✔ Submit` tab).
const TAB_GLYPHS: &[char] = &['☐', '☒', '☑', '✔'];

/// Tab-bar glyphs meaning "this tab already has an answer".
const ANSWERED_TAB_GLYPHS: &[char] = &['☒', '☑', '✔'];

/// Trailing decoration on the tab bar's last entry (`←  ☐ A  ✔ Submit  →`).
const TAB_BAR_ARROWS: &[char] = &['→', '←'];

/// The footer phrase below the options — both observed variants carry it
/// ("Enter to select · ↑/↓ to navigate" / "Enter to select · Tab/Arrow keys").
const FOOTER_ANCHOR: &str = "Enter to select";

/// One entry of the picker's tab bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestionTab {
    /// The tab's header text (`Toppings`, `Submit`, …).
    pub label: String,
    /// Rendered with an answered glyph (`☒`/`☑`/`✔`).
    pub answered: bool,
}

/// A detected AskUserQuestion picker.
#[derive(Clone, Debug, PartialEq)]
pub struct QuestionSnapshot {
    /// The visible question text (wrapped lines re-joined with spaces).
    pub text: String,
    pub options: Vec<QuestionOption>,
    /// Any option row carried a `[ ]`/`[✔]` checkbox.
    pub multi_select: bool,
    /// Per-option checkbox state, parallel to `options` (all `false` on a
    /// single-select picker) — the starting point a remote multiSelect answer
    /// toggles from.
    pub checked: Vec<bool>,
    /// The tab bar above the options, left to right. A single-question ask
    /// renders one tab and no `Submit`.
    pub tabs: Vec<QuestionTab>,
    /// Index into `tabs` of the tab being shown: the first unanswered one,
    /// else the last (the review step). `None` when the bar is unparseable.
    pub current_tab: Option<usize>,
    /// The picker is on the final `✔ Submit` review step.
    pub review: bool,
}

/// Box-drawing characters of the option-preview pane (EXP-394): a preview
/// question renders side-by-side — narrow option column left, framed preview
/// pane right — so option rows carry pane borders/content after the label.
/// The label is cut at the first such character.
const PREVIEW_PANE_CHARS: &[char] = &['│', '┌', '┐', '└', '┘', '├', '┤', '─'];

/// Claude's synthetic free-text row ("Type something." — the trailing dot
/// varies by version). On a SINGLE-select picker its digit only moves the
/// cursor, typed characters then fill the row in place, and Enter submits
/// them as that question's answer — while Enter on the still-EMPTY row
/// declines the whole ask (verified live on v2.1.233, EXP-513). Published
/// with `free_text` so clients collect the reply before answering.
fn is_free_text_label(label: &str) -> bool {
    label
        .trim_end_matches(['.', '…'])
        .trim()
        .eq_ignore_ascii_case("type something")
}

/// Claude's synthetic "Chat about this" row. Its digit INSTANTLY cancels the
/// entire ask — every answer already given included — and drops to the
/// composer (verified live on v2.1.233). It normally sits below the rule and
/// falls out with the boundary stop; the label match keeps it out even if a
/// future layout numbers it above the rule. Remote steerers chat via the
/// composer instead (the EXP-334 Esc-reroute).
fn is_chat_about_this_label(label: &str) -> bool {
    label
        .trim_end_matches(['.', '…'])
        .trim()
        .eq_ignore_ascii_case("chat about this")
}

/// One parsed option row: number, label (checkbox stripped), and the checkbox
/// state — `None` on a single-select row, `Some(ticked)` on a multiSelect one.
fn parse_option_row(row: &str) -> Option<(u32, &str, Option<bool>)> {
    let row = row.strip_prefix(SELECTION_MARKER).unwrap_or(row).trim_start();
    let dot = row.find('.')?;
    let number: u32 = row[..dot].parse().ok()?;
    if !(1..=9).contains(&number) {
        return None;
    }
    let mut label = row[dot + 1..].trim_start();
    let mut checkbox = None;
    if let Some(rest) = label.strip_prefix('[') {
        // A checkbox is a short bracket group (`[ ]` / `[✔]`), never a long
        // bracketed label — measure in chars, the check glyph is multi-byte.
        if let Some(close) = rest.find(']') {
            let inner = &rest[..close];
            if inner.chars().count() <= 2 {
                checkbox = Some(!inner.trim().is_empty());
                label = rest[close + 1..].trim_start();
            }
        }
    }
    if let Some(pane) = label.find(PREVIEW_PANE_CHARS) {
        label = &label[..pane];
    }
    let label = label.trim_end();
    (!label.is_empty()).then_some((number, label, checkbox))
}

/// A horizontal-rule row (the separators framing the picker box).
fn is_rule(t: &str) -> bool {
    !t.is_empty() && t.chars().all(|c| matches!(c, '╌' | '─' | '═' | '┄' | '┈'))
}

fn is_tab_line(t: &str) -> bool {
    t.chars().any(|c| TAB_GLYPHS.contains(&c))
}

/// How many tab glyphs a line carries. The real bar of a multi-question ask
/// has one per question plus `✔ Submit` (≥2); the review screen's per-answer
/// summary rows carry at most one `✔` each, so glyph count separates the two.
fn tab_glyph_count(t: &str) -> usize {
    t.chars().filter(|c| TAB_GLYPHS.contains(c)).count()
}

/// A line that terminates option/question scanning in either direction.
fn is_boundary(t: &str) -> bool {
    t.is_empty() || is_rule(t) || is_tab_line(t)
}

/// Split a tab-bar line into its entries: each starts at a tab glyph and runs
/// to the next glyph (trailing arrows/whitespace trimmed off).
fn parse_tab_bar(line: &str) -> Vec<QuestionTab> {
    let mut tabs: Vec<QuestionTab> = Vec::new();
    let mut current: Option<QuestionTab> = None;
    for ch in line.chars() {
        if TAB_GLYPHS.contains(&ch) {
            if let Some(tab) = current.take() {
                tabs.push(tab);
            }
            current = Some(QuestionTab {
                label: String::new(),
                answered: ANSWERED_TAB_GLYPHS.contains(&ch),
            });
            continue;
        }
        if let Some(tab) = &mut current {
            tab.label.push(ch);
        }
    }
    tabs.extend(current);
    for tab in &mut tabs {
        tab.label = tab
            .label
            .trim_matches(|c: char| c.is_whitespace() || TAB_BAR_ARROWS.contains(&c))
            .to_string();
    }
    tabs.retain(|tab| !tab.label.is_empty());
    tabs
}

/// The tab being shown: claude answers left to right, so the first unanswered
/// tab is the live one; with everything answered the picker sits on the last
/// tab (`✔ Submit`).
fn current_tab_index(tabs: &[QuestionTab]) -> Option<usize> {
    if tabs.is_empty() {
        return None;
    }
    Some(
        tabs.iter()
            .position(|tab| !tab.answered)
            .unwrap_or(tabs.len() - 1),
    )
}

/// Whether the visible step is the ask's review/submit tab: the current tab is
/// the `Submit` one AND the options offer submitting.
fn is_review(tabs: &[QuestionTab], current: Option<usize>, options: &[QuestionOption]) -> bool {
    let submits = options
        .iter()
        .any(|option| option.label.to_ascii_lowercase().starts_with("submit"));
    let on_submit_tab = current
        .and_then(|index| tabs.get(index))
        .is_some_and(|tab| tab.label.eq_ignore_ascii_case("submit"));
    submits && on_submit_tab
}

/// Detect an AskUserQuestion picker on a visible-screen snapshot.
pub fn detect(lines: &[String]) -> Option<QuestionSnapshot> {
    detect_impl(lines, false)
}

/// Detect while a hook already says an AskUserQuestion IS pending (EXP-394).
/// A picker taller than the grid — long option descriptions on the default
/// 80x24 remote-session terminal — scrolls its TAB BAR (and on the review
/// step even the footer) out of the viewport, and [`detect`]'s anchor
/// requirements then reject the very picker the ask is parked on: remote
/// answers were parked, dropped after the retry TTL, and never acked, so the
/// mobile stepper rolled back to question 1 with nothing injected. With the
/// hook vouching that an ask is on screen, the missing anchors are excused:
/// - no tab bar: accepted with the footer as the remaining anchor
///   (`tabs`/`current_tab` come back empty/None; answer routing matches on
///   the question TEXT, which stays visible at the top of the box),
/// - no tab bar AND no footer: accepted only when an option row offers
///   submitting (the overflowing ≥2.1.220 review step).
/// The strict entry stays the one lookalike-proof enough for the hook-less
/// legacy path.
pub fn detect_during_ask(lines: &[String]) -> Option<QuestionSnapshot> {
    detect_impl(lines, true)
}

fn detect_impl(lines: &[String], ask_pending: bool) -> Option<QuestionSnapshot> {
    // Plan-approval screens belong to the plan watcher — never double-detect.
    if plan_picker::detect(lines).is_some() {
        return None;
    }

    let marker_idx = lines.iter().rposition(|line| {
        let t = line.trim_start();
        t.starts_with(SELECTION_MARKER) && parse_option_row(t).is_some()
    })?;
    let marker_number = parse_option_row(lines[marker_idx].trim_start())?.0;

    // Expand upward to option 1, skipping per-option description lines but
    // never crossing a blank line, rule, or tab bar.
    let mut first_idx = marker_idx;
    let mut expect = marker_number;
    let mut idx = marker_idx;
    while expect > 1 {
        idx = idx.checked_sub(1)?;
        let t = lines[idx].trim();
        if let Some((n, _, _)) = parse_option_row(t) {
            if n != expect - 1 {
                return None;
            }
            first_idx = idx;
            expect = n;
        } else if is_boundary(t) {
            return None;
        }
    }

    // Collect downward from option 1, skipping description lines, stopping at
    // any boundary (the rule keeps the synthetic "Chat about this" out).
    let mut options = Vec::new();
    let mut checked = Vec::new();
    let mut multi_select = false;
    let mut next = 1u32;
    let mut last_option_idx = first_idx;
    for (i, line) in lines.iter().enumerate().skip(first_idx) {
        let t = line.trim();
        match parse_option_row(t) {
            // "Chat about this" instantly cancels the whole ask — never
            // offered, wherever a claude version renders it (EXP-513).
            Some((n, label, _)) if n == next && is_chat_about_this_label(label) => break,
            Some((n, label, checkbox)) if n == next => {
                options.push(QuestionOption::new(label, n.to_string()));
                checked.push(checkbox == Some(true));
                multi_select |= checkbox.is_some();
                last_option_idx = i;
                next += 1;
            }
            Some(_) => break,
            None if is_boundary(t) || t.contains(FOOTER_ANCHOR) => break,
            None => {} // description line under an option
        }
    }
    if options.len() < 2 || marker_number >= next {
        return None;
    }
    // Mark the free-text row — single-select only: in a multiSelect picker
    // its digit TOGGLES like any row and the reply-typing flow is different
    // machinery (deliberately unsupported for now, EXP-513).
    if !multi_select {
        for option in &mut options {
            option.free_text = is_free_text_label(&option.label);
        }
    }

    // Anchors: a tab-bar line above the options, the footer below them. The
    // review screen renders ✔-carrying answer-summary rows BETWEEN the bar and
    // the options — anchoring on the nearest glyph line landed on a summary
    // row there, parsing junk tabs and missing `review` (EXP-275). Prefer the
    // nearest line with ≥2 glyphs (the real multi-question bar); a
    // single-question ask has a 1-glyph bar and no summary rows, so the
    // any-glyph fallback stays correct for it.
    let tab_idx = lines[..first_idx]
        .iter()
        .rposition(|l| tab_glyph_count(l.trim()) >= 2)
        .or_else(|| lines[..first_idx].iter().rposition(|l| is_tab_line(l.trim())));
    // The tab bar is a hard anchor UNLESS a hook vouches an ask is pending —
    // an overflowing picker scrolls the bar out of the viewport (EXP-394).
    if tab_idx.is_none() && !ask_pending {
        return None;
    }
    let tabs = tab_idx.map(|idx| parse_tab_bar(&lines[idx])).unwrap_or_default();
    // claude ≥2.1.220 renders the REVIEW step without the footer (and without
    // the rule + "Chat about this") — requiring it stranded every remote
    // multi-question ask on the review screen (EXP-374). There the bar itself
    // is the anchor: every question tab answered plus the ✔ Submit tab (≥2
    // glyphs). Ordinary question tabs keep the footer requirement — it is
    // what excludes footer-less lookalikes (the workspace-trust prompt's
    // single-☐ bar, plain numbered lists under stray glyphs). During a
    // hook-confirmed ask the OVERFLOWING review step shows neither bar nor
    // footer — there the submit option row is the remaining anchor.
    let has_footer = lines[last_option_idx + 1..]
        .iter()
        .any(|l| l.contains(FOOTER_ANCHOR));
    let offers_submit = options
        .iter()
        .any(|option| option.label.to_ascii_lowercase().starts_with("submit"));
    if !has_footer
        && !(tabs.len() >= 2 && tabs.iter().all(|tab| tab.answered))
        && !(ask_pending && tab_idx.is_none() && offers_submit)
    {
        return None;
    }

    // Question text: the contiguous non-blank block right above the options
    // (long questions wrap — re-join the lines), bounded by the tab bar (when
    // visible — an overflowed one leaves the block bounded by the screen top).
    let mut text_lines: Vec<&str> = Vec::new();
    for line in lines[tab_idx.map_or(0, |idx| idx + 1)..first_idx].iter().rev() {
        let t = line.trim();
        if is_boundary(t) || parse_option_row(t).is_some() {
            if text_lines.is_empty() {
                continue; // still skipping the gap under the question
            }
            break;
        }
        text_lines.push(t);
    }
    text_lines.reverse();
    let text = text_lines.join(" ");
    if text.trim().is_empty() {
        return None;
    }

    let current_tab = current_tab_index(&tabs);
    let review = is_review(&tabs, current_tab, &options);
    Some(QuestionSnapshot {
        text,
        options,
        multi_select,
        checked,
        tabs,
        current_tab,
        review,
    })
}

/// Whitespace-insensitive question-text identity — the grid renders the text
/// re-wrapped, the transcript carries it raw; stripping ALL whitespace makes
/// the two comparable (and survives mid-word wrap points).
pub fn normalize_question_text(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// The option number the `❯` cursor sits on, if any. The free-text reply
/// choreography (EXP-513) types only once the cursor verifiably reached the
/// free-text row — characters typed while the cursor is elsewhere are eaten
/// by the picker, and the closing Enter would activate whatever row IS
/// highlighted.
pub fn selected_option(lines: &[String]) -> Option<u32> {
    lines.iter().rev().find_map(|line| {
        let t = line.trim_start();
        t.starts_with(SELECTION_MARKER)
            .then(|| parse_option_row(t).map(|(n, _, _)| n))
            .flatten()
    })
}

/// Debounce depth — a question must be present with the SAME text (or absent)
/// on this many consecutive ticks before the machine transitions.
const STREAK: u8 = 2;

/// Per-session question-picker state machine. Unlike the plan watcher it has
/// no Resolved output — resolution reaches viewers through the transcript
/// flush (the answered twin's `Question answered:` narrations) — but it DOES
/// re-fire when the visible question changes, so the multi-question tab flow
/// publishes each question as it comes up.
#[derive(Default)]
pub struct QuestionPickerWatcher {
    /// Normalized text of the question currently published as pending.
    pending: Option<String>,
    present_streak: u8,
    absent_streak: u8,
    candidate: Option<QuestionSnapshot>,
}

impl QuestionPickerWatcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether an AskUserQuestion picker is currently pending on screen —
    /// the EXP-214 "needs input" signal (clears once the picker leaves the
    /// grid for [`STREAK`] consecutive ticks).
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Feed one poll tick; returns a snapshot to publish when a new question
    /// settles on screen. `display_offset > 0` (viewport scrolled into
    /// history) freezes the machine entirely.
    pub fn tick(
        &mut self,
        detection: Option<QuestionSnapshot>,
        display_offset: usize,
    ) -> Option<QuestionSnapshot> {
        if display_offset > 0 {
            return None;
        }
        match detection {
            Some(snapshot) => {
                self.absent_streak = 0;
                let normalized = normalize_question_text(&snapshot.text);
                if self.pending.as_deref() == Some(normalized.as_str()) {
                    return None; // steady state (checkbox toggles don't re-fire)
                }
                let same_candidate = self
                    .candidate
                    .as_ref()
                    .is_some_and(|c| normalize_question_text(&c.text) == normalized);
                // Keep the LATEST snapshot (options may have settled further).
                self.candidate = Some(snapshot);
                self.present_streak = if same_candidate {
                    self.present_streak + 1
                } else {
                    1
                };
                if self.present_streak >= STREAK {
                    self.present_streak = 0;
                    self.pending = Some(normalized);
                    return self.candidate.take();
                }
                None
            }
            None => {
                self.present_streak = 0;
                self.candidate = None;
                if self.pending.is_none() {
                    return None;
                }
                self.absent_streak += 1;
                if self.absent_streak >= STREAK {
                    self.absent_streak = 0;
                    self.pending = None;
                }
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(rows: &[&str]) -> Vec<String> {
        rows.iter().map(|r| r.to_string()).collect()
    }

    /// The single-select picker as rendered by claude v2.1.215 (captured).
    fn color_screen() -> Vec<String> {
        screen(&[
            "❯ Use the AskUserQuestion tool.",
            "──────────────────────────────────────────",
            " ☐ Color",
            "",
            "Which color do you prefer?",
            "",
            "❯ 1. Red",
            "     Warm and vibrant",
            "  2. Green",
            "     Calm and natural",
            "  3. Blue",
            "     Cool and serene",
            "  4. Type something.",
            "──────────────────────────────────────────",
            "  5. Chat about this",
            "",
            "Enter to select · ↑/↓ to navigate · Esc to cancel",
        ])
    }

    /// The multi-question multiSelect variant (captured).
    fn toppings_screen() -> Vec<String> {
        screen(&[
            "──────────────────────────────────────────",
            "←  ☐ Toppings  ☐ Size  ✔ Submit  →",
            "",
            "Which toppings do you want?",
            "",
            "❯ 1. [✔] Cheese",
            "  2. [ ] Ham",
            "  3. [✔] Mushrooms",
            "  4. [ ] Type something",
            "     Next",
            "──────────────────────────────────────────",
            "  5. Chat about this",
            "",
            "Enter to select · Tab/Arrow keys to navigate · Esc to cancel",
        ])
    }

    #[test]
    fn detects_single_select_with_descriptions() {
        let snap = detect(&color_screen()).expect("picker detected");
        assert_eq!(snap.text, "Which color do you prefer?");
        assert!(!snap.multi_select);
        assert_eq!(
            snap.options
                .iter()
                .map(|o| (o.key.as_str(), o.label.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("1", "Red"),
                ("2", "Green"),
                ("3", "Blue"),
                ("4", "Type something."),
            ]
        );
    }

    #[test]
    fn detects_multi_select_and_strips_checkboxes() {
        let snap = detect(&toppings_screen()).expect("picker detected");
        assert_eq!(snap.text, "Which toppings do you want?");
        assert!(snap.multi_select);
        assert_eq!(
            snap.options
                .iter()
                .map(|o| (o.key.as_str(), o.label.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("1", "Cheese"),
                ("2", "Ham"),
                ("3", "Mushrooms"),
                ("4", "Type something"),
            ]
        );
        // EXP-249: the ticked set a remote multiSelect answer toggles from.
        assert_eq!(snap.checked, vec![true, false, true, false]);
    }

    #[test]
    fn reads_the_tab_bar_and_the_current_tab() {
        // EXP-249: `☐` = unanswered, `☒`/`☑`/`✔` = answered, and claude
        // advances left to right — so the first unanswered tab is the one on
        // screen.
        let snap = detect(&toppings_screen()).expect("picker detected");
        assert_eq!(
            snap.tabs,
            vec![
                QuestionTab { label: "Toppings".into(), answered: false },
                QuestionTab { label: "Size".into(), answered: false },
                QuestionTab { label: "Submit".into(), answered: true },
            ]
        );
        assert_eq!(snap.current_tab, Some(0));
        assert!(!snap.review);

        // A single-question ask renders one tab and no Submit.
        let snap = detect(&color_screen()).expect("picker detected");
        assert_eq!(
            snap.tabs,
            vec![QuestionTab { label: "Color".into(), answered: false }]
        );
        assert_eq!(snap.current_tab, Some(0));
        assert!(!snap.review);
    }

    #[test]
    fn the_second_tab_is_current_once_the_first_is_answered() {
        let mut lines = toppings_screen();
        lines[1] = "←  ☒ Toppings  ☐ Size  ✔ Submit  →".into();
        lines[3] = "Which size?".into();
        let snap = detect(&lines).expect("picker detected");
        assert_eq!(snap.current_tab, Some(1));
        assert!(snap.tabs[0].answered);
        assert!(!snap.review);
    }

    #[test]
    fn chat_about_this_below_the_rule_is_never_offered() {
        let snap = detect(&color_screen()).unwrap();
        assert!(snap.options.iter().all(|o| o.label != "Chat about this"));
    }

    #[test]
    fn chat_about_this_above_the_rule_is_still_never_offered() {
        // A layout that numbers the row INSIDE the option run (no rule
        // between) must exclude it by label — its digit instantly cancels
        // the whole ask (EXP-513).
        let lines = screen(&[
            " ☐ Color",
            "",
            "Which color do you prefer?",
            "",
            "❯ 1. Red",
            "  2. Green",
            "  3. Chat about this",
            "",
            "Enter to select · ↑/↓ to navigate · Esc to cancel",
        ]);
        let snap = detect(&lines).expect("picker detected");
        assert_eq!(
            snap.options
                .iter()
                .map(|o| o.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Red", "Green"]
        );
    }

    #[test]
    fn the_free_text_row_is_flagged_on_single_select_only() {
        // EXP-513: "Type something." is claude's inline free-text row —
        // flagged so clients collect a reply before answering.
        let snap = detect(&color_screen()).unwrap();
        assert_eq!(
            snap.options
                .iter()
                .map(|o| (o.label.as_str(), o.free_text))
                .collect::<Vec<_>>(),
            vec![
                ("Red", false),
                ("Green", false),
                ("Blue", false),
                ("Type something.", true),
            ]
        );
        // In a multiSelect picker the digit TOGGLES — the reply flow is
        // different machinery, deliberately unmarked.
        let snap = detect(&toppings_screen()).unwrap();
        assert!(snap.options.iter().all(|o| !o.free_text));
    }

    #[test]
    fn selected_option_reads_the_cursor_row() {
        // The free-text choreography types only once the ❯ verifiably sits
        // on the free-text row (EXP-513).
        assert_eq!(selected_option(&color_screen()), Some(1));
        let mut lines = color_screen();
        lines[6] = "  1. Red".into();
        lines[9] = "❯ 4. Type something.".into();
        assert_eq!(selected_option(&lines), Some(4));
        // Typed characters replace the label in place — the row still reads.
        lines[9] = "❯ 4. purple".into();
        assert_eq!(selected_option(&lines), Some(4));
        lines[9] = "  4. Type something.".into();
        assert_eq!(selected_option(&screen(&["no picker here"])), None);
        assert_eq!(selected_option(&lines), None);
    }

    #[test]
    fn wrapped_question_text_is_rejoined() {
        let mut lines = color_screen();
        lines[4] = "Which unified plan feature set should be used across".into();
        lines.insert(5, "all three pricing surfaces?".into());
        let snap = detect(&lines).expect("picker detected");
        assert_eq!(
            snap.text,
            "Which unified plan feature set should be used across all three pricing surfaces?"
        );
    }

    #[test]
    fn requires_tab_bar_and_footer_anchors() {
        // No tab bar above.
        let lines = screen(&[
            "Pick a database:",
            "❯ 1. Postgres",
            "  2. SQLite",
            "Enter to select · ↑/↓ to navigate",
        ]);
        assert_eq!(detect(&lines), None);

        // No footer below (e.g. the workspace-trust prompt says
        // "Enter to confirm").
        let lines = screen(&[
            " ☐ Trust",
            "Is this a project you trust?",
            "❯ 1. Yes, I trust this folder",
            "  2. No, exit",
            "Enter to confirm · Esc to cancel",
        ]);
        assert_eq!(detect(&lines), None);
    }

    #[test]
    fn plan_approval_screens_are_excluded() {
        // A plan picker with the question glyph noise added — plan_picker owns
        // it, question detection must stay silent.
        let lines = screen(&[
            " ☐ progress",
            "Ready to code?",
            " Here is Claude's plan:",
            "## Plan",
            "",
            " ❯ 1. Yes, auto-accept edits",
            "   2. Yes, manually approve edits",
            "Enter to select · ↑/↓ to navigate",
        ]);
        assert_eq!(detect(&lines), None);
    }

    #[test]
    fn marker_may_sit_on_a_later_option() {
        let mut lines = color_screen();
        lines[6] = "  1. Red".into();
        lines[10] = "❯ 3. Blue".into();
        let snap = detect(&lines).expect("picker detected");
        assert_eq!(snap.options.len(), 4);
    }

    #[test]
    fn plain_numbered_list_without_marker_is_not_a_picker() {
        let mut lines = color_screen();
        lines[6] = "  1. Red".into();
        assert_eq!(detect(&lines), None);
    }

    #[test]
    fn review_screen_detects_as_a_submit_question() {
        // The multi-question flow ends on a review tab — a regular picker
        // whose question is the "Ready to submit" line (useful remotely).
        let lines = screen(&[
            "←  ☒ Toppings  ☒ Size  ✔ Submit  →",
            "",
            "Review your answers",
            "",
            " ● Which toppings do you want?",
            "   → Mushrooms, Cheese",
            "",
            "Ready to submit your answers?",
            "",
            "❯ 1. Submit answers",
            "  2. Cancel",
            "",
            "Enter to select · Tab/Arrow keys to navigate · Esc to cancel",
        ]);
        let snap = detect(&lines).expect("picker detected");
        assert_eq!(snap.text, "Ready to submit your answers?");
        assert_eq!(snap.options.len(), 2);
        // EXP-249: every question tab is answered and the picker offers
        // submitting ⇒ this is the ask's final step.
        assert!(snap.review);
        assert_eq!(snap.current_tab, Some(2));
    }

    #[test]
    fn review_screen_with_checkmarked_summary_rows_still_reads_the_real_tab_bar() {
        // EXP-275: the real review screen's per-answer summary rows can carry
        // ✔ glyphs. Anchoring the tab bar on the NEAREST glyph line landed on
        // a summary row, parsed junk tabs, and never flagged `review` — so
        // the `#submit` step was never published and the remote flow stalled.
        let lines = screen(&[
            "←  ☒ Toppings  ☒ Size  ✔ Submit  →",
            "",
            "Review your answers",
            "",
            " ✔ Which toppings do you want?",
            "   Mushrooms, Cheese",
            " ✔ Which size?",
            "   Large",
            "",
            "Ready to submit your answers?",
            "",
            "❯ 1. Submit answers",
            "  2. Cancel",
            "",
            "Enter to select · Tab/Arrow keys to navigate · Esc to cancel",
        ]);
        let snap = detect(&lines).expect("picker detected");
        assert_eq!(snap.text, "Ready to submit your answers?");
        assert_eq!(
            snap.tabs,
            vec![
                QuestionTab { label: "Toppings".into(), answered: true },
                QuestionTab { label: "Size".into(), answered: true },
                QuestionTab { label: "Submit".into(), answered: true },
            ]
        );
        assert_eq!(snap.current_tab, Some(2));
        assert!(snap.review);
    }

    #[test]
    fn footerless_review_screen_still_detects() {
        // Captured against claude v2.1.220 (EXP-374): the review step lost
        // its "Enter to select" footer, the rule AND the "Chat about this"
        // row. Requiring the footer stranded every remote multi-question ask
        // on this screen — the fully answered tab bar anchors it instead.
        let lines = screen(&[
            "────────────────────────────────────────────",
            "←  ☒ Docs label  ☒ Pricing  ☒ Scope  ☒ Tone  ✔ Submit  →",
            "",
            "Review your answers",
            "",
            " ● The `/docs/coding/` page is titled \"Coding with Claude\" — what",
            "   should it become?",
            "   → \"Coding agents\" (Recommended)",
            " ● Which pricing surface should be treated as canonical?",
            "   → Billing settings page",
            " ● Should the cleanup also rewrite the INSTALL.md runbook?",
            "   → Both",
            " ● What tone should the rewritten docs take?",
            "   → Concise technical",
            "",
            "Ready to submit your answers?",
            "",
            "❯ 1. Submit answers",
            "  2. Cancel",
            "",
        ]);
        let snap = detect(&lines).expect("footerless review detected");
        assert_eq!(snap.text, "Ready to submit your answers?");
        assert!(snap.review);
        assert_eq!(snap.current_tab, Some(4));
        assert_eq!(
            snap.options
                .iter()
                .map(|o| (o.key.as_str(), o.label.as_str()))
                .collect::<Vec<_>>(),
            vec![("1", "Submit answers"), ("2", "Cancel")]
        );
    }

    #[test]
    fn footerless_question_tab_is_still_rejected() {
        // The footer requirement stays for ordinary tabs (unanswered ones in
        // the bar) — it is what keeps footer-less lookalikes out.
        let mut lines = toppings_screen();
        lines.retain(|l| !l.contains(FOOTER_ANCHOR));
        assert_eq!(detect(&lines), None);
    }

    /// Captured against claude v2.1.220 on the default 80x24 remote-session
    /// grid (EXP-394): three long option descriptions make the picker taller
    /// than the screen, and the TAB BAR scrolls out of the viewport — the
    /// visible screen starts at the question text.
    fn overflow_screen() -> Vec<String> {
        screen(&[
            "For the in-film readability on phones, which approach should this PR take?",
            "❯ 1. Mobile camera zoom (Recommended)",
            "     Thread a small flag from LoopMoviePlayer through Reel into the segments and",
            "     give each one a tighter, refocused Camera key set - crop off the",
            "     rail/outer chrome, focus the issue list + detail. This is the crop-and-zoom",
            "     equivalent of your narrow-window screenshot, without relaying out the",
            "     mocks. Needs a mobile poster regenerated too. Verified by rendering stills",
            "     at each chapter and looking at them.",
            "  2. Sizing only, film untouched",
            "     Ship just the layout fix (gap + bottom-bar clipping + bigger film) and file",
            "     the in-film zoom as a separate issue. Smallest, safest PR; phones still",
            "     show unreadable 11px mock type.",
            "  3. 1:1 mobile composition",
            "     A second Remotion composition at 1:1 for phones. Every segment is laid out",
            "     in 1920x1080 comp coords with hardcoded positions, so this is effectively",
            "     re-authoring all five scenes - very large, and doubles the maintenance",
            "     surface forever. I would avoid it.",
            "  4. Type something.",
            "────────────────────────────────────────────────────────────────────────────────",
            "  5. Chat about this",
            "Enter to select · Tab/Arrow keys to navigate · Esc to cancel",
        ])
    }

    #[test]
    fn overflowing_question_screen_detects_only_with_the_hook_voucher() {
        // EXP-394: without a hook saying an ask is pending, the tab bar stays
        // a hard anchor — the overflowed screen is rejected …
        assert_eq!(detect(&overflow_screen()), None);
        // … but during a hook-confirmed ask the footer alone anchors it.
        let snap = detect_during_ask(&overflow_screen()).expect("overflowed picker detected");
        assert_eq!(
            snap.text,
            "For the in-film readability on phones, which approach should this PR take?"
        );
        assert_eq!(
            snap.options
                .iter()
                .map(|o| (o.key.as_str(), o.label.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("1", "Mobile camera zoom (Recommended)"),
                ("2", "Sizing only, film untouched"),
                ("3", "1:1 mobile composition"),
                ("4", "Type something."),
            ]
        );
        assert!(snap.tabs.is_empty());
        assert_eq!(snap.current_tab, None);
        assert!(!snap.review);
    }

    #[test]
    fn overflowing_footerless_review_detects_via_its_submit_row() {
        // The ≥2.1.220 review step has no footer (EXP-374); overflowing it
        // also scrolls the tab bar off. With both anchors gone the submit
        // option row is what remains — accepted only during a pending ask.
        let lines = screen(&[
            "   → \"Coding agents\" (Recommended)",
            " ● Which pricing surface should be treated as canonical?",
            "   → Billing settings page",
            "",
            "Ready to submit your answers?",
            "",
            "❯ 1. Submit answers",
            "  2. Cancel",
            "",
        ]);
        assert_eq!(detect(&lines), None);
        let snap = detect_during_ask(&lines).expect("overflowed review detected");
        assert_eq!(snap.text, "Ready to submit your answers?");
        assert_eq!(
            snap.options
                .iter()
                .map(|o| o.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Submit answers", "Cancel"]
        );
    }

    #[test]
    fn preview_layout_options_are_cut_at_the_pane_border() {
        // Captured against claude v2.1.220 at 120x40 (EXP-394): an option
        // carrying a `preview` renders SIDE-BY-SIDE — narrow option column
        // left, framed preview pane right — so option rows carry pane
        // borders/content, labels wrap inside the column, and the footer
        // gains "n to add notes · Tab to switch questions".
        let lines = screen(&[
            "────────────────────────────────────────────────────────────",
            "←  ☐ Readability  ☐ Mobile width  ✔ Submit  →",
            "For the in-film readability on phones, which approach should this PR take?",
            "❯ 1. Mobile camera zoom           ┌───────────────────────────────────────────────┐",
            "    (Recommended)                 │ boardlive  s: 1.12 -> ~1.9, focus x/y retuned │",
            "  2. Sizing only, film            │ codeeverywhere  s: 1.06 -> ~1.7               │",
            "    untouched                     │ reviewmerge  s: 1.12 -> ~1.9                  │",
            "  3. 1:1 mobile composition       │ feedback  s: 1.45 -> ~2.0                     │",
            "                                  │ platforms  (no Camera) -> own scale wrapper   │",
            "                                  │                                               │",
            "                                  │ - loop-poster-mobile.webp, picture swap       │",
            "                                  └───────────────────────────────────────────────┘",
            "                                  Notes: press n to add notes",
            "────────────────────────────────────────────────────────────",
            "  Chat about this",
            "Enter to select · ↑/↓ to navigate · n to add notes · Tab to switch questions · Esc to cancel",
        ]);
        let snap = detect(&lines).expect("preview picker detected");
        assert_eq!(
            snap.text,
            "For the in-film readability on phones, which approach should this PR take?"
        );
        // Pane borders/content never leak into the labels (the wrapped label
        // tails are lost to the column — answer routing is by KEY, and the
        // hook already published the full labels).
        assert_eq!(
            snap.options
                .iter()
                .map(|o| (o.key.as_str(), o.label.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("1", "Mobile camera zoom"),
                ("2", "Sizing only, film"),
                ("3", "1:1 mobile composition"),
            ]
        );
        assert_eq!(snap.current_tab, Some(0));
        assert!(!snap.review);
    }

    #[test]
    fn during_ask_detection_still_needs_an_anchor() {
        // No tab bar, no footer, no submit row — a plain ❯-marked numbered
        // list stays rejected even while an ask is pending.
        let lines = screen(&[
            "Pick a database:",
            "❯ 1. Postgres",
            "  2. SQLite",
        ]);
        assert_eq!(detect_during_ask(&lines), None);
    }

    #[test]
    fn an_ordinary_tab_is_never_mistaken_for_the_review_step() {
        // "Submit the form?" as a QUESTION, on its own unanswered tab: the
        // option wording alone must not flip `review`.
        let lines = screen(&[
            "←  ☐ Deploy  ✔ Submit  →",
            "",
            "Submit the form?",
            "",
            "❯ 1. Submit now",
            "  2. Wait",
            "",
            "Enter to select · Tab/Arrow keys to navigate · Esc to cancel",
        ]);
        let snap = detect(&lines).expect("picker detected");
        assert_eq!(snap.current_tab, Some(0));
        assert!(!snap.review);
    }

    #[test]
    fn normalization_is_whitespace_insensitive() {
        assert_eq!(
            normalize_question_text("Which  color\ndo you prefer?"),
            normalize_question_text("Which color do you prefer?")
        );
    }

    #[test]
    fn watcher_debounces_and_refires_on_question_change() {
        let color = detect(&color_screen()).unwrap();
        let toppings = detect(&toppings_screen()).unwrap();
        let mut w = QuestionPickerWatcher::new();

        assert_eq!(w.tick(None, 0), None);
        // First sighting: debounce.
        assert_eq!(w.tick(Some(color.clone()), 0), None);
        // Second consecutive sighting: fire.
        assert_eq!(w.tick(Some(color.clone()), 0).map(|s| s.text), Some(color.text.clone()));
        // Steady state: silent.
        assert_eq!(w.tick(Some(color.clone()), 0), None);
        // The visible question changes (tab advance): debounce, then fire.
        assert_eq!(w.tick(Some(toppings.clone()), 0), None);
        assert_eq!(
            w.tick(Some(toppings.clone()), 0).map(|s| s.text),
            Some(toppings.text.clone())
        );
        // Absence clears pending; the same question later re-fires.
        assert_eq!(w.tick(None, 0), None);
        assert_eq!(w.tick(None, 0), None);
        assert_eq!(w.tick(Some(toppings.clone()), 0), None);
        assert!(w.tick(Some(toppings.clone()), 0).is_some());
    }

    #[test]
    fn watcher_ignores_checkbox_toggles() {
        let mut toggled = toppings_screen();
        let mut w = QuestionPickerWatcher::new();
        w.tick(detect(&toggled), 0);
        assert!(w.tick(detect(&toggled), 0).is_some());
        // Toggling a checkbox changes the options but not the question — the
        // steady state must hold.
        toggled[6] = "  2. [✔] Ham".into();
        assert_eq!(w.tick(detect(&toggled), 0), None);
    }

    #[test]
    fn watcher_freezes_while_scrolled() {
        let color = detect(&color_screen()).unwrap();
        let mut w = QuestionPickerWatcher::new();
        assert_eq!(w.tick(Some(color.clone()), 3), None);
        assert_eq!(w.tick(Some(color.clone()), 3), None);
        // Back at the bottom: the debounce starts fresh.
        assert_eq!(w.tick(Some(color.clone()), 0), None);
        assert!(w.tick(Some(color), 0).is_some());
    }

    #[test]
    fn one_flicker_frame_does_not_reset_pending() {
        let color = detect(&color_screen()).unwrap();
        let mut w = QuestionPickerWatcher::new();
        w.tick(Some(color.clone()), 0);
        assert!(w.tick(Some(color.clone()), 0).is_some());
        // One absent frame (mid-render poll), then the picker again — no
        // re-fire, still the same pending question.
        assert_eq!(w.tick(None, 0), None);
        assert_eq!(w.tick(Some(color), 0), None);
    }
}
