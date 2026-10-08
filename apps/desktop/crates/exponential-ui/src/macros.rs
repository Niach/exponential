//! The macro expander: applies `catalog/macros.json` (plus extension macros)
//! to a normalized tree until every node is native. Mirrors `src/macros.ts`;
//! `fixtures/catalog-macros.json` locks the output.

use std::collections::HashSet;

use indexmap::IndexMap;
use serde_json::{Map, Value};

use crate::catalog::CatalogView;
use crate::expr::{eval_condition, eval_value, ExprContext};
use crate::types::{MacroDef, MacroTemplate, Props, Recipe, TemplateChild, UiNode};

const MAX_DEPTH: usize = 16;

struct Expansion<'a> {
    view: &'a CatalogView,
    macro_: &'a str,
    def: &'a MacroDef,
    source: &'a UiNode,
    claimed: HashSet<String>,
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
            .source
            .props
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

fn expand_template(tpl: &MacroTemplate, exp: &mut Expansion, ctx: &ExprContext, parent_id: &str) -> Vec<UiNode> {
    if let Some(cond) = &tpl.if_ {
        let all = match cond {
            Value::Array(conds) => conds.iter().all(|c| eval_condition(c, ctx)),
            single => eval_condition(single, ctx),
        };
        if !all {
            return Vec::new();
        }
    }
    if let Some(any) = &tpl.any {
        if !any.iter().any(|c| eval_condition(c, ctx)) {
            return Vec::new();
        }
    }
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
                vars.insert("index".to_string(), crate::json::number(index as f64));
                let item_ctx = ExprContext { id: ctx.id, props: ctx.props, vars };
                build_node(tpl, exp, &item_ctx, format!("{parent_id}.{}.{index}", tpl.part))
            })
            .collect();
    }
    let id = if tpl.part == "root" { exp.source.id.clone() } else { format!("{parent_id}.{}", tpl.part) };
    vec![build_node(tpl, exp, ctx, id)]
}

fn build_node(tpl: &MacroTemplate, exp: &mut Expansion, ctx: &ExprContext, id: String) -> UiNode {
    let props = match eval_value(&Value::Object(tpl.props.clone().unwrap_or_default()), ctx) {
        Some(Value::Object(p)) => p,
        _ => Props::new(),
    };
    let mut node = UiNode::new(id.clone(), tpl.component.clone());
    node.props = props;
    if let Some(style) = &tpl.style {
        if let Some(Value::Object(style)) = eval_value(&Value::Object(style.clone()), ctx) {
            if !style.is_empty() {
                node.style = Some(style);
            }
        }
    }
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
    if let Some(slots) = &tpl.slots {
        for (slot, reference) in slots {
            let Some(name) = reference.strip_prefix("$slot:").filter(|n| !n.is_empty()) else { continue };
            if let Some(value) = exp.source.slots.as_ref().and_then(|s| s.get(name)) {
                node.slots.get_or_insert_with(IndexMap::new).insert(slot.clone(), value.clone());
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
            TemplateChild::Splice(_) => {}
            TemplateChild::Node(child) => {
                let built = expand_template(child, exp, ctx, &id);
                node.children.extend(built);
            }
        }
    }
    node
}

/// Expand one macro node (children already native) into its template.
fn expand_macro(node: UiNode, macro_: &str, def: &MacroDef, view: &CatalogView, depth: usize) -> Result<UiNode, String> {
    if depth > MAX_DEPTH {
        return Err(format!("macro {macro_}: expansion deeper than {MAX_DEPTH}"));
    }
    let mut exp = Expansion { view, macro_, def, source: &node, claimed: HashSet::new() };
    let ctx = ExprContext { id: &node.id, props: &node.props, vars: IndexMap::new() };
    let mut root = expand_template(&def.root, &mut exp, &ctx, &node.id)
        .into_iter()
        .next()
        .ok_or_else(|| format!("macro {macro_}: the root template produced no node"))?;
    let claimed = exp.claimed;
    root.style = merge_style(root.style.as_ref(), node.style.as_ref()).filter(|s| !s.is_empty());
    if let Some(accessibility) = node.accessibility.as_ref().filter(|a| js_truthy(a)) {
        root.accessibility = Some(accessibility.clone());
    }
    let mut remaining = root.on.take().unwrap_or_default();
    for (event, action) in node.on.iter().flatten() {
        if !claimed.contains(event) {
            remaining.insert(event.clone(), action.clone());
        }
    }
    root.on = (!remaining.is_empty()).then_some(remaining);
    expand_tree(&root, view, depth + 1)
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

fn expand_tree(node: &UiNode, view: &CatalogView, depth: usize) -> Result<UiNode, String> {
    let mut out = node.clone();
    out.children = node.children.iter().map(|c| expand_tree(c, view, depth)).collect::<Result<_, _>>()?;
    if let Some(slots) = &node.slots {
        let mut expanded = IndexMap::new();
        for (slot, child) in slots {
            expanded.insert(slot.clone(), expand_tree(child, view, depth)?);
        }
        out.slots = Some(expanded);
    }
    if let Some(def) = view.macros.get(&out.component) {
        if view.components.get(&out.component).is_some_and(|d| d.is_macro()) {
            let component = out.component.clone();
            return expand_macro(out, &component, def, view, depth);
        }
    }
    Ok(out)
}

/// The normalized tree with every macro (core + extensions) expanded into
/// natives, recipes attached. Pure: the input is not mutated. `Err` when an
/// expansion nests deeper than 16 macros.
pub fn expand_macros(root: &UiNode, view: &CatalogView) -> Result<UiNode, String> {
    expand_tree(root, view, 0)
}
