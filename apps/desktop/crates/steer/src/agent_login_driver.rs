//! What a login PTY's grid means, for the two executors that drive one
//! (EXP-484 Phase D).
//!
//! The remote `agent_login` device command runs an agent's own sign-in
//! command in a PTY nobody is watching — the desktop IDE opens it as a
//! terminal tab, the headless daemon opens it with no UI at all — and both
//! poll the grid every 250 ms with exactly one question: is there something
//! the requester can act on yet? [`observe_login_screen`] is that question,
//! and it is the ONLY place the per-agent screen dialects meet:
//!
//! * **claude** — [`crate::login_picker::detect`] first (the mid-session
//!   `/login` TUI, EXP-430's screens), then the anchor-less
//!   [`crate::login_picker::detect_trusted_url`] fallback, because
//!   `claude auth login --claudeai` prints a bare sign-in URL with none of
//!   that TUI around it.
//! * **codex** — [`crate::codex_login_picker::detect`]: URL plus device
//!   code, the only pair a remote requester can finish.
//! * **pi** — nothing. pi's `/login` opens a provider flow inside its TUI
//!   with no remote-finishable handle, so remote sign-in refuses pi outright
//!   and this returns [`LoginObservation::Nothing`] for it.
//!
//! [`LoginObservation::MethodPicker`] exists for the defensive `\r` the
//! executors write once: `--claudeai` should never render a method picker,
//! but a claude release that starts doing so would otherwise hang the PTY
//! until the 10-minute timeout with nobody able to press Enter.

/// What one poll of a login PTY's grid found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoginObservation {
    /// The sign-in link is up (with Codex's device code beside it) — this
    /// is what completes the `agent_login` command early.
    Url { url: String, code: Option<String> },
    /// A login-method picker is waiting on a keypress; the executor writes
    /// `\r` once to take the highlighted (default) option.
    MethodPicker,
    /// The login failed on screen; the sentence is the agent's own.
    Failed(String),
    /// Nothing actionable yet.
    Nothing,
}

/// Read one login screen for `agent` (a [`coding::CodingAgent::id`] string:
/// `claude` / `codex` / `pi`; anything else observes nothing).
///
/// `steer` does not depend on `coding` (§3.1 keeps that direction free), so
/// the agent arrives as its wire id — callers pass `agent.id()`.
pub fn observe_login_screen(agent: &str, lines: &[String]) -> LoginObservation {
    match agent {
        "claude" => observe_claude(lines),
        "codex" => observe_codex(lines),
        // pi: no remote sign-in (see the module docs).
        _ => LoginObservation::Nothing,
    }
}

fn observe_claude(lines: &[String]) -> LoginObservation {
    use crate::login_picker::{detect, detect_trusted_url, LoginPhase};

    match detect(lines) {
        Some(LoginPhase::UrlPrompt { url }) => LoginObservation::Url { url, code: None },
        Some(LoginPhase::MethodPicker { .. }) => LoginObservation::MethodPicker,
        Some(LoginPhase::Error { message }) => LoginObservation::Failed(message),
        // Success is not actionable: the row's signed-in flip arrives from
        // the re-probe after the PTY exits, not from the grid.
        Some(LoginPhase::Success) | None => match detect_trusted_url(lines) {
            Some(url) => LoginObservation::Url { url, code: None },
            None => LoginObservation::Nothing,
        },
    }
}

fn observe_codex(lines: &[String]) -> LoginObservation {
    use crate::codex_login_picker::{detect, CodexLoginPhase};

    match detect(lines) {
        Some(CodexLoginPhase::DeviceCode { url, code }) => LoginObservation::Url {
            url,
            code: Some(code),
        },
        Some(CodexLoginPhase::Error { message }) => LoginObservation::Failed(message),
        Some(CodexLoginPhase::Success) | None => LoginObservation::Nothing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(rows: &[&str]) -> Vec<String> {
        rows.iter().map(|r| r.to_string()).collect()
    }

    #[test]
    fn claude_reads_the_anchored_url_screen() {
        let anchored = screen(&[
            "  Browser didn't open? Use the url below to sign in (c to copy)",
            "",
            "https://claude.ai/oauth/authorize?code=true&client_id=abc&state=xyz",
            "",
            "  Paste code here if prompted >",
        ]);
        assert_eq!(
            observe_login_screen("claude", &anchored),
            LoginObservation::Url {
                url: "https://claude.ai/oauth/authorize?code=true&client_id=abc&state=xyz"
                    .to_string(),
                code: None,
            }
        );
    }

    #[test]
    fn claude_falls_back_to_a_bare_trusted_url() {
        // `claude auth login --claudeai` — no TUI, no anchor, one link.
        let bare = screen(&[
            "Opening your browser to sign in…",
            "https://claude.ai/oauth/authorize?code=true&state=xyz",
            "",
            "Waiting for the browser…",
        ]);
        assert_eq!(
            observe_login_screen("claude", &bare),
            LoginObservation::Url {
                url: "https://claude.ai/oauth/authorize?code=true&state=xyz".to_string(),
                code: None,
            }
        );

        // Off-domain links are never published as a sign-in link.
        let spoof = screen(&["https://evil.test/oauth/authorize?code=true"]);
        assert_eq!(
            observe_login_screen("claude", &spoof),
            LoginObservation::Nothing
        );
    }

    #[test]
    fn claude_reads_the_method_picker_and_errors() {
        let picker = screen(&[
            "  Select login method:",
            "",
            "  ❯ 1. Claude account with subscription",
            "    2. Anthropic Console account",
        ]);
        assert_eq!(
            observe_login_screen("claude", &picker),
            LoginObservation::MethodPicker
        );

        let failed = screen(&[
            "  OAuth error: Request failed with status code 400",
            "  Press Enter to retry.",
        ]);
        assert_eq!(
            observe_login_screen("claude", &failed),
            LoginObservation::Failed(
                "OAuth error: Request failed with status code 400".to_string()
            )
        );
    }

    #[test]
    fn codex_reads_the_device_code_screen() {
        let device = screen(&[
            "  Open this URL in your browser:",
            "  https://auth.openai.com/codex/device",
            "  and enter the code: WDJB-MJHT",
        ]);
        assert_eq!(
            observe_login_screen("codex", &device),
            LoginObservation::Url {
                url: "https://auth.openai.com/codex/device".to_string(),
                code: Some("WDJB-MJHT".to_string()),
            }
        );

        let expired = screen(&[
            "  https://auth.openai.com/codex/device",
            "  and enter the code: WDJB-MJHT",
            "  The device code expired.",
        ]);
        assert_eq!(
            observe_login_screen("codex", &expired),
            LoginObservation::Failed("The device code expired.".to_string())
        );
    }

    #[test]
    fn pi_and_unknown_agents_observe_nothing() {
        let device = screen(&[
            "  Open this URL in your browser:",
            "  https://auth.openai.com/codex/device",
            "  and enter the code: WDJB-MJHT",
        ]);
        assert_eq!(observe_login_screen("pi", &device), LoginObservation::Nothing);
        assert_eq!(
            observe_login_screen("something-else", &device),
            LoginObservation::Nothing
        );
        assert_eq!(observe_login_screen("claude", &[]), LoginObservation::Nothing);
    }
}
