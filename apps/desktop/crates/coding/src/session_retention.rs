//! EXP-886 — how long THIS machine keeps the session history of finished
//! runs: the device-only transcripts (`{data_dir}/journal/<id>.jsonl`,
//! `steer::history`) and the resume records (`runs.json`,
//! [`crate::run_registry`]).
//!
//! One key in the shared `{data_dir}/settings.json`, read by BOTH hosts (the
//! desktop app writes it from Settings → Sessions; the headless CLI daemon
//! honours it at boot without a command of its own):
//!
//! `"sessionRetentionDays": <positive integer>` — keep that many days.
//! Absent, `null`, or anything else (zero, negative, a string, a fraction) =
//! UNLIMITED, the default: history is kept forever. Garbage never deletes
//! anything.
//!
//! gpui-free on purpose, like the rest of this crate.

use std::io;
use std::path::Path;
use std::time::Duration;

/// The `settings.json` key.
pub const SETTINGS_KEY: &str = "sessionRetentionDays";

/// The windows the desktop Settings pane offers, in menu order. `None` =
/// Unlimited (the default).
pub const CHOICES: [(Option<u32>, &str); 4] = [
    (None, "Unlimited"),
    (Some(365), "1 year"),
    (Some(90), "90 days"),
    (Some(30), "30 days"),
];

const SECS_PER_DAY: u64 = 24 * 60 * 60;

/// The value of [`SETTINGS_KEY`], tolerant of anything: only a positive
/// integer that fits `u32` is a window; everything else is unlimited.
pub fn parse(value: Option<&serde_json::Value>) -> Option<u32> {
    value?
        .as_u64()
        .filter(|days| *days > 0)
        .and_then(|days| u32::try_from(days).ok())
}

/// The days as a pruning age; `None` = unlimited.
pub fn as_duration(days: Option<u32>) -> Option<Duration> {
    days.map(|days| Duration::from_secs(u64::from(days) * SECS_PER_DAY))
}

fn read_root(data_dir: &Path) -> Option<serde_json::Value> {
    std::fs::read_to_string(data_dir.join("settings.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .filter(serde_json::Value::is_object)
}

/// The configured window in days; `None` = unlimited (also for a missing or
/// unreadable settings file).
pub fn load_days(data_dir: &Path) -> Option<u32> {
    parse(read_root(data_dir)?.get(SETTINGS_KEY))
}

/// [`load_days`] as a pruning age.
pub fn load(data_dir: &Path) -> Option<Duration> {
    as_duration(load_days(data_dir))
}

/// Persist the window (`None` REMOVES the key = unlimited), merging into the
/// shared file under the machine-wide settings section so no other
/// subsystem's key is lost.
pub fn save_days(data_dir: &Path, days: Option<u32>) -> io::Result<()> {
    let _guard = api::settings_lock::locked(data_dir);
    std::fs::create_dir_all(data_dir)?;
    let mut root =
        read_root(data_dir).unwrap_or_else(|| serde_json::Value::Object(Default::default()));
    if let Some(object) = root.as_object_mut() {
        match days.filter(|days| *days > 0) {
            Some(days) => {
                object.insert(SETTINGS_KEY.to_string(), serde_json::Value::from(days));
            }
            None => {
                object.remove(SETTINGS_KEY);
            }
        }
    }
    let mut rendered = serde_json::to_string_pretty(&root).unwrap_or_else(|_| "{}".to_string());
    rendered.push('\n');
    api::atomic_file::write_atomic(&data_dir.join("settings.json"), &rendered)
}

/// Drop the resume records older than `retention` (`None` = unlimited, a
/// no-op). The journal half lives in `steer::prune_session_history`, which
/// calls this — the coding crate never depends on steer.
pub fn prune_run_records(data_dir: &Path, retention: Option<Duration>) -> usize {
    let Some(retention) = retention else {
        return 0;
    };
    let cutoff = crate::run_registry::now_secs().saturating_sub(retention.as_secs());
    crate::run_registry::prune_older_than(data_dir, cutoff)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "exp-session-retention-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn only_a_positive_integer_is_a_window() {
        assert_eq!(parse(Some(&json!(30))), Some(30));
        assert_eq!(parse(Some(&json!(365))), Some(365));
        for garbage in [
            json!(null),
            json!(0),
            json!(-5),
            json!(1.5),
            json!("30"),
            json!(true),
            json!([30]),
            json!({ "days": 30 }),
            json!(u64::from(u32::MAX) + 1),
        ] {
            assert_eq!(parse(Some(&garbage)), None, "{garbage} must read as unlimited");
        }
        assert_eq!(parse(None), None);
        assert_eq!(as_duration(Some(2)), Some(Duration::from_secs(2 * SECS_PER_DAY)));
        assert_eq!(as_duration(None), None);
    }

    #[test]
    fn the_default_is_unlimited() {
        let dir = temp_dir("default");
        assert_eq!(load(&dir), None, "no settings file");
        std::fs::write(dir.join("settings.json"), "not json").unwrap();
        assert_eq!(load(&dir), None, "an unreadable file");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_round_trips_and_keeps_foreign_keys() {
        let dir = temp_dir("save");
        std::fs::write(
            dir.join("settings.json"),
            r#"{"defaultAgent":"codex","cliAutoUpdate":true}"#,
        )
        .unwrap();
        save_days(&dir, Some(90)).unwrap();
        assert_eq!(load_days(&dir), Some(90));
        let root: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(root["defaultAgent"], "codex");
        assert_eq!(root["cliAutoUpdate"], true);
        assert_eq!(root[SETTINGS_KEY], 90);

        // Unlimited removes the key rather than writing a sentinel.
        save_days(&dir, None).unwrap();
        assert_eq!(load_days(&dir), None);
        let raw = std::fs::read_to_string(dir.join("settings.json")).unwrap();
        assert!(!raw.contains(SETTINGS_KEY), "{raw}");
        assert!(raw.contains("defaultAgent"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_choices_are_unlimited_first_then_shrinking() {
        let days: Vec<Option<u32>> = CHOICES.iter().map(|(days, _)| *days).collect();
        assert_eq!(days, vec![None, Some(365), Some(90), Some(30)]);
    }
}
