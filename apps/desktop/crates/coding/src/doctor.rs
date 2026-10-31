//! The tooling doctor (masterplan-v3 §7.7, EXP-201): runs `--version` on
//! every agent CLI (`claude`, `codex` — each at its configured/probed
//! path) and on `git`, capturing success + version string or the spawn error.
//!
//! Gating is per-agent (EXP-201): **git is required for every launch**, but a
//! missing optional agent only blocks launches that SELECT it —
//! [`DoctorReport::first_failure_for`] is the launcher's step-0 gate, and a
//! machine without codex installed still codes with claude. The
//! Start-coding affordance itself only needs git + at least one usable agent
//! ([`DoctorReport::any_agent_ok`]); the dialog names the selected agent's
//! failure. Errors stay actionable per the spec copy: "claude not found on
//! PATH — set an absolute path" / "git not found on PATH".
//!
//! A resolvable Claude that is OLDER than [`MIN_CLAUDE_VERSION`] also fails
//! its check (with "run: claude update" copy) — one version gate replaces
//! the old per-flag `--help` probe and its whole degradation matrix. Codex
//! has NO minimum version yet (presence-only, deliberately lenient).
//!
//! EXP-409: an installed agent that is SIGNED OUT fails its check too —
//! installed-but-not-signed-in equals not installed for every gate, because
//! a logged-out agent spawns onto its login prompt and a headless/remote
//! session hangs there invisibly. Probes: `claude auth status` (local JSON,
//! `{"loggedIn": bool}`) and `codex login status` (exit 0 / "Not logged
//! in"). Every probe FAILS OPEN: an
//! unrecognisable answer (older CLI, changed output) leaves the check green
//! rather than falsely bricking a working install.
//!
//! [`DoctorReport::installed_agents`] is the steer presence input (EXP-201):
//! the device advertises which agent CLIs it can actually run, so remote
//! Start-coding pickers only offer those.
//!
//! Every probe runs with the terminal layer's augmented login `PATH`
//! ([`terminal::pty::login_path`], §6.12) — the SAME environment the engine
//! spawns the agent into. A `.app`/`.desktop` launch carries a minimal PATH
//! without Homebrew/npm-global, so probing with the process PATH reported
//! codex as "not found" on machines where every launch worked (EXP-206).
//!
//! Blocking `std::process` calls — callers run this off the foreground
//! executor (settings "Check tools" button, onboarding, launch step 0).

use crate::agent::CodingAgent;
use crate::agent_accounts::{now_iso, AgentAccount, AgentAccounts, Importable};
use crate::settings::Settings;
#[cfg(test)]
use std::time::Duration;
use std::collections::BTreeMap;
use std::path::Path;

use eagent::detect as rt;
pub use eagent::detect::{
    is_signed_in, parse_claude_auth_status, parse_claude_auth_status_full, parse_claude_version,
    parse_codex_login_status, parse_codex_version, parse_version_output, probe_codex_acp,
    ClaudeAuthStatus, Tool, MIN_CLAUDE_ACP_VERSION, MIN_CLAUDE_VERSION, MIN_CODEX_ACP_VERSION,
};
// EXP-1426: the probes moved to `eagent::detect`; the
// crate-internal callers keep these paths.
pub(crate) use eagent::detect::{
    output_with_stdin_timeout, output_with_timeout, probe_claude_auth_status_in,
    probe_codex_auth_in, PROBE_TIMEOUT,
};


/// EXP-746 (D9): the BUILD capabilities every host advertises to
/// `devices.register`, whatever it can currently run. ONE list — the desktop
/// (`ui::steer_wiring`) and the CLI daemon both call [`device_caps`], which
/// is what makes a new cap reach both hosts (they used to be hand-synced
/// copies, and a one-sided edit silently made one host un-targetable).
///
/// EXP-1292: only caps the server or a client still READS. A start is a
/// `queued` sessions row, never a relay frame, so the start-frame caps
/// (`agent-start`, `start-prompt`, `steer-files`, `action-inputs`,
/// `actions`), the EXP-481 admin trio (`resume`/`worktrees`/
/// `launch-defaults`) and `stale-end` (no staleness sweep ends a row) went.
///
/// - `agent-login` (EXP-484) — signing IN is exactly what a machine with no
///   runnable agent needs, so it is a build cap.
/// - `acp` (EXP-746) — this build speaks the ACP engine and steering v2:
///   `set_config`/`set_mode` frames, `config_state`/`usage` kinds.
/// - `account-switch` (EXP-849) — this build moves a session (or the
///   machine's default login) to another account's profile; clients offer
///   the switch only with it (beside [`RESUME_RUN_CAP`] for a live run).
/// - `account-remove` (EXP-862) — this build runs `agent_profile_remove`:
///   delete ONE account's login from this machine (its profile dir and its
///   index row), never the account itself. The server refuses to queue it
///   without the cap.
/// - `account-sign-out` (EXP-1137) — this build runs `agent_profile_sign_out`
///   (sign one login out here, keep its row). Same reasoning.
/// - `agent-import` — this build runs `agent_login {agent, import: "true"}`:
///   move the agent's AMBIENT login into an account profile (commit by
///   email). Clients offer the remote Import pill only with it.
/// - `agent-usage-refresh` (EXP-792) — this build runs
///   `agent_usage_refresh` (a forced usage re-read, 429 floor kept).
/// - `update-now` (FEED-36) — this build runs `update_now`: ends every live
///   session and applies a queued self-update right away (the CLI daemon;
///   the desktop advertises it too but updates through its own updater).
///
/// Ceiling check: `devices.register`'s caps input accepts 24 caps
/// (`apps/web/src/lib/trpc/devices.ts`); this is 8 + 2 = 10.
pub const DEVICE_CAPS: [&str; 8] = [
    "agent-login",
    "acp",
    ACCOUNT_SWITCH_CAP,
    ACCOUNT_REMOVE_CAP,
    ACCOUNT_SIGN_OUT_CAP,
    AGENT_IMPORT_CAP,
    "agent-usage-refresh",
    "update-now",
];

/// EXP-849's account-switch cap, by name: the ONE place the literal lives, so
/// a client deciding whether a machine can move a live run (or its default
/// login) to another account never repeats the string.
pub const ACCOUNT_SWITCH_CAP: &str = "account-switch";

/// EXP-862's account-remove cap, by name: the ONE place the literal lives, so
/// a client deciding whether a machine can delete one of its logins never
/// repeats the string.
pub const ACCOUNT_REMOVE_CAP: &str = "account-remove";

/// EXP-1137's account-sign-out cap, by name: the ONE place the literal lives,
/// so a client deciding whether a machine can sign one of its logins out
/// never repeats the string.
pub const ACCOUNT_SIGN_OUT_CAP: &str = "account-sign-out";

/// The ambient-login import cap, by name (see [`DEVICE_CAPS`]).
pub const AGENT_IMPORT_CAP: &str = "agent-import";

/// The RUN capabilities — advertised only while at least one agent is
/// RUNNABLE (EXP-409: a machine whose only agents are signed out cannot run
/// a session either).
///
/// - `resume-run` (EXP-637) — this build resumes a session natively from its
///   local run record (EXP-1292: a message to an engineless session wakes
///   the SAME row); clients gate their resume affordances on it.
/// - `computer-use-run` (EXP-1249) — this build honours a session's
///   `computer_use` picker; clients hide the toggle for a machine without
///   it, which would silently fall back to its own switch.
pub const RUN_CAPS: [&str; 2] = [RESUME_RUN_CAP, COMPUTER_USE_RUN_CAP];

/// EXP-1249's per-run computer-use cap, by name: the contract's
/// `codingSession.computerUseCap`.
pub const COMPUTER_USE_RUN_CAP: &str = domain::contract::CODING_SESSION_COMPUTER_USE_CAP;

/// EXP-637's resume cap, by name: the ONE place the literal lives, so a
/// client deciding whether another machine can take a resume (EXP-800) never
/// repeats the string.
pub const RESUME_RUN_CAP: &str = "resume-run";

/// The caps to advertise for a doctor snapshot: the build caps always, plus
/// the run caps while anything is runnable.
pub fn device_caps(advertised: &AgentAdvertisement) -> Vec<String> {
    let mut caps: Vec<String> = DEVICE_CAPS.iter().map(|cap| cap.to_string()).collect();
    if !advertised.agents.is_empty() {
        caps.extend(RUN_CAPS.iter().map(|cap| cap.to_string()));
    }
    caps
}

/// Exponential's sign-in copy for a [`Tool`] row (the product's settings
/// and accounts, so it stays here when `Tool` moved to the runtime).
pub trait ToolMessages {
    /// The EXP-409 red actionable message for an installed-but-signed-out
    /// agent (never produced for git). EXP-792 (A3): the fix is a button in
    /// the product, never a terminal command the person has to type.
    fn signed_out_message(self) -> &'static str;

    /// A launch with NO profile to run on: no ambient login is ever used, so
    /// the fix is a sign-in on this device.
    fn no_profile_message(agent: CodingAgent) -> String
    where
        Self: Sized;

    /// EXP-1138: a launch on a NAMED account profile that is signed out,
    /// named by the address it last signed in as.
    fn profile_signed_out_message(self, email: Option<&str>) -> String;
}

impl ToolMessages for Tool {
    fn signed_out_message(self) -> &'static str {
        match self {
            Tool::Claude => "claude is installed but not signed in. Sign in from Settings → Agents, or from the Sign in button on the failed start.",
            Tool::Codex => "Codex is not signed in on this machine. Sign in from Settings → Agents, or from the Sign in button on the failed start.",
            Tool::Git => "",
        }
    }

    fn no_profile_message(agent: CodingAgent) -> String {
        format!("Sign in to {} on this device first.", agent.label())
    }

    fn profile_signed_out_message(self, email: Option<&str>) -> String {
        match email {
            Some(email) => format!(
                "{self} account {email} is signed out on this machine. Sign in from Settings → Agents, or pick another account."
            ),
            None => format!(
                "That {self} account is signed out on this machine. Sign in from Settings → Agents, or pick another account."
            ),
        }
    }
}


/// One doctor row (§7.7): green check + version, or a red actionable error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCheck {
    pub tool: Tool,
    pub ok: bool,
    pub version: Option<String>,
    pub error: Option<String>,
    /// EXP-409 sign-in state over the account PROFILES (no ambient login is
    /// ever used): `Some(true)` = some profile is signed in, `Some(false)` =
    /// none is (or there is none) — the check is then also `!ok`, with
    /// [`Tool::signed_out_message`] as the error but `version` kept, so UIs
    /// distinguish "sign in" from "install". `None` = not applicable (git) or
    /// unknown (a profile probe answered unreadably: fail open).
    pub authed: Option<bool>,
    /// EXP-484: WHO the LAST USED profile is signed in as (claude's `auth
    /// status` JSON, codex's presence-only answer, enriched from the usage
    /// cache by [`crate::agent_usage::collect_if_due`]); a signed-out row
    /// when there is no profile. `None` for git and for a check that never
    /// reached its auth probe.
    pub account: Option<AgentAccount>,
    /// EXP-484: whether the last used profile's usage windows may be fetched
    /// at all — [`ClaudeAuthStatus::usage_eligible`] for claude, signed in for
    /// codex (it answers over its app-server).
    pub usage_eligible: bool,
    /// EXP-746: whether this agent can run on the ACP engine.
    /// **Non-fatal for the doctor** — it never touches `ok`,
    /// [`DoctorReport::any_agent_ok`] or [`DoctorReport::first_failure_for`]
    /// — but it IS the coding gate (EXP-773): anything but `Some(true)`
    /// refuses a launch with `acp_note`, there being no terminal path to
    /// fall back to. `None` = not applicable (git) or never probed.
    pub acp: Option<bool>,
    /// Why `acp` is not `Some(true)` — one short line, rendered under the
    /// agent's doctor row ("not supported (…)").
    pub acp_note: Option<String>,
    /// EXP-1138: the first account profile of this agent that probed signed
    /// in (the last used login first). `None` when none did.
    pub signed_in_profile: Option<String>,
    /// The agent's AMBIENT login (`~/.claude` / `~/.codex`, no config-dir
    /// override) while it is signed in — never run on, only offered for
    /// IMPORT (`AgentAccount::importable`, the doctor item's `import`).
    pub importable: Option<Importable>,
    /// Every account profile's sign-in probe from this check, the last used
    /// first — what the heartbeat's `profiles[]` rows and the usage
    /// collector's eligibility read, so a beat spawns no probe of its own.
    pub profiles: Vec<ProfileProbe>,
}

/// One account profile's answer inside [`ToolCheck::profiles`]. An
/// unreadable probe reads as signed OUT here (a profile is explicit); the
/// gate itself fails open on it ([`ToolCheck::authed`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileProbe {
    pub id: String,
    pub account: AgentAccount,
    /// Whether this login's usage windows may be fetched at all.
    pub usage_eligible: bool,
}

impl ToolCheck {
    /// Whether NO profile of this agent is signed in on this machine — the
    /// one failure whose fix is a login, not an install.
    pub fn signed_out(&self) -> bool {
        self.authed == Some(false)
    }
}

/// `{ claude, codex, git }` — the §7.7 report, one row per agent CLI plus
/// git (EXP-201; the old two-row `agent`/`git` shape is gone).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoctorReport {
    pub claude: ToolCheck,
    pub codex: ToolCheck,
    pub git: ToolCheck,
}

impl DoctorReport {
    /// The check backing `agent`.
    pub fn check_for(&self, agent: CodingAgent) -> &ToolCheck {
        match agent {
            CodingAgent::Claude => &self.claude,
            CodingAgent::Codex => &self.codex,
        }
    }

    /// EXP-1137: re-run ONE agent's check (`check_agent`, exactly what
    /// [`run_doctor`] would build for it) — after a sign-out, a sign-in
    /// commit or an import, this report's row is stale and would keep naming
    /// the logins as they were.
    pub fn reprobe_agent(&mut self, settings: &Settings, data_dir: &Path, agent: CodingAgent) {
        let check = check_agent(settings, data_dir, agent);
        match agent {
            CodingAgent::Claude => self.claude = check,
            CodingAgent::Codex => self.codex = check,
        }
    }

    /// The launcher's step-0 gate for a launch selecting `agent`: git AND
    /// that agent must resolve — a missing OTHER agent never blocks.
    pub fn first_failure_for(&self, agent: CodingAgent) -> Option<&ToolCheck> {
        [self.check_for(agent), &self.git]
            .into_iter()
            .find(|check| !check.ok)
    }

    /// Whether ANY agent CLI is usable (the Start-coding affordance's gate
    /// half — the dialog names the selected agent's failure itself).
    pub fn any_agent_ok(&self) -> bool {
        CodingAgent::ALL
            .into_iter()
            .any(|agent| self.check_for(agent).ok)
    }

    /// The agents this machine can actually launch — the steer presence
    /// advertisement (EXP-201). A too-old claude is NOT usable (its argv
    /// would carry flags the CLI rejects), so it drops out here too — as
    /// does an agent with no signed-in profile (EXP-409).
    pub fn installed_agents(&self) -> Vec<CodingAgent> {
        CodingAgent::ALL
            .into_iter()
            .filter(|agent| self.check_for(*agent).ok)
            .collect()
    }

    /// The agents that are installed but have NO signed-in login (EXP-409) —
    /// advertised separately so remote UIs can say "sign in on that machine"
    /// instead of pretending the binary is missing.
    pub fn unauthed_agents(&self) -> Vec<CodingAgent> {
        CodingAgent::ALL
            .into_iter()
            .filter(|agent| self.check_for(*agent).signed_out())
            .collect()
    }

    /// EXP-1138: the launch gate's ACCOUNT half — `None` when the LOGIN a
    /// launch of `agent` on `account` would spend is usable, else a red copy
    /// of the agent's check naming what to do. Runs AFTER the agent's own
    /// check passed (install, version, any login). A launch with no profile
    /// (`None`, the retired `system`, a stale id) is refused outright: no
    /// ambient login is ever spent ([`Tool::no_profile_message`]). Only a
    /// PROVABLE sign-out of a profile refuses — an unreadable probe fails
    /// open.
    pub fn account_failure(
        &self,
        settings: &Settings,
        data_dir: &Path,
        agent: CodingAgent,
        account: Option<&str>,
    ) -> Option<ToolCheck> {
        self.account_failure_with_path(
            settings,
            data_dir,
            agent,
            account,
            &terminal::pty::login_path(),
        )
    }

    /// [`Self::account_failure`] with the PATH injected (tests).
    pub(crate) fn account_failure_with_path(
        &self,
        settings: &Settings,
        data_dir: &Path,
        agent: CodingAgent,
        account: Option<&str>,
        path_env: &str,
    ) -> Option<ToolCheck> {
        let check = self.check_for(agent);
        if !check.ok {
            return None;
        }
        let Some(dir) = crate::agent_profiles::account_dir(data_dir, Some(agent), account) else {
            return Some(ToolCheck {
                ok: false,
                error: Some(Tool::no_profile_message(agent)),
                authed: Some(false),
                signed_in_profile: None,
                ..check.clone()
            });
        };
        let id = account.map(str::trim).unwrap_or_default();
        if check.signed_in_profile.as_deref() == Some(id) {
            return None;
        }
        let program = settings.resolved_path_for(agent);
        if probe_profile_signed_in(agent, &program, path_env, &dir) != Some(false) {
            return None;
        }
        let email = crate::agent_profiles::known_email(data_dir, agent, id);
        Some(ToolCheck {
            ok: false,
            error: Some(check.tool.profile_signed_out_message(email.as_deref())),
            authed: Some(false),
            signed_in_profile: None,
            ..check.clone()
        })
    }

    /// The steer presence advertisement as wire ids — the ONE shape both
    /// producers (desktop control channel + CLI daemon) send and compare for
    /// re-advertise change detection, so an uninstall of a signed-out agent
    /// re-dials just like an install does — and, since the launch defaults
    /// ride along (EXP-437), so does a Settings → Agents edit.
    pub fn agent_advertisement(&self, settings: &Settings) -> AgentAdvertisement {
        AgentAdvertisement {
            agents: self
                .installed_agents()
                .into_iter()
                .map(|agent| agent.id().to_string())
                .collect(),
            unauthed_agents: self
                .unauthed_agents()
                .into_iter()
                .map(|agent| agent.id().to_string())
                .collect(),
            default_agent: settings.default_agent.id().to_string(),
            acp_agents: self
                .installed_agents()
                .into_iter()
                .filter(|agent| self.check_for(*agent).acp == Some(true))
                .map(|agent| agent.id().to_string())
                .collect(),
            launch_defaults: self
                .installed_agents()
                .into_iter()
                .map(|agent| {
                    let options = crate::argv::LaunchOptions::defaults_for(settings, agent);
                    (
                        agent.id().to_string(),
                        AgentLaunchDefaults {
                            model: options.model,
                            effort: options.effort,
                            ultracode: options.ultracode,
                            plan_mode: options.plan_mode,
                        },
                    )
                })
                .collect(),
        }
    }

    /// EXP-484: who is signed in per agent, as the `devices.agentAccounts`
    /// wire map. One entry per INSTALLED agent (a probe that never resolved
    /// a binary has nothing to say, and the clients omit the row entirely);
    /// a signed-out install is present with `signedIn: false`. `now` is the
    /// ISO stamp every entry gets — deliberately an argument, so one
    /// collection pass stamps one instant.
    ///
    /// Deliberately NOT part of [`AgentAdvertisement`]: `checked_at` moves
    /// on every probe, and the advertisement is the anti-flap key for the
    /// relay re-dial.
    pub fn agent_accounts(&self, now: &str) -> AgentAccounts {
        let mut accounts = AgentAccounts::new();
        for agent in CodingAgent::ALL {
            let check = self.check_for(agent);
            if let Some(account) = &check.account {
                let mut account = account.clone();
                account.checked_at = now.to_string();
                account.importable = check.importable.clone();
                accounts.insert(agent.id().to_string(), account);
            }
        }
        self.stamp_versions(&mut accounts);
        accounts
    }

    /// Write each installed agent's CLI version onto its top-level account
    /// row (`AgentAccount::version`). Runs LAST on every path that builds the
    /// heartbeat map — the collector copies identity fields one by one and
    /// a probe's row never carries a version, so a stamp taken earlier would
    /// be lost. An agent the doctor could not run keeps no version.
    pub fn stamp_versions(&self, accounts: &mut AgentAccounts) {
        for agent in CodingAgent::ALL {
            if let Some(account) = accounts.get_mut(agent.id()) {
                account.version = self.check_for(agent).version.clone();
            }
        }
    }

    /// EXP-792 (EXP-747 B3): [`DoctorReport::agent_accounts`] plus the
    /// device's ACCOUNT PROFILES — one probe per profile dir, so a machine
    /// with two claude logins reports both. `profiles` = exactly the profile
    /// dirs (absent when there are none; the ambient login is never a row).
    ///
    /// The top-level fields keep naming the ACTIVE profile, which is the
    /// login a run without an explicit account lands on, and read signed out
    /// when there is no profile.
    pub fn agent_accounts_with_profiles(
        &self,
        settings: &Settings,
        data_dir: &Path,
        now: &str,
    ) -> AgentAccounts {
        self.agent_accounts_detailed(settings, data_dir, now).accounts
    }

    /// EXP-808: [`Self::agent_accounts_with_profiles`] plus the one thing
    /// the wire rows cannot carry — whether each LOGIN's usage windows may
    /// be fetched at all ([`ProfileAccounts::usage_eligible`]).
    ///
    /// Both are by-products of the sign-in probe the doctor already ran per
    /// profile dir ([`ToolCheck::profiles`]), so a beat spawns no `auth
    /// status` of its own; only a profile the doctor never saw (created
    /// since its last run) is probed here.
    pub fn agent_accounts_detailed(
        &self,
        settings: &Settings,
        data_dir: &Path,
        now: &str,
    ) -> ProfileAccounts {
        let mut accounts = self.agent_accounts(now);
        let mut usage_eligible = BTreeMap::new();
        for agent in CodingAgent::ALL {
            let Some(base) = accounts.get(agent.id()).cloned() else {
                continue;
            };
            if crate::agent_profiles::config_env_var(agent).is_none() {
                continue;
            }
            let profiles = crate::agent_profiles::list(data_dir, agent);
            if profiles.is_empty() {
                continue;
            }
            let active = crate::agent_profiles::active_profile(data_dir, agent);
            let check = self.check_for(agent);
            let program = settings.path_for(agent);
            let path_env = terminal::pty::login_path();
            let mut rows = Vec::new();
            for profile in &profiles {
                // The doctor's own probe of this profile; one it never saw
                // (created since) gets its own `auth status` now.
                let (account, eligible) = match check
                    .profiles
                    .iter()
                    .find(|probe| probe.id == profile.id)
                {
                    Some(probe) => (probe.account.clone(), probe.usage_eligible),
                    None => match crate::agent_profiles::profile_dir(data_dir, agent, &profile.id) {
                        Some(dir) => {
                            let (_, probe) = probe_profile_full(agent, program, &path_env, &dir, now);
                            (probe.account, probe.usage_eligible)
                        }
                        None => (
                            AgentAccount {
                                signed_in: false,
                                checked_at: now.to_string(),
                                ..AgentAccount::default()
                            },
                            false,
                        ),
                    },
                };
                usage_eligible.insert(
                    crate::usage_cache::entry_key(agent.id(), &profile.id),
                    eligible,
                );
                rows.push(crate::agent_accounts::AgentProfileEntry {
                    id: profile.id.clone(),
                    signed_in: account.signed_in,
                    email: account.email.clone(),
                    plan: account.plan.clone(),
                    active: Some(&profile.id) == active.as_ref(),
                    checked_at: now.to_string(),
                    last_login_at: profile.last_login_at.clone(),
                    // EXP-849: `auth status` answers IDENTITY only. Health is
                    // the usage probe's verdict and the collector stamps it
                    // (`agent_usage::apply_health`); a login this probe names
                    // may still be revoked server-side.
                    ..crate::agent_accounts::AgentProfileEntry::default()
                });
            }
            // The top-level fields follow the ACTIVE profile so a client that
            // reads only them names the login a default run uses.
            let mut account = base;
            if let Some(row) = rows.iter().find(|row| row.active) {
                account.signed_in = row.signed_in;
                account.email = row.email.clone();
                account.plan = row.plan.clone();
            }
            account.profiles = rows;
            accounts.insert(agent.id().to_string(), account);
        }
        // EXP-1013: a signed-out login still names its last email.
        crate::agent_profiles::remember_emails(data_dir, &mut accounts);
        // The rebuilt top-level rows above start from the probe's identity,
        // which names no version — stamp last.
        self.stamp_versions(&mut accounts);
        ProfileAccounts {
            accounts,
            usage_eligible,
        }
    }
}

/// EXP-808 — [`DoctorReport::agent_accounts_detailed`]'s answer: the wire
/// map plus the per-LOGIN usage eligibility that never rides the wire.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProfileAccounts {
    pub accounts: AgentAccounts,
    /// `usage_cache::entry_key(agent, profile)` → may this login's usage
    /// windows be fetched at all. One entry per profile; a login absent here
    /// is never read.
    pub usage_eligible: BTreeMap<String, bool>,
}

/// What a device tells the relay + registry about its agent CLIs (EXP-409):
/// `agents` = runnable (installed AND signed in), `unauthed_agents` =
/// installed but signed out (unusable; listed so UIs can say "sign in").
/// EXP-437 adds the machine's per-agent launch defaults so remote
/// Start-coding dialogs pre-fill this device's configuration.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentAdvertisement {
    pub agents: Vec<String>,
    pub unauthed_agents: Vec<String>,
    /// `settings.default_agent` as a wire id — remote pickers preselect it
    /// (clamped to `agents` client-side; it may name an uninstalled agent).
    pub default_agent: String,
    /// One entry per RUNNABLE agent, values capability-masked via
    /// [`crate::argv::LaunchOptions::defaults_for`] — the same resolver the
    /// local Start-coding dialog seeds from. `BTreeMap` for deterministic
    /// wire serialization (the steer frames are byte-locked in tests).
    pub launch_defaults: BTreeMap<String, AgentLaunchDefaults>,
    /// EXP-746: the runnable agents that also speak ACP ([`ToolCheck::acp`]).
    /// EXP-749 puts it on the `devices.register` payload and the synced
    /// `devices.acp_agents` column, so remote pickers can SAY which agents
    /// can actually run a session on that machine (EXP-773: an agent that is
    /// not ACP-ready cannot start one at all). An empty list is a real answer
    /// ("none of them"); only a NULL column — an older build's row — means
    /// "unknown, assume all".
    pub acp_agents: Vec<String>,
}

/// One agent's launch defaults on this machine (EXP-437): mirrors
/// [`crate::argv::LaunchOptions`] minus the agent tag. Blank `model`/`effort`
/// = "CLI default / omit the flag" (valid per the agent's vocabulary).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentLaunchDefaults {
    pub model: String,
    pub effort: String,
    pub ultracode: bool,
    pub plan_mode: bool,
}

impl AgentAdvertisement {
    /// Nothing installed at all — the device stays fully offline for remote
    /// start (EXP-367). With only signed-out agents it still dials, so the
    /// machine list can explain itself.
    pub fn nothing_installed(&self) -> bool {
        self.agents.is_empty() && self.unauthed_agents.is_empty()
    }
}

/// Run every check: each agent's resolved program
/// ([`Settings::resolved_path_for`]) — claude version-gated against
/// [`MIN_CLAUDE_VERSION`], every agent sign-in-gated (EXP-409) — and plain
/// `git` from PATH.
///
///
/// EXP-849: every check here is a millisecond `--version`/credential probe —
/// the only real handshake left is [`probe_codex_acp`], which `exponential
/// doctor` runs on demand.
///
/// `data_dir` holds the account profiles: an agent's sign-in state is its
/// PROFILES' ([`apply_auth_gate`]); its ambient login only feeds
/// [`ToolCheck::importable`].
pub fn run_doctor(settings: &Settings, data_dir: &Path) -> DoctorReport {
    // EXP-419: a Windows installer edits the registry PATH, which a running
    // process never sees — re-read it so "Check tools" (and every later
    // spawn) finds a just-installed git/agent without an app restart.
    terminal::process::refresh_windows_path();
    DoctorReport {
        claude: check_agent(settings, data_dir, CodingAgent::Claude),
        codex: check_agent(settings, data_dir, CodingAgent::Codex),
        git: check_tool(Tool::Git, "git"),
    }
}

/// ONE agent's row of [`run_doctor`]: the tool check plus its gates, so a
/// re-probe of a single agent (EXP-1137: after a sign-out) reads exactly what
/// the full doctor would.
fn check_agent(settings: &Settings, data_dir: &Path, agent: CodingAgent) -> ToolCheck {
    let program = settings.resolved_path_for(agent);
    let path_env = terminal::pty::login_path();
    match agent {
        CodingAgent::Claude => {
            let mut claude = check_tool(Tool::Claude, &program);
            apply_version_gate(&mut claude);
            apply_auth_gate(&mut claude, &program, &path_env, data_dir);
            claude
        }
        CodingAgent::Codex => {
            // EXP-1232: the managed build is probed only once it is in
            // place; until then the row says what the fetch is doing.
            if settings.codex_is_managed() {
                let state = crate::managed_codex::state(data_dir);
                if state != crate::managed_codex::State::Installed {
                    return managed_codex_check(state);
                }
            }
            let mut codex = check_tool(Tool::Codex, &program);
            // Before the ACP readiness, which reads `ok`.
            apply_auth_gate(&mut codex, &program, &path_env, data_dir);
            apply_codex_acp(&mut codex);
            codex
        }
    }
}

/// EXP-1232: the codex row while the managed build is NOT in place. No
/// login and no binary reads as signed out (the fix is the sign-in, which
/// fetches); a running fetch and a failed one carry their own messages,
/// which `device_doctor` keys its rows off and the launch gate shows.
pub(crate) fn managed_codex_check(state: crate::managed_codex::State) -> ToolCheck {
    use crate::managed_codex::{State, DOWNLOADING_MESSAGE, DOWNLOAD_FAILED_PREFIX};
    let (error, authed) = match state {
        State::Installed | State::Downloading => (DOWNLOADING_MESSAGE.to_string(), None),
        State::Failed(reason) => (format!("{DOWNLOAD_FAILED_PREFIX} {reason}"), None),
        State::NotWanted => (Tool::Codex.signed_out_message().to_string(), Some(false)),
    };
    ToolCheck {
        tool: Tool::Codex,
        ok: false,
        version: None,
        error: Some(error.clone()),
        authed,
        account: None,
        usage_eligible: false,
        acp: Some(false),
        acp_note: Some(error),
        signed_in_profile: None,
        importable: None,
        profiles: Vec::new(),
    }
}

/// Flip a GREEN claude check red when its version parses BELOW
/// [`MIN_CLAUDE_VERSION`], and stamp the SEPARATE, non-fatal ACP readiness
/// (EXP-746) from [`MIN_CLAUDE_ACP_VERSION`]. An unparseable version stays
/// green — never falsely block a nonstandard build — and its ACP readiness
/// stays unknown (`None`), which refuses a coding launch with the note.
fn apply_version_gate(check: &mut ToolCheck) {
    if !check.ok {
        return;
    }
    let Some(version) = check.version.as_deref().and_then(parse_claude_version) else {
        return;
    };
    let (major, minor, patch) = version;
    let acp_ready = version >= MIN_CLAUDE_ACP_VERSION;
    check.acp = Some(acp_ready);
    if !acp_ready {
        let (acp_major, acp_minor, acp_patch) = MIN_CLAUDE_ACP_VERSION;
        check.acp_note = Some(format!(
            "Claude Code {major}.{minor}.{patch} has no ACP control protocol. \
Update to {acp_major}.{acp_minor}.{acp_patch}+ to run coding sessions."
        ));
    }
    if version < MIN_CLAUDE_VERSION {
        let (min_major, min_minor, min_patch) = MIN_CLAUDE_VERSION;
        check.ok = false;
        check.error = Some(format!(
            "Claude Code {major}.{minor}.{patch} is too old. Update to \
{min_major}.{min_minor}.{min_patch}+ (run: claude update)."
        ));
    }
}

/// EXP-746: codex's ACP readiness. `codex app-server` is what the adapter
/// drives, so a resolved, signed-in codex reads as ready HERE, without a
/// probe: [`run_doctor`] runs on the launch path (step 0) and inline in the
/// daemon every 5 minutes, and a 10-second app-server handshake on either
/// would be paid on every launch. [`probe_codex_acp`] is the deep check for
/// `exponential doctor`, and the engine's own `initialize` is the
/// authoritative one.
///
/// EXP-758: with a VERSION FLOOR ([`MIN_CODEX_ACP_VERSION`]) — claude has one
/// too, so codex was the one agent an unusably old build of
/// which still resolved to the engine and then died in the handshake. Like
/// claude's, the floor never reddens the doctor row, and an unparseable
/// version stays ready: never falsely demote a nonstandard build.
/// EXP-773: a launch below the floor is REFUSED with the note (there is no
/// terminal path left to fall back to).
fn apply_codex_acp(check: &mut ToolCheck) {
    check.acp = Some(check.ok);
    if !check.ok {
        check.acp_note = Some("codex is not available".to_string());
        return;
    }
    let Some(version) = check.version.as_deref().and_then(parse_codex_version) else {
        return;
    };
    if version >= MIN_CODEX_ACP_VERSION {
        return;
    }
    let (major, minor, patch) = version;
    let (min_major, min_minor, min_patch) = MIN_CODEX_ACP_VERSION;
    check.acp = Some(false);
    check.acp_note = Some(format!(
        "Codex {major}.{minor}.{patch} has no app-server this build can drive. \
Update to {min_major}.{min_minor}.{min_patch}+ to run coding sessions."
    ));
}


/// EXP-409, over the account PROFILES: stamp `authed`, the last used
/// profile's identity and the first signed-in profile on a still-green agent
/// check, and flip it red when no profile is signed in (or there is none).
/// Runs AFTER the version gate (an old claude may predate `auth status`);
/// skips already-failed checks and git. The AMBIENT login is probed too, but
/// only to say whether there is one to import ([`ToolCheck::importable`]).
fn apply_auth_gate(check: &mut ToolCheck, program: &str, path_env: &str, data_dir: &Path) {
    let Some(agent) = check.tool.agent() else {
        return;
    };
    if !check.ok {
        return;
    }
    let now = now_iso();
    check.importable = probe_ambient_importable(agent, program, path_env);
    let active = crate::agent_profiles::active_profile(data_dir, agent);
    let mut ids: Vec<String> = active.clone().into_iter().collect();
    ids.extend(
        crate::agent_profiles::list(data_dir, agent)
            .into_iter()
            .map(|profile| profile.id)
            .filter(|id| Some(id) != active.as_ref()),
    );
    let mut any_unknown = false;
    let mut signed_in_profile = None;
    let mut active_probe: Option<(AgentAccount, bool)> = None;
    let mut probes = Vec::new();
    for id in &ids {
        let Some(dir) = crate::agent_profiles::profile_dir(data_dir, agent, id) else {
            continue;
        };
        let (raw, probe) = probe_profile_full(agent, program, path_env, &dir, &now);
        match raw {
            Some(true) if signed_in_profile.is_none() => signed_in_profile = Some(id.clone()),
            None => any_unknown = true,
            _ => {}
        }
        if Some(id) == active.as_ref() {
            active_probe = Some((probe.account.clone(), probe.usage_eligible));
        }
        probes.push(ProfileProbe {
            id: id.clone(),
            account: probe.account,
            usage_eligible: probe.usage_eligible,
        });
    }
    check.profiles = probes;
    let (account, usage_eligible) = active_probe.unwrap_or_else(|| {
        (
            AgentAccount {
                signed_in: false,
                checked_at: now.clone(),
                ..AgentAccount::default()
            },
            false,
        )
    });
    check.account = Some(account);
    check.usage_eligible = usage_eligible;
    // Fail open on an unreadable answer, like every probe here: only a
    // provable "nobody is signed in" turns the row red.
    check.authed = if signed_in_profile.is_some() {
        Some(true)
    } else if any_unknown {
        None
    } else {
        Some(false)
    };
    check.signed_in_profile = signed_in_profile;
    if check.authed == Some(false) {
        check.ok = false;
        check.error = Some(check.tool.signed_out_message().to_string());
    }
}

/// The AMBIENT login of `agent` (no config-dir override), when it is signed
/// in: who it is, for the import offer. claude: `auth status`. codex: `login
/// status` (an unreadable answer counts when a credential file is there),
/// named by its `auth.json` id token.
fn probe_ambient_importable(agent: CodingAgent, program: &str, path_env: &str) -> Option<Importable> {
    match agent {
        CodingAgent::Claude => {
            let status = probe_claude_auth_status_in(program, path_env, None)?;
            status.logged_in.then(|| Importable {
                email: status.email.clone(),
                plan: status.subscription_type.clone(),
            })
        }
        CodingAgent::Codex => {
            let home = crate::codex_trust::codex_home(None)?;
            let auth = home.join(crate::agent_login::CODEX_AUTH_FILE);
            if !auth.is_file() || probe_codex_auth_in(program, path_env, None) == Some(false) {
                return None;
            }
            let identity = crate::agent_login::codex_identity(&auth);
            Some(Importable {
                email: identity.email,
                plan: identity.plan,
            })
        }
    }
}

/// The wire account row of a [`ClaudeAuthStatus`] (Exponential's
/// `devices.agentAccounts` vocabulary, so it stays here).
pub trait ClaudeAuthStatusExt {
    /// This status as the wire account row.
    fn account(&self, now: &str) -> AgentAccount;
}

impl ClaudeAuthStatusExt for ClaudeAuthStatus {
    fn account(&self, now: &str) -> AgentAccount {
        AgentAccount {
            signed_in: self.logged_in,
            email: self.logged_in.then(|| self.email.clone()).flatten(),
            plan: self
                .logged_in
                .then(|| self.subscription_type.clone())
                .flatten(),
            checked_at: now.to_string(),
            ..AgentAccount::default()
        }
    }
}


/// EXP-792 (EXP-747 B3): who is signed in inside ONE account profile dir.
struct ProfileAuth {
    account: AgentAccount,
    /// Whether this login's usage windows may be fetched at all
    /// ([`ClaudeAuthStatus::usage_eligible`]; codex answers over its
    /// app-server whenever it is signed in).
    usage_eligible: bool,
}

/// Who is signed in inside ONE account profile dir of `agent` — the
/// doctor's probes pointed at the profile's config dir — plus the RAW answer
/// ([`probe_profile_signed_in`]'s `Option<bool>`) from the same spawn. A
/// profile is explicit, so an unreadable answer reads as signed OUT in the
/// account row (the gate fails open on the raw `None` instead).
fn probe_profile_full(
    agent: CodingAgent,
    program: &str,
    path_env: &str,
    config_dir: &Path,
    now: &str,
) -> (Option<bool>, ProfileAuth) {
    let signed_out = |now: &str| AgentAccount {
        signed_in: false,
        checked_at: now.to_string(),
        ..AgentAccount::default()
    };
    let Some(key) = crate::agent_profiles::config_env_var(agent) else {
        return (Some(false), ProfileAuth { account: signed_out(now), usage_eligible: false });
    };
    let config = Some((key, config_dir));
    match agent {
        CodingAgent::Claude => match probe_claude_auth_status_in(program, path_env, config) {
            Some(status) => (
                Some(status.logged_in),
                ProfileAuth {
                    account: status.account(now),
                    usage_eligible: status.usage_eligible(),
                },
            ),
            None => (None, ProfileAuth { account: signed_out(now), usage_eligible: false }),
        },
        CodingAgent::Codex => {
            let raw = probe_codex_auth_in(program, path_env, config);
            let authed = raw.unwrap_or(false);
            (
                raw,
                ProfileAuth {
                    account: AgentAccount {
                        signed_in: authed,
                        checked_at: now.to_string(),
                        ..AgentAccount::default()
                    },
                    usage_eligible: authed,
                },
            )
        }
    }
}

/// EXP-1138: the RAW sign-in answer inside one profile dir — `Some(false)`
/// only when the CLI provably says signed out, `None` for a probe that never
/// ran or answered unreadably. The launch gate's rule: only a provable
/// sign-out refuses (the ambient gate fails open the same way), unlike
/// [`ToolCheck::profiles`], whose silence reads as signed out for the
/// heartbeat.
pub(crate) fn probe_profile_signed_in(
    agent: CodingAgent,
    program: &str,
    path_env: &str,
    config_dir: &Path,
) -> Option<bool> {
    let key = crate::agent_profiles::config_env_var(agent)?;
    let config = Some((key, config_dir));
    match agent {
        CodingAgent::Claude => {
            probe_claude_auth_status_in(program, path_env, config).map(|status| status.logged_in)
        }
        CodingAgent::Codex => probe_codex_auth_in(program, path_env, config),
    }
}

/// `<program> --version` ([`rt::check_tool`]), resolved against the
/// augmented login PATH (§6.12), matching the spawn environment the agent
/// will actually run in.
pub fn check_tool(tool: Tool, program: &str) -> ToolCheck {
    check_tool_with_path(tool, program, &terminal::pty::login_path())
}

/// [`check_tool`] with the PATH injected — split out so tests can probe a
/// stub-only directory deterministically.
fn check_tool_with_path(tool: Tool, program: &str, path_env: &str) -> ToolCheck {
    rt::check_tool(tool, program, path_env).into()
}

impl From<rt::ToolCheck> for ToolCheck {
    /// The runtime's probe, with none of the gates applied yet.
    fn from(check: rt::ToolCheck) -> Self {
        ToolCheck {
            tool: check.tool,
            ok: check.ok,
            version: check.version,
            error: check.error,
            authed: None,
            account: None,
            usage_eligible: false,
            acp: None,
            acp_note: None,
            signed_in_profile: None,
            importable: None,
            profiles: Vec::new(),
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_git_version_line() {
        assert_eq!(
            parse_version_output(Tool::Git, "git version 2.39.5 (Apple Git-154)\n"),
            Some("2.39.5 (Apple Git-154)".to_string())
        );
        assert_eq!(
            parse_version_output(Tool::Git, "git version 2.45.0\n"),
            Some("2.45.0".to_string())
        );
    }

    #[test]
    fn parses_claude_codex_and_pi_version_lines() {
        assert_eq!(
            parse_version_output(Tool::Claude, "1.0.35 (Claude Code)\n"),
            Some("1.0.35 (Claude Code)".to_string())
        );
        // Tolerates warning noise before the version line.
        assert_eq!(
            parse_version_output(Tool::Claude, "\n  2.1.0 (Claude Code)\n"),
            Some("2.1.0 (Claude Code)".to_string())
        );
        // Codex prints a `codex-cli ` prefix.
        assert_eq!(
            parse_version_output(Tool::Codex, "codex-cli 0.46.0\n"),
            Some("0.46.0".to_string())
        );
        assert_eq!(
            parse_version_output(Tool::Codex, "0.46.0\n"),
            Some("0.46.0".to_string())
        );
    }

    #[test]
    fn empty_output_parses_to_none() {
        assert_eq!(parse_version_output(Tool::Claude, ""), None);
        assert_eq!(parse_version_output(Tool::Git, "   \n \n"), None);
    }

    #[test]
    fn claude_version_triples_parse_and_junk_does_not() {
        assert_eq!(parse_claude_version("2.1.215 (Claude Code)"), Some((2, 1, 215)));
        assert_eq!(parse_claude_version("  9.9.9 (Claude Code stub)"), Some((9, 9, 9)));
        assert_eq!(parse_claude_version("2.1.203"), Some((2, 1, 203)));
        // Not a plain three-part leading version → None (gate stays open).
        assert_eq!(parse_claude_version("git version 2.39.5"), None);
        assert_eq!(parse_claude_version("2.1"), None);
        assert_eq!(parse_claude_version("2.1.203.7"), None);
        assert_eq!(parse_claude_version("v2.1.203"), None);
        assert_eq!(parse_claude_version(""), None);
    }

    fn green(tool: Tool, version: &str) -> ToolCheck {
        ToolCheck {
            tool,
            ok: true,
            version: Some(version.to_string()),
            error: None,
            authed: None,
            account: None,
            usage_eligible: false,
            signed_in_profile: None,
            importable: None,
            profiles: Vec::new(),
            acp: None,
            acp_note: None,
        }
    }

    fn red(tool: Tool) -> ToolCheck {
        ToolCheck {
            tool,
            ok: false,
            version: None,
            error: Some(tool.not_found_message().to_string()),
            authed: None,
            account: None,
            usage_eligible: false,
            signed_in_profile: None,
            importable: None,
            profiles: Vec::new(),
            acp: None,
            acp_note: None,
        }
    }

    /// The version gate: below-minimum flips red with the actionable
    /// "claude update" copy; at/above minimum and unparseable stay green.
    #[test]
    fn version_gate_blocks_old_clis_with_update_copy() {
        let mut old = green(Tool::Claude, "2.1.199 (Claude Code)");
        apply_version_gate(&mut old);
        assert!(!old.ok);
        assert_eq!(
            old.error.as_deref(),
            Some("Claude Code 2.1.199 is too old. Update to 2.1.215+ (run: claude update).")
        );

        // Exactly the minimum and newer stay green.
        for version in ["2.1.215 (Claude Code)", "2.1.230 (Claude Code)", "3.0.0"] {
            let mut check = green(Tool::Claude, version);
            apply_version_gate(&mut check);
            assert!(check.ok, "{version} must pass the gate");
            assert_eq!(check.error, None);
        }

        // Unparseable version → green (never falsely block a nonstandard
        // build).
        let mut odd = green(Tool::Claude, "nightly (Claude Code)");
        apply_version_gate(&mut odd);
        assert!(odd.ok);

        // A check that already failed is left alone (keeps its own error).
        let mut dead = red(Tool::Claude);
        apply_version_gate(&mut dead);
        assert_eq!(
            dead.error.as_deref(),
            Some("claude not found on PATH. Set an absolute path.")
        );
    }

    /// `run_doctor` end-to-end against stub claude binaries: an old version
    /// fails the CLAUDE gate with the update copy; a new one passes.
    #[cfg(unix)]
    #[test]
    fn run_doctor_gates_on_the_stub_version() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-coding-doctor-gate-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();

        let write_stub = |name: &str, version: &str| {
            let path = dir.join(name);
            fs::write(&path, format!("#!/bin/sh\necho '{version} (Claude Code)'\n")).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            path
        };

        // Exec'ing a just-written script can hit ETXTBSY when a concurrent
        // test's fork briefly holds the write fd (the suite spawns many git
        // children) — retry the transient race instead of flaking.
        let run_doctor_retrying = |settings: &Settings| {
            for _ in 0..20 {
                let report = run_doctor(settings, &dir);
                let busy = report
                    .first_failure_for(CodingAgent::Claude)
                    .and_then(|check| check.error.as_deref())
                    .is_some_and(|error| error.contains("Text file busy"));
                if !busy {
                    return report;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            run_doctor(settings, &dir)
        };

        // A profile, so the new stub's (unreadable) sign-in answer fails open.
        crate::agent_profiles::create(&dir, CodingAgent::Claude).unwrap();
        let old = write_stub("claude-old", "2.1.100");
        let settings = Settings {
            claude_path: old.to_string_lossy().into_owned(),
            ..Settings::default()
        };
        let report = run_doctor_retrying(&settings);
        assert!(report.first_failure_for(CodingAgent::Claude).is_some());
        assert_eq!(
            report
                .first_failure_for(CodingAgent::Claude)
                .and_then(|c| c.error.as_deref()),
            Some("Claude Code 2.1.100 is too old. Update to 2.1.215+ (run: claude update).")
        );

        let new = write_stub("claude-new", "2.1.215");
        let settings = Settings {
            claude_path: new.to_string_lossy().into_owned(),
            ..Settings::default()
        };
        let report = run_doctor_retrying(&settings);
        assert!(
            report.first_failure_for(CodingAgent::Claude).is_none(),
            "2.1.215 must pass: {:?}",
            report.first_failure_for(CodingAgent::Claude)
        );

        let _ = fs::remove_dir_all(&dir);
    }

    /// EXP-206: bare tool names resolve against the INJECTED login PATH, not
    /// the process PATH — a stub-only dir finds the stub, and an empty PATH
    /// misses even a real `git`. This is what fixes codex installed in
    /// Homebrew's bin showing "not found" under a GUI launch's minimal PATH.
    #[cfg(unix)]
    #[test]
    fn check_tool_resolves_against_the_injected_path() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-coding-doctor-path-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let stub = dir.join("codex");
        fs::write(&stub, "#!/bin/sh\necho 'codex-cli 0.46.0'\n").unwrap();
        fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();

        // Retry the transient ETXTBSY race (see run_doctor_gates_on_the_stub_version).
        let mut check = check_tool_with_path(Tool::Codex, "codex", &dir.to_string_lossy());
        for _ in 0..20 {
            if !check
                .error
                .as_deref()
                .is_some_and(|error| error.contains("Text file busy"))
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
            check = check_tool_with_path(Tool::Codex, "codex", &dir.to_string_lossy());
        }
        assert!(check.ok, "stub-only PATH must resolve: {:?}", check.error);
        assert_eq!(check.version.as_deref(), Some("0.46.0"));

        // An empty injected PATH misses even a real git — the process PATH
        // must play no part in resolution.
        let check = check_tool_with_path(Tool::Git, "git", "");
        assert!(!check.ok);
        assert_eq!(check.error.as_deref(), Some("git not found on PATH"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_binary_yields_the_actionable_spec_message() {
        let check = check_tool(Tool::Claude, "definitely-not-a-real-binary-exp");
        assert!(!check.ok);
        assert_eq!(
            check.error.as_deref(),
            Some("claude not found on PATH. Set an absolute path.")
        );
        let check = check_tool(Tool::Codex, "definitely-not-a-real-binary-exp");
        assert_eq!(
            check.error.as_deref(),
            Some("codex did not start at the configured path. Remove codexPath from settings.json or set an absolute path.")
        );
        let check = check_tool(Tool::Git, "definitely-not-a-real-binary-exp");
        assert_eq!(check.error.as_deref(), Some("git not found on PATH"));
    }

    #[test]
    fn real_git_passes_the_doctor() {
        // git is a hard dependency of this repo's own CI — a real invocation
        // keeps the success path honest.
        let check = check_tool(Tool::Git, "git");
        assert!(check.ok, "git --version failed: {:?}", check.error);
        let version = check.version.unwrap();
        assert!(!version.is_empty());
        assert!(!version.starts_with("git version"), "prefix not stripped: {version}");
    }

    /// EXP-201 per-agent gating: a missing codex never blocks a claude launch;
    /// a missing git blocks EVERY launch; the presence advertisement lists
    /// exactly the usable agents.
    #[test]
    fn report_gates_per_agent_and_advertises_installed() {
        let report = DoctorReport {
            claude: green(Tool::Claude, "2.1.215 (Claude Code)"),
            codex: red(Tool::Codex),
            git: green(Tool::Git, "2.45.0"),
        };
        assert_eq!(report.first_failure_for(CodingAgent::Claude), None);
        assert_eq!(
            report.first_failure_for(CodingAgent::Codex),
            Some(&report.codex)
        );
        assert!(report.any_agent_ok());
        assert_eq!(report.installed_agents(), vec![CodingAgent::Claude]);

        // git missing blocks every agent (§7.1 step 1 ANDs git in).
        let no_git = DoctorReport {
            git: red(Tool::Git),
            ..report.clone()
        };
        assert_eq!(
            no_git.first_failure_for(CodingAgent::Claude),
            Some(&no_git.git)
        );

        // No agent at all: the affordance-level gate flips.
        let none = DoctorReport {
            claude: red(Tool::Claude),
            ..report.clone()
        };
        assert!(!none.any_agent_ok());
        assert!(none.installed_agents().is_empty());

        // Both installed → both advertised, in ALL order.
        let all = DoctorReport {
            codex: green(Tool::Codex, "0.46.0"),
            ..report.clone()
        };
        assert_eq!(
            all.installed_agents(),
            vec![CodingAgent::Claude, CodingAgent::Codex]
        );
    }

    /// EXP-746 (D9): the caps list lives HERE, once — the desktop
    /// (`ui::steer_wiring`) and the CLI daemon both call [`device_caps`],
    /// so a new cap can no longer reach one host and miss the other.
    fn advert(agents: &[&str]) -> AgentAdvertisement {
        AgentAdvertisement {
            agents: agents.iter().map(|agent| agent.to_string()).collect(),
            unauthed_agents: Vec::new(),
            default_agent: "claude".to_string(),
            launch_defaults: agents
                .iter()
                .map(|agent| (agent.to_string(), AgentLaunchDefaults::default()))
                .collect(),
            acp_agents: Vec::new(),
        }
    }

    /// The run caps ride only while an agent is runnable. EXP-1292: the
    /// trigger caps, `chat`, `fix-conflicts` and the start-frame caps are
    /// gone with what they gated.
    #[test]
    fn run_caps_ride_only_while_an_agent_is_runnable() {
        let caps = device_caps(&advert(&["claude"]));
        for gone in [
            "triggers",
            "automations",
            "chat",
            "fix-conflicts",
            "actions",
            "action-inputs",
            "start-prompt",
            "steer-files",
            "agent-start",
            "stale-end",
            "resume",
            "worktrees",
            "launch-defaults",
        ] {
            assert!(!caps.contains(&gone.to_string()), "{gone}");
        }
        // EXP-1249: the per-run computer-use flag rides the run caps.
        assert_eq!(COMPUTER_USE_RUN_CAP, "computer-use-run");
        assert!(RUN_CAPS.contains(&COMPUTER_USE_RUN_CAP));
        assert!(!DEVICE_CAPS.contains(&COMPUTER_USE_RUN_CAP));
        assert!(caps.contains(&RESUME_RUN_CAP.to_string()));
        assert!(!device_caps(&advert(&[])).contains(&RESUME_RUN_CAP.to_string()));
    }

    /// EXP-484: signing IN is exactly what a machine with no runnable agent
    /// needs, so `agent-login` is a BUILD cap — it rides even when nothing
    /// is signed in.
    #[test]
    fn device_caps_include_agent_login_without_runnable_agents() {
        assert!(DEVICE_CAPS.contains(&"agent-login"));
        assert!(!RUN_CAPS.contains(&"agent-login"));
        let signed_out = device_caps(&advert(&[]));
        assert!(signed_out.contains(&"agent-login".to_string()));
        assert!(device_caps(&advert(&["claude"])).contains(&"agent-login".to_string()));
        // EXP-849: pointing a machine at one of its logins is part of the same
        // signed-out story — a build cap beside `agent-login`, never a run
        // cap.
        assert!(DEVICE_CAPS.contains(&ACCOUNT_SWITCH_CAP));
        assert!(!RUN_CAPS.contains(&ACCOUNT_SWITCH_CAP));
        assert!(signed_out.contains(&ACCOUNT_SWITCH_CAP.to_string()));
        // EXP-862: removing a login is the same story again — a build cap,
        // advertised with nothing runnable, since a signed-out machine is
        // exactly where a dead account gets cleaned up.
        assert_eq!(ACCOUNT_REMOVE_CAP, "account-remove");
        assert!(DEVICE_CAPS.contains(&ACCOUNT_REMOVE_CAP));
        assert!(!RUN_CAPS.contains(&ACCOUNT_REMOVE_CAP));
        assert!(signed_out.contains(&ACCOUNT_REMOVE_CAP.to_string()));
        // EXP-1137: signing a login out is a build cap too — a signed-out
        // machine may still hold the dead credential the sign-out clears.
        assert_eq!(ACCOUNT_SIGN_OUT_CAP, "account-sign-out");
        assert!(DEVICE_CAPS.contains(&ACCOUNT_SIGN_OUT_CAP));
        assert!(!RUN_CAPS.contains(&ACCOUNT_SIGN_OUT_CAP));
        assert!(signed_out.contains(&ACCOUNT_SIGN_OUT_CAP.to_string()));
    }

    /// EXP-792: a forced usage refresh is a property of the BINARY — a
    /// build cap, advertised while signed out, and the whole list stays
    /// under `capsInput`'s ceiling of 24. The retired `mcp` cap (device-run
    /// MCP OAuth) is gone: the server holds MCP credentials now.
    #[test]
    fn device_caps_include_usage_refresh_under_the_ceiling() {
        assert!(!DEVICE_CAPS.contains(&"mcp"));
        assert!(DEVICE_CAPS.contains(&"agent-usage-refresh"));
        // FEED-36: the web's "Update now" shows only for a build that runs it.
        assert!(DEVICE_CAPS.contains(&"update-now"));
        assert!(!RUN_CAPS.contains(&"update-now"));
        let signed_out = device_caps(&advert(&[]));
        assert!(signed_out.contains(&"agent-usage-refresh".to_string()));
        assert!(device_caps(&advert(&["claude"])).len() <= 24);
    }

    /// The install's CLI version rides each installed agent's top-level
    /// account row (the device settings' per-agent Update control reads it);
    /// an agent the doctor could not run keeps none, and stamping never
    /// invents a row.
    #[test]
    fn stamp_versions_names_the_installed_cli_on_the_account_row() {
        let report = DoctorReport {
            claude: green(Tool::Claude, "2.1.281"),
            codex: red(Tool::Codex),
            git: green(Tool::Git, "2.45.0"),
        };
        let mut accounts = AgentAccounts::new();
        accounts.insert(
            "claude".to_string(),
            AgentAccount {
                signed_in: true,
                checked_at: "T".into(),
                ..AgentAccount::default()
            },
        );
        accounts.insert(
            "codex".to_string(),
            AgentAccount {
                signed_in: true,
                checked_at: "T".into(),
                version: Some("stale".into()),
                ..AgentAccount::default()
            },
        );
        report.stamp_versions(&mut accounts);
        assert_eq!(accounts["claude"].version.as_deref(), Some("2.1.281"));
        assert_eq!(accounts["codex"].version, None);
        assert_eq!(accounts.len(), 2);
    }

    /// EXP-792 (A3): the signed-out fix is a button, never a command to type.
    #[test]
    fn signed_out_message_never_asks_for_a_terminal_command() {
        for tool in [Tool::Claude, Tool::Codex] {
            // EXP-1138: the account-aware refusals hold the same line.
            for message in [
                tool.signed_out_message().to_string(),
                tool.profile_signed_out_message(Some("w@acme.test")),
                tool.profile_signed_out_message(None),
            ] {
                assert!(message.contains("Settings → Agents"), "{message}");
                assert!(!message.contains('`'), "{message}");
                assert!(!message.contains("terminal"), "{message}");
            }
        }
        // No profile to run on: the contract's refusal, byte for byte.
        assert_eq!(
            Tool::no_profile_message(CodingAgent::Claude),
            "Sign in to Claude Code on this device first."
        );
    }

    /// EXP-746: `acp` is a BUILD cap — it says this binary speaks the engine
    /// and steering v2, not that any agent on this machine is ready today
    /// (per-agent readiness is local, [`AgentAdvertisement::acp_agents`]).
    /// The whole list stays inside `devices.register`'s 24-cap ceiling.
    #[test]
    fn build_caps_include_acp() {
        assert!(DEVICE_CAPS.contains(&"acp"));
        assert!(!RUN_CAPS.contains(&"acp"));
        assert!(device_caps(&advert(&[])).contains(&"acp".to_string()));
        assert!(device_caps(&advert(&["claude"])).len() <= 24);
    }

    /// EXP-746: ACP readiness is not part of the DOCTOR's launch gate — a
    /// machine whose agents all lack it still passes every tool check
    /// (EXP-773's refusal is the launcher's, with the note below).
    #[test]
    fn acp_readiness_is_non_fatal() {
        let mut claude = green(Tool::Claude, "2.1.215 (Claude Code)");
        claude.acp = Some(false);
        claude.acp_note = Some("too old".to_string());
        let report = DoctorReport {
            claude,
            codex: red(Tool::Codex),
            git: green(Tool::Git, "2.45.0"),
        };
        assert!(report.any_agent_ok());
        assert_eq!(report.first_failure_for(CodingAgent::Claude), None);
        assert_eq!(report.installed_agents(), vec![CodingAgent::Claude]);
        assert!(report
            .agent_advertisement(&Settings::default())
            .acp_agents
            .is_empty());
    }

    /// EXP-746: the two claude version gates are SEPARATE — a CLI new enough
    /// to launch but too old for the control protocol stays green with
    /// `acp: Some(false)` and a note; at/above the ACP minimum it is ready.
    #[test]
    fn claude_acp_gate_is_separate_from_the_launch_gate() {
        let mut between = green(Tool::Claude, "2.1.240 (Claude Code)");
        apply_version_gate(&mut between);
        assert!(between.ok, "the launch gate is MIN_CLAUDE_VERSION alone");
        assert_eq!(between.acp, Some(false));
        assert!(between
            .acp_note
            .as_deref()
            .is_some_and(|note| note.contains("2.1.263")));

        let mut ready = green(Tool::Claude, "2.1.263 (Claude Code)");
        apply_version_gate(&mut ready);
        assert!(ready.ok);
        assert_eq!(ready.acp, Some(true));
        assert_eq!(ready.acp_note, None);

        // Too old for BOTH gates: red for the launch, and not ACP-ready.
        let mut old = green(Tool::Claude, "2.1.199 (Claude Code)");
        apply_version_gate(&mut old);
        assert!(!old.ok);
        assert_eq!(old.acp, Some(false));

        // Unparseable stays green and unknown (→ the launch gate refuses it,
        // EXP-773; the doctor row itself never falsely blocks).
        let mut odd = green(Tool::Claude, "nightly (Claude Code)");
        apply_version_gate(&mut odd);
        assert!(odd.ok);
        assert_eq!(odd.acp, None);
    }

    /// EXP-758: codex gets the same version floor claude has. Below it the
    /// app-server handshake the adapter is written against is unverified,
    /// so the launch is refused (EXP-773) — with a note saying why.
    #[test]
    fn codex_acp_gate_has_a_version_floor() {
        let mut old = green(Tool::Codex, "0.143.9");
        apply_codex_acp(&mut old);
        assert!(old.ok, "the launch gate is presence + sign-in alone");
        assert_eq!(old.acp, Some(false));
        assert!(
            old.acp_note
                .as_deref()
                .is_some_and(|note| note.contains("0.144.0")),
            "{:?}",
            old.acp_note
        );

        let mut floor = green(Tool::Codex, "0.144.0");
        apply_codex_acp(&mut floor);
        assert_eq!(floor.acp, Some(true));
        assert_eq!(floor.acp_note, None);

        let mut newer = green(Tool::Codex, "0.153.3");
        apply_codex_acp(&mut newer);
        assert_eq!(newer.acp, Some(true));

        // Unparseable stays READY: never falsely demote a nonstandard build
        // (the engine's own `initialize` is the authoritative check).
        let mut odd = green(Tool::Codex, "nightly");
        apply_codex_acp(&mut odd);
        assert_eq!(odd.acp, Some(true));
        assert_eq!(odd.acp_note, None);

        // A codex that is not there at all keeps the presence note.
        let mut missing = red(Tool::Codex);
        apply_codex_acp(&mut missing);
        assert_eq!(missing.acp, Some(false));
        assert_eq!(missing.acp_note.as_deref(), Some("codex is not available"));

        // The version parse takes the bare triple `parse_version_output`
        // leaves behind once the `codex-cli ` prefix is stripped.
        assert_eq!(parse_codex_version("0.144.5"), Some((0, 144, 5)));
        assert_eq!(parse_codex_version("nightly"), None);
    }

    /// EXP-437: the advertisement carries the machine's per-agent launch
    /// defaults — capability-masked (codex never plan/ultracode), blank
    /// efforts preserved, only RUNNABLE agents listed, and the
    /// default agent passed through even when it is not installed.
    #[test]
    fn advertisement_carries_capability_masked_launch_defaults() {
        let report = DoctorReport {
            claude: green(Tool::Claude, "2.1.215 (Claude Code)"),
            codex: green(Tool::Codex, "0.46.0"),
            git: green(Tool::Git, "2.45.0"),
        };
        let settings = Settings {
            claude_model: "opus".into(),
            claude_effort: "".into(),
            claude_ultracode: true,
            codex_model: "".into(),
            codex_effort: "high".into(),
            ..Settings::default()
        };
        let advert = report.agent_advertisement(&settings);
        assert_eq!(advert.agents, vec!["claude", "codex"]);
        assert_eq!(advert.default_agent, "claude");
        // Only runnable agents get a defaults entry.
        assert_eq!(
            advert.launch_defaults.keys().collect::<Vec<_>>(),
            vec!["claude", "codex"]
        );
        let claude = &advert.launch_defaults["claude"];
        assert_eq!(claude.model, "opus");
        assert_eq!(claude.effort, "");
        assert!(claude.ultracode);
        // S-3: no plan-mode default.
        assert!(!claude.plan_mode);
        // Capability masking: codex can never carry ultracode/plan, but its
        // blank model rides through.
        let codex = &advert.launch_defaults["codex"];
        assert_eq!(codex.model, "");
        assert_eq!(codex.effort, "high");
        assert!(!codex.ultracode);
        assert!(!codex.plan_mode);

        // A default agent that is NOT runnable still passes through as an id
        // (clients clamp); it simply has no launch_defaults entry.
        let report = DoctorReport {
            codex: red(Tool::Codex),
            ..report
        };
        let settings = Settings {
            default_agent: CodingAgent::Codex,
            ..settings
        };
        let advert = report.agent_advertisement(&settings);
        assert_eq!(advert.default_agent, "codex");
        assert!(!advert.launch_defaults.contains_key("codex"));
    }

    /// EXP-409: `claude auth status` JSON classification — noise-tolerant,
    /// fails open on junk.
    #[test]
    fn claude_auth_status_parses_logged_in_flag() {
        assert_eq!(
            parse_claude_auth_status(
                "{\n  \"loggedIn\": true,\n  \"authMethod\": \"claude.ai\",\n  \"apiProvider\": \"firstParty\"\n}\n"
            ),
            Some(true)
        );
        assert_eq!(
            parse_claude_auth_status("{\"loggedIn\": false, \"authMethod\": \"none\"}"),
            Some(false)
        );
        // Warning noise before the JSON is tolerated.
        assert_eq!(
            parse_claude_auth_status("some warning line\n{\"loggedIn\": true}"),
            Some(true)
        );
        // No JSON / no flag / not a bool → fail open.
        assert_eq!(parse_claude_auth_status("Unknown command `auth`"), None);
        assert_eq!(parse_claude_auth_status("{\"authMethod\": \"none\"}"), None);
        assert_eq!(parse_claude_auth_status("{\"loggedIn\": \"yes\"}"), None);
        assert_eq!(parse_claude_auth_status(""), None);
    }

    /// EXP-409: `codex login status` classification — "not logged in" beats
    /// the exit code; an unknown failure (old build without the subcommand)
    /// fails open.
    #[test]
    fn codex_login_status_classifies_output() {
        assert_eq!(parse_codex_login_status(true, "Logged in using ChatGPT\n"), Some(true));
        assert_eq!(parse_codex_login_status(false, "Not logged in\n"), Some(false));
        // Some builds print the answer but still exit 0.
        assert_eq!(parse_codex_login_status(true, "Not logged in\n"), Some(false));
        assert_eq!(parse_codex_login_status(false, "error: unknown subcommand `login`"), None);
    }

    /// EXP-484: the full `claude auth status` object — the account fields
    /// ride through, absent ones tolerate, and `usage_eligible` gates the
    /// OAuth usage fetch on a first-party claude.ai login.
    #[test]
    fn claude_auth_status_full_names_the_account_and_gates_usage() {
        let status = parse_claude_auth_status_full(
            "{\"loggedIn\": true, \"authMethod\": \"claude.ai\", \"apiProvider\": \"firstParty\", \"email\": \"dev@acme.test\", \"subscriptionType\": \"max\"}",
        )
        .unwrap();
        assert!(status.logged_in);
        assert_eq!(status.email.as_deref(), Some("dev@acme.test"));
        assert_eq!(status.subscription_type.as_deref(), Some("max"));
        assert!(status.usage_eligible());
        let account = status.account("2026-08-28T10:00:00.000Z");
        assert!(account.signed_in);
        assert_eq!(account.email.as_deref(), Some("dev@acme.test"));
        assert_eq!(account.plan.as_deref(), Some("max"));
        assert_eq!(account.checked_at, "2026-08-28T10:00:00.000Z");

        // An absent apiProvider is first-party by omission.
        let status = parse_claude_auth_status_full(
            "{\"loggedIn\": true, \"authMethod\": \"claude.ai\"}",
        )
        .unwrap();
        assert!(status.usage_eligible());
        assert_eq!(status.account("now").email, None);

        // An API-key / Bedrock login has no usage windows to read.
        for body in [
            "{\"loggedIn\": true, \"authMethod\": \"apiKey\"}",
            "{\"loggedIn\": true, \"authMethod\": \"claude.ai\", \"apiProvider\": \"bedrock\"}",
            "{\"loggedIn\": false, \"authMethod\": \"claude.ai\"}",
        ] {
            assert!(!parse_claude_auth_status_full(body).unwrap().usage_eligible(), "{body}");
        }
        // A signed-OUT status names nobody, even with a stale email around.
        let signed_out = parse_claude_auth_status_full(
            "{\"loggedIn\": false, \"email\": \"dev@acme.test\", \"subscriptionType\": \"max\"}",
        )
        .unwrap();
        let account = signed_out.account("now");
        assert!(!account.signed_in);
        assert_eq!(account.email, None);
        assert_eq!(account.plan, None);
        // Junk still fails open, exactly like the bool-only parse.
        assert_eq!(parse_claude_auth_status_full("nope"), None);
    }

    /// EXP-484: the accounts map covers INSTALLED agents only, restamps
    /// every entry with the passed instant, and never leaks into the
    /// advertisement (whose change detection would then flap every probe).
    /// A throwaway data dir for the profile collectors (no tempfile dep in
    /// this crate — the same shape `agent_profiles`' own tests use).
    fn profile_data_dir(tag: &str) -> std::path::PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-doctor-profiles-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create data dir");
        dir
    }

    /// No profile: no rows (the ambient login is never one), the top-level
    /// fields are the check's signed-out row, and an ambient login rides as
    /// `importable`.
    #[test]
    fn an_agent_with_no_profile_reports_no_rows_and_its_importable_login() {
        let dir = profile_data_dir("none");
        let mut claude = green(Tool::Claude, "2.1.215 (Claude Code)");
        claude.account = Some(AgentAccount {
            signed_in: false,
            checked_at: "2026-01-01T00:00:00.000Z".into(),
            ..AgentAccount::default()
        });
        claude.importable = Some(Importable {
            email: Some("dev@acme.test".into()),
            plan: Some("max".into()),
        });
        let report = DoctorReport {
            claude,
            codex: red(Tool::Codex),
            git: green(Tool::Git, "2.44.0"),
        };
        let accounts =
            report.agent_accounts_with_profiles(&Settings::default(), &dir, "2026-02-02T00:00:00.000Z");
        let claude = accounts.get("claude").expect("claude row");
        assert!(claude.profiles.is_empty());
        assert!(!claude.signed_in);
        assert_eq!(claude.email, None, "the ambient login never names the row");
        assert_eq!(
            claude.importable.as_ref().and_then(|found| found.email.as_deref()),
            Some("dev@acme.test")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every profile is a row — no `system` row, no label — the last used one
    /// carries `active`, and a committed sign-in's stamp rides `lastLoginAt`.
    #[test]
    fn every_profile_is_a_row_and_the_active_one_is_flagged() {
        let dir = profile_data_dir("rows");
        let home = crate::agent_profiles::create(&dir, CodingAgent::Claude).unwrap();
        let work = crate::agent_profiles::create(&dir, CodingAgent::Claude).unwrap();
        crate::agent_profiles::note_last_used(&dir, CodingAgent::Claude, &work.id).unwrap();
        crate::agent_profiles::stamp_login(&dir, CodingAgent::Claude, &work.id, Some("w@acme.test"))
            .unwrap();
        let report = DoctorReport {
            claude: green(Tool::Claude, "2.1.215 (Claude Code)"),
            codex: red(Tool::Codex),
            git: green(Tool::Git, "2.44.0"),
        };
        let mut claude = report.claude.clone();
        claude.account = Some(AgentAccount::default());
        let report = DoctorReport { claude, ..report };
        // The doctor probed neither profile, so each is probed here — with a
        // binary that cannot exist: unreadable reads signed OUT.
        let settings = Settings {
            claude_path: "/nonexistent/exp792-claude".to_string(),
            ..Settings::default()
        };
        let accounts =
            report.agent_accounts_with_profiles(&settings, &dir, "2026-02-02T00:00:00.000Z");
        let claude = accounts.get("claude").expect("claude row");
        let ids: Vec<&str> = claude.profiles.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, vec![home.id.as_str(), work.id.as_str()]);
        assert!(!claude.profiles[0].active);
        assert!(claude.profiles[1].active);
        assert!(claude.profiles.iter().all(|row| !row.signed_in), "unreadable reads signed out");
        assert!(claude.profiles[1].last_login_at.is_some());
        assert_eq!(claude.profiles[0].last_login_at, None);
        // EXP-1013: the signed-out row still names its last email.
        assert_eq!(claude.profiles[1].email.as_deref(), Some("w@acme.test"));
        assert_eq!(claude.email.as_deref(), Some("w@acme.test"), "top level = the active row");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn agent_accounts_cover_installed_agents_only() {
        let mut claude = green(Tool::Claude, "2.1.215 (Claude Code)");
        claude.account = Some(AgentAccount {
            signed_in: true,
            email: Some("dev@acme.test".into()),
            plan: Some("max".into()),
            checked_at: "2026-01-01T00:00:00.000Z".into(),
            ..AgentAccount::default()
        });
        let mut codex = green(Tool::Codex, "0.46.0");
        codex.account = Some(AgentAccount {
            signed_in: true,
            checked_at: "2026-01-01T00:00:00.000Z".into(),
            ..AgentAccount::default()
        });
        let report = DoctorReport {
            claude,
            codex,
            git: green(Tool::Git, "2.45.0"),
        };
        let accounts = report.agent_accounts("2026-08-28T10:00:00.000Z");
        assert_eq!(accounts.keys().collect::<Vec<_>>(), vec!["claude", "codex"]);
        assert_eq!(accounts["claude"].email.as_deref(), Some("dev@acme.test"));
        assert_eq!(accounts["claude"].checked_at, "2026-08-28T10:00:00.000Z");
        assert_eq!(accounts["codex"].checked_at, "2026-08-28T10:00:00.000Z");
        assert_eq!(accounts["codex"].email, None);

        // The advertisement must NOT carry any of it (anti-flap key).
        let advert = report.agent_advertisement(&Settings::default());
        let wire = format!("{advert:?}");
        assert!(!wire.contains("dev@acme.test"), "accounts never ride the advertisement");
        assert!(!wire.contains("checked_at"));
    }


    /// EXP-409 end-to-end against stub binaries, over the PROFILES: an agent
    /// whose profile is signed out flips red with the sign-in copy (version
    /// kept), a signed-in one stays green with `authed: Some(true)`, an agent
    /// with NO profile is signed out whatever its ambient login says, and the
    /// report's gates treat signed-out as not installed while
    /// `unauthed_agents` still names it.
    #[cfg(unix)]
    #[test]
    fn auth_gate_flips_signed_out_agents() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let dir = profile_data_dir("auth");
        let bare = profile_data_dir("auth-bare");
        for agent in CodingAgent::ALL {
            crate::agent_profiles::create(&dir, agent).unwrap();
        }
        let write_stub = |name: &str, body: &str| {
            let path = dir.join(name);
            fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            crate::test_support::wait_until_executable(&path);
            path
        };

        // Retry the transient ETXTBSY race (see run_doctor_gates_on_the_stub_version).
        let checked_in = |data_dir: &Path, tool: Tool, program: &std::path::Path| {
            let program = program.to_string_lossy();
            for _ in 0..20 {
                let mut check = check_tool(tool, &program);
                apply_auth_gate(&mut check, &program, &terminal::pty::login_path(), data_dir);
                let busy = check
                    .error
                    .as_deref()
                    .is_some_and(|error| error.contains("Text file busy"));
                if !busy {
                    return check;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            panic!("stub stayed ETXTBSY");
        };
        let checked = |tool: Tool, program: &std::path::Path| checked_in(&dir, tool, program);

        let claude_out = write_stub(
            "claude-out",
            "case \"$1\" in\n--version) echo '9.9.9 (Claude Code)';;\nauth) echo '{\"loggedIn\": false, \"authMethod\": \"none\"}';;\nesac",
        );
        let check = checked(Tool::Claude, &claude_out);
        assert!(!check.ok);
        assert!(check.signed_out());
        assert_eq!(check.version.as_deref(), Some("9.9.9 (Claude Code)"));
        assert_eq!(
            check.error.as_deref(),
            Some("claude is installed but not signed in. Sign in from Settings → Agents, or from the Sign in button on the failed start.")
        );
        assert_eq!(check.importable, None);

        let claude_in = write_stub(
            "claude-in",
            "case \"$1\" in\n--version) echo '9.9.9 (Claude Code)';;\nauth) echo '{\"loggedIn\": true, \"email\": \"dev@acme.test\"}';;\nesac",
        );
        let check = checked(Tool::Claude, &claude_in);
        assert!(check.ok, "{:?}", check.error);
        assert_eq!(check.authed, Some(true));
        assert!(check.signed_in_profile.is_some());
        assert_eq!(
            check.importable.as_ref().and_then(|found| found.email.as_deref()),
            Some("dev@acme.test"),
            "this stub's ambient login is signed in too"
        );
        // No profile at all: signed out, whatever the ambient login says —
        // which is still offered for import.
        let check = checked_in(&bare, Tool::Claude, &claude_in);
        assert!(!check.ok);
        assert!(check.signed_out());
        assert!(check.importable.is_some());

        let codex_out = write_stub(
            "codex-out",
            "case \"$1\" in\n--version) echo 'codex-cli 0.46.0';;\nlogin) echo 'Not logged in' >&2; exit 1;;\nesac",
        );
        let check = checked(Tool::Codex, &codex_out);
        assert!(!check.ok);
        assert!(check.signed_out());
        assert_eq!(
            check.error.as_deref(),
            Some("Codex is not signed in on this machine. Sign in from Settings → Agents, or from the Sign in button on the failed start.")
        );

        // The report-level view: signed-out codex is out of installed_agents
        // but named by unauthed_agents; the affordance gate ignores it.
        let report = DoctorReport {
            claude: checked(Tool::Claude, &claude_in),
            codex: checked(Tool::Codex, &codex_out),
            git: green(Tool::Git, "2.45.0"),
        };
        assert_eq!(report.installed_agents(), vec![CodingAgent::Claude]);
        assert_eq!(report.unauthed_agents(), vec![CodingAgent::Codex]);
        assert!(report.any_agent_ok());
        assert!(report.first_failure_for(CodingAgent::Codex).is_some());

        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&bare);
    }

    /// EXP-414: a wedged probe is killed at the deadline instead of stalling
    /// the caller (the CLI daemon runs the doctor inline every 5 minutes).
    #[cfg(unix)]
    #[test]
    fn output_with_timeout_kills_a_hung_child() {
        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", "sleep 30"]);
        let started = std::time::Instant::now();
        let err = output_with_timeout(cmd, Duration::from_millis(300)).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "must return at the deadline, not the child's own runtime"
        );
    }

    /// A fast child behaves exactly like `Command::output()` — stdout,
    /// stderr and exit status all ride through.
    #[cfg(unix)]
    #[test]
    fn output_with_timeout_passes_a_fast_child_through() {
        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", "echo out; echo err >&2; exit 3"]);
        let output = output_with_timeout(cmd, Duration::from_secs(10)).unwrap();
        assert_eq!(String::from_utf8_lossy(&output.stdout), "out\n");
        assert_eq!(String::from_utf8_lossy(&output.stderr), "err\n");
        assert_eq!(output.status.code(), Some(3));
    }

    // ---- the agent runs on its PROFILES only ----

    /// The stub agents + a data dir for the profile-aware doctor: every
    /// login probes signed OUT unless its config dir carries the marker.
    #[cfg(unix)]
    fn profile_doctor(tag: &str) -> (std::path::PathBuf, Settings) {
        let dir = profile_data_dir(tag);
        let settings = Settings {
            claude_path: crate::test_support::auth_stub(&dir, "claude", "9.9.9 (Claude Code)"),
            codex_path: crate::test_support::auth_stub(&dir, "codex", "9.9.9"),
            ..Settings::default()
        };
        (dir, settings)
    }

    /// A signed-in profile makes the agent runnable: the check is green on
    /// it, names it, mirrors its identity, the advertisement lists the agent
    /// (and the run caps with it), and codex's ACP readiness follows.
    #[cfg(unix)]
    #[test]
    fn a_signed_in_profile_makes_the_agent_runnable() {
        let (dir, settings) = profile_doctor("runnable");
        for agent in CodingAgent::ALL {
            let work = crate::agent_profiles::create(&dir, agent).unwrap();
            crate::test_support::sign_in_profile(&dir, agent, &work.id);
        }
        let report = run_doctor(&settings, &dir);
        for agent in CodingAgent::ALL {
            let check = report.check_for(agent);
            assert!(check.ok, "{agent:?}: {:?}", check.error);
            assert_eq!(check.error, None);
            assert_eq!(check.authed, Some(true));
            assert!(!check.signed_out());
            let work = crate::agent_profiles::list(&dir, agent)[0].id.clone();
            assert_eq!(check.signed_in_profile.as_deref(), Some(work.as_str()));
            assert!(
                check.account.as_ref().is_some_and(|account| account.signed_in),
                "the top-level row mirrors the last used profile"
            );
            assert_eq!(check.importable, None, "the stub's ambient login is signed out");
        }
        assert_eq!(report.claude.account.as_ref().unwrap().email.as_deref(), Some("work@acme.test"));
        assert_eq!(report.codex.acp, Some(true));
        assert_eq!(report.installed_agents(), CodingAgent::ALL.to_vec());
        assert!(report.unauthed_agents().is_empty());
        let advertised = report.agent_advertisement(&settings);
        assert_eq!(advertised.agents, vec!["claude", "codex"]);
        assert!(device_caps(&advertised).contains(&RESUME_RUN_CAP.to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// No signed-in profile — none at all, or only signed-out ones — is a
    /// signed-out agent, red with the sign-in copy.
    #[cfg(unix)]
    #[test]
    fn no_signed_in_profile_turns_the_agent_red() {
        let (dir, settings) = profile_doctor("red");
        let report = run_doctor(&settings, &dir);
        for agent in CodingAgent::ALL {
            let check = report.check_for(agent);
            assert!(!check.ok, "{agent:?}: no profile");
            assert!(check.signed_out());
            assert!(check.account.as_ref().is_some_and(|account| !account.signed_in));
        }
        for agent in CodingAgent::ALL {
            crate::agent_profiles::create(&dir, agent).unwrap();
        }
        let report = run_doctor(&settings, &dir);
        for agent in CodingAgent::ALL {
            let check = report.check_for(agent);
            assert!(!check.ok);
            assert!(check.signed_out());
            assert_eq!(check.signed_in_profile, None);
            assert_eq!(check.error.as_deref(), Some(check.tool.signed_out_message()));
        }
        assert_eq!(report.codex.acp, Some(false));
        assert!(report.installed_agents().is_empty());
        assert_eq!(report.unauthed_agents(), CodingAgent::ALL.to_vec());
        assert!(!device_caps(&report.agent_advertisement(&settings))
            .contains(&RESUME_RUN_CAP.to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The last used login is probed first, so the profile the check names is
    /// the one an unnamed launch lands on.
    #[cfg(unix)]
    #[test]
    fn the_last_used_profile_is_probed_first() {
        let (dir, settings) = profile_doctor("default-first");
        let agent = CodingAgent::Claude;
        let home = crate::agent_profiles::create(&dir, agent).unwrap();
        let work = crate::agent_profiles::create(&dir, agent).unwrap();
        crate::test_support::sign_in_profile(&dir, agent, &home.id);
        crate::test_support::sign_in_profile(&dir, agent, &work.id);
        crate::agent_profiles::note_last_used(&dir, agent, &work.id).unwrap();
        let report = run_doctor(&settings, &dir);
        assert_eq!(report.claude.signed_in_profile.as_deref(), Some(work.id.as_str()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The launch gate's account half: a launch with NO profile (none named
    /// and none resolved, the retired `system`, a stale id) is refused — no
    /// ambient login is ever spent; the signed-in profile passes without a
    /// second probe; a signed-out profile refuses by its email.
    #[cfg(unix)]
    #[test]
    fn account_failure_judges_the_login_the_launch_spends() {
        let (dir, settings) = profile_doctor("account-gate");
        let agent = CodingAgent::Codex;
        let work = crate::agent_profiles::create(&dir, agent).unwrap();
        let home = crate::agent_profiles::create(&dir, agent).unwrap();
        crate::test_support::sign_in_profile(&dir, agent, &work.id);
        let report = run_doctor(&settings, &dir);
        assert!(report.codex.ok);
        let path = terminal::pty::login_path();
        let gate = |account: Option<&str>| {
            report.account_failure_with_path(&settings, &dir, agent, account, &path)
        };

        for none in [None, Some("system"), Some(""), Some("deadbeef")] {
            let failed = gate(none).unwrap_or_else(|| panic!("{none:?} must refuse"));
            assert!(!failed.ok);
            assert_eq!(failed.error.as_deref(), Some("Sign in to Codex on this device first."));
            assert!(failed.signed_out(), "the red copy offers a sign-in");
            assert_eq!(failed.version.as_deref(), Some("9.9.9"), "the check is kept");
        }
        assert_eq!(gate(Some(&work.id)), None);
        let failed = gate(Some(&home.id)).expect("a signed-out profile refuses");
        assert_eq!(
            failed.error.as_deref(),
            Some("That codex account is signed out on this machine. Sign in from Settings → Agents, or pick another account.")
        );
        crate::agent_profiles::stamp_login(&dir, agent, &home.id, Some("h@acme.test")).unwrap();
        let failed = gate(Some(&home.id)).expect("still signed out");
        assert_eq!(
            failed.error.as_deref(),
            Some("codex account h@acme.test is signed out on this machine. Sign in from Settings → Agents, or pick another account.")
        );
        assert_eq!(failed.authed, Some(false));

        // A check the agent gate already refused has nothing to add.
        let mut red_report = report.clone();
        red_report.codex.ok = false;
        assert_eq!(
            red_report.account_failure_with_path(&settings, &dir, agent, None, &path),
            None
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An UNREADABLE profile probe fails open: a stub whose `auth status` is
    /// not the sign-in JSON never turns the row red nor refuses a launch on
    /// that profile.
    #[cfg(unix)]
    #[test]
    fn an_unreadable_profile_probe_fails_open() {
        let dir = profile_data_dir("unreadable");
        let settings = Settings {
            claude_path: crate::test_support::acp_ready_stub(&dir, "claude", "9.9.9 (Claude Code)"),
            ..Settings::default()
        };
        let agent = CodingAgent::Claude;
        let work = crate::agent_profiles::create(&dir, agent).unwrap();
        let report = run_doctor(&settings, &dir);
        assert!(report.claude.ok);
        assert_eq!(report.claude.authed, None);
        assert_eq!(
            report.account_failure(&settings, &dir, agent, Some(&work.id)),
            None
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
