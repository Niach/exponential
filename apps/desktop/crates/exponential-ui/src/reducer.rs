//! The reducer: a surface's flat component list (A2UI `updateComponents`,
//! core OR basic catalog OR an extension) → ONE normalized tree in the core
//! vocabulary, macros expanded, every unknown component the explicit
//! `Unknown` placeholder. Mirrors `src/reducer.ts`; the fixtures lock it.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use indexmap::IndexMap;
use serde_json::Value;

use crate::basic_map::map_basic_component;
use crate::limits;
use crate::catalog::{CatalogView, A2UI_BASIC_CATALOG_ID, UNKNOWN_COMPONENT};
use crate::macros::expand_macros_with_issues;
use crate::types::{FlatChildren, FlatComponent, NestedNode, Props, ReduceIssue, UiNode};
use crate::validate::validate_props;

/// The keys of a flat component that are not props (round 1 adds `visible`).
pub const RESERVED_KEYS: &[&str] = &["id", "component", "children", "slots", "on", "style", "visible", "accessibility", "template"];
const RESERVED: &[&str] = RESERVED_KEYS;

/// A `visible` value is a boolean or a dynamic value (a binding or a call).
pub fn valid_visible(value: &Value) -> bool {
    value.is_boolean() || crate::expr::is_dynamic(value)
}

/// A component's slot list admits a name when it lists it or lists `*`.
pub fn slot_allowed(slots: Option<&Vec<String>>, name: &str) -> bool {
    slots.is_some_and(|s| s.iter().any(|x| x == "*" || x == name))
}

const VISIBLE_ISSUE: &str = "visible: expected a boolean, a binding or a function call";

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
    /// Round 2 (docs/round-2-contract.md §4): the nodes a data `template`
    /// renders per item, by component id, LIFTED out of the tree (a
    /// template node listed as a child too never renders in place),
    /// validated and expanded like the root, in discovery order. `None`
    /// without templates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub templates: Option<IndexMap<String, UiNode>>,
}

/// Builds a template id the trees do not hold (the flat path): the node and
/// the issues its build raised.
type BuildMissing<'a> = dyn FnMut(&str) -> (Option<UiNode>, Vec<ReduceIssue>) + 'a;

/// Lift every node a `template.component` names out of the trees (children
/// and slots, any depth, the root excepted) into the template table; ids
/// no tree holds are built with `build_missing` (the flat path: the node
/// and the issues its build raised), else reported. Nested templates (a
/// template inside a template node) are lifted too. A template that would
/// instantiate itself (its owner, an ancestor of its owner, the root, or
/// through other templates) is an issue (`template: cycle through this
/// id`) and stays in place: each round puts the cyclic ones back (which
/// may give their new ancestors edges) and lifts again, until none is
/// left. `src/reducer.ts liftTemplates`.
fn lift_templates(root: &mut UiNode, issues: &mut Vec<ReduceIssue>, build_missing: &mut BuildMissing) -> IndexMap<String, UiNode> {
    let original = root.clone();
    // Cyclic ids: kept in the tree (never stripped, never built) or, when
    // built, built again (their subtree still names templates) but dropped.
    let mut in_place: Vec<String> = Vec::new();
    let mut dropped: Vec<String> = Vec::new();
    let mut cycles: Vec<ReduceIssue> = Vec::new();
    loop {
        *root = original.clone();
        let mut pass_issues = Vec::new();
        let pass = lift_pass(root, &mut pass_issues, build_missing, &in_place);
        let lifted: Vec<&String> = pass.order.iter().filter(|id| pass.found.contains_key(*id) && !in_place.contains(id) && !dropped.contains(id)).collect();
        // Edges: a lifted template → the lifted templates its subtree's owners name.
        fn walk(n: &UiNode, lifted: &[&String], out: &mut Vec<String>) {
            if let Some(t) = &n.template {
                if lifted.iter().any(|l| **l == t.component) && !out.contains(&t.component) {
                    out.push(t.component.clone());
                }
            }
            for slot in n.slots.iter().flat_map(|s| s.values()) {
                walk(slot, lifted, out);
            }
            for c in &n.children {
                walk(c, lifted, out);
            }
        }
        let deps: HashMap<&str, Vec<String>> = lifted
            .iter()
            .map(|id| {
                let mut out = Vec::new();
                walk(&pass.found[id.as_str()], &lifted, &mut out);
                (id.as_str(), out)
            })
            .collect();
        let reaches_itself = |from: &str| {
            let mut seen: HashSet<&str> = HashSet::new();
            let mut stack: Vec<&str> = deps.get(from).map(|d| d.iter().map(String::as_str).collect()).unwrap_or_default();
            while let Some(id) = stack.pop() {
                if id == from {
                    return true;
                }
                if seen.insert(id) {
                    stack.extend(deps.get(id).into_iter().flatten().map(String::as_str));
                }
            }
            false
        };
        let cyclic: Vec<String> = lifted.iter().filter(|id| reaches_itself(id)).map(|id| id.to_string()).collect();
        if cyclic.is_empty() {
            issues.extend(pass_issues);
            if pass.root_named {
                issues.push(ReduceIssue { id: original.id.clone(), message: "template: cycle through this id".into() });
            }
            issues.extend(cycles);
            let mut found = pass.found;
            return pass.order.iter().filter(|id| lifted.contains(id)).filter_map(|id| found.shift_remove(id).map(|n| (id.clone(), n))).collect();
        }
        for id in cyclic {
            if pass.built.contains(&id) {
                dropped.push(id.clone());
            } else {
                in_place.push(id.clone());
            }
            cycles.push(ReduceIssue { id, message: "template: cycle through this id".into() });
        }
    }
}

/// One lifting pass of [`lift_templates`]: `in_place` ids are never
/// stripped nor built.
struct LiftPass {
    /// Every template node found or built, by id.
    found: IndexMap<String, UiNode>,
    /// The template ids in discovery order.
    order: Vec<String>,
    /// The ids `build_missing` built.
    built: Vec<String>,
    /// A template names the root.
    root_named: bool,
}

fn lift_pass(root: &mut UiNode, issues: &mut Vec<ReduceIssue>, build_missing: &mut BuildMissing, in_place: &[String]) -> LiftPass {
    fn collect(n: &UiNode, root_id: &str, want: &mut Vec<String>, root_named: &mut bool) {
        if let Some(t) = &n.template {
            if t.component == root_id {
                *root_named = true;
            } else if !want.contains(&t.component) {
                want.push(t.component.clone());
            }
        }
        for slot in n.slots.iter().flat_map(|s| s.values()) {
            collect(slot, root_id, want, root_named);
        }
        for c in &n.children {
            collect(c, root_id, want, root_named);
        }
    }
    fn strip(n: &mut UiNode, want: &[String], found: &mut IndexMap<String, UiNode>) {
        let children = std::mem::take(&mut n.children);
        for c in children {
            if want.contains(&c.id) {
                if !found.contains_key(&c.id) {
                    found.insert(c.id.clone(), c);
                }
            } else {
                n.children.push(c);
            }
        }
        if let Some(slots) = &mut n.slots {
            let names: Vec<String> = slots.iter().filter(|(_, v)| want.contains(&v.id)).map(|(k, _)| k.clone()).collect();
            for name in names {
                if let Some(slot) = slots.shift_remove(&name) {
                    if !found.contains_key(&slot.id) {
                        found.insert(slot.id.clone(), slot);
                    }
                }
            }
            if slots.is_empty() {
                n.slots = None;
            }
        }
        for slot in n.slots.iter_mut().flat_map(|s| s.values_mut()) {
            strip(slot, want, found);
        }
        for c in &mut n.children {
            strip(c, want, found);
        }
    }
    let root_id = root.id.clone();
    let mut want: Vec<String> = Vec::new();
    let mut found: IndexMap<String, UiNode> = IndexMap::new();
    let mut missing: Vec<String> = Vec::new();
    let mut built: Vec<String> = Vec::new();
    let mut root_named = false;
    collect(root, &root_id, &mut want, &mut root_named);
    let mut before = (usize::MAX, 0, 0);
    while before != (want.len(), found.len(), missing.len()) {
        before = (want.len(), found.len(), missing.len());
        let strippable: Vec<String> = want.iter().filter(|id| !in_place.contains(id)).cloned().collect();
        strip(root, &strippable, &mut found);
        loop {
            let n = found.len();
            for i in 0..n {
                let mut node = std::mem::replace(&mut found[i], UiNode::new("", ""));
                strip(&mut node, &strippable, &mut found);
                found[i] = node;
            }
            if found.len() == n {
                break;
            }
        }
        for id in want.clone() {
            if found.contains_key(&id) || missing.contains(&id) || in_place.contains(&id) {
                continue;
            }
            let (node, raised) = build_missing(&id);
            issues.extend(raised);
            match node {
                Some(node) => {
                    built.push(id.clone());
                    found.insert(id, node);
                }
                None => {
                    missing.push(id.clone());
                    issues.push(ReduceIssue { id, message: "template: no component with this id".into() });
                }
            }
        }
        for node in found.values() {
            collect(node, &root_id, &mut want, &mut root_named);
        }
    }
    LiftPass { found, order: want, built, root_named }
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
/// Never inlined: its frame stays off the recursive builders (VAPP-103).
#[inline(never)]
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
    for slot in node.slots.iter().flat_map(|s| s.keys()) {
        if !slot_allowed(def.slots.as_ref(), slot) {
            issues.push(ReduceIssue { id: id.to_string(), message: format!("slots.{slot}: {} has no such slot", node.component) });
        }
    }
    for issue in crate::validate::validate_node(node) {
        issues.push(ReduceIssue { id: id.to_string(), message: format!("{}: {}", issue.path, issue.message) });
    }
}

fn expand(node: UiNode, issues: &mut Vec<ReduceIssue>, options: &ReduceOptions) -> UiNode {
    match expand_macros_with_issues(&node, &options.view) {
        Ok((expanded, expansion_issues)) => {
            issues.extend(expansion_issues);
            expanded
        }
        Err(message) => {
            issues.push(ReduceIssue { id: node.id.clone(), message });
            node
        }
    }
}

/// Expand the root, then the lifted templates (one issue list).
fn finish(root: UiNode, mut issues: Vec<ReduceIssue>, lifted: IndexMap<String, UiNode>, options: &ReduceOptions) -> ReduceResult {
    let root = if options.expand { expand(root, &mut issues, options) } else { root };
    let templates = (!lifted.is_empty()).then(|| lifted.into_iter().map(|(id, n)| (id, if options.expand { expand(n, &mut issues, options) } else { n })).collect());
    ReduceResult { root, issues, templates }
}

struct Builder<'a> {
    by_id: HashMap<&'a str, &'a FlatComponent>,
    options: &'a ReduceOptions,
    basic: bool,
    visiting: HashSet<String>,
    /// VAPP-103: every id already placed (a second place is refused).
    placed: HashSet<String>,
    budget: Budget,
    issues: Vec<ReduceIssue>,
}

/// VAPP-103: the nodes one surface may still place (`maxComponents`).
struct Budget {
    left: usize,
    reported: bool,
}

impl Budget {
    fn new() -> Self {
        Budget { left: limits::MAX_COMPONENTS, reported: false }
    }

    /// Take one node; the first refusal is an issue.
    fn spend(&mut self, id: &str, issues: &mut Vec<ReduceIssue>) -> bool {
        if self.left > 0 {
            self.left -= 1;
            return true;
        }
        if !self.reported {
            issues.push(ReduceIssue { id: id.to_string(), message: limits::components_issue() });
        }
        self.reported = true;
        false
    }
}

impl Builder<'_> {
    fn build(&mut self, id: &str, depth: usize) -> Option<UiNode> {
        crate::deep(|| self.build_node(id, depth))
    }

    fn build_node(&mut self, id: &str, depth: usize) -> Option<UiNode> {
        let catalog_id = self.options.catalog_id.as_str();
        let Some(&flat) = self.by_id.get(id) else {
            self.issues.push(ReduceIssue { id: id.to_string(), message: "no component with this id".into() });
            return Some(unknown(id, &format!("#{id}"), catalog_id));
        };
        if self.visiting.contains(id) {
            self.issues.push(ReduceIssue { id: id.to_string(), message: "cycle through this id".into() });
            return Some(unknown(id, &flat.component, catalog_id));
        }
        if self.placed.contains(id) {
            self.issues.push(ReduceIssue { id: id.to_string(), message: limits::USED_TWICE_ISSUE.into() });
            return None;
        }
        if !self.budget.spend(id, &mut self.issues) {
            return None;
        }
        self.placed.insert(id.to_string());
        if depth > limits::MAX_DEPTH {
            self.issues.push(ReduceIssue { id: id.to_string(), message: limits::depth_issue() });
            return Some(unknown(id, &flat.component, catalog_id));
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
                    node.children = mapped.children_ids.iter().filter_map(|c| self.build(c, depth + 1)).collect();
                    node.template = mapped.template;
                    node.style = mapped.style;
                    node.on = mapped.on;
                    for (slot, child_id) in &mapped.slots {
                        if let Some(child) = self.build(child_id, depth + 1) {
                            node.slots.get_or_insert_with(IndexMap::new).insert(slot.clone(), child);
                        }
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
                Some(FlatChildren::Ids(ids)) => node.children = ids.iter().filter_map(|c| self.build(c, depth + 1)).collect(),
                Some(t @ FlatChildren::Template { .. }) => node.template = t.template(),
                None => {}
            }
            node.style = flat.style.clone();
            if let Some(visible) = &flat.visible {
                if valid_visible(visible) {
                    node.visible = Some(visible.clone());
                } else {
                    self.issues.push(ReduceIssue { id: id.to_string(), message: VISIBLE_ISSUE.into() });
                }
            }
            node.on = flat.on.clone();
            for (slot, child_id) in flat.slots.iter().flatten() {
                if let Some(child) = self.build(child_id, depth + 1) {
                    node.slots.get_or_insert_with(IndexMap::new).insert(slot.clone(), child);
                }
            }
        }
        if let Some(accessibility) = flat.accessibility.as_ref().filter(|a| js_truthy(a)) {
            node.accessibility = Some(accessibility.clone());
        }
        self.visiting.remove(id);
        validate_node(&node, id, self.options, true, &mut self.issues);
        Some(node)
    }
}

/// Reduce a flat component list. Children resolve from the root down, so
/// components nothing references (a basic Button's consumed Text child) do
/// not appear. A missing or cyclic reference becomes an `Unknown`
/// placeholder plus an issue. VAPP-103: an id placed a second time (two
/// parents, or one parent twice) renders at its first place only (`id used
/// twice`), a node deeper than `maxDepth` is an `Unknown` placeholder, and
/// past `maxComponents` nodes the rest is dropped (`catalog/limits.json`).
pub fn reduce_surface(components: &[FlatComponent], options: &ReduceOptions) -> ReduceResult {
    crate::roomy(|| reduce_flat(components, options))
}

fn reduce_flat(components: &[FlatComponent], options: &ReduceOptions) -> ReduceResult {
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
    let mut builder = Builder {
        by_id,
        options,
        basic: options.catalog_id == A2UI_BASIC_CATALOG_ID,
        visiting: HashSet::new(),
        placed: HashSet::new(),
        budget: Budget::new(),
        issues,
    };
    let mut root = builder.build(&root_id, 1).unwrap_or_else(|| unknown(&root_id, &format!("#{root_id}"), &options.catalog_id));
    let mut issues = std::mem::take(&mut builder.issues);
    // A template id builds ONCE (its id is placed then); the lifting passes
    // that ask again get the same node and issues.
    let mut built: HashMap<String, (Option<UiNode>, Vec<ReduceIssue>)> = HashMap::new();
    let lifted = lift_templates(&mut root, &mut issues, &mut |id| {
        if let Some(done) = built.get(id) {
            return done.clone();
        }
        let node = if builder.by_id.contains_key(id) && !builder.placed.contains(id) { builder.build(id, 1) } else { None };
        let out = (node, std::mem::take(&mut builder.issues));
        built.insert(id.to_string(), out.clone());
        out
    });
    finish(root, issues, lifted, options)
}

/// The nested authoring form → a normalized tree (same validation and
/// expansion as [`reduce_surface`]; `root_id` is ignored).
pub fn reduce_nested(tree: &NestedNode, options: &ReduceOptions) -> ReduceResult {
    crate::roomy(|| reduce_tree(tree, options))
}

fn reduce_tree(tree: &NestedNode, options: &ReduceOptions) -> ReduceResult {
    let mut issues = Vec::new();
    let mut budget = Budget::new();
    let mut root = walk_nested(tree, options, &mut issues, &mut budget, 1).unwrap_or_else(|| unknown(&tree.id, &tree.component, &options.catalog_id));
    let lifted = lift_templates(&mut root, &mut issues, &mut |_| (None, Vec::new()));
    finish(root, issues, lifted, options)
}

fn walk_nested(n: &NestedNode, options: &ReduceOptions, issues: &mut Vec<ReduceIssue>, budget: &mut Budget, depth: usize) -> Option<UiNode> {
    crate::deep(|| walk_nested_node(n, options, issues, budget, depth))
}

fn walk_nested_node(n: &NestedNode, options: &ReduceOptions, issues: &mut Vec<ReduceIssue>, budget: &mut Budget, depth: usize) -> Option<UiNode> {
    if !budget.spend(&n.id, issues) {
        return None;
    }
    if depth > limits::MAX_DEPTH {
        issues.push(ReduceIssue { id: n.id.clone(), message: limits::depth_issue() });
        return Some(unknown(&n.id, &n.component, &options.catalog_id));
    }
    if !options.view.components.contains_key(&n.component) {
        issues.push(ReduceIssue { id: n.id.clone(), message: format!("unknown component {}", n.component) });
        return Some(unknown(&n.id, &n.component, &options.catalog_id));
    }
    let mut node = UiNode::new(n.id.clone(), n.component.clone());
    node.props = n.props.clone().unwrap_or_default();
    node.children = n.children.iter().flatten().filter_map(|c| walk_nested(c, options, issues, budget, depth + 1)).collect();
    node.style = n.style.clone();
    if let Some(visible) = &n.visible {
        if valid_visible(visible) {
            node.visible = Some(visible.clone());
        } else {
            issues.push(ReduceIssue { id: n.id.clone(), message: VISIBLE_ISSUE.into() });
        }
    }
    node.on = n.on.clone();
    node.accessibility = n.accessibility.clone().filter(js_truthy);
    node.template = n.template.clone();
    if let Some(slots) = &n.slots {
        let mut out = IndexMap::new();
        for (slot, child) in slots {
            if let Some(built) = walk_nested(child, options, issues, budget, depth + 1) {
                out.insert(slot.clone(), built);
            }
        }
        node.slots = Some(out);
    }
    validate_node(&node, &n.id, options, false, issues);
    Some(node)
}

/// Pre-order ids of a tree (slots first, then children), the painters'
/// accessibility order.
pub fn preorder(root: &UiNode) -> Vec<String> {
    let mut out = Vec::new();
    root.walk(&mut |n| out.push(n.id.clone()));
    out
}
