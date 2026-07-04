//! Run-config → terminal glue (masterplan-v3 §7.4/§7.5, EXP-2d/e) — the
//! gpui-free core the run bar drives.
//!
//! §7.4's contract: the `coding`/`ui` layer never touches PTYs; a launch
//! builds a [`SpawnSpec`] (**argv-direct — never a shell**, §6.13: `$SHELL
//! -lc "<command>"` would reintroduce shell parsing of the stored string and
//! reverse the §7.3.5 Trust-&-Run posture) and hands it to the §06
//! `TerminalManager` as a `TabKind::Run(run_config_id)` tab. This module
//! owns:
//!
//! - [`resolve_cwd`] — joins the (validated, relative) config `cwd` onto the
//!   active worktree/clone root; re-checks the server's §7.3.3 rules
//!   desktop-side (defense in depth — never trust the fetched row);
//! - [`run_spawn_spec`] — the §7.4 `SpawnSpec` builder (program = `argv[0]`,
//!   args = `argv[1..]`, blocked env keys stripped client-side too);
//! - [`run_root`] — the root rule (v4 §4.6): ALWAYS the project's trunk clone
//!   root `<repos_root>/<owner>/<name>` (the run bar is trunk-only — no
//!   active-worktree branch); [`shell_cwd`] — the `+` shell-tab cwd rule
//!   (trunk root when it exists on disk, else `$HOME`);
//! - [`play_state`] — the §7.5 play↔stop state machine (pure, unit-tested);
//! - [`terminate`]/[`force_kill`] + [`STOP_GRACE`] — §7.5 stop semantics
//!   (SIGTERM, then SIGKILL after a grace period; portable-pty's own killer
//!   only speaks SIGHUP);
//! - [`parse_argv_line`]/[`format_argv_line`] — the §7.3.3 shell-*like*
//!   tokenizer (verbatim port of `apps/web/src/lib/run-configs.ts`) for the
//!   editor dialog's single monospace command line;
//! - [`sort_order_after_move`] — fractional-index reorder for the editor
//!   (`update.sortOrder`, §7.3.2).

use std::path::{Path, PathBuf};
use std::time::Duration;

use api::run_configs::RunConfig;
use terminal::SpawnSpec;

use crate::git_worktree::clone_path;

/// §7.5: grace period between SIGTERM and SIGKILL on stop.
pub const STOP_GRACE: Duration = Duration::from_secs(3);

// ---------------------------------------------------------------------------
// cwd validation + resolution (§7.3.3 rules, re-checked desktop-side)
// ---------------------------------------------------------------------------

/// Port of the server's `runConfigCwdError`: cwd must stay inside the
/// checkout — relative, no `..` segments. `None` = valid.
pub fn cwd_error(cwd: &str) -> Option<&'static str> {
    if cwd.starts_with('/') || cwd.starts_with('\\') || has_drive_prefix(cwd) {
        return Some("cwd must be a relative path inside the repository");
    }
    if cwd.split(['/', '\\']).any(|segment| segment == "..") {
        return Some("cwd must not contain \"..\" segments");
    }
    None
}

/// `^[A-Za-z]:[\\/]` — the Windows drive-absolute form the server rejects.
fn has_drive_prefix(cwd: &str) -> bool {
    let bytes = cwd.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

/// §7.4 `resolve_cwd`: join the validated relative `cwd` onto the active
/// worktree/clone root; `None`/empty = the root itself. Fails (instead of
/// spawning somewhere surprising) when the fetched row violates the §7.3.3
/// rules the server should have enforced.
pub fn resolve_cwd(root: &Path, cwd: Option<&str>) -> Result<PathBuf, String> {
    let Some(raw) = cwd else {
        return Ok(root.to_path_buf());
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(root.to_path_buf());
    }
    if let Some(error) = cwd_error(trimmed) {
        return Err(error.to_string());
    }
    Ok(root.join(trimmed))
}

// ---------------------------------------------------------------------------
// env hygiene (mirror of the server's sanitizeRunConfigEnv — defense in depth)
// ---------------------------------------------------------------------------

/// Keys that would hijack the spawned process beyond its own argv: `PATH`
/// swaps the resolved binary, `LD_PRELOAD`/`DYLD_*` inject code.
/// Case-insensitive (verbatim server rule).
pub fn is_blocked_env_key(key: &str) -> bool {
    let upper = key.to_uppercase();
    upper == "PATH" || upper == "LD_PRELOAD" || upper.starts_with("DYLD_")
}

// ---------------------------------------------------------------------------
// SpawnSpec (§7.4) — argv-direct, never a shell
// ---------------------------------------------------------------------------

/// Build the §7.4 [`SpawnSpec`] for a run config: `program = argv[0]`,
/// `args = argv[1..]`, cwd resolved against `root`, env overlaid with the
/// blocked keys stripped (the server already strips them; a row that somehow
/// carries one must still not reach the child).
pub fn run_spawn_spec(config: &RunConfig, root: &Path) -> Result<SpawnSpec, String> {
    let program = config
        .argv
        .first()
        .filter(|program| !program.trim().is_empty())
        .ok_or_else(|| format!("run config \"{}\" has an empty command", config.name))?;
    let cwd = resolve_cwd(root, config.cwd.as_deref())?;
    let mut spec = SpawnSpec::new(program.clone())
        .args(config.argv[1..].iter().cloned())
        .cwd(cwd);
    for (key, value) in &config.env {
        if is_blocked_env_key(key) {
            continue;
        }
        spec = spec.env(key.clone(), value.clone());
    }
    Ok(spec)
}

/// Root rule (v4 §4.6): run configs ALWAYS resolve against the project's
/// **trunk** clone root (`<repos_root>/<owner>/<name>`) — never an issue
/// worktree. Running something inside a worktree is a power move done from a
/// worktree shell tab, not from the run bar (v4 §4.2: the run bar is
/// trunk-only). There is deliberately no active-worktree branch here.
pub fn run_root(repos_root: &Path, repo_full_name: &str) -> PathBuf {
    clone_path(repos_root, repo_full_name)
}

/// `+` shell-tab cwd rule (v4 §4.6): open at the **trunk** clone root when it
/// exists on disk, else `None` → the caller's `$HOME` fallback ("`$HOME` only
/// while the clone doesn't exist yet or on non-project screens"). `trunk_root`
/// is `None` off a project screen (no trunk to point at); `Some(root)` on a
/// project screen, where the clone may or may not have finished cloning yet —
/// an absent directory degrades to `$HOME` rather than spawning the shell in a
/// path that isn't there.
pub fn shell_cwd(trunk_root: Option<PathBuf>) -> Option<PathBuf> {
    trunk_root.filter(|root| root.is_dir())
}

// ---------------------------------------------------------------------------
// Play/stop state machine (§7.5)
// ---------------------------------------------------------------------------

/// What the run-bar play button is right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayState {
    /// Nothing launchable selected — play renders disabled.
    Disabled,
    /// Selected config has no live tab — clicking launches (an exited tab is
    /// re-used/re-run, §7.5).
    Play,
    /// Selected config has a live child — "play becomes a stop button".
    Stop,
}

/// Derive the button state from the selected config and the window's run
/// tabs (`(run_config_id, is_running)` — the §06 manager's `TabKind::Run`
/// tabs). Exited tabs do NOT hold the button in Stop: §6.7 flips it back on
/// the exit edge.
pub fn play_state(selected: Option<&str>, run_tabs: &[(String, bool)]) -> PlayState {
    let Some(selected) = selected else {
        return PlayState::Disabled;
    };
    let running = run_tabs
        .iter()
        .any(|(config_id, is_running)| config_id == selected && *is_running);
    if running {
        PlayState::Stop
    } else {
        PlayState::Play
    }
}

// ---------------------------------------------------------------------------
// Stop signals (§7.5: SIGTERM → grace → SIGKILL)
// ---------------------------------------------------------------------------

/// Ask the child to exit (SIGTERM). The §06 wait thread captures the exit and
/// flips the tab to `Exited` — this function only signals.
pub fn terminate(pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    #[cfg(not(unix))]
    let _ = pid;
}

/// The post-grace hammer (SIGKILL). Callers must re-check the tab is still
/// running first — never signal a reaped (potentially reused) pid.
pub fn force_kill(pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(pid as i32, libc::SIGKILL);
    }
    #[cfg(not(unix))]
    let _ = pid;
}

// ---------------------------------------------------------------------------
// argv <-> single editable command line (§7.3.3, verbatim web port)
// ---------------------------------------------------------------------------

/// Shell-*like* tokenizer for the editor's monospace command line:
/// whitespace-split; `'…'` literal; `"…"` with `\"`/`\\` escapes; backslash
/// escapes the next char outside quotes. **Nothing is ever run through a
/// shell** — the desktop spawns the resulting argv as-is.
pub fn parse_argv_line(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut argv = Vec::new();
    let mut current = String::new();
    let mut in_token = false;
    let mut quote: Option<char> = None;

    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        match quote {
            Some('\'') => {
                if ch == '\'' {
                    quote = None;
                } else {
                    current.push(ch);
                }
            }
            Some('"') => {
                if ch == '\\' && matches!(chars.get(i + 1), Some(&'"') | Some(&'\\')) {
                    current.push(chars[i + 1]);
                    i += 1;
                } else if ch == '"' {
                    quote = None;
                } else {
                    current.push(ch);
                }
            }
            _ => {
                if ch == '\'' || ch == '"' {
                    quote = Some(ch);
                    in_token = true;
                } else if ch == '\\' && i + 1 < chars.len() {
                    current.push(chars[i + 1]);
                    in_token = true;
                    i += 1;
                } else if ch.is_whitespace() {
                    if in_token {
                        argv.push(std::mem::take(&mut current));
                        in_token = false;
                    }
                } else {
                    current.push(ch);
                    in_token = true;
                }
            }
        }
        i += 1;
    }
    // An unterminated quote just consumes the rest of the line — forgiving,
    // like the web editor; format_argv_line output is always terminated.
    if in_token {
        argv.push(current);
    }
    argv
}

/// Round-trips through [`parse_argv_line`]: plain args stay bare, anything
/// with whitespace/quotes/backslashes (or empty) is double-quoted with `\"`
/// and `\\` escapes.
pub fn format_argv_line(argv: &[String]) -> String {
    argv.iter()
        .map(|arg| {
            let bare = !arg.is_empty()
                && !arg
                    .chars()
                    .any(|c| c.is_whitespace() || matches!(c, '\'' | '"' | '\\'));
            if bare {
                return arg.clone();
            }
            let mut quoted = String::with_capacity(arg.len() + 2);
            quoted.push('"');
            for c in arg.chars() {
                if matches!(c, '\\' | '"') {
                    quoted.push('\\');
                }
                quoted.push(c);
            }
            quoted.push('"');
            quoted
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// env <-> "KEY=value per line" (the editor's env field)
// ---------------------------------------------------------------------------

/// Parse the editor's env textarea: one `KEY=value` per line, blank lines
/// ignored, keys following the conventional NAME grammar
/// (`[A-Za-z_][A-Za-z0-9_]*` — the server's envSchema). Values keep their
/// exact text after the first `=`.
pub fn parse_env_lines(raw: &str) -> Result<std::collections::BTreeMap<String, String>, String> {
    let mut env = std::collections::BTreeMap::new();
    for (ix, line) in raw.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            return Err(format!("Line {}: expected KEY=value", ix + 1));
        };
        let key = key.trim();
        let valid = !key.is_empty()
            && key
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !valid {
            return Err(format!("Line {}: invalid variable name \"{key}\"", ix + 1));
        }
        env.insert(key.to_string(), value.to_string());
    }
    Ok(env)
}

/// Inverse of [`parse_env_lines`] for pre-filling the editor.
pub fn format_env_lines(env: &std::collections::BTreeMap<String, String>) -> String {
    env.iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------
// Reorder (§7.3.2: reorder = update.sortOrder)
// ---------------------------------------------------------------------------

/// New `sortOrder` for the row at `from` landing at index `to` (indices into
/// the CURRENT display order, `orders` = the current sortOrder column):
/// midpoint between the new neighbors, or first−1 / last+1 at the edges.
/// `None` = no-op move.
pub fn sort_order_after_move(orders: &[f64], from: usize, to: usize) -> Option<f64> {
    if from == to || from >= orders.len() || to >= orders.len() {
        return None;
    }
    let mut rest = orders.to_vec();
    rest.remove(from);
    let before = to.checked_sub(1).map(|i| rest[i]);
    let after = rest.get(to).copied();
    Some(match (before, after) {
        (Some(b), Some(a)) => (b + a) / 2.0,
        (None, Some(a)) => a - 1.0,
        (Some(b), None) => b + 1.0,
        (None, None) => 1.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn config(name: &str, argv: &[&str], cwd: Option<&str>) -> RunConfig {
        RunConfig {
            id: "rc-1".to_string(),
            project_id: "proj-1".to_string(),
            name: name.to_string(),
            argv: argv.iter().map(|s| s.to_string()).collect(),
            cwd: cwd.map(str::to_string),
            env: BTreeMap::new(),
            sort_order: 0.0,
            created_at: None,
            updated_at: None,
        }
    }

    // ---- cwd rules (mirror the server vectors) -------------------------

    #[test]
    fn cwd_rejects_absolute_and_dotdot() {
        assert!(cwd_error("/etc").is_some());
        assert!(cwd_error("\\\\share").is_some());
        assert!(cwd_error("C:\\code").is_some());
        assert!(cwd_error("c:/code").is_some());
        assert!(cwd_error("../up").is_some());
        assert!(cwd_error("apps/../../up").is_some());
        assert!(cwd_error("apps\\..\\up").is_some());
        assert_eq!(cwd_error("apps/web"), None);
        assert_eq!(cwd_error("a..b/c"), None); // ".." must be a full segment
    }

    #[test]
    fn resolve_cwd_joins_or_defaults_to_root() {
        let root = Path::new("/repos/acme/web");
        assert_eq!(resolve_cwd(root, None).unwrap(), root);
        assert_eq!(resolve_cwd(root, Some("")).unwrap(), root);
        assert_eq!(resolve_cwd(root, Some("  ")).unwrap(), root);
        assert_eq!(
            resolve_cwd(root, Some("apps/web")).unwrap(),
            Path::new("/repos/acme/web/apps/web")
        );
        assert!(resolve_cwd(root, Some("/abs")).is_err());
        assert!(resolve_cwd(root, Some("../out")).is_err());
    }

    // ---- SpawnSpec (§7.4: argv-direct, never a shell) -------------------

    #[test]
    fn spawn_spec_is_argv_direct() {
        let mut cfg = config("dev", &["bun", "run", "dev"], Some("apps/web"));
        cfg.env.insert("PORT".to_string(), "5173".to_string());
        let spec = run_spawn_spec(&cfg, Path::new("/repos/acme/web")).unwrap();
        assert_eq!(spec.program, "bun"); // argv[0], not $SHELL
        assert_eq!(spec.args, vec!["run", "dev"]); // argv[1..], no -lc
        assert_eq!(
            spec.cwd.as_deref(),
            Some(Path::new("/repos/acme/web/apps/web"))
        );
        assert!(spec
            .env
            .contains(&("PORT".to_string(), "5173".to_string())));
    }

    #[test]
    fn spawn_spec_strips_blocked_env_keys() {
        // Server already strips these — the desktop must too (§7.3.5 defense
        // in depth: a hand-crafted row must not hijack the child).
        let mut cfg = config("dev", &["bun", "dev"], None);
        cfg.env.insert("path".to_string(), "/evil".to_string());
        cfg.env
            .insert("LD_PRELOAD".to_string(), "evil.so".to_string());
        cfg.env
            .insert("DYLD_INSERT_LIBRARIES".to_string(), "evil.dylib".to_string());
        cfg.env.insert("NODE_ENV".to_string(), "test".to_string());
        let spec = run_spawn_spec(&cfg, Path::new("/root")).unwrap();
        assert_eq!(
            spec.env,
            vec![("NODE_ENV".to_string(), "test".to_string())]
        );
    }

    #[test]
    fn spawn_spec_rejects_empty_program_or_bad_cwd() {
        assert!(run_spawn_spec(&config("x", &[], None), Path::new("/r")).is_err());
        assert!(run_spawn_spec(&config("x", &["  "], None), Path::new("/r")).is_err());
        assert!(
            run_spawn_spec(&config("x", &["ls"], Some("../up")), Path::new("/r")).is_err()
        );
    }

    #[test]
    fn run_root_is_the_clone_path() {
        assert_eq!(
            run_root(Path::new("/repos"), "acme/web"),
            Path::new("/repos/acme/web")
        );
    }

    #[test]
    fn shell_cwd_uses_trunk_only_when_it_exists() {
        // Off a project screen → no trunk to point at → $HOME (None).
        assert_eq!(shell_cwd(None), None);
        // Project screen but the clone hasn't landed yet → $HOME (None).
        assert_eq!(
            shell_cwd(Some(PathBuf::from("/repos/acme/does-not-exist"))),
            None
        );
        // Existing trunk clone dir → open there. Use a real dir (the crate's
        // own manifest dir) so `is_dir()` holds without touching the network.
        let real = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        assert_eq!(shell_cwd(Some(real.clone())), Some(real));
    }

    // ---- play/stop state machine (§7.5) ---------------------------------

    #[test]
    fn play_state_machine() {
        let tabs = |entries: &[(&str, bool)]| -> Vec<(String, bool)> {
            entries
                .iter()
                .map(|(id, running)| (id.to_string(), *running))
                .collect()
        };

        // Nothing selected → disabled.
        assert_eq!(play_state(None, &[]), PlayState::Disabled);
        // Selected, no tab yet → play.
        assert_eq!(play_state(Some("rc-1"), &[]), PlayState::Play);
        // Selected + live tab → stop.
        assert_eq!(
            play_state(Some("rc-1"), &tabs(&[("rc-1", true)])),
            PlayState::Stop
        );
        // Child exited → §6.7 flips back to play (exit-code strip shows).
        assert_eq!(
            play_state(Some("rc-1"), &tabs(&[("rc-1", false)])),
            PlayState::Play
        );
        // Another config's live tab doesn't hold OUR button in stop.
        assert_eq!(
            play_state(Some("rc-1"), &tabs(&[("rc-2", true)])),
            PlayState::Play
        );
        // Multiple tabs: any live tab for the selected config wins.
        assert_eq!(
            play_state(Some("rc-1"), &tabs(&[("rc-1", false), ("rc-1", true)])),
            PlayState::Stop
        );
    }

    // ---- argv line round-trip (§7.3.3, web parity) ----------------------

    #[test]
    fn parse_argv_line_web_vectors() {
        assert_eq!(parse_argv_line("bun run dev"), vec!["bun", "run", "dev"]);
        assert_eq!(
            parse_argv_line("echo 'hello world'"),
            vec!["echo", "hello world"]
        );
        assert_eq!(
            parse_argv_line(r#"echo "a \"b\" c""#),
            vec!["echo", r#"a "b" c"#]
        );
        assert_eq!(
            parse_argv_line(r#"printf "back\\slash""#),
            vec!["printf", r"back\slash"]
        );
        assert_eq!(parse_argv_line(r"a\ b"), vec!["a b"]); // escape outside quotes
        assert_eq!(parse_argv_line("  spaced   out  "), vec!["spaced", "out"]);
        assert_eq!(parse_argv_line(""), Vec::<String>::new());
        assert_eq!(parse_argv_line("\"\""), vec![""]); // explicit empty arg
        assert_eq!(parse_argv_line("'unterminated rest"), vec!["unterminated rest"]);
        // Adjacent quoted + bare segments concatenate into one token.
        assert_eq!(parse_argv_line("pre'mid'post"), vec!["premidpost"]);
    }

    #[test]
    fn format_then_parse_round_trips() {
        let cases: Vec<Vec<String>> = vec![
            vec!["bun".into(), "run".into(), "dev".into()],
            vec!["echo".into(), "hello world".into()],
            vec!["echo".into(), r#"quote " here"#.into()],
            vec!["printf".into(), r"back\slash".into()],
            vec!["x".into(), "".into()],
            vec!["single'quote".into()],
        ];
        for argv in cases {
            let line = format_argv_line(&argv);
            assert_eq!(parse_argv_line(&line), argv, "line: {line}");
        }
    }

    #[test]
    fn format_argv_line_keeps_plain_args_bare() {
        assert_eq!(
            format_argv_line(&["bun".into(), "run".into(), "dev".into()]),
            "bun run dev"
        );
        assert_eq!(
            format_argv_line(&["echo".into(), "a b".into()]),
            r#"echo "a b""#
        );
    }

    // ---- reorder --------------------------------------------------------

    #[test]
    fn sort_order_after_move_midpoints_and_edges() {
        let orders = [1.0, 2.0, 3.0];
        // Move last to front → before first (0.0).
        assert_eq!(sort_order_after_move(&orders, 2, 0), Some(0.0));
        // Move first to end → after last (4.0).
        assert_eq!(sort_order_after_move(&orders, 0, 2), Some(4.0));
        // Move first between the remaining two → midpoint of 2 and 3.
        assert_eq!(sort_order_after_move(&orders, 0, 1), Some(2.5));
        // Move up by one: lands between "two above" and "one above".
        assert_eq!(sort_order_after_move(&orders, 2, 1), Some(1.5));
        // No-ops.
        assert_eq!(sort_order_after_move(&orders, 1, 1), None);
        assert_eq!(sort_order_after_move(&orders, 5, 0), None);
        assert_eq!(sort_order_after_move(&[], 0, 0), None);
    }

    #[test]
    fn env_lines_round_trip() {
        let parsed = parse_env_lines("PORT=5173\n\nNODE_ENV=dev with spaces\n_X=a=b\n").unwrap();
        assert_eq!(parsed.get("PORT").map(String::as_str), Some("5173"));
        assert_eq!(
            parsed.get("NODE_ENV").map(String::as_str),
            Some("dev with spaces")
        );
        // Value keeps everything after the FIRST '='.
        assert_eq!(parsed.get("_X").map(String::as_str), Some("a=b"));
        assert_eq!(parse_env_lines(&format_env_lines(&parsed)).unwrap(), parsed);
    }

    #[test]
    fn env_lines_reject_bad_shapes() {
        assert!(parse_env_lines("NO_EQUALS_HERE").is_err());
        assert!(parse_env_lines("1BAD=x").is_err());
        assert!(parse_env_lines("BAD-NAME=x").is_err());
        assert!(parse_env_lines("=x").is_err());
        assert_eq!(parse_env_lines("").unwrap().len(), 0);
    }

    #[test]
    fn blocked_env_keys_are_case_insensitive() {
        assert!(is_blocked_env_key("PATH"));
        assert!(is_blocked_env_key("path"));
        assert!(is_blocked_env_key("Ld_Preload"));
        assert!(is_blocked_env_key("DYLD_LIBRARY_PATH"));
        assert!(is_blocked_env_key("dyld_insert_libraries"));
        assert!(!is_blocked_env_key("NODE_ENV"));
        assert!(!is_blocked_env_key("MY_PATH")); // suffix ≠ PATH
    }
}
