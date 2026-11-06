//! The bundle (runner.json `bundle`): where `app/` sits per platform, and
//! reading it: `runner.json` through `runner_config`, then the package file.
//! The runner never reads the network for its package.

use std::path::{Path, PathBuf};

use eengine::runner::{bundle_config_file, bundle_dir, bundle_package_file};
use eengine::{runner_config, RunnerConfig, RunnerFailure};
use serde_json::Value;

/// The desktop platforms a runner is built for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Macos,
    Windows,
    Linux,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Platform::Macos
        } else if cfg!(target_os = "windows") {
            Platform::Windows
        } else {
            Platform::Linux
        }
    }
}

/// Where `app/` may sit for an executable at `exe`, in order:
/// macOS `<name>.app/Contents/Resources/app`, Windows `<install dir>/app`,
/// Linux `<exe dir>/app`, then `<exe dir>/../share/<config id>/app` (AppImage,
/// .deb; `<config id>` is checked by [`find_bundle`]).
pub fn candidates(platform: Platform, exe: &Path) -> Vec<PathBuf> {
    let Some(dir) = exe.parent() else { return Vec::new() };
    match platform {
        Platform::Macos => vec![dir.join("..").join("Resources").join(bundle_dir())],
        Platform::Windows => vec![dir.join(bundle_dir())],
        Platform::Linux => {
            let mut out = vec![dir.join(bundle_dir())];
            let share = dir.join("..").join("share");
            let mut ids: Vec<PathBuf> = std::fs::read_dir(&share).into_iter().flatten().filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.join(bundle_dir()).is_dir()).collect();
            ids.sort();
            out.extend(ids.into_iter().map(|p| p.join(bundle_dir())));
            out
        }
    }
}

/// The config id a `share/<id>/app` candidate must carry, if it is one.
fn share_id(candidate: &Path) -> Option<String> {
    let id_dir = candidate.parent()?;
    let share = id_dir.parent()?;
    (share.file_name()? == "share").then(|| id_dir.file_name()?.to_str().map(str::to_string)).flatten()
}

/// The first candidate that is a directory; a `share/<id>/app` one only
/// when its runner.json says that `id`.
pub fn find_bundle(platform: Platform, exe: &Path) -> Option<PathBuf> {
    candidates(platform, exe).into_iter().find(|c| {
        if !c.is_dir() {
            return false;
        }
        match share_id(c).filter(|_| platform == Platform::Linux) {
            Some(id) => read_json(&c.join(bundle_config_file())).ok().and_then(|v| v.get("id").and_then(Value::as_str).map(str::to_string)).as_deref() == Some(id.as_str()),
            None => true,
        }
    })
}

fn read_json(path: &Path) -> Result<Value, ()> {
    let text = std::fs::read_to_string(path).map_err(|_| ())?;
    serde_json::from_str(&text).map_err(|_| ())
}

/// A bundle that reads: its config and its package file (signed or bare).
#[derive(Clone, Debug, PartialEq)]
pub struct Bundle {
    pub dir: PathBuf,
    pub config: RunnerConfig,
    pub file: Value,
}

/// Why a bundle does not read; `config` when runner.json was fine (the window keeps its name).
#[derive(Clone, Debug, PartialEq)]
pub struct Unreadable {
    pub kind: RunnerFailure,
    pub config: Option<RunnerConfig>,
}

/// Read `app/`: no directory = `missing`; runner.json absent, unparsable or
/// with issues = `config`; no package file = `missing`; a package file that
/// is not JSON = `invalid`. (Signature and validation are the Engine's load.)
pub fn read_bundle(dir: Option<&Path>) -> Result<Bundle, Unreadable> {
    let fail = |kind, config| Unreadable { kind, config };
    let Some(dir) = dir else { return Err(fail(RunnerFailure::Missing, None)) };
    let raw = read_json(&dir.join(bundle_config_file())).unwrap_or(Value::Null);
    let (config, _issues) = runner_config(&raw);
    let Some(config) = config else { return Err(fail(RunnerFailure::Config, None)) };
    let path = dir.join(bundle_package_file());
    if !path.is_file() {
        return Err(fail(RunnerFailure::Missing, Some(config)));
    }
    let Ok(file) = read_json(&path) else { return Err(fail(RunnerFailure::Invalid, Some(config))) };
    Ok(Bundle { dir: dir.to_path_buf(), config, file })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn write_bundle(dir: &Path, config: &Value, pkg: Option<&str>) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("runner.json"), config.to_string()).unwrap();
        if let Some(pkg) = pkg {
            std::fs::write(dir.join("package.app.json"), pkg).unwrap();
        }
    }

    #[test]
    fn the_locations_per_platform() {
        let exe = Path::new("/Applications/Album.app/Contents/MacOS/exponential-runner");
        assert_eq!(candidates(Platform::Macos, exe), vec![PathBuf::from("/Applications/Album.app/Contents/MacOS/../Resources/app")]);
        let exe = Path::new("C:/Program Files/Album/Album.exe");
        assert_eq!(candidates(Platform::Windows, exe), vec![PathBuf::from("C:/Program Files/Album/app")]);
        let exe = Path::new("/nonexistent/usr/bin/album");
        assert_eq!(candidates(Platform::Linux, exe), vec![PathBuf::from("/nonexistent/usr/bin/app")]);
    }

    #[test]
    fn linux_beside_the_exe_then_share_by_config_id() {
        let root = tempfile::tempdir().unwrap();
        let exe = root.path().join("usr/bin/album");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        let config = json!({"id": "at.acme.album", "name": "Album", "version": "1.0.0"});
        // A share dir whose runner.json names another id is not this bundle.
        write_bundle(&root.path().join("usr/share/at.acme.other/app"), &config, Some("{}"));
        assert_eq!(find_bundle(Platform::Linux, &exe), None);
        let share = root.path().join("usr/share/at.acme.album/app");
        write_bundle(&share, &config, Some("{}"));
        assert_eq!(find_bundle(Platform::Linux, &exe), Some(root.path().join("usr/bin/../share/at.acme.album/app")));
        let beside = root.path().join("usr/bin/app");
        write_bundle(&beside, &config, Some("{}"));
        assert_eq!(find_bundle(Platform::Linux, &exe), Some(beside));
    }

    #[test]
    fn reading_a_bundle_and_its_failures() {
        assert_eq!(read_bundle(None).unwrap_err().kind, RunnerFailure::Missing);
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("app");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(read_bundle(Some(&dir)).unwrap_err(), Unreadable { kind: RunnerFailure::Config, config: None });
        write_bundle(&dir, &json!({"id": "at.acme.album", "name": "Album", "version": "1.0.0", "telemetry": "on"}), None);
        assert_eq!(read_bundle(Some(&dir)).unwrap_err().kind, RunnerFailure::Config);
        write_bundle(&dir, &json!({"id": "at.acme.album", "name": "Album", "version": "1.0.0"}), None);
        let missing = read_bundle(Some(&dir)).unwrap_err();
        assert_eq!((missing.kind, missing.config.map(|c| c.name)), (RunnerFailure::Missing, Some("Album".into())));
        std::fs::write(dir.join("package.app.json"), "not json").unwrap();
        assert_eq!(read_bundle(Some(&dir)).unwrap_err().kind, RunnerFailure::Invalid);
        std::fs::write(dir.join("package.app.json"), r#"{"id": "acme.album"}"#).unwrap();
        let bundle = read_bundle(Some(&dir)).unwrap();
        assert_eq!((bundle.config.id.as_str(), bundle.config.network, bundle.file["id"].as_str()), ("at.acme.album", eengine::NetworkPolicy::Off, Some("acme.album")));
    }
}
