//! One module per target: each turns the prepared app + its runner kit into
//! the target's outputs (`outputs`), writing the platform manifest and signing.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::icons::Icon;
use crate::prepare::{config_value, Prepared};

pub mod android;
pub mod ios;
pub mod linux;
pub mod macos;
pub mod web;
pub mod windows;

/// One target's run.
pub struct Job<'a> {
    pub prepared: &'a Prepared,
    pub icon: &'a Icon,
    /// The target's runner kit (`kit::resolve`), read only.
    pub kit: PathBuf,
    /// Where the outputs go (`--out`).
    pub out: PathBuf,
    /// A scratch directory of this target, removed after the run.
    pub work: PathBuf,
}

/// What a target wrote, and what a person should know (ad-hoc signing, a debug key…).
#[derive(Debug, Default)]
pub struct Output {
    pub files: Vec<PathBuf>,
    pub notes: Vec<String>,
}

impl Job<'_> {
    /// Write the bundle (runner.json `bundle`) into `dir`: `package.app.json` byte for byte, `runner.json`.
    pub fn write_app_dir(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join(eengine::runner::bundle_package_file()), &self.prepared.file_bytes)?;
        let config = serde_json::to_string_pretty(&config_value(&self.prepared.config))?;
        std::fs::write(dir.join(eengine::runner::bundle_config_file()), format!("{config}\n"))?;
        Ok(())
    }

    /// The kit's third-party notices.
    pub fn notices(&self) -> PathBuf {
        self.kit.join(crate::kit::notices_file())
    }

    /// An `outputs` pattern filled in: `{name}`, `{version}`, `{id}`, `{arch}`.
    pub fn output_name(&self, pattern: &str, arch: &str) -> String {
        let c = &self.prepared.config;
        pattern.replace("{name}", &c.name).replace("{version}", &c.version).replace("{id}", &c.id).replace("{arch}", arch)
    }

    /// This target's `[signing.<target>]` table of bundle.toml, if any.
    pub fn signing(&self, target: &str) -> Option<&serde_json::Map<String, serde_json::Value>> {
        self.prepared.signing.get(target).and_then(serde_json::Value::as_object)
    }
}

/// Run one target.
pub fn bundle(target: &str, job: &Job) -> Result<Output> {
    match target {
        "macos" => macos::bundle(job),
        "windows" => windows::bundle(job),
        "linux" => linux::bundle(job),
        "android" => android::bundle(job),
        "ios" => ios::bundle(job),
        "web" => web::bundle(job),
        other => anyhow::bail!("unknown target `{other}`"),
    }
}
