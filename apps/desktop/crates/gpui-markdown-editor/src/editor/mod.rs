//! Reusable GPUI Markdown editor state and controller.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::*;

use self::context_menu::{ContextMenuState, TableInsertDialogState};
use self::tree::DocumentTree;
use crate::api::{
    EditorCommand, MarkdownEditorEvent, MarkdownEditorMode, MarkdownEditorOptions, SourceSelection,
};
use crate::components::{
    Block, BlockKind, BlockRecord, FootnoteDefinitionBinding, FootnoteReferenceLocation,
    FootnoteRegistry, FootnoteResolvedOccurrence, ImageReferenceDefinitions, InlineTextTree,
    LinkReferenceDefinitions, TableAxisHighlight, TableAxisKind, TableAxisMarker,
    TableCellPosition, TableColumnAlignment, TableData, TableRuntime, UndoCaptureKind,
    parse_image_reference_definitions, parse_link_reference_definitions,
    serialize_table_cell_markdown,
};
use crate::environment::MarkdownEditorEnvironment;

mod context_menu;
mod document;
mod events;
mod history;
mod render;
mod runtime_context;
mod selection;
mod serialization;
mod source_mapping;
mod table_edit;
#[cfg(test)]
mod tests;
mod tree;
mod window_state;

/// Top-level reusable editor component.
pub struct Editor {
    document: DocumentTree,
    table_cells: HashMap<EntityId, TableCellBinding>,
    pub(crate) view_mode: ViewMode,
    environment: Arc<MarkdownEditorEnvironment>,
    history_limit: usize,
    revision: u64,
    pending_focus: Option<EntityId>,
    active_entity_id: Option<EntityId>,
    pending_scroll_active_block_into_view: bool,
    pending_scroll_recheck_after_layout: bool,
    scroll_handle: ScrollHandle,
    last_scroll_viewport_size: Option<Size<Pixels>>,
    prev_visible_block_ids: Vec<EntityId>,
    row_stride_cache: HashMap<EntityId, f32>,
    prev_render_window: Option<(usize, usize)>,
    context_menu: Option<ContextMenuState>,
    table_insert_dialog: Option<TableInsertDialogState>,
    context_menu_submenu_close_task: Option<Task<()>>,
    table_axis_preview: Option<TableAxisSelection>,
    table_axis_selection: Option<TableAxisSelection>,
    cross_block_selection: Option<CrossBlockSelection>,
    cross_block_drag: Option<CrossBlockDrag>,
    rendered_select_all_cycle: Option<RenderedSelectAllCycle>,
    scrollbar_hovered: bool,
    scrollbar_visible_until: Instant,
    scrollbar_fade_task: Option<Task<()>>,
    scroll_recheck_task: Option<Task<()>>,
    scrollbar_drag: Option<ScrollbarDragSession>,
    undo_history: Vec<HistoryEntry>,
    redo_history: Vec<HistoryEntry>,
    pending_undo_capture: Option<PendingUndoCapture>,
    last_selection_snapshot: UndoSelectionSnapshot,
    last_stable_source_text: String,
    history_restore_in_progress: bool,
    image_reference_definitions: Arc<ImageReferenceDefinitions>,
    link_reference_definitions: Arc<LinkReferenceDefinitions>,
    footnote_registry: Arc<FootnoteRegistry>,
}

impl EventEmitter<MarkdownEditorEvent> for Editor {}

#[derive(Clone)]
struct TableCellBinding {
    table_block: Entity<Block>,
    cell: Entity<Block>,
    position: TableCellPosition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TableAxisSelection {
    table_block_id: EntityId,
    kind: TableAxisKind,
    index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ScrollbarGeometry {
    track_height: f32,
    thumb_height: f32,
    thumb_top: f32,
    max_scroll_y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RenderWindow {
    run_start: usize,
    run_end: usize,
    top_h: f32,
    bottom_h: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ScrollbarDragSession {
    pointer_offset_y: f32,
    track_height: f32,
    thumb_height: f32,
    max_scroll_y: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UndoSelectionSnapshot {
    range: std::ops::Range<usize>,
    reversed: bool,
}

#[derive(Clone, Debug)]
struct HistoryEntry {
    source_text: String,
    selection: UndoSelectionSnapshot,
    timestamp: Instant,
    kind: UndoCaptureKind,
}

#[derive(Clone, Debug)]
struct PendingUndoCapture {
    snapshot: HistoryEntry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CrossBlockSelectionEndpoint {
    pub(super) entity_id: EntityId,
    pub(super) offset: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CrossBlockSelection {
    pub(super) anchor: CrossBlockSelectionEndpoint,
    pub(super) focus: CrossBlockSelectionEndpoint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CrossBlockDrag {
    pub(super) anchor: CrossBlockSelectionEndpoint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RenderedSelectAllCycle {
    entity_id: EntityId,
    count: u8,
    last_pressed_at: Instant,
}

#[derive(Clone)]
pub(super) struct SourceTargetMapping {
    entity: Entity<Block>,
    full_source_range: std::ops::Range<usize>,
    content_to_source: Vec<usize>,
    source_to_content: Vec<usize>,
}

pub(super) type ViewMode = MarkdownEditorMode;

impl Editor {
    const HISTORY_COALESCE_WINDOW: Duration = Duration::from_millis(1_000);
    const RENDERED_SELECT_ALL_CYCLE_WINDOW: Duration = Duration::from_millis(750);

    pub fn new(
        markdown: impl Into<String>,
        options: MarkdownEditorOptions,
        cx: &mut Context<Self>,
    ) -> Self {
        let mode = options.mode;
        let mut editor = Self::build(
            markdown.into(),
            Arc::new(options.environment),
            options.history_limit,
            cx,
        );
        if mode == MarkdownEditorMode::Source {
            editor.toggle_view_mode(cx);
        }
        editor
    }

    pub fn with_environment(
        markdown: impl Into<String>,
        environment: MarkdownEditorEnvironment,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new(
            markdown,
            MarkdownEditorOptions {
                environment,
                ..MarkdownEditorOptions::default()
            },
            cx,
        )
    }

    pub fn from_markdown(
        cx: &mut Context<Self>,
        markdown: String,
        document_base_dir: Option<PathBuf>,
    ) -> Self {
        let options = MarkdownEditorOptions {
            environment: MarkdownEditorEnvironment {
                document_base_dir,
                ..MarkdownEditorEnvironment::default()
            },
            ..MarkdownEditorOptions::default()
        };
        Self::new(markdown, options, cx)
    }

    fn build(
        markdown: String,
        environment: Arc<MarkdownEditorEnvironment>,
        history_limit: usize,
        cx: &mut Context<Self>,
    ) -> Self {
        let normalized = markdown.replace("\r\n", "\n").replace('\r', "\n");
        let mut roots = Self::build_root_blocks_from_markdown(cx, &normalized);
        if roots.is_empty() {
            roots.push(Self::new_block(cx, BlockRecord::paragraph(String::new())));
        }

        let mut document = DocumentTree::new(roots);
        document.rebuild_metadata_and_snapshot(cx);
        let pending_focus = document.first_root().map(|block| block.entity_id());
        let mut editor = Self {
            document,
            table_cells: HashMap::new(),
            view_mode: ViewMode::Rendered,
            environment,
            history_limit,
            revision: 0,
            pending_focus,
            active_entity_id: pending_focus,
            pending_scroll_active_block_into_view: true,
            pending_scroll_recheck_after_layout: true,
            scroll_handle: ScrollHandle::new(),
            last_scroll_viewport_size: None,
            prev_visible_block_ids: Vec::new(),
            row_stride_cache: HashMap::new(),
            prev_render_window: None,
            context_menu: None,
            table_insert_dialog: None,
            context_menu_submenu_close_task: None,
            table_axis_preview: None,
            table_axis_selection: None,
            cross_block_selection: None,
            cross_block_drag: None,
            rendered_select_all_cycle: None,
            scrollbar_hovered: false,
            scrollbar_visible_until: Instant::now(),
            scrollbar_fade_task: None,
            scroll_recheck_task: None,
            scrollbar_drag: None,
            undo_history: Vec::new(),
            redo_history: Vec::new(),
            pending_undo_capture: None,
            last_selection_snapshot: Self::empty_selection_snapshot(),
            last_stable_source_text: normalized,
            history_restore_in_progress: false,
            image_reference_definitions: Arc::default(),
            link_reference_definitions: Arc::default(),
            footnote_registry: Arc::default(),
        };
        editor.rebuild_table_runtimes(cx);
        editor.rebuild_image_runtimes(cx);
        editor.sync_all_block_environments(cx);
        editor.pending_focus = editor.first_focusable_entity_id(cx);
        editor.active_entity_id = editor.pending_focus;
        editor.refresh_stable_document_snapshot(cx);
        editor
    }

    pub fn markdown(&self, cx: &App) -> String {
        self.current_document_source(cx)
    }

    pub fn mode(&self) -> MarkdownEditorMode {
        self.view_mode
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_history.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_history.is_empty()
    }

    pub fn source_selection(&self, cx: &App) -> SourceSelection {
        let snapshot = self.capture_source_selection_snapshot(cx);
        SourceSelection {
            range: snapshot.range,
            reversed: snapshot.reversed,
        }
    }

    pub fn environment(&self) -> &MarkdownEditorEnvironment {
        &self.environment
    }

    pub fn execute(&mut self, command: EditorCommand, cx: &mut Context<Self>) {
        match command {
            EditorCommand::Undo => self.undo_document(cx),
            EditorCommand::Redo => self.redo_document(cx),
            EditorCommand::ToggleMode => self.toggle_view_mode(cx),
            EditorCommand::SetMode(mode) => self.set_mode(mode, cx),
        }
    }
}
