//! What an embedding app plugs into the painter: icons, the action/input
//! sinks, URL handling, font mapping and an optional richer markdown view.
//! Every method has a default, so [`NoHost`] is a working (if silent) host.

use exponential_ui::host::{media_request, safe_href, MediaOptions, MediaRequest, UrlPolicy};
use exponential_ui::measure::TextStyle;
use exponential_ui::surface::PlacedNode;

/// A painter that failed (`catalog/host.json` paint): it paints an empty
/// box; [`HostPlugin::on_paint_error`] hears it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaintError {
    pub surface_id: String,
    pub component_id: String,
    pub message: String,
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

/// The embedding app's side of the painter.
pub trait HostPlugin: 'static {
    /// An icon registry CONCEPT name (`ui-send`, `nav-inbox`…) → an SVG asset
    /// path for gpui `svg().path(..)`. `None` = the painter's built-in chrome
    /// glyph when it has one, else the placeholder circle.
    fn icon(&self, _name: &str) -> Option<gpui::SharedString> {
        None
    }

    /// A server event fired (forward it to the producer).
    fn on_action(&self, _event: &ActionEvent, _cx: &mut gpui::App) {}

    /// A host function call (`functionCall` to a non-built-in name). The
    /// default ignores it; [`crate::runtime::host_plugin`] routes it through
    /// an [`crate::runtime::ExponentialHost`] (policy gate, consent, handler).
    fn on_function_call(&self, _event: &FunctionCallEvent, _cx: &mut gpui::App) {}

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

    /// Map an `Image`/`Video`/`Avatar`/markdown image `src` to the request
    /// the image loader makes (absolute url + headers, e.g. auth for
    /// `/api/attachments`). `None` = denied: nothing loads. The default:
    /// `resolve_url(src)` through [`Self::media_options`]. Whatever this
    /// returns is re-checked against `media_options()`'s schemes and hosts.
    fn media_request(&self, src: &str) -> Option<MediaRequest> {
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
