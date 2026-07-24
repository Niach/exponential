//! Component-level scrolling, mode switching, and host-facing state updates.

use std::collections::HashSet;
use std::sync::Arc;

use gpui::*;

use super::*;
use crate::api::{LinkRequest, MarkdownEditorEvent, MarkdownEditorMode, SourceSelection};
use crate::environment::MarkdownEditorEnvironment;
use crate::theme::Theme;

impl Editor {
    pub(super) fn scrollbar_geometry(
        viewport_height: f32,
        max_scroll_y: f32,
        current_scroll_y: f32,
    ) -> ScrollbarGeometry {
        let track_height = viewport_height.max(20.0);
        let content_height = viewport_height + max_scroll_y;
        let thumb_height = if max_scroll_y > 0.5 {
            (track_height * (viewport_height / content_height)).clamp(28.0, track_height)
        } else {
            track_height
        };
        let progress = if max_scroll_y > 0.0 {
            current_scroll_y.clamp(0.0, max_scroll_y) / max_scroll_y
        } else {
            0.0
        };
        let thumb_top = (track_height - thumb_height).max(0.0) * progress;
        ScrollbarGeometry {
            track_height,
            thumb_height,
            thumb_top,
            max_scroll_y,
        }
    }

    pub(super) fn scroll_offset_for_thumb_top(
        thumb_top: f32,
        track_height: f32,
        thumb_height: f32,
        max_scroll_y: f32,
    ) -> f32 {
        if max_scroll_y <= 0.0 {
            return 0.0;
        }
        let travel = (track_height - thumb_height).max(0.0);
        if travel <= 0.0 {
            return 0.0;
        }
        max_scroll_y * (thumb_top / travel).clamp(0.0, 1.0)
    }

    pub(super) fn rendered_window(
        strides: &[f32],
        scroll_y: f32,
        viewport_height: f32,
        overdraw: f32,
        focus_row: Option<usize>,
    ) -> RenderWindow {
        let n = strides.len();
        if n == 0 {
            return RenderWindow {
                run_start: 0,
                run_end: 0,
                top_h: 0.0,
                bottom_h: 0.0,
            };
        }

        let band_top = scroll_y - overdraw;
        let band_bottom = scroll_y + viewport_height + overdraw;
        let mut run_start = n;
        let mut run_end = 0usize;
        let mut top_of_start = 0.0f32;
        let mut bottom_of_end = 0.0f32;
        let mut cursor = 0.0f32;
        for (index, &stride) in strides.iter().enumerate() {
            let top = cursor;
            let bottom = cursor + stride.max(0.0);
            if bottom >= band_top && top <= band_bottom {
                if index < run_start {
                    run_start = index;
                    top_of_start = top;
                }
                run_end = index + 1;
                bottom_of_end = bottom;
            }
            cursor = bottom;
        }
        let total = cursor;
        if run_start >= run_end {
            run_start = n - 1;
            run_end = n;
            top_of_start = total - strides[n - 1].max(0.0);
            bottom_of_end = total;
        }
        if let Some(focus_row) = focus_row {
            let focus_row = focus_row.min(n - 1);
            if focus_row < run_start {
                run_start = focus_row;
                top_of_start = strides[..focus_row].iter().map(|s| s.max(0.0)).sum();
            }
            if focus_row + 1 > run_end {
                run_end = focus_row + 1;
                bottom_of_end = strides[..=focus_row].iter().map(|s| s.max(0.0)).sum();
            }
        }
        RenderWindow {
            run_start,
            run_end,
            top_h: top_of_start.max(0.0),
            bottom_h: (total - bottom_of_end).max(0.0),
        }
    }

    pub(super) fn centered_column_ratio(
        viewport_width: f32,
        dimensions: &crate::theme::ThemeDimensions,
    ) -> f32 {
        if viewport_width <= dimensions.centered_shrink_start {
            return 1.0;
        }
        let t = ((viewport_width - dimensions.centered_shrink_start)
            / (dimensions.centered_shrink_end - dimensions.centered_shrink_start))
            .clamp(0.0, 1.0);
        1.0 - t * (1.0 - dimensions.centered_min_ratio)
    }

    pub(crate) fn centered_column_width(
        viewport_width: f32,
        dimensions: &crate::theme::ThemeDimensions,
    ) -> f32 {
        let available = (viewport_width - dimensions.editor_padding * 2.0).max(1.0);
        (available * Self::centered_column_ratio(viewport_width, dimensions))
            .max(320.0)
            .min(available)
    }

    pub(crate) fn on_toggle_view_mode_action(
        &mut self,
        _: &crate::components::ToggleViewMode,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_view_mode_from_ui(cx);
    }

    pub(super) fn toggle_view_mode_from_ui(&mut self, cx: &mut Context<Self>) {
        self.end_block_pointer_selection_sessions(cx);
        self.last_selection_snapshot = self.capture_source_selection_snapshot(cx);
        self.toggle_view_mode(cx);
    }

    pub(crate) fn on_undo(
        &mut self,
        _: &crate::components::Undo,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.undo_document(cx);
    }

    pub(crate) fn on_redo(
        &mut self,
        _: &crate::components::Redo,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.redo_document(cx);
    }

    pub fn set_mode(&mut self, mode: MarkdownEditorMode, cx: &mut Context<Self>) {
        if self.view_mode != mode {
            self.toggle_view_mode(cx);
        }
    }

    pub fn toggle_mode(&mut self, cx: &mut Context<Self>) {
        self.toggle_view_mode(cx);
    }

    pub(crate) fn toggle_view_mode(&mut self, cx: &mut Context<Self>) {
        self.end_block_pointer_selection_sessions(cx);
        let selection_snapshot = self.capture_source_selection_snapshot(cx);
        self.clear_cross_block_selection(cx);
        self.rendered_select_all_cycle = None;
        match self.view_mode {
            ViewMode::Rendered => {
                let markdown = self.document.markdown_text(cx);
                let block = Self::new_block(cx, BlockRecord::paragraph(markdown));
                block.update(cx, |block, _cx| block.set_source_document_mode());
                self.document.replace_roots(vec![block], cx);
                self.view_mode = ViewMode::Source;
                self.table_cells.clear();
            }
            ViewMode::Source => {
                let source = self.document.raw_source_text(cx);
                let mut roots = Self::build_root_blocks_from_markdown(cx, &source);
                if roots.is_empty() {
                    roots.push(Self::new_block(cx, BlockRecord::paragraph(String::new())));
                }
                self.document.replace_roots(roots, cx);
                self.view_mode = ViewMode::Rendered;
                self.rebuild_table_runtimes(cx);
                self.rebuild_image_runtimes(cx);
            }
        }
        self.sync_all_block_environments(cx);
        self.apply_selection_snapshot_in_current_mode(&selection_snapshot, cx);
        self.pending_scroll_active_block_into_view = true;
        self.pending_scroll_recheck_after_layout = true;
        self.last_scroll_viewport_size = None;
        self.table_axis_preview = None;
        self.table_axis_selection = None;
        self.dismiss_contextual_overlays(cx);
        self.sync_table_axis_visuals(cx);
        self.refresh_stable_document_snapshot(cx);
        cx.emit(MarkdownEditorEvent::ModeChanged {
            mode: self.view_mode,
        });
        cx.notify();
    }

    pub fn replace_markdown(&mut self, markdown: impl Into<String>, cx: &mut Context<Self>) {
        let normalized = markdown.into().replace("\r\n", "\n").replace('\r', "\n");
        let mut roots = if self.view_mode == ViewMode::Source {
            let block = Self::new_block(cx, BlockRecord::paragraph(normalized.clone()));
            block.update(cx, |block, _cx| block.set_source_document_mode());
            vec![block]
        } else {
            Self::build_root_blocks_from_markdown(cx, &normalized)
        };
        if roots.is_empty() {
            roots.push(Self::new_block(cx, BlockRecord::paragraph(String::new())));
        }
        self.document.replace_roots(roots, cx);
        self.undo_history.clear();
        self.redo_history.clear();
        self.pending_undo_capture = None;
        self.table_cells.clear();
        self.rebuild_table_runtimes(cx);
        self.rebuild_image_runtimes(cx);
        self.sync_all_block_environments(cx);
        self.pending_focus = self.first_focusable_entity_id(cx);
        self.active_entity_id = self.pending_focus;
        self.last_stable_source_text = normalized;
        self.refresh_stable_document_snapshot(cx);
        cx.notify();
    }

    pub fn set_source_selection(&mut self, selection: SourceSelection, cx: &mut Context<Self>) {
        let snapshot = UndoSelectionSnapshot {
            range: selection.range.clone(),
            reversed: selection.reversed,
        };
        self.apply_selection_snapshot_in_current_mode(&snapshot, cx);
        self.last_selection_snapshot = snapshot;
        cx.emit(MarkdownEditorEvent::SelectionChanged(selection));
        cx.notify();
    }

    /// Returns the immutable theme currently used by this editor instance.
    pub fn theme(&self) -> Arc<Theme> {
        self.environment.theme.clone()
    }

    /// Replaces only this editor instance's presentation tokens.
    ///
    /// Theme changes do not mutate Markdown, selection, revision, or history.
    pub fn set_theme(&mut self, theme: Arc<Theme>, cx: &mut Context<Self>) {
        if Arc::ptr_eq(&self.environment.theme, &theme) {
            return;
        }
        let mut environment = (*self.environment).clone();
        environment.theme = theme;
        self.environment = Arc::new(environment);
        self.sync_all_block_environments(cx);
        cx.notify();
    }

    pub fn set_environment(
        &mut self,
        environment: MarkdownEditorEnvironment,
        cx: &mut Context<Self>,
    ) {
        self.environment = Arc::new(environment);
        self.sync_all_block_environments(cx);
        self.rebuild_image_runtimes(cx);
        cx.notify();
    }

    pub(super) fn sync_all_block_environments(&self, cx: &mut Context<Self>) {
        let environment = self.environment.clone();
        let mut seen = HashSet::new();
        for visible in self.document.visible_blocks() {
            if seen.insert(visible.entity.entity_id()) {
                visible.entity.update(cx, |block, _cx| {
                    block.set_environment(environment.clone());
                });
            }
        }
        for binding in self.table_cells.values() {
            if seen.insert(binding.cell.entity_id()) {
                binding.cell.update(cx, |block, _cx| {
                    block.set_environment(environment.clone());
                });
            }
        }
    }

    pub(super) fn mark_dirty(&mut self, cx: &mut Context<Self>) {
        self.sync_all_block_environments(cx);
        self.revision = self.revision.saturating_add(1);
        cx.emit(MarkdownEditorEvent::Changed {
            revision: self.revision,
        });
        cx.notify();
    }

    pub(super) fn request_active_block_scroll_into_view(&mut self, cx: &mut Context<Self>) {
        self.pending_scroll_recheck_after_layout = true;
        if !self.pending_scroll_active_block_into_view {
            self.pending_scroll_active_block_into_view = true;
            cx.notify();
        }
    }

    pub(super) fn viewport_size_changed(previous: Size<Pixels>, current: Size<Pixels>) -> bool {
        const EPSILON: f32 = 0.5;
        (f32::from(previous.width) - f32::from(current.width)).abs() > EPSILON
            || (f32::from(previous.height) - f32::from(current.height)).abs() > EPSILON
    }

    pub(crate) fn request_open_link_prompt(
        &mut self,
        prompt_target: String,
        open_target: String,
        cx: &mut Context<Self>,
    ) {
        cx.emit(MarkdownEditorEvent::OpenLinkRequested(LinkRequest {
            prompt_target,
            open_target,
        }));
    }
}
