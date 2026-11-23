//! #EDE-216: a shell document on disk is Express (`<name>.express`, the
//! A2UI inference format, `eui_core::express`). `packages/ede/fixtures/express.json`
//! is the spec (its `$comment`); `tests/contract.rs` replays its cases.
//!
//! - `surface("<name>", extensions={ede: {…}})` opens the document's one
//!   surface; the `ede` extension IS the surface header (surface.json).
//! - `$/path = value` lines are its data model (the document's `data`).
//! - `root = …` is its top component (Express names it `root`).
//! - `#` comments are the document's notes (the compiler drops them).
//!
//! [`compile_document`] turns the file into the JSON form ede-ui has always
//! validated (`{surface, root, data}`, the nested tree), so
//! [`crate::validate_document`] and everything after it stay as they were;
//! [`decompile_document`] writes a JSON document as Express (the one-time
//! migration of `<name>.json`, and `set` of a JSON document).

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use eui_core::express::a2ui_express::{
    compile::{CompileError, CompileErrorKind, Compiler},
    decompile::{decompile, DecompileOptions, Order},
    schema::Catalog,
    syntax::{self, Expr, ExprKind, StatementKind, Target},
};
use serde_json::{json, Map, Value};

use crate::doc::{validate_document, Document, Issue};

/// A document's file suffix.
pub const SUFFIX: &str = ".express";

/// The `surface()` extension that holds the surface header.
pub const EXTENSION: &str = "ede";

/// The host functions a shell document may call, with their arguments
/// (express.json `hostFunctions`): the Express compiler knows them beside
/// the core catalog's, so a misspelt one or an unknown argument is a
/// compile error. Which of them a document may call is still its
/// `surface.functions` (Tier C).
pub const HOST_FUNCTIONS: &[(&str, &[&str])] = &[
    ("ede_alt_tab_select", &["window"]),
    ("ede_chat", &["action", "agentId", "answer", "insert", "key", "messageId", "prompt", "query", "replyId", "sessionId", "step", "text"]),
    ("ede_close", &["window"]),
    ("ede_close_document", &["name"]),
    ("ede_config", &["diagnostics", "theme"]),
    ("ede_connect", &["action", "instance"]),
    ("ede_focus", &["window"]),
    ("ede_history", &["action"]),
    ("ede_issue", &["identifier", "source", "sourceId"]),
    ("ede_launch", &["app", "command", "new"]),
    ("ede_lock", &[]),
    ("ede_logout", &[]),
    ("ede_minimize", &["window"]),
    ("ede_needs_you_open", &["id"]),
    ("ede_notify_action", &["id", "key"]),
    ("ede_notify_clear", &[]),
    ("ede_notify_close", &["id", "toast"]),
    ("ede_notify_open", &["id"]),
    ("ede_open_document", &["name"]),
    ("ede_output", &["mode", "name", "scale"]),
    ("ede_pin", &["app"]),
    ("ede_power", &["action"]),
    ("ede_rail", &[]),
    ("ede_select", &["window"]),
    ("ede_service", &["action", "args", "domain"]),
    ("ede_settings", &["key", "page", "value"]),
    ("ede_share", &["account", "action", "name"]),
    ("ede_space", &["session", "size"]),
    ("ede_space_close", &["space"]),
    ("ede_start_menu", &["activate", "category", "toggle"]),
    ("ede_switcher", &["account", "action", "id", "kind"]),
    ("ede_taskbar", &["app"]),
    ("ede_taskbar_menu", &["app", "value"]),
    ("ede_taskbar_scroll", &["app", "direction"]),
    ("ede_tray", &["service"]),
    ("ede_tray_menu", &["id", "service"]),
    ("ede_unpin", &["app"]),
    ("ede_workspace", &["index"]),
];

/// The width a decompiled document's comment lines wrap at.
const COMMENT_WIDTH: usize = 100;

/// The core catalog plus [`HOST_FUNCTIONS`], built once per process.
pub fn compiler() -> &'static Compiler {
    static COMPILER: OnceLock<Compiler> = OnceLock::new();
    COMPILER.get_or_init(|| {
        let mut catalog: Value = serde_json::from_str(eui_core::express::EXPRESS_CATALOG_JSON).expect("the Express catalog parses");
        if let Some(any) = catalog.pointer_mut("/$defs/anyFunction/oneOf").and_then(Value::as_array_mut) {
            any.extend(HOST_FUNCTIONS.iter().map(|(name, _)| json!({"$ref": format!("#/functions/{name}")})));
        }
        let functions = catalog["functions"].as_object_mut().expect("the Express catalog has functions");
        for (name, args) in HOST_FUNCTIONS {
            let properties: Map<String, Value> = args.iter().map(|a| (a.to_string(), json!({}))).collect();
            functions.insert(
                name.to_string(),
                json!({
                    "type": "object",
                    "properties": {"@call": {"const": name}, "args": {"type": "object", "properties": properties, "unevaluatedProperties": false}},
                    "required": ["@call"],
                    "returnType": "void",
                    "allowedCallers": "rendererOnly",
                    "requiresUserActivation": false
                }),
            );
        }
        let catalog = Catalog::from_json_str(&catalog.to_string()).expect("the shell's Express catalog loads");
        Compiler::new(vec![catalog], None).expect("the shell's Express catalog is usable")
    })
}

/// Compile and validate `<name>.express`: [`compile_document`], then
/// [`validate_document`]; every issue names the Express line it is about.
pub fn validate_express(name: &str, source: &str) -> Result<Document, Vec<Issue>> {
    let lines = Lines::of(source);
    let document = compile_document(name, source)?;
    validate_document(name, &document).map_err(|issues| issues.into_iter().map(|i| lines.locate(i)).collect())
}

/// `<name>.express` → the document's JSON form `{surface, root, data?}`
/// (the nested tree, root id `root`), or the issues that refuse it, each
/// at its line. Structure only: [`validate_document`] does the rest.
pub fn compile_document(name: &str, source: &str) -> Result<Value, Vec<Issue>> {
    let lines = Lines::of(source);
    let messages = compiler().compile(source, name).map_err(|e| vec![lines.compile_issue(&e)])?;
    let mut issues = Vec::new();
    let mut create: Option<&Map<String, Value>> = None;
    for message in &messages {
        if let Some(c) = message.get("createSurface").and_then(Value::as_object) {
            if create.is_some() {
                issues.push(lines.at_surface("surface", "a shell document is ONE surface(…)"));
            } else {
                create = Some(c);
            }
        } else if message.get("updateComponents").is_some() {
            issues.push(lines.at_var("root", "root", "required: a document's top component is `root = …` (Express names it root)"));
        } else if message.get("updateDataModel").is_some() {
            issues.push(lines.at_data("data", "data lines ($/… = …) belong after surface(…), in its scope"));
        } else {
            let path = lines.other.map(|l| format!("line {l}")).unwrap_or_else(|| "express".into());
            issues.push(Issue::new(path, "a shell document holds its surface(…), its data and its components only (no deleteSurface or top-level function calls)"));
        }
    }
    let Some(create) = create else {
        if issues.is_empty() {
            issues.push(lines.at_var("root", "root", "required: a document's top component is `root = …` (Express names it root)"));
        }
        return Err(issues);
    };
    if create.get("surfaceId").and_then(Value::as_str) != Some(name) {
        let id = create.get("surfaceId").and_then(Value::as_str).unwrap_or("");
        issues.push(lines.at_surface("surface", &format!("surface({id:?}) names another document; this file's is surface({name:?}, …)")));
    }
    if create.contains_key("sendDataModel") {
        issues.push(lines.at_surface("surface", "sendDataModel is not a shell document's (only extensions={ede: {…}})"));
    }
    let extensions = create.get("metadata").and_then(|m| m.get("extensions")).and_then(Value::as_object);
    for key in extensions.into_iter().flat_map(|e| e.keys()) {
        if key != EXTENSION {
            issues.push(lines.at_surface("surface", &format!("unknown extension {key}; a shell document's surface takes extensions={{ede: {{…}}}}")));
        }
    }
    let header = extensions.and_then(|e| e.get(EXTENSION)).cloned();
    if header.is_none() {
        issues.push(lines.at_surface("surface", &format!("required: surface({name:?}, extensions={{ede: {{layer, anchor, size, …}}}}), how the surface sits on the screen")));
    }
    let components = create.get("components").and_then(Value::as_array).cloned().unwrap_or_default();
    let root = match nest(&components) {
        Ok(root) => Some(root),
        Err(nest_issues) => {
            issues.extend(nest_issues.into_iter().map(|i| lines.locate(i)));
            None
        }
    };
    if !issues.is_empty() {
        return Err(issues);
    }
    let mut document = Map::new();
    document.insert("surface".into(), header.unwrap_or(Value::Null));
    document.insert("root".into(), root.unwrap_or(Value::Null));
    if let Some(data) = create.get("dataModel") {
        document.insert("data".into(), data.clone());
    }
    Ok(Value::Object(document))
}

/// A JSON shell document → Express. The root becomes `root` (Express
/// requires it); `$comment` becomes the leading `#` lines; `$schema` and a
/// node's keys the nested form ignores are dropped. Structure only: a
/// document that does not validate still converts when its shape does.
pub fn decompile_document(name: &str, document: &Value) -> Result<String, Vec<Issue>> {
    let Some(obj) = document.as_object() else {
        return Err(vec![Issue::new("file", "expected a document object {surface, root | components, data}")]);
    };
    let mut issues = Vec::new();
    if let Some(id) = obj.get("catalogId").and_then(Value::as_str).filter(|id| *id != eui_core::CORE_CATALOG_ID) {
        issues.push(Issue::new("catalogId", format!("{id}: an Express document uses the core catalog")));
    }
    let Some(header) = obj.get("surface").cloned() else {
        issues.push(Issue::new("surface", "required: how the surface sits on the screen"));
        return Err(issues);
    };
    let components = match (obj.get("root"), obj.get("components")) {
        (Some(root), None) => {
            let mut out = Vec::new();
            match flatten(root, true, &mut out) {
                Ok(_) => out,
                Err(e) => {
                    issues.push(e);
                    Vec::new()
                }
            }
        }
        (None, Some(Value::Array(list))) => list.clone(),
        _ => {
            issues.push(Issue::new("file", "expected root (a nested tree) or components (a flat list)"));
            Vec::new()
        }
    };
    if !issues.is_empty() {
        return Err(issues);
    }
    let mut create = Map::new();
    create.insert("surfaceId".into(), name.into());
    create.insert("catalogId".into(), eui_core::CORE_CATALOG_ID.into());
    create.insert("components".into(), components.into());
    if let Some(data) = obj.get("data").filter(|d| d.as_object().is_some_and(|d| !d.is_empty())) {
        create.insert("dataModel".into(), data.clone());
    }
    create.insert("metadata".into(), json!({"extensions": {EXTENSION: header}}));
    let messages = [json!({"version": "v1.0", "createSurface": create})];
    let options = DecompileOptions { keyword_args: true, order: Order::Upstream };
    let body = decompile(compiler().catalogs(), &messages, &options).map_err(|e| vec![Issue::new("express", e.to_string())])?;
    let mut out = String::new();
    if let Some(comment) = obj.get("$comment").and_then(Value::as_str) {
        for line in comment.lines() {
            for wrapped in wrap(line, COMMENT_WIDTH) {
                out.push_str(&format!("# {wrapped}").trim_end().to_string());
                out.push('\n');
            }
        }
    }
    out.push_str(body.trim_end());
    out.push('\n');
    Ok(out)
}

/// `text` in lines of at most `width` chars, broken at spaces.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split(' ') {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    lines.push(line);
    lines
}

/// The keys of a nested node besides its props.
const NODE_KEYS: &[&str] = &["style", "visible", "on", "accessibility", "catalogId", "metadata"];

/// The nested authoring form → A2UI's flat list, parents first; the top
/// node's id becomes `root` when `top`. Returns the node's id.
fn flatten(node: &Value, top: bool, out: &mut Vec<Value>) -> Result<String, Issue> {
    let Some(n) = node.as_object() else {
        return Err(Issue::new("root", "expected a node {id, component, …}"));
    };
    let written = n.get("id").and_then(Value::as_str).unwrap_or("?").to_string();
    let id = if top { "root".to_string() } else { written.clone() };
    if !top && id == "root" {
        return Err(Issue::new("#root", "the id root is the top component's in Express; rename this one"));
    }
    let Some(component) = n.get("component").and_then(Value::as_str) else {
        return Err(Issue::new(format!("#{written}"), "expected a component name"));
    };
    let mut flat = Map::new();
    flat.insert("id".into(), id.clone().into());
    flat.insert("component".into(), component.into());
    if let Some(props) = n.get("props").and_then(Value::as_object) {
        for (k, v) in props {
            // #AXC-19: the check mixin rides the wire as `@checks`.
            let key = if k == "checks" { "@checks".to_string() } else { k.clone() };
            flat.insert(key, v.clone());
        }
    }
    // A `style` PROP is the base the node's `style` overrides (eui-core's
    // layout); the flat form has one `style`: the two merged.
    let prop_style = flat.remove("style").and_then(|s| s.as_object().cloned());
    for key in NODE_KEYS {
        if let Some(v) = n.get(*key) {
            flat.insert(key.to_string(), v.clone());
        }
    }
    if let Some(mut base) = prop_style {
        if let Some(own) = flat.get("style").and_then(Value::as_object) {
            base.extend(own.clone());
        }
        flat.insert("style".into(), base.into());
    }
    let at = out.len();
    out.push(Value::Null);
    let children: &[Value] = n.get("children").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]);
    match n.get("template").and_then(Value::as_object) {
        Some(template) => {
            let item = template.get("component").and_then(Value::as_str).unwrap_or("");
            let mut list = Map::new();
            list.insert("componentId".into(), item.into());
            list.insert("path".into(), template.get("path").cloned().unwrap_or(Value::Null));
            if let Some(key) = template.get("key") {
                list.insert("key".into(), key.clone());
            }
            flat.insert("children".into(), list.into());
            // A template item written inside its owner (a top-level template)
            // becomes a free component; nest() puts it back.
            for child in children {
                if child.get("id").and_then(Value::as_str) != Some(item) {
                    return Err(Issue::new(format!("#{written}"), "a component with a template has no other children"));
                }
                flatten(child, false, out)?;
            }
        }
        None => {
            if n.contains_key("children") {
                let ids = children.iter().map(|c| flatten(c, false, out)).collect::<Result<Vec<_>, _>>()?;
                flat.insert("children".into(), ids.into());
            }
        }
    }
    if let Some(slots) = n.get("slots").and_then(Value::as_object) {
        let mut ids = Map::new();
        for (slot, child) in slots {
            ids.insert(slot.clone(), flatten(child, false, out)?.into());
        }
        flat.insert("slots".into(), ids.into());
    }
    out[at] = Value::Object(flat);
    Ok(id)
}

/// A2UI's flat list → the nested tree from `root`. A template's item that
/// no parent names sits inside its owner (the nested form's place for it);
/// any other component the tree does not reach is an issue, so is one
/// placed twice.
fn nest(components: &[Value]) -> Result<Value, Vec<Issue>> {
    let by_id: HashMap<&str, &Map<String, Value>> = components.iter().filter_map(|c| c.as_object()).filter_map(|c| Some((c.get("id")?.as_str()?, c))).collect();
    let mut named: HashSet<&str> = HashSet::new();
    let mut template_of: HashMap<&str, &str> = HashMap::new();
    for c in by_id.values() {
        let id = c.get("id").and_then(Value::as_str).unwrap_or("");
        match c.get("children") {
            Some(Value::Array(ids)) => named.extend(ids.iter().filter_map(Value::as_str)),
            Some(Value::Object(t)) => {
                if let Some(item) = t.get("componentId").and_then(Value::as_str) {
                    template_of.insert(id, item);
                }
            }
            _ => {}
        }
        if let Some(slots) = c.get("slots").and_then(Value::as_object) {
            named.extend(slots.values().filter_map(Value::as_str));
        }
    }
    struct Nester<'a> {
        by_id: &'a HashMap<&'a str, &'a Map<String, Value>>,
        named: &'a HashSet<&'a str>,
        template_of: &'a HashMap<&'a str, &'a str>,
        placed: HashSet<String>,
        issues: Vec<Issue>,
    }
    impl Nester<'_> {
        /// The free template item of `id`, if it has one.
        fn free_item(&self, id: &str) -> Option<String> {
            self.template_of.get(id).filter(|item| !self.named.contains(**item) && self.by_id.contains_key(**item)).map(|s| s.to_string())
        }

        fn children(&mut self, parent: &str, ids: &[Value]) -> Vec<Value> {
            let mut out = Vec::new();
            for id in ids {
                let Some(id) = id.as_str() else { continue };
                if let Some(node) = self.build(parent, id) {
                    out.push(node);
                }
            }
            out
        }

        fn build(&mut self, parent: &str, id: &str) -> Option<Value> {
            let Some(c) = self.by_id.get(id).copied() else {
                self.issues.push(Issue::new(format!("#{parent}"), format!("{id} is not a component of this document")));
                return None;
            };
            if !self.placed.insert(id.to_string()) {
                self.issues.push(Issue::new(format!("#{id}"), "placed twice: a component has one parent"));
                return None;
            }
            let mut node = Map::new();
            let mut props = Map::new();
            node.insert("id".into(), id.into());
            node.insert("component".into(), c.get("component").cloned().unwrap_or(Value::Null));
            let wire_checks = c.contains_key("@checks");
            for (k, v) in c {
                match k.as_str() {
                    "id" | "component" | "children" | "slots" => {}
                    k if NODE_KEYS.contains(&k) => {
                        node.insert(k.into(), v.clone());
                    }
                    "@checks" => {
                        props.insert("checks".into(), v.clone());
                    }
                    "checks" if wire_checks => {}
                    _ => {
                        props.insert(k.clone(), v.clone());
                    }
                }
            }
            if !props.is_empty() {
                node.insert("props".into(), props.into());
            }
            match c.get("children") {
                Some(Value::Array(ids)) => {
                    let children = self.children(id, ids);
                    node.insert("children".into(), children.into());
                }
                Some(Value::Object(t)) => {
                    let mut template = Map::new();
                    template.insert("component".into(), t.get("componentId").cloned().unwrap_or(Value::Null));
                    template.insert("path".into(), t.get("path").cloned().unwrap_or(Value::Null));
                    if let Some(key) = t.get("key") {
                        template.insert("key".into(), key.clone());
                    }
                    node.insert("template".into(), template.into());
                    // A free item (no parent names it) sits inside its owner.
                    let children: Vec<Value> = self.free_item(id).and_then(|item| self.build(id, &item)).into_iter().collect();
                    node.insert("children".into(), children.into());
                }
                _ => {}
            }
            if let Some(slots) = c.get("slots").and_then(Value::as_object) {
                let mut out = Map::new();
                for (slot, child) in slots {
                    if let Some(child) = child.as_str().and_then(|child| self.build(id, child)) {
                        out.insert(slot.clone(), child);
                    }
                }
                node.insert("slots".into(), out.into());
            }
            Some(Value::Object(node))
        }
    }
    let mut nester = Nester { by_id: &by_id, named: &named, template_of: &template_of, placed: HashSet::new(), issues: Vec::new() };
    if !by_id.contains_key("root") {
        return Err(vec![Issue::new("#root", "required: a document's top component is `root = …` (Express names it root)")]);
    }
    let root = nester.build("root", "root");
    for c in components {
        let Some(id) = c.get("id").and_then(Value::as_str) else { continue };
        if !nester.placed.contains(id) {
            nester.issues.push(Issue::new(format!("#{id}"), "not in the tree: no component's children, slots or template names it"));
        }
    }
    match root {
        Some(root) if nester.issues.is_empty() => Ok(root),
        _ => Err(nester.issues),
    }
}

/// Where each statement of an Express file is: an issue names its line.
#[derive(Debug, Default)]
struct Lines {
    /// The `surface(…)` statement's line.
    surface: Option<u32>,
    /// The first data line.
    data: Option<u32>,
    /// The first top-level statement that is neither: a call.
    other: Option<u32>,
    /// Component id → (variable, line), in file order.
    components: Vec<(String, String, u32)>,
    /// Per statement: its line and every call in it with its keywords.
    calls: Vec<(u32, Vec<(String, Vec<String>)>)>,
}

impl Lines {
    fn of(source: &str) -> Lines {
        let parsed = syntax::parse(source);
        let mut lines = Lines::default();
        for statement in &parsed.program.statements {
            let line = statement.span.start.line;
            let mut calls = Vec::new();
            match &statement.kind {
                StatementKind::Assign { target: Target::Variable { name, .. }, value } => {
                    if let ExprKind::Call(call) = &value.kind {
                        let id = call
                            .args
                            .iter()
                            .find(|a| a.name.as_ref().is_some_and(|(n, _)| n == "id"))
                            .and_then(|a| match &a.value.kind {
                                ExprKind::String(s) => Some(s.clone()),
                                _ => None,
                            })
                            .unwrap_or_else(|| name.clone());
                        lines.components.push((id, name.clone(), line));
                    }
                    collect_calls(value, &mut calls);
                }
                StatementKind::Assign { target: Target::Path { .. }, value } => {
                    lines.data.get_or_insert(line);
                    collect_calls(value, &mut calls);
                }
                StatementKind::Expr(expr) => {
                    if matches!(&expr.kind, ExprKind::Call(c) if c.name == "surface") {
                        lines.surface.get_or_insert(line);
                    } else {
                        lines.other.get_or_insert(line);
                        collect_calls(expr, &mut calls);
                    }
                }
            }
            lines.calls.push((line, calls));
        }
        lines
    }

    fn at(line: Option<u32>, what: &str, message: &str) -> Issue {
        match line {
            Some(line) => Issue::new(format!("line {line} {what}"), message),
            None => Issue::new(what, message),
        }
    }

    fn at_surface(&self, what: &str, message: &str) -> Issue {
        Self::at(self.surface, what, message)
    }

    fn at_data(&self, what: &str, message: &str) -> Issue {
        Self::at(self.data, what, message)
    }

    fn at_var(&self, var: &str, what: &str, message: &str) -> Issue {
        Self::at(self.components.iter().find(|(_, v, _)| v == var).map(|c| c.2), what, message)
    }

    fn line_of_id(&self, id: &str) -> Option<u32> {
        self.components.iter().find(|(i, _, _)| i == id).map(|c| c.2)
    }

    /// An issue of the JSON form at the Express line it is about.
    fn locate(&self, issue: Issue) -> Issue {
        let line = if let Some(id) = issue.path.strip_prefix('#') {
            self.line_of_id(id)
        } else if issue.path == "surface" || issue.path.starts_with("surface.") {
            self.surface
        } else if issue.path == "data" {
            self.data
        } else {
            None
        };
        match line {
            Some(line) => Issue::new(format!("line {line} {}", issue.path), issue.message),
            None => issue,
        }
    }

    /// The first statement holding a call to `function` (with `keyword`).
    fn line_of_call(&self, function: &str, keyword: Option<&str>) -> Option<u32> {
        self.calls.iter().find(|(_, calls)| calls.iter().any(|(name, kws)| name == function && keyword.is_none_or(|k| kws.iter().any(|kw| kw == k)))).map(|(line, _)| *line)
    }

    /// The statement whose component is `component` (with `keyword`).
    fn line_of_component(&self, component: &str, keyword: Option<&str>) -> Option<u32> {
        self.line_of_call(component, keyword)
    }

    fn compile_issue(&self, e: &CompileError) -> Issue {
        let mut message = e.message.clone();
        if let Some(help) = &e.help {
            message = format!("{message} ({help})");
        }
        let (line, what) = match &e.kind {
            CompileErrorKind::Syntax { line, column } => {
                let prefix = format!("Syntax error at line {line}:{column}: ");
                let message = message.split_once(&prefix).map(|(_, rest)| rest.to_string()).unwrap_or(message);
                return Issue::new(format!("line {line}:{column}"), message);
            }
            CompileErrorKind::UndefinedRoot { .. } => {
                return Issue::new("root", "required: a document's top component is `root = …` (Express names it root)");
            }
            CompileErrorKind::UnknownComponent { component } => (self.line_of_component(component, None), String::new()),
            // A capitalised name is a component the catalog lacks (the
            // compiler reads any unknown call as a function).
            CompileErrorKind::UnknownFunction { function } if function.starts_with(|c: char| c.is_ascii_uppercase()) => {
                message = format!("unknown component {function}");
                (self.line_of_component(function, None), String::new())
            }
            CompileErrorKind::UnknownFunction { function } => (self.line_of_call(function, None), String::new()),
            _ if message.contains("with a root component") => {
                return Issue::new("root", "required: a document's top component is `root = …` (Express names it root)");
            }
            CompileErrorKind::MissingRequiredProperty { component, .. } => (self.line_of_component(component, None), String::new()),
            CompileErrorKind::UnknownProperty { component, property }
            | CompileErrorKind::DuplicateProperty { component, property }
            | CompileErrorKind::ForbiddenDatabinding { component, property } => (self.line_of_component(component, Some(property)), String::new()),
            CompileErrorKind::InvalidParam { function, param } | CompileErrorKind::DuplicateParam { function, param } => (self.line_of_call(function, Some(param)), String::new()),
            CompileErrorKind::IdCollision { id } => (self.components.iter().filter(|(i, _, _)| i == id).nth(1).map(|c| c.2), format!("#{id}")),
            _ => {
                // A message naming a variable in quotes: that statement.
                let named = self.components.iter().find(|(_, var, _)| message.contains(&format!("'{var}'")));
                (named.map(|c| c.2), named.map(|c| format!("#{}", c.0)).unwrap_or_default())
            }
        };
        let what = if what.is_empty() {
            // The statement's component id, if it is one.
            line.and_then(|l| self.components.iter().find(|c| c.2 == l)).map(|c| format!("#{}", c.0)).unwrap_or_default()
        } else {
            what
        };
        match line {
            Some(line) if what.is_empty() => Issue::new(format!("line {line}"), message),
            Some(line) => Issue::new(format!("line {line} {what}"), message),
            None => Issue::new("express", message),
        }
    }
}

/// Every call in `expr` with its keywords.
fn collect_calls(expr: &Expr, out: &mut Vec<(String, Vec<String>)>) {
    match &expr.kind {
        ExprKind::Call(call) => {
            out.push((call.name.clone(), call.args.iter().filter_map(|a| a.name.as_ref().map(|(n, _)| n.clone())).collect()));
            for a in &call.args {
                collect_calls(&a.value, out);
            }
        }
        ExprKind::Check { args, .. } => args.iter().for_each(|a| collect_calls(a, out)),
        ExprKind::List(items) => items.iter().for_each(|i| collect_calls(i, out)),
        ExprKind::Map(entries) => entries.iter().for_each(|e| collect_calls(&e.value, out)),
        _ => {}
    }
}
