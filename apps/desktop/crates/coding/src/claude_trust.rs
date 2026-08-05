//! Pre-accept Claude Code's first-run prompts (EXP-414).
//!
//! An interactive `claude` TUI parks on up to three startup screens before
//! any prompt runs, which blocks a REMOTELY started session forever (nobody
//! is at the terminal to press enter — the trust screen is deliberately
//! excluded from steer question detection, so the phone sees nothing):
//!
//! - first-run onboarding (theme picker), gated by the top-level
//!   `hasCompletedOnboarding` in `~/.claude.json`
//! - the per-DIRECTORY "Do you trust the files in this folder?" dialog,
//!   gated by `projects."<cwd>".hasTrustDialogAccepted` — and every session
//!   spawns in a fresh worktree path, so this one re-blocks per session
//! - the one-time `--dangerously-skip-permissions` warning, gated by the
//!   top-level `bypassPermissionsModeAccepted` (only relevant when the user
//!   opted into skip-permissions — plan mode's `--allow-…` variant does not
//!   show it)
//!
//! There is no CLI flag that pre-accepts any of these; the only mechanism is
//! the state file claude itself persists, so the launcher seeds the exact
//! keys claude writes when the user answers. Unlike codex_trust, an existing
//! `false` IS flipped to `true`: claude writes `false` as its pre-answer
//! scaffold (declining the dialog exits without persisting a refusal — there
//! is no "never trust" state), and that scaffold is exactly the state that
//! blocks a remote session.
//!
//! Posture: additive and best-effort, never load-bearing — every unknown key
//! (history, metrics, …) rides through the serde_json round-trip verbatim,
//! an unparseable or non-object file is left untouched, and the steady state
//! writes nothing. Claude rewrites this file continuously while a session
//! runs; our once-per-new-directory read-modify-write races that benignly
//! (write-then-rename keeps the file parseable either way).

use std::path::{Path, PathBuf};

/// `$CLAUDE_CONFIG_DIR/.claude.json` when set, else `$HOME/.claude.json`
/// (claude relocates all of its state under `CLAUDE_CONFIG_DIR`).
fn claude_config_path() -> Option<PathBuf> {
    match std::env::var_os("CLAUDE_CONFIG_DIR") {
        Some(dir) if !dir.is_empty() => Some(PathBuf::from(dir).join(".claude.json")),
        _ => Some(dirs::home_dir()?.join(".claude.json")),
    }
}

/// Ensure `cwd` (and its canonicalized twin, when different — claude keys
/// `projects` by the resolved process cwd) is onboarded + trusted in the
/// user's claude config. Best-effort: failures are logged, never returned.
pub fn ensure_onboarded(cwd: &Path, seed_bypass: bool) {
    // The launcher unit tests run prepare end-to-end with temp worktrees —
    // they must never seed those throwaway paths into the developer's REAL
    // claude config (the tests below exercise [`ensure_onboarded_in_config`]
    // against explicit temp files instead).
    #[cfg(test)]
    {
        let _ = (cwd, seed_bypass);
    }
    #[cfg(not(test))]
    ensure_onboarded_live(cwd, seed_bypass);
}

#[cfg_attr(test, allow(dead_code))]
fn ensure_onboarded_live(cwd: &Path, seed_bypass: bool) {
    let Some(config) = claude_config_path() else {
        return;
    };
    let mut paths = vec![cwd.to_path_buf()];
    if let Ok(canonical) = std::fs::canonicalize(cwd) {
        if canonical != *cwd {
            paths.push(canonical);
        }
    }
    match ensure_onboarded_in_config(&config, &paths, seed_bypass) {
        Ok(false) => {}
        Ok(true) => log::info!(
            "claude trust: seeded onboarding/trust for {}",
            cwd.display()
        ),
        Err(err) => log::warn!("claude trust: {err} — claude may show its first-run prompts"),
    }
}

/// The testable core: seed `hasCompletedOnboarding` (+ optionally
/// `bypassPermissionsModeAccepted`) and, per path, the `projects` entry's
/// `hasTrustDialogAccepted`/`hasCompletedProjectOnboarding`, preserving
/// every other key verbatim. Returns whether anything needed writing.
fn ensure_onboarded_in_config(
    config: &Path,
    paths: &[PathBuf],
    seed_bypass: bool,
) -> Result<bool, String> {
    let existing = match std::fs::read_to_string(config) {
        Ok(existing) => existing,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(format!("read {}: {err}", config.display())),
    };
    let mut root: serde_json::Value = if existing.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(&existing)
            .map_err(|err| format!("parse {}: {err}", config.display()))?
    };
    let Some(top) = root.as_object_mut() else {
        return Err(format!("{}: top level is not an object", config.display()));
    };
    let mut changed = seed_true(top, "hasCompletedOnboarding");
    if seed_bypass {
        changed |= seed_true(top, "bypassPermissionsModeAccepted");
    }
    let projects = top
        .entry("projects")
        .or_insert_with(|| serde_json::json!({}));
    let Some(projects) = projects.as_object_mut() else {
        return Err(format!("{}: `projects` is not an object", config.display()));
    };
    for path in paths {
        let entry = projects
            .entry(path.to_string_lossy().into_owned())
            .or_insert_with(|| serde_json::json!({}));
        // A non-object project entry: skip, never clobber.
        let Some(entry) = entry.as_object_mut() else {
            continue;
        };
        changed |= seed_true(entry, "hasTrustDialogAccepted");
        changed |= seed_true(entry, "hasCompletedProjectOnboarding");
    }
    if !changed {
        return Ok(false);
    }

    if let Some(dir) = config.parent() {
        std::fs::create_dir_all(dir).map_err(|err| format!("create {}: {err}", dir.display()))?;
    }
    let serialized = serde_json::to_string_pretty(&root)
        .map_err(|err| format!("serialize {}: {err}", config.display()))?;
    // Write-then-rename: .claude.json is the user's own claude state and a
    // torn write would break every claude invocation, not just ours.
    let temp = config.with_extension("json.exp-tmp");
    std::fs::write(&temp, serialized).map_err(|err| format!("write {}: {err}", temp.display()))?;
    std::fs::rename(&temp, config).map_err(|err| {
        let _ = std::fs::remove_file(&temp);
        format!("replace {}: {err}", config.display())
    })?;
    Ok(true)
}

fn seed_true(obj: &mut serde_json::Map<String, serde_json::Value>, key: &str) -> bool {
    if obj.get(key) == Some(&serde_json::Value::Bool(true)) {
        return false;
    }
    obj.insert(key.to_string(), serde_json::Value::Bool(true));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-claude-trust-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        dir.join(".claude.json")
    }

    fn parsed(config: &Path) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(config).unwrap()).unwrap()
    }

    fn project(root: &serde_json::Value, path: &str) -> serde_json::Value {
        root["projects"][path].clone()
    }

    #[test]
    fn creates_the_config_and_is_idempotent() {
        let config = temp_config("fresh");
        let paths = vec![PathBuf::from("/repos/exponential.worktrees/exp-abc")];
        assert_eq!(
            ensure_onboarded_in_config(&config, &paths, true),
            Ok(true)
        );
        let root = parsed(&config);
        assert_eq!(root["hasCompletedOnboarding"], true);
        assert_eq!(root["bypassPermissionsModeAccepted"], true);
        let entry = project(&root, "/repos/exponential.worktrees/exp-abc");
        assert_eq!(entry["hasTrustDialogAccepted"], true);
        assert_eq!(entry["hasCompletedProjectOnboarding"], true);
        // Second call: already seeded, nothing rewritten.
        let before = std::fs::read_to_string(&config).unwrap();
        assert_eq!(
            ensure_onboarded_in_config(&config, &paths, true),
            Ok(false)
        );
        assert_eq!(std::fs::read_to_string(&config).unwrap(), before);
        let _ = std::fs::remove_dir_all(config.parent().unwrap());
    }

    #[test]
    fn merges_without_clobbering_unknown_keys() {
        let config = temp_config("merge");
        std::fs::create_dir_all(config.parent().unwrap()).unwrap();
        std::fs::write(
            &config,
            serde_json::json!({
                "numStartups": 42,
                "theme": "dark",
                "projects": {
                    "/other/repo": {
                        "hasTrustDialogAccepted": true,
                        "history": [{"display": "fix the tests"}],
                        "exampleFiles": ["a.rs"]
                    }
                }
            })
            .to_string(),
        )
        .unwrap();
        let added =
            ensure_onboarded_in_config(&config, &[PathBuf::from("/new/repo")], false).unwrap();
        assert!(added);
        let root = parsed(&config);
        assert_eq!(root["numStartups"], 42);
        assert_eq!(root["theme"], "dark");
        let other = project(&root, "/other/repo");
        assert_eq!(other["history"][0]["display"], "fix the tests");
        assert_eq!(other["exampleFiles"][0], "a.rs");
        assert_eq!(project(&root, "/new/repo")["hasTrustDialogAccepted"], true);
        let _ = std::fs::remove_dir_all(config.parent().unwrap());
    }

    #[test]
    fn flips_claudes_own_false_scaffold() {
        // Unlike codex_trust, `false` here is claude's pre-answer scaffold
        // (there is no persisted "never trust" state) and is exactly what
        // blocks a remote session — so it is flipped, not skipped.
        let config = temp_config("scaffold");
        std::fs::create_dir_all(config.parent().unwrap()).unwrap();
        std::fs::write(
            &config,
            serde_json::json!({
                "hasCompletedOnboarding": false,
                "projects": {"/repo": {"hasTrustDialogAccepted": false}}
            })
            .to_string(),
        )
        .unwrap();
        assert_eq!(
            ensure_onboarded_in_config(&config, &[PathBuf::from("/repo")], false),
            Ok(true)
        );
        let root = parsed(&config);
        assert_eq!(root["hasCompletedOnboarding"], true);
        assert_eq!(project(&root, "/repo")["hasTrustDialogAccepted"], true);
        let _ = std::fs::remove_dir_all(config.parent().unwrap());
    }

    #[test]
    fn bypass_only_seeded_when_requested() {
        let config = temp_config("bypass");
        assert_eq!(
            ensure_onboarded_in_config(&config, &[PathBuf::from("/repo")], false),
            Ok(true)
        );
        assert!(parsed(&config)
            .get("bypassPermissionsModeAccepted")
            .is_none());
        // A pre-existing acceptance causes no rewrite on a bypass run.
        let before = std::fs::read_to_string(&config).unwrap();
        assert_eq!(
            ensure_onboarded_in_config(&config, &[PathBuf::from("/repo")], false),
            Ok(false)
        );
        assert_eq!(std::fs::read_to_string(&config).unwrap(), before);
        let _ = std::fs::remove_dir_all(config.parent().unwrap());
    }

    #[test]
    fn an_unparseable_config_is_left_alone() {
        let config = temp_config("broken");
        std::fs::create_dir_all(config.parent().unwrap()).unwrap();
        std::fs::write(&config, "not { json").unwrap();
        assert!(
            ensure_onboarded_in_config(&config, &[PathBuf::from("/repo")], true).is_err()
        );
        assert_eq!(std::fs::read_to_string(&config).unwrap(), "not { json");
        let _ = std::fs::remove_dir_all(config.parent().unwrap());
    }

    #[test]
    fn non_object_shapes_never_corrupt() {
        // Top level not an object → untouched.
        let config = temp_config("toplevel");
        std::fs::create_dir_all(config.parent().unwrap()).unwrap();
        std::fs::write(&config, "[1, 2]").unwrap();
        assert!(
            ensure_onboarded_in_config(&config, &[PathBuf::from("/repo")], false).is_err()
        );
        assert_eq!(std::fs::read_to_string(&config).unwrap(), "[1, 2]");

        // `projects` not an object → untouched.
        std::fs::write(&config, r#"{"projects": "oops"}"#).unwrap();
        assert!(
            ensure_onboarded_in_config(&config, &[PathBuf::from("/repo")], false).is_err()
        );
        assert_eq!(
            std::fs::read_to_string(&config).unwrap(),
            r#"{"projects": "oops"}"#
        );

        // A non-object project ENTRY is skipped while the rest still seeds.
        std::fs::write(
            &config,
            serde_json::json!({"projects": {"/weird": "oops"}}).to_string(),
        )
        .unwrap();
        assert_eq!(
            ensure_onboarded_in_config(
                &config,
                &[PathBuf::from("/weird"), PathBuf::from("/repo")],
                false
            ),
            Ok(true)
        );
        let root = parsed(&config);
        assert_eq!(root["projects"]["/weird"], "oops");
        assert_eq!(project(&root, "/repo")["hasTrustDialogAccepted"], true);
        let _ = std::fs::remove_dir_all(config.parent().unwrap());
    }

    #[test]
    fn raw_and_canonical_cwd_both_seed() {
        let config = temp_config("twin");
        let paths = vec![
            PathBuf::from("/repos/link/worktree"),
            PathBuf::from("/repos/real/worktree"),
        ];
        assert_eq!(ensure_onboarded_in_config(&config, &paths, false), Ok(true));
        let root = parsed(&config);
        assert_eq!(
            project(&root, "/repos/link/worktree")["hasTrustDialogAccepted"],
            true
        );
        assert_eq!(
            project(&root, "/repos/real/worktree")["hasTrustDialogAccepted"],
            true
        );
        let _ = std::fs::remove_dir_all(config.parent().unwrap());
    }
}
