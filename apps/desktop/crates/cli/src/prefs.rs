//! CLI-only preferences riding the shared `{data_dir}/settings.json` —
//! merge-preserving reads/writes exactly like `api::device_identity` and
//! `coding::Settings` (the file is the ONE per-install store; foreign keys
//! must survive every write).

use std::path::Path;

fn read_root(data_dir: &Path) -> serde_json::Value {
    std::fs::read_to_string(data_dir.join("settings.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::Value::Object(Default::default()))
}

fn write_key(data_dir: &Path, key: &str, value: serde_json::Value) {
    let mut root = read_root(data_dir);
    if let Some(object) = root.as_object_mut() {
        object.insert(key.to_string(), value);
    }
    let persist = || -> std::io::Result<()> {
        std::fs::create_dir_all(data_dir)?;
        let mut rendered = serde_json::to_string_pretty(&root).unwrap_or_else(|_| "{}".to_string());
        rendered.push('\n');
        std::fs::write(data_dir.join("settings.json"), rendered)
    };
    let _ = persist();
}

/// `None` = never asked (the first interactive run prompts).
pub fn auto_update(data_dir: &Path) -> Option<bool> {
    read_root(data_dir).get("cliAutoUpdate")?.as_bool()
}

pub fn set_auto_update(data_dir: &Path, enabled: bool) {
    write_key(data_dir, "cliAutoUpdate", serde_json::Value::Bool(enabled));
}

/// Unix-seconds timestamp of the last release-check (throttles the
/// unauthenticated GitHub API to a handful of calls per day).
pub fn last_update_check(data_dir: &Path) -> Option<u64> {
    read_root(data_dir).get("cliLastUpdateCheck")?.as_u64()
}

pub fn set_last_update_check(data_dir: &Path, epoch_secs: u64) {
    write_key(
        data_dir,
        "cliLastUpdateCheck",
        serde_json::Value::Number(epoch_secs.into()),
    );
}

pub fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
