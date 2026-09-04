//! Codex approval-dialog detection on the live terminal grid (EXP-455).
//!
//! Codex exec/patch/permission approvals are never persisted to the rollout
//! (`codex_activity` module docs) — the modal renders only in the TUI's
//! bottom pane, so a `--ask-for-approval on-request` session steered
//! remotely used to dead-end on it with no signal at all. Like the claude
//! pickers, the emitter watches the grid:
//!
//! * [`detect`] recognizes the approval overlay on a plain-text screen
//!   snapshot and parses the REAL option labels/keys off the numbered rows
//!   plus the title and the context lines (reason / permission rule / the
//!   `$ command` line).
//! * [`CodexApprovalWatcher`] is the snapshot-keyed state machine (debounced
//!   like the claude watchers; a CHANGED dialog re-fires
//!   [`Transition::Show`] without an intervening [`Transition::Resolved`]).
//!
//! Anchors verified against the codex-cli 0.144.5 source and its committed
//! render snapshots (`codex-rs/tui/src/bottom_pane/approval_overlay.rs` +
//! `snapshots/…approval_overlay…`): the title is one of four fixed phrases
//! (plus the MCP-elicitation `<server> needs your approval.`), rows render
//! `› 1. Yes, proceed (y)` with the `›` cursor, and the footer is
//! `Press enter to confirm or esc to cancel`. The `request_user_input`
//! bottom pane — same row shape, arbitrary question text — carries its own
//! `… enter to submit answer …` footer: [`detect`] rejects it outright (the
//! rollout already published those questions as cards), and [`detect_ask`]
//! reads it instead, so a remote answer to one of those cards can be
//! actuated on the grid.

use crate::frames::QuestionOption;

/// The approval overlay's title phrases (codex-cli 0.144.5,
/// `approval_overlay.rs`). Fragments chosen to survive word-wrap.
const TITLE_ANCHORS: &[&str] = &[
    "Would you like to run the following command",
    "Would you like to make the following edits",
    "Would you like to grant these permissions",
    "Do you want to approve network access",
    "needs your approval",
];

/// The approval overlay's footer — required below the options, so an echoed
/// numbered list under approval-ish prose can never look like a live modal.
const FOOTER_ANCHOR: &str = "Press enter to confirm";

/// The `request_user_input` bottom pane's footer — that picker owns its
/// screen (the rollout already published its questions as cards), and it is
/// the anchor [`detect_ask`] keys on.
const ASK_FOOTER: &str = "enter to submit answer";

/// The `request_user_input` pane's step header (`Question 2/3 (1 unanswered)`).
const STEP_ANCHOR: &str = "Question ";

/// The selection cursor codex renders on the highlighted option row.
const SELECTION_MARKER: char = '›';

/// How many consecutive non-option rows the option-run walk skips as wrapped
/// label continuations.
const CONTINUATION_MAX: usize = 3;

/// How far below the last option the footer may sit.
const FOOTER_WINDOW: usize = 4;

/// Cap on captured context lines (reason / permission rule / command).
const CONTEXT_LINES_MAX: usize = 8;

/// A detected approval overlay.
#[derive(Clone, Debug, PartialEq)]
pub struct ApprovalSnapshot {
    /// The title line ("Would you like to run the following command?").
    pub title: String,
    /// The lines between the title and the options (`Reason: …`,
    /// `Permission rule: …`, `$ cat /tmp/readme.txt`), capped.
    pub context: Vec<String>,
    pub options: Vec<QuestionOption>,
}

/// One state-machine step outcome.
#[derive(Clone, Debug, PartialEq)]
pub enum Transition {
    /// An approval overlay settled on screen (or the settled one changed) —
    /// publish it as an answerable question.
    Show(ApprovalSnapshot),
    /// The pending overlay left the grid — answered or dismissed; retire the
    /// published question.
    Resolved,
}

/// Detect an approval overlay on a visible-screen snapshot.
pub fn detect(lines: &[String]) -> Option<ApprovalSnapshot> {
    if lines.iter().any(|line| line.contains(ASK_FOOTER)) {
        return None;
    }

    let marker_idx = lines.iter().rposition(|line| {
        let t = line.trim_start();
        t.starts_with(SELECTION_MARKER) && parse_option_row(t).is_some()
    })?;
    let marker_number = parse_option_row(lines[marker_idx].trim_start())?.0;
    let first_idx = first_option_idx(lines, marker_idx, marker_number)?;
    let (options, last_option_idx) = collect_options(lines, first_idx);
    if options.len() < 2 || marker_number as usize > options.len() {
        return None;
    }

    // The confirm footer must sit right below the options.
    let footer_end = (last_option_idx + 1 + FOOTER_WINDOW).min(lines.len());
    if !lines[last_option_idx + 1..footer_end]
        .iter()
        .any(|line| line.contains(FOOTER_ANCHOR))
    {
        return None;
    }

    // A title anchor must sit above the options.
    let title_idx = lines[..first_idx]
        .iter()
        .rposition(|line| TITLE_ANCHORS.iter().any(|a| line.contains(a)))?;
    let title = lines[title_idx].trim().to_string();
    let context: Vec<String> = lines[title_idx + 1..first_idx]
        .iter()
        .map(|line| line.trim())
        .filter(|t| !t.is_empty())
        .take(CONTEXT_LINES_MAX)
        .map(str::to_string)
        .collect();

    Some(ApprovalSnapshot {
        title,
        context,
        options,
    })
}

/// Walk UP from the cursor row to the row holding option 1, skipping wrapped
/// label continuations. Shared by both pickers — their rows render alike.
fn first_option_idx(lines: &[String], marker_idx: usize, marker_number: u32) -> Option<usize> {
    let mut first_idx = marker_idx;
    let mut expect = marker_number;
    let mut skipped = 0usize;
    while expect > 1 {
        let prev_idx = first_idx.checked_sub(1)?;
        let prev = lines[prev_idx].trim_start();
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
    (first_idx..=marker_idx)
        .find(|&idx| matches!(parse_option_row(lines[idx].trim_start()), Some((1, _))))
}

/// Collect the option run DOWNWARD from option 1, folding continuation rows
/// into the previous label. Returns the options and the last row they occupy.
fn collect_options(lines: &[String], first_idx: usize) -> (Vec<QuestionOption>, usize) {
    let mut options: Vec<QuestionOption> = Vec::new();
    let mut next = 1u32;
    let mut skipped = 0usize;
    let mut last_option_idx = first_idx;
    for (idx, line) in lines.iter().enumerate().skip(first_idx) {
        let t = line.trim_start();
        match parse_option_row(t) {
            Some((n, label)) if n == next => {
                options.push(QuestionOption::new(label, n.to_string()));
                next += 1;
                skipped = 0;
                last_option_idx = idx;
            }
            Some(_) => break,
            None => {
                if t.is_empty() || skipped >= CONTINUATION_MAX {
                    break;
                }
                skipped += 1;
                if let Some(last) = options.last_mut() {
                    last.label = format!("{} {}", last.label, t.trim());
                    last_option_idx = idx;
                }
            }
        }
    }
    (options, last_option_idx)
}

/// The live `request_user_input` pane (EXP-455 sibling): the picker codex
/// renders for the questions the ROLLOUT already published as cards, so it is
/// never published again from here — this is the actuation surface a remote
/// answer needs.
///
/// It differs from the approval overlay in the one way that matters: its
/// footer says `enter to submit answer`, i.e. picking a row does NOT submit
/// it. So an answer moves the cursor and confirms with Enter, and
/// [`AskPaneSnapshot::selected`] is the proof the move landed BEFORE Enter is
/// ever sent — no blind submit can pick the wrong row.
#[derive(Clone, Debug, PartialEq)]
pub struct AskPaneSnapshot {
    /// The `Question 2/3` header codex renders for a multi-question ask: the
    /// 1-based step and the ask's question count. Absent on a pane that
    /// renders no header.
    pub step: Option<(u32, u32)>,
    /// The 1-based option row the `›` cursor sits on.
    pub selected: u32,
    pub options: Vec<QuestionOption>,
}

/// Detect the `request_user_input` pane on a visible-screen snapshot.
pub fn detect_ask(lines: &[String]) -> Option<AskPaneSnapshot> {
    let footer_idx = lines.iter().rposition(|line| line.contains(ASK_FOOTER))?;
    let marker_idx = lines[..footer_idx].iter().rposition(|line| {
        let t = line.trim_start();
        t.starts_with(SELECTION_MARKER) && parse_option_row(t).is_some()
    })?;
    let selected = parse_option_row(lines[marker_idx].trim_start())?.0;
    let first_idx = first_option_idx(lines, marker_idx, selected)?;
    let (options, last_option_idx) = collect_options(lines, first_idx);
    // One option is a legitimate ask (unlike an approval, which always
    // offers a refusal), but the cursor must sit on a row that exists.
    if options.is_empty() || selected as usize > options.len() {
        return None;
    }
    // The footer must sit right below the option run — a numbered list
    // echoed further up the scrollback is not a live pane.
    if footer_idx <= last_option_idx || footer_idx - last_option_idx > FOOTER_WINDOW {
        return None;
    }
    let step = lines[..first_idx]
        .iter()
        .rev()
        .take(FOOTER_WINDOW)
        .find_map(|line| parse_step(line));
    Some(AskPaneSnapshot {
        step,
        selected,
        options,
    })
}

/// `Question 2/3 (1 unanswered)` → `(2, 3)`.
fn parse_step(line: &str) -> Option<(u32, u32)> {
    let rest = line.trim().strip_prefix(STEP_ANCHOR)?;
    let (step, total) = rest.split_whitespace().next()?.split_once('/')?;
    Some((step.parse().ok()?, total.parse().ok()?))
}

/// Parse one option row (`› 1. Yes, proceed (y)`) into its number and label.
/// The input is trim_start-ed.
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

/// Debounce depth, matching the claude grid watchers.
const STREAK: u8 = 2;

/// Per-session approval-overlay state machine — see the module docs.
#[derive(Default)]
pub struct CodexApprovalWatcher {
    /// The settled overlay, between its `Show` and its `Resolved`.
    pending: Option<ApprovalSnapshot>,
    candidate: Option<ApprovalSnapshot>,
    present_streak: u8,
    absent_streak: u8,
}

impl CodexApprovalWatcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether an approval overlay is currently pending on screen — the
    /// EXP-214 "needs input" signal.
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
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

    /// The exec approval overlay, verbatim from the codex-cli 0.144.5
    /// committed render snapshot (`…additional_permissions_prompt.snap`).
    fn exec_approval_screen() -> Vec<String> {
        screen(&[
            "",
            "  Would you like to run the following command?",
            "",
            "  Reason: need filesystem access",
            "",
            "  Permission rule: network; read `/tmp/readme.txt`; write `/tmp/out.txt`",
            "",
            "  $ cat /tmp/readme.txt",
            "",
            "› 1. Yes, proceed (y)",
            "  2. No, and tell Codex what to do differently (esc)",
            "",
            "  Press enter to confirm or esc to cancel",
        ])
    }

    /// The network approval overlay, verbatim from `…network_exec_prompt.snap`.
    fn network_approval_screen() -> Vec<String> {
        screen(&[
            "",
            "  Do you want to approve network access to \"example.com\"?",
            "",
            "  Reason: network request blocked",
            "",
            "",
            "› 1. Yes, just this once (y)",
            "  2. Yes, and allow this host for this conversation (a)",
            "  3. Yes, and allow this host in the future (p)",
            "  4. No, and tell Codex what to do differently (esc)",
            "",
            "  Press enter to confirm or esc to cancel",
        ])
    }

    /// The permissions overlay, verbatim from `…permissions_prompt.snap`.
    fn permissions_approval_screen() -> Vec<String> {
        screen(&[
            "",
            "  Would you like to grant these permissions?",
            "",
            "  Reason: need workspace access",
            "",
            "  Permission rule: network; read `/tmp/readme.txt`; write `/tmp/out.txt`",
            "",
            "› 1. Yes, grant these permissions for this turn (y)",
            "  2. Yes, grant for this turn with strict auto review (r)",
            "  3. Yes, grant these permissions for this session (a)",
            "  4. No, continue without permissions (d)",
            "",
            "  Press enter to confirm or esc to cancel",
        ])
    }

    /// A `request_user_input` picker, verbatim from its render snapshot —
    /// same row shape, its own footer.
    fn request_user_input_screen() -> Vec<String> {
        screen(&[
            "",
            "  Question 1/1 (1 unanswered)",
            "  Would you like to run the tests now?",
            "",
            "  › 1. Option 1  First choice.",
            "    2. Option 2  Second choice.",
            "",
            "  tab to add notes | enter to submit answer | esc to interrupt",
        ])
    }

    #[test]
    fn detects_the_exec_approval_with_real_labels() {
        let snap = detect(&exec_approval_screen()).expect("overlay detected");
        assert_eq!(snap.title, "Would you like to run the following command?");
        assert_eq!(
            snap.context,
            vec![
                "Reason: need filesystem access",
                "Permission rule: network; read `/tmp/readme.txt`; write `/tmp/out.txt`",
                "$ cat /tmp/readme.txt",
            ]
        );
        assert_eq!(
            snap.options
                .iter()
                .map(|o| (o.key.as_str(), o.label.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("1", "Yes, proceed (y)"),
                ("2", "No, and tell Codex what to do differently (esc)"),
            ]
        );
    }

    #[test]
    fn detects_the_network_and_permissions_variants() {
        let snap = detect(&network_approval_screen()).expect("overlay detected");
        assert!(snap.title.contains("approve network access"));
        assert_eq!(snap.options.len(), 4);
        assert_eq!(snap.options[2].key, "3");

        let snap = detect(&permissions_approval_screen()).expect("overlay detected");
        assert!(snap.title.contains("grant these permissions"));
        assert_eq!(snap.options.len(), 4);
    }

    #[test]
    fn a_wrapped_option_label_is_folded_into_its_row() {
        let lines = screen(&[
            "  Would you like to run the following command?",
            "",
            "  $ git push origin main",
            "",
            "› 1. Yes, proceed (y)",
            "  2. Yes, and don't ask again for commands that start with",
            "  `git push` (a)",
            "  3. No, and tell Codex what to do differently (esc)",
            "",
            "  Press enter to confirm or esc to cancel",
        ]);
        let snap = detect(&lines).expect("overlay detected");
        assert_eq!(snap.options.len(), 3);
        assert_eq!(
            snap.options[1].label,
            "Yes, and don't ask again for commands that start with `git push` (a)"
        );
    }

    #[test]
    fn the_request_user_input_picker_is_rejected() {
        // Its questions already ride the rollout as cards — the approval
        // watcher must never double-publish it, even with an approval-ish
        // question text.
        assert_eq!(detect(&request_user_input_screen()), None);
    }

    #[test]
    fn detect_ask_reads_the_step_cursor_and_rows() {
        let snap = detect_ask(&request_user_input_screen()).expect("ask pane detected");
        assert_eq!(snap.step, Some((1, 1)));
        assert_eq!(snap.selected, 1);
        assert_eq!(
            snap.options
                .iter()
                .map(|o| (o.key.as_str(), o.label.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("1", "Option 1  First choice."),
                ("2", "Option 2  Second choice."),
            ]
        );

        // The cursor on a later row of a later question — what a multi-
        // question ask looks like mid-flight.
        let moved = screen(&[
            "",
            "  Question 2/3 (2 unanswered)",
            "  Which database?",
            "",
            "    1. Postgres",
            "  › 2. SQLite",
            "",
            "  tab to add notes | enter to submit answer | esc to interrupt",
        ]);
        let snap = detect_ask(&moved).expect("ask pane detected");
        assert_eq!(snap.step, Some((2, 3)));
        assert_eq!(snap.selected, 2);
    }

    #[test]
    fn detect_ask_ignores_the_approval_overlay_and_a_stale_echo() {
        // The approval overlay is the OTHER handler's surface.
        assert_eq!(detect_ask(&exec_approval_screen()), None);
        // A numbered list scrolled far above the footer is not a live pane.
        let stale = screen(&[
            "  › 1. Option 1",
            "    2. Option 2",
            "",
            "",
            "",
            "",
            "  tab to add notes | enter to submit answer | esc to interrupt",
        ]);
        assert_eq!(detect_ask(&stale), None);
    }

    #[test]
    fn options_without_the_confirm_footer_are_ignored() {
        let mut lines = exec_approval_screen();
        lines.retain(|line| !line.contains(FOOTER_ANCHOR));
        assert_eq!(detect(&lines), None);
    }

    #[test]
    fn options_without_a_title_anchor_are_ignored() {
        let lines = screen(&[
            "  Pick a database:",
            "",
            "› 1. Postgres",
            "  2. SQLite",
            "",
            "  Press enter to confirm or esc to cancel",
        ]);
        assert_eq!(detect(&lines), None);
    }

    #[test]
    fn no_marker_means_no_overlay() {
        let mut lines = exec_approval_screen();
        lines[9] = "  1. Yes, proceed (y)".to_string();
        assert_eq!(detect(&lines), None);
    }

    #[test]
    fn watcher_debounces_show_and_resolve_and_refires_on_change() {
        let exec = exec_approval_screen();
        let blank = screen(&["  Working…"]);
        let mut w = CodexApprovalWatcher::new();

        assert_eq!(w.tick(&blank, 0), None);
        assert_eq!(w.tick(&exec, 0), None);
        match w.tick(&exec, 0) {
            Some(Transition::Show(snap)) => assert_eq!(snap.options.len(), 2),
            other => panic!("expected Show, got {other:?}"),
        }
        assert!(w.is_pending());
        assert_eq!(w.tick(&exec, 0), None);

        // The next dialog paints in place: fresh Show, no Resolved between.
        let network = network_approval_screen();
        assert_eq!(w.tick(&network, 0), None);
        assert!(matches!(w.tick(&network, 0), Some(Transition::Show(_))));

        // Gone for two ticks: Resolved.
        assert_eq!(w.tick(&blank, 0), None);
        assert_eq!(w.tick(&blank, 0), Some(Transition::Resolved));
        assert!(!w.is_pending());
    }

    #[test]
    fn watcher_freezes_while_scrolled() {
        let exec = exec_approval_screen();
        let blank = screen(&["$"]);
        let mut w = CodexApprovalWatcher::new();

        assert_eq!(w.tick(&exec, 3), None);
        assert_eq!(w.tick(&exec, 3), None);
        assert_eq!(w.tick(&blank, 0), None);

        w.tick(&exec, 0);
        assert!(matches!(w.tick(&exec, 0), Some(Transition::Show(_))));
        assert_eq!(w.tick(&blank, 5), None);
        assert_eq!(w.tick(&blank, 5), None);
        assert!(w.is_pending());
    }
}
