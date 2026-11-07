//! Round 9 §9.3: the regex dialect.
//!
//! ONE hand-written parser + safety checker + backtracking matcher; no
//! platform engine. The dialect is the ECMAScript Pattern subset in `u`
//! mode (no other flags). Order of [`regex_test`]:
//!
//! 1. pattern longer than 256 code points → `regex: pattern longer than 256 code points`;
//! 2. parse → `regex: <reason> at <code point offset>`;
//! 3. safety rules R2–R4 (pre-order, R2 then R3 then R4 per node) → `regex: rule <n>`;
//! 4. value coercion (null → `""`, number/boolean → JS text, object/array →
//!    not valid, > 4096 code points → not valid);
//! 5. search match under a 1,000,000-step cap → `regex: too expensive`.
//!
//! The matcher walks the AST in continuation-passing form with an explicit
//! backtrack stack (no recursion but lookaheads, so a 4096-code-point input
//! never deepens the native stack). One step = one attempt of one AST node at
//! one position, counted as `fixtures/regex.json` `$comment` locks it.

use std::rc::Rc;

use serde_json::Value;

pub const MAX_PATTERN_CODE_POINTS: usize = 256;
pub const MAX_VALUE_CODE_POINTS: usize = 4096;
pub const MAX_STEPS: u64 = 1_000_000;

const MAX_CP: u32 = 0x10FFFF;

/// Ok(true/false) = {"valid": true|false}; Err(message) = EXPRESSION_ERROR with that message
/// (e.g. "regex: pattern longer than 256 code points", "regex: <reason> at <offset>", "regex: rule 3", "regex: too expensive").
pub fn regex_test(value: &Value, pattern: &str) -> Result<bool, String> {
    regex_step_count(value, &Value::String(pattern.to_string())).map(|(valid, _)| valid)
}

/// [`regex_test`] for a pattern that may not be a string (`regex: pattern is
/// not a string at 0`), plus the steps the match took (`fixtures/regex.json`
/// `steps`; 0 when no match ran).
pub fn regex_step_count(value: &Value, pattern: &Value) -> Result<(bool, u64), String> {
    let Some(pattern) = pattern.as_str() else { return Err("regex: pattern is not a string at 0".to_string()) };
    let ast = checked_ast(pattern)?;
    let text = match value {
        Value::Null => String::new(),
        Value::Bool(_) | Value::Number(_) => crate::json::to_js_string(value),
        Value::String(s) => s.clone(),
        Value::Array(_) | Value::Object(_) => return Ok((false, 0)),
    };
    let input: Vec<u32> = text.chars().map(|c| c as u32).collect();
    if input.len() > MAX_VALUE_CODE_POINTS {
        return Ok((false, 0));
    }
    let mut m = Matcher { input: &input, steps: 0, cap: MAX_STEPS };
    let valid = m.search(&ast).map_err(|TooExpensive| "regex: too expensive".to_string())?;
    Ok((valid, m.steps))
}

/// Steps 1-3 only (length, parse, safety) — for static checks.
pub fn check_pattern(pattern: &str) -> Result<(), String> {
    checked_ast(pattern).map(|_| ())
}

fn checked_ast(pattern: &str) -> Result<Node, String> {
    let p: Vec<u32> = pattern.chars().map(|c| c as u32).collect();
    if p.len() > MAX_PATTERN_CODE_POINTS {
        return Err(format!("regex: pattern longer than {MAX_PATTERN_CODE_POINTS} code points"));
    }
    let ast = Parser { p, i: 0 }
        .parse()
        .map_err(|(reason, at)| format!("regex: {reason} at {at}"))?;
    if let Some(rule) = safety(&ast) {
        return Err(format!("regex: rule {rule}"));
    }
    Ok(ast)
}

// ---------------------------------------------------------------------------
// Code point sets: sorted, merged, inclusive ranges over U+0000–U+10FFFF.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Set(Vec<(u32, u32)>);

impl Set {
    fn empty() -> Self {
        Set(Vec::new())
    }

    fn of(ranges: &[(u32, u32)]) -> Self {
        Set::normalized(ranges.to_vec())
    }

    fn single(c: u32) -> Self {
        Set(vec![(c, c)])
    }

    fn normalized(mut v: Vec<(u32, u32)>) -> Self {
        v.sort_unstable();
        let mut out: Vec<(u32, u32)> = Vec::with_capacity(v.len());
        for (a, b) in v {
            if let Some(last) = out.last_mut() {
                if a <= last.1.saturating_add(1) {
                    last.1 = last.1.max(b);
                    continue;
                }
            }
            out.push((a, b));
        }
        Set(out)
    }

    fn union(&self, other: &Set) -> Set {
        let mut v = self.0.clone();
        v.extend_from_slice(&other.0);
        Set::normalized(v)
    }

    fn complement(&self) -> Set {
        let mut out = Vec::new();
        let mut next = 0u32;
        for &(a, b) in &self.0 {
            if a > next {
                out.push((next, a - 1));
            }
            next = b + 1;
        }
        if next <= MAX_CP {
            out.push((next, MAX_CP));
        }
        Set(out)
    }

    fn intersects(&self, other: &Set) -> bool {
        let (mut i, mut j) = (0, 0);
        while i < self.0.len() && j < other.0.len() {
            let (a, b) = self.0[i];
            let (c, d) = other.0[j];
            if a <= d && c <= b {
                return true;
            }
            if b < d {
                i += 1;
            } else {
                j += 1;
            }
        }
        false
    }

    fn contains(&self, c: u32) -> bool {
        let idx = self.0.partition_point(|&(_, b)| b < c);
        idx < self.0.len() && self.0[idx].0 <= c
    }
}

fn digit_set() -> Set {
    Set::of(&[(0x30, 0x39)])
}

fn word_set() -> Set {
    Set::of(&[(0x30, 0x39), (0x41, 0x5A), (0x5F, 0x5F), (0x61, 0x7A)])
}

fn space_set() -> Set {
    Set::of(&[
        (0x09, 0x0D),
        (0x20, 0x20),
        (0xA0, 0xA0),
        (0x1680, 0x1680),
        (0x2000, 0x200A),
        (0x2028, 0x2029),
        (0x202F, 0x202F),
        (0x205F, 0x205F),
        (0x3000, 0x3000),
        (0xFEFF, 0xFEFF),
    ])
}

fn dot_set() -> Set {
    Set::of(&[(0x0A, 0x0A), (0x0D, 0x0D), (0x2028, 0x2029)]).complement()
}

fn is_ascii_word(c: u32) -> bool {
    matches!(c, 0x30..=0x39 | 0x41..=0x5A | 0x5F | 0x61..=0x7A)
}

// ---------------------------------------------------------------------------
// AST

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Assert {
    Start,
    End,
    WordBoundary,
    NotWordBoundary,
}

#[derive(Clone, Debug)]
enum Node {
    /// A leaf: a literal, `.`, a class or a class escape.
    Set(Set),
    Assert(Assert),
    Look { negative: bool, child: Box<Node> },
    /// `(…)` or `(?:…)`: transparent (there are no backreferences).
    Group(Box<Node>),
    Concat(Vec<Node>),
    Alt(Vec<Node>),
    Repeat { child: Box<Node>, min: u32, max: Option<u32>, lazy: bool },
}

// ---------------------------------------------------------------------------
// Parser

type PResult<T> = Result<T, (&'static str, usize)>;

/// A quantifier bound saturates at 2^32 − 1; the order check compares the
/// exact digit strings.
const SATURATE: u32 = u32::MAX;

fn strip_zeros(digits: &str) -> &str {
    let t = digits.trim_start_matches('0');
    if t.is_empty() {
        "0"
    } else {
        t
    }
}

fn bound_value(digits: &str) -> u32 {
    let d = strip_zeros(digits);
    if d.len() > 10 {
        SATURATE
    } else {
        d.parse::<u64>().map_or(SATURATE, |v| v.min(u64::from(SATURATE)) as u32)
    }
}

fn bound_greater(a: &str, b: &str) -> bool {
    let (x, y) = (strip_zeros(a), strip_zeros(b));
    if x.len() != y.len() {
        x.len() > y.len()
    } else {
        x > y
    }
}

fn is_digit(c: Option<u32>) -> bool {
    c.is_some_and(|c| (0x30..=0x39).contains(&c))
}

fn hex_val(c: Option<u32>) -> Option<u32> {
    char::from_u32(c?)?.to_digit(16)
}

fn ch(c: char) -> Option<u32> {
    Some(c as u32)
}

fn class_escape(e: u32) -> Option<Set> {
    Some(match char::from_u32(e)? {
        'd' => digit_set(),
        'D' => digit_set().complement(),
        'w' => word_set(),
        'W' => word_set().complement(),
        's' => space_set(),
        'S' => space_set().complement(),
        _ => return None,
    })
}

enum ClassAtom {
    Cp(u32),
    Set(Set),
}

/// The port of `src/regex.ts parse`: left to right, the first error wins.
struct Parser {
    p: Vec<u32>,
    i: usize,
}

impl Parser {
    fn at(&self, j: usize) -> Option<u32> {
        self.p.get(j).copied()
    }

    fn cur(&self) -> Option<u32> {
        self.at(self.i)
    }

    fn parse(mut self) -> PResult<Node> {
        let root = self.disjunction()?;
        if self.i < self.p.len() {
            return Err(("unmatched )", self.i));
        }
        Ok(root)
    }

    /// `{` at j as `{n}`, `{n,}` or `{n,m}` → its bounds and end.
    fn braces(&self, j: usize) -> Option<(String, Option<String>, usize)> {
        let digits = |mut k: usize| {
            let mut out = String::new();
            while is_digit(self.at(k)) {
                out.push(char::from_u32(self.p[k]).unwrap_or('0'));
                k += 1;
            }
            (out, k)
        };
        let (min, k) = digits(j + 1);
        if min.is_empty() {
            return None;
        }
        if self.at(k) == ch('}') {
            return Some((min.clone(), Some(min), k + 1));
        }
        if self.at(k) != ch(',') {
            return None;
        }
        let (max, k) = digits(k + 1);
        if self.at(k) != ch('}') {
            return None;
        }
        Some((min, (!max.is_empty()).then_some(max), k + 1))
    }

    fn quantifier_ahead(&self) -> bool {
        let c = self.cur();
        c == ch('*') || c == ch('+') || c == ch('?') || (c == ch('{') && self.braces(self.i).is_some())
    }

    fn disjunction(&mut self) -> PResult<Node> {
        let mut branches = vec![self.alternative()?];
        while self.cur() == ch('|') {
            self.i += 1;
            branches.push(self.alternative()?);
        }
        Ok(if branches.len() == 1 { branches.pop().expect("one") } else { Node::Alt(branches) })
    }

    fn alternative(&mut self) -> PResult<Node> {
        let mut items = Vec::new();
        while self.i < self.p.len() && self.cur() != ch('|') && self.cur() != ch(')') {
            items.push(self.term()?);
        }
        Ok(Node::Concat(items))
    }

    fn close_group(&mut self, open: usize) -> PResult<()> {
        if self.cur() != ch(')') {
            return Err(("unmatched (", open));
        }
        self.i += 1;
        Ok(())
    }

    fn term(&mut self) -> PResult<Node> {
        let c = self.cur();
        let (next, after) = (self.at(self.i + 1), self.at(self.i + 2));
        let assertion = if c == ch('^') {
            self.i += 1;
            Some(Node::Assert(Assert::Start))
        } else if c == ch('$') {
            self.i += 1;
            Some(Node::Assert(Assert::End))
        } else if c == ch('\\') && (next == ch('b') || next == ch('B')) {
            self.i += 2;
            Some(Node::Assert(if next == ch('B') { Assert::NotWordBoundary } else { Assert::WordBoundary }))
        } else if c == ch('(') && next == ch('?') && (after == ch('=') || after == ch('!')) {
            let open = self.i;
            self.i += 3;
            let child = self.disjunction()?;
            self.close_group(open)?;
            Some(Node::Look { negative: after == ch('!'), child: Box::new(child) })
        } else {
            None
        };
        if let Some(assertion) = assertion {
            if self.quantifier_ahead() {
                return Err(("quantifier on assertion", self.i));
            }
            return Ok(assertion);
        }
        let atom = self.atom()?;
        let q = self.cur();
        let (min, max) = if q == ch('*') {
            (0, None)
        } else if q == ch('+') {
            (1, None)
        } else if q == ch('?') {
            (0, Some(1))
        } else if let Some((lo, hi, end)) = (q == ch('{')).then(|| self.braces(self.i)).flatten() {
            if hi.as_deref().is_some_and(|h| bound_greater(&lo, h)) {
                return Err(("quantifier range out of order", self.i));
            }
            self.i = end - 1;
            (bound_value(&lo), hi.as_deref().map(bound_value))
        } else {
            return Ok(atom);
        };
        self.i += 1;
        let lazy = self.cur() == ch('?');
        if lazy {
            self.i += 1;
        }
        Ok(Node::Repeat { child: Box::new(atom), min, max, lazy })
    }

    fn atom(&mut self) -> PResult<Node> {
        let at = self.i;
        let c = self.cur().unwrap_or(0);
        let is = |x: char| c == x as u32;
        if is('.') {
            self.i += 1;
            return Ok(Node::Set(dot_set()));
        }
        if is('(') {
            if self.at(at + 1) == ch('?') {
                let kind = self.at(at + 2);
                if kind == ch(':') {
                    self.i += 3;
                } else if kind == ch('<') {
                    let behind = self.at(at + 3) == ch('=') || self.at(at + 3) == ch('!');
                    return Err((if behind { "lookbehind" } else { "named group" }, at));
                } else {
                    return Err(("inline flags", at));
                }
            } else {
                self.i += 1;
            }
            let child = self.disjunction()?;
            self.close_group(at)?;
            return Ok(Node::Group(Box::new(child)));
        }
        if is('[') {
            return self.class();
        }
        if is(']') {
            return Err(("unmatched ]", at));
        }
        if is('}') {
            return Err(("lone }", at));
        }
        if is('{') {
            return Err((if self.braces(at).is_some() { "nothing to repeat" } else { "lone {" }, at));
        }
        if is('*') || is('+') || is('?') {
            return Err(("nothing to repeat", at));
        }
        if is('\\') {
            let Some(e) = self.at(at + 1) else { return Err(("invalid escape", at)) };
            if let Some(set) = class_escape(e) {
                self.i += 2;
                return Ok(Node::Set(set));
            }
            if (0x31..=0x39).contains(&e) || e == 'k' as u32 {
                return Err(("backreference", at));
            }
            return Ok(Node::Set(Set::single(self.char_escape(false)?)));
        }
        self.i += 1;
        Ok(Node::Set(Set::single(c)))
    }

    /// `\` at i (not a class escape, not \b \B, not a backreference) → its code point.
    fn char_escape(&mut self, in_class: bool) -> PResult<u32> {
        let at = self.i;
        let Some(e) = self.at(at + 1) else { return Err(("invalid escape", at)) };
        let simple = match char::from_u32(e) {
            Some('t') => Some(9),
            Some('n') => Some(10),
            Some('v') => Some(11),
            Some('f') => Some(12),
            Some('r') => Some(13),
            _ => None,
        };
        if let Some(v) = simple {
            self.i += 2;
            return Ok(v);
        }
        match char::from_u32(e) {
            Some('0') => {
                if is_digit(self.at(at + 2)) {
                    return Err(("octal escape", at));
                }
                self.i += 2;
                Ok(0)
            }
            Some('c') => Err(("control escape", at)),
            Some('p' | 'P') => Err(("property escape", at)),
            Some('x') => match (hex_val(self.at(at + 2)), hex_val(self.at(at + 3))) {
                (Some(h1), Some(h2)) => {
                    self.i += 4;
                    Ok(h1 * 16 + h2)
                }
                _ => Err(("invalid escape", at)),
            },
            Some('u') => self.unicode_escape(),
            Some(x) if "^$\\.*+?()[]{}|/".contains(x) || (in_class && x == '-') => {
                self.i += 2;
                Ok(e)
            }
            _ => Err(("invalid escape", at)),
        }
    }

    fn hex4(&self, j: usize) -> Option<u32> {
        (0..4).try_fold(0, |v, k| Some(v * 16 + hex_val(self.at(j + k))?))
    }

    fn unicode_escape(&mut self) -> PResult<u32> {
        let at = self.i;
        if self.at(at + 2) == ch('{') {
            let mut k = at + 3;
            let mut v = 0u32;
            let mut digits = 0;
            while let Some(h) = hex_val(self.at(k)) {
                v = (v * 16 + h).min(MAX_CP + 1);
                digits += 1;
                k += 1;
            }
            if digits == 0 || self.at(k) != ch('}') {
                return Err(("invalid escape", at));
            }
            if v > MAX_CP {
                return Err(("code point out of range", at));
            }
            self.i = k + 1;
            return Ok(v);
        }
        let Some(v) = self.hex4(at + 2) else { return Err(("invalid escape", at)) };
        self.i += 6;
        if (0xd800..=0xdbff).contains(&v) && self.cur() == ch('\\') && self.at(self.i + 1) == ch('u') {
            if let Some(t) = self.hex4(self.i + 2).filter(|t| (0xdc00..=0xdfff).contains(t)) {
                self.i += 6;
                return Ok(0x10000 + ((v - 0xd800) << 10) + (t - 0xdc00));
            }
        }
        Ok(v)
    }

    fn class_atom(&mut self) -> PResult<ClassAtom> {
        let at = self.i;
        if self.cur() != ch('\\') {
            self.i += 1;
            return Ok(ClassAtom::Cp(self.p[at]));
        }
        let Some(e) = self.at(at + 1) else { return Err(("invalid escape", at)) };
        if let Some(set) = class_escape(e) {
            self.i += 2;
            return Ok(ClassAtom::Set(set));
        }
        if e == 'b' as u32 {
            self.i += 2;
            return Ok(ClassAtom::Cp(8));
        }
        if (0x31..=0x39).contains(&e) {
            return Err(("octal escape", at));
        }
        Ok(ClassAtom::Cp(self.char_escape(true)?))
    }

    fn class(&mut self) -> PResult<Node> {
        let open = self.i;
        self.i += 1;
        let negative = self.cur() == ch('^');
        if negative {
            self.i += 1;
        }
        let mut ranges: Vec<(u32, u32)> = Vec::new();
        loop {
            if self.i >= self.p.len() {
                return Err(("unmatched [", open));
            }
            if self.cur() == ch(']') {
                self.i += 1;
                break;
            }
            let start = self.i;
            let a = self.class_atom()?;
            if self.cur() == ch('-') && self.i + 1 < self.p.len() && self.at(self.i + 1) != ch(']') {
                self.i += 1;
                let b = self.class_atom()?;
                let (ClassAtom::Cp(lo), ClassAtom::Cp(hi)) = (&a, &b) else { return Err(("class escape in range", start)) };
                if lo > hi {
                    return Err(("range out of order", start));
                }
                ranges.push((*lo, *hi));
            } else {
                match a {
                    ClassAtom::Cp(c) => ranges.push((c, c)),
                    ClassAtom::Set(set) => ranges.extend(set.0),
                }
            }
        }
        let set = Set::normalized(ranges);
        Ok(Node::Set(if negative { set.complement() } else { set }))
    }
}

// ---------------------------------------------------------------------------
// Safety check (R2–R4)

fn chars(n: &Node) -> Set {
    match n {
        Node::Set(s) => s.clone(),
        Node::Assert(_) | Node::Look { .. } => Set::empty(),
        Node::Group(c) | Node::Repeat { child: c, .. } => chars(c),
        Node::Concat(v) | Node::Alt(v) => v.iter().fold(Set::empty(), |acc, x| acc.union(&chars(x))),
    }
}

fn nullable(n: &Node) -> bool {
    match n {
        Node::Set(_) => false,
        Node::Assert(_) | Node::Look { .. } => true,
        Node::Repeat { child, min, .. } => *min == 0 || nullable(child),
        Node::Group(c) => nullable(c),
        Node::Concat(v) => v.iter().all(nullable),
        Node::Alt(v) => v.iter().any(nullable),
    }
}

fn first(n: &Node) -> Set {
    match n {
        Node::Set(s) => s.clone(),
        Node::Assert(_) | Node::Look { .. } => Set::empty(),
        Node::Group(c) | Node::Repeat { child: c, .. } => first(c),
        Node::Alt(v) => v.iter().fold(Set::empty(), |acc, x| acc.union(&first(x))),
        Node::Concat(v) => {
            let mut acc = Set::empty();
            for item in v {
                acc = acc.union(&first(item));
                if !nullable(item) {
                    break;
                }
            }
            acc
        }
    }
}

fn unwrap(mut n: &Node) -> &Node {
    loop {
        n = match n {
            Node::Group(c) => c,
            Node::Concat(v) | Node::Alt(v) if v.len() == 1 => &v[0],
            _ => return n,
        };
    }
}

fn is_unbounded(n: &Node) -> bool {
    matches!(n, Node::Repeat { max: None, .. })
}

fn contains_unbounded(n: &Node) -> bool {
    is_unbounded(n)
        || match n {
            Node::Set(_) | Node::Assert(_) => false,
            Node::Look { child, .. } | Node::Group(child) | Node::Repeat { child, .. } => {
                contains_unbounded(child)
            }
            Node::Concat(v) | Node::Alt(v) => v.iter().any(contains_unbounded),
        }
}

fn rule2(n: &Node) -> bool {
    matches!(n, Node::Repeat { child, max, .. } if max.is_none_or(|m| m > 1) && contains_unbounded(child))
}

fn rule3(n: &Node) -> bool {
    let Node::Repeat { child, max: None, .. } = n else { return false };
    let Node::Alt(branches) = unwrap(child) else { return false };
    let firsts: Vec<Set> = branches.iter().map(first).collect();
    (0..firsts.len()).any(|i| (i + 1..firsts.len()).any(|j| firsts[i].intersects(&firsts[j])))
}

fn rule4(n: &Node) -> bool {
    let Node::Concat(items) = n else { return false };
    for i in 0..items.len() {
        if !is_unbounded(unwrap(&items[i])) {
            continue;
        }
        for j in i + 1..items.len() {
            if is_unbounded(unwrap(&items[j])) && chars(&items[i]).intersects(&chars(&items[j])) {
                return true;
            }
            if !nullable(&items[j]) {
                break;
            }
        }
    }
    false
}

fn any_node(n: &Node, f: fn(&Node) -> bool) -> bool {
    f(n)
        || match n {
            Node::Look { child, .. } | Node::Group(child) | Node::Repeat { child, .. } => any_node(child, f),
            Node::Concat(v) | Node::Alt(v) => v.iter().any(|x| any_node(x, f)),
            Node::Set(_) | Node::Assert(_) => false,
        }
}

/// The lowest rule the pattern breaks, anywhere in the tree.
fn safety(root: &Node) -> Option<u8> {
    [(2, rule2 as fn(&Node) -> bool), (3, rule3), (4, rule4)].into_iter().find(|(_, f)| any_node(root, *f)).map(|(n, _)| n)
}

// ---------------------------------------------------------------------------
// Matcher: ECMAScript backtracking in continuation-passing form, with an
// explicit backtrack stack (the AST walk of `src/regex.ts`, so the step count
// of `fixtures/regex.json` is the same). One step = one `match(node, pos, k)`.

#[derive(Debug)]
struct TooExpensive;

type Next<'a> = Option<Rc<Cont<'a>>>;

enum Cont<'a> {
    Seq { items: &'a [Node], i: usize, next: Next<'a> },
    Rep { q: &'a Node, min: u32, max: Option<u32>, x: usize, next: Next<'a> },
}

#[derive(Clone)]
enum Goal<'a> {
    Match { node: &'a Node, pos: usize, cont: Next<'a> },
    Continue { pos: usize, cont: Next<'a> },
}

/// ECMAScript RepeatMatcher(min, max, greedy, x, c): the next goal; the other
/// choice goes on the backtrack stack.
fn repeat<'a>(q: &'a Node, min: u32, max: Option<u32>, x: usize, next: Next<'a>, stack: &mut Vec<Goal<'a>>) -> Goal<'a> {
    let Node::Repeat { child, lazy, .. } = q else { unreachable!("repeat over a quantifier") };
    if max == Some(0) {
        return Goal::Continue { pos: x, cont: next };
    }
    let atom = Goal::Match { node: child, pos: x, cont: Some(Rc::new(Cont::Rep { q, min, max, x, next: next.clone() })) };
    if min != 0 {
        return atom;
    }
    if *lazy {
        stack.push(atom);
        Goal::Continue { pos: x, cont: next }
    } else {
        stack.push(Goal::Continue { pos: x, cont: next });
        atom
    }
}

struct Matcher<'v> {
    input: &'v [u32],
    steps: u64,
    cap: u64,
}

impl Matcher<'_> {
    fn word_at(&self, i: Option<usize>) -> bool {
        i.and_then(|i| self.input.get(i)).is_some_and(|c| is_ascii_word(*c))
    }

    /// Does `root` match at `start`? A lookahead runs its own (atomic) sub-match.
    fn run(&mut self, root: &Node, start: usize) -> Result<bool, TooExpensive> {
        let mut stack: Vec<Goal> = Vec::new();
        let mut goal = Some(Goal::Match { node: root, pos: start, cont: None });
        while let Some(g) = goal.take() {
            goal = match g {
                Goal::Match { node, pos, cont } => {
                    self.steps += 1;
                    if self.steps > self.cap {
                        return Err(TooExpensive);
                    }
                    match node {
                        Node::Set(set) => {
                            if self.input.get(pos).is_some_and(|c| set.contains(*c)) {
                                Some(Goal::Continue { pos: pos + 1, cont })
                            } else {
                                stack.pop()
                            }
                        }
                        Node::Assert(a) => {
                            let hit = match a {
                                Assert::Start => pos == 0,
                                Assert::End => pos == self.input.len(),
                                Assert::WordBoundary | Assert::NotWordBoundary => {
                                    (self.word_at(pos.checked_sub(1)) != self.word_at(Some(pos))) == (*a == Assert::WordBoundary)
                                }
                            };
                            if hit { Some(Goal::Continue { pos, cont }) } else { stack.pop() }
                        }
                        Node::Look { negative, child } => {
                            if self.run(child, pos)? != *negative { Some(Goal::Continue { pos, cont }) } else { stack.pop() }
                        }
                        Node::Group(child) => Some(Goal::Match { node: child, pos, cont }),
                        Node::Concat(items) => Some(Goal::Continue { pos, cont: Some(Rc::new(Cont::Seq { items, i: 0, next: cont })) }),
                        Node::Alt(branches) => {
                            for b in branches[1..].iter().rev() {
                                stack.push(Goal::Match { node: b, pos, cont: cont.clone() });
                            }
                            Some(Goal::Match { node: &branches[0], pos, cont })
                        }
                        Node::Repeat { min, max, .. } => Some(repeat(node, *min, *max, pos, cont, &mut stack)),
                    }
                }
                Goal::Continue { pos, cont } => match cont.as_deref() {
                    None => return Ok(true),
                    Some(Cont::Seq { items, i, next }) => {
                        if *i == items.len() {
                            Some(Goal::Continue { pos, cont: next.clone() })
                        } else {
                            let tail = Rc::new(Cont::Seq { items, i: i + 1, next: next.clone() });
                            Some(Goal::Match { node: &items[*i], pos, cont: Some(tail) })
                        }
                    }
                    Some(Cont::Rep { q, min, max, x, next }) => {
                        if *min == 0 && pos == *x {
                            stack.pop()
                        } else {
                            Some(repeat(q, min.saturating_sub(1), max.map(|m| m - 1), pos, next.clone(), &mut stack))
                        }
                    }
                },
            };
        }
        Ok(false)
    }

    /// Search semantics: each start offset in order, one shared step budget.
    fn search(&mut self, root: &Node) -> Result<bool, TooExpensive> {
        for start in 0..=self.input.len() {
            if self.run(root, start)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn t(value: &str, pattern: &str) -> bool {
        regex_test(&json!(value), pattern).unwrap_or_else(|e| panic!("{pattern}: {e}"))
    }

    fn err(pattern: &str) -> String {
        check_pattern(pattern).expect_err(pattern)
    }

    #[test]
    fn spec_safety_examples() {
        assert_eq!(err("(a+)+"), "regex: rule 2");
        assert_eq!(err("(?:a*b){2,5}"), "regex: rule 2");
        assert_eq!(err("(a|ab)*c"), "regex: rule 3");
        assert_eq!(err(r"\d+\s*\d+"), "regex: rule 4");
        assert_eq!(err(".*.*"), "regex: rule 4");
        assert!(check_pattern(r"\w+\s+\w+").is_ok());
        assert!(check_pattern(r"^(?=.*\d).{8,}$").is_ok());
    }

    #[test]
    fn safety_details() {
        // R2 reaches into lookaheads; `?` (max 1) is exempt.
        assert_eq!(err("(?:(?=a+)b)+"), "regex: rule 2");
        assert!(check_pattern("(a+)?").is_ok());
        assert!(check_pattern("(?:a{2,3}){2}").is_ok());
        // R2 before R3 on the same node.
        assert_eq!(err("(a+|a)*"), "regex: rule 2");
        // R3: disjoint first sets pass; nullable prefixes widen first().
        assert!(check_pattern("(a|b)*").is_ok());
        assert_eq!(err("(?:x?a|ab)*"), "regex: rule 3");
        assert_eq!(err("(?:x?a|xb)*"), "regex: rule 3");
        assert!(check_pattern("(?:xa|b)+c").is_ok());
        // R3 only for unbounded quantifiers.
        assert!(check_pattern("(a|ab){3}c").is_ok());
        // R4: a non-nullable item in between breaks the pair.
        assert!(check_pattern(r"\d+x\d+").is_ok());
        assert_eq!(err(r"\d+(?:x?)\d+"), "regex: rule 4");
        assert_eq!(err(r"(?:\d+)\s*(\d+)"), "regex: rule 4");
        assert!(check_pattern(r"[a-z]+[0-9]+").is_ok());
        // Nested concatenations are checked too; pre-order reports the outer first.
        assert_eq!(err(r"x(?:a*a*)"), "regex: rule 4");
        assert_eq!(err(r"(a+)+(?:b|bc)*"), "regex: rule 2");
        // Exact intersection across high planes.
        assert!(check_pattern(r"[\u{10000}-\u{10FFFE}]+\u{10FFFF}*").is_ok());
        assert_eq!(err(r"[\u{10000}-\u{10FFFF}]+\u{10FFFF}*"), "regex: rule 4");
        assert_eq!(err(r"\S+\D+"), "regex: rule 4");
    }

    #[test]
    fn validators() {
        let email = r"^[^\s@]+@[^\s@]+\.[^\s@]+$";
        assert!(t("a.b@example.com", email));
        assert!(!t("a.b@example", email));
        assert!(!t("a b@example.com", email));
        assert!(t("12345", "^[0-9]{5}$"));
        assert!(!t("1234", "^[0-9]{5}$"));
        assert!(!t("123456", "^[0-9]{5}$"));
        assert!(t("555-1234", r"^\d{3}-\d{4}$"));
        assert!(!t("5551234", r"^\d{3}-\d{4}$"));
        let pw = r"^(?=.*\d)(?=.*[A-Z]).{8,}$";
        assert!(t("Passw0rdX", pw));
        assert!(!t("password1", pw));
        assert!(!t("Pa1", pw));
        assert!(t("hello world", r"\w+\s+\w+"));
        assert!(t("+43 660 1234567", r"^\+?[0-9 ]{6,20}$"));
    }

    #[test]
    fn search_semantics_and_anchors() {
        assert!(t("xxabcxx", "abc"));
        assert!(!t("xxabcxx", "^abc"));
        assert!(!t("xxabcxx", "abc$"));
        assert!(t("", ""));
        assert!(t("anything", ""));
        assert!(t("", "^$"));
        assert!(!t("a", "^$"));
        assert!(t("ABC", "B"));
        assert!(!t("abc", "B"), "case-sensitive");
        assert!(t("cat", "dog|cat"));
        assert!(t("xay", "x(?:a|b)y"));
        assert!(!t("xcy", "x(a|b)y"));
        // `$` is input end only (no multiline).
        assert!(!t("a\nb", "a$"));
    }

    #[test]
    fn greedy_lazy_and_counts() {
        assert!(t("aaa", "^a{2,3}$"));
        assert!(!t("aaaa", "^a{2,3}$"));
        assert!(t("aaaa", "^a{2,}$"));
        assert!(!t("a", "^a{2,}$"));
        assert!(t("aa", "^a{2}$"));
        assert!(t("<a><b>", "^<.+?>$"));
        assert!(t("<a>", "^<.*?>$"));
        assert!(t("ab", "^a??b$"));
        assert!(t("b", "^a??b$"));
        assert!(t("aaab", "^a*?b$"));
        assert!(t("", "^(?:)*$"));
        assert!(t("", "^(?:){3}$"));
        // Empty iterations past min stop the loop instead of spinning.
        assert!(t("ab", "^(?:a|)*b$"));
    }

    #[test]
    fn lookahead_is_atomic() {
        assert!(!t("aaa", "^(?=(a+?))a$"));
        assert!(t("aaa", "^(?=(a+?))a+$"));
        // A positive lookahead does not consume.
        assert!(t("abc", "^(?=abc)abc$"));
        assert!(!t("abd", "^(?=abc)ab."));
        // Negative.
        assert!(t("abd", "^(?!abc)ab."));
        assert!(!t("abc", "^(?!abc)ab."));
        assert!(t("foo", r"^(?!.*bar).*$"));
        assert!(!t("foobar", r"^(?!.*bar).*$"));
        // Nested lookaheads.
        assert!(t("ab", "^(?=a(?=b))ab$"));
        assert!(!t("ac", "^(?=a(?!c))a"));
    }

    #[test]
    fn atomicity_in_steps() {
        // Without backreferences atomicity only shows in the step count: once
        // the lookahead matched via "a", the "ab" branch is never tried.
        let ast = checked_ast("^(?=a|ab)x").unwrap();
        let input: Vec<u32> = "abz".chars().map(|c| c as u32).collect();
        let mut m = Matcher { input: &input, steps: 0, cap: u64::MAX };
        assert!(!m.run(&ast, 0).unwrap());
        assert_eq!(m.steps, 7, "concat, ^, look, alt, concat, a, x");
        assert!(t("ab", "^(?=(?:a|ab)$)"), "backtracking inside is fine");
        assert!(t("ab", "^(?=(?:a|ab)$)ab"));
    }

    #[test]
    fn classes_and_escapes() {
        assert!(t("-", "^[a-]$"));
        assert!(t("-", "^[-a]$"));
        assert!(t("-", r"^[a\-z]$"));
        assert!(!t("b", r"^[a\-z]$"));
        assert!(t("\u{8}", r"^[\b]$"));
        assert!(t("x", r"^[^\d]$"));
        assert!(!t("5", r"^[^\d]$"));
        assert!(t("5", r"^[\dx]$"));
        assert!(t("\t", r"^\t$"));
        assert!(t("\u{0}", r"^\0$"));
        assert!(t("A", r"^\x41$"));
        assert!(t("A", r"^A$"));
        assert!(t("😀", r"^\u{1F600}$"));
        assert!(t("😀", r"^😀$"));
        assert!(t("😀", "^.$"), "one code point");
        assert!(!t("\n", "^.$"));
        assert!(!t("\u{2028}", "^.$"));
        assert!(t("\u{FEFF}", r"^\s$"));
        assert!(t("é", r"^\W$"));
        assert!(t("a.b", r"^a\.b$"));
        assert!(!t("axb", r"^a\.b$"));
        assert!(t("/", r"^\/$"));
        assert!(t("{}", r"^\{\}$"));
        assert!(t("foo bar", r"\bbar\b"));
        assert!(!t("foobar", r"\bbar"));
        assert!(t("foobar", r"\Bbar"));
        assert!(t("[", "^[[]$"));
    }

    #[test]
    fn value_coercion() {
        assert!(regex_test(&Value::Null, "^$").unwrap());
        assert!(regex_test(&json!(42), "^42$").unwrap());
        assert!(regex_test(&json!(1.5), r"^1\.5$").unwrap());
        assert!(regex_test(&json!(1e21), r"^1e\+21$").unwrap());
        assert!(regex_test(&json!(true), "^true$").unwrap());
        assert!(!regex_test(&json!({"a": 1}), "").unwrap());
        assert!(!regex_test(&json!([1]), "").unwrap());
        // Pattern errors win over the value check (steps 1–3 come first).
        assert!(regex_test(&json!([1]), "(").is_err());
        let max = "a".repeat(MAX_VALUE_CODE_POINTS);
        assert!(regex_test(&json!(max), "^a+$").unwrap());
        let over = "a".repeat(MAX_VALUE_CODE_POINTS + 1);
        assert!(!regex_test(&json!(over), "a").unwrap());
    }

    #[test]
    fn step_cap() {
        // Safe by R2–R4 but quadratic under search semantics.
        let value = json!("a".repeat(MAX_VALUE_CODE_POINTS));
        assert_eq!(regex_test(&value, "a*b").unwrap_err(), "regex: too expensive");
        // Anchored, the same pattern is linear.
        assert!(!regex_test(&value, "^a*b").unwrap());
        // The counter itself, under a lowered cap.
        let ast = checked_ast("abc").unwrap();
        let input: Vec<u32> = "xxabc".chars().map(|c| c as u32).collect();
        let mut m = Matcher { input: &input, steps: 0, cap: u64::MAX };
        assert!(m.run(&ast, 2).unwrap());
        assert_eq!(m.steps, 4, "concat, a, b, c");
        assert!(Matcher { input: &input, steps: 0, cap: 100 }.search(&ast).unwrap());
        assert!(Matcher { input: &input, steps: 0, cap: 3 }.search(&ast).is_err());
    }
}
