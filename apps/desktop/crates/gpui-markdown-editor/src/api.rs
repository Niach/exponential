use std::ops::Range;

/// The two editing representations supported by the component.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownEditorMode {
    /// Structured, rendered block editing.
    Rendered,
    /// Raw Markdown editing in one source buffer.
    Source,
}

/// Construction options for one editor instance.
#[derive(Clone)]
pub struct MarkdownEditorOptions {
    pub mode: MarkdownEditorMode,
    pub environment: crate::environment::MarkdownEditorEnvironment,
    pub history_limit: usize,
}

impl Default for MarkdownEditorOptions {
    fn default() -> Self {
        Self {
            mode: MarkdownEditorMode::Rendered,
            environment: crate::environment::MarkdownEditorEnvironment::default(),
            history_limit: 200,
        }
    }
}

/// Document-level commands that can be invoked without synthesizing GPUI input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorCommand {
    Undo,
    Redo,
    ToggleMode,
    SetMode(MarkdownEditorMode),
}

/// A selection expressed in UTF-8 byte offsets into `MarkdownEditor::markdown`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceSelection {
    pub range: Range<usize>,
    pub reversed: bool,
}

/// A link activation delegated to the host application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkRequest {
    pub prompt_target: String,
    pub open_target: String,
}

/// Observable events emitted across the component boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkdownEditorEvent {
    /// The document changed. The text is intentionally omitted to avoid a full
    /// serialization allocation on every keystroke.
    Changed {
        revision: u64,
    },
    ModeChanged {
        mode: MarkdownEditorMode,
    },
    SelectionChanged(SourceSelection),
    OpenLinkRequested(LinkRequest),
    Error {
        message: String,
    },
}
