//! NEW-58: the linux capturer (screen-memory.json CAPTURE). Filled by its lane.

use std::path::Path;

use super::{Capturer, Frame};

struct Stub;

impl Capturer for Stub {
    fn grab_all(&mut self) -> Result<Vec<Frame>, String> {
        Ok(Vec::new())
    }

    fn grab(&mut self, _display: u32) -> Result<Option<Frame>, String> {
        Ok(None)
    }
}

pub(super) fn open(_state_dir: &Path) -> Result<Box<dyn Capturer>, String> {
    Ok(Box::new(Stub))
}
