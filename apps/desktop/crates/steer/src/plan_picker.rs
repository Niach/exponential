//! Plan-approval picker detection on the live terminal grid (EXP-150).
//!
//! The activity emitter's transcript tail cannot see a PENDING plan approval:
//! `claude` (observed on v2.1.211) flushes a turn's transcript entries —
//! including the `ExitPlanMode` tool_use — only once the plan picker is
//! answered, so the transcript-derived plan `question` used to reach viewers
//! only AFTER approval, and then looked answerable. The desktop owns the
//! parsed terminal grid, and the picker IS on screen exactly while it is
//! pending — so the emitter watches the grid instead:
//!
//! * [`detect`] recognizes the picker on a plain-text screen snapshot
//!   ([`terminal::screen_lines`]) and parses the REAL option labels/keys off
//!   the numbered rows — version-proof against `claude` re-wording them.
//!   Both observed variants match: the full "Ready to code?" picker and the
//!   two-option "Exit plan mode?" one.
//! * [`PlanPickerWatcher`] is the per-session state machine the emitter
//!   ticks once per poll: 2 consecutive detections ⇒ [`Transition::Show`]
//!   (emit the plan question NOW, while it is answerable), 2 consecutive
//!   absences while pending ⇒ [`Transition::Resolved`] (emit a narration so
//!   viewers retire the card the moment it is answered). A scrolled viewport
//!   (`display_offset > 0`) freezes the machine — scrolling history past an
//!   old picker must not fake either transition.

use crate::frames::QuestionOption;

/// The anchor phrases that mark a plan-approval picker. A numbered list
/// alone is NOT enough (a plan body usually contains one) — options are only
/// trusted when one of these lines sits above them on the same screen.
///
/// `"Ready to code?"` renders at the TOP of the plan card, so on a long plan
/// it scrolls off the visible grid before the options reach the bottom —
/// which used to make `detect` miss the picker entirely. `"written up a
/// plan"` (from the sentence `claude` pins directly above the options —
/// "Claude has written up a plan and is ready to execute. Would you like to
/// proceed?") never scrolls off, so it catches long plans. We anchor on
/// `"written up a plan"` rather than `"Would you like to proceed?"` because
/// `claude` word-wraps the latter across two grid rows, defeating
/// `line.contains`.
const ANCHORS: &[&str] = &[
    "Ready to code?",
    "written up a plan",
    "Exit plan mode?",
    "wants to exit plan mode",
];

/// The selection cursor `claude` renders on the highlighted option row —
/// required once per option run so echoed markdown can never look like a
/// live picker.
const SELECTION_MARKER: char = '❯';

/// A detected picker: the parsed answer options plus (when the "Ready to
/// code?" variant renders the plan inline) the first visible plan line, used
/// as a soft cross-check when resolving the plan body from `~/.claude/plans`.
#[derive(Clone, Debug, PartialEq)]
pub struct PickerSnapshot {
    pub options: Vec<QuestionOption>,
    pub plan_box_first_line: Option<String>,
}

/// One state-machine step outcome.
#[derive(Clone, Debug, PartialEq)]
pub enum Transition {
    /// The picker just settled on screen — publish the plan question.
    Show(PickerSnapshot),
    /// The pending picker was answered (or dismissed) — publish a resolution
    /// narration so the question stops being the trailing feed item.
    Resolved,
}

/// Detect a plan-approval picker on a visible-screen snapshot.
///
/// Strategy: find the `❯`-marked option row (bottom-most wins — the picker
/// sits below any plan body), expand up/down over consecutively numbered
/// option rows, and require the run to start at `1.` with ≥2 options and an
/// anchor phrase somewhere above it.
pub fn detect(lines: &[String]) -> Option<PickerSnapshot> {
    let marker_idx = lines.iter().rposition(|line| {
        let t = line.trim_start();
        t.starts_with(SELECTION_MARKER) && parse_option_row(t).is_some()
    })?;
    let marker_number = parse_option_row(lines[marker_idx].trim_start())?.0;

    // Expand upward to option 1.
    let mut first_idx = marker_idx;
    let mut expect = marker_number;
    while expect > 1 {
        let prev = first_idx.checked_sub(1)?;
        let (n, _) = parse_option_row(lines[prev].trim_start())?;
        if n != expect - 1 {
            return None;
        }
        first_idx = prev;
        expect = n;
    }

    // Collect downward from option 1 while the numbering stays consecutive.
    let mut options = Vec::new();
    let mut next = 1u32;
    for line in &lines[first_idx..] {
        match parse_option_row(line.trim_start()) {
            Some((n, label)) if n == next => {
                options.push(QuestionOption {
                    label: label.to_string(),
                    key: n.to_string(),
                });
                next += 1;
            }
            _ => break,
        }
    }
    if options.len() < 2 || marker_idx - first_idx >= options.len() {
        return None;
    }

    // An anchor phrase must sit above the options.
    let anchored = lines[..first_idx]
        .iter()
        .any(|line| ANCHORS.iter().any(|a| line.contains(a)));
    if !anchored {
        return None;
    }

    Some(PickerSnapshot {
        plan_box_first_line: plan_box_first_line(&lines[..first_idx]),
        options,
    })
}

/// Parse one option row (`❯ 2. Yes, manually approve edits` / `3. No, keep
/// planning`) into its number and label. The input is trim_start-ed.
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

/// The first visible plan line of the "Ready to code?" variant: the first
/// non-empty, non-rule line after "Here is Claude's plan:". `None` for the
/// bare "Exit plan mode?" variant.
fn plan_box_first_line(lines: &[String]) -> Option<String> {
    let start = lines
        .iter()
        .position(|line| line.contains("Here is Claude's plan:"))?;
    lines[start + 1..]
        .iter()
        .map(|line| line.trim())
        .find(|t| !t.is_empty() && !is_rule(t))
        .map(str::to_string)
}

/// A horizontal-rule row (the `╌`/`─` separators framing the plan box).
fn is_rule(t: &str) -> bool {
    t.chars().all(|c| matches!(c, '╌' | '─' | '═' | '┄' | '┈'))
}

/// Debounce depth: a picker must be present (or absent) on this many
/// consecutive ticks before the machine transitions — one mid-render frame
/// (the emitter polls between escape-sequence batches) must not flap it.
const STREAK: u8 = 2;

/// Per-session plan-picker state machine — see the module docs.
#[derive(Default)]
pub struct PlanPickerWatcher {
    pending: bool,
    present_streak: u8,
    absent_streak: u8,
    snapshot: Option<PickerSnapshot>,
}

impl PlanPickerWatcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a plan-approval picker is currently pending on screen (between
    /// a `Show` and its `Resolved`) — the EXP-214 "needs input" signal.
    pub fn is_pending(&self) -> bool {
        self.pending
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
                if self.pending {
                    return None;
                }
                self.present_streak += 1;
                self.snapshot = Some(snapshot);
                if self.present_streak >= STREAK {
                    self.pending = true;
                    self.present_streak = 0;
                    return self.snapshot.take().map(Transition::Show);
                }
                None
            }
            None => {
                self.present_streak = 0;
                self.snapshot = None;
                if !self.pending {
                    return None;
                }
                self.absent_streak += 1;
                if self.absent_streak >= STREAK {
                    self.pending = false;
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

    /// The full v2.1.211 picker as rendered in the live TUI (captured).
    fn ready_to_code_screen() -> Vec<String> {
        screen(&[
            "────────────────────────────────────────",
            "Ready to code?",
            " Here is Claude's plan:",
            "╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌",
            "## Plan",
            "1. Do the thing",
            "2. Do the other thing",
            "╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌",
            "",
            "────────────────────────────────────────",
            " Claude has written up a plan and is ready to execute. Would you like to",
            " proceed?",
            "",
            " ❯ 1. Yes, auto-accept edits",
            "   2. Yes, manually approve edits",
            "   3. No, refine with Ultraplan on Claude Code on the web",
            "   4. Tell Claude what to change",
            "     shift+tab to approve with this feedback",
            " ctrl+g to edit Vim · ~/.claude/plans/some-slug.md",
        ])
    }

    /// The bare two-option variant (captured).
    fn exit_plan_mode_screen() -> Vec<String> {
        screen(&[
            "Exit plan mode?",
            "  Claude wants to exit plan mode",
            "❯ 1. Yes",
            "2. No",
        ])
    }

    #[test]
    fn detects_the_ready_to_code_picker_with_real_labels() {
        let snap = detect(&ready_to_code_screen()).expect("picker detected");
        assert_eq!(
            snap.options
                .iter()
                .map(|o| (o.key.as_str(), o.label.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("1", "Yes, auto-accept edits"),
                ("2", "Yes, manually approve edits"),
                ("3", "No, refine with Ultraplan on Claude Code on the web"),
                ("4", "Tell Claude what to change"),
            ]
        );
        // The plan-box cross-check line skips the rule row.
        assert_eq!(snap.plan_box_first_line.as_deref(), Some("## Plan"));
    }

    #[test]
    fn detects_the_picker_when_ready_to_code_has_scrolled_off() {
        // A long plan pushes the top-of-card "Ready to code?" header above the
        // visible grid; only the sentence claude pins directly over the
        // options survives. Detection must still fire via the "written up a
        // plan" anchor (regression: long batch plans showed nothing remotely).
        let lines = screen(&[
            "   (…top of the plan has scrolled off…)",
            "- EXP-241: keep the Start-coding sheet rows in place on tap.",
            "- EXP-239: add the mobile bulk-select bar.",
            "────────────────────────────────────────",
            " Claude has written up a plan and is ready to execute. Would you like to",
            " proceed?",
            "",
            " ❯ 1. Yes, auto-accept edits",
            "   2. Yes, manually approve edits",
            "   3. No, refine with Ultraplan on Claude Code on the web",
            "   4. Tell Claude what to change",
            "     shift+tab to approve with this feedback",
            " ctrl+g to edit Vim · ~/.claude/plans/some-slug.md",
        ]);
        // "Ready to code?" is NOT on this screen.
        assert!(!lines.iter().any(|l| l.contains("Ready to code?")));
        let snap = detect(&lines).expect("picker detected via the bottom anchor");
        assert_eq!(snap.options.len(), 4);
        assert_eq!(snap.options[0].key, "1");
        assert_eq!(snap.options[3].key, "4");
    }

    #[test]
    fn detects_the_bare_exit_plan_mode_picker() {
        let snap = detect(&exit_plan_mode_screen()).expect("picker detected");
        assert_eq!(
            snap.options
                .iter()
                .map(|o| (o.key.as_str(), o.label.as_str()))
                .collect::<Vec<_>>(),
            vec![("1", "Yes"), ("2", "No")]
        );
        assert_eq!(snap.plan_box_first_line, None);
    }

    #[test]
    fn selection_marker_may_sit_on_any_option() {
        let mut lines = exit_plan_mode_screen();
        lines[2] = "1. Yes".to_string();
        lines[3] = "❯ 2. No".to_string();
        let snap = detect(&lines).expect("picker detected");
        assert_eq!(snap.options.len(), 2);
    }

    #[test]
    fn plan_body_numbered_list_alone_is_not_a_picker() {
        // No ❯ marker anywhere: an echoed/streamed plan with a numbered list
        // (even below an anchor-like phrase) must not detect.
        let lines = screen(&[
            "Ready to code?",
            "1. Do the thing",
            "2. Do the other thing",
        ]);
        assert_eq!(detect(&lines), None);
    }

    #[test]
    fn options_without_an_anchor_are_ignored() {
        let lines = screen(&[
            "Pick a database:",
            "❯ 1. Postgres",
            "  2. SQLite",
        ]);
        assert_eq!(detect(&lines), None);
    }

    #[test]
    fn marker_on_a_plan_list_does_not_leak_plan_lines_into_options() {
        // The ❯-marked run is expanded strictly over consecutive numbering —
        // a plan list above the options can't merge into them.
        let mut lines = ready_to_code_screen();
        // Sanity: options still parse when the plan list uses 1./2. too.
        let snap = detect(&lines).expect("picker detected");
        assert_eq!(snap.options.len(), 4);
        // And a marker row whose run doesn't start at 1 is rejected.
        lines[13] = "   1. Yes, auto-accept edits".to_string();
        lines[14] = "❯ 3. Yes, manually approve edits".to_string();
        assert_eq!(detect(&lines), None);
    }

    #[test]
    fn watcher_debounces_show_and_resolve() {
        let picker = ready_to_code_screen();
        let blank = screen(&["$ claude", "✳ Deliberating…"]);
        let mut w = PlanPickerWatcher::new();

        assert_eq!(w.tick(&blank, 0), None);
        // First sighting: no transition yet (debounce).
        assert_eq!(w.tick(&picker, 0), None);
        // Second consecutive sighting: Show with the parsed options.
        match w.tick(&picker, 0) {
            Some(Transition::Show(snap)) => assert_eq!(snap.options.len(), 4),
            other => panic!("expected Show, got {other:?}"),
        }
        // Steady state while pending: silent.
        assert_eq!(w.tick(&picker, 0), None);
        // One absent frame: still pending (debounce).
        assert_eq!(w.tick(&blank, 0), None);
        assert_eq!(w.tick(&picker, 0), None);
        assert_eq!(w.tick(&blank, 0), None);
        // Second consecutive absence: Resolved.
        assert_eq!(w.tick(&blank, 0), Some(Transition::Resolved));
        // And back to idle.
        assert_eq!(w.tick(&blank, 0), None);
    }

    #[test]
    fn watcher_freezes_while_scrolled() {
        let picker = ready_to_code_screen();
        let blank = screen(&["$"]);
        let mut w = PlanPickerWatcher::new();

        // A picker seen only in scrolled-back history never Shows.
        assert_eq!(w.tick(&picker, 3), None);
        assert_eq!(w.tick(&picker, 3), None);
        assert_eq!(w.tick(&blank, 0), None);

        // A pending picker scrolled out of view never Resolves.
        assert_eq!(w.tick(&picker, 0), None);
        assert!(matches!(w.tick(&picker, 0), Some(Transition::Show(_))));
        assert_eq!(w.tick(&blank, 5), None);
        assert_eq!(w.tick(&blank, 5), None);
        // Back at the bottom with the picker still up: still pending.
        assert_eq!(w.tick(&picker, 0), None);
    }

    #[test]
    fn reject_then_new_picker_shows_again() {
        let picker = exit_plan_mode_screen();
        let blank = screen(&["working…"]);
        let mut w = PlanPickerWatcher::new();
        w.tick(&picker, 0);
        assert!(matches!(w.tick(&picker, 0), Some(Transition::Show(_))));
        w.tick(&blank, 0);
        assert_eq!(w.tick(&blank, 0), Some(Transition::Resolved));
        w.tick(&picker, 0);
        assert!(matches!(w.tick(&picker, 0), Some(Transition::Show(_))));
    }
}
