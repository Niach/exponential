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
//!
//! EXP-827: a remote login can target an account PROFILE
//! (`crate::agent_profiles`): the payload names an existing one
//! (`profileId`) or asks for a fresh one (`newProfileLabel`), and the host
//! points the CLI's config-dir variable at that profile's dir for the
//! logout and the login it spawns ([`parse_login_payload`],
//! [`resolve_login_profile`], [`logout_in`]). The result names the id it
//! signed into (`profileId`, `system` for the ambient login) so the
//! requester can pair the link with the profile row the next heartbeat
//! ships.

use std::path::Path;

use serde::{Deserialize, Serialize};

use terminal::pty::SpawnSpec;

use crate::agent::CodingAgent;
use crate::agent_profiles::{self, SYSTEM_PROFILE};
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

/// Sign `agent` OUT on this machine (the first half of a switch), in the
/// ambient login. See [`logout_in`].
pub fn logout(settings: &Settings, agent: CodingAgent) -> Result<(), String> {
    logout_in(settings, agent, None)
}

/// Sign `agent` OUT on this machine (the first half of a switch). pi has no
/// logout — its credentials are provider files, and clearing them is not
/// ours to do. `env` is the profile's config-dir pair
/// ([`agent_profiles::config_env`]) so a switch inside a profile signs out
/// THAT login and never the ambient one; `None` = the ambient login.
/// Blocking; `Err` carries a user-facing sentence.
pub fn logout_in(
    settings: &Settings,
    agent: CodingAgent,
    env: Option<&(String, String)>,
) -> Result<(), String> {
    let args: &[&str] = match agent {
        CodingAgent::Claude => &["auth", "logout"],
        CodingAgent::Codex => &["logout"],
        CodingAgent::Pi => return Ok(()),
    };
    let program = settings.resolved_path_for(agent);
    let mut cmd = terminal::process::background_command(&program);
    cmd.env("PATH", terminal::pty::login_path()).args(args);
    if let Some((key, value)) = env {
        cmd.env(key, value);
    }
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

/// EXP-827: which account an `agent_login` command signs into.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoginTarget {
    /// The ambient login (no `profileId`, no `newProfileLabel`).
    System,
    /// An existing profile, by id (`system` lands on [`Self::System`]).
    Profile(String),
    /// Create a profile with this label first, then sign into it.
    NewProfile(String),
}

/// A parsed `agent_login` payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginRequest {
    pub agent: CodingAgent,
    /// Sign out first (inside the target profile's dir).
    pub switch: bool,
    pub target: LoginTarget,
}

/// The sentence both hosts refuse a remote pi login with (the clients show
/// a failed row's `result` verbatim).
pub const PI_NO_REMOTE_LOGIN: &str = "pi has no remote sign-in";
pub const UNKNOWN_AGENT: &str = "This machine does not know that agent.";
pub const MALFORMED_PAYLOAD: &str = "Malformed command payload.";

/// Parse an `agent_login` payload: `{agent, switch: "true"|"false",
/// profileId?, newProfileLabel?}`. `Err` carries the refusal sentence the
/// command completes with. pi is refused outright: its `/login` is a slash
/// command inside its TUI with nothing to hand back, and it has no profiles
/// either.
pub fn parse_login_payload(payload: &serde_json::Value) -> Result<LoginRequest, String> {
    let raw_agent = payload["agent"].as_str().unwrap_or_default();
    let agent = match CodingAgent::parse(raw_agent) {
        Some(CodingAgent::Pi) => return Err(PI_NO_REMOTE_LOGIN.to_string()),
        Some(agent) => agent,
        // Byte-identical to the pre-profile refusals: a blank agent was a
        // malformed payload, an unknown one an unknown agent.
        None if raw_agent.is_empty() => return Err(MALFORMED_PAYLOAD.to_string()),
        None => return Err(UNKNOWN_AGENT.to_string()),
    };
    let switch = match payload["switch"].as_str().unwrap_or("false") {
        "true" => true,
        "false" => false,
        _ => return Err(MALFORMED_PAYLOAD.to_string()),
    };
    let profile_id = payload["profileId"].as_str().map(str::trim).filter(|id| !id.is_empty());
    let new_label = payload["newProfileLabel"]
        .as_str()
        .map(str::trim)
        .filter(|label| !label.is_empty());
    let target = match (profile_id, new_label) {
        (Some(_), Some(_)) => {
            return Err("Pick an existing profile or a new label, not both.".to_string());
        }
        (Some(id), None) if id == SYSTEM_PROFILE => LoginTarget::System,
        (Some(id), None) => LoginTarget::Profile(id.to_string()),
        (None, Some(label)) => LoginTarget::NewProfile(label.to_string()),
        (None, None) => LoginTarget::System,
    };
    Ok(LoginRequest {
        agent,
        switch,
        target,
    })
}

/// Turn the target into the profile id the login lands on, creating a new
/// profile when asked. `Err` is the refusal sentence: an unknown id, an
/// agent without profiles, a label the index would not take. A freshly
/// created profile is NOT made the device's active one; the requester
/// picks that on the Accounts page.
pub fn resolve_login_profile(
    data_dir: &Path,
    agent: CodingAgent,
    target: &LoginTarget,
) -> Result<String, String> {
    match target {
        LoginTarget::System => Ok(SYSTEM_PROFILE.to_string()),
        LoginTarget::Profile(id) => {
            if agent_profiles::config_env_var(agent).is_none() {
                return Err(format!("{} has no account profiles.", agent.id()));
            }
            match agent_profiles::profile_dir(data_dir, agent, id) {
                Some(_) => Ok(id.clone()),
                None => Err(format!(
                    "This machine has no {} profile {id}.",
                    agent.label()
                )),
            }
        }
        LoginTarget::NewProfile(label) => agent_profiles::create(data_dir, agent, label)
            .map(|profile| profile.id)
            .map_err(|err| format!("Could not create the profile: {err}")),
    }
}

/// The config-dir pair the login (and its logout) runs under for
/// `profile_id`; `None` for `system`.
pub fn login_env(data_dir: &Path, agent: CodingAgent, profile_id: &str) -> Option<(String, String)> {
    agent_profiles::config_env(data_dir, agent, Some(profile_id))
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
    /// EXP-827: the profile the login signed into (`system` for the
    /// ambient login). Absent on a pre-profile build's result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
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
            profile_id: None,
        }
    }

    /// EXP-827: name the profile the login landed on.
    pub fn with_profile(mut self, profile_id: impl Into<String>) -> Self {
        self.profile_id = Some(profile_id.into());
        self
    }

    /// The login ended without ever showing one.
    pub fn failed(agent: CodingAgent, message: impl Into<String>) -> Self {
        Self {
            agent: agent.id().to_string(),
            phase: LoginPhase::Failed,
            url: None,
            code: None,
            message: Some(message.into()),
            profile_id: None,
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

    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "exp-agent-login-{tag}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// EXP-827: the payload's three shapes, and the refusals. A blank
    /// `profileId`/`newProfileLabel` reads as absent; `system` as an id is
    /// the ambient login.
    #[test]
    fn login_payload_parses_profile_id_new_label_or_neither() {
        let neither = parse_login_payload(&serde_json::json!({"agent": "claude", "switch": "true"}))
            .unwrap();
        assert_eq!(
            neither,
            LoginRequest {
                agent: CodingAgent::Claude,
                switch: true,
                target: LoginTarget::System,
            }
        );
        let existing = parse_login_payload(&serde_json::json!({
            "agent": "codex", "switch": "false", "profileId": "0badf00d"
        }))
        .unwrap();
        assert_eq!(existing.agent, CodingAgent::Codex);
        assert!(!existing.switch);
        assert_eq!(existing.target, LoginTarget::Profile("0badf00d".to_string()));
        let fresh = parse_login_payload(&serde_json::json!({
            "agent": "claude", "newProfileLabel": "  Work  "
        }))
        .unwrap();
        assert_eq!(fresh.target, LoginTarget::NewProfile("Work".to_string()));
        assert!(!fresh.switch, "switch defaults to false");
        let blank = parse_login_payload(&serde_json::json!({
            "agent": "claude", "profileId": "", "newProfileLabel": "  "
        }))
        .unwrap();
        assert_eq!(blank.target, LoginTarget::System);
        let system = parse_login_payload(&serde_json::json!({
            "agent": "claude", "profileId": "system"
        }))
        .unwrap();
        assert_eq!(system.target, LoginTarget::System);

        // Refusals.
        assert_eq!(
            parse_login_payload(&serde_json::json!({"agent": "pi", "profileId": "0badf00d"})),
            Err(PI_NO_REMOTE_LOGIN.to_string())
        );
        assert_eq!(
            parse_login_payload(&serde_json::json!({"agent": "pi"})),
            Err(PI_NO_REMOTE_LOGIN.to_string())
        );
        assert_eq!(
            parse_login_payload(&serde_json::json!({"agent": "gemini"})),
            Err(UNKNOWN_AGENT.to_string())
        );
        assert_eq!(
            parse_login_payload(&serde_json::json!({})),
            Err(MALFORMED_PAYLOAD.to_string())
        );
        assert_eq!(
            parse_login_payload(&serde_json::json!({"agent": "claude", "switch": "yes"})),
            Err(MALFORMED_PAYLOAD.to_string())
        );
        assert!(parse_login_payload(&serde_json::json!({
            "agent": "claude", "profileId": "0badf00d", "newProfileLabel": "Work"
        }))
        .is_err());
    }

    /// EXP-827: `system` resolves without touching the disk, an unknown id
    /// is refused, a new label creates the profile (indexed, NOT active)
    /// and the login env points at its dir; pi has no profiles to log into.
    #[test]
    fn login_profile_resolves_creates_and_refuses() {
        let dir = scratch_dir("resolve");
        assert_eq!(
            resolve_login_profile(&dir, CodingAgent::Claude, &LoginTarget::System).unwrap(),
            SYSTEM_PROFILE
        );
        assert_eq!(login_env(&dir, CodingAgent::Claude, SYSTEM_PROFILE), None);
        assert!(resolve_login_profile(
            &dir,
            CodingAgent::Claude,
            &LoginTarget::Profile("0badf00d".to_string())
        )
        .is_err());

        let id = resolve_login_profile(
            &dir,
            CodingAgent::Claude,
            &LoginTarget::NewProfile("Work".to_string()),
        )
        .unwrap();
        let listed = agent_profiles::list(&dir, CodingAgent::Claude);
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[1].id, id);
        assert_eq!(listed[1].label, "Work");
        assert_eq!(
            agent_profiles::active_profile(&dir, CodingAgent::Claude),
            SYSTEM_PROFILE,
            "a new profile does not become the device default"
        );
        // Now it exists, so it resolves as an existing profile too.
        assert_eq!(
            resolve_login_profile(&dir, CodingAgent::Claude, &LoginTarget::Profile(id.clone()))
                .unwrap(),
            id
        );
        let (key, value) = login_env(&dir, CodingAgent::Claude, &id).unwrap();
        assert_eq!(key, "CLAUDE_CONFIG_DIR");
        assert!(value.ends_with(&id));

        // pi: no profiles, existing or new.
        assert!(resolve_login_profile(
            &dir,
            CodingAgent::Pi,
            &LoginTarget::Profile("0badf00d".to_string())
        )
        .is_err());
        assert!(resolve_login_profile(
            &dir,
            CodingAgent::Pi,
            &LoginTarget::NewProfile("Work".to_string())
        )
        .is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-827: the result names the profile; an older result without one
    /// still parses.
    #[test]
    fn login_progress_carries_the_profile_id() {
        let progress = LoginProgress::url(CodingAgent::Claude, "https://claude.ai/x", None)
            .with_profile("0badf00d");
        let text = progress.to_result_text();
        assert!(text.contains("\"profileId\":\"0badf00d\""), "{text}");
        assert_eq!(LoginProgress::parse(&text).unwrap().profile_id.as_deref(), Some("0badf00d"));
        let legacy = LoginProgress::parse(r#"{"agent":"claude","phase":"url","url":"https://x"}"#)
            .unwrap();
        assert_eq!(legacy.profile_id, None);
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
