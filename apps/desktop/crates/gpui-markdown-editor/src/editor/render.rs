//! Rendering for the reusable editor component.

use std::time::{Duration, Instant};

use gpui::prelude::FluentBuilder as _;
use gpui::*;

use super::Editor;
use crate::components::{Block, CalloutVariant};
use crate::theme::{Theme, ThemeDimensions};

/// Rows within this many pixels of the viewport stay mounted.
const RENDER_OVERDRAW_PX: f32 = 800.0;

fn editor_text_font(family: &str) -> Font {
    // FontFallbacks is internally `Arc<Vec<String>>` — building it once
    // per process and Arc-cloning per render is the right shape, since
    // editor_text_font() is called from Editor::render on every frame.
    static FALLBACKS: std::sync::OnceLock<FontFallbacks> = std::sync::OnceLock::new();
    let fallbacks = FALLBACKS
        .get_or_init(|| {
            FontFallbacks::from_fonts(tibetan_font_fallbacks_for_target_os(std::env::consts::OS))
        })
        .clone();
    // EXP-261 vendoring: family comes from the theme (upstream hardcoded
    // `.SystemUIFont`).
    let mut font = font(family);
    font.fallbacks = Some(fallbacks);
    font
}

fn tibetan_font_fallbacks_for_target_os(target_os: &str) -> Vec<String> {
    let families = match target_os {
        "windows" => &[
            "Microsoft Himalaya",
            "Noto Serif Tibetan",
            "Noto Sans Tibetan",
            "BabelStone Tibetan",
        ][..],
        "macos" => &["Kailasa", "Noto Serif Tibetan", "Noto Sans Tibetan"][..],
        _ => &[
            "Noto Serif Tibetan",
            "Noto Sans Tibetan",
            "Microsoft Himalaya",
            "Kailasa",
            "BabelStone Tibetan",
        ][..],
    };
    families
        .iter()
        .map(|family| (*family).to_string())
        .collect()
}

/// Adjacent-row metadata used to collapse spacing inside visual groups.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RenderedRowSpacingInfo {
    quote_group_anchor: Option<uuid::Uuid>,
    visible_quote_group_anchor: Option<uuid::Uuid>,
    callout_anchor: Option<uuid::Uuid>,
    callout_variant: Option<CalloutVariant>,
    is_callout_header: bool,
    footnote_anchor: Option<uuid::Uuid>,
    is_footnote_header: bool,
    /// EXP-277 vendoring: image-adjacent rows widen the gap (see
    /// [`rendered_row_top_gap`]).
    is_standalone_image: bool,
}

impl RenderedRowSpacingInfo {
    fn from_block(block: &Block) -> Self {
        Self {
            quote_group_anchor: block.quote_group_anchor,
            visible_quote_group_anchor: block.visible_quote_group_anchor,
            callout_anchor: block.callout_anchor,
            callout_variant: block.callout_variant,
            is_callout_header: block.kind().is_callout(),
            footnote_anchor: block.footnote_anchor,
            is_footnote_header: block.kind().is_footnote_definition(),
            is_standalone_image: block.renders_as_standalone_image(),
        }
    }
}

fn rendered_row_top_gap(
    previous: Option<RenderedRowSpacingInfo>,
    current: RenderedRowSpacingInfo,
    default_gap: f32,
) -> f32 {
    let Some(previous) = previous else {
        return 0.0;
    };

    if previous.quote_group_anchor.is_some()
        && previous.quote_group_anchor == current.quote_group_anchor
    {
        0.0
    } else if previous.is_standalone_image || current.is_standalone_image {
        // EXP-277 vendoring: the default 6px gap is a needle-thin target for
        // the image gap-click affordance — double it around images.
        default_gap * 2.0
    } else {
        default_gap
    }
}

fn callout_colors(variant: CalloutVariant, theme: &Theme) -> (Hsla, Hsla) {
    let c = &theme.colors;
    match variant {
        CalloutVariant::Note => (c.callout_note_border, c.callout_note_bg),
        CalloutVariant::Tip => (c.callout_tip_border, c.callout_tip_bg),
        CalloutVariant::Important => (c.callout_important_border, c.callout_important_bg),
        CalloutVariant::Warning => (c.callout_warning_border, c.callout_warning_bg),
        CalloutVariant::Caution => (c.callout_caution_border, c.callout_caution_bg),
    }
}

fn callout_row_top_gap(
    previous: Option<RenderedRowSpacingInfo>,
    current: RenderedRowSpacingInfo,
    dimensions: &ThemeDimensions,
) -> f32 {
    let Some(previous) = previous else {
        return 0.0;
    };

    if previous.visible_quote_group_anchor.is_some()
        && previous.visible_quote_group_anchor == current.visible_quote_group_anchor
    {
        return 0.0;
    }

    if previous.is_callout_header {
        dimensions.callout_header_margin_bottom
    } else {
        dimensions.callout_body_gap
    }
}

fn footnote_row_top_gap(previous: Option<RenderedRowSpacingInfo>, default_gap: f32) -> f32 {
    let Some(previous) = previous else {
        return 0.0;
    };

    if previous.is_footnote_header {
        default_gap * 0.75
    } else {
        default_gap
    }
}

/// Width of a top-level render row: the centered fixed column standalone,
/// the full host slot when embedded (EXP-261 vendoring).
fn row_width(row: Div, centered_width: Option<f32>) -> Div {
    match centered_width {
        Some(width) => row.w(px(width)).max_w(relative(1.0)),
        None => row.w_full(),
    }
}

fn footnote_group_shell(
    children: Vec<AnyElement>,
    theme: &Theme,
    dimensions: &ThemeDimensions,
) -> AnyElement {
    div()
        .w_full()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .gap(px(0.0))
        .px(px(dimensions.footnote_padding_x))
        .py(px(dimensions.footnote_padding_y))
        .rounded(px(dimensions.footnote_radius))
        .border(px(1.0))
        .border_color(theme.colors.footnote_border)
        .bg(theme.colors.footnote_bg)
        .children(children)
        .into_any_element()
}

impl Editor {
    fn apply_pending_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(entity_id) = self.pending_focus.take()
            && let Some(block) = self.focusable_entity_by_id(entity_id)
        {
            let focus_handle = block.read(cx).focus_handle.clone();
            focus_handle.focus(window, cx);
        }
    }

    /// The scroll handle the caret-follow math runs against: the editor's
    /// own in standalone mode, the HOST's tracked handle in embedded mode
    /// (EXP-288 — `None` when the host never handed one over, which
    /// disables following entirely, the pre-EXP-288 embedded behavior).
    fn follow_handle(&self) -> Option<&ScrollHandle> {
        if self.embedded {
            self.external_scroll_handle.as_ref()
        } else {
            Some(&self.scroll_handle)
        }
    }

    fn ensure_focused_caret_visible(&mut self, window: &Window, cx: &App) -> bool {
        let Some(focused_block) = self.focused_edit_target(window, cx) else {
            return false;
        };
        let Some(active_bounds) =
            focused_block.read_with(cx, |block, _cx| block.active_range_or_cursor_bounds())
        else {
            return false;
        };
        let Some(handle) = self.follow_handle() else {
            return false;
        };

        let viewport = handle.bounds();
        let padding = px(20.0);
        let top_limit = viewport.top() + padding;
        let bottom_limit = viewport.bottom() - padding;
        let mut offset = handle.offset();
        let mut changed = false;

        if active_bounds.top() < top_limit {
            offset.y += top_limit - active_bounds.top();
            changed = true;
        } else if active_bounds.bottom() > bottom_limit {
            offset.y -= active_bounds.bottom() - bottom_limit;
            changed = true;
        }

        if changed {
            let max_offset_y = handle.max_offset().y.max(px(0.0));
            offset.y = offset.y.min(px(0.0)).max(-max_offset_y);
            handle.set_offset(offset);
        }

        true
    }

    fn apply_pending_scroll_into_view(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.scrollbar_drag.is_some() {
            return;
        }

        if !self.pending_scroll_active_block_into_view {
            return;
        }

        // scroll_to_item indexed children by position, which the spacers break;
        // the focused block is always mounted, so pixel math on its bounds works.
        let has_bounds = self.ensure_focused_caret_visible(window, cx);
        if self.pending_scroll_recheck_after_layout {
            self.pending_scroll_recheck_after_layout = false;
            self.schedule_scroll_recheck(cx);
            return;
        }

        if !has_bounds {
            self.schedule_scroll_recheck(cx);
            return;
        }

        self.pending_scroll_active_block_into_view = false;
        self.scroll_recheck_task = None;
    }

    /// Requests a repaint one frame out so a still-pending scroll-into-view can
    /// retry once the target block has been laid out. `cx.notify()` is swallowed
    /// when called from within `render`, so without this the retry would wait
    /// for the next external notify (e.g. the cursor blink, ~0.5s later).
    fn schedule_scroll_recheck(&mut self, cx: &mut Context<Self>) {
        self.scroll_recheck_task = Some(cx.spawn(async move |this: WeakEntity<Self>, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
            let _ = this.update(cx, |_this, cx| cx.notify());
        }));
    }

    fn sync_scroll_viewport(&mut self, viewport_size: Size<Pixels>, cx: &mut Context<Self>) {
        match self.last_scroll_viewport_size {
            Some(previous) if Self::viewport_size_changed(previous, viewport_size) => {
                self.last_scroll_viewport_size = Some(viewport_size);
                self.request_active_block_scroll_into_view(cx);
            }
            Some(_) => {}
            None => {
                self.last_scroll_viewport_size = Some(viewport_size);
            }
        }
    }
}
impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.apply_pending_focus(window, cx);
        // EXP-261 vendoring: in embedded mode the HOST owns scrolling — the
        // internal scroll handle is untracked and all offset math is skipped
        // UNLESS the host handed its tracked handle over (EXP-288), which
        // re-enables the caret-follow machinery against the host container.
        let follows = self.follow_handle().is_some();
        if follows {
            self.apply_pending_scroll_into_view(window, cx);
        }
        self.last_selection_snapshot = self.capture_source_selection_snapshot(cx);

        // The internal handle's bounds still drive the standalone scrollbar
        // math below; the follow-handle's bounds drive the caret-follow
        // resize re-assertion (identical in standalone mode).
        let viewport_bounds = self.scroll_handle.bounds();
        let viewport_size = self
            .follow_handle()
            .unwrap_or(&self.scroll_handle)
            .bounds()
            .size;
        if follows {
            self.sync_scroll_viewport(viewport_size, cx);
        }

        let theme = self.environment.theme.clone();

        let d = &theme.dimensions;
        let visible_blocks = self.document.visible_blocks().to_vec();

        // EXP-268: the empty-editing placeholder is a DOCUMENT affordance —
        // only the FIRST block of a fully empty document may show it (web
        // tiptap parity: `p.is-editor-empty:first-child`). Sync the per-block
        // flag here, change-gated so a steady state never re-notifies.
        let document_empty = visible_blocks
            .iter()
            .all(|visible| visible.entity.read(cx).display_text().is_empty());
        for (ix, visible) in visible_blocks.iter().enumerate() {
            let show = document_empty && ix == 0;
            if visible.entity.read(cx).show_empty_placeholder != show {
                visible.entity.update(cx, |block, cx| {
                    block.show_empty_placeholder = show;
                    cx.notify();
                });
            }
        }

        let editor = cx.entity().downgrade();
        let scroll_trigger_padding = (d.block_min_height * 0.75).max(16.0);
        let max_scroll_y = f32::from(self.scroll_handle.max_offset().y.max(px(0.0)));
        let viewport_height = f32::from(viewport_bounds.size.height.max(px(1.0)));
        // Extra room below the last block so the lowest line can be scrolled up
        // to the viewport center instead of being pinned to the bottom edge.
        let scroll_beyond_bottom = viewport_height * 0.5;
        let viewport_width = f32::from(viewport_bounds.size.width.max(px(1.0)));
        let has_overflow = max_scroll_y > 0.5;

        // EXP-261 vendoring: embedded editors fill the host slot instead of
        // centering a fixed column (the untracked viewport is 0 anyway), so
        // their rows take `w_full` rather than a pixel column width. A fake
        // huge px width does NOT work here: `max_w(relative(1.0))` cannot
        // resolve against the indefinite parent in taffy's intrinsic pass, so
        // every paragraph was measured unwrapped — one line tall — and the row
        // kept that height while paint wrapped it into three, overlapping the
        // rows below.
        let centered_width = if self.embedded {
            None
        } else {
            Some(Self::centered_column_width(
                viewport_width,
                &theme.dimensions,
            ))
        };
        let current_scroll_y = (-f32::from(self.scroll_handle.offset().y)).clamp(0.0, max_scroll_y);
        let scrollbar_geometry =
            Self::scrollbar_geometry(viewport_height, max_scroll_y, current_scroll_y);
        let track_height = scrollbar_geometry.track_height;
        let thumb_height = scrollbar_geometry.thumb_height;
        let thumb_top = scrollbar_geometry.thumb_top;

        let show_custom_scrollbar = has_overflow
            && (self.scrollbar_drag.is_some()
                || self.scrollbar_hovered
                || Instant::now() <= self.scrollbar_visible_until);

        // Spacing metadata is read on demand instead of pre-collected into a
        // Vec<RenderedRowSpacingInfo> sized to all visible blocks. For long
        // documents this skips a ~tens-of-KB allocation per frame; per-block
        // entity.read_with is a cheap immutable lock + 7-field struct copy.
        let spacing_for = |index: usize| -> RenderedRowSpacingInfo {
            visible_blocks[index]
                .entity
                .read_with(cx, |block, _cx| RenderedRowSpacingInfo::from_block(block))
        };
        let mut previous_row_spacing = None;
        // One entry per render row; off-screen rows are dropped after windowing.
        let mut row_elements: Vec<AnyElement> = Vec::new();
        let mut row_starts: Vec<usize> = Vec::new();
        // Each row's leading `mt` gap; the top spacer subtracts the first mounted
        // row's, since that row re-applies it.
        let mut row_top_gaps: Vec<f32> = Vec::new();
        let mut index = 0usize;
        while index < visible_blocks.len() {
            let first_visible = visible_blocks[index].clone();
            let first_spacing = spacing_for(index);
            let top_gap = rendered_row_top_gap(previous_row_spacing, first_spacing, d.block_gap);

            if let (Some(callout_anchor), Some(callout_variant)) =
                (first_spacing.callout_anchor, first_spacing.callout_variant)
            {
                let mut group_children = Vec::new();
                let mut group_end = index;
                let mut previous_callout_row = None;
                while group_end < visible_blocks.len()
                    && spacing_for(group_end).callout_anchor == Some(callout_anchor)
                {
                    let row_spacing = spacing_for(group_end);
                    if let Some(footnote_anchor) = row_spacing.footnote_anchor {
                        let mut footnote_children = Vec::new();
                        let mut footnote_end = group_end;
                        let mut previous_footnote_row = None;
                        while footnote_end < visible_blocks.len()
                            && spacing_for(footnote_end).callout_anchor == Some(callout_anchor)
                            && spacing_for(footnote_end).footnote_anchor == Some(footnote_anchor)
                        {
                            let footnote_spacing = spacing_for(footnote_end);
                            let entity = visible_blocks[footnote_end].entity.clone();
                            let row = div()
                                .w_full()
                                .flex_shrink_0()
                                .mt(px(footnote_row_top_gap(previous_footnote_row, d.block_gap)))
                                .child(entity.clone());
                            let row = if self.view_mode == super::ViewMode::Rendered {
                                let row_editor = editor.clone();
                                let entity_id = entity.entity_id();
                                row.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                                    let _ = row_editor.update(cx, |editor, cx| {
                                        editor.on_block_context_menu_mouse_down(
                                            entity_id, event, window, cx,
                                        );
                                    });
                                })
                            } else {
                                row
                            };
                            footnote_children.push(row.into_any_element());
                            previous_footnote_row = Some(footnote_spacing);
                            footnote_end += 1;
                        }

                        group_children.push(
                            div()
                                .w_full()
                                .flex_shrink_0()
                                .mt(px(callout_row_top_gap(
                                    previous_callout_row,
                                    row_spacing,
                                    d,
                                )))
                                .child(footnote_group_shell(footnote_children, &theme, d))
                                .into_any_element(),
                        );
                        previous_callout_row = Some(spacing_for(footnote_end - 1));
                        group_end = footnote_end;
                        continue;
                    }

                    let entity = visible_blocks[group_end].entity.clone();
                    let row = div()
                        .w_full()
                        .flex_shrink_0()
                        .mt(px(callout_row_top_gap(
                            previous_callout_row,
                            row_spacing,
                            d,
                        )))
                        .child(entity.clone());
                    let row = if self.view_mode == super::ViewMode::Rendered {
                        let row_editor = editor.clone();
                        let entity_id = entity.entity_id();
                        row.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                            let _ = row_editor.update(cx, |editor, cx| {
                                editor
                                    .on_block_context_menu_mouse_down(entity_id, event, window, cx);
                            });
                        })
                    } else {
                        row
                    };
                    group_children.push(row.into_any_element());
                    previous_callout_row = Some(row_spacing);
                    group_end += 1;
                }

                let (accent, background) = callout_colors(callout_variant, &theme);
                row_starts.push(index);
                row_top_gaps.push(top_gap);
                row_elements.push(
                    div()
                        .map(|row| row_width(row, centered_width))
                        .flex_shrink_0()
                        .mt(px(top_gap))
                        .flex()
                        .flex_col()
                        .gap(px(0.0))
                        .px(px(d.callout_padding_x))
                        .py(px(d.callout_padding_y))
                        .rounded(px(d.callout_radius))
                        .border_l(px(d.callout_border_width))
                        .border_color(accent)
                        .bg(background)
                        .children(group_children)
                        .into_any_element(),
                );
                previous_row_spacing = Some(spacing_for(group_end - 1));
                index = group_end;
                continue;
            }

            if let Some(footnote_anchor) = first_spacing.footnote_anchor {
                let mut group_children = Vec::new();
                let mut group_end = index;
                let mut previous_footnote_row = None;
                while group_end < visible_blocks.len()
                    && spacing_for(group_end).footnote_anchor == Some(footnote_anchor)
                {
                    let row_spacing = spacing_for(group_end);
                    let entity = visible_blocks[group_end].entity.clone();
                    let row = div()
                        .w_full()
                        .flex_shrink_0()
                        .mt(px(footnote_row_top_gap(previous_footnote_row, d.block_gap)))
                        .child(entity.clone());
                    let row = if self.view_mode == super::ViewMode::Rendered {
                        let row_editor = editor.clone();
                        let entity_id = entity.entity_id();
                        row.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                            let _ = row_editor.update(cx, |editor, cx| {
                                editor
                                    .on_block_context_menu_mouse_down(entity_id, event, window, cx);
                            });
                        })
                    } else {
                        row
                    };
                    group_children.push(row.into_any_element());
                    previous_footnote_row = Some(row_spacing);
                    group_end += 1;
                }

                row_starts.push(index);
                row_top_gaps.push(top_gap);
                row_elements.push(
                    div()
                        .map(|row| row_width(row, centered_width))
                        .flex_shrink_0()
                        .mt(px(top_gap))
                        .child(footnote_group_shell(group_children, &theme, d))
                        .into_any_element(),
                );
                previous_row_spacing = Some(spacing_for(group_end - 1));
                index = group_end;
                continue;
            }

            let entity = first_visible.entity.clone();
            let row = div()
                .map(|row| row_width(row, centered_width))
                .flex_shrink_0()
                .mt(px(top_gap))
                .child(entity.clone());
            let row = if self.view_mode == super::ViewMode::Rendered {
                let row_editor = editor.clone();
                let entity_id = entity.entity_id();
                row.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                    let _ = row_editor.update(cx, |editor, cx| {
                        editor.on_block_context_menu_mouse_down(entity_id, event, window, cx);
                    });
                })
            } else {
                row
            };
            row_starts.push(index);
            row_top_gaps.push(top_gap);
            row_elements.push(row.into_any_element());
            previous_row_spacing = Some(first_spacing);
            index += 1;
        }

        // The focused row is always kept mounted so its caret is not blurred; a
        // table cell maps to its containing table block's row.
        let focus_row = self
            .focused_edit_target_entity_id(window, cx)
            .and_then(|id| {
                self.document.visible_index_for_entity_id(id).or_else(|| {
                    self.table_cell_binding(id).and_then(|binding| {
                        self.document
                            .visible_index_for_entity_id(binding.table_block.entity_id())
                    })
                })
            })
            .map(|visible_index| {
                row_starts
                    .partition_point(|&start| start <= visible_index)
                    .saturating_sub(1)
            });

        // A row's first block keys its cached height; its painted top (from last
        // frame) feeds the footprints below.
        let row_first_ids: Vec<EntityId> = row_starts
            .iter()
            .map(|&start| visible_blocks[start].entity.entity_id())
            .collect();
        let row_tops: Vec<Option<f32>> = row_starts
            .iter()
            .map(|&start| {
                visible_blocks[start]
                    .entity
                    .read_with(cx, |block, _cx| block.last_bounds)
                    .map(|bounds| f32::from(bounds.top()))
            })
            .collect();

        // On a structural edit the row indices no longer match last frame, so the
        // cache refresh below is skipped; its block-keyed entries still hold.
        let structural_change = visible_blocks.len() != self.prev_visible_block_ids.len()
            || visible_blocks
                .iter()
                .zip(&self.prev_visible_block_ids)
                .any(|(visible, prev)| visible.entity.entity_id() != *prev);
        if structural_change {
            self.prev_visible_block_ids = visible_blocks
                .iter()
                .map(|v| v.entity.entity_id())
                .collect();
        }

        // Rows mounted together last frame shared one scroll offset, so their
        // adjacent painted-top differences are scroll-free heights. Caching those,
        // not raw positions, is what keeps the window stable while scrolling.
        if !structural_change {
            if let Some((prev_start, prev_end)) = self.prev_render_window {
                let prev_end = prev_end.min(row_first_ids.len());
                for row in prev_start..prev_end.saturating_sub(1) {
                    if let (Some(top), Some(next_top)) = (row_tops[row], row_tops[row + 1]) {
                        let stride = next_top - top;
                        if stride > 0.0 && stride.is_finite() {
                            self.row_stride_cache.insert(row_first_ids[row], stride);
                        }
                    }
                }
            }
        }

        // Unmeasured rows use the minimum block height: a lower bound, so the
        // window over-mounts rather than ever landing on a spacer.
        let estimate = d.block_min_height.max(1.0);
        let strides: Vec<f32> = row_first_ids
            .iter()
            .map(|id| self.row_stride_cache.get(id).copied().unwrap_or(estimate))
            .collect();

        // Bound the cache against block churn, only when it outgrows the live rows.
        if self.row_stride_cache.len() > row_first_ids.len().saturating_mul(2) {
            let live: std::collections::HashSet<EntityId> = row_first_ids.iter().copied().collect();
            self.row_stride_cache.retain(|id, _| live.contains(id));
        }

        // EXP-261 vendoring: embedded mode mounts every row — descriptions
        // are short and the host scroll container clips; windowing math would
        // otherwise run against a zero-sized untracked viewport.
        let render_window = if self.embedded {
            super::RenderWindow {
                run_start: 0,
                run_end: row_first_ids.len(),
                top_h: 0.0,
                bottom_h: 0.0,
            }
        } else {
            Self::rendered_window(
                &strides,
                current_scroll_y,
                viewport_height,
                RENDER_OVERDRAW_PX,
                focus_row,
            )
        };
        self.prev_render_window = Some((render_window.run_start, render_window.run_end));

        // The first mounted row re-applies its `mt`, so drop it from the top
        // spacer to avoid shifting content down by a gap.
        let top_h = match row_top_gaps.get(render_window.run_start) {
            Some(gap) => (render_window.top_h - gap).max(0.0),
            None => render_window.top_h,
        };
        let mut block_rows: Vec<AnyElement> =
            Vec::with_capacity(render_window.run_end - render_window.run_start + 2);
        if top_h > 0.5 {
            block_rows.push(
                div()
                    .w_full()
                    .flex_shrink_0()
                    .h(px(top_h))
                    .into_any_element(),
            );
        }
        for (row_index, element) in row_elements.into_iter().enumerate() {
            if row_index >= render_window.run_start && row_index < render_window.run_end {
                block_rows.push(element);
            }
        }
        if render_window.bottom_h > 0.5 {
            block_rows.push(
                div()
                    .w_full()
                    .flex_shrink_0()
                    .h(px(render_window.bottom_h))
                    .into_any_element(),
            );
        }

        // EXP-277 vendoring: embedded mode has no editor padding, so a
        // document starting/ending on a standalone image would leave nowhere
        // to click above/below it — give those edges a real (hit-testable)
        // margin the gap-click handler resolves.
        let (image_edge_top, image_edge_bottom) = if self.embedded {
            let visible = self.document.flatten_visible_blocks();
            let edge_is_image = |slot: Option<&crate::editor::tree::VisibleBlock>| {
                slot.is_some_and(|visible| {
                    visible.entity.read(cx).renders_as_standalone_image()
                })
            };
            (edge_is_image(visible.first()), edge_is_image(visible.last()))
        } else {
            (false, false)
        };

        let scroll_content = div()
            .id("editor-scroll-inner")
            .flex()
            .flex_col()
            .bg(theme.colors.editor_background)
            .when(image_edge_top, |this| this.pt(px(12.0)))
            .when(image_edge_bottom, |this| this.pb(px(12.0)))
            .when(!self.embedded, |this| {
                // Standalone rows are a fixed-width column centred in the
                // viewport.
                this.items_center()
                    .flex_grow(1.)
                    .h_full()
                    .overflow_y_scroll()
                    .scrollbar_width(px(0.0))
                    .track_scroll(&self.scroll_handle)
            })
            // EXP-261 vendoring: embedded rows fill the host slot, so they
            // STRETCH. Centring them would shrink-wrap each row to its content
            // and centre it — single-line paragraphs would drift to the middle
            // of the column while wrapped ones stayed flush left.
            .when(self.embedded, |this| this.w_full().items_stretch())
            .on_hover(cx.listener(Self::on_editor_hover))
            .capture_any_mouse_down(cx.listener(Self::on_editor_capture_mouse_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_editor_mouse_down))
            .on_mouse_move(cx.listener(Self::on_editor_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_editor_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_editor_mouse_up))
            .when(!self.embedded, |this| {
                this.on_scroll_wheel(cx.listener(Self::on_editor_scroll_wheel))
                    .p(px(d.editor_padding))
                    .pb(px(d.editor_padding
                        + scroll_trigger_padding
                        + scroll_beyond_bottom))
            })
            .children(block_rows);
        let scroll_content = if self.view_mode == super::ViewMode::Rendered {
            scroll_content.on_mouse_down(
                MouseButton::Right,
                cx.listener(Self::on_editor_context_menu_mouse_down),
            )
        } else {
            scroll_content
        };

        let content_area = div()
            .id("editor-scroll")
            .w_full()
            .when(!self.embedded, |this| this.h_full().flex_1())
            .min_w(px(0.0))
            .bg(theme.colors.editor_background)
            .relative()
            .child(scroll_content);

        let content_area = if show_custom_scrollbar {
            let scrollbar_editor = editor.clone();
            let track_origin_y = f32::from(viewport_bounds.origin.y);
            content_area.child(
                div()
                    .id("editor-scrollbar-thumb")
                    .absolute()
                    .occlude()
                    .top(px(thumb_top))
                    .right(px(d.scrollbar_right))
                    .w(px(d.scrollbar_width))
                    .h(px(thumb_height))
                    .rounded(px(999.0))
                    .bg(theme.colors.scrollbar_thumb)
                    .cursor_pointer()
                    .on_hover(cx.listener(Self::on_editor_hover))
                    .on_mouse_down(MouseButton::Left, move |event, _window, cx| {
                        let pointer_offset_y =
                            f32::from(event.position.y) - track_origin_y - thumb_top;
                        let _ = scrollbar_editor.update(cx, |editor, cx| {
                            cx.stop_propagation();
                            editor.start_scrollbar_drag(
                                pointer_offset_y,
                                track_height,
                                thumb_height,
                                max_scroll_y,
                                cx,
                            );
                        });
                    })
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |_thumb_bounds, _, window, _| {
                                window.on_mouse_event({
                                    let editor = editor.clone();
                                    move |_event: &MouseUpEvent, phase, _window, cx| {
                                        if !phase.bubble() {
                                            return;
                                        }
                                        let _ = editor.update(cx, |editor, cx| {
                                            editor.end_scrollbar_drag(cx);
                                        });
                                    }
                                });

                                window.on_mouse_event({
                                    let editor = editor.clone();
                                    move |event: &MouseMoveEvent, phase, _window, cx| {
                                        if !phase.bubble() || !event.dragging() {
                                            return;
                                        }

                                        let pointer_y_in_track =
                                            f32::from(event.position.y) - track_origin_y;
                                        let _ = editor.update(cx, |editor, cx| {
                                            editor.update_scrollbar_drag(pointer_y_in_track, cx);
                                        });
                                    }
                                });
                            },
                        )
                        .size_full(),
                    ),
            )
        } else {
            content_area
        };

        // Repaint when the Cmd/Ctrl follow modifier toggles so a hovered link's
        // hand cursor updates without moving the pointer. `ModifiersChanged` is
        // dispatched along the focused element's path to the root, and this root
        // is an ancestor of every block, so one listener here covers a link in any
        // block while editing. Gated to the secondary modifier so Shift during
        // selection does not repaint.
        let follow_modifier_active = window.modifiers().secondary();

        let base = div()
            .w_full()
            .when(!self.embedded, |this| this.h_full())
            .flex()
            .flex_col()
            .relative()
            .bg(theme.colors.editor_background)
            .font(editor_text_font(&theme.fonts.ui_family))
            .on_modifiers_changed(move |event, window, _| {
                if event.modifiers.secondary() != follow_modifier_active {
                    window.refresh();
                }
            })
            .capture_action(cx.listener(Self::on_copy_capture))
            .capture_action(cx.listener(Self::on_cut_capture))
            .capture_action(cx.listener(Self::on_delete_capture))
            .capture_action(cx.listener(Self::on_delete_back_capture))
            .capture_key_down(cx.listener(Self::on_editor_key_down_capture))
            .on_action(cx.listener(Self::on_undo))
            .on_action(cx.listener(Self::on_redo))
            .on_action(cx.listener(Self::on_toggle_view_mode_action))
            .on_action(cx.listener(Self::on_page_up))
            .on_action(cx.listener(Self::on_page_down))
            .on_action(cx.listener(Self::on_jump_to_top))
            .on_action(cx.listener(Self::on_jump_to_bottom))
            .on_action(cx.listener(Self::on_dismiss_transient_ui));
        let base = base.child(content_area);
        let base = if let Some(context_menu) = self.render_context_menu_overlay(&theme, cx) {
            base.child(context_menu)
        } else {
            base
        };
        if let Some(table_dialog) = self.render_table_insert_dialog_overlay(&theme, cx) {
            base.child(table_dialog)
        } else {
            base
        }
    }
}
