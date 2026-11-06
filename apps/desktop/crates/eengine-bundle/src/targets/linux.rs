//! `--target linux` (bundle.json `outputs.linux`, `permissions.linux`, `signing.linux`).

use anyhow::{bail, Result};

use super::{Job, Output};

pub fn bundle(_job: &Job) -> Result<Output> {
    bail!("--target linux is not built yet")
}
