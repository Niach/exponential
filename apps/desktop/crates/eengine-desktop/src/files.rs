//! `files`: the platform's open and save dialogs.

use std::sync::Arc;

use eengine::providers::Failure;
use eengine::Capability;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::mcp::{self, answer};
use crate::{mime, Dialogs, NativeError, NativeResult, OpenRequest, SaveRequest};

pub struct FilesCapability {
    dialogs: Arc<dyn Dialogs>,
}

#[derive(Deserialize)]
struct OpenArgs {
    #[serde(default)]
    accept: Vec<String>,
    #[serde(default)]
    multiple: bool,
}

/// The bridge's `{name, mimeType, data}`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileArgs {
    pub name: String,
    pub mime_type: String,
    pub data: String,
}

impl FileArgs {
    pub(crate) fn decode(self) -> NativeResult<crate::NamedBytes> {
        let bytes = mcp::decode(&self.data)?;
        Ok(crate::NamedBytes { name: self.name, mime_type: self.mime_type, bytes })
    }
}

impl FilesCapability {
    pub fn new(dialogs: Arc<dyn Dialogs>) -> Self {
        FilesCapability { dialogs }
    }

    fn open(&self, args: OpenArgs) -> NativeResult<Value> {
        let request = OpenRequest { title: "Open".into(), filter: mime::filter(&args.accept), multiple: args.multiple };
        let Some(paths) = self.dialogs.open(&request)? else { return Ok(json!({"files": []})) };
        let max = if args.multiple { 100 } else { 1 };
        let mut files = Vec::new();
        for path in paths.into_iter().take(max) {
            let bytes = mcp::read_file(&path)?;
            let name = mcp::file_name(&path);
            let mime_type = mcp::truncate(&mime::of_file(&name, &bytes), 100);
            files.push(json!({"name": name, "mimeType": mime_type, "data": mcp::encode(&bytes)}));
        }
        Ok(json!({"files": files}))
    }

    fn save(&self, args: FileArgs) -> NativeResult<Value> {
        let file = args.decode()?;
        let request = SaveRequest { name: file.name.clone(), mime_type: file.mime_type.clone() };
        let Some(path) = self.dialogs.save(&request)? else { return Ok(json!({"saved": false})) };
        std::fs::write(&path, &file.bytes).map_err(|e| NativeError::Failed(format!("cannot write {}: {e}", path.display())))?;
        Ok(json!({"saved": true}))
    }
}

impl Capability for FilesCapability {
    fn name(&self) -> &str {
        "files"
    }

    fn tools(&self) -> Vec<Value> {
        mcp::tools("files")
    }

    fn call(&self, tool: &str, arguments: &Value) -> Result<Value, Failure> {
        let result = match tool {
            "open" => mcp::arguments(arguments).and_then(|a| self.open(a)),
            "save" => mcp::arguments(arguments).and_then(|a| self.save(a)),
            _ => return Err(mcp::unknown("files", tool)),
        };
        Ok(answer(result))
    }
}
