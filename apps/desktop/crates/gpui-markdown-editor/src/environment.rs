use std::path::PathBuf;
use std::sync::Arc;

use crate::host::{
    ImagePasteHandler, ImageSourceResolver, ReferenceDecorator, default_image_paste_handler,
};
use crate::strings::I18nStrings;
use crate::theme::Theme;

/// Presentation and resource state owned by one editor instance.
///
/// The component never reads Velotype application globals. Every block shares
/// this immutable snapshot and receives a replacement when the host changes a
/// setting.
#[derive(Clone)]
pub struct MarkdownEditorEnvironment {
    pub theme: Arc<Theme>,
    pub strings: Arc<I18nStrings>,
    pub document_base_dir: Option<PathBuf>,
    pub show_source_line_numbers: bool,
    pub show_table_headers: bool,
    pub image_paste_handler: Arc<dyn ImagePasteHandler>,
    /// EXP-261 vendoring: optional host hook for image-source resolution
    /// (authenticated attachment fetching, staged paste bytes).
    pub image_source_resolver: Option<Arc<dyn ImageSourceResolver>>,
    /// EXP-261 vendoring: optional host hook decorating `@email` / `#IDENT`
    /// tokens as pills.
    pub reference_decorator: Option<Arc<dyn ReferenceDecorator>>,
    /// EXP-261 vendoring: show the drag handle that resizes standalone
    /// images (host persists the width via `MarkdownEditorEvent::ImageResized`).
    pub enable_image_resize: bool,
    /// EXP-261 vendoring: whether clicking a rendered standalone image swaps
    /// it for its raw `![](...)` markdown so the caret can edit it. Hosts that
    /// give images their own overlay controls (menu + resize handles) turn
    /// this OFF — a click anywhere in the block would otherwise replace the
    /// picture with source text, which reads as the image having vanished.
    pub enable_image_source_editing: bool,
    /// EXP-261 vendoring: whether a focused `[text](url)` expands to its
    /// markdown source so the target can be typed over. Hosts with a toolbar
    /// link control turn this OFF, so a link keeps rendering as a link with
    /// the caret inside it (web parity). Reference links and autolinks expand
    /// either way — their source spelling is preserved verbatim, so it has to
    /// stay reachable.
    pub expand_focused_links: bool,
    /// EXP-335: the editor's last PAINTED content width (f32 bits; 0 = not
    /// measured yet), recorded each frame by the editor render's
    /// width-recorder canvas. The `Arc` is shared across environment clones,
    /// so every block reads the same cell. Embedded editors fill their host
    /// slot — no viewport-derived math can know that width — and standalone
    /// image rows cap their width budget with it (an oversized budget hands
    /// the picture a too-tall layout box that `Contain` letterboxes).
    pub layout_width: Arc<std::sync::atomic::AtomicU32>,
}

impl Default for MarkdownEditorEnvironment {
    fn default() -> Self {
        Self {
            theme: Arc::new(Theme::default_theme()),
            strings: Arc::new(I18nStrings::en_us()),
            document_base_dir: None,
            show_source_line_numbers: true,
            show_table_headers: true,
            image_paste_handler: default_image_paste_handler(),
            image_source_resolver: None,
            reference_decorator: None,
            enable_image_resize: false,
            enable_image_source_editing: true,
            expand_focused_links: true,
            layout_width: Arc::new(std::sync::atomic::AtomicU32::new(0)),
        }
    }
}
