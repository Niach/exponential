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
