//! The tooling doctor (masterplan-v3 §7.7, EXP-201): runs `--version` on
//! every agent CLI (`claude`, `codex`, `pi` — each at its configured/probed
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
//! and pi have NO minimum version yet (presence-only, deliberately lenient).
//!
//! EXP-409: an installed agent that is SIGNED OUT fails its check too —
//! installed-but-not-signed-in equals not installed for every gate, because
//! a logged-out agent spawns onto its login prompt and a headless/remote
//! session hangs there invisibly. Probes: `claude auth status` (local JSON,
//! `{"loggedIn": bool}`), `codex login status` (exit 0 / "Not logged in"),
//! and pi credential presence (`~/.pi/agent/auth.json` or a provider API-key
//! env var — pi has no login command). Every probe FAILS OPEN: an
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
//! codex/pi as "not found" on machines where every launch worked (EXP-206).
//!
//! Blocking `std::process` calls — callers run this off the foreground
//! executor (settings "Check tools" button, onboarding, launch step 0).

use crate::agent::CodingAgent;
use crate::agent_accounts::{now_iso, pi_account, AgentAccount, AgentAccounts};
use crate::settings::Settings;
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};
use terminal::process::background_command;

/// EXP-414: the deadline for every probe shell-out. The CLI daemon re-runs
/// the doctor inline on a 5-minute cadence — an agent CLI that wedges (a
/// hung update check, a dead network filesystem) would otherwise stall that
/// loop, and with it the device's presence, indefinitely.
pub(crate) const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// The minimum supported Claude Code version: `--permission-mode auto`
/// (EXP-201's default posture) is verified on 2.1.215; `--effort ultracode`
/// landed in 2.1.203, `--permission-mode plan`/`manual` in 2.1.200 —
/// everything the launcher's claude argv relies on.
pub const MIN_CLAUDE_VERSION: (u32, u32, u32) = (2, 1, 215);

/// EXP-746: the minimum Claude Code that speaks the ACP engine's control
/// protocol (`--input-format stream-json` + `--permission-prompt-tool
/// stdio`). Deliberately SEPARATE from [`MIN_CLAUDE_VERSION`]: it never
/// turns the doctor row red, but a coding launch below it is REFUSED with
/// [`ToolCheck::acp_note`] saying why (EXP-773: the engine is the one
/// transport, there is no terminal fallback).
pub const MIN_CLAUDE_ACP_VERSION: (u32, u32, u32) = (2, 1, 263);

/// EXP-758: the same floor for codex, and for the same reason — an older
/// `codex app-server` resolves to ACP, fails the `initialize` + `thread/start`
/// handshake the adapter is written against, and leaves a session row nobody
/// can end but by hand.
///
/// The 0.144 line is what the adapter and its fixtures were recorded from
/// (`engine/tests/codex_adapter.rs`: "recorded from the installed codex
/// 0.144.5"), and nothing older was ever exercised — so it is the lowest
/// version this build claims. Pinned at `.0` rather than `.5` because the
/// patch releases inside that minor share the app-server shape; anything
/// below it is unverified, not known-broken, which is exactly why the check
/// never turns the doctor row red — but a coding launch below it is refused
/// with the note (EXP-773), the same as claude's.
pub const MIN_CODEX_ACP_VERSION: (u32, u32, u32) = (0, 144, 0);

/// EXP-746 (D9): the BUILD capabilities every host advertises to
/// `devices.register`, whatever it can currently run. ONE list — the desktop
/// (`ui::steer_wiring`) and the CLI daemon both call [`device_caps`], which
/// is what makes a new cap reach both hosts (they used to be hand-synced
/// copies, and a one-sided edit silently made one host un-targetable).
///
/// - `resume`/`worktrees`/`launch-defaults` (EXP-481) — the device-admin
///   protocol.
/// - `agent-login` (EXP-484) — signing IN is exactly what a machine with no
///   runnable agent needs, so it is a build cap.
/// - `agent-start` (EXP-679) — this build reads a start frame's
///   `started_reason` and forwards it to `codingSessions.start`, so an
///   agent-parented start lands UNATTENDED. The server refuses one against a
///   device without it.
/// - `acp` (EXP-746) — this build speaks the ACP engine and steering v2:
///   `set_config`/`set_mode` frames, `config_state`/`usage` kinds.
/// - `agent-login-code` (EXP-765) — this build runs `agent_login_code`: it
///   types the authorization code claude's browser page hands the requester
///   into the login PTY still waiting for it. EXP-745 dropped the server and
///   current-client gates on it (every device runs the command), but SHIPPED
///   iOS/Android builds still hide their code field for a machine that does
///   not advertise it, so the cap stays declared.
/// - `mcp` (EXP-792) — this build runs `mcp_oauth_start`/`mcp_oauth_code`
///   and reports per-server MCP readiness on the heartbeat; the server
///   refuses `beginOAuth` against a device without it.
/// - `agent-usage-refresh` (EXP-792) — this build runs
///   `agent_usage_refresh` (a forced usage re-read, 429 floor kept).
/// - `update-now` (FEED-36) — this build runs `update_now`: ends every live
///   session and applies a queued self-update right away (the CLI daemon;
///   the desktop advertises it too but updates through its own updater).
///
/// Ceiling check: `devices.register`'s caps input accepts 24 caps
/// (`apps/web/src/lib/trpc/devices.ts`); this is 10 + 6 = 16.
pub const DEVICE_CAPS: [&str; 10] = [
    "resume",
    "worktrees",
    "launch-defaults",
    "agent-login",
    "agent-start",
    "acp",
    "agent-login-code",
    "mcp",
    "agent-usage-refresh",
    "update-now",
];

/// The action-run capabilities — advertised only while at least one agent is
/// RUNNABLE (EXP-409: a machine whose only agents are signed out cannot run
/// actions either). EXP-530's `automations` (this host evaluates the triggers
/// bound to its device id), EXP-615's `chat` (the hidden `builtin:chat`
/// action) and EXP-637's `resume-run` (resume an ended run out of the local
/// run registry) ride with them.
pub const ACTION_CAPS: [&str; 6] = [
    "actions",
    "action-inputs",
    "fix-conflicts",
    "automations",
    "chat",
    RESUME_RUN_CAP,
];

/// EXP-637's resume cap, by name: the ONE place the literal lives, so a
/// client deciding whether another machine can take a resume (EXP-800) never
/// repeats the string.
pub const RESUME_RUN_CAP: &str = "resume-run";

/// The caps to advertise for a doctor snapshot: the build caps always, plus
/// the action caps while anything is runnable.
pub fn device_caps(advertised: &AgentAdvertisement) -> Vec<String> {
    let mut caps: Vec<String> = DEVICE_CAPS.iter().map(|cap| cap.to_string()).collect();
    if !advertised.agents.is_empty() {
        caps.extend(ACTION_CAPS.iter().map(|cap| cap.to_string()));
    }
    caps
}

/// The local binaries the launcher ever shells out to (§7.1 step 3:
/// argv `git` + the agent CLIs, never `gh`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Claude,
    Codex,
    Pi,
    Git,
}

impl Tool {
    pub fn label(self) -> &'static str {
        match self {
            Tool::Claude => "claude",
            Tool::Codex => "codex",
            Tool::Pi => "pi",
            Tool::Git => "git",
        }
    }

    /// The agent this tool check backs (`None` for git).
    pub fn agent(self) -> Option<CodingAgent> {
        match self {
            Tool::Claude => Some(CodingAgent::Claude),
            Tool::Codex => Some(CodingAgent::Codex),
            Tool::Pi => Some(CodingAgent::Pi),
            Tool::Git => None,
        }
    }

    /// The §7.7 red actionable message for a missing binary.
    fn not_found_message(self) -> &'static str {
        match self {
            Tool::Claude => "claude not found on PATH. Set an absolute path.",
            Tool::Codex => "codex not found on PATH. Set an absolute path.",
            Tool::Pi => "pi not found on PATH. Set an absolute path.",
            Tool::Git => "git not found on PATH",
        }
    }

    /// The EXP-409 red actionable message for an installed-but-signed-out
    /// agent (never produced for git). EXP-792 (A3): the fix is a button in
    /// the product, never a terminal command the person has to type.
    fn signed_out_message(self) -> &'static str {
        match self {
            Tool::Claude => "claude is installed but not signed in. Sign in from Settings → Agents, or from the Sign in button on the failed start.",
            Tool::Codex => "codex is installed but not signed in. Sign in from Settings → Agents, or from the Sign in button on the failed start.",
            Tool::Pi => "pi has no provider credentials. Sign in from Settings → Agents, or from the Sign in button on the failed start.",
            Tool::Git => "",
        }
    }
}

impl fmt::Display for Tool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// One doctor row (§7.7): green check + version, or a red actionable error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCheck {
    pub tool: Tool,
    pub ok: bool,
    pub version: Option<String>,
    pub error: Option<String>,
    /// EXP-409 sign-in state: `Some(false)` = installed but signed out (the
    /// check is then also `!ok`, with [`Tool::signed_out_message`] as the
    /// error but `version` kept — UIs distinguish "sign in" from "install").
    /// `None` = not applicable (git) or unknown (probe failed open).
    pub authed: Option<bool>,
    /// EXP-484: WHO is signed in on this machine — filled from the same
    /// sign-in probe the gate above runs (claude's `auth status` JSON, pi's
    /// credential files, codex's presence-only answer, enriched from the
    /// usage cache by [`crate::agent_usage::collect_if_due`]). `None` for
    /// git and for a check that never reached its auth probe.
    pub account: Option<AgentAccount>,
    /// EXP-484: whether this agent's usage windows may be fetched at all —
    /// [`ClaudeAuthStatus::usage_eligible`] for claude, `false` elsewhere
    /// (codex answers over its app-server, pi over its own credential).
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
}

impl ToolCheck {
    /// Whether this row is the installed-but-signed-out state — the one
    /// failure whose fix is a login, not an install.
    pub fn signed_out(&self) -> bool {
        self.authed == Some(false)
    }
}

/// `{ claude, codex, pi, git }` — the §7.7 report, one row per agent CLI plus
/// git (EXP-201; the old two-row `agent`/`git` shape is gone).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoctorReport {
    pub claude: ToolCheck,
    pub codex: ToolCheck,
    pub pi: ToolCheck,
    pub git: ToolCheck,
}

impl DoctorReport {
    /// The check backing `agent`.
    pub fn check_for(&self, agent: CodingAgent) -> &ToolCheck {
        match agent {
            CodingAgent::Claude => &self.claude,
            CodingAgent::Codex => &self.codex,
            CodingAgent::Pi => &self.pi,
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
    /// does a signed-out agent (EXP-409).
    pub fn installed_agents(&self) -> Vec<CodingAgent> {
        CodingAgent::ALL
            .into_iter()
            .filter(|agent| self.check_for(*agent).ok)
            .collect()
    }

    /// The agents that are installed but SIGNED OUT (EXP-409) — advertised
    /// separately so remote UIs can say "sign in on that machine" instead of
    /// pretending the binary is missing.
    pub fn unauthed_agents(&self) -> Vec<CodingAgent> {
        CodingAgent::ALL
            .into_iter()
            .filter(|agent| self.check_for(*agent).signed_out())
            .collect()
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
                accounts.insert(agent.id().to_string(), account);
            }
        }
        accounts
    }

    /// Whether the AMBIENT login of `agent` may be asked for usage windows
    /// (EXP-808) — the same judgement [`probe_profile_auth`] makes for a
    /// profile dir, made from the doctor's own check: claude needs a
    /// first-party `claude.ai` subscription
    /// ([`ClaudeAuthStatus::usage_eligible`]), codex and pi need only to be
    /// installed and signed in (they answer over their own surfaces).
    pub fn ambient_usage_eligible(&self, agent: CodingAgent) -> bool {
        let check = self.check_for(agent);
        if check.version.is_none() || check.signed_out() {
            return false;
        }
        match agent {
            CodingAgent::Claude => check.usage_eligible,
            CodingAgent::Codex | CodingAgent::Pi => true,
        }
    }

    /// EXP-792 (EXP-747 B3): [`DoctorReport::agent_accounts`] plus the
    /// device's ACCOUNT PROFILES — one probe per profile dir, so a machine
    /// with two claude logins reports both.
    ///
    /// A machine that never added a second account keeps the pre-profile
    /// payload BYTE for byte (`profiles` stays empty): only `system` exists
    /// and it is the active one, so there is nothing a profile row would say
    /// that the top-level fields do not. That is what lets old clients and
    /// old servers read this map unchanged.
    ///
    /// The top-level fields keep naming the ACTIVE profile, which is the
    /// login a run without an explicit account lands on.
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
    /// The eligibility is a by-product of the sign-in probe this pass
    /// already runs per profile dir, so the usage collector reads it here
    /// instead of spawning a second `auth status` of its own.
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
            let active = crate::agent_profiles::active_profile(data_dir, agent);
            if profiles.len() <= 1 && active == crate::agent_profiles::SYSTEM_PROFILE {
                continue;
            }
            let program = settings.path_for(agent);
            let path_env = terminal::pty::login_path();
            let mut rows = Vec::new();
            for profile in &profiles {
                let system = profile.id == crate::agent_profiles::SYSTEM_PROFILE;
                // The ambient login is the one the doctor already probed;
                // every other profile gets its own `auth status` inside its
                // config dir. An unreadable answer reads as signed OUT: a
                // profile is explicit, so silence is not "assume fine".
                let (account, eligible) = if system {
                    let eligible = self.ambient_usage_eligible(agent);
                    (base.clone(), eligible)
                } else {
                    crate::agent_profiles::profile_dir(data_dir, agent, &profile.id)
                        .and_then(|dir| {
                            probe_profile_auth(agent, program, &path_env, &dir, now)
                        })
                        .map(|probe| (probe.account, probe.usage_eligible))
                        .unwrap_or_else(|| {
                            (
                                AgentAccount {
                                    signed_in: false,
                                    checked_at: now.to_string(),
                                    ..AgentAccount::default()
                                },
                                false,
                            )
                        })
                };
                usage_eligible.insert(
                    crate::usage_cache::entry_key(agent.id(), &profile.id),
                    eligible,
                );
                rows.push(crate::agent_accounts::AgentProfileEntry {
                    id: profile.id.clone(),
                    label: Some(profile.label.clone()),
                    signed_in: account.signed_in,
                    email: account.email.clone(),
                    plan: account.plan.clone(),
                    active: profile.id == active,
                    checked_at: now.to_string(),
                    usage: None,
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
    /// windows be fetched at all. Only agents that HAVE custom profiles
    /// appear here; every other login is judged by
    /// [`DoctorReport::ambient_usage_eligible`].
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

/// How thorough a doctor pass is (EXP-755). It changes exactly ONE check:
/// pi's rpc handshake ([`probe_pi_rpc`]), the only probe that spawns a real
/// protocol conversation instead of a millisecond `--version` shell-out.
///
/// * [`DoctorDepth::Quick`] — every HOT caller: desktop launch, every
///   `prepare` (§7.1 step 0), the CLI daemon's 5-minute recheck, the account
///   status pass. An UNCHANGED pi reuses the last verdict.
/// * [`DoctorDepth::Deep`] — `exponential doctor` alone: a hand-typed
///   command can afford the handshake, and "I just reinstalled it, tell me
///   now" is exactly what it is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoctorDepth {
    Quick,
    Deep,
}

/// Run every check: each agent's resolved program
/// ([`Settings::resolved_path_for`]) — claude version-gated against
/// [`MIN_CLAUDE_VERSION`], every agent sign-in-gated (EXP-409) — and plain
/// `git` from PATH.
///
/// [`DoctorDepth::Quick`]: this is the launch/daemon path, so pi's rpc
/// handshake is only paid once per pi binary ([`run_doctor_deep`] forces it).
pub fn run_doctor(settings: &Settings) -> DoctorReport {
    run_doctor_with_depth(settings, DoctorDepth::Quick)
}

/// [`run_doctor`] with every deep probe forced (EXP-755) — `exponential
/// doctor`'s pass.
pub fn run_doctor_deep(settings: &Settings) -> DoctorReport {
    run_doctor_with_depth(settings, DoctorDepth::Deep)
}

fn run_doctor_with_depth(settings: &Settings, depth: DoctorDepth) -> DoctorReport {
    // EXP-419: a Windows installer edits the registry PATH, which a running
    // process never sees — re-read it so "Check tools" (and every later
    // spawn) finds a just-installed git/agent without an app restart.
    terminal::process::refresh_windows_path();
    let claude_program = settings.resolved_path_for(CodingAgent::Claude);
    let codex_program = settings.resolved_path_for(CodingAgent::Codex);
    let pi_program = settings.resolved_path_for(CodingAgent::Pi);
    let mut claude = check_tool(Tool::Claude, &claude_program);
    apply_version_gate(&mut claude);
    apply_auth_gate(&mut claude, &claude_program);
    let mut codex = check_tool(Tool::Codex, &codex_program);
    apply_auth_gate(&mut codex, &codex_program);
    apply_codex_acp(&mut codex);
    let mut pi = check_tool(Tool::Pi, &pi_program);
    apply_auth_gate(&mut pi, &pi_program);
    probe_pi_rpc(&mut pi, settings, depth);
    DoctorReport {
        claude,
        codex,
        pi,
        git: check_tool(Tool::Git, "git"),
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
/// and pi has its probe, so codex was the one agent an unusably old build of
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

/// The DEEP codex ACP check: the real `codex app-server --listen stdio://`
/// handshake, bounded by [`PROBE_TIMEOUT`] and killed on drop. Deliberately
/// NOT part of [`run_doctor`] (see [`apply_codex_acp`]) — `exponential
/// doctor` and the settings pane call it on demand.
pub fn probe_codex_acp(program: &str, path_env: &str) -> bool {
    crate::codex_app_server::probe(program, path_env, PROBE_TIMEOUT).is_ok()
}

/// The copy a pi build without the rpc mode gets (EXP-746).
const PI_NO_RPC_MODE_NOTE: &str = "This pi build has no rpc mode. Update pi to run coding \
sessions.";

/// EXP-755: the last `pi --mode rpc` verdict, per resolved program path.
///
/// IN-PROCESS on purpose. [`run_doctor`] takes no `data_dir` — it runs from
/// the launcher, the daemon loop and the settings pane alike — and the one
/// on-disk cache in this crate ([`crate::usage_cache`]) is a file because two
/// PROCESSES share one token budget there. Nothing is shared here: the cost
/// is a local spawn, and a fresh process paying it once is correct.
///
/// Keyed by PATH rather than held in one slot so probes of two different pi
/// binaries (a settings edit, the test suite's parallel stubs) never evict
/// each other.
static PI_RPC_CACHE: Mutex<BTreeMap<String, PiRpcVerdict>> = Mutex::new(BTreeMap::new());

/// What identifies "the same pi": the resolved program, the version it
/// printed and its mtime. A bare name whose metadata does not resolve (a
/// shim, a `pi` the OS finds on PATH) keys on path + version ALONE — an
/// in-place update of such a build keeps the old verdict until its version
/// string moves or `exponential doctor` re-probes.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PiRpcStamp {
    program: String,
    version: Option<String>,
    modified: Option<SystemTime>,
}

#[derive(Clone, Debug)]
struct PiRpcVerdict {
    stamp: PiRpcStamp,
    acp: bool,
    note: Option<String>,
}

/// EXP-746: pi's ACP readiness — a real `pi --mode rpc` handshake, bounded
/// by [`PROBE_TIMEOUT`] and killed on every exit path.
///
/// EXP-755 wraps it in two things it lacked. The program comes from the
/// CALLER's `settings` (it used to resolve a DEFAULT `Settings`, so a
/// hand-configured `pi_path` was never the binary probed — the spawn failed
/// and the check stayed green by fail-open), and a [`DoctorDepth::Quick`]
/// pass reuses the cached verdict for an unchanged pi instead of paying the
/// handshake on every launch, every prepare and every daemon recheck.
fn probe_pi_rpc(check: &mut ToolCheck, settings: &Settings, depth: DoctorDepth) {
    if !check.ok {
        check.acp = Some(false);
        check.acp_note = Some("pi is not available".to_string());
        return;
    }
    check.acp = Some(true);

    let program = settings.resolved_path_for(CodingAgent::Pi);
    let stamp = PiRpcStamp {
        modified: std::fs::metadata(&program)
            .and_then(|meta| meta.modified())
            .ok(),
        version: check.version.clone(),
        program,
    };
    if depth == DoctorDepth::Quick {
        if let Some(cached) = cached_pi_rpc(&stamp) {
            check.acp = Some(cached.acp);
            check.acp_note = cached.note;
            return;
        }
    }
    let probed = pi_rpc_handshake(&stamp.program);
    // Fail open for THIS call whatever happened (the engine's own handshake is
    // the authoritative one).
    let supported = probed.unwrap_or(true);
    let note = (!supported).then(|| PI_NO_RPC_MODE_NOTE.to_string());
    check.acp = Some(supported);
    check.acp_note = note.clone();
    // EXP-766: only a REAL verdict is worth remembering. An indeterminate
    // probe used to cache its fail-open `true` under the binary's stamp, so a
    // pi that could not be spawned once read as ready until the binary
    // changed or the process restarted.
    if probed.is_none() {
        return;
    }
    if let Ok(mut cache) = PI_RPC_CACHE.lock() {
        cache.insert(
            stamp.program.clone(),
            PiRpcVerdict {
                stamp,
                acp: supported,
                note,
            },
        );
    }
}

/// The cached verdict for exactly this pi, or `None` when the binary moved,
/// changed version or changed on disk since it was taken.
fn cached_pi_rpc(stamp: &PiRpcStamp) -> Option<PiRpcVerdict> {
    let cache = PI_RPC_CACHE.lock().ok()?;
    cache
        .get(&stamp.program)
        .filter(|verdict| &verdict.stamp == stamp)
        .cloned()
}

/// The handshake itself: spawn `<program> --mode rpc`, ask one `get_state`
/// and close stdin. `true` = this build speaks rpc.
///
/// It HAS to be a handshake: `pi --mode <anything>` parses leniently and
/// exits 0 with no output on stdin EOF, so PRESENCE proves nothing and only
/// an answered `get_state` distinguishes a build that has the rpc mode from
/// one that does not. That also rules out [`output_with_timeout`], which
/// pins `Stdio::null()` on stdin; the recipe below is the same otherwise
/// (own process group, drained pipes, killed at the deadline).
///
/// `None` = INDETERMINATE (no spawn, no stdio, a wedged child). The caller
/// still fails open on it — the engine's own handshake is the authoritative
/// one and a false negative here would refuse every coding launch on a
/// working install (EXP-773: there is no terminal transport to demote to) —
/// but a `None` is never CACHED (EXP-766), so the next pass probes again
/// instead of trusting an answer nobody gave.
fn pi_rpc_handshake(program: &str) -> Option<bool> {
    use std::io::{Read as _, Write as _};
    use std::process::Stdio;
    use wait_timeout::ChildExt as _;

    let mut cmd = background_command(program);
    cmd.env("PATH", terminal::pty::login_path())
        .args(["--mode", "rpc"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        cmd.process_group(0);
    }
    let Ok(mut child) = cmd.spawn() else {
        return None;
    };
    let Some(mut stdin) = child.stdin.take() else {
        return None;
    };
    let Some(mut stdout) = child.stdout.take() else {
        return None;
    };
    // One command, then EOF: pi's rpc loop ends with its stdin, so the child
    // reaps itself and the deadline below is only the wedged-child guard.
    let asked = stdin
        .write_all(b"{\"id\":\"1\",\"type\":\"get_state\"}\n")
        .and_then(|()| stdin.flush())
        .is_ok();
    drop(stdin);
    if !asked {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    }
    // Drain on a thread: a child that fills the pipe buffer would never exit.
    let reader = std::thread::spawn(move || {
        let mut answer = String::new();
        let _ = stdout.read_to_string(&mut answer);
        answer
    });
    let exited = matches!(child.wait_timeout(PROBE_TIMEOUT), Ok(Some(_)));
    if !exited {
        #[cfg(unix)]
        unsafe {
            libc::killpg(child.id() as i32, libc::SIGKILL);
        }
        let _ = child.kill();
        let _ = child.wait();
        return None;
    }
    Some(answered_get_state(&reader.join().unwrap_or_default()))
}

/// Did the child answer our `get_state` on its rpc stream? Line-delimited
/// JSON, so a stray log line before or after the answer is fine.
fn answered_get_state(stdout: &str) -> bool {
    stdout.lines().any(|line| {
        serde_json::from_str::<serde_json::Value>(line.trim()).is_ok_and(|value| {
            value.get("type").and_then(serde_json::Value::as_str) == Some("response")
                && value.get("command").and_then(serde_json::Value::as_str) == Some("get_state")
                && value
                    .get("success")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
        })
    })
}

/// EXP-409: stamp `authed` on a still-green agent check and flip it red when
/// the agent is provably signed out. Runs AFTER the version gate (an old
/// claude may predate `auth status`); skips already-failed checks and git.
fn apply_auth_gate(check: &mut ToolCheck, program: &str) {
    apply_auth_gate_with_path(check, program, &terminal::pty::login_path())
}

/// [`apply_auth_gate`] with the PATH injected — split out so tests can probe
/// stub binaries deterministically.
fn apply_auth_gate_with_path(check: &mut ToolCheck, program: &str, path_env: &str) {
    if !check.ok {
        return;
    }
    let now = now_iso();
    let authed = match check.tool {
        Tool::Claude => {
            let status = probe_claude_auth_status(program, path_env);
            if let Some(status) = &status {
                check.account = Some(status.account(&now));
                check.usage_eligible = status.usage_eligible();
            }
            status.map(|status| status.logged_in)
        }
        Tool::Codex => {
            let authed = probe_codex_auth(program, path_env);
            // EXP-484: `codex login status` answers presence, never WHO —
            // the email/plan arrive from the app-server probe and are
            // merged in by `agent_usage::collect_if_due`.
            check.account = Some(AgentAccount {
                signed_in: authed.unwrap_or(true),
                checked_at: now.clone(),
                ..AgentAccount::default()
            });
            authed
        }
        Tool::Pi => {
            let state = read_pi_credentials();
            check.account = Some(pi_account(
                state.auth_json.as_deref(),
                state.settings_json.as_deref(),
                state.env_credential,
                &now,
            ));
            pi_auth_state(state.auth_json.as_deref(), state.env_credential)
        }
        Tool::Git => return,
    };
    check.authed = authed;
    if authed == Some(false) {
        check.ok = false;
        check.error = Some(check.tool.signed_out_message().to_string());
    }
}

/// The full `claude auth status` answer (EXP-484). `loggedIn` is the EXP-409
/// gate; the rest names the account and decides whether its usage windows
/// may be read at all ([`Self::usage_eligible`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClaudeAuthStatus {
    pub logged_in: bool,
    /// `claude.ai` on a subscription login; `apiKey`/absent otherwise.
    pub auth_method: Option<String>,
    /// `firstParty` on a direct Anthropic account; `bedrock`/`vertex` on a
    /// cloud-provider login (no OAuth usage endpoint there).
    pub api_provider: Option<String>,
    pub email: Option<String>,
    /// `max`/`pro`/… — the caption's plan half.
    pub subscription_type: Option<String>,
}

impl ClaudeAuthStatus {
    /// Whether this login can answer the OAuth usage endpoint: a signed-in
    /// claude.ai subscription on first-party Anthropic. An API-key or
    /// Bedrock/Vertex login has no usage windows to read — never spend a
    /// request finding that out again.
    pub fn usage_eligible(&self) -> bool {
        self.logged_in
            && self.auth_method.as_deref() == Some("claude.ai")
            && self
                .api_provider
                .as_deref()
                .is_none_or(|provider| provider == "firstParty")
    }

    /// This status as the wire account row.
    pub fn account(&self, now: &str) -> AgentAccount {
        AgentAccount {
            signed_in: self.logged_in,
            email: self.logged_in.then(|| self.email.clone()).flatten(),
            plan: self
                .logged_in
                .then(|| self.subscription_type.clone())
                .flatten(),
            checked_at: now.to_string(),
            profiles: Vec::new(),
        }
    }
}

/// `claude auth status` prints local JSON with a `loggedIn` bool (verified on
/// 2.1.220; [`MIN_CLAUDE_VERSION`] builds carry it). Any spawn failure or
/// unrecognisable output fails open to `None`.
fn probe_claude_auth_status(program: &str, path_env: &str) -> Option<ClaudeAuthStatus> {
    probe_claude_auth_status_in(program, path_env, None)
}

/// [`probe_claude_auth_status`] inside one config dir — EXP-792: the
/// `(CLAUDE_CONFIG_DIR, dir)` pair of an account profile, so the answer
/// names THAT login.
fn probe_claude_auth_status_in(
    program: &str,
    path_env: &str,
    config: Option<(&str, &Path)>,
) -> Option<ClaudeAuthStatus> {
    let mut cmd = background_command(program);
    cmd.env("PATH", path_env).args(["auth", "status"]);
    if let Some((key, dir)) = config {
        cmd.env(key, dir);
    }
    let output = output_with_timeout(cmd, PROBE_TIMEOUT).ok()?;
    parse_claude_auth_status_full(&String::from_utf8_lossy(&output.stdout))
}

/// EXP-792 (EXP-747 B3): who is signed in inside ONE account profile dir of
/// `agent` — the same probes the doctor's auth gate runs, pointed at the
/// profile's config dir. `None` for pi (no profiles) and for a probe that
/// never ran. A profile is explicit, so an unreadable answer reads as
/// signed OUT here (the ambient gate fails open instead).
pub(crate) struct ProfileAuth {
    pub account: AgentAccount,
    /// Whether this login's usage windows may be fetched at all
    /// ([`ClaudeAuthStatus::usage_eligible`]; codex answers over its
    /// app-server whenever it is signed in).
    pub usage_eligible: bool,
}

pub(crate) fn probe_profile_auth(
    agent: CodingAgent,
    program: &str,
    path_env: &str,
    config_dir: &Path,
    now: &str,
) -> Option<ProfileAuth> {
    let key = crate::agent_profiles::config_env_var(agent)?;
    let config = Some((key, config_dir));
    match agent {
        CodingAgent::Claude => {
            let status = probe_claude_auth_status_in(program, path_env, config)?;
            Some(ProfileAuth {
                account: status.account(now),
                usage_eligible: status.usage_eligible(),
            })
        }
        CodingAgent::Codex => {
            let authed = probe_codex_auth_in(program, path_env, config).unwrap_or(false);
            Some(ProfileAuth {
                account: AgentAccount {
                    signed_in: authed,
                    checked_at: now.to_string(),
                    ..AgentAccount::default()
                },
                usage_eligible: authed,
            })
        }
        CodingAgent::Pi => None,
    }
}

/// Pull `loggedIn` out of `claude auth status` output, tolerating noise
/// before the JSON object.
pub fn parse_claude_auth_status(stdout: &str) -> Option<bool> {
    parse_claude_auth_status_full(stdout).map(|status| status.logged_in)
}

/// The whole `claude auth status` object (EXP-484). `loggedIn` must be a
/// real bool — everything else is optional and tolerated absent, so an
/// older/narrower answer still gates correctly and simply names nobody.
pub fn parse_claude_auth_status_full(stdout: &str) -> Option<ClaudeAuthStatus> {
    let json = &stdout[stdout.find('{')?..];
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let text = |key: &str| {
        value
            .get(key)
            .and_then(|found| found.as_str())
            .map(str::trim)
            .filter(|found| !found.is_empty())
            .map(str::to_string)
    };
    Some(ClaudeAuthStatus {
        logged_in: value.get("loggedIn")?.as_bool()?,
        auth_method: text("authMethod"),
        api_provider: text("apiProvider"),
        email: text("email"),
        subscription_type: text("subscriptionType"),
    })
}

/// `codex login status`: exit 0 = logged in; a "not logged in" answer = signed
/// out; anything else (no such subcommand on an old build) fails open.
fn probe_codex_auth(program: &str, path_env: &str) -> Option<bool> {
    probe_codex_auth_in(program, path_env, None)
}

/// [`probe_codex_auth`] inside one config dir (EXP-792: a profile's
/// `(CODEX_HOME, dir)` pair).
fn probe_codex_auth_in(program: &str, path_env: &str, config: Option<(&str, &Path)>) -> Option<bool> {
    let mut cmd = background_command(program);
    cmd.env("PATH", path_env).args(["login", "status"]);
    if let Some((key, dir)) = config {
        cmd.env(key, dir);
    }
    let output = output_with_timeout(cmd, PROBE_TIMEOUT).ok()?;
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    parse_codex_login_status(output.status.success(), &combined)
}

/// Classify `codex login status` output (split out for tests).
pub fn parse_codex_login_status(success: bool, output: &str) -> Option<bool> {
    if output.to_lowercase().contains("not logged in") {
        return Some(false);
    }
    success.then_some(true)
}

/// pi has NO login command: credentials are provider API keys, from
/// `~/.pi/agent/auth.json` (oauth/key entries per provider) or environment
/// variables. The env list mirrors the notable providers in `pi --help`;
/// a stale/invalid key is undetectable here — this only catches "no
/// credential anywhere", which is the state that hangs a session.
const PI_PROVIDER_ENV_VARS: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_OAUTH_TOKEN",
    "OPENAI_API_KEY",
    "AZURE_OPENAI_API_KEY",
    "GEMINI_API_KEY",
    "GROQ_API_KEY",
    "DEEPSEEK_API_KEY",
    "XAI_API_KEY",
    "MISTRAL_API_KEY",
    "CEREBRAS_API_KEY",
    "FIREWORKS_API_KEY",
    "TOGETHER_API_KEY",
    "OPENROUTER_API_KEY",
    "AI_GATEWAY_API_KEY",
    "ZAI_API_KEY",
    "MOONSHOT_API_KEY",
    "MINIMAX_API_KEY",
    "KIMI_API_KEY",
    "OPENCODE_API_KEY",
    "NVIDIA_API_KEY",
    "CLOUDFLARE_API_KEY",
    "AWS_BEARER_TOKEN_BEDROCK",
];

/// pi's on-disk credential state: its provider credentials, its settings
/// (for `defaultProvider`) and whether any provider API key is exported.
pub(crate) struct PiCredentials {
    pub auth_json: Option<String>,
    pub settings_json: Option<String>,
    pub env_credential: bool,
}

pub(crate) fn read_pi_credentials() -> PiCredentials {
    let env_credential = PI_PROVIDER_ENV_VARS
        .iter()
        .any(|name| std::env::var(name).is_ok_and(|value| !value.trim().is_empty()));
    let read = |file: &str| {
        dirs::home_dir()
            .map(|home| home.join(".pi").join("agent").join(file))
            .filter(|path| path.exists())
            .map(|path| std::fs::read_to_string(&path).unwrap_or_default())
    };
    PiCredentials {
        auth_json: read("auth.json"),
        // EXP-484: pi names no account — its caption is the PROVIDER it
        // would run against, which lives here and nowhere else.
        settings_json: read("settings.json"),
        env_credential,
    }
}

/// Classify pi's credential presence (split out for tests): any env key OR a
/// non-empty provider map in auth.json = signed in; a present-but-unreadable
/// auth.json fails open; nothing anywhere = signed out.
pub fn pi_auth_state(auth_json: Option<&str>, env_credential: bool) -> Option<bool> {
    if env_credential {
        return Some(true);
    }
    match auth_json {
        Some(text) => match serde_json::from_str::<serde_json::Value>(text) {
            Ok(serde_json::Value::Object(map)) => Some(!map.is_empty()),
            _ => None,
        },
        None => Some(false),
    }
}

/// Parse `major.minor.patch` off a claude version line
/// (`"2.1.207 (Claude Code)"` → `(2, 1, 207)`). Anything that is not a plain
/// three-part leading version yields `None`.
pub fn parse_claude_version(line: &str) -> Option<(u32, u32, u32)> {
    let token = line.trim().split_whitespace().next()?;
    let mut parts = token.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// EXP-758: the same parse for codex. [`parse_version_output`] has already
/// stripped the `codex-cli ` prefix, so the stored version is the bare
/// `0.144.5` triple [`parse_claude_version`] reads — named separately so the
/// two ACP floors read alike at their call sites.
pub fn parse_codex_version(line: &str) -> Option<(u32, u32, u32)> {
    parse_claude_version(line)
}

/// `<program> --version`, capturing stdout/stderr — never a shell. Resolves
/// `program` against the augmented login PATH (§6.12), matching the spawn
/// environment the agent will actually run in.
pub fn check_tool(tool: Tool, program: &str) -> ToolCheck {
    check_tool_with_path(tool, program, &terminal::pty::login_path())
}

/// [`check_tool`] with the PATH injected — split out so tests can probe a
/// stub-only directory deterministically.
fn check_tool_with_path(tool: Tool, program: &str, path_env: &str) -> ToolCheck {
    let mut cmd = background_command(program);
    cmd.env("PATH", path_env).arg("--version");
    match output_with_timeout(cmd, PROBE_TIMEOUT) {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            match parse_version_output(tool, &stdout) {
                Some(version) => ToolCheck { tool, ok: true, version: Some(version), error: None, authed: None, account: None, usage_eligible: false, acp: None, acp_note: None },
                None => ToolCheck {
                    tool,
                    ok: false,
                    version: None,
                    error: Some(format!("{program} --version produced no output")),
                    authed: None,
                    account: None,
                    usage_eligible: false,
                    acp: None,
                    acp_note: None,
                },
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let detail = first_line(&stderr)
                .map(str::to_string)
                .unwrap_or_else(|| format!("exit code {}", output.status.code().unwrap_or(-1)));
            ToolCheck {
                tool,
                ok: false,
                version: None,
                error: Some(format!("{program} --version failed: {detail}")),
                authed: None,
                account: None,
                usage_eligible: false,
                acp: None,
                acp_note: None,
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => ToolCheck {
            tool,
            ok: false,
            version: None,
            error: Some(tool.not_found_message().to_string()),
            authed: None,
            account: None,
            usage_eligible: false,
            acp: None,
            acp_note: None,
        },
        Err(err) => ToolCheck {
            tool,
            ok: false,
            version: None,
            error: Some(format!("could not run {program}: {err}")),
            authed: None,
            account: None,
            usage_eligible: false,
            acp: None,
            acp_note: None,
        },
    }
}

/// `Command::output()` with a deadline (EXP-414). On timeout the child is
/// killed and reaped and `ErrorKind::TimedOut` returns — the auth probes'
/// `.ok()?` then fails OPEN (`authed: None`), and `check_tool_with_path`
/// surfaces it as an ordinary "could not run" failure.
pub(crate) fn output_with_timeout(
    mut cmd: std::process::Command,
    timeout: Duration,
) -> std::io::Result<std::process::Output> {
    use std::io::Read as _;
    use std::process::Stdio;
    use wait_timeout::ChildExt as _;

    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    // Own process group: a wedged probe may have children of its own (a
    // shell wrapper's real binary, an updater it spawned) — killing just the
    // direct child would leave them holding the pipes.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn()?;
    // Drain the pipes on threads: a child that fills the ~64KB pipe buffer
    // would otherwise never exit and the wait below would never return.
    let mut out_pipe = child.stdout.take().expect("stdout piped above");
    let mut err_pipe = child.stderr.take().expect("stderr piped above");
    let out = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out_pipe.read_to_end(&mut buf);
        buf
    });
    let err = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = err_pipe.read_to_end(&mut buf);
        buf
    });
    match child.wait_timeout(timeout)? {
        Some(status) => Ok(std::process::Output {
            status,
            stdout: out.join().unwrap_or_default(),
            stderr: err.join().unwrap_or_default(),
        }),
        None => {
            #[cfg(unix)]
            unsafe {
                libc::killpg(child.id() as i32, libc::SIGKILL);
            }
            let _ = child.kill();
            let _ = child.wait(); // reap the direct child
            // Deliberately NOT joining the readers: a survivor outside the
            // process group could still hold a pipe; the threads exit on
            // their own once every writer is gone.
            drop(out);
            drop(err);
            Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("probe exceeded {timeout:?}"),
            ))
        }
    }
}

/// First non-empty line of `--version` output, with the tool's own noise
/// prefix stripped (`git version 2.39.5 …` → `2.39.5 …`; `codex-cli 0.46.0`
/// → `0.46.0`; claude's `1.0.35 (Claude Code)` and pi's bare semver pass
/// through).
pub fn parse_version_output(tool: Tool, stdout: &str) -> Option<String> {
    let line = first_line(stdout)?;
    let stripped = match tool {
        Tool::Git => line.strip_prefix("git version ").unwrap_or(line),
        Tool::Codex => line.strip_prefix("codex-cli ").unwrap_or(line),
        Tool::Claude | Tool::Pi => line,
    };
    let trimmed = stripped.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn first_line(text: &str) -> Option<&str> {
    text.lines().map(str::trim).find(|line| !line.is_empty())
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
        // pi prints a bare version.
        assert_eq!(
            parse_version_output(Tool::Pi, "0.80.10\n"),
            Some("0.80.10".to_string())
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
                let report = run_doctor(settings);
                let busy = report
                    .first_failure_for(CodingAgent::Claude)
                    .and_then(|check| check.error.as_deref())
                    .is_some_and(|error| error.contains("Text file busy"));
                if !busy {
                    return report;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            run_doctor(settings)
        };

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
    /// misses even a real `git`. This is what fixes codex/pi installed in
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
            Some("codex not found on PATH. Set an absolute path.")
        );
        let check = check_tool(Tool::Pi, "definitely-not-a-real-binary-exp");
        assert_eq!(
            check.error.as_deref(),
            Some("pi not found on PATH. Set an absolute path.")
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

    /// EXP-201 per-agent gating: a missing pi never blocks a claude launch;
    /// a missing git blocks EVERY launch; the presence advertisement lists
    /// exactly the usable agents.
    #[test]
    fn report_gates_per_agent_and_advertises_installed() {
        let report = DoctorReport {
            claude: green(Tool::Claude, "2.1.215 (Claude Code)"),
            codex: red(Tool::Codex),
            pi: red(Tool::Pi),
            git: green(Tool::Git, "2.45.0"),
        };
        assert_eq!(report.first_failure_for(CodingAgent::Claude), None);
        assert_eq!(
            report.first_failure_for(CodingAgent::Codex),
            Some(&report.codex)
        );
        assert_eq!(report.first_failure_for(CodingAgent::Pi), Some(&report.pi));
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

        // All three installed → all three advertised, in ALL order.
        let all = DoctorReport {
            codex: green(Tool::Codex, "0.46.0"),
            pi: green(Tool::Pi, "0.80.10"),
            ..report.clone()
        };
        assert_eq!(
            all.installed_agents(),
            vec![CodingAgent::Claude, CodingAgent::Codex, CodingAgent::Pi]
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

    /// The automation host must advertise itself or the web pickers hide it.
    /// (Moved here from `cli::commands::daemon` with the caps themselves.)
    #[test]
    fn action_caps_advertise_automations() {
        assert!(ACTION_CAPS.contains(&"automations"));
        let caps = device_caps(&advert(&["claude"]));
        assert!(caps.contains(&"automations".to_string()));
        // Nothing runnable = nothing to run an automation with.
        assert!(!device_caps(&advert(&[])).contains(&"automations".to_string()));
        // EXP-615: the same for chat — remote Chat starts gate on this cap,
        // so an agent-less machine must never advertise it.
        assert!(ACTION_CAPS.contains(&"chat"));
        assert!(caps.contains(&"chat".to_string()));
        assert!(!device_caps(&advert(&[])).contains(&"chat".to_string()));
    }

    /// EXP-484: signing IN is exactly what a machine with no runnable agent
    /// needs, so `agent-login` is a BUILD cap — it rides even when nothing
    /// is signed in.
    #[test]
    fn device_caps_include_agent_login_without_runnable_agents() {
        assert!(DEVICE_CAPS.contains(&"agent-login"));
        assert!(!ACTION_CAPS.contains(&"agent-login"));
        let signed_out = device_caps(&advert(&[]));
        assert!(signed_out.contains(&"agent-login".to_string()));
        assert!(device_caps(&advert(&["claude"])).contains(&"agent-login".to_string()));
        // EXP-765: handing the login its code is part of the same signed-out
        // story — a build cap beside `agent-login`, never an action cap.
        assert!(DEVICE_CAPS.contains(&"agent-login-code"));
        assert!(!ACTION_CAPS.contains(&"agent-login-code"));
        assert!(signed_out.contains(&"agent-login-code".to_string()));
    }

    /// EXP-792: running `mcp_oauth_*` and a forced usage refresh are
    /// properties of the BINARY — build caps, advertised while signed out,
    /// and the whole list stays under `capsInput`'s ceiling of 16.
    #[test]
    fn device_caps_include_mcp_and_usage_refresh_under_the_ceiling() {
        assert!(DEVICE_CAPS.contains(&"mcp"));
        assert!(DEVICE_CAPS.contains(&"agent-usage-refresh"));
        // FEED-36: the web's "Update now" shows only for a build that runs it.
        assert!(DEVICE_CAPS.contains(&"update-now"));
        assert!(!ACTION_CAPS.contains(&"update-now"));
        assert!(!ACTION_CAPS.contains(&"mcp"));
        let signed_out = device_caps(&advert(&[]));
        assert!(signed_out.contains(&"mcp".to_string()));
        assert!(signed_out.contains(&"agent-usage-refresh".to_string()));
        assert!(device_caps(&advert(&["claude"])).len() <= 16);
    }

    /// EXP-792 (A3): the signed-out fix is a button, never a command to type.
    #[test]
    fn signed_out_message_never_asks_for_a_terminal_command() {
        for tool in [Tool::Claude, Tool::Codex, Tool::Pi] {
            let message = tool.signed_out_message();
            assert!(message.contains("Sign in from Settings → Agents"), "{message}");
            assert!(!message.contains('`'), "{message}");
            assert!(!message.contains("terminal"), "{message}");
        }
    }

    /// EXP-679: `agent-start` asserts this build understands a start frame's
    /// `started_reason` — a PROTOCOL property of the binary, not of what it
    /// can run, so it rides with the build caps and the server may gate an
    /// agent-parented start on it.
    #[test]
    fn device_caps_include_agent_start_as_a_build_cap() {
        assert!(DEVICE_CAPS.contains(&"agent-start"));
        assert!(!ACTION_CAPS.contains(&"agent-start"));
        assert!(device_caps(&advert(&[])).contains(&"agent-start".to_string()));
        assert!(device_caps(&advert(&["claude"])).contains(&"agent-start".to_string()));
    }

    /// EXP-746: `acp` is a BUILD cap — it says this binary speaks the engine
    /// and steering v2, not that any agent on this machine is ready today
    /// (per-agent readiness is local, [`AgentAdvertisement::acp_agents`]).
    /// The whole list stays inside `devices.register`'s 16-cap ceiling.
    #[test]
    fn build_caps_include_acp() {
        assert!(DEVICE_CAPS.contains(&"acp"));
        assert!(!ACTION_CAPS.contains(&"acp"));
        assert!(device_caps(&advert(&[])).contains(&"acp".to_string()));
        assert!(device_caps(&advert(&["claude"])).len() <= 16);
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
            pi: red(Tool::Pi),
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
            pi: red(Tool::Pi),
            git: green(Tool::Git, "2.45.0"),
        };
        let settings = Settings {
            claude_model: "opus".into(),
            claude_effort: "".into(),
            claude_ultracode: true,
            claude_plan_mode: true,
            codex_model: "".into(),
            codex_effort: "high".into(),
            pi_model: "grok-4.5".into(),
            ..Settings::default()
        };
        let advert = report.agent_advertisement(&settings);
        assert_eq!(advert.agents, vec!["claude", "codex"]);
        assert_eq!(advert.default_agent, "claude");
        // Only runnable agents get a defaults entry — pi (red) has none.
        assert_eq!(
            advert.launch_defaults.keys().collect::<Vec<_>>(),
            vec!["claude", "codex"]
        );
        let claude = &advert.launch_defaults["claude"];
        assert_eq!(claude.model, "opus");
        assert_eq!(claude.effort, "");
        assert!(claude.ultracode);
        assert!(claude.plan_mode);
        // Capability masking: codex can never carry ultracode/plan, but its
        // blank model rides through.
        let codex = &advert.launch_defaults["codex"];
        assert_eq!(codex.model, "");
        assert_eq!(codex.effort, "high");
        assert!(!codex.ultracode);
        assert!(!codex.plan_mode);

        // A default agent that is NOT runnable still passes through as an id
        // (clients clamp); it simply has no launch_defaults entry.
        let settings = Settings {
            default_agent: CodingAgent::Pi,
            ..settings
        };
        let advert = report.agent_advertisement(&settings);
        assert_eq!(advert.default_agent, "pi");
        assert!(!advert.launch_defaults.contains_key("pi"));

        // A RUNNABLE pi advertises its own plan default (EXP-441) — plan
        // rides through, ultracode stays masked.
        let report = DoctorReport {
            pi: green(Tool::Pi, "0.84.1"),
            ..report
        };
        let advert = report.agent_advertisement(&settings);
        let pi = &advert.launch_defaults["pi"];
        assert_eq!(pi.model, "grok-4.5");
        assert!(pi.plan_mode, "pi_plan_mode defaults ON");
        assert!(!pi.ultracode);
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

    #[test]
    fn agent_accounts_stay_pre_profile_until_a_second_account_exists() {
        // EXP-792 (EXP-747 B5): the whole compatibility promise. One ambient
        // login = the exact payload every shipped client already decodes.
        let dir = profile_data_dir("pre");
        let mut claude = green(Tool::Claude, "2.1.215 (Claude Code)");
        claude.account = Some(AgentAccount {
            signed_in: true,
            email: Some("dev@acme.test".into()),
            plan: Some("max".into()),
            checked_at: "2026-01-01T00:00:00.000Z".into(),
            profiles: Vec::new(),
        });
        let report = DoctorReport {
            claude,
            codex: red(Tool::Codex),
            pi: red(Tool::Pi),
            git: green(Tool::Git, "2.44.0"),
        };
        let settings = Settings::default();
        let accounts =
            report.agent_accounts_with_profiles(&settings, &dir, "2026-02-02T00:00:00.000Z");
        let claude = accounts.get("claude").expect("claude row");
        assert!(claude.profiles.is_empty(), "no second account, no profile rows");
        assert_eq!(claude.email.as_deref(), Some("dev@acme.test"));
    }

    #[test]
    fn a_second_profile_lists_both_and_the_active_one_leads() {
        // The added profile has no config dir content, so its probe fails and
        // it reads signed OUT — the ambient row keeps naming the login a
        // default run lands on.
        let dir = profile_data_dir("second");
        crate::agent_profiles::create(&dir, CodingAgent::Claude, "Work")
            .expect("create profile");
        let mut claude = green(Tool::Claude, "2.1.215 (Claude Code)");
        claude.account = Some(AgentAccount {
            signed_in: true,
            email: Some("dev@acme.test".into()),
            plan: Some("max".into()),
            checked_at: "2026-01-01T00:00:00.000Z".into(),
            profiles: Vec::new(),
        });
        let report = DoctorReport {
            claude,
            codex: red(Tool::Codex),
            pi: red(Tool::Pi),
            git: green(Tool::Git, "2.44.0"),
        };
        let mut settings = Settings::default();
        // A binary that cannot exist: the profile probe must fail CLOSED.
        settings.claude_path = "/nonexistent/exp792-claude".to_string();
        let accounts =
            report.agent_accounts_with_profiles(&settings, &dir, "2026-02-02T00:00:00.000Z");
        let claude = accounts.get("claude").expect("claude row");
        assert_eq!(claude.profiles.len(), 2, "system + the added profile");
        let system = &claude.profiles[0];
        assert_eq!(system.id, crate::agent_profiles::SYSTEM_PROFILE);
        assert!(system.active, "the ambient login is the default account");
        assert!(system.signed_in);
        let work = &claude.profiles[1];
        assert_eq!(work.label.as_deref(), Some("Work"));
        assert!(!work.active);
        assert!(!work.signed_in, "an unreadable profile probe reads signed out");
        assert_eq!(claude.email.as_deref(), Some("dev@acme.test"));
    }

    #[test]
    fn agent_accounts_cover_installed_agents_only() {
        let mut claude = green(Tool::Claude, "2.1.215 (Claude Code)");
        claude.account = Some(AgentAccount {
            signed_in: true,
            email: Some("dev@acme.test".into()),
            plan: Some("max".into()),
            checked_at: "2026-01-01T00:00:00.000Z".into(),
            profiles: Vec::new(),
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
            // Not installed: no probe ran, so no row at all.
            pi: red(Tool::Pi),
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

    /// EXP-409: pi credential presence — env key or a non-empty auth.json
    /// map counts; unreadable auth.json fails open; nothing = signed out.
    #[test]
    fn pi_auth_state_classifies_credentials() {
        assert_eq!(pi_auth_state(None, true), Some(true));
        assert_eq!(pi_auth_state(Some("{\"openai-codex\": {\"type\": \"oauth\"}}"), false), Some(true));
        assert_eq!(pi_auth_state(Some("{}"), false), Some(false));
        assert_eq!(pi_auth_state(None, false), Some(false));
        // Present but unparseable → fail open.
        assert_eq!(pi_auth_state(Some("not json"), false), None);
        assert_eq!(pi_auth_state(Some("[]"), false), None);
    }

    /// EXP-409 end-to-end against stub binaries: a signed-out claude/codex
    /// flips red with the sign-in copy (version kept), a signed-in one stays
    /// green with `authed: Some(true)`, and the report's gates treat
    /// signed-out as not installed while `unauthed_agents` still names it.
    #[cfg(unix)]
    #[test]
    fn auth_gate_flips_signed_out_agents() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-coding-doctor-auth-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let write_stub = |name: &str, body: &str| {
            let path = dir.join(name);
            fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            path
        };

        // Retry the transient ETXTBSY race (see run_doctor_gates_on_the_stub_version).
        let checked = |tool: Tool, program: &std::path::Path| {
            let program = program.to_string_lossy();
            for _ in 0..20 {
                let mut check = check_tool(tool, &program);
                apply_auth_gate_with_path(&mut check, &program, &terminal::pty::login_path());
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

        let claude_in = write_stub(
            "claude-in",
            "case \"$1\" in\n--version) echo '9.9.9 (Claude Code)';;\nauth) echo '{\"loggedIn\": true}';;\nesac",
        );
        let check = checked(Tool::Claude, &claude_in);
        assert!(check.ok, "{:?}", check.error);
        assert_eq!(check.authed, Some(true));

        let codex_out = write_stub(
            "codex-out",
            "case \"$1\" in\n--version) echo 'codex-cli 0.46.0';;\nlogin) echo 'Not logged in' >&2; exit 1;;\nesac",
        );
        let check = checked(Tool::Codex, &codex_out);
        assert!(!check.ok);
        assert!(check.signed_out());
        assert_eq!(
            check.error.as_deref(),
            Some("codex is installed but not signed in. Sign in from Settings → Agents, or from the Sign in button on the failed start.")
        );

        // The report-level view: signed-out codex is out of installed_agents
        // but named by unauthed_agents; the affordance gate ignores it.
        let report = DoctorReport {
            claude: checked(Tool::Claude, &claude_in),
            codex: checked(Tool::Codex, &codex_out),
            pi: red(Tool::Pi),
            git: green(Tool::Git, "2.45.0"),
        };
        assert_eq!(report.installed_agents(), vec![CodingAgent::Claude]);
        assert_eq!(report.unauthed_agents(), vec![CodingAgent::Codex]);
        assert!(report.any_agent_ok());
        assert!(report.first_failure_for(CodingAgent::Codex).is_some());

        let _ = fs::remove_dir_all(&dir);
    }

    /// EXP-746: a `get_state` answer only counts when it is a SUCCESSFUL
    /// response to that very command — anything else leaves the build
    /// unproven (and, from the probe, marked not supported).
    #[test]
    fn answered_get_state_needs_a_successful_get_state_response() {
        assert!(answered_get_state(
            "{\"id\":\"1\",\"type\":\"response\",\"command\":\"get_state\",\"success\":true}"
        ));
        // Line-delimited: log noise around the answer is fine.
        assert!(answered_get_state(
            "starting pi\n{\"id\":\"1\",\"type\":\"response\",\"command\":\"get_state\",\"success\":true}\nbye"
        ));
        for answer in [
            "",
            "not json",
            // The right command, but it failed.
            "{\"type\":\"response\",\"command\":\"get_state\",\"success\":false}",
            // Success, but a different command.
            "{\"type\":\"response\",\"command\":\"ping\",\"success\":true}",
            // An event, not a response.
            "{\"type\":\"event\",\"command\":\"get_state\",\"success\":true}",
            // No success flag at all.
            "{\"type\":\"response\",\"command\":\"get_state\"}",
        ] {
            assert!(!answered_get_state(answer), "{answer}");
        }
    }

    /// A stub `pi` that counts its rpc invocations: `--version` prints a
    /// version, every other invocation reads one line off stdin, appends to
    /// a log and (with `rpc`) answers the `get_state`.
    #[cfg(unix)]
    struct PiStub {
        dir: std::path::PathBuf,
        path: std::path::PathBuf,
        log: std::path::PathBuf,
    }

    #[cfg(unix)]
    impl PiStub {
        fn new(tag: &str, rpc: bool) -> Self {
            use std::fs;
            use std::os::unix::fs::PermissionsExt;

            let mut dir = std::env::temp_dir();
            dir.push(format!(
                "exp-coding-doctor-pi-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&dir).unwrap();
            let log = dir.join("runs.log");
            let path = dir.join("pi");
            let answer = match rpc {
                true => "echo '{\"id\":\"1\",\"type\":\"response\",\"command\":\"get_state\",\"success\":true}'",
                // A build WITHOUT the rpc mode: it parses the flag, says
                // nothing on stdout and exits 0 (the real degradation).
                false => "echo 'pi: unknown mode' >&2",
            };
            fs::write(
                &path,
                format!(
                    "#!/bin/sh\ncase \"$1\" in\n--version) echo '0.80.10';;\n*) read line\necho run >> '{}'\n{answer};;\nesac\n",
                    log.display()
                ),
            )
            .unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            Self { dir, path, log }
        }

        fn settings(&self) -> Settings {
            Settings {
                pi_path: self.path.to_string_lossy().into_owned(),
                ..Settings::default()
            }
        }

        /// How many rpc handshakes actually reached the binary.
        fn runs(&self) -> usize {
            std::fs::read_to_string(&self.log)
                .map(|text| text.lines().filter(|line| !line.trim().is_empty()).count())
                .unwrap_or(0)
        }

        /// Forget this stub's cached verdict (test-only, see below).
        fn forget(&self) {
            PI_RPC_CACHE
                .lock()
                .unwrap()
                .remove(&self.path.to_string_lossy().into_owned());
        }

        /// Move the binary's mtime forward — an in-place `pi` update.
        fn bump_mtime(&self, secs: i64) {
            let modified = std::fs::metadata(&self.path)
                .unwrap()
                .modified()
                .unwrap()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap();
            let when = libc::timeval {
                tv_sec: modified.as_secs() as libc::time_t + secs as libc::time_t,
                tv_usec: 0,
            };
            let times = [when, when];
            let path = std::ffi::CString::new(self.path.to_string_lossy().as_bytes()).unwrap();
            assert_eq!(unsafe { libc::utimes(path.as_ptr(), times.as_ptr()) }, 0);
        }
    }

    #[cfg(unix)]
    impl Drop for PiStub {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// One probe that DID reach the stub. Exec'ing a just-written script can
    /// hit ETXTBSY while a concurrent test's fork holds the write fd; that
    /// spawn failure fails OPEN and caches a verdict the assertions must not
    /// read, so drop it and retry (see run_doctor_gates_on_the_stub_version).
    #[cfg(unix)]
    fn probe_expecting_a_run(stub: &PiStub, depth: DoctorDepth) -> ToolCheck {
        let before = stub.runs();
        let settings = stub.settings();
        for _ in 0..20 {
            let mut check = green(Tool::Pi, "0.80.10");
            probe_pi_rpc(&mut check, &settings, depth);
            if stub.runs() > before {
                return check;
            }
            stub.forget();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("pi stub never ran");
    }

    /// EXP-755: the probe uses the CALLER's configured `pi_path` (it used to
    /// resolve a default `Settings`, so a hand-configured pi was never the
    /// binary probed), and a second QUICK pass over the unchanged binary
    /// reuses the verdict instead of paying the handshake again — this runs
    /// on every launch, every prepare and the daemon's 5-minute recheck.
    #[cfg(unix)]
    #[test]
    fn probe_pi_rpc_uses_the_configured_path_and_reuses_the_verdict_per_stamp() {
        let stub = PiStub::new("reuse", true);
        let check = probe_expecting_a_run(&stub, DoctorDepth::Quick);
        assert_eq!(check.acp, Some(true), "{:?}", check.acp_note);
        assert_eq!(check.acp_note, None);
        assert_eq!(stub.runs(), 1, "the configured path must be the one probed");

        let mut again = green(Tool::Pi, "0.80.10");
        probe_pi_rpc(&mut again, &stub.settings(), DoctorDepth::Quick);
        assert_eq!(again.acp, Some(true));
        assert_eq!(stub.runs(), 1, "an unchanged pi is never re-probed");
    }

    /// The stamp covers the binary's mtime: an in-place `pi` update re-probes
    /// on the very next quick pass, so a build that GAINED the rpc mode is
    /// picked up without an app restart.
    #[cfg(unix)]
    #[test]
    fn a_touched_pi_binary_re_probes() {
        let stub = PiStub::new("touched", true);
        probe_expecting_a_run(&stub, DoctorDepth::Quick);
        assert_eq!(stub.runs(), 1);

        stub.bump_mtime(2);
        let check = probe_expecting_a_run(&stub, DoctorDepth::Quick);
        assert_eq!(check.acp, Some(true));
        assert_eq!(stub.runs(), 2, "a changed binary invalidates its verdict");
    }

    /// `exponential doctor` re-runs the handshake even when the stamp still
    /// matches — the command exists to answer "is it ready NOW".
    #[cfg(unix)]
    #[test]
    fn a_deep_doctor_re_probes_the_same_stamp() {
        let stub = PiStub::new("deep", true);
        probe_expecting_a_run(&stub, DoctorDepth::Quick);
        assert_eq!(stub.runs(), 1);

        let check = probe_expecting_a_run(&stub, DoctorDepth::Deep);
        assert_eq!(check.acp, Some(true));
        assert_eq!(stub.runs(), 2, "a deep pass ignores the cache");
    }

    /// A pi that answers nothing on its rpc stream is marked not supported,
    /// with the note the doctor rows and the session screen render — and the
    /// verdict caches like any other (the note rides the cache too).
    #[cfg(unix)]
    #[test]
    fn a_pi_without_rpc_mode_is_marked_not_supported() {
        let stub = PiStub::new("norpc", false);
        let check = probe_expecting_a_run(&stub, DoctorDepth::Quick);
        assert_eq!(check.acp, Some(false));
        assert_eq!(check.acp_note.as_deref(), Some(PI_NO_RPC_MODE_NOTE));
        assert!(check
            .acp_note
            .as_deref()
            .is_some_and(|note| note.contains("no rpc mode")
                && note.contains("Update pi")));

        let mut again = green(Tool::Pi, "0.80.10");
        probe_pi_rpc(&mut again, &stub.settings(), DoctorDepth::Quick);
        assert_eq!(again.acp, Some(false));
        assert_eq!(again.acp_note.as_deref(), Some(PI_NO_RPC_MODE_NOTE));
        assert_eq!(stub.runs(), 1);
    }

    /// A pi that is not installed at all never reaches the handshake — the
    /// row is red already, and the note says so.
    #[test]
    fn a_missing_pi_is_not_probed_at_all() {
        let mut check = red(Tool::Pi);
        probe_pi_rpc(&mut check, &Settings::default(), DoctorDepth::Deep);
        assert_eq!(check.acp, Some(false));
        assert_eq!(check.acp_note.as_deref(), Some("pi is not available"));
    }

    /// EXP-766: an INDETERMINATE probe still fails open for that one call, but
    /// it is never cached. Caching it pinned "ready" onto a pi that could not
    /// even be spawned, for the rest of the process.
    #[test]
    fn an_indeterminate_probe_is_not_cached() {
        let program = std::env::temp_dir()
            .join(format!("exp766-pi-missing-{}", std::process::id()))
            .to_string_lossy()
            .into_owned();
        let settings = Settings {
            pi_path: program.clone(),
            ..Settings::default()
        };
        let mut check = green(Tool::Pi, "0.80.10");
        probe_pi_rpc(&mut check, &settings, DoctorDepth::Quick);
        // Fail-open is unchanged: the engine's handshake decides.
        assert_eq!(check.acp, Some(true));
        assert_eq!(check.acp_note, None);
        assert!(
            PI_RPC_CACHE.lock().unwrap().get(&program).is_none(),
            "an answer nobody gave is not a verdict"
        );
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
}
