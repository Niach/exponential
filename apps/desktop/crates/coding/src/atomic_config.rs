//! Write-then-rename replacement for a user-owned config file that preserves
//! the original file's permission bits. A plain `fs::write` + `rename` creates
//! the temp at the umask default and the rename carries that mode onto the
//! target — silently widening a 0600 file. The trust seeders' targets
//! (`~/.claude.json`, codex `config.toml`) hold OAuth state and API keys, so
//! they must never come out more readable than they went in.
//!
//! EXP-766: the writer itself now lives in [`api::atomic_file`] — the crate
//! below `coding`, so `api::device_identity` and the CLI's prefs share it —
//! and picks its own UNIQUE temp name (a fixed one let two processes collide
//! on the same temp) plus an fsync before the rename.

use std::path::Path;

pub use api::atomic_file::write_atomic;

/// Replace `config`'s contents in one rename, keeping its permission bits.
pub fn replace_preserving_mode(config: &Path, contents: &str) -> Result<(), String> {
    write_atomic(config, contents).map_err(|err| format!("replace {}: {err}", config.display()))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    fn temp_config(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-atomic-config-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("config.json")
    }

    fn mode_of(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    fn temp_leftovers(dir: &Path) -> usize {
        std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count()
    }

    #[test]
    fn keeps_a_0600_target_at_0600() {
        let config = temp_config("keep");
        std::fs::write(&config, "{}").unwrap();
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600)).unwrap();

        replace_preserving_mode(&config, "{\"a\":1}").unwrap();

        assert_eq!(mode_of(&config), 0o600);
        assert_eq!(std::fs::read_to_string(&config).unwrap(), "{\"a\":1}");
        assert_eq!(temp_leftovers(config.parent().unwrap()), 0);
        let _ = std::fs::remove_dir_all(config.parent().unwrap());
    }

    #[test]
    fn creates_a_fresh_target_at_0600() {
        let config = temp_config("fresh");

        replace_preserving_mode(&config, "{}").unwrap();

        assert_eq!(mode_of(&config), 0o600);
        let _ = std::fs::remove_dir_all(config.parent().unwrap());
    }
}
