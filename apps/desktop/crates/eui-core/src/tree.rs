//! #VAPP-285 (packages/exponential-ui/docs/round-7-contract.md): the Tree's
//! two pure rules, the TS reference's `src/tree.ts`; both replay
//! `fixtures/tree.json`.
//!
//! [`tree_rows`]: nested items → the flat rows the `Tree` macro draws (one
//! Row each, depth → the tree guides), depth-first. A node with `children`
//! (an array, even empty) or `hasChildren: true` is a BRANCH; one with
//! `hasChildren` and no `children` array is LAZY (it folds through `load`,
//! and while open a LOADING row sits under it). A literal `expanded` decides
//! now (only the shown rows); a bound one emits every row with `shown` /
//! `open` / `next` / `toggled` as `expr` calls over it.
//!
//! [`tree_key`]: the WAI-ARIA tree keys over the VISIBLE rows (level, and
//! `expanded` on a branch): Up/Down, Home/End, the open key (ArrowRight,
//! ArrowLeft in rtl) opens a closed branch or enters an open one, the close
//! key closes an open branch or goes to the parent.

use serde_json::{json, Map, Value};

use crate::expr::{is_dynamic, MAX_RANGE_ITEMS};

fn expr(text: String, vars: Map<String, Value>, fallback: Value) -> Value {
    json!({"call": "expr", "args": {"expr": text, "vars": Value::Object(vars), "default": fallback}})
}

/// `e.exists(x, x == a0) && …` over the ids (a bound `expanded` = `e`).
fn all_open(expanded: &Value, ids: &[String]) -> Value {
    if ids.is_empty() {
        return Value::Bool(true);
    }
    let mut vars = Map::new();
    vars.insert("e".into(), expanded.clone());
    for (i, id) in ids.iter().enumerate() {
        vars.insert(format!("a{i}"), Value::String(id.clone()));
    }
    let text = (0..ids.len()).map(|i| format!("e.exists(x, x == a{i})")).collect::<Vec<_>>().join(" && ");
    expr(text, vars, Value::Bool(false))
}

fn routing(row: &mut Map<String, Value>, branch: bool, lazy: bool) {
    let fold = if branch {
        if lazy {
            "load"
        } else {
            "toggle"
        }
    } else {
        ""
    };
    row.insert("pressEvent".into(), Value::String(if branch { fold } else { "select" }.into()));
    row.insert("fold".into(), Value::String(fold.into()));
    row.insert("selectOn".into(), Value::String(if branch { "" } else { "press" }.into()));
    row.insert("foldOn".into(), Value::String(if branch { "press" } else { "" }.into()));
    row.insert("toggleOn".into(), Value::String(if branch { "toggle" } else { "" }.into()));
    row.insert("pressable".into(), Value::Bool(true));
}

struct Walk<'a> {
    expanded: &'a Value,
    bound: bool,
    open: Vec<String>,
    rows: Vec<Value>,
}

impl Walk<'_> {
    fn walk(&mut self, list: Option<&Value>, depth: usize, ancestors: &[String]) {
        let Some(Value::Array(list)) = list else { return };
        for raw in list {
            if self.rows.len() >= MAX_RANGE_ITEMS {
                return;
            }
            let Value::Object(item) = raw else { continue };
            let Some(id) = item.get("id").and_then(Value::as_str).filter(|id| !id.is_empty()) else { continue };
            let id = id.to_string();
            let has_array = matches!(item.get("children"), Some(Value::Array(_)));
            let branch = has_array || item.get("hasChildren") == Some(&Value::Bool(true));
            let lazy = item.get("hasChildren") == Some(&Value::Bool(true)) && !has_array;
            let mut row = Map::new();
            row.insert("id".into(), Value::String(id.clone()));
            row.insert("title".into(), item.get("title").filter(|t| t.is_string()).cloned().unwrap_or_else(|| Value::String(id.clone())));
            row.insert("depth".into(), json!(depth));
            row.insert("level".into(), json!(depth + 1));
            row.insert("kind".into(), Value::String(if branch { "branch" } else { "leaf" }.into()));
            row.insert("shown".into(), if self.bound { all_open(self.expanded, ancestors) } else { Value::Bool(true) });
            routing(&mut row, branch, lazy);
            if let Some(icon) = item.get("icon") {
                row.insert("icon".into(), icon.clone());
            }
            if let Some(meta) = item.get("meta").and_then(Value::as_str).filter(|m| !m.is_empty()) {
                row.insert("meta".into(), Value::String(meta.into()));
            }
            let mut is_open = Value::Bool(false);
            if branch {
                if self.bound {
                    let mut vars = Map::new();
                    vars.insert("e".into(), self.expanded.clone());
                    vars.insert("i".into(), Value::String(id.clone()));
                    is_open = expr("e.exists(x, x == i)".into(), vars.clone(), Value::Bool(false));
                    row.insert("open".into(), is_open.clone());
                    row.insert("next".into(), json!({"call": "not", "args": {"value": is_open.clone()}}));
                    row.insert("toggled".into(), expr("e.exists(x, x == i) ? e.filter(x, x != i) : e + [i]".into(), vars, json!([id.clone()])));
                } else {
                    let open = self.open.contains(&id);
                    is_open = Value::Bool(open);
                    row.insert("open".into(), Value::Bool(open));
                    row.insert("next".into(), Value::Bool(!open));
                }
            }
            // The default glyphs: a folder (open while open), a file.
            if !row.contains_key("icon") {
                let icon = if !branch {
                    Value::String("ui-file".into())
                } else if self.bound {
                    json!({"call": "cond", "args": {"if": is_open.clone(), "then": "ui-folder-open", "else": "ui-folder"}})
                } else if is_open == Value::Bool(true) {
                    Value::String("ui-folder-open".into())
                } else {
                    Value::String("ui-folder".into())
                };
                row.insert("icon".into(), icon);
            }
            self.rows.push(Value::Object(row));
            if !branch || (!self.bound && is_open != Value::Bool(true)) {
                continue;
            }
            let mut inner = ancestors.to_vec();
            inner.push(id.clone());
            if lazy {
                if self.rows.len() >= MAX_RANGE_ITEMS {
                    return;
                }
                let shown = if self.bound { all_open(self.expanded, &inner) } else { Value::Bool(true) };
                self.rows.push(json!({
                    "id": format!("{id}/loading"), "title": "$string.loading", "depth": depth + 1, "level": depth + 2, "kind": "loading", "shown": shown,
                    "pressEvent": "", "fold": "", "selectOn": "", "foldOn": "", "toggleOn": "", "pressable": false
                }));
                continue;
            }
            self.walk(item.get("children"), depth + 1, &inner);
        }
    }
}

/// The flat rows of nested `items` under `expanded` (module docs).
pub fn tree_rows(items: Option<&Value>, expanded: Option<&Value>) -> Vec<Value> {
    let null = Value::Null;
    let expanded = expanded.unwrap_or(&null);
    let open = match expanded {
        Value::Array(list) => list.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        _ => Vec::new(),
    };
    let mut walk = Walk { expanded, bound: is_dynamic(expanded), open, rows: Vec::new() };
    walk.walk(items, 0, &[]);
    walk.rows
}

/// One visible row as [`tree_key`] reads it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TreeKeyRow {
    pub level: u32,
    /// `Some` on a branch, `None` on a leaf (and the loading row).
    pub expanded: Option<bool>,
}

/// What a key does on a row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TreeKeyResult {
    Focus(usize),
    Toggle,
}

impl TreeKeyResult {
    /// The fixture's JSON: `{focus: i}` or `{toggle: true}`.
    pub fn to_json(self) -> Value {
        match self {
            TreeKeyResult::Focus(i) => json!({"focus": i}),
            TreeKeyResult::Toggle => json!({"toggle": true}),
        }
    }
}

/// The WAI-ARIA tree keys (module docs). `key` = a DOM key name.
pub fn tree_key(rows: &[TreeKeyRow], index: usize, key: &str, rtl: bool) -> Option<TreeKeyResult> {
    let row = rows.get(index)?;
    let (open_key, close_key) = if rtl { ("ArrowLeft", "ArrowRight") } else { ("ArrowRight", "ArrowLeft") };
    let last = rows.len() - 1;
    match key {
        "ArrowDown" => (index < last).then_some(TreeKeyResult::Focus(index + 1)),
        "ArrowUp" => (index > 0).then(|| TreeKeyResult::Focus(index - 1)),
        "Home" => (index > 0).then_some(TreeKeyResult::Focus(0)),
        "End" => (index < last).then_some(TreeKeyResult::Focus(last)),
        k if k == open_key => match row.expanded {
            Some(false) => Some(TreeKeyResult::Toggle),
            Some(true) => rows.get(index + 1).filter(|next| next.level == row.level + 1).map(|_| TreeKeyResult::Focus(index + 1)),
            None => None,
        },
        k if k == close_key => {
            if row.expanded == Some(true) {
                return Some(TreeKeyResult::Toggle);
            }
            (0..index).rev().find(|&i| rows[i].level < row.level).filter(|&i| rows[i].level + 1 == row.level).map(TreeKeyResult::Focus)
        }
        _ => None,
    }
}
