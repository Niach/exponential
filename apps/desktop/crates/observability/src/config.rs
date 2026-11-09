//! #LOGS-2: where diagnostics go and whether they go at all.
//!
//! The server the app (or daemon) talks to decides: `GET
//! <server>/api/monitoring/config?platform=desktop|cli` answers `{ enabled,
//! dsn, environment, tracesSampleRate }` (self-hosters and dev: `{ enabled:
//! false }`), and envelopes go back to `POST <server>/api/monitoring`, never
//! straight to GlitchTip. The last answer is cached in
//! `{data_dir}/diagnostics.json` next to the person's "Send diagnostics"
//! switch (local, never synced), so the next launch initialises before the
//! network and early panics are caught.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const FILE_NAME: &str = "diagnostics.json";

/// The server's answer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteConfig {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dsn: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub traces_sample_rate: Option<f32>,
}

/// `{data_dir}/diagnostics.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalFile {
    /// The "Send diagnostics" switch (desktop Settings, `exponential
    /// diagnostics on|off`). Default on.
    #[serde(default = "default_true")]
    pub send_diagnostics: bool,
    /// The server the last launch connected to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_server: Option<String>,
    /// The last config each server answered.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub servers: BTreeMap<String, RemoteConfig>,
}

fn default_true() -> bool {
    true
}

impl Default for LocalFile {
    fn default() -> Self {
        Self { send_diagnostics: true, last_server: None, servers: BTreeMap::new() }
    }
}

pub fn path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE_NAME)
}

pub fn load(data_dir: &Path) -> LocalFile {
    std::fs::read_to_string(path(data_dir))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// Replace-by-rename: the IDE and the CLI daemon share the data dir.
pub fn save(data_dir: &Path, file: &LocalFile) {
    let target = path(data_dir);
    let tmp = target.with_extension(format!("json.{}.tmp", std::process::id()));
    let Ok(mut rendered) = serde_json::to_string_pretty(file) else { return };
    rendered.push('\n');
    let _ = std::fs::create_dir_all(data_dir);
    if std::fs::write(&tmp, rendered).is_ok() && std::fs::rename(&tmp, &target).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// Read-modify-write of the shared file.
pub fn update(data_dir: &Path, change: impl FnOnce(&mut LocalFile)) -> LocalFile {
    let mut file = load(data_dir);
    change(&mut file);
    save(data_dir, &file);
    file
}

/// `EXP_DIAGNOSTICS=0|false|off` turns diagnostics off for this process.
pub fn env_opt_out() -> bool {
    std::env::var("EXP_DIAGNOSTICS")
        .is_ok_and(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "0" | "false" | "off" | "no"))
}

/// The server's base URL without a trailing slash.
pub fn normalize_server(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

pub fn config_url(server: &str, platform: &str) -> String {
    format!("{}/api/monitoring/config?platform={platform}", normalize_server(server))
}

pub fn tunnel_url(server: &str) -> String {
    format!("{}/api/monitoring", normalize_server(server))
}

/// `prod` | `staging` | `dev` when nobody said: a debug build or a local
/// server is dev, a staging host staging, the rest prod.
pub fn derive_environment(server: Option<&str>) -> &'static str {
    if cfg!(debug_assertions) {
        return "dev";
    }
    let Some(server) = server else { return "prod" };
    let host = server
        .split("://")
        .nth(1)
        .unwrap_or(server)
        .split(['/', ':'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if host == "localhost" || host.starts_with("127.") || host.ends_with(".local") || host == "0.0.0.0" {
        "dev"
    } else if host.contains("staging") {
        "staging"
    } else {
        "prod"
    }
}

/// `SENTRY_TRACES_SAMPLE_RATE` > the server's rate > 0.1.
pub fn traces_sample_rate(remote: Option<f32>) -> f32 {
    std::env::var("SENTRY_TRACES_SAMPLE_RATE")
        .ok()
        .and_then(|raw| raw.trim().parse::<f32>().ok())
        .or(remote)
        .unwrap_or(0.1)
        .clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_defaults_to_on_and_round_trips() {
        let dir = std::env::temp_dir().join(format!("obs-config-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(load(&dir).send_diagnostics, "a missing file = on");
        let saved = update(&dir, |file| {
            file.send_diagnostics = false;
            file.last_server = Some("https://app.exponential.at".into());
            file.servers.insert(
                "https://app.exponential.at".into(),
                RemoteConfig { enabled: true, dsn: Some("https://k@g.test/1".into()), environment: Some("prod".into()), traces_sample_rate: Some(0.2) },
            );
        });
        assert_eq!(load(&dir), saved);
        std::fs::write(path(&dir), r#"{"servers":{"x":{"enabled":false}}}"#).unwrap();
        let file = load(&dir);
        assert!(file.send_diagnostics);
        assert_eq!(file.servers["x"], RemoteConfig::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_server_answer_parses() {
        let on: RemoteConfig = serde_json::from_str(r#"{"enabled":true,"dsn":"https://k@g/2","environment":"staging","tracesSampleRate":0.1}"#).unwrap();
        assert_eq!(on.environment.as_deref(), Some("staging"));
        assert_eq!(on.traces_sample_rate, Some(0.1));
        let off: RemoteConfig = serde_json::from_str(r#"{"enabled":false}"#).unwrap();
        assert!(!off.enabled && off.dsn.is_none());
    }

    #[test]
    fn urls() {
        assert_eq!(config_url("https://app.exponential.at/", "cli"), "https://app.exponential.at/api/monitoring/config?platform=cli");
        assert_eq!(tunnel_url("https://app.exponential.at"), "https://app.exponential.at/api/monitoring");
    }

    #[test]
    fn environments() {
        if cfg!(debug_assertions) {
            assert_eq!(derive_environment(Some("https://app.exponential.at")), "dev");
        } else {
            assert_eq!(derive_environment(Some("https://app.exponential.at")), "prod");
            assert_eq!(derive_environment(Some("https://staging.exponential.at")), "staging");
            assert_eq!(derive_environment(Some("http://localhost:3000")), "dev");
        }
    }
}
