//! The built-in `Markdown` painter: a small GFM subset (paragraphs, headings,
//! bullet / numbered / task lists nested by marker indentation, block quotes,
//! fenced code, tables, rules, block images; bold / italic / strike / code /
//! links inline) laid out as absolutely placed blocks. VAPP-103: link and
//! image destinations parse balanced parentheses (CommonMark); a link the
//! URL policy denies paints as text, an image the media policy denies as
//! its alt text ([`resolve_images`]); an image inside a line paints its alt
//! text. The SAME block layout answers the measurer and places the
//! painted blocks, so the height taffy reserves is the height painted
//! (the spike's markdown estimate was 7 px short until it counted a gap per
//! block). No HTML passthrough: tags are text.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use exponential_ui::theme::{Mode, ResolvedTheme};
use gpui::{
    div, prelude::*, px, AnyElement, App, Font, FontStyle, FontWeight, Hsla, InteractiveText, SharedString, StrikethroughStyle, StyledText, TextLayout, TextRun, UnderlineStyle, Window,
};
use serde_json::Map;

use super::color::color_of;
use super::parts::{mono_family, part_props, part_visual, px_prop, spacing, theme_color};

/// One inline span.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Inline {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strike: bool,
    pub link: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlockKind {
    Paragraph,
    Heading(u8),
    /// `marker` = "•" or "3."; `task` = a GFM task box state; `depth` =
    /// the nesting level (0 = top).
    ListItem { marker: String, task: Option<bool>, depth: u8 },
    Quote,
    CodeBlock,
    Table { header: Vec<Vec<Inline>>, rows: Vec<Vec<Vec<Inline>>> },
    Rule,
    /// A paragraph that is one `![alt](src)`: painted through the host's
    /// media policy and loader in an `image_height` box (the alt text there
    /// while it loads or when it fails).
    Image { src: String, alt: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub kind: BlockKind,
    pub inlines: Vec<Inline>,
    /// Consecutive list items share a group (no paragraph gap inside it).
    pub list_group: Option<usize>,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Default)]
struct Flags {
    bold: bool,
    italic: bool,
    strike: bool,
}

fn push_text(out: &mut Vec<Inline>, text: &str, f: Flags, link: Option<&str>) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = out.last_mut() {
        if !last.code && last.bold == f.bold && last.italic == f.italic && last.strike == f.strike && last.link.as_deref() == link {
            last.text.push_str(text);
            return;
        }
    }
    out.push(Inline { text: text.to_string(), bold: f.bold, italic: f.italic, code: false, strike: f.strike, link: link.map(str::to_string) });
}

/// `[label](dest)` at the start of `s` → (label, dest, consumed bytes): the
/// label's brackets balance, the destination is `<…>` or a run without
/// whitespace whose parentheses balance (CommonMark), an optional quoted
/// title is skipped.
pub fn link_at(s: &str) -> Option<(&str, String, usize)> {
    let b = s.as_bytes();
    if b.first() != Some(&b'[') {
        return None;
    }
    let mut depth = 0i32;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 1,
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        i += 1;
    }
    if i >= b.len() || b.get(i + 1) != Some(&b'(') {
        return None;
    }
    let label = &s[1..i];
    let mut j = i + 2;
    let dest = if b.get(j) == Some(&b'<') {
        let close = s[j..].find('>')? + j;
        let inner = &s[j + 1..close];
        if inner.contains(['\n', '<']) {
            return None;
        }
        j = close + 1;
        inner.to_string()
    } else {
        let start = j;
        let mut parens = 0i32;
        while j < b.len() {
            let c = b[j];
            if c == b'\\' && j + 1 < b.len() {
                j += 2;
                continue;
            }
            if c.is_ascii_whitespace() {
                break;
            }
            if c == b'(' {
                parens += 1;
            } else if c == b')' {
                if parens == 0 {
                    break;
                }
                parens -= 1;
            }
            j += 1;
        }
        if parens != 0 {
            return None;
        }
        s[start..j].replace("\\(", "(").replace("\\)", ")")
    };
    let rest = &s[j..];
    let ws = rest.len() - rest.trim_start().len();
    if ws > 0 {
        let t = &rest[ws..];
        if let Some(q) = t.chars().next().filter(|c| *c == '"' || *c == '\'') {
            if let Some(end) = t[1..].find(q) {
                j += ws + end + 2;
            }
        }
    }
    while b.get(j) == Some(&b' ') {
        j += 1;
    }
    if b.get(j) != Some(&b')') {
        return None;
    }
    Some((label, dest, j + 1))
}

/// A whole line that is exactly one image `![alt](src)` → (alt, src).
fn whole_image(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix('!')?;
    let (alt, src, n) = link_at(rest)?;
    (n == rest.len()).then(|| (alt.to_string(), src))
}

/// The host's policy over parsed blocks: an image block whose src the
/// media policy refuses becomes a paragraph of its alt text (the measurer
/// and the painter both resolve, so a denied image never reserves a box);
/// a link the URL policy refuses loses its href (plain text, not pressable).
pub fn resolve_urls(blocks: Vec<Block>, link_allowed: impl Fn(&str) -> bool, image_allowed: impl Fn(&str) -> bool) -> Vec<Block> {
    let strip = |inlines: &mut Vec<Inline>| {
        for span in inlines.iter_mut() {
            if span.link.as_deref().is_some_and(|h| !link_allowed(h)) {
                span.link = None;
            }
        }
    };
    blocks
        .into_iter()
        .map(|mut b| {
            if let BlockKind::Image { src, alt } = &b.kind {
                if !image_allowed(src) {
                    b = Block { kind: BlockKind::Paragraph, inlines: parse_inline(alt), list_group: None };
                }
            }
            strip(&mut b.inlines);
            if let BlockKind::Table { header, rows } = &mut b.kind {
                header.iter_mut().chain(rows.iter_mut().flatten()).for_each(&strip);
            }
            b
        })
        .collect()
}

/// The blocks of `text` under the host's URL and media policies.
pub fn parse_for(host: &dyn crate::host::HostPlugin, text: &str) -> Vec<Block> {
    let urls = host.url_policy();
    resolve_urls(parse(text), |h| exponential_ui::host::safe_href(urls.as_ref(), h).is_some(), |src| crate::media::allowed_request(host, src).is_some())
}

/// A delimited run `ddTEXTdd` at the start of `s` → (TEXT, consumed).
fn delimited<'a>(s: &'a str, delim: &str) -> Option<(&'a str, usize)> {
    let rest = s.strip_prefix(delim)?;
    let end = rest.find(delim)?;
    let inner = &rest[..end];
    if inner.is_empty() || inner.starts_with(char::is_whitespace) {
        return None;
    }
    // GFM: `_` never delimits inside a word (`snake_case_name`).
    if delim.starts_with('_') && rest[end + delim.len()..].chars().next().is_some_and(char::is_alphanumeric) {
        return None;
    }
    Some((inner, delim.len() * 2 + end))
}

fn parse_inline_into(s: &str, f: Flags, link: Option<&str>, out: &mut Vec<Inline>) {
    let mut plain_start = 0;
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < s.len() {
        if !s.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let rest = &s[i..];
        let c = bytes[i];
        let mut handled: Option<usize> = None;
        if c == b'`' {
            if let Some(end) = rest[1..].find('`') {
                push_text(out, &s[plain_start..i], f, link);
                out.push(Inline { text: rest[1..1 + end].to_string(), code: true, link: link.map(str::to_string), ..Inline::default() });
                handled = Some(end + 2);
            }
        } else if c == b'!' && rest[1..].starts_with('[') {
            // An image inside a line paints its alt text.
            if let Some((alt, _src, n)) = link_at(&rest[1..]) {
                push_text(out, &s[plain_start..i], f, link);
                push_text(out, alt, f, link);
                handled = Some(n + 1);
            }
        } else if c == b'[' {
            if let Some((label, href, n)) = link_at(rest) {
                push_text(out, &s[plain_start..i], f, link);
                parse_inline_into(label, f, Some(href.as_str()), out);
                handled = Some(n);
            }
        } else if (c == b'*' || c == b'_' || c == b'~') && !(c == b'_' && s[..i].chars().next_back().is_some_and(char::is_alphanumeric)) {
            let double = match c {
                b'*' => "**",
                b'_' => "__",
                _ => "~~",
            };
            if let Some((inner, n)) = delimited(rest, double) {
                push_text(out, &s[plain_start..i], f, link);
                let nf = if c == b'~' { Flags { strike: true, ..f } } else { Flags { bold: true, ..f } };
                parse_inline_into(inner, nf, link, out);
                handled = Some(n);
            } else if c != b'~' {
                let single = if c == b'*' { "*" } else { "_" };
                if let Some((inner, n)) = delimited(rest, single) {
                    push_text(out, &s[plain_start..i], f, link);
                    parse_inline_into(inner, Flags { italic: true, ..f }, link, out);
                    handled = Some(n);
                }
            }
        }
        match handled {
            Some(n) => {
                i += n;
                plain_start = i;
            }
            None => i += 1,
        }
    }
    push_text(out, &s[plain_start..], f, link);
}

/// The inline spans of one line of markdown.
pub fn parse_inline(s: &str) -> Vec<Inline> {
    let mut out = Vec::new();
    parse_inline_into(s, Flags::default(), None, &mut out);
    out
}

fn is_table_row(line: &str) -> bool {
    let t = line.trim();
    t.len() >= 2 && t.starts_with('|') && t.ends_with('|')
}

fn is_table_rule(line: &str) -> bool {
    is_table_row(line) && cells(line).iter().all(|c| !c.is_empty() && c.trim_matches(':').chars().all(|ch| ch == '-') && c.contains('-'))
}

fn cells(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut prev_backslash = false;
    for ch in t.chars() {
        if ch == '|' && !prev_backslash {
            out.push(cur.trim().to_string());
            cur.clear();
        } else if ch == '|' {
            cur.pop();
            cur.push('|');
        } else {
            cur.push(ch);
        }
        prev_backslash = ch == '\\';
    }
    out.push(cur.trim().to_string());
    out
}

/// A list item line → (marker indent in columns, marker, the rest).
fn list_marker(line: &str) -> Option<(usize, String, &str)> {
    let t = line.trim_start();
    let indent = line[..line.len() - t.len()].chars().map(|c| if c == '\t' { 4 } else { 1 }).sum();
    let (marker, rest) = list_marker_of(t)?;
    Some((indent, marker, rest))
}

fn list_marker_of(t: &str) -> Option<(String, &str)> {
    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = t.strip_prefix(bullet) {
            return Some(("•".to_string(), rest));
        }
    }
    let digits = t.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 && digits < 10 {
        let after = &t[digits..];
        if let Some(rest) = after.strip_prefix(". ").or_else(|| after.strip_prefix(") ")) {
            return Some((format!("{}.", &t[..digits]), rest));
        }
    }
    None
}

fn is_rule(line: &str) -> bool {
    let t: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    t.len() >= 3 && (t.chars().all(|c| c == '-') || t.chars().all(|c| c == '*') || t.chars().all(|c| c == '_'))
}

/// Parse the block structure of a markdown document.
pub fn parse(text: &str) -> Vec<Block> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();
    let mut blocks = Vec::new();
    let mut para: Vec<&str> = Vec::new();
    let mut group = 0usize;
    let flush = |para: &mut Vec<&str>, blocks: &mut Vec<Block>| {
        if !para.is_empty() {
            let joined = para.join(" ");
            match whole_image(joined.trim()) {
                Some((alt, src)) => blocks.push(Block { kind: BlockKind::Image { src, alt: alt.clone() }, inlines: parse_inline(&alt), list_group: None }),
                None => blocks.push(Block { kind: BlockKind::Paragraph, inlines: parse_inline(&joined), list_group: None }),
            }
            para.clear();
        }
    };
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            flush(&mut para, &mut blocks);
            let mut body = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim().starts_with("```") {
                body.push(lines[i]);
                i += 1;
            }
            i += 1;
            blocks.push(Block { kind: BlockKind::CodeBlock, inlines: vec![Inline { text: body.join("\n"), code: true, ..Inline::default() }], list_group: None });
            continue;
        }
        let hashes = trimmed.bytes().take_while(|b| *b == b'#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
            flush(&mut para, &mut blocks);
            blocks.push(Block { kind: BlockKind::Heading(hashes as u8), inlines: parse_inline(trimmed[hashes..].trim()), list_group: None });
            i += 1;
            continue;
        }
        if trimmed.starts_with('>') {
            flush(&mut para, &mut blocks);
            let mut body = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with('>') {
                let l = lines[i].trim_start().trim_start_matches('>');
                body.push(l.strip_prefix(' ').unwrap_or(l).trim());
                i += 1;
            }
            let joined: Vec<&str> = body.into_iter().filter(|l| !l.is_empty()).collect();
            blocks.push(Block { kind: BlockKind::Quote, inlines: parse_inline(&joined.join(" ")), list_group: None });
            continue;
        }
        if is_rule(trimmed) && para.is_empty() {
            blocks.push(Block { kind: BlockKind::Rule, inlines: Vec::new(), list_group: None });
            i += 1;
            continue;
        }
        if list_marker(line).is_some() {
            flush(&mut para, &mut blocks);
            group += 1;
            // The indents of the open lists, outermost first: an item 2+
            // columns past its list's nests under the previous item.
            let mut lists: Vec<usize> = Vec::new();
            while i < lines.len() {
                let Some((indent, marker, rest)) = list_marker(lines[i]) else { break };
                while lists.len() > 1 && indent < *lists.last().unwrap() {
                    lists.pop();
                }
                match lists.last() {
                    None => lists.push(indent),
                    Some(&top) if indent >= top + 2 => lists.push(indent),
                    _ => {}
                }
                let depth = (lists.len() - 1).min(u8::MAX as usize) as u8;
                let (task, body) = match rest.strip_prefix("[ ] ") {
                    Some(b) => (Some(false), b),
                    None => match rest.strip_prefix("[x] ").or_else(|| rest.strip_prefix("[X] ")) {
                        Some(b) => (Some(true), b),
                        None => (None, rest),
                    },
                };
                blocks.push(Block { kind: BlockKind::ListItem { marker, task, depth }, inlines: parse_inline(body.trim()), list_group: Some(group) });
                i += 1;
            }
            continue;
        }
        if is_table_row(line) && i + 1 < lines.len() && is_table_rule(lines[i + 1]) {
            flush(&mut para, &mut blocks);
            let header: Vec<Vec<Inline>> = cells(line).iter().map(|c| parse_inline(c)).collect();
            i += 2;
            let mut rows = Vec::new();
            while i < lines.len() && is_table_row(lines[i]) {
                rows.push(cells(lines[i]).iter().map(|c| parse_inline(c)).collect());
                i += 1;
            }
            blocks.push(Block { kind: BlockKind::Table { header, rows }, inlines: Vec::new(), list_group: None });
            continue;
        }
        if trimmed.is_empty() {
            flush(&mut para, &mut blocks);
        } else {
            para.push(trimmed);
        }
        i += 1;
    }
    flush(&mut para, &mut blocks);
    blocks
}

/// The plain text of some spans.
pub fn plain(inlines: &[Inline]) -> String {
    inlines.iter().map(|i| i.text.as_str()).collect()
}

// ---------------------------------------------------------------------------
// Layout (shared by the measurer and the painter)
// ---------------------------------------------------------------------------

/// Typography of one block kind.
#[derive(Debug, Clone, PartialEq)]
pub struct TextSpec {
    pub size: f32,
    pub line_height: f32,
    pub weight: u16,
    pub family: Option<String>,
}

/// Resolved metrics of the `Markdown` recipe parts.
#[derive(Debug, Clone, PartialEq)]
pub struct MdStyles {
    pub body: TextSpec,
    pub heading: TextSpec,
    pub code: TextSpec,
    pub code_pad: f32,
    pub quote_border: f32,
    pub quote_pad: f32,
    /// The paragraph rhythm (CSS: `margin-bottom: sm`).
    pub block_gap: f32,
    /// Above a heading (`margin-top: md`) and below it (`xs`).
    pub heading_top: f32,
    pub heading_bottom: f32,
    pub list_indent: f32,
    /// A block image's box height (loaded or not, so measure = paint).
    pub image_height: f32,
    pub cell_pad_h: f32,
    pub cell_pad_v: f32,
}

impl MdStyles {
    /// Plain metrics for a body text style (geometry mode, tests).
    pub fn plain(body: TextSpec) -> MdStyles {
        MdStyles {
            heading: TextSpec { size: body.size + 4.0, line_height: body.line_height + 8.0, weight: 600, family: body.family.clone() },
            code: TextSpec { size: (body.size - 2.0).max(10.0), line_height: body.line_height, weight: 400, family: None },
            list_indent: (body.size * 1.4).round(),
            image_height: 160.0,
            body,
            code_pad: 12.0,
            quote_border: 2.0,
            quote_pad: 12.0,
            block_gap: 8.0,
            heading_top: 12.0,
            heading_bottom: 4.0,
            cell_pad_h: 8.0,
            cell_pad_v: 4.0,
        }
    }

    /// From the theme's `Markdown/*` recipes (the root's own text style is
    /// the body; `owner_props` = the node props).
    pub fn resolve(theme: Option<&ResolvedTheme>, mode: Mode, body: TextSpec, owner_props: &Map<String, serde_json::Value>) -> MdStyles {
        let mut s = MdStyles::plain(body);
        if theme.is_none() {
            return s;
        }
        let part = |name: &str| part_props(theme, mode, "Markdown", name, owner_props, &[]);
        let spec = |p: &Map<String, serde_json::Value>, base: &TextSpec| TextSpec {
            size: px_prop(p, "fontSize").unwrap_or(base.size),
            line_height: px_prop(p, "lineHeight").unwrap_or(base.line_height),
            weight: p.get("fontWeight").and_then(|v| v.as_u64()).map(|w| w as u16).unwrap_or(base.weight),
            family: p.get("fontFamily").and_then(|v| v.as_str()).map(str::to_string).or_else(|| base.family.clone()),
        };
        let heading = part("heading");
        s.heading = spec(&heading, &TextSpec { weight: 600, ..s.body.clone() });
        let code = part("codeBlock");
        s.code = spec(&code, &TextSpec { family: mono_family(theme), ..s.code.clone() });
        s.code_pad = px_prop(&code, "padding").or_else(|| px_prop(&code, "paddingHorizontal")).unwrap_or(s.code_pad);
        let quote = part("quote");
        s.quote_border = px_prop(&quote, "borderWidth").unwrap_or(s.quote_border);
        s.quote_pad = px_prop(&quote, "paddingHorizontal").unwrap_or(s.quote_pad);
        s.block_gap = spacing(theme, "sm");
        s.heading_top = spacing(theme, "md");
        s.heading_bottom = spacing(theme, "xs");
        s.cell_pad_h = spacing(theme, "sm");
        s.cell_pad_v = spacing(theme, "xs");
        s
    }

    fn spec_of(&self, kind: &BlockKind) -> &TextSpec {
        match kind {
            BlockKind::Heading(_) => &self.heading,
            BlockKind::CodeBlock => &self.code,
            _ => &self.body,
        }
    }
}

/// Where one block sits (relative to the content box).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockBox {
    pub y: f32,
    pub height: f32,
    /// Tables: each row's height (header first).
    pub rows: Vec<f32>,
}

/// The whole document laid out at one width.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MdLayout {
    pub blocks: Vec<BlockBox>,
    pub height: f32,
}

impl MdLayout {
    /// The height shown under a `lines` clamp (`None` = all of it): the
    /// first `n` body lines, like the web's `-webkit-line-clamp`.
    pub fn clamped_height(&self, lines: Option<usize>, line_height: f32) -> f32 {
        match lines {
            Some(n) if n > 0 => self.height.min(n as f32 * line_height),
            _ => self.height,
        }
    }
}

/// What the layout asks the text system: the height of `inlines` wrapped at
/// `width` (`None` = one line per paragraph) and the width they take.
pub trait MdText {
    fn height(&mut self, inlines: &[Inline], spec: &TextSpec, width: Option<f32>) -> f32;
    fn width(&mut self, inlines: &[Inline], spec: &TextSpec) -> f32;
    fn widest_word(&mut self, inlines: &[Inline], spec: &TextSpec) -> f32;
}

fn margins(kind: &BlockKind, s: &MdStyles) -> (f32, f32) {
    match kind {
        BlockKind::Heading(_) => (s.heading_top, s.heading_bottom),
        _ => (0.0, s.block_gap),
    }
}

/// The gap above block `i` (CSS margins collapse; first/last margins drop).
fn gap_before(blocks: &[Block], i: usize, s: &MdStyles) -> f32 {
    if i == 0 {
        return 0.0;
    }
    let (prev, cur) = (&blocks[i - 1], &blocks[i]);
    if prev.list_group.is_some() && prev.list_group == cur.list_group {
        return 0.0;
    }
    margins(&prev.kind, s).1.max(margins(&cur.kind, s).0)
}

/// Lay the blocks out `width` wide.
pub fn layout(blocks: &[Block], s: &MdStyles, width: f32, text: &mut dyn MdText) -> MdLayout {
    let mut y = 0.0;
    let mut out = Vec::with_capacity(blocks.len());
    for (i, b) in blocks.iter().enumerate() {
        y += gap_before(blocks, i, s);
        let spec = s.spec_of(&b.kind);
        let (height, rows) = match &b.kind {
            BlockKind::Paragraph | BlockKind::Heading(_) => (text.height(&b.inlines, spec, Some(width.max(1.0))), Vec::new()),
            BlockKind::ListItem { depth, .. } => (text.height(&b.inlines, spec, Some((width - s.list_indent * (*depth as f32 + 1.0)).max(1.0))), Vec::new()),
            BlockKind::Image { .. } => (s.image_height, Vec::new()),
            BlockKind::Quote => (text.height(&b.inlines, spec, Some((width - s.quote_border - s.quote_pad).max(1.0))), Vec::new()),
            BlockKind::CodeBlock => (text.height(&b.inlines, spec, None) + 2.0 * s.code_pad, Vec::new()),
            BlockKind::Rule => (1.0, Vec::new()),
            BlockKind::Table { header, rows } => {
                let cols = header.len().max(rows.iter().map(Vec::len).max().unwrap_or(0)).max(1);
                let cell_w = (width / cols as f32 - 2.0 * s.cell_pad_h).max(1.0);
                let mut heights = Vec::new();
                for row in std::iter::once(header).chain(rows.iter()) {
                    let h = row.iter().map(|c| text.height(c, &s.body, Some(cell_w))).fold(s.body.line_height, f32::max);
                    heights.push(h + 2.0 * s.cell_pad_v + 1.0);
                }
                (heights.iter().sum::<f32>() + 1.0, heights)
            }
        };
        out.push(BlockBox { y, height, rows });
        y += height;
    }
    MdLayout { blocks: out, height: y }
}

/// Max-content width: the widest block on one line.
pub fn max_content_width(blocks: &[Block], s: &MdStyles, text: &mut dyn MdText) -> f32 {
    blocks
        .iter()
        .map(|b| {
            let spec = s.spec_of(&b.kind);
            match &b.kind {
                BlockKind::Paragraph | BlockKind::Heading(_) => text.width(&b.inlines, spec),
                BlockKind::ListItem { depth, .. } => text.width(&b.inlines, spec) + s.list_indent * (*depth as f32 + 1.0),
                BlockKind::Image { .. } => (s.image_height * 1.5).round(),
                BlockKind::Quote => text.width(&b.inlines, spec) + s.quote_border + s.quote_pad,
                BlockKind::CodeBlock => b.inlines[0].text.lines().map(|l| text.width(&[Inline { text: l.to_string(), ..b.inlines[0].clone() }], spec)).fold(0.0, f32::max) + 2.0 * s.code_pad,
                BlockKind::Rule => 0.0,
                BlockKind::Table { header, rows } => {
                    let cols = header.len().max(1) as f32;
                    let widest = std::iter::once(header).chain(rows.iter()).flat_map(|r| r.iter()).map(|c| text.width(c, &s.body)).fold(0.0, f32::max);
                    cols * (widest + 2.0 * s.cell_pad_h)
                }
            }
        })
        .fold(0.0, f32::max)
        .ceil()
}

/// Min-content width: the widest word (code blocks never wrap).
pub fn min_content_width(blocks: &[Block], s: &MdStyles, text: &mut dyn MdText) -> f32 {
    blocks
        .iter()
        .map(|b| {
            let spec = s.spec_of(&b.kind);
            match &b.kind {
                BlockKind::CodeBlock => max_content_width(std::slice::from_ref(b), s, text),
                BlockKind::Table { header, .. } => header.len().max(1) as f32 * (2.0 * s.cell_pad_h + 16.0),
                BlockKind::ListItem { depth, .. } => text.widest_word(&b.inlines, spec) + s.list_indent * (*depth as f32 + 1.0),
                BlockKind::Image { .. } => 1.0,
                BlockKind::Quote => text.widest_word(&b.inlines, spec) + s.quote_border + s.quote_pad,
                _ => text.widest_word(&b.inlines, spec),
            }
        })
        .fold(0.0, f32::max)
        .ceil()
}

// ---------------------------------------------------------------------------
// Text runs (one builder for the measurer and the painter)
// ---------------------------------------------------------------------------

/// Colours the runs paint with.
#[derive(Debug, Clone, Copy)]
pub struct RunColors {
    pub ink: Hsla,
    pub link: Hsla,
    pub code_bg: Option<Hsla>,
}

/// The string and runs of some spans in one base font.
pub fn runs(inlines: &[Inline], family: &SharedString, mono: &SharedString, weight: u16, colors: RunColors) -> (SharedString, Vec<TextRun>) {
    let mut text = String::new();
    let mut out = Vec::with_capacity(inlines.len());
    for span in inlines {
        if span.text.is_empty() {
            continue;
        }
        text.push_str(&span.text);
        let font = Font {
            family: if span.code { mono.clone() } else { family.clone() },
            features: Default::default(),
            fallbacks: None,
            weight: FontWeight(if span.bold { weight.max(600) } else { weight } as f32),
            style: if span.italic { FontStyle::Italic } else { FontStyle::Normal },
        };
        let color = if span.link.is_some() { colors.link } else { colors.ink };
        out.push(TextRun {
            len: span.text.len(),
            font,
            color,
            background_color: if span.code { colors.code_bg } else { None },
            underline: span.link.as_ref().map(|_| UnderlineStyle { thickness: px(1.0), color: Some(colors.link), wavy: false }),
            strikethrough: span.strike.then_some(StrikethroughStyle { thickness: px(1.0), color: Some(colors.ink) }),
        });
    }
    (text.into(), out)
}

// ---------------------------------------------------------------------------
// Painting
// ---------------------------------------------------------------------------

/// The painted text units of every Markdown leaf (by node index), in
/// document order: each unit's gpui text layout (hit-testing once painted)
/// and its text. Selection and copy read it.
pub type MdUnits = Rc<RefCell<HashMap<u32, Vec<(TextLayout, SharedString)>>>>;

/// A text selection inside one Markdown leaf: `(unit, byte)` ends, in
/// either order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MdSelection {
    pub node: u32,
    pub anchor: (usize, usize),
    pub head: (usize, usize),
}

impl MdSelection {
    /// `(start, end)` in document order.
    pub fn ordered(&self) -> ((usize, usize), (usize, usize)) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// The selected byte range of unit `u` whose text is `len` bytes long.
    pub fn range_in(&self, u: usize, len: usize) -> Option<Range<usize>> {
        let ((su, sb), (eu, eb)) = self.ordered();
        if self.is_empty() || u < su || u > eu {
            return None;
        }
        let start = if u == su { sb.min(len) } else { 0 };
        let end = if u == eu { eb.min(len) } else { len };
        (start < end).then_some(start..end)
    }

    /// The selected text (units joined by line feeds).
    pub fn text(&self, units: &[(TextLayout, SharedString)]) -> String {
        let mut out: Vec<&str> = Vec::new();
        for (u, (_, text)) in units.iter().enumerate() {
            if let Some(r) = self.range_in(u, text.len()) {
                let (a, b) = (floor_char(text, r.start), floor_char(text, r.end));
                out.push(&text[a..b]);
            }
        }
        out.join("\n")
    }
}

/// The nearest char boundary at or before `i`.
fn floor_char(s: &str, i: usize) -> usize {
    let mut i = i.min(s.len());
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Runs with `range` painted on the selection background (runs split at
/// its ends).
pub fn highlight(runs: Vec<TextRun>, range: Option<Range<usize>>, bg: Hsla) -> Vec<TextRun> {
    let Some(range) = range else { return runs };
    let mut out = Vec::with_capacity(runs.len() + 2);
    let mut at = 0;
    for run in runs {
        let (start, end) = (at, at + run.len);
        at = end;
        let cuts = [start, range.start.clamp(start, end), range.end.clamp(start, end), end];
        for w in cuts.windows(2) {
            if w[1] > w[0] {
                let selected = w[0] >= range.start && w[1] <= range.end;
                out.push(TextRun { len: w[1] - w[0], background_color: if selected { Some(bg) } else { run.background_color }, ..run.clone() });
            }
        }
    }
    out
}

/// Everything the block painter needs besides the blocks.
pub struct MdPaint {
    pub styles: MdStyles,
    pub family: SharedString,
    pub mono: SharedString,
    pub heading_family: SharedString,
    pub colors: RunColors,
    pub muted: Hsla,
    pub border: Hsla,
    pub code_block_bg: Option<Hsla>,
    /// Right-to-left: list markers and the quote bar sit on the right.
    pub rtl: bool,
    pub on_link: LinkHandler,
    /// A block image's source through the host's media policy (`None` =
    /// denied: the alt text paints).
    pub image: ImageResolver,
    /// The leaf's node index, the unit registry, the selection to paint.
    pub node: u32,
    pub units: Option<MdUnits>,
    pub selection: Option<MdSelection>,
    pub selection_bg: Hsla,
}

/// What a markdown link press calls (the href).
pub type LinkHandler = Rc<dyn Fn(&str, &mut Window, &mut App)>;
/// A block image's src → what `img()` paints (`None` = denied).
pub type ImageResolver = Rc<dyn Fn(&str) -> Option<gpui::ImageSource>>;

impl MdPaint {
    /// Colours from the theme (`Markdown/link`, `code`, `codeBlock`, `quote`).
    pub fn colors(theme: Option<&ResolvedTheme>, mode: Mode, ink: Hsla, owner_props: &Map<String, serde_json::Value>) -> (RunColors, Hsla, Hsla, Option<Hsla>) {
        let link = color_of(part_visual(theme, mode, "Markdown", "link", owner_props, &[]).color.as_deref()).or_else(|| theme_color(theme, mode, "primary")).unwrap_or(ink);
        let code_bg = color_of(part_visual(theme, mode, "Markdown", "code", owner_props, &[]).background_color.as_deref()).or_else(|| theme_color(theme, mode, "muted"));
        let block_bg = color_of(part_visual(theme, mode, "Markdown", "codeBlock", owner_props, &[]).background_color.as_deref()).or(code_bg);
        let quote = part_visual(theme, mode, "Markdown", "quote", owner_props, &[]);
        let muted = color_of(quote.color.as_deref()).or_else(|| theme_color(theme, mode, "mutedForeground")).unwrap_or(ink.opacity(0.7));
        let border = color_of(quote.border_color.as_deref()).or_else(|| theme_color(theme, mode, "border")).unwrap_or(ink.opacity(0.2));
        (RunColors { ink, link, code_bg }, muted, border, block_bg)
    }
}

/// A selectable text unit: registered in [`MdUnits`], the selection
/// highlighted in its runs, link ranges pressable.
fn unit_text(id: SharedString, text: SharedString, runs: Vec<TextRun>, links: Vec<(Range<usize>, String)>, p: &MdPaint) -> AnyElement {
    let unit = p.units.as_ref().map(|u| u.borrow().get(&p.node).map_or(0, Vec::len)).unwrap_or(0);
    let range = p.selection.filter(|s| s.node == p.node).and_then(|s| s.range_in(unit, text.len()));
    let styled = StyledText::new(text.clone()).with_runs(highlight(runs, range, p.selection_bg));
    if let Some(units) = &p.units {
        units.borrow_mut().entry(p.node).or_default().push((styled.layout().clone(), text));
    }
    if links.is_empty() {
        styled.into_any_element()
    } else {
        let on_link = p.on_link.clone();
        let hrefs: Vec<String> = links.iter().map(|(_, h)| h.clone()).collect();
        InteractiveText::new(id, styled)
            .on_click(links.into_iter().map(|(r, _)| r).collect(), move |ix, window, cx| {
                if let Some(h) = hrefs.get(ix) {
                    on_link(h, window, cx)
                }
            })
            .into_any_element()
    }
}

fn text_block(id: SharedString, inlines: &[Inline], spec: &TextSpec, family: &SharedString, p: &MdPaint, colors: RunColors) -> AnyElement {
    let (text, runs) = runs(inlines, family, &p.mono, spec.weight, colors);
    let links: Vec<(std::ops::Range<usize>, String)> = {
        let mut at = 0;
        let mut out = Vec::new();
        for span in inlines {
            if let Some(href) = &span.link {
                out.push((at..at + span.text.len(), href.clone()));
            }
            at += span.text.len();
        }
        out
    };
    let body = unit_text(id, text, runs, links, p);
    div().w_full().text_size(px(spec.size)).line_height(px(spec.line_height)).child(body).into_any_element()
}

/// The left edge of table column `c` of `cols` (mirrored in RTL: column 0
/// at the right, like an HTML table under `dir=rtl`).
pub fn table_cell_left(c: usize, cols: usize, col_w: f32, rtl: bool) -> f32 {
    let visual = if rtl { cols.saturating_sub(1).saturating_sub(c) } else { c };
    visual as f32 * col_w
}

/// Paint `blocks` laid out at `width` (`lay` from [`layout`] at that width).
pub fn paint(id: &str, blocks: &[Block], lay: &MdLayout, width: f32, p: &MdPaint) -> AnyElement {
    let s = &p.styles;
    if let Some(units) = &p.units {
        units.borrow_mut().insert(p.node, Vec::new());
    }
    let mut root = div().relative().w(px(width)).h(px(lay.height));
    for (i, (b, bx)) in blocks.iter().zip(&lay.blocks).enumerate() {
        let bid: SharedString = format!("{id}.md{i}").into();
        let placed = div().absolute().left_0().top(px(bx.y)).w(px(width)).h(px(bx.height));
        let el: AnyElement = match &b.kind {
            BlockKind::Paragraph => placed.when(p.rtl, |d| d.text_right()).child(text_block(bid, &b.inlines, &s.body, &p.family, p, p.colors)).into_any_element(),
            BlockKind::Heading(_) => placed.when(p.rtl, |d| d.text_right()).child(text_block(bid, &b.inlines, &s.heading, &p.heading_family, p, p.colors)).into_any_element(),
            BlockKind::ListItem { marker, task, depth } => {
                let marker = match task {
                    Some(true) => "☑".to_string(),
                    Some(false) => "☐".to_string(),
                    None if *depth > 0 && marker == "•" => "◦".to_string(),
                    None => marker.clone(),
                };
                let lead = s.list_indent * *depth as f32;
                let body_w = (width - lead - s.list_indent).max(1.0);
                let (marker_x, body_x) = if p.rtl { (width - lead - s.list_indent, 0.0) } else { (lead, lead + s.list_indent) };
                placed
                    .child(div().absolute().left(px(marker_x)).top_0().w(px(s.list_indent)).text_size(px(s.body.size)).line_height(px(s.body.line_height)).text_color(p.colors.ink).when(p.rtl, |d| d.text_right()).child(SharedString::from(marker)))
                    .child(div().absolute().left(px(body_x)).top_0().w(px(body_w)).when(p.rtl, |d| d.text_right()).child(text_block(bid, &b.inlines, &s.body, &p.family, p, p.colors)))
                    .into_any_element()
            }
            BlockKind::Image { src, alt } => {
                let alt_el = text_block(bid.clone(), &b.inlines, &s.body, &p.family, p, RunColors { ink: p.muted, ..p.colors });
                match (p.image)(src) {
                    Some(source) => {
                        let fallback_alt: SharedString = alt.clone().into();
                        let muted = p.muted;
                        placed
                            .child(
                                gpui::img(source)
                                    .size_full()
                                    .object_fit(gpui::ObjectFit::Contain)
                                    .with_fallback(move || div().text_color(muted).child(fallback_alt.clone()).into_any_element()),
                            )
                            .into_any_element()
                    }
                    None => placed.child(alt_el).into_any_element(),
                }
            }
            BlockKind::Quote => placed
                .child(div().absolute().left(px(if p.rtl { width - s.quote_border } else { 0.0 })).top_0().h_full().w(px(s.quote_border)).bg(p.border))
                .child(
                    div()
                        .absolute()
                        .left(px(if p.rtl { 0.0 } else { s.quote_border + s.quote_pad }))
                        .when(p.rtl, |d| d.text_right())
                        .top_0()
                        .w(px((width - s.quote_border - s.quote_pad).max(1.0)))
                        .child(text_block(bid, &b.inlines, &s.body, &p.family, p, RunColors { ink: p.muted, ..p.colors })),
                )
                .into_any_element(),
            BlockKind::CodeBlock => {
                let text: SharedString = b.inlines.first().map(|i| i.text.clone()).unwrap_or_default().into();
                let font = Font { family: p.mono.clone(), features: Default::default(), fallbacks: None, weight: FontWeight(s.code.weight as f32), style: FontStyle::Normal };
                let run = TextRun { len: text.len(), font, color: p.colors.ink, background_color: None, underline: None, strikethrough: None };
                let runs = if text.is_empty() { Vec::new() } else { vec![run] };
                placed
                    .rounded(px(6.0))
                    .when_some(p.code_block_bg, |d, bg| d.bg(bg))
                    .overflow_hidden()
                    .child(
                        div()
                            .absolute()
                            .left(px(s.code_pad))
                            .top(px(s.code_pad))
                            .text_size(px(s.code.size))
                            .line_height(px(s.code.line_height))
                            .whitespace_nowrap()
                            .child(unit_text(bid, text, runs, Vec::new(), p)),
                    )
                    .into_any_element()
            }
            BlockKind::Rule => placed.bg(p.border).into_any_element(),
            BlockKind::Table { header, rows } => {
                let cols = header.len().max(rows.iter().map(Vec::len).max().unwrap_or(0)).max(1);
                let col_w = width / cols as f32;
                let mut table = placed.border_1().border_color(p.border).rounded(px(6.0)).overflow_hidden();
                let mut y = 0.0;
                for (r, row) in std::iter::once(header).chain(rows.iter()).enumerate() {
                    let h = bx.rows.get(r).copied().unwrap_or(s.body.line_height);
                    for c in 0..cols {
                        let cell = row.get(c).cloned().unwrap_or_default();
                        let weight = if r == 0 { s.body.weight.max(600) } else { s.body.weight };
                        let spec = TextSpec { weight, ..s.body.clone() };
                        // RTL: column 0 sits at the right, the separator on
                        // the cell's right side, the text right-aligned.
                        table = table.child(
                            div()
                                .absolute()
                                .left(px(table_cell_left(c, cols, col_w, p.rtl)))
                                .top(px(y))
                                .w(px(col_w))
                                .h(px(h))
                                .when(r > 0, |d| d.border_t_1().border_color(p.border))
                                .when(c > 0 && !p.rtl, |d| d.border_l_1().border_color(p.border))
                                .when(c > 0 && p.rtl, |d| d.border_r_1().border_color(p.border))
                                .child(
                                    div()
                                        .absolute()
                                        .left(px(s.cell_pad_h))
                                        .top(px(s.cell_pad_v))
                                        .w(px((col_w - 2.0 * s.cell_pad_h).max(1.0)))
                                        .when(p.rtl, |d| d.text_right())
                                        .child(text_block(format!("{bid}.{r}.{c}").into(), &cell, &spec, &p.family, p, p.colors)),
                                ),
                        );
                    }
                    y += h;
                }
                table.into_any_element()
            }
        };
        root = root.child(el);
    }
    root.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_columns_mirror_in_rtl() {
        assert_eq!((0..3).map(|c| table_cell_left(c, 3, 100.0, false)).collect::<Vec<_>>(), [0.0, 100.0, 200.0]);
        assert_eq!((0..3).map(|c| table_cell_left(c, 3, 100.0, true)).collect::<Vec<_>>(), [200.0, 100.0, 0.0], "column 0 = the right edge");
        assert_eq!(table_cell_left(0, 1, 80.0, true), 0.0);
    }

    /// 8 px per character, the core's fixed measure, wrapping greedily.
    struct Fixed;
    impl MdText for Fixed {
        fn height(&mut self, inlines: &[Inline], spec: &TextSpec, width: Option<f32>) -> f32 {
            let text = plain(inlines);
            let lines: f32 = text
                .split('\n')
                .map(|l| match width {
                    None => 1.0,
                    Some(w) => ((l.chars().count() as f32 * 8.0) / w).ceil().max(1.0),
                })
                .sum();
            lines * spec.line_height
        }
        fn width(&mut self, inlines: &[Inline], _: &TextSpec) -> f32 {
            plain(inlines).chars().count() as f32 * 8.0
        }
        fn widest_word(&mut self, inlines: &[Inline], _: &TextSpec) -> f32 {
            plain(inlines).split_whitespace().map(|w| w.chars().count()).max().unwrap_or(0) as f32 * 8.0
        }
    }

    fn body() -> TextSpec {
        TextSpec { size: 14.0, line_height: 20.0, weight: 400, family: None }
    }

    #[test]
    fn selections_cover_units_in_document_order() {
        let sel = MdSelection { node: 1, anchor: (2, 3), head: (0, 4) };
        assert_eq!(sel.ordered(), ((0, 4), (2, 3)));
        assert_eq!(sel.range_in(0, 10), Some(4..10));
        assert_eq!(sel.range_in(1, 6), Some(0..6));
        assert_eq!(sel.range_in(2, 10), Some(0..3));
        assert_eq!(sel.range_in(3, 10), None);
        let empty = MdSelection { node: 1, anchor: (1, 2), head: (1, 2) };
        assert!(empty.is_empty() && empty.range_in(1, 5).is_none());
        let font = Font { family: "x".into(), features: Default::default(), fallbacks: None, weight: FontWeight::NORMAL, style: FontStyle::Normal };
        let run = |len| TextRun { len, font: font.clone(), color: gpui::black(), background_color: None, underline: None, strikethrough: None };
        let out = highlight(vec![run(4), run(6)], Some(2..7), gpui::red());
        assert_eq!(out.iter().map(|r| (r.len, r.background_color.is_some())).collect::<Vec<_>>(), vec![(2, false), (2, true), (3, true), (3, false)]);
    }

    #[test]
    fn inline_spans_parse() {
        let spans = parse_inline("**Bold** and *it* with `code` and [a link](https://x.y) ~~gone~~");
        assert_eq!(plain(&spans), "Bold and it with code and a link gone");
        assert!(spans[0].bold);
        assert!(spans.iter().any(|s| s.italic && s.text == "it"));
        assert!(spans.iter().any(|s| s.code && s.text == "code"));
        assert!(spans.iter().any(|s| s.link.as_deref() == Some("https://x.y") && s.text == "a link"));
        assert!(spans.iter().any(|s| s.strike && s.text == "gone"));
        assert_eq!(plain(&parse_inline("snake_case_name stays")), "snake_case_name stays");
    }

    #[test]
    fn blocks_parse() {
        let doc = "# Title\n\nPara one\ncontinues.\n\n- a\n- [x] b\n1. first\n\n> quoted\n\n```\nlet x = 1;\n```\n\n| a | b |\n| --- | :-: |\n| 1 | 2 \\| 3 |\n\n---";
        let blocks = parse(doc);
        let kinds: Vec<&str> = blocks
            .iter()
            .map(|b| match &b.kind {
                BlockKind::Paragraph => "p",
                BlockKind::Heading(_) => "h",
                BlockKind::ListItem { .. } => "li",
                BlockKind::Quote => "q",
                BlockKind::CodeBlock => "code",
                BlockKind::Table { .. } => "table",
                BlockKind::Rule => "hr",
                BlockKind::Image { .. } => "img",
            })
            .collect();
        assert_eq!(kinds, ["h", "p", "li", "li", "li", "q", "code", "table", "hr"]);
        assert_eq!(plain(&blocks[1].inlines), "Para one continues.");
        assert_eq!(blocks[3].kind, BlockKind::ListItem { marker: "•".into(), task: Some(true), depth: 0 });
        assert_eq!(blocks[2].list_group, blocks[4].list_group);
        if let BlockKind::Table { rows, .. } = &blocks[7].kind {
            assert_eq!(plain(&rows[0][1]), "2 | 3");
        } else {
            panic!("table");
        }
    }

    #[test]
    fn destinations_balance_parentheses_like_commonmark() {
        assert_eq!(link_at("[a](https://x/A_(b))"), Some(("a", "https://x/A_(b)".to_string(), 20)));
        assert_eq!(link_at("[a](a(b)) rest").map(|l| l.1), Some("a(b)".to_string()));
        assert_eq!(link_at("[i](javascript:alert(3))").map(|l| l.1), Some("javascript:alert(3)".to_string()));
        assert_eq!(link_at("[a](x \"title\")").map(|l| l.1), Some("x".to_string()));
        assert_eq!(link_at("[a](<x y>)").map(|l| l.1), Some("x y".to_string()));
        assert!(link_at("[a](x y)").is_none());
        assert!(link_at("[a](x(").is_none());
        let spans = parse_inline("see [w](https://en.wikipedia.org/wiki/A_(b)) end");
        assert!(spans.iter().any(|s| s.link.as_deref() == Some("https://en.wikipedia.org/wiki/A_(b)") && s.text == "w"));
        assert_eq!(plain(&spans), "see w end");
    }

    #[test]
    fn lists_nest_by_marker_indentation() {
        let blocks = parse("- a\n  - a1\n    1. deep\n  - a2\n- b");
        let depths: Vec<u8> = blocks.iter().map(|b| if let BlockKind::ListItem { depth, .. } = b.kind { depth } else { 99 }).collect();
        assert_eq!(depths, [0, 1, 2, 1, 0]);
        assert_eq!(blocks[2].kind, BlockKind::ListItem { marker: "1.".into(), task: None, depth: 2 });
        let s = MdStyles::plain(body());
        // A nested item wraps narrower: "a1" fits, its indent counts.
        assert_eq!(max_content_width(&parse("- a\n  - ab"), &s, &mut Fixed), 16.0 + 2.0 * s.list_indent);
    }

    #[test]
    fn whole_line_images_are_blocks_and_the_policy_resolves_them() {
        let blocks = parse("![chart](https://exponential.at/c.png)\n\ntext ![inline](https://x/y.png) [bad](javascript:alert(1)) [ok](https://exponential.at)");
        assert_eq!(blocks[0].kind, BlockKind::Image { src: "https://exponential.at/c.png".into(), alt: "chart".into() });
        assert_eq!(plain(&blocks[1].inlines), "text inline bad ok", "an inline image is its alt text");
        let s = MdStyles::plain(body());
        assert_eq!(layout(&blocks[..1], &s, 300.0, &mut Fixed).height, s.image_height);
        let resolved = resolve_urls(blocks, |h| h.starts_with("https:"), |_| false);
        assert_eq!(resolved[0].kind, BlockKind::Paragraph, "a denied image is its alt text");
        assert_eq!(plain(&resolved[0].inlines), "chart");
        let links: Vec<Option<&str>> = resolved[1].inlines.iter().map(|s| s.link.as_deref()).filter(Option::is_some).collect();
        assert_eq!(links, [Some("https://exponential.at")], "a denied link is plain text");
        let denied = parse("![x](javascript:alert(3))");
        assert_eq!(resolve_urls(denied, |_| true, |src| !src.starts_with("javascript:"))[0].kind, BlockKind::Paragraph);
    }

    #[test]
    fn the_height_estimate_counts_a_gap_per_block_and_none_inside_a_list() {
        let s = MdStyles::plain(body());
        // "Looking…" (20 chars = 160 px) at 100 px wraps to 2 lines.
        let blocks = parse("Looking for an alter\n\n- one\n- two\n\nEnd");
        let lay = layout(&blocks, &s, 100.0, &mut Fixed);
        // p: 40, gap 8, li 20, li 20 (no gap), gap 8, p 20.
        assert_eq!(lay.height, 40.0 + 8.0 + 20.0 + 20.0 + 8.0 + 20.0);
        assert_eq!(lay.blocks[2].y, 68.0);
        // A heading collapses its top margin with the paragraph's bottom.
        let h = layout(&parse("para\n\n## Head"), &s, 400.0, &mut Fixed);
        assert_eq!(h.blocks[1].y, 20.0 + 12.0);
        // Code blocks are padded and never wrap.
        let c = layout(&parse("```\na\nb\n```"), &s, 10.0, &mut Fixed);
        assert_eq!(c.height, 2.0 * s.code.line_height + 2.0 * s.code_pad);
        assert_eq!(min_content_width(&parse("short loooooong"), &s, &mut Fixed), 72.0);
        assert_eq!(max_content_width(&parse("- ab"), &s, &mut Fixed), 16.0 + s.list_indent);
    }
}
