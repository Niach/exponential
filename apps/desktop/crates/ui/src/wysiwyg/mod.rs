//! EXP-261: host integration of the vendored WYSIWYG markdown editor
//! (`gpui-markdown-editor`, Velotype core — see that crate's NOTICE).
//!
//! The block editor in [`crate::markdown`] stays compiled (comment composer
//! still uses it); the one-line revert lever is in
//! `description_editor.rs::install`.

mod description;
mod images;
mod refs;
mod theme_bridge;
mod toolbar;

pub(crate) use description::{OnSave, WysiwygDescription};
pub(crate) use theme_bridge::editor_theme_with_placeholder;

/// The vendored editor's own per-block horizontal padding
/// (`gpui-markdown-editor` `dimensions.block_padding_x`, unchanged by the
/// theme bridge). Host slots subtract it from their gutters and the toolbar
/// compensates its icon-button inset against it (EXP-285) so the format-bar
/// glyphs, the title and the description text all share one left edge.
pub(crate) const WYSIWYG_BLOCK_PADDING_X: f32 = 12.;
