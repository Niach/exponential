//! Reusable native Markdown editor component for GPUI.

mod actions;
mod api;
mod components;
mod editor;
mod environment;
mod host;
mod strings;
mod theme;
mod theme_config;

pub use actions::*;
pub use api::*;
pub use editor::Editor as MarkdownEditor;
pub use environment::MarkdownEditorEnvironment;
pub use host::{ImagePasteHandler, ImageTarget, InsertOriginalImagePath, PastedImage};
pub use strings::I18nStrings as MarkdownEditorStrings;
pub use theme::{
    Placeholders, Theme as MarkdownEditorTheme, ThemeColors, ThemeDimensions, ThemeTypography,
};
pub use theme_config::{
    MarkdownEditorBuiltinTheme, MarkdownEditorThemePack, MarkdownEditorThemePatch,
};
