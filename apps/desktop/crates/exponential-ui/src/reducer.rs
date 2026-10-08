//! The reducer: a surface's flat component list (A2UI `updateComponents`,
//! core OR basic catalog OR an extension) → ONE normalized tree in the core
//! vocabulary, macros expanded, every unknown component the explicit
//! `Unknown` placeholder. Mirrors `src/reducer.ts`; the fixtures lock it.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use indexmap::IndexMap;
use serde_json::Value;

use crate::basic_map::map_basic_component;
use crate::catalog::{CatalogView, A2UI_BASIC_CATALOG_ID, UNKNOWN_COMPONENT};
use crate::macros::expand_macros;
use crate::types::{FlatChildren, FlatComponent, NestedNode, Props, ReduceIssue, Template, UiNode};
use crate::validate::validate_props;

const RESERVED: &[&str] = &["id", "component", "children", "slots", "on", "style", "accessibility", "template"];

#[derive(Debug, Clone)]
pub struct ReduceOptions {
    /// The surface's catalog: the core, the lite subset, the basic catalog or an extension's id.
    pub catalog_id: String,
    /// The core plus the registered extensions.
    pub view: Arc<CatalogView>,
    /// The root component id (A2UI: `root`).
    pub root_id: Option<String>,
    /// Expand macros (default true); false keeps the pre-expansion tree.
    pub expand: bool,
    /// Record prop validation issues (default true).
    pub validate: bool,
}

impl ReduceOptions {
    pub fn new(catalog_id: &str) -> Self {
        ReduceOptions { catalog_id: catalog_id.to_string(), view: CatalogView::core(), root_id: None, expand: true, validate: true }
    }

    pub fn with_view(self, view: Arc<CatalogView>) -> Self {
        ReduceOptions { view, ..self }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ReduceResult {
    pub root: UiNode,
    pub issues: Vec<ReduceIssue>,
}

fn unknown(id: &str, component: &str, catalog_id: &str) -> UiNode {
    let mut node = UiNode::new(id, UNKNOWN_COMPONENT);
    node.props.insert("component".into(), Value::String(component.to_string()));
    node.props.insert("catalogId".into(), Value::String(catalog_id.to_string()));
    node
}

fn own_props(flat: &FlatComponent) -> Props {
    flat.rest.iter().filter(|(k, _)| !RESERVED.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// JavaScript truthiness (`if (value)`).
fn js_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

/// `skip_unknown`: the flat path never validates a placeholder; the nested
/// path validates whatever the author wrote (an authored `Unknown` too).
fn validate_node(node: &UiNode, id: &str, options: &ReduceOptions, skip_unknown: bool, issues: &mut Vec<ReduceIssue>) {
    if !options.validate || (skip_unknown && node.component == UNKNOWN_COMPONENT) {
        return;
    }
    let Some(def) = options.view.components.get(&node.component) else { return };
    for issue in validate_props(def, &node.props, "props", &options.view) {
        issues.push(ReduceIssue { id: id.to_string(), message: format!("{}: {}", issue.path, issue.message) });
    }
    if def.children == "none" && !node.children.is_empty() {
        issues.push(ReduceIssue { id: id.to_string(), message: format!("{} takes no children", node.component) });
    }
}

fn finish(root: UiNode, mut issues: Vec<ReduceIssue>, options: &ReduceOptions) -> ReduceResult {
    if !options.expand {
        return ReduceResult { root, issues };
    }
    match expand_macros(&root, &options.view) {
        Ok(expanded) => ReduceResult { root: expanded, issues },
        Err(message) => {
            issues.push(ReduceIssue { id: root.id.clone(), message });
            ReduceResult { root, issues }
        }
    }
}

struct Builder<'a> {
    by_id: HashMap<&'a str, &'a FlatComponent>,
    options: &'a ReduceOptions,
    basic: bool,
    visiting: HashSet<String>,
    issues: Vec<ReduceIssue>,
}

impl Builder<'_> {
    fn build(&mut self, id: &str) -> UiNode {
        let catalog_id = self.options.catalog_id.as_str();
        let Some(&flat) = self.by_id.get(id) else {
            self.issues.push(ReduceIssue { id: id.to_string(), message: "no component with this id".into() });
            return unknown(id, &format!("#{id}"), catalog_id);
        };
        if self.visiting.contains(id) {
            self.issues.push(ReduceIssue { id: id.to_string(), message: "cycle through this id".into() });
            return unknown(id, &flat.component, catalog_id);
        }
        self.visiting.insert(id.to_string());
        let mut node;
        if self.basic {
            let by_id = &self.by_id;
            let lookup = |id: &str| by_id.get(id).map(|f| (*f).clone());
            match map_basic_component(flat, &lookup) {
                None => {
                    self.issues.push(ReduceIssue { id: id.to_string(), message: format!("basic component {} has no mapping", flat.component) });
                    node = unknown(id, &flat.component, catalog_id);
                }
                Some(mapped) => {
                    node = UiNode::new(id, mapped.component);
                    node.props = mapped.props;
                    node.children = mapped.children_ids.iter().map(|c| self.build(c)).collect();
                    node.template = mapped.template;
                    node.style = mapped.style;
                    node.on = mapped.on;
                    for (slot, child_id) in &mapped.slots {
                        let child = self.build(child_id);
                        node.slots.get_or_insert_with(IndexMap::new).insert(slot.clone(), child);
                    }
                }
            }
        } else if !self.options.view.components.contains_key(&flat.component) {
            self.issues.push(ReduceIssue { id: id.to_string(), message: format!("unknown component {}", flat.component) });
            node = unknown(id, &flat.component, catalog_id);
        } else {
            node = UiNode::new(id, flat.component.clone());
            node.props = own_props(flat);
            match &flat.children {
                Some(FlatChildren::Ids(ids)) => node.children = ids.iter().map(|c| self.build(c)).collect(),
                Some(FlatChildren::Template { component_id, path }) => {
                    node.template = Some(Template { component: component_id.clone(), path: path.clone() })
                }
                None => {}
            }
            node.style = flat.style.clone();
            node.on = flat.on.clone();
            for (slot, child_id) in flat.slots.iter().flatten() {
                let child = self.build(child_id);
                node.slots.get_or_insert_with(IndexMap::new).insert(slot.clone(), child);
            }
        }
        if let Some(accessibility) = flat.accessibility.as_ref().filter(|a| js_truthy(a)) {
            node.accessibility = Some(accessibility.clone());
        }
        self.visiting.remove(id);
        validate_node(&node, id, self.options, true, &mut self.issues);
        node
    }
}

/// Reduce a flat component list. Children resolve from the root down, so
/// components nothing references (a basic Button's consumed Text child) do
/// not appear. A missing or cyclic reference becomes an `Unknown`
/// placeholder plus an issue.
pub fn reduce_surface(components: &[FlatComponent], options: &ReduceOptions) -> ReduceResult {
    let mut issues = Vec::new();
    let mut by_id: HashMap<&str, &FlatComponent> = HashMap::new();
    for flat in components {
        if by_id.contains_key(flat.id.as_str()) {
            issues.push(ReduceIssue { id: flat.id.clone(), message: "duplicate id; the last definition wins".into() });
        }
        by_id.insert(flat.id.as_str(), flat);
    }
    let root_id = options.root_id.clone().unwrap_or_else(|| "root".to_string());
    if !options.view.knows_catalog(&options.catalog_id) {
        issues.push(ReduceIssue { id: root_id.clone(), message: format!("unsupported catalog {}", options.catalog_id) });
    }
    let mut builder = Builder { by_id, options, basic: options.catalog_id == A2UI_BASIC_CATALOG_ID, visiting: HashSet::new(), issues };
    let root = builder.build(&root_id);
    finish(root, builder.issues, options)
}

/// The nested authoring form → a normalized tree (same validation and
/// expansion as [`reduce_surface`]; `root_id` is ignored).
pub fn reduce_nested(tree: &NestedNode, options: &ReduceOptions) -> ReduceResult {
    let mut issues = Vec::new();
    let root = walk_nested(tree, options, &mut issues);
    finish(root, issues, options)
}

fn walk_nested(n: &NestedNode, options: &ReduceOptions, issues: &mut Vec<ReduceIssue>) -> UiNode {
    if !options.view.components.contains_key(&n.component) {
        issues.push(ReduceIssue { id: n.id.clone(), message: format!("unknown component {}", n.component) });
        return unknown(&n.id, &n.component, &options.catalog_id);
    }
    let mut node = UiNode::new(n.id.clone(), n.component.clone());
    node.props = n.props.clone().unwrap_or_default();
    node.children = n.children.iter().flatten().map(|c| walk_nested(c, options, issues)).collect();
    node.style = n.style.clone();
    node.on = n.on.clone();
    node.accessibility = n.accessibility.clone().filter(js_truthy);
    node.template = n.template.clone();
    if let Some(slots) = &n.slots {
        let mut out = IndexMap::new();
        for (slot, child) in slots {
            out.insert(slot.clone(), walk_nested(child, options, issues));
        }
        node.slots = Some(out);
    }
    validate_node(&node, &n.id, options, false, issues);
    node
}

/// Pre-order ids of a tree (slots first, then children), the painters'
/// accessibility order.
pub fn preorder(root: &UiNode) -> Vec<String> {
    let mut out = Vec::new();
    root.walk(&mut |n| out.push(n.id.clone()));
    out
}
