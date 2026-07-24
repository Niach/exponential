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

/// EXP-261 vendoring: the formatting commands a host toolbar drives, mirroring
/// the web `StaticToolbar`
/// (`apps/web/src/components/issue-editor/markdown-editor.tsx`). Every one is a
/// TOGGLE: re-applying the active block kind returns it to a paragraph, and the
/// inline marks flip off when the whole selection already carries them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormatCommand {
    Bold,
    Italic,
    Strikethrough,
    Code,
    /// CommonMark heading level; the toolbar offers 1–3 like web.
    Heading(u8),
    BulletList,
    OrderedList,
    TaskList,
    Quote,
    /// Point the selection — or the whole link under a collapsed caret — at
    /// this target; `None` removes the link.
    Link(Option<String>),
    ClearFormatting,
}

/// EXP-261 vendoring: which formats are active at the caret/selection — the
/// toolbar's pressed-button state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FormatState {
    pub bold: bool,
    pub italic: bool,
    pub strikethrough: bool,
    pub code: bool,
    /// Heading level of the focused block, if it is one.
    pub heading: Option<u8>,
    pub bullet_list: bool,
    pub ordered_list: bool,
    pub task_list: bool,
    pub quote: bool,
    /// Whether the selection is non-empty.
    pub has_selection: bool,
    /// Target of the link at the caret/selection — the toolbar prefills its
    /// URL field with it and lights the Link button when it is `Some`.
    pub link: Option<String>,
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
        /// `None` drops the `?w=` param — the image returns to full width.
        width: Option<f32>,
    },
    /// EXP-261 vendoring: Cmd/Ctrl+click on a decorated `@email` / `#IDENT`
    /// pill — the host routes it (issue refs navigate in-app).
    ReferenceClicked {
        kind: crate::host::ReferenceKind,
        value: String,
    },
}
