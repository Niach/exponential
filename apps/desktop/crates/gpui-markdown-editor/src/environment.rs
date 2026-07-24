use std::path::PathBuf;
use std::sync::Arc;

use crate::host::{ImagePasteHandler, default_image_paste_handler};
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
        }
    }
}
