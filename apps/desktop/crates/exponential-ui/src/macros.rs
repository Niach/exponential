//! The macro expander: applies `catalog/macros.json` (plus extension macros)
//! to a normalized tree until every node is native. Mirrors `src/macros.ts`;
//! `fixtures/catalog-macros.json` locks the output.
//!
//! Round 1 (docs/round-1-contract.md §1–2): a `$if`/`$any` that cannot be
//! decided at expansion time (a bound input) emits the part WITH `visible` =
//! the bound condition; `"$slot:name"` children splice an author slot in
//! place and a template `slots` entry may be a template node; a RESPONSIVE
//! prop (`{base, sm?, md?…}`) is expanded per breakpoint into `@media
//! (min-width: $breakpoint.<bp>)` blocks on every part whose style it
//! changes; a `$set` on a part whose macro prop is BOUND adds the `set`
//! write-back to that part's action (two-way binding without a host round
//! trip); a part's `$a11y` becomes the node's `accessibility` (role, states,
//! name: §6).

use std::collections::HashSet;
use std::sync::LazyLock;

use indexmap::IndexMap;
use serde_json::{Map, Value};

use crate::catalog::{CatalogView, TOKEN_GROUPS};
use crate::expr::{eval_condition_value, eval_value, is_binding, Decided, ExprContext};
use crate::json;
use crate::types::{MacroDef, MacroTemplate, Props, Recipe, ReduceIssue, TemplateChild, UiNode};

const MAX_DEPTH: usize = 16;

/// The breakpoint names in ascending order (`tokens.json` `breakpoint`).
pub static BREAKPOINTS: LazyLock<Vec<String>> = LazyLock::new(|| TOKEN_GROUPS.get("breakpoint").cloned().unwrap_or_default());

struct Variant {
    breakpoint: String,
    props: Props,
}

struct Expansion<'a> {
    view: &'a CatalogView,
    macro_: &'a str,
    def: &'a MacroDef,
    source: &'a UiNode,
    /// The author's props with every responsive value at its `base`.
    base_props: Props,
    /// One per breakpoint a responsive prop names, ascending.
    variants: Vec<Variant>,
    claimed: HashSet<String>,
    /// Expansion problems (an author function that replaces a `$set` write).
    issues: Vec<ReduceIssue>,
}

/// `{base, sm?, md?, lg?, xl?}`: an object with `base` and only breakpoint
/// keys besides.
pub fn is_responsive_value(value: &Value) -> bool {
    let Some(obj) = value.as_object() else { return false };
    obj.contains_key("base") && obj.keys().all(|k| k == "base" || BREAKPOINTS.iter().any(|b| b == k))
}

/// The value a responsive prop takes at a breakpoint: the nearest defined
/// key at or below it, `base` under the first one. `None` = the base.
pub fn responsive_at<'v>(value: &'v Value, breakpoint: Option<&str>) -> Option<&'v Value> {
    let obj = value.as_object()?;
    let mut out = obj.get("base");
    let Some(breakpoint) = breakpoint else { return out };
    for bp in BREAKPOINTS.iter() {
        if let Some(v) = obj.get(bp) {
            out = Some(v);
        }
        if bp == breakpoint {
            break;
        }
    }
    out
}

/// The author's props with every responsive object replaced by its value at
/// `breakpoint` (`None` = base).
pub fn props_at(props: &Props, breakpoint: Option<&str>) -> Props {
    let mut out = Props::new();
    for (k, v) in props {
        if is_responsive_value(v) {
            if let Some(at) = responsive_at(v, breakpoint) {
                out.insert(k.clone(), at.clone());
            }
        } else {
            out.insert(k.clone(), v.clone());
        }
    }
    out
}

fn responsive_variants(props: &Props) -> Vec<Variant> {
    let mut used: HashSet<&str> = HashSet::new();
    for v in props.values() {
        if is_responsive_value(v) {
            for k in v.as_object().into_iter().flat_map(|o| o.keys()) {
                if k != "base" {
                    used.insert(k.as_str());
                }
            }
        }
    }
    BREAKPOINTS
        .iter()
        .filter(|bp| used.contains(bp.as_str()))
        .map(|bp| Variant { breakpoint: bp.clone(), props: props_at(props, Some(bp)) })
        .collect()
}

/// JS `{ ...value }` for the value an object spread meets: objects copy,
/// strings spread their characters by index, everything else is `{}`.
fn spread(value: Option<&Value>) -> Map<String, Value> {
    match value {
        Some(Value::Object(map)) => map.clone(),
        Some(Value::String(s)) => s.chars().enumerate().map(|(i, c)| (i.to_string(), Value::String(c.to_string()))).collect(),
        _ => Map::new(),
    }
}

/// Merge `over` into `base`; condition objects (`@media …`, `:pressed`) merge
/// key by key.
fn merge_style(base: Option<&Props>, over: Option<&Props>) -> Option<Props> {
    if base.is_none() && over.is_none() {
        return None;
    }
    let mut out = base.cloned().unwrap_or_default();
    for (k, v) in over.into_iter().flatten() {
        if let Value::Object(nested) = v {
            let mut merged = spread(out.get(k));
            for (nk, nv) in nested {
                merged.insert(nk.clone(), nv.clone());
            }
            out.insert(k.clone(), Value::Object(merged));
        } else {
            out.insert(k.clone(), v.clone());
        }
    }
    Some(out)
}

/// A deep copy of `action` with `context` merged into `event.context`.
fn routed_action(action: &Value, context: Option<&Value>) -> Value {
    let mut out = action.clone();
    if let (Some(context), Some(Value::Object(event))) = (context.filter(|c| js_truthy(c)), out.get_mut("event")) {
        let mut merged = spread(event.get("context").filter(|c| !c.is_null()));
        merged.extend(spread(Some(context)));
        event.insert("context".into(), Value::Object(merged));
    }
    out
}

/// The macro's recipe discriminators: the author's value, else the prop's
/// catalog default, then the evaluated `$recipe` keys.
fn recipe_props(exp: &Expansion, ctx: &ExprContext, extra: Option<&Props>) -> Props {
    let mut props = Props::new();
    let schema = exp.view.components.get(exp.macro_).map(|d| &d.props);
    for name in &exp.def.recipe_props {
        let value = exp
            .base_props
            .get(name)
            .filter(|v| !v.is_null())
            .cloned()
            .or_else(|| schema.and_then(|s| s.get(name)).and_then(|p| p.default.clone()));
        if let Some(value) = value {
            props.insert(name.clone(), value);
        }
    }
    if let Some(extra) = extra {
        if let Some(Value::Object(evaluated)) = eval_value(&Value::Object(extra.clone()), ctx) {
            for (k, v) in evaluated {
                props.insert(k, v);
            }
        }
    }
    props
}

/// `and{values}` / `or{values}` over several conditions; one alone is itself.
fn combine(mut conditions: Vec<Value>, op: &str) -> Value {
    if conditions.len() == 1 {
        return conditions.pop().expect("one");
    }
    crate::expr::call(op, vec![("values", Some(Value::Array(conditions)))])
}

/// The part's `$if`/`$any`: `Bool(false)` = leave it out, `Bool(true)` =
/// emit it, a dynamic value = emit it with that as `visible`.
fn decide_conditions(tpl: &MacroTemplate, ctx: &ExprContext) -> Decided {
    let mut bound: Vec<Value> = Vec::new();
    if let Some(cond) = &tpl.if_ {
        let conds: Vec<&Value> = match cond {
            Value::Array(list) => list.iter().collect(),
            single => vec![single],
        };
        for c in conds {
            match eval_condition_value(Some(c), ctx) {
                Decided::Bool(false) => return Decided::Bool(false),
                Decided::Bool(true) => {}
                Decided::Dynamic(v) => bound.push(v),
            }
        }
    }
    if let Some(any) = &tpl.any {
        let mut decided = false;
        let mut dynamic: Vec<Value> = Vec::new();
        for c in any {
            match eval_condition_value(Some(c), ctx) {
                Decided::Bool(true) => {
                    decided = true;
                    break;
                }
                Decided::Bool(false) => {}
                Decided::Dynamic(v) => dynamic.push(v),
            }
        }
        if !decided {
            if dynamic.is_empty() {
                return Decided::Bool(false);
            }
            bound.push(combine(dynamic, "or"));
        }
    }
    if bound.is_empty() {
        Decided::Bool(true)
    } else {
        Decided::Dynamic(combine(bound, "and"))
    }
}

fn expand_template(tpl: &MacroTemplate, exp: &mut Expansion, ctx: &ExprContext, parent_id: &str) -> Vec<UiNode> {
    let visible = match decide_conditions(tpl, ctx) {
        Decided::Bool(false) => return Vec::new(),
        Decided::Bool(true) => None,
        Decided::Dynamic(v) => Some(v),
    };
    if let Some(each) = &tpl.each {
        let Some(Value::Array(items)) = eval_value(&Value::String(format!("{{{each}}}")), ctx) else {
            return Vec::new();
        };
        let as_ = tpl.as_.as_deref().unwrap_or("item");
        return items
            .into_iter()
            .enumerate()
            .map(|(index, item)| {
                let mut vars = ctx.vars.clone();
                vars.insert(as_.to_string(), item);
                vars.insert("index".to_string(), json::number(index as f64));
                let item_ctx = ExprContext { id: ctx.id, props: ctx.props, vars };
                build_node(tpl, exp, &item_ctx, format!("{parent_id}.{}.{index}", tpl.part), visible.clone())
            })
            .collect();
    }
    let id = if tpl.part == "root" { exp.source.id.clone() } else { format!("{parent_id}.{}", tpl.part) };
    vec![build_node(tpl, exp, ctx, id, visible)]
}

/// The part's style: evaluated with the base props, then once per
/// breakpoint variant; every key a variant changes lands in its media block.
fn part_style(tpl: &MacroTemplate, exp: &Expansion, ctx: &ExprContext) -> Option<Props> {
    let template = tpl.style.as_ref()?;
    let Some(Value::Object(mut style)) = eval_value(&Value::Object(template.clone()), ctx) else { return None };
    let mut previous = style.clone();
    for variant in &exp.variants {
        let variant_ctx = ExprContext { id: ctx.id, props: &variant.props, vars: ctx.vars.clone() };
        let Some(Value::Object(at)) = eval_value(&Value::Object(template.clone()), &variant_ctx) else { continue };
        let mut diff = Map::new();
        for (k, v) in &at {
            if k.starts_with('@') || k.starts_with(':') {
                continue;
            }
            if !previous.get(k).is_some_and(|p| json::equal(p, v)) {
                diff.insert(k.clone(), v.clone());
            }
        }
        if !diff.is_empty() {
            let key = format!("@media (min-width: $breakpoint.{})", variant.breakpoint);
            let mut block = spread(style.get(&key));
            block.extend(diff);
            style.insert(key, Value::Object(block));
        }
        previous = at;
    }
    (!style.is_empty()).then_some(style)
}

fn build_node(tpl: &MacroTemplate, exp: &mut Expansion, ctx: &ExprContext, id: String, visible: Option<Value>) -> UiNode {
    let props = match eval_value(&Value::Object(tpl.props.clone().unwrap_or_default()), ctx) {
        Some(Value::Object(p)) => p,
        _ => Props::new(),
    };
    let mut node = UiNode::new(id.clone(), tpl.component.clone());
    node.props = props;
    node.style = part_style(tpl, exp, ctx);
    node.visible = visible;
    node.recipe = Some(Recipe {
        macro_: exp.macro_.to_string(),
        part: tpl.part.clone(),
        props: recipe_props(exp, ctx, tpl.recipe.as_ref()),
    });
    if let Some(routes) = &tpl.on {
        let mut on = IndexMap::new();
        for (part_event, macro_event) in routes {
            let Some(action) = exp.source.on.as_ref().and_then(|o| o.get(macro_event)).filter(|a| js_truthy(a)) else {
                continue;
            };
            exp.claimed.insert(macro_event.clone());
            let context = tpl.context.as_ref().and_then(|c| eval_value(&Value::Object(c.clone()), ctx));
            on.insert(part_event.clone(), routed_action(action, context.as_ref()));
        }
        if !on.is_empty() {
            node.on = Some(on);
        }
    }
    if let Some(sets) = &tpl.set {
        for (part_event, spec) in sets {
            let Some(bound) = exp.base_props.get(&spec.prop).filter(|b| is_binding(b)) else { continue };
            let has_function = node.on.as_ref().and_then(|on| on.get(part_event)).and_then(|a| a.get("functionCall").filter(|f| js_truthy(f)).or_else(|| a.get("function"))).is_some_and(js_truthy);
            if has_function {
                // An Action carries ONE function (`functionCall`, or the legacy
                // `function` key): the author's routed function
                // call wins and the bound prop is not written back (reported,
                // never lost silently). Route an `event` to keep the write.
                let macro_event = tpl.on.as_ref().and_then(|o| o.get(part_event)).unwrap_or(part_event);
                let message = format!(
                    "on.{macro_event}: a function action replaces the two-way set of props.{}; write it in that function's handler or use an event",
                    spec.prop
                );
                if !exp.issues.iter().any(|i| i.id == exp.source.id && i.message == message) {
                    exp.issues.push(ReduceIssue { id: exp.source.id.clone(), message });
                }
                continue;
            }
            let path = bound["path"].clone();
            let value = eval_value(&spec.value, ctx);
            let mut args = Map::new();
            args.insert("path".into(), path);
            if let Some(value) = value {
                args.insert("value".into(), value);
            }
            let mut function = Map::new();
            function.insert("call".into(), Value::String("set".into()));
            function.insert("args".into(), Value::Object(args));
            let on = node.on.get_or_insert_with(IndexMap::new);
            let mut action = match on.get(part_event) {
                Some(Value::Object(existing)) => existing.clone(),
                _ => Map::new(),
            };
            action.insert("function".into(), Value::Object(function));
            on.insert(part_event.clone(), Value::Object(action));
        }
    }
    if let Some(a11y) = &tpl.a11y {
        if let Some(Value::Object(evaluated)) = eval_value(&Value::Object(a11y.clone()), ctx) {
            if !evaluated.is_empty() {
                node.accessibility = Some(Value::Object(evaluated));
            }
        }
    }
    if let Some(slots) = &tpl.slots {
        for (slot, reference) in slots {
            match reference {
                TemplateChild::Splice(reference) => {
                    let Some(name) = reference.strip_prefix("$slot:").filter(|n| !n.is_empty()) else { continue };
                    if let Some(value) = exp.source.slots.as_ref().and_then(|s| s.get(name)) {
                        node.slots.get_or_insert_with(IndexMap::new).insert(slot.clone(), value.clone());
                    }
                }
                TemplateChild::Node(part) => {
                    if let Some(built) = expand_template(part, exp, ctx, &id).into_iter().next() {
                        node.slots.get_or_insert_with(IndexMap::new).insert(slot.clone(), built);
                    }
                }
            }
        }
    }
    for child in tpl.children.iter().flatten() {
        match child {
            TemplateChild::Splice(s) if s == "$children" => {
                node.children.extend(exp.source.children.iter().cloned());
                if let Some(template) = &exp.source.template {
                    node.template = Some(template.clone());
                }
            }
            TemplateChild::Splice(s) => {
                if let Some(name) = s.strip_prefix("$slot:") {
                    if let Some(value) = exp.source.slots.as_ref().and_then(|slots| slots.get(name)) {
                        node.children.push(value.clone());
                    }
                }
            }
            TemplateChild::Node(child) => {
                let built = expand_template(child, exp, ctx, &id);
                node.children.extend(built);
            }
        }
    }
    node
}

/// Expand one macro node (children already native) into its template.
fn expand_macro(node: UiNode, macro_: &str, def: &MacroDef, view: &CatalogView, depth: usize, issues: &mut Vec<ReduceIssue>) -> Result<UiNode, String> {
    if depth > MAX_DEPTH {
        return Err(format!("macro {macro_}: expansion deeper than {MAX_DEPTH}"));
    }
    let mut base_props = props_at(&node.props, None);
    // Round 3: an `on.press` implies `pressable` on a macro that offers it (Row, Chip).
    if node.on.as_ref().is_some_and(|o| o.contains_key("press")) && !base_props.contains_key("pressable") && view.components.get(macro_).is_some_and(|d| d.props.contains_key("pressable")) {
        base_props.insert("pressable".into(), Value::Bool(true));
    }
    let variants = responsive_variants(&node.props);
    let mut exp = Expansion { view, macro_, def, source: &node, base_props: base_props.clone(), variants, claimed: HashSet::new(), issues: Vec::new() };
    let ctx = ExprContext { id: &node.id, props: &base_props, vars: IndexMap::new() };
    let mut root = expand_template(&def.root, &mut exp, &ctx, &node.id)
        .into_iter()
        .next()
        .ok_or_else(|| format!("macro {macro_}: the root template produced no node"))?;
    let claimed = exp.claimed;
    for issue in exp.issues {
        if !issues.contains(&issue) {
            issues.push(issue);
        }
    }
    root.style = merge_style(root.style.as_ref(), node.style.as_ref()).filter(|s| !s.is_empty());
    if let Some(visible) = &node.visible {
        root.visible = Some(visible.clone());
    }
    // The author's label/description (or an outer macro part's `$a11y`) win
    // over the root part's own.
    if let Some(Value::Object(accessibility)) = &node.accessibility {
        let mut merged = spread(root.accessibility.as_ref());
        for (k, v) in accessibility {
            merged.insert(k.clone(), v.clone());
        }
        root.accessibility = Some(Value::Object(merged));
    }
    let mut remaining = root.on.take().unwrap_or_default();
    for (event, action) in node.on.iter().flatten() {
        if !claimed.contains(event) {
            remaining.insert(event.clone(), action.clone());
        }
    }
    root.on = (!remaining.is_empty()).then_some(remaining);
    expand_tree(&root, view, depth + 1, issues)
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

fn expand_tree(node: &UiNode, view: &CatalogView, depth: usize, issues: &mut Vec<ReduceIssue>) -> Result<UiNode, String> {
    let mut out = node.clone();
    out.children = node.children.iter().map(|c| expand_tree(c, view, depth, issues)).collect::<Result<_, _>>()?;
    if let Some(slots) = &node.slots {
        let mut expanded = IndexMap::new();
        for (slot, child) in slots {
            expanded.insert(slot.clone(), expand_tree(child, view, depth, issues)?);
        }
        out.slots = Some(expanded);
    }
    if let Some(def) = view.macros.get(&out.component) {
        if view.components.get(&out.component).is_some_and(|d| d.is_macro()) {
            let component = out.component.clone();
            return expand_macro(out, &component, def, view, depth, issues);
        }
    }
    Ok(out)
}

/// The normalized tree with every macro (core + extensions) expanded into
/// natives, recipes attached. Pure: the input is not mutated. `Err` when an
/// expansion nests deeper than 16 macros. Expansion problems are dropped; see
/// [`expand_macros_with_issues`].
pub fn expand_macros(root: &UiNode, view: &CatalogView) -> Result<UiNode, String> {
    expand_macros_with_issues(root, view).map(|(node, _)| node)
}

/// [`expand_macros`] plus the expansion problems (an author function that
/// replaces a `$set` write-back), deduplicated per node: the TS
/// `expandMacros(root, {issues})`.
pub fn expand_macros_with_issues(root: &UiNode, view: &CatalogView) -> Result<(UiNode, Vec<ReduceIssue>), String> {
    let mut issues = Vec::new();
    let mut node = expand_tree(root, view, 0, &mut issues)?;
    // Round 3: the tree guides of nested Rows come from their siblings, so
    // they are filled AFTER the whole tree is native (`tree_guides`).
    crate::tree_guides::apply_tree_guides(&mut node);
    Ok((node, issues))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn responsive_values_pick_the_nearest_breakpoint_at_or_below() {
        let v = json!({"base": 1, "md": 2, "xl": 4});
        assert!(is_responsive_value(&v));
        assert!(!is_responsive_value(&json!({"base": 1, "huge": 2})));
        assert!(!is_responsive_value(&json!({"md": 2})));
        assert_eq!(responsive_at(&v, None), Some(&json!(1)));
        assert_eq!(responsive_at(&v, Some("sm")), Some(&json!(1)));
        assert_eq!(responsive_at(&v, Some("md")), Some(&json!(2)));
        assert_eq!(responsive_at(&v, Some("lg")), Some(&json!(2)));
        assert_eq!(responsive_at(&v, Some("xl")), Some(&json!(4)));
        assert_eq!(BREAKPOINTS.as_slice(), ["sm", "md", "lg", "xl"]);
    }
}
