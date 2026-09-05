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

use crate::agent::{CodingAgent, CODEX_EFFORTS, CODEX_MODELS, PI_MODELS, PI_THINKING};

pub const DEFAULT_CLAUDE_PATH: &str = "claude";
pub const DEFAULT_CODEX_PATH: &str = "codex";
pub const DEFAULT_PI_PATH: &str = "pi";
pub const DEFAULT_REPOS_ROOT: &str = "~/Exponential/repos";
pub const DEFAULT_BRANCH_PREFIX: &str = "exp/";
/// §7.7 default coding model — passed as `--model fable` on every spawn.
/// Explicit-always so the user's `claude` CLI default is never silently
/// consumed by coding sessions or E2E tests. `fable` is Claude Code's FAMILY
/// alias, resolved by the CLI to the newest Fable (`latest_per_family`:
/// `claude-fable-5-1` since 2.1.257; the previous one stays reachable as
/// `fable-5` / `claude-fable-5`). Deliberately an alias, never a pinned
/// `claude-fable-5-1`: pinning would go stale at the next release and churn
/// the contract on four clients, while the alias tracks the CLI's own default.
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

/// Settings keys retired by past reworks, scrubbed from the file on every
/// [`Settings::save`] (the merge-save would otherwise carry them forever):
/// the release-run subagent knobs and the release toggles (EXP-106 — batch
/// runs replaced release runs), the even older `releaseAutonomous`, and the
/// EXP-201 per-run-mode toggle pairs (EXP-206 merged them into the
/// per-AGENT fields below — issue and batch runs share one default), and
/// the EXP-484 pinned usage window (EXP-688 renders every window as a card,
/// so there is nothing left to pin), and the EXP-690 per-agent skip toggles
/// (every run bypasses permissions now, so there is nothing to persist), and
/// the EXP-282 rail state (EXP-723 removed the collapsible rail — the sidebar
/// is always expanded, so there is no preference left to hold).
/// Foreign top-level keys other subsystems own (`launchDefaultsSync`,
/// `actionAutomations`) ride the merge-save untouched and must never enter
/// this list.
const DEAD_KEYS: [&str; 15] = [
    "usageWindow",
    "subagentModel",
    "subagentEffort",
    "releaseUltracode",
    "releasePlanMode",
    "releaseAutonomous",
    "issueUltracode",
    "batchUltracode",
    "issuePlanMode",
    "batchPlanMode",
    "issueSkipPermissions",
    "batchSkipPermissions",
    "claudeSkipPermissions",
    "codexSkipPermissions",
    "railExpanded",
];

/// The resolved coding settings. `repos_root` is stored in its raw
/// (possibly `~`-prefixed) form and tilde-expanded at use
/// ([`Settings::repos_root_path`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// The agent the Start-coding dialog preselects (EXP-201) — still
    /// overridable per launch. Lenient on load: an unknown/hand-edited value
    /// degrades to Claude WITHOUT failing the whole settings parse (a typed
    /// enum error would silently reset every other setting).
    #[serde(deserialize_with = "lenient_agent")]
    pub default_agent: CodingAgent,
    /// Program name or absolute path of the Claude CLI (§7.7 — the doctor's
    /// target and the launcher's spawn program, used verbatim).
    pub claude_path: String,
    /// Program name or absolute path of the Codex CLI (EXP-201).
    pub codex_path: String,
    /// Program name or absolute path of the pi CLI (EXP-201).
    pub pi_path: String,
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
    /// Codex model slug (`-m`); one of [`CODEX_MODELS`] or blank (= omit the
    /// flag — Codex's own default model applies).
    pub codex_model: String,
    /// Codex reasoning effort (`-c model_reasoning_effort=<v>`); one of
    /// [`CODEX_EFFORTS`] or blank (= omit).
    pub codex_effort: String,
    /// pi model pattern (`--model`, fuzzy-resolved by pi); one of
    /// [`PI_MODELS`] or blank (= omit — pi's own default model applies).
    pub pi_model: String,
    /// pi thinking level (`--thinking`); one of [`PI_THINKING`] or blank.
    pub pi_thinking: String,
    /// Claude "dynamic workflows" (`--effort ultracode`) default — OFF by
    /// default. EXP-206: ONE default for issue and batch runs alike (the old
    /// per-run-mode pairs are dead keys); the Start-coding dialog prefills
    /// from it and every launch can still override. A MISSING key fills from
    /// this struct's manual [`Default`] impl (the container-level
    /// `#[serde(default)]` uses `Settings::default()`, not `bool::default()`)
    /// — locked by a test below.
    pub claude_ultracode: bool,
    /// Claude native plan-mode default — ON by default (Claude presents a
    /// plan for approval in the terminal before editing).
    pub claude_plan_mode: bool,
    /// pi plan-mode default — ON by default like Claude's (EXP-441: pi's
    /// plan mode is the launcher-injected `.exp-pi-plan.ts` extension).
    pub pi_plan_mode: bool,
    /// EXP-288: program name or absolute path of the shell new terminal tabs
    /// spawn (launched as a login shell on unix). Not a launcher knob — it
    /// lives here because this file is the app's ONE merge-preserving
    /// per-install store. `None`/blank = auto (the platform's
    /// `default_shell()` resolution in the terminal crate).
    pub terminal_shell: Option<String>,
    /// EXP-723: id of the newest `ui::changelog::LATEST` entry the user
    /// dismissed or opened — the rail's "What's new" card renders only while
    /// this differs from it (the desktop mirror of the web
    /// `changelog-seen.ts` per-device key). Per-DEVICE like every other key
    /// here, never synced. `None` = never seen anything.
    pub changelog_seen_id: Option<String>,
    /// EXP-551: recently picked BASE emoji unicodes, most recent first (the
    /// picker's "Recent" section). Capped at 24 by the ui writer. Per-DEVICE
    /// like the rail state, never synced. (The EXP-551 `emojiSkinTone` key is
    /// retired — EXP-600 dropped skin tones — and merges through saves like
    /// any other unknown key.)
    pub emoji_recents: Vec<String>,
    /// EXP-367: whether this install has shown the first-run "Set up your
    /// tools" onboarding step. Per-DEVICE (the toolchain is local), so it
    /// lives here and not on the account; shown once on every fresh install
    /// and never again after Continue/"Set up later".
    pub tools_setup_seen: bool,
    /// EXP-638: whether THIS machine raises OS notifications when a synced
    /// `notifications` row lands. Per-DEVICE by design (a user-level column
    /// would fight across devices), so it lives here and never syncs; the
    /// server-side per-type prefs still decide which types may leave the
    /// app at all. ON by default — a fresh install toasts until switched off.
    pub os_notifications: bool,
    /// EXP-742: whether THIS machine's collapsed terminal dock is the
    /// floating BUBBLE (a glass card pinned to the panel's bottom-right
    /// corner carrying the live-session count, an aggregate status dot and
    /// the strip's rich-tab chips) instead of the 29px bottom strip. The
    /// open dock is the same either way; the key only picks the collapsed
    /// form the dock returns to. Per-DEVICE like every other ui pref here,
    /// never synced. OFF by default — the strip is what every install knows.
    pub terminal_dock_bubble: bool,
}

/// Deserialize [`Settings::default_agent`] leniently: any non-string or
/// unknown value degrades to Claude instead of failing the WHOLE settings
/// parse (which would silently reset every other field to defaults).
fn lenient_agent<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<CodingAgent, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(value
        .as_str()
        .and_then(CodingAgent::parse)
        .unwrap_or_default())
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            default_agent: CodingAgent::Claude,
            claude_path: DEFAULT_CLAUDE_PATH.to_string(),
            codex_path: DEFAULT_CODEX_PATH.to_string(),
            pi_path: DEFAULT_PI_PATH.to_string(),
            repos_root: DEFAULT_REPOS_ROOT.to_string(),
            branch_prefix: DEFAULT_BRANCH_PREFIX.to_string(),
            claude_model: DEFAULT_CLAUDE_MODEL.to_string(),
            claude_effort: DEFAULT_CLAUDE_EFFORT.to_string(),
            codex_model: String::new(),
            codex_effort: String::new(),
            pi_model: String::new(),
            pi_thinking: String::new(),
            claude_ultracode: false,
            claude_plan_mode: true,
            pi_plan_mode: true,
            terminal_shell: None,
            changelog_seen_id: None,
            tools_setup_seen: false,
            emoji_recents: Vec::new(),
            os_notifications: true,
            terminal_dock_bubble: false,
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
        if settings.codex_path.trim().is_empty() {
            settings.codex_path = defaults.codex_path;
        }
        if settings.pi_path.trim().is_empty() {
            settings.pi_path = defaults.pi_path;
        }
        if settings.repos_root.trim().is_empty() {
            settings.repos_root = defaults.repos_root;
        }
        if settings.branch_prefix.trim().is_empty() {
            settings.branch_prefix = defaults.branch_prefix;
        }
        settings.claude_model =
            normalize_choice(&settings.claude_model, &MODEL_ALIASES, DEFAULT_CLAUDE_MODEL);
        settings.claude_effort = normalize_choice(&settings.claude_effort, &EFFORT_LEVELS, "");
        // Codex/pi allow BLANK ("CLI default") — unknown values degrade to it.
        settings.codex_model = normalize_choice(&settings.codex_model, &CODEX_MODELS, "");
        settings.codex_effort = normalize_choice(&settings.codex_effort, &CODEX_EFFORTS, "");
        settings.pi_model = normalize_choice(&settings.pi_model, &PI_MODELS, "");
        settings.pi_thinking = normalize_choice(&settings.pi_thinking, &PI_THINKING, "");
        // Blank/whitespace shell degrades to None (= auto-detect).
        settings.terminal_shell = settings
            .terminal_shell
            .filter(|shell| !shell.trim().is_empty());
        // EXP-551: a hand-edited file must not produce an unbounded recents
        // list.
        settings.emoji_recents.truncate(24);
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
            for key in DEAD_KEYS {
                target.remove(key);
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

    /// The configured program for `agent`, probed like
    /// [`Self::resolved_claude_path`] (bare default names check the per-user
    /// install locations before PATH).
    pub fn resolved_path_for(&self, agent: CodingAgent) -> String {
        match agent {
            CodingAgent::Claude => self.resolved_claude_path(),
            CodingAgent::Codex => resolve_program(
                &self.codex_path,
                DEFAULT_CODEX_PATH,
                &[&[".local", "bin", "codex"]],
                dirs::home_dir(),
            ),
            CodingAgent::Pi => resolve_program(
                &self.pi_path,
                DEFAULT_PI_PATH,
                &[&[".local", "bin", "pi"]],
                dirs::home_dir(),
            ),
        }
    }

    /// The RAW configured path field for `agent` (settings UI + doctor copy).
    pub fn path_for(&self, agent: CodingAgent) -> &str {
        match agent {
            CodingAgent::Claude => &self.claude_path,
            CodingAgent::Codex => &self.codex_path,
            CodingAgent::Pi => &self.pi_path,
        }
    }

    /// The default model choice for `agent` (dialog seed).
    pub fn model_for(&self, agent: CodingAgent) -> &str {
        match agent {
            CodingAgent::Claude => &self.claude_model,
            CodingAgent::Codex => &self.codex_model,
            CodingAgent::Pi => &self.pi_model,
        }
    }

    /// The default effort/thinking choice for `agent` (dialog seed).
    pub fn effort_for(&self, agent: CodingAgent) -> &str {
        match agent {
            CodingAgent::Claude => &self.claude_effort,
            CodingAgent::Codex => &self.codex_effort,
            CodingAgent::Pi => &self.pi_thinking,
        }
    }

    /// The plan-mode default for `agent` (dialog seed). Always `false` for
    /// codex: it has no launch-into-plan mode.
    pub fn plan_mode_for(&self, agent: CodingAgent) -> bool {
        match agent {
            CodingAgent::Claude => self.claude_plan_mode,
            CodingAgent::Codex => false,
            CodingAgent::Pi => self.pi_plan_mode,
        }
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
pub(crate) fn normalize_choice(raw: &str, allowed: &[&str], fallback: &str) -> String {
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
    resolve_program(
        raw,
        DEFAULT_CLAUDE_PATH,
        &[
            // The official native installer's location — the install the
            // user's login shell almost certainly runs.
            &[".local", "bin", "claude"],
            // Older `claude install` local location.
            &[".claude", "local", "claude"],
        ],
        home,
    )
}

/// The shared bare-name probe (see [`Settings::resolved_claude_path`] for
/// the PATH-skew rationale): an explicit non-default `raw` is used verbatim;
/// the bare default probes each home-relative `candidates` segment list
/// before falling back to PATH resolution.
pub fn resolve_program(
    raw: &str,
    default: &str,
    candidates: &[&[&str]],
    home: Option<PathBuf>,
) -> String {
    if raw != default {
        return raw.to_string();
    }
    if let Some(home) = home {
        for candidate in candidate_paths(&home, candidates, cfg!(windows)) {
            if candidate.is_file() {
                return candidate.to_string_lossy().into_owned();
            }
        }
    }
    raw.to_string()
}

/// The concrete probe paths for the home-relative candidate segment lists.
/// On Windows the native installers ship `<name>.exe` (EXP-419 — the
/// extensionless probe always missed there), so each candidate is probed
/// with `.exe` appended first, then bare.
fn candidate_paths(home: &Path, candidates: &[&[&str]], windows: bool) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for segments in candidates {
        let mut candidate = home.to_path_buf();
        for segment in *segments {
            candidate.push(segment);
        }
        if windows {
            let mut exe = candidate.clone().into_os_string();
            exe.push(".exe");
            paths.push(PathBuf::from(exe));
        }
        paths.push(candidate);
    }
    paths
}

/// `~` / `~/…` → `home`-rooted path; anything else passes through. (Only the
/// current user's home is expanded — `~other` is left verbatim, matching
/// common CLI behavior.)
///
/// The tail is pushed segment-by-segment: a single `home.join(rest)` keeps
/// the embedded `/` of `DEFAULT_REPOS_ROOT` verbatim, which renders mixed
/// separators on Windows (`C:\Users\x\Exponential/repos` — EXP-366). On
/// Windows a hand-typed `~\…` tail with backslashes is accepted too;
/// elsewhere `\` stays an ordinary filename character.
pub fn expand_tilde(raw: &str, home: PathBuf) -> PathBuf {
    let tail = raw.strip_prefix("~/").or_else(|| {
        if cfg!(windows) {
            raw.strip_prefix("~\\")
        } else {
            None
        }
    });
    if raw == "~" {
        home
    } else if let Some(rest) = tail {
        let separators: &[char] = if cfg!(windows) { &['/', '\\'] } else { &['/'] };
        let mut path = home;
        for segment in rest.split(separators).filter(|s| !s.is_empty()) {
            path.push(segment);
        }
        path
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
    fn windows_candidates_probe_exe_first() {
        let home = Path::new("/home/x");
        let candidates: &[&[&str]] = &[&[".local", "bin", "claude"]];
        let unix = candidate_paths(home, candidates, false);
        assert_eq!(unix, vec![home.join(".local/bin/claude")]);
        // EXP-419: the native Windows installer ships claude.exe — it must be
        // probed (before the bare name, which can't execute there anyway).
        let windows = candidate_paths(home, candidates, true);
        assert_eq!(
            windows,
            vec![
                PathBuf::from(format!("{}.exe", home.join(".local/bin/claude").display())),
                home.join(".local/bin/claude"),
            ]
        );
    }

    #[test]
    fn defaults_match_the_spec_table() {
        let settings = Settings::default();
        assert_eq!(settings.default_agent, CodingAgent::Claude);
        assert_eq!(settings.claude_path, "claude");
        assert_eq!(settings.codex_path, "codex");
        assert_eq!(settings.pi_path, "pi");
        assert_eq!(settings.repos_root, "~/Exponential/repos");
        assert_eq!(settings.branch_prefix, "exp/");
        assert_eq!(settings.claude_model, "fable");
        assert_eq!(settings.claude_effort, "");
        // Codex/pi default to the CLI's own model + effort (blank = omit).
        assert_eq!(settings.codex_model, "");
        assert_eq!(settings.codex_effort, "");
        assert_eq!(settings.pi_model, "");
        assert_eq!(settings.pi_thinking, "");
        // Per-agent run defaults (EXP-206 — no issue/batch split): Claude
        // plan mode ON, ultracode OFF. EXP-690: skip-permissions is no
        // longer a setting — every run bypasses.
        assert!(!settings.claude_ultracode);
        assert!(settings.claude_plan_mode);
        assert!(settings.pi_plan_mode);
        // EXP-288: no shell override by default (auto-detect).
        assert_eq!(settings.terminal_shell, None);
        // EXP-367: a fresh install has not seen the tools onboarding step.
        assert!(!settings.tools_setup_seen);
    }

    /// EXP-288: a blank/whitespace `terminalShell` degrades to None (auto)
    /// instead of spawning an empty program.
    #[test]
    fn blank_terminal_shell_degrades_to_auto() {
        let dir = TempDir::new("shell");
        let path = dir.0.join("settings.json");
        fs::write(&path, r#"{"terminalShell":"  "}"#).unwrap();
        assert_eq!(Settings::load(&path).terminal_shell, None);
        fs::write(&path, r#"{"terminalShell":"/opt/homebrew/bin/fish"}"#).unwrap();
        assert_eq!(
            Settings::load(&path).terminal_shell.as_deref(),
            Some("/opt/homebrew/bin/fish")
        );
    }

    /// EXP-201: `defaultAgent` round-trips; unknown or mistyped values
    /// degrade to Claude WITHOUT resetting the rest of the file (the lenient
    /// deserializer must never fail the whole parse).
    #[test]
    fn default_agent_round_trips_and_degrades_leniently() {
        let dir = TempDir::new("agent");
        let path = dir.0.join("settings.json");

        fs::write(&path, r#"{"defaultAgent":"codex","claudeModel":"sonnet"}"#).unwrap();
        let settings = Settings::load(&path);
        assert_eq!(settings.default_agent, CodingAgent::Codex);
        assert_eq!(settings.claude_model, "sonnet");

        // Unknown string → Claude, other fields intact.
        fs::write(&path, r#"{"defaultAgent":"cursor","claudeModel":"sonnet"}"#).unwrap();
        let settings = Settings::load(&path);
        assert_eq!(settings.default_agent, CodingAgent::Claude);
        assert_eq!(settings.claude_model, "sonnet", "parse must not reset the file");

        // Wrong TYPE → Claude, other fields intact.
        fs::write(&path, r#"{"defaultAgent":42,"claudeModel":"sonnet"}"#).unwrap();
        let settings = Settings::load(&path);
        assert_eq!(settings.default_agent, CodingAgent::Claude);
        assert_eq!(settings.claude_model, "sonnet");

        // Save writes the lowercase id.
        let mut settings = Settings::default();
        settings.default_agent = CodingAgent::Pi;
        settings.save(&path).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains(r#""defaultAgent": "pi""#), "raw: {raw}");
        assert_eq!(Settings::load(&path).default_agent, CodingAgent::Pi);
    }

    /// EXP-201: the codex/pi model + effort fields normalize into their own
    /// closed sets, with blank ("CLI default") as the fallback.
    #[test]
    fn codex_and_pi_choices_normalize_on_load() {
        let dir = TempDir::new("agent-choices");
        let path = dir.0.join("settings.json");
        fs::write(
            &path,
            r#"{"codexModel":" GPT-5.6-Sol ","codexEffort":"max","piModel":"grok-4.5","piThinking":" XHigh "}"#,
        )
        .unwrap();
        let settings = Settings::load(&path);
        assert_eq!(settings.codex_model, "gpt-5.6-sol");
        assert_eq!(settings.codex_effort, "", "codex has no max — degrade to blank");
        assert_eq!(settings.pi_model, "grok-4.5");
        assert_eq!(settings.pi_thinking, "xhigh");
    }

    /// EXP-742: a file written before the key existed keeps the strip
    /// (missing = OFF), and the bubble pick survives a merge-save.
    #[test]
    fn terminal_dock_bubble_default_off_and_round_trip_on() {
        let dir = TempDir::new("dock-bubble");
        let path = dir.0.join("settings.json");
        fs::write(&path, r#"{"claudePath":"claude"}"#).unwrap();
        assert!(!Settings::load(&path).terminal_dock_bubble);

        let mut settings = Settings::load(&path);
        settings.terminal_dock_bubble = true;
        settings.save(&path).unwrap();
        assert!(Settings::load(&path).terminal_dock_bubble);
        let raw: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(raw["terminalDockBubble"], serde_json::Value::Bool(true));
    }

    #[test]
    fn os_notifications_default_on_and_round_trip_off() {
        // EXP-638: a file written before the key existed keeps toasting
        // (missing = ON), and an explicit opt-out survives a merge-save.
        let dir = TempDir::new("os-notifications");
        let path = dir.0.join("settings.json");
        fs::write(&path, r#"{"claudePath":"claude"}"#).unwrap();
        assert!(Settings::load(&path).os_notifications);

        let mut settings = Settings::load(&path);
        settings.os_notifications = false;
        settings.save(&path).unwrap();
        assert!(!Settings::load(&path).os_notifications);
        let raw: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(raw["osNotifications"], serde_json::Value::Bool(false));
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
    /// alias sets; anything unknown falls back (model → fable, effort →
    /// blank) so the argv can never carry a value the CLI rejects.
    #[test]
    fn model_and_effort_values_normalize_on_load() {
        let dir = TempDir::new("normalize");
        let path = dir.0.join("settings.json");
        fs::write(&path, r#"{"claudeModel":" Opus ","claudeEffort":" XHigh "}"#).unwrap();
        let settings = Settings::load(&path);
        assert_eq!(settings.claude_model, "opus");
        assert_eq!(settings.claude_effort, "xhigh");

        fs::write(&path, r#"{"claudeModel":"haiku","claudeEffort":"extreme"}"#).unwrap();
        let settings = Settings::load(&path);
        assert_eq!(settings.claude_model, "fable", "unknown model → fable");
        assert_eq!(settings.claude_effort, "", "unknown effort → omit");
    }

    /// Per-agent run fields (EXP-206): MISSING keys must fill from the
    /// manual `Default` impl (container-level `#[serde(default)]`) — Claude
    /// plan mode TRUE, everything else FALSE. Explicit bools round-trip; the
    /// dead release-era AND per-run-mode keys are ignored on load and
    /// scrubbed from the file on save.
    #[test]
    fn agent_run_fields_fill_from_defaults_and_dead_keys_are_scrubbed() {
        let dir = TempDir::new("agent-fields");
        let path = dir.0.join("settings.json");
        fs::write(&path, r#"{"claudeModel":"sonnet"}"#).unwrap();
        let settings = Settings::load(&path);
        assert!(!settings.claude_ultracode, "missing key must default FALSE");
        assert!(settings.claude_plan_mode, "missing key must default TRUE");
        assert!(settings.pi_plan_mode, "missing key must default TRUE");

        fs::write(
            &path,
            r#"{"claudeUltracode":true,"claudePlanMode":false,"piPlanMode":false,"claudeSkipPermissions":true,"codexSkipPermissions":true,"subagentModel":"opus","releaseUltracode":false,"issueUltracode":true,"batchPlanMode":true,"issueSkipPermissions":true}"#,
        )
        .unwrap();
        let settings = Settings::load(&path);
        assert!(settings.claude_ultracode);
        assert!(!settings.claude_plan_mode);
        assert!(!settings.pi_plan_mode);

        // plan_mode_for maps per agent; codex has no launch-into-plan mode.
        let defaults = Settings::default();
        assert!(defaults.plan_mode_for(CodingAgent::Claude));
        assert!(defaults.plan_mode_for(CodingAgent::Pi));
        assert!(!defaults.plan_mode_for(CodingAgent::Codex));
        assert!(!settings.plan_mode_for(CodingAgent::Claude));
        assert!(!settings.plan_mode_for(CodingAgent::Pi));

        // Saving scrubs the retired keys the merge-save would otherwise
        // carry forever.
        settings.save(&path).unwrap();
        let root: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        for dead in [
            "subagentModel",
            "releaseUltracode",
            "issueUltracode",
            "batchUltracode",
            "issuePlanMode",
            "batchPlanMode",
            "issueSkipPermissions",
            "batchSkipPermissions",
            "claudeSkipPermissions",
            "codexSkipPermissions",
        ] {
            assert!(root.get(dead).is_none(), "{dead} must be scrubbed");
        }
        assert_eq!(root["claudeUltracode"], true);
    }

    #[test]
    fn save_load_round_trips_camel_case() {
        let dir = TempDir::new("roundtrip");
        let path = dir.0.join("settings.json");
        let settings = Settings {
            default_agent: CodingAgent::Codex,
            claude_path: "/opt/homebrew/bin/claude".to_string(),
            codex_path: "/opt/homebrew/bin/codex".to_string(),
            pi_path: "/opt/homebrew/bin/pi".to_string(),
            repos_root: "~/code/repos".to_string(),
            branch_prefix: "feat/".to_string(),
            claude_model: "sonnet".to_string(),
            claude_effort: "xhigh".to_string(),
            codex_model: "gpt-5.6-terra".to_string(),
            codex_effort: "high".to_string(),
            pi_model: "grok-4.5".to_string(),
            pi_thinking: "high".to_string(),
            claude_ultracode: true,
            claude_plan_mode: false,
            pi_plan_mode: false,
            terminal_shell: Some("/opt/homebrew/bin/fish".to_string()),
            changelog_seen_id: Some("2026-09-relations-and-design-refresh".to_string()),
            tools_setup_seen: true,
            emoji_recents: vec!["🎉".to_string()],
            os_notifications: true,
            terminal_dock_bubble: true,
        };
        settings.save(&path).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains("\"claudePath\""), "camelCase keys: {raw}");
        assert!(raw.contains("\"toolsSetupSeen\""), "camelCase keys: {raw}");
        assert!(raw.contains("\"terminalShell\""), "camelCase keys: {raw}");
        assert!(raw.contains("\"claudeModel\""), "camelCase keys: {raw}");
        assert!(raw.contains("\"claudeEffort\""), "camelCase keys: {raw}");
        assert!(raw.contains("\"claudeUltracode\""), "camelCase keys: {raw}");
        assert!(raw.contains("\"claudePlanMode\""), "camelCase keys: {raw}");
        assert!(raw.contains("\"piPlanMode\""), "camelCase keys: {raw}");
        assert!(raw.contains("\"changelogSeenId\""), "camelCase keys: {raw}");
        assert!(raw.contains("\"terminalDockBubble\""), "camelCase keys: {raw}");
        assert_eq!(Settings::load(&path), settings);
    }

    /// EXP-688: the EXP-484 per-agent pinned usage window is gone (every
    /// window renders as its own card). An old settings.json carrying the
    /// key still loads — unknown keys are ignored — and the merge-save
    /// scrubs it out via [`DEAD_KEYS`] instead of carrying it forever.
    #[test]
    fn the_retired_usage_window_pick_is_scrubbed_on_save() {
        let dir = TempDir::new("usage-window");
        let path = dir.0.join("settings.json");
        fs::write(
            &path,
            r#"{"claudeModel":"opus","usageWindow":{"claude":"model:fable"}}"#,
        )
        .unwrap();
        let settings = Settings::load(&path);
        assert_eq!(settings.claude_model, "opus", "the rest still parses");
        settings.save(&path).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("usageWindow"), "{raw}");
    }

    /// EXP-723: the EXP-282 rail-expanded preference is gone (the sidebar is
    /// always open). Same contract as the usage window above — an old file
    /// still loads, and the merge-save scrubs the key instead of carrying it
    /// forever.
    #[test]
    fn the_retired_rail_state_is_scrubbed_on_save() {
        let dir = TempDir::new("rail-expanded");
        let path = dir.0.join("settings.json");
        fs::write(
            &path,
            r#"{"claudeModel":"opus","railExpanded":false}"#,
        )
        .unwrap();
        let settings = Settings::load(&path);
        assert_eq!(settings.claude_model, "opus", "the rest still parses");
        settings.save(&path).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("railExpanded"), "{raw}");
    }

    /// EXP-551: the emoji prefs are hand-editable like every other key —
    /// `load` caps the recents list (and ignores the retired EXP-551
    /// `emojiSkinTone` key like any other unknown key).
    #[test]
    fn emoji_prefs_normalize_on_load() {
        let dir = TempDir::new("emoji");
        let path = dir.0.join("settings.json");
        let recents: Vec<String> = (0..40).map(|i| format!("e{i}")).collect();
        let raw = serde_json::json!({
            "emojiSkinTone": 9,
            "emojiRecents": recents,
        });
        fs::write(&path, raw.to_string()).unwrap();
        let loaded = Settings::load(&path);
        assert_eq!(loaded.emoji_recents.len(), 24);
        assert_eq!(loaded.emoji_recents[0], "e0");
        assert!(Settings::default().emoji_recents.is_empty());
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

    /// EXP-366: the tilde tail must be pushed segment-by-segment — a single
    /// `join` keeps the embedded `/`, which renders mixed separators on
    /// Windows (`C:\Users\x\Exponential/repos`). Locked platform-neutrally:
    /// every tail segment becomes its own path component with no separator
    /// characters left inside.
    #[test]
    fn tilde_expansion_splits_tail_into_components() {
        let home = PathBuf::from("/home/tester");
        let expanded = expand_tilde("~/Exponential/repos", home.clone());
        assert_eq!(
            expanded.components().count(),
            home.components().count() + 2,
            "each tail segment is one component: {expanded:?}"
        );
        for component in expanded.components() {
            if let std::path::Component::Normal(part) = component {
                let part = part.to_string_lossy();
                assert!(
                    !part.contains('/') && !part.contains('\\'),
                    "no separator may survive inside a component: {part:?}"
                );
            }
        }
        // Doubled/trailing separators collapse instead of producing empty
        // components.
        assert_eq!(
            expand_tilde("~//Exponential//repos/", home.clone()),
            expand_tilde("~/Exponential/repos", home)
        );
    }
}
