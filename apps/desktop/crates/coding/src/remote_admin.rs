//! EXP-481: device-side execution of the server-authoritative device state —
//! applying `launch_defaults` patches onto the local [`Settings`], and the
//! `worktree_remove` / `worktree_prune` command bodies both binaries (desktop
//! + CLI daemon) run off the heartbeat's command pickup.
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ultracode: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_mode: Option<bool>,
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
    pub agents: BTreeMap<String, AgentDefaultsPatch>,
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
    if let Some(agent) = patch.default_agent.as_deref().and_then(CodingAgent::parse) {
        if settings.default_agent != agent {
            settings.default_agent = agent;
            changed = true;
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
                    CodingAgent::Pi => &mut settings.pi_model,
                };
                set_string(slot, model, &mut changed);
            }
        }
        if let Some(effort) = &entry.effort {
            if effort.is_empty() || agent.effort_values().contains(&effort.as_str()) {
                let slot = match agent {
                    CodingAgent::Claude => &mut settings.claude_effort,
                    CodingAgent::Codex => &mut settings.codex_effort,
                    CodingAgent::Pi => &mut settings.pi_thinking,
                };
                set_string(slot, effort, &mut changed);
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
                CodingAgent::Pi => set_bool(&mut settings.pi_plan_mode, plan_mode, &mut changed),
                CodingAgent::Codex => {}
            }
        }
    }
    changed
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
                ultracode: agent
                    .supports_ultracode()
                    .then_some(settings.claude_ultracode),
                plan_mode: agent
                    .supports_plan_mode()
                    .then_some(settings.plan_mode_for(agent)),
            },
        );
    }
    DefaultsPatch {
        default_agent: Some(settings.default_agent.id().to_string()),
        agents,
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
        assert!(codex.get("planMode").is_none(), "plan mode is claude+pi");
        // EXP-690: the retired key is never advertised on any agent.
        assert!(codex.get("skipPermissions").is_none());
        let pi = &wire["agents"]["pi"];
        assert!(pi.get("ultracode").is_none());
        assert!(pi.get("skipPermissions").is_none());
        assert!(pi.get("planMode").is_some());
    }

    #[test]
    fn defaults_wire_round_trips_through_apply() {
        let mut source = Settings::default();
        source.default_agent = CodingAgent::Pi;
        source.claude_model = "sonnet".into();
        source.claude_ultracode = true;
        source.codex_effort = "high".into();
        source.pi_plan_mode = false;
        let wire = defaults_wire(&source);
        let mut target = Settings::default();
        assert!(apply_defaults_patch(&mut target, &wire));
        assert_eq!(target.default_agent, CodingAgent::Pi);
        assert_eq!(target.claude_model, "sonnet");
        assert!(target.claude_ultracode);
        assert_eq!(target.codex_effort, "high");
        assert!(!target.pi_plan_mode);
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
