//! Whether and where eDE reports (#LOGS-2).
//!
//! 1. Diagnostics off (`EXP_DIAGNOSTICS=0`, or `[diagnostics] send = false`
//!    in `compositor.toml`, the Settings switch) = off, nothing fetched.
//! 2. The signed-in account's server (the IDE's shared `accounts.json`):
//!    no account = off. Envelopes ALWAYS go to `POST <server>/api/monitoring`
//!    (the collector's own host sits behind Cloudflare Access).
//! 3. `EXP_SENTRY_DSN` = a local development override naming the project
//!    (still sent through the tunnel); else the cached answer of
//!    `GET <server>/api/monitoring/config?platform=ede` decides: no cached
//!    answer or `{"enabled": false}` = off. A fresh answer is fetched in
//!    the background for the next launch.
//!
//! `SENTRY_ENVIRONMENT` and `SENTRY_TRACES_SAMPLE_RATE` override the
//! server's `environment` and `tracesSampleRate`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

/// Transactions sampled when nobody says otherwise.
pub const DEFAULT_TRACES_SAMPLE_RATE: f32 = 0.1;

/// Where envelopes go: `POST <server>/api/monitoring`, the envelope's
/// header naming `dsn` (the project).
#[derive(Clone)]
pub struct Target {
    pub url: String,
    pub dsn: String,
}

impl std::fmt::Debug for Target {
    // Never prints the DSN.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Target").field("url", &self.url).finish_non_exhaustive()
    }
}

/// What [`resolve`] decided: report, with these settings. No `Debug`: it
/// holds the DSN.
#[derive(Clone)]
pub struct Plan {
    pub dsn: sentry::types::Dsn,
    pub environment: String,
    pub traces_sample_rate: f32,
    pub target: Target,
    /// The account's pseudonymous user id (never its email or name).
    pub user_id: Option<String>,
}

/// The server's answer, as cached.
#[derive(Clone, Debug, PartialEq)]
pub struct Remote {
    pub enabled: bool,
    pub dsn: Option<String>,
    pub environment: Option<String>,
    pub traces_sample_rate: Option<f32>,
}

/// One signed-in account of `accounts.json`.
#[derive(Clone, Debug, PartialEq)]
pub struct Account {
    pub server: String,
    pub user_id: Option<String>,
}

pub fn resolve() -> Option<Plan> {
    let env = |k: &str| std::env::var(k).ok();
    if !diagnostics_on(env("EXP_DIAGNOSTICS").as_deref(), std::fs::read_to_string(compositor_toml()).ok().as_deref()) {
        return None;
    }
    let account = first_account()?;
    let environment = |server: Option<&str>| {
        env("SENTRY_ENVIRONMENT").filter(|e| !e.trim().is_empty()).or_else(|| server.map(str::to_owned)).unwrap_or_else(|| build_environment().to_owned())
    };
    let traces = |server: Option<f32>| env("SENTRY_TRACES_SAMPLE_RATE").and_then(|r| sample_rate(&r)).or(server.filter(|r| (0.0..=1.0).contains(r))).unwrap_or(DEFAULT_TRACES_SAMPLE_RATE);
    let url = format!("{}/api/monitoring", account.server);

    if let Some(raw) = env("EXP_SENTRY_DSN").map(|d| d.trim().to_owned()).filter(|d| !d.is_empty()) {
        let dsn: sentry::types::Dsn = raw.parse().ok()?;
        return Some(Plan { dsn, environment: environment(None), traces_sample_rate: traces(None), target: Target { url, dsn: raw }, user_id: account.user_id });
    }

    let remote = read_cache(&cache_path(), &account.server)?;
    if !remote.enabled {
        return None;
    }
    let raw = remote.dsn.clone()?;
    let dsn: sentry::types::Dsn = raw.parse().ok()?;
    Some(Plan {
        dsn,
        environment: environment(remote.environment.as_deref()),
        traces_sample_rate: traces(remote.traces_sample_rate),
        target: Target { url, dsn: raw },
        user_id: account.user_id,
    })
}

/// Fetches the server's answer on a background thread and caches it for
/// the next launch. Nothing when diagnostics are off, the DSN is
/// overridden, or no account names a server.
pub fn refresh_in_background() {
    if !diagnostics_on(std::env::var("EXP_DIAGNOSTICS").ok().as_deref(), std::fs::read_to_string(compositor_toml()).ok().as_deref()) {
        return;
    }
    if std::env::var("EXP_SENTRY_DSN").is_ok_and(|d| !d.trim().is_empty()) {
        return;
    }
    let Some(account) = first_account() else { return };
    let _ = std::thread::Builder::new().name("ede-monitoring-config".into()).spawn(move || {
        let url = format!("{}/api/monitoring/config?platform=ede", account.server);
        let client = match reqwest::blocking::Client::builder().timeout(Duration::from_secs(10)).user_agent(crate::RELEASE).build() {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!(target: "ede_observability", "monitoring config: {e}");
                return;
            }
        };
        let answer = client.get(&url).send().and_then(|r| r.error_for_status()).and_then(|r| r.text());
        match answer {
            Ok(text) => match serde_json::from_str::<Value>(&text).ok().as_ref().and_then(parse_remote) {
                Some(remote) => {
                    if let Err(e) = write_cache(&cache_path(), &account.server, &remote) {
                        tracing::debug!(target: "ede_observability", "monitoring config cache: {e}");
                    }
                }
                None => tracing::debug!(target: "ede_observability", "monitoring config: not the expected shape"),
            },
            // Offline or a server without the endpoint: the cache stays.
            Err(e) => tracing::debug!(target: "ede_observability", "monitoring config: {}", e.without_url()),
        }
    });
}

/// `EXP_DIAGNOSTICS` (`0`/`false`/`off`/`no` = off) wins, then
/// `[diagnostics] send` of `compositor.toml`; on by default.
pub fn diagnostics_on(env: Option<&str>, compositor_toml: Option<&str>) -> bool {
    if let Some(v) = env.map(|v| v.trim().to_ascii_lowercase()).filter(|v| !v.is_empty()) {
        return !matches!(v.as_str(), "0" | "false" | "off" | "no");
    }
    compositor_toml
        .and_then(|t| t.parse::<toml::Table>().ok())
        .and_then(|t| t.get("diagnostics")?.get("send")?.as_bool())
        .unwrap_or(true)
}

/// A rate in `[0, 1]`, else `None`.
pub fn sample_rate(text: &str) -> Option<f32> {
    text.trim().parse::<f32>().ok().filter(|r| (0.0..=1.0).contains(r))
}

/// `dev` for a debug build, else `prod`.
pub fn build_environment() -> &'static str {
    if cfg!(debug_assertions) {
        "dev"
    } else {
        "prod"
    }
}

/// `{ "enabled": true, "dsn", "environment", "tracesSampleRate" }` or
/// `{ "enabled": false }`; anything else is `None`. Enabled without a DSN
/// counts as off.
pub fn parse_remote(v: &Value) -> Option<Remote> {
    let enabled = v.get("enabled")?.as_bool()?;
    let dsn = v.get("dsn").and_then(Value::as_str).map(str::trim).filter(|d| !d.is_empty()).map(str::to_owned);
    Some(Remote {
        enabled: enabled && dsn.is_some(),
        dsn,
        environment: v.get("environment").and_then(Value::as_str).map(str::trim).filter(|e| !e.is_empty()).map(str::to_owned),
        traces_sample_rate: v.get("tracesSampleRate").and_then(Value::as_f64).map(|r| r as f32).filter(|r| (0.0..=1.0).contains(r)),
    })
}

/// The first account of `accounts.json` with an http(s) server, its URL
/// without a trailing slash.
pub fn parse_accounts(text: &str) -> Option<Account> {
    let v: Value = serde_json::from_str(text).ok()?;
    v.get("accounts")?.as_array()?.iter().find_map(|a| {
        let server = a.get("instanceUrl")?.as_str()?.trim().trim_end_matches('/');
        if !(server.starts_with("https://") || server.starts_with("http://")) {
            return None;
        }
        let user_id = a.get("userId").and_then(Value::as_str).filter(|u| !u.is_empty()).map(str::to_owned);
        Some(Account { server: server.to_owned(), user_id })
    })
}

/// The cached answer for `server` (another server's answer never applies).
pub fn read_cache(path: &Path, server: &str) -> Option<Remote> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    if v.get("server")?.as_str()? != server {
        return None;
    }
    parse_remote(v.get("config")?)
}

pub fn write_cache(path: &Path, server: &str, remote: &Remote) -> std::io::Result<()> {
    let mut config = json!({ "enabled": remote.enabled });
    if let Some(d) = &remote.dsn {
        config["dsn"] = json!(d);
    }
    if let Some(e) = &remote.environment {
        config["environment"] = json!(e);
    }
    if let Some(r) = remote.traces_sample_rate {
        config["tracesSampleRate"] = json!(r);
    }
    let body = serde_json::to_vec_pretty(&json!({ "server": server, "config": config }))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Several eDE processes refresh at login: write aside, then rename.
    let tmp = path.with_extension(format!("json.{}", std::process::id()));
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, path)
}

fn first_account() -> Option<Account> {
    data_dirs().iter().find_map(|d| parse_accounts(&std::fs::read_to_string(d.join("accounts.json")).ok()?))
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| home().join(fallback))
}

/// The IDE's data dir (crates/api `default_data_dir`): `EXP_DATA_DIR`, else
/// `$XDG_DATA_HOME/exponential`, then its staging twin.
fn data_dirs() -> Vec<PathBuf> {
    if let Some(dir) = std::env::var_os("EXP_DATA_DIR").filter(|d| !d.is_empty()) {
        return vec![PathBuf::from(dir)];
    }
    let base = xdg("XDG_DATA_HOME", ".local/share");
    vec![base.join("exponential"), base.join("exponential-staging")]
}

/// `$XDG_CONFIG_HOME/exponential-desktop/compositor.toml`.
pub fn compositor_toml() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("exponential-desktop").join("compositor.toml")
}

/// `$XDG_CACHE_HOME/exponential-desktop/monitoring.json`.
pub fn cache_path() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache").join("exponential-desktop").join("monitoring.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_default_on_and_both_switches_turn_it_off() {
        assert!(diagnostics_on(None, None));
        assert!(diagnostics_on(None, Some("[keyboard]\nlayout = \"de\"\n")));
        assert!(!diagnostics_on(Some("0"), None));
        assert!(!diagnostics_on(Some("off"), None));
        assert!(!diagnostics_on(None, Some("[diagnostics]\nsend = false\n")));
        assert!(diagnostics_on(Some("1"), Some("[diagnostics]\nsend = false\n")), "the variable wins");
        assert!(diagnostics_on(None, Some("not toml [[[")), "an unreadable file leaves the default");
    }

    #[test]
    fn the_servers_answer() {
        let on = parse_remote(&json!({"enabled": true, "dsn": "https://k@glitchtip.example/3", "environment": "staging", "tracesSampleRate": 0.25})).unwrap();
        assert!(on.enabled);
        assert_eq!(on.environment.as_deref(), Some("staging"));
        assert_eq!(on.traces_sample_rate, Some(0.25));
        assert_eq!(parse_remote(&json!({"enabled": false})).map(|r| r.enabled), Some(false));
        assert_eq!(parse_remote(&json!({"enabled": true})).map(|r| r.enabled), Some(false), "no DSN = off");
        assert_eq!(parse_remote(&json!({"enabled": true, "dsn": "x", "tracesSampleRate": 7})).unwrap().traces_sample_rate, None);
        assert_eq!(parse_remote(&json!({"error": "nope"})), None);
    }

    #[test]
    fn the_first_account_with_a_server() {
        let text = r#"{"accounts": [
            {"id": "a", "instanceUrl": "", "userId": "u0", "email": "x@y"},
            {"id": "b", "instanceUrl": "https://app.exponential.at/", "userId": "u1", "email": "a@b", "name": "A"}
        ]}"#;
        assert_eq!(parse_accounts(text), Some(Account { server: "https://app.exponential.at".into(), user_id: Some("u1".into()) }));
        assert_eq!(parse_accounts(r#"{"accounts": []}"#), None);
        assert_eq!(parse_accounts("garbage"), None);
    }

    #[test]
    fn the_cache_belongs_to_its_server() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/monitoring.json");
        let remote = Remote { enabled: true, dsn: Some("https://k@g.example/1".into()), environment: Some("prod".into()), traces_sample_rate: Some(0.1) };
        write_cache(&path, "https://app.exponential.at", &remote).unwrap();
        assert_eq!(read_cache(&path, "https://app.exponential.at"), Some(remote));
        assert_eq!(read_cache(&path, "https://other.example"), None);
    }

    #[test]
    fn sample_rates() {
        assert_eq!(sample_rate("0.5"), Some(0.5));
        assert_eq!(sample_rate(" 1 "), Some(1.0));
        assert_eq!(sample_rate("2"), None);
        assert_eq!(sample_rate("NaN"), None);
        assert_eq!(sample_rate("-0.1"), None);
    }

    #[test]
    fn a_target_never_prints_its_dsn() {
        let t = Target { url: "https://s/api/monitoring".into(), dsn: "https://secretkey@g.example/1".into() };
        assert!(!format!("{t:?}").contains("secretkey"));
    }
}
