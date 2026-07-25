//! §7.7 deviceId — the stable per-install device identity, generated once
//! and never regenerated. (Formerly part of the actions trust store, which
//! EXP-268 removed — actions run without a per-device trust prompt now.)

use std::fs;
use std::path::Path;

/// The stable per-install device UUID: the `deviceId` key of
/// `{data_dir}/settings.json`. Created (and persisted, merge-preserving) on
/// first call; identical forever after. If persisting fails the generated id
/// is still returned — callers keep working, the id just won't be stable
/// until the disk is writable again. Reads/writes merge over the existing
/// JSON object so keys owned by other subsystems (`claudePath`, `reposRoot`,
/// …) survive.
pub fn device_id(data_dir: &Path) -> String {
    let path = data_dir.join("settings.json");
    let mut root = fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::Value::Object(Default::default()));

    if let Some(existing) = root
        .get("deviceId")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        return existing.to_string();
    }

    let generated = uuid::Uuid::new_v4().to_string();
    if let Some(object) = root.as_object_mut() {
        object.insert(
            "deviceId".to_string(),
            serde_json::Value::String(generated.clone()),
        );
    }
    let persist = || -> std::io::Result<()> {
        fs::create_dir_all(data_dir)?;
        let mut rendered = serde_json::to_string_pretty(&root)
            .unwrap_or_else(|_| "{}".to_string());
        rendered.push('\n');
        fs::write(&path, rendered)
    };
    let _ = persist();
    generated
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let mut path = std::env::temp_dir();
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            path.push(format!("exp-device-{tag}-{}-{nanos}", std::process::id()));
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
    fn device_id_is_stable_and_uuid_shaped() {
        let dir = TempDir::new("device-id");
        let first = device_id(&dir.0);
        let second = device_id(&dir.0);
        assert_eq!(first, second, "generate once, never regenerate (§7.7)");
        assert_eq!(first.len(), 36);
        assert!(uuid::Uuid::parse_str(&first).is_ok());
    }

    #[test]
    fn device_id_preserves_foreign_settings_keys() {
        // settings.json is shared with the coding settings (§7.7) — the merge
        // write must not clobber them.
        let dir = TempDir::new("device-merge");
        fs::write(
            dir.0.join("settings.json"),
            r#"{"claudePath":"/opt/claude","reposRoot":"~/code"}"#,
        )
        .unwrap();
        let id = device_id(&dir.0);
        let root: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dir.0.join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(root["claudePath"], "/opt/claude");
        assert_eq!(root["reposRoot"], "~/code");
        assert_eq!(root["deviceId"], id.as_str());
    }

    #[test]
    fn device_id_respects_an_existing_value() {
        let dir = TempDir::new("device-existing");
        fs::write(
            dir.0.join("settings.json"),
            r#"{"deviceId":"pre-existing-id"}"#,
        )
        .unwrap();
        assert_eq!(device_id(&dir.0), "pre-existing-id");
    }
}
