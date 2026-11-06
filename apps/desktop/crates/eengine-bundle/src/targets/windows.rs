//! `--target windows` (bundle.json `outputs.windows`, `permissions.windows`, `signing.windows`).

use anyhow::{bail, Result};

use super::{Job, Output};

pub fn bundle(_job: &Job) -> Result<Output> {
    bail!("--target windows is not built yet")
}
