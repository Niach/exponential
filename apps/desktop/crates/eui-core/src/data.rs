//! The surface DATA MODEL and A2UI's dynamic values: a prop may be a `{path}`
//! binding (a JSON Pointer into the model, relative inside a template item)
//! or a `{call, args}` client function; `resolve_value` turns a prop tree
//! into literals for one pass. Round 1 (docs/round-1-contract.md §1): the
//! BIND pass of `src/dynamic.ts` — [`bind_tree`] (props resolved along their
//! schema with DATA props verbatim, styles, recipe props and `accessibility`
//! resolved at any depth, falsy `visible` dropped, `$string.<id>` through the
//! surface's table, row-scoped slots left for [`bind_row_slot`]) and
//! [`run_action`] (evaluate, then `set`, then the event);
//! `fixtures/bind-time.json` locks both. The function table
//! = the core functions (`expr::core_function`) + the basic catalog's
//! `and`/`or`/`not`/`required` with the core truthiness (0 is TRUE) + the
//! basic validators and formatters.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::expr::{coerce_boolean, truthy};
use crate::format::Formatter;
use crate::json;
use crate::limits;
use crate::strings::{resolve_string, StringTable};
use crate::catalog::CatalogView;
use crate::types::{ComponentDef, DefSchema, PropSchema, UiNode};

pub use crate::expr::{is_binding, is_call, is_dynamic};

fn unescape(token: &str) -> String {
    token.replace("~1", "/").replace("~0", "~")
}

/// A pointer's tokens; `` and `/` are the whole model.
pub fn pointer_tokens(pointer: &str) -> Vec<String> {
    if pointer.is_empty() || pointer == "/" {
        return Vec::new();
    }
    pointer.strip_prefix('/').unwrap_or(pointer).split('/').map(unescape).collect()
}

/// `path` made absolute against a template scope (`/items/3`): `/a/b`
/// stays, `b/c` joins the scope, `` (empty) is the scope itself.
pub fn absolute_path(path: &str, scope: &str) -> String {
    if path.starts_with('/') {
        return path.to_string();
    }
    if path.is_empty() {
        return scope.to_string();
    }
    format!("{scope}/{path}")
}

pub fn get_pointer<'a>(data: &'a Value, pointer: &str) -> Option<&'a Value> {
    if pointer.is_empty() || pointer == "/" {
        return Some(data);
    }
    // VAPP-132: [`pointer_tokens`] without its allocations (a read per
    // bound item); a token is unescaped only when it holds a `~`.
    let mut cur = data;
    for raw in pointer.strip_prefix('/').unwrap_or(pointer).split('/') {
        let token: std::borrow::Cow<str> = if raw.contains('~') { unescape(raw).into() } else { raw.into() };
        let token = token.as_ref();
        cur = match cur {
            Value::Array(items) => items.get(read_index(token, items.len())?)?,
            Value::Object(map) => map.get(token)?,
            _ => return None,
        };
    }
    Some(cur)
}

/// The element a token READS in an array of `len` items: digits only (the
/// tokens [`array_index`] writes; never `+1`), and `-` = the LAST element,
/// the one an append (`-`) just landed on: a landed write reads back
/// (EXP-1385, `fixtures/pointer.json`; RFC 6901 leaves `-` to the application).
fn read_index(token: &str, len: usize) -> Option<usize> {
    if token == "-" {
        return len.checked_sub(1);
    }
    if token.is_empty() || !token.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    token.parse::<usize>().ok()
}

/// A pointer's tokens for a WRITE (`` and `/` = the whole model), or the
/// limit it breaks (`maxPointerBytes`, `maxPointerSegments`).
pub fn write_tokens(pointer: &str) -> Result<Vec<String>, String> {
    if pointer.len() > limits::MAX_POINTER_BYTES {
        return Err(format!("data: pointer longer than {} bytes", limits::MAX_POINTER_BYTES));
    }
    if pointer.is_empty() || pointer == "/" {
        return Ok(Vec::new());
    }
    let body = pointer.strip_prefix('/').unwrap_or(pointer);
    if body.split('/').count() > limits::MAX_POINTER_SEGMENTS {
        return Err(format!("data: pointer has more than {} segments", limits::MAX_POINTER_SEGMENTS));
    }
    Ok(body.split('/').map(unescape).collect())
}

/// The index a token names in an array of `len` items: digits up to `len`
/// (`len` and `-` append), else the refusal (JSON Pointer: never a gap,
/// never a key). `src/dynamic.ts arrayIndex`.
fn array_index(token: &str, len: usize) -> Result<usize, String> {
    if token == "-" {
        return Ok(len);
    }
    if token.is_empty() || !token.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("data: {} is not an array index", Value::String(token.to_string())));
    }
    match token.parse::<usize>() {
        Ok(i) if i <= len => Ok(i),
        _ => Err(format!("data: index {token} is past the end of the array ({len} items)")),
    }
}

/// Set (or remove with `None`) the value at `pointer`, creating OBJECTS on
/// the way (the TS `writePointer`); an array takes an index up to its
/// length (`length` and `-` append). VAPP-103: refused (data unchanged):
/// a pointer past `maxPointerBytes` / `maxPointerSegments`, a non-index
/// token or an index past the end of an array. Iterative: any depth.
pub fn set_pointer(data: &mut Value, pointer: &str, value: Option<Value>) -> Result<(), String> {
    let tokens = write_tokens(pointer)?;
    let Some((last, path)) = tokens.split_last() else {
        *data = value.unwrap_or_else(|| Value::Object(Map::new()));
        return Ok(());
    };
    // Check first: a refusal may only come from an EXISTING array, and the
    // walk below creates containers (fresh ones are objects).
    let mut probe = Some(&*data);
    for token in &tokens {
        match probe {
            Some(Value::Array(items)) => {
                let i = array_index(token, items.len())?;
                probe = items.get(i);
            }
            Some(Value::Object(map)) => probe = map.get(token),
            _ => break,
        }
    }
    let mut cur = data;
    for token in path {
        if !(cur.is_object() || cur.is_array()) {
            *cur = Value::Object(Map::new());
        }
        cur = match cur {
            Value::Array(items) => {
                let i = array_index(token, items.len())?;
                if i == items.len() {
                    items.push(Value::Null);
                }
                &mut items[i]
            }
            Value::Object(map) => map.entry(token.clone()).or_insert(Value::Null),
            _ => unreachable!(),
        };
    }
    if !(cur.is_object() || cur.is_array()) {
        *cur = Value::Object(Map::new());
    }
    match cur {
        Value::Array(items) => {
            let i = array_index(last, items.len())?;
            match value {
                None => {
                    if i < items.len() {
                        items.remove(i);
                    }
                }
                Some(v) if i == items.len() => items.push(v),
                Some(v) => items[i] = v,
            }
        }
        Value::Object(map) => match value {
            None => {
                map.remove(last);
            }
            Some(v) => {
                map.insert(last.clone(), v);
            }
        },
        _ => unreachable!(),
    }
    Ok(())
}

/// What `resolve_value` needs.
#[derive(Clone, Copy)]
pub struct ResolveContext<'a> {
    pub data: &'a Value,
    /// The template item's pointer, for relative paths.
    pub scope: &'a str,
    /// The surface's built-in string table: a `$string.<id>` value resolves
    /// through it (`None` = left as written).
    pub strings: Option<&'a StringTable>,
    /// A value standing in for a pointer prefix (a literal Table row is the
    /// data scope of its slot cells): `(prefix, value)`.
    pub overlay: Option<(&'a str, &'a Value)>,
    /// A LITERAL item that is not in the data model (a Table row of literal
    /// `rows`, the TS `DataScope.item`): relative paths read inside it,
    /// absolute ones still the data, and a relative `set` writes nothing.
    pub item: Option<&'a Value>,
    /// The catalog whose prop schemas decide which props are DATA
    /// ([`bind_tree`]); `None` = the core catalog.
    pub view: Option<&'a CatalogView>,
    /// Round 2: the surface's formatter (`SurfaceSettings.locale` + the
    /// host's zone); the format functions run through it. `None` = the
    /// English fallback ([`crate::format::ENGLISH`]).
    pub formatter: Option<&'a dyn Formatter>,
    /// The clock (epoch ms) `formatRelativeTime` reads without a `now`
    /// argument; `None` = the wall clock.
    pub now: Option<f64>,
}

impl<'a> ResolveContext<'a> {
    pub fn new(data: &'a Value, scope: &'a str) -> ResolveContext<'a> {
        ResolveContext { data, scope, strings: None, overlay: None, item: None, view: None, formatter: None, now: None }
    }

    /// Format through the surface's formatter (see [`ResolveContext::formatter`]).
    pub fn with_formatter(self, formatter: &'a dyn Formatter) -> ResolveContext<'a> {
        ResolveContext { formatter: Some(formatter), ..self }
    }

    /// A fixed clock for `formatRelativeTime` (fixtures, a host re-binding
    /// once a minute).
    pub fn with_now(self, now_ms: f64) -> ResolveContext<'a> {
        ResolveContext { now: Some(now_ms), ..self }
    }

    /// The formatter in effect.
    pub fn formatter(&self) -> &'a dyn Formatter {
        self.formatter.unwrap_or(&crate::format::ENGLISH)
    }

    /// Relative paths read inside `item` (see [`ResolveContext::item`]).
    pub fn with_item(self, item: &'a Value) -> ResolveContext<'a> {
        ResolveContext { item: Some(item), ..self }
    }

    /// Bind against an extended catalog (its components' prop schemas).
    pub fn with_view(self, view: &'a CatalogView) -> ResolveContext<'a> {
        ResolveContext { view: Some(view), ..self }
    }

    /// The value a binding path names: inside the literal item for a
    /// relative path under an item scope, else at its absolute pointer.
    pub fn read_path(&self, path: &str) -> Option<&'a Value> {
        if !path.starts_with('/') {
            if let Some(item) = self.item {
                return if path.is_empty() { Some(item) } else { get_pointer(item, &format!("/{path}")) };
            }
        }
        self.read(&absolute_path(path, self.scope))
    }

    pub fn with_strings(self, strings: &'a StringTable) -> ResolveContext<'a> {
        ResolveContext { strings: Some(strings), ..self }
    }

    pub fn with_overlay(self, prefix: &'a str, value: &'a Value) -> ResolveContext<'a> {
        ResolveContext { overlay: Some((prefix, value)), ..self }
    }

    /// The value at an absolute pointer (through the overlay when it names
    /// the overlay's prefix).
    pub fn read(&self, pointer: &str) -> Option<&'a Value> {
        if let Some((prefix, value)) = self.overlay {
            if let Some(rest) = pointer.strip_prefix(prefix) {
                if rest.is_empty() || rest.starts_with('/') {
                    return get_pointer(value, rest);
                }
            }
        }
        get_pointer(self.data, pointer)
    }
}


fn num(v: Option<&Value>) -> f64 {
    v.map(json::to_number).unwrap_or(f64::NAN)
}

fn interpolate(text: &str, ctx: &ResolveContext) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                let expr = after[..end].trim();
                let v = ctx.read(&absolute_path(expr, ctx.scope));
                if let Some(v) = v.filter(|v| !v.is_null()) {
                    out.push_str(&json::to_js_string(v));
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// The function table: the six format functions through the context's
/// formatter (round 2), the core functions, then the client functions of
/// the basic catalog (`and`/`or`/`not`/`required` with the core
/// truthiness, `src/dynamic.ts LOGIC_FUNCTIONS`). `openUrl` returns the url
/// as a string the surface turns into an event for the host. `None` =
/// unknown function or an undefined result.
pub fn call_function(name: &str, args: &Map<String, Value>, ctx: &ResolveContext) -> Option<Value> {
    if let Some(result) = crate::format::call_format_function(name, args, ctx.formatter(), ctx.now.unwrap_or_else(crate::format::now_ms)) {
        return result;
    }
    // VAPP-191: the expression functions, on the surface clock and in the
    // surface (team) zone (the formatter's offset).
    if crate::cel::CALL_NAMES.contains(&name) {
        let formatter = ctx.formatter();
        let offset = move |ms: f64| formatter.offset_minutes(ms);
        let env = crate::cel::ExpressionEnv { now: ctx.now, offset: Some(&offset), max_steps: None };
        if let Some(result) = crate::cel::call_expression_function(name, args, &env) {
            return result;
        }
    }
    if let Some(result) = crate::expr::core_function(name, args) {
        return result;
    }
    let a = |k: &str| args.get(k);
    let s = |k: &str| a(k).filter(|v| !v.is_null()).map(json::to_js_string).unwrap_or_default();
    Some(match name {
        "required" => validation(a("value").is_some_and(|v| truthy(v) && !v.as_array().is_some_and(Vec::is_empty))),
        // Round 9 §9.3: the hand-written dialect; a refused pattern or a
        // match past the step cap FAILS the evaluation (EXPRESSION_ERROR).
        "regex" => match crate::regex::regex_step_count(a("value").unwrap_or(&Value::Null), a("pattern").unwrap_or(&Value::Null)) {
            Ok((ok, _)) => validation(ok),
            Err(message) => {
                ctx.fail(&message);
                return None;
            }
        },
        "length" => {
            let n = s("value").chars().count() as f64;
            validation(a("min").is_none_or(|m| n >= json::to_number(m)) && a("max").is_none_or(|m| n <= json::to_number(m)))
        }
        "numeric" => {
            let n = num(a("value"));
            validation(n.is_finite() && a("min").is_none_or(|m| n >= json::to_number(m)) && a("max").is_none_or(|m| n <= json::to_number(m)))
        }
        "email" => {
            let v = s("value");
            let ok = v.split_once('@').is_some_and(|(local, domain)| {
                !local.is_empty() && !local.contains(char::is_whitespace) && domain.contains('.') && !domain.contains('@') && !domain.starts_with('.') && !domain.ends_with('.') && !domain.contains(char::is_whitespace)
            });
            validation(ok)
        }
        "formatString" => Value::String(interpolate(&s("value"), ctx)),
        "openUrl" => return a("url").filter(|v| v.is_string()).cloned(),
        // Round 9 §6.7: DynamicBoolean args under the 1.0 coercion table.
        "and" => Value::Bool(a("values").and_then(Value::as_array).is_some_and(|v| v.iter().all(|x| coerce_boolean(Some(x))))),
        "or" => Value::Bool(a("values").and_then(Value::as_array).is_some_and(|v| v.iter().any(|x| coerce_boolean(Some(x))))),
        "not" => Value::Bool(!coerce_boolean(a("value"))),
        _ => return None,
    })
}


/// Round 9 §6.6: a validator's answer, EXACTLY `{"valid": <bool>}`.
pub fn validation(valid: bool) -> Value {
    let mut m = Map::new();
    m.insert("valid".into(), Value::Bool(valid));
    Value::Object(m)
}

/// Round 9 §6.6: one `CheckRule` evaluated to its ValidationResult.
/// `true` → `{valid: true}`; `false` (or anything the coercion makes false)
/// → `{valid: false, message?, severity: "error"}`; a ValidationResult as
/// is, its `message` filled from the rule, `severity` defaulted; a failed
/// evaluation → `{valid: false, message: rule.message ?? <failure>, severity}`.
pub fn evaluate_check(rule: &Value, ctx: &ResolveContext) -> Value {
    let message = rule.get("message").filter(|m| m.is_string()).cloned();
    let failure = FailureScope::new();
    let resolved = rule.get("condition").and_then(|c| failure.run(|| resolve_value(c, ctx)));
    let failed = failure.take();
    let fail = |message: Option<Value>| {
        let mut m = Map::new();
        m.insert("valid".into(), Value::Bool(false));
        if let Some(message) = message {
            m.insert("message".into(), message);
        }
        m.insert("severity".into(), Value::String("error".into()));
        Value::Object(m)
    };
    if let Some(reason) = failed {
        return fail(message.or(Some(Value::String(reason))));
    }
    match resolved {
        Some(Value::Object(mut o)) if o.get("valid").is_some_and(Value::is_boolean) => {
            if !o.contains_key("message") {
                if let Some(message) = message {
                    o.insert("message".into(), message);
                }
            }
            if !o.contains_key("severity") && o.get("valid") == Some(&Value::Bool(false)) {
                o.insert("severity".into(), Value::String("error".into()));
            }
            Value::Object(o)
        }
        other => {
            if coerce_boolean(other.as_ref()) {
                validation(true)
            } else {
                fail(message)
            }
        }
    }
}

/// Is a ValidationResult failing? (`valid` false.)
pub fn check_failed(result: &Value) -> bool {
    result.get("valid") != Some(&Value::Bool(true))
}

/// A failing result's severity (`error` when absent or unknown).
pub fn check_severity(result: &Value) -> &str {
    match result.get("severity").and_then(Value::as_str) {
        Some(s @ ("warning" | "info")) => s,
        _ => "error",
    }
}

thread_local! {
    static FAILURES: std::cell::RefCell<Vec<Option<String>>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Round 9 §6.8: collects the FIRST failure (a refused regex, a reactive
/// effectful call …) of the evaluations run inside it. Nested scopes each
/// see their own; a failure outside any scope is dropped (the value is
/// `null` either way).
pub struct FailureScope(());

impl FailureScope {
    pub fn new() -> Self {
        FAILURES.with(|f| f.borrow_mut().push(None));
        FailureScope(())
    }

    /// Run `f` inside the scope.
    pub fn run<R>(&self, f: impl FnOnce() -> R) -> R {
        f()
    }

    /// The first failure recorded (and closes the scope).
    pub fn take(self) -> Option<String> {
        let out = FAILURES.with(|f| f.borrow_mut().pop().flatten());
        std::mem::forget(self);
        out
    }
}

impl Default for FailureScope {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for FailureScope {
    fn drop(&mut self) {
        FAILURES.with(|f| {
            f.borrow_mut().pop();
        });
    }
}

impl ResolveContext<'_> {
    /// Record an evaluation failure (§6.8) in the innermost [`FailureScope`].
    pub fn fail(&self, message: &str) {
        FAILURES.with(|f| {
            if let Some(slot) = f.borrow_mut().last_mut() {
                if slot.is_none() {
                    *slot = Some(message.to_string());
                }
            }
        });
    }
}

/// A prop or style value with its bindings and calls resolved at ANY depth
/// (`resolveDynamic`). Plain objects recurse (a check's `condition`, a menu
/// item's label); `$string.<id>` strings resolve through the context's
/// table. `None` = undefined.
pub fn resolve_value(value: &Value, ctx: &ResolveContext) -> Option<Value> {
    if is_binding(value) {
        return ctx.read_path(value["@path"].as_str().unwrap_or("")).cloned();
    }
    if is_call(value) {
        let name = value["@call"].as_str().unwrap_or("");
        let mut args = Map::new();
        if let Some(raw) = value.get("args").and_then(Value::as_object) {
            for (k, v) in raw {
                if let Some(r) = resolve_value(v, ctx) {
                    args.insert(k.clone(), r);
                }
            }
        }
        // A var bound to nothing (a row's missing optional field) reads null
        // in the expression, never an unknown name (spec §4.2, expressions.json).
        if name == "expr" {
            if let (Some(raw), Some(Value::Object(vars))) = (value.get("args").and_then(|a| a.get("vars")).and_then(Value::as_object), args.get_mut("vars")) {
                for k in raw.keys() {
                    vars.entry(k.clone()).or_insert(Value::Null);
                }
            }
        }
        // VAPP-195: a binding never throws: a failing (or unknown) function
        // shows the call's `default` (absent = no value).
        return call_function(name, &args, ctx).or_else(|| args.get("default").cloned());
    }
    match value {
        Value::String(s) if ctx.strings.is_some() => Some(Value::String(resolve_string(s, ctx.strings.expect("strings")).to_string())),
        Value::Array(items) => Some(Value::Array(items.iter().map(|i| resolve_value(i, ctx).unwrap_or(Value::Null)).collect())),
        Value::Object(map) => {
            let mut out = Map::new();
            for (k, v) in map {
                // Round 9 §3.3: `@@x` → `@x`; a refused `@x` key is dropped
                // (the entry check refuses it before it gets here).
                let Some(key) = crate::expr::literal_key(k) else { continue };
                if let Some(r) = resolve_value(v, ctx) {
                    out.insert(key.into_owned(), r);
                }
            }
            Some(Value::Object(out))
        }
        other => Some(other.clone()),
    }
}

/// Every binding path a prop tree reads (absolute), for dependency tracking.
pub fn binding_paths(value: &Value, scope: &str, out: &mut Vec<String>) {
    if is_binding(value) {
        out.push(absolute_path(value["@path"].as_str().unwrap_or(""), scope));
        return;
    }
    match value {
        Value::Array(items) => items.iter().for_each(|i| binding_paths(i, scope, out)),
        Value::Object(map) => map.values().for_each(|v| binding_paths(v, scope, out)),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// The bind pass (round 1, `src/dynamic.ts`)
// ---------------------------------------------------------------------------

/// A node's `visible`: absent = shown; else its resolved truthiness.
pub fn is_visible(visible: Option<&Value>, ctx: &ResolveContext) -> bool {
    match visible {
        None => true,
        // Round 9 §6.7: the 1.0 boolean coercion.
        Some(v) => coerce_boolean(resolve_value(v, ctx).as_ref()),
    }
}

/// A DATA schema: a shape-less `object` (any object) or an array of them —
/// Table `rows` in the core catalog. A literal value there is the author's
/// data, copied verbatim: no binding, call or `$string.<id>` inside it is
/// interpreted. A dynamic value AT the position still resolves (and its
/// result is never descended). `src/dynamic.ts isDataSchema`.
pub fn is_data_schema(schema: Option<&PropSchema>) -> bool {
    match schema {
        Some(s) if s.type_ == "object" => s.shape.is_none(),
        Some(s) if s.type_ == "array" => s.items.as_deref().is_some_and(|items| is_data_schema(Some(items))),
        _ => false,
    }
}

/// One prop value resolved along its schema: dynamic → resolved, DATA
/// literal → verbatim, arrays and shaped objects → per item / property,
/// anything else → [`resolve_value`] (any depth).
pub fn resolve_prop(value: &Value, schema: Option<&PropSchema>, ctx: &ResolveContext, defs: &indexmap::IndexMap<String, DefSchema>) -> Option<Value> {
    if is_dynamic(value) {
        return resolve_value(value, ctx);
    }
    if is_data_schema(schema) {
        return Some(value.clone());
    }
    if let (Some(s), Value::Array(items)) = (schema, value) {
        if s.type_ == "array" {
            if let Some(item_schema) = s.items.as_deref() {
                return Some(Value::Array(items.iter().map(|i| resolve_prop(i, Some(item_schema), ctx, defs).unwrap_or(Value::Null)).collect()));
            }
        }
    }
    let shape = schema.filter(|s| s.type_ == "object").and_then(|s| s.shape.as_ref()).and_then(|name| defs.get(name));
    if let (Some(shape), Value::Object(map)) = (shape, value) {
        let mut out = Map::new();
        for (k, v) in map {
            if let Some(r) = resolve_prop(v, shape.properties.get(k), ctx, defs) {
                out.insert(k.clone(), r);
            }
        }
        return Some(Value::Object(out));
    }
    resolve_value(value, ctx)
}

/// A node's props resolved along its component's schema (an unknown
/// component's props resolve at any depth).
pub fn resolve_node_props(def: Option<&ComponentDef>, props: &Map<String, Value>, ctx: &ResolveContext, defs: &indexmap::IndexMap<String, DefSchema>) -> Map<String, Value> {
    let mut out = Map::new();
    for (k, v) in props {
        if let Some(r) = resolve_prop(v, def.and_then(|d| d.props.get(k)), ctx, defs) {
            out.insert(k.clone(), r);
        }
    }
    out
}

/// The BIND pass a renderer runs over an expanded tree for one data model:
/// every prop resolved along its schema ([`resolve_node_props`]: DATA props
/// verbatim), every style value, recipe prop and `accessibility` value
/// resolved, a node whose `visible` resolves falsy dropped with its subtree,
/// `visible` itself removed. Actions stay unresolved (they evaluate at press
/// time, [`run_action`]); a `template` and the slots of a ROW-SCOPED
/// component (Table cells) stay UNBOUND — the painter binds them per item /
/// per row ([`bind_row_slot`]). Pure; `fixtures/bind-time.json` locks it.
pub fn bind_tree(node: &UiNode, ctx: &ResolveContext) -> Option<UiNode> {
    match ctx.view {
        Some(view) => bind_node(node, ctx, view),
        None => bind_node(node, ctx, &CatalogView::core()),
    }
}

fn bind_node(node: &UiNode, ctx: &ResolveContext, view: &CatalogView) -> Option<UiNode> {
    if !is_visible(node.visible.as_ref(), ctx) {
        return None;
    }
    let resolve_map = |m: &Map<String, Value>| match resolve_value(&Value::Object(m.clone()), ctx) {
        Some(Value::Object(out)) => out,
        _ => Map::new(),
    };
    let def = view.components.get(&node.component);
    let mut out = UiNode::new(node.id.clone(), node.component.clone());
    out.props = resolve_node_props(def, &node.props, ctx, &view.defs);
    out.style = node.style.as_ref().map(resolve_map);
    out.on = node.on.clone();
    out.accessibility = match node.accessibility.as_ref().map(|a| resolve_value(a, ctx)) {
        Some(Some(Value::Object(a))) if !a.is_empty() => Some(Value::Object(a)),
        _ => None,
    };
    out.children = node.children.iter().filter_map(|c| bind_node(c, ctx, view)).collect();
    if let Some(slots) = &node.slots {
        if def.is_some_and(ComponentDef::has_row_slots) {
            out.slots = Some(slots.clone());
        } else {
            let bound: indexmap::IndexMap<String, UiNode> = slots.iter().filter_map(|(k, c)| bind_node(c, ctx, view).map(|b| (k.clone(), b))).collect();
            if !bound.is_empty() {
                out.slots = Some(bound);
            }
        }
    }
    out.template = node.template.clone();
    out.recipe = node.recipe.as_ref().map(|r| crate::types::Recipe { macro_: r.macro_.clone(), part: r.part.clone(), props: resolve_map(&r.props), authored: r.authored.clone() });
    Some(out)
}

/// A row-scoped slot cell bound for row `index` (`src/dynamic.ts
/// bindRowSlot` + `rowScope`). `rows_prop` = the UNBOUND `rows` value: a
/// binding → relative paths read the row at `<its pointer>/<index>` in the
/// data model (and a `set` writes into it); a literal or a call → relative
/// paths read inside `rows[index]` (a relative `set` writes nothing).
/// `index` = the row's index in `rows` as given, before any local sort.
pub fn bind_row_slot(slot: &UiNode, rows_prop: &Value, rows: &[Value], index: usize, ctx: &ResolveContext) -> Option<UiNode> {
    if is_binding(rows_prop) {
        let base = format!("{}/{index}", absolute_path(rows_prop["@path"].as_str().unwrap_or(""), ctx.scope));
        let row_ctx = ResolveContext { scope: &base, item: None, ..*ctx };
        return bind_tree(slot, &row_ctx);
    }
    let null = Value::Null;
    let row_ctx = ResolveContext { item: Some(rows.get(index).unwrap_or(&null)), ..*ctx };
    bind_tree(slot, &row_ctx)
}

/// Round 2 (§4): a List's `section` header bound for section `index`
/// (`src/dynamic.ts bindSectionHeader` + `sectionScope`): relative paths
/// read the LITERAL item `{value, count, index}`, absolute ones the data.
pub fn bind_section_header(slot: &UiNode, section: &crate::list::ListSection, index: usize, ctx: &ResolveContext) -> Option<UiNode> {
    let item = serde_json::json!({"value": section.value, "count": section.count, "index": index});
    let section_ctx = ResolveContext { item: Some(&item), ..*ctx };
    bind_tree(slot, &section_ctx)
}

/// An event an action dispatches, its context resolved BEFORE any write.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionEvent {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<Value>,
}

/// A function other than `set` the host must run (`openUrl`…), args resolved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionCall {
    pub call: String,
    pub args: Map<String, Value>,
}

/// What a press does (`src/dynamic.ts ActionOutcome`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionOutcome {
    /// The data model after the action's `set` (unchanged without one).
    pub data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<ActionEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<ActionCall>,
    /// The absolute pointer `set` wrote (not part of the TS outcome; the
    /// surface reports it as `DataChanged`).
    #[serde(skip)]
    pub written: Option<(String, Value)>,
    /// VAPP-103: the `set` was refused ([`set_pointer`]'s reason).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// What a press does: resolve the function args and the event context
/// against the data AS IT IS, then apply `set` (relative paths against the
/// scope, missing objects created), then hand back the event.
pub fn run_action(action: &Value, ctx: &ResolveContext) -> ActionOutcome {
    let mut out = ActionOutcome { data: ctx.data.clone(), event: None, call: None, written: None, error: None };
    let event = action.get("event").filter(|e| e.is_object()).map(|e| ActionEvent {
        name: e.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
        context: e.get("context").filter(|c| !c.is_null()).map(|c| resolve_value(c, ctx).unwrap_or(Value::Null)),
    });
    // A2UI's `functionCall`: the only action key.
    if let Some(function) = action.get("functionCall").filter(|f| f.is_object()) {
        let name = function.get("@call").and_then(Value::as_str).unwrap_or("").to_string();
        let args = match resolve_value(function.get("args").unwrap_or(&Value::Object(Map::new())), ctx) {
            Some(Value::Object(a)) => a,
            _ => Map::new(),
        };
        match (name.as_str(), args.get("path").and_then(Value::as_str)) {
            // A relative path under a literal-item scope names no data: no write.
            ("set", Some(path)) if !path.starts_with('/') && ctx.item.is_some() => {}
            ("set", None) => {}
            ("set", Some(path)) => {
                let pointer = absolute_path(path, ctx.scope);
                let value = args.get("value").cloned();
                // No value: the key exists without one, which JSON drops
                // (the containers on the way are still created).
                let written = set_pointer(&mut out.data, &pointer, Some(value.clone().unwrap_or(Value::Null))).and_then(|()| match value {
                    None => set_pointer(&mut out.data, &pointer, None),
                    Some(_) => Ok(()),
                });
                match written {
                    Ok(()) => out.written = Some((pointer, value.unwrap_or(Value::Null))),
                    Err(e) => out.error = Some(e),
                }
            }
            _ => out.call = Some(ActionCall { call: name, args }),
        }
    }
    out.event = event;
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pointers_get_and_set() {
        let mut data = json!({"user": {"name": "Ada"}, "items": [1, 2]});
        assert_eq!(get_pointer(&data, "/user/name"), Some(&json!("Ada")));
        assert_eq!(get_pointer(&data, "/items/1"), Some(&json!(2)));
        set_pointer(&mut data, "/user/age", Some(json!(36))).unwrap();
        set_pointer(&mut data, "/items/-", Some(json!(3))).unwrap();
        set_pointer(&mut data, "/items/0", None).unwrap();
        assert_eq!(data, json!({"user": {"name": "Ada", "age": 36}, "items": [2, 3]}));
        set_pointer(&mut data, "/new/deep/key", Some(json!(true))).unwrap();
        assert_eq!(get_pointer(&data, "/new/deep/key"), Some(&json!(true)));
        assert_eq!(absolute_path("name", "/items/3"), "/items/3/name");
        assert_eq!(absolute_path("", "/items/3"), "/items/3");
        assert_eq!(absolute_path("/x", "/items/3"), "/x");
        assert_eq!(absolute_path("x", ""), "/x");
        assert_eq!(absolute_path("", ""), "");
        let mut fresh = json!({});
        set_pointer(&mut fresh, "/a/0/b", Some(json!(1))).unwrap();
        assert_eq!(fresh, json!({"a": {"0": {"b": 1}}}), "missing containers are objects (writePointer)");
    }

    #[test]
    fn functions_resolve() {
        let data = json!({"n": 3, "email": "a@b.co", "price": 1234.5});
        let ctx = ResolveContext::new(&data, "");
        let call = |name: &str, args: Value| resolve_value(&json!({"@call": name, "args": args}), &ctx);
        assert_eq!(call("required", json!({"value": {"@path": "/email"}})), Some(json!(true)));
        assert_eq!(call("email", json!({"value": {"@path": "/email"}})), Some(json!(true)));
        assert_eq!(call("regex", json!({"value": "ab12", "pattern": "^[a-z]+\\d{2}$"})), Some(json!(true)));
        assert_eq!(call("regex", json!({"value": "ab1", "pattern": "^[a-z]+\\d{2}$"})), Some(json!(false)));
        assert_eq!(call("formatNumber", json!({"value": {"@path": "/price"}})), Some(json!("1,234.5")));
        assert_eq!(call("formatPercent", json!({"value": 0.256})), Some(json!("26%")));
        assert_eq!(call("formatNumber", json!({"value": 1234.5, "decimals": 2})), Some(json!("1,234.50")));
        assert_eq!(call("formatCurrency", json!({"value": 1234.5, "currency": "USD"})), Some(json!("$1,234.50")));
        assert_eq!(call("formatString", json!({"value": "n = ${/n}"})), Some(json!("n = 3")));
        assert_eq!(call("pluralize", json!({"value": {"@path": "/n"}, "one": "item", "other": "items"})), Some(json!("items")));
        assert_eq!(call("pluralize", json!({"value": 1, "other": "items"})), Some(json!("items")));
        assert_eq!(call("pluralize", json!({"value": "x"})), None);
        assert_eq!(call("formatDate", json!({"value": "2026-10-07", "format": "d MMM yyyy"})), Some(json!("7 Oct 2026")));
        assert_eq!(call("formatDate", json!({"value": "2026-10-07", "format": "EEE"})), Some(json!("Wed")));
        assert_eq!(call("and", json!({"values": [true, 1]})), Some(json!(true)));
        assert_eq!(call("not", json!({"value": ""})), Some(json!(true)));
        assert_eq!(call("length", json!({"value": "abc", "min": 2, "max": 3})), Some(json!(true)));
        assert_eq!(call("numeric", json!({"value": "12", "min": 10})), Some(json!(true)));
    }

    #[test]
    fn a_section_header_binds_its_section() {
        let header: UiNode = serde_json::from_value(json!({"id": "h", "component": "Text", "props": {"text": {"@call": "concat", "args": {"values": [{"@path": "value"}, " · ", {"@path": "count"}, " · ", {"@path": "/title"}]}}}})).unwrap();
        let data = json!({"title": "Inbox"});
        let section = crate::list::ListSection { value: "Today".into(), start: 0, count: 2 };
        let bound = bind_section_header(&header, &section, 0, &ResolveContext::new(&data, "")).unwrap();
        assert_eq!(bound.props["text"], json!("Today · 2 · Inbox"));
    }

    #[test]
    fn a_bound_submenu_resolves_to_the_rows_of_its_source() {
        // Round 3: `menuItem.items` is bindable; a submenu's rows may come
        // from a host source (`bindDataModel` → statuses, members, labels).
        let view = CatalogView::core();
        let def = view.components.get("Menu").expect("Menu");
        let props = json!({"items": [
            {"label": "Rename", "value": "rename"},
            {"kind": "submenu", "label": "Status", "items": {"@path": "/statuses"}},
        ]});
        let props = props.as_object().unwrap();
        assert_eq!(crate::validate::validate_props(def, props, "props", &view), vec![]);
        let data = json!({"statuses": [{"label": "Backlog", "value": "backlog"}, {"label": "Done", "value": "done", "icon": "ui-check"}]});
        let out = resolve_node_props(Some(def), props, &ResolveContext::new(&data, ""), &view.defs);
        assert_eq!(out["items"][0], json!({"label": "Rename", "value": "rename"}));
        assert_eq!(out["items"][1]["items"], data["statuses"]);
        // An unset source resolves to nothing: the submenu has no rows.
        let empty = resolve_node_props(Some(def), props, &ResolveContext::new(&json!({}), ""), &view.defs);
        assert!(empty["items"][1].get("items").is_none());
    }

    #[test]
    fn template_scope_resolves_relative_paths() {
        let data = json!({"items": [{"name": "one"}, {"name": "two"}]});
        let ctx = ResolveContext::new(&data, "/items/1");
        assert_eq!(resolve_value(&json!({"@path": "name"}), &ctx), Some(json!("two")));
        let mut paths = Vec::new();
        binding_paths(&json!({"a": {"@path": "name"}, "b": [{"@path": "/x"}]}), "/items/1", &mut paths);
        assert_eq!(paths, vec!["/items/1/name", "/x"]);
    }
}
