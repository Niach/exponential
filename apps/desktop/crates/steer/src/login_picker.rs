//! Claude login-flow detection on the live terminal grid (EXP-430).
//!
//! When claude's OAuth token expires mid-session it prints "Please run
//! /login" — that line reaches viewers through the transcript tail, and a
//! steered `/login` reaches the PTY, but the interactive login TUI it opens
//! is invisible remotely: it renders only on the grid, never in the
//! transcript, so a headless-server session dead-ends without SSH. Like the
//! plan picker (EXP-150), the emitter watches the grid instead:
//!
//! * [`detect`] recognizes the login screens on a plain-text snapshot
//!   ([`terminal::screen_lines`]): the method picker (published as an
//!   answerable question), the OAuth-URL/paste-code screen (published as a
//!   narration carrying the reconstructed URL — the remote user opens it in
//!   a browser and sends the code back as an ordinary message), the OAuth
//!   error screen, and — best-effort — the success screen.
//! * [`LoginWatcher`] is the per-session state machine, debounced like
//!   [`crate::plan_picker::PlanPickerWatcher`] but keyed on the settled
//!   [`LoginPhase`] so phase CHANGES (picker → URL → error) re-fire
//!   [`Transition::Show`] without an intervening [`Transition::Resolved`],
//!   and a retry's fresh URL re-fires too. [`LoginPhase::Success`] is only
//!   surfaced as a continuation of a pending login phase — claude echoing
//!   "Login successful" in ordinary output must never look like a login.
//!
//! Anchors were captured live from claude v2.1.222 (onboarding and
//! mid-session `/login` render the same screens; mid-session adds a "Login"
//! title and an "Esc to cancel" footer, neither of which is load-bearing).

use crate::frames::QuestionOption;

/// The line above the method options. Captured: `Select login method:`.
const METHOD_ANCHOR: &str = "Select login method";

/// The line above the sign-in URL. Captured: `Browser didn't open? Use the
/// url below to sign in (c to copy)` — anchored on the middle fragment
/// because a narrower grid could word-wrap the leading sentence away.
const URL_ANCHOR: &str = "Use the url below to sign in";

/// The OAuth failure screen. Captured after a bad code: `OAuth error:
/// Request failed with status code 400` + `Press Enter to retry.`.
const ERROR_ANCHOR: &str = "OAuth error";
const ERROR_RETRY_ANCHOR: &str = "Press Enter to retry";

/// Best-effort success anchors — the success screen was not capturable
/// without completing a real login, so detection FAILS OPEN: if neither
/// matches, the flow still resolves via absence.
const SUCCESS_ANCHORS: &[&str] = &["Login successful", "Logged in as"];

/// The selection cursor on the highlighted option row — required so echoed
/// markdown can never look like a live picker.
const SELECTION_MARKER: char = '❯';

/// EXP-444: the only domains a detected sign-in URL may point at to be
/// published verbatim (through `redact_exact_only`). The grid is
/// agent-writable — anything an agent prints can match [`URL_ANCHOR`], and an
/// off-domain "sign-in link" rendered as a tappable authorize anchor inside
/// the trusted activity feed is a phishing primitive, not a login.
const TRUSTED_LOGIN_DOMAINS: &[&str] = &["claude.ai", "claude.com", "anthropic.com"];

/// Whether a detected sign-in URL points at an Anthropic-owned host
/// (exactly a [`TRUSTED_LOGIN_DOMAINS`] entry, or a subdomain of one).
pub fn is_trusted_login_url(url: &str) -> bool {
    host_in(url, TRUSTED_LOGIN_DOMAINS)
}

/// Whether `url`'s host is exactly one of `domains` or a subdomain of one.
/// https-only; a userinfo `@` in the authority is rejected outright —
/// `https://claude.com@evil.com/` has no legitimate use on a login screen.
/// Shared with [`crate::codex_login_picker`], which trusts a different
/// domain list over the same rules (EXP-484).
pub(crate) fn host_in(url: &str, domains: &[&str]) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    if authority.is_empty() || authority.contains('@') {
        return false;
    }
    let host = authority
        .rsplit_once(':')
        .map(|(host, port)| {
            // Only strip a real port suffix; anything non-numeric after a
            // colon is a malformed authority, not a port.
            if port.chars().all(|c| c.is_ascii_digit()) && !port.is_empty() {
                host
            } else {
                ""
            }
        })
        .unwrap_or(authority)
        .to_ascii_lowercase();
    if host.is_empty() {
        return false;
    }
    domains
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

/// One detected login-flow screen.
#[derive(Clone, Debug, PartialEq)]
pub enum LoginPhase {
    /// `Select login method:` above a `❯`-marked numbered option run.
    MethodPicker { options: Vec<QuestionOption> },
    /// The sign-in URL + "Paste code here if prompted" screen, with the URL
    /// reconstructed across its hard-wrapped grid rows.
    UrlPrompt { url: String },
    /// `OAuth error: …` + `Press Enter to retry.` (a bad/expired code).
    Error { message: String },
    /// Best-effort success detection (see [`SUCCESS_ANCHORS`]).
    Success,
}

/// One state-machine step outcome.
#[derive(Clone, Debug, PartialEq)]
pub enum Transition {
    /// A login screen settled (or the settled screen changed) — publish it.
    Show(LoginPhase),
    /// Every login screen left the grid while one was pending — retire the
    /// published question/state (answered locally, cancelled, or completed
    /// without a recognizable success screen).
    Resolved,
}

/// Detect a login-flow screen on a visible-screen snapshot.
///
/// Precedence: method picker, then error, then URL prompt, then success —
/// the error screen must never half-match a lesser phase.
pub fn detect(lines: &[String]) -> Option<LoginPhase> {
    if let Some(options) = detect_method_picker(lines) {
        return Some(LoginPhase::MethodPicker { options });
    }
    if let Some(message) = detect_error(lines) {
        return Some(LoginPhase::Error { message });
    }
    if let Some(url) = detect_url_prompt(lines) {
        return Some(LoginPhase::UrlPrompt { url });
    }
    detect_success(lines).then_some(LoginPhase::Success)
}

/// The method picker: a `❯`-marked consecutively-numbered option run
/// starting at `1.`, with [`METHOD_ANCHOR`] somewhere above it. Same
/// expand-from-the-marker strategy as `plan_picker::detect`.
fn detect_method_picker(lines: &[String]) -> Option<Vec<QuestionOption>> {
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

    // Collect downward while the numbering stays consecutive.
    let mut options = Vec::new();
    let mut next = 1u32;
    for line in &lines[first_idx..] {
        match parse_option_row(line.trim_start()) {
            Some((n, label)) if n == next => {
                options.push(QuestionOption::new(label, n.to_string()));
                next += 1;
            }
            _ => break,
        }
    }
    if options.len() < 2 || marker_idx - first_idx >= options.len() {
        return None;
    }

    lines[..first_idx]
        .iter()
        .any(|line| line.contains(METHOD_ANCHOR))
        .then_some(options)
}

/// Parse one option row (`❯ 1. Claude account with subscription · …`) into
/// its number and label. The input is trim_start-ed.
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

/// The URL screen: [`URL_ANCHOR`] followed by an `https://` line. The URL
/// renders at column 0 and hard-wraps at the grid width with no spaces
/// (`screen_lines` already trims trailing whitespace), so it is
/// reconstructed by appending consecutive lines until a blank line — or any
/// line containing whitespace, which can only be the screen's next prose
/// line on a grid too narrow for a blank separator.
fn detect_url_prompt(lines: &[String]) -> Option<String> {
    let anchor_idx = lines.iter().position(|line| line.contains(URL_ANCHOR))?;
    let start = lines[anchor_idx + 1..]
        .iter()
        .position(|line| line.trim_start().starts_with("https://"))?
        + anchor_idx
        + 1;
    Some(join_wrapped_url(lines, start))
}

/// Reconstruct a hard-wrapped URL starting at `lines[start]`: append
/// consecutive rows until a blank line — or any line containing whitespace,
/// which can only be the screen's next prose line on a grid too narrow for a
/// blank separator. Shared with [`crate::codex_login_picker`] (EXP-484).
pub(crate) fn join_wrapped_url(lines: &[String], start: usize) -> String {
    let mut url = String::new();
    for line in lines.iter().skip(start) {
        let fragment = line.trim();
        if fragment.is_empty() || fragment.contains(char::is_whitespace) {
            break;
        }
        url.push_str(fragment);
    }
    url
}

/// The anchor-less variant used by the EXP-484 login DRIVER: the first
/// `https://` row on the grid, wrap-joined, kept only when it points at a
/// trusted host. `claude auth login --claudeai` prints its sign-in URL with
/// none of [`detect`]'s TUI anchors around it — that flow is a plain
/// non-interactive command, not the mid-session `/login` screen — so the
/// driver falls back to this after [`detect`] finds nothing. The session
/// emitter deliberately keeps using [`detect`] (EXP-430 behaviour is
/// untouched): the trusted-host test is all that stands between the grid and
/// a published link, and an anchored screen is the stronger signal.
pub fn detect_trusted_url(lines: &[String]) -> Option<String> {
    let start = lines
        .iter()
        .position(|line| line.trim_start().starts_with("https://"))?;
    let url = join_wrapped_url(lines, start);
    is_trusted_login_url(&url).then_some(url)
}

fn detect_error(lines: &[String]) -> Option<String> {
    let message = lines
        .iter()
        .map(|line| line.trim())
        .find(|line| line.contains(ERROR_ANCHOR))?;
    lines
        .iter()
        .any(|line| line.contains(ERROR_RETRY_ANCHOR))
        .then(|| message.to_string())
}

fn detect_success(lines: &[String]) -> bool {
    lines
        .iter()
        .any(|line| SUCCESS_ANCHORS.iter().any(|a| line.contains(a)))
}

/// Debounce depth, matching the other grid watchers: a screen must hold for
/// this many consecutive ticks before the machine transitions — one
/// mid-render frame (a half-painted URL) must not flap it.
const STREAK: u8 = 2;

/// Per-session login-flow state machine — see the module docs.
#[derive(Default)]
pub struct LoginWatcher {
    /// The settled phase, between its `Show` and the flow's `Resolved`.
    pending: Option<LoginPhase>,
    candidate: Option<LoginPhase>,
    present_streak: u8,
    absent_streak: u8,
}

impl LoginWatcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the login flow currently waits on a human (the EXP-214
    /// "needs input" signal). Success is excluded — the emitter dismisses
    /// that screen itself.
    pub fn is_pending(&self) -> bool {
        matches!(
            self.pending,
            Some(LoginPhase::MethodPicker { .. })
                | Some(LoginPhase::UrlPrompt { .. })
                | Some(LoginPhase::Error { .. })
        )
    }

    /// Feed one poll tick. `display_offset > 0` (viewport scrolled into
    /// history) freezes the machine entirely.
    pub fn tick(&mut self, lines: &[String], display_offset: usize) -> Option<Transition> {
        if display_offset > 0 {
            return None;
        }
        match detect(lines) {
            Some(phase) => {
                self.absent_streak = 0;
                if self.pending.as_ref() == Some(&phase) {
                    self.candidate = None;
                    self.present_streak = 0;
                    return None;
                }
                // Success out of nowhere is claude ECHOING login-ish text in
                // ordinary output — only a continuation of a pending login
                // phase may surface it (and trigger the emitter's Enter).
                if matches!(phase, LoginPhase::Success) && self.pending.is_none() {
                    self.candidate = None;
                    self.present_streak = 0;
                    return None;
                }
                if self.candidate.as_ref() == Some(&phase) {
                    self.present_streak += 1;
                } else {
                    self.candidate = Some(phase);
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

    /// The mid-session `/login` method picker as captured on v2.1.222.
    fn method_picker_screen() -> Vec<String> {
        screen(&[
            "▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔",
            "   Login",
            "",
            "   Claude Code can be used with your Claude subscription or billed based on API usage through your Console account.",
            "",
            "   Select login method:",
            "",
            "   ❯ 1. Claude account with subscription · Pro, Max, Team, or Enterprise",
            "     2. Anthropic Console account · API usage billing",
            "     3. 3rd-party platform · Amazon Bedrock, Microsoft Foundry, or Vertex AI",
            "",
            "   Esc to cancel",
        ])
    }

    /// The URL/paste-code screen as captured on a 120-column grid: the URL
    /// starts at column 0 and hard-wraps over four rows with no spaces.
    fn url_screen() -> Vec<String> {
        screen(&[
            "   Login",
            "",
            "   Browser didn't open? Use the url below to sign in (c to copy)",
            "",
            "https://claude.com/cai/oauth/authorize?code=true&client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e&response_type=code&redir",
            "ect_uri=https%3A%2F%2Fplatform.claude.com%2Foauth%2Fcode%2Fcallback&scope=org%3Acreate_api_key+user%3Aprofile+user%3Ainf",
            "erence+user%3Asessions%3Aclaude_code+user%3Amcp_servers+user%3Afile_upload&code_challenge=j7BY1qKMJ1Y2LC5xNqD5VUJayK_UZb",
            "Pl_FCJLsmPZzk&code_challenge_method=S256&state=joiGbKCc8WwbICmveDWnCjihN6dnqxVjkxcYKIMI6SE",
            "",
            "",
            "   Paste code here if prompted >",
            "",
            "   Esc to cancel",
        ])
    }

    const FULL_URL: &str = "https://claude.com/cai/oauth/authorize?code=true&client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e&response_type=code&redirect_uri=https%3A%2F%2Fplatform.claude.com%2Foauth%2Fcode%2Fcallback&scope=org%3Acreate_api_key+user%3Aprofile+user%3Ainference+user%3Asessions%3Aclaude_code+user%3Amcp_servers+user%3Afile_upload&code_challenge=j7BY1qKMJ1Y2LC5xNqD5VUJayK_UZbPl_FCJLsmPZzk&code_challenge_method=S256&state=joiGbKCc8WwbICmveDWnCjihN6dnqxVjkxcYKIMI6SE";

    fn error_screen() -> Vec<String> {
        screen(&[
            "   Login",
            "",
            "   OAuth error: Request failed with status code 400",
            "",
            "   Press Enter to retry.",
            "",
            "   Esc to cancel",
        ])
    }

    fn repl_screen() -> Vec<String> {
        screen(&["❯ Try \"fix lint errors\"", "  ⏵⏵ auto mode on"])
    }

    #[test]
    fn detects_the_method_picker_with_real_labels() {
        let Some(LoginPhase::MethodPicker { options }) = detect(&method_picker_screen()) else {
            panic!("method picker not detected");
        };
        assert_eq!(
            options
                .iter()
                .map(|o| (o.key.as_str(), o.label.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (
                    "1",
                    "Claude account with subscription · Pro, Max, Team, or Enterprise"
                ),
                ("2", "Anthropic Console account · API usage billing"),
                (
                    "3",
                    "3rd-party platform · Amazon Bedrock, Microsoft Foundry, or Vertex AI"
                ),
            ]
        );
    }

    #[test]
    fn method_options_without_the_anchor_are_ignored() {
        let lines = screen(&[
            "Pick a database:",
            "❯ 1. Postgres",
            "  2. SQLite",
        ]);
        assert_eq!(detect(&lines), None);
    }

    #[test]
    fn method_options_without_a_marker_are_ignored() {
        let mut lines = method_picker_screen();
        lines[7] = "   1. Claude account with subscription · Pro, Max, Team, or Enterprise"
            .to_string();
        assert_eq!(detect(&lines), None);
    }

    #[test]
    fn reconstructs_the_wrapped_url() {
        let Some(LoginPhase::UrlPrompt { url }) = detect(&url_screen()) else {
            panic!("url screen not detected");
        };
        assert_eq!(url, FULL_URL);
    }

    #[test]
    fn url_reconstruction_stops_at_prose_when_the_blank_separator_is_missing() {
        // A grid too narrow for the blank row: the next prose line contains
        // whitespace and must not be appended to the URL.
        let lines = screen(&[
            " Use the url below to sign in (c to copy)",
            "https://claude.com/cai/oauth/authorize?code=tr",
            "ue&state=abc",
            "   Paste code here if prompted >",
        ]);
        let Some(LoginPhase::UrlPrompt { url }) = detect(&lines) else {
            panic!("url screen not detected");
        };
        assert_eq!(url, "https://claude.com/cai/oauth/authorize?code=true&state=abc");
    }

    #[test]
    fn detects_the_error_screen() {
        assert_eq!(
            detect(&error_screen()),
            Some(LoginPhase::Error {
                message: "OAuth error: Request failed with status code 400".to_string()
            })
        );
    }

    #[test]
    fn oauth_error_without_the_retry_line_is_not_the_error_screen() {
        // Claude quoting "OAuth error: …" in ordinary narration.
        let lines = screen(&["The logs show OAuth error: expired token, so…"]);
        assert_eq!(detect(&lines), None);
    }

    #[test]
    fn watcher_debounces_show_and_resolve() {
        let mut w = LoginWatcher::new();
        assert_eq!(w.tick(&repl_screen(), 0), None);
        // First sighting: debounce.
        assert_eq!(w.tick(&method_picker_screen(), 0), None);
        assert!(matches!(
            w.tick(&method_picker_screen(), 0),
            Some(Transition::Show(LoginPhase::MethodPicker { .. }))
        ));
        assert!(w.is_pending());
        // Steady state: silent.
        assert_eq!(w.tick(&method_picker_screen(), 0), None);
        // One absent frame: still pending.
        assert_eq!(w.tick(&repl_screen(), 0), None);
        assert!(w.is_pending());
        assert_eq!(w.tick(&repl_screen(), 0), Some(Transition::Resolved));
        assert!(!w.is_pending());
        assert_eq!(w.tick(&repl_screen(), 0), None);
    }

    #[test]
    fn phase_change_refires_show_without_resolved() {
        let mut w = LoginWatcher::new();
        w.tick(&method_picker_screen(), 0);
        assert!(matches!(w.tick(&method_picker_screen(), 0), Some(Transition::Show(_))));
        // Picker answered locally → URL screen: Show(UrlPrompt), no Resolved.
        assert_eq!(w.tick(&url_screen(), 0), None);
        assert!(matches!(
            w.tick(&url_screen(), 0),
            Some(Transition::Show(LoginPhase::UrlPrompt { .. }))
        ));
        assert!(w.is_pending());
        // Bad code → error screen.
        assert_eq!(w.tick(&error_screen(), 0), None);
        assert!(matches!(
            w.tick(&error_screen(), 0),
            Some(Transition::Show(LoginPhase::Error { .. }))
        ));
    }

    #[test]
    fn a_changed_url_refires_show() {
        // Retry loop: same phase shape, different URL (fresh state param).
        let mut w = LoginWatcher::new();
        w.tick(&url_screen(), 0);
        assert!(matches!(w.tick(&url_screen(), 0), Some(Transition::Show(_))));
        let mut retry = url_screen();
        retry[7] = "Pl_FCJLsmPZzk&code_challenge_method=S256&state=DIFFERENT".to_string();
        assert_eq!(w.tick(&retry, 0), None);
        assert!(matches!(
            w.tick(&retry, 0),
            Some(Transition::Show(LoginPhase::UrlPrompt { .. }))
        ));
    }

    #[test]
    fn a_half_painted_url_does_not_settle() {
        // Mid-render frames differ tick to tick — the candidate resets and
        // nothing fires until the screen holds still for STREAK ticks.
        let mut w = LoginWatcher::new();
        let mut half = url_screen();
        half.truncate(6);
        assert_eq!(w.tick(&half, 0), None);
        assert_eq!(w.tick(&url_screen(), 0), None);
        assert!(matches!(w.tick(&url_screen(), 0), Some(Transition::Show(_))));
    }

    #[test]
    fn success_is_only_a_continuation() {
        let success = screen(&["   Login successful. Press Enter to continue…"]);
        // Standalone success-ish text (claude echoing it in output): ignored.
        let mut w = LoginWatcher::new();
        assert_eq!(w.tick(&success, 0), None);
        assert_eq!(w.tick(&success, 0), None);
        assert!(!w.is_pending());

        // As a continuation of a pending login phase: surfaced.
        let mut w = LoginWatcher::new();
        w.tick(&url_screen(), 0);
        assert!(matches!(w.tick(&url_screen(), 0), Some(Transition::Show(_))));
        assert_eq!(w.tick(&success, 0), None);
        assert!(matches!(
            w.tick(&success, 0),
            Some(Transition::Show(LoginPhase::Success))
        ));
        // Success no longer needs input.
        assert!(!w.is_pending());
    }

    #[test]
    fn watcher_freezes_while_scrolled() {
        let mut w = LoginWatcher::new();
        // A picker seen only in scrolled-back history never Shows.
        assert_eq!(w.tick(&method_picker_screen(), 3), None);
        assert_eq!(w.tick(&method_picker_screen(), 3), None);
        assert_eq!(w.tick(&repl_screen(), 0), None);

        // A pending flow scrolled out of view never Resolves.
        w.tick(&method_picker_screen(), 0);
        assert!(matches!(w.tick(&method_picker_screen(), 0), Some(Transition::Show(_))));
        assert_eq!(w.tick(&repl_screen(), 5), None);
        assert_eq!(w.tick(&repl_screen(), 5), None);
        assert!(w.is_pending());
        assert_eq!(w.tick(&method_picker_screen(), 0), None);
    }

    #[test]
    fn login_screens_do_not_match_the_other_pickers_and_vice_versa() {
        // The login method picker must not detect as a plan or ask picker —
        // and their screens must not detect as a login flow. Keeping these
        // disjoint is what keeps the publisher's Esc-reroute and the answer
        // machinery from crossing wires.
        assert_eq!(crate::plan_picker::detect(&method_picker_screen()), None);
        assert_eq!(crate::question_picker::detect(&method_picker_screen()), None);
        assert_eq!(
            crate::question_picker::detect_during_ask(&method_picker_screen()),
            None
        );
        assert_eq!(crate::plan_picker::detect(&url_screen()), None);
        assert_eq!(crate::question_picker::detect(&url_screen()), None);

        let plan_screen = screen(&[
            " Claude has written up a plan and is ready to execute. Would you like to",
            " proceed?",
            "",
            " ❯ 1. Yes, auto-accept edits",
            "   2. Yes, manually approve edits",
        ]);
        assert!(crate::plan_picker::detect(&plan_screen).is_some());
        assert_eq!(detect(&plan_screen), None);
    }

    /// EXP-484: `claude auth login --claudeai` prints the sign-in URL with
    /// none of the TUI anchors around it — the driver's fallback reads a
    /// bare line, and only a trusted host may come back.
    #[test]
    fn detect_trusted_url_finds_a_bare_url_without_the_anchor() {
        let bare = screen(&[
            "Opening your browser to sign in…",
            "",
            "https://claude.com/cai/oauth/authorize?code=true&client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e&response_type=code&redir",
            "ect_uri=https%3A%2F%2Fplatform.claude.com%2Foauth%2Fcode%2Fcallback&scope=org%3Acreate_api_key+user%3Aprofile+user%3Ainf",
            "erence+user%3Asessions%3Aclaude_code+user%3Amcp_servers+user%3Afile_upload&code_challenge=j7BY1qKMJ1Y2LC5xNqD5VUJayK_UZb",
            "Pl_FCJLsmPZzk&code_challenge_method=S256&state=joiGbKCc8WwbICmveDWnCjihN6dnqxVjkxcYKIMI6SE",
            "",
            "Waiting for the browser…",
        ]);
        // No anchor: the EXP-430 emitter path still sees nothing here.
        assert_eq!(detect(&bare), None);
        assert_eq!(detect_trusted_url(&bare).as_deref(), Some(FULL_URL));
        // The anchored screen reads the same through the fallback.
        assert_eq!(detect_trusted_url(&url_screen()).as_deref(), Some(FULL_URL));
        // Off-domain links are refused, and a screen with no link at all is
        // simply nothing.
        assert_eq!(
            detect_trusted_url(&screen(&["https://evil.test/oauth/authorize?code=true"])),
            None
        );
        assert_eq!(detect_trusted_url(&repl_screen()), None);
    }

    /// EXP-444: only Anthropic-owned hosts may travel verbatim through the
    /// exact-only redaction path.
    #[test]
    fn trusted_login_urls_accept_anthropic_domains_only() {
        for url in [
            "https://claude.ai/oauth/authorize?code=abc",
            "https://claude.com/login",
            "https://platform.claude.com/oauth?x=1#frag",
            "https://console.anthropic.com/oauth/authorize",
            "https://claude.ai:443/oauth",
        ] {
            assert!(is_trusted_login_url(url), "{url} should be trusted");
        }
        for url in [
            "http://claude.com/login",              // https only
            "https://claude.com.evil.com/login",    // suffix spoof
            "https://evil.com/claude.com",          // domain in the path
            "https://claude.com@evil.com/login",    // userinfo trick
            "https://claudeXai/whatever",           // no dot boundary
            "https://xclaude.ai/login",             // not a subdomain
            "https://claude.com:evil/x",            // malformed port
            "https://",                             // empty authority
            "notaurl",
        ] {
            assert!(!is_trusted_login_url(url), "{url} must be refused");
        }
    }
}
