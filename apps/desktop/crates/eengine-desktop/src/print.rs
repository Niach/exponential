//! `print`: the platform's print dialog for a PDF, PNG or JPEG. `print.pdf`
//! is the Engine's own (no native call), so this lists only `print`.

use std::sync::Arc;

use eengine::providers::Failure;
use eengine::Capability;
use serde_json::{json, Value};

use crate::files::FileArgs;
use crate::mcp::{self, answer};
use crate::{mime, NativeError, NativeResult, Printer};

pub struct PrintCapability {
    printer: Arc<dyn Printer>,
}

/// The kinds a printer is handed: the bridge sniffs, this re-checks the bytes.
pub(crate) fn printable(bytes: &[u8]) -> Option<&'static str> {
    mime::sniff(bytes).filter(|m| matches!(*m, "application/pdf" | "image/png" | "image/jpeg"))
}

impl PrintCapability {
    pub fn new(printer: Arc<dyn Printer>) -> Self {
        PrintCapability { printer }
    }

    fn print(&self, args: FileArgs) -> NativeResult<Value> {
        let mut job = args.decode()?;
        let Some(kind) = printable(&job.bytes) else { return Err(NativeError::Failed("not a PDF, PNG or JPEG".into())) };
        job.mime_type = kind.to_string();
        let printed = self.printer.print(&job)?;
        Ok(json!({"printed": printed}))
    }
}

impl Capability for PrintCapability {
    fn name(&self) -> &str {
        "print"
    }

    fn tools(&self) -> Vec<Value> {
        mcp::tools("print")
    }

    fn call(&self, tool: &str, arguments: &Value) -> Result<Value, Failure> {
        let result = match tool {
            "print" => mcp::arguments(arguments).and_then(|a| self.print(a)),
            _ => return Err(mcp::unknown("print", tool)),
        };
        Ok(answer(result))
    }
}
