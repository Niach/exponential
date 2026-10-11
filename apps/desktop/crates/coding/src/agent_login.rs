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
//!
//! [`LoginProgress`] is the wire between the machine running the login and
//! the client that asked for it: as soon as the sign-in URL is on screen the
//! device completes its `agent_login` command with this JSON, so the
//! requester can show the link (and Codex's device code) without waiting for
//! the whole login to finish. `devices.completeCommand` caps a result at
//! 2000 chars — [`LoginProgress::to_result_text`] keeps the URL whole and
//! truncates the message instead.
//!
//! Every sign-in runs in a fresh STAGING dir (the CLI's config-dir variable
//! points there, [`begin_login`]) and is COMMITTED by email once the CLI
//! exits signed in ([`commit_login`]): the profile already signed in as that
//! address gets the fresh credential, else the staging dir becomes a new
//! profile. A login never touches an existing profile (or the ambient
//! login) before it is known WHO signed in, so signing in as A from B's
//! "Sign in" refreshes A and leaves B alone. [`import_ambient`] moves the
//! CLI's own ambient login into a profile by the same rules.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use terminal::pty::SpawnSpec;

use crate::agent::CodingAgent;
use crate::agent_profiles;
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
        },
        // Device auth prints a URL plus a short code — the shape a phone can
        // finish.
        CodingAgent::Codex => LoginPlan {
            spawn: SpawnSpec::new(program).args(["login", "--device-auth"]),
            title: "Sign in to Codex".to_string(),
        },
    };
    if remote {
        plan.spawn.env.push(("BROWSER".to_string(), "true".to_string()));
    }
    plan
}

/// Sign `agent` OUT inside one config dir: `env` is the profile's
/// config-dir pair ([`agent_profiles::config_env`]), so only THAT login goes.
/// Blocking; `Err` carries a user-facing sentence.
pub fn logout_in(
    settings: &Settings,
    agent: CodingAgent,
    env: Option<&(String, String)>,
) -> Result<(), String> {
    let args: &[&str] = match agent {
        CodingAgent::Claude => &["auth", "logout"],
        CodingAgent::Codex => &["logout"],
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

/// EXP-1137: codex's credential file inside a config dir (`$CODEX_HOME` or a
/// profile's dir). The ONE codex file this crate ever deletes or moves.
pub const CODEX_AUTH_FILE: &str = "auth.json";

/// EXP-1137 — sign `profile_id`'s login of `agent` OUT on this machine and
/// keep the profile (its dir, its index row, its remembered email): the
/// "Sign out" entry.
///
/// claude: the CLI's own `auth logout` inside that profile's config dir
/// ([`logout_in`]; the refresher already treats a vanished store as a
/// sign-out).
///
/// codex: the credential FILE in the profile's dir is deleted — never `codex
/// logout`, which revokes the session with OpenAI server-side for every
/// machine sharing it. A missing file is a login already gone.
///
/// `Err` carries a user-facing sentence. Nothing here verifies the result:
/// the caller re-probes ([`crate::agent_usage::sign_out_profile`]), because a
/// codex build that keeps its credential in the OS keyring would still be
/// signed in after the file went.
pub fn sign_out_in(
    settings: &Settings,
    data_dir: &Path,
    agent: CodingAgent,
    profile_id: &str,
) -> Result<(), String> {
    let Some(home) = agent_profiles::profile_dir(data_dir, agent, profile_id) else {
        return Err(format!(
            "This machine has no {} config dir for that account.",
            agent.label()
        ));
    };
    match agent {
        CodingAgent::Claude => logout_in(
            settings,
            agent,
            agent_profiles::config_env(data_dir, agent, Some(profile_id)).as_ref(),
        ),
        CodingAgent::Codex => remove_codex_credential(&home).map(|_| ()),
    }
}

/// EXP-1137: delete `<home>/auth.json`; `Ok(false)` = there was none.
pub(crate) fn remove_codex_credential(home: &Path) -> Result<bool, String> {
    match std::fs::remove_file(home.join(CODEX_AUTH_FILE)) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(format!(
            "Could not remove codex's credential file ({}): {err}",
            home.join(CODEX_AUTH_FILE).display()
        )),
    }
}

/// What an `agent_login` command (or a local sign-in) is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoginTarget {
    /// "Add account": a sign-in with no intended profile.
    Add,
    /// "Sign in" on an existing profile, by id. Only the duplicate check
    /// reads it: the login still commits into whichever profile its EMAIL
    /// names.
    Profile(String),
    /// Move the ambient login into a profile ([`import_ambient`]); nothing is
    /// spawned.
    Import,
}

impl LoginTarget {
    /// The intended profile, for [`commit_login`]'s duplicate check.
    pub fn intended(&self) -> Option<&str> {
        match self {
            LoginTarget::Profile(id) => Some(id.as_str()),
            LoginTarget::Add | LoginTarget::Import => None,
        }
    }
}

/// A parsed `agent_login` payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginRequest {
    pub agent: CodingAgent,
    pub target: LoginTarget,
}

/// The sentences both hosts refuse a remote login with (the clients show a
/// failed row's `result` verbatim).
pub const UNKNOWN_AGENT: &str = "This machine does not know that agent.";
pub const MALFORMED_PAYLOAD: &str = "Malformed command payload.";

/// Parse an `agent_login` payload: `{agent, switch?: "true"|"false",
/// profileId?, import?: "true"}`. `switch` is still validated but means
/// nothing any more (a login runs in a fresh dir, so there is nothing to sign
/// out of first); a legacy `newProfileLabel` is ignored; `profileId` blank or
/// the retired `system` reads as absent. `Err` carries the refusal sentence
/// the command completes with.
pub fn parse_login_payload(payload: &serde_json::Value) -> Result<LoginRequest, String> {
    let raw_agent = payload["agent"].as_str().unwrap_or_default();
    let agent = match CodingAgent::parse(raw_agent) {
        Some(agent) => agent,
        // Byte-identical to the pre-profile refusals: a blank agent was a
        // malformed payload, an unknown one an unknown agent.
        None if raw_agent.is_empty() => return Err(MALFORMED_PAYLOAD.to_string()),
        None => return Err(UNKNOWN_AGENT.to_string()),
    };
    if !matches!(payload["switch"].as_str().unwrap_or("false"), "true" | "false") {
        return Err(MALFORMED_PAYLOAD.to_string());
    }
    let import = match payload["import"].as_str().unwrap_or("false") {
        "true" => true,
        "false" => false,
        _ => return Err(MALFORMED_PAYLOAD.to_string()),
    };
    let profile_id = payload["profileId"]
        .as_str()
        .filter(|id| !agent_profiles::is_unpinned(Some(id)))
        .map(|id| id.trim().to_string());
    let target = match (profile_id, import) {
        (Some(_), true) => return Err(MALFORMED_PAYLOAD.to_string()),
        (None, true) => LoginTarget::Import,
        (Some(id), false) => LoginTarget::Profile(id),
        (None, false) => LoginTarget::Add,
    };
    Ok(LoginRequest { agent, target })
}

/// The `{email} was already added. Refreshed it.` warning (the device-doctor
/// fixture's `copy.alreadyAdded`, byte-identical ×4).
pub fn already_added(email: &str) -> String {
    format!("{email} was already added. Refreshed it.")
}

/// Where a sign-in or an import landed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginCommit {
    pub agent: CodingAgent,
    /// The profile that now holds the credential.
    pub profile_id: String,
    /// Who signed in, when the CLI says.
    pub email: Option<String>,
    /// An EXISTING profile was refreshed that the person did not aim at (an
    /// "Add account"/Import that found its email already here, or a "Sign
    /// in" on one profile that signed in as another's address).
    pub duplicate: bool,
}

impl LoginCommit {
    /// The warning toast for a duplicate ([`already_added`]); `None` otherwise.
    pub fn duplicate_warning(&self) -> Option<String> {
        self.duplicate
            .then(|| self.email.as_deref().map(already_added))
            .flatten()
    }

    /// An Import's plain result text: `Imported {email}.`, or the duplicate
    /// warning when the login was already here.
    pub fn import_result_text(&self) -> String {
        if let Some(warning) = self.duplicate_warning() {
            return warning;
        }
        match &self.email {
            Some(email) => format!("Imported {email}."),
            None => format!("Imported the {} login.", self.agent.label()),
        }
    }
}

/// Start a sign-in: a fresh STAGING dir for `agent`
/// ([`agent_profiles::staging_dir`]) the login runs in, plus the config-dir
/// pair that points the CLI at it. `Err` is the refusal sentence.
pub fn begin_login(data_dir: &Path, agent: CodingAgent) -> Result<(PathBuf, (String, String)), String> {
    let var = agent_profiles::config_env_var(agent)
        .ok_or_else(|| format!("{} has no account profiles.", agent.id()))?;
    let dir = agent_profiles::staging_dir(data_dir, agent)
        .map_err(|err| format!("Could not prepare the sign-in: {err}"))?;
    let env = (var.to_string(), dir.to_string_lossy().into_owned());
    Ok((dir, env))
}

/// An abandoned or failed sign-in: delete its staging dir (and, on macOS, the
/// keychain item claude named after it). Touches nothing else.
pub fn abandon_login(agent: CodingAgent, staging: &Path) {
    if agent == CodingAgent::Claude {
        crate::claude_oauth::forget_dir_credential(staging);
    }
    let _ = std::fs::remove_dir_all(staging);
}

/// The sign-in in `staging` FINISHED (the CLI exited cleanly): read who
/// signed in and commit it ([`commit_staging`]). `intended` = the profile the
/// person clicked "Sign in" on (`None` = "Add account"). The staging dir is
/// gone afterwards, either way. `Err` is the sentence to show.
pub fn commit_login(
    settings: &Settings,
    data_dir: &Path,
    agent: CodingAgent,
    staging: &Path,
    intended: Option<&str>,
) -> Result<LoginCommit, String> {
    let email = match agent {
        CodingAgent::Claude => {
            let status = crate::doctor::probe_claude_auth_status_in(
                &settings.resolved_path_for(agent),
                &terminal::pty::login_path(),
                Some(("CLAUDE_CONFIG_DIR", staging)),
            );
            match status {
                Some(status) if status.logged_in => status.email,
                _ => {
                    abandon_login(agent, staging);
                    return Err(format!("The {} sign-in did not finish.", agent.label()));
                }
            }
        }
        CodingAgent::Codex => {
            let auth = staging.join(CODEX_AUTH_FILE);
            if !auth.is_file() {
                abandon_login(agent, staging);
                return Err(format!("The {} sign-in did not finish.", agent.label()));
            }
            codex_identity(&auth).email
        }
    };
    commit_staging(settings, data_dir, agent, staging, email, intended)
}

/// Commit a signed-in `staging` dir as `email` (C/D of the accounts
/// contract): the profile already known as that address (what its CLI says
/// now, else `emails.json`) takes the credential and the staging dir goes;
/// with no such profile the staging dir BECOMES a new one. Either way the
/// profile's `lastLoginAt` is stamped, its email remembered and its usage
/// cache forgotten. The caller re-runs the doctor.
fn commit_staging(
    settings: &Settings,
    data_dir: &Path,
    agent: CodingAgent,
    staging: &Path,
    email: Option<String>,
    intended: Option<&str>,
) -> Result<LoginCommit, String> {
    let email = email
        .map(|email| email.trim().to_string())
        .filter(|email| !email.is_empty());
    let matched = email.as_deref().and_then(|email| {
        agent_profiles::find_by_email(data_dir, agent, email, |id| {
            profile_email(settings, data_dir, agent, id)
        })
    });
    let landed = match &matched {
        Some(id) => {
            let Some(target) = agent_profiles::profile_dir(data_dir, agent, id) else {
                abandon_login(agent, staging);
                return Err(format!("This machine has no {} profile {id}.", agent.label()));
            };
            if let Err(err) = move_login(agent, staging, &target) {
                abandon_login(agent, staging);
                return Err(err);
            }
            abandon_login(agent, staging);
            id.clone()
        }
        None => {
            let adopted = match agent_profiles::adopt_dir(data_dir, agent, staging) {
                Ok(profile) => profile,
                Err(err) => {
                    abandon_login(agent, staging);
                    return Err(format!("Could not add the account: {err}"));
                }
            };
            if agent == CodingAgent::Claude {
                if let Some(dir) = agent_profiles::profile_dir(data_dir, agent, &adopted.id) {
                    if let Err(err) = crate::claude_oauth::rehome_after_rename(staging, &dir) {
                        log::warn!("agent_login: the new login's credential stayed behind: {err}");
                    }
                }
            }
            adopted.id
        }
    };
    if let Err(err) = agent_profiles::stamp_login(data_dir, agent, &landed, email.as_deref()) {
        log::warn!("agent_login: the sign-in stamp was not recorded ({landed}): {err}");
    }
    crate::usage_cache::forget_profile(data_dir, agent.id(), &landed);
    let duplicate = matched.is_some() && intended.map(str::trim) != Some(landed.as_str());
    Ok(LoginCommit {
        agent,
        profile_id: landed,
        email,
        duplicate,
    })
}

/// The address profile `id` is signed in as RIGHT NOW (claude's `auth
/// status` in its dir; codex's `auth.json` id token). `None` = signed out or
/// unnamed.
fn profile_email(settings: &Settings, data_dir: &Path, agent: CodingAgent, id: &str) -> Option<String> {
    let dir = agent_profiles::profile_dir(data_dir, agent, id)?;
    match agent {
        CodingAgent::Claude => crate::doctor::probe_claude_auth_status_in(
            &settings.resolved_path_for(agent),
            &terminal::pty::login_path(),
            Some(("CLAUDE_CONFIG_DIR", &dir)),
        )
        .filter(|status| status.logged_in)
        .and_then(|status| status.email),
        CodingAgent::Codex => codex_identity(&dir.join(CODEX_AUTH_FILE)).email,
    }
}

/// Move the login the CLI wrote under `from` into the existing profile dir
/// `to`. claude: the credential document ([`crate::claude_oauth::move_credential`])
/// plus `.claude.json`'s `oauthAccount`; codex: `auth.json`.
fn move_login(agent: CodingAgent, from: &Path, to: &Path) -> Result<(), String> {
    match agent {
        CodingAgent::Claude => {
            match crate::claude_oauth::move_credential(from, to) {
                Ok(true) => {}
                Ok(false) => return Err("The sign-in left no credential behind.".to_string()),
                Err(err) => return Err(format!("Could not store the sign-in: {err}")),
            }
            copy_oauth_account(&from.join(CLAUDE_STATE_FILE), &to.join(CLAUDE_STATE_FILE));
            Ok(())
        }
        CodingAgent::Codex => move_file(&from.join(CODEX_AUTH_FILE), &to.join(CODEX_AUTH_FILE))
            .map_err(|err| format!("Could not store the sign-in: {err}")),
    }
}

/// claude's per-config-dir state file; its `oauthAccount` key names the login.
const CLAUDE_STATE_FILE: &str = ".claude.json";

/// Overwrite the `oauthAccount` key of the claude state file `to` with
/// `from`'s (every other key of `to` stays). Best effort: the key is the
/// CLI's cached identity, never the credential.
fn copy_oauth_account(from: &Path, to: &Path) {
    let read = |path: &Path| {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
    };
    let Some(account) = read(from).and_then(|doc| doc.get("oauthAccount").cloned()) else {
        return;
    };
    let mut target = read(to)
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}));
    target["oauthAccount"] = account;
    let written = serde_json::to_string_pretty(&target)
        .map_err(std::io::Error::other)
        .and_then(|json| crate::atomic_config::write_atomic(to, &json));
    if let Err(err) = written {
        log::warn!("agent_login: oauthAccount not copied to {}: {err}", to.display());
    }
}

/// Rename, falling back to copy + delete across filesystems.
fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to)?;
    std::fs::remove_file(from)
}

/// IMPORT: move `agent`'s AMBIENT login (`~/.claude`, `~/.codex`) into a
/// profile — the same commit-by-email as a sign-in ([`commit_staging`]), with
/// no intended profile (an existing same-email profile = duplicate). The
/// ambient login is SIGNED OUT afterwards by deleting the store it was read
/// from — never `claude auth logout` / `codex logout`, which would revoke the
/// credential that just moved.
pub fn import_ambient(settings: &Settings, data_dir: &Path, agent: CodingAgent) -> Result<LoginCommit, String> {
    let nothing = || format!("There is no {} login to import on this machine.", agent.label());
    let (staging, _) = begin_login(data_dir, agent)?;
    match agent {
        CodingAgent::Claude => {
            let status = crate::doctor::probe_claude_auth_status_in(
                &settings.resolved_path_for(agent),
                &terminal::pty::login_path(),
                None,
            );
            let Some(status) = status.filter(|status| status.logged_in) else {
                abandon_login(agent, &staging);
                return Err(nothing());
            };
            let store = match crate::claude_oauth::read_store(None) {
                crate::claude_oauth::StoreRead::Found(store) => store,
                crate::claude_oauth::StoreRead::Missing => {
                    abandon_login(agent, &staging);
                    return Err(nothing());
                }
                crate::claude_oauth::StoreRead::Denied => {
                    abandon_login(agent, &staging);
                    return Err(format!(
                        "The {} credential store refused the read.",
                        agent.label()
                    ));
                }
            };
            if let Err(err) = crate::claude_oauth::write_profile_credential(&staging, &store.document) {
                abandon_login(agent, &staging);
                return Err(format!("Could not store the login: {err}"));
            }
            if let Some(ambient) = crate::claude_trust::claude_config_path(None) {
                copy_oauth_account(&ambient, &staging.join(CLAUDE_STATE_FILE));
            }
            let commit = commit_staging(settings, data_dir, agent, &staging, status.email, None)?;
            if let Err(err) = crate::claude_oauth::delete_store(&store.source) {
                log::warn!("agent_login: the imported ambient claude login stayed in place: {err}");
            }
            Ok(commit)
        }
        CodingAgent::Codex => {
            let Some(auth) = crate::codex_trust::codex_home(None)
                .map(|home| home.join(CODEX_AUTH_FILE))
                .filter(|auth| auth.is_file())
            else {
                abandon_login(agent, &staging);
                return Err(nothing());
            };
            if let Err(err) = std::fs::copy(&auth, staging.join(CODEX_AUTH_FILE)) {
                abandon_login(agent, &staging);
                return Err(format!("Could not store the login: {err}"));
            }
            let email = codex_identity(&auth).email;
            let commit = commit_staging(settings, data_dir, agent, &staging, email, None)?;
            if let Err(err) = std::fs::remove_file(&auth) {
                log::warn!("agent_login: the imported ambient codex login stayed in place: {err}");
            }
            Ok(commit)
        }
    }
}

/// Who a codex `auth.json` is signed in as: the `email` claim of its
/// `tokens.id_token` (a JWT payload, read WITHOUT verifying — identity only,
/// never trusted for anything else) and the ChatGPT plan beside it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CodexIdentity {
    pub email: Option<String>,
    pub plan: Option<String>,
}

pub fn codex_identity(auth_json: &Path) -> CodexIdentity {
    let claims = std::fs::read_to_string(auth_json)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|doc| doc["tokens"]["id_token"].as_str().map(str::to_string))
        .and_then(|jwt| jwt.split('.').nth(1).and_then(base64url_decode))
        .and_then(|payload| serde_json::from_slice::<serde_json::Value>(&payload).ok());
    let Some(claims) = claims else {
        return CodexIdentity::default();
    };
    let text = |value: &serde_json::Value| {
        value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    CodexIdentity {
        email: text(&claims["email"]),
        plan: text(&claims["https://api.openai.com/auth"]["chatgpt_plan_type"]),
    }
}

/// Unpadded (or padded) base64url → bytes; `None` on any other byte.
fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buffer: u32 = 0;
    let mut bits = 0u32;
    for byte in input.trim_end_matches('=').bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            _ => return None,
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(out)
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
            ..Settings::default()
        };
        let claude = login_plan(&settings, CodingAgent::Claude, false);
        assert_eq!(claude.spawn.program, "/bin/claude");
        assert_eq!(claude.spawn.args, vec!["auth", "login", "--claudeai"]);

        let codex = login_plan(&settings, CodingAgent::Codex, false);
        assert_eq!(codex.spawn.args, vec!["login", "--device-auth"]);
    }

    /// EXP-1137: a codex sign-out is a FILE delete inside that login's config
    /// dir — never `codex logout` — and a missing file is a login already
    /// gone. A named profile's file is the one under its own dir; nothing
    /// else in the dir is touched.
    #[test]
    fn a_codex_sign_out_deletes_only_that_profiles_credential_file() {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-1137-sign-out-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let profile = agent_profiles::create(&dir, CodingAgent::Codex).unwrap();
        let home = agent_profiles::profile_dir(&dir, CodingAgent::Codex, &profile.id).unwrap();
        std::fs::write(home.join(CODEX_AUTH_FILE), "{\"tokens\":{}}").unwrap();
        std::fs::write(home.join("config.toml"), "model = \"o3\"").unwrap();

        let settings = Settings::default();
        sign_out_in(&settings, &dir, CodingAgent::Codex, &profile.id).unwrap();
        assert!(!home.join(CODEX_AUTH_FILE).exists(), "the credential file is gone");
        assert!(home.join("config.toml").exists(), "the CLI's other files stay");
        assert!(
            agent_profiles::get(&dir, CodingAgent::Codex, &profile.id).is_some(),
            "the profile itself stays"
        );
        // Already signed out = nothing to do, not an error.
        assert_eq!(remove_codex_credential(&home), Ok(false));
        sign_out_in(&settings, &dir, CodingAgent::Codex, &profile.id).unwrap();
        // An id this machine does not have names no dir.
        let refusal = sign_out_in(&settings, &dir, CodingAgent::Codex, "deadbeef").unwrap_err();
        assert!(refusal.contains("no Codex config dir"), "{refusal}");
        let _ = std::fs::remove_dir_all(&dir);
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

    /// The payload's shapes, and the refusals: a blank/`system` `profileId`
    /// reads as absent (Add account), a legacy `newProfileLabel` is ignored,
    /// `switch` is validated and otherwise meaningless, `import` is its own
    /// target and never combines with a profile.
    #[test]
    fn login_payload_parses_add_profile_or_import() {
        let add = parse_login_payload(&serde_json::json!({"agent": "claude", "switch": "true"}))
            .unwrap();
        assert_eq!(add, LoginRequest { agent: CodingAgent::Claude, target: LoginTarget::Add });
        let existing = parse_login_payload(&serde_json::json!({
            "agent": "codex", "switch": "false", "profileId": " 0badf00d "
        }))
        .unwrap();
        assert_eq!(existing.agent, CodingAgent::Codex);
        assert_eq!(existing.target, LoginTarget::Profile("0badf00d".to_string()));
        assert_eq!(existing.target.intended(), Some("0badf00d"));
        let legacy = parse_login_payload(&serde_json::json!({
            "agent": "claude", "newProfileLabel": "  Work  "
        }))
        .unwrap();
        assert_eq!(legacy.target, LoginTarget::Add, "a legacy label is ignored");
        for blank in ["", "  ", "system"] {
            let parsed = parse_login_payload(&serde_json::json!({
                "agent": "claude", "profileId": blank
            }))
            .unwrap();
            assert_eq!(parsed.target, LoginTarget::Add, "{blank:?}");
        }
        let import = parse_login_payload(&serde_json::json!({"agent": "codex", "import": "true"}))
            .unwrap();
        assert_eq!(import.target, LoginTarget::Import);
        assert_eq!(import.target.intended(), None);

        // Refusals.
        assert_eq!(
            parse_login_payload(&serde_json::json!({"agent": "pi"})),
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
        assert_eq!(
            parse_login_payload(&serde_json::json!({"agent": "claude", "import": "1"})),
            Err(MALFORMED_PAYLOAD.to_string())
        );
        assert_eq!(
            parse_login_payload(&serde_json::json!({
                "agent": "claude", "profileId": "0badf00d", "import": "true"
            })),
            Err(MALFORMED_PAYLOAD.to_string())
        );
    }

    /// A codex `auth.json` names its login by the id token's claims, read
    /// without a signature check; anything unreadable names nobody.
    #[test]
    fn a_codex_login_is_named_by_its_id_token() {
        let dir = scratch_dir("codex-identity");
        let payload = r#"{"email":"dev@acme.test","https://api.openai.com/auth":{"chatgpt_plan_type":"pro"}}"#;
        let encoded = base64url_encode(payload.as_bytes());
        let auth = dir.join(CODEX_AUTH_FILE);
        std::fs::write(
            &auth,
            format!(r#"{{"tokens":{{"id_token":"eyJhbGciOiJub25lIn0.{encoded}.sig"}}}}"#),
        )
        .unwrap();
        assert_eq!(
            codex_identity(&auth),
            CodexIdentity { email: Some("dev@acme.test".into()), plan: Some("pro".into()) }
        );
        std::fs::write(&auth, r#"{"OPENAI_API_KEY":"sk-x"}"#).unwrap();
        assert_eq!(codex_identity(&auth), CodexIdentity::default());
        assert_eq!(codex_identity(&dir.join("missing.json")), CodexIdentity::default());
        assert_eq!(base64url_decode("aGk"), Some(b"hi".to_vec()));
        assert_eq!(base64url_decode("a*b"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn base64url_encode(bytes: &[u8]) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let n = chunk.iter().enumerate().fold(0u32, |acc, (i, b)| acc | (u32::from(*b) << (16 - 8 * i)));
            for i in 0..=chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            }
        }
        out
    }

    fn codex_auth(dir: &Path, email: &str, token: &str) {
        let payload = base64url_encode(format!(r#"{{"email":"{email}"}}"#).as_bytes());
        std::fs::write(
            dir.join(CODEX_AUTH_FILE),
            format!(r#"{{"tokens":{{"id_token":"h.{payload}.s","refresh_token":"{token}"}}}}"#),
        )
        .unwrap();
    }

    /// THE dedupe rule, end to end on codex (no CLI needed: its identity is
    /// the file). A, B, C on the device; "Sign in" on C signs in as A → A's
    /// profile takes the fresh credential (duplicate warning), B and C stay
    /// untouched, and no fourth profile appears. A new email adds a profile
    /// with no warning. A failed login leaves nothing behind.
    #[test]
    fn a_login_commits_into_the_profile_of_its_email() {
        let dir = scratch_dir("commit");
        let settings = Settings::default();
        let agent = CodingAgent::Codex;
        let mut ids = Vec::new();
        for (email, token) in [("a@acme.test", "a1"), ("b@acme.test", "b1"), ("c@acme.test", "c1")] {
            let (staging, (var, value)) = begin_login(&dir, agent).unwrap();
            assert_eq!(var, "CODEX_HOME");
            assert_eq!(Path::new(&value), staging.as_path());
            codex_auth(&staging, email, token);
            let commit = commit_login(&settings, &dir, agent, &staging, None).unwrap();
            assert!(!commit.duplicate, "{email} is new here");
            assert_eq!(commit.duplicate_warning(), None);
            assert!(!staging.exists(), "the staging dir became the profile");
            ids.push(commit.profile_id);
        }
        assert_eq!(agent_profiles::list(&dir, agent).len(), 3);
        let stamp_before = |id: &str| agent_profiles::get(&dir, agent, id).unwrap().last_login_at;
        let (b_stamp, c_stamp) = (stamp_before(&ids[1]), stamp_before(&ids[2]));

        // "Sign in" on C, signed in as A (in another case).
        let (staging, _) = begin_login(&dir, agent).unwrap();
        codex_auth(&staging, " A@ACME.test ", "a2");
        let commit = commit_login(&settings, &dir, agent, &staging, Some(&ids[2])).unwrap();
        assert_eq!(commit.profile_id, ids[0]);
        assert!(commit.duplicate);
        assert_eq!(
            commit.duplicate_warning().as_deref(),
            Some("A@ACME.test was already added. Refreshed it.")
        );
        assert!(!staging.exists());
        assert_eq!(agent_profiles::list(&dir, agent).len(), 3, "never A, B, A");
        let home = |id: &str| agent_profiles::profile_dir(&dir, agent, id).unwrap();
        let token = |id: &str| std::fs::read_to_string(home(id).join(CODEX_AUTH_FILE)).unwrap();
        assert!(token(&ids[0]).contains("\"a2\""), "A has the fresh credential");
        assert!(token(&ids[1]).contains("\"b1\""));
        assert!(token(&ids[2]).contains("\"c1\""), "C is untouched");
        assert_eq!(stamp_before(&ids[1]), b_stamp);
        assert_eq!(stamp_before(&ids[2]), c_stamp);
        assert!(stamp_before(&ids[0]).is_some());

        // Signing in as C on C refreshes C quietly.
        let (staging, _) = begin_login(&dir, agent).unwrap();
        codex_auth(&staging, "c@acme.test", "c2");
        let commit = commit_login(&settings, &dir, agent, &staging, Some(&ids[2])).unwrap();
        assert_eq!(commit.profile_id, ids[2]);
        assert!(!commit.duplicate);

        // A login that never wrote a credential: nothing is touched.
        let (staging, _) = begin_login(&dir, agent).unwrap();
        assert!(commit_login(&settings, &dir, agent, &staging, None).is_err());
        assert!(!staging.exists());
        assert_eq!(agent_profiles::list(&dir, agent).len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_import_result_names_the_login_or_warns_about_a_duplicate() {
        let commit = LoginCommit {
            agent: CodingAgent::Claude,
            profile_id: "0badf00d".into(),
            email: Some("dev@acme.test".into()),
            duplicate: false,
        };
        assert_eq!(commit.import_result_text(), "Imported dev@acme.test.");
        let duplicate = LoginCommit { duplicate: true, ..commit.clone() };
        assert_eq!(
            duplicate.import_result_text(),
            "dev@acme.test was already added. Refreshed it."
        );
        let unnamed = LoginCommit { email: None, ..commit };
        assert_eq!(unnamed.import_result_text(), "Imported the Claude Code login.");
    }

    /// The fixture's copy, byte for byte.
    #[test]
    fn the_already_added_copy_matches_the_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/device-doctor.json"
        ))
        .unwrap();
        assert_eq!(
            already_added("{email}"),
            fixture["copy"]["alreadyAdded"].as_str().unwrap()
        );
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
