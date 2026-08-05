//! EXP-421: drag & drop. External file drops and internal image-block
//! repositioning share one mechanism: `drop_target_root_index` resolves a
//! window-space Y into a root-block insertion index (block geometry comes
//! from the `last_bounds` every visible row records — images included since
//! the EXP-421 hit-testing fix), and `drop_indicator` renders the insertion
//! line while a drag is over the editor. The editor owns index resolution,
//! the indicator, image insertion and the internal move; the HOST owns
//! file-type policy (inline image vs attachment) via
//! [`crate::api::MarkdownEditorEvent::ExternalFilesDropped`].

use gpui::*;

use super::Editor;
use crate::components::{BlockRecord, PastedImageSource, UndoCaptureKind};

/// Drag payload for repositioning a standalone image block. Carries only the
/// block's entity id — the drop target re-resolves the root index at drop
/// time, so mid-drag document changes cannot corrupt the move.
#[derive(Clone, Debug)]
pub(crate) struct ImageBlockDrag {
    pub entity_id: EntityId,
}

/// The chip gpui renders under the pointer during an image-block drag — a
/// small rounded label, never the image bytes.
pub(crate) struct ImageDragGhost {
    label: SharedString,
    bg: Hsla,
    border: Hsla,
    text: Hsla,
}

impl ImageDragGhost {
    pub(crate) fn new(label: impl Into<SharedString>, bg: Hsla, border: Hsla, text: Hsla) -> Self {
        let label: SharedString = label.into();
        let label = if label.is_empty() {
            SharedString::from("Image")
        } else {
            label
        };
        Self {
            label,
            bg,
            border,
            text,
        }
    }
}

impl Render for ImageDragGhost {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.0))
            .py(px(3.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(self.border)
            .bg(self.bg)
            .text_color(self.text)
            .text_size(px(12.0))
            .child(self.label.clone())
    }
}

impl Editor {
    /// Resolves a window-space Y into `(root insertion index, boundary Y)`.
    /// The boundary Y (window space) is where the indicator line renders:
    /// the midpoint of the gap between the two neighboring roots, or the
    /// top/bottom edge at the document's ends. Roots that have not painted
    /// bounds yet are skipped for geometry but keep their index.
    pub(super) fn drop_target_root_index(&self, y: Pixels, cx: &App) -> (usize, Pixels) {
        // (root index, top, bottom) for every root with known geometry. A
        // root's own bounds cover its first row; its LAST visible descendant
        // supplies the bottom (nested lists render as separate rows).
        let mut rows: Vec<(usize, Pixels, Pixels)> = Vec::new();
        for (index, root) in self.document.root_blocks().iter().enumerate() {
            let Some(own) = root.read(cx).last_bounds else {
                continue;
            };
            let bottom = self
                .document
                .last_visible_descendant(root.entity_id())
                .and_then(|descendant| descendant.read(cx).last_bounds)
                .map(|bounds| bounds.bottom())
                .unwrap_or_else(|| own.bottom())
                .max(own.bottom());
            rows.push((index, own.top(), bottom));
        }

        let Some(first) = rows.first().copied() else {
            return (0, y);
        };
        if y < (first.1 + first.2) / 2.0 {
            return (first.0, first.1);
        }
        for pair in rows.windows(2) {
            let (_, _, prev_bottom) = pair[0];
            let (next_index, next_top, next_bottom) = pair[1];
            if y < (next_top + next_bottom) / 2.0 {
                return (next_index, (prev_bottom + next_top) / 2.0);
            }
        }
        let (last_index, _, last_bottom) = *rows.last().expect("rows is non-empty");
        (last_index + 1, last_bottom)
    }

    /// Updates the indicator from an in-flight drag. Positions outside the
    /// editor's bounds clear it (the drag may leave and re-enter).
    pub(super) fn update_drop_indicator(
        &mut self,
        editor_bounds: Bounds<Pixels>,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let next = editor_bounds
            .contains(&position)
            .then(|| self.drop_target_root_index(position.y, cx));
        if next != self.drop_indicator {
            self.drop_indicator = next;
            cx.notify();
        }
    }

    pub(super) fn clear_drop_indicator(&mut self, cx: &mut Context<Self>) {
        if self.drop_indicator.is_some() {
            self.drop_indicator = None;
            cx.notify();
        }
    }

    /// Inserts one image paragraph per path at the given root index — the
    /// external-file-drop insertion. Same host `ImagePasteHandler` seam as a
    /// paste (staged `draft://` sources heal once the host upload lands);
    /// ONE undo step for the whole batch.
    pub fn insert_image_paths_at(
        &mut self,
        root_index: usize,
        paths: Vec<std::path::PathBuf>,
        cx: &mut Context<Self>,
    ) {
        if paths.is_empty() {
            return;
        }
        self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
        let mut blocks = Vec::new();
        for path in paths {
            match self.pasted_image_markdown(&PastedImageSource::LocalPath(path)) {
                Ok(markdown) => blocks.push(Self::new_block(cx, BlockRecord::paragraph(markdown))),
                Err(err) => self.show_image_paste_error(err, cx),
            }
        }
        if blocks.is_empty() {
            // Nothing materialized — finalize is a no-op on an unchanged
            // document, so the prepared capture just dissolves.
            self.finalize_pending_undo_capture(cx);
            return;
        }
        let root_index = root_index.min(self.document.root_count());
        let last = blocks.last().cloned();
        self.document.insert_blocks_at(None, root_index, blocks, cx);
        if let Some(last) = last {
            self.focus_block(last.entity_id());
        }
        self.rebuild_image_runtimes(cx);
        self.mark_dirty(cx);
        self.finalize_pending_undo_capture(cx);
        cx.notify();
    }

    /// Moves a ROOT block (the internal image reposition) to `root_index`,
    /// as one undo step. Non-root blocks and no-op moves are ignored.
    pub(crate) fn move_root_block(
        &mut self,
        entity_id: EntityId,
        root_index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(source_index) = self
            .document
            .root_blocks()
            .iter()
            .position(|root| root.entity_id() == entity_id)
        else {
            return;
        };
        let mut target_index = root_index.min(self.document.root_count());
        // Removing the source first shifts everything after it left by one.
        if source_index < target_index {
            target_index -= 1;
        }
        if target_index == source_index {
            return;
        }
        self.prepare_undo_capture(UndoCaptureKind::NonCoalescible, cx);
        self.document.with_structure_mutation(cx, |tree, cx| {
            if let Some((block, _location)) = tree.remove_block_by_id_raw(entity_id, cx) {
                tree.insert_blocks_at_raw(None, target_index, vec![block], cx);
            }
        });
        self.mark_dirty(cx);
        self.finalize_pending_undo_capture(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    // No `use super::*` here: gpui's glob would shadow std's `#[test]` with
    // the `gpui::test` attribute macro.
    use gpui::{AppContext as _, Bounds, Context, TestAppContext, point, px, size};

    use super::super::Editor;
    use crate::MarkdownEditorEnvironment;
    use crate::host::{ImagePasteHandler, ImageTarget, PastedImage};

    fn init(cx: &mut TestAppContext) {
        cx.update(|cx| {
            cx.bind_keys(crate::actions::default_key_bindings());
        });
    }

    /// Hand-assigned bounds, the selection.rs test pattern: row `i` spans
    /// y = 32i .. 32i + 24.
    fn assign_visible_block_bounds(editor: &mut Editor, cx: &mut Context<Editor>) {
        for (index, visible) in editor
            .document
            .visible_blocks()
            .to_vec()
            .into_iter()
            .enumerate()
        {
            visible.entity.update(cx, move |block, _cx| {
                block.last_bounds = Some(Bounds::new(
                    point(px(0.0), px(index as f32 * 32.0)),
                    size(px(400.0), px(24.0)),
                ));
            });
        }
    }

    #[test]
    fn drop_target_root_index_resolves_gap_boundaries() {
        let mut cx = TestAppContext::single();
        init(&mut cx);
        let editor = cx.new(|cx| {
            Editor::from_markdown(cx, "alpha\n\n![a](p.png)\n\nbeta".to_string(), None)
        });
        editor.update(&mut cx, |editor, cx| {
            assign_visible_block_bounds(editor, cx);

            // Above the first row's center → before the document.
            assert_eq!(editor.drop_target_root_index(px(5.0), cx), (0, px(0.0)));
            // Between rows 0 and 1 → the gap midpoint (24 + 32) / 2.
            assert_eq!(editor.drop_target_root_index(px(30.0), cx), (1, px(28.0)));
            // Between rows 1 and 2.
            assert_eq!(editor.drop_target_root_index(px(70.0), cx), (2, px(60.0)));
            // Below everything → after the last root, at its bottom edge.
            assert_eq!(editor.drop_target_root_index(px(200.0), cx), (3, px(88.0)));
        });
    }

    #[test]
    fn move_root_block_round_trips_and_undo_restores_order() {
        let mut cx = TestAppContext::single();
        init(&mut cx);
        let editor = cx.new(|cx| {
            Editor::from_markdown(cx, "![a](p.png)\n\nalpha\n\nbeta".to_string(), None)
        });
        editor.update(&mut cx, |editor, cx| {
            let image_id = editor.document.root_blocks()[0].entity_id();
            editor.move_root_block(image_id, 3, cx);
            assert_eq!(editor.markdown(cx), "alpha\n\nbeta\n\n![a](p.png)");

            // ONE undo step restores the original order.
            editor.undo_document(cx);
            assert_eq!(editor.markdown(cx), "![a](p.png)\n\nalpha\n\nbeta");

            // Moving to its own index is a no-op (no dirty, no history entry).
            let history_len = editor.undo_history.len();
            let image_id = editor.document.root_blocks()[0].entity_id();
            editor.move_root_block(image_id, 0, cx);
            assert_eq!(editor.markdown(cx), "![a](p.png)\n\nalpha\n\nbeta");
            assert_eq!(editor.undo_history.len(), history_len);
        });
    }

    /// Stages every dropped file as a `draft://` source — the host upload
    /// pipeline's shape.
    struct DraftPasteHandler;

    impl ImagePasteHandler for DraftPasteHandler {
        fn materialize(
            &self,
            source: PastedImage,
            _document_base_dir: Option<&std::path::Path>,
        ) -> anyhow::Result<ImageTarget> {
            let PastedImage::LocalPath(path) = source else {
                anyhow::bail!("unexpected non-path drop");
            };
            let stem = path.file_stem().unwrap().to_string_lossy().to_string();
            Ok(ImageTarget {
                alt: stem.clone(),
                source: format!("draft://{stem}.png"),
            })
        }
    }

    #[test]
    fn external_image_drop_serializes_a_draft_paragraph_at_the_index() {
        let mut cx = TestAppContext::single();
        init(&mut cx);
        let editor = cx.new(|cx| {
            let environment = MarkdownEditorEnvironment {
                image_paste_handler: Arc::new(DraftPasteHandler),
                ..Default::default()
            };
            Editor::with_environment("alpha\n\nbeta", environment, cx)
        });
        editor.update(&mut cx, |editor, cx| {
            editor.insert_image_paths_at(
                1,
                vec![
                    std::path::PathBuf::from("/tmp/pic.png"),
                    std::path::PathBuf::from("/tmp/shot.png"),
                ],
                cx,
            );
            assert_eq!(
                editor.markdown(cx),
                "alpha\n\n![pic](draft://pic.png)\n\n![shot](draft://shot.png)\n\nbeta"
            );

            // The whole batch is ONE undo step.
            editor.undo_document(cx);
            assert_eq!(editor.markdown(cx), "alpha\n\nbeta");
        });
    }
}
