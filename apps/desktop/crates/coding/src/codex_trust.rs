//! Pre-trust the codex project directory (EXP-389).
//!
//! An interactive `codex` TUI parks on its "Do you trust the contents of
//! this directory?" onboarding screen whenever the project is not in the
//! `[projects]` table of `$CODEX_HOME|~/.codex/config.toml` — which blocks a
//! REMOTELY started session forever (nobody is at the desktop to press
//! enter, and the phone sees nothing: the rollout file only materializes
//! after the screen is answered).
//!
//! The `-c projects."<path>".trust_level="trusted"` CLI override is NOT a
//! fix: codex's `-c` key parser splits on every literal `.`, so any path
//! containing a dot (`exponential.worktrees/…`) breaks the override
//! (verified against codex-cli 0.144.5). So the launcher writes the exact
//! entry codex itself persists when the user answers "Yes, continue":
//!
//! ```toml
//! [projects."/abs/path/to/repo"]
//! trust_level = "trusted"
//! ```
//!
//! Codex resolves the trust subject to the ROOT git project — for a linked
//! worktree that is the main clone, not the worktree (also verified) — so
//! callers pass the CLONE for repo-backed runs and the cwd itself for
//! repo-less scratch runs. Both stay stable per repo/action, so the config
//! never accumulates per-session entries.
//!
//! Posture: additive and best-effort, never load-bearing — the config is
//! parsed before AND after the append, and anything unexpected (unreadable
//! file, unparseable TOML, an existing entry of any trust level) leaves the
//! file untouched; codex then just shows its screen like today.

use std::path::{Path, PathBuf};

/// `$CODEX_HOME|~/.codex` (mirrors `codex_sessions::default_codex_sessions_root`).
fn codex_home() -> Option<PathBuf> {
    match std::env::var_os("CODEX_HOME") {
        Some(dir) if !dir.is_empty() => Some(PathBuf::from(dir)),
        _ => Some(dirs::home_dir()?.join(".codex")),
    }
}

/// Ensure `root` (and its canonicalized twin, when different — codex
/// resolves the cwd before the trust lookup) is a trusted project in the
/// user's codex config. Best-effort: failures are logged, never returned.
pub fn ensure_trusted(root: &Path) {
    // The launcher unit tests run prepare end-to-end with temp clones — they
    // must never append those throwaway paths to the developer's REAL codex
    // config (the tests below exercise [`ensure_trusted_in_config`] against
    // explicit temp files instead).
    #[cfg(test)]
    {
        let _ = root;
    }
    #[cfg(not(test))]
    ensure_trusted_live(root);
}

#[cfg_attr(test, allow(dead_code))]
fn ensure_trusted_live(root: &Path) {
    let Some(home) = codex_home() else {
        return;
    };
    let mut paths = vec![root.to_path_buf()];
    if let Ok(canonical) = std::fs::canonicalize(root) {
        if canonical != *root {
            paths.push(canonical);
        }
    }
    match ensure_trusted_in_config(&home.join("config.toml"), &paths) {
        Ok(0) => {}
        Ok(added) => log::info!(
            "codex trust: recorded {added} project entr{} for {}",
            if added == 1 { "y" } else { "ies" },
            root.display()
        ),
        Err(err) => log::warn!("codex trust: {err} — codex may show its trust prompt"),
    }
}

/// The testable core: append `[projects."<path>"] trust_level = "trusted"`
/// for every path not already present in `config`, returning how many were
/// added. A path with an EXISTING entry is never touched, whatever its
/// trust level — appending a duplicate table would corrupt the whole file.
fn ensure_trusted_in_config(config: &Path, paths: &[PathBuf]) -> Result<usize, String> {
    let existing = match std::fs::read_to_string(config) {
        Ok(existing) => existing,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(format!("read {}: {err}", config.display())),
    };
    let parsed: toml::Value = if existing.trim().is_empty() {
        toml::Value::Table(Default::default())
    } else {
        toml::from_str(&existing).map_err(|err| format!("parse {}: {err}", config.display()))?
    };
    let projects = parsed.get("projects").and_then(toml::Value::as_table);
    let missing: Vec<String> = paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .filter(|path| !projects.is_some_and(|table| table.contains_key(path)))
        .collect();
    if missing.is_empty() {
        return Ok(0);
    }

    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    for path in &missing {
        updated.push_str(&format!(
            "\n[projects.{}]\ntrust_level = \"trusted\"\n",
            toml_key(path)
        ));
    }
    // Never persist anything codex's own parser would reject (a `projects`
    // inline table, say, would make the append a duplicate-key error).
    toml::from_str::<toml::Value>(&updated)
        .map_err(|err| format!("appended config would not parse: {err}"))?;

    if let Some(dir) = config.parent() {
        std::fs::create_dir_all(dir).map_err(|err| format!("create {}: {err}", dir.display()))?;
    }
    // Write-then-rename: config.toml is the user's own codex config and a
    // torn write would break every codex invocation, not just ours.
    let temp = config.with_extension("toml.exp-tmp");
    crate::atomic_config::replace_preserving_mode(config, &temp, &updated)?;
    Ok(missing.len())
}

/// A TOML basic-string key segment for an absolute path (escapes `\` and
/// `"` — Windows paths ride as `"C:\\Users\\…"`).
fn toml_key(path: &str) -> String {
    let mut out = String::with_capacity(path.len() + 2);
    out.push('"');
    for c in path.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-codex-trust-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        dir.join("config.toml")
    }

    fn trusted_paths(config: &Path) -> Vec<String> {
        let parsed: toml::Value =
            toml::from_str(&std::fs::read_to_string(config).unwrap()).unwrap();
        parsed
            .get("projects")
            .and_then(toml::Value::as_table)
            .map(|table| {
                table
                    .iter()
                    .filter(|(_, entry)| {
                        entry.get("trust_level").and_then(toml::Value::as_str)
                            == Some("trusted")
                    })
                    .map(|(key, _)| key.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn creates_the_config_and_is_idempotent() {
        let config = temp_config("fresh");
        let paths = vec![PathBuf::from("/repos/exponential.worktrees/main")];
        assert_eq!(ensure_trusted_in_config(&config, &paths), Ok(1));
        assert_eq!(
            trusted_paths(&config),
            vec!["/repos/exponential.worktrees/main".to_string()],
            "dotted paths must survive as ONE quoted key segment"
        );
        // Second call: already present, nothing rewritten.
        let before = std::fs::read_to_string(&config).unwrap();
        assert_eq!(ensure_trusted_in_config(&config, &paths), Ok(0));
        assert_eq!(std::fs::read_to_string(&config).unwrap(), before);
        let _ = std::fs::remove_dir_all(config.parent().unwrap());
    }

    #[test]
    fn appends_without_touching_existing_content() {
        let config = temp_config("append");
        std::fs::create_dir_all(config.parent().unwrap()).unwrap();
        let existing = "model = \"gpt-5.6-terra\"\n\n[projects.\"/old/repo\"]\ntrust_level = \"trusted\"\n";
        std::fs::write(&config, existing).unwrap();
        let added = ensure_trusted_in_config(&config, &[PathBuf::from("/new/repo")]).unwrap();
        assert_eq!(added, 1);
        let updated = std::fs::read_to_string(&config).unwrap();
        assert!(updated.starts_with(existing), "prior content is preserved verbatim");
        let mut trusted = trusted_paths(&config);
        trusted.sort();
        assert_eq!(trusted, vec!["/new/repo".to_string(), "/old/repo".to_string()]);
        let _ = std::fs::remove_dir_all(config.parent().unwrap());
    }

    #[test]
    fn an_existing_entry_is_never_overridden_whatever_its_level() {
        // The user answered codex's prompt for this path already — a repeat
        // append would be a duplicate table (config-corrupting) AND would
        // override an explicit distrust decision.
        let config = temp_config("existing");
        std::fs::create_dir_all(config.parent().unwrap()).unwrap();
        std::fs::write(
            &config,
            "[projects.\"/repo\"]\ntrust_level = \"untrusted\"\n",
        )
        .unwrap();
        let before = std::fs::read_to_string(&config).unwrap();
        assert_eq!(
            ensure_trusted_in_config(&config, &[PathBuf::from("/repo")]),
            Ok(0)
        );
        assert_eq!(std::fs::read_to_string(&config).unwrap(), before);
        let _ = std::fs::remove_dir_all(config.parent().unwrap());
    }

    #[test]
    fn an_unparseable_config_is_left_alone() {
        let config = temp_config("broken");
        std::fs::create_dir_all(config.parent().unwrap()).unwrap();
        std::fs::write(&config, "not [ valid toml").unwrap();
        assert!(ensure_trusted_in_config(&config, &[PathBuf::from("/repo")]).is_err());
        assert_eq!(std::fs::read_to_string(&config).unwrap(), "not [ valid toml");
        let _ = std::fs::remove_dir_all(config.parent().unwrap());
    }

    #[test]
    fn windows_style_backslashes_and_quotes_are_escaped() {
        let config = temp_config("escape");
        let path = PathBuf::from(r#"C:\Users\dev\my "repo""#);
        assert_eq!(ensure_trusted_in_config(&config, &[path.clone()]), Ok(1));
        assert_eq!(trusted_paths(&config), vec![path.to_string_lossy().into_owned()]);
        let _ = std::fs::remove_dir_all(config.parent().unwrap());
    }
}
