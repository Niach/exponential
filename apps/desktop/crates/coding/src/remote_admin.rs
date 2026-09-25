//! EXP-481: device-side execution of the server-authoritative device state —
//! applying `launch_defaults` patches onto the local [`Settings`], and the
//! `worktree_remove` / `worktree_prune` command bodies both binaries (desktop
//! + CLI daemon) run off the heartbeat's command pickup.
//!
//! EXP-1020: NOTHING in this build emits `worktree_remove` /
//! `worktree_prune` any more — the worktrees section left the device
//! settings on all four clients, and Settings → Worktrees cleans locally
//! instead. The EXECUTORS below stay because an installed client BELOW the
//! version floor still shows that section and still queues those commands;
//! deleting the executor would make a button those builds already ship fail.
//! EXP-1060 retires the kinds once `CLIENT_MIN_VERSION_{IOS,ANDROID,DESKTOP,
//! CLI}` all pass this release.
//!
//! All blocking; callers background-execute. Refusal messages travel
//! verbatim in `devices.completeCommand`, so they are written for the
//! issuing UI, not for logs.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::agent::CodingAgent;
use crate::git_worktree::{list_worktrees, run_git};
use crate::prune::{worktree_dirty_state, DirtyState, PrunePolicy};
use crate::settings::Settings;

// ---------------------------------------------------------------------------
// Launch-defaults patches (the devices row's `launch_defaults` jsonb)
// ---------------------------------------------------------------------------

/// One agent's entry in a defaults patch. Only PRESENT fields apply.
///
/// Every `None` is OMITTED on the wire, never an explicit `null` — the
/// server's zod schema rejected nulls with a 400 that silently killed
/// `devices.register` on 0.14.10 daemons (EXP-495). Deserialization still
/// accepts both absent and null.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct AgentDefaultsPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// EXP-981: the model this agent's SUBAGENTS run on; claude-only, so it
    /// is OMITTED for every other agent (the capability mask below).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagent_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ultracode: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_mode: Option<bool>,    /// EXP-1082: `Settings.auto_rotate_accounts` — claude-only, so it is
    /// OMITTED for every other agent and applied only from claude's entry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_rotate_accounts: Option<bool>,
}

/// The wire form of the devices row's `launch_defaults` column — the SAME
/// camelCase shape `devices.setLaunchDefaults` accepts and the heartbeat
/// returns. `BTreeMap` keeps serialization deterministic (the fingerprint in
/// [`crate::launch_defaults_sync`] hashes the canonical JSON).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct DefaultsPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_agent: Option<String>,
    /// EXP-872: the profile id of `default_agent`'s logins the machine
    /// launches as by default — "default agent" became "default account", so
    /// the PAIR travels together. Beside a `default_agent`, absent = the
    /// ambient login, so applying such a patch CLEARS a stored pick (the
    /// server carries the pin forward for clients that never send the key,
    /// so its copy is authoritative for the pair); a blank value clears too.
    /// Absent with no `default_agent` either says nothing and leaves it alone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_account: Option<String>,
    pub agents: BTreeMap<String, AgentDefaultsPatch>,
    /// EXP-1029/EXP-1020: the WORKFLOW model pair new workflows started on
    /// this machine are seeded from (`DeviceWorkflowDefaults`). Absent from
    /// a machine that predates it — the server then carries its stored copy
    /// forward rather than wiping it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow: Option<WorkflowDefaultsPatch>,
}

/// The `launch_defaults.workflow` object: the cheap model (leaf nodes and
/// the subagents inside them) and the strong one (contract, integration and
/// `risk: high` nodes, and every agent review). A HALF pair seeds nothing,
/// so both names ride together or neither does.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", default)]
pub struct WorkflowDefaultsPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strong_model: Option<String>,
}

/// Apply `patch` onto `settings`, FIELD-wise and ignore-invalid: a value
/// outside the agent's closed vocabulary (or a toggle the agent doesn't
/// support) is dropped without touching the field — a patch must never
/// RESET what it didn't validly set (unlike `normalize_choice`'s load-time
/// fallback). Returns whether anything changed. Persisting stays the
/// caller's `Settings::save` (merge-preserving).
pub fn apply_defaults_patch(settings: &mut Settings, patch: &DefaultsPatch) -> bool {
    fn set_string(slot: &mut String, value: &str, changed: &mut bool) {
        if slot != value {
            *slot = value.to_string();
            *changed = true;
        }
    }
    fn set_bool(slot: &mut bool, value: bool, changed: &mut bool) {
        if *slot != value {
            *slot = value;
            *changed = true;
        }
    }
    let mut changed = false;
    let patch_agent = patch.default_agent.as_deref().and_then(CodingAgent::parse);
    if let Some(agent) = patch_agent {
        if settings.default_agent != agent {
            settings.default_agent = agent;
            changed = true;
        }
    }
    // EXP-872: the account is one of the default agent's logins, so the PAIR
    // is what a patch names. A present value sets it (blank = clear). An
    // ABSENT one beside a valid default agent is the cleared state too: the
    // server can never deliver a blank (zod `min(1)`, null-free jsonb) and
    // carries the stored pin forward for key-less older clients, so "agent,
    // no account" in its copy means the ambient login, whether the agent
    // stayed or switched. Only a patch naming NEITHER leaves the pin alone.
    let next_account = match patch.default_account.as_deref() {
        Some(account) => Some((!account.trim().is_empty()).then(|| account.trim().to_string())),
        None => patch_agent.map(|_| None),
    };
    if let Some(next) = next_account {
        if settings.default_account != next {
            settings.default_account = next;
            changed = true;
        }
    }
    // EXP-1020: the workflow pair, each half clamped to the DEFAULT AGENT's
    // vocabulary — the same rule `Settings::load` applies, and the same one
    // web's `workflowDefaultsFor` applies. It runs AFTER `default_agent`
    // above, so a patch that switches the agent and the pair together is
    // judged against the new agent. Clamping against claude's aliases alone
    // silently dropped a codex pair set on web, which the next
    // `defaults_wire` push then overwrote with opus/fable.
    if let Some(workflow) = &patch.workflow {
        let vocabulary: &[&str] = match settings.default_agent {
            CodingAgent::Claude => &crate::settings::MODEL_ALIASES,
            CodingAgent::Codex => &crate::agent::CODEX_MODELS,
        };
        for (value, slot) in [
            (&workflow.model, &mut settings.workflow_model),
            (&workflow.strong_model, &mut settings.workflow_strong_model),
        ] {
            if let Some(value) = value {
                if vocabulary.contains(&value.as_str()) {
                    set_string(slot, value, &mut changed);
                }
            }
        }
    }
    for (agent_id, entry) in &patch.agents {
        let Some(agent) = CodingAgent::parse(agent_id) else {
            continue;
        };
        if let Some(model) = &entry.model {
            let valid = agent.model_values().contains(&model.as_str())
                || (model.is_empty() && agent.allows_blank_model());
            if valid {
                let slot = match agent {
                    CodingAgent::Claude => &mut settings.claude_model,
                    CodingAgent::Codex => &mut settings.codex_model,
                };
                set_string(slot, model, &mut changed);
            }
        }
        if let Some(effort) = &entry.effort {
            if effort.is_empty() || agent.effort_values().contains(&effort.as_str()) {
                let slot = match agent {
                    CodingAgent::Claude => &mut settings.claude_effort,
                    CodingAgent::Codex => &mut settings.codex_effort,
                };
                set_string(slot, effort, &mut changed);
            }
        }
        if let Some(subagent_model) = &entry.subagent_model {
            // EXP-981: blank is the valid "the CLI's own default" value.
            let valid = subagent_model.is_empty()
                || agent.model_values().contains(&subagent_model.as_str());
            if valid && agent.supports_subagent_model() {
                set_string(
                    &mut settings.claude_subagent_model,
                    subagent_model,
                    &mut changed,
                );
            }
        }
        if let Some(ultracode) = entry.ultracode {
            if agent.supports_ultracode() {
                set_bool(&mut settings.claude_ultracode, ultracode, &mut changed);
            }
        }
        if let Some(plan_mode) = entry.plan_mode {
            match agent {
                CodingAgent::Claude => {
                    set_bool(&mut settings.claude_plan_mode, plan_mode, &mut changed)
                }
                CodingAgent::Codex => {}
            }
        }
        if let Some(auto_rotate) = entry.auto_rotate_accounts {
            if agent == CodingAgent::Claude {
                set_bool(&mut settings.auto_rotate_accounts, auto_rotate, &mut changed);
            }
        }
    }
    changed
}

/// EXP-1020: overlay every LAUNCH-DEFAULT field of `from` onto `onto`, and
/// nothing else. THE definition both settings surfaces lean on — the Agents
/// pane (which adds the two CLI paths it also owns) and the Device settings
/// dialog's own-device branch — so the two can never disagree about what a
/// launch default is.
///
/// It exists because the dialog hand-copied the list and a field added later
/// (`claude_subagent_model`, the EXP-1029 workflow pair) was then saved
/// everywhere EXCEPT on this machine's own row: the hub reseeded the selects
/// from the stale file and `launch_defaults_sync`'s PushLocal later shipped
/// the stale copy back over the server's. One function, one test
/// ([`overlay_carries_every_launch_default`]), no list to forget.
///
/// NOT copied: the CLI paths, `repos_root`, `branch_prefix`, the terminal
/// shell and the UI-state fields — those belong to other panes, and a
/// launch-defaults save must never roll one of them back.
pub fn overlay_launch_defaults(onto: &mut Settings, from: &Settings) {
    onto.default_agent = from.default_agent;
    onto.default_account = from.default_account.clone();
    onto.claude_model = from.claude_model.clone();
    onto.claude_effort = from.claude_effort.clone();
    onto.claude_subagent_model = from.claude_subagent_model.clone();
    onto.workflow_model = from.workflow_model.clone();
    onto.workflow_strong_model = from.workflow_strong_model.clone();
    onto.codex_model = from.codex_model.clone();
    onto.codex_effort = from.codex_effort.clone();
    onto.claude_ultracode = from.claude_ultracode;
    onto.claude_plan_mode = from.claude_plan_mode;
    onto.auto_rotate_accounts = from.auto_rotate_accounts;
}

/// The PUSH direction: this machine's launch defaults as the full wire
/// object. Covers ALL agents (the server stores configuration even for a
/// not-currently-installed agent — it applies the day the CLI lands),
/// unlike the doctor advertisement's runnable-only map.
pub fn defaults_wire(settings: &Settings) -> DefaultsPatch {
    let mut agents = BTreeMap::new();
    for agent in CodingAgent::ALL {
        agents.insert(
            agent.id().to_string(),
            AgentDefaultsPatch {
                model: Some(settings.model_for(agent).to_string()),
                effort: Some(settings.effort_for(agent).to_string()),
                subagent_model: agent
                    .supports_subagent_model()
                    .then(|| settings.subagent_model_for(agent).to_string()),
                ultracode: agent
                    .supports_ultracode()
                    .then_some(settings.claude_ultracode),
                plan_mode: agent
                    .supports_plan_mode()
                    .then_some(settings.plan_mode_for(agent)),
                auto_rotate_accounts: (agent == CodingAgent::Claude)
                    .then_some(settings.auto_rotate_accounts),
            },
        );
    }
    DefaultsPatch {
        default_agent: Some(settings.default_agent.id().to_string()),
        default_account: settings.default_account.clone(),
        agents,
        workflow: Some(WorkflowDefaultsPatch {
            model: Some(settings.workflow_model.clone()),
            strong_model: Some(settings.workflow_strong_model.clone()),
        }),
    }
}

// ---------------------------------------------------------------------------
// Remote worktree removal
// ---------------------------------------------------------------------------

/// Why a remote `worktree_remove` refused. `message()` strings travel in
/// `completeCommand` — user-facing.
#[derive(Debug, PartialEq, Eq)]
pub enum RemoveWorktreeError {
    /// A live local session holds the branch.
    BranchHeld,
    /// Modified/staged tracked files — real work, never removed remotely.
    TrackedChanges,
    /// A coding launch holds the clone's gate — transient, retry.
    GateBusy,
    /// No worktree of the clone is on that branch (already gone).
    NotFound,
    Git(String),
}

impl RemoveWorktreeError {
    pub fn message(&self) -> String {
        match self {
            Self::BranchHeld => "A live session is using this worktree.".to_string(),
            Self::TrackedChanges => {
                "It has uncommitted changes — remove it on the machine itself.".to_string()
            }
            Self::GateBusy => "A session is being launched — try again in a moment.".to_string(),
            Self::NotFound => "That worktree is already gone.".to_string(),
            Self::Git(detail) => detail.clone(),
        }
    }
}

/// Remove `branch`'s worktree of `clone`. Refuses held branches and tracked
/// changes; untracked-only debris rides `--force` (the prune's EXP-465
/// stance). The branch itself is KEPT — parity with the local
/// Settings → Local repositories removal, and the next Start-coding reuses
/// it. Runs inside the clone's launch gate so it can never race a launch
/// mid-`worktree add`.
pub fn remove_worktree_remote(
    clone: &Path,
    branch: &str,
    held: &HashSet<String>,
) -> Result<(), RemoveWorktreeError> {
    if held.contains(branch) {
        return Err(RemoveWorktreeError::BranchHeld);
    }
    let result = crate::launch_gate::try_exclusive(clone, || {
        let entries = list_worktrees(clone)
            .map_err(|err| RemoveWorktreeError::Git(err.detail.clone()))?;
        let target = entries
            .iter()
            .skip(1)
            .find(|entry| entry.branch.as_deref() == Some(branch) && entry.path != *clone)
            .ok_or(RemoveWorktreeError::NotFound)?;
        let path = target.path.to_string_lossy().into_owned();
        let args: Vec<&str> = match worktree_dirty_state(&target.path) {
            DirtyState::TrackedChanges => return Err(RemoveWorktreeError::TrackedChanges),
            DirtyState::Clean => vec!["worktree", "remove", &path],
            DirtyState::UntrackedOnly => vec!["worktree", "remove", "--force", &path],
        };
        run_git(Some(clone), &args, None, &format!("git worktree remove ({branch})"))
            .map(|_| ())
            .map_err(|err| RemoveWorktreeError::Git(err.detail))
    });
    result.ok_or(RemoveWorktreeError::GateBusy)?
}

/// The CLI's (and remote-command) prune policy: GIT-TRUTH ONLY — no synced
/// issue facts (the daemon runs no sync engine), so `merged`/`finished`
/// stay empty and prefix + landed checks decide. Live sessions ride
/// `keep`/`busy_paths`; unlanded commits are `NotLanded`-protected; fresh
/// launches are shielded by the prune's `LAUNCH_GRACE`. Residual (accepted
/// v1): a >grace-old 0-commits-ahead worktree of a still-open issue is
/// removed — it recreates on the next start, and the agent's conversation
/// store survives outside the worktree.
pub fn conservative_prune_policy(
    branch_prefix: &str,
    keep: HashSet<String>,
    busy_paths: Vec<PathBuf>,
    run_registry_dir: Option<PathBuf>,
) -> PrunePolicy {
    let mut prefixes = vec![
        "exp/batch-".to_string(),
        // EXP-637: chat runs live under their own lowercase namespace, which
        // a custom user prefix would otherwise leave unswept.
        crate::batch_launcher::CHAT_BRANCH_PREFIX.to_string(),
    ];
    if !branch_prefix.is_empty() && !prefixes.iter().any(|p| p == branch_prefix) {
        prefixes.push(branch_prefix.to_string());
    }
    PrunePolicy {
        prefixes,
        // Resolved per clone via `effective_default_branch` (origin/HEAD) —
        // never a fabricated `main`.
        default_branch: None,
        keep,
        busy_paths,
        merged: HashSet::new(),
        finished: HashSet::new(),
        delete_stale_branches: true,
        run_registry_dir,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_worktree::{create_worktree, TokenUrl};
    use std::fs;
    use std::process::Command;

    struct TempDir(PathBuf);

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn temp_dir(tag: &str) -> TempDir {
        let mut path = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("exp-remote-admin-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn git(cwd: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    fn seed(dir: &Path) -> PathBuf {
        let origin = dir.join("origin-src");
        fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--quiet", "-b", "main"]);
        fs::write(origin.join("README.md"), "seed\n").unwrap();
        git(&origin, &["add", "."]);
        git(&origin, &["commit", "--quiet", "-m", "seed"]);
        let clone = dir.join("clone");
        git(dir, &["clone", "--quiet", origin.to_str().unwrap(), clone.to_str().unwrap()]);
        clone
    }

    fn worktree(clone: &Path, branch: &str) -> PathBuf {
        create_worktree(clone, branch, "origin/main", &TokenUrl::new("acme/web", "ghs_dead"))
            .unwrap()
    }

    // -- apply_defaults_patch --------------------------------------------------

    #[test]
    fn patch_applies_valid_fields_and_ignores_invalid_field_wise() {
        let mut settings = Settings::default();
        let patch: DefaultsPatch = serde_json::from_value(serde_json::json!({
            "defaultAgent": "codex",
            "agents": {
                "claude": { "model": "opus", "ultracode": true },
                // Invalid model must NOT reset the field; the valid toggle
                // beside it still applies. EXP-690: `skipPermissions` is a
                // retired key an old server copy may still carry — it must
                // deserialize and be ignored, never fail the whole patch.
                "codex": { "model": "not-a-model", "skipPermissions": true, "ultracode": true },
                "cursor": { "model": "opus" },
            }
        }))
        .unwrap();
        assert!(apply_defaults_patch(&mut settings, &patch));
        assert_eq!(settings.default_agent, CodingAgent::Codex);
        assert_eq!(settings.claude_model, "opus");
        assert!(settings.claude_ultracode);
        assert_eq!(settings.codex_model, "", "invalid model left untouched");
        // ultracode is claude-only — the codex entry's true was masked.
        // (claude's own entry set it; reset and re-check the mask alone.)
        let mut fresh = Settings::default();
        let codex_only: DefaultsPatch = serde_json::from_value(serde_json::json!({
            "agents": { "codex": { "ultracode": true, "planMode": true } }
        }))
        .unwrap();
        assert!(!apply_defaults_patch(&mut fresh, &codex_only));
        assert!(!fresh.claude_ultracode);
        assert!(fresh.claude_plan_mode, "codex planMode never lands anywhere");
    }

    #[test]
    fn auto_rotate_accounts_is_claude_only() {
        // EXP-1082: the wire carries it on claude's entry alone …
        let wire = serde_json::to_value(defaults_wire(&Settings::default())).unwrap();
        assert_eq!(wire["agents"]["claude"]["autoRotateAccounts"], true);
        assert!(wire["agents"]["codex"].get("autoRotateAccounts").is_none());
        // … a claude patch flips it, a codex one never lands.
        let mut settings = Settings::default();
        let codex_only: DefaultsPatch = serde_json::from_value(serde_json::json!({
            "agents": { "codex": { "autoRotateAccounts": false } }
        }))
        .unwrap();
        assert!(!apply_defaults_patch(&mut settings, &codex_only));
        assert!(settings.auto_rotate_accounts);
        let claude: DefaultsPatch = serde_json::from_value(serde_json::json!({
            "agents": { "claude": { "autoRotateAccounts": false } }
        }))
        .unwrap();
        assert!(apply_defaults_patch(&mut settings, &claude));
        assert!(!settings.auto_rotate_accounts);
    }

    #[test]
    fn blank_model_valid_for_codex_pi_only() {
        let mut settings = Settings::default();
        settings.codex_model = "gpt-5.6-sol".into();
        let patch: DefaultsPatch = serde_json::from_value(serde_json::json!({
            "agents": {
                "codex": { "model": "" },
                "claude": { "model": "" },
            }
        }))
        .unwrap();
        assert!(apply_defaults_patch(&mut settings, &patch));
        assert_eq!(settings.codex_model, "", "blank = CLI default for codex");
        assert_eq!(settings.claude_model, "fable", "claude is explicit-always");
    }

    fn pinned(agent: CodingAgent, account: &str) -> Settings {
        let mut settings = Settings::default();
        settings.default_agent = agent;
        settings.default_account = Some(account.to_string());
        settings
    }

    fn patch(value: serde_json::Value) -> DefaultsPatch {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn an_agent_without_an_account_clears_the_local_pin() {
        // The server's cleared state: same agent, no `defaultAccount` key.
        let mut settings = pinned(CodingAgent::Claude, "0a1b2c3d");
        assert!(apply_defaults_patch(
            &mut settings,
            &patch(serde_json::json!({ "defaultAgent": "claude" }))
        ));
        assert_eq!(settings.default_account, None);
        // Already clear: nothing changed.
        assert!(!apply_defaults_patch(
            &mut settings,
            &patch(serde_json::json!({ "defaultAgent": "claude" }))
        ));
    }

    #[test]
    fn switching_the_agent_without_an_account_clears_the_local_pin() {
        // The old agent's profile names nothing under the new one.
        let mut settings = pinned(CodingAgent::Claude, "0a1b2c3d");
        assert!(apply_defaults_patch(
            &mut settings,
            &patch(serde_json::json!({ "defaultAgent": "codex" }))
        ));
        assert_eq!(settings.default_agent, CodingAgent::Codex);
        assert_eq!(settings.default_account, None);
    }

    #[test]
    fn a_named_account_sets_and_a_blank_one_clears() {
        let mut settings = pinned(CodingAgent::Claude, "0a1b2c3d");
        assert!(apply_defaults_patch(
            &mut settings,
            &patch(serde_json::json!({ "defaultAgent": "codex", "defaultAccount": " 9f8e7d6c " }))
        ));
        assert_eq!(settings.default_agent, CodingAgent::Codex);
        assert_eq!(settings.default_account.as_deref(), Some("9f8e7d6c"));
        assert!(apply_defaults_patch(
            &mut settings,
            &patch(serde_json::json!({ "defaultAgent": "codex", "defaultAccount": "" }))
        ));
        assert_eq!(settings.default_account, None);
    }

    #[test]
    fn a_patch_naming_no_valid_agent_leaves_the_pin_alone() {
        // Neither half of the pair: a per-agent edit says nothing about it.
        let mut settings = pinned(CodingAgent::Claude, "0a1b2c3d");
        apply_defaults_patch(
            &mut settings,
            &patch(serde_json::json!({ "agents": { "claude": { "model": "sonnet" } } })),
        );
        assert_eq!(settings.default_account.as_deref(), Some("0a1b2c3d"));
        // An agent this build cannot parse is ignored, and so is its pair.
        assert!(!apply_defaults_patch(
            &mut settings,
            &patch(serde_json::json!({ "defaultAgent": "cursor" }))
        ));
        assert_eq!(settings.default_agent, CodingAgent::Claude);
        assert_eq!(settings.default_account.as_deref(), Some("0a1b2c3d"));
    }

    #[test]
    fn a_pinned_account_round_trips_through_the_wire() {
        let source = pinned(CodingAgent::Codex, "0a1b2c3d");
        let mut target = Settings::default();
        assert!(apply_defaults_patch(&mut target, &defaults_wire(&source)));
        assert_eq!(target.default_agent, CodingAgent::Codex);
        assert_eq!(target.default_account.as_deref(), Some("0a1b2c3d"));
        assert!(!apply_defaults_patch(&mut target, &defaults_wire(&source)));
    }

    #[test]
    fn identical_patch_reports_unchanged() {
        let mut settings = Settings::default();
        let wire = defaults_wire(&settings);
        assert!(!apply_defaults_patch(&mut settings, &wire));
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn defaults_wire_never_serializes_null() {
        // EXP-495: capability-masked toggles must be OMITTED, not null —
        // the server's zod schema 400'd explicit nulls, silently failing
        // `devices.register` on every 0.14.10 daemon.
        let wire = serde_json::to_value(defaults_wire(&Settings::default())).unwrap();
        let rendered = serde_json::to_string(&wire).unwrap();
        assert!(!rendered.contains("null"), "no nulls on the wire: {rendered}");
        let codex = &wire["agents"]["codex"];
        assert!(codex.get("ultracode").is_none(), "ultracode is claude-only");
        // EXP-981: the subagent pin is claude-only too.
        assert!(
            codex.get("subagentModel").is_none(),
            "the subagent model is claude-only"
        );
        assert!(codex.get("planMode").is_none(), "plan mode is claude-only");
        // EXP-690: the retired key is never advertised on any agent.
        assert!(codex.get("skipPermissions").is_none());
        let claude = &wire["agents"]["claude"];
        assert!(claude.get("skipPermissions").is_none());
        assert!(claude.get("planMode").is_some());
        assert!(claude.get("subagentModel").is_some());
        // EXP-849: pi is gone from the wire entirely.
        assert!(wire["agents"].get("pi").is_none());
    }

    #[test]
    fn defaults_wire_round_trips_through_apply() {
        let mut source = Settings::default();
        source.default_agent = CodingAgent::Codex;
        source.claude_model = "sonnet".into();
        source.claude_ultracode = true;
        source.codex_effort = "high".into();
        source.claude_plan_mode = false;
        let wire = defaults_wire(&source);
        let mut target = Settings::default();
        assert!(apply_defaults_patch(&mut target, &wire));
        assert_eq!(target.default_agent, CodingAgent::Codex);
        assert_eq!(target.claude_model, "sonnet");
        assert!(target.claude_ultracode);
        assert_eq!(target.codex_effort, "high");
        assert!(!target.claude_plan_mode);
    }

    // EXP-1020: the "Workflow settings" pair.
    #[test]
    fn the_workflow_pair_rides_the_wire_and_clamps_field_wise() {
        let mut source = Settings::default();
        source.workflow_model = "sonnet".into();
        source.workflow_strong_model = "opus".into();
        let wire = defaults_wire(&source);
        let workflow = wire.workflow.as_ref().expect("the pair rides the wire");
        assert_eq!(workflow.model.as_deref(), Some("sonnet"));
        assert_eq!(workflow.strong_model.as_deref(), Some("opus"));

        let mut target = Settings::default();
        assert!(apply_defaults_patch(&mut target, &wire));
        assert_eq!(target.workflow_model, "sonnet");
        assert_eq!(target.workflow_strong_model, "opus");

        // A name outside the DEFAULT AGENT's vocabulary is dropped WITHOUT
        // resetting the field — the same rule every other patch value
        // follows. `target` defaults to claude, so a codex name is foreign.
        let patch = DefaultsPatch {
            workflow: Some(WorkflowDefaultsPatch {
                model: Some("gpt-5.6-sol".into()),
                strong_model: None,
            }),
            ..DefaultsPatch::default()
        };
        assert!(!apply_defaults_patch(&mut target, &patch));
        assert_eq!(target.workflow_model, "sonnet");
        assert_eq!(target.workflow_strong_model, "opus");
    }

    /// EXP-1020: a CODEX pair set on web must survive the trip — clamping it
    /// against claude's aliases dropped it, and the next `defaults_wire`
    /// push then shipped opus/fable back over the user's choice.
    #[test]
    fn a_codex_workflow_pair_rides_when_codex_is_the_default_agent() {
        let mut target = Settings::default();
        let patch = DefaultsPatch {
            default_agent: Some("codex".into()),
            workflow: Some(WorkflowDefaultsPatch {
                model: Some("gpt-5.6-sol".into()),
                strong_model: Some("gpt-5.6-luna".into()),
            }),
            ..DefaultsPatch::default()
        };
        assert!(apply_defaults_patch(&mut target, &patch));
        assert_eq!(target.default_agent, CodingAgent::Codex);
        assert_eq!(target.workflow_model, "gpt-5.6-sol");
        assert_eq!(target.workflow_strong_model, "gpt-5.6-luna");

        // And the reverse: claude names are foreign once codex is default.
        let patch = DefaultsPatch {
            default_agent: Some("codex".into()),
            workflow: Some(WorkflowDefaultsPatch {
                model: Some("opus".into()),
                strong_model: Some("fable".into()),
            }),
            ..DefaultsPatch::default()
        };
        assert!(!apply_defaults_patch(&mut target, &patch));
        assert_eq!(target.workflow_model, "gpt-5.6-sol");
    }

    /// EXP-1020: the overlay carries EVERY launch default and touches
    /// nothing else. `from` sets every field away from its default, so a
    /// launch field added later and forgotten in `overlay_launch_defaults`
    /// fails here — extend this fixture when you add one.
    #[test]
    fn overlay_carries_every_launch_default() {
        let from = Settings {
            default_agent: CodingAgent::Codex,
            default_account: Some("0a1b2c3d".into()),
            claude_path: "/usr/local/bin/claude".into(),
            codex_path: "/usr/local/bin/codex".into(),
            repos_root: "/tmp/repos".into(),
            branch_prefix: "wip/".into(),
            claude_model: "sonnet".into(),
            claude_effort: "xhigh".into(),
            claude_subagent_model: "sonnet".into(),
            workflow_model: "gpt-5.6-terra".into(),
            workflow_strong_model: "gpt-5.6-luna".into(),
            codex_model: "gpt-5.6-terra".into(),
            codex_effort: "high".into(),
            claude_ultracode: true,
            claude_plan_mode: false,
            auto_rotate_accounts: false,
            terminal_shell: Some("/bin/zsh".into()),
            changelog_seen_id: Some("2026-09-24".into()),
            emoji_recents: vec!["🎉".into()],
            tools_setup_seen: true,
            os_notifications: false,
        };
        assert_ne!(from, Settings::default(), "the fixture must differ everywhere");

        let mut onto = Settings::default();
        overlay_launch_defaults(&mut onto, &from);

        // What a launch-defaults save is allowed to change: everything in
        // `from` EXCEPT the fields other panes own.
        let expected = Settings {
            claude_path: Settings::default().claude_path,
            codex_path: Settings::default().codex_path,
            repos_root: Settings::default().repos_root,
            branch_prefix: Settings::default().branch_prefix,
            terminal_shell: Settings::default().terminal_shell,
            changelog_seen_id: Settings::default().changelog_seen_id,
            emoji_recents: Settings::default().emoji_recents,
            tools_setup_seen: Settings::default().tools_setup_seen,
            os_notifications: Settings::default().os_notifications,
            ..from.clone()
        };
        assert_eq!(onto, expected);
        // The three the Device settings dialog used to drop on its own row.
        assert_eq!(onto.claude_subagent_model, "sonnet");
        assert_eq!(onto.workflow_model, "gpt-5.6-terra");
        assert_eq!(onto.workflow_strong_model, "gpt-5.6-luna");
    }

    // -- remove_worktree_remote ------------------------------------------------

    #[test]
    fn removes_clean_and_untracked_refuses_tracked_and_held() {
        let dir = temp_dir("remove");
        let clone = seed(&dir.0);

        // Held branch refuses before touching git.
        let wt = worktree(&clone, "exp/EXP-1");
        let held: HashSet<String> = ["exp/EXP-1".to_string()].into();
        assert_eq!(
            remove_worktree_remote(&clone, "exp/EXP-1", &held),
            Err(RemoveWorktreeError::BranchHeld)
        );

        // Tracked changes refuse.
        fs::write(wt.join("README.md"), "edited\n").unwrap();
        assert_eq!(
            remove_worktree_remote(&clone, "exp/EXP-1", &HashSet::new()),
            Err(RemoveWorktreeError::TrackedChanges)
        );
        assert!(wt.exists());
        git(&wt, &["checkout", "--quiet", "--", "README.md"]);

        // Untracked-only debris goes with --force.
        fs::write(wt.join("scratch.txt"), "debris\n").unwrap();
        assert_eq!(remove_worktree_remote(&clone, "exp/EXP-1", &HashSet::new()), Ok(()));
        assert!(!wt.exists());
        // The branch survives (local removal parity — resume recreates).
        let out = Command::new("git")
            .args(["rev-parse", "--verify", "--quiet", "refs/heads/exp/EXP-1"])
            .current_dir(&clone)
            .output()
            .unwrap();
        assert!(out.status.success(), "branch must be kept");

        assert_eq!(
            remove_worktree_remote(&clone, "exp/EXP-1", &HashSet::new()),
            Err(RemoveWorktreeError::NotFound)
        );
    }

    #[test]
    fn gate_busy_refuses_transiently() {
        let dir = temp_dir("gate");
        let clone = seed(&dir.0);
        let _wt = worktree(&clone, "exp/EXP-2");
        let hold = crate::launch_gate::hold(&clone);
        assert_eq!(
            remove_worktree_remote(&clone, "exp/EXP-2", &HashSet::new()),
            Err(RemoveWorktreeError::GateBusy)
        );
        drop(hold);
        assert_eq!(remove_worktree_remote(&clone, "exp/EXP-2", &HashSet::new()), Ok(()));
    }

    #[test]
    fn conservative_policy_is_git_truth_only() {
        let keep: HashSet<String> = ["exp/EXP-9".to_string()].into();
        let policy = conservative_prune_policy("exp/", keep.clone(), vec![PathBuf::from("/x")], None);
        assert!(policy.prefixes.contains(&"exp/".to_string()));
        assert!(policy.prefixes.contains(&"exp/batch-".to_string()));
        assert_eq!(policy.default_branch, None, "resolved per clone, never fabricated");
        assert_eq!(policy.keep, keep);
        assert!(policy.merged.is_empty() && policy.finished.is_empty());
        assert!(policy.delete_stale_branches);
    }
}
