//! The run: prepare once, then every requested target in order.

use std::path::PathBuf;

use anyhow::{bail, Context as _, Result};

use crate::icons::Icon;
use crate::prepare::{prepare, targets};
use crate::targets::{self, Job, Output};

pub struct Options {
    pub package: PathBuf,
    pub icon: PathBuf,
    pub config: PathBuf,
    pub targets: Vec<String>,
    /// Workspace-built runner kits (`<runners>/<target>`); `None` = the pinned release.
    pub runners: Option<PathBuf>,
    pub out: PathBuf,
    pub allow_unsigned: bool,
    /// Where fetched release kits are cached.
    pub cache: PathBuf,
}

/// bundle.toml → JSON (what `prepare` reads).
pub fn read_bundle_toml(text: &str) -> Result<serde_json::Value> {
    let value: toml::Value = toml::from_str(text).context("bundle.toml is not TOML")?;
    Ok(serde_json::to_value(value)?)
}

pub fn run(opts: &Options) -> Result<Vec<(String, Output)>> {
    let known = targets();
    for t in &opts.targets {
        if !known.contains(t) {
            bail!("unknown target `{t}` (one of {})", known.join(", "));
        }
    }
    if opts.targets.is_empty() {
        bail!("no --target (one or more of {})", known.join(", "));
    }
    let file = std::fs::read(&opts.package).with_context(|| format!("read {}", opts.package.display()))?;
    let toml_text = std::fs::read_to_string(&opts.config).with_context(|| format!("read {}", opts.config.display()))?;
    let prepared = prepare(&file, &read_bundle_toml(&toml_text)?, opts.allow_unsigned)?;
    let icon = Icon::from_svg(&std::fs::read(&opts.icon).with_context(|| format!("read {}", opts.icon.display()))?)?;
    std::fs::create_dir_all(&opts.out)?;
    let mut done = Vec::new();
    for target in &opts.targets {
        let kit = crate::kit::resolve(target, opts.runners.as_deref(), &opts.cache)?;
        let work = opts.out.join(".work").join(target);
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(&work)?;
        let job = Job { prepared: &prepared, icon: &icon, kit, out: opts.out.clone(), work: work.clone() };
        let result = targets::bundle(target, &job).with_context(|| format!("--target {target}"));
        let _ = std::fs::remove_dir_all(&work);
        done.push((target.clone(), result?));
    }
    let _ = std::fs::remove_dir(opts.out.join(".work"));
    Ok(done)
}
