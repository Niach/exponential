//! Coding-side settings (masterplan-v3 §7.7, DC-3) — the launcher knobs:
//!
//! | Setting            | Default              | Used by                      |
//! |--------------------|----------------------|------------------------------|
//! | Claude CLI path    | `claude`             | §7.1 step 7 spawn + doctor   |
//! | Repos root         | `~/Exponential/repos`| §7.1 step 3 worktree layout  |
//! | Branch prefix      | `exp/`               | `<prefix><IDENTIFIER>` branch|
//! | Claude model       | `fable`              | §7.1 step 7 `--model` argv   |
//!
//! Persisted to a small `settings.json` in the app data dir — **local,
//! per-install, never synced**. Saving merges into the existing JSON object
//! (unknown top-level keys from other subsystems are preserved), so this file
//! can be shared with future ui/steer settings without clobbering them.
//!
//! **There is deliberately NO personal-API-key field here**: the
//! `expu_` key is hidden state in the api crate's file token store; settings
//! render a status row + Regenerate, never a value (§7.2).

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const DEFAULT_CLAUDE_PATH: &str = "claude";
pub const DEFAULT_REPOS_ROOT: &str = "~/Exponential/repos";
pub const DEFAULT_BRANCH_PREFIX: &str = "exp/";
/// §7.7 default coding model — passed as `--model fable` on every spawn.
/// Explicit-always so the user's `claude` CLI default is never silently
/// consumed by coding sessions or E2E tests.
pub const DEFAULT_CLAUDE_MODEL: &str = "fable";
/// The `--model` aliases the CLI accepts (and the ui selects offer) —
/// [`Settings::load`] normalizes anything else back to the default.
pub const MODEL_ALIASES: [&str; 3] = ["fable", "opus", "sonnet"];
/// The `--effort` levels the CLI accepts (blank = omit the flag) —
/// [`Settings::load`] normalizes anything else back to blank.
pub const EFFORT_LEVELS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];
/// Default reasoning effort — EMPTY, meaning "omit --effort" (the CLI's own
/// default applies). Blank is a VALID value here (EXP-56).
pub const DEFAULT_CLAUDE_EFFORT: &str = "";
/// Default subagent model for RELEASE runs (EXP-56) — EMPTY, meaning
/// "inherit `claude_model` at use". Blank is a VALID value; `load` must NOT
/// degrade it (mirrors [`DEFAULT_CLAUDE_EFFORT`]).
pub const DEFAULT_SUBAGENT_MODEL: &str = "";
/// Default subagent effort for RELEASE runs — EMPTY = inherit/omit. Blank is
/// VALID; never degraded on load.
pub const DEFAULT_SUBAGENT_EFFORT: &str = "";

/// The resolved coding settings. `repos_root` is stored in its raw
/// (possibly `~`-prefixed) form and tilde-expanded at use
/// ([`Settings::repos_root_path`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Program name or absolute path of the Claude CLI (§7.7 — the doctor's
    /// target and the launcher's spawn program, used verbatim).
    pub claude_path: String,
    /// Raw repos-&-worktrees root; may start with `~`.
    pub repos_root: String,
    /// Prepended to the issue identifier for the coding branch (`exp/EXP-42`).
    pub branch_prefix: String,
    /// The Claude model, passed as `--model <value>` on every spawn (§7.7 —
    /// explicit-always). One of [`MODEL_ALIASES`]; `load` normalizes anything
    /// else to [`DEFAULT_CLAUDE_MODEL`].
    pub claude_model: String,
    /// Reasoning effort, passed as `--effort <value>` when non-blank. One of
    /// [`EFFORT_LEVELS`] or blank (= omit the flag); `load` normalizes
    /// anything else to blank.
    pub claude_effort: String,
    /// RELEASE-run subagent model default (the launch dialog's prefill,
    /// EXP-56). Blank is VALID = inherit `claude_model` at use; otherwise one
    /// of [`MODEL_ALIASES`] (`load` normalizes anything else to blank).
    pub subagent_model: String,
    /// RELEASE-run subagent effort default. Blank is VALID = inherit/omit;
    /// otherwise one of [`EFFORT_LEVELS`] (`load` normalizes anything else
    /// to blank).
    pub subagent_effort: String,
    /// RELEASE-run "dynamic workflows" (ultracode) default — ON by default.
    /// A MISSING key fills from this struct's manual [`Default`] impl (the
    /// container-level `#[serde(default)]` uses `Settings::default()`, not
    /// `bool::default()`), so absent stays `true` — locked by a test below.
    pub release_ultracode: bool,
    /// RELEASE-run native plan mode default — OFF by default (an orchestrator
    /// usually runs unattended). Replaces the old `releaseAutonomous` key,
    /// which is simply ignored on load.
    pub release_plan_mode: bool,
    /// SINGLE-ISSUE-run native plan mode default — ON by default (Claude
    /// presents a plan for approval in the terminal before editing).
    pub issue_plan_mode: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            claude_path: DEFAULT_CLAUDE_PATH.to_string(),
            repos_root: DEFAULT_REPOS_ROOT.to_string(),
            branch_prefix: DEFAULT_BRANCH_PREFIX.to_string(),
            claude_model: DEFAULT_CLAUDE_MODEL.to_string(),
            claude_effort: DEFAULT_CLAUDE_EFFORT.to_string(),
            subagent_model: DEFAULT_SUBAGENT_MODEL.to_string(),
            subagent_effort: DEFAULT_SUBAGENT_EFFORT.to_string(),
            release_ultracode: true,
            release_plan_mode: false,
            issue_plan_mode: true,
        }
    }
}

impl Settings {
    /// Canonical settings file location: `{data_dir}/settings.json`
    /// (`data_dir` = [`api::default_data_dir`]).
    pub fn default_path(data_dir: &Path) -> PathBuf {
        data_dir.join("settings.json")
    }

    /// Load settings, tolerating a missing/corrupt file (→ defaults) and
    /// unknown keys (→ ignored, but see [`Settings::save`] which preserves
    /// them). Empty strings degrade to the field default and model/effort
    /// values normalize to the known alias sets, so a hand-edited file can
    /// never produce an unusable launcher or an argv the CLI rejects.
    pub fn load(path: &Path) -> Settings {
        let mut settings = fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Settings>(&raw).ok())
            .unwrap_or_default();
        let defaults = Settings::default();
        if settings.claude_path.trim().is_empty() {
            settings.claude_path = defaults.claude_path;
        }
        if settings.repos_root.trim().is_empty() {
            settings.repos_root = defaults.repos_root;
        }
        if settings.branch_prefix.trim().is_empty() {
            settings.branch_prefix = defaults.branch_prefix;
        }
        settings.claude_model =
            normalize_choice(&settings.claude_model, &MODEL_ALIASES, DEFAULT_CLAUDE_MODEL);
        settings.subagent_model = normalize_choice(&settings.subagent_model, &MODEL_ALIASES, "");
        settings.claude_effort = normalize_choice(&settings.claude_effort, &EFFORT_LEVELS, "");
        settings.subagent_effort = normalize_choice(&settings.subagent_effort, &EFFORT_LEVELS, "");
        settings
    }

    /// Persist, merging over any existing JSON object so top-level keys owned
    /// by other subsystems survive a coding-settings save.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut root = fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
            .filter(serde_json::Value::is_object)
            .unwrap_or_else(|| serde_json::Value::Object(Default::default()));
        let ours = serde_json::to_value(self).expect("settings serialize cannot fail");
        if let (Some(target), Some(source)) = (root.as_object_mut(), ours.as_object()) {
            for (key, value) in source {
                target.insert(key.clone(), value.clone());
            }
        }
        let mut rendered = serde_json::to_string_pretty(&root).expect("render settings json");
        rendered.push('\n');
        fs::write(path, rendered)
    }

    /// The claude program to spawn / doctor-check. An explicit path (anything
    /// other than the bare default) is used verbatim; the bare `claude` is
    /// probed against the well-known per-user install locations before
    /// falling back to PATH resolution. GUI apps launched from the desktop
    /// don't inherit a login-shell PATH, so bare `claude` can otherwise
    /// resolve to a stale system-wide install (an old `npm -g` shim in
    /// /usr/local/bin) while the user's shell runs the current native
    /// install from `~/.local/bin` — version skew that surfaces as startup
    /// warnings about invalid config files in the embedded terminal.
    pub fn resolved_claude_path(&self) -> String {
        resolve_claude_program(&self.claude_path, dirs::home_dir())
    }

    /// The `<repos_root>` of §7.1's worktree layout, tilde-expanded.
    pub fn repos_root_path(&self) -> PathBuf {
        expand_tilde(
            &self.repos_root,
            dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")),
        )
    }
}

/// Lowercase-trim `raw`; anything outside `allowed` (except blank, which
/// always maps to `fallback`) also maps to `fallback`. Keeps every persisted
/// model/effort value inside the closed alias sets the CLI accepts.
fn normalize_choice(raw: &str, allowed: &[&str], fallback: &str) -> String {
    let cleaned = raw.trim().to_ascii_lowercase();
    if allowed.contains(&cleaned.as_str()) {
        cleaned
    } else {
        fallback.to_string()
    }
}

/// See [`Settings::resolved_claude_path`]. Split out (with `home` injected)
/// for testability.
pub fn resolve_claude_program(raw: &str, home: Option<PathBuf>) -> String {
    if raw != DEFAULT_CLAUDE_PATH {
        return raw.to_string();
    }
    if let Some(home) = home {
        let candidates = [
            // The official native installer's location — the install the
            // user's login shell almost certainly runs.
            home.join(".local").join("bin").join("claude"),
            // Older `claude install` local location.
            home.join(".claude").join("local").join("claude"),
        ];
        for candidate in candidates {
            if candidate.is_file() {
                return candidate.to_string_lossy().into_owned();
            }
        }
    }
    raw.to_string()
}

/// `~` / `~/…` → `home`-rooted path; anything else passes through. (Only the
/// current user's home is expanded — `~other` is left verbatim, matching
/// common CLI behavior.)
pub fn expand_tilde(raw: &str, home: PathBuf) -> PathBuf {
    if raw == "~" {
        home
    } else if let Some(rest) = raw.strip_prefix("~/") {
        home.join(rest)
    } else {
        PathBuf::from(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let mut path = std::env::temp_dir();
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            path.push(format!(
                "exp-coding-settings-{tag}-{}-{nanos}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn bare_claude_prefers_the_per_user_native_install() {
        let dir = TempDir::new("resolve");
        // No install in the fake home → PATH fallback.
        assert_eq!(
            resolve_claude_program("claude", Some(dir.0.clone())),
            "claude"
        );
        // The native installer's binary wins over PATH.
        let bin = dir.0.join(".local").join("bin");
        fs::create_dir_all(&bin).unwrap();
        let installed = bin.join("claude");
        fs::write(&installed, "").unwrap();
        assert_eq!(
            resolve_claude_program("claude", Some(dir.0.clone())),
            installed.to_string_lossy()
        );
        // An explicit override is always used verbatim.
        assert_eq!(
            resolve_claude_program("/opt/homebrew/bin/claude", Some(dir.0.clone())),
            "/opt/homebrew/bin/claude"
        );
    }

    #[test]
    fn defaults_match_the_spec_table() {
        let settings = Settings::default();
        assert_eq!(settings.claude_path, "claude");
        assert_eq!(settings.repos_root, "~/Exponential/repos");
        assert_eq!(settings.branch_prefix, "exp/");
        assert_eq!(settings.claude_model, "fable");
        assert_eq!(settings.claude_effort, "");
        // EXP-56 release-run defaults: blank subagent fields (= inherit),
        // ultracode ON. Plan mode: ON for issue runs, OFF for release runs.
        assert_eq!(settings.subagent_model, "");
        assert_eq!(settings.subagent_effort, "");
        assert!(settings.release_ultracode);
        assert!(!settings.release_plan_mode);
        assert!(settings.issue_plan_mode);
    }

    #[test]
    fn missing_file_loads_defaults() {
        let dir = TempDir::new("missing");
        let settings = Settings::load(&dir.0.join("settings.json"));
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn corrupt_file_loads_defaults() {
        let dir = TempDir::new("corrupt");
        let path = dir.0.join("settings.json");
        fs::write(&path, "{not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
    }

    #[test]
    fn blank_values_degrade_to_defaults() {
        let dir = TempDir::new("blank");
        let path = dir.0.join("settings.json");
        fs::write(
            &path,
            r#"{"claudePath":"","reposRoot":"  ","branchPrefix":"","claudeModel":"  "}"#,
        )
        .unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
    }

    /// Blank effort is a VALID value ("omit --effort") — it must survive load
    /// as blank, and a known level round-trips (EXP-56).
    #[test]
    fn blank_effort_is_preserved_and_a_set_effort_round_trips() {
        let dir = TempDir::new("effort");
        let path = dir.0.join("settings.json");
        fs::write(&path, r#"{"claudeEffort":"  "}"#).unwrap();
        assert_eq!(Settings::load(&path).claude_effort, "");
        fs::write(&path, r#"{"claudeEffort":"high"}"#).unwrap();
        assert_eq!(Settings::load(&path).claude_effort, "high");
        // Absent key → the empty default.
        fs::write(&path, r#"{}"#).unwrap();
        assert_eq!(Settings::load(&path).claude_effort, "");
    }

    /// Model/effort values normalize on load: lowercase-trim into the closed
    /// alias sets; anything unknown falls back (model → fable, subagent
    /// model and efforts → blank) so the argv can never carry a value the
    /// CLI rejects.
    #[test]
    fn model_and_effort_values_normalize_on_load() {
        let dir = TempDir::new("normalize");
        let path = dir.0.join("settings.json");
        fs::write(
            &path,
            r#"{"claudeModel":" Opus ","subagentModel":"SONNET","claudeEffort":" XHigh ","subagentEffort":"max"}"#,
        )
        .unwrap();
        let settings = Settings::load(&path);
        assert_eq!(settings.claude_model, "opus");
        assert_eq!(settings.subagent_model, "sonnet");
        assert_eq!(settings.claude_effort, "xhigh");
        assert_eq!(settings.subagent_effort, "max");

        fs::write(
            &path,
            r#"{"claudeModel":"haiku","subagentModel":"gpt","claudeEffort":"extreme","subagentEffort":"ultracode"}"#,
        )
        .unwrap();
        let settings = Settings::load(&path);
        assert_eq!(settings.claude_model, "fable", "unknown model → fable");
        assert_eq!(settings.subagent_model, "", "unknown subagent model → inherit");
        assert_eq!(settings.claude_effort, "", "unknown effort → omit");
        assert_eq!(settings.subagent_effort, "");
    }

    /// EXP-56 release-run fields: MISSING keys must fill from the manual
    /// `Default` impl (container-level `#[serde(default)]`) — ultracode TRUE,
    /// issue plan mode TRUE, release plan mode FALSE. Blank subagent
    /// model/effort are VALID ("inherit"); explicit bools round-trip; the
    /// dead `releaseAutonomous` key is ignored.
    #[test]
    fn release_run_fields_fill_from_defaults_and_blanks_survive() {
        let dir = TempDir::new("release-fields");
        let path = dir.0.join("settings.json");
        fs::write(&path, r#"{"claudeModel":"sonnet"}"#).unwrap();
        let settings = Settings::load(&path);
        assert!(settings.release_ultracode, "missing key must default TRUE");
        assert!(!settings.release_plan_mode, "missing key must default FALSE");
        assert!(settings.issue_plan_mode, "missing key must default TRUE");
        assert_eq!(settings.subagent_model, "");
        assert_eq!(settings.subagent_effort, "");

        fs::write(
            &path,
            r#"{"subagentModel":"  ","subagentEffort":"","releaseUltracode":false,"releasePlanMode":true,"issuePlanMode":false,"releaseAutonomous":false}"#,
        )
        .unwrap();
        let settings = Settings::load(&path);
        assert_eq!(settings.subagent_model, "", "blank = inherit");
        assert_eq!(settings.subagent_effort, "");
        assert!(!settings.release_ultracode);
        assert!(settings.release_plan_mode);
        assert!(!settings.issue_plan_mode);
    }

    #[test]
    fn save_load_round_trips_camel_case() {
        let dir = TempDir::new("roundtrip");
        let path = dir.0.join("settings.json");
        let settings = Settings {
            claude_path: "/opt/homebrew/bin/claude".to_string(),
            repos_root: "~/code/repos".to_string(),
            branch_prefix: "feat/".to_string(),
            claude_model: "sonnet".to_string(),
            claude_effort: "xhigh".to_string(),
            subagent_model: "opus".to_string(),
            subagent_effort: "low".to_string(),
            release_ultracode: false,
            release_plan_mode: true,
            issue_plan_mode: false,
        };
        settings.save(&path).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"claudePath\""), "camelCase keys: {raw}");
        assert!(raw.contains("\"claudeModel\""), "camelCase keys: {raw}");
        assert!(raw.contains("\"claudeEffort\""), "camelCase keys: {raw}");
        assert!(raw.contains("\"subagentModel\""), "camelCase keys: {raw}");
        assert!(raw.contains("\"subagentEffort\""), "camelCase keys: {raw}");
        assert!(raw.contains("\"releaseUltracode\""), "camelCase keys: {raw}");
        assert!(raw.contains("\"releasePlanMode\""), "camelCase keys: {raw}");
        assert!(raw.contains("\"issuePlanMode\""), "camelCase keys: {raw}");
        assert_eq!(Settings::load(&path), settings);
    }

    #[test]
    fn save_preserves_foreign_top_level_keys() {
        let dir = TempDir::new("merge");
        let path = dir.0.join("settings.json");
        fs::write(&path, r#"{"deviceId":"dev-123","claudePath":"old"}"#).unwrap();
        Settings::default().save(&path).unwrap();
        let root: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["deviceId"], "dev-123"); // §7.7 deviceId etc. survive
        assert_eq!(root["claudePath"], "claude");
    }

    #[test]
    fn tilde_expansion() {
        let home = PathBuf::from("/home/tester");
        assert_eq!(expand_tilde("~", home.clone()), home);
        assert_eq!(
            expand_tilde("~/Exponential/repos", home.clone()),
            PathBuf::from("/home/tester/Exponential/repos")
        );
        assert_eq!(
            expand_tilde("/abs/path", home.clone()),
            PathBuf::from("/abs/path")
        );
        assert_eq!(expand_tilde("~other/x", home), PathBuf::from("~other/x"));
    }
}
