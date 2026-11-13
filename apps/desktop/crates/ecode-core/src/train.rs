//! The merge train's pure half (#EXP-1617, `fixtures/merge-train.json`
//! `resolver` + `pipeline`): a port of `packages/domain-contract/src/
//! merge-train.ts` (globMatch, resolveConflicts, trainChecksMode and the
//! pipeline types) and of `scripts/merge-train/strategies.ts` (the generic
//! resolver's file strategies), so an `ecode` repository's train composes a
//! batch exactly like the GitHub one (`ecode`'s `compose` feature runs the
//! git half). Base = the common ancestor, ours = the train tip (earlier
//! units), theirs = the later unit.

use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Map, Number, Value};

// ── the repo's `.exponential/merge-train.json` ─────────────────────────────

/// How a matched conflicted path resolves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResolveStrategy {
    PrependUnion,
    Renumber,
    Regenerate,
    JsonUnion,
    TakeLater,
}

/// One `generated` rule (`verify`: a renumber's regenerated files).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResolveRule {
    pub name: String,
    pub paths: Vec<String>,
    pub strategy: ResolveStrategy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TrainCi {
    #[serde(default)]
    pub workflows: Vec<String>,
}

/// A manifest glob set and the lock file name it pairs with.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LockManifests {
    pub paths: Vec<String>,
    pub lock: String,
}

/// #LOGS-2 (`repoPolicy.lockfiles`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LockfilePolicy {
    pub paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifests: Option<Vec<LockManifests>>,
}

/// The repo's policy (`TrainRepoConfig`); `generated` absent = none.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TrainRepoConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ci: Option<TrainCi>,
    #[serde(default, deserialize_with = "null_as_empty")]
    pub generated: Vec<ResolveRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lockfiles: Option<LockfilePolicy>,
}

fn null_as_empty<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<ResolveRule>, D::Error> {
    Ok(Option::<Vec<ResolveRule>>::deserialize(d)?.unwrap_or_default())
}

// ── the pipeline's contract ─────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrainPipelinePr {
    pub number: u64,
    pub head_sha: String,
    /// Committed once as `{title} (#{number})`.
    pub title: String,
    /// `Name <email>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrainPipelineUnit {
    pub prs: Vec<TrainPipelinePr>,
    /// A commit whose required checks were seen green (a lone unit only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tested_sha: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrainMode {
    Test,
    Untested,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrainChecksMode {
    Dispatched,
    Proven,
    Skipped,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UnitOutcome {
    Composed,
    Conflict,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Against {
    Base,
    Batch,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrainUnitOutcome {
    pub prs: Vec<u64>,
    pub outcome: UnitOutcome,
    pub files: Vec<String>,
    /// The resolver rule names applied to this unit.
    pub resolved: Vec<String>,
    /// #EXP-1711, a conflict only: `batch` = only an earlier unit is in the way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub against: Option<Against>,
    /// #LOGS-2, composed only: the lockfiles that drifted from the unit's head.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drifted: Option<Vec<String>>,
}

/// The `exp-train` result's JSON.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrainPipelineOutcome {
    pub train: String,
    pub base_sha: String,
    pub head: Option<String>,
    pub checks: TrainChecksMode,
    pub units: Vec<TrainUnitOutcome>,
    pub error: Option<String>,
}

// ── globs and the resolver ─────────────────────────────────────────────────

enum Tok {
    Lit(char),
    Star,
    One,
    AnyDirs,
    Any,
}

/// A regex `.`: anything but a line terminator.
fn dot(c: char) -> bool {
    !matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

fn glob_tokens(glob: &str) -> Vec<Tok> {
    let chars: Vec<char> = glob.chars().collect();
    let mut out = vec![];
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '*' && chars.get(i + 1) == Some(&'*') {
            if chars.get(i + 2) == Some(&'/') {
                out.push(Tok::AnyDirs);
                i += 3;
            } else {
                out.push(Tok::Any);
                i += 2;
            }
            continue;
        }
        out.push(match c {
            '*' => Tok::Star,
            '?' => Tok::One,
            c => Tok::Lit(c),
        });
        i += 1;
    }
    out
}

fn glob_at(t: &[Tok], p: &[char]) -> bool {
    match t.first() {
        None => p.is_empty(),
        Some(Tok::Lit(c)) => p.first() == Some(c) && glob_at(&t[1..], &p[1..]),
        Some(Tok::One) => p.first().is_some_and(|c| *c != '/' && dot(*c)) && glob_at(&t[1..], &p[1..]),
        Some(Tok::Star) => {
            for i in 0..=p.len() {
                if glob_at(&t[1..], &p[i..]) {
                    return true;
                }
                if i == p.len() || p[i] == '/' || !dot(p[i]) {
                    return false;
                }
            }
            false
        }
        Some(Tok::Any) => {
            for i in 0..=p.len() {
                if glob_at(&t[1..], &p[i..]) {
                    return true;
                }
                if i == p.len() || !dot(p[i]) {
                    return false;
                }
            }
            false
        }
        Some(Tok::AnyDirs) => {
            if glob_at(&t[1..], p) {
                return true;
            }
            for i in 0..p.len() {
                if !dot(p[i]) {
                    return false;
                }
                if p[i] == '/' && glob_at(&t[1..], &p[i + 1..]) {
                    return true;
                }
            }
            false
        }
    }
}

/// merge-train.ts `globMatch`, a repo glob: `**` = any run of path segments (incl. none), `*` = within
/// one segment, `?` = one character; anchored at the repo root.
pub fn train_glob_match(glob: &str, path: &str) -> bool {
    let path: Vec<char> = path.chars().collect();
    glob_at(&glob_tokens(glob), &path)
}

/// One rule's share of a unit's conflicted paths.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveStep {
    pub name: String,
    pub strategy: ResolveStrategy,
    pub paths: Vec<String>,
    pub command: Option<String>,
}

/// `{ok: true, steps}` or `{ok: false, files}`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveVerdict {
    Resolved(Vec<ResolveStep>),
    Conflict(Vec<String>),
}

impl Serialize for ResolveVerdict {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut out = Map::new();
        match self {
            Self::Resolved(steps) => {
                out.insert("ok".into(), Value::Bool(true));
                out.insert("steps".into(), serde_json::to_value(steps).map_err(serde::ser::Error::custom)?);
            }
            Self::Conflict(files) => {
                out.insert("ok".into(), Value::Bool(false));
                out.insert("files".into(), files.iter().cloned().map(Value::String).collect());
            }
        }
        out.serialize(s)
    }
}

/// The index of the first rule matching `path`.
pub fn rule_of(path: &str, policy: &[ResolveRule]) -> Option<usize> {
    policy.iter().position(|rule| rule.paths.iter().any(|glob| train_glob_match(glob, path)))
}

/// A unit's conflicted paths under the policy: every path matched (first
/// rule wins) = one step per rule in policy order, its paths in input
/// order; ANY unmatched path = a conflict naming only the unmatched files.
pub fn resolve_conflicts(paths: &[String], policy: &[ResolveRule]) -> ResolveVerdict {
    let mut unmatched = vec![];
    let mut by_rule: Vec<Vec<String>> = vec![vec![]; policy.len()];
    for path in paths {
        match rule_of(path, policy) {
            Some(i) => by_rule[i].push(path.clone()),
            None => unmatched.push(path.clone()),
        }
    }
    if !unmatched.is_empty() {
        return ResolveVerdict::Conflict(unmatched);
    }
    ResolveVerdict::Resolved(
        policy
            .iter()
            .zip(by_rule)
            .filter(|(_, paths)| !paths.is_empty())
            .map(|(rule, paths)| ResolveStep { name: rule.name.clone(), strategy: rule.strategy, paths, command: rule.command.clone() })
            .collect(),
    )
}

/// `trainChecksMode`'s input (`pipeline.checksModes`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct ChecksModeInput {
    pub mode: TrainMode,
    pub composed: bool,
    /// Any composed unit's `resolved` non-empty.
    pub resolved: bool,
    /// Any composed unit's `drifted` non-empty.
    pub drifted: bool,
    /// A lone unit whose composed tree is its tested_sha's.
    pub proven: bool,
    /// Any composed PR whose head does not contain base_sha.
    pub behind: bool,
    pub workflows: Vec<String>,
}

/// COMPOSE's CI mode: a Merge now is untested only when nothing was
/// resolved, no lockfile drifted and no PR is behind the base.
pub fn train_checks_mode(input: &ChecksModeInput) -> TrainChecksMode {
    if !input.composed {
        return TrainChecksMode::None;
    }
    if input.mode == TrainMode::Untested && !input.resolved && !input.drifted && !input.behind {
        return TrainChecksMode::Skipped;
    }
    if input.proven {
        return TrainChecksMode::Proven;
    }
    if input.workflows.is_empty() { TrainChecksMode::None } else { TrainChecksMode::Dispatched }
}

// ── the file strategies (strategies.ts) ─────────────────────────────────────

/// A JSON text as JS's `JSON.parse` sees it: integral numbers are plain
/// numbers (`1.0` = `1`), object keys iterate array indexes first.
pub(crate) fn js_json(value: Value) -> Value {
    match value {
        Value::Number(n) => match n.as_f64() {
            Some(f) if n.is_f64() && f.fract() == 0.0 && f.abs() < 9_007_199_254_740_992.0 => Value::Number(Number::from(f as i64)),
            _ => Value::Number(n),
        },
        Value::Array(items) => Value::Array(items.into_iter().map(js_json).collect()),
        Value::Object(map) => {
            let order: Vec<String> = crate::js::entries(&map).into_iter().map(|(k, _)| k.clone()).collect();
            let mut map = map;
            Value::Object(order.into_iter().map(|k| { let v = map.remove(&k).unwrap_or(Value::Null); (k, js_json(v)) }).collect())
        }
        other => other,
    }
}

/// `JSON.stringify` equality (key order counts); `None` = undefined.
fn same(a: Option<&Value>, b: Option<&Value>) -> bool {
    a.map(Value::to_string) == b.map(Value::to_string)
}

fn union_array(base: &[Value], ours: &[Value], theirs: &[Value]) -> Vec<Value> {
    let keys = |list: &[Value]| list.iter().map(Value::to_string).collect::<std::collections::HashSet<_>>();
    let (base_keys, their_keys, our_keys) = (keys(base), keys(theirs), keys(ours));
    // Either side's deletion of a base item holds; each side's additions
    // stay, ours first, then theirs in order.
    let mut out: Vec<Value> =
        ours.iter().filter(|v| !base_keys.contains(&v.to_string()) || their_keys.contains(&v.to_string())).cloned().collect();
    out.extend(theirs.iter().filter(|v| !our_keys.contains(&v.to_string()) && !base_keys.contains(&v.to_string())).cloned());
    out
}

/// A three-way JSON union: a key only one side changed takes that side; a
/// key both changed recurses into objects, unions arrays, else the later
/// unit (theirs) wins. `None` = absent.
pub fn merge_json(base: Option<&Value>, ours: Option<&Value>, theirs: Option<&Value>) -> Option<Value> {
    if same(theirs, base) {
        return ours.cloned();
    }
    if same(ours, base) {
        return theirs.cloned();
    }
    match (ours, theirs) {
        (Some(Value::Object(o)), Some(Value::Object(t))) => {
            let empty = Map::new();
            let b = match base {
                Some(Value::Object(b)) => b,
                _ => &empty,
            };
            let mut keys: Vec<&String> = crate::js::entries(o).into_iter().map(|(k, _)| k).collect();
            for (k, _) in crate::js::entries(t) {
                if !o.contains_key(k) {
                    keys.push(k);
                }
            }
            let mut out = Map::new();
            for k in keys {
                if let Some(v) = merge_json(b.get(k), o.get(k), t.get(k)) {
                    out.insert(k.clone(), v);
                }
            }
            Some(js_json(Value::Object(out)))
        }
        (Some(Value::Array(o)), Some(Value::Array(t))) => {
            let b: &[Value] = match base {
                Some(Value::Array(b)) => b,
                _ => &[],
            };
            Some(Value::Array(union_array(b, o, t)))
        }
        _ => theirs.cloned(),
    }
}

/// `JSON.stringify(value, null, indent)`.
pub(crate) fn js_stringify(value: &Value, indent: &str) -> String {
    let mut out = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(indent.as_bytes());
    let mut ser = serde_json::Serializer::with_formatter(&mut out, formatter);
    if value.serialize(&mut ser).is_err() {
        return value.to_string();
    }
    String::from_utf8(out).unwrap_or_default()
}

/// A JS `\s`.
fn js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}

/// The text form: the train tip's indentation and final newline. `Err` = a
/// side is not JSON (the compose fails, as the TS engine throws).
pub fn json_union(base: Option<&str>, ours: &str, theirs: &str) -> Result<String, String> {
    let mut indent = String::from("  ");
    for (i, _) in ours.match_indices('\n') {
        let run: String = ours[i + 1..].chars().take_while(|c| *c == ' ' || *c == '\t').collect();
        if !run.is_empty() && ours[i + 1 + run.len()..].chars().next().is_some_and(|c| !js_space(c)) {
            indent = run;
            break;
        }
    }
    let parse = |text: &str| serde_json::from_str::<Value>(text).map(js_json).map_err(|e| format!("JSON.parse: {e}"));
    let base = base.map(parse).transpose()?;
    let merged = merge_json(base.as_ref(), Some(&parse(ours)?), Some(&parse(theirs)?)).unwrap_or(Value::Null);
    Ok(js_stringify(&merged, &indent) + if ours.ends_with('\n') { "\n" } else { "" })
}

/// The leading number of a numbered file (`0042_brave_owl.sql` → 42).
pub fn file_number(path: &str) -> Option<u64> {
    let name = path.rsplit('/').next().unwrap_or("");
    let digits: String = name.chars().take_while(char::is_ascii_digit).collect();
    let next = name[digits.len()..].chars().next();
    (!digits.is_empty() && matches!(next, Some('_' | '-')))
        .then(|| digits.bytes().fold(0u64, |n, d| n.saturating_mul(10).saturating_add(u64::from(d - b'0'))))
}

/// Do the regenerated files carry exactly the dropped ones' contents (names
/// aside)? Trailing whitespace is not a difference.
pub fn same_contents(dropped: &[String], regenerated: &[String]) -> bool {
    fn norm(list: &[String]) -> Vec<&str> {
        let mut out: Vec<&str> = list.iter().map(|s| s.trim_end_matches(js_space)).collect();
        out.sort_unstable();
        out
    }
    norm(dropped) == norm(regenerated)
}

/// `text.match(/[^\n]*\n|[^\n]+$/g)`.
fn lines_of(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// base → side as ONE pure insertion: the split points it fits ([lo, hi])
/// and its length; None = not one.
fn insertion(base: &[&str], side: &[&str]) -> Option<(usize, usize, usize)> {
    if side.len() <= base.len() {
        return None;
    }
    let k = side.len() - base.len();
    let mut p = 0;
    while p < base.len() && base[p] == side[p] {
        p += 1;
    }
    let mut s = 0;
    while s < base.len() && base[base.len() - 1 - s] == side[side.len() - 1 - s] {
        s += 1;
    }
    let lo = base.len() - s;
    (lo <= p).then_some((lo, p, k))
}

fn word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The names of `/\bconst\s+([A-Za-z_][A-Za-z0-9_]*)/g`, in order.
fn declared(text: &str) -> Vec<String> {
    let b = text.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i + 5 <= b.len() {
        if &b[i..i + 5] == b"const" && (i == 0 || !word(b[i - 1])) {
            let rest = &text[i + 5..];
            let spaces: usize = rest.chars().take_while(|c| js_space(*c)).map(char::len_utf8).sum();
            let name = &rest[spaces..];
            if spaces > 0 && name.bytes().next().is_some_and(|c| c.is_ascii_alphabetic() || c == b'_') {
                let len = name.bytes().take_while(|c| word(*c)).count();
                out.push(name[..len].to_string());
                i += 5 + spaces + len;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// The byte offsets of `\b<name>\b` (name = an identifier).
fn word_hits(text: &str, name: &str) -> Vec<usize> {
    let b = text.as_bytes();
    text.match_indices(name)
        .map(|(i, _)| i)
        .filter(|i| (*i == 0 || !word(b[i - 1])) && b.get(i + name.len()).is_none_or(|c| !word(*c)))
        .collect()
}

/// The numbers N of `\b<prefix>(\d+)\b`.
fn numbered(text: &str, prefix: &str) -> Vec<u64> {
    let b = text.as_bytes();
    let mut out = vec![];
    for (i, _) in text.match_indices(prefix) {
        if i > 0 && word(b[i - 1]) {
            continue;
        }
        let digits = &b[i + prefix.len()..];
        let n = digits.iter().take_while(|c| c.is_ascii_digit()).count();
        if n > 0 && digits.get(n).is_none_or(|c| !word(*c)) {
            out.push(digits[..n].iter().fold(0u64, |acc, d| acc.saturating_mul(10).saturating_add(u64::from(d - b'0'))));
        }
    }
    out
}

/// prepend-union, entry-aware (#LOGS-2): base → ours and base → theirs each
/// ONE pure insertion at one common split point (the earliest) = base
/// prefix + theirs' block + ours' block + base rest. A `const <NAME>_<N>`
/// theirs' block declares that ours already declares is renumbered to the
/// next free N when the block names it only there. Anything else = None (a
/// conflict), never a line union, never a const declared twice.
pub fn prepend_union(base: &str, ours: &str, theirs: &str) -> Option<String> {
    if theirs == base || theirs == ours {
        return Some(ours.to_string());
    }
    if ours == base {
        return Some(theirs.to_string());
    }
    let (b, o, t) = (lines_of(base), lines_of(ours), lines_of(theirs));
    let (io, it) = (insertion(&b, &o)?, insertion(&b, &t)?);
    let at = io.0.max(it.0);
    if at > io.1.min(it.1) {
        return None;
    }
    let our_block = o[at..at + io.2].concat();
    let mut their_block = t[at..at + it.2].concat();
    let mut taken = declared(ours);
    let mut names = declared(&their_block);
    let mut seen = std::collections::HashSet::new();
    names.retain(|n| seen.insert(n.clone()));
    for name in names {
        if !taken.contains(&name) {
            continue;
        }
        // /^(.*_)(\d+)$/
        let digits = name.bytes().rev().take_while(u8::is_ascii_digit).count();
        let prefix = &name[..name.len() - digits];
        if digits == 0 || !prefix.ends_with('_') {
            return None;
        }
        let hits = word_hits(&their_block, &name);
        if hits.len() != 1 {
            return None;
        }
        let max = numbered(&format!("{ours}\n{their_block}"), prefix).into_iter().max().unwrap_or(0);
        let next = format!("{prefix}{}", max.saturating_add(1));
        their_block.replace_range(hits[0]..hits[0] + name.len(), &next);
        taken.push(next);
    }
    let text = [b[..at].concat(), their_block, our_block, b[at..].concat()].concat();
    let names = declared(&text);
    let unique: std::collections::HashSet<&String> = names.iter().collect();
    (unique.len() == names.len()).then_some(text)
}

/// The test shapes of a repo's two changelog files (#LOGS-2,
/// `scripts/merge-train/changelog-shapes.ts`): the web `CHANGELOG` array and
/// the desktop mirror's `LATEST` + `PREVIOUS_<N>`. Shared by both crates'
/// tests so they replay the TS cases verbatim.
#[doc(hidden)]
pub mod changelog_shapes {
    fn ts_entry(id: &str) -> String {
        format!("  {{\n    id: `{id}`,\n    date: `2026-11-09`,\n    title: `{}`,\n  }},\n", id.to_uppercase())
    }

    /// The web changelog, newest first.
    pub fn web_changelog(ids: &[&str]) -> String {
        format!("// Newest first.\nexport const CHANGELOG: ChangelogEntry[] = [\n{}]\n\nexport const tail = 1\n", ids.iter().map(|id| ts_entry(id)).collect::<String>())
    }

    fn rs_fields(id: &str) -> String {
        format!("    id: \"{id}\",\n    date: \"2026-11-09\",\n    title: \"{}\",\n}};\n", id.to_uppercase())
    }

    /// The desktop mirror: LATEST + the earlier heads, highest N first.
    pub fn rust_changelog(latest: &str, previous: &[(u32, &str)]) -> String {
        let mut out = format!("pub(crate) const LATEST: ChangelogEntry = ChangelogEntry {{\n{}", rs_fields(latest));
        for (n, id) in previous {
            out += &format!("\n/// An earlier head entry.\n#[allow(dead_code)]\nconst PREVIOUS_{n}: ChangelogEntry = ChangelogEntry {{\n{}", rs_fields(id));
        }
        out + "\nfn render() {}\n"
    }
}
