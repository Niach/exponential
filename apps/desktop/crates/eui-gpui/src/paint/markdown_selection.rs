//! #VAPP-258: a Markdown text unit joined to the HOST's window-level text
//! selection (gpui-base's `TextSelectionLayer`): one participant per unit,
//! ordered by its place on screen, so a sweep runs across every Markdown leaf
//! of every surface in the window and the host's copy yields the whole range
//! in reading order. [`crate::host::HostPlugin::markdown_window_selection`].
//!
//! #EXP-1739: a chip or an inline code span is a run of non-breaking spaces
//! under its own box, so the unit copies through [`super::markdown::copy_text`]
//! (the chip's label), never the placeholder: a selection inside a code span
//! was all whitespace, which the layer drops, and Copy came back empty.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use gpui::{point, AnyElement, App, Bounds, ElementId, Hsla, IntoElement, Pixels, SharedString, Subscription, TextLayout, Window};
use gpui_base::{TextSelectionHandle, TextSelectionRegistration, TextSelectionRun};

/// The unit's element, its shaped text, its chips' byte ranges with their
/// labels and the highlight colour.
pub struct SelectableUnit {
    id: ElementId,
    child: AnyElement,
    layout: TextLayout,
    text: SharedString,
    chips: Vec<(Range<usize>, String)>,
    highlight: Hsla,
}

impl SelectableUnit {
    pub fn new(id: impl Into<SharedString>, child: AnyElement, layout: TextLayout, text: SharedString, highlight: Hsla) -> Self {
        Self { id: ElementId::Name(id.into()), child, layout, text, chips: Vec::new(), highlight }
    }

    /// The inline chips (and code spans) in `text`: copy gives their labels.
    pub fn chips(mut self, chips: Vec<(Range<usize>, String)>) -> Self {
        self.chips = chips;
        self
    }
}

/// What the unit's copy reads: the last painted text, chips and selected range.
#[derive(Default)]
pub struct Painted {
    text: SharedString,
    chips: Vec<(Range<usize>, String)>,
    range: Option<Range<usize>>,
}

/// The copy text of `painted`'s selected range (chips as their labels).
fn painted_copy(painted: &Painted) -> String {
    painted.range.clone().map(|range| super::markdown::copy_text(&painted.text, range, &painted.chips)).unwrap_or_default()
}

/// Stable document order from geometry: top to bottom, then left to right,
/// quantized to 1/8 px (equal orders do not fall back to registration order;
/// a per-frame counter re-publishes every frame).
fn document_order(bounds: &Bounds<Pixels>) -> u64 {
    let quantize = |value: Pixels| (((f32::from(value) + 131_072.0).max(0.0)) * 8.0) as u64;
    (quantize(bounds.origin.y) << 24) | (quantize(bounds.origin.x) & 0xFF_FFFF)
}

struct UnitState {
    selection: TextSelectionHandle,
    painted: Rc<RefCell<Painted>>,
    _refresh: Subscription,
}

/// The row boxes `range` covers in `layout`: every hard line (a code block's
/// `\n`-separated lines), one box per wrapped row. Rows stack the way
/// [`TextLayout::position_for_index`] places them, so the boxes sit where the
/// layer's hit-test put the characters.
pub fn range_boxes(layout: &TextLayout, range: std::ops::Range<usize>) -> Vec<Bounds<Pixels>> {
    if range.start >= range.end {
        return Vec::new();
    }
    let bounds = layout.bounds();
    let line_height = layout.line_height();
    let mut out = Vec::new();
    let (mut line_start, mut line_top) = (0usize, bounds.top());
    for line in layout.line_layouts().iter() {
        let line_end = line_start + line.len();
        if line_start >= range.end {
            break;
        }
        if line_end >= range.start {
            let mut rows: Vec<usize> = Vec::with_capacity(line.wrap_boundaries.len() + 2);
            rows.push(0);
            for boundary in line.wrap_boundaries.iter() {
                let run = &line.unwrapped_layout.runs[boundary.run_ix];
                rows.push(run.glyphs[boundary.glyph_ix].index.min(line.len()));
            }
            rows.push(line.len());
            rows.dedup();
            if rows.len() < 2 {
                rows.push(line.len());
            }
            // The local range on this line; a selection running past its end
            // takes the line break as a sliver (an empty line still shows).
            let local = range.start.saturating_sub(line_start)..range.end.saturating_sub(line_start).min(line.len());
            let past_end = range.end > line_end;
            for row in 0..rows.len().saturating_sub(1) {
                let (row_start, row_end) = (rows[row], rows[row + 1]);
                let last_row = row + 2 == rows.len();
                let start = local.start.max(row_start).min(row_end);
                let end = local.end.min(row_end).max(row_start);
                let sliver = past_end && last_row && local.start <= row_end;
                if start >= end && !sliver {
                    continue;
                }
                let x0 = line.unwrapped_layout.x_for_index(row_start);
                let left = line.unwrapped_layout.x_for_index(start) - x0;
                let mut right = line.unwrapped_layout.x_for_index(end) - x0;
                if sliver {
                    right += line_height * 0.25;
                }
                let y = line_top + line_height * row as f32;
                out.push(Bounds::from_corners(point(bounds.left() + left, y), point(bounds.left() + right, y + line_height)));
            }
        }
        line_top += line.size(line_height).height;
        line_start = line_end + 1;
    }
    out
}

impl IntoElement for SelectableUnit {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl gpui::Element for SelectableUnit {
    type RequestLayoutState = ();
    type PrepaintState = (TextSelectionHandle, Rc<RefCell<Painted>>);

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&gpui::GlobalElementId>, _: Option<&gpui::InspectorElementId>, window: &mut Window, cx: &mut App) -> (gpui::LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(&mut self, id: Option<&gpui::GlobalElementId>, _: Option<&gpui::InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) -> Self::PrepaintState {
        self.child.prepaint(window, cx);
        let (selection, painted) = window.with_element_state::<UnitState, _>(id.expect("a SelectableUnit has an id"), |retained, window| {
            let state = retained.unwrap_or_else(|| {
                let selection = TextSelectionHandle::new(self.text.clone(), cx);
                let refresh = selection.refresh_window_on_change(window, cx);
                let painted = Rc::new(RefCell::new(Painted::default()));
                let copied = painted.clone();
                selection.copy_with(move |_| painted_copy(&copied.borrow()), cx);
                UnitState { selection, painted, _refresh: refresh }
            });
            ((state.selection.clone(), state.painted.clone()), state)
        });
        // #EXP-1739: the text is what a menu opened on it follows (the
        // vendored gpui-component only: the `overlay-anchor` feature).
        #[cfg(feature = "overlay-anchor")]
        gpui_component::overlay_anchor::report_anchor("overlay-anchor", bounds, window, cx);
        let hitbox = window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal);
        selection.register(TextSelectionRegistration::new(hitbox, bounds).with_document_order(document_order(&bounds)).with_text_bounds(vec![bounds]), window, cx);
        (selection, painted)
    }

    fn paint(&mut self, _: Option<&gpui::GlobalElementId>, _: Option<&gpui::InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), (selection, painted): &mut Self::PrepaintState, window: &mut Window, cx: &mut App) {
        let projection = selection.update_runs(&[TextSelectionRun::new(self.text.clone(), self.layout.clone(), bounds).with_document_order(0)], cx);
        let range = projection.ranges().first().cloned().flatten();
        if let Some(range) = range.clone() {
            for segment in range_boxes(&self.layout, range) {
                window.paint_quad(gpui::fill(segment, self.highlight));
            }
        }
        *painted.borrow_mut() = Painted { text: self.text.clone(), chips: std::mem::take(&mut self.chips), range };
        self.child.paint(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{div, px, Context, ParentElement, Render, Styled, StyledText, TestAppContext, VisualTestContext};

    /// A code block is ONE unit with `\n`-separated lines: a selection over
    /// it highlights every line, not only the first (Danny's bug).
    #[gpui::test]
    fn a_selection_highlights_every_line_of_a_multi_line_unit(cx: &mut TestAppContext) {
        let text = "cd ~/repo\n\n# build\nscripts/mkboot.sh build";
        let styled = StyledText::new(text);
        let layout = styled.layout().clone();
        struct View(StyledText);
        impl Render for View {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                let text: SharedString = "cd ~/repo\n\n# build\nscripts/mkboot.sh build".into();
                let styled = std::mem::replace(&mut self.0, StyledText::new(text));
                div().w(px(600.)).child(styled)
            }
        }
        let (_view, cx) = cx.add_window_view(|_, _| View(styled));
        let cx: &mut VisualTestContext = cx;
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let line_height = layout.line_height();
        let top = layout.bounds().top();

        let boxes = range_boxes(&layout, 0..text.len());
        let rows: Vec<usize> = boxes.iter().map(|b| ((b.top() - top) / line_height).round() as usize).collect();
        assert_eq!(rows, vec![0, 1, 2, 3], "one box per line, the empty one too: {boxes:?}");
        assert!(boxes[3].size.width > boxes[2].size.width, "each box spans its own line's text");

        // From inside line 1 to inside line 4: the first box starts mid-line.
        let from = "cd ".len();
        let to = text.rfind("build").unwrap() + 2;
        let boxes = range_boxes(&layout, from..to);
        assert_eq!(boxes.len(), 4);
        assert!(boxes[0].left() > layout.bounds().left(), "starts at its caret");
        assert_eq!(boxes[3].left(), layout.bounds().left(), "the last line starts at the margin");

        // A range inside the third line only.
        let start = text.find("# build").unwrap();
        let boxes = range_boxes(&layout, start..start + 7);
        assert_eq!(boxes.len(), 1);
        assert_eq!(((boxes[0].top() - top) / line_height).round() as usize, 2);
    }
}
