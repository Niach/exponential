//! Codex device-auth login detection on the live terminal grid (EXP-484).
//!
//! `codex login --device-auth` is the remote-friendly half of the sign-in
//! story: it prints a sign-in URL plus a short device code, and the person
//! finishing it needs neither the keyboard nor the screen of the machine
//! that runs it. The remote `agent_login` device command relies on that —
//! the executor runs the login in a headless PTY and completes the command
//! as soon as the URL and the code are on the grid.
//!
//! Codex renders that screen with no stable TUI anchors (unlike claude's
//! `/login` flow, [`crate::login_picker`]), and its wording has drifted
//! between releases, so detection is deliberately shape-based rather than
//! string-exact:
//!
//! * the URL is the FIRST `https://` on the grid, wrap-joined across the
//!   rows it hard-wrapped onto, and kept only when its host is one of
//!   [`CODEX_TRUSTED_LOGIN_DOMAINS`] — the grid is agent-writable, so an
//!   off-domain "sign-in link" published into a trusted surface would be a
//!   phishing primitive, not a login (EXP-444's rule, second domain list);
//! * the code is an `XXXX-XXXXX`-shaped token on (or right below) a line
//!   that says "code" — one line, two lines, boxed or bare all read the
//!   same.
//!
//! [`CodexLoginPhase::DeviceCode`] needs BOTH halves: a URL without its code
//! cannot be finished remotely, so half a screen is not a screen.

use crate::login_picker::{host_in, join_wrapped_url};

/// EXP-444, applied to Codex: the only hosts a detected Codex sign-in URL
/// may point at. `auth.openai.com` is what 0.144.5 prints; `chatgpt.com`
/// covers the ChatGPT-account variant of the same flow.
pub const CODEX_TRUSTED_LOGIN_DOMAINS: &[&str] = &["auth.openai.com", "chatgpt.com"];

/// Success anchors — best effort, like the claude picker's: the flow also
/// resolves by absence when the PTY exits.
const SUCCESS_ANCHORS: &[&str] = &["Successfully logged in", "Logged in as", "Login successful"];

/// Words that mean the device flow died. Only honoured BELOW the code
/// prompt: above it they are ordinary output (a banner, a warning, an
/// unrelated agent line).
const ERROR_ANCHORS: &[&str] = &["error", "expired", "denied", "failed"];

/// How far below a "code" prompt the code itself may sit (blank rows and a
/// box border in between are normal).
const CODE_LOOKAHEAD: usize = 3;

/// Whether a detected Codex sign-in URL points at an OpenAI-owned login
/// host (exactly a [`CODEX_TRUSTED_LOGIN_DOMAINS`] entry, or a subdomain of
/// one). https-only, userinfo authorities refused — see [`host_in`].
pub fn is_trusted_codex_login_url(url: &str) -> bool {
    host_in(url, CODEX_TRUSTED_LOGIN_DOMAINS)
}

/// One detected Codex login screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CodexLoginPhase {
    /// The device-auth screen: a trusted sign-in URL AND its short code.
    DeviceCode { url: String, code: String },
    /// Best-effort success detection.
    Success,
    /// The flow failed below the code prompt (expired code, denied, …).
    Error { message: String },
}

/// Detect a Codex device-auth screen on a visible-screen snapshot.
///
/// Precedence: success, then error, then the device-code screen — the URL
/// and code stay on the grid after the flow moves on, so a settled outcome
/// must win over the screen that produced it.
pub fn detect(lines: &[String]) -> Option<CodexLoginPhase> {
    if lines
        .iter()
        .any(|line| SUCCESS_ANCHORS.iter().any(|anchor| line.contains(anchor)))
    {
        return Some(CodexLoginPhase::Success);
    }
    if let Some(message) = detect_error(lines) {
        return Some(CodexLoginPhase::Error { message });
    }
    let url = detect_url(lines)?;
    let code = detect_code(lines)?;
    Some(CodexLoginPhase::DeviceCode { url, code })
}

/// The first `https://` on the grid, wrap-joined and host-checked.
///
/// Two shapes both occur: a bare row carrying nothing but the URL (which
/// may hard-wrap onto the next rows), and a URL embedded in prose or inside
/// a box border (which never wraps usefully — take the token and stop).
fn detect_url(lines: &[String]) -> Option<String> {
    let idx = lines.iter().position(|line| line.contains("https://"))?;
    let line = lines[idx].as_str();
    let at = line.find("https://")?;
    let tail = &line[at..];
    let token: String = tail.chars().take_while(|c| !c.is_whitespace()).collect();
    let bare_row = line[..at].trim().is_empty() && token.len() == tail.trim_end().len();
    let url = if bare_row {
        // Stop the wrap-join before a bare code row — appending the device
        // code to the URL would break both halves at once.
        let stop = lines
            .iter()
            .enumerate()
            .skip(idx + 1)
            .find(|(_, line)| code_token(line.trim()).is_some())
            .map(|(at, _)| at)
            .unwrap_or(lines.len());
        join_wrapped_url(&lines[..stop], idx)
    } else {
        token
    };
    let url = url
        .trim_end_matches(['.', ',', ';', ')', ']', '"', '\'', '>'])
        .to_string();
    is_trusted_codex_login_url(&url).then_some(url)
}

/// The device code: an `XXXX-XXXXX` token on a line that says "code", or on
/// the first non-blank line below such a prompt.
fn detect_code(lines: &[String]) -> Option<String> {
    for (idx, line) in lines.iter().enumerate() {
        if !mentions_code(line) {
            continue;
        }
        if let Some(code) = code_in(line) {
            return Some(code);
        }
        if let Some(code) = lines
            .iter()
            .skip(idx + 1)
            .take(CODE_LOOKAHEAD)
            .filter(|line| !line.trim().is_empty())
            .find_map(|line| code_in(line))
        {
            return Some(code);
        }
    }
    None
}

/// The index of the code prompt — the boundary below which failure words
/// mean this login failed.
fn code_prompt_idx(lines: &[String]) -> Option<usize> {
    lines.iter().position(|line| mentions_code(line))
}

fn detect_error(lines: &[String]) -> Option<String> {
    let prompt = code_prompt_idx(lines)?;
    lines
        .iter()
        .skip(prompt + 1)
        .map(|line| line.trim())
        .find(|line| {
            let lowered = line.to_ascii_lowercase();
            ERROR_ANCHORS.iter().any(|anchor| lowered.contains(anchor))
        })
        .map(|line| line.to_string())
}

/// Whether a line says the WORD "code" — `Codex` must not count, or the
/// product's own name would turn every banner into a code prompt.
fn mentions_code(line: &str) -> bool {
    let lowered = line.to_ascii_lowercase();
    let bytes = lowered.as_bytes();
    let mut from = 0;
    while let Some(rel) = lowered[from..].find("code") {
        let at = from + rel;
        let before_ok = at == 0 || !bytes[at - 1].is_ascii_alphabetic();
        let after = bytes.get(at + 4).copied();
        let after_ok = after.map(|b| !b.is_ascii_alphanumeric()).unwrap_or(true);
        if before_ok && after_ok {
            return true;
        }
        from = at + 4;
    }
    false
}

/// The first code-shaped token on a line (box borders and punctuation
/// around it are stripped).
fn code_in(line: &str) -> Option<String> {
    line.split_whitespace().find_map(code_token)
}

/// `XXXX-XXXXX`: two 4–8 character upper-case/digit runs around ONE dash.
fn code_token(raw: &str) -> Option<String> {
    let token = raw.trim_matches(|c: char| !(c.is_ascii_alphanumeric() || c == '-'));
    let (head, tail) = token.split_once('-')?;
    let run_ok = |run: &str| {
        (4..=8).contains(&run.len())
            && run
                .chars()
                .all(|c| c.is_ascii_digit() || c.is_ascii_uppercase())
    };
    (run_ok(head) && run_ok(tail)).then(|| token.to_string())
}

/// Debounce depth, matching [`crate::login_picker::LoginWatcher`]: a screen
/// must hold for this many consecutive ticks before the machine transitions.
const STREAK: u8 = 2;

/// What a settled tick means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Transition {
    /// A Codex login screen settled (or the settled screen changed).
    Show(CodexLoginPhase),
    /// Every login screen left the grid while one was pending.
    Resolved,
}

/// Per-session Codex login state machine — the same shape as
/// [`crate::login_picker::LoginWatcher`], keyed on [`CodexLoginPhase`].
#[derive(Default)]
pub struct CodexLoginWatcher {
    pending: Option<CodexLoginPhase>,
    candidate: Option<CodexLoginPhase>,
    present_streak: u8,
    absent_streak: u8,
}

impl CodexLoginWatcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the flow currently waits on a human.
    pub fn is_pending(&self) -> bool {
        matches!(
            self.pending,
            Some(CodexLoginPhase::DeviceCode { .. }) | Some(CodexLoginPhase::Error { .. })
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
                // Success out of nowhere is codex ECHOING login-ish text in
                // ordinary output — only a continuation of a pending phase
                // may surface it.
                if matches!(phase, CodexLoginPhase::Success) && self.pending.is_none() {
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

    /// codex 0.144.5 `codex login --device-auth`: prose, then a bare URL
    /// row, then the code on its own prompt line.
    fn device_code_screen() -> Vec<String> {
        screen(&[
            "  Sign in to Codex",
            "",
            "  Open this URL in your browser:",
            "  https://auth.openai.com/codex/device",
            "",
            "  and enter the code: WDJB-MJHT",
            "",
            "  Waiting for authorization…",
        ])
    }

    #[test]
    fn detects_the_device_code_screen() {
        assert_eq!(
            detect(&device_code_screen()),
            Some(CodexLoginPhase::DeviceCode {
                url: "https://auth.openai.com/codex/device".to_string(),
                code: "WDJB-MJHT".to_string(),
            })
        );

        // Variation: URL and code on ONE prose line.
        let one_line = screen(&[
            "  Open https://auth.openai.com/codex/device and enter code WDJB-MJHT",
            "  Waiting…",
        ]);
        assert_eq!(
            detect(&one_line),
            Some(CodexLoginPhase::DeviceCode {
                url: "https://auth.openai.com/codex/device".to_string(),
                code: "WDJB-MJHT".to_string(),
            })
        );

        // Variation: boxed, with the code a couple of blank rows below its
        // prompt.
        let boxed = screen(&[
            "╭────────────────────────────────────────────╮",
            "│ Open this URL in your browser:             │",
            "│ https://auth.openai.com/codex/device       │",
            "│                                            │",
            "│ Then enter the code:                       │",
            "│                                            │",
            "│ WDJB-MJHT                                  │",
            "╰────────────────────────────────────────────╯",
        ]);
        assert_eq!(
            detect(&boxed),
            Some(CodexLoginPhase::DeviceCode {
                url: "https://auth.openai.com/codex/device".to_string(),
                code: "WDJB-MJHT".to_string(),
            })
        );

        // A URL with no code is not a finishable screen.
        let url_only = screen(&["  https://auth.openai.com/codex/device", "  Waiting…"]);
        assert_eq!(detect(&url_only), None);
    }

    #[test]
    fn reconstructs_a_wrapped_url() {
        // A long device URL hard-wraps at the grid width, and the code row
        // must not be swallowed into it.
        let wrapped = screen(&[
            "  Open this URL in your browser:",
            "",
            "https://auth.openai.com/codex/device?user_code=WDJB-MJHT&iss=https%3A%2F%2F",
            "auth.openai.com",
            "",
            "  Then enter the code:",
            "",
            "  WDJB-MJHT",
        ]);
        assert_eq!(
            detect(&wrapped),
            Some(CodexLoginPhase::DeviceCode {
                url: "https://auth.openai.com/codex/device?user_code=WDJB-MJHT&iss=https%3A%2F%2Fauth.openai.com".to_string(),
                code: "WDJB-MJHT".to_string(),
            })
        );
    }

    #[test]
    fn refuses_untrusted_hosts() {
        // The grid is agent-writable: a login-looking screen pointing
        // somewhere else is phishing, not a login.
        let spoof = screen(&[
            "  Open this URL in your browser:",
            "  https://auth.openai.com.evil.test/codex/device",
            "  and enter the code: WDJB-MJHT",
        ]);
        assert_eq!(detect(&spoof), None);

        // A trusted URL further down does not rescue an untrusted first
        // one — the FIRST https on the grid is the candidate.
        let mixed = screen(&[
            "  https://evil.test/codex/device",
            "  https://auth.openai.com/codex/device",
            "  and enter the code: WDJB-MJHT",
        ]);
        assert_eq!(detect(&mixed), None);
    }

    #[test]
    fn code_word_without_a_code_is_not_a_prompt() {
        let no_code = screen(&[
            "  Open this URL in your browser:",
            "  https://auth.openai.com/codex/device",
            "  and enter the code shown in your browser",
            "  Waiting for authorization…",
        ]);
        assert_eq!(detect(&no_code), None);

        // "Codex" is not the word "code".
        assert!(!mentions_code("  Sign in to Codex"));
        assert!(mentions_code("  and enter the code: WDJB-MJHT"));
        // Shapes that are not codes.
        assert_eq!(code_in("  abc-defg"), None, "lower case");
        assert_eq!(code_in("  AB-CDEF"), None, "run too short");
        assert_eq!(code_in("  WDJB-MJHT-XTRA"), None, "two dashes");
        assert_eq!(code_in("│ WDJB-MJHT │"), Some("WDJB-MJHT".to_string()));
    }

    #[test]
    fn watcher_debounces_show_and_resolve() {
        let mut watcher = CodexLoginWatcher::new();
        let idle = screen(&["$ ", ""]);
        assert_eq!(watcher.tick(&device_code_screen(), 0), None, "one tick only");
        let shown = watcher.tick(&device_code_screen(), 0);
        assert!(matches!(
            shown,
            Some(Transition::Show(CodexLoginPhase::DeviceCode { .. }))
        ));
        assert!(watcher.is_pending());
        // A settled screen re-reads as nothing new.
        assert_eq!(watcher.tick(&device_code_screen(), 0), None);
        // Scrolled back into history: frozen.
        assert_eq!(watcher.tick(&idle, 4), None);
        assert!(watcher.is_pending());
        assert_eq!(watcher.tick(&idle, 0), None);
        assert_eq!(watcher.tick(&idle, 0), Some(Transition::Resolved));
        assert!(!watcher.is_pending());

        // Success only surfaces as a continuation of a pending phase.
        let success = screen(&["  Successfully logged in as pat@example.com"]);
        let mut fresh = CodexLoginWatcher::new();
        assert_eq!(fresh.tick(&success, 0), None);
        assert_eq!(fresh.tick(&success, 0), None);

        let mut flow = CodexLoginWatcher::new();
        flow.tick(&device_code_screen(), 0);
        flow.tick(&device_code_screen(), 0);
        assert_eq!(flow.tick(&success, 0), None);
        assert_eq!(
            flow.tick(&success, 0),
            Some(Transition::Show(CodexLoginPhase::Success))
        );
    }

    #[test]
    fn an_expired_code_reads_as_an_error() {
        let expired = screen(&[
            "  Open this URL in your browser:",
            "  https://auth.openai.com/codex/device",
            "  and enter the code: WDJB-MJHT",
            "",
            "  The device code expired. Press Enter to try again.",
        ]);
        assert_eq!(
            detect(&expired),
            Some(CodexLoginPhase::Error {
                message: "The device code expired. Press Enter to try again.".to_string(),
            })
        );

        // Failure words ABOVE the code prompt are ordinary output.
        let noisy = screen(&[
            "  warning: previous login failed, retrying",
            "  Open this URL in your browser:",
            "  https://auth.openai.com/codex/device",
            "  and enter the code: WDJB-MJHT",
        ]);
        assert!(matches!(
            detect(&noisy),
            Some(CodexLoginPhase::DeviceCode { .. })
        ));
    }

    #[test]
    fn trusted_codex_urls_accept_openai_domains_only() {
        for url in [
            "https://auth.openai.com/codex/device",
            "https://auth.openai.com/codex/device?user_code=WDJB-MJHT",
            "https://chatgpt.com/codex/device",
            "https://auth.openai.com:443/codex/device",
        ] {
            assert!(is_trusted_codex_login_url(url), "{url} should be trusted");
        }
        for url in [
            "http://auth.openai.com/codex/device",  // https only
            "https://auth.openai.com.evil.test/x",  // suffix spoof
            "https://openai.com/codex/device",      // parent domain is not on the list
            "https://auth.openai.com@evil.test/x",  // userinfo trick
            "https://xchatgpt.com/device",          // not a subdomain
            "https://claude.ai/oauth/authorize",    // the other agent's list
            "notaurl",
        ] {
            assert!(!is_trusted_codex_login_url(url), "{url} must be refused");
        }
    }
}
