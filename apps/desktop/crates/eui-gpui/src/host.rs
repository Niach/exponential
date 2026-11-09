//! What an embedding app plugs into the painter: icons, the action/input
//! sinks, URL handling, font mapping and an optional richer markdown view.
//! Every method has a default, so [`NoHost`] is a working (if silent) host.

use eui_core::host::{media_request, safe_href, MediaOptions, MediaRequest, UrlPolicy};
use eui_core::measure::TextStyle;
use eui_core::surface::PlacedNode;

/// A painter that failed (`catalog/host.json` paint): it paints an empty
/// box; [`HostPlugin::on_paint_error`] hears it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaintError {
    pub surface_id: String,
    pub component_id: String,
    pub message: String,
}

/// #VAPP-258 (`catalog/host.json` markdown): a host's `attachment(href)`
/// answer: `kind` in `MARKDOWN_ATTACHMENT_KINDS` (`video`, `audio`, `file`;
/// anything else, or an empty `name`, keeps the paragraph).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct MarkdownAttachment {
    pub kind: String,
    pub name: String,
}

/// #VAPP-258 step 5 (`catalog/host.json` markdown.options): how this host's
/// Markdowns render. The default = what a Markdown painted before them.
/// `hard_breaks` = `breaks: hard` (every newline inside a paragraph, list
/// item or quote paragraph breaks the line); `chat` = `density: chat` (the
/// `Markdown/*` recipes resolved with `density: chat`: the theme's chat
/// rhythm); `selectable` = whether press-drag selects and copies (`None` =
/// gpui's default, it does); `streaming` = the text is a growing prefix (its
/// stable blocks memoized, its unfinished tail held back); `image_max_height`
/// = a block image's height cap (#VAPP-258, [`ImageCap`]).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MarkdownOptions {
    pub hard_breaks: bool,
    pub chat: bool,
    pub selectable: Option<bool>,
    pub streaming: bool,
    pub image_max_height: ImageCap,
}

/// #VAPP-258 (`markdown.options.imageMaxHeight`): a block image's height cap.
/// `Catalog` = unset (`markdown.imageMaxHeight`, 160); `None` = no cap (the
/// width at the image's aspect, at most the window's height); `Height(px)`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ImageCap {
    #[default]
    Catalog,
    None,
    Height(f32),
}

impl MarkdownOptions {
    /// The cap as applied: `None` = none, an invalid height = the catalog's.
    pub fn image_cap(&self) -> Option<f32> {
        match self.image_max_height {
            ImageCap::None => None,
            ImageCap::Height(h) if h.is_finite() && h > 0.0 => Some(h),
            _ => Some(eui_core::host::MARKDOWN_IMAGE_MAX_HEIGHT as f32),
        }
    }
}

/// A server event (A2UI `action`) the surface fired.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionEvent {
    pub surface_id: String,
    /// The interaction that fired it (`press`, `change`, `select`, `submit`…).
    pub event: String,
    /// The action's `event.name`.
    pub name: String,
    pub component_id: String,
    /// The action's resolved `event.context`.
    pub context: serde_json::Value,
    pub payload: Option<serde_json::Value>,
    /// Round 9 §1.3: the action's resolved `event.userMessage`.
    pub user_message: Option<String>,
}

/// An `on.<event> = {functionCall: {call, args}}` naming a HOST function
/// (not one of the catalog's built-ins): the host gates it through its
/// function policy and runs its registered handler
/// ([`crate::runtime::ExponentialHost::call_function`]).
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionCallEvent {
    pub surface_id: String,
    pub component_id: String,
    pub name: String,
    /// The call's args, resolved against the data model and scope.
    pub args: serde_json::Value,
}

/// A host-owned text edit: `Change` while typing (debounced), `Commit` on
/// blur / Enter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    Change,
    Commit,
}

/// One host-owned input edit. `revision` increases monotonically per field;
/// an echo the host writes back is applied only when it is not stale.
#[derive(Debug, Clone, PartialEq)]
pub struct InputEvent {
    pub surface_id: String,
    pub component_id: String,
    pub name: String,
    /// The data-model path the value is bound to, if any.
    pub path: Option<String>,
    pub value: serde_json::Value,
    pub revision: u64,
    pub kind: InputKind,
}

/// VAPP-271 (`catalog/host.json` media.appFile): a host's own app files, the
/// bytes an `app-file:<fileId>` src names (the Engine runner: its
/// BlobStore). Asked OFF the main thread by the media loader (so `Send +
/// Sync`), under the image limits; `None` = no such file (the fallback
/// paints). A closure `Fn(&str) -> Option<Vec<u8>>` is one.
pub trait AppFiles: Send + Sync + 'static {
    fn read(&self, file_id: &str) -> Option<Vec<u8>>;
}

impl<F: Fn(&str) -> Option<Vec<u8>> + Send + Sync + 'static> AppFiles for F {
    fn read(&self, file_id: &str) -> Option<Vec<u8>> {
        self(file_id)
    }
}

/// The embedding app's side of the painter.
pub trait HostPlugin: 'static {
    /// Round 9 §5.2, §6.8: a layout left issues or reactive failures on the
    /// surface: the host sends them (`CallHub::flush_surface`).
    fn after_layout(&self, _surface_id: &str, _cx: &mut gpui::App) {}

    /// An icon registry CONCEPT name (`ui-send`, `nav-inbox`…) → an SVG asset
    /// path for gpui `svg().path(..)`. `None` = the painter's built-in chrome
    /// glyph when it has one, else the placeholder circle.
    fn icon(&self, _name: &str) -> Option<gpui::SharedString> {
        None
    }

    /// A server event fired (forward it to the producer).
    fn on_action(&self, _event: &ActionEvent, _cx: &mut gpui::App) {}

    /// #EXP-1725: the pointer entered or left `component_id` (a pressable
    /// node, a template instance's id). What a row reveals on hover is the
    /// host's data.
    fn on_hover(&self, _component_id: &str, _hovered: bool, _cx: &mut gpui::App) {}

    /// #EXP-1725: the host paints the icon called `name` itself (a mark that
    /// animates, a tint the row's data decides); `None` = [`Self::icon`].
    fn icon_element(&self, _name: &str, _size: f32, _color: gpui::Hsla, _cx: &gpui::App) -> Option<gpui::AnyElement> {
        None
    }

    /// #EDE-65 lane O: the host paints the Embed `component_id` itself (a
    /// row it owns that no catalog component draws: the IDE's tool rows and
    /// Work card in the shared chat), by its painted id like
    /// [`Self::icon_element`] by name. `Some(height)` at `width` claims it:
    /// no nested surface is made and [`Self::host_leaf`] fills its frame;
    /// `None` = the Embed's own nested surface. A claimed leaf whose height
    /// moves asks for a new measure with `SurfaceView::host_leaf_resized`.
    fn host_leaf_height(&self, _component_id: &str, _width: f32, _window: &mut gpui::Window, _cx: &mut gpui::App) -> Option<f32> {
        None
    }

    /// The claimed Embed's element filling its frame (asked only for an id
    /// [`Self::host_leaf_height`] claimed).
    fn host_leaf(&self, _component_id: &str, _window: &mut gpui::Window, _cx: &mut gpui::App) -> Option<gpui::AnyElement> {
        None
    }

    /// #EDE-65 lane O: before the claimed leaf `component_id` paints at
    /// `width` (prepaint: outside any layout, so the host may lay its own
    /// element out): bring the height [`Self::host_leaf_height`] answers up
    /// to date. `true` = it changed: the surface lays out again in the SAME
    /// frame, so the leaf never paints at a stale height. Default: nothing.
    fn host_leaf_prepaint(&self, _component_id: &str, _width: f32, _window: &mut gpui::Window, _cx: &mut gpui::App) -> bool {
        false
    }

    /// A host function call (`functionCall` to a non-built-in name). The
    /// default ignores it; [`crate::runtime::host_plugin`] routes it through
    /// an [`crate::runtime::ExponentialHost`] (policy gate, consent, handler).
    fn on_function_call(&self, _event: &FunctionCallEvent, _cx: &mut gpui::App) {}

    /// Round 9 §6.8: an action with both: the function first, `then` (its
    /// event) only when the function did not fail.
    fn on_function_call_then(&self, event: &FunctionCallEvent, then: Box<dyn FnOnce(&mut gpui::App)>, cx: &mut gpui::App) {
        self.on_function_call(event, cx);
        then(cx);
    }

    /// The URL policy EVERY href passes (Link, markdown links, `openUrl`):
    /// `catalog/host.json` urls. `None` = the default schemes, no base.
    fn url_policy(&self) -> Option<UrlPolicy> {
        None
    }

    /// The media policy every src passes (schemes, hosts, base, header
    /// rules): `catalog/host.json` media. Default = https/http/data, no
    /// local files.
    fn media_options(&self) -> MediaOptions {
        MediaOptions::default()
    }

    /// A host's deny-only narrowing of the media policy, asked with the RAW
    /// `src` before any resolution: `false` = nothing loads (the denied-src
    /// rendering), `true` = the policy decides as usual. It can never widen
    /// the policy. Default: `true` (the generic SDK policy is unchanged).
    fn media_allowed(&self, _src: &str) -> bool {
        true
    }

    /// Map an `Image`/`Video`/`Avatar`/markdown image `src` to the request
    /// the image loader makes (absolute url + headers, e.g. auth for
    /// `/api/attachments`). `None` = denied: nothing loads. The default:
    /// [`Self::media_allowed`], then `resolve_url(src)` through
    /// [`Self::media_options`]. Whatever this returns is re-checked against
    /// `media_options()`'s schemes, hosts and private opt-ins.
    fn media_request(&self, src: &str) -> Option<MediaRequest> {
        if !self.media_allowed(src) {
            return None;
        }
        media_request(&self.resolve_url(src), &self.media_options())
    }

    /// A host-owned text edit: `Change` debounced 150 ms with a monotonically
    /// increasing revision, `Commit` on blur / Enter.
    fn on_input(&self, _event: &InputEvent, _cx: &mut gpui::App) {}

    /// `openUrl` or a `Link` press. The default opens only what
    /// [`Self::url_policy`] allows; an override must apply it too.
    fn open_url(&self, url: &str, cx: &mut gpui::App) {
        if let Some(href) = safe_href(self.url_policy().as_ref(), url) {
            cx.open_url(&href)
        }
    }

    /// #EXP-1674: the text of a `hostLinks` Text (the `Row` subtitle) as
    /// segments that concatenate back to it; `None` = no links. eUI knows
    /// nothing of what they mean; [`eui_core::host::police_text_links`] applies.
    fn text_links(&self, _text: &str, _cx: &gpui::App) -> Option<Vec<eui_core::host::TextLink>> {
        None
    }

    /// #VAPP-258 (`catalog/host.json` markdown): a Markdown block image's
    /// natural `(width, height)`, asked with the RAW src by the measurer and
    /// the painter alike (answer purely): the box keeps that aspect ratio,
    /// capped by the available width and `imageMaxHeight`. `None` (or a
    /// non-positive or non-finite size) = the fixed box.
    fn image_size(&self, _src: &str, _cx: &gpui::App) -> Option<(f32, f32)> {
        None
    }

    /// #VAPP-258: does a press on a Markdown block image call
    /// [`Self::on_image_press`]? Default `false`: not pressable.
    fn image_pressable(&self, _cx: &gpui::App) -> bool {
        false
    }

    /// #VAPP-258: a Markdown block image was pressed (the RAW src): a
    /// lightbox. Only asked when [`Self::image_pressable`].
    fn on_image_press(&self, _src: &str, _window: &mut gpui::Window, _cx: &mut gpui::App) {}

    /// #VAPP-258: a top-level Markdown paragraph that is exactly ONE link
    /// (asked with its RAW destination, before the text links) becomes a
    /// video / audio / file block when the href passes the URL policy and
    /// this classifies it. `None` = the paragraph stays.
    fn attachment(&self, _href: &str, _cx: &gpui::App) -> Option<MarkdownAttachment> {
        None
    }

    /// #VAPP-258 (gpui host extra): a version of this host's Markdown answers
    /// for `text` (text links, chips, image sizes, attachments, options): the
    /// same version = the same answers, so an unchanged paint reuses the
    /// laid-out document. `None` (the default) = ask again every paint.
    fn markdown_hooks_version(&self, _text: &str, _cx: &gpui::App) -> Option<u64> {
        None
    }

    /// #VAPP-258 (gpui host extra): whether a long Markdown builds only the
    /// blocks near the host's visible region (the surface's whole box scrolls
    /// in the host, no scroller inside it). Default `false`: every block is
    /// built each frame (only those crossing the clip lay out and paint).
    fn markdown_visible_only(&self, _cx: &gpui::App) -> bool {
        false
    }

    /// #VAPP-258 (gpui host extra): the host's WINDOW-level text selection
    /// selects Markdown text (gpui-base's `TextSelectionLayer`, mounted by
    /// the host's root): a press-drag sweeps across every Markdown leaf of
    /// every surface in the window in document order, the host's copy gives
    /// the whole range. The leaf's own selection is off then. Default `false`.
    fn markdown_window_selection(&self, _cx: &gpui::App) -> bool {
        false
    }

    /// #VAPP-258 (gpui host extra): a Markdown inline chip's icon `name`
    /// (the [`eui_core::host::TextLinkChip`] icon) at `size` in the chip's
    /// label `color`, painted by the host (an issue's status glyph in its own
    /// tint). `None` = the icon registry ([`Self::icon`]). The box is the
    /// chip's icon slot whatever this returns (measure = paint).
    fn chip_icon(&self, _name: &str, _size: f32, _color: gpui::Hsla) -> Option<gpui::AnyElement> {
        None
    }

    /// #VAPP-258 (gpui host extra): does a press on a Markdown task box call
    /// [`Self::on_task_toggle`]? Default `false`: the box is inert.
    fn tasks_pressable(&self, _cx: &gpui::App) -> bool {
        false
    }

    /// #VAPP-258: the `ordinal`-th task item of the Markdown (document order,
    /// 0-based) was pressed; `checked` = its state BEFORE the press. Only
    /// asked when [`Self::tasks_pressable`]; the host rewrites its text.
    fn on_task_toggle(&self, _ordinal: usize, _checked: bool, _window: &mut gpui::Window, _cx: &mut gpui::App) {}

    /// #VAPP-258 step 5: the Markdown render options (`catalog/host.json`
    /// markdown.options), read by the measurer and the painter alike.
    fn markdown_options(&self, _cx: &gpui::App) -> MarkdownOptions {
        MarkdownOptions::default()
    }

    /// #EDE-65 lane O (gpui host extra): the Markdown render options for ONE
    /// text (a host whose surface shows many Markdowns answers per text: the
    /// one reply still streaming takes `streaming`). Read by the measurer and
    /// the painter alike. Default: [`Self::markdown_options`].
    fn markdown_options_for(&self, _text: &str, cx: &gpui::App) -> MarkdownOptions {
        self.markdown_options(cx)
    }

    /// A `Video` / `AudioPlayer` press hands its policed source to the
    /// system player (gpui has no media pipeline, so nothing plays inline):
    /// an http(s) src without headers goes through [`Self::open_url`]; a
    /// request with headers or a `data:` url is fetched under the media
    /// limits into a temporary file, and that file (or a host-allowed
    /// `file:` src) arrives here. Default: the system opener.
    fn open_media_file(&self, path: &std::path::Path, cx: &mut gpui::App) {
        cx.open_with_system(path)
    }

    /// `onPaintError` (`catalog/host.json` paint): a component's painter
    /// failed (it panicked) and painted an empty box.
    /// [`crate::runtime::host_plugin`] forwards it to the agent as an A2UI
    /// `RENDER_FAILED` error.
    fn on_paint_error(&self, _error: &PaintError, _cx: &mut gpui::App) {}

    /// A component whose painter failed got new props: it paints again (a
    /// new failure reports again).
    fn on_paint_retry(&self, _surface_id: &str, _component_id: &str, _cx: &mut gpui::App) {}

    /// An `Unknown` placeholder was painted (once per structure version).
    fn on_unknown(&self, _node: &PlacedNode) {}

    /// VAPP-281 (round-7 contract): with `SurfaceSettings::hosted_popups`,
    /// the anchored Popover / Menu layers the HOST shows outside the surface
    /// (eDE: an `xdg_popup` per layer; a desktop: a popup window), heard
    /// whenever the set or its geometry changes (empty = all closed). Each
    /// [`eui_core::surface::Layer`] carries `anchor_frame` (surface
    /// coordinates), `placement.side` + `align`, and its root frame (the
    /// popup's unclipped size, its origin where the core would put it). The
    /// view paints none of them; the host renders
    /// [`crate::view::SurfaceView::hosted_popup_element`] in its popup and
    /// dismisses through [`crate::view::SurfaceView::dismiss_hosted_popup`].
    fn on_hosted_popups(&self, _layers: &[eui_core::surface::Layer]) {}

    /// VAPP-271: the app files an `app-file:<fileId>` src reads from,
    /// resolved BEFORE the media policy (it is no URL). Default `None`: a
    /// host without app files leaves such a src unresolved (nothing loads,
    /// the missing-image fallback paints).
    fn app_files(&self) -> Option<std::sync::Arc<dyn AppFiles>> {
        None
    }

    /// Rewrite a media `src` BEFORE the media policy (signed URLs).
    fn resolve_url(&self, src: &str) -> String {
        src.to_string()
    }

    /// A theme font family NAME (`"Inter"`) → the family gpui loads. The app
    /// registers the font files itself (`cx.text_system().add_fonts`).
    fn font_family(&self, family: &str) -> gpui::SharedString {
        family.to_string().into()
    }

    /// Speak `text` through the platform screen reader (`live` = `polite`
    /// or `assertive`): Form errors, CodeBlock `copied`, `announce`
    /// commands, live regions. The painter also exposes the latest
    /// announcement as a `status` node; gpui has no live-region API.
    fn announce(&self, _text: &str, _live: &str, _cx: &mut gpui::App) {}

    /// Round 2 §5: scroll the host's scroller (the one showing the WHOLE
    /// surface) so the surface's `(x, y)` sits at its top-left — a
    /// `scrollToIndex` on a list that windows against the host viewport.
    /// The host then reports its offset back through the visible region.
    fn scroll_surface(&self, _x: f32, _y: f32, _cx: &mut gpui::App) {}

    /// VAPP-162 (round-5 contract §7): THE CAMERA of a QrScanner is the
    /// host's. Asked on every paint of an enabled scanner `component_id`
    /// with the code `formats` it wants (default `["qr"]`): return the live
    /// feed element (painted filling the viewfinder, under the corner
    /// brackets and the sweeping scan line), or `None` = no camera (the
    /// placeholder: `ui-scan` + `cameraUnavailable`). The host decodes codes
    /// itself and hands each decoded string to
    /// [`crate::view::SurfaceView::qr_scanned`], which fires `scanned
    /// {value}` once per distinct value. Default: no camera (the desktop has
    /// no camera host yet).
    fn qr_scanner_feed(&self, _component_id: &str, _formats: &[String], _window: &mut gpui::Window, _cx: &mut gpui::App) -> Option<gpui::AnyElement> {
        None
    }

    /// Open a file picker for a FileUpload. Return `true` when the host
    /// handles it (and later calls `SurfaceView::files_picked`); `false` =
    /// the painter opens the platform picker itself.
    fn pick_files(&self, _request: &FilePickRequest, _cx: &mut gpui::App) -> bool {
        false
    }

    /// Picked or dropped files reached a FileUpload: read and upload the
    /// bytes (the surface's `upload` event carries only name/size/type).
    fn on_upload(&self, _event: &UploadEvent, _cx: &mut gpui::App) {}

    /// A richer markdown renderer for `Markdown` leaves; `None` = the
    /// built-in block painter (whose height the measurer predicts exactly).
    fn markdown(
        &self,
        _text: &str,
        _text_style: &TextStyle,
        _width: f32,
        _window: &mut gpui::Window,
        _cx: &mut gpui::App,
    ) -> Option<gpui::AnyElement> {
        None
    }
}

/// A FileUpload's request for the platform file picker.
#[derive(Debug, Clone, PartialEq)]
pub struct FilePickRequest {
    pub surface_id: String,
    pub component_id: String,
    /// The `accept` filter (`image/*,.pdf`), when set.
    pub accept: Option<String>,
    pub multiple: bool,
}

/// Files the user picked or dropped on a FileUpload: the paths (the BYTES
/// stay with the host, never in an event) and the metadata the surface's
/// `upload {files}` event carries.
#[derive(Debug, Clone, PartialEq)]
pub struct UploadEvent {
    pub surface_id: String,
    pub component_id: String,
    /// The field's `name`.
    pub name: String,
    pub paths: Vec<std::path::PathBuf>,
}

/// The silent default host.
pub struct NoHost;

impl HostPlugin for NoHost {}
