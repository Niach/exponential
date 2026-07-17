//! The markdown editor + rendered view (masterplan-v3 §4.5).
//!
//! [`MarkdownEditor`] is the editable surface screens B/C consume: a
//! **block-based** model in the iOS `IssueEditorModel` sense — the block list
//! is the single source of truth while editing (text blocks are
//! gpui-component `InputState`s holding canonical per-block GFM source,
//! image blocks are structural), and full-document markdown is derived only
//! on demand via [`MarkdownEditor::markdown`] (`serialize(parse(..))`, the
//! byte-parity pair). This is the spec's sanctioned v1 shape ("source is the
//! markdown — correct-by-construction round-trip") organized around the block
//! model so images are first-class: paste/drop/picker insert real image
//! blocks with staging + upload, never ad-hoc text.
//!
//! [`MarkdownView`] is the read-only rendered view (issue description
//! preview, comments): headings/lists/quotes/code, toggleable task
//! checkboxes, images fetched through the auth-gated attachment transport,
//! and live `@email` / `#IDENT` **pills** resolved against the synced
//! collections ([`RefResolver`]) — re-resolved on every render, so a pill
//! that could not resolve yet lights up once its issue syncs (§4.5).

use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    canvas, deferred, div, img, point, px, App, AppContext as _, Bounds, ClipboardEntry, Context,
    ElementId, Entity, Focusable as _, FontStyle, FontWeight, HighlightStyle,
    InteractiveElement as _,
    InteractiveText, IntoElement, ParentElement as _, Pixels, Render, SharedString,
    StatefulInteractiveElement as _, StrikethroughStyle, Styled as _, StyledImage as _, StyledText,
    Subscription, TextRun, UnderlineStyle, Window,
};
use gpui_component::input::{self, Input, InputEvent, InputState, Position};
use gpui_component::text::TextView;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex, v_flex, ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _,
};

use super::autocomplete::{detect_trigger, CompletionItem, CompletionSource};
use super::blocks::{BlockKind, ContentBlock, InlineKind, InlineMark, ListType, ParagraphAttrs};
use super::image_paste::{
    new_draft_url, pasted_image_parts, read_image_file, validate_image, AttachmentTransport,
    StagedImage,
};
use super::parse::markdown_to_blocks;
use super::serialize::blocks_to_markdown;
use super::toolbar::{self, LinePrefix};

// ---------------------------------------------------------------------------
// RefResolver — live @email / #IDENT resolution (§4.5 pills)
// ---------------------------------------------------------------------------

/// `email` → member display name (None ⇒ not a known member; stays text).
pub type MemberNameResolver = Rc<dyn Fn(&str, &App) -> Option<String>>;
/// `IDENTIFIER` → does the issue exist in this workspace?
pub type IssueExistsResolver = Rc<dyn Fn(&str, &App) -> bool>;

/// Resolves decoration tokens against the synced collections at render time.
#[derive(Clone)]
pub struct RefResolver {
    /// `email` → member display name (None ⇒ not a known member; stays text).
    pub member_name: MemberNameResolver,
    /// `IDENTIFIER` → does the issue exist in this workspace?
    pub issue_exists: IssueExistsResolver,
}

impl RefResolver {
    /// Resolve against the §05 collections of the given workspace. Reads the
    /// live store on every call — the §4.5 "decoration pass re-runs when the
    /// issues store changes" rule falls out of gpui re-render + this.
    pub fn from_store(workspace_id: impl Into<String>) -> Self {
        let ws_members = workspace_id.into();
        let ws_issues = ws_members.clone();
        Self {
            member_name: Rc::new(move |email, cx| {
                let collections = sync::Store::global(cx).collections();
                let members = collections.workspace_members.read(cx);
                let users = collections.users.read(cx);
                let needle = email.to_lowercase();
                members
                    .iter()
                    .filter(|m| m.workspace_id == ws_members)
                    .filter_map(|m| users.get(&m.user_id))
                    .find(|u| {
                        u.email
                            .as_deref()
                            .is_some_and(|e| e.to_lowercase() == needle)
                    })
                    .map(|u| u.name.clone().unwrap_or_else(|| email.to_string()))
            }),
            issue_exists: Rc::new(move |identifier, cx| {
                let collections = sync::Store::global(cx).collections();
                collections
                    .issues_in_workspace(&ws_issues, cx)
                    .iter()
                    .any(|issue| issue.identifier.eq_ignore_ascii_case(identifier))
            }),
        }
    }

    /// Never resolves anything (tokens stay literal text).
    pub fn disabled() -> Self {
        Self {
            member_name: Rc::new(|_, _| None),
            issue_exists: Rc::new(|_, _| false),
        }
    }
}

// ---------------------------------------------------------------------------
// ImageCache — async, auth-gated attachment bytes (shared editor + view)
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub(crate) enum ImageSlot {
    Loading,
    Ready(Arc<gpui::Image>),
    /// A fetch failure is NEVER permanent: the timestamp gates a re-fetch on
    /// the next render after [`RETRY_AFTER`] — a transient network hiccup
    /// (stale keep-alive socket, brief offline) must not brick every image
    /// for the rest of the session.
    Failed(std::time::Instant),
}

/// How long a failed fetch is displayed before the next render retries it.
const RETRY_AFTER: std::time::Duration = std::time::Duration::from_secs(5);

/// Caches decoded attachment images per URL. `/api/attachments/{id}` is
/// auth-gated, so bytes go through the [`AttachmentTransport`] (bearer
/// header) on the background executor — never a bare `img(uri)`.
pub struct ImageCache {
    transport: Option<Arc<dyn AttachmentTransport>>,
    slots: HashMap<String, ImageSlot>,
}

impl ImageCache {
    pub fn new(transport: Option<Arc<dyn AttachmentTransport>>) -> Self {
        Self {
            transport,
            slots: HashMap::new(),
        }
    }

    pub fn set_transport(&mut self, transport: Option<Arc<dyn AttachmentTransport>>) {
        self.transport = transport;
    }

    /// Register locally-staged bytes (a `draft://` image) for rendering.
    pub fn insert_bytes(&mut self, url: String, content_type: &str, bytes: Vec<u8>) {
        let format = sniff_format(content_type, &bytes);
        let image = Arc::new(gpui::Image::from_bytes(format, bytes));
        self.slots.insert(url, ImageSlot::Ready(image));
    }

    /// Re-key staged bytes once the upload resolved `draft://x` → real URL.
    pub fn alias(&mut self, from: &str, to: String) {
        if let Some(slot) = self.slots.get(from).cloned() {
            self.slots.insert(to, slot);
        }
    }

    pub(crate) fn slot(&mut self, url: &str, cx: &mut Context<Self>) -> ImageSlot {
        if let Some(slot) = self.slots.get(url) {
            // Ready/Loading are terminal-ish; a failure older than the
            // backoff falls through and re-fetches.
            match slot {
                ImageSlot::Failed(at) if at.elapsed() >= RETRY_AFTER => {}
                _ => return slot.clone(),
            }
        }
        if url.starts_with(super::image_paste::DRAFT_SCHEME) {
            // Draft bytes are inserted eagerly; a miss means they're gone.
            return ImageSlot::Failed(std::time::Instant::now());
        }
        let Some(transport) = self.transport.clone() else {
            return ImageSlot::Failed(std::time::Instant::now());
        };
        self.slots.insert(url.to_string(), ImageSlot::Loading);
        let url_owned = url.to_string();
        cx.spawn(async move |this, cx| {
            let fetch_url = url_owned.clone();
            let result = cx
                .background_executor()
                .spawn(async move { transport.fetch(&fetch_url) })
                .await;
            this.update(cx, |cache, cx| {
                let slot = match result {
                    Ok(bytes) => {
                        let format = sniff_format("", &bytes);
                        ImageSlot::Ready(Arc::new(gpui::Image::from_bytes(format, bytes)))
                    }
                    Err(error) => {
                        log::warn!("attachment fetch failed for {url_owned}: {error}");
                        ImageSlot::Failed(std::time::Instant::now())
                    }
                };
                cache.slots.insert(url_owned.clone(), slot);
                cx.notify();
            })
            .ok();
        })
        .detach();
        ImageSlot::Loading
    }
}

/// Magic-byte sniff with a mime fallback (attachment bytes may be any of the
/// accepted types; gpui decodes per declared format).
fn sniff_format(content_type: &str, bytes: &[u8]) -> gpui::ImageFormat {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        gpui::ImageFormat::Png
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        gpui::ImageFormat::Jpeg
    } else if bytes.starts_with(b"GIF8") {
        gpui::ImageFormat::Gif
    } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        gpui::ImageFormat::Webp
    } else if bytes.starts_with(b"BM") {
        gpui::ImageFormat::Bmp
    } else {
        gpui::ImageFormat::from_mime_type(content_type).unwrap_or(gpui::ImageFormat::Png)
    }
}

fn render_image_slot(
    id: impl Into<ElementId>,
    images: Option<&Entity<ImageCache>>,
    url: &str,
    alt: &str,
    cx: &mut App,
) -> gpui::AnyElement {
    let slot = images
        .map(|images| images.update(cx, |cache, cx| cache.slot(url, cx)))
        .unwrap_or_else(|| ImageSlot::Failed(std::time::Instant::now()));
    match slot {
        ImageSlot::Ready(image) => {
            let rendered = img(image)
                .max_w_full()
                .h(px(260.))
                .object_fit(gpui::ObjectFit::ScaleDown)
                .rounded(px(4.));
            // Click-to-open (EXP-33): an in-app lightbox over the shared
            // [`ImageCache`] — never the web browser. The preview itself
            // carries the "Open in browser" affordance.
            match images {
                Some(images) => {
                    let images = images.clone();
                    let url = url.to_string();
                    let alt = alt.to_string();
                    div()
                        .id(id.into())
                        .cursor_pointer()
                        .on_click(move |_, window, cx| {
                            // In the blurred-editor preview a bubbling click
                            // would also start editing behind the lightbox.
                            cx.stop_propagation();
                            crate::image_preview::open_image_preview(
                                url.clone(),
                                alt.clone(),
                                Some(images.clone()),
                                window,
                                cx,
                            );
                        })
                        .child(rendered)
                        .into_any_element()
                }
                None => rendered.into_any_element(),
            }
        }
        ImageSlot::Loading => placeholder_box("Loading image…", cx),
        ImageSlot::Failed(_) => placeholder_box(
            &if alt.is_empty() {
                "Image unavailable".to_string()
            } else {
                format!("Image unavailable — {alt}")
            },
            cx,
        ),
    }
}

pub(crate) fn placeholder_box(label: &str, cx: &App) -> gpui::AnyElement {
    div()
        .w_full()
        .h(px(80.))
        .rounded(px(4.))
        .bg(cx.theme().muted)
        .flex()
        .items_center()
        .justify_center()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(SharedString::from(label.to_string()))
        .into_any_element()
}

// ---------------------------------------------------------------------------
// MarkdownEditor
// ---------------------------------------------------------------------------

enum EditorBlock {
    Text {
        id: u64,
        input: Entity<InputState>,
        bounds: Rc<std::cell::Cell<Bounds<Pixels>>>,
        _sub: Subscription,
    },
    Image {
        id: u64,
        url: String,
        alt: String,
    },
}

impl EditorBlock {
    fn id(&self) -> u64 {
        match self {
            Self::Text { id, .. } | Self::Image { id, .. } => *id,
        }
    }
}

struct ActiveCompletion {
    block_id: u64,
    token: super::autocomplete::PendingToken,
    items: Vec<CompletionItem>,
    selected: usize,
}

struct LinkEditor {
    url: Entity<InputState>,
    text: Entity<InputState>,
}

type ChangeCallback = Rc<dyn Fn(&str, &mut Window, &mut App)>;
type BlurCallback = Rc<dyn Fn(&mut Window, &mut App)>;
type OpenIssueCallback = Rc<dyn Fn(&str, &mut Window, &mut App)>;

/// The editable markdown surface — see the module docs for the seam.
pub struct MarkdownEditor {
    blocks: Vec<EditorBlock>,
    focused_block: Option<u64>,
    placeholder: SharedString,
    on_change: Option<ChangeCallback>,
    on_blur: Option<BlurCallback>,
    /// Fires with the canonical markdown after a STRUCTURAL edit (image
    /// insert/remove) that must persist immediately without waiting for a
    /// blur — screens wire this to the same save path as `on_blur`.
    on_commit: Option<ChangeCallback>,
    completion_source: Option<Rc<dyn CompletionSource>>,
    completion: Option<ActiveCompletion>,
    /// Shared image cache (create with [`MarkdownEditor::images`] to reuse in
    /// a sibling [`MarkdownView`]).
    images: Entity<ImageCache>,
    transport: Option<Arc<dyn AttachmentTransport>>,
    /// When set, pasted/picked images upload immediately to this issue
    /// (detail editor). When `None` they stay staged as `draft://` blocks
    /// (create dialog) — resolve at submit via
    /// [`super::image_paste::upload_staged_images`].
    upload_issue_id: Option<String>,
    staged: Vec<StagedImage>,
    uploads_in_flight: usize,
    link_editor: Option<LinkEditor>,
    error: Option<SharedString>,
    /// Blurred-preview seam (EXP-161): while no text block owns focus the
    /// editor renders a [`MarkdownView`] of its own document — live
    /// `@email`/`#IDENT` pills, clickable links, rendered GFM — and swaps
    /// back to the editable blocks on click. Opt-in (detail description);
    /// the create dialog keeps the always-editable surface.
    preview_when_blurred: bool,
    resolver: Option<RefResolver>,
    on_open_issue: Option<OpenIssueCallback>,
}

impl MarkdownEditor {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let images = cx.new(|_| ImageCache::new(None));
        let mut this = Self {
            blocks: Vec::new(),
            focused_block: None,
            placeholder: "Add description...".into(),
            on_change: None,
            on_blur: None,
            on_commit: None,
            completion_source: None,
            completion: None,
            images,
            transport: None,
            upload_issue_id: None,
            staged: Vec::new(),
            uploads_in_flight: 0,
            link_editor: None,
            error: None,
            preview_when_blurred: false,
            resolver: None,
            on_open_issue: None,
        };
        this.rebuild_from_markdown("", window, cx);
        this
    }

    // -- Configuration seam (screens B/C) -----------------------------------

    pub fn set_placeholder(&mut self, placeholder: impl Into<SharedString>) {
        self.placeholder = placeholder.into();
    }

    /// The auth-gated HTTP seam for image upload + attachment rendering.
    pub fn set_transport(&mut self, transport: Arc<dyn AttachmentTransport>, cx: &mut App) {
        self.images
            .update(cx, |cache, _| cache.set_transport(Some(transport.clone())));
        self.transport = Some(transport);
    }

    /// `Some(issue_id)` ⇒ immediate upload on paste (detail editor); `None`
    /// ⇒ pre-save staging (create dialog).
    pub fn set_upload_issue(&mut self, issue_id: Option<String>) {
        self.upload_issue_id = issue_id;
    }

    pub fn set_completion_source(&mut self, source: Rc<dyn CompletionSource>) {
        self.completion_source = Some(source);
    }

    /// Fires with the **canonical** markdown after every edit.
    pub fn set_on_change(&mut self, on_change: impl Fn(&str, &mut Window, &mut App) + 'static) {
        self.on_change = Some(Rc::new(on_change));
    }

    /// Fires when an inner input blurs (screens debounce/save-on-blur).
    pub fn set_on_blur(&mut self, on_blur: impl Fn(&mut Window, &mut App) + 'static) {
        self.on_blur = Some(Rc::new(on_blur));
    }

    /// Fires with the canonical markdown after a STRUCTURAL edit (image
    /// insert/remove) that must persist immediately — screens wire this to the
    /// same save path as blur.
    pub fn set_on_commit(&mut self, on_commit: impl Fn(&str, &mut Window, &mut App) + 'static) {
        self.on_commit = Some(Rc::new(on_commit));
    }

    /// The shared image cache (pass to a sibling [`MarkdownView`]).
    pub fn images(&self) -> Entity<ImageCache> {
        self.images.clone()
    }

    /// Render the read-only [`MarkdownView`] (decoration pills, clickable
    /// links) while no text block owns focus; clicking it re-enters editing
    /// (EXP-161).
    pub fn set_preview_when_blurred(&mut self, preview: bool) {
        self.preview_when_blurred = preview;
    }

    /// Resolve `@email`/`#IDENT` into pills in the blurred preview (§4.5).
    pub fn set_resolver(&mut self, resolver: RefResolver) {
        self.resolver = Some(resolver);
    }

    /// Clicking a resolved `#IDENT` pill in the blurred preview.
    pub fn set_on_open_issue(
        &mut self,
        on_open_issue: impl Fn(&str, &mut Window, &mut App) + 'static,
    ) {
        self.on_open_issue = Some(Rc::new(on_open_issue));
    }

    // -- Document I/O --------------------------------------------------------

    /// Load markdown into the block model (replaces the document).
    pub fn set_markdown(&mut self, markdown: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.rebuild_from_markdown(markdown, window, cx);
        cx.notify();
    }

    /// Derive the canonical full-document markdown from the blocks
    /// (`serialize(parse(join))` — the byte-parity pair).
    pub fn markdown(&self, cx: &App) -> String {
        let mut parts: Vec<String> = Vec::new();
        for block in &self.blocks {
            match block {
                EditorBlock::Text { input, .. } => {
                    let value = input.read(cx).value().to_string();
                    if !value.trim().is_empty() {
                        parts.push(value);
                    }
                }
                EditorBlock::Image { url, alt, .. } => parts.push(format!("![{alt}]({url})")),
            }
        }
        // Editor-input canonicalization (EXP-118): a plain Enter's lone `\n`
        // in a text block means a paragraph break, not a GFM soft break.
        super::canonicalize_editor_input(&parts.join("\n\n"))
    }

    /// Images staged (pasted pre-save) and still referenced by the document.
    pub fn staged_images(&self, cx: &App) -> Vec<StagedImage> {
        let markdown = self.markdown(cx);
        self.staged
            .iter()
            .filter(|staged| markdown.contains(&staged.draft_url))
            .cloned()
            .collect()
    }

    pub fn is_uploading(&self) -> bool {
        self.uploads_in_flight > 0
    }

    /// Whether any text block currently owns keyboard focus (the user is
    /// mid-edit).
    pub fn is_focused(&self, window: &Window, cx: &App) -> bool {
        self.blocks.iter().any(|block| match block {
            EditorBlock::Text { input, .. } => input.read(cx).focus_handle(cx).is_focused(window),
            _ => false,
        })
    }

    /// Focus the last text block (append position).
    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        let input = self.blocks.iter().rev().find_map(|block| match block {
            EditorBlock::Text { input, .. } => Some(input.clone()),
            _ => None,
        });
        if let Some(input) = input {
            input.update(cx, |state, cx| state.focus(window, cx));
            // The blurred preview swaps to the edit surface off this focus —
            // the input's Focus event can't fire while it is unmounted.
            cx.notify();
        }
    }

    // -- Internals -----------------------------------------------------------

    fn rebuild_from_markdown(
        &mut self,
        markdown: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let blocks = markdown_to_blocks(markdown);
        self.blocks = blocks
            .iter()
            .enumerate()
            .map(|(index, block)| match block {
                ContentBlock::Text { .. } => {
                    let fragment = blocks_to_markdown(std::slice::from_ref(block));
                    let placeholder = (index == 0).then(|| self.placeholder.clone());
                    self.new_text_block_with_placeholder(&fragment, placeholder, window, cx)
                }
                ContentBlock::Image { url, alt, .. } => EditorBlock::Image {
                    id: super::blocks::next_block_id(),
                    url: url.clone(),
                    alt: alt.clone(),
                },
            })
            .collect();
        self.focused_block = None;
        self.completion = None;
    }

    fn new_text_block(
        &self,
        source: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> EditorBlock {
        self.new_text_block_with_placeholder(source, None, window, cx)
    }

    fn new_text_block_with_placeholder(
        &self,
        source: &str,
        placeholder: Option<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> EditorBlock {
        let id = super::blocks::next_block_id();
        let source = source.to_string();
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .auto_grow(1, 200)
                .default_value(source);
            if let Some(placeholder) = placeholder {
                state = state.placeholder(placeholder);
            }
            state
        });
        let sub = cx.subscribe_in(&input, window, move |this, input, event, window, cx| {
            this.on_input_event(id, input.clone(), event, window, cx);
        });
        EditorBlock::Text {
            id,
            input,
            bounds: Rc::new(std::cell::Cell::new(Bounds::default())),
            _sub: sub,
        }
    }

    fn text_input(&self, block_id: u64) -> Option<Entity<InputState>> {
        self.blocks.iter().find_map(|block| match block {
            EditorBlock::Text { id, input, .. } if *id == block_id => Some(input.clone()),
            _ => None,
        })
    }

    /// The block that toolbar/paste operations target.
    fn target_block(&self) -> Option<u64> {
        self.focused_block
            .filter(|id| self.text_input(*id).is_some())
            .or_else(|| {
                self.blocks.iter().rev().find_map(|block| match block {
                    EditorBlock::Text { id, .. } => Some(*id),
                    _ => None,
                })
            })
    }

    fn on_input_event(
        &mut self,
        block_id: u64,
        input: Entity<InputState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                self.refresh_completion(block_id, &input, cx);
                self.emit_change(window, cx);
                cx.notify();
            }
            InputEvent::Focus => {
                self.focused_block = Some(block_id);
                cx.notify();
            }
            InputEvent::Blur => {
                if self
                    .completion
                    .as_ref()
                    .is_some_and(|c| c.block_id == block_id)
                {
                    self.completion = None;
                }
                if let Some(on_blur) = self.on_blur.clone() {
                    on_blur(window, cx);
                }
                cx.notify();
            }
            InputEvent::PressEnter { .. } => {}
        }
    }

    fn emit_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(on_change) = self.on_change.clone() {
            let markdown = self.markdown(cx);
            on_change(&markdown, window, cx);
        }
    }

    /// Structural-edit commit: persist the document immediately (image
    /// insert/remove) without waiting for a blur. Runs the mirror-updating
    /// `on_change` first, then the save-triggering `on_commit`.
    fn emit_commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.emit_change(window, cx);
        if let Some(on_commit) = self.on_commit.clone() {
            let markdown = self.markdown(cx);
            on_commit(&markdown, window, cx);
        }
    }

    // -- Autocomplete (§4.6) --------------------------------------------------

    fn refresh_completion(
        &mut self,
        block_id: u64,
        input: &Entity<InputState>,
        cx: &mut Context<Self>,
    ) {
        let Some(source) = self.completion_source.clone() else {
            self.completion = None;
            return;
        };
        let (value, cursor) = {
            let state = input.read(cx);
            (state.value().to_string(), state.cursor())
        };
        let Some(token) = detect_trigger(&value, cursor) else {
            self.completion = None;
            return;
        };
        let items = source.query(token.trigger, &token.query, cx);
        if items.is_empty() {
            self.completion = None;
            return;
        }
        self.completion = Some(ActiveCompletion {
            block_id,
            token,
            items,
            selected: 0,
        });
    }

    fn accept_completion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(completion) = self.completion.take() else {
            return;
        };
        let Some(item) = completion.items.get(completion.selected).cloned() else {
            return;
        };
        let Some(input) = self.text_input(completion.block_id) else {
            return;
        };
        input.update(cx, |state, cx| {
            let value = state.value().to_string();
            let start = completion.token.start;
            let end = (start + 1 + completion.token.query.len()).min(value.len());
            if start > value.len() {
                return;
            }
            let new_value = format!("{}{} {}", &value[..start], item.insert, &value[end..]);
            let caret = start + item.insert.len() + 1;
            let position = byte_offset_to_position(&new_value, caret);
            state.set_value(new_value, window, cx);
            state.set_cursor_position(position, window, cx);
        });
        self.emit_change(window, cx);
        cx.notify();
    }

    fn move_completion(&mut self, delta: isize, cx: &mut Context<Self>) {
        if let Some(completion) = self.completion.as_mut() {
            let len = completion.items.len() as isize;
            if len > 0 {
                let next = (completion.selected as isize + delta).rem_euclid(len);
                completion.selected = next as usize;
                cx.notify();
            }
        }
    }

    // -- Keyboard capture (runs BEFORE the InputState's own handlers) ---------

    fn on_move_up(&mut self, _: &input::MoveUp, _: &mut Window, cx: &mut Context<Self>) {
        if self.completion.is_some() {
            self.move_completion(-1, cx);
            cx.stop_propagation();
        }
    }

    fn on_move_down(&mut self, _: &input::MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        if self.completion.is_some() {
            self.move_completion(1, cx);
            cx.stop_propagation();
        }
    }

    fn on_escape(&mut self, _: &input::Escape, _: &mut Window, cx: &mut Context<Self>) {
        if self.completion.is_some() {
            self.completion = None;
            cx.stop_propagation();
            cx.notify();
        }
    }

    fn on_enter(&mut self, action: &input::Enter, window: &mut Window, cx: &mut Context<Self>) {
        if self.completion.is_some() && !action.shift {
            self.accept_completion(window, cx);
            cx.stop_propagation();
        }
    }

    fn on_tab(&mut self, _: &input::IndentInline, window: &mut Window, cx: &mut Context<Self>) {
        if self.completion.is_some() {
            self.accept_completion(window, cx);
            cx.stop_propagation();
        }
    }

    // -- Clipboard image paste -------------------------------------

    fn on_paste(&mut self, _: &input::Paste, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        // Image entry → ours. Text-only → let the InputState paste run.
        let mut handled = false;
        for entry in item.entries() {
            match entry {
                ClipboardEntry::Image(image) => {
                    let (mime, filename) = pasted_image_parts(image.format());
                    self.insert_image_bytes(
                        filename,
                        mime.to_string(),
                        image.bytes().to_vec(),
                        window,
                        cx,
                    );
                    handled = true;
                }
                ClipboardEntry::ExternalPaths(paths) => {
                    for path in paths.paths() {
                        if let Ok((filename, mime, bytes)) = read_image_file(path) {
                            self.insert_image_bytes(filename, mime, bytes, window, cx);
                            handled = true;
                        }
                    }
                }
                ClipboardEntry::String(_) => {}
            }
        }
        if handled {
            cx.stop_propagation();
        }
    }

    /// The single image entry point (paste + drop + picker all land here).
    fn insert_image_bytes(
        &mut self,
        filename: String,
        content_type: String,
        bytes: Vec<u8>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(reason) = validate_image(&content_type, bytes.len()) {
            self.error = Some(reason.into());
            cx.notify();
            return;
        }
        self.error = None;
        let draft_url = new_draft_url();
        self.images.update(cx, |cache, _| {
            cache.insert_bytes(draft_url.clone(), &content_type, bytes.clone())
        });
        let staged = StagedImage {
            draft_url: draft_url.clone(),
            filename,
            content_type,
            bytes: Arc::new(bytes),
        };
        self.staged.push(staged.clone());
        self.insert_image_block(&draft_url, window, cx);

        // Detail-editor mode: upload immediately and swap the draft URL for
        // the canonical relative form (web paste parity).
        if let (Some(issue_id), Some(transport)) =
            (self.upload_issue_id.clone(), self.transport.clone())
        {
            self.uploads_in_flight += 1;
            let draft = draft_url.clone();
            cx.spawn_in(window, async move |this, cx| {
                let upload_staged = staged;
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        transport.upload(
                            &issue_id,
                            &upload_staged.filename,
                            &upload_staged.content_type,
                            &upload_staged.bytes,
                        )
                    })
                    .await;
                this.update_in(cx, |this, window, cx| {
                    this.uploads_in_flight = this.uploads_in_flight.saturating_sub(1);
                    match result {
                        Ok(uploaded) => {
                            this.images
                                .update(cx, |cache, _| cache.alias(&draft, uploaded.url.clone()));
                            for block in &mut this.blocks {
                                if let EditorBlock::Image { url, .. } = block {
                                    if *url == draft {
                                        *url = uploaded.url.clone();
                                    }
                                }
                            }
                            this.staged.retain(|s| s.draft_url != draft);
                            // Structural edit — the draft URL is now the
                            // canonical relative URL; persist immediately so
                            // the inserted image survives without a blur.
                            this.emit_commit(window, cx);
                        }
                        Err(error) => {
                            log::warn!("image upload failed: {error}");
                            this.error = Some("Image upload failed".into());
                            this.remove_image_by_url(&draft, window, cx);
                        }
                    }
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        self.emit_change(window, cx);
        cx.notify();
    }

    /// Split the focused text block at the caret and put the image between
    /// the halves (iOS block-split semantics).
    fn insert_image_block(&mut self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        let image_block = EditorBlock::Image {
            id: super::blocks::next_block_id(),
            url: url.to_string(),
            alt: "image".to_string(),
        };
        let Some(target) = self.target_block() else {
            self.blocks.push(image_block);
            let trailing = self.new_text_block("", window, cx);
            self.blocks.push(trailing);
            return;
        };
        let index = self
            .blocks
            .iter()
            .position(|block| block.id() == target)
            .unwrap_or(self.blocks.len().saturating_sub(1));
        let Some(input) = self.text_input(target) else {
            return;
        };
        let (before, after) = {
            let state = input.read(cx);
            let value = state.value().to_string();
            let mut cursor = state.cursor().min(value.len());
            while cursor > 0 && !value.is_char_boundary(cursor) {
                cursor -= 1;
            }
            (
                value[..cursor].trim_end_matches('\n').to_string(),
                value[cursor..].trim_start_matches('\n').to_string(),
            )
        };
        input.update(cx, |state, cx| state.set_value(before, window, cx));
        let tail = self.new_text_block(&after, window, cx);
        let tail_input = match &tail {
            EditorBlock::Text { input, .. } => Some(input.clone()),
            _ => None,
        };
        self.blocks.insert(index + 1, image_block);
        self.blocks.insert(index + 2, tail);
        if let Some(tail_input) = tail_input {
            tail_input.update(cx, |state, cx| {
                state.set_cursor_position(Position::new(0, 0), window, cx)
            });
        }
    }

    fn remove_image_by_url(&mut self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.blocks.iter().position(
            |block| matches!(block, EditorBlock::Image { url: u, .. } if u == url),
        ) else {
            return;
        };
        self.remove_image_at(index, window, cx);
    }

    fn remove_image_at(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.blocks.len() || !matches!(self.blocks[index], EditorBlock::Image { .. }) {
            return;
        }
        if let EditorBlock::Image { url, .. } = &self.blocks[index] {
            let url = url.clone();
            self.staged.retain(|s| s.draft_url != url);
        }
        self.blocks.remove(index);
        // Merge now-adjacent text blocks so the document keeps the block
        // invariants (no double text blocks with a phantom separator).
        if index > 0 && index < self.blocks.len() {
            let (left, right) = (self.blocks[index - 1].id(), self.blocks[index].id());
            if let (Some(left_input), Some(right_input)) =
                (self.text_input(left), self.text_input(right))
            {
                let left_value = left_input.read(cx).value().to_string();
                let right_value = right_input.read(cx).value().to_string();
                let merged = match (left_value.trim().is_empty(), right_value.trim().is_empty()) {
                    (true, _) => right_value,
                    (_, true) => left_value,
                    (false, false) => format!("{left_value}\n\n{right_value}"),
                };
                left_input.update(cx, |state, cx| state.set_value(merged, window, cx));
                self.blocks.remove(index);
            }
        }
        // Structural edit — persist immediately (image removal must survive
        // without a subsequent blur; masterplan §8.2).
        self.emit_commit(window, cx);
        cx.notify();
    }

    // -- Toolbar ops (called from toolbar.rs) ----------------------------------

    fn with_target_input(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        transform: impl Fn(&str, Range<usize>) -> (String, usize),
    ) {
        let Some(target) = self.target_block() else {
            return;
        };
        let Some(input) = self.text_input(target) else {
            return;
        };
        input.update(cx, |state, cx| {
            let value = state.value().to_string();
            let range = state.selected_range();
            let (new_value, caret) = transform(&value, range);
            let position = byte_offset_to_position(&new_value, caret.min(new_value.len()));
            state.set_value(new_value, window, cx);
            state.set_cursor_position(position, window, cx);
        });
        self.emit_change(window, cx);
        cx.notify();
    }

    pub(super) fn apply_inline_wrap(
        &mut self,
        delim: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_target_input(window, cx, move |value, range| {
            toolbar::toggle_wrap(value, range, delim)
        });
    }

    pub(super) fn apply_line_prefix(
        &mut self,
        prefix: LinePrefix,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.with_target_input(window, cx, move |value, range| {
            toolbar::toggle_line_prefix(value, range, prefix)
        });
    }

    pub(super) fn clear_formatting(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.with_target_input(window, cx, |value, range| {
            toolbar::strip_inline_formatting(value, range)
        });
    }

    pub(super) fn open_link_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let url = cx.new(|cx| InputState::new(window, cx).placeholder("https://…"));
        let text = cx.new(|cx| InputState::new(window, cx).placeholder("Link text"));
        url.update(cx, |state, cx| state.focus(window, cx));
        self.link_editor = Some(LinkEditor { url, text });
        cx.notify();
    }

    pub(super) fn close_link_editor(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.link_editor = None;
        cx.notify();
    }

    pub(super) fn link_editor_inputs(&self) -> Option<(Entity<InputState>, Entity<InputState>)> {
        self.link_editor
            .as_ref()
            .map(|editor| (editor.url.clone(), editor.text.clone()))
    }

    pub(super) fn apply_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(link_editor) = self.link_editor.take() else {
            return;
        };
        let url = link_editor.url.read(cx).value().trim().to_string();
        let text = link_editor.text.read(cx).value().trim().to_string();
        if url.is_empty() {
            cx.notify();
            return;
        }
        self.with_target_input(window, cx, move |value, range| {
            let (start, end) = (range.start.min(value.len()), range.end.min(value.len()));
            let selection = &value[start..end];
            let label = if !text.is_empty() {
                text.clone()
            } else if !selection.is_empty() {
                selection.to_string()
            } else {
                url.clone()
            };
            let markdown_link = format!("[{label}]({url})");
            let new_value = format!("{}{markdown_link}{}", &value[..start], &value[end..]);
            (new_value, start + markdown_link.len())
        });
    }

    /// Toolbar image button — native file picker through the same path as
    /// paste (§4.5: one upload path for paste + drop + picker).
    pub(super) fn pick_image(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Insert".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                for path in paths {
                    match read_image_file(&path) {
                        Ok((filename, mime, bytes)) => {
                            this.insert_image_bytes(filename, mime, bytes, window, cx);
                        }
                        Err(error) => {
                            this.error = Some(SharedString::from(error.to_string()));
                            cx.notify();
                        }
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    // -- Rendering -------------------------------------------------------------

    fn render_completion(&self, window: &Window, cx: &Context<Self>) -> Option<gpui::AnyElement> {
        let completion = self.completion.as_ref()?;
        let (input, bounds) = self.blocks.iter().find_map(|block| match block {
            EditorBlock::Text {
                id, input, bounds, ..
            } if *id == completion.block_id => Some((input.clone(), bounds.clone())),
            _ => None,
        })?;

        // Caret-anchored position: block origin + estimated caret x (shape
        // the current line up to the caret with the surrounding text style)
        // + (row + 1) rows down.
        let state = input.read(cx);
        let value = state.value().to_string();
        let cursor = state.cursor().min(value.len());
        let position = state.cursor_position();
        let line_height = state.line_height().unwrap_or(px(20.));
        let scroll = state.scroll_offset();
        let line_start = value[..cursor].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let line_text = value[line_start..cursor].to_string();

        let text_style = window.text_style();
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let caret_x = if line_text.is_empty() {
            px(0.)
        } else {
            let run = TextRun {
                len: line_text.len(),
                font: text_style.font(),
                color: gpui::black(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            window
                .text_system()
                .shape_line(SharedString::from(line_text), font_size, &[run], None)
                .width
        };

        let origin = bounds.get().origin;
        let anchor = point(
            origin.x + caret_x + px(8.),
            origin.y + scroll.y + line_height * (position.line as f32 + 1.) + px(8.),
        );

        let theme = cx.theme();
        let items = completion.items.clone();
        let selected = completion.selected;
        let menu = v_flex()
            .id("md-completion")
            .occlude()
            .min_w(px(260.))
            .max_w(px(380.))
            .p_1()
            .gap_0p5()
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .border_1()
            .border_color(theme.border)
            .rounded(px(6.))
            .shadow_md()
            .children(items.iter().enumerate().map(|(index, item)| {
                let is_selected = index == selected;
                h_flex()
                    .id(ElementId::from(("md-completion-item", index)))
                    .w_full()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded(px(4.))
                    .when(is_selected, |el| el.bg(theme.accent))
                    .hover(|el| el.bg(theme.accent))
                    .cursor_pointer()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            if let Some(completion) = this.completion.as_mut() {
                                completion.selected = index;
                            }
                            this.accept_completion(window, cx);
                        }),
                    )
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(item.label.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .truncate()
                            .child(item.detail.clone()),
                    )
                    .into_any_element()
            }));

        Some(
            deferred(
                gpui::anchored()
                    .position(anchor)
                    .snap_to_window_with_margin(px(8.))
                    .child(menu),
            )
            .with_priority(200)
            .into_any_element(),
        )
    }

    fn render_edit_blocks(&mut self, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let images = self.images.clone();
        let mut elements = Vec::with_capacity(self.blocks.len());
        for (index, block) in self.blocks.iter().enumerate() {
            match block {
                EditorBlock::Text { input, bounds, .. } => {
                    let bounds = bounds.clone();
                    elements.push(
                        div()
                            .relative()
                            .w_full()
                            .child(
                                Input::new(input)
                                    .appearance(false)
                                    .w_full(),
                            )
                            .child(
                                canvas(
                                    move |element_bounds, _, _| bounds.set(element_bounds),
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            )
                            .into_any_element(),
                    );
                }
                EditorBlock::Image { id, url, alt, .. } => {
                    let block_id = *id;
                    let image = render_image_slot(
                        ElementId::from(("md-image-open", index)),
                        Some(&images),
                        url,
                        alt,
                        cx,
                    );
                    elements.push(
                        div()
                            .relative()
                            .w_full()
                            .child(image)
                            .child(
                                div().absolute().top_1().right_1().child(
                                    Button::new(ElementId::from(("md-image-remove", index)))
                                        .ghost()
                                        .xsmall()
                                        .icon(Icon::new(IconName::Close))
                                        .tooltip("Remove image")
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            // Never also fire the image's
                                            // open-preview click beneath.
                                            cx.stop_propagation();
                                            let Some(index) = this
                                                .blocks
                                                .iter()
                                                .position(|b| b.id() == block_id)
                                            else {
                                                return;
                                            };
                                            this.remove_image_at(index, window, cx);
                                        })),
                                ),
                            )
                            .into_any_element(),
                    );
                }
            }
        }
        elements
    }
}

/// Read-only rendered surface of a blurred [`MarkdownEditor`] (EXP-161):
/// the same [`MarkdownView`] pass comments use — decoration pills, clickable
/// links, task toggles — over the editor's own document. Clicking anywhere
/// else in it focuses the editor (append position) and swaps the editable
/// blocks back in.
fn render_editor_preview(
    editor: &MarkdownEditor,
    markdown: String,
    cx: &mut Context<MarkdownEditor>,
) -> gpui::AnyElement {
    let mut view = MarkdownView::new("md-editor-preview-view", markdown)
        .images(editor.images.clone());
    if let Some(resolver) = editor.resolver.clone() {
        view = view.resolver(resolver);
    }
    if let Some(on_open_issue) = editor.on_open_issue.clone() {
        view = view.on_open_issue(move |identifier, window, cx| {
            cx.stop_propagation();
            on_open_issue(identifier, window, cx);
        });
    }
    let entity = cx.entity();
    view = view.on_source_edit(move |markdown, window, cx| {
        // A task-checkbox toggle is a structural edit: persist immediately
        // (there is no blur to ride on) and stay in preview.
        cx.stop_propagation();
        entity.update(cx, |this, cx| {
            this.set_markdown(&markdown, window, cx);
            this.emit_commit(window, cx);
        });
    });
    div()
        .id("md-editor-preview")
        .w_full()
        .py_1()
        .min_h(px(96.))
        .cursor_text()
        .on_click(cx.listener(|this, _, window, cx| {
            this.focus(window, cx);
        }))
        .child(view)
        .into_any_element()
}

impl Render for MarkdownEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.preview_when_blurred
            && self.link_editor.is_none()
            && self.error.is_none()
            && !self.is_focused(window, cx)
        {
            let markdown = self.markdown(cx);
            if !markdown.trim().is_empty() {
                return render_editor_preview(self, markdown, cx);
            }
        }

        let theme_border = cx.theme().border;
        let error = self.error.clone();
        let completion = self.render_completion(window, cx);

        v_flex()
            .key_context("MarkdownEditor")
            .w_full()
            .border_1()
            .border_color(theme_border)
            .rounded(px(6.))
            .capture_action(cx.listener(Self::on_move_up))
            .capture_action(cx.listener(Self::on_move_down))
            .capture_action(cx.listener(Self::on_escape))
            .capture_action(cx.listener(Self::on_enter))
            .capture_action(cx.listener(Self::on_tab))
            .capture_action(cx.listener(Self::on_paste))
            .child(toolbar::render_toolbar(self, cx))
            .when_some(error, |el, error| {
                el.child(
                    div()
                        .px_2()
                        .py_1()
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(error),
                )
            })
            .child(
                v_flex()
                    .p_1()
                    .gap_1()
                    .min_h(px(96.))
                    .children(self.render_edit_blocks(cx)),
            )
            .when_some(completion, |el, completion| el.child(completion))
            .into_any_element()
    }
}

/// Byte offset → `(line, character)` [`Position`] (character = chars, the
/// gpui-component rope convention). `pub(crate)`: the comment composer's
/// lightweight mention input (§4.6) shares it.
pub(crate) fn byte_offset_to_position(text: &str, offset: usize) -> Position {
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let before = &text[..offset];
    let line = before.matches('\n').count() as u32;
    let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let character = before[line_start..].chars().count() as u32;
    Position::new(line, character)
}

// ---------------------------------------------------------------------------
// MarkdownView — read-only rendered markdown (+ pills + task toggles)
// ---------------------------------------------------------------------------

type SourceEditCallback = Rc<dyn Fn(String, &mut Window, &mut App)>;

/// Read-only rendered GFM with live decoration pills. Construct per render
/// (it is an element, not a view); pass a shared [`ImageCache`] so image
/// bytes survive re-renders.
#[derive(gpui::IntoElement)]
pub struct MarkdownView {
    id: SharedString,
    source: String,
    resolver: Option<RefResolver>,
    images: Option<Entity<ImageCache>>,
    on_open_issue: Option<OpenIssueCallback>,
    on_source_edit: Option<SourceEditCallback>,
}

impl MarkdownView {
    pub fn new(id: impl Into<SharedString>, source: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            source: source.into(),
            resolver: None,
            images: None,
            on_open_issue: None,
            on_source_edit: None,
        }
    }

    /// Resolve `@email`/`#IDENT` into pills against the synced collections.
    pub fn resolver(mut self, resolver: RefResolver) -> Self {
        self.resolver = Some(resolver);
        self
    }

    pub fn images(mut self, images: Entity<ImageCache>) -> Self {
        self.images = Some(images);
        self
    }

    /// Clicking a resolved `#IDENT` pill.
    pub fn on_open_issue(
        mut self,
        on_open_issue: impl Fn(&str, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_open_issue = Some(Rc::new(on_open_issue));
        self
    }

    /// Fired with the FULL new markdown when a task checkbox is toggled in
    /// view mode. Without it, checkboxes render disabled.
    pub fn on_source_edit(
        mut self,
        on_source_edit: impl Fn(String, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_source_edit = Some(Rc::new(on_source_edit));
        self
    }
}

/// A clickable range's target in a rendered line.
enum ClickTarget {
    Url(String),
    Issue(String),
}

impl gpui::RenderOnce for MarkdownView {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let blocks = Rc::new(markdown_to_blocks(&self.source));
        let mut children: Vec<gpui::AnyElement> = Vec::new();

        for (block_index, block) in blocks.iter().enumerate() {
            match block {
                ContentBlock::Image { url, alt, .. } => {
                    children.push(render_image_slot(
                        ElementId::from(SharedString::from(format!(
                            "{}-image-{block_index}",
                            self.id
                        ))),
                        self.images.as_ref(),
                        url,
                        alt,
                        cx,
                    ));
                }
                ContentBlock::Text { content, .. } => {
                    if content.is_empty() {
                        continue;
                    }
                    let lines = content.lines();
                    let mut line_index = 0usize;
                    while line_index < lines.len() {
                        let attrs = content
                            .paragraphs
                            .get(line_index)
                            .cloned()
                            .unwrap_or(ParagraphAttrs::PLAIN);
                        if attrs.kind == BlockKind::CodeBlock {
                            // Group the fenced run and delegate to the
                            // gpui-component highlighter (§4.5).
                            let start = line_index;
                            while line_index < lines.len()
                                && content
                                    .paragraphs
                                    .get(line_index)
                                    .map(|a| a.kind == BlockKind::CodeBlock)
                                    .unwrap_or(false)
                            {
                                line_index += 1;
                            }
                            let lang = attrs.code_lang.clone().unwrap_or_default();
                            let code = lines[start..line_index].join("\n");
                            let fenced = format!("```{lang}\n{code}\n```");
                            children.push(
                                div()
                                    .w_full()
                                    .child(TextView::markdown(
                                        ElementId::from(SharedString::from(format!(
                                            "{}-code-{block_index}-{start}",
                                            self.id
                                        ))),
                                        fenced,
                                    ))
                                    .into_any_element(),
                            );
                            continue;
                        }
                        if attrs.kind == BlockKind::ThematicBreak {
                            children.push(
                                div()
                                    .w_full()
                                    .h_px()
                                    .my_2()
                                    .bg(cx.theme().border)
                                    .into_any_element(),
                            );
                            line_index += 1;
                            continue;
                        }
                        let element = render_view_line(
                            &self,
                            &blocks,
                            block_index,
                            line_index,
                            lines[line_index],
                            &attrs,
                            line_marks(content, &lines, line_index),
                            cx,
                        );
                        children.push(element);
                        line_index += 1;
                    }
                }
            }
        }

        v_flex().w_full().gap_1p5().children(children)
    }
}

/// Line-local marks (byte offsets into the line) for line `index`.
fn line_marks(
    content: &super::blocks::RichText,
    lines: &[&str],
    index: usize,
) -> Vec<InlineMark> {
    let mut start = 0usize;
    for line in lines.iter().take(index) {
        start += line.len() + 1;
    }
    let end = start + lines[index].len();
    content
        .marks
        .iter()
        .filter_map(|m| {
            let s = m.start.max(start);
            let e = m.end.min(end);
            (e > s).then(|| InlineMark {
                start: s - start,
                end: e - start,
                kind: m.kind,
                href: m.href.clone(),
            })
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn render_view_line(
    view: &MarkdownView,
    blocks: &Rc<Vec<ContentBlock>>,
    block_index: usize,
    line_index: usize,
    line: &str,
    attrs: &ParagraphAttrs,
    marks: Vec<InlineMark>,
    cx: &mut App,
) -> gpui::AnyElement {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let text_id = ElementId::from(SharedString::from(format!(
        "{}-line-{block_index}-{line_index}",
        view.id
    )));
    let display = build_display_line(line, &marks, view.resolver.as_ref(), cx);
    let mono = theme.mono_font_family.clone();

    let styled = StyledText::new(SharedString::from(display.text.clone()))
        .with_highlights(display.highlights.clone())
        .with_font_family_overrides(
            display
                .code_ranges
                .iter()
                .map(|range| (range.clone(), mono.clone()))
                .collect::<Vec<_>>(),
        );

    let text_element: gpui::AnyElement = if display.targets.is_empty() {
        styled.into_any_element()
    } else {
        let ranges: Vec<Range<usize>> = display.targets.iter().map(|(r, _)| r.clone()).collect();
        let targets: Rc<Vec<(Range<usize>, ClickTarget)>> = Rc::new(display.targets);
        let on_open_issue = view.on_open_issue.clone();
        InteractiveText::new(text_id, styled)
            .on_click(ranges, move |index, window, cx| {
                let Some((_, target)) = targets.get(index) else {
                    return;
                };
                // Inside the blurred-editor preview a click outside any target
                // means "start editing" — a hit on a link/pill must not also
                // bubble into that.
                cx.stop_propagation();
                match target {
                    ClickTarget::Url(url) => {
                        if let Err(error) = api::opener::open_in_browser(url) {
                            log::warn!("open link failed: {error}");
                        }
                        let _ = window;
                    }
                    ClickTarget::Issue(identifier) => {
                        if let Some(on_open_issue) = &on_open_issue {
                            on_open_issue(identifier, window, cx);
                        }
                    }
                }
            })
            .into_any_element()
    };

    // Wrap with block-level styling + list gutter.
    let content: gpui::AnyElement = match attrs.kind {
        BlockKind::Heading => {
            let el = div()
                .font_weight(FontWeight::BOLD)
                .w_full()
                .child(text_element);
            match attrs.heading_level {
                1 => el.text_xl(),
                2 => el.text_lg(),
                _ => el.text_base(),
            }
            .into_any_element()
        }
        BlockKind::Blockquote => h_flex()
            .w_full()
            .border_l_2()
            .border_color(theme.border)
            .pl_2()
            .text_color(muted)
            .child(text_element)
            .into_any_element(),
        BlockKind::ListItem => {
            let indent = px(12. * attrs.list_depth as f32);
            let gutter: gpui::AnyElement = match attrs.list_type {
                Some(ListType::Checklist) => {
                    let checked = attrs.checked;
                    let editable = view.on_source_edit.is_some();
                    let on_source_edit = view.on_source_edit.clone();
                    let blocks = blocks.clone();
                    Checkbox::new(ElementId::from(SharedString::from(format!(
                        "{}-task-{block_index}-{line_index}",
                        view.id
                    ))))
                    .checked(checked)
                    .disabled(!editable)
                    .on_click(move |_, window, cx| {
                        let Some(on_source_edit) = on_source_edit.clone() else {
                            return;
                        };
                        let mut new_blocks: Vec<ContentBlock> = (*blocks).clone();
                        if let Some(ContentBlock::Text { content, .. }) =
                            new_blocks.get_mut(block_index)
                        {
                            if let Some(attrs) = content.paragraphs.get_mut(line_index) {
                                attrs.checked = !attrs.checked;
                            }
                        }
                        let markdown = blocks_to_markdown(&new_blocks);
                        on_source_edit(markdown, window, cx);
                    })
                    .into_any_element()
                }
                Some(ListType::Ordered) => div()
                    .text_color(muted)
                    .child(SharedString::from(format!("{}.", attrs.ordered_index)))
                    .into_any_element(),
                _ => div()
                    .text_color(muted)
                    .child(SharedString::from("•"))
                    .into_any_element(),
            };
            h_flex()
                .w_full()
                .items_start()
                .gap_2()
                .pl(indent)
                .child(gutter)
                .child(div().flex_1().child(text_element))
                .into_any_element()
        }
        _ => div().w_full().child(text_element).into_any_element(),
    };

    content
}

// ---------------------------------------------------------------------------
// Display-line construction: inline styles + decoration pills
// ---------------------------------------------------------------------------

struct DisplayLine {
    text: String,
    highlights: Vec<(Range<usize>, HighlightStyle)>,
    code_ranges: Vec<Range<usize>>,
    targets: Vec<(Range<usize>, ClickTarget)>,
}

/// A decoration token found in the source line.
struct Decoration {
    range: Range<usize>,
    display_text: String,
    style: DecorationStyle,
}

enum DecorationStyle {
    MentionPill,
    IssuePill(String),
}

fn build_display_line(
    line: &str,
    marks: &[InlineMark],
    resolver: Option<&RefResolver>,
    cx: &App,
) -> DisplayLine {
    // 1. Find decorations on the source line (skipping inline-code ranges —
    //    code spans render literally).
    let code_spans: Vec<Range<usize>> = marks
        .iter()
        .filter(|m| m.kind == InlineKind::InlineCode)
        .map(|m| m.start..m.end)
        .collect();
    let in_code = |offset: usize| code_spans.iter().any(|r| r.contains(&offset));

    let mut decorations: Vec<Decoration> = Vec::new();
    if let Some(resolver) = resolver {
        for token in scan_mentions(line) {
            if in_code(token.start) {
                continue;
            }
            let email = &line[token.start + 1..token.end];
            if let Some(name) = (resolver.member_name)(email, cx) {
                decorations.push(Decoration {
                    range: token.clone(),
                    display_text: format!("@{name}"),
                    style: DecorationStyle::MentionPill,
                });
            }
        }
        for token in scan_issue_refs(line) {
            if in_code(token.start) {
                continue;
            }
            let identifier = line[token.start + 1..token.end].to_uppercase();
            if (resolver.issue_exists)(&identifier, cx) {
                decorations.push(Decoration {
                    range: token.clone(),
                    display_text: line[token.clone()].to_string(),
                    style: DecorationStyle::IssuePill(identifier),
                });
            }
        }
    }
    decorations.sort_by_key(|d| d.range.start);

    // 2. Build the display string + orig→display offset mapping.
    struct Replacement {
        orig: Range<usize>,
        display: Range<usize>,
    }
    let mut display = String::new();
    let mut replacements: Vec<Replacement> = Vec::new();
    let mut last = 0usize;
    for decoration in &decorations {
        if decoration.range.start < last {
            continue; // overlapping decoration; keep the first
        }
        display.push_str(&line[last..decoration.range.start]);
        let display_start = display.len();
        display.push_str(&decoration.display_text);
        replacements.push(Replacement {
            orig: decoration.range.clone(),
            display: display_start..display.len(),
        });
        last = decoration.range.end;
    }
    display.push_str(&line[last..]);

    let map_offset = |offset: usize| -> usize {
        let mut delta: isize = 0;
        for rep in &replacements {
            if offset <= rep.orig.start {
                break;
            }
            if offset < rep.orig.end {
                // Inside a replaced token: snap to the display token edge.
                return if offset == rep.orig.start {
                    rep.display.start
                } else {
                    rep.display.end
                };
            }
            delta += rep.display.len() as isize - rep.orig.len() as isize;
        }
        (offset as isize + delta).max(0) as usize
    };

    // 3. Collect styled ranges in display coordinates.
    let theme = cx.theme();
    #[derive(Default, Clone, PartialEq)]
    struct Bits {
        bold: bool,
        italic: bool,
        strike: bool,
        code: bool,
        link: bool,
        mention: bool,
        issue: bool,
    }
    let mut ranges: Vec<(Range<usize>, Bits)> = Vec::new();
    for mark in marks {
        let range = map_offset(mark.start)..map_offset(mark.end);
        if range.is_empty() {
            continue;
        }
        let mut bits = Bits::default();
        match mark.kind {
            InlineKind::Bold => bits.bold = true,
            InlineKind::Italic => bits.italic = true,
            InlineKind::Strikethrough => bits.strike = true,
            InlineKind::InlineCode => bits.code = true,
            InlineKind::Link => bits.link = true,
        }
        ranges.push((range, bits));
    }
    let mut targets: Vec<(Range<usize>, ClickTarget)> = Vec::new();
    for mark in marks {
        if mark.kind == InlineKind::Link {
            if let Some(href) = &mark.href {
                let range = map_offset(mark.start)..map_offset(mark.end);
                if !range.is_empty() {
                    targets.push((range, ClickTarget::Url(href.clone())));
                }
            }
        }
    }
    for (decoration, replacement) in decorations.iter().zip(&replacements) {
        let mut bits = Bits::default();
        match &decoration.style {
            DecorationStyle::MentionPill => bits.mention = true,
            DecorationStyle::IssuePill(identifier) => {
                bits.issue = true;
                targets.push((
                    replacement.display.clone(),
                    ClickTarget::Issue(identifier.clone()),
                ));
            }
        }
        if bits != Bits::default() {
            ranges.push((replacement.display.clone(), bits));
        }
    }

    // 4. Flatten overlapping ranges into disjoint highlight runs.
    let mut boundaries: Vec<usize> = vec![0, display.len()];
    for (range, _) in &ranges {
        boundaries.push(range.start);
        boundaries.push(range.end);
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    boundaries.retain(|b| display.is_char_boundary(*b));

    let mut highlights: Vec<(Range<usize>, HighlightStyle)> = Vec::new();
    let mut code_ranges: Vec<Range<usize>> = Vec::new();
    for pair in boundaries.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if b <= a {
            continue;
        }
        let mut bits = Bits::default();
        for (range, range_bits) in &ranges {
            if range.start <= a && range.end >= b {
                bits.bold |= range_bits.bold;
                bits.italic |= range_bits.italic;
                bits.strike |= range_bits.strike;
                bits.code |= range_bits.code;
                bits.link |= range_bits.link;
                bits.mention |= range_bits.mention;
                bits.issue |= range_bits.issue;
            }
        }
        if bits == Bits::default() {
            continue;
        }
        let mut style = HighlightStyle::default();
        if bits.bold {
            style.font_weight = Some(FontWeight::BOLD);
        }
        if bits.italic {
            style.font_style = Some(FontStyle::Italic);
        }
        if bits.strike {
            style.strikethrough = Some(StrikethroughStyle {
                thickness: px(1.),
                color: None,
            });
        }
        if bits.code {
            style.background_color = Some(theme.muted);
            code_ranges.push(a..b);
        }
        if bits.link {
            style.color = Some(theme.link);
            style.underline = Some(UnderlineStyle {
                thickness: px(1.),
                color: None,
                wavy: false,
            });
        }
        if bits.mention || bits.issue {
            style.background_color = Some(theme.secondary);
            style.color = Some(theme.secondary_foreground);
            style.font_weight = Some(FontWeight::MEDIUM);
        }
        highlights.push((a..b, style));
    }

    DisplayLine {
        text: display,
        highlights,
        code_ranges,
        targets,
    }
}

// -- Token scanners (mirror web `mentions.ts` / `issue-refs.ts`) ------------

/// `@<email>` occurrences (byte ranges incl. the `@`). The email is
/// `local@domain.tld` with the web `MENTION_RE` charset.
fn scan_mentions(line: &str) -> Vec<Range<usize>> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    let is_local = |c: u8| c.is_ascii_alphanumeric() || b"._%+-".contains(&c);
    let is_domain = |c: u8| c.is_ascii_alphanumeric() || c == b'.' || c == b'-';
    while i < bytes.len() {
        if bytes[i] != b'@' {
            i += 1;
            continue;
        }
        // local part
        let local_start = i + 1;
        let mut j = local_start;
        while j < bytes.len() && is_local(bytes[j]) {
            j += 1;
        }
        if j == local_start || j >= bytes.len() || bytes[j] != b'@' {
            i += 1;
            continue;
        }
        // domain
        let domain_start = j + 1;
        let mut k = domain_start;
        while k < bytes.len() && is_domain(bytes[k]) {
            k += 1;
        }
        let domain = &line[domain_start..k];
        // Needs a dot + ≥2-alpha TLD (web regex `\.[A-Za-z]{2,}`); trim
        // trailing dots/hyphens that the regex would not consume.
        let mut end = k;
        while end > domain_start
            && (bytes[end - 1] == b'.' || bytes[end - 1] == b'-')
        {
            end -= 1;
        }
        let domain = &domain[..end - domain_start];
        let tld_ok = domain
            .rsplit('.')
            .next()
            .map(|tld| tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic()))
            .unwrap_or(false)
            && domain.contains('.');
        if tld_ok {
            out.push(i..end);
            i = end;
        } else {
            i += 1;
        }
    }
    out
}

/// `#IDENTIFIER` occurrences (byte ranges incl. the `#`) — the web
/// `ISSUE_REF_SOURCE` contract: `(?<![\w#])#([A-Za-z][A-Za-z0-9]*-\d+)(?![\w-])`.
fn scan_issue_refs(line: &str) -> Vec<Range<usize>> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'#' {
            i += 1;
            continue;
        }
        // Lookbehind: not a word char or '#'.
        if i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_' || bytes[i - 1] == b'#') {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        if j >= bytes.len() || !bytes[j].is_ascii_alphabetic() {
            i += 1;
            continue;
        }
        while j < bytes.len() && bytes[j].is_ascii_alphanumeric() {
            j += 1;
        }
        if j >= bytes.len() || bytes[j] != b'-' {
            i += 1;
            continue;
        }
        let digit_start = j + 1;
        let mut k = digit_start;
        while k < bytes.len() && bytes[k].is_ascii_digit() {
            k += 1;
        }
        if k == digit_start {
            i += 1;
            continue;
        }
        // Lookahead: not a word char or '-'.
        if k < bytes.len()
            && (bytes[k].is_ascii_alphanumeric() || bytes[k] == b'_' || bytes[k] == b'-')
        {
            i = k;
            continue;
        }
        out.push(i..k);
        i = k;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_issue_refs_with_boundaries() {
        assert_eq!(scan_issue_refs("see #EXP-12 now"), vec![4..11]);
        assert_eq!(scan_issue_refs("foo#EXP-1"), Vec::<Range<usize>>::new());
        assert_eq!(scan_issue_refs("##EXP-1"), Vec::<Range<usize>>::new());
        assert_eq!(scan_issue_refs("#EXP-1-2"), Vec::<Range<usize>>::new());
        assert_eq!(scan_issue_refs("#EXP-115abc"), Vec::<Range<usize>>::new());
        assert_eq!(scan_issue_refs("(#a1-2)"), vec![1..6]);
    }

    #[test]
    fn scans_mentions() {
        assert_eq!(scan_mentions("cc @jane@example.com!"), vec![3..20]);
        assert_eq!(scan_mentions("no mention here"), Vec::<Range<usize>>::new());
        assert_eq!(scan_mentions("@bad@nodot"), Vec::<Range<usize>>::new());
        // Trailing period is punctuation, not part of the domain.
        assert_eq!(scan_mentions("ping @a.b@c.io."), vec![5..14]);
    }

    #[test]
    fn byte_offset_positions() {
        let text = "ab\ncdé f";
        assert_eq!(byte_offset_to_position(text, 0), Position::new(0, 0));
        assert_eq!(byte_offset_to_position(text, 2), Position::new(0, 2));
        assert_eq!(byte_offset_to_position(text, 3), Position::new(1, 0));
        // 'é' is 2 bytes: byte 7 (after é) is char 3 of line 1.
        assert_eq!(byte_offset_to_position(text, 7), Position::new(1, 3));
    }
}
