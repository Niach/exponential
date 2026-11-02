//! VAPP-191 (eApps Q2, spec `docs/spec/apps.md` §4.2/§4.3): the EXPRESSION
//! language, a CEL (Common Expression Language) subset with one number type
//! (every JSON number is a CEL double), JSON values only, maths, date/time
//! and duration functions, a type checker over declared variable types (a
//! package's record kinds) and cost caps (length, nesting, no nested
//! comprehensions, a step limit). The Rust twin of
//! `packages/exponential-ui/src/expressions.ts`, line by line;
//! `catalog/expressions.json` is the spec and `fixtures/expressions.json`
//! locks both. Pure: the clock and the team zone's UTC offset come in
//! through [`ExpressionClock`]. The A2UI call form (`{"call": "mul", "args":
//! {"a", "b"}}`) of every function with a `call` spelling runs the same code
//! ([`call_expression_function`], in the bind table).

use std::collections::HashMap;

use indexmap::IndexMap;
use serde_json::{Map, Value};

use crate::dates::{days_in_month, format_day, iso_week, parse_day, weekday_of, CalendarDay};
use crate::format::{parse_date_value, ZoneOffset};
use crate::json;

/// The cost caps (`limits`): source characters (code points), nesting of
/// `(…)`/`[…]`/`{…}`/calls, evaluation steps.
pub const MAX_LENGTH: usize = 1024;
pub const MAX_DEPTH: usize = 32;
pub const MAX_STEPS: usize = 10_000;

/// One function's fixed signature (`fixtures/expressions.json` `functions`).
#[derive(Debug, Clone, Copy)]
pub struct Signature {
    pub params: &'static [&'static str],
    pub returns: &'static str,
    /// The A2UI call-form argument names (`None` = CEL only).
    pub call: Option<&'static [&'static str]>,
    pub units: Option<&'static [&'static str]>,
    pub formats: Option<&'static [&'static str]>,
    /// Methods only: the receiver type.
    pub receiver: Option<&'static str>,
}

const fn sig(params: &'static [&'static str], returns: &'static str, call: Option<&'static [&'static str]>) -> Signature {
    Signature { params, returns, call, units: None, formats: None, receiver: None }
}

const DAY_UNITS: &[&str] = &["day", "week", "month", "quarter", "year"];
const DIFF_UNITS: &[&str] = &["second", "minute", "hour", "day", "week", "month", "quarter", "year"];
const DURATION_FORMATS: &[&str] = &["h:mm", "hh:mm", "h:mm:ss", "hh:mm:ss"];

/// The functions in catalog order.
pub const FUNCTIONS: &[(&str, Signature)] = &[
    ("size", sig(&["string|list|map"], "number", None)),
    ("string", sig(&["dyn"], "string", None)),
    ("double", sig(&["number|string"], "number", None)),
    ("int", sig(&["number|string"], "number", None)),
    ("mul", sig(&["number", "number"], "number", Some(&["a", "b"]))),
    ("div", sig(&["number", "number"], "number", Some(&["a", "b"]))),
    ("mod", sig(&["number", "number"], "number", Some(&["a", "b"]))),
    ("round", sig(&["number", "number?"], "number", Some(&["value", "digits"]))),
    ("floor", sig(&["number"], "number", Some(&["value"]))),
    ("ceil", sig(&["number"], "number", Some(&["value"]))),
    ("abs", sig(&["number"], "number", Some(&["value"]))),
    ("min", sig(&["number..."], "number", Some(&["values"]))),
    ("max", sig(&["number..."], "number", Some(&["values"]))),
    ("today", sig(&[], "string", Some(&[]))),
    ("now", sig(&[], "string", Some(&[]))),
    ("dateAdd", Signature { units: Some(DAY_UNITS), ..sig(&["string", "number", "unit"], "string", Some(&["date", "n", "unit"])) }),
    ("dateDiff", Signature { units: Some(DIFF_UNITS), ..sig(&["string", "string", "unit"], "number", Some(&["from", "to", "unit"])) }),
    ("startOf", Signature { units: Some(DAY_UNITS), ..sig(&["string", "unit"], "string", Some(&["date", "unit"])) }),
    ("endOf", Signature { units: Some(DAY_UNITS), ..sig(&["string", "unit"], "string", Some(&["date", "unit"])) }),
    ("weekday", sig(&["string"], "number", Some(&["date"]))),
    ("isoWeek", sig(&["string"], "number", Some(&["date"]))),
    ("hours", sig(&["number"], "number", Some(&["minutes"]))),
    ("formatDuration", Signature { formats: Some(DURATION_FORMATS), ..sig(&["number", "format?"], "string", Some(&["hours", "format"])) }),
];

/// The receiver methods (`x.size()`, `s.contains(t)`…).
pub const METHODS: &[(&str, Signature)] = &[
    ("size", Signature { receiver: Some("string|list|map"), ..sig(&[], "number", None) }),
    ("contains", Signature { receiver: Some("string"), ..sig(&["string"], "bool", None) }),
    ("startsWith", Signature { receiver: Some("string"), ..sig(&["string"], "bool", None) }),
    ("endsWith", Signature { receiver: Some("string"), ..sig(&["string"], "bool", None) }),
];

/// `l.map(x, e)` `l.filter(x, p)` `l.all(x, p)` `l.exists(x, p)`: never nested.
pub const COMPREHENSIONS: &[&str] = &["map", "filter", "all", "exists"];
/// CEL's reserved words: never a name.
pub const RESERVED: &[&str] = &["as", "break", "const", "continue", "else", "for", "function", "if", "import", "let", "loop", "package", "namespace", "return", "var", "void", "while"];
/// The row keys every record carries.
pub const ROW_KEYS: &[&str] = &["id", "createdAt", "updatedAt", "createdBy", "authorId", "author"];
/// The call-form names: every function with a `call` spelling + `expr`.
pub const CALL_NAMES: &[&str] = &["mul", "div", "mod", "round", "floor", "ceil", "abs", "min", "max", "today", "now", "dateAdd", "dateDiff", "startOf", "endOf", "weekday", "isoWeek", "hours", "formatDuration", "expr"];
/// The names whose value reads the clock.
pub const CLOCK_NAMES: &[&str] = &["today", "now", "expr"];

pub fn function(name: &str) -> Option<&'static Signature> {
    FUNCTIONS.iter().find(|(n, _)| *n == name).map(|(_, s)| s)
}
pub fn method(name: &str) -> Option<&'static Signature> {
    METHODS.iter().find(|(n, _)| *n == name).map(|(_, s)| s)
}

/// The tables as `fixtures/expressions.json` spells them.
pub fn tables_json() -> Value {
    let one = |s: &Signature| {
        let mut m = Map::new();
        if let Some(r) = s.receiver {
            m.insert("receiver".into(), Value::from(r));
        }
        m.insert("params".into(), Value::from(s.params.to_vec()));
        m.insert("returns".into(), Value::from(s.returns));
        if let Some(c) = s.call {
            m.insert("call".into(), Value::from(c.to_vec()));
        }
        if let Some(u) = s.units {
            m.insert("units".into(), Value::from(u.to_vec()));
        }
        if let Some(f) = s.formats {
            m.insert("formats".into(), Value::from(f.to_vec()));
        }
        Value::Object(m)
    };
    serde_json::json!({
        "limits": {"maxLength": MAX_LENGTH, "maxDepth": MAX_DEPTH, "maxSteps": MAX_STEPS},
        "functions": FUNCTIONS.iter().map(|(n, s)| (n.to_string(), one(s))).collect::<Map<_, _>>(),
        "methods": METHODS.iter().map(|(n, s)| (n.to_string(), one(s))).collect::<Map<_, _>>(),
        "comprehensions": COMPREHENSIONS,
        "reserved": RESERVED,
        "rowKeys": ROW_KEYS,
    })
}

/// Where `today()` and `now()` stand: epoch ms (`None` = the wall clock) and
/// the team zone's offset (`None` = UTC).
#[derive(Clone, Default)]
pub struct ExpressionClock {
    pub now: Option<f64>,
    pub zone: Option<ZoneOffset>,
}

impl ExpressionClock {
    /// A fixed instant at a fixed offset (minutes east of UTC).
    pub fn fixed(now_ms: f64, offset_minutes: i32) -> Self {
        ExpressionClock { now: Some(now_ms), zone: Some(std::sync::Arc::new(move |_| offset_minutes)) }
    }
    fn offset_at(&self, ms: f64) -> f64 {
        self.zone.as_ref().map(|z| z(ms) as f64).unwrap_or(0.0)
    }
    fn now_ms(&self) -> f64 {
        self.now.unwrap_or_else(crate::format::now_ms)
    }
}

type Fail<T> = Result<T, String>;

fn fail<T>(at: usize, message: impl AsRef<str>) -> Fail<T> {
    Err(format!("column {}: {}", at + 1, message.as_ref()))
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(String, f64),
    Str(String),
    Id(String),
    Op(&'static str),
    Eof,
}

#[derive(Debug, Clone)]
struct Token {
    t: Tok,
    at: usize,
}

const OPS2: &[&str] = &["==", "!=", "<=", ">=", "&&", "||"];
const OPS1: &[&str] = &["+", "-", "*", "/", "%", "<", ">", "!", "?", ":", ".", ",", "(", ")", "[", "]", "{", "}"];

fn is_digit(c: Option<&char>) -> bool {
    c.is_some_and(|c| c.is_ascii_digit())
}
fn is_id_start(c: Option<&char>) -> bool {
    c.is_some_and(|c| c.is_ascii_alphabetic() || *c == '_')
}
fn is_id_char(c: Option<&char>) -> bool {
    is_id_start(c) || is_digit(c)
}

fn lex(chars: &[char]) -> Fail<Vec<Token>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if matches!(c, ' ' | '\t' | '\n' | '\r') {
            i += 1;
            continue;
        }
        let at = i;
        if c.is_ascii_digit() {
            while is_digit(chars.get(i)) {
                i += 1;
            }
            if chars.get(i) == Some(&'.') && is_digit(chars.get(i + 1)) {
                i += 1;
                while is_digit(chars.get(i)) {
                    i += 1;
                }
            }
            if matches!(chars.get(i), Some('e' | 'E')) && (is_digit(chars.get(i + 1)) || (matches!(chars.get(i + 1), Some('+' | '-')) && is_digit(chars.get(i + 2)))) {
                i += 2;
                while is_digit(chars.get(i)) {
                    i += 1;
                }
            }
            if is_id_char(chars.get(i)) || (chars.get(i) == Some(&'.') && is_digit(chars.get(i + 1))) {
                return fail(at, "invalid number");
            }
            let text: String = chars[at..i].iter().collect();
            let n: f64 = text.parse().unwrap_or(f64::NAN);
            if !n.is_finite() {
                return fail(at, "invalid number");
            }
            out.push(Token { t: Tok::Num(text, n), at });
            continue;
        }
        if is_id_start(Some(&c)) {
            while is_id_char(chars.get(i)) {
                i += 1;
            }
            out.push(Token { t: Tok::Id(chars[at..i].iter().collect()), at });
            continue;
        }
        if c == '"' || c == '\'' {
            i += 1;
            let mut s = String::new();
            loop {
                let d = match chars.get(i) {
                    None | Some('\n') => return fail(at, "unterminated string"),
                    Some(d) => *d,
                };
                if d == c {
                    break;
                }
                if d == '\\' {
                    let simple = match chars.get(i + 1) {
                        Some('\\') => Some('\\'),
                        Some('"') => Some('"'),
                        Some('\'') => Some('\''),
                        Some('n') => Some('\n'),
                        Some('r') => Some('\r'),
                        Some('t') => Some('\t'),
                        _ => None,
                    };
                    if let Some(e) = simple {
                        s.push(e);
                        i += 2;
                        continue;
                    }
                    if chars.get(i + 1) == Some(&'u') {
                        let hex = chars.get(i + 2..i + 6).filter(|h| h.iter().all(char::is_ascii_hexdigit));
                        let code = hex.and_then(|h| u32::from_str_radix(&h.iter().collect::<String>(), 16).ok()).and_then(char::from_u32);
                        match code {
                            Some(ch) => {
                                s.push(ch);
                                i += 6;
                                continue;
                            }
                            None => return fail(i, "invalid escape"),
                        }
                    }
                    return fail(i, "invalid escape");
                }
                s.push(d);
                i += 1;
            }
            i += 1;
            out.push(Token { t: Tok::Str(s), at });
            continue;
        }
        if let Some(next) = chars.get(i + 1) {
            let two: String = [c, *next].iter().collect();
            if let Some(op) = OPS2.iter().find(|o| **o == two) {
                out.push(Token { t: Tok::Op(op), at });
                i += 2;
                continue;
            }
        }
        let one = c.to_string();
        if let Some(op) = OPS1.iter().find(|o| **o == one) {
            out.push(Token { t: Tok::Op(op), at });
            i += 1;
            continue;
        }
        return fail(at, format!("unexpected character \"{c}\""));
    }
    out.push(Token { t: Tok::Eof, at: chars.len() });
    Ok(out)
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// A parsed expression (`at` = the 0-based code point of its operator, call,
/// field or literal).
#[derive(Debug, Clone)]
pub enum Node {
    Lit(usize, Value),
    Id(usize, String),
    List(usize, Vec<Node>),
    Map(usize, Vec<(Node, Node)>),
    Field(usize, Box<Node>, String),
    Index(usize, Box<Node>, Box<Node>),
    Unary(usize, &'static str, Box<Node>),
    Binary(usize, &'static str, Box<Node>, Box<Node>),
    Cond(usize, Box<Node>, Box<Node>, Box<Node>),
    Call(usize, String, Option<Box<Node>>, Vec<Node>),
    /// `has(target.name)`: the field's column, its target and name.
    Has(usize, usize, Box<Node>, String),
    Comp(usize, String, Box<Node>, String, Box<Node>),
}

impl Node {
    fn at(&self) -> usize {
        match self {
            Node::Lit(a, _) | Node::Id(a, _) | Node::List(a, _) | Node::Map(a, _) | Node::Field(a, ..) | Node::Index(a, ..) | Node::Unary(a, ..) | Node::Binary(a, ..) | Node::Cond(a, ..) | Node::Call(a, ..) | Node::Has(a, ..) | Node::Comp(a, ..) => *a,
        }
    }
}

const RELATIONS: &[&str] = &["==", "!=", "<", "<=", ">", ">="];

struct Parser {
    toks: Vec<Token>,
    i: usize,
    depth: usize,
    comps: usize,
}

fn describe(t: &Token) -> String {
    match &t.t {
        Tok::Eof => "end".into(),
        Tok::Str(_) => "string".into(),
        Tok::Num(text, _) => format!("\"{text}\""),
        Tok::Id(v) => format!("\"{v}\""),
        Tok::Op(v) => format!("\"{v}\""),
    }
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.toks[self.i]
    }
    fn is_op(&self, v: &str) -> bool {
        matches!(&self.peek().t, Tok::Op(o) if *o == v)
    }
    fn eat(&mut self, v: &str) -> Fail<usize> {
        if !self.is_op(v) {
            return fail(self.peek().at, format!("expected \"{v}\""));
        }
        self.i += 1;
        Ok(self.toks[self.i - 1].at)
    }
    fn op_here(&self) -> Option<&'static str> {
        match &self.peek().t {
            Tok::Op(o) => Some(o),
            _ => None,
        }
    }

    fn parse(&mut self) -> Fail<Node> {
        let node = self.expr()?;
        let t = self.peek();
        if t.t != Tok::Eof {
            return fail(t.at, format!("unexpected {}", describe(t)));
        }
        Ok(node)
    }

    fn expr(&mut self) -> Fail<Node> {
        crate::deep(|| self.expr_here())
    }
    fn expr_here(&mut self) -> Fail<Node> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return fail(self.peek().at, format!("nests deeper than {MAX_DEPTH}"));
        }
        let test = self.or()?;
        let mut node = test;
        if self.is_op("?") {
            let at = self.eat("?")?;
            let then = self.or()?;
            self.eat(":")?;
            let otherwise = self.expr()?;
            node = Node::Cond(at, Box::new(node), Box::new(then), Box::new(otherwise));
        }
        self.depth -= 1;
        Ok(node)
    }
    fn or(&mut self) -> Fail<Node> {
        let mut left = self.and()?;
        while self.is_op("||") {
            let at = self.eat("||")?;
            left = Node::Binary(at, "||", Box::new(left), Box::new(self.and()?));
        }
        Ok(left)
    }
    fn and(&mut self) -> Fail<Node> {
        let mut left = self.relation()?;
        while self.is_op("&&") {
            let at = self.eat("&&")?;
            left = Node::Binary(at, "&&", Box::new(left), Box::new(self.relation()?));
        }
        Ok(left)
    }
    fn relation(&mut self) -> Fail<Node> {
        let mut left = self.addition()?;
        loop {
            let t = self.peek();
            let at = t.at;
            let op: &'static str = match &t.t {
                Tok::Op(o) if RELATIONS.contains(o) => o,
                Tok::Id(v) if v == "in" => "in",
                _ => return Ok(left),
            };
            self.i += 1;
            left = Node::Binary(at, op, Box::new(left), Box::new(self.addition()?));
        }
    }
    fn addition(&mut self) -> Fail<Node> {
        let mut left = self.multiplication()?;
        while let Some(op @ ("+" | "-")) = self.op_here() {
            let at = self.peek().at;
            self.i += 1;
            left = Node::Binary(at, op, Box::new(left), Box::new(self.multiplication()?));
        }
        Ok(left)
    }
    fn multiplication(&mut self) -> Fail<Node> {
        let mut left = self.unary()?;
        while let Some(op @ ("*" | "/" | "%")) = self.op_here() {
            let at = self.peek().at;
            self.i += 1;
            left = Node::Binary(at, op, Box::new(left), Box::new(self.unary()?));
        }
        Ok(left)
    }
    fn unary(&mut self) -> Fail<Node> {
        let mut ops: Vec<(usize, &'static str)> = Vec::new();
        while let Some(op @ ("!" | "-")) = self.op_here() {
            ops.push((self.peek().at, op));
            self.i += 1;
        }
        let mut node = self.member()?;
        for (at, op) in ops.into_iter().rev() {
            node = Node::Unary(at, op, Box::new(node));
        }
        Ok(node)
    }
    fn args(&mut self) -> Fail<Vec<Node>> {
        self.eat("(")?;
        let mut out = Vec::new();
        if !self.is_op(")") {
            out.push(self.expr()?);
            while self.is_op(",") {
                self.i += 1;
                out.push(self.expr()?);
            }
        }
        self.eat(")")?;
        Ok(out)
    }
    fn member(&mut self) -> Fail<Node> {
        let mut node = self.primary()?;
        loop {
            if self.is_op(".") {
                self.i += 1;
                let name_tok = self.peek().clone();
                let Tok::Id(name) = name_tok.t else {
                    return fail(name_tok.at, "expected a field name");
                };
                self.i += 1;
                if !self.is_op("(") {
                    node = Node::Field(name_tok.at, Box::new(node), name);
                    continue;
                }
                if COMPREHENSIONS.contains(&name.as_str()) {
                    if self.comps > 0 {
                        return fail(name_tok.at, "comprehensions do not nest");
                    }
                    self.comps += 1;
                    let mut args = self.args()?;
                    self.comps -= 1;
                    if args.len() != 2 || !matches!(args[0], Node::Id(..)) {
                        return fail(name_tok.at, format!("{name} takes a variable name and an expression"));
                    }
                    let body = args.pop().unwrap();
                    let Node::Id(_, v) = args.pop().unwrap() else { unreachable!() };
                    node = Node::Comp(name_tok.at, name, Box::new(node), v, Box::new(body));
                    continue;
                }
                let args = self.args()?;
                node = Node::Call(name_tok.at, name, Some(Box::new(node)), args);
                continue;
            }
            if self.is_op("[") {
                let at = self.eat("[")?;
                let index = self.expr()?;
                self.eat("]")?;
                node = Node::Index(at, Box::new(node), Box::new(index));
                continue;
            }
            return Ok(node);
        }
    }
    fn primary(&mut self) -> Fail<Node> {
        let t = self.peek().clone();
        match t.t {
            Tok::Num(_, n) => {
                self.i += 1;
                Ok(Node::Lit(t.at, json::number(n)))
            }
            Tok::Str(s) => {
                self.i += 1;
                Ok(Node::Lit(t.at, Value::String(s)))
            }
            Tok::Id(v) => {
                if v == "true" || v == "false" {
                    self.i += 1;
                    return Ok(Node::Lit(t.at, Value::Bool(v == "true")));
                }
                if v == "null" {
                    self.i += 1;
                    return Ok(Node::Lit(t.at, Value::Null));
                }
                if v == "in" || RESERVED.contains(&v.as_str()) {
                    return fail(t.at, format!("\"{v}\" is reserved"));
                }
                self.i += 1;
                if !self.is_op("(") {
                    return Ok(Node::Id(t.at, v));
                }
                let mut args = self.args()?;
                if v == "has" {
                    if args.len() != 1 || !matches!(args[0], Node::Field(..)) {
                        return fail(t.at, "has takes a field selection like has(a.b)");
                    }
                    let Node::Field(fat, target, name) = args.pop().unwrap() else { unreachable!() };
                    return Ok(Node::Has(t.at, fat, target, name));
                }
                Ok(Node::Call(t.at, v, None, args))
            }
            Tok::Op("(") => {
                self.i += 1;
                let node = self.expr()?;
                self.eat(")")?;
                Ok(node)
            }
            Tok::Op("[") => {
                self.i += 1;
                let mut items = Vec::new();
                while !self.is_op("]") {
                    items.push(self.expr()?);
                    if !self.is_op(",") {
                        break;
                    }
                    self.i += 1;
                }
                self.eat("]")?;
                Ok(Node::List(t.at, items))
            }
            Tok::Op("{") => {
                self.i += 1;
                let mut entries = Vec::new();
                while !self.is_op("}") {
                    let key = self.expr()?;
                    self.eat(":")?;
                    entries.push((key, self.expr()?));
                    if !self.is_op(",") {
                        break;
                    }
                    self.i += 1;
                }
                self.eat("}")?;
                Ok(Node::Map(t.at, entries))
            }
            Tok::Eof => fail(t.at, "expected an expression"),
            _ => fail(t.at, format!("unexpected {}", describe(&t))),
        }
    }
}

/// The parsed expression, or the first problem with it (`column N: …`; over
/// [`MAX_LENGTH`] characters: no column).
pub fn parse_expression(source: &str) -> Result<Node, String> {
    let chars: Vec<char> = source.chars().collect();
    if chars.len() > MAX_LENGTH {
        return Err(format!("longer than {MAX_LENGTH} characters"));
    }
    let toks = lex(&chars)?;
    Parser { toks, i: 0, depth: 0, comps: 0 }.parse()
}

// ---------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------

pub fn type_of_value(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "list",
        Value::Object(_) => "map",
    }
}

/// Deep JSON equality, numbers by value.
pub fn expr_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| expr_equal(p, q)),
        (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| expr_equal(v, w))),
        _ => a == b,
    }
}

/// A decimal number spelled in a string (`" 7.5 "`, `-1e3`), else `None`.
pub fn decimal_text(text: &str) -> Option<f64> {
    let t = text.trim_matches(|c| matches!(c, ' ' | '\t' | '\n' | '\r'));
    let b = t.as_bytes();
    let mut i = 0;
    if b.first() == Some(&b'-') {
        i += 1;
    }
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let int_digits = i - int_start;
    if int_digits > 0 {
        if i < b.len() && b[i] == b'.' {
            i += 1;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
        }
    } else {
        if i >= b.len() || b[i] != b'.' {
            return None;
        }
        i += 1;
        let f = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == f {
            return None;
        }
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let e = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == e {
            return None;
        }
    }
    if i != b.len() {
        return None;
    }
    t.parse::<f64>().ok().filter(|n| n.is_finite())
}

fn js_round(v: f64) -> f64 {
    (v + 0.5).floor()
}

fn number_text(n: f64) -> String {
    json::number_to_string(n)
}

fn as_num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Dates
// ---------------------------------------------------------------------------

const MS_DAY: f64 = 86_400_000.0;
const DATE_UNITS: &[&str] = &["day", "week", "month", "quarter", "year"];

/// Days since 1970-01-01 (proleptic Gregorian).
pub fn day_number(day: CalendarDay) -> i64 {
    let y = if day.m <= 2 { day.y - 1 } else { day.y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (day.m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day.d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The calendar day `n` days after 1970-01-01.
pub fn day_from_number(n: i64) -> CalendarDay {
    let z = n + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    CalendarDay { y: era * 400 + yoe + i64::from(m <= 2), m, d }
}

fn local_day(ms: f64, clock: &ExpressionClock) -> CalendarDay {
    day_from_number(((ms + clock.offset_at(ms) * 60_000.0) / MS_DAY).floor() as i64)
}

fn checked_day(day: CalendarDay, at: usize) -> Fail<Value> {
    if day.y < 0 || day.y > 9999 {
        return fail(at, "date out of range");
    }
    Ok(Value::String(format_day(day)))
}

fn day_arg(v: &Value, name: &str, at: usize, clock: &ExpressionClock) -> Fail<CalendarDay> {
    let Value::String(s) = v else {
        return fail(at, format!("{name} needs a date, got {}", type_of_value(v)));
    };
    if s.chars().count() == 10 {
        if let Some(day) = parse_day(s) {
            return Ok(day);
        }
    } else if parse_day(s).is_some() {
        if let Some(parsed) = parse_date_value(v) {
            if !parsed.date_only {
                return Ok(local_day(parsed.ms, clock));
            }
        }
    }
    fail(at, format!("\"{s}\" is not a date"))
}

fn time_of_day(s: &str) -> Option<f64> {
    let b = s.as_bytes();
    let two = |i: usize| (b[i].is_ascii_digit() && b[i + 1].is_ascii_digit()).then(|| f64::from((b[i] - b'0') * 10 + (b[i + 1] - b'0')));
    let (h, m, sec) = match b.len() {
        5 if b[2] == b':' => (two(0)?, two(3)?, 0.0),
        8 if b[2] == b':' && b[5] == b':' => (two(0)?, two(3)?, two(6)?),
        _ => return None,
    };
    Some(h * 3600.0 + m * 60.0 + sec).filter(|_| h < 24.0 && m < 60.0 && sec < 60.0)
}

fn is_time_shape(s: &str) -> bool {
    let b = s.as_bytes();
    let dd = |i: usize| b[i].is_ascii_digit() && b[i + 1].is_ascii_digit();
    match b.len() {
        5 => b[2] == b':' && dd(0) && dd(3),
        8 => b[2] == b':' && b[5] == b':' && dd(0) && dd(3) && dd(6),
        _ => false,
    }
}

fn instant_arg(v: &Value, at: usize, clock: &ExpressionClock) -> Fail<(f64, bool)> {
    let Value::String(s) = v else {
        return fail(at, format!("dateDiff needs dates or times, got {}", type_of_value(v)));
    };
    if is_time_shape(s) {
        if let Some(secs) = time_of_day(s) {
            return Ok((secs * 1000.0, true));
        }
    } else if s.chars().count() == 10 {
        if let Some(day) = parse_day(s) {
            let utc = day_number(day) as f64 * MS_DAY;
            return Ok((utc - clock.offset_at(utc) * 60_000.0, false));
        }
    } else if parse_day(s).is_some() {
        if let Some(parsed) = parse_date_value(v) {
            if !parsed.date_only {
                return Ok((parsed.ms, false));
            }
        }
    }
    fail(at, format!("\"{s}\" is not a date or a time"))
}

fn unit_arg(v: &Value, name: &str, at: usize) -> Fail<String> {
    let units = function(name).and_then(|s| s.units).unwrap_or(&[]);
    match v {
        Value::String(s) if units.contains(&s.as_str()) => Ok(s.clone()),
        Value::String(s) => fail(at, format!("unknown unit \"{s}\" ({})", units.join(", "))),
        other => fail(at, format!("unknown unit {} ({})", type_of_value(other), units.join(", "))),
    }
}

fn add_months(day: CalendarDay, n: i64) -> CalendarDay {
    let total = day.y * 12 + (day.m - 1) + n;
    let y = total.div_euclid(12);
    let m = total - y * 12 + 1;
    CalendarDay { y, m, d: day.d.min(days_in_month(y, m)) }
}

fn start_of(day: CalendarDay, unit: &str) -> CalendarDay {
    match unit {
        "week" => day_from_number(day_number(day) - ((weekday_of(day.y, day.m, day.d) as i64 + 6) % 7)),
        "month" => CalendarDay { d: 1, ..day },
        "quarter" => CalendarDay { y: day.y, m: (day.m - 1) / 3 * 3 + 1, d: 1 },
        "year" => CalendarDay { y: day.y, m: 1, d: 1 },
        _ => day,
    }
}

fn end_of(day: CalendarDay, unit: &str) -> CalendarDay {
    match unit {
        "week" => day_from_number(day_number(start_of(day, "week")) + 6),
        "month" => CalendarDay { d: days_in_month(day.y, day.m), ..day },
        "quarter" => {
            let m = (day.m - 1) / 3 * 3 + 3;
            CalendarDay { y: day.y, m, d: days_in_month(day.y, m) }
        }
        "year" => CalendarDay { y: day.y, m: 12, d: 31 },
        _ => day,
    }
}

fn months_between(a: CalendarDay, b: CalendarDay) -> i64 {
    let mut m = b.y * 12 + b.m - (a.y * 12 + a.m);
    if m > 0 && b.d < a.d {
        m -= 1;
    }
    if m < 0 && b.d > a.d {
        m += 1;
    }
    m
}

fn format_duration(hours: f64, format: &str) -> String {
    let seconds = format.ends_with(":ss");
    let total = js_round(hours.abs() * if seconds { 3600.0 } else { 60.0 }) as i64;
    let sign = if hours < 0.0 && total > 0 { "-" } else { "" };
    let width = if format.starts_with("hh") { 2 } else { 1 };
    if !seconds {
        return format!("{sign}{:0width$}:{:02}", total / 60, total % 60);
    }
    format!("{sign}{:0width$}:{:02}:{:02}", total / 3600, total % 3600 / 60, total % 60)
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

fn num(v: &Value, name: &str, at: usize) -> Fail<f64> {
    as_num(v).map_or_else(|| fail(at, format!("{name} needs a number, got {}", type_of_value(v))), Ok)
}
fn text<'v>(v: &'v Value, name: &str, at: usize) -> Fail<&'v str> {
    match v {
        Value::String(s) => Ok(s),
        _ => fail(at, format!("{name} needs a string, got {}", type_of_value(v))),
    }
}
fn whole(v: &Value, name: &str, at: usize) -> Fail<f64> {
    let n = num(v, name, at)?;
    if n.fract() != 0.0 {
        return fail(at, format!("{name} needs a whole number, got {}", number_text(n)));
    }
    Ok(n)
}
fn finite(n: f64, at: usize) -> Fail<Value> {
    if n.is_finite() {
        Ok(json::number(n))
    } else {
        fail(at, "number out of range")
    }
}

fn arity(name: &str, sig: &Signature, got: usize, at: usize) -> Fail<()> {
    let required = sig.params.iter().filter(|p| !p.ends_with('?') && !p.ends_with("...")).count();
    let variadic = sig.params.iter().any(|p| p.ends_with("..."));
    let max = if variadic { usize::MAX } else { sig.params.len() };
    let min = if variadic { required.max(1) } else { required };
    if got >= min && got <= max {
        return Ok(());
    }
    let s = |n: usize| format!("{n} argument{}", if n == 1 { "" } else { "s" });
    let want = if variadic {
        format!("at least {}", s(min))
    } else if min == max {
        s(min)
    } else {
        format!("{min} to {}", s(max))
    };
    fail(at, format!("{name} takes {want}, got {got}"))
}

fn spread(args: &[Value], name: &str, at: usize) -> Fail<Vec<f64>> {
    let list: &[Value] = match args {
        [Value::Array(items)] => items,
        _ => args,
    };
    if list.is_empty() {
        return fail(at, format!("{name} needs at least one number"));
    }
    list.iter().map(|v| num(v, name, at)).collect()
}

const NULL: Value = Value::Null;

fn arg(args: &[Value], i: usize) -> &Value {
    args.get(i).unwrap_or(&NULL)
}

fn size_of(v: &Value, at: usize) -> Fail<Value> {
    match v {
        Value::String(s) => Ok(json::number(s.chars().count() as f64)),
        Value::Array(a) => Ok(json::number(a.len() as f64)),
        Value::Object(m) => Ok(json::number(m.len() as f64)),
        _ => fail(at, format!("size needs a string, a list or a map, got {}", type_of_value(v))),
    }
}

fn double_of(v: &Value, at: usize) -> Fail<f64> {
    match v {
        Value::Number(n) => Ok(n.as_f64().unwrap_or(0.0)),
        Value::String(s) => decimal_text(s).map_or_else(|| fail(at, format!("\"{s}\" is not a number")), Ok),
        _ => fail(at, format!("double needs a number or a string, got {}", type_of_value(v))),
    }
}

fn call_function(name: &str, args: &[Value], at: usize, clock: &ExpressionClock) -> Fail<Value> {
    let a = |i: usize| arg(args, i);
    Ok(match name {
        "size" => return size_of(a(0), at),
        "string" => Value::String(match a(0) {
            Value::Null => "null".into(),
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => number_text(n.as_f64().unwrap_or(0.0)),
            Value::String(s) => s.clone(),
            other => return fail(at, format!("string cannot convert a {}", type_of_value(other))),
        }),
        "double" => json::number(double_of(a(0), at)?),
        "int" => json::number(double_of(a(0), at)?.trunc()),
        "mul" => return finite(num(a(0), "mul", at)? * num(a(1), "mul", at)?, at),
        "div" => {
            let d = num(a(1), "div", at)?;
            if d == 0.0 {
                return fail(at, "division by zero");
            }
            return finite(num(a(0), "div", at)? / d, at);
        }
        "mod" => {
            let d = num(a(1), "mod", at)?;
            if d == 0.0 {
                return fail(at, "modulo by zero");
            }
            return finite(num(a(0), "mod", at)? % d, at);
        }
        "round" => {
            let n = num(a(0), "round", at)?;
            if a(1).is_null() {
                json::number(js_round(n))
            } else {
                let d = whole(a(1), "round", at)?;
                if !(0.0..=10.0).contains(&d) {
                    return fail(at, format!("round takes 0 to 10 digits, got {}", number_text(d)));
                }
                let f = 10f64.powi(d as i32);
                json::number(js_round(n * f) / f)
            }
        }
        "floor" => json::number(num(a(0), "floor", at)?.floor()),
        "ceil" => json::number(num(a(0), "ceil", at)?.ceil()),
        "abs" => json::number(num(a(0), "abs", at)?.abs()),
        "min" => json::number(spread(args, "min", at)?.into_iter().fold(f64::INFINITY, |m, x| if x < m { x } else { m })),
        "max" => json::number(spread(args, "max", at)?.into_iter().fold(f64::NEG_INFINITY, |m, x| if x > m { x } else { m })),
        "today" => Value::String(format_day(local_day(clock.now_ms(), clock))),
        "now" => {
            let ms = (clock.now_ms() / 1000.0).floor() * 1000.0;
            let offset = clock.offset_at(ms) as i64;
            let local = ms + offset as f64 * 60_000.0;
            let days = (local / MS_DAY).floor();
            let secs = ((local - days * MS_DAY) / 1000.0).floor() as i64;
            let o = offset.abs();
            Value::String(format!(
                "{}T{:02}:{:02}:{:02}{}{:02}:{:02}",
                format_day(day_from_number(days as i64)),
                secs / 3600,
                secs % 3600 / 60,
                secs % 60,
                if offset < 0 { "-" } else { "+" },
                o / 60,
                o % 60
            ))
        }
        "dateAdd" => {
            let day = day_arg(a(0), "dateAdd", at, clock)?;
            let k = whole(a(1), "dateAdd", at)? as i64;
            let u = unit_arg(a(2), "dateAdd", at)?;
            return match u.as_str() {
                "day" => checked_day(day_from_number(day_number(day) + k), at),
                "week" => checked_day(day_from_number(day_number(day) + k * 7), at),
                "quarter" => checked_day(add_months(day, k * 3), at),
                "year" => checked_day(add_months(day, k * 12), at),
                _ => checked_day(add_months(day, k), at),
            };
        }
        "dateDiff" => {
            let u = unit_arg(a(2), "dateDiff", at)?;
            if !DATE_UNITS.contains(&u.as_str()) {
                let (x, xt) = instant_arg(a(0), at, clock)?;
                let (y, yt) = instant_arg(a(1), at, clock)?;
                if xt != yt {
                    return fail(at, "dateDiff compares two times or two dates");
                }
                let unit = match u.as_str() {
                    "hour" => 3_600_000.0,
                    "minute" => 60_000.0,
                    _ => 1000.0,
                };
                return Ok(json::number(((y - x) / unit).trunc()));
            }
            let x = day_arg(a(0), "dateDiff", at, clock)?;
            let y = day_arg(a(1), "dateDiff", at, clock)?;
            let days = day_number(y) - day_number(x);
            let months = months_between(x, y);
            json::number(match u.as_str() {
                "day" => days as f64,
                "week" => (days as f64 / 7.0).trunc(),
                "month" => months as f64,
                "quarter" => (months as f64 / 3.0).trunc(),
                _ => (months as f64 / 12.0).trunc(),
            })
        }
        "startOf" => {
            let day = day_arg(a(0), "startOf", at, clock)?;
            Value::String(format_day(start_of(day, &unit_arg(a(1), "startOf", at)?)))
        }
        "endOf" => {
            let day = day_arg(a(0), "endOf", at, clock)?;
            Value::String(format_day(end_of(day, &unit_arg(a(1), "endOf", at)?)))
        }
        "weekday" => {
            let d = day_arg(a(0), "weekday", at, clock)?;
            json::number(match weekday_of(d.y, d.m, d.d) {
                0 => 7.0,
                n => n as f64,
            })
        }
        "isoWeek" => {
            let d = day_arg(a(0), "isoWeek", at, clock)?;
            json::number(iso_week(d.y, d.m, d.d).1 as f64)
        }
        "hours" => json::number(num(a(0), "hours", at)? / 60.0),
        "formatDuration" => {
            let hours = num(a(0), "formatDuration", at)?;
            let f = if a(1).is_null() { DURATION_FORMATS[0] } else { text(a(1), "formatDuration", at)? };
            if !DURATION_FORMATS.contains(&f) {
                return fail(at, format!("unknown duration format \"{f}\" ({})", DURATION_FORMATS.join(", ")));
            }
            Value::String(format_duration(hours, f))
        }
        _ => return fail(at, format!("unknown function \"{name}\"")),
    })
}

fn call_method(name: &str, target: &Value, args: &[Value], at: usize) -> Fail<Value> {
    let s = |n: &str| -> Fail<(String, String)> { Ok((text(target, n, at)?.to_string(), text(arg(args, 0), n, at)?.to_string())) };
    Ok(match name {
        "size" => return size_of(target, at),
        "contains" => {
            let (t, x) = s("contains")?;
            Value::Bool(t.contains(&x))
        }
        "startsWith" => {
            let (t, x) = s("startsWith")?;
            Value::Bool(t.starts_with(&x))
        }
        "endsWith" => {
            let (t, x) = s("endsWith")?;
            Value::Bool(t.ends_with(&x))
        }
        _ => return fail(at, format!("unknown method \"{name}\"")),
    })
}

// ---------------------------------------------------------------------------
// Evaluator
// ---------------------------------------------------------------------------

struct State<'c> {
    steps: usize,
    clock: &'c ExpressionClock,
}

type Scope = HashMap<String, Value>;

fn field(target: &Value, name: &str, at: usize) -> Fail<Value> {
    match target {
        Value::Object(m) => Ok(m.get(name).cloned().unwrap_or(Value::Null)),
        _ => fail(at, format!("no field \"{name}\" on {}", type_of_value(target))),
    }
}

fn evaluate(n: &Node, scope: &Scope, st: &mut State) -> Fail<Value> {
    crate::deep(|| evaluate_here(n, scope, st))
}

fn evaluate_here(n: &Node, scope: &Scope, st: &mut State) -> Fail<Value> {
    st.steps += 1;
    if st.steps > MAX_STEPS {
        return fail(n.at(), format!("step limit of {MAX_STEPS} exceeded"));
    }
    match n {
        Node::Lit(_, v) => Ok(v.clone()),
        Node::Id(_, name) => Ok(scope.get(name).cloned().unwrap_or(Value::Null)),
        Node::List(_, items) => Ok(Value::Array(items.iter().map(|x| evaluate(x, scope, st)).collect::<Fail<_>>()?)),
        Node::Map(_, entries) => {
            let mut out = Map::new();
            for (k, v) in entries {
                let key = evaluate(k, scope, st)?;
                let Value::String(key) = key else {
                    return fail(k.at(), format!("a map key must be a string, got {}", type_of_value(&key)));
                };
                if out.contains_key(&key) {
                    return fail(k.at(), format!("duplicate key \"{key}\""));
                }
                let value = evaluate(v, scope, st)?;
                out.insert(key, value);
            }
            Ok(Value::Object(out))
        }
        Node::Field(at, target, name) => field(&evaluate(target, scope, st)?, name, *at),
        Node::Has(_, fat, target, name) => {
            let target = evaluate(target, scope, st)?;
            let Value::Object(m) = &target else {
                return fail(*fat, format!("no field \"{name}\" on {}", type_of_value(&target)));
            };
            Ok(Value::Bool(m.get(name).is_some_and(|v| !v.is_null())))
        }
        Node::Index(at, target, index) => {
            let target = evaluate(target, scope, st)?;
            let index = evaluate(index, scope, st)?;
            match &target {
                Value::Array(items) => {
                    let i = match as_num(&index) {
                        Some(i) if i.fract() == 0.0 => i,
                        Some(i) => return fail(*at, format!("a list index must be a whole number, got {}", number_text(i))),
                        None => return fail(*at, format!("a list index must be a whole number, got {}", type_of_value(&index))),
                    };
                    if i < 0.0 || i >= items.len() as f64 {
                        return fail(*at, format!("index {} out of range ({} items)", number_text(i), items.len()));
                    }
                    Ok(items[i as usize].clone())
                }
                Value::Object(m) => {
                    let Value::String(k) = &index else {
                        return fail(*at, format!("a map key must be a string, got {}", type_of_value(&index)));
                    };
                    Ok(m.get(k).cloned().unwrap_or(Value::Null))
                }
                _ => fail(*at, format!("cannot index a {}", type_of_value(&target))),
            }
        }
        Node::Unary(at, op, arg) => {
            let v = evaluate(arg, scope, st)?;
            if *op == "!" {
                return match v {
                    Value::Bool(b) => Ok(Value::Bool(!b)),
                    _ => fail(*at, format!("! needs a bool, got {}", type_of_value(&v))),
                };
            }
            match as_num(&v) {
                Some(x) => Ok(json::number(-x)),
                None => fail(*at, format!("- needs a number, got {}", type_of_value(&v))),
            }
        }
        Node::Cond(at, test, then, otherwise) => {
            let t = evaluate(test, scope, st)?;
            let Value::Bool(b) = t else {
                return fail(*at, format!("?: needs a bool, got {}", type_of_value(&t)));
            };
            evaluate(if b { then } else { otherwise }, scope, st)
        }
        Node::Binary(at, op, left, right) => binary(*at, op, left, right, scope, st),
        Node::Call(at, name, target, args) => {
            let target = match target {
                Some(t) => Some(evaluate(t, scope, st)?),
                None => None,
            };
            let args: Vec<Value> = args.iter().map(|x| evaluate(x, scope, st)).collect::<Fail<_>>()?;
            if let Some(target) = target {
                let Some(sig) = method(name) else {
                    return fail(*at, format!("unknown method \"{name}\""));
                };
                arity(name, sig, args.len(), *at)?;
                return call_method(name, &target, &args, *at);
            }
            let Some(sig) = function(name) else {
                return fail(*at, format!("unknown function \"{name}\""));
            };
            arity(name, sig, args.len(), *at)?;
            call_function(name, &args, *at, st.clock)
        }
        Node::Comp(at, op, target, var, body) => {
            let target = evaluate(target, scope, st)?;
            let Value::Array(items) = target else {
                return fail(*at, format!("{op} needs a list, got {}", type_of_value(&target)));
            };
            let mut inner = scope.clone();
            let test = |item: Value, inner: &mut Scope, st: &mut State| -> Fail<bool> {
                inner.insert(var.clone(), item);
                match evaluate(body, inner, st)? {
                    Value::Bool(b) => Ok(b),
                    v => fail(body.at(), format!("{op} needs a bool, got {}", type_of_value(&v))),
                }
            };
            match op.as_str() {
                "map" => {
                    let mut out = Vec::with_capacity(items.len());
                    for item in items {
                        inner.insert(var.clone(), item);
                        out.push(evaluate(body, &inner, st)?);
                    }
                    Ok(Value::Array(out))
                }
                "filter" => {
                    let mut out = Vec::new();
                    for item in items {
                        if test(item.clone(), &mut inner, st)? {
                            out.push(item);
                        }
                    }
                    Ok(Value::Array(out))
                }
                "all" => {
                    for item in items {
                        if !test(item, &mut inner, st)? {
                            return Ok(Value::Bool(false));
                        }
                    }
                    Ok(Value::Bool(true))
                }
                _ => {
                    for item in items {
                        if test(item, &mut inner, st)? {
                            return Ok(Value::Bool(true));
                        }
                    }
                    Ok(Value::Bool(false))
                }
            }
        }
    }
}

fn binary(at: usize, op: &str, left: &Node, right: &Node, scope: &Scope, st: &mut State) -> Fail<Value> {
    if op == "&&" || op == "||" {
        let l = evaluate(left, scope, st)?;
        let Value::Bool(lb) = l else {
            return fail(at, format!("{op} needs bools, got {}", type_of_value(&l)));
        };
        if (op == "&&" && !lb) || (op == "||" && lb) {
            return Ok(l);
        }
        let r = evaluate(right, scope, st)?;
        if !r.is_boolean() {
            return fail(at, format!("{op} needs bools, got {}", type_of_value(&r)));
        }
        return Ok(r);
    }
    let l = evaluate(left, scope, st)?;
    let r = evaluate(right, scope, st)?;
    let got = format!("{} and {}", type_of_value(&l), type_of_value(&r));
    match op {
        "==" => Ok(Value::Bool(expr_equal(&l, &r))),
        "!=" => Ok(Value::Bool(!expr_equal(&l, &r))),
        "in" => match &r {
            Value::Array(items) => Ok(Value::Bool(items.iter().any(|x| expr_equal(x, &l)))),
            Value::Object(m) => Ok(Value::Bool(matches!(&l, Value::String(k) if m.contains_key(k)))),
            _ => fail(at, format!("in needs a list or a map, got {}", type_of_value(&r))),
        },
        "<" | "<=" | ">" | ">=" => {
            let c = match (&l, &r) {
                (Value::Number(_), Value::Number(_)) => {
                    let (x, y) = (as_num(&l).unwrap(), as_num(&r).unwrap());
                    if x < y {
                        -1
                    } else if x > y {
                        1
                    } else {
                        0
                    }
                }
                (Value::String(x), Value::String(y)) => x.cmp(y) as i32,
                _ => return fail(at, format!("{op} compares numbers or strings, got {got}")),
            };
            Ok(Value::Bool(match op {
                "<" => c < 0,
                "<=" => c <= 0,
                ">" => c > 0,
                _ => c >= 0,
            }))
        }
        "+" => match (&l, &r) {
            (Value::Number(_), Value::Number(_)) => finite(as_num(&l).unwrap() + as_num(&r).unwrap(), at),
            (Value::String(x), Value::String(y)) => Ok(Value::String(format!("{x}{y}"))),
            (Value::Array(x), Value::Array(y)) => Ok(Value::Array(x.iter().chain(y).cloned().collect())),
            _ => fail(at, format!("+ needs two numbers, strings or lists, got {got}")),
        },
        _ => {
            let (Some(x), Some(y)) = (as_num(&l), as_num(&r)) else {
                return fail(at, format!("{op} needs numbers, got {got}"));
            };
            match op {
                "-" => finite(x - y, at),
                "*" => finite(x * y, at),
                _ if y == 0.0 => fail(at, if op == "/" { "division by zero" } else { "modulo by zero" }),
                "/" => finite(x / y, at),
                _ => Ok(json::number(x % y)),
            }
        }
    }
}

/// Evaluate `source` over `vars` (a missing name reads null): the value, or
/// why it has none.
pub fn eval_expression(source: &str, vars: &Map<String, Value>, clock: &ExpressionClock) -> Result<Value, String> {
    let ast = parse_expression(source)?;
    eval_parsed(&ast, vars, clock)
}

/// Evaluate an already parsed expression.
pub fn eval_parsed(ast: &Node, vars: &Map<String, Value>, clock: &ExpressionClock) -> Result<Value, String> {
    let scope: Scope = vars.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    evaluate(ast, &scope, &mut State { steps: 0, clock })
}

/// `{value}` or `{error}`, as the fixture spells an evaluation.
pub fn eval_json(source: &str, vars: &Map<String, Value>, clock: &ExpressionClock) -> Value {
    match eval_expression(source, vars, clock) {
        Ok(v) => serde_json::json!({ "value": v }),
        Err(e) => serde_json::json!({ "error": e }),
    }
}

// ---------------------------------------------------------------------------
// Type checker
// ---------------------------------------------------------------------------

/// A static type: bool number string null dyn, `list<T>`, map, or a record
/// kind (its declared fields; the row keys are strings).
#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    Bool,
    Number,
    String,
    Null,
    Dyn,
    Map,
    List(Box<Ty>),
    Kind(String, IndexMap<String, Ty>),
}

impl Ty {
    pub fn name(&self) -> String {
        match self {
            Ty::Bool => "bool".into(),
            Ty::Number => "number".into(),
            Ty::String => "string".into(),
            Ty::Null => "null".into(),
            Ty::Dyn => "dyn".into(),
            Ty::Map => "map".into(),
            Ty::List(of) => format!("list<{}>", of.name()),
            Ty::Kind(name, _) => name.clone(),
        }
    }
    fn tag(&self) -> &'static str {
        match self {
            Ty::Bool => "bool",
            Ty::Number => "number",
            Ty::String => "string",
            Ty::Null => "null",
            Ty::Dyn => "dyn",
            Ty::Map => "map",
            Ty::List(_) => "list",
            Ty::Kind(..) => "kind",
        }
    }
    fn family(&self) -> &'static str {
        match self {
            Ty::Kind(..) => "map",
            t => t.tag(),
        }
    }
}

fn schema_type(prop: Option<&Value>) -> Ty {
    match prop.and_then(|p| p.get("type")).and_then(Value::as_str) {
        Some("string") => Ty::String,
        Some("number" | "integer") => Ty::Number,
        Some("boolean") => Ty::Bool,
        Some("array") => Ty::List(Box::new(match prop.and_then(|p| p.get("items")) {
            Some(items) => schema_type(Some(items)),
            None => Ty::Dyn,
        })),
        _ => Ty::Dyn,
    }
}

fn kind_type(name: &str, kind: &Value) -> Ty {
    let mut fields: IndexMap<String, Ty> = ROW_KEYS.iter().map(|k| (k.to_string(), Ty::String)).collect();
    if let Some(props) = kind.get("schema").and_then(|s| s.get("properties")).and_then(Value::as_object) {
        for (f, prop) in props {
            fields.insert(f.clone(), schema_type(Some(prop)));
        }
    }
    Ty::Kind(name.to_string(), fields)
}

/// A declared type spelled `bool number string null dyn map list list<T>`
/// or a kind's name (`kinds` = the package's `records`); `None` = unknown.
pub fn parse_type_name(text: &str, kinds: &Map<String, Value>) -> Option<Ty> {
    match text {
        "bool" => return Some(Ty::Bool),
        "number" => return Some(Ty::Number),
        "string" => return Some(Ty::String),
        "null" => return Some(Ty::Null),
        "dyn" => return Some(Ty::Dyn),
        "map" => return Some(Ty::Map),
        "list" => return Some(Ty::List(Box::new(Ty::Dyn))),
        _ => {}
    }
    if let Some(inner) = text.strip_prefix("list<").and_then(|t| t.strip_suffix('>')).filter(|t| !t.is_empty()) {
        return parse_type_name(inner, kinds).map(|t| Ty::List(Box::new(t)));
    }
    kinds.get(text).map(|k| kind_type(text, k))
}

/// The static type of a JSON value (a template's literal var).
pub fn type_of_literal(v: &Value) -> Ty {
    match v {
        Value::Null => Ty::Null,
        Value::Bool(_) => Ty::Bool,
        Value::Number(_) => Ty::Number,
        Value::String(_) => Ty::String,
        Value::Array(items) => Ty::List(Box::new(join(items.iter().map(type_of_literal).collect()))),
        Value::Object(_) => Ty::Map,
    }
}

fn join(types: Vec<Ty>) -> Ty {
    let had = !types.is_empty();
    let real: Vec<Ty> = types.into_iter().filter(|t| *t != Ty::Null).collect();
    match real.first() {
        None => {
            if had {
                Ty::Null
            } else {
                Ty::Dyn
            }
        }
        Some(first) => {
            let n = first.name();
            if real.iter().all(|t| t.name() == n) {
                first.clone()
            } else {
                Ty::Dyn
            }
        }
    }
}

fn strip_suffix(param: &str) -> &str {
    param.strip_suffix('?').or_else(|| param.strip_suffix("...")).unwrap_or(param)
}

fn fits(t: &Ty, param: &str) -> bool {
    if matches!(t, Ty::Dyn | Ty::Null) {
        return true;
    }
    strip_suffix(param).split('|').any(|want| match want {
        "dyn" => true,
        "unit" | "format" => *t == Ty::String,
        "map" => matches!(t, Ty::Map | Ty::Kind(..)),
        "list" => matches!(t, Ty::List(_)),
        w => match (w.strip_prefix("list<").and_then(|x| x.strip_suffix('>')), t) {
            (Some(inner), Ty::List(of)) => fits(of, inner),
            (Some(_), _) => false,
            (None, t) => t.tag() == w,
        },
    })
}

struct Checker {
    issues: Vec<String>,
}

type TypeScope = HashMap<String, Ty>;

impl Checker {
    fn issue(&mut self, at: usize, message: impl AsRef<str>) -> Ty {
        self.issues.push(format!("column {}: {}", at + 1, message.as_ref()));
        Ty::Dyn
    }

    fn check(&mut self, n: &Node, scope: &TypeScope) -> Ty {
        crate::deep(|| self.check_here(n, scope))
    }

    fn check_here(&mut self, n: &Node, scope: &TypeScope) -> Ty {
        match n {
            Node::Lit(_, v) => type_of_literal(v),
            Node::Id(at, name) => match scope.get(name) {
                Some(t) => t.clone(),
                None => self.issue(*at, format!("unknown name \"{name}\"")),
            },
            Node::List(_, items) => {
                let types = items.iter().map(|x| self.check(x, scope)).collect();
                Ty::List(Box::new(join(types)))
            }
            Node::Map(_, entries) => {
                for (k, v) in entries {
                    let key = self.check(k, scope);
                    if !fits(&key, "string") {
                        self.issue(k.at(), format!("a map key must be a string, got {}", key.name()));
                    }
                    self.check(v, scope);
                }
                Ty::Map
            }
            Node::Field(at, target, name) => {
                let t = self.check(target, scope);
                self.field(&t, name, *at)
            }
            Node::Has(_, fat, target, name) => {
                let t = self.check(target, scope);
                self.field(&t, name, *fat);
                Ty::Bool
            }
            Node::Index(at, target, index) => {
                let target = self.check(target, scope);
                let index = self.check(index, scope);
                match &target {
                    Ty::List(of) => {
                        if fits(&index, "number") {
                            (**of).clone()
                        } else {
                            self.issue(*at, format!("a list index must be a whole number, got {}", index.name()))
                        }
                    }
                    Ty::Map | Ty::Kind(..) | Ty::Dyn => {
                        if fits(&index, "string") || target == Ty::Dyn {
                            Ty::Dyn
                        } else {
                            self.issue(*at, format!("a map key must be a string, got {}", index.name()))
                        }
                    }
                    _ => self.issue(*at, format!("cannot index a {}", target.name())),
                }
            }
            Node::Unary(at, op, arg) => {
                let t = self.check(arg, scope);
                let (want, ty) = if *op == "!" { ("bool", Ty::Bool) } else { ("number", Ty::Number) };
                if fits(&t, want) {
                    ty
                } else {
                    self.issue(*at, format!("{op} needs a {want}, got {}", t.name()))
                }
            }
            Node::Cond(at, test, then, otherwise) => {
                let t = self.check(test, scope);
                if !fits(&t, "bool") {
                    self.issue(*at, format!("?: needs a bool, got {}", t.name()));
                }
                let a = self.check(then, scope);
                let b = self.check(otherwise, scope);
                join(vec![a, b])
            }
            Node::Binary(at, op, left, right) => self.binary(*at, op, left, right, scope),
            Node::Call(at, name, target, args) => self.call(*at, name, target.as_deref(), args, scope),
            Node::Comp(at, op, target, var, body) => {
                let target = self.check(target, scope);
                if !matches!(target, Ty::List(_) | Ty::Dyn) {
                    self.issue(*at, format!("{op} needs a list, got {}", target.name()));
                }
                let mut inner = scope.clone();
                inner.insert(
                    var.clone(),
                    match &target {
                        Ty::List(of) => (**of).clone(),
                        _ => Ty::Dyn,
                    },
                );
                let b = self.check(body, &inner);
                if op == "map" {
                    return Ty::List(Box::new(b));
                }
                if !fits(&b, "bool") {
                    self.issue(body.at(), format!("{op} needs a bool, got {}", b.name()));
                }
                if op == "filter" {
                    if matches!(target, Ty::List(_)) {
                        target
                    } else {
                        Ty::List(Box::new(Ty::Dyn))
                    }
                } else {
                    Ty::Bool
                }
            }
        }
    }

    fn field(&mut self, target: &Ty, name: &str, at: usize) -> Ty {
        match target {
            Ty::Kind(kind, fields) => match fields.get(name) {
                Some(t) => t.clone(),
                None => self.issue(at, format!("{kind} has no field \"{name}\"")),
            },
            Ty::Map | Ty::Dyn => Ty::Dyn,
            t => self.issue(at, format!("no field \"{name}\" on {}", t.name())),
        }
    }

    fn binary(&mut self, at: usize, op: &str, left: &Node, right: &Node, scope: &TypeScope) -> Ty {
        let l = self.check(left, scope);
        let r = self.check(right, scope);
        let got = format!("{} and {}", l.name(), r.name());
        let plus = |c: &mut Checker| c.issue(at, format!("+ needs two numbers, strings or lists, got {got}"));
        match op {
            "&&" | "||" => {
                for t in [&l, &r] {
                    if !fits(t, "bool") {
                        self.issue(at, format!("{op} needs bools, got {}", t.name()));
                    }
                }
                Ty::Bool
            }
            "==" | "!=" => {
                if !matches!(l, Ty::Dyn | Ty::Null) && !matches!(r, Ty::Dyn | Ty::Null) && l.family() != r.family() {
                    self.issue(at, format!("{op} compares {} with {}", l.name(), r.name()));
                }
                Ty::Bool
            }
            "in" => {
                if !matches!(r, Ty::List(_) | Ty::Map | Ty::Kind(..) | Ty::Dyn) {
                    self.issue(at, format!("in needs a list or a map, got {}", r.name()));
                }
                Ty::Bool
            }
            "<" | "<=" | ">" | ">=" => {
                let ok = (fits(&l, "number") && fits(&r, "number")) || (fits(&l, "string") && fits(&r, "string"));
                if !ok || l == Ty::Null || r == Ty::Null {
                    self.issue(at, format!("{op} compares numbers or strings, got {got}"));
                }
                Ty::Bool
            }
            "+" => {
                let simple = |t: &Ty| matches!(t, Ty::Number | Ty::String | Ty::List(_));
                if l == Ty::Null || r == Ty::Null {
                    return plus(self);
                }
                if l == Ty::Dyn {
                    return if simple(&r) {
                        r
                    } else if r == Ty::Dyn {
                        Ty::Dyn
                    } else {
                        plus(self)
                    };
                }
                if r == Ty::Dyn {
                    return if simple(&l) { l } else { plus(self) };
                }
                if matches!(l, Ty::Number | Ty::String) && l == r {
                    return l;
                }
                if let (Ty::List(a), Ty::List(b)) = (&l, &r) {
                    return Ty::List(Box::new(join(vec![(**a).clone(), (**b).clone()])));
                }
                plus(self)
            }
            _ => {
                if !fits(&l, "number") || !fits(&r, "number") || l == Ty::Null || r == Ty::Null {
                    return self.issue(at, format!("{op} needs numbers, got {got}"));
                }
                Ty::Number
            }
        }
    }

    fn call(&mut self, at: usize, name: &str, target: Option<&Node>, args: &[Node], scope: &TypeScope) -> Ty {
        let target_ty = target.map(|t| self.check(t, scope));
        let types: Vec<Ty> = args.iter().map(|x| self.check(x, scope)).collect();
        let sig = if target.is_some() { method(name) } else { function(name) };
        let Some(sig) = sig else {
            return self.issue(at, format!("unknown {} \"{name}\"", if target.is_some() { "method" } else { "function" }));
        };
        let returns = parse_type_name(sig.returns, &Map::new()).unwrap_or(Ty::Dyn);
        if let (Some(t), Some(receiver)) = (&target_ty, sig.receiver) {
            if !fits(t, receiver) {
                self.issue(at, format!("{name} needs a {}, got {}", receiver.split('|').collect::<Vec<_>>().join(" or "), t.name()));
            }
        }
        if let Err(e) = arity(name, sig, types.len(), at) {
            self.issues.push(e);
            return returns;
        }
        let variadic = sig.params.iter().find(|p| p.ends_with("..."));
        let list_spread = variadic.is_some() && types.len() == 1 && matches!(types[0], Ty::List(_));
        for (i, t) in types.iter().enumerate() {
            let param: &str = match variadic {
                Some(v) if i + 1 >= sig.params.len() => v,
                _ => sig.params[i],
            };
            let want = if list_spread { format!("list<{}>", param.strip_suffix("...").unwrap_or(param)) } else { param.to_string() };
            if !fits(t, &want) {
                self.issue(args[i].at(), format!("argument {} of {name} must be {}, got {}", i + 1, strip_suffix(&want), t.name()));
            }
            if let Node::Lit(lat, Value::String(s)) = &args[i] {
                if param == "unit" {
                    let units = sig.units.unwrap_or(&[]);
                    if !units.contains(&s.as_str()) {
                        self.issue(*lat, format!("unknown unit \"{s}\" ({})", units.join(", ")));
                    }
                }
                if param.strip_suffix('?').unwrap_or(param) == "format" {
                    let formats = sig.formats.unwrap_or(&[]);
                    if !formats.contains(&s.as_str()) {
                        self.issue(*lat, format!("unknown duration format \"{s}\" ({})", formats.join(", ")));
                    }
                }
            }
        }
        returns
    }
}

/// The check of an expression against declared variable types (`types` =
/// name → a type name, `kinds` = the package's record kinds): its static
/// type name and every problem (`column N: …`; a parse error alone when it
/// does not parse; a bad declaration as `"<name>": unknown type "…"`).
pub fn check_expression(source: &str, types: &Map<String, Value>, kinds: &Map<String, Value>) -> (String, Vec<String>) {
    let ast = match parse_expression(source) {
        Ok(ast) => ast,
        Err(e) => return ("dyn".into(), vec![e]),
    };
    let mut scope = TypeScope::new();
    let mut issues = Vec::new();
    for (name, spelled) in types {
        let spelled = spelled.as_str().unwrap_or("");
        let t = parse_type_name(spelled, kinds);
        if t.is_none() {
            issues.push(format!("\"{name}\": unknown type \"{spelled}\""));
        }
        scope.insert(name.clone(), t.unwrap_or(Ty::Dyn));
    }
    check_parsed(&ast, &scope, issues)
}

/// Check a parsed expression over a typed scope (`issues` = earlier ones).
pub fn check_parsed(ast: &Node, scope: &HashMap<String, Ty>, issues: Vec<String>) -> (String, Vec<String>) {
    let mut checker = Checker { issues };
    let t = checker.check(ast, scope);
    (t.name(), checker.issues)
}

// ---------------------------------------------------------------------------
// The A2UI call form
// ---------------------------------------------------------------------------

fn call_arg(v: Option<&Value>, param: &str) -> Value {
    match v {
        None => Value::Null,
        Some(Value::String(s)) if strip_suffix(param) == "number" => decimal_text(s).map(json::number).unwrap_or_else(|| Value::String(s.clone())),
        Some(v) => v.clone(),
    }
}

/// One call-form function with a clock: the named args mapped onto the
/// function's params; `None` when it fails. `expr` = `{source, vars}`.
pub fn call_expression_function(name: &str, args: &Map<String, Value>, clock: &ExpressionClock) -> Option<Value> {
    if name == "expr" {
        let source = args.get("source")?.as_str()?;
        let empty = Map::new();
        let vars = args.get("vars").and_then(Value::as_object).unwrap_or(&empty);
        return eval_expression(source, vars, clock).ok();
    }
    let sig = function(name)?;
    let call = sig.call?;
    let mut values: Vec<Value> = Vec::new();
    for (i, key) in call.iter().enumerate() {
        let param = sig.params[i];
        let v = args.get(*key);
        if param.ends_with("...") {
            if let Some(Value::Array(items)) = v {
                values.extend(items.iter().map(|x| call_arg(Some(x), param)));
            }
        } else if v.is_some() || !param.ends_with('?') {
            values.push(call_arg(v, param));
        }
    }
    while values.last().is_some_and(Value::is_null) && sig.params.get(values.len() - 1).is_some_and(|p| p.ends_with('?')) {
        values.pop();
    }
    arity(name, sig, values.len(), 0).ok()?;
    call_function(name, &values, 0, clock).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_numbers_round_trip() {
        for n in [-719_468, -1, 0, 1, 20_759, 2_932_896] {
            assert_eq!(day_number(day_from_number(n)), n);
        }
        assert_eq!(day_number(CalendarDay { y: 2026, m: 11, d: 2 }), 20_759);
    }

    #[test]
    fn decimal_text_is_strict() {
        assert_eq!(decimal_text(" 7.5 "), Some(7.5));
        assert_eq!(decimal_text("-1e3"), Some(-1000.0));
        assert_eq!(decimal_text(".5"), Some(0.5));
        assert_eq!(decimal_text("1."), Some(1.0));
        for bad in ["", "0x10", "inf", "1e", "-", "1_0", "NaN"] {
            assert_eq!(decimal_text(bad), None, "{bad}");
        }
    }
}
