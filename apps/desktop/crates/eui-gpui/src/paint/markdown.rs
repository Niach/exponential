//! The built-in `Markdown` painter: a small GFM subset (paragraphs, headings,
//! bullet / numbered / task lists nested by marker indentation, block quotes,
//! fenced code, tables, rules, block images; bold / italic / strike / code /
//! links inline) laid out as absolutely placed blocks. VAPP-103: link and
//! image destinations parse balanced parentheses (CommonMark); a link the
//! URL policy denies paints as text, an image the media policy denies as
//! its alt text ([`resolve_images`]); #FEED-208: an image directly in a
//! top-level paragraph's text splits it around a block image, anywhere else
//! (a heading, list item, cell, quote, or inside a span) it paints its alt
//! text. The SAME block layout answers the measurer and places the
//! painted blocks, so the height taffy reserves is the height painted
//! (the spike's markdown estimate was 7 px short until it counted a gap per
//! block). No HTML passthrough: tags are text.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use eui_core::recipes::markdown_inline_recipes;
use eui_core::theme::{Mode, RecipeQuery, ResolvedTheme};
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
    /// #VAPP-258: the host's chip for this text link (`catalog/host.json`
    /// markdown): painted as the Chip look inline in the run.
    pub chip: Option<InlineChip>,
}

/// #VAPP-258: an inline chip (a policed `textLinks` segment's `chip`). Its
/// run's `text` is the source text until [`size_chips`] puts in the
/// non-breaking placeholder that reserves the chip's width in the line, so
/// the text system wraps it like one unbreakable word (measure = paint).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct InlineChip {
    pub kind: String,
    pub label: String,
    pub icon: Option<String>,
    /// #VAPP-258: the identifier painted mono before the label.
    pub ident: Option<String>,
    /// The text the run stood for (`#ABC-1`).
    pub source: String,
    /// The chip box width ([`size_chips`]; 0 before).
    pub width: f32,
    /// Capped at the wrap width: the label truncates.
    pub capped: bool,
}

/// #VAPP-258: what a one-link paragraph became (`attachment(href)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentKind {
    Video,
    Audio,
    File,
}

impl AttachmentKind {
    pub fn parse(kind: &str) -> Option<AttachmentKind> {
        match kind {
            "video" => Some(AttachmentKind::Video),
            "audio" => Some(AttachmentKind::Audio),
            "file" => Some(AttachmentKind::File),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            AttachmentKind::Video => "video",
            AttachmentKind::Audio => "audio",
            AttachmentKind::File => "file",
        }
    }
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
    /// A paragraph that is one `![alt](src)` (#FEED-208: or an image directly
    /// in a top-level paragraph's text, which splits around it): painted
    /// through the host's media policy and loader in an `image_height` box
    /// (the alt text there while it loads or when it fails). #VAPP-258:
    /// `size` = the host's `image_size(src)` (the box keeps its aspect,
    /// [`image_box`]).
    Image { src: String, alt: String, size: Option<(f32, f32)> },
    /// #VAPP-258: a top-level paragraph that is exactly one link the host
    /// classified (`attachment(href)`): the Video / AudioPlayer / FileUpload
    /// file row look. `href` = the policed destination.
    Attachment { kind: AttachmentKind, name: String, href: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub kind: BlockKind,
    pub inlines: Vec<Inline>,
    /// Consecutive list items share a group (no paragraph gap inside it).
    pub list_group: Option<usize>,
    /// #VAPP-258: a code block's fence info string (` ```ts ` = `ts`).
    pub lang: Option<String>,
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

/// #VAPP-258 (markdown-render.json): the common named entities and numeric
/// references decoded (text, never code).
pub fn decode_entities(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.contains('&') {
        return std::borrow::Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let decoded = tail[1..].find(';').filter(|&n| n > 0 && n <= 9).and_then(|n| {
            let body = &tail[1..1 + n];
            let ch = if let Some(num) = body.strip_prefix('#') {
                let code = match num.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => num.parse::<u32>().ok(),
                };
                code.filter(|c| *c > 0).and_then(char::from_u32).map(String::from)
            } else {
                match body {
                    "amp" => Some("&".to_string()),
                    "lt" => Some("<".to_string()),
                    "gt" => Some(">".to_string()),
                    "quot" => Some("\"".to_string()),
                    "apos" => Some("'".to_string()),
                    "nbsp" => Some("\u{a0}".to_string()),
                    _ => None,
                }
            };
            ch.map(|c| (c, n + 2))
        });
        match decoded {
            Some((c, n)) => {
                out.push_str(&c);
                rest = &tail[n..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    std::borrow::Cow::Owned(out)
}

fn push_text(out: &mut Vec<Inline>, raw: &str, f: Flags, link: Option<&str>) {
    if raw.is_empty() {
        return;
    }
    let decoded = decode_entities(raw);
    let text: &str = &decoded;
    if let Some(last) = out.last_mut() {
        if !last.code && last.bold == f.bold && last.italic == f.italic && last.strike == f.strike && last.link.as_deref() == link {
            last.text.push_str(text);
            return;
        }
    }
    out.push(Inline { text: text.to_string(), bold: f.bold, italic: f.italic, code: false, strike: f.strike, link: link.map(str::to_string), chip: None });
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
    link_closed_at(s, i)
}

/// [`link_at`] with the label's closing `]` already found at `i`
/// (`s.len()` or more = none).
fn link_closed_at(s: &str, i: usize) -> Option<(&str, String, usize)> {
    let b = s.as_bytes();
    if i >= b.len() || b.get(i + 1) != Some(&b'(') {
        return None;
    }
    let label = &s[1..i];
    let mut j = i + 2;
    let dest = if b.get(j) == Some(&b'<') {
        // The first `>`, `<` or newline ends the scan (a run of unclosed
        // `<` destinations stays linear).
        let close = s[j + 1..].find(['>', '<', '\n'])? + j + 1;
        if b[close] != b'>' {
            return None;
        }
        let inner = &s[j + 1..close];
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
                // CommonMark lets an implementation cap the nesting (at
                // least 32): every scan stays short.
                parens += 1;
                if parens > MAX_NESTING {
                    return None;
                }
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
/// media policy refuses becomes a paragraph of its alt text (dropped
/// without one; the measurer and the painter both resolve, so a denied
/// image never reserves a box);
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
        .filter_map(|mut b| {
            if let BlockKind::Image { src, alt, .. } = &b.kind {
                if !image_allowed(src) {
                    if alt.is_empty() {
                        return None;
                    }
                    b = Block { kind: BlockKind::Paragraph, inlines: vec![Inline { text: alt.clone(), ..Inline::default() }], list_group: None, lang: None };
                }
            }
            strip(&mut b.inlines);
            if let BlockKind::Table { header, rows } = &mut b.kind {
                header.iter_mut().chain(rows.iter_mut().flatten()).for_each(&strip);
            }
            Some(b)
        })
        .collect()
}

/// The blocks of `text` under the host's URL and media policies and its
/// Markdown options (#VAPP-258 step 5: hard breaks, a streamed prefix).
pub fn parse_for(host: &dyn crate::host::HostPlugin, text: &str, cx: &App) -> Vec<Block> {
    let urls = host.url_policy();
    let options = host.markdown_options(cx);
    resolve_urls(parse_with(text, options.hard_breaks, options.streaming), |h| eui_core::host::safe_href(urls.as_ref(), h).is_some(), |src| crate::media::allowed_request(host, src).is_some())
}

/// `fixtures/markdown-text-links.json`: every text run through a host's text
/// links: `segments` = one piece's answer, policed, `None` = text. Code, link
/// labels, block images and bare urls never ask; only the marks change, so the
/// layout is the same. `None` = nothing linked (the blocks stand).
pub fn link_text(blocks: &[Block], segments: impl Fn(&str) -> Option<Vec<eui_core::host::TextLink>>) -> Option<Vec<Block>> {
    let mut linked = false;
    let mut spans = |list: &[Inline]| -> Vec<Inline> {
        let mut out = Vec::with_capacity(list.len());
        for span in list {
            if span.code || span.link.is_some() {
                out.push(span.clone());
                continue;
            }
            for (piece, is_url) in bare_url_pieces(&span.text) {
                match if is_url { None } else { segments(piece) } {
                    Some(answer) => {
                        linked = true;
                        out.extend(answer.into_iter().map(|segment| {
                            // #VAPP-258: a chip only on an allowed href, a known kind and a label.
                            let chip = segment.chip.filter(|chip| segment.href.is_some() && chip.is_valid()).map(|chip| InlineChip { kind: chip.kind, label: chip.label, icon: chip.icon, ident: chip.ident.filter(|i| !i.is_empty()), source: segment.text.clone(), ..InlineChip::default() });
                            Inline { text: segment.text, link: segment.href, chip, ..span.clone() }
                        }));
                    }
                    None => out.push(Inline { text: piece.to_string(), ..span.clone() }),
                }
            }
        }
        out
    };
    let out: Vec<Block> = blocks
        .iter()
        .map(|b| {
            let mut b = b.clone();
            match &mut b.kind {
                BlockKind::Table { header, rows } => {
                    for cell in header.iter_mut().chain(rows.iter_mut().flatten()) {
                        *cell = spans(&cell[..]);
                    }
                }
                BlockKind::CodeBlock | BlockKind::Rule | BlockKind::Image { .. } | BlockKind::Attachment { .. } => {}
                _ => b.inlines = spans(&b.inlines[..]),
            }
            b
        })
        .collect();
    linked.then_some(out)
}

/// #VAPP-258 (`catalog/host.json` markdown): what the host answers a
/// Markdown, already narrowed to plain closures (each asked with the RAW
/// destination or text, by the measurer and the painter alike).
pub struct MdHooks<'a> {
    /// `attachment(href)` (unpoliced: [`apply_hooks`] checks the kind).
    pub attachment: &'a dyn Fn(&str) -> Option<crate::host::MarkdownAttachment>,
    /// `image_size(src)` (unchecked: [`image_box`] drops a bad size).
    pub image_size: &'a dyn Fn(&str) -> Option<(f32, f32)>,
    /// The URL policy: the href a press navigates to, `None` = denied.
    pub href: &'a dyn Fn(&str) -> Option<String>,
    /// `text_links(piece)`, POLICED ([`eui_core::host::police_text_links`]).
    pub text_links: &'a dyn Fn(&str) -> Option<Vec<eui_core::host::TextLink>>,
}

/// A size the image box may use: both sides finite and positive.
fn usable_size(size: Option<(f32, f32)>) -> Option<(f32, f32)> {
    size.filter(|(w, h)| w.is_finite() && h.is_finite() && *w > 0.0 && *h > 0.0)
}

/// #VAPP-258: a block image's box `(width, height)` in `available` px. With
/// a `max_height` cap: the natural size scaled by min(1, available / width,
/// max_height / height), else the fixed box (available × max_height). With
/// none: the available width at the size's aspect (4:3 unknown), scaled down
/// to `viewport` when taller (`None` = no limit).
pub fn image_box(size: Option<(f32, f32)>, available: f32, max_height: Option<f32>, viewport: Option<f32>) -> (f32, f32) {
    let Some(max_height) = max_height else {
        let ratio = usable_size(size).map_or(eui_core::host::MARKDOWN_UNSIZED_ASPECT as f32, |(w, h)| h / w);
        let h = available * ratio;
        return match viewport.filter(|v| v.is_finite() && *v > 0.0) {
            Some(v) if h > v => (v / ratio, v),
            _ => (available, h),
        };
    };
    match usable_size(size) {
        Some((w, h)) => {
            let k = 1f32.min(available / w).min(max_height / h);
            (w * k, h * k)
        }
        None => (available, max_height),
    }
}

/// The one link a paragraph is made of: every run carries it, nothing else.
fn sole_link(inlines: &[Inline]) -> Option<&str> {
    let first = inlines.first()?.link.as_deref()?;
    inlines.iter().all(|span| span.link.as_deref() == Some(first)).then_some(first)
}

/// #VAPP-258: the host's hooks over policed blocks ([`resolve_urls`]):
/// a top-level one-link paragraph the host classifies becomes an
/// [`BlockKind::Attachment`] (asked before the text links), a block image
/// gets the host's size, then every text run asks the text links
/// ([`link_text`], chips included). `None` = nothing changed.
pub fn apply_hooks(blocks: &[Block], hooks: &MdHooks) -> Option<Vec<Block>> {
    let mut changed = false;
    let hosted: Vec<Block> = blocks
        .iter()
        .map(|b| match &b.kind {
            BlockKind::Paragraph => {
                let attachment = sole_link(&b.inlines).and_then(|raw| {
                    let answer = (hooks.attachment)(raw)?;
                    let kind = AttachmentKind::parse(&answer.kind)?;
                    let href = (hooks.href)(raw)?;
                    (!answer.name.is_empty()).then_some(BlockKind::Attachment { kind, name: answer.name, href })
                });
                match attachment {
                    Some(kind) => {
                        changed = true;
                        Block { kind, inlines: Vec::new(), list_group: None, lang: None }
                    }
                    None => b.clone(),
                }
            }
            BlockKind::Image { src, alt, .. } => match usable_size((hooks.image_size)(src)) {
                Some(size) => {
                    changed = true;
                    Block { kind: BlockKind::Image { src: src.clone(), alt: alt.clone(), size: Some(size) }, ..b.clone() }
                }
                None => b.clone(),
            },
            _ => b.clone(),
        })
        .collect();
    match link_text(&hosted, hooks.text_links) {
        Some(linked) => Some(linked),
        None => changed.then_some(hosted),
    }
}

/// [`apply_hooks`] with a host's answers under its URL policy.
pub fn hosted(host: &dyn crate::host::HostPlugin, blocks: &[Block], cx: &App) -> Option<Vec<Block>> {
    let urls = host.url_policy();
    apply_hooks(
        blocks,
        &MdHooks {
            attachment: &|href| host.attachment(href, cx),
            // #VAPP-258 (markdown-host.json `loaded`): an unknown size is the
            // decoded one once the picture loaded (the media cache).
            image_size: &|src| usable_size(host.image_size(src, cx)).or_else(|| crate::media::loaded_size(src)),
            href: &|href| eui_core::host::safe_href(urls.as_ref(), href),
            text_links: &|piece| host.text_links(piece, cx).and_then(|links| eui_core::host::police_text_links(urls.as_ref(), piece, links)),
        },
    )
}

/// A run cut around its bare urls (`http://` or `https://`, any case, not glued
/// to an ASCII word character, up to the next ASCII space, tab or newline); no
/// empty piece.
fn bare_url_pieces(text: &str) -> Vec<(&str, bool)> {
    let bytes = text.as_bytes();
    let starts = |i: usize, prefix: &str| bytes.len() >= i + prefix.len() && bytes[i..i + prefix.len()].eq_ignore_ascii_case(prefix.as_bytes());
    let mut out = Vec::new();
    let (mut at, mut i) = (0, 0);
    while i < bytes.len() {
        let glued = i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
        if glued || !(starts(i, "http://") || starts(i, "https://")) {
            i += 1;
            continue;
        }
        let end = bytes[i..].iter().position(|c| matches!(c, b' ' | b'\t' | b'\n')).map_or(bytes.len(), |n| i + n);
        if i > at {
            out.push((&text[at..i], false));
        }
        out.push((&text[i..end], true));
        at = end;
        i = end;
    }
    if at < bytes.len() {
        out.push((&text[at..], false));
    }
    out
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

/// Link labels (and destination parentheses) nest at most this deep; a
/// deeper `[label](href)` is text (100k nested `[` aborted on the stack and
/// cost O(n²)).
const MAX_NESTING: i32 = 32;

/// One inline parse: the text and, per byte, the closing `]` of the `[`
/// there (`usize::MAX` = none), found ONCE (escapes skip the next byte), so
/// a run of unclosed or nested brackets stays linear.
struct InlineSrc<'a> {
    s: &'a str,
    close: Vec<usize>,
}

impl<'a> InlineSrc<'a> {
    fn new(s: &'a str) -> Self {
        let b = s.as_bytes();
        let mut close = vec![usize::MAX; b.len()];
        let mut open: Vec<usize> = Vec::new();
        let mut i = 0;
        while i < b.len() {
            match b[i] {
                b'\\' => i += 1,
                b'[' => open.push(i),
                b']' => {
                    if let Some(o) = open.pop() {
                        close[o] = i;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        InlineSrc { s, close }
    }

    /// The link at byte `i` inside `[i, end)`: (label range, href, consumed).
    fn link(&self, i: usize, end: usize) -> Option<(Range<usize>, String, usize)> {
        let c = self.close[i];
        if c >= end {
            return None;
        }
        let (_, href, n) = link_closed_at(&self.s[i..end], c - i)?;
        Some((i + 1..c, href, n))
    }
}

/// #FEED-208: a top-level paragraph cut at the images directly in its text.
enum Piece {
    /// The spans before an image.
    Text(Vec<Inline>),
    Image { src: String, alt: String },
}

/// The spans of `src.s[start..end]` (`depth` = enclosing link labels).
/// #FEED-208: `images` (only a top-level paragraph's own frame, never inside
/// a span or a link label) takes each image directly in the text: the spans
/// so far move into it, then the image.
#[allow(clippy::too_many_arguments)]
fn parse_inline_into(src: &InlineSrc, start: usize, end: usize, f: Flags, link: Option<&str>, depth: i32, out: &mut Vec<Inline>, mut images: Option<&mut Vec<Piece>>) {
    let s = &src.s[..end];
    let mut plain_start = start;
    let mut i = start;
    let bytes = s.as_bytes();
    while i < s.len() {
        if !s.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let rest = &s[i..];
        let c = bytes[i];
        let mut handled: Option<usize> = None;
        if c == b'\\' && bytes.get(i + 1).is_some_and(u8::is_ascii_punctuation) && i + 1 < s.len() {
            // #VAPP-258: a backslash-escaped punctuation character is literal.
            push_text(out, &s[plain_start..i], f, link);
            push_text(out, &s[i + 1..i + 2], f, link);
            handled = Some(2);
        } else if c == b'`' {
            if let Some(close) = rest[1..].find('`') {
                push_text(out, &s[plain_start..i], f, link);
                out.push(Inline { text: rest[1..1 + close].to_string(), code: true, link: link.map(str::to_string), ..Inline::default() });
                handled = Some(close + 2);
            }
        } else if c == b'!' && rest[1..].starts_with('[') {
            // #FEED-208: an image directly in a top-level paragraph's text
            // splits it (`images`); anywhere else it paints its alt text.
            if let Some((alt, dest, n)) = src.link(i + 1, end) {
                push_text(out, &s[plain_start..i], f, link);
                match images.as_deref_mut() {
                    Some(pieces) => {
                        pieces.push(Piece::Text(std::mem::take(out)));
                        pieces.push(Piece::Image { src: dest, alt: decode_entities(&s[alt]).into_owned() });
                    }
                    None => push_text(out, &s[alt], f, link),
                }
                handled = Some(n + 1);
            }
        } else if c == b'[' && depth < MAX_NESTING {
            if let Some((label, href, n)) = src.link(i, end) {
                push_text(out, &s[plain_start..i], f, link);
                parse_inline_into(src, label.start, label.end, f, Some(href.as_str()), depth + 1, out, None);
                handled = Some(n);
            }
        } else if (c == b'*' || c == b'_' || c == b'~') && !(c == b'_' && s[..i].chars().next_back().is_some_and(char::is_alphanumeric)) {
            let double = match c {
                b'*' => "**",
                b'_' => "__",
                _ => "~~",
            };
            let triple = if c == b'~' { None } else { delimited(rest, if c == b'*' { "***" } else { "___" }) };
            if let Some((inner, n)) = triple {
                // `***x***` is both.
                push_text(out, &s[plain_start..i], f, link);
                parse_inline_into(src, i + 3, i + 3 + inner.len(), Flags { bold: true, italic: true, ..f }, link, depth, out, None);
                handled = Some(n);
            } else if let Some((inner, n)) = delimited(rest, double) {
                push_text(out, &s[plain_start..i], f, link);
                let nf = if c == b'~' { Flags { strike: true, ..f } } else { Flags { bold: true, ..f } };
                let at = i + double.len();
                parse_inline_into(src, at, at + inner.len(), nf, link, depth, out, None);
                handled = Some(n);
            } else if c != b'~' {
                let single = if c == b'*' { "*" } else { "_" };
                if let Some((inner, n)) = delimited(rest, single) {
                    push_text(out, &s[plain_start..i], f, link);
                    parse_inline_into(src, i + 1, i + 1 + inner.len(), Flags { italic: true, ..f }, link, depth, out, None);
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
    let src = InlineSrc::new(s);
    parse_inline_into(&src, 0, s.len(), Flags::default(), None, 0, &mut out, None);
    out
}

/// #FEED-208 (markdown-render.json `image_split_*`): a top-level paragraph
/// with images directly in its text = the text before each image (a
/// paragraph), the image (a block image), the text after it. ASCII spaces,
/// tabs and line breaks at a split edge trim; an empty side drops.
fn split_images(text: &str) -> Vec<Block> {
    let paragraph = |inlines: Vec<Inline>| Block { kind: BlockKind::Paragraph, inlines, list_group: None, lang: None };
    let mut pieces = Vec::new();
    let mut tail = Vec::new();
    let src = InlineSrc::new(text);
    parse_inline_into(&src, 0, text.len(), Flags::default(), None, 0, &mut tail, Some(&mut pieces));
    if pieces.is_empty() {
        return vec![paragraph(tail)];
    }
    pieces.push(Piece::Text(tail));
    let edge: &[char] = &[' ', '\t', '\n'];
    let plain_run = |i: &Inline| !i.bold && !i.italic && !i.code && !i.strike && i.link.is_none();
    pieces
        .into_iter()
        .filter_map(|piece| match piece {
            Piece::Text(mut inlines) => {
                while let Some(first) = inlines.first_mut().filter(|i| plain_run(&**i)) {
                    let cut = first.text.len() - first.text.trim_start_matches(edge).len();
                    first.text.replace_range(..cut, "");
                    if !first.text.is_empty() {
                        break;
                    }
                    inlines.remove(0);
                }
                while let Some(last) = inlines.last_mut().filter(|i| plain_run(&**i)) {
                    let kept = last.text.trim_end_matches(edge).len();
                    last.text.truncate(kept);
                    if !last.text.is_empty() {
                        break;
                    }
                    inlines.pop();
                }
                (!inlines.is_empty()).then(|| paragraph(inlines))
            }
            Piece::Image { src, alt } => {
                let inlines = if alt.is_empty() { Vec::new() } else { vec![Inline { text: alt.clone(), ..Inline::default() }] };
                Some(Block { kind: BlockKind::Image { src, alt, size: None }, inlines, list_group: None, lang: None })
            }
        })
        .collect()
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

/// A line that opens a block of its own (never a continuation).
fn starts_block(line: &str) -> bool {
    let t = line.trim();
    let hashes = t.bytes().take_while(|b| *b == b'#').count();
    t.starts_with("```") || t.starts_with("~~~") || t.starts_with('>') || is_table_row(line) || is_rule(t) || ((1..=6).contains(&hashes) && t[hashes..].starts_with(' '))
}

/// Paragraph lines joined: a line ending in a backslash or two spaces breaks
/// hard (a newline in the run), any other line break is a space (#VAPP-258
/// step 5: a newline too under `hard`).
fn join_paragraph(para: &[&str], hard: bool) -> String {
    let mut text = String::new();
    for (k, line) in para.iter().enumerate() {
        if k > 0 {
            if text.ends_with('\\') {
                text.pop();
                text.push('\n');
            } else if text.ends_with("  ") {
                let kept = text.trim_end_matches(' ').len();
                text.truncate(kept);
                text.push('\n');
            } else {
                text.push(if hard { '\n' } else { ' ' });
            }
        }
        text.push_str(line);
    }
    let kept = text.trim_end_matches(' ').len();
    text.truncate(kept);
    text
}

fn is_rule(line: &str) -> bool {
    let t: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    t.len() >= 3 && (t.chars().all(|c| c == '-') || t.chars().all(|c| c == '*') || t.chars().all(|c| c == '_'))
}

/// Parse the block structure of a markdown document.
pub fn parse(text: &str) -> Vec<Block> {
    parse_hard(text, false)
}

/// #VAPP-258 step 5 (`catalog/host.json` markdown.options): `hard` = every
/// newline inside a paragraph, list item or quote paragraph is a line break;
/// `streaming` = the text is a growing prefix ([`parse_streaming`]).
pub fn parse_with(text: &str, hard: bool, streaming: bool) -> Vec<Block> {
    if streaming {
        parse_streaming(text, hard)
    } else {
        parse_hard(text, hard)
    }
}

/// A line that opens a fence (` ``` ` / `~~~`): the 3-char fence its close starts with.
fn fence_open(line: &str) -> Option<&'static str> {
    let t = line.trim();
    if t.starts_with("```") {
        Some("```")
    } else if t.starts_with("~~~") {
        Some("~~~")
    } else {
        None
    }
}

/// `text`'s lines from `from` (a line start outside a fence) on: the index
/// after its last COMPLETE blank line outside a fence (else `from`), whether
/// a fence is still open at the end, and whether the last line is that open
/// fence's opener.
fn scan_fences(text: &str, from: usize) -> (usize, bool, bool) {
    let mut fence: Option<&str> = None;
    let mut boundary = from;
    let mut at = from;
    let mut opener_last;
    loop {
        let nl = text[at..].find('\n').map(|n| at + n);
        let line = &text[at..nl.unwrap_or(text.len())];
        opener_last = false;
        match fence {
            Some(f) => {
                if line.trim().starts_with(f) {
                    fence = None;
                }
            }
            None => match (fence_open(line), nl) {
                (Some(f), _) => {
                    fence = Some(f);
                    opener_last = true;
                }
                (None, Some(n)) if line.trim().is_empty() => boundary = n + 1,
                _ => {}
            },
        }
        match nl {
            Some(n) => at = n + 1,
            None => break,
        }
    }
    (boundary, fence.is_some(), opener_last)
}

/// A streamed parse remembered: the stable blocks before `boundary`.
struct StreamMemo {
    text: String,
    hard: bool,
    boundary: usize,
    stable: Rc<Vec<Block>>,
}

const STREAM_MEMO_CAP: usize = 16;

thread_local! {
    static STREAM_MEMOS: RefCell<Vec<StreamMemo>> = const { RefCell::new(Vec::new()) };
}

/// Forget every streamed parse (tests: a cold parse of a prefix).
pub fn reset_stream_memo() {
    STREAM_MEMOS.with(|m| m.borrow_mut().clear());
}

/// The largest list group of `blocks` (groups of a later chunk start past it).
fn max_group(blocks: &[Block]) -> usize {
    blocks.iter().filter_map(|b| b.list_group).max().unwrap_or(0)
}

fn offset_groups(blocks: &mut [Block], by: usize) {
    for b in blocks {
        if let Some(g) = b.list_group.as_mut() {
            *g += by;
        }
    }
}

/// #VAPP-258 step 5 (`fixtures/markdown-render.json` streamingCases): a
/// growing prefix = the stable blocks before its last complete blank line
/// outside a fence (memoized per text: only what extends the old text
/// reparses) + the tail, whose unfinished end is held back.
pub fn parse_streaming(raw: &str, hard: bool) -> Vec<Block> {
    let text = raw.replace("\r\n", "\n").replace('\r', "\n");
    let (from, prior) = STREAM_MEMOS.with(|memos| {
        let memos = memos.borrow();
        memos
            .iter()
            .filter(|m| m.hard == hard && text.starts_with(&m.text))
            .max_by_key(|m| m.text.len())
            .map_or((0, Rc::new(Vec::new())), |m| (m.boundary, m.stable.clone()))
    });
    let (boundary, _, _) = scan_fences(&text, from);
    let stable = if boundary > from {
        let mut more = parse_hard(&text[from..boundary], hard);
        offset_groups(&mut more, max_group(&prior));
        let mut all = (*prior).clone();
        all.extend(more);
        Rc::new(all)
    } else {
        prior
    };
    STREAM_MEMOS.with(|memos| {
        let mut memos = memos.borrow_mut();
        memos.retain(|m| !(m.hard == hard && text.starts_with(&m.text)));
        memos.insert(0, StreamMemo { text: text.clone(), hard, boundary, stable: stable.clone() });
        memos.truncate(STREAM_MEMO_CAP);
    });
    let mut tail = text[boundary..].to_string();
    let mut out = (*stable).clone();
    if tail.is_empty() {
        return out;
    }
    let (_, open, opener_last) = scan_fences(&tail, 0);
    if open {
        // The open fence's last line waits while it is empty or only fence
        // characters (a closing fence on its way).
        if !opener_last {
            if let Some(nl) = tail.rfind('\n') {
                if tail[nl + 1..].trim().chars().all(|c| c == '`' || c == '~') {
                    tail.truncate(nl);
                }
            }
        }
    } else {
        while let Some(held) = hold_trailing(&tail) {
            tail.truncate(held);
        }
    }
    let mut blocks = parse_hard(&tail, hard);
    offset_groups(&mut blocks, max_group(&out));
    if let Some(last) = blocks.last_mut() {
        if matches!(last.kind, BlockKind::Paragraph | BlockKind::Heading(_) | BlockKind::Quote | BlockKind::ListItem { .. }) {
            hold_back(&mut last.inlines);
        }
        if last.kind == BlockKind::Paragraph && last.inlines.iter().all(|i| i.text.is_empty()) {
            blocks.pop();
        }
    }
    out.extend(blocks);
    out
}

/// Where the tail's unfinished end starts, outside a fence (`None` = it
/// ends finished): a last line that is only a list marker or heading hashes
/// so far, a trailing run of `*` / `_`, or of backticks / tildes unless they
/// are the line's only text (a fence opening or closing).
fn hold_trailing(tail: &str) -> Option<usize> {
    if tail.is_empty() {
        return None;
    }
    let line_start = tail.rfind('\n').map_or(0, |n| n + 1);
    let line = tail[line_start..].trim_start_matches([' ', '\t']);
    let hashes = line.trim_end_matches([' ', '\t']);
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    let marker = matches!(line, "-" | "+" | "*")
        || ((1..=6).contains(&hashes.len()) && hashes.bytes().all(|b| b == b'#'))
        || ((1..=9).contains(&digits) && (line.len() == digits || (line.len() == digits + 1 && matches!(line.as_bytes()[digits], b'.' | b')'))));
    if marker {
        return Some(line_start.saturating_sub(1));
    }
    let run = |set: &[u8]| tail.len() - tail.trim_end_matches(|c: char| c.is_ascii() && set.contains(&(c as u8))).len();
    let stars = run(b"*_");
    if stars > 0 {
        return Some(tail.len() - stars);
    }
    let ticks = run(b"`~");
    if ticks > 0 {
        let start = tail.len() - ticks;
        let before = &tail[line_start.min(start)..start];
        if !before.chars().all(|c| c == ' ' || c == '\t') {
            return Some(start);
        }
    }
    None
}

/// The streaming tail's last block: its plain runs (no flags, no link, no
/// chip) hold back what would flicker: an unfinished link at the end paints
/// as its label, and every opening delimiter run is dropped (a maximal run of
/// `*` / `_` / backticks, or of 2+ `~`, not after an ASCII letter, digit or
/// delimiter, followed by a non-space or ending the run).
fn hold_back(inlines: &mut Vec<Inline>) {
    let plain = |i: &Inline| !i.bold && !i.italic && !i.code && !i.strike && i.link.is_none() && i.chip.is_none();
    if let Some(last) = inlines.last_mut().filter(|i| plain(i)) {
        if let Some(open) = last.text.rfind('[') {
            let rest = &last.text[open + 1..];
            let label_end = rest.find(']').unwrap_or(rest.len());
            let after = &rest[label_end..];
            let unfinished = after.is_empty() || after == "]" || after.strip_prefix("](").is_some_and(|d| d.chars().all(|c| c != '(' && c != ')' && !c.is_whitespace()));
            if unfinished {
                let start = if open > 0 && last.text.as_bytes()[open - 1] == b'!' { open - 1 } else { open };
                let label = rest[..label_end].to_string();
                last.text.truncate(start);
                last.text.push_str(&label);
            }
        }
    }
    for span in inlines.iter_mut().filter(|i| plain(i)) {
        span.text = drop_delimiters(&span.text);
    }
    inlines.retain(|i| !i.text.is_empty() || i.chip.is_some());
}

fn drop_delimiters(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    let delim = |c: u8| matches!(c, b'*' | b'_' | b'`');
    while i < b.len() {
        let c = b[i];
        let tilde = c == b'~';
        if delim(c) || tilde {
            let mut j = i;
            while j < b.len() && (if tilde { b[j] == b'~' } else { delim(b[j]) }) {
                j += 1;
            }
            let before_ok = i == 0 || !(b[i - 1].is_ascii_alphanumeric() || delim(b[i - 1]) || b[i - 1] == b'~');
            let after_ok = j == b.len() || !s[j..].starts_with(char::is_whitespace);
            if !(before_ok && after_ok && (!tilde || j - i >= 2)) {
                out.push_str(&s[i..j]);
            }
            i = j;
            continue;
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn parse_hard(text: &str, hard: bool) -> Vec<Block> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();
    let mut blocks = Vec::new();
    let mut para: Vec<&str> = Vec::new();
    let mut group = 0usize;
    let flush = |para: &mut Vec<&str>, blocks: &mut Vec<Block>| {
        if !para.is_empty() {
            let joined = join_paragraph(para, hard);
            match whole_image(joined.trim()) {
                Some((alt, src)) => blocks.push(Block { kind: BlockKind::Image { src, alt: alt.clone(), size: None }, inlines: parse_inline(&alt), list_group: None, lang: None }),
                // #FEED-208: images directly in its text split it.
                None => blocks.extend(split_images(&joined)),
            }
            para.clear();
        }
    };
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            flush(&mut para, &mut blocks);
            let fence = &trimmed[..3];
            let lang = trimmed.trim_start_matches(['`', '~']).split_whitespace().next().map(str::to_string);
            let mut body = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim().starts_with(fence) {
                body.push(lines[i]);
                i += 1;
            }
            i += 1;
            blocks.push(Block { kind: BlockKind::CodeBlock, inlines: vec![Inline { text: body.join("\n"), code: true, ..Inline::default() }], list_group: None, lang });
            continue;
        }
        let hashes = trimmed.bytes().take_while(|b| *b == b'#').count();
        if (1..=6).contains(&hashes) && trimmed[hashes..].starts_with(' ') {
            flush(&mut para, &mut blocks);
            blocks.push(Block { kind: BlockKind::Heading(hashes as u8), inlines: parse_inline(trimmed[hashes..].trim()), list_group: None, lang: None });
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
            // Paragraphs join with a space; under `hard` a paragraph's lines
            // with a newline.
            let mut joined = String::new();
            let mut gap = false;
            for l in body {
                if l.is_empty() {
                    gap = true;
                    continue;
                }
                if !joined.is_empty() {
                    joined.push(if hard && !gap { '\n' } else { ' ' });
                }
                joined.push_str(l);
                gap = false;
            }
            blocks.push(Block { kind: BlockKind::Quote, inlines: parse_inline(&joined), list_group: None, lang: None });
            continue;
        }
        if is_rule(trimmed) && para.is_empty() {
            blocks.push(Block { kind: BlockKind::Rule, inlines: Vec::new(), list_group: None, lang: None });
            i += 1;
            continue;
        }
        if list_marker(line).is_some() {
            flush(&mut para, &mut blocks);
            group += 1;
            // The indents of the open lists, outermost first: an item 2+
            // columns past its list's nests under the previous item.
            let mut lists: Vec<usize> = Vec::new();
            let mut last_body = String::new();
            while i < lines.len() {
                let Some((indent, marker, rest)) = list_marker(lines[i]) else {
                    // A non-blank line under an item that starts no block continues it.
                    let next = lines[i].trim();
                    if next.is_empty() || starts_block(lines[i]) {
                        break;
                    }
                    last_body.push(if hard { '\n' } else { ' ' });
                    last_body.push_str(next);
                    if let Some(last) = blocks.last_mut() {
                        last.inlines = parse_inline(&last_body);
                    }
                    i += 1;
                    continue;
                };
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
                last_body = body.trim().to_string();
                blocks.push(Block { kind: BlockKind::ListItem { marker, task, depth }, inlines: parse_inline(&last_body), list_group: Some(group), lang: None });
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
            blocks.push(Block { kind: BlockKind::Table { header, rows }, inlines: Vec::new(), list_group: None, lang: None });
            continue;
        }
        if trimmed.is_empty() {
            flush(&mut para, &mut blocks);
        } else {
            para.push(line.trim_start());
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

/// Round 9 (`fixtures/markdown-inline.json` `documentPaint`): inline code,
/// the `Markdown/code` recipe: its type (`size`; `None` = 0.925 em of the
/// run), a `radius`-cornered fill, `pad_h` (`None` = 0.3 em) reserved in the
/// line on each side and `pad_v` over the font's own ascent and descent, as
/// the web's inline box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InlineCode {
    pub size: Option<f32>,
    pub pad_h: Option<f32>,
    pub pad_v: f32,
    pub radius: f32,
}

impl Default for InlineCode {
    fn default() -> Self {
        InlineCode { size: None, pad_h: None, pad_v: 0.0, radius: 4.0 }
    }
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
    /// A block image's box height cap (loaded or not, so measure = paint);
    /// `None` = no cap (`markdown.options.imageMaxHeight: null`).
    pub image_height: Option<f32>,
    /// #VAPP-258: the viewport (window) height an uncapped image stops at.
    pub viewport_height: Option<f32>,
    /// #VAPP-258: a block image's corner radius (the `Markdown/image`
    /// recipe's `borderRadius`; 0 without a theme).
    pub image_radius: f32,
    /// #VAPP-258: a block image's border (the `Markdown/image` recipe's
    /// `borderWidth` + `borderColor`), drawn inside its corners: the box
    /// never changes. 0 / None = none.
    pub image_border_width: f32,
    pub image_border_color: Option<gpui::Hsla>,
    pub cell_pad_h: f32,
    pub cell_pad_v: f32,
    /// #VAPP-258: a heading's type and margins per `level` (index = level
    /// - 1; `None` = `heading`, `heading_top`, `heading_bottom`).
    pub heading_levels: Vec<Option<TextSpec>>,
    pub heading_tops: Vec<Option<f32>>,
    pub heading_bottoms: Vec<Option<f32>>,
    /// #VAPP-258: a list item's `Markdown/listItem` margins (collapsing
    /// with each other and, at a list's end, with the block gap).
    pub item_top: f32,
    pub item_bottom: f32,
    /// Round 9: inline code (`Markdown/code`).
    pub inline_code: InlineCode,
    /// #VAPP-258: a fenced block's border (`Markdown/codeBlock`
    /// borderWidth), inside its box: the box grows by it on each side.
    pub code_border: f32,
    /// #VAPP-258: the inline chip's metrics.
    pub chip: ChipMetrics,
    /// #VAPP-258: the attachment blocks' metrics.
    pub attach: AttachMetrics,
}

/// #VAPP-258 (`catalog/host.json` markdown): the inline chip's box: height
/// min(`$control.badge`, the line height), `$spacing.sm` side padding, the
/// icon at `$control.iconSm` then `$spacing.xs`, the label in the Text
/// `caption` style.
#[derive(Debug, Clone, PartialEq)]
pub struct ChipMetrics {
    pub height: f32,
    pub pad: f32,
    pub gap: f32,
    pub icon: f32,
    pub caption: TextSpec,
    /// #VAPP-258: the ident's type (`Markdown/chipIdent`; `None` = the mono
    /// family at the label's size).
    pub ident: Option<TextSpec>,
    /// #VAPP-258: the box's corner radius (`None` = a capsule).
    pub radius: Option<f32>,
    /// #VAPP-258: a theme's `Markdown/chip*` metrics per chip kind.
    pub kinds: HashMap<String, ChipKind>,
}

/// #VAPP-258: one chip kind's `Markdown/chip*` metrics (`None` = the base).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChipKind {
    pub caption: Option<TextSpec>,
    pub ident: Option<TextSpec>,
    pub pad: Option<f32>,
    pub radius: Option<f32>,
}

impl ChipMetrics {
    /// The chip box width for a label `label_w` wide (and an ident
    /// `ident_w` wide).
    pub fn width(&self, label_w: f32, icon: bool, ident_w: Option<f32>) -> f32 {
        2.0 * self.pad + label_w + if icon { self.icon + self.gap } else { 0.0 } + ident_w.map_or(0.0, |w| w + self.gap)
    }

    /// The metrics a chip of `kind` lays out with (its kind's overrides on top).
    pub fn resolved(&self, kind: &str) -> ChipMetrics {
        let Some(k) = self.kinds.get(kind) else { return self.clone() };
        ChipMetrics {
            caption: k.caption.clone().unwrap_or_else(|| self.caption.clone()),
            ident: k.ident.clone().or_else(|| self.ident.clone()),
            pad: k.pad.unwrap_or(self.pad),
            radius: k.radius.or(self.radius),
            kinds: HashMap::new(),
            ..self.clone()
        }
    }

    /// The ident's type: its own, else the mono family at the label's size.
    pub fn ident_spec(&self, mono: Option<String>) -> TextSpec {
        self.ident.clone().unwrap_or_else(|| TextSpec { family: mono, ..self.caption.clone() })
    }

    /// The chip box height in a line `line_height` tall.
    pub fn box_height(&self, line_height: f32) -> f32 {
        self.height.min(line_height)
    }
}

/// #VAPP-258: what the Video / AudioPlayer / FileUpload `file` looks take.
#[derive(Debug, Clone, PartialEq)]
pub struct AttachMetrics {
    /// Layout `mediaIntrinsicWidth` / `mediaAspectRatio` (the Video box).
    pub media_width: f32,
    pub media_ratio: f32,
    /// AudioPlayer: the title line, `$spacing.xs`, the controls row.
    pub audio_gap: f32,
    pub audio_row: f32,
    /// FileUpload `file`: its min height, side padding, gap, the icon, the
    /// name's text.
    pub file_row: f32,
    pub file_pad: f32,
    pub file_gap: f32,
    pub file_icon: f32,
    pub file_name: TextSpec,
}

impl InlineCode {
    /// Its side padding on a run of `base`.
    pub fn pad_h(&self, base: &TextSpec) -> f32 {
        self.pad_h.unwrap_or(base.size * 0.3)
    }

    /// Its type on a run of `base`.
    pub fn spec(&self, base: &TextSpec, family: Option<String>) -> TextSpec {
        TextSpec { size: self.size.unwrap_or(base.size * 0.925), family, ..base.clone() }
    }
}

impl MdStyles {
    /// #VAPP-258 (`markdown.options.imageMaxHeight`): these styles with the
    /// host's image cap and the window height an uncapped image stops at
    /// (the measurer and the painter both lay out with them).
    pub fn imaged(mut self, options: &crate::host::MarkdownOptions, viewport: f32) -> Self {
        self.image_height = options.image_cap();
        self.viewport_height = (viewport > 0.0).then_some(viewport);
        self
    }

    /// Plain metrics for a body text style (geometry mode, tests).
    pub fn plain(body: TextSpec) -> MdStyles {
        let chip = ChipMetrics { height: 20.0, pad: 8.0, gap: 4.0, icon: 16.0, caption: TextSpec { size: 12.0, line_height: 16.0, weight: 400, family: body.family.clone() }, ident: None, radius: None, kinds: HashMap::new() };
        let attach = AttachMetrics {
            media_width: eui_core::layout::MEDIA_INTRINSIC_WIDTH as f32,
            media_ratio: eui_core::layout::MEDIA_ASPECT_RATIO as f32,
            audio_gap: 4.0,
            audio_row: 40.0,
            file_row: 40.0,
            file_pad: 8.0,
            file_gap: 8.0,
            file_icon: 16.0,
            file_name: body.clone(),
        };
        MdStyles {
            chip,
            attach,
            heading: TextSpec { size: body.size + 4.0, line_height: body.line_height + 8.0, weight: 600, family: body.family.clone() },
            code: TextSpec { size: (body.size - 2.0).max(10.0), line_height: body.line_height, weight: 400, family: None },
            list_indent: (body.size * 1.4).round(),
            image_height: Some(160.0),
            viewport_height: None,
            image_radius: 0.0,
            image_border_width: 0.0,
            image_border_color: None,
            body,
            code_pad: 12.0,
            quote_border: 2.0,
            quote_pad: 12.0,
            block_gap: 8.0,
            heading_top: 12.0,
            heading_bottom: 4.0,
            cell_pad_h: 8.0,
            cell_pad_v: 4.0,
            heading_levels: vec![None; 6],
            heading_tops: vec![None; 6],
            heading_bottoms: vec![None; 6],
            item_top: 0.0,
            item_bottom: 0.0,
            inline_code: InlineCode::default(),
            code_border: 0.0,
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
        let part_with = |name: &str, key: &str, value: serde_json::Value| {
            let mut props = owner_props.clone();
            props.insert(key.to_string(), value);
            part_props(theme, mode, "Markdown", name, &props, &[])
        };
        let spec = |p: &Map<String, serde_json::Value>, base: &TextSpec| TextSpec {
            size: px_prop(p, "fontSize").unwrap_or(base.size),
            line_height: px_prop(p, "lineHeight").unwrap_or(base.line_height),
            weight: p.get("fontWeight").and_then(|v| v.as_u64()).map(|w| w as u16).unwrap_or(base.weight),
            family: p.get("fontFamily").and_then(|v| v.as_str()).map(str::to_string).or_else(|| base.family.clone()),
        };
        // #VAPP-258 step 5: the chat rhythm re-resolves the root (the node's
        // text style knows no density): its type is the body, its gap the
        // block gap. The document keeps the node's style and the `sm` gap.
        // Round 9 §9.1: the inline profile's paragraph IS a Text of `variant`
        // and its heading a Heading, whatever the host's density: the
        // recipes come from `markdown_inline_recipes`, the block gaps stay
        // the document's.
        let inline = inline_variant(owner_props);
        let typed = |recipes: Vec<RecipeQuery>| {
            let mut merged = Map::new();
            for q in recipes {
                merged.extend(theme.map(|t| eui_core::theme::resolve_recipe(t, &q, mode)).unwrap_or_default());
            }
            merged
        };
        let chat = inline.is_none() && owner_props.get("density").and_then(|v| v.as_str()) == Some("chat");
        let root = chat.then(|| part("root"));
        if let Some(root) = &root {
            s.body = spec(root, &s.body);
        }
        if inline.is_some() {
            s.body = spec(&typed(markdown_inline_recipes("paragraph", inline, None)), &s.body);
        }
        let mut heading_props = owner_props.clone();
        if inline.is_some() {
            heading_props.remove("density");
        }
        let heading = part_props(theme, mode, "Markdown", "heading", &heading_props, &[]);
        let heading_base = TextSpec { weight: 600, ..s.body.clone() };
        s.heading = spec(&heading, &heading_base);
        if inline.is_some() {
            s.heading = spec(&typed(markdown_inline_recipes("heading", None, Some(2))), &heading_base);
        }
        // #VAPP-258: each heading level's type and margins (the `level` the
        // painter supplies; margins collapse like CSS block margins).
        s.heading_top = px_prop(&heading, "marginTop").unwrap_or_else(|| spacing(theme, "md"));
        s.heading_bottom = px_prop(&heading, "marginBottom").unwrap_or_else(|| spacing(theme, "xs"));
        for level in 1..=6u8 {
            heading_props.insert("level".into(), serde_json::Value::from(level));
            let p = part_props(theme, mode, "Markdown", "heading", &heading_props, &[]);
            s.heading_levels[level as usize - 1] = Some(if inline.is_some() { spec(&typed(markdown_inline_recipes("heading", None, Some(level))), &heading_base) } else { spec(&p, &heading_base) });
            s.heading_tops[level as usize - 1] = px_prop(&p, "marginTop");
            s.heading_bottoms[level as usize - 1] = px_prop(&p, "marginBottom");
        }
        let item = part("listItem");
        s.item_top = px_prop(&item, "marginTop").unwrap_or(0.0);
        s.item_bottom = px_prop(&item, "marginBottom").unwrap_or(0.0);
        let inline_code = part("code");
        s.inline_code = InlineCode {
            size: px_prop(&inline_code, "fontSize"),
            pad_h: px_prop(&inline_code, "paddingHorizontal").or_else(|| px_prop(&inline_code, "padding")),
            pad_v: px_prop(&inline_code, "paddingVertical").or_else(|| px_prop(&inline_code, "padding")).unwrap_or(0.0),
            radius: px_prop(&inline_code, "borderRadius").unwrap_or(4.0),
        };
        let code = part("codeBlock");
        s.code = spec(&code, &TextSpec { family: mono_family(theme), ..s.code.clone() });
        s.code_pad = px_prop(&code, "padding").or_else(|| px_prop(&code, "paddingHorizontal")).unwrap_or(s.code_pad);
        s.code_border = px_prop(&code, "borderWidth").unwrap_or(0.0);
        let quote = part("quote");
        s.quote_border = px_prop(&quote, "borderWidth").unwrap_or(s.quote_border);
        s.quote_pad = px_prop(&quote, "paddingHorizontal").unwrap_or(s.quote_pad);
        s.image_radius = px_prop(&part("image"), "borderRadius").unwrap_or(0.0);
        s.image_border_width = px_prop(&part("image"), "borderWidth").unwrap_or(0.0);
        s.image_border_color = color_of(part_visual(theme, mode, "Markdown", "image", owner_props, &[]).border_color.as_deref());
        s.block_gap = root.as_ref().and_then(|r| px_prop(r, "gap")).unwrap_or_else(|| spacing(theme, "sm"));
        // #VAPP-258: every cell's paddings (`Markdown/cell`).
        let cell = part("cell");
        s.cell_pad_h = px_prop(&cell, "paddingHorizontal").or_else(|| px_prop(&cell, "padding")).unwrap_or_else(|| spacing(theme, "sm"));
        s.cell_pad_v = px_prop(&cell, "paddingVertical").or_else(|| px_prop(&cell, "padding")).unwrap_or_else(|| spacing(theme, "xs"));
        // #VAPP-258: the chip (the Chip macro's root + its caption label).
        let caption = part_props(theme, mode, "Text", "root", &Map::from_iter([("variant".to_string(), serde_json::Value::from("caption"))]), &[]);
        s.chip = ChipMetrics {
            height: super::parts::control(theme, "badge", s.chip.height),
            pad: spacing(theme, "sm"),
            gap: spacing(theme, "xs"),
            icon: super::parts::control(theme, "iconSm", s.chip.icon),
            caption: TextSpec { weight: 400, ..spec(&caption, &s.chip.caption) },
            ident: None,
            radius: None,
            kinds: HashMap::new(),
        };
        // #VAPP-258: a theme's `Markdown/chip*` rules per chip kind (their
        // metrics; the painter resolves the colours).
        for kind in eui_core::host::contract::MARKDOWN_CHIP_KINDS {
            let (bx, label, ident) = (part_with("chip", "chip", (*kind).into()), part_with("chipLabel", "chip", (*kind).into()), part_with("chipIdent", "chip", (*kind).into()));
            let typed = |p: &Map<String, serde_json::Value>| ["fontSize", "lineHeight", "fontWeight", "fontFamily"].iter().any(|k| p.contains_key(*k));
            let look = ChipKind {
                caption: typed(&label).then(|| spec(&label, &s.chip.caption)),
                ident: typed(&ident).then(|| spec(&ident, &TextSpec { family: mono_family(theme), ..s.chip.caption.clone() })),
                pad: px_prop(&bx, "paddingHorizontal").or_else(|| px_prop(&bx, "padding")),
                radius: px_prop(&bx, "borderRadius"),
            };
            if look != ChipKind::default() {
                s.chip.kinds.insert((*kind).to_string(), look);
            }
        }
        let file = part_props(theme, mode, "FileUpload", "file", owner_props, &[]);
        let file_name = part_props(theme, mode, "FileUpload", "fileName", owner_props, &[]);
        s.attach.audio_gap = spacing(theme, "xs");
        s.attach.audio_row = px_prop(&part_props(theme, mode, "AudioPlayer", "controls", owner_props, &[]), "height").unwrap_or_else(|| super::parts::control(theme, "row", 40.0));
        s.attach.file_row = px_prop(&file, "minHeight").or_else(|| px_prop(&file, "height")).unwrap_or_else(|| super::parts::control(theme, "row", 40.0));
        s.attach.file_pad = px_prop(&file, "paddingHorizontal").or_else(|| px_prop(&file, "padding")).unwrap_or_else(|| spacing(theme, "sm"));
        s.attach.file_gap = px_prop(&file, "gap").unwrap_or_else(|| spacing(theme, "sm"));
        s.attach.file_icon = super::parts::control(theme, "iconSm", 16.0);
        s.attach.file_name = spec(&file_name, &s.body);
        s
    }

    /// A table header cell's type: the body, semibold (measured and painted).
    fn header_cell(&self) -> TextSpec {
        TextSpec { weight: self.body.weight.max(600), ..self.body.clone() }
    }

    fn spec_of(&self, kind: &BlockKind) -> &TextSpec {
        match kind {
            BlockKind::Heading(level) => self.heading_levels.get(level_slot(*level)).and_then(Option::as_ref).unwrap_or(&self.heading),
            BlockKind::CodeBlock => &self.code,
            _ => &self.body,
        }
    }
}

/// #VAPP-258 step 5: the node props a Markdown's recipes resolve with:
/// `density: chat` added when the host's options ask for the chat rhythm
/// (a painter-supplied recipe prop, `catalog/recipes.json`).
pub fn density_props(props: &Map<String, serde_json::Value>, chat: bool) -> std::borrow::Cow<'_, Map<String, serde_json::Value>> {
    if !chat {
        return std::borrow::Cow::Borrowed(props);
    }
    let mut p = props.clone();
    p.insert("density".into(), serde_json::Value::from("chat"));
    std::borrow::Cow::Owned(p)
}

/// Round 9 §9.1: an inline-profile Markdown's paragraph `variant`
/// (`None` = the document profile).
pub fn inline_variant(props: &Map<String, serde_json::Value>) -> Option<&'static str> {
    (props.get("profile").and_then(|v| v.as_str()) == Some("inline")).then(|| if props.get("variant").and_then(|v| v.as_str()) == Some("caption") { "caption" } else { "body" })
}

/// The text colour of an inline paragraph (its `Text/root`).
pub fn inline_ink(theme: Option<&ResolvedTheme>, mode: Mode, variant: &str) -> Option<Hsla> {
    let query = markdown_inline_recipes("paragraph", Some(variant), None).pop()?;
    color_of(theme.map(|t| eui_core::style::visual(&eui_core::theme::resolve_recipe(t, &query, mode), eui_core::style::BoxKind::Leaf)).unwrap_or_default().color.as_deref())
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
    /// #VAPP-258: one line of `text` shaped in `spec` (bold / italic as a
    /// run of these flags paints it): a chip's label, its placeholder.
    fn run_width(&mut self, text: &str, spec: &TextSpec, bold: bool, italic: bool) -> f32;
    /// Round 9: [`Self::run_width`] in the code font.
    fn code_width(&mut self, text: &str, spec: &TextSpec, bold: bool, italic: bool) -> f32 {
        self.run_width(text, spec, bold, italic)
    }
    /// #VAPP-258: where each byte offset `at` of `inlines` wrapped at `width`
    /// lands: `(x in its line, that line's top, that line's width)` — the
    /// painter places the inline chips there.
    fn spots(&mut self, inlines: &[Inline], spec: &TextSpec, width: f32, at: &[usize]) -> Vec<(f32, f32, f32)>;
}

/// The non-breaking spaces a chip's placeholder is made of (both glue in
/// UAX #14 and in gpui's line wrapper): the wider first.
const CHIP_FILL: [char; 2] = ['\u{a0}', '\u{202f}'];

/// The placeholder that reserves `target` px (never more than `cap`): the
/// fewest non-breaking spaces at least `target` wide, from the two widths.
fn chip_fill(target: f32, cap: f32, wide: f32, narrow: f32) -> (String, f32) {
    let wide = wide.max(0.01);
    let mut best: Option<(f32, usize, usize)> = None;
    let narrows = if narrow > 0.01 && (narrow - wide).abs() > 0.01 { 8 } else { 0 };
    for b in 0..=narrows {
        let rest = (target - b as f32 * narrow).max(0.0);
        let a = (rest / wide - 1e-3).ceil().max(0.0) as usize;
        let total = a as f32 * wide + b as f32 * narrow;
        if a + b > 0 && total <= cap + 0.01 && best.is_none_or(|(t, _, _)| total < t) {
            best = Some((total, a, b));
        }
    }
    let (a, b) = match best {
        Some((_, a, b)) => (a, b),
        // Even the target does not fit: as many as the cap takes.
        None => (((cap / wide).floor() as usize).max(1), 0),
    };
    let mut out = String::new();
    out.extend(std::iter::repeat_n(CHIP_FILL[0], a));
    out.extend(std::iter::repeat_n(CHIP_FILL[1], b));
    (out, a as f32 * wide + b as f32 * narrow)
}

/// The text width each block's runs wrap at `width` wide (`None` = no
/// wrap), as [`layout`] lays them out; a table's cells get their own.
fn wrap_of(kind: &BlockKind, s: &MdStyles, width: Option<f32>) -> Option<f32> {
    let w = width?;
    Some(match kind {
        BlockKind::ListItem { depth, .. } => (w - s.list_indent * (*depth as f32 + 1.0)).max(1.0),
        BlockKind::Quote => (w - s.quote_border - s.quote_pad).max(1.0),
        BlockKind::Table { header, rows } => {
            let cols = header.len().max(rows.iter().map(Vec::len).max().unwrap_or(0)).max(1);
            cell_text_width(w, cols, s)
        }
        _ => w.max(1.0),
    })
}

/// A table cell's text width `width` wide with `cols` columns: whole pixels
/// (#EXP-1728), so the box gpui rounds the cell's text into is exactly the
/// width it was measured at (a fractional one could round down and wrap a
/// line the layout counted as one).
fn cell_text_width(width: f32, cols: usize, s: &MdStyles) -> f32 {
    (width / cols.max(1) as f32 - 2.0 * s.cell_pad_h).floor().max(1.0)
}

/// A chip's `kind` for inline code: gpui shapes a text in ONE size, so a code
/// span is a placeholder in the line under its own box, like a chip. One wider
/// than its line stays a breaking mono run in the block's size.
pub const CODE_CHIP: &str = "code";

/// #VAPP-258: every inline chip sized (the label shaped in the caption
/// style, capped at its block's wrap width `width` wide) and its run's text
/// swapped for the placeholder that reserves that width, so the measurer
/// and the painter wrap the SAME runs. Round 9: inline code too. `None` = no
/// chip.
pub fn size_chips(blocks: &[Block], s: &MdStyles, width: Option<f32>, text: &mut dyn MdText) -> Option<Vec<Block>> {
    let has_chip = |list: &[Inline]| list.iter().any(|i| i.chip.is_some() || i.code);
    let any = blocks.iter().any(|b| (b.kind != BlockKind::CodeBlock && has_chip(&b.inlines)) || matches!(&b.kind, BlockKind::Table { header, rows } if header.iter().chain(rows.iter().flatten()).any(|c| has_chip(c))));
    if !any {
        return None;
    }
    let mut size = |list: &mut Vec<Inline>, spec: &TextSpec, cap: f32| {
        for span in list.iter_mut() {
            let (bold, italic) = (span.bold, span.italic);
            if span.code {
                let (type_, pad) = (s.inline_code.spec(spec, s.code.family.clone()), s.inline_code.pad_h(spec));
                let width = text.code_width(&span.text, &type_, bold, italic) + 2.0 * pad;
                if width <= cap {
                    span.chip = Some(InlineChip { kind: CODE_CHIP.to_string(), label: span.text.clone(), source: span.text.clone(), width, ..InlineChip::default() });
                    span.code = false;
                }
            }
            let Some(chip) = span.chip.as_mut() else { continue };
            if chip.kind == CODE_CHIP {
                let wide = text.run_width(&CHIP_FILL[0].to_string(), spec, bold, italic);
                let narrow = text.run_width(&CHIP_FILL[1].to_string(), spec, bold, italic);
                let (fill, total) = chip_fill(chip.width, cap, wide, narrow);
                span.text = fill;
                chip.width = total.min(cap);
                continue;
            }
            let m = s.chip.resolved(&chip.kind);
            let label = text.run_width(&chip.label, &m.caption, false, false);
            let ident = chip.ident.as_ref().map(|i| text.run_width(i, &m.ident_spec(s.code.family.clone()), false, false));
            let want = m.width(label, chip.icon.is_some(), ident);
            chip.capped = want > cap;
            chip.width = want.min(cap);
            let wide = text.run_width(&CHIP_FILL[0].to_string(), spec, bold, italic);
            let narrow = text.run_width(&CHIP_FILL[1].to_string(), spec, bold, italic);
            let (fill, total) = chip_fill(chip.width, cap, wide, narrow);
            span.text = fill;
            // #VAPP-258 step 5: the box spans its whole placeholder (the
            // fill rounds up to whole spaces): no stray gap after the chip.
            if total > chip.width && total <= cap + 0.01 {
                chip.width = total.min(cap);
            }
        }
    };
    Some(
        blocks
            .iter()
            .map(|b| {
                let mut b = b.clone();
                let cap = wrap_of(&b.kind, s, width).unwrap_or(f32::INFINITY);
                let spec = s.spec_of(&b.kind).clone();
                if let BlockKind::Table { header, rows } = &mut b.kind {
                    // The cells wrap in their row's spec (as `layout` measures them).
                    let head = s.header_cell();
                    for cell in header.iter_mut() {
                        size(cell, &head, cap);
                    }
                    for cell in rows.iter_mut().flatten() {
                        size(cell, &s.body, cap);
                    }
                } else if b.kind != BlockKind::CodeBlock {
                    size(&mut b.inlines, &spec, cap);
                }
                b
            })
            .collect(),
    )
}

/// A heading `level`'s slot in the per-level tables (levels clamp to 1...6).
fn level_slot(level: u8) -> usize {
    level.clamp(1, 6) as usize - 1
}

fn margins(kind: &BlockKind, s: &MdStyles) -> (f32, f32) {
    match kind {
        BlockKind::Heading(level) => {
            let slot = level_slot(*level);
            (s.heading_tops.get(slot).copied().flatten().unwrap_or(s.heading_top), s.heading_bottoms.get(slot).copied().flatten().unwrap_or(s.heading_bottom))
        }
        BlockKind::ListItem { .. } => (s.item_top, s.item_bottom.max(s.block_gap)),
        _ => (0.0, s.block_gap),
    }
}

/// The gap above block `i` (CSS margins collapse; first/last margins drop;
/// inside a list, the items' own margins).
fn gap_before(blocks: &[Block], i: usize, s: &MdStyles) -> f32 {
    if i == 0 {
        return 0.0;
    }
    let (prev, cur) = (&blocks[i - 1], &blocks[i]);
    if prev.list_group.is_some() && prev.list_group == cur.list_group {
        return s.item_bottom.max(s.item_top);
    }
    margins(&prev.kind, s).1.max(margins(&cur.kind, s).0)
}

/// Lay the blocks out `width` wide.
pub fn layout(blocks: &[Block], s: &MdStyles, width: f32, text: &mut dyn MdText) -> MdLayout {
    let _span = tracing::trace_span!("eui.markdown.layout", blocks = blocks.len()).entered();
    let mut y = 0.0;
    let mut out = Vec::with_capacity(blocks.len());
    for (i, b) in blocks.iter().enumerate() {
        y += gap_before(blocks, i, s);
        let spec = s.spec_of(&b.kind);
        let (height, rows) = match &b.kind {
            BlockKind::Paragraph | BlockKind::Heading(_) => (text.height(&b.inlines, spec, Some(width.max(1.0))), Vec::new()),
            BlockKind::ListItem { depth, .. } => (text.height(&b.inlines, spec, Some((width - s.list_indent * (*depth as f32 + 1.0)).max(1.0))), Vec::new()),
            BlockKind::Image { size, .. } => (image_box(*size, width.max(1.0), s.image_height, s.viewport_height).1, Vec::new()),
            BlockKind::Attachment { kind, .. } => (attachment_height(*kind, s, width), Vec::new()),
            BlockKind::Quote => (text.height(&b.inlines, spec, Some((width - s.quote_border - s.quote_pad).max(1.0))), Vec::new()),
            BlockKind::CodeBlock => (text.height(&b.inlines, spec, None) + 2.0 * (s.code_pad + s.code_border), Vec::new()),
            BlockKind::Rule => (1.0, Vec::new()),
            BlockKind::Table { header, rows } => {
                let cols = header.len().max(rows.iter().map(Vec::len).max().unwrap_or(0)).max(1);
                let cell_w = cell_text_width(width, cols, s);
                let mut heights = Vec::new();
                // #EXP-1728: the header row wraps in the weight it paints in.
                let head = s.header_cell();
                for (r, row) in std::iter::once(header).chain(rows.iter()).enumerate() {
                    let spec = if r == 0 { &head } else { &s.body };
                    let h = row.iter().map(|c| text.height(c, spec, Some(cell_w))).fold(s.body.line_height, f32::max);
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

/// #VAPP-258: an attachment block's height `width` wide: the Video box
/// (min(width, mediaIntrinsicWidth) / mediaAspectRatio), the AudioPlayer
/// (title line + gap + controls row), the FileUpload file row.
pub fn attachment_height(kind: AttachmentKind, s: &MdStyles, width: f32) -> f32 {
    let a = &s.attach;
    match kind {
        AttachmentKind::Video => width.max(1.0).min(a.media_width) / a.media_ratio,
        AttachmentKind::Audio => s.body.line_height + a.audio_gap + a.audio_row,
        AttachmentKind::File => a.file_row.max(a.file_name.line_height),
    }
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
                BlockKind::Image { size: Some(size), .. } => image_box(Some(*size), size.0, Some(s.image_height.unwrap_or(size.1)), s.viewport_height).0,
                BlockKind::Image { .. } => (s.image_height.unwrap_or(160.0) * 1.5).round(),
                BlockKind::Attachment { kind: AttachmentKind::File, name, .. } => {
                    let a = &s.attach;
                    2.0 * a.file_pad + a.file_icon + a.file_gap + text.width(&[Inline { text: name.clone(), ..Inline::default() }], &a.file_name)
                }
                BlockKind::Attachment { .. } => s.attach.media_width,
                BlockKind::Quote => text.width(&b.inlines, spec) + s.quote_border + s.quote_pad,
                BlockKind::CodeBlock => b.inlines[0].text.lines().map(|l| text.width(&[Inline { text: l.to_string(), ..b.inlines[0].clone() }], spec)).fold(0.0, f32::max) + 2.0 * (s.code_pad + s.code_border),
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
                BlockKind::Image { .. } | BlockKind::Attachment { .. } => 1.0,
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
        // #VAPP-258: a chip's placeholder paints nothing (the chip box
        // sits over it).
        if span.chip.is_some() {
            out.push(TextRun { len: span.text.len(), font, color: gpui::transparent_black(), background_color: None, underline: None, strikethrough: None });
            continue;
        }
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

/// #VAPP-258: a fenced block's runs in ONE font: CodeBlock's tokens (the
/// catalog's ONE tokenizer, `eui_core::code::tokenize_code`) each in
/// `token_color(kind)` when the fence names a highlighted language
/// ([`eui_core::code::markdown_code_language`]), else one run in the ink.
/// Colour only, so the shaped text (the measured height) never changes.
pub fn code_block_runs(text: &str, lang: Option<&str>, font: &Font, ink: Hsla, token_color: &dyn Fn(&str) -> Option<Hsla>) -> Vec<TextRun> {
    let run = |len: usize, color: Hsla| TextRun { len, font: font.clone(), color, background_color: None, underline: None, strikethrough: None };
    if text.is_empty() {
        return Vec::new();
    }
    let Some(language) = eui_core::code::markdown_code_language(lang) else { return vec![run(text.len(), ink)] };
    let mut out: Vec<TextRun> = Vec::new();
    let mut push = |len: usize, color: Hsla| match out.last_mut() {
        Some(last) if last.color == color => last.len += len,
        _ => out.push(run(len, color)),
    };
    let mut cache: HashMap<String, Hsla> = HashMap::new();
    for (i, line) in eui_core::code::tokenize_code(text, language).iter().enumerate() {
        if i > 0 {
            push(1, ink);
        }
        for t in line {
            let color = if t.kind == "plain" { ink } else { *cache.entry(t.kind.clone()).or_insert_with(|| token_color(&t.kind).unwrap_or(ink)) };
            push(t.text.len(), color);
        }
    }
    // The tokens concatenate to the text; anything else paints it plain.
    if out.iter().map(|r| r.len).sum::<usize>() == text.len() { out } else { vec![run(text.len(), ink)] }
}

// ---------------------------------------------------------------------------
// Painting
// ---------------------------------------------------------------------------

/// The painted text units of every Markdown leaf (by node index), in
/// document order: each unit's gpui text layout (hit-testing once painted)
/// and its text. Selection and copy read it.
pub type MdUnits = Rc<RefCell<HashMap<u32, Vec<MdUnit>>>>;

/// One selectable text run of a Markdown leaf: its layout (hit-tested once
/// painted), its text, the inline chips' byte ranges with their labels (a
/// copy gives the label, not the placeholder), and whether it was laid out
/// THIS frame (#VAPP-258 step 5: a long document lays out only the blocks
/// that show; an off-screen unit is never hit-tested).
#[derive(Clone)]
pub struct MdUnit {
    pub layout: TextLayout,
    pub text: SharedString,
    pub chips: Vec<(Range<usize>, String)>,
    pub live: Rc<std::cell::Cell<bool>>,
}

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

    /// The selected text (units joined by line feeds; #VAPP-258 step 5: a
    /// chip the selection touches copies as its label).
    pub fn text(&self, units: &[MdUnit]) -> String {
        let mut out: Vec<String> = Vec::new();
        for (u, unit) in units.iter().enumerate() {
            let text = &unit.text;
            if let Some(r) = self.range_in(u, text.len()) {
                let (a, b) = (floor_char(text, r.start), floor_char(text, r.end));
                out.push(copy_text(text, a..b, &unit.chips));
            }
        }
        out.join("\n")
    }
}

/// `text[range]` with every chip it touches as its label (once).
pub fn copy_text(text: &str, range: Range<usize>, chips: &[(Range<usize>, String)]) -> String {
    let mut out = String::new();
    let mut at = range.start;
    for (chip, label) in chips {
        if chip.end <= range.start || chip.start >= range.end {
            continue;
        }
        if chip.start > at {
            out.push_str(&text[at..chip.start]);
        }
        out.push_str(label);
        at = at.max(chip.end);
    }
    if at < range.end {
        out.push_str(&text[at..range.end]);
    }
    out
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
    /// A task item's box by its checked state (the theme's Checkbox `box`).
    pub task_box: TaskBox,
    /// #VAPP-258: a code token kind → CodeBlock's colour for it
    /// (`CodeBlock/token {kind}`; `None` = the ink).
    pub code_token: CodeTokenColor,
    /// YO-30: spans are cut per cluster against it (`None` = as built).
    pub text_system: Option<std::sync::Arc<gpui::TextSystem>>,
    /// #VAPP-258: the inline chip's look (the Chip root recipe: background,
    /// border; the radius is full) and its label colour (Chip/label, else
    /// the run's ink).
    pub chip_box: super::PaintStyle,
    pub chip_label: Option<Hsla>,
    /// #VAPP-258: each chip kind's `Markdown/chip*` look over the Chip's
    /// (the box style, the label and ident colours).
    pub chip_looks: HashMap<String, ChipLook>,
    /// #VAPP-258: the fenced block's border colour (its width = the
    /// styles' `code_border`) and the table header row's fill.
    pub code_block_border: Option<Hsla>,
    pub header_bg: Option<Hsla>,
    /// An icon registry name at a size in a colour (the host's icons).
    pub icon: IconPainter,
    /// #VAPP-258: `on_image_press` (`None` = a block image is not pressable).
    pub image_press: Option<LinkHandler>,
    /// #VAPP-258: `on_task_toggle(ordinal, checked)` (`None` = task boxes
    /// are inert).
    pub task_press: Option<TaskPress>,
    /// #VAPP-258: the host's window-level text selection (gpui-base's
    /// `TextSelectionLayer`) selects the units, highlighted in this colour;
    /// `None` = the leaf's own selection.
    pub window_selection: Option<Hsla>,
    /// #VAPP-258: the leaf-local band `(top, bottom)` the host's viewport
    /// shows (a margin included): a lazy block wholly outside it is not
    /// built this frame (its text units stay, not live). `None` = build all.
    pub visible: Option<(f32, f32)>,
    /// #VAPP-258: each attachment block's element by block index (the
    /// Video / AudioPlayer natives and the FileUpload file row, built by
    /// the view that owns them), sized to its laid-out box.
    pub attachments: RefCell<HashMap<usize, AnyElement>>,
    /// #VAPP-258 step 5: the laid-out flag of the block being built (a
    /// lazy block's units are live only once it shows; `None` = live).
    pub live: RefCell<Option<Rc<std::cell::Cell<bool>>>>,
}

/// #VAPP-258: one chip kind's colours over the Chip look.
#[derive(Clone, Default)]
pub struct ChipLook {
    pub background: Option<Hsla>,
    pub border: Option<Hsla>,
    pub border_width: Option<f32>,
    pub label: Option<Hsla>,
    pub ident: Option<Hsla>,
}

impl ChipLook {
    /// Each chip kind's `Markdown/chip`, `chipLabel` and `chipIdent` colours
    /// under `owner_props` (the density); a kind without a rule is absent.
    pub fn of_kinds(theme: Option<&ResolvedTheme>, mode: Mode, owner_props: &Map<String, serde_json::Value>) -> HashMap<String, ChipLook> {
        let mut out = HashMap::new();
        for kind in eui_core::host::contract::MARKDOWN_CHIP_KINDS {
            let mut props = owner_props.clone();
            props.insert("chip".into(), (*kind).into());
            props.entry("density").or_insert_with(|| "document".into());
            let bx = part_visual(theme, mode, "Markdown", "chip", &props, &[]);
            let look = ChipLook {
                background: color_of(bx.background_color.as_deref()),
                border: color_of(bx.border_color.as_deref()),
                border_width: bx.border_width,
                label: color_of(part_visual(theme, mode, "Markdown", "chipLabel", &props, &[]).color.as_deref()),
                ident: color_of(part_visual(theme, mode, "Markdown", "chipIdent", &props, &[]).color.as_deref()),
            };
            if look.background.is_some() || look.border.is_some() || look.border_width.is_some() || look.label.is_some() || look.ident.is_some() {
                out.insert((*kind).to_string(), look);
            }
        }
        out
    }
}

/// A task box press: the task's ordinal in the document, its state before.
pub type TaskPress = Rc<dyn Fn(usize, bool, &mut Window, &mut App)>;

/// An icon name, size and colour → its element.
pub type IconPainter = Rc<dyn Fn(&str, f32, Hsla) -> AnyElement>;

/// A code token kind → its colour (`None` = the ink).
pub type CodeTokenColor = Rc<dyn Fn(&str) -> Option<Hsla>>;

/// Paints a task item's checkbox (`true` = checked).
pub type TaskBox = Rc<dyn Fn(bool) -> AnyElement>;

/// What a markdown link press calls (the href).
pub type LinkHandler = Rc<dyn Fn(&str, &mut Window, &mut App)>;
/// A block image's src → what `img()` paints (`None` = denied).
pub type ImageResolver = Rc<dyn Fn(&str) -> Option<gpui::ImageSource>>;

impl MdPaint {
    /// Runs cut per cluster ([`crate::fallback::cluster_runs`]), as the
    /// measurer shaped them.
    pub fn fallback(&self, text: &str, runs: Vec<TextRun>) -> Vec<TextRun> {
        match &self.text_system {
            Some(ts) => crate::fallback::cluster_runs(ts, text, &runs, &crate::fallback::default_chain()),
            None => runs,
        }
    }

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
fn unit_text(id: SharedString, text: SharedString, runs: Vec<TextRun>, links: Vec<(Range<usize>, String)>, chips: Vec<(Range<usize>, String)>, p: &MdPaint) -> AnyElement {
    let unit = p.units.as_ref().map(|u| u.borrow().get(&p.node).map_or(0, Vec::len)).unwrap_or(0);
    let range = p.selection.filter(|s| s.node == p.node).and_then(|s| s.range_in(unit, text.len()));
    let styled = StyledText::new(text.clone()).with_runs(highlight(runs, range, p.selection_bg));
    // #VAPP-258: the host's window-level selection takes the unit instead.
    let joined = p.window_selection.map(|bg| (styled.layout().clone(), text.clone(), chips.clone(), bg));
    if let Some(units) = &p.units {
        let live = p.live.borrow().clone().unwrap_or_else(|| Rc::new(std::cell::Cell::new(true)));
        units.borrow_mut().entry(p.node).or_default().push(MdUnit { layout: styled.layout().clone(), text, chips, live });
    }
    let el = unit_element(id.clone(), styled, links, p);
    match joined {
        Some((layout, text, chips, bg)) => super::markdown_selection::SelectableUnit::new(format!("{id}.sel"), el, layout, text, bg).chips(chips).into_any_element(),
        None => el,
    }
}

fn unit_element(id: SharedString, styled: StyledText, links: Vec<(Range<usize>, String)>, p: &MdPaint) -> AnyElement {
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

fn text_block(id: SharedString, inlines: &[Inline], spec: &TextSpec, family: &SharedString, p: &MdPaint, colors: RunColors, wrap: f32, text: &mut dyn MdText) -> AnyElement {
    let (shown, runs) = runs(inlines, family, &p.mono, spec.weight, colors);
    let runs = p.fallback(&shown, runs);
    // Link ranges (a chip opens its own href) and the chips' byte offsets.
    let mut links: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    let mut chips: Vec<(usize, &InlineChip, Option<&String>)> = Vec::new();
    let mut labels: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    let mut at = 0;
    for span in inlines {
        match (&span.chip, &span.link) {
            (Some(chip), href) => {
                chips.push((at, chip, href.as_ref()));
                labels.push((at..at + span.text.len(), chip.label.clone()));
            }
            (None, Some(href)) => links.push((at..at + span.text.len(), href.clone())),
            _ => {}
        }
        at += span.text.len();
    }
    let body = unit_text(id.clone(), shown, runs, links, labels, p);
    let mut el = div().relative().w_full().text_size(px(spec.size)).line_height(px(spec.line_height)).child(body);
    if !chips.is_empty() {
        let starts: Vec<usize> = chips.iter().map(|(at, _, _)| *at).collect();
        let spots = text.spots(inlines, spec, wrap, &starts);
        let h = p.styles.chip.box_height(spec.line_height);
        for (k, ((_, chip, href), (x, top, line_w))) in chips.into_iter().zip(spots).enumerate() {
            // A right-aligned (RTL) line starts where its width leaves off.
            let x = if p.rtl { x + (wrap - line_w).max(0.0) } else { x };
            let cid: SharedString = format!("{id}.chip{k}").into();
            el = el.child(if chip.kind == CODE_CHIP {
                inline_code(cid, chip, href.cloned(), (x, top), spec, p, colors)
            } else {
                inline_chip(cid, chip, href.cloned(), (x, top + (spec.line_height - h) / 2.0, h), p, colors.ink)
            });
        }
    }
    el.into_any_element()
}

/// Round 9: an inline code span on its line (`top`) at `x`: the
/// `Markdown/code` box from the font's descent to its ascent around the
/// block's baseline (+ `paddingVertical`), the label in the code type on that
/// baseline, centred across the placeholder.
fn inline_code(id: SharedString, chip: &InlineChip, href: Option<String>, (x, top): (f32, f32), spec: &TextSpec, p: &MdPaint, colors: RunColors) -> AnyElement {
    let code = &p.styles.inline_code;
    let type_ = code.spec(spec, None);
    let color = if href.is_some() { colors.link } else { colors.ink };
    let font = Font { family: p.mono.clone(), features: Default::default(), fallbacks: None, weight: FontWeight(type_.weight as f32), style: FontStyle::Normal };
    let body = Font { family: p.family.clone(), features: Default::default(), fallbacks: None, weight: FontWeight(spec.weight as f32), style: FontStyle::Normal };
    let (ascent, descent, baseline) = p.text_system.as_ref().map_or((type_.size, type_.size * 0.25, spec.line_height * 0.75), |ts| {
        let (mono, base) = (ts.resolve_font(&font), ts.resolve_font(&body));
        // As a line lays out: half-leading + ascent (`descent` reads negative here).
        let (a, d) = (f32::from(ts.ascent(base, px(spec.size))), f32::from(ts.descent(base, px(spec.size))).abs());
        (f32::from(ts.ascent(mono, px(type_.size))), f32::from(ts.descent(mono, px(type_.size))).abs(), (spec.line_height - a - d) / 2.0 + a)
    });
    let label: SharedString = chip.label.clone().into();
    let runs = p.fallback(&label, vec![TextRun { len: label.len(), font, color, background_color: None, underline: href.as_ref().map(|_| UnderlineStyle { thickness: px(1.0), color: Some(colors.link), wavy: false }), strikethrough: None }]);
    let row = div()
        .id(id)
        .absolute()
        .left(px(x))
        .top(px(top + baseline - ascent - code.pad_v))
        .w(px(chip.width))
        .h(px(ascent + descent + 2.0 * code.pad_v))
        .rounded(px(code.radius))
        .when_some(colors.code_bg, |d, bg| d.bg(bg))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(type_.size))
        .line_height(px(ascent + descent))
        .whitespace_nowrap()
        .text_color(color)
        .child(StyledText::new(label).with_runs(runs));
    match href {
        Some(href) => {
            let on_link = p.on_link.clone();
            row.cursor_pointer()
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(move |_, window, cx| on_link(&href, window, cx))
                .into_any_element()
        }
        None => row.into_any_element(),
    }
}

/// #VAPP-258: the inline chip box at `(x, y)` `h` tall in its text block: the
/// Chip look (background, border inside the box, a full radius,
/// `$spacing.sm` side padding), the icon then the caption label, one line.
/// A press opens the policed href like a link and never reaches what is
/// behind (a Row, the text selection).
fn inline_chip(id: SharedString, chip: &InlineChip, href: Option<String>, (x, y, h): (f32, f32, f32), p: &MdPaint, ink: Hsla) -> AnyElement {
    let m = &p.styles.chip.resolved(&chip.kind);
    let own = p.chip_looks.get(&chip.kind).cloned().unwrap_or_default();
    let mut look = p.chip_box.clone();
    look.radii = [m.radius.unwrap_or(h / 2.0); 4];
    if let Some(bg) = own.background {
        look.bg = Some(bg);
    }
    if let Some(c) = own.border {
        look.border_color = Some(c);
    }
    if let Some(w) = own.border_width {
        look.border = [w; 4];
    }
    let color = own.label.or(p.chip_label).unwrap_or(ink);
    let font = Font { family: p.family.clone(), features: Default::default(), fallbacks: None, weight: FontWeight(m.caption.weight as f32), style: FontStyle::Normal };
    let label_font = font.clone();
    let label: SharedString = chip.label.clone().into();
    let runs = p.fallback(&label, vec![TextRun { len: label.len(), font, color, background_color: None, underline: None, strikethrough: None }]);
    let label_el = div().text_size(px(m.caption.size)).line_height(px(m.caption.line_height)).whitespace_nowrap().text_color(color);
    let label_el = if chip.capped { label_el.min_w_0().flex_1().overflow_hidden().text_ellipsis() } else { label_el.flex_none() };
    let mut row = super::natives::styled_box(div(), &look, chip.width, h)
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(chip.width))
        .h(px(h))
        .flex()
        .when(p.rtl, |d| d.flex_row_reverse())
        .items_center()
        .gap(px(m.gap))
        .px(px(m.pad))
        .overflow_hidden();
    if let Some(icon) = &chip.icon {
        row = row.child(div().flex_none().child((p.icon)(icon, m.icon, color)));
    }
    if let Some(ident) = &chip.ident {
        // #VAPP-258: the ident in its own (mono) type, before the label.
        let spec = m.ident_spec(p.styles.code.family.clone());
        let ident_color = own.ident.unwrap_or(color);
        let family = spec.family.clone().map(SharedString::from).unwrap_or_else(|| p.mono.clone());
        let ident_font = Font { family, features: Default::default(), fallbacks: None, weight: FontWeight(spec.weight as f32), style: FontStyle::Normal };
        // Both lines sit centred in the chip; the ident moves by the gap
        // between the two fonts' baselines so it shares the title's.
        let shift = p.text_system.as_ref().map_or(0.0, |ts| baseline_shift(ts, (&label_font, m.caption.size, m.caption.line_height), (&ident_font, spec.size, spec.line_height)));
        let text: SharedString = ident.clone().into();
        let runs = p.fallback(&text, vec![TextRun { len: text.len(), font: ident_font, color: ident_color, background_color: None, underline: None, strikethrough: None }]);
        row = row.child(div().flex_none().relative().top(px(shift)).text_size(px(spec.size)).line_height(px(spec.line_height)).whitespace_nowrap().text_color(ident_color).child(StyledText::new(text).with_runs(runs)));
    }
    let row = row.child(label_el.child(StyledText::new(label).with_runs(runs))).id(id);
    match href {
        Some(href) => {
            let on_link = p.on_link.clone();
            row.cursor_pointer()
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(move |_, window, cx| on_link(&href, window, cx))
                .into_any_element()
        }
        None => row.into_any_element(),
    }
}

/// #VAPP-258: how far a line box of `other` (font, size, line height)
/// moves down so its baseline meets `base`'s when both are centred on one
/// row (gpui: baseline = (line height - ascent - descent) / 2 + ascent).
pub fn baseline_shift(ts: &gpui::TextSystem, base: (&Font, f32, f32), other: (&Font, f32, f32)) -> f32 {
    let at = |(font, size, line_height): (&Font, f32, f32)| -> Option<f32> {
        let id = ts.resolve_font(font);
        Some(-line_height / 2.0 + f32::from(ts.baseline_offset(id, px(size), px(line_height))))
    };
    match (at(base), at(other)) {
        (Some(a), Some(b)) => a - b,
        _ => 0.0,
    }
}

/// The left edge of table column `c` of `cols` (mirrored in RTL: column 0
/// at the right, like an HTML table under `dir=rtl`).
pub fn table_cell_left(c: usize, cols: usize, col_w: f32, rtl: bool) -> f32 {
    let visual = if rtl { cols.saturating_sub(1).saturating_sub(c) } else { c };
    visual as f32 * col_w
}

/// The bullet's width and its centre's distance from the indent's edge, in em.
pub const BULLET_EM: f32 = 0.36;
pub const BULLET_CENTRE_EM: f32 = 0.46;

/// Paint `blocks` laid out at `width` (`lay` from [`layout`] at that width).
pub fn paint(id: &str, blocks: &[Block], lay: &MdLayout, width: f32, p: &MdPaint, text: &mut dyn MdText) -> AnyElement {
    let s = &p.styles;
    if let Some(units) = &p.units {
        units.borrow_mut().insert(p.node, Vec::new());
    }
    let mut root = div().relative().w(px(width)).h(px(lay.height));
    // #VAPP-258 step 5: a long document lays out and paints only the blocks
    // that cross the clip ([`LazyMdBlock`]); every block keeps its box.
    let lazy = blocks.len() >= eui_core::host::MARKDOWN_LAZY_BLOCKS && !forced_eager();
    for (i, (b, bx)) in blocks.iter().zip(&lay.blocks).enumerate() {
        let bid: SharedString = format!("{id}.md{i}").into();
        let live = lazy.then(|| Rc::new(std::cell::Cell::new(false)));
        *p.live.borrow_mut() = live.clone();
        if let (Some(_), Some((top, bottom))) = (&live, p.visible) {
            // Far from the host's viewport: nothing to lay out (the root
            // keeps the document's height), only its units.
            if bx.y + bx.height < top || bx.y > bottom {
                stub_units(b, p);
                continue;
            }
        }
        let placed = if lazy { div().relative() } else { div().absolute().left_0().top(px(bx.y)) }.w(px(width)).h(px(bx.height));
        let el: AnyElement = match &b.kind {
            BlockKind::Paragraph => placed.when(p.rtl, |d| d.text_right()).child(text_block(bid, &b.inlines, &s.body, &p.family, p, p.colors, width.max(1.0), text)).into_any_element(),
            BlockKind::Heading(_) => placed.when(p.rtl, |d| d.text_right()).child(text_block(bid, &b.inlines, s.spec_of(&b.kind), &p.heading_family, p, p.colors, width.max(1.0), text)).into_any_element(),
            BlockKind::ListItem { marker, task, depth } => {
                let lead = s.list_indent * *depth as f32;
                let body_w = (width - lead - s.list_indent).max(1.0);
                let (marker_x, body_x) = if p.rtl { (width - lead - s.list_indent, 0.0) } else { (lead, lead + s.list_indent) };
                let gutter = div().absolute().left(px(marker_x)).top_0().w(px(s.list_indent));
                let gutter = match task {
                    // The Checkbox square, centred on the first line.
                    Some(checked) => {
                        let task_box = (p.task_box)(*checked);
                        let task_box = match &p.task_press {
                            Some(press) => {
                                let (press, checked) = (press.clone(), *checked);
                                let ordinal = blocks[..i].iter().filter(|b| matches!(b.kind, BlockKind::ListItem { task: Some(_), .. })).count();
                                div()
                                    .id(SharedString::from(format!("{bid}.task")))
                                    .role(gpui::Role::CheckBox)
                                    .aria_label(plain(&b.inlines))
                                    .aria_toggled(if checked { gpui::Toggled::True } else { gpui::Toggled::False })
                                    .cursor_pointer()
                                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                    .on_click(move |_, window, cx| {
                                        cx.stop_propagation();
                                        press(ordinal, checked, window, cx)
                                    })
                                    .child(task_box)
                                    .into_any_element()
                            }
                            None => task_box,
                        };
                        gutter.h(px(s.body.line_height)).flex().items_center().when(p.rtl, |d| d.justify_end()).child(task_box)
                    }
                    // The web's outside `disc` at every depth
                    // (`fixtures/markdown-inline.json` `documentPaint`).
                    None if marker == "•" => {
                        let d = s.body.size * BULLET_EM;
                        let from_edge = s.body.size * BULLET_CENTRE_EM - d / 2.0;
                        let x = if p.rtl { s.list_indent - from_edge - d } else { from_edge };
                        gutter.h(px(s.body.line_height)).child(div().absolute().left(px(x)).top(px((s.body.line_height - d) / 2.0)).size(px(d)).rounded_full().bg(p.colors.ink))
                    }
                    // Its outside `3.`, one space before the item's text.
                    None => {
                        let space = text.run_width(" ", &s.body, false, false);
                        gutter.text_size(px(s.body.size)).line_height(px(s.body.line_height)).text_color(p.colors.ink).map(|d| if p.rtl { d.pl(px(space)) } else { d.text_right().pr(px(space)) }).child(SharedString::from(marker.clone()))
                    }
                };
                placed
                    .child(gutter)
                    .child(div().absolute().left(px(body_x)).top_0().w(px(body_w)).when(p.rtl, |d| d.text_right()).child(text_block(bid, &b.inlines, &s.body, &p.family, p, p.colors, body_w, text)))
                    .into_any_element()
            }
            BlockKind::Image { src, alt, size } => {
                let alt_el = text_block(bid.clone(), &b.inlines, &s.body, &p.family, p, RunColors { ink: p.muted, ..p.colors }, width.max(1.0), text);
                match (p.image)(src) {
                    Some(source) => {
                        let fallback_alt: SharedString = alt.clone().into();
                        let muted = p.muted;
                        // #VAPP-258: the host-sized box, start-aligned (the
                        // fixed box fills the width).
                        let (bw, bh) = image_box(*size, width.max(1.0), s.image_height, s.viewport_height);
                        let frame = div()
                            .absolute()
                            .top_0()
                            .left(px(if p.rtl { (width - bw).max(0.0) } else { 0.0 }))
                            .w(px(bw))
                            .h(px(bh))
                            .rounded(px(s.image_radius))
                            .overflow_hidden()
                            .when_some(s.image_border_color.filter(|_| s.image_border_width > 0.0), |d, c| d.border(px(s.image_border_width)).border_color(c))
                            .child(gpui::img(source).size_full().object_fit(gpui::ObjectFit::Contain).with_fallback(move || div().text_color(muted).child(fallback_alt.clone()).into_any_element()));
                        let frame: AnyElement = match &p.image_press {
                            Some(press) => {
                                let (press, src) = (press.clone(), src.clone());
                                frame
                                    .id(SharedString::from(format!("{bid}.image")))
                                    .cursor_pointer()
                                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                    .on_click(move |_, window, cx| press(&src, window, cx))
                                    .into_any_element()
                            }
                            None => frame.into_any_element(),
                        };
                        placed.child(frame).into_any_element()
                    }
                    None => placed.child(alt_el).into_any_element(),
                }
            }
            BlockKind::Attachment { .. } => match p.attachments.borrow_mut().remove(&i) {
                Some(el) => placed.child(el).into_any_element(),
                None => placed.into_any_element(),
            },
            BlockKind::Quote => placed
                .child(div().absolute().left(px(if p.rtl { width - s.quote_border } else { 0.0 })).top_0().h_full().w(px(s.quote_border)).bg(p.border))
                .child(
                    div()
                        .absolute()
                        .left(px(if p.rtl { 0.0 } else { s.quote_border + s.quote_pad }))
                        .when(p.rtl, |d| d.text_right())
                        .top_0()
                        .w(px((width - s.quote_border - s.quote_pad).max(1.0)))
                        .child(text_block(bid, &b.inlines, &s.body, &p.family, p, RunColors { ink: p.muted, ..p.colors }, (width - s.quote_border - s.quote_pad).max(1.0), text)),
                )
                .into_any_element(),
            BlockKind::CodeBlock => {
                let text: SharedString = b.inlines.first().map(|i| i.text.clone()).unwrap_or_default().into();
                let font = Font { family: p.mono.clone(), features: Default::default(), fallbacks: None, weight: FontWeight(s.code.weight as f32), style: FontStyle::Normal };
                let runs = code_block_runs(&text, b.lang.as_deref(), &font, p.colors.ink, &*p.code_token);
                let runs = if text.is_empty() { Vec::new() } else { p.fallback(&text, runs) };
                placed
                    .rounded(px(6.0))
                    .when_some(p.code_block_bg, |d, bg| d.bg(bg))
                    // #VAPP-258: the `Markdown/codeBlock` border, inside the box.
                    .when(s.code_border > 0.0, |d| d.border(px(s.code_border)).border_color(p.code_block_border.unwrap_or(p.border)))
                    .overflow_hidden()
                    .child(
                        div()
                            .absolute()
                            .left(px(s.code_pad))
                            .top(px(s.code_pad))
                            .text_size(px(s.code.size))
                            .line_height(px(s.code.line_height))
                            .whitespace_nowrap()
                            .child(unit_text(bid, text, runs, Vec::new(), Vec::new(), p)),
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
                        let spec = if r == 0 { s.header_cell() } else { s.body.clone() };
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
                                // #VAPP-258: the header row's `Markdown/headerCell` fill.
                                .when(r == 0, |d| d.when_some(p.header_bg, |d, bg| d.bg(bg)))
                                .when(c > 0 && !p.rtl, |d| d.border_l_1().border_color(p.border))
                                .when(c > 0 && p.rtl, |d| d.border_r_1().border_color(p.border))
                                .child(
                                    div()
                                        .absolute()
                                        .left(px(s.cell_pad_h))
                                        .top(px(s.cell_pad_v))
                                        .w(px(cell_text_width(width, cols, s)))
                                        .when(p.rtl, |d| d.text_right())
                                        .child(text_block(format!("{bid}.{r}.{c}").into(), &cell, &spec, &p.family, p, p.colors, cell_text_width(width, cols, s), text)),
                                ),
                        );
                    }
                    y += h;
                }
                table.into_any_element()
            }
        };
        root = match live {
            Some(live) => root.child(LazyMdBlock { y: bx.y, width, height: bx.height, live, child: Some(el), painted: false }),
            None => root.child(el),
        };
    }
    *p.live.borrow_mut() = None;
    root.into_any_element()
}

/// #VAPP-258: the text units a block registers when built, for a block that
/// is not built this frame (outside the host's viewport): same count, same
/// text and chip labels, not live (a selection across it still copies it).
fn stub_units(b: &Block, p: &MdPaint) {
    let Some(units) = &p.units else { return };
    let unit = |inlines: &[Inline]| {
        let mut text = String::new();
        let mut chips = Vec::new();
        for span in inlines.iter().filter(|span| !span.text.is_empty()) {
            if let Some(chip) = &span.chip {
                chips.push((text.len()..text.len() + span.text.len(), chip.label.clone()));
            }
            text.push_str(&span.text);
        }
        (SharedString::from(text), chips)
    };
    let pieces: Vec<(SharedString, Vec<(Range<usize>, String)>)> = match &b.kind {
        BlockKind::Paragraph | BlockKind::Heading(_) | BlockKind::ListItem { .. } | BlockKind::Quote | BlockKind::Image { .. } => vec![unit(&b.inlines)],
        BlockKind::CodeBlock => vec![(b.inlines.first().map(|i| i.text.clone()).unwrap_or_default().into(), Vec::new())],
        BlockKind::Table { header, rows } => {
            let cols = header.len().max(rows.iter().map(Vec::len).max().unwrap_or(0)).max(1);
            std::iter::once(header).chain(rows.iter()).flat_map(|row| (0..cols).map(|c| unit(row.get(c).map_or(&[][..], Vec::as_slice))).collect::<Vec<_>>()).collect()
        }
        BlockKind::Rule | BlockKind::Attachment { .. } => Vec::new(),
    };
    let mut units = units.borrow_mut();
    let list = units.entry(p.node).or_default();
    for (text, chips) in pieces {
        list.push(MdUnit { layout: TextLayout::default(), text, chips, live: Rc::new(std::cell::Cell::new(false)) });
    }
}

#[cfg(test)]
thread_local! {
    /// Tests: lay out every block of a long document (the before picture).
    pub static FORCE_EAGER: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn forced_eager() -> bool {
    #[cfg(test)]
    return FORCE_EAGER.with(std::cell::Cell::get);
    #[cfg(not(test))]
    false
}

/// #VAPP-258 step 5 (`catalog/host.json` markdown `lazyBlocks`, the app's
/// `lazy_block.rs`): one block of a long Markdown at its laid-out box. Layout
/// reserves the box and builds nothing; in prepaint, where the box is final
/// for this frame, the block is laid out and painted only when it crosses
/// the clip (the scroll viewport), and its text units go live (hit-testable)
/// then. The measured height never depends on what shows.
pub struct LazyMdBlock {
    y: f32,
    width: f32,
    height: f32,
    live: Rc<std::cell::Cell<bool>>,
    child: Option<AnyElement>,
    painted: bool,
}

impl IntoElement for LazyMdBlock {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl gpui::Element for LazyMdBlock {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&gpui::GlobalElementId>, _: Option<&gpui::InspectorElementId>, window: &mut Window, cx: &mut App) -> (gpui::LayoutId, ()) {
        let mut style = gpui::Style { position: gpui::Position::Absolute, flex_shrink: 0., ..Default::default() };
        style.inset.top = px(self.y).into();
        style.inset.left = px(0.).into();
        style.size.width = px(self.width).into();
        style.size.height = px(self.height).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(&mut self, _: Option<&gpui::GlobalElementId>, _: Option<&gpui::InspectorElementId>, bounds: gpui::Bounds<gpui::Pixels>, _: &mut (), window: &mut Window, cx: &mut App) {
        self.painted = false;
        if !window.content_mask().bounds.intersects(&bounds) {
            return;
        }
        let Some(child) = self.child.as_mut() else { return };
        child.layout_as_root(gpui::size(gpui::AvailableSpace::Definite(bounds.size.width), gpui::AvailableSpace::Definite(bounds.size.height)), window, cx);
        child.prepaint_at(bounds.origin, window, cx);
        self.painted = true;
        self.live.set(true);
    }

    fn paint(&mut self, _: Option<&gpui::GlobalElementId>, _: Option<&gpui::InspectorElementId>, _: gpui::Bounds<gpui::Pixels>, _: &mut (), _: &mut (), window: &mut Window, cx: &mut App) {
        if self.painted {
            if let Some(child) = self.child.as_mut() {
                child.paint(window, cx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `fixtures/markdown-text-links.json`: a Markdown's text asks the host's text links.
    #[test]
    fn markdown_text_asks_the_hosts_text_links() {
        use eui_core::host::{police_text_links, safe_href, TextLink};
        let path = format!("{}/../../../../packages/exponential-ui/fixtures/markdown-text-links.json", env!("CARGO_MANIFEST_DIR"));
        let f: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json");
        let hook = |text: &str| -> Option<Vec<TextLink>> {
            let mut out = Vec::new();
            let mut at = 0;
            let mut from = 0;
            while let Some(n) = text[from..].find('#') {
                let start = from + n;
                let letters = text[start + 1..].find(|c: char| !c.is_ascii_uppercase()).unwrap_or(text.len() - start - 1);
                let dash = start + 1 + letters;
                let digits = text[dash..].strip_prefix('-').map_or(0, |rest| rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len()));
                from = start + 1;
                if letters == 0 || digits == 0 {
                    continue;
                }
                let end = dash + 1 + digits;
                let Some(href) = f["known"][&text[start + 1..end]].as_str() else { continue };
                out.push(TextLink { text: text[at..start].to_string(), href: None, chip: None });
                out.push(TextLink { text: text[start..end].to_string(), href: Some(href.to_string()), chip: None });
                at = end;
                from = end;
            }
            if out.is_empty() {
                return None;
            }
            out.push(TextLink { text: text[at..].to_string(), href: None, chip: None });
            Some(out)
        };
        let cases = f["cases"].as_array().expect("cases");
        assert!(!cases.is_empty());
        for c in cases {
            let blocks = resolve_urls(parse(c["text"].as_str().unwrap()), |h| safe_href(None, h).is_some(), |_| true);
            let painted = link_text(&blocks, |piece| hook(piece).and_then(|links| police_text_links(None, piece, links))).unwrap_or(blocks);
            let mut links = Vec::new();
            for block in &painted {
                let mut spans: Vec<&Inline> = block.inlines.iter().collect();
                if let BlockKind::Table { header, rows } = &block.kind {
                    spans.extend(header.iter().flatten());
                    spans.extend(rows.iter().flatten().flatten());
                }
                links.extend(spans.iter().filter_map(|s| s.link.as_ref().map(|href| serde_json::json!({"text": s.text, "href": href}))));
            }
            assert_eq!(serde_json::Value::Array(links), c["links"], "{}", c["name"]);
        }
    }

    /// #VAPP-258: `fixtures/markdown-host.json`'s fake host: its text links
    /// (the first `links` key the piece continues with, at each index), its
    /// image sizes and attachments, under the fixture's URL policy.
    struct FakeHost {
        f: serde_json::Value,
        policy: eui_core::host::UrlPolicy,
    }

    impl FakeHost {
        fn load() -> FakeHost {
            let path = format!("{}/../../../../packages/exponential-ui/fixtures/markdown-host.json", env!("CARGO_MANIFEST_DIR"));
            let f: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json");
            let policy = eui_core::host::UrlPolicy { base_url: f["policy"]["baseUrl"].as_str().map(str::to_string), link_base: f["policy"]["linkBase"].as_str().map(str::to_string), ..Default::default() };
            FakeHost { f, policy }
        }

        fn text_links(&self, piece: &str) -> Option<Vec<eui_core::host::TextLink>> {
            use eui_core::host::TextLink;
            let links = self.f["links"].as_object().expect("links");
            let mut out = Vec::new();
            let (mut at, mut i) = (0, 0);
            while i < piece.len() {
                let hit = links.iter().find(|(key, _)| piece[i..].starts_with(key.as_str()));
                match hit {
                    Some((key, v)) => {
                        if i > at {
                            out.push(TextLink { text: piece[at..i].to_string(), ..Default::default() });
                        }
                        let chip = v.get("chip").map(|c| serde_json::from_value(c.clone()).expect("chip"));
                        out.push(TextLink { text: key.clone(), href: v["href"].as_str().map(str::to_string), chip });
                        i += key.len();
                        at = i;
                    }
                    None => i += piece[i..].chars().next().map_or(1, char::len_utf8),
                }
            }
            if out.is_empty() {
                return None;
            }
            if at < piece.len() {
                out.push(TextLink { text: piece[at..].to_string(), ..Default::default() });
            }
            Some(out)
        }

        /// The policed, hooked blocks of `text` (what the painter paints).
        fn blocks(&self, text: &str) -> Vec<Block> {
            let urls = Some(&self.policy);
            let blocks = resolve_urls(parse(text), |h| eui_core::host::safe_href(urls, h).is_some(), |_| true);
            let size = |src: &str| self.f["images"].get(src).map(|v| (v["width"].as_f64().unwrap() as f32, v["height"].as_f64().unwrap() as f32));
            let attachment = |href: &str| self.f["attachments"].get(href).map(|v| serde_json::from_value::<crate::host::MarkdownAttachment>(v.clone()).expect("attachment"));
            let href = |h: &str| eui_core::host::safe_href(urls, h);
            let links = |piece: &str| self.text_links(piece).and_then(|links| eui_core::host::police_text_links(urls, piece, links));
            apply_hooks(&blocks, &MdHooks { attachment: &attachment, image_size: &size, href: &href, text_links: &links }).unwrap_or(blocks)
        }
    }

    /// The markdown-render.json normal form plus `chip`, `attachment` and an
    /// image's `width` / `height`; `link` = the href as painted (policed).
    fn host_normal(blocks: &[Block], href: &dyn Fn(&str) -> Option<String>) -> serde_json::Value {
        use serde_json::{json, Map, Value};
        let num = |v: f32| if v.fract() == 0.0 { json!(v as i64) } else { json!(v) };
        let runs = |inlines: &[Inline]| -> Value {
            let mut out: Vec<Map<String, Value>> = Vec::new();
            let mut last: Option<(bool, bool, bool, bool, Option<String>)> = None;
            for run in inlines.iter().filter(|r| !r.text.is_empty()) {
                let link = run.link.as_deref().and_then(href);
                let key = (run.bold, run.italic, run.code, run.strike, link.clone());
                if run.chip.is_none() && last.as_ref() == Some(&key) {
                    let prev = out.last_mut().unwrap();
                    let text = format!("{}{}", prev["text"].as_str().unwrap(), run.text);
                    prev.insert("text".into(), json!(text));
                    continue;
                }
                let mut r = Map::new();
                let text = run.chip.as_ref().map_or(run.text.as_str(), |c| c.source.as_str());
                r.insert("text".into(), json!(text));
                if run.bold { r.insert("bold".into(), json!(true)); }
                if run.italic { r.insert("italic".into(), json!(true)); }
                if run.code { r.insert("code".into(), json!(true)); }
                if run.strike { r.insert("strike".into(), json!(true)); }
                if let Some(link) = &link { r.insert("link".into(), json!(link)); }
                if let Some(chip) = &run.chip {
                    let mut c = json!({"kind": chip.kind, "label": chip.label});
                    if let Some(icon) = &chip.icon { c["icon"] = json!(icon); }
                    if let Some(ident) = &chip.ident { c["ident"] = json!(ident); }
                    r.insert("chip".into(), c);
                }
                out.push(r);
                last = if run.chip.is_some() { None } else { Some(key) };
            }
            Value::Array(out.into_iter().map(Value::Object).collect())
        };
        Value::Array(
            blocks
                .iter()
                .map(|b| match &b.kind {
                    BlockKind::Paragraph => json!({ "kind": "paragraph", "runs": runs(&b.inlines) }),
                    BlockKind::Quote => json!({ "kind": "quote", "runs": runs(&b.inlines) }),
                    BlockKind::Heading(level) => json!({ "kind": "heading", "level": level, "runs": runs(&b.inlines) }),
                    BlockKind::ListItem { marker, task, depth } => {
                        let mut o = json!({ "kind": "listItem", "marker": marker, "depth": depth, "runs": runs(&b.inlines) });
                        if let Some(task) = task { o["task"] = json!(task); }
                        o
                    }
                    BlockKind::CodeBlock => json!({ "kind": "codeBlock", "text": plain(&b.inlines) }),
                    BlockKind::Table { header, rows } => json!({
                        "kind": "table",
                        "header": header.iter().map(|c| runs(c)).collect::<Vec<_>>(),
                        "rows": rows.iter().map(|r| r.iter().map(|c| runs(c)).collect::<Vec<_>>()).collect::<Vec<_>>(),
                    }),
                    BlockKind::Rule => json!({ "kind": "rule" }),
                    BlockKind::Image { src, alt, size } => {
                        let mut o = json!({ "kind": "image", "src": src, "alt": alt });
                        if let Some((w, h)) = size { o["width"] = num(*w); o["height"] = num(*h); }
                        o
                    }
                    BlockKind::Attachment { kind, name, href } => json!({ "kind": "attachment", "type": kind.name(), "name": name, "href": href }),
                })
                .collect(),
        )
    }

    /// #VAPP-258: `fixtures/markdown-host.json`: the fake host's answers
    /// through the painter's pipeline (policy → hooks → text links) in the
    /// normal form, and the block image boxes.
    #[test]
    fn markdown_host_fixture() {
        let host = FakeHost::load();
        let href = |h: &str| eui_core::host::safe_href(Some(&host.policy), h);
        let cases = host.f["cases"].as_array().expect("cases");
        assert!(!cases.is_empty());
        let mut failures = Vec::new();
        for case in cases {
            let actual = host_normal(&host.blocks(case["text"].as_str().unwrap()), &href);
            if actual != case["blocks"] {
                failures.push(format!("{}\n  expected {}\n  actual   {actual}", case["name"], case["blocks"]));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        let max = host.f["imageMaxHeight"].as_f64().map_or(eui_core::host::MARKDOWN_IMAGE_MAX_HEIGHT, |v| v) as f32;
        assert_eq!(max, 160.0);
        for b in host.f["imageBoxes"].as_array().expect("imageBoxes") {
            let pair = |v: &serde_json::Value| v.as_object().map(|s| (s["width"].as_f64().unwrap() as f32, s["height"].as_f64().unwrap() as f32));
            // `loaded`: an unknown (or invalid) host size takes the decoded one.
            let size = usable_size(pair(&b["size"])).or_else(|| pair(&b["loaded"]));
            // `maxHeight`: absent = 160, null = none; `viewport`: absent = unknown.
            let cap = match b.get("maxHeight") {
                None => Some(max),
                Some(v) => v.as_f64().map(|v| v as f32),
            };
            let (w, h) = image_box(size, b["available"].as_f64().unwrap() as f32, cap, b["viewport"].as_f64().map(|v| v as f32));
            let (ew, eh) = (b["box"]["width"].as_f64().unwrap() as f32, b["box"]["height"].as_f64().unwrap() as f32);
            assert!((w - ew).abs() < 0.01 && (h - eh).abs() < 0.01, "{}: {w}×{h}, expected {ew}×{eh}", b["name"]);
        }
    }

    /// #VAPP-258: no hook, no answer = the blocks as without hooks.
    #[test]
    fn no_host_answer_changes_nothing() {
        let blocks = parse("see #ABC-1\n\n[clip](https://x.example/clip.mp4)\n\n![a](https://x.example/a.png)");
        let none = MdHooks { attachment: &|_| None, image_size: &|_| None, href: &|h| Some(h.to_string()), text_links: &|_| None };
        assert!(apply_hooks(&blocks, &none).is_none());
        assert!(size_chips(&blocks, &MdStyles::plain(body()), Some(300.0), &mut Fixed).is_none());
    }

    #[test]
    fn a_host_size_changes_the_image_height() {
        let s = MdStyles::plain(body());
        let src = "https://x.example/banner.png";
        let blocks = parse(&format!("![banner]({src})"));
        assert_eq!(layout(&blocks, &s, 320.0, &mut Fixed).height, 160.0, "the fixed box");
        let hooks = MdHooks { attachment: &|_| None, image_size: &|s| (s == src).then_some((800.0, 200.0)), href: &|h| Some(h.to_string()), text_links: &|_| None };
        let sized = apply_hooks(&blocks, &hooks).expect("sized");
        assert_eq!(layout(&sized, &s, 320.0, &mut Fixed).height, 80.0, "320 / 800 of 200");
        assert_eq!(layout(&sized, &s, 160.0, &mut Fixed).height, 40.0);
        let bad = MdHooks { image_size: &|_| Some((f32::NAN, 10.0)), ..hooks };
        assert!(apply_hooks(&blocks, &bad).is_none(), "a non-finite size is no size");
    }

    #[test]
    fn attachment_blocks_reserve_their_natives_height() {
        let s = MdStyles::plain(body());
        let text = "[a](https://x.example/a.mp4)\n\n[b](https://x.example/b.m4a)\n\n[c](https://x.example/c.pdf)";
        let kind = |h: &str| Some(crate::host::MarkdownAttachment { kind: if h.ends_with("mp4") { "video" } else if h.ends_with("m4a") { "audio" } else { "file" }.into(), name: h.rsplit('/').next().unwrap().into() });
        let hooks = MdHooks { attachment: &kind, image_size: &|_| None, href: &|h| Some(h.to_string()), text_links: &|_| None };
        let blocks = apply_hooks(&parse(text), &hooks).expect("attachments");
        assert!(blocks.iter().all(|b| matches!(b.kind, BlockKind::Attachment { .. })));
        let lay = layout(&blocks, &s, 600.0, &mut Fixed);
        let video = 320.0 / s.attach.media_ratio;
        assert_eq!(lay.blocks.iter().map(|b| b.height).collect::<Vec<_>>(), vec![video, 20.0 + 4.0 + 40.0, 40.0]);
        assert_eq!(lay.height, video + 8.0 + 64.0 + 8.0 + 40.0);
        // A narrow column narrows the video box (16:9 kept).
        assert!((layout(&blocks[..1], &s, 160.0, &mut Fixed).height - 90.0).abs() < 0.01);
    }

    #[test]
    fn a_chip_placeholder_reserves_at_least_its_width() {
        // 3.9 / 2.4 px spaces: the fewest that cover 50 px.
        let fill = chip_fill(50.0, 400.0, 3.9, 2.4).0;
        let w = |f: &str| f.chars().map(|c| if c == CHIP_FILL[0] { 3.9 } else { 2.4 }).sum::<f32>();
        assert!(w(&fill) >= 50.0 && w(&fill) < 50.0 + 1.0, "{}", w(&fill));
        assert!(w(&chip_fill(50.0, 40.0, 3.9, 2.4).0) <= 40.0, "never past the cap");
        assert_eq!(chip_fill(16.0, 400.0, 8.0, 8.0).0.chars().count(), 2, "one width: whole spaces");
        assert!(chip_fill(1.0, 0.5, 8.0, 8.0).0.chars().count() == 1, "never empty");
    }

    /// #VAPP-258: a host-sized chip through gpui's text system: a chip that
    /// fits keeps the line (the measured height = the plain text's), the
    /// measured wrap is the painted one (the spots), and a chip that does
    /// not fit moves WHOLE to the next line.
    #[gpui::test]
    fn chips_measure_as_they_paint(cx: &mut gpui::TestAppContext) {
        let window = cx.add_window(|_, _| gpui::Empty);
        window
            .update(cx, |_, window, cx| {
                let fonts = crate::measure::Fonts::new(Rc::new(crate::host::NoHost), None, cx.text_system().clone());
                let mut sh = crate::measure::Shaper::new(window, &fonts);
                let s = MdStyles::plain(body());
                let host = FakeHost::load();
                let chipped = host.blocks("see #ABC-1 now");
                let plain_text = parse("see #ABC-1 now");
                let sized = size_chips(&chipped, &s, Some(400.0), &mut sh).expect("a chip");
                let chip = sized[0].inlines.iter().find_map(|i| i.chip.clone()).expect("chip");
                let label = MdText::run_width(&mut sh, "ABC-1 Fix login", &s.chip.caption, false, false);
                let want = s.chip.width(label, true, None);
                let ph = sized[0].inlines.iter().find(|i| i.chip.is_some()).unwrap().text.clone();
                assert!(ph.chars().all(|c| CHIP_FILL.contains(&c)));
                let wide = MdText::run_width(&mut sh, &CHIP_FILL[0].to_string(), &s.body, false, false);
                assert!(chip.width >= want - 0.01 && chip.width < want + wide, "the chip box {} covers its look {want}", chip.width);
                // #VAPP-258 step 5: the box spans its placeholder exactly (no stray gap after it).
                assert!((MdText::run_width(&mut sh, &ph, &s.body, false, false) - chip.width).abs() < 0.05, "the placeholder is the chip box");
                let h = layout(&sized, &s, 400.0, &mut sh).height;
                assert_eq!(h, layout(&plain_text, &s, 400.0, &mut sh).height, "a chip never grows a line");
                assert_eq!(h, s.body.line_height);
                let start = "see ".len();
                let spot = sh.spots(&sized[0].inlines, &s.body, 400.0, &[start])[0];
                assert_eq!(spot.1, 0.0);
                assert!((spot.0 - MdText::run_width(&mut sh, "see ", &s.body, false, false)).abs() < 0.01);
                // Too narrow for "see " + the chip: the chip starts line 2.
                let narrow = MdText::run_width(&mut sh, "see ", &s.body, false, false) + chip.width - 1.0;
                let sized = size_chips(&chipped, &s, Some(narrow), &mut sh).unwrap();
                let lay = layout(&sized, &s, narrow, &mut sh);
                let spot = sh.spots(&sized[0].inlines, &s.body, narrow, &[start])[0];
                assert_eq!((spot.0, spot.1), (0.0, s.body.line_height), "the chip wraps whole");
                assert!(lay.height >= 2.0 * s.body.line_height);
                // Wider than the column: capped, the label truncates.
                let sized = size_chips(&chipped, &s, Some(60.0), &mut sh).unwrap();
                let chip = sized[0].inlines.iter().find_map(|i| i.chip.clone()).unwrap();
                assert!(chip.capped && chip.width <= 60.0);
            })
            .unwrap();
    }

    /// #VAPP-258: in a surface, a chip press opens its policed href (not
    /// the text behind), a block image press calls `on_image_press`, and the
    /// measured frame is the painted layout's height.
    #[gpui::test]
    fn chip_and_image_presses_reach_the_host(cx: &mut gpui::TestAppContext) {
        use crate::view::{SurfaceView, SurfaceViewOptions};
        use std::cell::RefCell;
        #[derive(Default, Clone)]
        struct Host {
            opened: Rc<RefCell<Vec<String>>>,
            pressed: Rc<RefCell<Vec<String>>>,
        }
        impl crate::host::HostPlugin for Host {
            fn url_policy(&self) -> Option<eui_core::host::UrlPolicy> {
                Some(FakeHost::load().policy)
            }
            fn text_links(&self, text: &str, _: &App) -> Option<Vec<eui_core::host::TextLink>> {
                FakeHost::load().text_links(text)
            }
            fn open_url(&self, url: &str, _: &mut App) {
                self.opened.borrow_mut().push(eui_core::host::safe_href(self.url_policy().as_ref(), url).unwrap_or_default());
            }
            fn image_size(&self, _: &str, _: &App) -> Option<(f32, f32)> {
                Some((400.0, 100.0))
            }
            fn image_pressable(&self, _: &App) -> bool {
                true
            }
            fn on_image_press(&self, src: &str, _: &mut Window, _: &mut App) {
                self.pressed.borrow_mut().push(src.to_string());
            }
            fn attachment(&self, href: &str, _: &App) -> Option<crate::host::MarkdownAttachment> {
                FakeHost::load().f["attachments"].get(href).map(|v| serde_json::from_value(v.clone()).unwrap())
            }
        }
        cx.update(gpui_component::init);
        let host = Host::default();
        let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("neutral"), mode: Mode::Light, host: Rc::new(host.clone()), width: Some(400.0), ..Default::default() };
        let (view, cx) = cx.add_window_view(move |window, cx| SurfaceView::new(options, window, cx));
        let tree = serde_json::json!({"id": "root", "component": "Box", "style": {"width": 400}, "children": [
            {"id": "md", "component": "Markdown", "props": {"text": "see #ABC-1 now\n\n![x](https://app.example/a.png)\n\n[the report](https://app.example/api/attachments/report.pdf)\n\n[clip](https://app.example/api/attachments/clip.mp4)\n\n[voice](https://app.example/api/attachments/voice.m4a)"}}
        ]});
        view.update(cx, |v, cx| {
            v.set_nested(serde_json::from_value(tree).unwrap(), cx);
            v.record_bounds(true);
        });
        for _ in 0..2 {
            cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.run_until_parked();
        }
        let b = view.read_with(cx, |v, _| v.painted_bounds("md")).expect("painted");
        let w = f32::from(b.size.width);
        // One body line, the gap, the 400×100 image scaled to the width.
        let theme = eui_core::themes::builtin_theme("neutral").unwrap();
        let body_lh = crate::paint::parts::px_prop(&crate::paint::parts::part_props(Some(&theme), Mode::Light, "Text", "root", &Map::new(), &[]), "lineHeight").unwrap_or(20.0);
        let styles = MdStyles::resolve(Some(&theme), Mode::Light, TextSpec { size: 14.0, line_height: body_lh, weight: 400, family: None }, &Map::new());
        let gap = spacing(Some(&theme), "sm");
        let file_top = body_lh + gap + image_box(Some((400.0, 100.0)), w, Some(160.0), None).1 + gap;
        let expected = file_top
            + [AttachmentKind::File, AttachmentKind::Video, AttachmentKind::Audio].iter().map(|k| attachment_height(*k, &styles, w)).sum::<f32>()
            + 2.0 * gap;
        assert!((f32::from(b.size.height) - expected).abs() < 0.5, "measured {} vs laid out {expected}", f32::from(b.size.height));
        // The chip sits after "see " on the first line.
        let at = b.origin + gpui::point(px(60.0), px(body_lh / 2.0));
        cx.simulate_click(at, gpui::Modifiers::default());
        assert_eq!(*host.opened.borrow(), vec!["https://app.example/i/ABC-1".to_string()]);
        let img = b.origin + gpui::point(px(10.0), px(body_lh + spacing(Some(&theme), "sm") + 10.0));
        cx.simulate_click(img, gpui::Modifiers::default());
        assert_eq!(*host.pressed.borrow(), vec!["https://app.example/a.png".to_string()]);
        // The file row opens its policed href.
        cx.simulate_click(b.origin + gpui::point(px(40.0), px(file_top + 10.0)), gpui::Modifiers::default());
        assert_eq!(host.opened.borrow().last().map(String::as_str), Some("https://app.example/api/attachments/report.pdf"));
    }

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
        fn run_width(&mut self, text: &str, _: &TextSpec, _: bool, _: bool) -> f32 {
            text.chars().count() as f32 * 8.0
        }
        fn spots(&mut self, inlines: &[Inline], _: &TextSpec, _: f32, at: &[usize]) -> Vec<(f32, f32, f32)> {
            let text = plain(inlines);
            at.iter().map(|&a| (text[..a].chars().count() as f32 * 8.0, 0.0, text.chars().count() as f32 * 8.0)).collect()
        }
    }

    fn body() -> TextSpec {
        TextSpec { size: 14.0, line_height: 20.0, weight: 400, family: None }
    }

    /// #EXP-1728: the header row is measured in the weight it paints in
    /// (semibold is wider: a header that fits in the body weight wraps).
    #[test]
    fn a_table_header_wraps_in_its_semibold_weight() {
        struct Weighted;
        impl MdText for Weighted {
            fn height(&mut self, inlines: &[Inline], spec: &TextSpec, width: Option<f32>) -> f32 {
                let per = if spec.weight >= 600 { 9.0 } else { 8.0 };
                let w = plain(inlines).chars().count() as f32 * per;
                width.map_or(1.0, |width| (w / width).ceil().max(1.0)) * spec.line_height
            }
            fn width(&mut self, inlines: &[Inline], s: &TextSpec) -> f32 {
                Fixed.width(inlines, s)
            }
            fn widest_word(&mut self, inlines: &[Inline], s: &TextSpec) -> f32 {
                Fixed.widest_word(inlines, s)
            }
            fn run_width(&mut self, text: &str, s: &TextSpec, bold: bool, italic: bool) -> f32 {
                Fixed.run_width(text, s, bold, italic)
            }
            fn spots(&mut self, inlines: &[Inline], s: &TextSpec, width: f32, at: &[usize]) -> Vec<(f32, f32, f32)> {
                Fixed.spots(inlines, s, width, at)
            }
        }
        let s = MdStyles::plain(body());
        // One column 216 wide = 200 of text: 25 characters fit at 8 px, not at 9.
        let blocks = parse("| C shared renderer (vel) |\n|---|\n| pixel-identical |");
        let lay = layout(&blocks, &s, 200.0 + 2.0 * s.cell_pad_h, &mut Weighted);
        let row = |lines: f32| lines * s.body.line_height + 2.0 * s.cell_pad_v + 1.0;
        assert_eq!(lay.blocks[0].rows, vec![row(2.0), row(1.0)], "the header wraps to two lines");
    }

    /// #EXP-1728: a cell's text wraps at whole pixels (a fractional width
    /// the painted box rounds down from wrapped a line the layout counted as
    /// one): 19 characters = 152 px fit 152.75 at measure, not 152 at paint.
    #[test]
    fn table_cells_wrap_at_whole_pixels() {
        let s = MdStyles::plain(body());
        let cell = 152.75 + 2.0 * s.cell_pad_h;
        let blocks = parse("| a | b | c | d |\n|---|---|---|---|\n| Cost of a new compo | x | y | z |");
        let lay = layout(&blocks, &s, 4.0 * cell, &mut Fixed);
        assert_eq!(cell_text_width(4.0 * cell, 4, &s), 152.0);
        assert_eq!(lay.blocks[0].rows[1], 20.0 + 2.0 * s.cell_pad_v + 1.0, "19 × 8 = 152 fits 152");
        let lay = layout(&parse("| a |\n|---|\n| Cost of a new compon |"), &s, cell, &mut Fixed);
        assert_eq!(lay.blocks[0].rows[1], 40.0 + 2.0 * s.cell_pad_v + 1.0, "160 px wraps at 152");
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
                BlockKind::Attachment { .. } => "attachment",
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

    /// VAPP-103 rfix: 100k nested link labels aborted on the stack (and
    /// cost O(n²)); unclosed runs of `[`, `[a](<` and `[a](x(` were
    /// quadratic. Labels nest 32 deep (deeper = text), every case is linear
    /// and runs on a small stack.
    #[test]
    fn hostile_brackets_parse_linearly_on_a_small_stack() {
        let n = 100_000;
        let cases = [
            format!("{}a{}", "[".repeat(n), "](x)".repeat(n)),
            "[".repeat(n),
            "[a](<".repeat(n / 5),
            "[a](x(".repeat(n / 6),
            "![a](".repeat(n / 5),
            format!("{}a{}", "[*".repeat(n / 2), "*](x)".repeat(n / 2)),
        ];
        for case in cases {
            let t = std::time::Instant::now();
            let text = std::thread::Builder::new().stack_size(256 * 1024).spawn(move || plain(&parse_inline(&case)).len()).unwrap().join().expect("no stack overflow");
            assert!(text > 0);
            assert!(t.elapsed() < std::time::Duration::from_secs(3), "{:?}", t.elapsed());
        }
        // 32 levels still link; the 33rd label is text.
        let deep = |k: usize| format!("{}a{}", "[".repeat(k), "](x)".repeat(k));
        assert!(parse_inline(&deep(32)).iter().all(|s| s.link.as_deref() == Some("x")));
        assert!(parse_inline(&deep(33)).iter().any(|s| s.text.contains('[')));
    }

    /// Round 9: inline code is a placeholder under its own box, its padding
    /// in the line; one wider than the line, and a fence, stay text.
    #[test]
    fn inline_code_reserves_its_box_and_a_fence_does_not() {
        let mut s = MdStyles::plain(body());
        s.inline_code = InlineCode { size: Some(12.0), pad_h: Some(4.0), pad_v: 0.0, radius: 6.0 };
        let blocks = parse("see `code`\n\n```\nfenced\n```");
        let sized = size_chips(&blocks, &s, Some(300.0), &mut Fixed).expect("a code span");
        let chip = sized[0].inlines[1].chip.as_ref().expect("the code chip");
        assert_eq!((chip.kind.as_str(), chip.label.as_str()), (CODE_CHIP, "code"));
        assert!(chip.width >= Fixed.code_width("code", &s.body, false, false) + 8.0);
        assert!(sized[1].inlines[0].chip.is_none(), "a fence stays text");
        let narrow = size_chips(&blocks, &s, Some(10.0), &mut Fixed).expect("a code span");
        assert!(narrow[0].inlines.iter().all(|i| i.chip.is_none()), "wider than its line: it breaks");
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
        let blocks = parse("![chart](https://exponential.at/c.png)\n\ntext **![inline](https://x/y.png)** [bad](javascript:alert(1)) [ok](https://exponential.at)");
        assert_eq!(blocks[0].kind, BlockKind::Image { src: "https://exponential.at/c.png".into(), alt: "chart".into(), size: None });
        assert_eq!(plain(&blocks[1].inlines), "text inline bad ok", "an image inside a span is its alt text");
        // #FEED-208: an image directly in a top-level paragraph splits it.
        let split = parse("see ![x](https://x/y.png) here");
        assert_eq!(split.len(), 3);
        assert_eq!(split[1].kind, BlockKind::Image { src: "https://x/y.png".into(), alt: "x".into(), size: None });
        assert_eq!((plain(&split[0].inlines), plain(&split[2].inlines)), ("see".to_string(), "here".to_string()));
        let s = MdStyles::plain(body());
        assert_eq!(Some(layout(&blocks[..1], &s, 300.0, &mut Fixed).height), s.image_height);
        let resolved = resolve_urls(blocks, |h| h.starts_with("https:"), |_| false);
        assert_eq!(resolved[0].kind, BlockKind::Paragraph, "a denied image is its alt text");
        assert_eq!(plain(&resolved[0].inlines), "chart");
        let links: Vec<Option<&str>> = resolved[1].inlines.iter().map(|s| s.link.as_deref()).filter(Option::is_some).collect();
        assert_eq!(links, [Some("https://exponential.at")], "a denied link is plain text");
        let denied = parse("![x](javascript:alert(3))");
        assert_eq!(resolve_urls(denied, |_| true, |src| !src.starts_with("javascript:"))[0].kind, BlockKind::Paragraph);
        assert!(resolve_urls(parse("![](javascript:x)"), |_| true, |_| false).is_empty(), "a denied image without alt text is dropped");
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

    /// #VAPP-258: `fixtures/markdown-render.json`, the golden render fixture:
    /// this parser's blocks in the shared normal form. A pending case must
    /// still differ (the list only shrinks).
    #[test]
    fn markdown_render_fixture() {
        use serde_json::{json, Map, Value};
        let path = format!("{}/../../../../packages/exponential-ui/fixtures/markdown-render.json", env!("CARGO_MANIFEST_DIR"));
        let f: Value = serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json");
        let pending: Vec<&str> = f["pending"]["gpui"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
        fn runs(inlines: &[Inline]) -> Value {
            let mut out: Vec<Map<String, Value>> = Vec::new();
            let mut last: Option<(bool, bool, bool, bool, Option<String>)> = None;
            for run in inlines.iter().filter(|r| !r.text.is_empty()) {
                let key = (run.bold, run.italic, run.code, run.strike, run.link.clone());
                if last.as_ref() == Some(&key) {
                    let prev = out.last_mut().unwrap();
                    let text = format!("{}{}", prev["text"].as_str().unwrap(), run.text);
                    prev.insert("text".into(), json!(text));
                    continue;
                }
                let mut r = Map::new();
                r.insert("text".into(), json!(run.text));
                if run.bold { r.insert("bold".into(), json!(true)); }
                if run.italic { r.insert("italic".into(), json!(true)); }
                if run.code { r.insert("code".into(), json!(true)); }
                if run.strike { r.insert("strike".into(), json!(true)); }
                if let Some(link) = &run.link { r.insert("link".into(), json!(link)); }
                out.push(r);
                last = Some(key);
            }
            Value::Array(out.into_iter().map(Value::Object).collect())
        }
        let normal = |blocks: Vec<Block>| -> Value {
            Value::Array(
                blocks
                    .into_iter()
                    .map(|b| match &b.kind {
                        BlockKind::Paragraph => json!({ "kind": "paragraph", "runs": runs(&b.inlines) }),
                        BlockKind::Quote => json!({ "kind": "quote", "runs": runs(&b.inlines) }),
                        BlockKind::Heading(level) => json!({ "kind": "heading", "level": level, "runs": runs(&b.inlines) }),
                        BlockKind::ListItem { marker, task, depth } => {
                            let mut o = json!({ "kind": "listItem", "marker": marker, "depth": depth, "runs": runs(&b.inlines) });
                            if let Some(task) = task { o["task"] = json!(task); }
                            o
                        }
                        BlockKind::CodeBlock => {
                            let mut o = json!({ "kind": "codeBlock", "text": b.inlines.iter().map(|i| i.text.as_str()).collect::<String>() });
                            if let Some(lang) = &b.lang { o["lang"] = json!(lang); }
                            o
                        }
                        BlockKind::Table { header, rows } => json!({
                            "kind": "table",
                            "header": header.iter().map(|c| runs(c)).collect::<Vec<_>>(),
                            "rows": rows.iter().map(|r| r.iter().map(|c| runs(c)).collect::<Vec<_>>()).collect::<Vec<_>>(),
                        }),
                        BlockKind::Rule => json!({ "kind": "rule" }),
                        BlockKind::Image { src, alt, .. } => json!({ "kind": "image", "src": src, "alt": alt }),
                        BlockKind::Attachment { .. } => unreachable!("no host, no attachment"),
                    })
                    .collect(),
            )
        };
        let mut failures = Vec::new();
        for case in f["cases"].as_array().expect("cases") {
            let name = case["name"].as_str().unwrap();
            let actual = normal(parse(case["text"].as_str().unwrap()));
            let same = actual == case["blocks"];
            if pending.contains(&name) {
                if same { failures.push(format!("{name} passes now: drop it from pending.gpui")); }
            } else if !same {
                failures.push(format!("{name}\n  expected {}\n  actual   {actual}", case["blocks"]));
            }
        }
        // #VAPP-258 step 5: host options (`breaks`), and documents streamed in
        // prefix by prefix, in order (the memo) and cold.
        let hard = |o: &Value| o["breaks"].as_str() == Some("hard");
        for case in f["optionCases"].as_array().expect("optionCases") {
            let actual = normal(parse_with(case["text"].as_str().unwrap(), hard(&case["options"]), false));
            if actual != case["blocks"] {
                failures.push(format!("{}\n  expected {}\n  actual   {actual}", case["name"], case["blocks"]));
            }
        }
        for case in f["streamingCases"].as_array().expect("streamingCases") {
            let text = case["text"].as_str().unwrap();
            let h = hard(&case["options"]);
            for cold in [false, true] {
                reset_stream_memo();
                for (i, blocks) in case["steps"].as_array().unwrap().iter().enumerate() {
                    if cold {
                        reset_stream_memo();
                    }
                    let actual = normal(parse_with(&text[..=i], h, true));
                    if actual != *blocks {
                        failures.push(format!("{} {:?} (cold {cold})\n  expected {blocks}\n  actual   {actual}", case["name"], &text[..=i]));
                        break;
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// #VAPP-258 step 5: a streamed parse's stable blocks keep their list
    /// groups apart (a gap between two lists), and the memo answers a text
    /// that extends a remembered one from its boundary on.
    #[test]
    fn streamed_lists_keep_their_groups() {
        reset_stream_memo();
        let _ = parse_streaming("- a\n- b\n\n", false);
        let blocks = parse_streaming("- a\n- b\n\n- c", false);
        let groups: Vec<Option<usize>> = blocks.iter().map(|b| b.list_group).collect();
        assert_eq!(groups[0], groups[1]);
        assert_ne!(groups[1], groups[2], "a later list is its own group");
        assert_eq!(blocks, parse_with("- a\n- b\n\n- c", false, false));
    }

    /// #VAPP-258 step 5: `density: chat` resolves the Markdown recipes with
    /// the chat rule (the neutral theme: a 22 px body line, base headings,
    /// the `sm` gap); the document keeps the node's text style.
    #[test]
    fn chat_density_resolves_the_chat_recipes() {
        let theme = eui_core::themes::builtin_theme("neutral").expect("neutral");
        let body = TextSpec { size: 14.0, line_height: 20.0, weight: 400, family: None };
        let doc = MdStyles::resolve(Some(&theme), Mode::Light, body.clone(), &Map::new());
        let chat = MdStyles::resolve(Some(&theme), Mode::Light, body, &density_props(&Map::new(), true));
        assert_eq!(doc.body.line_height, 20.0);
        assert_eq!(chat.body.line_height, 22.0);
        assert_eq!(chat.block_gap, spacing(Some(&theme), "sm"));
        assert!(chat.heading.size < doc.heading.size, "chat headings sit one step down");
        let blocks = parse("one\n\ntwo");
        assert!(layout(&blocks, &chat, 300.0, &mut Fixed).height > layout(&blocks, &doc, 300.0, &mut Fixed).height);
    }

    /// #VAPP-258 step 5 (`lazyBlocks`): a 60-block Markdown in a 200 px
    /// ScrollArea lays out only the blocks that show (their units go live),
    /// at the same measured height as the eager layout; scrolling brings the
    /// next blocks in.
    #[gpui::test]
    fn a_long_markdown_lays_out_only_visible_blocks(cx: &mut gpui::TestAppContext) {
        use crate::view::{SurfaceView, SurfaceViewOptions};
        cx.update(gpui_component::init);
        let text = (0..60).map(|i| format!("Paragraph {i} of a long description.")).collect::<Vec<_>>().join("\n\n");
        let options = SurfaceViewOptions { theme: eui_core::themes::builtin_theme("neutral"), mode: Mode::Light, host: Rc::new(crate::host::NoHost), width: Some(400.0), ..Default::default() };
        let (view, cx) = cx.add_window_view(move |window, cx| SurfaceView::new(options, window, cx));
        let tree = serde_json::json!({"id": "root", "component": "ScrollArea", "style": {"width": 400, "height": 200}, "children": [
            {"id": "md", "component": "Markdown", "props": {"text": text}}
        ]});
        let mut run = |eager: bool| {
            FORCE_EAGER.with(|f| f.set(eager));
            view.update(cx, |v, cx| {
                v.set_nested(serde_json::from_value(tree.clone()).unwrap(), cx);
                v.record_bounds(true);
                cx.notify();
            });
            for _ in 0..2 {
                cx.update(|window, cx| window.draw(cx).clear(cx));
                cx.run_until_parked();
            }
            let h = view.read_with(cx, |v, _| v.painted_bounds("md")).expect("painted").size.height;
            (f32::from(h), view.read_with(cx, |v, _| v.md_unit_counts("md")))
        };
        let (eager_h, (eager_live, total)) = run(true);
        let (lazy_h, (lazy_live, lazy_total)) = run(false);
        FORCE_EAGER.with(|f| f.set(false));
        assert_eq!(total, 60);
        assert_eq!(eager_live, 60);
        assert_eq!(lazy_total, 60);
        assert!(lazy_live > 0 && lazy_live < 20, "{lazy_live} of 60 blocks laid out");
        assert!((eager_h - lazy_h).abs() < 0.01, "the measured height is exact either way ({eager_h} vs {lazy_h})");
    }

    /// #VAPP-258 step 5: a selection across a chip copies its label.
    #[test]
    fn a_copy_across_a_chip_gives_its_label() {
        let text = "see \u{a0}\u{a0}\u{a0} now";
        let chip = "see ".len().."see \u{a0}\u{a0}\u{a0}".len();
        assert_eq!(copy_text(text, 0..text.len(), &[(chip.clone(), "ABC-1".into())]), "see ABC-1 now");
        assert_eq!(copy_text(text, 0..3, &[(chip.clone(), "ABC-1".into())]), "see");
        assert_eq!(copy_text(text, chip.start + 2..text.len(), &[(chip, "ABC-1".into())]), "ABC-1 now");
    }

    /// #VAPP-258: a `ts` fence paints CodeBlock's token kinds in the
    /// CodeBlock/token recipe's colours (the neutral theme's), one font; an
    /// unknown or empty language is one run in the ink.
    #[test]
    fn a_ts_fence_paints_code_block_token_colours() {
        let blocks = parse("```ts\nconst a = 1 // one\n```");
        assert_eq!(blocks[0].lang.as_deref(), Some("ts"));
        let text = &blocks[0].inlines[0].text;
        let theme = eui_core::themes::builtin_theme("neutral").expect("neutral");
        let colour = |kind: &str| crate::paint::parts::code_token_look(Some(&*theme), Mode::Light, &Map::new(), kind).0;
        let ink = gpui::black();
        let font = Font { family: "mono".into(), features: Default::default(), fallbacks: None, weight: FontWeight(400.0), style: FontStyle::Normal };
        let runs = code_block_runs(text, blocks[0].lang.as_deref(), &font, ink, &colour);
        assert_eq!(runs.iter().map(|r| r.len).sum::<usize>(), text.len());
        assert!(runs.iter().all(|r| r.font == font), "colour only: one font");
        let tokens = eui_core::code::tokenize_code(text, "ts");
        let kinds: Vec<&str> = tokens[0].iter().map(|t| t.kind.as_str()).collect();
        for kind in ["keyword", "number", "comment"] {
            assert!(kinds.contains(&kind), "{kind} in {kinds:?}");
            let c = colour(kind).expect("the recipe colours it");
            assert!(runs.iter().any(|r| r.color == c), "{kind} painted in its CodeBlock colour");
        }
        assert!(runs.len() >= 3);
        for lang in [None, Some("klingon"), Some("plain"), Some("ansi")] {
            let plain = code_block_runs(text, lang, &font, ink, &colour);
            assert_eq!(plain.len(), 1);
            assert_eq!(plain[0].color, ink);
        }
    }
}
