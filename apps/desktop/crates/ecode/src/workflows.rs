//! GitHub workflow YAML TEXT → the pipeline model (feature `workflows`,
//! #EXP-1617): what the server does with `yaml` before
//! `ecode_core::read_workflow`, for a host without a server (Rerusted). YAML
//! 1.2 core schema via yaml-rust2: `on` stays a plain key, only true/false
//! are booleans. Text that does not parse = one `yaml` error at the root
//! (`packages/ecode/fixtures/workflow.json` `text` cases).

use std::path::Path;

use ecode_core::{read_workflow, PipelineError, WorkflowRead};
use serde_json::{Map, Number, Value};
use yaml_rust2::{Yaml, YamlLoader};

use crate::{Error, Result};

/// The directory GitHub keeps workflows in.
pub const WORKFLOW_DIR: &str = ".github/workflows";

fn yaml_error() -> WorkflowRead {
    WorkflowRead { file: None, errors: vec![PipelineError { path: String::new(), code: "yaml".into() }] }
}

/// A YAML node as the JSON tree a JS YAML parser yields (map keys become
/// strings, whole floats become integers).
fn to_json(node: &Yaml) -> Option<Value> {
    Some(match node {
        Yaml::Null => Value::Null,
        Yaml::Boolean(b) => Value::Bool(*b),
        Yaml::Integer(i) => Value::Number((*i).into()),
        Yaml::Real(text) => {
            let f: f64 = node.as_f64().or_else(|| text.parse().ok())?;
            if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.0e15 {
                Value::Number((f as i64).into())
            } else {
                Number::from_f64(f).map_or(Value::Null, Value::Number)
            }
        }
        Yaml::String(s) => Value::String(s.clone()),
        Yaml::Array(items) => Value::Array(items.iter().map(to_json).collect::<Option<_>>()?),
        Yaml::Hash(map) => {
            let mut out = Map::new();
            for (k, v) in map {
                let key = match k {
                    Yaml::String(s) => s.clone(),
                    Yaml::Integer(i) => i.to_string(),
                    Yaml::Boolean(b) => b.to_string(),
                    Yaml::Real(r) => r.clone(),
                    Yaml::Null => "null".into(),
                    _ => return None,
                };
                // Duplicate keys are a parse error (as the server's strict parse).
                if out.insert(key, to_json(v)?).is_some() {
                    return None;
                }
            }
            Value::Object(out)
        }
        Yaml::Alias(_) | Yaml::BadValue => return None,
    })
}

/// One workflow's YAML text read into the pipeline model.
pub fn read_workflow_text(text: &str) -> WorkflowRead {
    let Ok(docs) = YamlLoader::load_from_str(text) else { return yaml_error() };
    let tree = match docs.first() {
        None => Value::Null,
        Some(doc) => match to_json(doc) {
            Some(tree) => tree,
            None => return yaml_error(),
        },
    };
    read_workflow(&tree)
}

/// One workflow file of a checkout.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowFile {
    /// The file name (`ci.yml`).
    pub name: String,
    pub read: WorkflowRead,
}

/// Every `*.yml` / `*.yaml` in `<checkout>/.github/workflows`, by name; an
/// empty list when the directory does not exist.
pub fn read_workflows(checkout: &Path) -> Result<Vec<WorkflowFile>> {
    let dir = checkout.join(WORKFLOW_DIR);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(Error::Invalid(format!("{}: {e}", dir.display()))),
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(".yml") || n.ends_with(".yaml"))
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let text = std::fs::read_to_string(dir.join(&name)).map_err(|e| Error::Invalid(format!("{name}: {e}")))?;
            Ok(WorkflowFile { read: read_workflow_text(&text), name })
        })
        .collect()
}
