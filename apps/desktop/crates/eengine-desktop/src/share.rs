//! `share`: the platform's share sheet.

use std::sync::Arc;

use eengine::providers::Failure;
use eengine::Capability;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::files::FileArgs;
use crate::mcp::{self, answer};
use crate::{NativeError, NativeResult, ShareItem, ShareSheet};

pub struct ShareCapability {
    sheet: Arc<dyn ShareSheet>,
}

#[derive(Deserialize)]
struct SendArgs {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    file: Option<FileArgs>,
}

impl ShareCapability {
    pub fn new(sheet: Arc<dyn ShareSheet>) -> Self {
        ShareCapability { sheet }
    }

    fn send(&self, args: SendArgs) -> NativeResult<Value> {
        let file = args.file.map(FileArgs::decode).transpose()?;
        let item = ShareItem { title: args.title, text: args.text, url: args.url, file };
        if item.text.is_none() && item.url.is_none() && item.file.is_none() {
            return Err(NativeError::Failed("nothing to share".into()));
        }
        let shared = self.sheet.share(&item)?;
        Ok(json!({"shared": shared}))
    }
}

impl Capability for ShareCapability {
    fn name(&self) -> &str {
        "share"
    }

    fn tools(&self) -> Vec<Value> {
        mcp::tools("share")
    }

    fn call(&self, tool: &str, arguments: &Value) -> Result<Value, Failure> {
        let result = match tool {
            "send" => mcp::arguments(arguments).and_then(|a| self.send(a)),
            _ => return Err(mcp::unknown("share", tool)),
        };
        Ok(answer(result))
    }
}
