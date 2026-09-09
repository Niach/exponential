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
//!   top-level `bypassPermissionsModeAccepted`. EXP-690: every launch seeds
//!   it, plan-mode runs included — plan mode's `--allow-…` variant does not
//!   show the warning at boot, but Shift+Tab into bypass would.
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

/// `<config_dir>/.claude.json` for an explicit dir (EXP-792: a profile's
/// `CLAUDE_CONFIG_DIR`, the launcher passes it), else
/// `$CLAUDE_CONFIG_DIR/.claude.json` when set, else `$HOME/.claude.json`
/// (claude relocates all of its state under `CLAUDE_CONFIG_DIR`).
fn claude_config_path(config_dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(dir) = config_dir {
        return Some(dir.join(".claude.json"));
    }
    match std::env::var_os("CLAUDE_CONFIG_DIR") {
        Some(dir) if !dir.is_empty() => Some(PathBuf::from(dir).join(".claude.json")),
        _ => Some(dirs::home_dir()?.join(".claude.json")),
    }
}

/// Ensure `cwd` (and its canonicalized twin, when different — claude keys
/// `projects` by the resolved process cwd) is onboarded + trusted in the
/// user's claude config. Best-effort: failures are logged, never returned.
///
/// EXP-792: `config_dir` is the run's account PROFILE dir (the
/// `CLAUDE_CONFIG_DIR` its spawn carries) — the flags must land in the
/// config the run will read, not the ambient one; `None` = the ambient
/// login.
pub fn ensure_onboarded(cwd: &Path, seed_bypass: bool, config_dir: Option<&Path>) {
    // The launcher unit tests run prepare end-to-end with temp worktrees —
    // they must never seed those throwaway paths into the developer's REAL
    // claude config (the tests below exercise [`ensure_onboarded_in_config`]
    // against explicit temp files instead).
    #[cfg(test)]
    {
        let _ = (cwd, seed_bypass, config_dir);
    }
    #[cfg(not(test))]
    ensure_onboarded_live(cwd, seed_bypass, config_dir);
}

#[cfg_attr(test, allow(dead_code))]
fn ensure_onboarded_live(cwd: &Path, seed_bypass: bool, config_dir: Option<&Path>) {
    let Some(config) = claude_config_path(config_dir) else {
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
    crate::atomic_config::replace_preserving_mode(config, &serialized)?;
    Ok(true)
}

fn seed_true(obj: &mut serde_json::Map<String, serde_json::Value>, key: &str) -> bool {
    if obj.get(key) == Some(&serde_json::Value::Bool(true)) {
        return false;
    }
    obj.insert(key.to_string(), serde_json::Value::Bool(true));
    true
}

// ---------------------------------------------------------------------------
// EXP-757: the other direction — dropping the entries seeded for scratch
// dirs that no longer exist. Same posture as the seeder: only `projects`
// keys are touched (never the top-level flags), everything else rides
// through verbatim, an unparseable or non-object file is left alone, and
// the steady state writes nothing.
// ---------------------------------------------------------------------------

/// Drop the `projects` entries for exactly `paths` (the launcher seeds the
/// raw cwd and its canonical twin; the caller passes both). Best-effort.
pub fn forget(paths: &[PathBuf]) {
    // Same reason as [`ensure_onboarded`]: the tests exercise the `_in_config`
    // core against temp files and must never touch the developer's config.
    #[cfg(test)]
    {
        let _ = paths;
    }
    #[cfg(not(test))]
    forget_live(paths);
}

#[cfg_attr(test, allow(dead_code))]
fn forget_live(paths: &[PathBuf]) {
    let Some(config) = claude_config_path(None) else {
        return;
    };
    match forget_in_config(&config, paths) {
        Ok(0) => {}
        Ok(dropped) => log::info!("claude trust: dropped {dropped} scratch project entr{}", plural_y(dropped)),
        Err(err) => log::warn!("claude trust: {err}"),
    }
}

/// Drop every `projects` entry under `root` (raw or canonical) whose path is
/// no longer a directory — the scratch dirs a sweep just removed, and any an
/// older build left keyed. Returns how many were dropped. Best-effort.
pub fn forget_missing_under(root: &Path) -> usize {
    #[cfg(test)]
    {
        let _ = root;
        0
    }
    #[cfg(not(test))]
    forget_missing_under_live(root)
}

#[cfg_attr(test, allow(dead_code))]
fn forget_missing_under_live(root: &Path) -> usize {
    let Some(config) = claude_config_path(None) else {
        return 0;
    };
    let mut roots = vec![root.to_path_buf()];
    if let Ok(canonical) = std::fs::canonicalize(root) {
        if canonical != *root {
            roots.push(canonical);
        }
    }
    match forget_missing_under_in_config(&config, &roots, |path| path.is_dir()) {
        Ok(dropped) => {
            if dropped > 0 {
                log::info!("claude trust: dropped {dropped} stale scratch project entr{}", plural_y(dropped));
            }
            dropped
        }
        Err(err) => {
            log::warn!("claude trust: {err}");
            0
        }
    }
}

fn plural_y(count: usize) -> &'static str {
    if count == 1 {
        "y"
    } else {
        "ies"
    }
}

/// The testable core of [`forget`]: remove the listed keys, returning how
/// many were present.
fn forget_in_config(config: &Path, paths: &[PathBuf]) -> Result<usize, String> {
    retain_projects(config, |key| {
        !paths.iter().any(|path| path.to_string_lossy() == key)
    })
}

/// The testable core of [`forget_missing_under`]: `is_dir` is injected so
/// the rule (under a root, not the root itself, no longer a directory) is
/// testable without a filesystem.
fn forget_missing_under_in_config(
    config: &Path,
    roots: &[PathBuf],
    is_dir: impl Fn(&Path) -> bool,
) -> Result<usize, String> {
    retain_projects(config, |key| {
        let path = Path::new(key);
        // `starts_with` is component-wise, so a trailing slash or a
        // different separator spelling still matches its root.
        let under_root = roots
            .iter()
            .any(|root| path.starts_with(root) && path != root.as_path());
        // Kept unless it is one of ours AND its directory is gone.
        !under_root || is_dir(path)
    })
}

/// Load, keep the `projects` keys `keep` approves, write back only when
/// something was dropped. Missing file = nothing to forget.
fn retain_projects(config: &Path, keep: impl Fn(&str) -> bool) -> Result<usize, String> {
    let existing = match std::fs::read_to_string(config) {
        Ok(existing) => existing,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(err) => return Err(format!("read {}: {err}", config.display())),
    };
    if existing.trim().is_empty() {
        return Ok(0);
    }
    let mut root: serde_json::Value = serde_json::from_str(&existing)
        .map_err(|err| format!("parse {}: {err}", config.display()))?;
    let Some(top) = root.as_object_mut() else {
        return Err(format!("{}: top level is not an object", config.display()));
    };
    let Some(projects) = top.get_mut("projects") else {
        return Ok(0);
    };
    let Some(projects) = projects.as_object_mut() else {
        return Err(format!("{}: `projects` is not an object", config.display()));
    };
    let before = projects.len();
    projects.retain(|key, _| keep(key));
    let dropped = before - projects.len();
    if dropped == 0 {
        return Ok(0);
    }
    let serialized = serde_json::to_string_pretty(&root)
        .map_err(|err| format!("serialize {}: {err}", config.display()))?;
    // EXP-757 runs this at every session end AND every start, so it now races
    // a `claude` that owns the same file (history, `numStartups`, a trust
    // answer). Our rename would replace ITS write wholesale with the copy we
    // parsed a moment ago. Re-read: if the bytes moved under us, the sweep is
    // abandoned — the next one (every start, every end) retries against the
    // current file. Cheap and one-sided: a false skip only delays a cleanup,
    // a false write loses the user's data.
    match std::fs::read_to_string(config) {
        Ok(current) if current == existing => {}
        _ => {
            log::info!(
                "{} changed while pruning its trust entries; retrying on the next sweep",
                config.display()
            );
            return Ok(0);
        }
    }
    crate::atomic_config::replace_preserving_mode(config, &serialized)?;
    Ok(dropped)
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

    // ---- EXP-757: forgetting ----

    fn seeded_config(tag: &str, keys: &[&str]) -> PathBuf {
        let config = temp_config(tag);
        let paths: Vec<PathBuf> = keys.iter().map(PathBuf::from).collect();
        assert_eq!(ensure_onboarded_in_config(&config, &paths, true), Ok(true));
        config
    }

    #[test]
    fn forget_drops_raw_and_canonical_keys_only() {
        let config = seeded_config(
            "forget",
            &[
                "/data/actions/builtin_chat/1a2b3c4d",
                "/private/data/actions/builtin_chat/1a2b3c4d",
                "/repos/exponential.worktrees/exp-abc",
            ],
        );
        let gone = vec![
            PathBuf::from("/data/actions/builtin_chat/1a2b3c4d"),
            PathBuf::from("/private/data/actions/builtin_chat/1a2b3c4d"),
        ];
        assert_eq!(forget_in_config(&config, &gone), Ok(2));
        let root = parsed(&config);
        assert!(project(&root, "/data/actions/builtin_chat/1a2b3c4d").is_null());
        assert!(project(&root, "/private/data/actions/builtin_chat/1a2b3c4d").is_null());
        assert_eq!(
            project(&root, "/repos/exponential.worktrees/exp-abc")["hasTrustDialogAccepted"],
            true
        );
        // The top-level flags are never ours to unset.
        assert_eq!(root["hasCompletedOnboarding"], true);
        assert_eq!(root["bypassPermissionsModeAccepted"], true);
        let _ = std::fs::remove_dir_all(config.parent().unwrap());
    }

    #[test]
    fn forget_is_a_no_op_without_a_write() {
        let config = seeded_config("forget-noop", &["/repos/kept"]);
        let before = std::fs::read_to_string(&config).unwrap();
        assert_eq!(
            forget_in_config(&config, &[PathBuf::from("/data/actions/x/y")]),
            Ok(0)
        );
        assert_eq!(std::fs::read_to_string(&config).unwrap(), before);
        // No file at all: nothing to forget, nothing created.
        let missing = temp_config("forget-missing");
        assert_eq!(
            forget_in_config(&missing, &[PathBuf::from("/data/actions/x/y")]),
            Ok(0)
        );
        assert!(!missing.exists());
        let _ = std::fs::remove_dir_all(config.parent().unwrap());
    }

    #[test]
    fn forget_missing_under_drops_only_gone_dirs_under_the_root() {
        let config = temp_config("forget-under");
        std::fs::create_dir_all(config.parent().unwrap()).unwrap();
        std::fs::write(
            &config,
            r#"{
  "hasCompletedOnboarding": true,
  "projects": {
    "/data/actions/builtin_chat/gone": {"hasTrustDialogAccepted": true, "history": [1]},
    "/data/actions/builtin_chat/gone/": {"hasTrustDialogAccepted": true},
    "/private/data/actions/builtin_chat/gone": {"hasTrustDialogAccepted": true},
    "/data/actions/builtin_chat/alive": {"hasTrustDialogAccepted": true},
    "/data/actions/legacy-action": {"hasTrustDialogAccepted": true},
    "/data/actions": {"hasTrustDialogAccepted": true},
    "/data/actions-not-ours/gone": {"hasTrustDialogAccepted": true},
    "/repos/exponential": {"hasTrustDialogAccepted": true}
  },
  "numStartups": 7
}"#,
        )
        .unwrap();
        let roots = vec![
            PathBuf::from("/data/actions"),
            PathBuf::from("/private/data/actions"),
        ];
        let alive = |path: &Path| path == Path::new("/data/actions/builtin_chat/alive");
        assert_eq!(
            forget_missing_under_in_config(&config, &roots, alive),
            Ok(4)
        );
        let root = parsed(&config);
        let projects = root["projects"].as_object().unwrap();
        let mut keys: Vec<&String> = projects.keys().collect();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "/data/actions",
                "/data/actions-not-ours/gone",
                "/data/actions/builtin_chat/alive",
                "/repos/exponential",
            ]
        );
        assert_eq!(root["numStartups"], 7);
        assert_eq!(root["hasCompletedOnboarding"], true);
        // Second pass: steady state, no write.
        let before = std::fs::read_to_string(&config).unwrap();
        assert_eq!(
            forget_missing_under_in_config(&config, &roots, alive),
            Ok(0)
        );
        assert_eq!(std::fs::read_to_string(&config).unwrap(), before);
        let _ = std::fs::remove_dir_all(config.parent().unwrap());
    }

    #[test]
    fn forget_leaves_an_unparseable_or_shapeless_config_alone() {
        let config = temp_config("forget-broken");
        std::fs::create_dir_all(config.parent().unwrap()).unwrap();
        std::fs::write(&config, "{ not json").unwrap();
        assert!(forget_in_config(&config, &[PathBuf::from("/data/actions/x/y")]).is_err());
        assert_eq!(std::fs::read_to_string(&config).unwrap(), "{ not json");

        std::fs::write(&config, r#"{"projects": []}"#).unwrap();
        assert!(
            forget_missing_under_in_config(&config, &[PathBuf::from("/data/actions")], |_| false)
                .is_err()
        );
        assert_eq!(std::fs::read_to_string(&config).unwrap(), r#"{"projects": []}"#);

        // No `projects` map at all: nothing to do, nothing written.
        std::fs::write(&config, r#"{"hasCompletedOnboarding": true}"#).unwrap();
        assert_eq!(
            forget_missing_under_in_config(&config, &[PathBuf::from("/data/actions")], |_| false),
            Ok(0)
        );
        assert_eq!(
            std::fs::read_to_string(&config).unwrap(),
            r#"{"hasCompletedOnboarding": true}"#
        );
        let _ = std::fs::remove_dir_all(config.parent().unwrap());
    }

    /// The `claude` CLI saving its own file between our read and our rename
    /// (EXP-757 runs this at every session start and end): the prune must
    /// abandon the write instead of replaying its stale copy over it. The
    /// `keep` closure runs exactly in that window, so it stands in for the CLI.
    #[test]
    fn a_concurrent_write_aborts_the_prune() {
        let config = temp_config("race");
        std::fs::create_dir_all(config.parent().unwrap()).unwrap();
        std::fs::write(
            &config,
            serde_json::json!({ "projects": { "/data/actions/x/y": {} } }).to_string(),
        )
        .unwrap();
        let concurrent =
            serde_json::json!({ "numStartups": 7, "projects": { "/data/actions/x/y": {} } })
                .to_string();
        assert_eq!(
            retain_projects(&config, |_| {
                std::fs::write(&config, &concurrent).unwrap();
                false
            }),
            Ok(0)
        );
        assert_eq!(std::fs::read_to_string(&config).unwrap(), concurrent);
        // The next sweep, with nobody else writing, prunes against the
        // CURRENT file — the CLI's key survives.
        assert_eq!(retain_projects(&config, |_| false), Ok(1));
        let root = parsed(&config);
        assert_eq!(root["numStartups"], 7);
        assert!(root["projects"].as_object().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(config.parent().unwrap());
    }
}
