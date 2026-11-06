//! Step 2 (`kits`): the prebuilt runner kit of a target: a workspace build
//! (`--runners <dir>`, `scripts/runner-kits.sh`) or the release assets of the
//! pinned `eengine-v<version>`. Every kit carries its `NOTICES.txt`.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context as _, Result};
use sha2::{Digest, Sha256};

use crate::contract::BUNDLE;

/// The `eengine-v*` release whose runners this bundler pins (lockstep with eengine).
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn notices_file() -> &'static str {
    BUNDLE["kits"]["notices"].as_str().unwrap_or("NOTICES.txt")
}

/// The release asset URL of a target's kit.
pub fn release_url(target: &str) -> String {
    BUNDLE["kits"]["release"].as_str().unwrap_or_default().replace("{version}", ENGINE_VERSION).replace("{target}", target)
}

/// A kit directory is usable when it holds its notices.
pub fn check(dir: &Path) -> Result<()> {
    if !dir.is_dir() {
        bail!("no runner kit at {}", dir.display());
    }
    if !dir.join(notices_file()).is_file() {
        bail!("the runner kit at {} has no {} (every export ships the notices of what it runs)", dir.display(), notices_file());
    }
    Ok(())
}

/// The kit of `target`: `<runners>/<target>`, else the pinned release's asset (cached).
pub fn resolve(target: &str, runners: Option<&Path>, cache: &Path) -> Result<PathBuf> {
    if let Some(runners) = runners {
        let dir = runners.join(target);
        check(&dir)?;
        return Ok(dir);
    }
    let dir = cache.join(format!("eengine-v{ENGINE_VERSION}")).join(target);
    if check(&dir).is_ok() {
        return Ok(dir);
    }
    fetch(target, &dir)?;
    check(&dir)?;
    Ok(dir)
}

fn fetch(target: &str, dir: &Path) -> Result<()> {
    let url = release_url(target);
    let tmp = dir.with_extension("download");
    std::fs::create_dir_all(&tmp)?;
    let archive = tmp.join("kit.tar.gz");
    let sum = tmp.join("kit.tar.gz.sha256");
    let get = |url: &str, to: &Path| Command::new("curl").args(["-fsSL", "-o"]).arg(to).arg(url).status().map(|s| s.success()).unwrap_or(false);
    if !get(&url, &archive) || !get(&format!("{url}.sha256"), &sum) {
        let _ = std::fs::remove_dir_all(&tmp);
        bail!("no published runner kit for eengine-v{ENGINE_VERSION} ({url}). Build the kits from the workspace (scripts/runner-kits.sh) and pass --runners <dir>");
    }
    let want = std::fs::read_to_string(&sum)?.split_whitespace().next().unwrap_or_default().to_lowercase();
    let got = format!("{:x}", Sha256::digest(std::fs::read(&archive)?));
    if want != got {
        let _ = std::fs::remove_dir_all(&tmp);
        bail!("the runner kit {url} does not match its .sha256");
    }
    std::fs::create_dir_all(dir)?;
    let ok = Command::new("tar").arg("-xzf").arg(&archive).arg("-C").arg(dir).status().context("tar")?.success();
    let _ = std::fs::remove_dir_all(&tmp);
    if !ok {
        bail!("could not unpack {url}");
    }
    Ok(())
}
