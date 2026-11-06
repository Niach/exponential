//! Storage (runner.json `storage`): per app id under the platform's app-data
//! directory (macOS `~/Library/Application Support`, Linux
//! `$XDG_DATA_HOME` or `~/.local/share`, Windows `%LOCALAPPDATA%`): the
//! records in `SqliteRecordStore`, the files in `FsBlobStore`.

use std::path::{Path, PathBuf};

use eengine::contract::RUNNER;

/// `<app-data>/<config id>`; `base` = the platform's app-data directory.
pub fn app_dir(base: &Path, id: &str) -> PathBuf {
    base.join(id)
}

/// The platform's app-data directory (`None` = it has none: the runner refuses to start).
pub fn platform_base() -> Option<PathBuf> {
    dirs::data_local_dir()
}

pub fn records_path(dir: &Path) -> PathBuf {
    dir.join(RUNNER["storage"]["records"].as_str().unwrap_or("records.sqlite"))
}

pub fn files_dir(dir: &Path) -> PathBuf {
    dir.join(RUNNER["storage"]["files"].as_str().unwrap_or("files"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_app_id_with_the_contract_names() {
        let dir = app_dir(Path::new("/data"), "at.acme.album");
        assert_eq!(dir, PathBuf::from("/data/at.acme.album"));
        assert_eq!(records_path(&dir), PathBuf::from("/data/at.acme.album/records.sqlite"));
        assert_eq!(files_dir(&dir), PathBuf::from("/data/at.acme.album/files"));
    }
}
