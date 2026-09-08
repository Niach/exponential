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
    crate::atomic_config::replace_preserving_mode(config, &updated)?;
    Ok(missing.len())
}

/// EXP-758: drop the `[projects."<path>"]` tables [`ensure_trusted`] appended
/// for `paths` (the raw and canonical spellings both, like the claude side).
///
/// [`crate::scratch::reclaim`] removes a repo-less run's directory and its
/// claude trust entries; without this the codex config kept growing a dead
/// `[projects]` block per reclaimed action dir. Best-effort and never
/// load-bearing: failures are logged, the config is left as it was, and codex
/// simply shows its trust screen again if a live path was ever dropped.
pub fn forget(paths: &[PathBuf]) {
    // Same reason as [`ensure_trusted`]: unit tests exercise the `_in_config`
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
    let Some(home) = codex_home() else {
        return;
    };
    match forget_in_config(&home.join("config.toml"), paths) {
        Ok(0) => {}
        Ok(dropped) => log::info!(
            "codex trust: dropped {dropped} project entr{}",
            if dropped == 1 { "y" } else { "ies" }
        ),
        Err(err) => log::warn!("codex trust: {err}"),
    }
}

/// The testable core of [`forget`]: remove each named project's TABLE BLOCK
/// textually, returning how many were present.
///
/// Textual, not a TOML round-trip, for the same reason [`ensure_trusted`]
/// appends text: `config.toml` is the USER's codex config, and re-serializing
/// it would rewrite their comments, ordering and formatting to satisfy a
/// removal we own two lines of. A block runs from its `[projects."<path>"]`
/// header to the next table header (or EOF); the result must still parse, or
/// nothing is written.
fn forget_in_config(config: &Path, paths: &[PathBuf]) -> Result<usize, String> {
    let existing = match std::fs::read_to_string(config) {
        Ok(existing) => existing,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(err) => return Err(format!("read {}: {err}", config.display())),
    };
    if existing.trim().is_empty() {
        return Ok(0);
    }
    let wanted: Vec<String> = paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    let mut dropped: Vec<String> = Vec::new();
    let mut kept = String::with_capacity(existing.len());
    // `true` while the lines belong to a block being dropped — everything up
    // to the next table header.
    let mut dropping = false;
    // `split_inclusive` keeps each line's own terminator, so a CRLF config
    // (Windows) survives this untouched — it is the user's file, not ours.
    for line in existing.split_inclusive('\n') {
        match table_header_key(line) {
            Some(header) => {
                dropping = header
                    .as_deref()
                    .is_some_and(|key| wanted.iter().any(|path| path == key));
                if dropping {
                    if let Some(key) = header {
                        dropped.push(key);
                    }
                    continue;
                }
            }
            None if dropping => continue,
            None => {}
        }
        kept.push_str(line);
    }
    if dropped.is_empty() {
        return Ok(0);
    }
    // Never leave behind something codex's own parser would reject.
    toml::from_str::<toml::Value>(&kept)
        .map_err(|err| format!("config would not parse after the removal: {err}"))?;
    crate::atomic_config::replace_preserving_mode(config, &kept)?;
    dropped.sort();
    dropped.dedup();
    Ok(dropped.len())
}

/// A table header line, decoded by TOML itself so every quoting style
/// (`[projects."/a/b"]`, `[projects.'/a/b']`) resolves to the same key:
///
/// - `None` — not a table header at all (a value line, a comment, blank).
/// - `Some(None)` — a header that is not one project's (`[projects]`,
///   `[model_providers.x]`); it still ENDS a block being dropped.
/// - `Some(Some(key))` — `[projects.<key>]`, with `key` the decoded path.
fn table_header_key(line: &str) -> Option<Option<String>> {
    let trimmed = line.trim();
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return None;
    }
    // An array-of-tables header is still a header, just never a project's.
    if trimmed.starts_with("[[") {
        return Some(None);
    }
    let Ok(parsed) = toml::from_str::<toml::Value>(&format!("{trimmed}\n")) else {
        return Some(None);
    };
    let key = parsed
        .get("projects")
        .and_then(toml::Value::as_table)
        .and_then(|table| table.keys().next().cloned());
    Some(key)
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

    /// EXP-758: the reclaim side. The named block goes, everything around it
    /// survives BYTE for byte (it is the user's own config), and the raw +
    /// canonical pair is one call.
    #[test]
    fn forget_drops_only_the_named_block() {
        let config = temp_config("forget");
        std::fs::create_dir_all(config.parent().unwrap()).unwrap();
        std::fs::write(
            &config,
            "# my codex config\nmodel = \"gpt-5.6-terra\"\n\n\
[projects.\"/keep/repo\"]\ntrust_level = \"trusted\"\n\n\
[projects.\"/data/actions/builtin_chat\"]\ntrust_level = \"trusted\"\n\n\
[projects.\"/private/data/actions/builtin_chat\"]\ntrust_level = \"trusted\"\n\n\
[mcp_servers.other]\nurl = \"http://x\"\n",
        )
        .unwrap();
        let dropped = forget_in_config(
            &config,
            &[
                PathBuf::from("/data/actions/builtin_chat"),
                PathBuf::from("/private/data/actions/builtin_chat"),
            ],
        )
        .unwrap();
        assert_eq!(dropped, 2);
        let updated = std::fs::read_to_string(&config).unwrap();
        assert_eq!(trusted_paths(&config), vec!["/keep/repo".to_string()]);
        assert!(updated.starts_with("# my codex config\nmodel = \"gpt-5.6-terra\"\n"));
        assert!(updated.contains("[mcp_servers.other]\nurl = \"http://x\"\n"));
        assert!(!updated.contains("builtin_chat"));
        // Idempotent: nothing left to drop, nothing rewritten.
        let before = updated;
        assert_eq!(
            forget_in_config(&config, &[PathBuf::from("/data/actions/builtin_chat")]),
            Ok(0)
        );
        assert_eq!(std::fs::read_to_string(&config).unwrap(), before);
        let _ = std::fs::remove_dir_all(config.parent().unwrap());
    }

    /// A missing config is nothing to forget, and a config whose only
    /// `projects` entry belongs to somebody else is left alone.
    #[test]
    fn forget_is_a_no_op_without_a_matching_entry() {
        let missing = temp_config("forget-missing");
        assert_eq!(forget_in_config(&missing, &[PathBuf::from("/x")]), Ok(0));

        let config = temp_config("forget-other");
        std::fs::create_dir_all(config.parent().unwrap()).unwrap();
        let existing = "[projects.\"/other/repo\"]\ntrust_level = \"trusted\"\n";
        std::fs::write(&config, existing).unwrap();
        assert_eq!(forget_in_config(&config, &[PathBuf::from("/x")]), Ok(0));
        assert_eq!(std::fs::read_to_string(&config).unwrap(), existing);
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
