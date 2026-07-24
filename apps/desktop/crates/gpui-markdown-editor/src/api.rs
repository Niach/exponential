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
    /// EXP-261 vendoring: embedded auto-height mode — the editor renders at
    /// natural content height inside a host-owned scroll container: no
    /// internal scrolling/scrollbar, no row virtualization, full-width rows,
    /// and no focus-steal on mount (focus arrives via click or
    /// [`crate::MarkdownEditor::focus_first_block`]).
    pub embedded: bool,
}

impl Default for MarkdownEditorOptions {
    fn default() -> Self {
        Self {
            mode: MarkdownEditorMode::Rendered,
            environment: crate::environment::MarkdownEditorEnvironment::default(),
            history_limit: 200,
            embedded: false,
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
#[derive(Clone, Debug, PartialEq)]
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
    /// EXP-261 vendoring: right-click on a standalone image — the HOST renders
    /// the context menu (view/download/copy-link/delete live host-side).
    ImageContextMenuRequested {
        /// Raw markdown `src` of the image (may carry a `?w=` display width).
        src: String,
        /// Window position of the triggering mouse event.
        position: gpui::Point<gpui::Pixels>,
    },
    /// EXP-261 vendoring: an image resize drag finished; the host persists the
    /// new display width (`?w=` URL param) via `rewrite_image_sources`.
    ImageResized {
        src: String,
        width: f32,
    },
    /// EXP-261 vendoring: Cmd/Ctrl+click on a decorated `@email` / `#IDENT`
    /// pill — the host routes it (issue refs navigate in-app).
    ReferenceClicked {
        kind: crate::host::ReferenceKind,
        value: String,
    },
}
