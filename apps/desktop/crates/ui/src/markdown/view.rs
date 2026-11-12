//! #VAPP-258: the app's read-only Markdown IS genUI's gpui Markdown
//! (`eui_gpui::paint::markdown`, the catalog `Markdown` leaf): one
//! implementation backs the genUI catalog and the app. [`MarkdownView`]
//! keeps the builder every read surface already used (comments, the chat
//! and the steer feed, results, previews, the blurred description editor)
//! and paints through a one-leaf [`SurfaceView`] per view id (kept across
//! frames, least recently used dropped), on the app's theme.
//!
//! This file only supplies the host hooks (`catalog/host.json` markdown),
//! as Android's `MarkdownView.kt` and iOS's `AppMarkdownHost.swift` do:
//! `#IDENT` refs (the status glyph in its tint), `@email` mentions and the
//! agent's in-app links (prose-links.json) as chips; links through the
//! app-link rule; `/api/attachments/{id}` pictures sized from the synced
//! rows, with the account's bearer, opening THE image preview; video /
//! audio / file links as media blocks; chat = `density: chat` (soft breaks,
//! as the desktop always rendered them), a 240 px picture cap and
//! `streaming` while a reply is written; documents = no picture cap; task
//! boxes toggle through `on_source_edit` (the gpui host extra).

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash as _, Hasher as _};
use std::rc::Rc;

use eui_core::host::policy::TextLinkChip;
use eui_core::host::{MediaOptions, MediaRequest, TextLink, UrlPolicy};
use eui_gpui::host::{HostPlugin, ImageCap, MarkdownAttachment, MarkdownOptions};
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{div, px, svg, App, AppContext as _, Entity, Hsla, IntoElement, ParentElement as _, SharedString, Styled as _, Window};
use serde_json::json;

use super::{ImageCache, RefResolver};
use crate::plan_card::CardSurfaceHost;

pub(crate) type Callback = Rc<dyn Fn(&str, &mut Window, &mut App)>;
type SourceEditCallback = Rc<dyn Fn(String, &mut Window, &mut App)>;

/// A chat's block picture height cap (px); documents have none (Android's
/// `CHAT_IMAGE_MAX_HEIGHT`).
pub(crate) const CHAT_IMAGE_MAX_HEIGHT: f32 = 240.;

/// The Markdown leaf's id inside its surface.
const LEAF_ID: &str = "md";

/// How many surfaces the window keeps (a transcript's visible rows plus
/// their overdraw a few times over; over it the least recently used half
/// goes, and comes back re-parsed when it shows again).
const SURFACE_CAP: usize = 512;

/// The link schemes of an issue chip and a mention chip (host-private: the
/// URL policy allows them, [`AppMarkdownHost::open_url`] routes them).
const ISSUE_SCHEME: &str = "exp-issue";
const MENTION_SCHEME: &str = "exp-mention";

/// A chip icon that is an issue's status: `status|<svg path>|<hex tint>`.
const STATUS_ICON: &str = "status|";

/// Read-only rendered GFM through genUI's Markdown. Construct per render
/// (it is an element); the surface behind it lives across frames by `id`.
#[derive(gpui::IntoElement)]
pub struct MarkdownView {
    id: SharedString,
    source: String,
    resolver: Option<RefResolver>,
    images: Option<Entity<ImageCache>>,
    on_open_issue: Option<Callback>,
    /// EXP-1188: replaces the default link open ([`crate::links::open_agent_link`]).
    on_open_link: Option<Callback>,
    /// #EXP-1493: replaces the default picture open (THE image preview).
    on_open_image: Option<Callback>,
    /// Task boxes toggle; fired with the FULL new markdown.
    on_source_edit: Option<SourceEditCallback>,
    /// EXP-1188: bare `http(s)://` URLs link (display-only).
    autolink: bool,
    /// #EXP-1674: agent prose's bare in-app URLs read as chips in this team.
    chip_team: Option<String>,
    selectable: bool,
    /// `density: chat` (EXP-698 / EXP-787: the transcript rhythm).
    chat: bool,
    /// `streaming: true`: the text is a growing prefix (a reply being written).
    streaming: bool,
}

impl MarkdownView {
    pub fn new(id: impl Into<SharedString>, source: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            source: source.into(),
            resolver: None,
            images: None,
            on_open_issue: None,
            on_open_link: None,
            on_open_image: None,
            on_source_edit: None,
            autolink: false,
            chip_team: None,
            selectable: false,
            chat: false,
            streaming: false,
        }
    }

    /// #EXP-1674 (prose-links.json): agent prose reads its bare in-app URLs as
    /// chips, in `team_id`'s scope. Display only.
    pub fn chip_app_links(mut self, team_id: impl Into<String>) -> Self {
        self.chip_team = Some(team_id.into());
        self
    }

    /// Resolve `@email` / `#IDENT` into chips against the synced collections.
    pub fn resolver(mut self, resolver: RefResolver) -> Self {
        self.resolver = Some(resolver);
        self
    }

    /// The screen's image cache: a pressed picture opens THE image preview
    /// over it (genUI loads the pictures itself).
    pub fn images(mut self, images: Entity<ImageCache>) -> Self {
        self.images = Some(images);
        self
    }

    /// Pressing a resolved `#IDENT` chip.
    pub fn on_open_issue(mut self, on_open_issue: impl Fn(&str, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_issue = Some(Rc::new(on_open_issue));
        self
    }

    /// EXP-1188: a pressed link's href (absolute: `~/x` expanded against the
    /// team, a relative path against the instance) goes here instead of
    /// [`crate::links::open_agent_link`].
    pub fn on_open_link(mut self, on_open_link: impl Fn(&str, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_link = Some(Rc::new(on_open_link));
        self
    }

    /// #EXP-1493: a pressed picture's raw `src` goes here instead of THE
    /// image preview.
    pub fn on_open_image(mut self, on_open_image: impl Fn(&str, &mut Window, &mut App) + 'static) -> Self {
        self.on_open_image = Some(Rc::new(on_open_image));
        self
    }

    /// EXP-1188: link bare `http(s)://` URLs (feeds; documents keep them bare).
    pub fn autolink(mut self, autolink: bool) -> Self {
        self.autolink = autolink;
        self
    }

    /// Press-drag selects within the text, the copy shortcut copies (the
    /// display text: chips give their labels).
    pub fn selectable(mut self, selectable: bool) -> Self {
        self.selectable = selectable;
        self
    }

    /// EXP-698: the chat rhythm (`density: chat`).
    pub fn chat(mut self, chat: bool) -> Self {
        self.chat = chat;
        self
    }

    /// #VAPP-258: the text is a reply still being written.
    pub fn streaming(mut self, streaming: bool) -> Self {
        self.streaming = streaming;
        self
    }

    /// Fired with the FULL new markdown when a task box is pressed. Without
    /// it, task boxes are inert.
    pub fn on_source_edit(mut self, on_source_edit: impl Fn(String, &mut Window, &mut App) + 'static) -> Self {
        self.on_source_edit = Some(Rc::new(on_source_edit));
        self
    }
}

/// Whether a markdown link href may be handed to the OS opener: an absolute
/// `http(s)://` URL ([`api::opener::is_web_url`]) or a `mailto:`. Anything
/// else — `file:///Applications/Calculator.app`, a relative path, a custom
/// scheme in an uploaded `.md` — is a no-op: the opener would run it.
pub(crate) fn is_openable_link(url: &str) -> bool {
    if api::opener::is_web_url(url) {
        return true;
    }
    let trimmed = url.trim();
    trimmed == url
        && trimmed.len() > "mailto:".len()
        && trimmed.get(.."mailto:".len()).is_some_and(|scheme| scheme.eq_ignore_ascii_case("mailto:"))
        && !trimmed.chars().any(char::is_control)
}

/// Open a markdown link href when [`is_openable_link`] allows it; log the
/// refusal otherwise (a blocked href is not an error the person can fix).
pub(crate) fn open_link_href(url: &str) {
    if !is_openable_link(url) {
        log::info!("markdown link ignored (not http(s)/mailto): {url}");
        return;
    }
    if let Err(error) = api::opener::open_in_browser(url) {
        log::warn!("open link failed: {error}");
    }
}

// ---------------------------------------------------------------------------
// The source handed to the painter
// ---------------------------------------------------------------------------

/// An entity chip spliced out of the source (prose-links.json).
#[derive(Clone, Debug, PartialEq)]
struct ProseEntity {
    label: String,
    href: String,
    kind: domain::prose_links::ProseChipKind,
}

const TOKEN_OPEN: char = '\u{E000}';
const TOKEN_CLOSE: char = '\u{E001}';

/// The text handed to the painter: every rewritten in-app link
/// (`[title](url#exp-chip-kind)`) replaced by a private-use token the text
/// links answer as its entity chip (a markdown link's label never asks the
/// host), and with `autolink` every bare `http(s)://` url outside code
/// written as a link. Android's `AppMarkdown.prepare`.
fn prepare(markdown: &str, autolink: bool) -> (String, Vec<ProseEntity>) {
    let chips = markdown.contains("#exp-chip-");
    let lowered = autolink.then(|| markdown.to_ascii_lowercase());
    let urls = lowered.as_deref().is_some_and(|l| l.contains("http://") || l.contains("https://"));
    let mut entities = Vec::new();
    if !chips && !urls {
        return (markdown.to_string(), entities);
    }
    let mut fence: Option<&str> = None;
    let mut out = String::with_capacity(markdown.len() + 16);
    for (index, line) in markdown.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        if let Some(mark) = fence_mark(line) {
            match fence {
                None => fence = Some(mark),
                Some(open) if open == mark => fence = None,
                Some(_) => {}
            }
            out.push_str(line);
        } else if fence.is_some() {
            out.push_str(line);
        } else {
            rewrite_line(line, chips, urls, &mut entities, &mut out);
        }
    }
    (out, entities)
}

fn fence_mark(line: &str) -> Option<&'static str> {
    let rest = line.trim_start_matches(' ');
    if line.len() - rest.len() > 3 {
        return None;
    }
    if rest.starts_with("```") {
        Some("```")
    } else if rest.starts_with("~~~") {
        Some("~~~")
    } else {
        None
    }
}

/// `[label](dest)` (an optional `"title"`) at the start of `s`, not an image:
/// (label, dest, byte length).
fn link_at(s: &str) -> Option<(&str, &str, usize)> {
    let b = s.as_bytes();
    if b.first() != Some(&b'[') {
        return None;
    }
    let mut i = 1;
    while i < b.len() && b[i] != b']' {
        i += if b[i] == b'\\' { 2 } else { 1 };
    }
    if i >= b.len() || b.get(i + 1) != Some(&b'(') {
        return None;
    }
    let label = &s[1..i];
    let start = i + 2;
    let mut j = start;
    while j < b.len() && b[j] != b')' && !b[j].is_ascii_whitespace() {
        j += 1;
    }
    let dest = &s[start..j];
    if dest.is_empty() {
        return None;
    }
    if b.get(j).is_some_and(u8::is_ascii_whitespace) {
        let rest = &s[j..];
        let trimmed = rest.trim_start();
        let tail = trimmed.strip_prefix('"')?;
        let close = tail.find('"')?;
        let after = tail[close + 1..].trim_start();
        if !after.starts_with(')') {
            return None;
        }
        let consumed = s.len() - after.len() + 1;
        return Some((label, dest, consumed));
    }
    (b.get(j) == Some(&b')')).then_some((label, dest, j + 1))
}

fn unescape(label: &str) -> String {
    if !label.contains('\\') {
        return label.to_string();
    }
    let mut out = String::with_capacity(label.len());
    let mut chars = label.chars();
    while let Some(c) = chars.next() {
        match (c, chars.clone().next()) {
            ('\\', Some(next)) => {
                out.push(next);
                chars.next();
            }
            _ => out.push(c),
        }
    }
    out
}

const URL_STOPS: &[u8] = b" \t<>()[]`\"'";
const URL_TRAILING: &[u8] = b".,;:!?*_~";

/// One line outside a fence: its code spans kept, chip links tokenized, bare
/// urls linked.
fn rewrite_line(line: &str, chips: bool, urls: bool, entities: &mut Vec<ProseEntity>, out: &mut String) {
    let b = line.as_bytes();
    let mut at = 0;
    while at < b.len() {
        let ch = b[at];
        if ch == b'`' {
            let run_end = at + b[at..].iter().take_while(|c| **c == b'`').count();
            let ticks = &line[at..run_end];
            let end = line[run_end..].find(ticks).map_or(run_end, |close| run_end + close + ticks.len());
            out.push_str(&line[at..end]);
            at = end;
            continue;
        }
        if ch == b'[' && (at == 0 || b[at - 1] != b'!') {
            if let Some((label, dest, n)) = link_at(&line[at..]) {
                let kind = chips.then(|| domain::prose_links::prose_chip_kind(dest)).flatten();
                match kind {
                    Some(kind) => {
                        out.push(TOKEN_OPEN);
                        out.push_str(&entities.len().to_string());
                        out.push(TOKEN_CLOSE);
                        entities.push(ProseEntity { label: unescape(label), href: dest.to_string(), kind });
                    }
                    None => out.push_str(&line[at..at + n]),
                }
                at += n;
                continue;
            }
        }
        if urls && (ch == b'h' || ch == b'H') {
            let rest = &line[at..];
            let lower = rest.get(..8).map(str::to_ascii_lowercase).unwrap_or_default();
            if lower.starts_with("http://") || lower.starts_with("https://") {
                let glued = at > 0 && (b[at - 1].is_ascii_alphanumeric() || matches!(b[at - 1], b'_' | b'[' | b'<' | b'('));
                if !glued {
                    let mut end = at;
                    while end < b.len() && !URL_STOPS.contains(&b[end]) {
                        end += 1;
                    }
                    while end > at && URL_TRAILING.contains(&b[end - 1]) {
                        end -= 1;
                    }
                    let url = &line[at..end];
                    if url.len() > "https://".len() {
                        out.push('[');
                        out.push_str(&url.replace('[', "\\[").replace(']', "\\]"));
                        out.push_str("](");
                        out.push_str(url);
                        out.push(')');
                        at = end;
                        continue;
                    }
                }
            }
        }
        let next = line[at..].chars().next().map_or(1, char::len_utf8);
        out.push_str(&line[at..at + next]);
        at += next;
    }
}

/// The `ordinal`-th task item of `markdown` (document order, outside fences,
/// quotes included) toggled; `None` = there is no such task.
pub(crate) fn toggle_task(markdown: &str, ordinal: usize) -> Option<String> {
    let mut fence: Option<&str> = None;
    let mut seen = 0usize;
    let mut offset = 0usize;
    for line in markdown.split('\n') {
        let start = offset;
        offset += line.len() + 1;
        if let Some(mark) = fence_mark(line) {
            match fence {
                None => fence = Some(mark),
                Some(open) if open == mark => fence = None,
                Some(_) => {}
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }
        let Some(box_at) = task_box_at(line) else { continue };
        if seen == ordinal {
            let at = start + box_at + 1;
            let flipped = if markdown.as_bytes()[at] == b' ' { "x" } else { " " };
            let mut out = markdown.to_string();
            out.replace_range(at..at + 1, flipped);
            return Some(out);
        }
        seen += 1;
    }
    None
}

/// Where a task line's `[ ]` / `[x]` starts (byte offset of the `[`).
fn task_box_at(line: &str) -> Option<usize> {
    let mut t = line.trim_start();
    while let Some(rest) = t.strip_prefix('>') {
        t = rest.trim_start();
    }
    let rest = ["- ", "* ", "+ "].iter().find_map(|bullet| t.strip_prefix(bullet)).or_else(|| {
        let digits = t.bytes().take_while(u8::is_ascii_digit).count();
        (digits > 0 && digits < 10).then(|| t[digits..].strip_prefix(". ").or_else(|| t[digits..].strip_prefix(") "))).flatten()
    })?;
    let rest_trimmed = rest;
    let is_box = ["[ ] ", "[x] ", "[X] "].iter().any(|b| rest_trimmed.starts_with(b));
    is_box.then(|| line.len() - rest_trimmed.len())
}

// ---------------------------------------------------------------------------
// The host
// ---------------------------------------------------------------------------

/// What one render of a [`MarkdownView`] asks of its host.
#[derive(Default)]
struct HostConfig {
    resolver: Option<RefResolver>,
    entities: Vec<ProseEntity>,
    images: Option<Entity<ImageCache>>,
    on_open_issue: Option<Callback>,
    on_open_link: Option<Callback>,
    on_open_image: Option<Callback>,
    on_source_edit: Option<SourceEditCallback>,
    /// The RAW markdown (task toggles rewrite it).
    source: String,
    chat: bool,
    selectable: bool,
    streaming: bool,
    /// [`hooks_mark`] of this render's text: the layout memo's version.
    mark: u64,
}

/// The app's Markdown host: the card host's media + URL base (the
/// instance's attachments with the account's bearer), the app's chips,
/// links, pictures and options.
pub(crate) struct AppMarkdownHost {
    media: CardSurfaceHost,
    config: RefCell<HostConfig>,
    /// #EDE-65 lane O: a DOCUMENT's host ([`Self::for_document`]) answers
    /// many Markdowns: each text's own [`hooks_mark`] (by [`text_key`]).
    document: Option<RefCell<DocumentMarks>>,
}

/// A document host's per-text answers' fingerprints, and the store revision
/// they were taken at.
#[derive(Default)]
struct DocumentMarks {
    revision: u64,
    resolves: bool,
    marks: HashMap<u64, u64>,
}

impl AppMarkdownHost {
    fn new(team_id: Option<String>, cx: &App) -> Self {
        Self { media: CardSurfaceHost::for_team(team_id, cx), config: RefCell::default(), document: None }
    }

    /// #EDE-65 lane O: ONE host for every Markdown of a document (the Chat
    /// tab's bubbles in the shared chat document): what a [`MarkdownView`]
    /// configures per view, [`Self::prepare_document`] sets for all of them.
    pub(crate) fn for_document(team_id: Option<String>, cx: &App) -> Self {
        Self { document: Some(RefCell::default()), ..Self::new(team_id, cx) }
    }

    /// The bodies as the document binds them, each prepared as a
    /// [`MarkdownView`] with `.chat(true).autolink(true).selectable(true)`
    /// prepares its own (the entity chips numbered across all of them), and
    /// this host set to answer for every one.
    pub(crate) fn prepare_document(&self, bodies: &[String], prose: DocumentProse, cx: &App) -> Vec<String> {
        let mut entities = Vec::new();
        let texts: Vec<String> = bodies
            .iter()
            .map(|body| {
                let source = match prose.chip_team.as_deref() {
                    Some(team_id) => crate::links::chip_prose_links(body, Some(team_id), cx),
                    None => body.clone(),
                };
                let (text, own) = prepare(&source, true);
                let text = renumber_tokens(&text, entities.len());
                entities.extend(own);
                text
            })
            .collect();
        let resolves = prose.resolver.is_some();
        *self.config.borrow_mut() = HostConfig {
            resolver: prose.resolver,
            entities,
            images: prose.images,
            on_open_issue: prose.on_open_issue,
            on_open_link: prose.on_open_link,
            on_open_image: None,
            on_source_edit: None,
            source: String::new(),
            chat: true,
            selectable: true,
            streaming: false,
            mark: 0,
        };
        if let Some(document) = &self.document {
            let revision = store_revision(cx);
            let mut old = std::mem::take(&mut *document.borrow_mut());
            let keep = old.revision == revision && old.resolves == resolves;
            let mut marks = HashMap::with_capacity(texts.len());
            for text in &texts {
                let key = text_key(text);
                let mark = match old.marks.remove(&key).filter(|_| keep) {
                    Some(mark) => mark,
                    None => hooks_mark(text, self, cx) ^ u64::from(resolves),
                };
                marks.insert(key, mark);
            }
            *document.borrow_mut() = DocumentMarks { revision, resolves, marks };
        }
        texts
    }
}

/// #EDE-65 lane O: what a document's Markdowns read as prose (the
/// [`MarkdownView`] builder's, once for all of them).
#[derive(Default)]
pub(crate) struct DocumentProse {
    pub resolver: Option<RefResolver>,
    pub images: Option<Entity<ImageCache>>,
    pub on_open_issue: Option<Callback>,
    pub on_open_link: Option<Callback>,
    /// The team whose bare in-app URLs read as chips.
    pub chip_team: Option<String>,
}

/// A text's key in a document host's marks.
fn text_key(text: &str) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// `text`'s entity tokens numbered from `base` (a document's bodies share
/// one entity list).
fn renumber_tokens(text: &str, base: usize) -> String {
    if base == 0 || !text.contains(TOKEN_OPEN) {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + 8);
    let mut rest = text;
    while let Some(open) = rest.find(TOKEN_OPEN) {
        let body = open + TOKEN_OPEN.len_utf8();
        let Some(close) = rest[body..].find(TOKEN_CLOSE).map(|i| body + i) else { break };
        out.push_str(&rest[..body]);
        match rest[body..close].parse::<usize>() {
            Ok(n) => out.push_str(&(n + base).to_string()),
            Err(_) => out.push_str(&rest[body..close]),
        }
        out.push(TOKEN_CLOSE);
        rest = &rest[close + TOKEN_CLOSE.len_utf8()..];
    }
    out.push_str(rest);
    out
}

/// The chip of a resolved `#IDENT`: the token as written + the title, the
/// status glyph in its tint.
fn issue_chip(token: &str, chip: &super::IssueChipData) -> TextLinkChip {
    let title = super::truncate_chip_title(&chip.title);
    // The mono identifier, then the title (web's chip; no title = the token).
    // Web's rule: the ident is the bare identifier (`EXP-398`, no `#`).
    let identifier = token.trim_start_matches('#');
    let (label, ident) = if title.is_empty() { (identifier.to_string(), None) } else { (title, Some(identifier.to_string())) };
    let tint = gpui::Rgba::from(chip.icon_color);
    let hex = format!("#{:02x}{:02x}{:02x}", (tint.r * 255.).round() as u8, (tint.g * 255.).round() as u8, (tint.b * 255.).round() as u8);
    TextLinkChip { kind: "issue".into(), label, icon: Some(format!("{STATUS_ICON}{}|{hex}", chip.icon_path)), ident }
}

/// entity-chip.json `entityRefIcon` of a prose chip's kind.
fn prose_chip_concept(kind: domain::prose_links::ProseChipKind) -> &'static str {
    use domain::prose_links::ProseChipKind;
    match kind {
        ProseChipKind::Issue => "ui-issue",
        ProseChipKind::Board => "nav-boards",
        ProseChipKind::Session => "coding-running",
        ProseChipKind::Agent => "settings-agents",
    }
}

/// `catalog/host.json` `textLinks` with chips over one text piece: the
/// entity tokens, the `#IDENT` refs that resolve and the known `@email`
/// mentions; `None` = nothing to link.
fn segments(piece: &str, config: &HostConfig, cx: &App) -> Option<Vec<TextLink>> {
    let mut hits: Vec<(usize, usize, TextLink)> = Vec::new();
    if !config.entities.is_empty() && piece.contains(TOKEN_OPEN) {
        let mut at = 0;
        while let Some(open) = piece[at..].find(TOKEN_OPEN).map(|i| at + i) {
            let body = open + TOKEN_OPEN.len_utf8();
            let Some(close) = piece[body..].find(TOKEN_CLOSE).map(|i| body + i) else { break };
            let end = close + TOKEN_CLOSE.len_utf8();
            if let Some(entity) = piece[body..close].parse::<usize>().ok().and_then(|n| config.entities.get(n)) {
                let chip = TextLinkChip { kind: "entity".into(), label: entity.label.clone(), icon: Some(prose_chip_concept(entity.kind).into()), ident: None };
                hits.push((open, end, TextLink { text: piece[open..end].to_string(), href: Some(entity.href.clone()), chip: Some(chip) }));
            }
            at = end;
        }
    }
    if let Some(resolver) = &config.resolver {
        if piece.contains('#') || resolver.bare {
            for range in super::scan_issue_refs_with(piece, resolver.bare) {
                let token = &piece[range.clone()];
                let identifier = token.trim_start_matches('#');
                let Some(chip) = (resolver.issue_chip)(identifier, cx) else { continue };
                hits.push((range.start, range.end, TextLink { text: token.to_string(), href: Some(format!("{ISSUE_SCHEME}:{identifier}")), chip: Some(issue_chip(token, &chip)) }));
            }
        }
        if piece.contains('@') {
            for range in super::scan_mentions(piece) {
                let token = &piece[range.clone()];
                let email = &token[1..];
                let Some(name) = (resolver.member_name)(email, cx) else { continue };
                let label = if name.trim().is_empty() { email.to_string() } else { name.trim().to_string() };
                hits.push((range.start, range.end, TextLink { text: token.to_string(), href: Some(format!("{MENTION_SCHEME}:{email}")), chip: Some(TextLinkChip { kind: "mention".into(), label, icon: None, ident: None }) }));
            }
        }
    }
    if hits.is_empty() {
        return None;
    }
    hits.sort_by_key(|(start, _, _)| *start);
    let mut out = Vec::with_capacity(hits.len() * 2 + 1);
    let mut at = 0;
    for (start, end, link) in hits {
        if start < at {
            continue;
        }
        if start > at {
            out.push(TextLink { text: piece[at..start].to_string(), href: None, chip: None });
        }
        out.push(link);
        at = end;
    }
    if at < piece.len() {
        out.push(TextLink { text: piece[at..].to_string(), href: None, chip: None });
    }
    Some(out)
}

impl HostPlugin for AppMarkdownHost {
    fn icon(&self, name: &str) -> Option<SharedString> {
        crate::exponential_ui_host::icon_path(name)
    }

    /// An issue chip's status glyph in its own tint.
    fn chip_icon(&self, name: &str, size: f32, _color: Hsla) -> Option<gpui::AnyElement> {
        let rest = name.strip_prefix(STATUS_ICON)?;
        let (path, hex) = rest.rsplit_once('|')?;
        let tint = eui_gpui::paint::color::parse_hex(hex)?;
        Some(svg().path(SharedString::from(path.to_string())).size(px(size)).flex_none().text_color(tint).into_any_element())
    }

    /// The card host's policy (the instance as the base, `~/x` against the
    /// team), plus `mailto`, the chips' own schemes and any private host (a
    /// `localhost` link an agent prints is the reader's to open, as before).
    fn url_policy(&self) -> Option<UrlPolicy> {
        let mut policy = self.media.url_policy().unwrap_or_default();
        policy.schemes = Some(["https", "http", "mailto", ISSUE_SCHEME, MENTION_SCHEME].map(str::to_string).to_vec());
        policy.allow_private = Some(vec!["*".into()]);
        Some(policy)
    }

    fn media_options(&self) -> MediaOptions {
        self.media.media_options()
    }

    fn media_request(&self, src: &str) -> Option<MediaRequest> {
        self.media.media_request(src)
    }

    fn open_url(&self, url: &str, cx: &mut App) {
        let (on_open_issue, on_open_link) = {
            let config = self.config.borrow();
            (config.on_open_issue.clone(), config.on_open_link.clone())
        };
        let url = url.to_string();
        if let Some(identifier) = url.strip_prefix(&format!("{ISSUE_SCHEME}:")) {
            let Some(on_open_issue) = on_open_issue else { return };
            let identifier = identifier.to_string();
            crate::navigation::on_active_window(cx, move |window, cx| on_open_issue(&identifier, window, cx));
            return;
        }
        if url.starts_with(&format!("{MENTION_SCHEME}:")) {
            return;
        }
        crate::navigation::on_active_window(cx, move |window, cx| match on_open_link {
            Some(open) => open(&url, window, cx),
            None => crate::links::open_agent_link(&url, window, cx),
        });
    }

    fn text_links(&self, text: &str, cx: &App) -> Option<Vec<TextLink>> {
        segments(text, &self.config.borrow(), cx)
    }

    /// The synced row's probed pixels, scaled to the `?w=` display width
    /// when the src carries one (EXP-52; web `attachmentImageSize`).
    fn image_size(&self, src: &str, cx: &App) -> Option<(f32, f32)> {
        scaled_to_width_param(src, crate::links::markdown_image_size(src, cx))
    }

    fn image_pressable(&self, _cx: &App) -> bool {
        let config = self.config.borrow();
        config.on_open_image.is_some() || config.images.is_some()
    }

    fn on_image_press(&self, src: &str, window: &mut Window, cx: &mut App) {
        let (on_open_image, images) = {
            let config = self.config.borrow();
            (config.on_open_image.clone(), config.images.clone())
        };
        if let Some(open) = on_open_image {
            return open(src, window, cx);
        }
        let url = super::image_url::strip_query(src).to_string();
        let size = super::attachment_natural_size(&url, cx);
        crate::image_preview::open_image_preview(url, String::new(), size, images, window, cx);
    }

    fn attachment(&self, href: &str, cx: &App) -> Option<MarkdownAttachment> {
        crate::links::markdown_attachment(href, cx)
    }

    fn markdown_options(&self, _cx: &App) -> MarkdownOptions {
        let config = self.config.borrow();
        MarkdownOptions {
            hard_breaks: false,
            chat: config.chat,
            selectable: Some(config.selectable),
            streaming: config.streaming,
            image_max_height: if config.chat { ImageCap::Height(CHAT_IMAGE_MAX_HEIGHT) } else { ImageCap::None },
        }
    }

    /// The answers' fingerprint this render took ([`hooks_mark`]).
    fn markdown_hooks_version(&self, text: &str, _cx: &App) -> Option<u64> {
        match &self.document {
            // A text the document did not prepare is asked again every paint.
            Some(document) => document.borrow().marks.get(&text_key(text)).copied(),
            None => Some(self.config.borrow().mark),
        }
    }

    /// EXP-521: a selectable text joins the window's TextSelection layer
    /// (the Root mounts it): sweeps and copy run across messages, comments
    /// and every other selectable view, in reading order.
    fn markdown_window_selection(&self, _cx: &App) -> bool {
        self.config.borrow().selectable
    }

    /// The surface's whole box scrolls in the app's own scroller.
    fn markdown_visible_only(&self, _cx: &App) -> bool {
        true
    }

    fn tasks_pressable(&self, _cx: &App) -> bool {
        self.config.borrow().on_source_edit.is_some()
    }

    fn on_task_toggle(&self, ordinal: usize, _checked: bool, window: &mut Window, cx: &mut App) {
        let (on_source_edit, source) = {
            let config = self.config.borrow();
            (config.on_source_edit.clone(), config.source.clone())
        };
        if let (Some(edit), Some(next)) = (on_source_edit, toggle_task(&source, ordinal)) {
            edit(next, window, cx);
        }
    }

    /// Sandbox, as the card host: no picker.
    fn pick_files(&self, _request: &eui_gpui::host::FilePickRequest, _cx: &mut App) -> bool {
        true
    }
}

/// `size` scaled to `src`'s `?w=` width (the aspect kept); unchanged
/// without one.
fn scaled_to_width_param(src: &str, size: Option<(f32, f32)>) -> Option<(f32, f32)> {
    let (w, h) = size.filter(|(w, h)| *w > 0. && *h > 0.)?;
    Some(match super::image_url::width_param_from_src(src) {
        Some(display) => (display, h * display / w),
        None => (w, h),
    })
}

// ---------------------------------------------------------------------------
// The surfaces
// ---------------------------------------------------------------------------

struct Slot {
    view: Entity<SurfaceView>,
    host: Rc<AppMarkdownHost>,
    team: Option<String>,
    /// The text the leaf holds.
    text: String,
    /// The host answers it was measured with ([`hooks_mark`]), and the
    /// store revision it was taken at ([`store_revision`]).
    mark: u64,
    revision: u64,
    used: u64,
}

/// The revisions of the collections the host's answers read (issues, their
/// statuses and boards, members, attachments, sessions, agents): the same
/// revision = the same answers for the same text. `0` without a store.
pub(crate) fn store_revision(cx: &App) -> u64 {
    let Some(store) = sync::Store::try_global(cx) else { return 0 };
    let c = store.collections();
    let mut h = std::hash::DefaultHasher::new();
    (
        c.issues.read(cx).revision(),
        c.issue_statuses.read(cx).revision(),
        c.boards.read(cx).revision(),
        c.users.read(cx).revision(),
        c.team_members.read(cx).revision(),
        c.attachments.read(cx).revision(),
        c.sessions.read(cx).revision(),
    )
        .hash(&mut h);
    h.finish()
}

#[derive(Default)]
struct Surfaces {
    tick: u64,
    slots: HashMap<(gpui::WindowId, SharedString), Slot>,
}

impl gpui::Global for Surfaces {}

/// A fingerprint of what the host answers `text` (chips' labels and icons,
/// attachment sizes and kinds): the measured layout holds while it does, a
/// change (an issue synced, renamed or moved; a row's size arrived) measures
/// the leaf again.
fn hooks_mark(text: &str, host: &AppMarkdownHost, cx: &App) -> u64 {
    let mut hasher = std::hash::DefaultHasher::new();
    let config = host.config.borrow();
    if config.resolver.is_some() {
        for line in text.split('\n') {
            if let Some(links) = segments(line, &config, cx) {
                for link in links.iter().filter_map(|l| l.chip.as_ref()) {
                    link.label.hash(&mut hasher);
                    link.icon.hash(&mut hasher);
                }
            }
        }
    }
    if text.contains("/api/attachments/") {
        let mut rest = text;
        while let Some(at) = rest.find("](") {
            rest = &rest[at + 2..];
            let end = rest.find([')', ' ']).unwrap_or(rest.len());
            let src = &rest[..end];
            if let Some((w, h)) = crate::links::markdown_image_size(src, cx) {
                (w.to_bits(), h.to_bits()).hash(&mut hasher);
            }
            if let Some(a) = crate::links::markdown_attachment(src, cx) {
                (a.kind, a.name).hash(&mut hasher);
            }
        }
    }
    hasher.finish()
}

fn leaf(text: &str) -> eui_core::NestedNode {
    serde_json::from_value(json!({"id": LEAF_ID, "component": "Markdown", "children": [], "props": {"text": text}})).expect("the Markdown leaf parses")
}

fn new_surface(id: &SharedString, host: Rc<AppMarkdownHost>, text: &str, window: &mut Window, cx: &mut App) -> Entity<SurfaceView> {
    let options = SurfaceViewOptions {
        surface_id: format!("md-{id}"),
        theme: eui_core::themes::builtin_theme(crate::exponential_ui_host::THEME_ID),
        mode: eui_core::theme::Mode::Dark,
        host,
        width: None,
        settings: theme::motion::reduce_motion().then(|| eui_core::surface::SurfaceSettings {
            mode: eui_core::theme::ModeSetting::Dark,
            reduced_motion: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    let tree = leaf(text);
    let view = cx.new(|cx| SurfaceView::new(options, window, cx));
    view.update(cx, |view, cx| {
        view.set_nested(tree, cx);
        // Only this element changes it: an unchanged frame skips its pass.
        view.set_measure_memo(true, cx);
    });
    view
}

impl gpui::RenderOnce for MarkdownView {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let mut source = self.source;
        if let Some(team_id) = self.chip_team.as_deref() {
            source = crate::links::chip_prose_links(&source, Some(team_id), cx);
        }
        let (text, entities) = prepare(&source, self.autolink);
        let key = (window.window_handle().window_id(), self.id.clone());
        let team = self.chip_team.clone();
        let surfaces = cx.default_global::<Surfaces>();
        surfaces.tick += 1;
        let tick = surfaces.tick;
        let existing = surfaces.slots.get_mut(&key).filter(|slot| slot.team == team).map(|slot| {
            slot.used = tick;
            (slot.view.clone(), slot.host.clone(), slot.text.clone(), slot.mark, slot.revision)
        });
        let revision = store_revision(cx);
        let resolves = self.resolver.is_some();
        let config = HostConfig {
            resolver: self.resolver,
            entities,
            images: self.images,
            on_open_issue: self.on_open_issue,
            on_open_link: self.on_open_link,
            on_open_image: self.on_open_image,
            on_source_edit: self.on_source_edit,
            source: source.clone(),
            chat: self.chat,
            selectable: self.selectable,
            streaming: self.streaming,
            mark: 0,
        };
        let options = |c: &HostConfig| (c.chat, c.selectable, c.streaming);
        let (view, host, held, old_mark, old_revision, fresh) = match existing {
            Some((view, host, held, mark, old_revision)) => {
                let changed_options = options(&host.config.borrow()) != options(&config);
                *host.config.borrow_mut() = config;
                let held = if changed_options { String::new() } else { held };
                (view, host, held, mark, old_revision, false)
            }
            None => {
                let host = Rc::new(AppMarkdownHost::new(team.clone(), cx));
                *host.config.borrow_mut() = config;
                let view = new_surface(&self.id, host.clone(), &text, window, cx);
                (view, host, text.clone(), 0, u64::MAX, true)
            }
        };
        if !fresh {
            if held != text {
                let tree = leaf(&text);
                view.update(cx, |view, cx| {
                    view.set_nested(tree, cx);
                });
            }
        }
        // The answers only move with the text or the store.
        // The surface lays out against the WINDOW's height, never the part of
        // it the scroller shows (a moving visible height re-laid every row
        // out on every scroll frame).
        let viewport = f32::from(window.viewport_size().height);
        view.update(cx, |view, cx| view.set_viewport_height(viewport, cx));
        let mark = if !fresh && held == text && old_revision == revision { old_mark } else { hooks_mark(&text, &host, cx) ^ u64::from(resolves) };
        host.config.borrow_mut().mark = mark;
        if !fresh && mark != old_mark && held == text {
            view.update(cx, |view, cx| {
                if let Some(index) = view.index_of(LEAF_ID) {
                    view.surface_mut().mark_dirty(index);
                }
                cx.notify();
            });
        }
        let surfaces = cx.global_mut::<Surfaces>();
        surfaces.slots.insert(key, Slot { view: view.clone(), host, team, text, mark, revision, used: tick });
        if surfaces.slots.len() > SURFACE_CAP {
            let mut ranked: Vec<(u64, (gpui::WindowId, SharedString))> = surfaces.slots.iter().map(|(key, slot)| (slot.used, key.clone())).collect();
            ranked.sort_unstable_by_key(|(used, _)| *used);
            for (_, stale) in ranked.into_iter().take(SURFACE_CAP / 2) {
                surfaces.slots.remove(&stale);
            }
        }
        div().w_full().min_w_0().child(view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #EDE-65 lane O: a document's bodies share one entity list: a later
    /// body's chip tokens count on from the bodies before it.
    #[test]
    fn a_documents_entity_tokens_number_across_its_bodies() {
        let token = |n: usize| format!("{TOKEN_OPEN}{n}{TOKEN_CLOSE}");
        let text = format!("see {} and {} done", token(0), token(1));
        assert_eq!(renumber_tokens(&text, 3), format!("see {} and {} done", token(3), token(4)));
        assert_eq!(renumber_tokens(&text, 0), text);
        let open = format!("half {TOKEN_OPEN}2 open");
        assert_eq!(renumber_tokens(&open, 5), open, "an unclosed token stays");
    }

    /// A markdown href reaches the OS opener only as http(s) or mailto.
    #[test]
    fn only_web_and_mailto_hrefs_are_openable() {
        assert!(is_openable_link("https://example.com/a?b=c"));
        assert!(is_openable_link("http://localhost:3000/t/acme"));
        assert!(is_openable_link("mailto:dev@acme.test"));
        assert!(is_openable_link("MAILTO:dev@acme.test"));
        assert!(!is_openable_link("mailto:"));
        assert!(!is_openable_link("file:///Applications/Calculator.app"));
        assert!(!is_openable_link("/etc/passwd"));
        assert!(!is_openable_link("../secrets.txt"));
        assert!(!is_openable_link("javascript:alert(1)"));
        assert!(!is_openable_link("x-apple.systempreferences:com.apple.preference"));
        assert!(!is_openable_link(" https://example.com"));
        assert!(!is_openable_link("mailto:a@b.c\n"));
        assert!(!is_openable_link(""));
    }

    #[test]
    fn prepare_links_bare_urls_outside_code_and_fences() {
        let (text, entities) = prepare("see https://x.example/a. and `https://y.example` too\n```\nhttps://z.example\n```", true);
        assert!(entities.is_empty());
        assert_eq!(text, "see [https://x.example/a](https://x.example/a). and `https://y.example` too\n```\nhttps://z.example\n```");
        let (kept, _) = prepare("[a](https://x.example) https://x.example", false);
        assert_eq!(kept, "[a](https://x.example) https://x.example");
    }

    #[test]
    fn prepare_tokenizes_prose_chip_links() {
        let (text, entities) = prepare("on [Mobile](https://app.example/t/acme/boards/mobile#exp-chip-board) now", false);
        assert_eq!(text, "on \u{E000}0\u{E001} now");
        assert_eq!(entities, vec![ProseEntity { label: "Mobile".into(), href: "https://app.example/t/acme/boards/mobile#exp-chip-board".into(), kind: domain::prose_links::ProseChipKind::Board }]);
    }

    #[test]
    fn toggle_task_flips_the_nth_task_outside_fences() {
        let md = "- [ ] one\n```\n- [ ] code\n```\n> - [x] quoted\n1. [ ] three";
        assert_eq!(toggle_task(md, 0).as_deref(), Some("- [x] one\n```\n- [ ] code\n```\n> - [x] quoted\n1. [ ] three"));
        assert_eq!(toggle_task(md, 1).as_deref(), Some("- [ ] one\n```\n- [ ] code\n```\n> - [ ] quoted\n1. [ ] three"));
        assert_eq!(toggle_task(md, 2).as_deref(), Some("- [ ] one\n```\n- [ ] code\n```\n> - [x] quoted\n1. [x] three"));
        assert_eq!(toggle_task(md, 3), None);
    }

    /// EXP-52: `?w=` is the display width, the aspect stays the row's.
    #[test]
    fn the_width_param_scales_the_synced_size() {
        assert_eq!(scaled_to_width_param("/api/attachments/a?w=400", Some((1600., 1200.))), Some((400., 300.)));
        assert_eq!(scaled_to_width_param("/api/attachments/a", Some((1600., 1200.))), Some((1600., 1200.)));
        assert_eq!(scaled_to_width_param("/api/attachments/a?w=400", None), None);
    }

    #[test]
    fn issue_chip_carries_the_status_glyph_and_tint() {
        let chip = issue_chip("#EXP-1", &crate::markdown::IssueChipData { issue_id: "i".into(), title: "Fix it".into(), icon_path: "icons/x.svg".into(), icon_color: gpui::rgb(0xff0000).into() });
        assert_eq!((chip.label.as_str(), chip.ident.as_deref()), ("Fix it", Some("EXP-1")));
        assert_eq!(chip.icon.as_deref(), Some("status|icons/x.svg|#ff0000"));
        assert!(chip.is_valid());
    }

    /// The app's embedded assets as the app serves them (`app/src/assets.rs`:
    /// `apps/desktop/assets`, then gpui-component's), read from disk.
    #[cfg(target_os = "macos")]
    pub(crate) struct DiskAssets;

    #[cfg(target_os = "macos")]
    impl gpui::AssetSource for DiskAssets {
        fn load(&self, path: &str) -> gpui::Result<Option<std::borrow::Cow<'static, [u8]>>> {
            let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets").join(path);
            match std::fs::read(file) {
                Ok(bytes) => Ok(Some(bytes.into())),
                Err(_) => gpui_component_assets::Assets.load(path),
            }
        }

        fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
            gpui_component_assets::Assets.list(path)
        }
    }

    /// The status glyph of an issue chip resolves (the host paints the
    /// registry's SVG in the status tint) and reaches the pixels: a resolved
    /// `#EXP-42` paints its tint, an unresolved text paints none of it.
    #[cfg(target_os = "macos")]
    #[test]
    fn an_issue_chip_paints_its_status_glyph_in_its_tint() {
        use gpui::{size, HeadlessAppContext, Render};
        use std::sync::Arc;
        const TINT: u32 = 0x22c55e;
        let glyph = crate::icons::glyph_svg_path(domain::options::IconGlyph::Circle);
        assert!(gpui::AssetSource::load(&DiskAssets, &glyph).ok().flatten().is_some(), "the glyph's asset {glyph} ships");

        struct Doc(&'static str);
        impl Render for Doc {
            fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
                let _span = tracing::trace_span!("render", view = "Doc").entered();
                let resolver = RefResolver {
                    member_name: Rc::new(|_, _| None),
                    issue_chip: Rc::new(|id, _| {
                        id.eq_ignore_ascii_case("EXP-42").then(|| crate::markdown::IssueChipData { issue_id: "i".into(), title: "Fix login".into(), icon_path: crate::icons::glyph_svg_path(domain::options::IconGlyph::Circle), icon_color: gpui::rgb(TINT).into() })
                    }),
                    bare: false,
                };
                div().size_full().bg(gpui::black()).p(px(8.)).child(MarkdownView::new(format!("glyph-{}", self.0), self.0).resolver(resolver))
            }
        }
        let shot = |text: &'static str| {
                        let mut cx = HeadlessAppContext::with_platform(crate::test_text_system(), Arc::new(DiskAssets), gpui_platform::current_headless_renderer);
            cx.update(|cx| {
                gpui_component::init(cx);
                theme::init(cx);
                let chip = issue_chip("#EXP-42", &crate::markdown::IssueChipData { issue_id: "i".into(), title: "Fix login".into(), icon_path: crate::icons::glyph_svg_path(domain::options::IconGlyph::Circle), icon_color: gpui::rgb(TINT).into() });
                let host = AppMarkdownHost::new(None, cx);
                assert!(host.chip_icon(chip.icon.as_deref().expect("a status icon"), 12., gpui::black()).is_some(), "the status icon is the host's");
            });
            let window = cx.open_window(size(px(480.), px(80.)), move |_, cx| cx.new(|_| Doc(text))).expect("a headless window");
            for _ in 0..3 {
                cx.update_window(window.into(), |_, window, cx| {
                    window.refresh();
                    window.draw(cx).clear(cx);
                })
                .expect("draws");
                cx.run_until_parked();
            }
            let image = cx.capture_screenshot(window.into()).expect("a picture");
            std::mem::forget(cx);
            image
        };
        let tinted = |image: &image::RgbaImage| {
            let (r, g, b) = ((TINT >> 16) as i32, ((TINT >> 8) & 0xff) as i32, (TINT & 0xff) as i32);
            image.pixels().filter(|p| (p[0] as i32 - r).abs() < 40 && (p[1] as i32 - g).abs() < 40 && (p[2] as i32 - b).abs() < 40).count()
        };
        let with = shot("see #EXP-42 now");
        let without = shot("see #EXP-41 now");
        assert!(tinted(&with) > 8, "the status glyph paints in its tint ({} px)", tinted(&with));
        assert_eq!(tinted(&without), 0, "no chip, no tint");
    }


    /// EXP-521 (#VAPP-258 kept): a sweep selects across two views (two
    /// messages) through the window's TextSelection layer, in reading order.
    /// The layer is mounted directly (a test window has no platform `Root`).
    #[gpui::test]
    async fn a_sweep_selects_across_views(cx: &mut gpui::TestAppContext) {
        use gpui::{Modifiers, MouseButton, Render, VisualTestContext};
        use gpui_base::TextSelectionLayer;

        struct Host;
        impl Render for Host {
            fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
                let _span = tracing::trace_span!("render", view = "Host").entered();
                div()
                    .size_full()
                    .child(TextSelectionLayer)
                    .child(MarkdownView::new("sweep-view-a", "alpha first comment body").selectable(true))
                    .child(MarkdownView::new("sweep-view-b", "beta second comment body").selectable(true))
            }
        }

        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let (_host, cx) = cx.add_window_view(|_, _| Host);
        let cx: &mut VisualTestContext = cx;
        for _ in 0..3 {
            cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.run_until_parked();
        }
        cx.simulate_mouse_down(gpui::point(px(1.), px(8.)), MouseButton::Left, Modifiers::default());
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        cx.simulate_mouse_move(gpui::point(px(400.), px(70.)), Some(MouseButton::Left), Modifiers::default());
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        cx.simulate_mouse_up(gpui::point(px(400.), px(70.)), MouseButton::Left, Modifiers::default());
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        let selected = cx.update(|window, cx| gpui_base::TextSelection::selected_text(window, cx));
        assert!(selected.contains("alpha first comment body"), "the first message: {selected:?}");
        assert!(selected.contains("beta second comment body"), "the second message: {selected:?}");
        assert!(selected.find("alpha").unwrap() < selected.find("beta").unwrap(), "reading order: {selected:?}");
    }

    /// Danny's bug: a sweep from the prose above a code block, across every
    /// one of its lines, into the prose below copies all of it with the
    /// block's line feeds kept (the highlight: `range_boxes`' test).
    #[gpui::test]
    async fn a_sweep_selects_every_line_of_a_code_block(cx: &mut gpui::TestAppContext) {
        use gpui::{Modifiers, MouseButton, Render, VisualTestContext};
        use gpui_base::TextSelectionLayer;

        const SOURCE: &str = "Live commands:\n\n```sh\ncd ~/repo\n\n# build\nscripts/mkboot.sh build\nsudo umount /mnt\n```\n\nThe last block needs you.";
        struct Host;
        impl Render for Host {
            fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
                div().size_full().child(TextSelectionLayer).child(MarkdownView::new("sweep-code", SOURCE).selectable(true))
            }
        }

        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let (_host, cx) = cx.add_window_view(|_, _| Host);
        let cx: &mut VisualTestContext = cx;
        for _ in 0..3 {
            cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.run_until_parked();
        }
        let sweep = |cx: &mut VisualTestContext, from: gpui::Point<gpui::Pixels>, to: gpui::Point<gpui::Pixels>| {
            cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
            cx.update(|window, cx| {
                let _ = window.draw(cx);
            });
            cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
            cx.update(|window, cx| {
                let _ = window.draw(cx);
            });
            cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
            cx.update(|window, cx| {
                let _ = window.draw(cx);
            });
            cx.update(|window, cx| gpui_base::TextSelection::selected_text(window, cx))
        };
        // The whole document, prose to prose.
        let all = sweep(cx, gpui::point(px(1.), px(4.)), gpui::point(px(700.), px(600.)));
        assert!(all.contains("Live commands:"), "the prose above: {all:?}");
        assert!(all.contains("cd ~/repo\n\n# build\nscripts/mkboot.sh build\nsudo umount /mnt"), "every code line, line feeds kept: {all:?}");
        assert!(all.contains("The last block needs you."), "the prose below: {all:?}");
        assert!(all.find("Live").unwrap() < all.find("cd ~").unwrap() && all.find("umount").unwrap() < all.find("The last").unwrap(), "reading order: {all:?}");
    }

}
