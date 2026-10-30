//! The read-only markdown view (VAPP-90): gpui-component's `TextView` with
//! the glass code-block recipe.
//!
//! The IDE's full `MarkdownView` (the editor with issue chips, attachment
//! transport and the image cache, `crates/ui/src/markdown/`) is app-specific
//! and stays in the IDE; what is generic — the `TextViewStyle` recipes and a
//! read-only view on them — lives here.

use gpui::{px, App, ElementId, SharedString, StyleRefinement, Styled as _};
use gpui_component::text::{TextView, TextViewStyle};

use crate::chrome::Chrome;

/// Shared markdown `TextView` style (EXP-282): code blocks get the glass
/// section fill (`fill_section`, `radius_md`) instead of the component's
/// default opaque `tokens.muted` panel. Everything else stays at the
/// component defaults.
pub fn markdown_style(chrome: &Chrome) -> TextViewStyle {
    let code_block = StyleRefinement::default()
        .bg(chrome.fill_section)
        .rounded(px(chrome.radius_md));
    TextViewStyle::default().code_block(code_block)
}

/// Markdown `TextView` style for a file viewer (EXP-282): the whole file is
/// fenced as one code block, so the block chrome disappears entirely — no
/// fill, no padding, no radius — and the code sits directly on the ground.
pub fn bare_code_markdown_style() -> TextViewStyle {
    let code_block = StyleRefinement::default()
        .bg(gpui::transparent_black())
        .p_0()
        .rounded_none();
    TextViewStyle::default().code_block(code_block)
}

/// The SDK's read-only markdown view: `text` rendered as GFM through
/// gpui-component's `TextView::markdown`, on [`markdown_style`] with the
/// installed chrome, selectable. Returns the `TextView` so a caller can chain
/// more (`.selection_format(…)`, a text size).
pub fn markdown_view(id: impl Into<ElementId>, text: impl Into<SharedString>, cx: &App) -> TextView {
    TextView::markdown(id, text)
        .style(markdown_style(Chrome::global(cx)))
        .selectable(true)
}
