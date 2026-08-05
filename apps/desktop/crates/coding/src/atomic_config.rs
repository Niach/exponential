//! Write-then-rename replacement for a user-owned config file that preserves
//! the original file's permission bits. A plain `fs::write` + `rename` creates
//! the temp at the umask default and the rename carries that mode onto the
//! target — silently widening a 0600 file. The trust seeders' targets
//! (`~/.claude.json`, codex `config.toml`) hold OAuth state and API keys, so
//! they must never come out more readable than they went in.

use std::path::Path;

pub fn replace_preserving_mode(
    config: &Path,
    temp: &Path,
    contents: &str,
) -> Result<(), String> {
    std::fs::write(temp, contents).map_err(|err| format!("write {}: {err}", temp.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // 0600 when the target does not exist yet: everything routed through
        // here is private per-user state.
        let mode = std::fs::metadata(config)
            .map(|meta| meta.permissions().mode() & 0o777)
            .unwrap_or(0o600);
        std::fs::set_permissions(temp, std::fs::Permissions::from_mode(mode))
            .map_err(|err| format!("chmod {}: {err}", temp.display()))?;
    }
    std::fs::rename(temp, config).map_err(|err| {
        let _ = std::fs::remove_file(temp);
        format!("replace {}: {err}", config.display())
    })?;
    Ok(())
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

    #[test]
    fn keeps_a_0600_target_at_0600() {
        let config = temp_config("keep");
        std::fs::write(&config, "{}").unwrap();
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600)).unwrap();

        let temp = config.with_extension("json.exp-tmp");
        replace_preserving_mode(&config, &temp, "{\"a\":1}").unwrap();

        assert_eq!(mode_of(&config), 0o600);
        assert_eq!(std::fs::read_to_string(&config).unwrap(), "{\"a\":1}");
        assert!(!temp.exists());
        let _ = std::fs::remove_dir_all(config.parent().unwrap());
    }

    #[test]
    fn creates_a_fresh_target_at_0600() {
        let config = temp_config("fresh");

        let temp = config.with_extension("json.exp-tmp");
        replace_preserving_mode(&config, &temp, "{}").unwrap();

        assert_eq!(mode_of(&config), 0o600);
        let _ = std::fs::remove_dir_all(config.parent().unwrap());
    }
}
