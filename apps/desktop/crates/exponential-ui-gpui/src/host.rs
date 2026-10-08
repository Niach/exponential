//! What an embedding app plugs into the painter: icons, the action/input
//! sinks, URL handling, font mapping and an optional richer markdown view.
//! Every method has a default, so [`NoHost`] is a working (if silent) host.

use exponential_ui::measure::TextStyle;
use exponential_ui::surface::PlacedNode;

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

    /// A host-owned text edit: `Change` debounced 150 ms with a monotonically
    /// increasing revision, `Commit` on blur / Enter.
    fn on_input(&self, _event: &InputEvent, _cx: &mut gpui::App) {}

    /// `openUrl` or a `Link` press.
    fn open_url(&self, url: &str, cx: &mut gpui::App) {
        cx.open_url(url)
    }

    /// An `Unknown` placeholder was painted (once per structure version).
    fn on_unknown(&self, _node: &PlacedNode) {}

    /// Map an `Image`/`Video`/`Avatar` `src` before loading it.
    fn resolve_url(&self, src: &str) -> String {
        src.to_string()
    }

    /// A theme font family NAME (`"Inter"`) → the family gpui loads. The app
    /// registers the font files itself (`cx.text_system().add_fonts`).
    fn font_family(&self, family: &str) -> gpui::SharedString {
        family.to_string().into()
    }

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

/// The silent default host.
pub struct NoHost;

impl HostPlugin for NoHost {}
