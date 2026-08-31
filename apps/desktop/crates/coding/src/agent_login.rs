//! EXP-484 — signing an agent CLI in from inside the product: what to spawn,
//! what (if anything) to type at it, and how the run reports the sign-in URL
//! back to whoever asked for it.
//!
//! Every agent already has a local login flow; this is a thin plan around
//! them, never a re-implementation:
//!
//! | agent  | spawn                          | notes                        |
//! |--------|--------------------------------|------------------------------|
//! | claude | `claude auth login --claudeai` | no TUI, no method picker     |
//! | codex  | `codex login --device-auth`    | prints a URL + a device code |
//! | pi     | bare `pi`, then `/login`       | no login command at all      |
//!
//! pi is LOCAL-ONLY: driving a TUI slash command is not something to do to a
//! machine nobody is sitting at ([`login_plan`] still describes it, the
//! remote executor refuses it).
//!
//! [`LoginProgress`] is the wire between the machine running the login and
//! the client that asked for it: as soon as the sign-in URL is on screen the
//! device completes its `agent_login` command with this JSON, so the
//! requester can show the link (and Codex's device code) without waiting for
//! the whole login to finish. `devices.completeCommand` caps a result at
//! 2000 chars — [`LoginProgress::to_result_text`] keeps the URL whole and
//! truncates the message instead.

use serde::{Deserialize, Serialize};

use terminal::pty::SpawnSpec;

use crate::agent::CodingAgent;
use crate::settings::Settings;

/// `devices.completeCommand`'s result cap.
pub const RESULT_TEXT_MAX: usize = 2000;

/// What to run for one agent's login, and what to type once it is ready.
/// (`SpawnSpec` is not `Eq`; tests compare its fields.)
#[derive(Clone, Debug)]
pub struct LoginPlan {
    pub spawn: SpawnSpec,
    /// The terminal tab's title.
    pub title: String,
    /// Sent into the PTY once [`Self::ready_anchor`] shows up — pi's
    /// `/login\r`, nothing for the agents with a real login command.
    pub typed_after_ready: Option<String>,
    /// The on-screen marker that means "the CLI is accepting input".
    pub ready_anchor: Option<String>,
}

/// The login plan for `agent`, against this machine's configured binaries.
///
/// `remote` = the sign-in was queued by an `agent_login` device command
/// (EXP-695): nobody is necessarily sitting at this machine, so the CLI must
/// not pop a browser HERE — the requester gets the link and opens it on
/// their own device. The agent CLIs launch the sign-in URL through
/// `$BROWSER` when it is set, so the plan pins `BROWSER=true` (`true` the
/// no-op command — claude's own background PTYs use the same value to
/// suppress exactly this).
pub fn login_plan(settings: &Settings, agent: CodingAgent, remote: bool) -> LoginPlan {
    let program = settings.resolved_path_for(agent);
    let mut plan = match agent {
        // Verified on 2.1.251: `--claudeai` picks the subscription method
        // outright, so there is no picker to drive.
        CodingAgent::Claude => LoginPlan {
            spawn: SpawnSpec::new(program).args(["auth", "login", "--claudeai"]),
            title: "Sign in to Claude".to_string(),
            typed_after_ready: None,
            ready_anchor: None,
        },
        // Device auth prints a URL plus a short code — the shape a phone can
        // finish.
        CodingAgent::Codex => LoginPlan {
            spawn: SpawnSpec::new(program).args(["login", "--device-auth"]),
            title: "Sign in to Codex".to_string(),
            typed_after_ready: None,
            ready_anchor: None,
        },
        // pi has no login command: its `/login` is a slash command inside
        // the running TUI, typed once the prompt appears.
        CodingAgent::Pi => LoginPlan {
            spawn: SpawnSpec::new(program),
            title: "Sign in to pi".to_string(),
            typed_after_ready: Some("/login\r".to_string()),
            ready_anchor: Some(PI_PROMPT.to_string()),
        },
    };
    if remote {
        plan.spawn.env.push(("BROWSER".to_string(), "true".to_string()));
    }
    plan
}

/// pi's input prompt.
pub const PI_PROMPT: &str = "❯";

/// Whether pi's prompt is on screen (the cue to type `/login`). Reads the
/// LAST few rendered lines only: the anchor also appears inside pi's banner
/// art and in earlier output.
pub fn pi_prompt_ready(lines: &[String]) -> bool {
    lines
        .iter()
        .rev()
        .take(5)
        .any(|line| line.trim_start().starts_with(PI_PROMPT))
}

/// What to warn about before switching accounts. Codex's logout REVOKES the
/// session server-side (every other machine signed in with it loses access),
/// so that one confirms; claude's is local and pi has no account at all.
pub fn warn_on_switch(agent: CodingAgent) -> Option<&'static str> {
    match agent {
        CodingAgent::Codex => Some(
            "Signing out of Codex revokes this session with OpenAI — other machines using it will need to sign in again.",
        ),
        CodingAgent::Claude | CodingAgent::Pi => None,
    }
}

/// Sign `agent` OUT on this machine (the first half of a switch). pi has no
/// logout — its credentials are provider files, and clearing them is not
/// ours to do. Blocking; `Err` carries a user-facing sentence.
pub fn logout(settings: &Settings, agent: CodingAgent) -> Result<(), String> {
    let args: &[&str] = match agent {
        CodingAgent::Claude => &["auth", "logout"],
        CodingAgent::Codex => &["logout"],
        CodingAgent::Pi => return Ok(()),
    };
    let program = settings.resolved_path_for(agent);
    let mut cmd = terminal::process::background_command(&program);
    cmd.env("PATH", terminal::pty::login_path()).args(args);
    match crate::doctor::output_with_timeout(cmd, crate::doctor::PROBE_TIMEOUT) {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let detail = stderr
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .unwrap_or("the command failed");
            Err(format!("Could not sign out of {}: {detail}", agent.id()))
        }
        Err(err) => Err(format!("Could not run {program}: {err}")),
    }
}

/// How far a login got. `Url` is the useful one — the sign-in link (and
/// Codex's device code) is on screen and the requester can act on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoginPhase {
    Url,
    Failed,
}

/// The `agent_login` command's result payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginProgress {
    pub agent: String,
    pub phase: LoginPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Codex's device code; claude's flow has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// A failure sentence (or a note beside a URL).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl LoginProgress {
    /// The sign-in URL is up.
    pub fn url(agent: CodingAgent, url: impl Into<String>, code: Option<String>) -> Self {
        Self {
            agent: agent.id().to_string(),
            phase: LoginPhase::Url,
            url: Some(url.into()),
            code,
            message: None,
        }
    }

    /// The login ended without ever showing one.
    pub fn failed(agent: CodingAgent, message: impl Into<String>) -> Self {
        Self {
            agent: agent.id().to_string(),
            phase: LoginPhase::Failed,
            url: None,
            code: None,
            message: Some(message.into()),
        }
    }

    /// Serialize for `devices.completeCommand`, inside its 2000-char cap.
    /// The cap is measured the way the server measures it (zod `.max` counts
    /// UTF-16 units) on the SERIALIZED text, and over it the MESSAGE shrinks
    /// until the whole thing fits — never the URL, which is the whole point
    /// of the answer, and never by cutting the JSON itself.
    pub fn to_result_text(&self) -> String {
        let fits = |text: &str| text.encode_utf16().count() <= RESULT_TEXT_MAX;
        let rendered = serde_json::to_string(self).unwrap_or_default();
        if fits(&rendered) {
            return rendered;
        }
        let mut trimmed = self.clone();
        let mut message: Vec<char> = self.message.as_deref().unwrap_or_default().chars().collect();
        loop {
            let cut = message.len().saturating_sub((message.len() / 4).max(16));
            message.truncate(cut);
            trimmed.message = (!message.is_empty()).then(|| message.iter().collect());
            let rendered = serde_json::to_string(&trimmed).unwrap_or_default();
            if fits(&rendered) || message.is_empty() {
                return rendered;
            }
        }
    }

    /// Read one back off a `device_commands.result`. `None` = a result from
    /// some other command kind (or an older build) — never an error.
    pub fn parse(text: &str) -> Option<Self> {
        serde_json::from_str(text.trim()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_spawn_each_agents_own_login() {
        let settings = Settings {
            claude_path: "/bin/claude".into(),
            codex_path: "/bin/codex".into(),
            pi_path: "/bin/pi".into(),
            ..Settings::default()
        };
        let claude = login_plan(&settings, CodingAgent::Claude, false);
        assert_eq!(claude.spawn.program, "/bin/claude");
        assert_eq!(claude.spawn.args, vec!["auth", "login", "--claudeai"]);
        assert_eq!(claude.typed_after_ready, None, "no picker to drive");

        let codex = login_plan(&settings, CodingAgent::Codex, false);
        assert_eq!(codex.spawn.args, vec!["login", "--device-auth"]);
        assert_eq!(codex.typed_after_ready, None);

        // pi has no login command: run it and type the slash command.
        let pi = login_plan(&settings, CodingAgent::Pi, false);
        assert!(pi.spawn.args.is_empty());
        assert_eq!(pi.typed_after_ready.as_deref(), Some("/login\r"));
        assert_eq!(pi.ready_anchor.as_deref(), Some("❯"));
    }

    /// EXP-695: a remote sign-in must not open a browser on the machine —
    /// the requester gets the link instead. A local one keeps the CLI's own
    /// browser launch.
    #[test]
    fn a_remote_login_suppresses_the_device_browser() {
        let settings = Settings {
            claude_path: "/bin/claude".into(),
            ..Settings::default()
        };
        let local = login_plan(&settings, CodingAgent::Claude, false);
        assert!(local.spawn.env.is_empty());
        let remote = login_plan(&settings, CodingAgent::Claude, true);
        assert_eq!(
            remote.spawn.env,
            vec![("BROWSER".to_string(), "true".to_string())]
        );
    }

    #[test]
    fn only_codex_warns_before_a_switch() {
        assert!(warn_on_switch(CodingAgent::Codex).is_some());
        assert_eq!(warn_on_switch(CodingAgent::Claude), None);
        assert_eq!(warn_on_switch(CodingAgent::Pi), None);
    }

    #[test]
    fn pi_prompt_is_detected_only_at_the_bottom_of_the_screen() {
        let ready = vec![
            "pi 0.84.1".to_string(),
            "".to_string(),
            "❯ ".to_string(),
        ];
        assert!(pi_prompt_ready(&ready));
        // The banner's own glyphs, far above the cursor, are not a prompt.
        let mut booting = vec!["  ❯❯❯ pi".to_string()];
        booting.extend((0..8).map(|n| format!("loading {n}")));
        assert!(!pi_prompt_ready(&booting));
        assert!(!pi_prompt_ready(&[]));
    }

    #[test]
    fn login_progress_round_trips_through_the_result_text() {
        let progress = LoginProgress::url(
            CodingAgent::Codex,
            "https://auth.openai.com/device",
            Some("WDJB-MJHT".to_string()),
        );
        let text = progress.to_result_text();
        assert_eq!(
            text,
            r#"{"agent":"codex","phase":"url","url":"https://auth.openai.com/device","code":"WDJB-MJHT"}"#
        );
        assert_eq!(LoginProgress::parse(&text), Some(progress));

        let failed = LoginProgress::failed(CodingAgent::Claude, "No sign-in URL appeared.");
        assert_eq!(
            failed.to_result_text(),
            r#"{"agent":"claude","phase":"failed","message":"No sign-in URL appeared."}"#
        );
        assert_eq!(LoginProgress::parse(&failed.to_result_text()), Some(failed));

        // Anything else on a command result reads as "not a login answer".
        assert_eq!(LoginProgress::parse("Pruned 2 worktrees"), None);
        assert_eq!(LoginProgress::parse(""), None);
    }

    #[test]
    fn an_oversized_message_is_trimmed_and_the_url_survives() {
        let url = format!("https://claude.ai/oauth/authorize?code={}", "x".repeat(400));
        let mut progress = LoginProgress::url(CodingAgent::Claude, url.clone(), None);
        progress.message = Some("y".repeat(4000));
        let text = progress.to_result_text();
        assert!(text.chars().count() <= RESULT_TEXT_MAX, "{}", text.len());
        let parsed = LoginProgress::parse(&text).unwrap();
        assert_eq!(parsed.url.as_deref(), Some(url.as_str()), "the URL is never trimmed");
        assert!(parsed.message.unwrap().len() < 4000);
    }
}
