//! `--target android` (bundle.json `outputs.android`, `permissions.android`, `signing.android`).

use anyhow::{bail, Result};

use super::{Job, Output};

pub fn bundle(_job: &Job) -> Result<Output> {
    bail!("--target android is not built yet")
}
