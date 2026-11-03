//! eApps R8 (VAPP-233, docs/spec/apps.md §3.6/§2.3): declarative record
//! migrations between package versions. A line-by-line port of
//! `src/host/record-migrations.ts`; `fixtures/record-migrations.json` is the
//! spec (`tests/record_migrations.rs` replays it).

use serde::Serialize;
use serde_json::{Map, Value};

use super::package::PackageIssue;
use super::records::is_record_kind;

pub const RECORD_MIGRATIONS_MAX: usize = 64;
pub const RECORD_MIGRATION_OPS_MAX: usize = 32;
pub const MIGRATION_OP_NAMES: &[&str] = &["rename", "default", "drop", "map", "renameKind"];

type Version = [u64; 3];

fn issue(path: impl Into<String>, message: impl Into<String>) -> PackageIssue {
    PackageIssue { path: path.into(), message: message.into() }
}

fn num_part(s: &str) -> Option<u64> {
    if s == "0" || (!s.is_empty() && !s.starts_with('0') && s.bytes().all(|b| b.is_ascii_digit())) {
        s.parse().ok()
    } else {
        None
    }
}

/// `MAJOR.MINOR.PATCH`, nothing else.
fn version_of(v: &str) -> Option<Version> {
    let parts: Vec<&str> = v.trim().split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    Some([num_part(parts[0])?, num_part(parts[1])?, num_part(parts[2])?])
}

/// `1`, `1.2`, `1.2.3` (missing parts = 0): what an operator compares against.
fn numeric_version(v: &str) -> Option<Version> {
    let parts: Vec<&str> = v.split('.').collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    let mut out = [0u64; 3];
    for (i, p) in parts.iter().enumerate() {
        out[i] = num_part(p)?;
    }
    Some(out)
}

/// One comparator of a range against a version; `None` = not a comparator.
fn comparator_holds(token: &str, v: &Version) -> Option<bool> {
    let (op, rest) = [">=", "<=", ">", "<", "=", "^", "~"].iter().find_map(|op| token.strip_prefix(op).map(|r| (*op, r))).unwrap_or(("", token));
    if rest.is_empty() {
        return None;
    }
    if op.is_empty() || op == "=" {
        let parts: Vec<&str> = rest.split('.').collect();
        if parts.len() > 3 {
            return None;
        }
        let mut holds = true;
        for (i, p) in parts.iter().enumerate() {
            if *p == "x" || *p == "*" {
                continue;
            }
            holds &= num_part(p)? == v[i];
        }
        return Some(holds);
    }
    let want = numeric_version(rest)?;
    let c = v.cmp(&want);
    Some(match op {
        ">=" => c.is_ge(),
        ">" => c.is_gt(),
        "<=" => c.is_le(),
        "<" => c.is_lt(),
        "~" => v[0] == want[0] && v[1] == want[1] && c.is_ge(),
        _ => {
            if want[0] > 0 {
                v[0] == want[0] && c.is_ge()
            } else if want[1] > 0 {
                v[0] == 0 && v[1] == want[1] && c.is_ge()
            } else {
                c.is_eq()
            }
        }
    })
}

/// A range's alternatives (`||`), each comparators (and); `None` = no range.
fn range_parts(range: &str) -> Option<Vec<Vec<&str>>> {
    if range.trim().is_empty() {
        return None;
    }
    let alts: Vec<Vec<&str>> = range.split("||").map(|a| a.split_whitespace().collect()).collect();
    if alts.iter().any(Vec::is_empty) {
        return None;
    }
    for a in &alts {
        for t in a {
            comparator_holds(t, &[0, 0, 0])?;
        }
    }
    Some(alts)
}

/// Is `range` a SemVer range this rule reads?
pub fn is_version_range(range: &str) -> bool {
    range_parts(range).is_some()
}

/// Does `version` satisfy `range`?
pub fn range_holds(version: &str, range: &str) -> bool {
    let (Some(v), Some(alts)) = (version_of(version), range_parts(range)) else { return false };
    alts.iter().any(|a| a.iter().all(|t| comparator_holds(t, &v) == Some(true)))
}

/// A package's migrations, the well-formed ones (`{from, kind, ops}`).
pub fn package_migrations(pkg: &Value) -> Vec<&Map<String, Value>> {
    pkg.get("migrations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter(|m| m.get("from").is_some_and(Value::is_string) && m.get("kind").is_some_and(Value::is_string) && m.get("ops").is_some_and(Value::is_array))
        .collect()
}

fn op_name(op: &Value) -> Option<&'static str> {
    let o = op.as_object()?;
    if o.len() != 1 {
        return None;
    }
    let key = o.keys().next()?;
    MIGRATION_OP_NAMES.iter().copied().find(|n| n == key)
}

/// A record migrated: its kind, data and how many migrations ran.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum MigrateResult {
    Done { kind: String, data: Value, applied: usize },
    Failed { error: String },
}

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

/// A record written under `version` as `kind` through `migrations` (see the TS twin).
pub fn migrate_record(migrations: &[&Map<String, Value>], version: &str, kind: &str, data: &Value, now: f64) -> MigrateResult {
    let mut k = kind.to_string();
    let mut d: Map<String, Value> = data.as_object().cloned().unwrap_or_default();
    let mut applied = 0;
    for (i, m) in migrations.iter().enumerate() {
        if m["kind"].as_str() != Some(k.as_str()) || !range_holds(version, m["from"].as_str().unwrap_or("")) {
            continue;
        }
        applied += 1;
        for (j, op) in m["ops"].as_array().into_iter().flatten().enumerate() {
            let at = format!("migration {i} op {j}");
            match op_name(op) {
                Some("rename") => {
                    let r = &op["rename"];
                    let (from, to) = (s(r, "from"), s(r, "to"));
                    if let Some(v) = d.remove(from) {
                        d.insert(to.to_string(), v);
                    }
                }
                Some("default") => {
                    let r = &op["default"];
                    let f = s(r, "field");
                    if d.get(f).is_none_or(Value::is_null) {
                        d.insert(f.to_string(), r.get("value").cloned().unwrap_or(Value::Null));
                    }
                }
                Some("drop") => {
                    d.remove(s(&op["drop"], "field"));
                }
                Some("map") => {
                    let r = &op["map"];
                    let ast = match crate::cel::parse_expression(s(r, "expr")) {
                        Ok(ast) => ast,
                        Err(e) => return MigrateResult::Failed { error: format!("{at}: {e}") },
                    };
                    let mut vars = d.clone();
                    for name in crate::cel::expression_names(&ast) {
                        vars.entry(name).or_insert(Value::Null);
                    }
                    let env = crate::cel::ExpressionEnv { now: Some(now), offset: None, max_steps: None };
                    match crate::cel::evaluate_node(&ast, &vars, &env) {
                        Ok(v) => {
                            d.insert(s(r, "field").to_string(), v);
                        }
                        Err(e) => return MigrateResult::Failed { error: format!("{at}: {e}") },
                    }
                }
                Some("renameKind") => k = op["renameKind"].as_str().unwrap_or("").to_string(),
                _ => return MigrateResult::Failed { error: format!("{at}: unknown op") },
            }
        }
    }
    MigrateResult::Done { kind: k, data: Value::Object(d), applied }
}

fn sorted_keys(o: &Map<String, Value>) -> Vec<&String> {
    let mut v: Vec<&String> = o.keys().collect();
    v.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    v
}

/// eApps R8: the issues of a package's `migrations` (see the TS twin).
pub fn validate_migrations(pkg: &Value) -> Vec<PackageIssue> {
    let Some(list) = pkg.get("migrations") else { return Vec::new() };
    let Some(list) = list.as_array() else { return vec![issue("/migrations", "migrations is a list of {from, kind, ops}")] };
    let mut issues = Vec::new();
    if list.len() > RECORD_MIGRATIONS_MAX {
        issues.push(issue("/migrations", format!("at most {RECORD_MIGRATIONS_MAX} migrations")));
    }
    let empty = Map::new();
    let records = pkg.get("records").and_then(Value::as_object).unwrap_or(&empty);
    let field = |v: Option<&Value>| v.and_then(Value::as_str).is_some_and(|s| !s.is_empty());
    for (i, m) in list.iter().enumerate() {
        let at = format!("/migrations/{i}");
        let Some(m) = m.as_object() else {
            issues.push(issue(at, "a migration is {from, kind, ops}"));
            continue;
        };
        for key in sorted_keys(m) {
            if !["from", "kind", "ops"].contains(&key.as_str()) {
                issues.push(issue(format!("{at}/{key}"), format!("unknown key {key}")));
            }
        }
        if !m.get("from").and_then(Value::as_str).is_some_and(is_version_range) {
            issues.push(issue(format!("{at}/from"), "from is a version or a range (1.2.3, 1.x, ^1.2.0, >=1.0.0 <2.0.0)"));
        }
        let mut kind: Option<String> = m.get("kind").and_then(Value::as_str).filter(|k| is_record_kind(k)).map(str::to_string);
        if kind.is_none() {
            issues.push(issue(format!("{at}/kind"), "kind is a kind name"));
        }
        let Some(ops) = m.get("ops").and_then(Value::as_array) else {
            issues.push(issue(format!("{at}/ops"), "ops is a list"));
            continue;
        };
        if ops.len() > RECORD_MIGRATION_OPS_MAX {
            issues.push(issue(format!("{at}/ops"), format!("at most {RECORD_MIGRATION_OPS_MAX} ops")));
        }
        for (j, op) in ops.iter().enumerate() {
            let op_at = format!("{at}/ops/{j}");
            let Some(name) = op_name(op) else {
                issues.push(issue(op_at, format!("an op is one of {}", MIGRATION_OP_NAMES.join(", "))));
                continue;
            };
            let arg = &op[name];
            let obj = arg.as_object();
            let len = obj.map_or(0, Map::len);
            let bad = |shape: &str| issue(format!("{op_at}/{name}"), format!("{name} is {shape}"));
            match name {
                "rename" if !(obj.is_some() && field(arg.get("from")) && field(arg.get("to")) && len == 2) => issues.push(bad("{from, to}")),
                "default" if !(obj.is_some() && field(arg.get("field")) && arg.get("value").is_some() && len == 2) => issues.push(bad("{field, value}")),
                "drop" if !(obj.is_some() && field(arg.get("field")) && len == 1) => issues.push(bad("{field}")),
                "map" => {
                    if !(obj.is_some() && field(arg.get("field")) && arg.get("expr").is_some_and(Value::is_string) && len == 2) {
                        issues.push(bad("{field, expr}"));
                    } else if let Err(e) = crate::cel::parse_expression(s(arg, "expr")) {
                        issues.push(issue(format!("{op_at}/map/expr"), e));
                    }
                }
                "renameKind" => match arg.as_str().filter(|k| is_record_kind(k)) {
                    None => issues.push(bad("a kind name")),
                    Some(k) => kind = Some(k.to_string()),
                },
                _ => {}
            }
        }
        if let Some(k) = kind {
            if !records.contains_key(&k) {
                issues.push(issue(format!("{at}/kind"), format!("the migration ends in {k}, which the package does not declare")));
            }
        }
    }
    issues
}

/// One incompatible change of a kind between two packages.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct KindChange {
    pub change: &'static str,
    pub field: Option<String>,
}

fn props_of(kind: Option<&Value>) -> Map<String, Value> {
    kind.and_then(|k| k.get("schema")).and_then(|s| s.get("properties")).and_then(Value::as_object).cloned().unwrap_or_default()
}
fn required_of(kind: Option<&Value>) -> Vec<String> {
    kind.and_then(|k| k.get("schema")).and_then(|s| s.get("required")).and_then(Value::as_array).map(|l| l.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
}
fn js_text(v: Option<&Value>) -> String {
    match v {
        None => "undefined".into(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) => "null".into(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::Array(_)) => "[array]".into(),
        Some(Value::Object(_)) => "[object Object]".into(),
    }
}
fn type_of(f: &Value) -> String {
    let t = f.get("type");
    let items = if t.and_then(Value::as_str) == Some("array") { f.get("items").filter(|i| i.is_object()).map(|i| format!("<{}>", js_text(i.get("type")))) } else { None };
    format!("{}{}", js_text(t), items.unwrap_or_default())
}
fn enum_of(f: &Value) -> Option<Vec<Value>> {
    let e = if f.get("type").and_then(Value::as_str) == Some("array") && f.get("items").is_some_and(Value::is_object) { f["items"].get("enum") } else { f.get("enum") };
    e.and_then(Value::as_array).cloned()
}

/// The incompatible changes of each kind from `before` to `after` (see the TS twin).
pub fn kind_changes(before: &Value, after: &Value) -> Vec<(String, Vec<KindChange>)> {
    let empty = Map::new();
    let b = before.get("records").and_then(Value::as_object).unwrap_or(&empty);
    let a = after.get("records").and_then(Value::as_object).unwrap_or(&empty);
    let mut out = Vec::new();
    for kind in sorted_keys(b) {
        let mut changes = Vec::new();
        if !a.contains_key(kind) {
            changes.push(KindChange { change: "removed", field: None });
        } else {
            let bp = props_of(b.get(kind));
            let ap = props_of(a.get(kind));
            let breq = required_of(b.get(kind));
            for f in sorted_keys(&bp) {
                let push = |c: &'static str| KindChange { change: c, field: Some(f.clone()) };
                match ap.get(f) {
                    None => changes.push(push("drops")),
                    Some(af) if type_of(&bp[f]) != type_of(af) => changes.push(push("retypes")),
                    Some(af) => {
                        if let Some(ae) = enum_of(af) {
                            let narrows = match enum_of(&bp[f]) {
                                None => true,
                                Some(be) => be.iter().any(|v| !ae.contains(v)),
                            };
                            if narrows {
                                changes.push(push("narrows"));
                            }
                        }
                    }
                }
            }
            let mut areq = required_of(a.get(kind));
            areq.sort_by(|x, y| x.encode_utf16().cmp(y.encode_utf16()));
            for f in areq {
                if !breq.contains(&f) && ap.contains_key(&f) && !changes.iter().any(|c| c.field.as_deref() == Some(f.as_str())) {
                    changes.push(KindChange { change: "requires", field: Some(f) });
                }
            }
        }
        if !changes.is_empty() {
            out.push((kind.clone(), changes));
        }
    }
    out
}

/// [`kind_changes`] as the JSON object the fixture spells.
pub fn kind_changes_json(before: &Value, after: &Value) -> Value {
    Value::Object(kind_changes(before, after).into_iter().map(|(k, c)| (k, serde_json::to_value(c).unwrap_or(Value::Null))).collect())
}

fn covered(migrations: &[&Map<String, Value>], version: &str, kind: &str, c: &KindChange) -> bool {
    let f = c.field.as_deref();
    for m in migrations {
        if m["kind"].as_str() != Some(kind) || !range_holds(version, m["from"].as_str().unwrap_or("")) {
            continue;
        }
        for op in m["ops"].as_array().into_iter().flatten() {
            let name = op_name(op);
            let arg = |n: &str, k: &str| op.get(n).and_then(|a| a.get(k)).and_then(Value::as_str);
            let hit = match c.change {
                "removed" => name == Some("renameKind"),
                "drops" => (name == Some("drop") && arg("drop", "field") == f) || (name == Some("rename") && arg("rename", "from") == f),
                "retypes" | "narrows" => name == Some("map") && arg("map", "field") == f,
                _ => (name == Some("default") && arg("default", "field") == f) || (name == Some("map") && arg("map", "field") == f) || (name == Some("rename") && arg("rename", "to") == f),
            };
            if hit {
                return true;
            }
        }
    }
    false
}

fn change_text(c: &str) -> &'static str {
    match c {
        "removed" => "is removed",
        "drops" => "drops",
        "retypes" => "changes the type of",
        "narrows" => "narrows the values of",
        _ => "requires",
    }
}

/// Why `after` may not replace `before` (see the TS twin); `None` = it may.
pub fn migration_refusal(before: &Value, after: &Value) -> Option<String> {
    let bvs = before.get("version").and_then(Value::as_str)?;
    let avs = after.get("version").and_then(Value::as_str)?;
    let (bv, av) = (version_of(bvs)?, version_of(avs)?);
    if av[0] <= bv[0] {
        return None;
    }
    let migrations = package_migrations(after);
    for (kind, changes) in kind_changes(before, after) {
        for c in &changes {
            if covered(&migrations, bvs, &kind, c) {
                continue;
            }
            let what = match &c.field {
                None => format!("{kind} {}", change_text(c.change)),
                Some(f) => format!("{kind} {} {f}", change_text(c.change)),
            };
            return Some(format!("{what} in {avs}: add a migration from {bvs} that covers it"));
        }
    }
    None
}
