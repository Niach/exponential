//! Claude permission-dialog detection on the live terminal grid (EXP-455).
//!
//! A session launched WITHOUT `--dangerously-skip-permissions` blocks on
//! claude's interactive permission dialog ("Do you want to proceed?") — a
//! screen that renders only on the grid, never in the transcript, exactly
//! like the plan picker (EXP-150) and the login flow (EXP-430). The hooks
//! sidecar's permission-flavored `Notification` says a prompt is UP but not
//! what it offers, so remote viewers used to get an informational card with
//! nothing to press and the session dead-ended. The emitter watches the grid
//! instead:
//!
//! * [`detect`] recognizes the dialog on a plain-text screen snapshot
//!   ([`terminal::screen_lines`]) and parses the REAL option labels/keys off
//!   the numbered rows plus the question, the tool headline and the context
//!   lines (the command / file preview) — version-proof against `claude`
//!   re-wording the options.
//! * [`PermissionPickerWatcher`] is the snapshot-keyed per-session state
//!   machine, debounced like [`crate::login_picker::LoginWatcher`]: a CHANGED
//!   dialog re-fires [`Transition::Show`] without an intervening
//!   [`Transition::Resolved`], so back-to-back prompts for successive tools
//!   each get a fresh card.
//!
//! Anchors were captured live from claude v2.0.42 (`Bash` and `Write`
//! dialogs; the option run tolerates the wrapped labels long "don't ask
//! again for … in <cwd>" rows produce) and re-verified against v2.1.233
//! (EXP-512), whose dialogs add a "This command requires approval" line
//! between the context and the question and frame file previews with `╌`
//! rules instead of a bordered box — the header walk skips interior rules
//! so the tool headline still resolves.

use crate::frames::QuestionOption;
use crate::{login_picker, plan_picker, question_picker};

/// The question line every observed permission dialog carries ("Do you want
/// to proceed?", "Do you want to create notes.txt?", "Do you want to make
/// this edit to …?"). It must sit within [`ANCHOR_WINDOW`] rows above the
/// option run — "Do you want to" is ordinary prose an agent can echo, so a
/// far-away match must never anchor an unrelated numbered list.
const ANCHOR: &str = "Do you want to";
const ANCHOR_WINDOW: usize = 5;

/// The selection cursor `claude` renders on the highlighted option row —
/// required so echoed markdown can never look like a live picker.
const SELECTION_MARKER: char = '❯';

/// How many consecutive non-option rows the option-run walk skips as wrapped
/// label continuations (captured: the cwd path of "don't ask again for curl
/// commands in <cwd>" wraps over two rows).
const CONTINUATION_MAX: usize = 3;

/// How far above the question the tool headline may sit (the dialog's rule
/// row, the headline, then the context box).
const HEADER_WINDOW: usize = 20;

/// Cap on captured context lines (the command / file preview between the
/// headline and the question).
const CONTEXT_LINES_MAX: usize = 8;

/// A detected permission dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct PermissionSnapshot {
    /// The dialog's tool headline ("Bash command", "Create file", …) —
    /// best-effort, `None` when the layout drifted.
    pub header: Option<String>,
    /// The context lines between the headline and the question (the command
    /// and its description, the file-preview body), box borders stripped.
    pub context: Vec<String>,
    /// The question line(s) ("Do you want to proceed?"), wraps re-joined.
    pub question: String,
    pub options: Vec<QuestionOption>,
}

/// One state-machine step outcome.
#[derive(Clone, Debug, PartialEq)]
pub enum Transition {
    /// A permission dialog settled on screen (or the settled dialog changed
    /// to a new one) — publish it as an answerable question.
    Show(PermissionSnapshot),
    /// The pending dialog left the grid — answered (locally or remotely) or
    /// dismissed; retire the published question.
    Resolved,
}

/// Detect a permission dialog on a visible-screen snapshot.
///
/// Strategy: find the `❯`-marked option row (bottom-most wins), expand
/// up/down over numbered rows tolerating wrapped-label continuation lines,
/// require the run to start at `1.` with ≥2 options and the "Do you want to"
/// question directly above it. Screens owned by the OTHER pickers are
/// rejected outright — an `AskUserQuestion` whose text contains "Do you want
/// to …" must keep its own (hook-identified) card, never a duplicate.
pub fn detect(lines: &[String]) -> Option<PermissionSnapshot> {
    if plan_picker::detect(lines).is_some()
        || question_picker::detect(lines).is_some()
        || login_picker::detect(lines).is_some()
    {
        return None;
    }

    let marker_idx = lines.iter().rposition(|line| {
        let t = strip_borders(line);
        t.starts_with(SELECTION_MARKER) && parse_option_row(t).is_some()
    })?;
    let marker_number = parse_option_row(strip_borders(&lines[marker_idx]))?.0;

    // Expand upward to option 1, skipping wrapped-label continuation rows.
    let mut first_idx = marker_idx;
    let mut expect = marker_number;
    let mut skipped = 0usize;
    while expect > 1 {
        let prev_idx = first_idx.checked_sub(1)?;
        let prev = strip_borders(&lines[prev_idx]);
        match parse_option_row(prev) {
            Some((n, _)) if n == expect - 1 => {
                first_idx = prev_idx;
                expect = n;
                skipped = 0;
            }
            Some(_) => return None,
            None => {
                if prev.is_empty() || skipped >= CONTINUATION_MAX {
                    return None;
                }
                skipped += 1;
                first_idx = prev_idx;
            }
        }
    }
    // `first_idx` may have walked onto a continuation row; snap to option 1.
    let first_idx = (first_idx..=marker_idx)
        .find(|&idx| matches!(parse_option_row(strip_borders(&lines[idx])), Some((1, _))))?;

    // Collect downward, folding continuation rows into the previous label.
    let mut options: Vec<QuestionOption> = Vec::new();
    let mut next = 1u32;
    let mut skipped = 0usize;
    for line in &lines[first_idx..] {
        let t = strip_borders(line);
        match parse_option_row(t) {
            Some((n, label)) if n == next => {
                options.push(QuestionOption::new(label, n.to_string()));
                next += 1;
                skipped = 0;
            }
            Some(_) => break,
            None => {
                if t.is_empty() || skipped >= CONTINUATION_MAX {
                    break;
                }
                skipped += 1;
                if let Some(last) = options.last_mut() {
                    last.label = format!("{} {t}", last.label);
                }
            }
        }
    }
    if options.len() < 2 || marker_number as usize > options.len() {
        return None;
    }

    // The question must sit directly above the options.
    let anchor_floor = first_idx.saturating_sub(ANCHOR_WINDOW);
    let anchor_idx = (anchor_floor..first_idx)
        .rev()
        .find(|&idx| lines[idx].contains(ANCHOR))?;
    let question = lines[anchor_idx..first_idx]
        .iter()
        .map(|line| strip_borders(line))
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    let (header, context) = header_and_context(&lines[..anchor_idx]);
    Some(PermissionSnapshot {
        header,
        context,
        question,
        options,
    })
}

/// Parse one option row (`❯ 1. Yes` / `3. No, and tell Claude what to do
/// differently (esc)`) into its number and label. The input is
/// border-stripped and trimmed.
fn parse_option_row(row: &str) -> Option<(u32, &str)> {
    let row = row.strip_prefix(SELECTION_MARKER).unwrap_or(row).trim_start();
    let dot = row.find('.')?;
    let number: u32 = row[..dot].parse().ok()?;
    if !(1..=9).contains(&number) {
        return None;
    }
    let label = row[dot + 1..].trim();
    (!label.is_empty()).then_some((number, label))
}

/// Strip the box borders a dialog variant may frame its rows with (the
/// v2.0.42 file preview does; a future claude may box the whole dialog) and
/// trim. A pure frame row (`╭───╮`) strips to empty.
fn strip_borders(line: &str) -> &str {
    line.trim()
        .trim_start_matches(['│', '╭', '╰'])
        .trim_end_matches(['│', '╮', '╯'])
        .trim_matches(|c: char| c == '─' || c.is_whitespace())
}

/// A horizontal-rule row — the dialog's top separator or (light variants) an
/// interior frame line.
fn is_rule(line: &str) -> bool {
    let t = line.trim();
    t.chars().count() >= 10 && t.chars().all(|c| matches!(c, '─' | '═' | '╌' | '┄'))
}

/// A SOLID rule — the dialog's own top separator. The v2.1.233 file preview
/// frames its body with light `╌` rules; those are interior and must never
/// anchor the headline walk.
fn is_heavy_rule(line: &str) -> bool {
    let t = line.trim();
    t.chars().count() >= 10 && t.chars().all(|c| matches!(c, '─' | '═'))
}

/// Best-effort headline + context: the first non-empty row below the nearest
/// HEAVY rule above the question is the tool headline ("Bash command");
/// everything between it and the question is context (borders stripped,
/// frame rows and interior light rules dropped, capped). No heavy rule in
/// reach ⇒ neither.
fn header_and_context(above: &[String]) -> (Option<String>, Vec<String>) {
    let floor = above.len().saturating_sub(HEADER_WINDOW);
    let Some(rule_idx) = (floor..above.len()).rev().find(|&idx| is_heavy_rule(&above[idx]))
    else {
        return (None, Vec::new());
    };
    let mut header = None;
    let mut context = Vec::new();
    for line in &above[rule_idx + 1..] {
        let t = strip_borders(line);
        if t.is_empty() || is_rule(line) {
            continue;
        }
        if header.is_none() {
            header = Some(t.to_string());
        } else if context.len() < CONTEXT_LINES_MAX {
            context.push(t.to_string());
        }
    }
    (header, context)
}

/// Debounce depth, matching the other grid watchers: one mid-render frame
/// must not flap the machine.
const STREAK: u8 = 2;

/// Per-session permission-dialog state machine — see the module docs.
#[derive(Default)]
pub struct PermissionPickerWatcher {
    /// The settled dialog, between its `Show` and its `Resolved`.
    pending: Option<PermissionSnapshot>,
    candidate: Option<PermissionSnapshot>,
    present_streak: u8,
    absent_streak: u8,
}

impl PermissionPickerWatcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a permission dialog is currently pending on screen — the
    /// EXP-214 "needs input" signal.
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Forget the latched dialog WITHOUT a [`Transition::Resolved`]: the
    /// caller swallowed the `Show`, and a latched snapshot would otherwise
    /// stay silent for good if the suppression cause clears while the same
    /// dialog is still on screen — unlatched, the steady dialog re-fires
    /// `Show` a debounce later (EXP-458).
    pub fn unlatch(&mut self) {
        self.pending = None;
        self.candidate = None;
        self.present_streak = 0;
        self.absent_streak = 0;
    }

    /// Feed one poll tick. `display_offset > 0` (viewport scrolled into
    /// history) freezes the machine entirely.
    pub fn tick(&mut self, lines: &[String], display_offset: usize) -> Option<Transition> {
        if display_offset > 0 {
            return None;
        }
        match detect(lines) {
            Some(snapshot) => {
                self.absent_streak = 0;
                if self.pending.as_ref() == Some(&snapshot) {
                    self.candidate = None;
                    self.present_streak = 0;
                    return None;
                }
                if self.candidate.as_ref() == Some(&snapshot) {
                    self.present_streak += 1;
                } else {
                    self.candidate = Some(snapshot);
                    self.present_streak = 1;
                }
                if self.present_streak >= STREAK {
                    let settled = self.candidate.take()?;
                    self.present_streak = 0;
                    self.pending = Some(settled.clone());
                    return Some(Transition::Show(settled));
                }
                None
            }
            None => {
                self.present_streak = 0;
                self.candidate = None;
                self.pending.as_ref()?;
                self.absent_streak += 1;
                if self.absent_streak >= STREAK {
                    self.pending = None;
                    self.absent_streak = 0;
                    return Some(Transition::Resolved);
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

    /// The Bash dialog as captured live on claude v2.0.42 — note the
    /// wrapped "don't ask again" label spanning two continuation rows.
    fn bash_permission_screen() -> Vec<String> {
        screen(&[
            "● Bash(curl -s https://example.com/)",
            "  ⎿  Running…",
            "",
            "────────────────────────────────────────────────────────────────",
            " Bash command",
            "",
            "   curl -s https://example.com/",
            "   Fetch content from example.com",
            "",
            " Do you want to proceed?",
            " ❯ 1. Yes",
            "  2. Yes, and don't ask again for curl commands in",
            "  /home/user/project",
            "  3. Tell Claude what to do differently",
            "",
            "",
            " Esc to exit",
        ])
    }

    /// The Write dialog as captured live on claude v2.0.42 — the file
    /// preview rides its own bordered box.
    fn write_permission_screen() -> Vec<String> {
        screen(&[
            "● Write(notes.txt)",
            "",
            "────────────────────────────────────────────────────────────────",
            " Create file",
            "╭──────────────────────────────────────────────────────────────╮",
            "│ notes.txt                                                    │",
            "│                                                              │",
            "│ hello notes                                                  │",
            "│                                                              │",
            "╰──────────────────────────────────────────────────────────────╯",
            " Do you want to create notes.txt?",
            " ❯ 1. Yes",
            "   2. Yes, allow all edits during this session (shift+tab)",
            "   3. No, and tell Claude what to do differently (esc)",
        ])
    }

    /// The Bash dialog as captured live on claude v2.1.233 (EXP-512) — a
    /// "This command requires approval" line now sits between the context
    /// and the question, and the don't-ask-again label carries the command.
    fn bash_permission_screen_v2_1_233() -> Vec<String> {
        screen(&[
            "❯ Run exactly this with the Bash tool: curl -s https://example.com/ | head -c 100",
            "",
            "● Fetching example.com and printing first 100 bytes",
            "  ⎿  $ curl -s https://example.com/ | head -c 100",
            "",
            "──────────────────────────────────────────────────────────────────────────────────────────",
            " Bash command",
            "",
            "   curl -s https://example.com/ | head -c 100",
            "   Fetch example.com and print first 100 bytes",
            "",
            " This command requires approval",
            "",
            " Do you want to proceed?",
            " ❯ 1. Yes",
            "   2. Yes, and don’t ask again for: curl -s https://example.com/",
            "   3. No",
            "",
            " Esc to cancel · Tab to amend · ctrl+e to explain",
        ])
    }

    /// The same dialog raised by a subagent (claude v2.1.233): the headline
    /// gains a "· from the …" suffix, nothing else moves.
    fn subagent_permission_screen_v2_1_233() -> Vec<String> {
        screen(&[
            "✻ Waiting for 2 background agents to finish",
            "",
            "──────────────────────────────────────────────────────────────────────────────────────────",
            " Bash command · from the general-purpose agent",
            "",
            "   curl -s https://ifconfig.me",
            "   Fetch public IP address",
            "",
            " This command requires approval",
            "",
            " Do you want to proceed?",
            " ❯ 1. Yes",
            "   2. Yes, and don’t ask again for: curl *",
            "   3. No",
            "",
            " Esc to cancel · Tab to amend · ctrl+e to explain",
        ])
    }

    /// The Write dialog as captured live on claude v2.1.233: the file
    /// preview rides between `╌` rules instead of a bordered box.
    fn write_permission_screen_v2_1_233() -> Vec<String> {
        screen(&[
            "● Write(notes.txt)",
            "",
            "──────────────────────────────────────────────────────────────────────────────────────────",
            " Create file",
            " notes.txt",
            "╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌",
            "  1 hello notes",
            "╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌",
            " Do you want to create notes.txt?",
            " ❯ 1. Yes",
            "   2. Yes, allow all edits during this session (shift+tab)",
            "   3. No",
            "",
            " Esc to cancel · Tab to amend",
        ])
    }

    #[test]
    fn detects_the_v2_1_233_bash_dialog() {
        let snap = detect(&bash_permission_screen_v2_1_233()).expect("dialog detected");
        assert_eq!(snap.header.as_deref(), Some("Bash command"));
        assert_eq!(
            snap.context,
            vec![
                "curl -s https://example.com/ | head -c 100",
                "Fetch example.com and print first 100 bytes",
                "This command requires approval",
            ]
        );
        assert_eq!(snap.question, "Do you want to proceed?");
        assert_eq!(
            snap.options
                .iter()
                .map(|o| (o.key.as_str(), o.label.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("1", "Yes"),
                ("2", "Yes, and don’t ask again for: curl -s https://example.com/"),
                ("3", "No"),
            ]
        );
    }

    #[test]
    fn detects_the_v2_1_233_subagent_dialog() {
        let snap = detect(&subagent_permission_screen_v2_1_233()).expect("dialog detected");
        assert_eq!(
            snap.header.as_deref(),
            Some("Bash command · from the general-purpose agent")
        );
        assert_eq!(snap.options.len(), 3);
    }

    #[test]
    fn detects_the_v2_1_233_write_dialog_through_the_ruled_preview() {
        let snap = detect(&write_permission_screen_v2_1_233()).expect("dialog detected");
        // The nearest rule above the question closes the preview and yields
        // no headline — the walk must settle on the outer rule instead.
        assert_eq!(snap.header.as_deref(), Some("Create file"));
        assert_eq!(snap.context, vec!["notes.txt", "1 hello notes"]);
        assert_eq!(snap.question, "Do you want to create notes.txt?");
        assert_eq!(snap.options.len(), 3);
    }

    #[test]
    fn anchor_less_setup_dialogs_stay_undetected() {
        // v2.1.233's "Set up auto mode?" prompt has numbered rows and a ❯
        // marker but no "Do you want to" question — it is NOT a permission
        // dialog and must never publish as one.
        let lines = screen(&[
            "──────────────────────────────────────────────────────────────────────────────────────────",
            "  Set up auto mode for your environment?",
            "",
            "  Auto mode lets Claude act without asking first.",
            "",
            "  ❯ 1. Set it up",
            "    2. Not now",
            "    3. Don't show again",
            "",
            "  Enter to confirm · Esc to cancel",
        ]);
        assert_eq!(detect(&lines), None);
    }

    #[test]
    fn detects_the_bash_dialog_with_wrapped_labels() {
        let snap = detect(&bash_permission_screen()).expect("dialog detected");
        assert_eq!(snap.header.as_deref(), Some("Bash command"));
        assert_eq!(
            snap.context,
            vec!["curl -s https://example.com/", "Fetch content from example.com"]
        );
        assert_eq!(snap.question, "Do you want to proceed?");
        assert_eq!(
            snap.options
                .iter()
                .map(|o| (o.key.as_str(), o.label.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("1", "Yes"),
                (
                    "2",
                    "Yes, and don't ask again for curl commands in /home/user/project"
                ),
                ("3", "Tell Claude what to do differently"),
            ]
        );
    }

    #[test]
    fn detects_the_write_dialog_through_the_preview_box() {
        let snap = detect(&write_permission_screen()).expect("dialog detected");
        assert_eq!(snap.header.as_deref(), Some("Create file"));
        assert_eq!(snap.context, vec!["notes.txt", "hello notes"]);
        assert_eq!(snap.question, "Do you want to create notes.txt?");
        assert_eq!(snap.options.len(), 3);
        assert_eq!(
            snap.options[2].label,
            "No, and tell Claude what to do differently (esc)"
        );
    }

    #[test]
    fn selection_marker_may_sit_on_any_option() {
        let mut lines = write_permission_screen();
        lines[11] = "   1. Yes".to_string();
        lines[13] = " ❯ 3. No, and tell Claude what to do differently (esc)".to_string();
        let snap = detect(&lines).expect("dialog detected");
        assert_eq!(snap.options.len(), 3);
    }

    #[test]
    fn marker_above_a_wrapped_label_still_expands_to_option_one() {
        let mut lines = bash_permission_screen();
        lines[10] = "  1. Yes".to_string();
        lines[13] = " ❯ 3. Tell Claude what to do differently".to_string();
        let snap = detect(&lines).expect("dialog detected");
        assert_eq!(snap.options.len(), 3);
        assert_eq!(snap.options[0].label, "Yes");
    }

    #[test]
    fn no_marker_means_no_dialog() {
        let mut lines = bash_permission_screen();
        lines[10] = "  1. Yes".to_string();
        assert_eq!(detect(&lines), None);
    }

    #[test]
    fn options_without_the_question_are_ignored() {
        let lines = screen(&[
            "Pick a database:",
            "❯ 1. Postgres",
            "  2. SQLite",
        ]);
        assert_eq!(detect(&lines), None);
    }

    #[test]
    fn a_far_away_do_you_want_never_anchors_an_unrelated_list() {
        // "Do you want to" is ordinary prose — echoed high on the screen it
        // must not claim a numbered list at the bottom.
        let lines = screen(&[
            "Do you want to try one of these approaches?",
            "",
            "",
            "",
            "",
            "",
            "Pick one:",
            "❯ 1. Postgres",
            "  2. SQLite",
        ]);
        assert_eq!(detect(&lines), None);
    }

    #[test]
    fn other_pickers_own_their_screens() {
        // A plan picker, an AskUserQuestion picker and the login picker must
        // never double-publish as a permission dialog — even when their text
        // contains the permission anchor.
        let plan = screen(&[
            " Claude has written up a plan and is ready to execute. Would you like to",
            " proceed?",
            "",
            " ❯ 1. Yes, auto-accept edits",
            "   2. Yes, manually approve edits",
        ]);
        assert!(plan_picker::detect(&plan).is_some());
        assert_eq!(detect(&plan), None);

        let ask = screen(&[
            "←  ☐ Approach  →",
            "",
            "Do you want to keep the legacy path?",
            "",
            "❯ 1. Yes",
            "  2. No",
            "",
            "Enter to select · Tab/Arrow keys to navigate · Esc to cancel",
        ]);
        assert!(question_picker::detect(&ask).is_some());
        assert_eq!(detect(&ask), None);

        let login = screen(&[
            "   Select login method:",
            "",
            "   ❯ 1. Claude account with subscription · Pro, Max, Team, or Enterprise",
            "     2. Anthropic Console account · API usage billing",
        ]);
        assert!(login_picker::detect(&login).is_some());
        assert_eq!(detect(&login), None);

        // And the permission screens stay out of the other detectors.
        for lines in [bash_permission_screen(), write_permission_screen()] {
            assert_eq!(plan_picker::detect(&lines), None);
            assert_eq!(question_picker::detect(&lines), None);
            assert_eq!(login_picker::detect(&lines), None);
        }
    }

    #[test]
    fn watcher_debounces_show_and_resolve() {
        let dialog = bash_permission_screen();
        let blank = screen(&["$ claude", "✳ Deliberating…"]);
        let mut w = PermissionPickerWatcher::new();

        assert_eq!(w.tick(&blank, 0), None);
        assert_eq!(w.tick(&dialog, 0), None);
        match w.tick(&dialog, 0) {
            Some(Transition::Show(snap)) => assert_eq!(snap.options.len(), 3),
            other => panic!("expected Show, got {other:?}"),
        }
        assert!(w.is_pending());
        // Steady state: silent.
        assert_eq!(w.tick(&dialog, 0), None);
        // One absent frame: still pending (debounce).
        assert_eq!(w.tick(&blank, 0), None);
        assert!(w.is_pending());
        assert_eq!(w.tick(&blank, 0), Some(Transition::Resolved));
        assert!(!w.is_pending());
        assert_eq!(w.tick(&blank, 0), None);
    }

    #[test]
    fn a_changed_dialog_refires_show_without_resolved() {
        // Back-to-back prompts: approve the Bash one locally, the Write one
        // paints in its place — the new dialog gets a fresh Show.
        let mut w = PermissionPickerWatcher::new();
        w.tick(&bash_permission_screen(), 0);
        assert!(matches!(
            w.tick(&bash_permission_screen(), 0),
            Some(Transition::Show(_))
        ));
        assert_eq!(w.tick(&write_permission_screen(), 0), None);
        match w.tick(&write_permission_screen(), 0) {
            Some(Transition::Show(snap)) => {
                assert_eq!(snap.header.as_deref(), Some("Create file"))
            }
            other => panic!("expected Show, got {other:?}"),
        }
        assert!(w.is_pending());
    }

    #[test]
    fn an_unlatched_dialog_refires_show_while_it_persists() {
        // EXP-458: the emitter swallows a Show while an ask/plan owns the
        // screen story — unlatching lets the still-up dialog re-fire so the
        // question publishes once the suppression lifts.
        let dialog = bash_permission_screen();
        let blank = screen(&["$ claude", "✳ Deliberating…"]);
        let mut w = PermissionPickerWatcher::new();

        w.tick(&dialog, 0);
        assert!(matches!(w.tick(&dialog, 0), Some(Transition::Show(_))));
        w.unlatch();
        assert!(!w.is_pending());
        // The steady dialog re-fires after the usual debounce.
        assert_eq!(w.tick(&dialog, 0), None);
        assert!(matches!(w.tick(&dialog, 0), Some(Transition::Show(_))));
        assert!(w.is_pending());

        // And an unlatched dialog leaving the grid never emits a phantom
        // Resolved for the Show nobody published.
        w.unlatch();
        assert_eq!(w.tick(&blank, 0), None);
        assert_eq!(w.tick(&blank, 0), None);
        assert!(!w.is_pending());
    }

    #[test]
    fn watcher_freezes_while_scrolled() {
        let dialog = bash_permission_screen();
        let blank = screen(&["$"]);
        let mut w = PermissionPickerWatcher::new();

        // A dialog seen only in scrolled-back history never Shows.
        assert_eq!(w.tick(&dialog, 3), None);
        assert_eq!(w.tick(&dialog, 3), None);
        assert_eq!(w.tick(&blank, 0), None);

        // A pending dialog scrolled out of view never Resolves.
        w.tick(&dialog, 0);
        assert!(matches!(w.tick(&dialog, 0), Some(Transition::Show(_))));
        assert_eq!(w.tick(&blank, 5), None);
        assert_eq!(w.tick(&blank, 5), None);
        assert!(w.is_pending());
        assert_eq!(w.tick(&dialog, 0), None);
    }

    #[test]
    fn a_half_painted_dialog_does_not_settle() {
        let mut w = PermissionPickerWatcher::new();
        let mut half = bash_permission_screen();
        half.truncate(12); // options cut mid-run
        assert_eq!(w.tick(&half, 0), None);
        assert_eq!(w.tick(&bash_permission_screen(), 0), None);
        assert!(matches!(
            w.tick(&bash_permission_screen(), 0),
            Some(Transition::Show(_))
        ));
    }
}
