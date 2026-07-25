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
//! rule (so the synthetic "Chat about this" is never offered). Plan-approval
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

    // Anchors: a tab-bar line above the options, the footer below them.
    let tab_idx = lines[..first_idx].iter().rposition(|l| is_tab_line(l.trim()))?;
    lines[last_option_idx + 1..]
        .iter()
        .position(|l| l.contains(FOOTER_ANCHOR))?;

    // Question text: the contiguous non-blank block right above the options
    // (long questions wrap — re-join the lines), bounded by the tab bar.
    let mut text_lines: Vec<&str> = Vec::new();
    for line in lines[tab_idx + 1..first_idx].iter().rev() {
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

    let tabs = parse_tab_bar(&lines[tab_idx]);
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
