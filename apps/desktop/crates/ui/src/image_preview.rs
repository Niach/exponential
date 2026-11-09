//! In-app image lightbox (EXP-33): clicking an attachment chip or an inline
//! description/comment image opens this modal preview — never the web
//! browser. Built on the shared native dialog window (EXP-284 — same modal
//! shape as the duplicate picker), so Esc-close and the dismiss semantics
//! come from the one dialog pattern the app uses. EXP-426: the window sizes
//! itself to the image's aspect ratio when the natural dimensions are known,
//! the in-window title is gone (the filename stays the OS window title), and
//! "Open in browser" sits in the header next to the ✕. EXP-824 adds the
//! media variant ([`open_media_preview`]): the poster (or a neutral box)
//! behind a big "Open in player" button — the desktop decodes no media, so
//! the preview is a launcher for the system player, not a player.

use std::rc::Rc;

use eui_gpui::controls::{ZoomableImage, ZoomableImageOptions};
use gpui::{
    div, img, px, size, App, AppContext as _, Entity, IntoElement, ParentElement, Pixels, Render,
    ScrollHandle, SharedString, Size, Styled, StyledImage as _, Subscription, Window,
};
use gpui_component::v_flex;

use domain::session_results::SESSION_RESULT_TALL_ASPECT;
use gpui_component::{
    button::{Button, ButtonVariants as _},
    ActiveTheme as _, Disableable as _, Icon,
};

use crate::controls::WebControl as _;
use crate::icons::ExpIcon;
use crate::markdown::{attachment_natural_size, placeholder_box, ImageCache, ImageSlot};
use crate::media_tile::{
    self, MediaKind, MediaTile, VIDEO_TILE_DEFAULT_H, VIDEO_TILE_DEFAULT_W,
};
use crate::native_dialog::{self, DialogContent, DialogSpec};
use crate::queries;

/// The chromeless header row's height share of the window (px_4/pt_3 padding
/// plus the xsmall button line) — [`preview_window_size`] reserves it so the
/// image itself gets the aspect-true remainder.
const PREVIEW_HEADER_H: f32 = 44.;

/// The image lightbox's picture margin (each side) and, EXP-1128, the right
/// gutter a strip column keeps clear for the overlay scrollbar.
const TALL_BODY_PAD: f32 = 8.;
const TALL_SCROLLBAR_GUTTER: f32 = 12.;

/// A TALL picture (width/height under the shared ×4
/// [`SESSION_RESULT_TALL_ASPECT`]) opens in a column window at a readable
/// width (the zoom view fits its width, from the top) instead of a sliver
/// scaled to fit the viewport height.
fn is_tall(natural: Option<(f32, f32)>) -> bool {
    matches!(natural, Some((width, height)) if width > 0. && height > 0.
        && width / height < SESSION_RESULT_TALL_ASPECT)
}

/// Open the lightbox for one image. `url` is the image's canonical (usually
/// relative `/api/attachments/{id}`) form — the same key the [`ImageCache`]
/// fetches by. Pass the owning surface's cache when it has one (editor /
/// rendered view) so already-decoded bytes show instantly; `None` builds a
/// fresh cache over the active account's attachment transport (chips).
/// `natural_size` is the image's probed pixel size when the caller knows it
/// (attachment rows, the WYSIWYG layout probe); attachment URLs fall back to
/// the synced probe, anything else to a generic viewport-share window.
pub(crate) fn open_image_preview(
    url: String,
    label: String,
    natural_size: Option<(f32, f32)>,
    images: Option<Entity<ImageCache>>,
    window: &mut Window,
    cx: &mut App,
) {
    let images = match images {
        Some(images) => images,
        None => {
            let transport = queries::attachment_transport(cx);
            cx.new(|_| ImageCache::new(transport))
        }
    };
    let open_url = queries::absolute_api_url(cx, &url);
    let label = preview_label(&label, &url);

    let natural = natural_size.or_else(|| attachment_natural_size(&url, cx));
    let window_size = preview_window_size(natural, window.viewport_size());
    let tall = is_tall(natural);
    // EXP-285: chromeless — traffic lights over the image corner read as
    // dirt; the header ✕ stays the dismissal. The label still names the OS
    // window even though the in-content header shows no title. EXP-1128: a
    // tall picture's column is resizable (wider = bigger text).
    let mut spec = DialogSpec::new(label.clone(), window_size).chromeless();
    if tall {
        spec = spec.resizable(size(px(420.), px(280.)));
    }
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let preview = cx.new(|cx| ImagePreview::new(url, &label, images, natural, window, cx));
        // The header's ✕ is the only mouse dismissal a lightbox has (there is
        // no Cancel footer) — without it the modal window is a dead end.
        let mut content = DialogContent::new(preview).chromeless_header("").padless();
        if let Some(open_url) = open_url {
            content = content.chromeless_header_actions(move |_, cx| {
                open_in_browser_button("image-preview-open-browser", open_url.clone(), cx)
            });
        }
        content
    });
}

/// The header's "Open in browser" affordance, shared by both lightboxes.
fn open_in_browser_button(id: &'static str, open_url: String, cx: &App) -> gpui::AnyElement {
    Button::new(id)
        .ghost()
        .web_xs()
        .icon(Icon::from(ExpIcon::ArrowUpRight).text_color(cx.theme().muted_foreground))
        .label("Open in browser")
        .on_click(move |_, _, _| {
            if let Err(error) = api::opener::open_in_browser(&open_url) {
                log::warn!("[ui] preview: open in browser failed: {error}");
            }
        })
        .into_any_element()
}

/// EXP-824: the lightbox for a video/audio attachment. Sized like an image
/// preview from the poster's box (16:9 default); the body is the poster or
/// a neutral box with a big "Open in player" button that runs the same
/// temp-download-and-open path as the tile click. "Open in browser" stays
/// in the header.
pub(crate) fn open_media_preview(
    tile: MediaTile,
    images: Option<Entity<ImageCache>>,
    window: &mut Window,
    cx: &mut App,
) {
    let images = match images {
        Some(images) => images,
        None => {
            let transport = queries::attachment_transport(cx);
            cx.new(|_| ImageCache::new(transport))
        }
    };
    let open_url = queries::absolute_api_url(
        cx,
        &format!("/api/attachments/{}", tile.attachment_id),
    );
    let natural = match tile.kind {
        MediaKind::Video => Some(tile.video_box()),
        MediaKind::Audio => Some((VIDEO_TILE_DEFAULT_W, VIDEO_TILE_DEFAULT_H)),
    };
    let window_size = preview_window_size(natural, window.viewport_size());
    let spec = DialogSpec::new(tile.label.clone(), window_size).chromeless();
    native_dialog::open_dialog_window(window, cx, spec, move |_, cx| {
        let preview = cx.new(|cx| MediaPreview::new(tile, images, cx));
        let mut content = DialogContent::new(preview).chromeless_header("").padless();
        if let Some(open_url) = open_url {
            content = content.chromeless_header_actions(move |_, cx| {
                open_in_browser_button("media-preview-open-browser", open_url.clone(), cx)
            });
        }
        content
    });
}

struct MediaPreview {
    tile: MediaTile,
    images: Entity<ImageCache>,
    _images_changed: Subscription,
}

impl MediaPreview {
    fn new(tile: MediaTile, images: Entity<ImageCache>, cx: &mut gpui::Context<Self>) -> Self {
        let images_changed = cx.observe(&images, |_, _, cx| cx.notify());
        Self {
            tile,
            images,
            _images_changed: images_changed,
        }
    }
}

impl Render for MediaPreview {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "MediaPreview").entered();
        let busy = media_tile::is_opening(&self.tile.attachment_id);
        let poster = self
            .tile
            .poster_url
            .clone()
            .map(|url| self.images.update(cx, |cache, cx| cache.slot(&url, cx)));
        let backdrop = match poster {
            Some(ImageSlot::Ready(image)) => img(image)
                .max_w_full()
                .max_h_full()
                .object_fit(gpui::ObjectFit::ScaleDown)
                .rounded(cx.theme().radius)
                .into_any_element(),
            _ => div()
                .size_full()
                .rounded(cx.theme().radius)
                .bg(theme::tokens::glass::FILL_SECTION.to_hsla())
                .into_any_element(),
        };
        let attachment_id = self.tile.attachment_id.clone();
        let label = self.tile.label.clone();
        let is_audio = self.tile.kind == MediaKind::Audio;
        let caption = self.tile.duration_chip();

        div()
            .size_full()
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .p_2()
            .child(backdrop)
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .child(media_tile::play_badge(72., busy))
                    .child(
                        Button::new("media-preview-open-player")
                            .primary()
                            .icon(Icon::from(ExpIcon::Play))
                            .label(if busy { "Opening…" } else { "Open in player" })
                            .disabled(busy)
                            .on_click(move |_, window, cx| {
                                media_tile::open_media_in_player(
                                    attachment_id.clone(),
                                    label.clone(),
                                    window,
                                    cx,
                                );
                            }),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(SharedString::from(match (is_audio, caption) {
                                (true, Some(duration)) => {
                                    format!("{} · {duration}", self.tile.label)
                                }
                                (true, None) => self.tile.label.clone(),
                                (false, Some(duration)) => duration,
                                (false, None) => String::new(),
                            })),
                    ),
            )
    }
}

/// Chip/alt label → filename fallback → generic.
fn preview_label(label: &str, url: &str) -> String {
    let trimmed = label.trim();
    if !trimmed.is_empty() && trimmed != "image" {
        return trimmed.to_string();
    }
    let filename = url
        .rsplit('/')
        .next()
        .and_then(|segment| segment.split('?').next())
        .unwrap_or_default();
    if !filename.is_empty() {
        filename.to_string()
    } else {
        "Image".to_string()
    }
}

/// The lightbox window's initial size. With known natural dimensions the
/// window matches the IMAGE's aspect ratio (plus the header strip): the image
/// scales to fit 90% of the viewport, never upscales, and the window floors
/// at 480×320 so the header controls always fit. Without dimensions, the old
/// generic viewport share. EXP-1128: a TALL picture gets a column —
/// its natural width within 480..=min(90% viewport, 1100), 90% of the height.
fn preview_window_size(natural: Option<(f32, f32)>, viewport: Size<Pixels>) -> Size<Pixels> {
    let (vw, vh) = (f32::from(viewport.width), f32::from(viewport.height));
    match natural {
        Some((width, _)) if is_tall(natural) => {
            size(px(width.min(vw * 0.9).min(1100.).max(480.)), px(vh * 0.9))
        }
        Some((width, height)) if width > 0. && height > 0. => {
            let scale = (vw * 0.9 / width)
                .min((vh * 0.9 - PREVIEW_HEADER_H) / height)
                .min(1.0);
            size(
                px((width * scale).max(480.)),
                px((height * scale + PREVIEW_HEADER_H).max(320.)),
            )
        }
        _ => size(px((vw * 0.8).min(1100.)), px(vh * 0.8)),
    }
}

// ---------------------------------------------------------------------------
// EXP-1586 — THE lightbox picture (every lightbox below)
// ---------------------------------------------------------------------------

/// The picture body of [`ImagePreview`], [`GuideViewer`] and
/// [`PictureGallery`]: the eUI zoom view (`eui_gpui::controls::ZoomableImage`,
/// catalog `zoomableImage`) over the cached decode — fit, never upscaled, a
/// TALL picture at fit-width from the top (the wheel scrolls it), zoom and
/// pan, the pill. A Tap does nothing here; ←/→ stay the host's (the view
/// never takes them), the zoom keys reach it through [`Self::key`] while the
/// host holds focus.
struct LightboxPicture {
    zoom: Entity<ZoomableImage>,
    /// The strip column's scroll (a strip-decoded TALL picture only).
    strips: ScrollHandle,
}

impl LightboxPicture {
    fn new<V: 'static>(alt: &str, cx: &mut gpui::Context<V>) -> Self {
        let options = ZoomableImageOptions {
            alt: SharedString::from(alt.to_string()),
            theme: eui_core::themes::builtin_theme(crate::exponential_ui_host::THEME_ID),
            mode: eui_core::theme::Mode::Dark,
            host: Rc::new(crate::exponential_ui_host::IdePlugin { media_origin: String::new(), uploads: None }),
            reduced_motion: theme::motion::reduce_motion(),
            ..ZoomableImageOptions::default()
        };
        let zoom = cx.new(|cx| ZoomableImage::new("", None, None, options, cx));
        Self { zoom, strips: ScrollHandle::new() }
    }

    /// A page change: the next picture opens at rest, without animation.
    fn show(&self, url: &str, cx: &mut App) {
        self.zoom.update(cx, |zoom, cx| zoom.set_source(url.to_string(), None, None, cx));
    }

    /// A key while the host holds focus (the view's own rule: no modifier
    /// chords); `true` = the view zoomed.
    fn key(&self, event: &gpui::KeyDownEvent, window: &mut Window, cx: &mut App) -> bool {
        let modifiers = event.keystroke.modifiers;
        if modifiers.platform || modifiers.control || modifiers.alt {
            return false;
        }
        let key = event
            .keystroke
            .key_char
            .as_deref()
            .filter(|key| !key.is_empty())
            .unwrap_or(event.keystroke.key.as_str())
            .to_string();
        self.zoom.update(cx, |zoom, cx| zoom.handle_key(&key, window, cx))
    }

    /// The picture filling a `relative` box `box_w` wide, `pad` in from its
    /// edges. `natural` = the probed size (`None` = the decoded one).
    fn render(
        &self,
        id: &'static str,
        url: &str,
        slot: ImageSlot,
        natural: Option<(f32, f32)>,
        box_w: f32,
        pad: f32,
        cx: &mut App,
    ) -> gpui::AnyElement {
        let fill = || div().absolute().inset_0().p(px(pad)).flex().items_center().justify_center();
        match slot {
            ImageSlot::Ready(image) => {
                let natural = natural.map(|(width, height)| (f64::from(width), f64::from(height)));
                self.zoom
                    .update(cx, |zoom, cx| zoom.set_source(url.to_string(), Some(image.into()), natural, cx));
                fill().child(div().size_full().child(self.zoom.clone())).into_any_element()
            }
            // Over the texture cap (`crate::tall_image`): the zoom view paints
            // one texture, so a strip-decoded TALL picture keeps the EXP-1128
            // scroll column (no zoom) at the box's width, less the overlay
            // scrollbar's gutter.
            ImageSlot::ReadyTall(tall) if is_tall(Some((tall.width as f32, tall.height as f32))) => {
                let inner_w = (box_w - pad * 2. - TALL_SCROLLBAR_GUTTER).max(1.);
                v_flex()
                    .absolute()
                    .inset_0()
                    .px(px(pad))
                    .pb(px(pad))
                    .child(crate::scroll_pane::v_scroll_pane(
                        SharedString::from(format!("{id}-scroll")),
                        &self.strips,
                        v_flex()
                            .w_full()
                            .pr(px(TALL_SCROLLBAR_GUTTER))
                            .child(crate::tall_image::render_tall(&tall, inner_w)),
                    ))
                    .into_any_element()
            }
            // A >16k px WIDE picture: strips at the box's width, clipped.
            ImageSlot::ReadyTall(tall) => {
                let width = (box_w - pad * 2.).min(tall.width as f32).max(1.);
                fill()
                    .child(
                        div()
                            .max_w_full()
                            .max_h_full()
                            .overflow_hidden()
                            .child(crate::tall_image::render_tall(&tall, width)),
                    )
                    .into_any_element()
            }
            ImageSlot::Loading => fill()
                .child(crate::feedback::skeleton_block(SharedString::from(format!("{id}-loading")), 80., cx))
                .into_any_element(),
            ImageSlot::Failed(_) | ImageSlot::Unrenderable => {
                fill().child(placeholder_box("Image unavailable", cx)).into_any_element()
            }
        }
    }
}

/// Keeps the host's focus across the dialog window becoming key (EXP-1348:
/// the shell takes it on that edge, which left the keys dead).
fn refocus_on_activation<V: gpui::Focusable>(
    focus_handle: &gpui::FocusHandle,
    window: &mut Window,
    cx: &mut gpui::Context<V>,
) -> Subscription {
    window.focus(focus_handle, cx);
    cx.observe_window_activation(window, |this, window, cx| {
        let focus_handle = this.focus_handle(cx);
        if window.is_window_active() && !focus_handle.contains_focused(window, cx) {
            window.focus(&focus_handle, cx);
        }
    })
}

struct ImagePreview {
    url: String,
    images: Entity<ImageCache>,
    natural: Option<(f32, f32)>,
    picture: LightboxPicture,
    focus_handle: gpui::FocusHandle,
    /// Re-render when the cache resolves the async fetch.
    _images_changed: Subscription,
    _activation: Subscription,
}

impl ImagePreview {
    fn new(
        url: String,
        label: &str,
        images: Entity<ImageCache>,
        natural: Option<(f32, f32)>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let images_changed = cx.observe(&images, |_, _, cx| cx.notify());
        let focus_handle = cx.focus_handle();
        let activation = refocus_on_activation(&focus_handle, window, cx);
        Self {
            url,
            images,
            natural,
            picture: LightboxPicture::new(label, cx),
            focus_handle,
            _images_changed: images_changed,
            _activation: activation,
        }
    }
}

impl gpui::Focusable for ImagePreview {
    fn focus_handle(&self, _cx: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ImagePreview {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "ImagePreview").entered();
        use gpui::InteractiveElement as _;
        let url = self.url.clone();
        let slot = self.images.update(cx, |cache, cx| cache.slot(&url, cx));
        let box_w = f32::from(window.viewport_size().width);
        // The window already matches the image's aspect ratio, so the
        // picture reads near-fullscreen with a slim margin.
        let body = self.picture.render("image-preview", &url, slot, self.natural, box_w, TALL_BODY_PAD, cx);
        div()
            .id("image-preview")
            .relative()
            .size_full()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if this.picture.key(event, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .child(body)
    }
}

// ---------------------------------------------------------------------------
// EXP-1283 — THE Guide picture viewer
// ---------------------------------------------------------------------------

/// The viewer's fixed side panel width (session-results.json `viewer`).
pub(crate) const GUIDE_VIEWER_PANEL_W: f32 = 360.;

/// The viewer dialog's window title (EXP-1348).
const GUIDE_VIEWER_TITLE: &str = "Results";

/// The Guide viewer's and the gallery's picture margin (each side).
const GUIDE_PICTURE_PAD: f32 = 16.;

/// The viewer's size floor: the panel and a picture both fit.
const GUIDE_VIEWER_MIN_W: f32 = 720.;
const GUIDE_VIEWER_MIN_H: f32 = 440.;

/// The viewer window over `viewport`: 90% wide (≤ 1280), 85% tall (≤ 800),
/// never under the size floor.
fn guide_viewer_window_size(viewport: Size<Pixels>) -> Size<Pixels> {
    let (vw, vh) = (f32::from(viewport.width), f32::from(viewport.height));
    size(
        px((vw * 0.9).min(1280.).max(GUIDE_VIEWER_MIN_W)),
        px((vh * 0.85).min(800.).max(GUIDE_VIEWER_MIN_H)),
    )
}

/// The canonical attachment URL the [`ImageCache`] fetches by.
fn attachment_url(attachment_id: &str) -> String {
    format!("/api/attachments/{attachment_id}")
}

/// Open THE Guide picture viewer on `attachment_id` (EXP-1283, ×4): the
/// picture large on the left, a 360px panel on the right — the topic, the
/// picture's label pill, the topic's text (GFM) and `‹ index / count ›` (the
/// native titlebar closes, EXP-1348); ←/→ walk every Guide picture ([`domain::guide_viewer::guide_viewer`],
/// no wrap), Esc closes. An attachment the Guide does not hold falls back to
/// the plain lightbox.
pub(crate) fn open_guide_viewer(
    source: crate::session_results::GuideViewerSource,
    attachment_id: String,
    images: Entity<ImageCache>,
    window: &mut Window,
    cx: &mut App,
) {
    if domain::guide_viewer::guide_viewer(Some(&source.guide), &attachment_id).is_none() {
        open_image_preview(attachment_url(&attachment_id), String::new(), None, Some(images), window, cx);
        return;
    }
    // EXP-1348: a real dialog window like every other IDE dialog (titlebar
    // strip, movable, resizable down to the panel + a picture), not the
    // chromeless lightbox panel. The title stays put while ←/→ step.
    let spec = DialogSpec::new(GUIDE_VIEWER_TITLE, guide_viewer_window_size(window.viewport_size()))
        .resizable(size(px(GUIDE_VIEWER_MIN_W), px(GUIDE_VIEWER_MIN_H)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| GuideViewer::new(source, attachment_id, images, window, cx));
        DialogContent::new(view).padless()
    });
}

struct GuideViewer {
    source: crate::session_results::GuideViewerSource,
    attachment_id: String,
    images: Entity<ImageCache>,
    picture: LightboxPicture,
    focus_handle: gpui::FocusHandle,
    _images_changed: Subscription,
    _activation: Subscription,
}

impl GuideViewer {
    fn new(
        source: crate::session_results::GuideViewerSource,
        attachment_id: String,
        images: Entity<ImageCache>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let images_changed = cx.observe(&images, |_, _, cx| cx.notify());
        let focus_handle = cx.focus_handle();
        // EXP-1348: a native-chrome dialog window loses the focus claimed
        // here as it becomes key (the shell then takes it), which left ←/→
        // dead: take it back on that edge — once per activation, never per
        // frame — whenever nothing inside the viewer holds it.
        let activation = refocus_on_activation(&focus_handle, window, cx);
        Self {
            source,
            attachment_id,
            images,
            picture: LightboxPicture::new("", cx),
            focus_handle,
            _images_changed: images_changed,
            _activation: activation,
        }
    }

    fn state(&self) -> Option<domain::guide_viewer::GuideViewerState> {
        domain::guide_viewer::guide_viewer(Some(&self.source.guide), &self.attachment_id)
    }

    /// Step to `id` (‹ › and ←/→); `None` = at an end, nothing happens.
    fn go(&mut self, id: Option<String>, cx: &mut gpui::Context<Self>) {
        if let Some(id) = id {
            self.picture.show(&attachment_url(&id), cx);
            self.attachment_id = id;
            cx.notify();
        }
    }

    fn picture(&self, window: &Window, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let url = attachment_url(&self.attachment_id);
        let slot = self.images.update(cx, |cache, cx| cache.slot(&url, cx));
        let box_w = f32::from(window.viewport_size().width) - GUIDE_VIEWER_PANEL_W;
        self.picture.render("guide-viewer", &url, slot, None, box_w, GUIDE_PICTURE_PAD, cx)
    }
}

impl gpui::Focusable for GuideViewer {
    fn focus_handle(&self, _cx: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for GuideViewer {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "GuideViewer").entered();
        use gpui::{InteractiveElement as _, StatefulInteractiveElement as _};
        let Some(state) = self.state() else {
            return div().size_full().into_any_element();
        };
        let (muted, foreground, border, background) = {
            let theme = cx.theme();
            (theme.muted_foreground, theme.foreground, theme.border, theme.background)
        };
        let picture = self.picture(window, cx);
        let (prev, next) = (state.prev_id.clone(), state.next_id.clone());
        let step = |id: &'static str, label: &'static str, glyph: ExpIcon, target: Option<String>, cx: &mut gpui::Context<Self>| {
            let disabled = target.is_none();
            crate::controls::ghost_icon_button(id, Icon::from(glyph), cx)
                .accessibility_label(label)
                .disabled(disabled)
                .on_click(cx.listener(move |this, _, _, cx| this.go(target.clone(), cx)))
        };
        let back = step("guide-viewer-prev", "Previous picture", crate::icons::registry::UI_CHEVRON_LEFT, prev.clone(), cx);
        let forward = step("guide-viewer-next", "Next picture", crate::icons::registry::UI_CHEVRON_RIGHT, next.clone(), cx);
        let text = state.text.clone().map(|text| {
            let mut view = crate::markdown::MarkdownView::new(
                SharedString::from(format!("guide-viewer-text-{}", self.attachment_id)),
                text,
            )
            .selectable(true)
            .autolink(true)
            .on_open_link(crate::links::link_opener(self.source.team_id.as_deref().map(str::to_string)));
            if let Some(team_id) = self.source.team_id.as_deref() {
                view = view.resolver(crate::session_chat::chat_text_resolver(team_id));
            }
            div().w_full().min_w_0().text_sm().text_color(foreground).child(view)
        });
        let panel = v_flex()
            .w(px(GUIDE_VIEWER_PANEL_W))
            .flex_shrink_0()
            .h_full()
            .border_l_1()
            .border_color(border)
            .bg(background)
            .child(
                v_flex()
                    .id("guide-viewer-panel")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_6()
                    .gap_3()
                    // EXP-1348: no in-content × — the native titlebar's close
                    // control is the window's one close affordance (Esc too).
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .text_lg()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(foreground)
                            .child(SharedString::from(state.topic.clone())),
                    )
                    .child(
                        gpui_component::h_flex().child(
                            div()
                                .px_2()
                                .py_0p5()
                                .rounded(px(theme::tokens::radius::MD))
                                .border_1()
                                .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                                .bg(theme::tokens::glass::FILL_ROW.to_hsla())
                                .text_xs()
                                .text_color(foreground)
                                .child(SharedString::from(state.label.clone())),
                        ),
                    )
                    .children(text),
            )
            .child(
                gpui_component::h_flex()
                    .w_full()
                    .px_6()
                    .py_4()
                    .items_center()
                    .justify_between()
                    .child(back)
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(SharedString::from(guide_viewer_position(state.index, state.count))),
                    )
                    .child(forward),
            );
        gpui_component::h_flex()
            .id("guide-viewer")
            .size_full()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "left" => this.go(prev.clone(), cx),
                    "right" => this.go(next.clone(), cx),
                    "escape" => native_dialog::close_dialog_window(window, cx),
                    _ if this.picture.key(event, window, cx) => {}
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .child(div().relative().flex_1().min_w_0().h_full().child(picture))
            .child(panel)
            .into_any_element()
    }
}

// ---------------------------------------------------------------------------
// #EXP-1493 — THE lightbox over a group of pictures (the plan page)
// ---------------------------------------------------------------------------

/// One picture of a [`open_picture_gallery`]: its canonical url (the
/// [`ImageCache`] key) and its alt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GalleryPicture {
    pub url: String,
    pub alt: String,
}

/// THE lightbox over `pictures` in order, opened on `index` (plan-page.json:
/// a press on ANY picture of the plan opens it over EVERY picture): the
/// picture fitted, the guide viewer's `‹ i / n ›` under it, ←/→ step (no
/// wrap), Esc closes. One picture = the plain lightbox.
pub(crate) fn open_picture_gallery(
    pictures: Vec<GalleryPicture>,
    index: usize,
    images: Entity<ImageCache>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(first) = pictures.get(index).cloned() else {
        return;
    };
    if pictures.len() < 2 {
        open_image_preview(first.url, first.alt, None, Some(images), window, cx);
        return;
    }
    let spec = DialogSpec::new(preview_label(&first.alt, &first.url), guide_viewer_window_size(window.viewport_size()))
        .resizable(size(px(GUIDE_VIEWER_MIN_W), px(GUIDE_VIEWER_MIN_H)));
    native_dialog::open_dialog_window(window, cx, spec, move |window, cx| {
        let view = cx.new(|cx| PictureGallery::new(pictures, index, images, window, cx));
        DialogContent::new(view).padless()
    });
}

struct PictureGallery {
    pictures: Vec<GalleryPicture>,
    index: usize,
    images: Entity<ImageCache>,
    picture: LightboxPicture,
    focus_handle: gpui::FocusHandle,
    _images_changed: Subscription,
    _activation: Subscription,
}

impl PictureGallery {
    fn new(
        pictures: Vec<GalleryPicture>,
        index: usize,
        images: Entity<ImageCache>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let images_changed = cx.observe(&images, |_, _, cx| cx.notify());
        let focus_handle = cx.focus_handle();
        // The guide viewer's activation edge (EXP-1348): ←/→ keep working
        // once the dialog window becomes key.
        let activation = refocus_on_activation(&focus_handle, window, cx);
        let picture = LightboxPicture::new("", cx);
        Self { pictures, index, images, picture, focus_handle, _images_changed: images_changed, _activation: activation }
    }

    /// Step by `delta`; an end does nothing.
    fn step(&mut self, delta: isize, cx: &mut gpui::Context<Self>) {
        let next = self.index as isize + delta;
        if next >= 0 && (next as usize) < self.pictures.len() {
            self.index = next as usize;
            self.picture.show(&self.pictures[self.index].url, cx);
            cx.notify();
        }
    }
}

impl gpui::Focusable for PictureGallery {
    fn focus_handle(&self, _cx: &App) -> gpui::FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for PictureGallery {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "PictureGallery").entered();
        use gpui::InteractiveElement as _;
        let Some(picture) = self.pictures.get(self.index).cloned() else {
            return div().size_full().into_any_element();
        };
        let muted = cx.theme().muted_foreground;
        let slot = self.images.update(cx, |cache, cx| cache.slot(&picture.url, cx));
        let box_w = f32::from(window.viewport_size().width);
        let body = self.picture.render("picture-gallery", &picture.url, slot, None, box_w, GUIDE_PICTURE_PAD, cx);
        let count = self.pictures.len();
        let back = crate::controls::ghost_icon_button("picture-gallery-prev", Icon::from(crate::icons::registry::UI_CHEVRON_LEFT), cx)
            .accessibility_label("Previous picture")
            .disabled(self.index == 0)
            .on_click(cx.listener(|this, _, _, cx| this.step(-1, cx)));
        let forward = crate::controls::ghost_icon_button("picture-gallery-next", Icon::from(crate::icons::registry::UI_CHEVRON_RIGHT), cx)
            .accessibility_label("Next picture")
            .disabled(self.index + 1 >= count)
            .on_click(cx.listener(|this, _, _, cx| this.step(1, cx)));
        v_flex()
            .id("picture-gallery")
            .size_full()
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "left" => this.step(-1, cx),
                    "right" => this.step(1, cx),
                    "escape" => native_dialog::close_dialog_window(window, cx),
                    _ if this.picture.key(event, window, cx) => {}
                    _ => return,
                }
                cx.stop_propagation();
            }))
            .child(div().relative().flex_1().min_h_0().w_full().child(body))
            .child(
                gpui_component::h_flex()
                    .flex_shrink_0()
                    .w_full()
                    .justify_center()
                    .items_center()
                    .gap_4()
                    .pb_4()
                    .child(back)
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child(SharedString::from(guide_viewer_position(self.index + 1, count))),
                    )
                    .child(forward),
            )
            .into_any_element()
    }
}

/// The panel's `index / count`.
pub(crate) fn guide_viewer_position(index: usize, count: usize) -> String {
    format!("{index} / {count}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_guide_viewer_reads_its_position_and_fits_the_window() {
        assert_eq!(guide_viewer_position(2, 3), "2 / 3");
        let window = guide_viewer_window_size(size(px(1600.), px(1000.)));
        assert_eq!(window, size(px(1280.), px(800.)));
        let small = guide_viewer_window_size(size(px(600.), px(400.)));
        assert_eq!(small, size(px(720.), px(440.)));
        assert!(f32::from(window.width) > GUIDE_VIEWER_PANEL_W * 2.);
    }

    #[test]
    fn preview_labels_fall_back_alt_then_filename_then_generic() {
        assert_eq!(preview_label(" shot ", "/api/attachments/a.png"), "shot");
        // The editor's synthetic "image" alt is not a real label.
        assert_eq!(
            preview_label("image", "/api/attachments/a-photo.png?w=1"),
            "a-photo.png"
        );
        assert_eq!(preview_label("", "/api/attachments/xyz"), "xyz");
        assert_eq!(preview_label("", ""), "Image");
    }

    #[test]
    fn preview_window_matches_image_aspect_within_viewport() {
        let viewport = size(px(2000.), px(1000.));
        // Landscape image fits the height budget: 900*0.9-44=856 tall.
        let sized = preview_window_size(Some((1600., 1200.)), viewport);
        let scale: f32 = (1000. * 0.9 - PREVIEW_HEADER_H) / 1200.;
        assert_eq!(f32::from(sized.width), 1600. * scale);
        assert_eq!(f32::from(sized.height), 1200. * scale + PREVIEW_HEADER_H);
    }

    #[test]
    fn preview_window_never_upscales_and_floors() {
        let viewport = size(px(2000.), px(1000.));
        // A small image keeps scale 1 and floors to the 480×320 minimum.
        let sized = preview_window_size(Some((200., 100.)), viewport);
        assert_eq!(f32::from(sized.width), 480.);
        assert_eq!(f32::from(sized.height), 320.);
    }

    #[test]
    fn preview_window_for_a_tall_image_is_a_scroll_column() {
        let viewport = size(px(2000.), px(1000.));
        // 780×25094: natural width inside the band, 90% of the height.
        let sized = preview_window_size(Some((780., 25094.)), viewport);
        assert_eq!(f32::from(sized.width), 780.);
        assert_eq!(f32::from(sized.height), 900.);
        // Wide-but-tall caps at 1100, narrow floors at 480.
        let sized = preview_window_size(Some((3000., 12000.)), viewport);
        assert_eq!(f32::from(sized.width), 1100.);
        let sized = preview_window_size(Some((200., 5000.)), viewport);
        assert_eq!(f32::from(sized.width), 480.);
        // Exactly 1/3 is NOT tall (strict, the shared rule).
        assert!(!is_tall(Some((300., 900.))));
        assert!(is_tall(Some((299., 900.))));
        assert!(!is_tall(None));
    }

    /// EXP-1586: the gallery's zoom keys reach the zoom view while the
    /// gallery holds focus, and a page (→) opens the next picture at rest.
    #[gpui::test]
    async fn the_gallery_zooms_by_key_and_pages_back_to_rest(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let images = cx.new(|_| {
            let mut cache = ImageCache::new(None);
            for (id, from, to) in [("a", [0x3f, 0x3f, 0x46], [0x52, 0x52, 0x5b]), ("b", [0x1e, 0x3a, 0x5f], [0x3b, 0x82, 0xf6])] {
                let png = crate::styleguide::entries::plan_card::picture_png(from, to, 800, 500);
                cache.insert_bytes(attachment_url(id), "image/png", png);
            }
            cache
        });
        let pictures = ["a", "b"]
            .map(|id| GalleryPicture { url: attachment_url(id), alt: id.to_string() })
            .to_vec();
        let (gallery, cx) =
            cx.add_window_view(|window, cx| PictureGallery::new(pictures, 0, images, window, cx));
        cx.run_until_parked();
        cx.update(|window, cx| window.refresh());
        cx.run_until_parked();
        let zoom = gallery.read_with(cx, |gallery, _| gallery.picture.zoom.clone());
        assert!(zoom.read_with(cx, |zoom, _| zoom.at_rest()));
        cx.simulate_keystrokes("=");
        cx.run_until_parked();
        assert!(!zoom.read_with(cx, |zoom, _| zoom.at_rest()), "= zooms in");
        cx.simulate_keystrokes("right");
        cx.run_until_parked();
        assert_eq!(gallery.read_with(cx, |gallery, _| gallery.index), 1);
        assert!(zoom.read_with(cx, |zoom, _| zoom.at_rest()), "a page opens at rest");
    }

    #[test]
    fn preview_window_without_dims_keeps_viewport_share() {
        let viewport = size(px(2000.), px(1000.));
        let sized = preview_window_size(None, viewport);
        assert_eq!(f32::from(sized.width), 1100.);
        assert_eq!(f32::from(sized.height), 800.);
    }
}
