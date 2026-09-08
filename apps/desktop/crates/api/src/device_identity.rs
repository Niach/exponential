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
    stable_id(data_dir, "deviceId")
}

/// The CLI daemon's OWN device identity (EXP-403): `cliDeviceId` in the same
/// settings.json. Deliberately DISTINCT from [`device_id`] — the relay keys
/// presence by (user, deviceId) and evicts a same-id reconnect with
/// CLOSE_REPLACED, so a desktop app and a CLI daemon sharing one machine
/// (and one settings.json) would evict each other in a loop if they shared
/// an id. Two ids ⇒ two registry rows ⇒ two independently startable
/// machines, which is also what the UI should show.
pub fn cli_device_id(data_dir: &Path) -> String {
    stable_id(data_dir, "cliDeviceId")
}

/// One read of settings.json, classified — EXP-766: "cannot parse" and "not
/// there" are DIFFERENT answers, and only the second one may mint a new id.
enum Root {
    /// Missing or empty: a fresh install.
    Fresh,
    /// A JSON object we can merge into.
    Object(serde_json::Value),
    /// Present but unreadable (a torn read, a hand-edit, a non-object doc).
    Unreadable,
}

fn read_root(path: &Path) -> Root {
    let Ok(raw) = fs::read_to_string(path) else {
        return Root::Fresh;
    };
    if raw.trim().is_empty() {
        return Root::Fresh;
    }
    match serde_json::from_str::<serde_json::Value>(&raw) {
        Ok(value) if value.is_object() => Root::Object(value),
        _ => Root::Unreadable,
    }
}

fn stable_id(data_dir: &Path, key: &str) -> String {
    let path = data_dir.join("settings.json");
    // EXP-781: read, mint and write under ONE machine-wide section. The
    // desktop app and the CLI daemon share a data dir (REV-20), so on a first
    // run they could both read a file with no id, both mint, and both write —
    // one machine minting two identities, which the relay keys presence by.
    // Held for the whole function: dropped before the read completes, the
    // lock would guard nothing.
    let _guard = crate::settings_lock::locked(data_dir);
    let mut root = match read_root(&path) {
        Root::Fresh => serde_json::Value::Object(Default::default()),
        Root::Object(root) => root,
        Root::Unreadable => {
            // A parse failure is NOT permission to re-mint: the id IS this
            // machine's identity on the relay, and re-minting it splits the
            // device in two. Retry once (a torn read is over in microseconds),
            // then serve an unpersisted id for this run rather than clobber a
            // file we could not read.
            std::thread::sleep(std::time::Duration::from_millis(50));
            match read_root(&path) {
                Root::Object(root) => root,
                Root::Fresh => serde_json::Value::Object(Default::default()),
                Root::Unreadable => {
                    log::warn!(
                        "{} is unreadable; using a temporary {key} for this run",
                        path.display()
                    );
                    return uuid::Uuid::new_v4().to_string();
                }
            }
        }
    };

    if let Some(existing) = root
        .get(key)
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
    {
        return existing.to_string();
    }

    let generated = uuid::Uuid::new_v4().to_string();
    if let Some(object) = root.as_object_mut() {
        object.insert(key.to_string(), serde_json::Value::String(generated.clone()));
    }
    let persist = || -> std::io::Result<()> {
        let mut rendered = serde_json::to_string_pretty(&root)
            .unwrap_or_else(|_| "{}".to_string());
        rendered.push('\n');
        // EXP-766: by rename, like every other writer of this file — a
        // truncating write is what let the next reader see a torn object.
        crate::atomic_file::write_atomic(&path, &rendered)
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

    /// EXP-766: an unreadable settings.json must never MINT over itself — a
    /// re-minted id splits the machine into two relay devices. The run gets a
    /// throwaway id, the file keeps its bytes, and the id comes back once the
    /// file parses again.
    #[test]
    fn an_unparseable_file_neither_re_mints_nor_is_clobbered() {
        let dir = TempDir::new("device-torn");
        let path = dir.0.join("settings.json");
        let torn = r#"{"deviceId":"kept-id","claudePa"#;
        fs::write(&path, torn).unwrap();

        let temporary = device_id(&dir.0);
        assert!(uuid::Uuid::parse_str(&temporary).is_ok());
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            torn,
            "the unreadable file survives untouched"
        );

        // The writer finished: the recorded id is back, and nothing was lost.
        fs::write(&path, r#"{"deviceId":"kept-id","claudePath":"/opt/claude"}"#).unwrap();
        assert_eq!(device_id(&dir.0), "kept-id");
    }

    #[test]
    fn an_empty_file_mints_once() {
        // Empty (not corrupt) is a fresh install: mint, persist, stay stable.
        let dir = TempDir::new("device-empty");
        fs::write(dir.0.join("settings.json"), "").unwrap();
        let first = device_id(&dir.0);
        assert_eq!(first, device_id(&dir.0));
    }

    #[test]
    fn cli_device_id_is_distinct_from_the_desktop_id() {
        // One machine, one settings.json, TWO relay identities — a shared id
        // would make the desktop app and the daemon evict each other's
        // control socket in a CLOSE_REPLACED loop (EXP-403 review finding).
        let dir = TempDir::new("device-cli");
        let desktop = device_id(&dir.0);
        let cli = cli_device_id(&dir.0);
        assert_ne!(desktop, cli);
        assert_eq!(cli, cli_device_id(&dir.0), "stable across calls");
        assert_eq!(desktop, device_id(&dir.0), "desktop id untouched");
    }
}
