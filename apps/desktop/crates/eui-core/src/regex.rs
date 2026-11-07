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
    let cps: Vec<char> = pattern.chars().collect();
    if cps.len() > MAX_PATTERN_CODE_POINTS {
        return Err(format!("regex: pattern longer than {MAX_PATTERN_CODE_POINTS} code points"));
    }
    let ast = Parser { cps, pos: 0 }
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

struct Parser {
    cps: Vec<char>,
    pos: usize,
}

enum Esc {
    Char(u32),
    Class(Set),
    WordBoundary,
    NotWordBoundary,
}

const SYNTAX_CHARS: &str = "^$\\.*+?()[]{}|/";

impl Parser {
    fn peek(&self) -> Option<char> {
        self.cps.get(self.pos).copied()
    }

    fn peek_at(&self, k: usize) -> Option<char> {
        self.cps.get(self.pos + k).copied()
    }

    fn parse(mut self) -> PResult<Node> {
        let node = self.disjunction()?;
        if self.pos < self.cps.len() {
            // The only thing a disjunction stops at, at top level, is `)`.
            return Err(("unmatched )", self.pos));
        }
        Ok(node)
    }

    fn disjunction(&mut self) -> PResult<Node> {
        let mut branches = vec![self.alternative()?];
        while self.peek() == Some('|') {
            self.pos += 1;
            branches.push(self.alternative()?);
        }
        Ok(if branches.len() == 1 { branches.pop().unwrap() } else { Node::Alt(branches) })
    }

    fn alternative(&mut self) -> PResult<Node> {
        let mut items = Vec::new();
        while let Some(c) = self.peek() {
            if c == '|' || c == ')' {
                break;
            }
            items.push(self.term()?);
        }
        Ok(Node::Concat(items))
    }

    fn term(&mut self) -> PResult<Node> {
        let start = self.pos;
        let c = self.peek().expect("term at end");
        let (atom, quantifiable) = match c {
            '^' => {
                self.pos += 1;
                (Node::Assert(Assert::Start), false)
            }
            '$' => {
                self.pos += 1;
                (Node::Assert(Assert::End), false)
            }
            '(' => self.group()?,
            '[' => (self.class()?, true),
            '.' => {
                self.pos += 1;
                (Node::Set(dot_set()), true)
            }
            '\\' => match self.escape(false)? {
                Esc::Char(c) => (Node::Set(Set::single(c)), true),
                Esc::Class(s) => (Node::Set(s), true),
                Esc::WordBoundary => (Node::Assert(Assert::WordBoundary), false),
                Esc::NotWordBoundary => (Node::Assert(Assert::NotWordBoundary), false),
            },
            '*' | '+' | '?' => return Err(("nothing to repeat", start)),
            '{' => {
                return Err(match self.quantifier() {
                    Ok(Some(_)) => ("nothing to repeat", start),
                    Ok(None) | Err(_) => ("lone {", start),
                });
            }
            '}' => return Err(("lone }", start)),
            ']' => return Err(("unmatched ]", start)),
            other => {
                self.pos += 1;
                (Node::Set(Set::single(other as u32)), true)
            }
        };
        let qstart = self.pos;
        match self.quantifier()? {
            None => Ok(atom),
            Some(_) if !quantifiable => Err(("nothing to repeat", qstart)),
            Some((min, max, lazy)) => {
                Ok(Node::Repeat { child: Box::new(atom), min, max, lazy })
            }
        }
    }

    /// A quantifier at the cursor (consumed), or None (nothing consumed).
    fn quantifier(&mut self) -> PResult<Option<(u32, Option<u32>, bool)>> {
        let start = self.pos;
        let (min, max) = match self.peek() {
            Some('*') => {
                self.pos += 1;
                (0, None)
            }
            Some('+') => {
                self.pos += 1;
                (1, None)
            }
            Some('?') => {
                self.pos += 1;
                (0, Some(1))
            }
            Some('{') => {
                self.pos += 1;
                let min = self.decimal().ok_or(("lone {", start))?;
                let max = if self.peek() == Some(',') {
                    self.pos += 1;
                    if self.peek() == Some('}') {
                        None
                    } else {
                        Some(self.decimal().ok_or(("lone {", start))?)
                    }
                } else {
                    Some(min)
                };
                if self.peek() != Some('}') {
                    self.pos = start;
                    return Err(("lone {", start));
                }
                self.pos += 1;
                if let Some(m) = max {
                    if min > m {
                        return Err(("numbers out of order in quantifier", start));
                    }
                }
                (min, max)
            }
            _ => return Ok(None),
        };
        let lazy = self.peek() == Some('?');
        if lazy {
            self.pos += 1;
        }
        Ok(Some((min, max, lazy)))
    }

    fn decimal(&mut self) -> Option<u32> {
        let mut n: Option<u32> = None;
        while let Some(d) = self.peek().and_then(|c| c.to_digit(10)) {
            self.pos += 1;
            n = Some(n.unwrap_or(0).saturating_mul(10).saturating_add(d));
        }
        n
    }

    fn group(&mut self) -> PResult<(Node, bool)> {
        let start = self.pos;
        self.pos += 1;
        let mut look: Option<bool> = None;
        if self.peek() == Some('?') {
            match self.peek_at(1) {
                Some(':') => self.pos += 2,
                Some('=') => {
                    self.pos += 2;
                    look = Some(false);
                }
                Some('!') => {
                    self.pos += 2;
                    look = Some(true);
                }
                Some('<') => {
                    return Err(match self.peek_at(2) {
                        Some('=') | Some('!') => ("lookbehind is not allowed", start),
                        _ => ("named groups are not allowed", start),
                    });
                }
                Some(c) if c.is_ascii_alphabetic() || c == '-' => {
                    return Err(("inline flags are not allowed", start));
                }
                _ => return Err(("invalid group", start)),
            }
        }
        let inner = self.disjunction()?;
        if self.peek() != Some(')') {
            return Err(("unterminated group", start));
        }
        self.pos += 1;
        Ok(match look {
            Some(negative) => (Node::Look { negative, child: Box::new(inner) }, false),
            None => (Node::Group(Box::new(inner)), true),
        })
    }

    fn class(&mut self) -> PResult<Node> {
        let start = self.pos;
        self.pos += 1;
        let negate = self.peek() == Some('^');
        if negate {
            self.pos += 1;
        }
        let mut ranges: Vec<(u32, u32)> = Vec::new();
        loop {
            match self.peek() {
                None => return Err(("unterminated class", start)),
                Some(']') => {
                    self.pos += 1;
                    break;
                }
                Some(_) => {
                    let a_at = self.pos;
                    let a = self.class_atom()?;
                    let is_range = self.peek() == Some('-')
                        && matches!(self.peek_at(1), Some(c) if c != ']');
                    if is_range {
                        self.pos += 1;
                        let b = self.class_atom()?;
                        match (a, b) {
                            (Esc::Char(x), Esc::Char(y)) => {
                                if x > y {
                                    return Err(("invalid range", a_at));
                                }
                                ranges.push((x, y));
                            }
                            _ => return Err(("invalid range", a_at)),
                        }
                    } else {
                        match a {
                            Esc::Char(x) => ranges.push((x, x)),
                            Esc::Class(s) => ranges.extend_from_slice(&s.0),
                            _ => unreachable!("no assertions in a class"),
                        }
                    }
                }
            }
        }
        let set = Set::normalized(ranges);
        Ok(Node::Set(if negate { set.complement() } else { set }))
    }

    fn class_atom(&mut self) -> PResult<Esc> {
        match self.peek() {
            Some('\\') => self.escape(true),
            Some(c) => {
                self.pos += 1;
                Ok(Esc::Char(c as u32))
            }
            None => Err(("unterminated class", self.pos)),
        }
    }

    fn hex(&mut self, n: usize) -> Option<u32> {
        let mut v = 0u32;
        for k in 0..n {
            v = v * 16 + self.peek_at(k)?.to_digit(16)?;
        }
        self.pos += n;
        Some(v)
    }

    /// At a `\`.
    fn escape(&mut self, in_class: bool) -> PResult<Esc> {
        let at = self.pos;
        self.pos += 1;
        let Some(c) = self.peek() else {
            return Err(("\\ at end of pattern", at));
        };
        self.pos += 1;
        let invalid = ("invalid escape", at);
        Ok(match c {
            'b' if in_class => Esc::Char(0x08),
            'b' => Esc::WordBoundary,
            'B' if in_class => return Err(invalid),
            'B' => Esc::NotWordBoundary,
            'd' => Esc::Class(digit_set()),
            'D' => Esc::Class(digit_set().complement()),
            'w' => Esc::Class(word_set()),
            'W' => Esc::Class(word_set().complement()),
            's' => Esc::Class(space_set()),
            'S' => Esc::Class(space_set().complement()),
            't' => Esc::Char(0x09),
            'n' => Esc::Char(0x0A),
            'v' => Esc::Char(0x0B),
            'f' => Esc::Char(0x0C),
            'r' => Esc::Char(0x0D),
            '0' => {
                if matches!(self.peek(), Some(d) if d.is_ascii_digit()) {
                    return Err(("legacy octal is not allowed", at));
                }
                Esc::Char(0)
            }
            '1'..='9' if in_class => return Err(("legacy octal is not allowed", at)),
            '1'..='9' => return Err(("backreferences are not allowed", at)),
            'k' if in_class => return Err(invalid),
            'k' => return Err(("backreferences are not allowed", at)),
            'p' | 'P' => return Err(("property escapes are not allowed", at)),
            'c' => return Err(("\\c is not allowed", at)),
            'x' => Esc::Char(self.hex(2).ok_or(invalid)?),
            'u' => Esc::Char(self.unicode_escape().ok_or(invalid)?),
            '-' if in_class => Esc::Char('-' as u32),
            c if SYNTAX_CHARS.contains(c) => Esc::Char(c as u32),
            _ => return Err(invalid),
        })
    }

    /// After `\u`: `HHHH` (a surrogate pair `\uHHHH\uHHHH` combines) or `{H…}`.
    fn unicode_escape(&mut self) -> Option<u32> {
        if self.peek() == Some('{') {
            self.pos += 1;
            let mut v: u32 = 0;
            let mut any = false;
            while let Some(d) = self.peek().and_then(|c| c.to_digit(16)) {
                self.pos += 1;
                any = true;
                v = v.saturating_mul(16).saturating_add(d);
                if v > MAX_CP {
                    return None;
                }
            }
            if !any || self.peek() != Some('}') {
                return None;
            }
            self.pos += 1;
            return Some(v);
        }
        let hi = self.hex(4)?;
        if (0xD800..=0xDBFF).contains(&hi) && self.peek() == Some('\\') && self.peek_at(1) == Some('u') {
            let save = self.pos;
            self.pos += 2;
            match self.hex(4) {
                Some(lo) if (0xDC00..=0xDFFF).contains(&lo) => {
                    return Some(0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00));
                }
                _ => self.pos = save,
            }
        }
        Some(hi)
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

/// The first rule hit, pre-order, R2 then R3 then R4 per node.
fn safety(n: &Node) -> Option<u8> {
    if let Node::Repeat { child, max, .. } = n {
        // R2: max > 1 (∞ included) over an atom holding an unbounded quantifier.
        if max.map_or(true, |m| m > 1) && contains_unbounded(child) {
            return Some(2);
        }
        // R3: unbounded over an alternation with overlapping first sets.
        if max.is_none() {
            if let Node::Alt(branches) = unwrap(child) {
                let firsts: Vec<Set> = branches.iter().map(first).collect();
                for i in 0..firsts.len() {
                    for j in i + 1..firsts.len() {
                        if firsts[i].intersects(&firsts[j]) {
                            return Some(3);
                        }
                    }
                }
            }
        }
    }
    if let Node::Concat(items) = n {
        // R4: two unbounded items, only nullable items between, sharing chars.
        for i in 0..items.len() {
            if !is_unbounded(unwrap(&items[i])) {
                continue;
            }
            let ci = chars(&items[i]);
            for j in i + 1..items.len() {
                if is_unbounded(unwrap(&items[j])) && ci.intersects(&chars(&items[j])) {
                    return Some(4);
                }
                if !nullable(&items[j]) {
                    break;
                }
            }
        }
    }
    match n {
        Node::Set(_) | Node::Assert(_) => None,
        Node::Look { child, .. } | Node::Group(child) | Node::Repeat { child, .. } => safety(child),
        Node::Concat(v) | Node::Alt(v) => v.iter().find_map(safety),
    }
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
    fn refusals() {
        let cases: &[(&str, &str)] = &[
            (r"(a)\1", "regex: backreferences are not allowed at 3"),
            (r"\k<x>", "regex: backreferences are not allowed at 0"),
            ("(?<=a)b", "regex: lookbehind is not allowed at 0"),
            ("(?<!a)b", "regex: lookbehind is not allowed at 0"),
            ("(?<n>a)", "regex: named groups are not allowed at 0"),
            (r"\p{L}", "regex: property escapes are not allowed at 0"),
            (r"x\P{L}", "regex: property escapes are not allowed at 1"),
            (r"\cA", "regex: \\c is not allowed at 0"),
            (r"\01", "regex: legacy octal is not allowed at 0"),
            (r"[\1]", "regex: legacy octal is not allowed at 1"),
            ("(?i)a", "regex: inline flags are not allowed at 0"),
            ("(?)", "regex: invalid group at 0"),
            (r"\q", "regex: invalid escape at 0"),
            (r"\a", "regex: invalid escape at 0"),
            (r"\xZ1", "regex: invalid escape at 0"),
            (r"\u12", "regex: invalid escape at 0"),
            (r"\u{110000}", "regex: invalid escape at 0"),
            (r"\-", "regex: invalid escape at 0"),
            (r"[\B]", "regex: invalid escape at 1"),
            ("a\\", "regex: \\ at end of pattern at 1"),
            ("a)", "regex: unmatched ) at 1"),
            ("a]", "regex: unmatched ] at 1"),
            ("a}", "regex: lone } at 1"),
            ("a{", "regex: lone { at 1"),
            ("a{1", "regex: lone { at 1"),
            ("a{,3}", "regex: lone { at 1"),
            ("{x", "regex: lone { at 0"),
            ("{2}", "regex: nothing to repeat at 0"),
            ("*a", "regex: nothing to repeat at 0"),
            ("a|+", "regex: nothing to repeat at 2"),
            ("a**", "regex: nothing to repeat at 2"),
            ("^*", "regex: nothing to repeat at 1"),
            (r"\b+", "regex: nothing to repeat at 2"),
            ("(?=a)?", "regex: nothing to repeat at 5"),
            ("a{3,2}", "regex: numbers out of order in quantifier at 1"),
            ("(ab", "regex: unterminated group at 0"),
            ("[ab", "regex: unterminated class at 0"),
            ("[z-a]", "regex: invalid range at 1"),
            (r"[\d-z]", "regex: invalid range at 1"),
            (r"[a-\d]", "regex: invalid range at 1"),
            ("é(", "regex: unterminated group at 1"),
            ("😀)", "regex: unmatched ) at 1"),
        ];
        for (pattern, message) in cases {
            assert_eq!(&err(pattern), message, "{pattern}");
            assert_eq!(regex_test(&json!("x"), pattern).unwrap_err(), *message);
        }
        let long = "a".repeat(257);
        assert_eq!(err(&long), "regex: pattern longer than 256 code points");
        assert!(check_pattern(&"é".repeat(256)).is_ok());
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
