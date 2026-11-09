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

use crate::controls::PointerContextMenuExt as _;
use crate::toast::Toast;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;
use std::sync::{Arc, Mutex, Weak};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    canvas, div, img, px, App, AppContext as _, Bounds, ClipboardEntry, ClipboardItem,
    Context, ElementId, Entity, EntityId, Focusable as _,
    ImageCacheError, ImageSource, InteractiveElement as _, IntoElement,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels, Render,
    RenderImage, ScrollHandle, SharedString, StatefulInteractiveElement as _,
    Styled as _, StyledImage as _, Subscription, TextRun, WeakEntity,
    Window,
};
use gpui_component::input::{self, InputEvent, InputState, Position, Textarea, TextareaState};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    menu::PopupMenuItem,
    v_flex, ActiveTheme as _, Icon, Sizable as _,
};

use super::image_url;
use super::view::MarkdownView;

use super::autocomplete::{detect_trigger, CompletionItem, CompletionSource};
use super::blocks::ContentBlock;
use super::image_paste::{
    new_draft_url, pasted_image_parts, read_image_file, validate_image, AttachmentTransport,
    StagedImage,
};
use super::parse::{markdown_to_blocks_for, SoftBreakMode};
use super::serialize::blocks_to_markdown;
use super::toolbar::{self, LinePrefix};
use crate::controls::{typeahead_menu, typeahead_row, TypeaheadArm};
use crate::icons::registry;

// ---------------------------------------------------------------------------
// RefResolver — live @email / #IDENT resolution (§4.5 pills)
// ---------------------------------------------------------------------------

/// `email` → member display name (None ⇒ not a known member; stays text).
pub type MemberNameResolver = Rc<dyn Fn(&str, &App) -> Option<String>>;
/// `IDENTIFIER` → the issue's chip data (None ⇒ no such issue; stays text).
pub type IssueChipResolver = Rc<dyn Fn(&str, &App) -> Option<IssueChipData>>;

/// EXP-423: everything a resolved `#IDENT` chip renders in the read-only
/// view — the title plus the pre-resolved status icon (SVG asset path +
/// tint), mirroring the wysiwyg `IssueChipSnapshot`.
#[derive(Clone, Debug)]
pub struct IssueChipData {
    /// EXP-760: the row id the hover preview loads (`issue_preview::card`).
    pub issue_id: String,
    pub title: String,
    pub icon_path: SharedString,
    pub icon_color: gpui::Hsla,
}

/// EXP-776: the memo behind [`RefResolver::memoized`] — lower-cased
/// identifier → the chip (or the cached miss).
pub type IssueChipCache = Rc<RefCell<HashMap<String, Option<IssueChipData>>>>;

/// Resolves decoration tokens against the synced collections at render time.
#[derive(Clone)]
pub struct RefResolver {
    /// `email` → member display name (None ⇒ not a known member; stays text).
    pub member_name: MemberNameResolver,
    /// `IDENTIFIER` → the issue's chip data (None ⇒ no such issue; stays
    /// text) — EXP-307: the pill shows `#IDENT Title`, not just the short
    /// code; EXP-423: plus the status icon in the leading gutter.
    pub issue_chip: IssueChipResolver,
    /// EXP-760: also chip BARE `EXP-758` tokens (no `#`). Display-only and
    /// opt-in — the STEERING views and every CHAT text (EXP-1309, web
    /// `ChatMarkdown`) set it because agents narrate identifiers that way;
    /// descriptions and comments keep the `#IDENT` contract, which
    /// is what auto-relations and the stored text are matched against. The
    /// pattern is the web `BARE_REF_SOURCE` (`lib/issue-refs.ts`), mirrored
    /// byte for byte here and on both natives.
    pub bare: bool,
}

impl RefResolver {
    /// Resolve against the §05 collections of the given team. Reads the
    /// live store on every call — the §4.5 "decoration pass re-runs when the
    /// issues store changes" rule falls out of gpui re-render + this.
    pub fn from_store(team_id: impl Into<String>) -> Self {
        let ws_members = team_id.into();
        let ws_issues = ws_members.clone();
        Self {
            member_name: Rc::new(move |email, cx| {
                let collections = sync::Store::global(cx).collections();
                let members = collections.team_members.read(cx);
                let users = collections.users.read(cx);
                let needle = email.to_lowercase();
                members
                    .iter()
                    .filter(|m| m.team_id == ws_members)
                    .filter_map(|m| users.get(&m.user_id))
                    .find(|u| {
                        u.email
                            .as_deref()
                            .is_some_and(|e| e.to_lowercase() == needle)
                    })
                    .map(|u| u.name.clone().unwrap_or_else(|| email.to_string()))
            }),
            issue_chip: Rc::new(move |identifier, cx| {
                let collections = sync::Store::global(cx).collections();
                // EXP-776: a SCAN, not `issues_in_team` — that clones and
                // sorts every issue of the team, and a streaming transcript
                // asks this once per bare `EXP-nnn` token per frame.
                let boards = collections.boards.read(cx);
                let board_ids: HashSet<&str> = boards
                    .iter()
                    .filter(|board| board.team_id == ws_issues)
                    .map(|board| board.id.as_str())
                    .collect();
                collections
                    .issues
                    .read(cx)
                    .iter()
                    .filter(|issue| board_ids.contains(issue.board_id.as_str()))
                    .find(|issue| issue.identifier.eq_ignore_ascii_case(identifier))
                    .map(|issue| {
                        // EXP-423: resolve status → glyph + tint per render,
                        // exactly like the wysiwyg snapshot (refs.rs).
                        let status = crate::queries::resolve_issue_status(cx, issue);
                        IssueChipData {
                            issue_id: issue.id.clone(),
                            title: issue.title.clone(),
                            icon_path: crate::icons::glyph_svg_path(status.glyph),
                            icon_color: crate::icons::status_tint_color(&status.tint, cx),
                        }
                    })
            }),
            bare: false,
        }
    }

    /// EXP-776: memoise `issue_chip` behind `cache`, keyed by the lower-cased
    /// identifier. The CALLER owns the cache and clears it when the issues or
    /// statuses collections notify (glyph and tint are part of the chip), so
    /// a resolver built once per frame answers every repeated token off one
    /// scan. A miss is cached too — an unknown identifier in a long
    /// transcript is the common case.
    pub fn memoized(mut self, cache: IssueChipCache) -> Self {
        let inner = self.issue_chip.clone();
        self.issue_chip = Rc::new(move |identifier, cx| {
            let key = identifier.to_ascii_lowercase();
            if let Some(hit) = cache.borrow().get(&key) {
                return hit.clone();
            }
            let resolved = inner(identifier, cx);
            cache.borrow_mut().insert(key, resolved.clone());
            resolved
        });
        self
    }

    /// See [`Self::bare`]. Steering views only.
    pub fn bare(mut self, bare: bool) -> Self {
        self.bare = bare;
        self
    }

    /// Never resolves anything (tokens stay literal text).
    pub fn disabled() -> Self {
        Self {
            member_name: Rc::new(|_, _| None),
            issue_chip: Rc::new(|_, _| None),
            bare: false,
        }
    }
}

// ---------------------------------------------------------------------------
// ImageCache — async, auth-gated attachment bytes (shared editor + view)
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub(crate) enum ImageSlot {
    Loading,
    Ready(CachedImage),
    /// EXP-1128: a picture over the GPU texture cap, decoded off the
    /// foreground and cut into strips ([`crate::tall_image`]) — as one
    /// texture it would paint nothing.
    ReadyTall(Arc<crate::tall_image::TallImage>),
    /// A fetch failure is NEVER permanent: the timestamp gates a re-fetch on
    /// the next render after [`RETRY_AFTER`] — a transient network hiccup
    /// (stale keep-alive socket, brief offline) must not brick every image
    /// for the rest of the session.
    Failed(std::time::Instant),
    /// EXP-1128: a picture over the texture cap whose DECODE failed — a
    /// permanent state, never retried: the bytes will not change, and a
    /// [`RETRY_AFTER`] loop would re-download and re-decode a huge file
    /// every 5 s for as long as it is on screen.
    Unrenderable,
}

/// How long a failed fetch is displayed before the next render retries it.
pub(crate) const RETRY_AFTER: std::time::Duration = std::time::Duration::from_secs(5);

/// One attachment picture: its encoded bytes (the clipboard's "Copy image")
/// and the decode the `img` element paints. Decoded by US — on the
/// background executor, on the first paint — and handed to gpui as
/// `ImageSource::Custom`, never `ImageSource::Image`: that path parks every
/// decode in `App.loading_assets`, keyed by a fresh `Image` id per
/// `from_bytes`, with NO eviction, so every re-opened run or issue leaked
/// another full-RGBA screenshot (≈20 MB retina) for the rest of the session.
/// Here the pixels live in this `Arc` alone: the last holder's drop frees
/// them and queues the sprite-atlas tiles for release
/// ([`crate::tall_image::queue_atlas_release`]).
#[derive(Clone)]
pub(crate) struct CachedImage(Arc<SharedImage>);

struct SharedImage {
    encoded: Arc<gpui::Image>,
    decode: Mutex<DecodeState>,
}

enum DecodeState {
    Idle,
    /// Decoding on the background executor; the views to re-render when done.
    Running(Vec<EntityId>),
    Done(Result<Arc<RenderImage>, ImageCacheError>),
}

/// The LIVE picture of every attachment URL (query-stripped), one per `App`:
/// a second view of the same picture (issue + lightbox, chat + Guide, the
/// same run reopened while a list still shows it) shares ONE decode and ONE
/// atlas tile instead of fetching and decoding its own. `Weak`: it retains
/// nothing no view is holding — memory is bounded by what is on screen,
/// never by history; dead entries are pruned as it grows.
#[derive(Default)]
struct LiveImages(HashMap<String, Weak<SharedImage>>);

impl gpui::Global for LiveImages {}

/// Prune dead entries once the registry holds this many.
const LIVE_IMAGES_PRUNE_AT: usize = 128;

impl CachedImage {
    /// The live picture for `key`, else a new one over `encoded`, registered
    /// for the next asker.
    fn for_key(key: &str, encoded: Arc<gpui::Image>, cx: &mut App) -> Self {
        if let Some(live) = Self::live(key, cx) {
            return live;
        }
        let image = Self(Arc::new(SharedImage {
            encoded,
            decode: Mutex::new(DecodeState::Idle),
        }));
        let registry = &mut cx.default_global::<LiveImages>().0;
        if registry.len() >= LIVE_IMAGES_PRUNE_AT {
            registry.retain(|_, weak| weak.strong_count() > 0);
        }
        registry.insert(key.to_string(), Arc::downgrade(&image.0));
        image
    }

    /// The picture another holder already has for `key`, if any.
    fn live(key: &str, cx: &App) -> Option<Self> {
        cx.try_global::<LiveImages>()?
            .0
            .get(key)
            .and_then(Weak::upgrade)
            .map(Self)
    }

    /// Locally-staged bytes (a `draft://` paste): only this editor shows
    /// them, so they stay out of the registry.
    fn unshared(encoded: Arc<gpui::Image>) -> Self {
        Self(Arc::new(SharedImage {
            encoded,
            decode: Mutex::new(DecodeState::Idle),
        }))
    }

    /// The encoded bytes — the clipboard and the vendored WYSIWYG editor
    /// (whose `ImageSourceResolution::Decoded` takes an `Arc<gpui::Image>`).
    pub(crate) fn encoded(&self) -> Arc<gpui::Image> {
        self.0.encoded.clone()
    }

    /// The picture's pixel size: the decoded one, else the encoded header's.
    pub(crate) fn decoded_size(&self) -> Option<(f32, f32)> {
        match &*self
            .0
            .decode
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
        {
            DecodeState::Done(Ok(image)) => {
                let size = image.size(0);
                Some((size.width.0 as f32, size.height.0 as f32))
            }
            // Not decoded yet: the encoded header's size.
            _ => image::ImageReader::new(std::io::Cursor::new(self.0.encoded.bytes()))
                .with_guessed_format()
                .ok()?
                .into_dimensions()
                .ok()
                .map(|(width, height)| (width as f32, height as f32)),
        }
    }

    #[cfg(test)]
    fn decoded(&self) -> Option<Arc<RenderImage>> {
        match &*self
            .0
            .decode
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
        {
            DecodeState::Done(Ok(image)) => Some(image.clone()),
            _ => None,
        }
    }
}

/// `img(image)` paints through the custom loader: gpui's own `ImageSource::
/// Image` would decode AGAIN into its never-evicted asset map.
impl From<CachedImage> for ImageSource {
    fn from(image: CachedImage) -> Self {
        let shared = image.0;
        ImageSource::Custom(Arc::new(move |window, cx| shared.clone().load(window, cx)))
    }
}

impl SharedImage {
    /// gpui's `use_asset` contract: the decode once it is there, `None` while
    /// it runs — the first caller starts it, every caller's view is
    /// re-rendered on completion (`Window::use_asset` notifies only the
    /// first).
    fn load(
        self: Arc<Self>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        let view = window.current_view();
        let mut state = self
            .decode
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match &mut *state {
            DecodeState::Done(result) => return Some(result.clone()),
            DecodeState::Running(waiters) => {
                if !waiters.contains(&view) {
                    waiters.push(view);
                }
                return None;
            }
            DecodeState::Idle => {}
        }
        *state = DecodeState::Running(vec![view]);
        drop(state);
        let renderer = cx.svg_renderer();
        let encoded = self.encoded.clone();
        let decode = cx.background_executor().spawn(async move {
            encoded
                .to_image_data(renderer)
                .map_err(|error| ImageCacheError::Other(Arc::new(error)))
        });
        // Only a `Weak` rides the task: when every holder drops mid-decode
        // the pixels are discarded, never parked anywhere.
        let weak = Arc::downgrade(&self);
        window
            .spawn(cx, async move |cx| {
                let result = decode.await;
                let Some(shared) = weak.upgrade() else {
                    return;
                };
                let waiters = {
                    let mut state = shared
                        .decode
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    match std::mem::replace(&mut *state, DecodeState::Done(result)) {
                        DecodeState::Running(waiters) => waiters,
                        _ => Vec::new(),
                    }
                };
                cx.on_next_frame(move |_, cx| {
                    for view in waiters {
                        cx.notify(view);
                    }
                });
            })
            .detach();
        None
    }
}

impl Drop for SharedImage {
    fn drop(&mut self) {
        if let Ok(DecodeState::Done(Ok(image))) = self.decode.get_mut().as_deref() {
            crate::tall_image::queue_atlas_release(image);
        }
    }
}

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

    /// The transport, for flows that fetch outside the cache (download).
    pub fn transport(&self) -> Option<Arc<dyn AttachmentTransport>> {
        self.transport.clone()
    }

    /// The decoded image for `url` if (and only if) it is already cached —
    /// the "Copy image" path; never triggers a fetch.
    pub(crate) fn ready_image(&self, url: &str) -> Option<Arc<gpui::Image>> {
        match self.slots.get(url) {
            Some(ImageSlot::Ready(image)) => Some(image.encoded()),
            // A strip-decoded picture has no single encoded image to put on
            // the clipboard (and at >16k px it would be unusable there).
            Some(ImageSlot::ReadyTall(_)) => None,
            _ => None,
        }
    }

    /// Register locally-staged bytes (a `draft://` image) for rendering.
    pub fn insert_bytes(&mut self, url: String, content_type: &str, bytes: Vec<u8>) {
        let format = sniff_format(content_type, &bytes);
        let image = CachedImage::unshared(Arc::new(gpui::Image::from_bytes(format, bytes)));
        self.slots.insert(url, ImageSlot::Ready(image));
    }

    /// Re-key staged bytes once the upload resolved `draft://x` → real URL.
    pub fn alias(&mut self, from: &str, to: String) {
        if let Some(slot) = self.slots.get(from).cloned() {
            self.slots.insert(to, slot);
        }
    }

    /// Drop every `Failed` slot so the next [`Self::slot`] call re-fetches
    /// immediately. The WYSIWYG description has no render-driven `slot()`
    /// loop to ride the [`RETRY_AFTER`] backoff, so its scheduled retry
    /// (EXP-354) waits the backoff out on its own timer and then evicts.
    pub(crate) fn evict_failed(&mut self) {
        self.slots
            .retain(|_, slot| !matches!(slot, ImageSlot::Failed(_)));
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
        // Another view already holds this picture: share its decode — no
        // fetch, no second RGBA copy, one atlas tile.
        if let Some(image) = CachedImage::live(url, cx) {
            self.slots
                .insert(url.to_string(), ImageSlot::Ready(image.clone()));
            return ImageSlot::Ready(image);
        }
        // A cache built before the account was live (a view opened at
        // launch, `EXP_DEV_SCREEN`, a restored tab) has no transport: take
        // the active account's now instead of painting "unavailable" for the
        // life of the view.
        if self.transport.is_none() {
            self.transport = crate::queries::attachment_transport(cx);
        }
        let Some(transport) = self.transport.clone() else {
            return ImageSlot::Failed(std::time::Instant::now());
        };
        self.slots.insert(url.to_string(), ImageSlot::Loading);
        let url_owned = url.to_string();
        cx.spawn(async move |this, cx| {
            let fetch_url = url_owned.clone();
            let log_url = url_owned.clone();
            // The slot is built on the BACKGROUND executor too: a picture over
            // the texture cap is fully decoded + stripped here (EXP-1128).
            // Plain bytes come back ENCODED: the picture is registered (and
            // later decoded, off its first paint) on the foreground.
            let fetched = cx
                .background_executor()
                .spawn(async move {
                    match transport.fetch(&fetch_url) {
                        Ok(bytes) if crate::tall_image::needs_strips(&bytes) => {
                            match crate::tall_image::decode_tall(&bytes) {
                                Ok(tall) => Fetched::Slot(ImageSlot::ReadyTall(Arc::new(tall))),
                                Err(error) => {
                                    log::warn!("tall image decode failed for {log_url}: {error}");
                                    Fetched::Slot(ImageSlot::Unrenderable)
                                }
                            }
                        }
                        Ok(bytes) => {
                            let format = sniff_format("", &bytes);
                            Fetched::Encoded(Arc::new(gpui::Image::from_bytes(format, bytes)))
                        }
                        Err(error) => {
                            log::warn!("attachment fetch failed for {log_url}: {error}");
                            Fetched::Slot(ImageSlot::Failed(std::time::Instant::now()))
                        }
                    }
                })
                .await;
            this.update(cx, |cache, cx| {
                let slot = match fetched {
                    Fetched::Slot(slot) => slot,
                    Fetched::Encoded(encoded) => {
                        ImageSlot::Ready(CachedImage::for_key(&url_owned, encoded, cx))
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

/// What the background fetch hands the foreground.
enum Fetched {
    Slot(ImageSlot),
    Encoded(Arc<gpui::Image>),
}

/// Magic-byte sniff with a mime fallback (attachment bytes may be any of the
/// accepted types; gpui decodes per declared format).
pub(crate) fn sniff_format(content_type: &str, bytes: &[u8]) -> gpui::ImageFormat {
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

/// Editor-side hooks for one image slot (EXP-256): identify the block for
/// delete/resize, carry its painted bounds cell, and the live drag width
/// while a handle is being dragged.
pub(crate) struct EditorImageHooks {
    editor: WeakEntity<MarkdownEditor>,
    block_id: u64,
    bounds: Rc<std::cell::Cell<Bounds<Pixels>>>,
    drag_width: Option<f32>,
}

/// Natural (probed) pixel size of an own-attachment URL from the synced
/// `attachments` collection — the resize drag's upper clamp and, in the
/// WYSIWYG editor, the element's own display size (web reads the same synced
/// dims).
pub(crate) fn attachment_natural_size(url: &str, cx: &App) -> Option<(f32, f32)> {
    let id = image_url::attachment_id_from_src(url)?;
    let attachment = sync::Store::try_global(cx)?
        .collections()
        .attachments
        .read(cx)
        .get(id)
        .cloned()?;
    Some((attachment.width? as f32, attachment.height? as f32))
}

/// Just the width half of [`attachment_natural_size`].
fn attachment_natural_width(url: &str, cx: &App) -> Option<f32> {
    attachment_natural_size(url, cx).map(|(width, _)| width)
}

/// Download filename: the synced attachment row's `filename`, else the URL's
/// last path segment, else a generic fallback (web `handleDownload`).
fn attachment_download_filename(url: &str, cx: &App) -> String {
    if let Some(id) = image_url::attachment_id_from_src(url) {
        if let Some(name) = sync::Store::global(cx)
            .collections()
            .attachments
            .read(cx)
            .get(id)
            .and_then(|attachment| attachment.filename.clone())
            .filter(|name| !name.trim().is_empty())
        {
            return name;
        }
    }
    image_url::strip_query(url)
        .rsplit('/')
        .next()
        .filter(|segment| !segment.is_empty())
        .map(|segment| segment.to_string())
        .unwrap_or_else(|| "image.png".to_string())
}

/// EXP-256 "Download": native save dialog (defaulting to the OS downloads
/// dir) → background fetch through the auth-gated transport → disk, with a
/// window notification either way.
pub(crate) fn download_image(
    url: String,
    images: &Entity<ImageCache>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(transport) = images.read(cx).transport() else {
        return;
    };
    let filename = attachment_download_filename(&url, cx);
    let directory = dirs::download_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let receiver = crate::file_picker::prompt_for_new_path(cx, &directory, Some(&filename));
    let handle = window.window_handle();
    cx.spawn(async move |cx| {
        // Receiver error = dialog dismissed/unsupported; None = cancelled.
        let Ok(Ok(Some(path))) = receiver.await else {
            return;
        };
        let fetch_url = url.clone();
        let write_path = path.clone();
        let result = cx
            .background_executor()
            .spawn(async move {
                let bytes = transport.fetch(&fetch_url)?;
                std::fs::write(&write_path, bytes)?;
                anyhow::Ok(())
            })
            .await;
        let note = match result {
            Ok(()) => Toast::info(format!(
                "Saved to {}",
                path.display()
            )),
            Err(error) => {
                log::warn!("attachment download failed for {url}: {error}");
                crate::action_error::failed_toast("download", "the file", Some(error.to_string()))
            }
        };
        let _ = handle.update(cx, |_, window, cx| {
            crate::toast::show(note, window, cx);
        });
    })
    .detach();
}

/// EXP-1128: the widest a strip-rendered (>16k px) inline picture draws
/// without a `?w=` — a readable column, not its natural 16k.
const TALL_INLINE_MAX_W: f32 = 720.;

/// Web-parity image slot (EXP-256): centered in the column, sized by the
/// `?w=` width param carried in the markdown URL (query-stripped for the
/// cache fetch — the server ignores `?w=`, resizing is a client display
/// hint), right-click context menu, and — with [`EditorImageHooks`] — hover
/// resize handles, the hover ✕ remove, and a Delete menu item. Used by both
/// the editor's blocks and [`MarkdownView`] (comments/timeline), so the
/// read-only surfaces render identically minus the editing affordances.
fn render_image_slot(
    id: impl Into<ElementId>,
    images: Option<&Entity<ImageCache>>,
    url: &str,
    alt: &str,
    hooks: Option<EditorImageHooks>,
    chat: bool,
    on_open: Option<OpenImageCallback>,
    cx: &mut App,
) -> gpui::AnyElement {
    let fetch_url = image_url::strip_query(url).to_string();
    // EXP-1280: a CHAT picture (the steer feed, THE plan / question card) =
    // the full content width at its own aspect, `plan-card.json`
    // `card.pictureRadius` inside a `card.pictureStroke` card stroke.
    let chat_picture = chat.then(domain::plan_card::card);
    let slot = images
        .map(|images| images.update(cx, |cache, cx| cache.slot(&fetch_url, cx)))
        .unwrap_or_else(|| ImageSlot::Failed(std::time::Instant::now()));
    let body = match slot {
        ImageSlot::Ready(image) => {
            let own_attachment = image_url::attachment_id_from_src(url).is_some();
            // Live drag width wins over the persisted `?w=`; external image
            // URLs keep their query untouched and get no width handling.
            let display_width = hooks
                .as_ref()
                .and_then(|hooks| hooks.drag_width)
                .or_else(|| {
                    own_attachment
                        .then(|| image_url::width_param_from_src(url))
                        .flatten()
                });
            // The chat picture's aspect: the synced row's probed size, else
            // the decoded picture's.
            let aspect = chat_picture
                .and_then(|_| attachment_natural_size(&fetch_url, cx).or_else(|| image.decoded_size()))
                .filter(|(w, h)| *w > 0. && *h > 0.)
                .map(|(w, h)| w / h);
            // gpui's `img` gives a FRACTION width its natural height, so the
            // chat picture's box carries the aspect and the image fills it.
            let rendered = match (chat_picture, aspect) {
                (Some(card), Some(aspect)) => div()
                    .w_full()
                    .aspect_ratio(aspect)
                    .child(
                        img(image)
                            .size_full()
                            .object_fit(gpui::ObjectFit::Cover)
                            .rounded(px(card.picture_radius)),
                    )
                    .into_any_element(),
                (chat_picture, _) => img(image)
                    .when_some(display_width, |el, width| el.w(px(width)))
                    .max_w_full()
                    .object_fit(gpui::ObjectFit::ScaleDown)
                    .rounded(px(chat_picture.map_or(4., |card| card.picture_radius)))
                    .into_any_element(),
            };
            // Click-to-open (EXP-33): an in-app lightbox over the shared
            // [`ImageCache`] — never the web browser. The preview itself
            // carries the "Open in browser" affordance; always full size
            // (query-stripped), web parity.
            let Some(images) = images else {
                return rendered.into_any_element();
            };
            let clickable = {
                let images = images.clone();
                let src = url.to_string();
                let url = fetch_url.clone();
                let alt = alt.to_string();
                let on_open = on_open.clone();
                // Fixed child id — unique globally through the wrapper's id
                // path (gpui element ids are hierarchical).
                div()
                    .id("md-image-open")
                    .role(gpui::Role::Button)
                    .aria_label(if alt.is_empty() { "Image".to_string() } else { alt.clone() })
                    .when(chat_picture.is_some(), |el| el.w_full())
                    .cursor_pointer()
                    .on_click(move |_, window, cx| {
                        // In the blurred-editor preview a bubbling click
                        // would also start editing behind the lightbox.
                        cx.stop_propagation();
                        if let Some(on_open) = &on_open {
                            return on_open(&src, window, cx);
                        }
                        crate::image_preview::open_image_preview(
                            url.clone(),
                            alt.clone(),
                            None,
                            Some(images.clone()),
                            window,
                            cx,
                        );
                    })
                    .child(rendered)
            };
            let group_name = SharedString::from(
                hooks
                    .as_ref()
                    .map(|hooks| format!("md-image-{}", hooks.block_id))
                    .unwrap_or_else(|| "md-image-view".to_string()),
            );
            // The stable id lives on the WRAPPER: `.context_menu` keys its
            // popup state off this element's id — an id-less element falls
            // back to a per-frame pointer address and the menu never stays
            // open across the re-render.
            let mut wrapper = div()
                .id(id.into())
                .relative()
                .max_w_full()
                .when_some(chat_picture, |el, card| {
                    el.w_full()
                        .overflow_hidden()
                        .rounded(px(card.picture_radius))
                        .border(px(card.picture_stroke))
                        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                })
                .group(group_name.clone())
                .child(clickable);
            if let Some(hooks) = &hooks {
                wrapper = wrapper
                    // Record the painted image bounds — the resize drag's
                    // start width (same cell pattern as the text blocks).
                    .child({
                        let bounds = hooks.bounds.clone();
                        canvas(
                            move |element_bounds, _, _| bounds.set(element_bounds),
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full()
                    })
                    // Hover ✕ remove (pre-EXP-256 affordance, kept).
                    .child(render_image_remove_button(hooks))
                    // Hover resize handles, own attachments only (web only
                    // resizes URLs it can persist a `?w=` onto).
                    .when(own_attachment, |el| {
                        el.child(render_image_resize_handle(hooks, true, &group_name))
                            .child(render_image_resize_handle(hooks, false, &group_name))
                    });
            }
            attach_image_context_menu(
                wrapper,
                images,
                &fetch_url,
                alt,
                own_attachment,
                hooks.as_ref(),
            )
            .into_any_element()
        }
        // EXP-1128: a picture over the texture cap renders as stacked strips
        // at the `?w=` width (else its natural width, capped so it reads as
        // a column, never wider than the slot). Click still opens the
        // lightbox; the resize/remove hooks and the context menu stay on the
        // ordinary path — a >16k px inline picture is a rarity.
        ImageSlot::ReadyTall(tall) => {
            let display_width = hooks
                .as_ref()
                .and_then(|hooks| hooks.drag_width)
                .or_else(|| {
                    image_url::attachment_id_from_src(url)
                        .and_then(|_| image_url::width_param_from_src(url))
                })
                .unwrap_or(tall.width as f32)
                .min(TALL_INLINE_MAX_W);
            let natural = attachment_natural_size(&fetch_url, cx)
                .or(Some((tall.width as f32, tall.height as f32)));
            let mut wrapper = div()
                .id(id.into())
                .max_w_full()
                .overflow_hidden()
                .rounded(px(4.))
                .child(crate::tall_image::render_tall(&tall, display_width));
            if let Some(images) = images {
                let images = images.clone();
                let src = url.to_string();
                let url = fetch_url.clone();
                let alt = alt.to_string();
                wrapper = wrapper.cursor_pointer().on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    if let Some(on_open) = &on_open {
                        return on_open(&src, window, cx);
                    }
                    crate::image_preview::open_image_preview(
                        url.clone(),
                        alt.clone(),
                        natural,
                        Some(images.clone()),
                        window,
                        cx,
                    );
                });
            }
            wrapper.into_any_element()
        }
        ImageSlot::Loading => crate::feedback::skeleton_block("image-loading", 80., cx),
        ImageSlot::Failed(_) | ImageSlot::Unrenderable => placeholder_box(
            &if alt.is_empty() {
                "Image unavailable".to_string()
            } else {
                format!("Image unavailable: {alt}")
            },
            cx,
        ),
    };
    // Web `.editor-image-node`: the node hugs the image and centers in the
    // column (fixed/natural-width child inside a `w_full` `justify_center`
    // row — the app's safe centering shape).
    h_flex()
        .w_full()
        .justify_center()
        .child(body)
        .into_any_element()
}

/// The hover ✕ on an editor image block (absolute top-right, appears with
/// the group hover like the resize handles).
fn render_image_remove_button(hooks: &EditorImageHooks) -> impl IntoElement {
    let editor = hooks.editor.clone();
    let block_id = hooks.block_id;
    div().absolute().top_1().right_1().child(
        Button::new(ElementId::from(("md-image-remove", block_id as usize)))
            .ghost().cursor_pointer()
            .xsmall()
            .icon(Icon::new(registry::UI_CLOSE))
            .tooltip("Remove image")
            .on_click(move |_, window, cx| {
                // Never also fire the image's open-preview click beneath.
                cx.stop_propagation();
                let _ = editor.update(cx, |this, cx| {
                    let Some(index) = this.blocks.iter().position(|b| b.id() == block_id) else {
                        return;
                    };
                    this.remove_image_at(index, window, cx);
                });
            }),
    )
}

/// One resize handle pill (web `.editor-image-handle`): vertically centered
/// on the left/right edge, visible on group hover, starts an
/// [`ImageResizeDrag`] on mouse-down (the editor's render registers the
/// window-level move/up listeners while a drag is live).
fn render_image_resize_handle(
    hooks: &EditorImageHooks,
    left_edge: bool,
    group_name: &SharedString,
) -> impl IntoElement {
    let editor = hooks.editor.clone();
    let block_id = hooks.block_id;
    let bounds = hooks.bounds.clone();
    div()
        .id(ElementId::from((
            if left_edge {
                "md-image-handle-l"
            } else {
                "md-image-handle-r"
            },
            block_id as usize,
        )))
        .absolute()
        .top(gpui::relative(0.5))
        .mt(px(-20.))
        .map(|el| {
            if left_edge {
                el.left_1p5()
            } else {
                el.right_1p5()
            }
        })
        .w(px(6.))
        .h(px(40.))
        .rounded_full()
        .bg(gpui::white().opacity(0.7))
        .border_1()
        .border_color(gpui::black().opacity(0.2))
        .invisible()
        .group_hover(group_name.clone(), |el| el.visible())
        .cursor_ew_resize()
        .on_mouse_down(MouseButton::Left, move |event: &MouseDownEvent, _, cx| {
            // A drag must never also open the lightbox beneath.
            cx.stop_propagation();
            let start_width = f32::from(bounds.get().size.width);
            let start_x = event.position.x;
            let _ = editor.update(cx, |this, cx| {
                let natural_width = this
                    .blocks
                    .iter()
                    .find_map(|block| match block {
                        EditorBlock::Image { id, url, .. } if *id == block_id => Some(url.clone()),
                        _ => None,
                    })
                    .and_then(|url| attachment_natural_width(&url, cx));
                this.image_resize = Some(ImageResizeDrag {
                    block_id,
                    left_edge,
                    start_x,
                    start_width,
                    natural_width,
                    latest: None,
                });
                cx.notify();
            });
        })
}

/// The right-click menu (web's image node "…" menu): View image · Download ·
/// Copy image (macOS/Windows — Linux clipboard backends are text-only at pin
/// 1d217ee) · Remove from description (editor blocks only, destructive).
fn attach_image_context_menu(
    wrapper: gpui::Stateful<gpui::Div>,
    images: &Entity<ImageCache>,
    fetch_url: &str,
    alt: &str,
    own_attachment: bool,
    hooks: Option<&EditorImageHooks>,
) -> impl IntoElement {
    let images = images.clone();
    let url = fetch_url.to_string();
    let alt = alt.to_string();
    let editor = hooks.map(|hooks| (hooks.editor.clone(), hooks.block_id));
    crate::controls::claim_right_click(wrapper).pointer_context_menu(move |menu, _window, menu_cx| {
        // Editor blocks: hold edit mode while the menu is open — the popup
        // focuses itself, and the resulting input blur would otherwise flip
        // the editor to preview and unmount this menu (see
        // `MarkdownEditor::image_menu_open`). Cleared on dismiss.
        if let Some((editor, _)) = editor.clone() {
            let menu_entity = menu_cx.entity();
            let _ = editor.update(menu_cx, |this, cx| {
                this.image_menu_open = true;
                this._image_menu_subscription = Some(cx.subscribe(
                    &menu_entity,
                    |this, _, _: &gpui::DismissEvent, cx| {
                        this.image_menu_open = false;
                        this._image_menu_subscription = None;
                        cx.notify();
                    },
                ));
                cx.notify();
            });
        }
        // EXP-421: parity with the WYSIWYG host menu and the web image menu —
        // icons per row, destructive "Remove from description", no Copy link.
        let mut menu = menu.item(
            crate::controls::pointer_label_item("View image", false)
                .icon(Icon::from(crate::icons::registry::UI_WATCH))
                .on_click({
                    let images = images.clone();
                    let url = url.clone();
                    let alt = alt.clone();
                    move |_, window, cx| {
                        crate::image_preview::open_image_preview(
                            url.clone(),
                            alt.clone(),
                            None,
                            Some(images.clone()),
                            window,
                            cx,
                        );
                    }
                }),
        );
        if own_attachment {
            menu = menu.item(
                crate::controls::pointer_label_item("Download", false)
                    .icon(Icon::from(crate::icons::registry::UI_DOWNLOAD))
                    .on_click({
                        let images = images.clone();
                        let url = url.clone();
                        move |_, window, cx| {
                            download_image(url.clone(), &images, window, cx);
                        }
                    }),
            );
            // Linux clipboard backends are text-only at pin 1d217ee (Wayland
            // offers only TEXT_MIME_TYPES; X11 routes write_to_clipboard
            // through set_text — its set_image exists but is unwired), so
            // Copy image ships on macOS/Windows only.
            if cfg!(any(target_os = "macos", target_os = "windows")) {
                menu = menu.item(
                    crate::controls::pointer_label_item("Copy image", false)
                        .icon(Icon::from(crate::icons::registry::UI_COPY))
                        .on_click({
                            let images = images.clone();
                            let url = url.clone();
                            move |_, _, cx| {
                                if let Some(image) = images.read(cx).ready_image(&url) {
                                    cx.write_to_clipboard(ClipboardItem::new_image(&image));
                                }
                            }
                        }),
                );
            }
        }
        if let Some((editor, block_id)) = editor.clone() {
            // EXP-697: no dividers in menus.
            menu = menu.item(
                PopupMenuItem::element(|_, cx| {
                    div()
                        .text_color(cx.theme().danger)
                        .child("Remove from description")
                })
                .icon(
                    Icon::from(crate::icons::registry::UI_DELETE)
                        .text_color(menu_cx.theme().danger),
                )
                .on_click(move |_, window, cx| {
                    let _ = editor.update(cx, |this, cx| {
                        let Some(index) = this.blocks.iter().position(|b| b.id() == block_id)
                        else {
                            return;
                        };
                        this.remove_image_at(index, window, cx);
                    });
                }),
            );
        }
        menu
    })
}

pub(crate) fn placeholder_box(label: &str, cx: &App) -> gpui::AnyElement {
    // No `w_full` here: a percent width resolves fit-content under a
    // `max_w`-clamped ancestor (EXP-179) — leaving the width auto lets the
    // parent flex column stretch it to the real column width instead.
    div()
        .h(px(80.))
        .rounded(px(4.))
        .bg(theme::tokens::glass::FILL_SECTION.to_hsla())
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
        input: Entity<TextareaState>,
        bounds: Rc<std::cell::Cell<Bounds<Pixels>>>,
        _sub: Subscription,
    },
    Image {
        id: u64,
        url: String,
        alt: String,
        /// Painted image bounds (same cell pattern as the text blocks) — the
        /// resize drag's start width.
        bounds: Rc<std::cell::Cell<Bounds<Pixels>>>,
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

/// An in-flight image resize drag (EXP-256; web `ResizeDrag`). Started by a
/// handle's mouse-down, advanced/finished by window-level listeners the
/// editor registers while `Some` (the gpui-component resizable-panel
/// pattern).
struct ImageResizeDrag {
    block_id: u64,
    left_edge: bool,
    start_x: Pixels,
    /// Painted width at mouse-down.
    start_width: f32,
    /// Natural probed width — the upper clamp; dragging to it drops the
    /// `?w=` param (markdown stays canonical-clean).
    natural_width: Option<f32>,
    /// Live width during the drag (None until the first move = plain click).
    latest: Option<f32>,
}

type ChangeCallback = Rc<dyn Fn(&str, &mut Window, &mut App)>;
type BlurCallback = Rc<dyn Fn(&mut Window, &mut App)>;
type OpenIssueCallback = Rc<dyn Fn(&str, &mut Window, &mut App)>;
/// #EXP-1493: a clicked picture's raw `src` (see [`MarkdownView::on_open_image`]).
type OpenImageCallback = Rc<dyn Fn(&str, &mut Window, &mut App)>;

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
    /// The completion menu's scroll (release review R5): ↑/↓ keep the
    /// selected row in view past the cap.
    completion_scroll: ScrollHandle,
    /// SLOP-28: the open `#` typeahead (THE search list at the caret) with
    /// the block it opened in, and the `#` the reader closed.
    typeahead: Option<(u64, crate::picker::search_list::OpenIssueTypeahead)>,
    typeahead_gate: crate::picker::search_list::TypeaheadGate,
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
    /// In-flight image resize drag (EXP-256), `None` otherwise.
    image_resize: Option<ImageResizeDrag>,
    /// An image context menu is open (EXP-256). The popup focuses itself, so
    /// without this flag the blur would flip a `preview_when_blurred` editor
    /// back to preview and unmount the very element hosting the menu.
    image_menu_open: bool,
    _image_menu_subscription: Option<Subscription>,
    error: Option<SharedString>,
    /// Blurred-preview seam (EXP-161): while no text block owns focus the
    /// editor renders a [`MarkdownView`] of its own document — live
    /// `@email`/`#IDENT` pills, clickable links, rendered GFM — and swaps
    /// back to the editable blocks on click. Opt-in (detail description);
    /// the create dialog keeps the always-editable surface.
    preview_when_blurred: bool,
    /// Edit-mode card chrome (border + rounding + inner padding). The create
    /// dialog keeps it (a bordered form field); the detail description turns
    /// it off (EXP-256 — entering edit mode must not shift the content
    /// sideways or draw a box around the whole description).
    chrome: bool,
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
            completion_scroll: ScrollHandle::new(),
            typeahead: None,
            typeahead_gate: Default::default(),
            images,
            transport: None,
            upload_issue_id: None,
            staged: Vec::new(),
            uploads_in_flight: 0,
            link_editor: None,
            image_resize: None,
            image_menu_open: false,
            _image_menu_subscription: None,
            error: None,
            preview_when_blurred: false,
            chrome: true,
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

    /// Toggle the edit-mode card chrome (border/rounding + inner padding).
    /// Off for the detail description (EXP-256): edit mode then aligns with
    /// the borderless blurred preview instead of jumping into a card.
    pub fn set_chrome(&mut self, chrome: bool) {
        self.chrome = chrome;
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
        let blocks = markdown_to_blocks_for(
            markdown,
            SoftBreakMode::Space,
            crate::queries::instance_origin(cx).as_deref(),
        );
        self.blocks = blocks
            .iter()
            .enumerate()
            .map(|(index, block)| match block {
                // EXP-726: a table has no structural editor block here (this
                // legacy block editor is superseded by the vendored WYSIWYG
                // surface, which edits tables natively). Its canonical raw
                // text goes into a text block, so it renders as source and
                // still re-serializes byte-identically.
                // EXP-824: an attachment link is source text here too — the
                // legacy editor keeps `[clip.mp4](url)` editable as-is.
                ContentBlock::Text { .. }
                | ContentBlock::Table { .. }
                | ContentBlock::AttachmentLink { .. } => {
                    let fragment = blocks_to_markdown(std::slice::from_ref(block));
                    let placeholder = (index == 0).then(|| self.placeholder.clone());
                    self.new_text_block_with_placeholder(&fragment, placeholder, window, cx)
                }
                ContentBlock::Image { url, alt, .. } => EditorBlock::Image {
                    id: super::blocks::next_block_id(),
                    url: url.clone(),
                    alt: alt.clone(),
                    bounds: Rc::new(std::cell::Cell::new(Bounds::default())),
                },
            })
            .collect();
        self.focused_block = None;
        self.completion = None;
        self.typeahead = None;
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
            let mut state = TextareaState::new(window, cx)
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

    fn text_input(&self, block_id: u64) -> Option<Entity<TextareaState>> {
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
        input: Entity<TextareaState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                self.refresh_completion(block_id, &input, window, cx);
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
        input: &Entity<TextareaState>,
        window: &mut Window,
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
        // SLOP-28: a `#` token opens THE search list at the caret.
        if let Some(team_id) = source.issue_team_id() {
            if let Some(prefill) = self.typeahead_gate.open_on(&value, cursor) {
                self.completion = None;
                if self.typeahead.is_none() {
                    self.open_typeahead(block_id, input, team_id, prefill, window, cx);
                }
                return;
            }
        }
        let Some(token) = detect_trigger(&value, cursor)
            .filter(|token| token.trigger != super::CompletionTrigger::IssueRef)
        else {
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

    /// SLOP-28: open the `#` typeahead on `prefill`, hung off the caret line.
    fn open_typeahead(
        &mut self,
        block_id: u64,
        input: &Entity<TextareaState>,
        team_id: String,
        prefill: crate::picker::search_list::TypeaheadPrefill,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::picker::search_list::{textarea_caret_anchor, OpenIssueTypeahead};
        let Some(bounds) = self.blocks.iter().find_map(|block| match block {
            EditorBlock::Text { id, bounds, .. } if *id == block_id => Some(bounds.clone()),
            _ => None,
        }) else {
            return;
        };
        let anchor = textarea_caret_anchor(input.read(cx), bounds.get().origin, prefill.start, window);
        let open = OpenIssueTypeahead::open(team_id, prefill, anchor, window, cx, {
            move |this: &mut Self, event, token, window, cx| {
                this.on_typeahead_event(block_id, event, token, window, cx)
            }
        });
        self.typeahead = Some((block_id, open));
    }

    /// SLOP-28: a pick writes `#IDENT ` over the token; Escape leaves the
    /// text as typed; Backspace in the empty field deletes the token; focus
    /// returns to the block on all three (a click away keeps it there).
    fn on_typeahead_event(
        &mut self,
        block_id: u64,
        event: &crate::picker::search_list::IssueTypeaheadEvent,
        token: &crate::picker::search_list::TypeaheadPrefill,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::picker::search_list::{typeahead_insert, IssueTypeaheadEvent};
        self.typeahead = None;
        let replacement = match event {
            IssueTypeaheadEvent::Pick(identifier) => Some(typeahead_insert(identifier)),
            IssueTypeaheadEvent::Close { delete_token: true } => Some(String::new()),
            IssueTypeaheadEvent::Close { delete_token: false } | IssueTypeaheadEvent::Dismiss => {
                self.typeahead_gate.close(token.start);
                None
            }
        };
        let Some(input) = self.text_input(block_id) else {
            cx.notify();
            return;
        };
        if let Some(replacement) = replacement {
            input.update(cx, |state, cx| {
                let value = state.value().to_string();
                let start = token.start;
                let end = start + token.token_len();
                if end > value.len() || !value.is_char_boundary(start) || !value.is_char_boundary(end) {
                    return;
                }
                let new_value = format!("{}{replacement}{}", &value[..start], &value[end..]);
                let position = byte_offset_to_position(&new_value, start + replacement.len());
                state.set_value(new_value, window, cx);
                state.set_cursor_position(position, window, cx);
            });
            self.emit_change(window, cx);
        }
        if !matches!(event, IssueTypeaheadEvent::Dismiss) {
            input.update(cx, |state, cx| state.focus(window, cx));
        }
        cx.notify();
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
                self.completion_scroll.scroll_to_item(next as usize);
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
                        // FEED-79: a failed upload is the error toast.
                        Err(error) => {
                            log::warn!("image upload failed: {error}");
                            crate::action_error::failed_action("upload", "the image", Some(error.to_string()), cx);
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
            bounds: Rc::new(std::cell::Cell::new(Bounds::default())),
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

    /// Advance a live resize drag (window-level mouse move). Web
    /// `moveResize`: clamp to [MIN, natural] and re-render at the live width.
    fn update_image_resize(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let Some(drag) = self.image_resize.as_mut() else {
            return;
        };
        let delta = f32::from(x - drag.start_x);
        let raw = if drag.left_edge {
            drag.start_width - delta
        } else {
            drag.start_width + delta
        };
        let max = drag
            .natural_width
            .unwrap_or(image_url::FALLBACK_MAX_RESIZE_WIDTH)
            .max(image_url::MIN_RESIZE_WIDTH);
        let clamped = raw.clamp(image_url::MIN_RESIZE_WIDTH, max).round();
        if drag.latest != Some(clamped) {
            drag.latest = Some(clamped);
            cx.notify();
        }
    }

    /// Finish a resize drag (window-level mouse up). Web `endResize`: no
    /// move = no change; at (or beyond) the natural width the `?w=` param is
    /// dropped; a changed URL is a structural edit → commit immediately.
    fn finish_image_resize(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = self.image_resize.take() else {
            return;
        };
        cx.notify();
        let Some(latest) = drag.latest else {
            return;
        };
        let at_full = drag.natural_width.is_some_and(|natural| latest >= natural);
        let width = (!at_full).then_some(latest as u32);
        let changed = self.blocks.iter_mut().any(|block| match block {
            EditorBlock::Image { id, url, .. } if *id == drag.block_id => {
                let next = image_url::src_with_width(url, width);
                if next != *url {
                    *url = next;
                    true
                } else {
                    false
                }
            }
            _ => false,
        });
        if changed {
            self.emit_commit(window, cx);
        }
    }

    fn remove_image_by_url(&mut self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self
            .blocks
            .iter()
            .position(|block| matches!(block, EditorBlock::Image { url: u, .. } if u == url))
        else {
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
        let receiver = crate::file_picker::prompt_for_paths(cx, gpui::PathPromptOptions {
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
                            crate::action_error::failed_action("attach", "the image", Some(error.to_string()), cx);
                        }
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    // -- Rendering -------------------------------------------------------------

    fn render_completion(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
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

        // The caret LINE's edges in window coordinates: the menu primitive
        // measures the room on both sides itself.
        let origin = bounds.get().origin;
        // The 8px is the field's inner padding on both axes: the text starts
        // one padding step below the box's top edge, so the caret line does
        // too (release review R5: without it the menu overlapped the line).
        let caret_left = origin.x + caret_x + px(8.);
        let caret_bottom =
            origin.y + scroll.y + line_height * (position.line as f32 + 1.) + px(8.);
        let caret_top = caret_bottom - line_height;

        let items = completion.items.clone();
        let selected = completion.selected;
        let rows: Vec<gpui::AnyElement> = items
            .into_iter()
            .enumerate()
            .map(|(index, item)| {
                // EXP-892: the selected row is the ONLY tinted one, and
                // hovering MOVES the selection onto the row under the
                // pointer instead of painting a second highlight.
                typeahead_row(
                    ElementId::from(("md-completion-item", index)),
                    index == selected,
                    cx,
                )
                    .on_hover(cx.listener(move |this, hovered: &bool, _window, cx| {
                        if !*hovered {
                            return;
                        }
                        if let Some(completion) = this.completion.as_mut() {
                            if completion.selected != index {
                                completion.selected = index;
                                cx.notify();
                            }
                        }
                    }))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            if let Some(completion) = this.completion.as_mut() {
                                completion.selected = index;
                            }
                            this.accept_completion(window, cx);
                        }),
                    )
                    // EXP-426: the shared decorated row — status glyph for
                    // `#`, avatar for `@`.
                    .child(super::completion_row_content(&item, cx))
                    .into_any_element()
            })
            .collect();
        // EXP-970: the ONE typeahead surface — capped, scrolling, and
        // flipped above the caret when the room below runs out.
        Some(typeahead_menu(
            "md-completion",
            TypeaheadArm::Anchored {
                caret_left,
                caret_top,
                caret_bottom,
            },
            rows,
            &self.completion_scroll,
            window,
            cx,
        ))
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
                                Textarea::new(input)
                                    .appearance(false)
                                    // Chrome-less (detail description): drop
                                    // the widget's built-in 12px input_px so
                                    // the text aligns with the blurred
                                    // preview instead of shifting sideways
                                    // on click (EXP-256; EXP-181 trick).
                                    .when(!self.chrome, |input| input.px_0())
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
                EditorBlock::Image {
                    id,
                    url,
                    alt,
                    bounds,
                    ..
                } => {
                    let block_id = *id;
                    let hooks = EditorImageHooks {
                        editor: cx.entity().downgrade(),
                        block_id,
                        bounds: bounds.clone(),
                        drag_width: self
                            .image_resize
                            .as_ref()
                            .filter(|drag| drag.block_id == block_id)
                            .and_then(|drag| drag.latest),
                    };
                    elements.push(render_image_slot(
                        ElementId::from(("md-image-open", index)),
                        Some(&images),
                        url,
                        alt,
                        Some(hooks),
                        false,
                        None,
                        cx,
                    ));
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
    // Per-entity id: the view id keys EXP-233's recorded content width, so
    // two live previews must never share one.
    let mut view = MarkdownView::new(
        SharedString::from(format!("md-editor-preview-view-{}", cx.entity_id())),
        markdown,
    )
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
        .role(gpui::Role::MultilineTextInput)
        // Web `MarkdownEditor`'s `aria-label` (`ariaLabel ?? \`Issue description\``;
        // no web caller overrides it, and the description is this editor's one use).
        .aria_label("Issue description")
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
        let _span = tracing::trace_span!("render", view = "MarkdownEditor").entered();
        if self.preview_when_blurred
            && self.typeahead.is_none()
            && self.link_editor.is_none()
            && self.error.is_none()
            && !self.image_menu_open
            && self.image_resize.is_none()
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
        // SLOP-28: the open `#` typeahead.
        let typeahead = self
            .typeahead
            .as_ref()
            .map(|(_, typeahead)| typeahead.render(window, cx));
        // While an image resize drag is live, one canvas child registers the
        // window-level move/up listeners each frame (the gpui-component
        // resizable-panel pattern) — the mouse may leave the handle mid-drag.
        let resize_overlay = self.image_resize.is_some().then(|| {
            let entity = cx.entity();
            canvas(
                |_, _, _| (),
                move |_, _, window, _| {
                    let move_entity = entity.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                        if !phase.bubble() {
                            return;
                        }
                        move_entity.update(cx, |this, cx| {
                            this.update_image_resize(event.position.x, cx);
                        });
                    });
                    let up_entity = entity.clone();
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
                        if !phase.bubble() {
                            return;
                        }
                        up_entity.update(cx, |this, cx| {
                            this.finish_image_resize(window, cx);
                        });
                    });
                },
            )
            .absolute()
            .size_full()
        });

        v_flex()
            .relative()
            .key_context("MarkdownEditor")
            .w_full()
            .when(self.chrome, |el| {
                el.border_1().border_color(theme_border).rounded(px(6.))
            })
            .capture_action(cx.listener(Self::on_move_up))
            .capture_action(cx.listener(Self::on_move_down))
            .capture_action(cx.listener(Self::on_escape))
            .capture_action(cx.listener(Self::on_enter))
            .capture_action(cx.listener(Self::on_tab))
            .capture_action(cx.listener(Self::on_paste))
            .child(toolbar::render_toolbar(self, window, cx))
            // An image the editor refuses (type, size) is INVALID INPUT: the
            // line under the toolbar (FEED-79).
            .when_some(error, |el, error| {
                el.child(crate::action_error::field_error(error, cx).px_2().py_1())
            })
            .child(
                v_flex()
                    // Chrome-less: keep the blurred preview's py_1-only
                    // layout so entering edit mode never shifts the text
                    // sideways (the border's px is gone too).
                    .map(|el| if self.chrome { el.p_1() } else { el.py_1() })
                    .gap_1()
                    .min_h(px(96.))
                    .children(self.render_edit_blocks(cx)),
            )
            .when_some(completion, |el, completion| el.child(completion))
            .children(typeahead)
            .when_some(resize_overlay, |el, overlay| el.child(overlay))
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

/// Keep issue chips readable (web parity: `MAX_CHIP_TITLE_LENGTH` in
/// `issue-ref-extension.ts` — 60 chars, ellipsis beyond).
pub(crate) fn truncate_chip_title(title: &str) -> String {
    const MAX_CHIP_TITLE_CHARS: usize = 60;
    let trimmed = title.trim();
    if trimmed.chars().count() <= MAX_CHIP_TITLE_CHARS {
        return trimmed.to_string();
    }
    let cut: String = trimmed.chars().take(MAX_CHIP_TITLE_CHARS - 1).collect();
    format!("{}…", cut.trim_end())
}

// -- Token scanners (mirror web `mentions.ts` / `issue-refs.ts`) ------------

/// `@<email>` occurrences (byte ranges incl. the `@`). The email is
/// `local@domain.tld` with the web `MENTION_RE` charset.
pub(crate) fn scan_mentions(line: &str) -> Vec<Range<usize>> {
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
        // Web `MENTION_RE` parity: the greedy domain run backtracks to the
        // LAST '.' with at least one domain char before it that is followed
        // by ≥2 ASCII alpha chars, and the match ends right after that
        // maximal alpha run (`@a@example.com2` → `@a@example.com`,
        // `@j@a.co-op` → `@j@a.co`). No qualifying dot — including a dot at
        // `domain_start`, where the regex's `[A-Za-z0-9.-]+` would be empty
        // — means no match at all.
        let mut end = None;
        let mut d = k;
        while d > domain_start + 1 {
            d -= 1;
            if bytes[d] != b'.' {
                continue;
            }
            let mut m = d + 1;
            while m < k && bytes[m].is_ascii_alphabetic() {
                m += 1;
            }
            if m - (d + 1) >= 2 {
                end = Some(m);
                break;
            }
        }
        if let Some(end) = end {
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
pub(crate) fn scan_issue_refs(line: &str) -> Vec<Range<usize>> {
    scan_issue_refs_with(line, false)
}

/// EXP-760: the same scan, optionally ALSO matching the BARE form agents
/// narrate (`EXP-758`, no `#`) — the web `ISSUE_REF_BARE_SOURCE` alternation
/// (`lib/issue-refs.ts`), left to right, first alternative wins:
///
/// - hash: `(?<![\w#])#([A-Za-z][A-Za-z0-9]*-\d+)(?![\w-])`
/// - bare: `(?<![\w#-])([A-Z][A-Z0-9]*-\d+)(?![\w-])`
///
/// The bare prefix must be UPPERCASE (`utf-8` is not a reference) and may
/// follow `/` (so `exp/EXP-758` chips) but never a word char, `#` or `-` (so
/// `foo-EXP-1` and the tail of `#EXP-115-2` stay text). Returned ranges cover
/// the token AS WRITTEN — the `#` included when it is there — so a caller
/// strips a leading `#` to get the identifier.
///
/// Only the caller's resolver decides what actually chips: an unresolved
/// token stays plain text either way.
pub(crate) fn scan_issue_refs_with(line: &str, bare: bool) -> Vec<Range<usize>> {
    let bytes = line.as_bytes();
    let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    // `{PREFIX}-{number}` starting at `start`; `upper` demands an uppercase
    // prefix (the bare arm). Returns the end offset, tail check included.
    let identifier_end = |start: usize, upper: bool| -> Option<usize> {
        let first = *bytes.get(start)?;
        if upper {
            if !first.is_ascii_uppercase() {
                return None;
            }
        } else if !first.is_ascii_alphabetic() {
            return None;
        }
        let mut j = start + 1;
        while j < bytes.len()
            && (if upper {
                bytes[j].is_ascii_uppercase() || bytes[j].is_ascii_digit()
            } else {
                bytes[j].is_ascii_alphanumeric()
            })
        {
            j += 1;
        }
        if bytes.get(j) != Some(&b'-') {
            return None;
        }
        let digits = j + 1;
        let mut k = digits;
        while k < bytes.len() && bytes[k].is_ascii_digit() {
            k += 1;
        }
        if k == digits {
            return None;
        }
        // Tail: `(?![\w-])`.
        match bytes.get(k) {
            Some(b) if word(*b) || *b == b'-' => None,
            _ => Some(k),
        }
    };
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let prev = i.checked_sub(1).map(|p| bytes[p]);
        if bytes[i] == b'#' {
            // Lookbehind `(?<![\w#])`.
            let glued = prev.is_some_and(|b| word(b) || b == b'#');
            if !glued {
                if let Some(end) = identifier_end(i + 1, false) {
                    out.push(i..end);
                    i = end;
                    continue;
                }
            }
        } else if bare {
            // Lookbehind `(?<![\w#-])`.
            let glued = prev.is_some_and(|b| word(b) || b == b'#' || b == b'-');
            if !glued {
                if let Some(end) = identifier_end(i, true) {
                    out.push(i..end);
                    i = end;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 4×4 PNG as the encoded picture a cache slot holds.
    fn test_png() -> Arc<gpui::Image> {
        let buffer = image::RgbaImage::from_pixel(4, 4, image::Rgba([10, 20, 30, 255]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(buffer)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Png, bytes))
    }

    /// Every asker of one URL gets the SAME picture while a holder lives;
    /// the last holder's drop forgets it — the registry retains nothing of
    /// its own, so memory is bounded by what is on screen.
    #[gpui::test]
    async fn cached_images_share_one_decode_per_url_and_release_with_the_last_holder(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let key = "test://cached-image/shared";
            let first = CachedImage::for_key(key, test_png(), cx);
            let second = CachedImage::for_key(key, test_png(), cx);
            assert!(Arc::ptr_eq(&first.0, &second.0), "one decode per URL");
            assert!(CachedImage::live(key, cx).is_some());
            drop(first);
            assert!(CachedImage::live(key, cx).is_some(), "a holder still lives");
            drop(second);
            assert!(
                CachedImage::live(key, cx).is_none(),
                "forgotten with the last holder"
            );
            let third = CachedImage::for_key(key, test_png(), cx);
            assert!(CachedImage::live(key, cx).is_some_and(|live| Arc::ptr_eq(&live.0, &third.0)));

            // Dead entries never pile up: past the prune mark an insert sweeps them.
            for i in 0..LIVE_IMAGES_PRUNE_AT {
                drop(CachedImage::for_key(
                    &format!("test://dead/{i}"),
                    test_png(),
                    cx,
                ));
            }
            drop(CachedImage::for_key("test://dead/last", test_png(), cx));
            assert!(
                cx.global::<LiveImages>().0.len() < LIVE_IMAGES_PRUNE_AT,
                "the sweep dropped the dead entries"
            );
            assert!(
                CachedImage::live(key, cx).is_some(),
                "the live one survived the sweep"
            );
        });
    }

    /// The whole lifetime through a real window: `img(CachedImage)` decodes
    /// off the first paint WITHOUT an entry in gpui's never-evicted asset
    /// map, paints an atlas tile, and the last holder's drop queues that tile
    /// for the shell's per-frame release.
    #[gpui::test]
    async fn cached_image_paints_outside_the_asset_map_and_frees_its_tile(
        cx: &mut gpui::TestAppContext,
    ) {
        struct Host {
            image: Option<CachedImage>,
        }
        impl Render for Host {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                let _span = tracing::trace_span!("render", view = "Host").entered();
                div().size_full().children(
                    self.image
                        .clone()
                        .map(|image| img(image).w(px(4.)).h(px(4.))),
                )
            }
        }

        let key = "test://cached-image/paint";
        let image = cx.update(|cx| CachedImage::for_key(key, test_png(), cx));
        let encoded = image.encoded();
        let (view, cx) = cx.add_window_view(|_, _| Host {
            image: Some(image.clone()),
        });
        let mut decoded = None;
        for _ in 0..6 {
            cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.run_until_parked();
            decoded = image.decoded();
        }
        let render = decoded.expect("decoded off the paint");
        cx.update(|window, cx| {
            assert!(
                !ImageSource::Image(encoded.clone()).is_asset_cached(cx),
                "the decode must never enter App.loading_assets"
            );
            assert!(window.has_image_atlas_entry(&render), "painted a tile");
        });

        drop(image);
        view.update(cx, |host, _| host.image = None);
        assert!(
            crate::tall_image::atlas_release_queued(render.id),
            "the last holder's drop queues the tile"
        );
        cx.update(|_, cx| assert!(CachedImage::live(key, cx).is_none()));
        cx.update(|window, _| crate::tall_image::release_dropped_strips(window));
        cx.update(|window, _| assert!(!window.has_image_atlas_entry(&render)));
    }

    #[test]
    fn scans_issue_refs_with_boundaries() {
        assert_eq!(scan_issue_refs("see #EXP-12 now"), vec![4..11]);
        assert_eq!(scan_issue_refs("foo#EXP-1"), Vec::<Range<usize>>::new());
        assert_eq!(scan_issue_refs("##EXP-1"), Vec::<Range<usize>>::new());
        assert_eq!(scan_issue_refs("#EXP-1-2"), Vec::<Range<usize>>::new());
        assert_eq!(scan_issue_refs("#EXP-115abc"), Vec::<Range<usize>>::new());
        assert_eq!(scan_issue_refs("(#a1-2)"), vec![1..6]);
        // Bare tokens are NOT references without the opt-in.
        assert_eq!(scan_issue_refs("see EXP-12 now"), Vec::<Range<usize>>::new());
    }

    #[test]
    fn bare_mode_scans_hash_and_bare_tokens() {
        let bare = |line: &str| scan_issue_refs_with(line, true);
        // Both forms, in one line, left to right.
        assert_eq!(bare("see EXP-12 and #EXP-13"), vec![4..10, 15..22]);
        // A branch name chips its identifier (the `/` is not a word char).
        assert_eq!(bare("exp/EXP-758"), vec![4..11]);
        // Lowercase prefixes are not identifiers — this is the rule that
        // keeps ordinary prose out.
        assert_eq!(bare("utf-8 and x86-64"), Vec::<Range<usize>>::new());
        // Glued to a word, a `#` or a `-`: still text.
        assert_eq!(bare("foo-EXP-1"), Vec::<Range<usize>>::new());
        assert_eq!(bare("fooEXP-1"), Vec::<Range<usize>>::new());
        assert_eq!(bare("##EXP-1"), Vec::<Range<usize>>::new());
        // Tail boundary, both alternatives.
        assert_eq!(bare("EXP-115abc"), Vec::<Range<usize>>::new());
        assert_eq!(bare("EXP-1-2"), Vec::<Range<usize>>::new());
        // Punctuation around it is fine.
        assert_eq!(bare("(EXP-9)."), vec![1..6]);
        // The hash alternative keeps its own, looser prefix rule.
        assert_eq!(bare("#exp-12"), vec![0..7]);
        assert_eq!(bare("exp-12"), Vec::<Range<usize>>::new());
    }

    /// #EXP-1674: THE scan, replayed ×4 (`fixtures/issue-refs.json`; web
    /// `issue-refs.test.ts`, iOS `IssueRefsTests`, Android `IssueRefsTest`).
    #[test]
    fn the_issue_ref_scan_replays_its_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../../packages/domain-contract/fixtures/issue-refs.json"
        ))
        .expect("issue-refs.json parses");
        let strings = |value: &serde_json::Value| -> Vec<String> {
            value
                .as_array()
                .expect("a token list")
                .iter()
                .map(|token| token.as_str().expect("a token").to_string())
                .collect()
        };
        for case in fixture["cases"].as_array().expect("cases") {
            let name = case["name"].as_str().expect("name");
            let text = case["text"].as_str().expect("text");
            let tokens = |bare: bool| -> Vec<String> {
                scan_issue_refs_with(text, bare).into_iter().map(|range| text[range].to_string()).collect()
            };
            assert_eq!(tokens(false), strings(&case["hash"]), "{name} (hash)");
            assert_eq!(tokens(true), strings(&case["bare"]), "{name} (bare)");
        }
    }

    /// EXP-760: only the STEERING resolver chips bare tokens; prose keeps the
    /// `#IDENT` contract (auto-relations are matched against that form).
    #[test]
    fn bare_chips_are_opt_in_per_resolver() {
        assert!(!test_resolver().bare);
        assert!(bare_resolver().bare);
    }

    #[test]
    fn scans_mentions() {
        assert_eq!(scan_mentions("cc @jane@example.com!"), vec![3..20]);
        assert_eq!(scan_mentions("no mention here"), Vec::<Range<usize>>::new());
        assert_eq!(scan_mentions("@bad@nodot"), Vec::<Range<usize>>::new());
        // Trailing period is punctuation, not part of the domain.
        assert_eq!(scan_mentions("ping @a.b@c.io."), vec![5..14]);
        // The regex backtracks instead of rejecting the whole token.
        assert_eq!(scan_mentions("@a@example.com2"), vec![0..14]);
        assert_eq!(scan_mentions("@j@a.co-op"), vec![0..7]);
        assert_eq!(scan_mentions("@a@b.co.uk2"), vec![0..10]);
        assert_eq!(scan_mentions("@x@y.com-"), vec![0..8]);
        // A dot at the domain start leaves `[A-Za-z0-9.-]+` empty.
        assert_eq!(scan_mentions("@a@.com"), Vec::<Range<usize>>::new());
        assert_eq!(scan_mentions("@a@b.c"), Vec::<Range<usize>>::new());
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

    fn test_resolver() -> RefResolver {
        RefResolver {
            member_name: Rc::new(|email, _| {
                (email == "ada@example.com").then(|| "Ada".to_string())
            }),
            issue_chip: Rc::new(|identifier, _| {
                (identifier == "EXP-42").then(|| IssueChipData {
                    issue_id: "i-42".to_string(),
                    title: "Fix login flow".to_string(),
                    // A real bundled asset, so the real-window paint test
                    // drives the paint_svg path (the asset may still be
                    // unloadable headless — tolerated via `.ok()`).
                    icon_path: SharedString::from("icons/circle.svg"),
                    icon_color: gpui::Hsla::default(),
                })
            }),
            bare: false,
        }
    }

    /// EXP-760: the same resolver in BARE mode — the steering feeds' opt-in.
    fn bare_resolver() -> RefResolver {
        test_resolver().bare(true)
    }

    /// EXP-381: [`PillText`] computes chip quads from the child's text layout
    /// during paint — this drives the whole path (layout handle, wrap-row
    /// segmentation, quad painting) through a real window so a layout-order
    /// regression panics here instead of in the running app.
    #[gpui::test]
    async fn markdown_view_paints_chip_pills_in_a_real_window(cx: &mut gpui::TestAppContext) {
        struct Host;
        impl Render for Host {
            fn render(
                &mut self,
                _window: &mut Window,
                _cx: &mut Context<Self>,
            ) -> impl IntoElement {
                let _span = tracing::trace_span!("render", view = "Host").entered();
                div().size_full().child(
                    MarkdownView::new(
                        SharedString::from("pill-paint-test"),
                        // Long enough to soft-wrap in the test window, so the
                        // per-row segmentation runs too.
                        "Ping @ada@example.com about #EXP-42 and again \
                         #EXP-42 #EXP-42 #EXP-42 #EXP-42 #EXP-42 #EXP-42",
                    )
                    .resolver(test_resolver()),
                )
            }
        }

        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let (_view, cx) = cx.add_window_view(|_window, _cx| Host);
        for _ in 0..2 {
            cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.run_until_parked();
        }
    }

}
