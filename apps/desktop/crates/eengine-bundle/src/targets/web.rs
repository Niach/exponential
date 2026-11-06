//! `--target web` (bundle.json `outputs.web`, `permissions.web`, `signing.web`).

use anyhow::{bail, Result};

use super::{Job, Output};

pub fn bundle(_job: &Job) -> Result<Output> {
    bail!("--target web is not built yet")
}
