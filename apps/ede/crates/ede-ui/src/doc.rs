//! What a file in the ui directory may hold: a shell document (an eUI
//! surface in the nested authoring form or as A2UI's flat component list)
//! or the theme. Both are validated by eui-core before anything applies.

use std::sync::Arc;

use eui_core::{
    reducer::{reduce_nested, reduce_surface, ReduceOptions},
    surface::{Surface, SurfaceOptions},
    theme::{try_load_theme, ResolvedTheme, ThemeOptions},
    themes::builtin_refs,
    FlatComponent, NestedNode, UiNode, CORE_CATALOG_ID,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::surface::{normalize_surface, SurfaceHeader};

/// The keys a shell document may carry.
pub const DOCUMENT_KEYS: &[&str] = &["$schema", "$comment", "catalogId", "surface", "root", "components", "data"];

/// One problem with a file: where (`#<node id>`, a key, or `file`) and what.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    pub path: String,
    pub message: String,
}

impl Issue {
    pub fn new(path: impl Into<String>, message: impl Into<String>) -> Issue {
        Issue { path: path.into(), message: message.into() }
    }
}

impl std::fmt::Display for Issue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

/// A document's components, as written.
#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    /// `"root": {"id", "component", "props", "children": [...]}`.
    Nested(NestedNode),
    /// `"components": [...]` (A2UI `updateComponents`), from the component `root`.
    Flat(Vec<FlatComponent>),
}

/// A shell document that passed validation.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    /// The file stem (`panel` for `panel.json`).
    pub name: String,
    /// Where the shell puts it (#EDE-8).
    pub surface: SurfaceHeader,
    pub catalog_id: String,
    pub body: Body,
    pub data: Option<Value>,
    /// The normalized tree eui-core reduced it to.
    pub tree: UiNode,
    /// The file's JSON as it was read.
    pub source: Value,
}

impl Document {
    /// A surface showing this document under `theme`; a painter lays it
    /// out and draws it.
    pub fn surface(&self, theme: Arc<ResolvedTheme>) -> Surface {
        let options = SurfaceOptions { catalog_id: self.catalog_id.clone(), theme: Some(theme), ..SurfaceOptions::default() };
        let mut surface = Surface::new(&self.name, options);
        match &self.body {
            Body::Nested(tree) => {
                surface.set_nested(tree.clone());
            }
            Body::Flat(components) => {
                surface.set_components(components.clone());
            }
        }
        if let Some(data) = &self.data {
            let _ = surface.set_data("", Some(data.clone()));
        }
        surface
    }
}

/// Validate a shell document: its shape, then every component through
/// eui-core's reducer (unknown components, props, styles, tokens, limits).
/// Any reducer issue refuses the document: a shell never applies half a
/// surface.
pub fn validate_document(name: &str, source: &Value) -> Result<Document, Vec<Issue>> {
    let Some(obj) = source.as_object() else {
        return Err(vec![Issue::new("file", "expected a document object {surface, root | components, data}")]);
    };
    let mut issues = Vec::new();
    for key in obj.keys() {
        if !DOCUMENT_KEYS.contains(&key.as_str()) {
            issues.push(Issue::new(key.clone(), format!("unknown key; known: {}", DOCUMENT_KEYS.join("|"))));
        }
    }
    let surface = match obj.get("surface") {
        None => {
            issues.push(Issue::new("surface", "required: how the surface sits on the screen"));
            None
        }
        Some(header) => match normalize_surface(header) {
            Ok(header) => Some(header),
            Err(header_issues) => {
                issues.extend(header_issues);
                None
            }
        },
    };
    let body = match (obj.get("root"), obj.get("components")) {
        (Some(root), None) => match serde_json::from_value::<NestedNode>(root.clone()) {
            Ok(tree) => Some(Body::Nested(tree)),
            Err(e) => {
                issues.push(Issue::new("root", e.to_string()));
                None
            }
        },
        (None, Some(components)) => match serde_json::from_value::<Vec<FlatComponent>>(components.clone()) {
            Ok(components) => Some(Body::Flat(components)),
            Err(e) => {
                issues.push(Issue::new("components", e.to_string()));
                None
            }
        },
        (Some(_), Some(_)) => {
            issues.push(Issue::new("file", "expected root OR components, not both"));
            None
        }
        (None, None) => {
            issues.push(Issue::new("file", "expected root (a nested tree) or components (a flat list)"));
            None
        }
    };
    // Tier C (#EDE-8): a document grants no privileges. Structural, on the
    // JSON as written, before eui-core's reducer sees it.
    let functions: &[String] = surface.as_ref().map(SurfaceHeader::functions).unwrap_or(&[]);
    let mut nodes: Vec<&Value> = Vec::new();
    match (obj.get("root"), obj.get("components")) {
        (Some(root), None) => collect_nodes(root, &mut nodes),
        (None, Some(Value::Array(list))) => nodes.extend(list.iter().filter(|c| c.is_object())),
        _ => {}
    }
    let mut seen = std::collections::HashSet::new();
    for node in nodes {
        let id = node.get("id").and_then(Value::as_str).unwrap_or("?");
        if !seen.insert(id.to_string()) {
            issues.push(Issue::new(format!("#{id}"), "id used more than once"));
        }
        let kind = node.get("props").filter(|p| p.is_object()).unwrap_or(node).get("type").and_then(Value::as_str);
        if node.get("component").and_then(Value::as_str) == Some("Input") && kind == Some("password") {
            issues.push(Issue::new(format!("#{id}"), "password inputs are refused in a shell document (a sealed surface asks for passwords)"));
        }
        if let Some(on) = node.get("on").and_then(Value::as_object) {
            for handler in on.values() {
                if let Some(call) = handler.get("functionCall").and_then(|f| f.get("call")).and_then(Value::as_str) {
                    if !functions.iter().any(|f| f == call) {
                        issues.push(Issue::new(format!("#{id}"), format!("function {call} is not in surface.functions")));
                    }
                }
            }
        }
    }
    let catalog_id = match obj.get("catalogId") {
        None => CORE_CATALOG_ID.to_string(),
        Some(Value::String(id)) if eui_core::catalog::SUPPORTED_CATALOG_IDS.contains(&id.as_str()) => id.clone(),
        Some(_) => {
            issues.push(Issue::new("catalogId", format!("expected one of {}", eui_core::catalog::SUPPORTED_CATALOG_IDS.join("|"))));
            CORE_CATALOG_ID.to_string()
        }
    };
    let data = obj.get("data").cloned();
    if data.as_ref().is_some_and(|d| !d.is_object()) {
        issues.push(Issue::new("data", "expected an object"));
    }
    let (Some(body), Some(surface)) = (body, surface) else {
        return Err(issues);
    };
    if !issues.is_empty() {
        return Err(issues);
    }
    let options = ReduceOptions::new(&catalog_id);
    let reduced = match &body {
        Body::Nested(tree) => reduce_nested(tree, &options),
        Body::Flat(components) => reduce_surface(components, &options),
    };
    issues.extend(reduced.issues.into_iter().map(|i| Issue::new(format!("#{}", i.id), i.message)));
    if !issues.is_empty() {
        return Err(issues);
    }
    Ok(Document { name: name.to_string(), surface, catalog_id, body, data, tree: reduced.root, source: source.clone() })
}

/// Every node of the nested authoring form, parents first.
fn collect_nodes<'a>(node: &'a Value, out: &mut Vec<&'a Value>) {
    if !node.is_object() {
        return;
    }
    out.push(node);
    if let Some(children) = node.get("children").and_then(Value::as_array) {
        children.iter().for_each(|c| collect_nodes(c, out));
    }
    if let Some(slots) = node.get("slots").and_then(Value::as_object) {
        slots.values().for_each(|c| collect_nodes(c, out));
    }
}

/// Validate and resolve `theme.json` (eUI's theme format; `extends` may
/// name a built-in theme).
pub fn validate_theme(source: &Value) -> Result<Arc<ResolvedTheme>, Vec<Issue>> {
    let refs = builtin_refs();
    try_load_theme(source, &ThemeOptions::core(&refs))
        .map(Arc::new)
        .map_err(|issues| issues.into_iter().map(|i| Issue::new(i.path, i.message)).collect())
}

/// Parse a file's bytes: JSON errors become one issue with the position.
pub fn parse(bytes: &[u8]) -> Result<Value, Vec<Issue>> {
    serde_json::from_slice(bytes).map_err(|e| vec![Issue::new(format!("line {}:{}", e.line(), e.column()), e.to_string())])
}
