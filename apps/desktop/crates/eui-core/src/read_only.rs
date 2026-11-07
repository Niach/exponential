//! VAPP-200 (spec apps.md §9.2): a READ-ONLY surface (`src/read-only.ts`).
//! A host whose viewer may read but not write (an app at app access
//! `read`) sets [`crate::surface::SurfaceSettings::read_only`]; the build
//! then gives every WRITING control `disabled: true`, as if its author had
//! written it (the components' own disabled look, nothing new). Pure;
//! `fixtures/read-only.json` locks it.
//!
//! A writing control: a node with an `on.<event>` action calling a write
//! (`records.*`, `actions.*`; a Form whose submit writes disables its
//! fields and buttons through the Form's cascade), or a form field with no
//! action of its own. A node whose own actions are all pure or host calls
//! (a `bind`, a `set`) is a view control and stays live.

use serde_json::Value;

use crate::types::UiNode;

/// The components that edit a value ([`crate::layout_tree::FORM_FIELDS`]).
pub const READ_ONLY_FIELDS: &[&str] = crate::layout_tree::FORM_FIELDS;

/// The call prefixes that write at app access `read`.
pub const READ_ONLY_WRITE_PREFIXES: &[&str] = &["records_", "actions_"];

/// Does this function name write (`records.*`, `actions.*`)?
pub fn call_writes(name: &str) -> bool {
    READ_ONLY_WRITE_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// Does this action (`{functionCall: {call}}`) call a write?
pub fn action_writes(action: &Value) -> bool {
    action.get("functionCall").and_then(|f| f.get("@call")).and_then(Value::as_str).is_some_and(call_writes)
}

/// Is `node` a writing control of a read-only surface?
pub fn disables(component: &str, on: Option<&indexmap::IndexMap<String, Value>>) -> bool {
    match on {
        Some(on) if !on.is_empty() => on.values().any(action_writes),
        _ => READ_ONLY_FIELDS.contains(&component),
    }
}

fn already_disabled(node: &UiNode) -> bool {
    node.props.get("disabled") == Some(&Value::Bool(true))
}

/// `node` with every writing control disabled (`props.disabled: true`), at
/// any depth (slots and children). A host runs it over the root AND each
/// lifted template; the surface does the same per node while it builds.
pub fn read_only_tree(node: &UiNode) -> UiNode {
    let mut out = node.clone();
    apply(&mut out);
    out
}

fn apply(node: &mut UiNode) {
    if disables(&node.component, node.on.as_ref()) && !already_disabled(node) {
        node.props.insert("disabled".into(), Value::Bool(true));
    }
    if let Some(slots) = node.slots.as_mut() {
        slots.values_mut().for_each(apply);
    }
    node.children.iter_mut().for_each(apply);
}

/// The ids a read-only surface disables, pre-order (slots before children):
/// the conformance check.
pub fn disabled_ids(node: &UiNode) -> Vec<String> {
    let mut out = Vec::new();
    collect(node, &mut out);
    out
}

fn collect(node: &UiNode, out: &mut Vec<String>) {
    if disables(&node.component, node.on.as_ref()) && !already_disabled(node) {
        out.push(node.id.clone());
    }
    for s in node.slots.iter().flat_map(|s| s.values()) {
        collect(s, out);
    }
    for c in &node.children {
        collect(c, out);
    }
}
