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
use crate::catalog::{CatalogView, SURFACE_COMPONENT, UNKNOWN_COMPONENT};
use crate::macros::expand_macros_with_templates;
use crate::types::{FlatChildren, FlatComponent, NestedNode, Props, ReduceIssue, UiNode};
use crate::validate::validate_props;

/// The keys of a flat component that are not props: round 9 §5.3, the
/// `ComponentCommon` envelope (`children`, `slots`, `on`, `style`,
/// `visible`, `checks` are declared props of the core components now).
pub const RESERVED_KEYS: &[&str] = &["id", "component", "catalogId", "accessibility", "metadata"];
/// Round 9 §5.1: the closed `accessibility` object.
pub const ACCESSIBILITY_KEYS: &[&str] = &["label", "description", "live", "hidden", "selected", "expanded"];

/// #EXP-1629: the state keys (DynamicBoolean: a boolean or a binding).
pub const ACCESSIBILITY_STATES: &[&str] = &["selected", "expanded"];
const LIVE_VALUES: &[&str] = &["off", "polite", "assertive"];
const VALIDATION_FAILED: &str = "VALIDATION_FAILED";
const UNALLOWED_CHILD: &str = "UNALLOWED_CHILD";
const UNALLOWED_PARENT: &str = "UNALLOWED_PARENT";
const UNSUPPORTED_CATALOG: &str = "UNSUPPORTED_CATALOG";

fn escape_token(t: &str) -> String {
    t.replace('~', "~0").replace('/', "~1")
}

/// A validate dotted path (`props.items[0].label`, `style.color`,
/// `on.press.functionCall`) → the component pointer (`/items/0/label`).
pub fn issue_pointer(path: &str) -> String {
    let rest = if path == "props" { "" } else { path.strip_prefix("props.").unwrap_or(path) };
    let mut out = String::new();
    for part in rest.split('.').filter(|p| !p.is_empty()) {
        let (head, tail) = part.find('[').map_or((part, ""), |i| part.split_at(i));
        // `name[0][1]` only; anything else is one token.
        let mut indexes = Vec::new();
        let mut t = tail;
        let well_formed = loop {
            if t.is_empty() {
                break true;
            }
            let Some(close) = t.find(']') else { break false };
            let digits = &t[1..close];
            if !t.starts_with('[') || digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                break false;
            }
            indexes.push(digits);
            t = &t[close + 1..];
        };
        if !well_formed {
            out.push('/');
            out.push_str(&escape_token(part));
            continue;
        }
        if !head.is_empty() {
            out.push('/');
            out.push_str(&escape_token(head));
        }
        for i in indexes {
            out.push('/');
            out.push_str(i);
        }
    }
    out
}

/// A `visible` value is a boolean or a dynamic value (a binding or a call).
pub fn valid_visible(value: &Value) -> bool {
    value.is_boolean() || crate::expr::is_dynamic(value)
}

/// A component's slot list admits a name when it lists it or lists `*`.
pub fn slot_allowed(slots: Option<&Vec<String>>, name: &str) -> bool {
    slots.is_some_and(|s| s.iter().any(|x| x == "*" || x == name))
}

const VISIBLE_ISSUE: &str = "visible: expected a boolean, a binding or a function call";

/// Round 9 §5.6: `allowedChildren: []` when the component takes no children
/// and has no slots.
pub fn allows_children(def: &crate::types::ComponentDef) -> bool {
    def.children != "none" || def.slots.as_ref().is_some_and(|s| !s.is_empty())
}

/// Round 9 §5.1: the closed accessibility object (unknown keys reported and
/// dropped).
fn accessibility_of(value: &Value, id: &str, issues: &mut Vec<ReduceIssue>) -> Option<Value> {
    let Some(o) = value.as_object() else {
        issues.push(ReduceIssue::at(id, "accessibility: expected an object", "/accessibility", VALIDATION_FAILED));
        return None;
    };
    let mut out = serde_json::Map::new();
    for (k, v) in o {
        if !ACCESSIBILITY_KEYS.contains(&k.as_str()) {
            issues.push(ReduceIssue::at(id, format!("accessibility.{k}: unknown key"), &format!("/accessibility/{}", escape_token(k)), VALIDATION_FAILED));
            continue;
        }
        if k == "live" && !v.as_str().is_some_and(|l| LIVE_VALUES.contains(&l)) {
            issues.push(ReduceIssue::at(id, format!("accessibility.live: expected one of {}", LIVE_VALUES.join("|")), "/accessibility/live", VALIDATION_FAILED));
            continue;
        }
        if ACCESSIBILITY_STATES.contains(&k.as_str()) && !v.is_boolean() && !crate::expr::is_dynamic(v) {
            issues.push(ReduceIssue::at(id, format!("accessibility.{k}: expected a boolean"), &format!("/accessibility/{k}"), VALIDATION_FAILED));
            continue;
        }
        out.insert(k.clone(), v.clone());
    }
    (!out.is_empty()).then_some(Value::Object(out))
}

#[derive(Debug, Clone)]
pub struct ReduceOptions {
    /// The surface's DEFAULT catalog (round 9 §5.5: a component's own
    /// `catalogId` wins): the core, the lite subset, the basic catalog or
    /// an extension's id; `""` = none (a component without its own catalog
    /// is the Unknown placeholder).
    pub catalog_id: String,
    /// Round 9 §5.5: the router's AVAILABLE catalog ids (`None`: core,
    /// core-lite, basic, then the view's extensions).
    pub available: Option<Vec<String>>,
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
        ReduceOptions { catalog_id: catalog_id.to_string(), available: None, view: CatalogView::core(), root_id: None, expand: true, validate: true }
    }

    pub fn with_view(self, view: Arc<CatalogView>) -> Self {
        ReduceOptions { view, ..self }
    }

    fn is_available(&self, catalog_id: &str) -> bool {
        match &self.available {
            Some(ids) => ids.iter().any(|id| id == catalog_id),
            None => self.view.knows_catalog(catalog_id),
        }
    }

    /// The Unknown placeholder's issue when `catalog` cannot be used.
    fn unavailable(&self, id: &str, catalog: &str) -> Option<ReduceIssue> {
        if catalog.is_empty() {
            return Some(ReduceIssue::at(id, "no catalog for this component", "", UNSUPPORTED_CATALOG));
        }
        (!self.is_available(catalog)).then(|| ReduceIssue::at(id, format!("catalog {catalog} is not available"), "", UNSUPPORTED_CATALOG))
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
                issues.push(ReduceIssue::new(original.id.clone(), "template: cycle through this id"));
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
            cycles.push(ReduceIssue::new(id, "template: cycle through this id"));
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
                    issues.push(ReduceIssue::new(id, "template: no component with this id"));
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
    if !catalog_id.is_empty() {
        node.props.insert("catalogId".into(), Value::String(catalog_id.to_string()));
    }
    node
}

fn own_props(flat: &FlatComponent) -> Props {
    flat.rest.iter().filter(|(k, _)| !RESERVED_KEYS.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// `metadata` is kept when it is an object (the entry check refused the rest).
fn metadata_of(metadata: Option<&Value>) -> Option<Value> {
    metadata.filter(|m| m.is_object()).cloned()
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
        issues.push(ReduceIssue::at(id, format!("{}: {}", issue.path, issue.message), &issue_pointer(&issue.path), VALIDATION_FAILED));
    }
    if !allows_children(def) && !node.children.is_empty() {
        issues.push(ReduceIssue::at(id, format!("{} takes no children", node.component), "/children", UNALLOWED_CHILD));
    }
    for slot in node.slots.iter().flat_map(|s| s.keys()) {
        if !slot_allowed(def.slots.as_ref(), slot) {
            issues.push(ReduceIssue::at(id, format!("slots.{slot}: {} has no such slot", node.component), &format!("/slots/{}", escape_token(slot)), VALIDATION_FAILED));
        }
    }
    for issue in crate::validate::validate_node_in(node, &options.view) {
        issues.push(ReduceIssue::at(id, format!("{}: {}", issue.path, issue.message), &issue_pointer(&issue.path), VALIDATION_FAILED));
    }
}

/// Round 9 §5.6: every node against its own `allowedParents` (`Surface` =
/// the root's parent). `parents` = node id → the (authored) parent component.
fn check_parents(root: &UiNode, lifted: &IndexMap<String, UiNode>, parents: &HashMap<String, String>, view: &CatalogView, issues: &mut Vec<ReduceIssue>) {
    fn walk(n: &UiNode, parents: &HashMap<String, String>, view: &CatalogView, issues: &mut Vec<ReduceIssue>) {
        if let (Some(allowed), Some(parent)) = (view.components.get(&n.component).and_then(|d| d.parents.as_ref()), parents.get(&n.id)) {
            if !allowed.contains(parent) {
                issues.push(ReduceIssue::at(n.id.clone(), format!("{} cannot be a child of {parent}; allowed parents: {}", n.component, allowed.join("|")), "", UNALLOWED_PARENT));
            }
        }
        for slot in n.slots.iter().flat_map(|s| s.values()) {
            crate::deep(|| walk(slot, parents, view, issues));
        }
        for c in &n.children {
            crate::deep(|| walk(c, parents, view, issues));
        }
    }
    walk(root, parents, view, issues);
    for t in lifted.values() {
        walk(t, parents, view, issues);
    }
}

fn expand(node: UiNode, issues: &mut Vec<ReduceIssue>, built: &mut Vec<UiNode>, options: &ReduceOptions) -> UiNode {
    match expand_macros_with_templates(&node, &options.view) {
        Ok((expanded, expansion_issues, templates)) => {
            issues.extend(expansion_issues);
            built.extend(templates);
            expanded
        }
        Err(message) => {
            issues.push(ReduceIssue::new(node.id.clone(), message));
            node
        }
    }
}

/// Expand the root, then the lifted templates (one issue list); then the
/// depth cap on the EXPANDED trees (VAPP-103 rfix: a macro adds levels, so
/// `maxDepth` bounds the reduced tree a host serializes, not the authored
/// one).
fn finish(root: UiNode, mut issues: Vec<ReduceIssue>, lifted: IndexMap<String, UiNode>, options: &ReduceOptions) -> ReduceResult {
    // eApps U3: the items a macro's `$template` parts built join the lifted
    // templates (after them, in build order: the root's first).
    let mut built: Vec<UiNode> = Vec::new();
    let mut root = if options.expand { expand(root, &mut issues, &mut built, options) } else { root };
    cap_depth(&mut root, 1, &mut issues, &options.catalog_id);
    // R8 F51: ONE budget over the expanded trees (the root's, then every
    // lifted template's): a macro's `$each` parts count.
    let mut budget = Budget { left: limits::MAX_COMPONENTS, reported: issues.iter().any(|i| i.message == limits::components_issue()) };
    budget.spend(&root.id, &mut issues);
    cap_components(&mut root, &mut budget, &mut issues);
    let mut templates: IndexMap<String, UiNode> = lifted
        .into_iter()
        .map(|(id, n)| {
            let mut n = if options.expand { expand(n, &mut issues, &mut built, options) } else { n };
            cap_depth(&mut n, 1, &mut issues, &options.catalog_id);
            budget.spend(&n.id, &mut issues);
            cap_components(&mut n, &mut budget, &mut issues);
            (id, n)
        })
        .collect();
    for mut n in built {
        if templates.contains_key(&n.id) {
            continue;
        }
        cap_depth(&mut n, 1, &mut issues, &options.catalog_id);
        budget.spend(&n.id, &mut issues);
        cap_components(&mut n, &mut budget, &mut issues);
        templates.insert(n.id.clone(), n);
    }
    let templates = (!templates.is_empty()).then_some(templates);
    ReduceResult { root, issues, templates }
}

/// R8 F51: the `maxComponents` budget over an EXPANDED tree (the flat
/// builder counted the authored nodes; a macro's `$each` multiplies them:
/// a Rating with `max: 10000` is 20,001 parts). Pre-order; a node past the
/// budget is dropped with its subtree (children, then slots), the first
/// refusal is the budget issue (once per surface).
fn cap_components(node: &mut UiNode, budget: &mut Budget, issues: &mut Vec<ReduceIssue>) {
    crate::deep(|| {
        node.children.retain_mut(|child| {
            if !budget.spend(&child.id, issues) {
                return false;
            }
            cap_components(child, budget, issues);
            true
        });
        if let Some(slots) = &mut node.slots {
            slots.retain(|_, child| {
                if !budget.spend(&child.id, issues) {
                    return false;
                }
                cap_components(child, budget, issues);
                true
            });
        }
    })
}

/// A node at level `maxDepth + 1` (the root = 1; children and slots one
/// level down) becomes the Unknown placeholder for its id and component,
/// its subtree dropped, with the depth issue (once per id).
fn cap_depth(node: &mut UiNode, depth: usize, issues: &mut Vec<ReduceIssue>, catalog_id: &str) {
    if depth > limits::MAX_DEPTH {
        if node.component != UNKNOWN_COMPONENT || !node.children.is_empty() || node.slots.is_some() {
            let issue = ReduceIssue::new(node.id.clone(), limits::depth_issue());
            if !issues.contains(&issue) {
                issues.push(issue);
            }
            *node = unknown(&node.id, &node.component, catalog_id);
        }
        return;
    }
    crate::deep(|| {
        for child in &mut node.children {
            cap_depth(child, depth + 1, issues, catalog_id);
        }
        for child in node.slots.iter_mut().flat_map(|s| s.values_mut()) {
            cap_depth(child, depth + 1, issues, catalog_id);
        }
    })
}

struct Builder<'a> {
    by_id: HashMap<&'a str, &'a FlatComponent>,
    options: &'a ReduceOptions,
    /// Round 9 §5.6: node id → the parent it was built under.
    parents: HashMap<String, String>,
    visiting: HashSet<String>,
    /// VAPP-103: every id already placed (a second place is refused).
    placed: HashSet<String>,
    /// R8 F52: the ids whose second place was already reported (a 4 MB
    /// `children: ["x", "x", …]` costs ONE issue, not one per reference).
    twice_reported: HashSet<String>,
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
            issues.push(ReduceIssue::new(id.to_string(), limits::components_issue()));
        }
        self.reported = true;
        false
    }
}

impl Builder<'_> {
    fn build(&mut self, id: &str, depth: usize) -> Option<UiNode> {
        crate::deep(|| self.build_node(id, depth))
    }

    /// R8 F52: the Unknown placeholders for a missing or cyclic id are
    /// placed nodes too, so they spend the budget BEFORE they are made: a
    /// message whose children list repeats an unknown id a million times
    /// stops at `maxComponents` placeholders (one budget issue), and a
    /// second place of an id is reported once per id.
    fn build_node(&mut self, id: &str, depth: usize) -> Option<UiNode> {
        let default_catalog = self.options.catalog_id.as_str();
        let flat = self.by_id.get(id).copied();
        if let Some(flat) = flat {
            // A cycle (the id is an ancestor) before a second place: the
            // ancestor is in `placed` too.
            if self.visiting.contains(id) {
                if !self.budget.spend(id, &mut self.issues) {
                    return None;
                }
                self.issues.push(ReduceIssue::new(id, "cycle through this id"));
                return Some(unknown(id, &flat.component, flat.catalog_id.as_deref().unwrap_or(default_catalog)));
            }
            if self.placed.contains(id) {
                if self.twice_reported.insert(id.to_string()) {
                    self.issues.push(ReduceIssue::new(id, limits::USED_TWICE_ISSUE));
                }
                return None;
            }
        }
        if !self.budget.spend(id, &mut self.issues) {
            return None;
        }
        let Some(flat) = flat else {
            self.issues.push(ReduceIssue::new(id, "no component with this id"));
            return Some(unknown(id, &format!("#{id}"), default_catalog));
        };
        self.placed.insert(id.to_string());
        // Round 9 §5.5: the component's catalog = its own `catalogId`, else
        // the surface default; a name is looked up ONLY there.
        let catalog = flat.catalog_id.as_deref().unwrap_or(default_catalog);
        if depth > limits::MAX_DEPTH {
            self.issues.push(ReduceIssue::new(id, limits::depth_issue()));
            return Some(unknown(id, &flat.component, catalog));
        }
        self.visiting.insert(id.to_string());
        let mut node;
        if let Some(issue) = self.options.unavailable(id, catalog) {
            self.issues.push(issue);
            node = unknown(id, &flat.component, catalog);
        } else if crate::functions::is_basic_catalog(catalog) {
            let by_id = &self.by_id;
            let lookup = |id: &str| by_id.get(id).map(|f| (*f).clone());
            let catalog_of = |f: &FlatComponent| f.catalog_id.clone().unwrap_or_else(|| default_catalog.to_string());
            match map_basic_component(flat, &lookup, &catalog_of) {
                None => {
                    self.issues.push(ReduceIssue::new(id, format!("basic component {} has no mapping", flat.component)));
                    node = unknown(id, &flat.component, catalog);
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
                    for (prop, message) in mapped.issues {
                        self.issues.push(ReduceIssue::at(id, message, &format!("/{}", escape_token(&prop)), VALIDATION_FAILED));
                    }
                }
            }
        } else if self.options.view.catalog_component(catalog, &flat.component).is_none() {
            self.issues.push(ReduceIssue::at(id, format!("unknown component {}", flat.component), "/component", VALIDATION_FAILED));
            node = unknown(id, &flat.component, catalog);
        } else {
            node = UiNode::new(id, flat.component.clone());
            node.props = own_props(flat);
            match &flat.children {
                Some(FlatChildren::Ids(ids)) => node.children = ids.iter().filter_map(|c| self.build(c, depth + 1)).collect(),
                Some(t @ FlatChildren::Template { .. }) => node.template = t.template(),
                None => {}
            }
            node.style = flat.style.clone();
            node.on = flat.on.clone();
            for (slot, child_id) in flat.slots.iter().flatten() {
                if let Some(child) = self.build(child_id, depth + 1) {
                    node.slots.get_or_insert_with(IndexMap::new).insert(slot.clone(), child);
                }
            }
        }
        for c in node.children.iter().chain(node.slots.iter().flat_map(|s| s.values())) {
            self.parents.insert(c.id.clone(), flat.component.clone());
        }
        // `visible`, `accessibility` and `metadata` hold on EVERY component, basic or core.
        if let Some(visible) = &flat.visible {
            if valid_visible(visible) {
                node.visible = Some(visible.clone());
            } else {
                self.issues.push(ReduceIssue::at(id, VISIBLE_ISSUE, "/visible", VALIDATION_FAILED));
            }
        }
        if let Some(accessibility) = &flat.accessibility {
            node.accessibility = accessibility_of(accessibility, id, &mut self.issues);
        }
        node.metadata = metadata_of(flat.metadata.as_ref());
        self.visiting.remove(id);
        validate_node(&node, id, self.options, true, &mut self.issues);
        Some(node)
    }
}

/// VAPP-103 rfix: the issues ONE component raises on its own (an unknown
/// component or basic mapping, its props, `visible`, children it may not
/// take, slots it lacks): what a streamed `updateComponents` reports at
/// once while the tree reduces lazily. Tree issues (missing ids, cycles,
/// depth, limits) come with the next reduce. `lookup` = the surface's
/// components (a basic component consumes its children).
pub fn component_issues(flat: &FlatComponent, options: &ReduceOptions, lookup: &dyn Fn(&str) -> Option<FlatComponent>) -> Vec<ReduceIssue> {
    let mut issues = Vec::new();
    let id = flat.id.as_str();
    let stub = |cid: &str| UiNode::new(cid, UNKNOWN_COMPONENT);
    let mut node;
    let catalog = flat.catalog_id.as_deref().unwrap_or(&options.catalog_id);
    if let Some(issue) = options.unavailable(id, catalog) {
        issues.push(issue);
        return issues;
    }
    if crate::functions::is_basic_catalog(catalog) {
        let catalog_of = |f: &FlatComponent| f.catalog_id.clone().unwrap_or_else(|| options.catalog_id.clone());
        match map_basic_component(flat, lookup, &catalog_of) {
            None => {
                issues.push(ReduceIssue::new(id, format!("basic component {} has no mapping", flat.component)));
                return issues;
            }
            Some(mapped) => {
                node = UiNode::new(id, mapped.component);
                node.props = mapped.props;
                node.children = mapped.children_ids.iter().map(|c| stub(c)).collect();
                node.template = mapped.template;
                node.style = mapped.style;
                node.on = mapped.on;
                for (slot, child_id) in &mapped.slots {
                    node.slots.get_or_insert_with(IndexMap::new).insert(slot.clone(), stub(child_id));
                }
                for (prop, message) in mapped.issues {
                    issues.push(ReduceIssue::at(id, message, &format!("/{}", escape_token(&prop)), VALIDATION_FAILED));
                }
            }
        }
    } else if options.view.catalog_component(catalog, &flat.component).is_none() {
        issues.push(ReduceIssue::at(id, format!("unknown component {}", flat.component), "/component", VALIDATION_FAILED));
        return issues;
    } else {
        node = UiNode::new(id, flat.component.clone());
        node.props = own_props(flat);
        match &flat.children {
            Some(FlatChildren::Ids(ids)) => node.children = ids.iter().map(|c| stub(c)).collect(),
            Some(t @ FlatChildren::Template { .. }) => node.template = t.template(),
            None => {}
        }
        node.style = flat.style.clone();
        node.on = flat.on.clone();
        for (slot, child_id) in flat.slots.iter().flatten() {
            node.slots.get_or_insert_with(IndexMap::new).insert(slot.clone(), stub(child_id));
        }
    }
    if let Some(visible) = &flat.visible {
        if valid_visible(visible) {
            node.visible = Some(visible.clone());
        } else {
            issues.push(ReduceIssue::at(id, VISIBLE_ISSUE, "/visible", VALIDATION_FAILED));
        }
    }
    if let Some(accessibility) = &flat.accessibility {
        node.accessibility = accessibility_of(accessibility, id, &mut issues);
    }
    validate_node(&node, id, options, true, &mut issues);
    issues
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
            issues.push(ReduceIssue::new(flat.id.clone(), "duplicate id; the last definition wins"));
        }
        by_id.insert(flat.id.as_str(), flat);
    }
    let root_id = options.root_id.clone().unwrap_or_else(|| "root".to_string());
    let mut builder = Builder {
        by_id,
        options,
        parents: HashMap::new(),
        visiting: HashSet::new(),
        placed: HashSet::new(),
        twice_reported: HashSet::new(),
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
    if options.validate {
        builder.parents.insert(root.id.clone(), SURFACE_COMPONENT.to_string());
        check_parents(&root, &lifted, &builder.parents, &options.view, &mut issues);
    }
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
    let mut parents = HashMap::new();
    let mut root = walk_nested(tree, options, &mut issues, &mut budget, &mut parents, 1).unwrap_or_else(|| unknown(&tree.id, &tree.component, &options.catalog_id));
    let lifted = lift_templates(&mut root, &mut issues, &mut |_| (None, Vec::new()));
    if options.validate {
        parents.insert(root.id.clone(), SURFACE_COMPONENT.to_string());
        check_parents(&root, &lifted, &parents, &options.view, &mut issues);
    }
    finish(root, issues, lifted, options)
}

fn walk_nested(n: &NestedNode, options: &ReduceOptions, issues: &mut Vec<ReduceIssue>, budget: &mut Budget, parents: &mut HashMap<String, String>, depth: usize) -> Option<UiNode> {
    crate::deep(|| walk_nested_node(n, options, issues, budget, parents, depth))
}

fn walk_nested_node(n: &NestedNode, options: &ReduceOptions, issues: &mut Vec<ReduceIssue>, budget: &mut Budget, parents: &mut HashMap<String, String>, depth: usize) -> Option<UiNode> {
    if !budget.spend(&n.id, issues) {
        return None;
    }
    let catalog = n.catalog_id.as_deref().unwrap_or(&options.catalog_id);
    if depth > limits::MAX_DEPTH {
        issues.push(ReduceIssue::new(n.id.clone(), limits::depth_issue()));
        return Some(unknown(&n.id, &n.component, catalog));
    }
    if let Some(issue) = options.unavailable(&n.id, catalog) {
        issues.push(issue);
        return Some(unknown(&n.id, &n.component, catalog));
    }
    if options.view.catalog_component(catalog, &n.component).is_none() {
        issues.push(ReduceIssue::at(n.id.clone(), format!("unknown component {}", n.component), "/component", VALIDATION_FAILED));
        return Some(unknown(&n.id, &n.component, catalog));
    }
    let mut node = UiNode::new(n.id.clone(), n.component.clone());
    node.props = n.props.clone().unwrap_or_default();
    node.children = n.children.iter().flatten().filter_map(|c| walk_nested(c, options, issues, budget, parents, depth + 1)).collect();
    for c in &node.children {
        parents.insert(c.id.clone(), n.component.clone());
    }
    node.style = n.style.clone();
    if let Some(visible) = &n.visible {
        if valid_visible(visible) {
            node.visible = Some(visible.clone());
        } else {
            issues.push(ReduceIssue::at(n.id.clone(), VISIBLE_ISSUE, "/visible", VALIDATION_FAILED));
        }
    }
    node.on = n.on.clone();
    if let Some(accessibility) = &n.accessibility {
        node.accessibility = accessibility_of(accessibility, &n.id, issues);
    }
    node.metadata = metadata_of(n.metadata.as_ref());
    node.template = n.template.clone();
    if let Some(slots) = &n.slots {
        let mut out = IndexMap::new();
        for (slot, child) in slots {
            if let Some(built) = walk_nested(child, options, issues, budget, parents, depth + 1) {
                parents.insert(built.id.clone(), n.component.clone());
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

/// Round 9 §13: what a card refuses beyond [`reduce_nested`]'s issues: the
/// entry check on every node (§5.2) and any name no card can reach (no agent
/// channel, §6.5). Each `<node id>: <reason>`.
pub fn card_problems(root: &Value) -> Vec<String> {
    fn known(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::Array(items) => items.iter().for_each(|i| known(i, out)),
            Value::Object(o) => {
                if let Some(call) = o.get("@call").and_then(Value::as_str) {
                    if !crate::validate::is_known_function_name(call) {
                        out.push(format!("unknown function {call}"));
                    }
                }
                o.values().for_each(|i| known(i, out));
            }
            _ => {}
        }
    }
    fn walk(n: &Value, out: &mut Vec<String>) {
        let Some(o) = n.as_object() else { return };
        let mut entry: serde_json::Map<String, Value> = o.iter().filter(|(k, _)| !matches!(k.as_str(), "props" | "children" | "slots" | "template")).map(|(k, v)| (k.clone(), v.clone())).collect();
        if let Some(props) = o.get("props").and_then(Value::as_object) {
            entry.extend(props.clone());
        }
        if let Some((at, message)) = crate::host::check_entry(&Value::Object(entry), Some(crate::catalog::CORE_CATALOG_ID), &CatalogView::core()) {
            let id = o.get("id").map(crate::json::to_js_string).unwrap_or_else(|| "undefined".into());
            out.push(format!("{id}: {} {message}", if at.is_empty() { "/" } else { at.as_str() }));
        }
        for k in ["props", "on", "visible", "accessibility"] {
            if let Some(v) = o.get(k) {
                known(v, out);
            }
        }
        for c in o.get("children").and_then(Value::as_array).into_iter().flatten() {
            walk(c, out);
        }
        for c in o.get("slots").and_then(Value::as_object).into_iter().flatten() {
            walk(c.1, out);
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

/// Round 9 §13 / §1.5: a card surface (`{version, root, data}`) at 1.0:
/// a card without `version` is the 0.9 form, upgraded while `accept_v09`
/// (`upgradeCard09`) and refused after; any other version is refused.
pub fn card_v10(card: &Value, accept_v09: bool) -> Result<Value, String> {
    let Some(o) = card.as_object() else { return Err("a card is a JSON object".into()) };
    match o.get("version") {
        None if accept_v09 => Ok(crate::upgrade09::upgrade_card09(card)),
        None => Err("version is required: write \"version\": \"1.0\"".into()),
        Some(Value::String(v)) if v == crate::catalog::A2UI_VERSION => Ok(card.clone()),
        Some(Value::String(v)) => Err(format!("unsupported version {v}")),
        Some(other) => Err(format!("unsupported version {}", serde_json::to_string(other).unwrap_or_default())),
    }
}
