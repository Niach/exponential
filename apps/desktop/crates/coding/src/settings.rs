//! Coding-side settings (masterplan-v3 §7.7, DC-3) — the three launcher knobs:
//!
//! | Setting            | Default              | Used by                      |
//! |--------------------|----------------------|------------------------------|
//! | Claude CLI path    | `claude`             | §7.1 step 7 spawn + doctor   |
//! | Repos root         | `~/Exponential/repos`| §7.1 step 3 worktree layout  |
//! | Branch prefix      | `exp/`               | `<prefix><IDENTIFIER>` branch|
//! | Claude model       | `opus`               | §7.1 step 7 `--model` argv   |
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
/// §7.7 default coding model — passed as `--model opus` on every spawn. Explicit
/// -always so the user's `claude` CLI default (possibly a scarcer model like
/// Fable) is never silently consumed by coding sessions or E2E tests.
pub const DEFAULT_CLAUDE_MODEL: &str = "opus";

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
    /// The Claude model, passed verbatim as `--model <value>` on every spawn
    /// (§7.7 — explicit-always; free text, common values opus/sonnet/haiku/fable).
    pub claude_model: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            claude_path: DEFAULT_CLAUDE_PATH.to_string(),
            repos_root: DEFAULT_REPOS_ROOT.to_string(),
            branch_prefix: DEFAULT_BRANCH_PREFIX.to_string(),
            claude_model: DEFAULT_CLAUDE_MODEL.to_string(),
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
    /// them). Empty strings degrade to the field default so a hand-blanked
    /// file can never produce an unusable launcher.
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
        if settings.claude_model.trim().is_empty() {
            settings.claude_model = defaults.claude_model;
        }
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
        assert_eq!(settings.claude_model, "opus");
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

    #[test]
    fn save_load_round_trips_camel_case() {
        let dir = TempDir::new("roundtrip");
        let path = dir.0.join("settings.json");
        let settings = Settings {
            claude_path: "/opt/homebrew/bin/claude".to_string(),
            repos_root: "~/code/repos".to_string(),
            branch_prefix: "feat/".to_string(),
            claude_model: "sonnet".to_string(),
        };
        settings.save(&path).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"claudePath\""), "camelCase keys: {raw}");
        assert!(raw.contains("\"claudeModel\""), "camelCase keys: {raw}");
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
