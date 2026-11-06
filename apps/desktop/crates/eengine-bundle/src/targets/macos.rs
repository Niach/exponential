//! `--target macos` (bundle.json `outputs.macos`, `permissions.macos`, `signing.macos`).

use anyhow::{bail, Result};

use super::{Job, Output};

pub fn bundle(_job: &Job) -> Result<Output> {
    bail!("--target macos is not built yet")
}
