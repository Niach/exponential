//! `--target ios` (bundle.json `outputs.ios`, `permissions.ios`, `signing.ios`).

use anyhow::{bail, Result};

use super::{Job, Output};

pub fn bundle(_job: &Job) -> Result<Output> {
    bail!("--target ios is not built yet")
}
