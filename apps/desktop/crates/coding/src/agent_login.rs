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
//! "Sign in" refreshes A and leaves B alone. Nothing ever copies a credential
//! in: the CLI's own ambient login is never imported (EXP-1532, every profile
//! signs in on its own).

use std::path::{Path, PathBuf};


use terminal::pty::SpawnSpec;

use crate::agent::CodingAgent;
use crate::agent_profiles;
use crate::settings::Settings;


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
    // EXP-1426: the argv, title and BROWSER pin are the agent core's.
    let command = eagent::login::login_command(
        agent,
        settings.resolved_path_for(agent),
        remote,
    );
    let mut spawn = SpawnSpec::new(command.program).args(command.args);
    spawn.env = command.env;
    LoginPlan { spawn, title: command.title }
}

/// Sign `agent` OUT inside one config dir: `env` is the profile's
/// config-dir pair ([`agent_profiles::config_env`]), so only THAT login goes.
/// Blocking; `Err` carries a user-facing sentence.
pub fn logout_in(
    settings: &Settings,
    agent: CodingAgent,
    env: Option<&(String, String)>,
) -> Result<(), String> {
    eagent::login::logout_in(
        agent,
        &settings.resolved_path_for(agent),
        &terminal::pty::login_path(),
        env.map(|(key, value)| (key.as_str(), value.as_str())),
    )
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
        CodingAgent::Claude => {
            // EXP-1533: a long-lived token goes too, or the profile would stay
            // signed in on it.
            crate::claude_token::remove(&home).map_err(|err| {
                format!("Could not remove the long-lived Claude login: {err}")
            })?;
            logout_in(
                settings,
                agent,
                agent_profiles::config_env(data_dir, agent, Some(profile_id)).as_ref(),
            )
        }
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
}

impl LoginTarget {
    /// The intended profile, for [`commit_login`]'s duplicate check.
    pub fn intended(&self) -> Option<&str> {
        match self {
            LoginTarget::Profile(id) => Some(id.as_str()),
            LoginTarget::Add => None,
        }
    }
}

/// A parsed `agent_login` payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginRequest {
    pub agent: CodingAgent,
    pub target: LoginTarget,
    /// EXP-1533: `longLived: "true"` — mint a `claude setup-token` instead
    /// of a browser login (claude only: [`parse_login_payload`] refuses
    /// anything else). EXP-1626: on a profile it PERSISTS that login (the
    /// token is attached to it, its browser login untouched); an Add with
    /// no profile (an older client's) becomes a NEW profile
    /// ([`long_lived_add_plan`]).
    pub long_lived: bool,
}

/// The sentences both hosts refuse a remote login with (the clients show a
/// failed row's `result` verbatim).
pub const UNKNOWN_AGENT: &str = "This machine does not know that agent.";
pub const MALFORMED_PAYLOAD: &str = "Malformed command payload.";
/// EXP-1532: an old client's `agent_login {import: "true"}`. Importing copied
/// a LIVE credential whose single-use refresh token the CLI still running on
/// the ambient login rotated, so the profile's copy died; it is gone.
pub const IMPORT_REMOVED: &str = "Import was removed: sign this account in on this device instead.";
/// EXP-1533: a `longLived` login names no claude profile of this machine.
/// Unchanged text (EXP-1600): an older device refuses a long-lived Add with
/// it, and clients fall back to the browser login on exactly this sentence.
pub const LONG_LIVED_NEEDS_PROFILE: &str = "A long-lived login needs a Claude account on this machine.";
/// EXP-1600: a `longLived` login for an agent other than claude.
pub const LONG_LIVED_CLAUDE_ONLY: &str = "A long-lived login is Claude only.";
/// EXP-1626: the result of an `agent_login` the requester walked away from
/// (`agent_login_cancel`), byte-identical to the server's
/// `AGENT_LOGIN_CANCELLED` (apps/web `lib/trpc/devices.ts`).
pub const LOGIN_CANCELLED: &str = "The sign-in was cancelled.";
/// EXP-1626: a long-lived run's pending code command, failed when the run
/// ends without a token.
pub const SIGN_IN_ENDED: &str = "The sign-in ended before it finished.";
/// EXP-1626: the success line of a code command (the ×4 `SIGNING_IN`).
pub const SIGNING_IN: &str = "Signing in…";
/// EXP-1626: what an `agent_login_cancel` answers when the run it names
/// already took a code (it is left to finish).
pub const CANCEL_TOO_LATE: &str = "A code was already entered: the sign-in continues.";
/// EXP-1626: what an `agent_login_cancel` answers when no run of that
/// command is live here (nothing to stop).
pub const NOTHING_TO_CANCEL: &str = "No sign-in of that request is running on this machine.";

/// EXP-1626: how long ANY sign-in run (remote, or a local long-lived one)
/// may live before the machine stops it itself.
pub const LOGIN_RUN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10 * 60);
/// EXP-1626: a code typed this recently keeps a run alive past
/// [`LOGIN_RUN_TIMEOUT`] until the grace passes.
pub const CODE_GRACE: std::time::Duration = std::time::Duration::from_secs(60);

/// EXP-1626 — whether a sign-in run that started `elapsed` ago (and last
/// took a code `since_code` ago, if ever) is to be stopped now.
pub fn login_run_expired(
    elapsed: std::time::Duration,
    since_code: Option<std::time::Duration>,
) -> bool {
    elapsed >= LOGIN_RUN_TIMEOUT && since_code.is_none_or(|since| since >= CODE_GRACE)
}

/// EXP-1626 — what an `agent_login_cancel` does to the run it names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelDisposition {
    /// No live run of that command here: answered ok, nothing to stop.
    NoRun,
    /// The run already took a code: left to finish (answered ok).
    CodeEntered,
    /// Stop it: kill its child, drop its staging (answered ok).
    Stop,
}

impl CancelDisposition {
    pub fn of(run_found: bool, code_entered: bool) -> Self {
        match (run_found, code_entered) {
            (false, _) => CancelDisposition::NoRun,
            (true, true) => CancelDisposition::CodeEntered,
            (true, false) => CancelDisposition::Stop,
        }
    }

    /// The cancel command's (ok) result.
    pub fn message(self) -> &'static str {
        match self {
            CancelDisposition::NoRun => NOTHING_TO_CANCEL,
            CancelDisposition::CodeEntered => CANCEL_TOO_LATE,
            CancelDisposition::Stop => LOGIN_CANCELLED,
        }
    }
}

/// EXP-1626 — the `loginCommandId` of an `agent_login_cancel` payload.
pub fn parse_cancel_payload(payload: &serde_json::Value) -> Result<String, String> {
    let agent = payload["agent"].as_str().unwrap_or_default();
    let id = payload["loginCommandId"].as_str().unwrap_or_default().trim();
    if CodingAgent::parse(agent).is_none() || id.is_empty() {
        return Err(MALFORMED_PAYLOAD.to_string());
    }
    Ok(id.to_string())
}

/// EXP-1626 — a code typed into a LONG-LIVED run, waiting for its outcome:
/// `claude setup-token` takes the code, then either prints the token or
/// says `OAuth error: … Press Enter to retry.` The code command completes
/// on that outcome, never at the keystroke.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingCode {
    pub command_id: String,
    /// A failure was already on screen when the code went in (a retry):
    /// it counts only once the screen showed something else first.
    failed_at_entry: bool,
    cleared: bool,
}

impl PendingCode {
    /// `failure_on_screen` = what the screen observed as the code went in.
    pub fn new(command_id: impl Into<String>, failure_on_screen: bool) -> Self {
        Self {
            command_id: command_id.into(),
            failed_at_entry: failure_on_screen,
            cleared: !failure_on_screen,
        }
    }

    /// One look at the run: `token` = a setup token was scraped, `failure`
    /// = the screen's claude `Failed` observation. `Some((ok, result))` =
    /// complete the code command with it now.
    pub fn observe(&mut self, token: bool, failure: Option<&str>) -> Option<(bool, String)> {
        if token {
            return Some((true, SIGNING_IN.to_string()));
        }
        match failure {
            Some(message) if self.cleared || !self.failed_at_entry => {
                Some((false, message.to_string()))
            }
            Some(_) => None,
            None => {
                self.cleared = true;
                None
            }
        }
    }

    /// The run ended: `token` = it printed one.
    pub fn ended(&self, token: bool) -> (bool, String) {
        if token {
            (true, SIGNING_IN.to_string())
        } else {
            (false, SIGN_IN_ENDED.to_string())
        }
    }
}

/// Parse an `agent_login` payload: `{agent, switch?: "true"|"false",
/// profileId?}`. `switch` is still validated but means nothing any more (a
/// login runs in a fresh dir, so there is nothing to sign out of first);
/// `profileId` blank or the retired `system` reads as absent. The retired
/// `import: "true"` is refused with [`IMPORT_REMOVED`]. EXP-1533/1612:
/// `longLived: "true"` needs claude ([`LONG_LIVED_CLAUDE_ONLY`]); with no
/// `profileId` it is a long-lived Add. An older client's `name` (EXP-1626's
/// removed profile name) is ignored.
/// `Err` carries the refusal sentence the command completes with.
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
    match payload["import"].as_str().unwrap_or("false") {
        "true" => return Err(IMPORT_REMOVED.to_string()),
        "false" => {}
        _ => return Err(MALFORMED_PAYLOAD.to_string()),
    }
    let profile_id = payload["profileId"]
        .as_str()
        .filter(|id| !agent_profiles::is_unpinned(Some(id)))
        .map(|id| id.trim().to_string());
    let target = match profile_id {
        Some(id) => LoginTarget::Profile(id),
        None => LoginTarget::Add,
    };
    let long_lived = match payload["longLived"].as_str().unwrap_or("false") {
        "true" => true,
        "false" => false,
        _ => return Err(MALFORMED_PAYLOAD.to_string()),
    };
    if long_lived && agent != CodingAgent::Claude {
        return Err(LONG_LIVED_CLAUDE_ONLY.to_string());
    }
    Ok(LoginRequest { agent, target, long_lived })
}

/// EXP-1626 — PERSIST claude profile `profile_id`'s login: `claude
/// setup-token` run in a fresh STAGING dir ([`begin_login`]), NEVER inside the
/// profile (its config-dir var points at the staging dir, so the browser
/// login the profile holds — its `.credentials.json` / keychain item — is
/// never read, refreshed or overwritten by the run). The caller hands the
/// dir and the scraped token to [`commit_setup_token`], or the dir to
/// [`abandon_login`] when the run printed none. `Err` is the refusal sentence
/// (no such profile: [`LONG_LIVED_NEEDS_PROFILE`]).
pub fn long_lived_plan(
    settings: &Settings,
    data_dir: &Path,
    profile_id: &str,
    remote: bool,
) -> Result<(PathBuf, LoginPlan), String> {
    let agent = CodingAgent::Claude;
    if agent_profiles::profile_dir(data_dir, agent, profile_id).is_none() {
        return Err(LONG_LIVED_NEEDS_PROFILE.to_string());
    }
    let (staging, env) = begin_login(data_dir, agent)?;
    let command = eagent::login::setup_token_command(settings.resolved_path_for(agent), remote);
    let mut spawn = SpawnSpec::new(command.program).args(command.args);
    spawn.env = command.env;
    spawn.env.push(env);
    Ok((staging, LoginPlan { spawn, title: command.title }))
}

/// EXP-1626 — commit a Persist login: the scraped token is attached to
/// profile `profile_id` ([`store_setup_token`]) and the run's `staging` dir
/// is dropped, either way. Returns the expiry (ISO).
pub fn commit_setup_token(
    data_dir: &Path,
    profile_id: &str,
    staging: &Path,
    scraped: (String, Option<i64>),
) -> Result<String, String> {
    let stored = store_setup_token(data_dir, profile_id, scraped);
    abandon_login(CodingAgent::Claude, staging);
    stored
}

/// EXP-1600 — a long-lived "Add account": `claude setup-token` in a fresh
/// STAGING dir ([`begin_login`]); the caller keeps the dir and hands it to
/// [`commit_setup_token_add`] with the scraped token, or to
/// [`abandon_login`] when the run printed none. `Err` is the refusal sentence.
pub fn long_lived_add_plan(
    settings: &Settings,
    data_dir: &Path,
    remote: bool,
) -> Result<(PathBuf, LoginPlan), String> {
    let agent = CodingAgent::Claude;
    let (staging, env) = begin_login(data_dir, agent)?;
    let command = eagent::login::setup_token_command(settings.resolved_path_for(agent), remote);
    let mut spawn = SpawnSpec::new(command.program).args(command.args);
    spawn.env = command.env;
    spawn.env.push(env);
    Ok((staging, LoginPlan { spawn, title: command.title }))
}

/// EXP-1600 — commit a long-lived Add: the scraped token goes into the
/// `staging` dir (0600), the dir BECOMES a new claude profile
/// ([`agent_profiles::adopt_dir`]), its `lastLoginAt` is stamped with no
/// email (a setup token names nobody) and its usage cache forgotten. Any
/// failure abandons the staging dir. Returns `(profile id, expiry ISO)`;
/// the token itself never leaves this function. EXP-1626: only an older
/// client's long-lived Add (no profile) still lands here.
pub fn commit_setup_token_add(
    data_dir: &Path,
    staging: &Path,
    scraped: (String, Option<i64>),
) -> Result<(String, String), String> {
    let agent = CodingAgent::Claude;
    let (token, lifetime) = scraped;
    let token = crate::claude_token::SetupToken::minted(
        token,
        chrono::Utc::now(),
        lifetime.unwrap_or(crate::claude_token::TOKEN_LIFETIME_SECS),
    );
    if let Err(err) = crate::claude_token::write(staging, &token) {
        abandon_login(agent, staging);
        return Err(format!("Could not store the long-lived Claude login: {err}"));
    }
    let profile = match agent_profiles::adopt_dir(data_dir, agent, staging) {
        Ok(profile) => profile,
        Err(err) => {
            abandon_login(agent, staging);
            return Err(format!("Could not add the account: {err}"));
        }
    };
    if let Err(err) = agent_profiles::stamp_login(data_dir, agent, &profile.id, None) {
        log::warn!("agent_login: the long-lived login stamp was not recorded ({}): {err}", profile.id);
    }
    crate::usage_cache::forget_profile(data_dir, agent.id(), &profile.id);
    Ok((profile.id, token.expires_at))
}

/// EXP-1533 — the token `claude setup-token` printed on `lines`, if it is
/// there yet: the host keeps the latest scrape and stores it once the CLI
/// is done ([`store_setup_token`]).
pub fn scrape_setup_token(lines: &[String]) -> Option<(String, Option<i64>)> {
    eagent::login::scrape_setup_token(lines)
}

/// EXP-1533 — store a scraped setup token in claude profile `profile_id`
/// (0600) and stamp the profile's `lastLoginAt`, so an open sign-in dialog
/// lands exactly as a browser sign-in does. Returns the expiry (ISO); the
/// token itself never leaves this function. EXP-1626: the profile's usage
/// cache is KEPT — its browser login (if any) still feeds the usage, and a
/// dead grant it already knows must stay known (the usage then reads
/// through the token, `agent_usage`).
pub fn store_setup_token(
    data_dir: &Path,
    profile_id: &str,
    scraped: (String, Option<i64>),
) -> Result<String, String> {
    let agent = CodingAgent::Claude;
    let dir = agent_profiles::profile_dir(data_dir, agent, profile_id)
        .ok_or_else(|| LONG_LIVED_NEEDS_PROFILE.to_string())?;
    let (token, lifetime) = scraped;
    let token = crate::claude_token::SetupToken::minted(
        token,
        chrono::Utc::now(),
        lifetime.unwrap_or(crate::claude_token::TOKEN_LIFETIME_SECS),
    );
    crate::claude_token::write(&dir, &token)
        .map_err(|err| format!("Could not store the long-lived Claude login: {err}"))?;
    if let Err(err) = agent_profiles::stamp_login(data_dir, agent, profile_id, None) {
        log::warn!("agent_login: the long-lived login stamp was not recorded ({profile_id}): {err}");
    }
    Ok(token.expires_at)
}

/// The `{email} was already added. Refreshed it.` warning (the device-doctor
/// fixture's `copy.alreadyAdded`, byte-identical ×4).
pub fn already_added(email: &str) -> String {
    format!("{email} was already added. Refreshed it.")
}

/// Where a sign-in landed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginCommit {
    pub agent: CodingAgent,
    /// The profile that now holds the credential.
    pub profile_id: String,
    /// Who signed in, when the CLI says.
    pub email: Option<String>,
    /// An EXISTING profile was refreshed that the person did not aim at (an
    /// "Add account" that found its email already here, or a "Sign
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

// EXP-1426: the login progress wire moved to the agent core.
pub use eagent::login::{LoginPhase, LoginProgress, RESULT_TEXT_MAX};

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
    /// reads as absent (Add account), `switch` is validated and otherwise
    /// meaningless, and the retired `import` (EXP-1532) is refused with its
    /// own sentence.
    #[test]
    fn login_payload_parses_add_or_profile_and_refuses_import() {
        let add = parse_login_payload(&serde_json::json!({"agent": "claude", "switch": "true"}))
            .unwrap();
        assert_eq!(
            add,
            LoginRequest { agent: CodingAgent::Claude, target: LoginTarget::Add, long_lived: false }
        );
        let existing = parse_login_payload(&serde_json::json!({
            "agent": "codex", "switch": "false", "profileId": " 0badf00d "
        }))
        .unwrap();
        assert_eq!(existing.agent, CodingAgent::Codex);
        assert_eq!(existing.target, LoginTarget::Profile("0badf00d".to_string()));
        assert_eq!(existing.target.intended(), Some("0badf00d"));
        for blank in ["", "  ", "system"] {
            let parsed = parse_login_payload(&serde_json::json!({
                "agent": "claude", "profileId": blank
            }))
            .unwrap();
            assert_eq!(parsed.target, LoginTarget::Add, "{blank:?}");
        }
        assert_eq!(
            parse_login_payload(&serde_json::json!({"agent": "codex", "import": "true"})),
            Err(IMPORT_REMOVED.to_string())
        );
        assert_eq!(
            parse_login_payload(&serde_json::json!({"agent": "codex", "import": "false"}))
                .unwrap()
                .target,
            LoginTarget::Add
        );

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
            Err(IMPORT_REMOVED.to_string())
        );
    }

    /// EXP-1533: `longLived` is claude + a profile only; the token a
    /// setup-token run printed lands 0600 in that profile, stamps its
    /// `lastLoginAt` (the dialogs' landing) and goes with a sign-out.
    #[test]
    fn a_long_lived_login_needs_a_claude_profile_and_stores_its_token() {
        let parsed = parse_login_payload(&serde_json::json!({
            "agent": "claude", "profileId": "0badf00d", "longLived": "true"
        }))
        .unwrap();
        assert!(parsed.long_lived);
        assert_eq!(parsed.target, LoginTarget::Profile("0badf00d".into()));
        // EXP-1600: a long-lived Add is valid; codex never long-lives.
        let add = parse_login_payload(&serde_json::json!({"agent": "claude", "longLived": "true"}))
            .unwrap();
        assert_eq!(
            add,
            LoginRequest { agent: CodingAgent::Claude, target: LoginTarget::Add, long_lived: true }
        );
        for payload in [
            serde_json::json!({"agent": "codex", "profileId": "0badf00d", "longLived": "true"}),
            serde_json::json!({"agent": "codex", "longLived": "true"}),
        ] {
            assert_eq!(parse_login_payload(&payload), Err(LONG_LIVED_CLAUDE_ONLY.to_string()));
        }
        assert_eq!(
            parse_login_payload(&serde_json::json!({"agent": "claude", "longLived": "yes"})),
            Err(MALFORMED_PAYLOAD.to_string())
        );

        let dir = scratch_dir("long-lived");
        let settings = Settings::default();
        assert_eq!(
            long_lived_plan(&settings, &dir, "0badf00d", true).err(),
            Some(LONG_LIVED_NEEDS_PROFILE.to_string())
        );
        let profile = agent_profiles::create(&dir, CodingAgent::Claude).unwrap();
        let home = agent_profiles::profile_dir(&dir, CodingAgent::Claude, &profile.id).unwrap();
        // EXP-1626: the profile holds a BROWSER login; persisting it must
        // never touch that file.
        let browser = r#"{"claudeAiOauth":{"accessToken":"at-browser","refreshToken":"rt-browser","expiresAt":4092066000000}}"#;
        std::fs::write(home.join(".credentials.json"), browser).unwrap();
        let (staging, plan) = long_lived_plan(&settings, &dir, &profile.id, true).unwrap();
        assert_eq!(plan.spawn.args, vec!["setup-token".to_string()]);
        assert!(plan.spawn.env.contains(&("BROWSER".to_string(), "true".to_string())));
        // EXP-1626: the run's config dir is a fresh STAGING dir, never the
        // profile's.
        assert!(staging.is_dir());
        assert_ne!(staging, home);
        assert!(!staging.starts_with(&home));
        assert!(plan
            .spawn
            .env
            .contains(&("CLAUDE_CONFIG_DIR".to_string(), staging.to_string_lossy().into_owned())));
        assert!(!plan
            .spawn
            .env
            .iter()
            .any(|(_, value)| value == &home.to_string_lossy().into_owned()));

        let screen: Vec<String> = ["Your OAuth token (valid for 1 year):", "", "sk-ant-oat01-secret-123456"]
            .iter()
            .map(|line| line.to_string())
            .collect();
        let scraped = scrape_setup_token(&screen).unwrap();
        let expires = commit_setup_token(&dir, &profile.id, &staging, scraped).unwrap();
        assert!(!expires.contains("secret"));
        assert!(!staging.exists(), "the staging dir is dropped");
        let stored = crate::claude_token::valid(&home).unwrap();
        assert_eq!(stored.token, "sk-ant-oat01-secret-123456");
        assert_eq!(stored.expires_at, expires);
        assert_eq!(
            std::fs::read_to_string(home.join(".credentials.json")).unwrap(),
            browser,
            "the browser login is untouched"
        );
        assert_eq!(agent_profiles::list(&dir, CodingAgent::Claude).len(), 1, "persisted in place");
        assert!(agent_profiles::get(&dir, CodingAgent::Claude, &profile.id)
            .unwrap()
            .last_login_at
            .is_some());

        // A run that printed no token: the staging goes, the profile keeps
        // what it had.
        let (staging, _) = long_lived_plan(&settings, &dir, &profile.id, false).unwrap();
        abandon_login(CodingAgent::Claude, &staging);
        assert!(!staging.exists());
        assert_eq!(crate::claude_token::valid(&home).unwrap().token, "sk-ant-oat01-secret-123456");
        // A failed store still drops the staging dir.
        let (staging, _) = long_lived_plan(&settings, &dir, &profile.id, false).unwrap();
        assert!(commit_setup_token(&dir, "0badf00d", &staging, ("sk-ant-oat01-x".into(), None)).is_err());
        assert!(!staging.exists());

        // A sign-out takes the token with it (the CLI's own logout then runs
        // a binary that does not exist: its refusal is beside the point).
        let missing = Settings {
            claude_path: dir.join("no-claude").to_string_lossy().into_owned(),
            ..Settings::default()
        };
        let _ = sign_out_in(&missing, &dir, CodingAgent::Claude, &profile.id);
        assert_eq!(crate::claude_token::read(&home), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-1600: a long-lived Add runs `claude setup-token` in a fresh
    /// staging dir; its token makes that dir a NEW profile (stamped, no
    /// email), and the staging dir is gone either way.
    #[test]
    fn a_long_lived_add_becomes_a_new_profile_holding_its_token() {
        let dir = scratch_dir("long-lived-add");
        let settings = Settings::default();
        let (staging, plan) = long_lived_add_plan(&settings, &dir, true).unwrap();
        assert!(staging.is_dir());
        assert_eq!(plan.spawn.args, vec!["setup-token".to_string()]);
        assert!(plan.spawn.env.contains(&("BROWSER".to_string(), "true".to_string())));
        assert!(plan
            .spawn
            .env
            .contains(&("CLAUDE_CONFIG_DIR".to_string(), staging.to_string_lossy().into_owned())));
        assert!(agent_profiles::list(&dir, CodingAgent::Claude).is_empty());

        let (id, expires) = commit_setup_token_add(
            &dir,
            &staging,
            ("sk-ant-oat01-added-123456".to_string(), Some(365 * 86_400)),
        )
        .unwrap();
        assert!(!staging.exists());
        let profiles = agent_profiles::list(&dir, CodingAgent::Claude);
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].id, id);
        assert!(profiles[0].last_login_at.is_some());
        let home = agent_profiles::profile_dir(&dir, CodingAgent::Claude, &id).unwrap();
        let stored = crate::claude_token::valid(&home).unwrap();
        assert_eq!(stored.token, "sk-ant-oat01-added-123456");
        assert_eq!(stored.expires_at, expires);
        assert_eq!(agent_profiles::known_email(&dir, CodingAgent::Claude, &id), None);
        // A run that printed no token is abandoned: nothing is added.
        let (staging, _) = long_lived_add_plan(&settings, &dir, false).unwrap();
        abandon_login(CodingAgent::Claude, &staging);
        assert!(!staging.exists());
        assert_eq!(agent_profiles::list(&dir, CodingAgent::Claude).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// EXP-1626: the profile name is gone — an older client's `name` is
    /// ignored, never an error.
    #[test]
    fn an_older_clients_name_is_ignored() {
        for payload in [
            serde_json::json!({"agent": "claude", "longLived": "true", "name": "Work"}),
            serde_json::json!({"agent": "claude", "longLived": "true", "profileId": "0badf00d", "name": "Work"}),
            serde_json::json!({"agent": "claude", "name": 7}),
        ] {
            assert!(parse_login_payload(&payload).is_ok(), "{payload}");
        }
    }

    /// EXP-1626: a run is stopped 10 minutes after it started, unless a
    /// code went in during the last minute (then once that minute passed).
    #[test]
    fn a_login_run_expires_after_ten_minutes_unless_a_code_just_went_in() {
        let min = |m: u64| std::time::Duration::from_secs(m * 60);
        let sec = std::time::Duration::from_secs;
        assert!(!login_run_expired(min(9), None));
        assert!(login_run_expired(min(10), None));
        assert!(login_run_expired(min(30), None));
        assert!(!login_run_expired(min(10), Some(sec(5))));
        assert!(!login_run_expired(min(11), Some(sec(59))));
        assert!(login_run_expired(min(11), Some(sec(60))));
        assert!(login_run_expired(min(10), Some(min(4))));
        assert!(!login_run_expired(min(2), Some(min(4))));
    }

    /// EXP-1626: a cancel stops a live run unless a code went in; no run is
    /// nothing to stop. Every answer is ok.
    #[test]
    fn a_cancel_stops_only_a_live_run_with_no_code_in_it() {
        assert_eq!(CancelDisposition::of(false, false), CancelDisposition::NoRun);
        assert_eq!(CancelDisposition::of(false, true), CancelDisposition::NoRun);
        assert_eq!(CancelDisposition::of(true, true), CancelDisposition::CodeEntered);
        assert_eq!(CancelDisposition::of(true, false), CancelDisposition::Stop);
        assert_eq!(CancelDisposition::Stop.message(), "The sign-in was cancelled.");
        assert_eq!(
            parse_cancel_payload(&serde_json::json!({"agent": "claude", "loginCommandId": " c-1 "})),
            Ok("c-1".to_string())
        );
        for bad in [
            serde_json::json!({"agent": "claude"}),
            serde_json::json!({"loginCommandId": "c-1"}),
            serde_json::json!({"agent": "pi", "loginCommandId": "c-1"}),
        ] {
            assert_eq!(parse_cancel_payload(&bad), Err(MALFORMED_PAYLOAD.to_string()));
        }
    }

    /// EXP-1626: a long-lived run's code completes on its OUTCOME: the
    /// token → `Signing in…`; setup-token's refusal after the code → that
    /// sentence; the run ending with neither → failed. A refusal already on
    /// screen when the code went in counts only once the screen moved on.
    #[test]
    fn a_long_lived_code_completes_on_its_outcome() {
        let refused = "That code wasn't accepted.";
        let mut code = PendingCode::new("code-1", false);
        assert_eq!(code.command_id, "code-1");
        assert_eq!(code.observe(false, None), None, "still waiting");
        assert_eq!(code.observe(true, None), Some((true, SIGNING_IN.to_string())));

        let mut code = PendingCode::new("code-2", false);
        assert_eq!(code.observe(false, Some(refused)), Some((false, refused.to_string())));

        let code = PendingCode::new("code-3", false);
        assert_eq!(code.ended(false), (false, SIGN_IN_ENDED.to_string()));
        assert_eq!(code.ended(true), (true, SIGNING_IN.to_string()));

        // A retry typed over the previous refusal.
        let mut code = PendingCode::new("code-4", true);
        assert_eq!(code.observe(false, Some(refused)), None, "the OLD refusal");
        assert_eq!(code.observe(false, None), None);
        assert_eq!(code.observe(false, Some(refused)), Some((false, refused.to_string())));
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
