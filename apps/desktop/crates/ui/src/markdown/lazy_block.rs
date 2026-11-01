//! #EXP-1546 — a long [`super::MarkdownView`]'s children, laid out only
//! where they show. `gpui::list` re-renders, lays out and paints EVERY
//! visible row on every frame, and a plan card is one row however long it
//! is: scrolling through a plan of hundreds of lines paid for all of them on
//! each frame. A [`LazyBlock`] whose height is KNOWN (measured on an earlier
//! frame at the same width, over the same source) reserves exactly that
//! height in layout and builds nothing; in prepaint, where its bounds are
//! final for THIS frame, it builds, lays out and paints its child only when
//! those bounds cross the clip (the list's viewport). Off-screen blocks cost
//! one fixed-size layout node. The look is unchanged: a block that shows
//! is the same element at the same width under the same text style; a
//! block whose measured height moved (a picture loaded, a pill retitled)
//! records it and refreshes once.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    px, size, ParentElement as _, Styled as _, AnyElement, App, AvailableSpace, Bounds, ElementId, IntoElement, LayoutId, Pixels, SharedString, Window,
};

/// A view goes lazy from this many children on: short bodies (every chat
/// bubble) keep the plain one-pass layout.
pub(crate) const LAZY_MIN_CHILDREN: usize = 24;

/// How many views keep their measured heights (a transcript's visible long
/// bodies a few times over; an entry is one `f32` per child).
const HEIGHTS_CAP: usize = 256;

/// The measured heights of one view's children, valid for one `stamp`
/// (the source, the width, the rem size).
struct Heights {
    stamp: u64,
    heights: Vec<Option<Pixels>>,
    used: u64,
}

thread_local! {
    static HEIGHTS: RefCell<(u64, HashMap<(gpui::WindowId, SharedString), Heights>)> =
        RefCell::new((0, HashMap::new()));
}

/// One view's children heights under `stamp`: the handle its
/// [`LazyBlock`]s read and write.
#[derive(Clone)]
pub(crate) struct HeightTable {
    key: Rc<(gpui::WindowId, SharedString)>,
    stamp: u64,
}

impl HeightTable {
    /// The table of `key` for `stamp` (a new stamp clears the old heights).
    pub(crate) fn new(key: (gpui::WindowId, SharedString), stamp: u64, children: usize) -> Self {
        HEIGHTS.with(|memo| {
            let mut memo = memo.borrow_mut();
            let (tick, entries) = &mut *memo;
            *tick += 1;
            let used = *tick;
            if !entries.contains_key(&key) && entries.len() >= HEIGHTS_CAP {
                let mut ranked: Vec<(u64, (gpui::WindowId, SharedString))> =
                    entries.iter().map(|(key, entry)| (entry.used, key.clone())).collect();
                ranked.sort_unstable_by_key(|(used, _)| *used);
                for (_, stale) in ranked.into_iter().take(HEIGHTS_CAP / 2) {
                    entries.remove(&stale);
                }
            }
            let entry = entries.entry(key.clone()).or_insert_with(|| Heights { stamp, heights: Vec::new(), used });
            if entry.stamp != stamp || entry.heights.len() != children {
                entry.stamp = stamp;
                entry.heights = vec![None; children];
            }
            entry.used = used;
        });
        Self { key: Rc::new(key), stamp }
    }

    fn get(&self, index: usize) -> Option<Pixels> {
        HEIGHTS.with(|memo| {
            let memo = memo.borrow();
            let entry = memo.1.get(&*self.key)?;
            (entry.stamp == self.stamp).then(|| entry.heights.get(index).copied().flatten()).flatten()
        })
    }

    /// Records `height`; whether it differs from what was known.
    fn set(&self, index: usize, height: Pixels) -> bool {
        HEIGHTS.with(|memo| {
            let mut memo = memo.borrow_mut();
            let Some(entry) = memo.1.get_mut(&*self.key) else {
                return false;
            };
            if entry.stamp != self.stamp {
                return false;
            }
            match entry.heights.get_mut(index) {
                Some(slot) if *slot != Some(height) => {
                    let moved = slot.is_some();
                    *slot = Some(height);
                    moved
                }
                _ => false,
            }
        })
    }
}

#[cfg(test)]
thread_local! {
    /// Tests: paint every view eagerly (the before picture) and count the
    /// children built.
    pub(crate) static FORCE_EAGER: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    pub(crate) static BUILT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Whether long views stay on the plain one-pass layout (tests only).
pub(crate) fn forced_eager() -> bool {
    #[cfg(test)]
    return FORCE_EAGER.with(std::cell::Cell::get);
    #[cfg(not(test))]
    false
}

fn count_build() {
    #[cfg(test)]
    BUILT.with(|built| built.set(built.get() + 1));
}

type Build = Box<dyn FnOnce(&mut Window, &mut App) -> AnyElement>;

/// One child of a long view: built eagerly (and measured) while its height
/// is unknown, else deferred to prepaint and skipped off-screen.
pub(crate) struct LazyBlock {
    table: HeightTable,
    index: usize,
    build: Option<Build>,
}

impl LazyBlock {
    pub(crate) fn new(
        table: &HeightTable,
        index: usize,
        build: impl FnOnce(&mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        Self { table: table.clone(), index, build: Some(Box::new(build)) }
    }
}

pub(crate) enum LazyLayout {
    /// Laid out in the parent's pass (its height was unknown).
    Eager(AnyElement),
    /// A fixed-height slot; the child is built in prepaint if it shows.
    Deferred,
}

impl IntoElement for LazyBlock {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl gpui::Element for LazyBlock {
    type RequestLayoutState = LazyLayout;
    type PrepaintState = Option<AnyElement>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&gpui::GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        match self.table.get(self.index) {
            Some(height) => {
                let mut style = gpui::Style::default();
                style.size.width = gpui::relative(1.).into();
                style.size.height = height.into();
                style.flex_shrink = 0.;
                (window.request_layout(style, [], cx), LazyLayout::Deferred)
            }
            None => {
                let build = self.build.take().expect("a lazy block builds once");
                count_build();
                let mut element = build(window, cx);
                (element.request_layout(window, cx), LazyLayout::Eager(element))
            }
        }
    }

    fn prepaint(
        &mut self,
        _id: Option<&gpui::GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        match layout {
            LazyLayout::Eager(element) => {
                element.prepaint(window, cx);
                self.table.set(self.index, bounds.size.height);
                None
            }
            LazyLayout::Deferred => {
                let clip = window.content_mask().bounds;
                if !clip.intersects(&bounds) {
                    return None;
                }
                let build = self.build.take()?;
                count_build();
                // The child's own slot: a column of THIS width (the parent
                // column stretched it there in the eager pass).
                let mut element = gpui_component::v_flex().w(bounds.size.width).child(build(window, cx)).into_any_element();
                let measured = element.layout_as_root(
                    size(AvailableSpace::Definite(bounds.size.width), AvailableSpace::MinContent),
                    window,
                    cx,
                );
                element.prepaint_at(bounds.origin, window, cx);
                if (measured.height - bounds.size.height).abs() > px(0.5) && self.table.set(self.index, measured.height) {
                    window.refresh();
                }
                Some(element)
            }
        }
    }

    fn paint(
        &mut self,
        _id: Option<&gpui::GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        _bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        deferred: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        match layout {
            LazyLayout::Eager(element) => element.paint(window, cx),
            LazyLayout::Deferred => {
                if let Some(element) = deferred {
                    element.paint(window, cx);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use gpui::{div, point, px, size, AppContext as _, IntoElement as _, ListAlignment, ListState, ParentElement as _, Styled as _};

    use super::{BUILT, FORCE_EAGER};
    use crate::markdown::MarkdownView;

    struct Long(ListState, String);

    impl gpui::Render for Long {
        fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
            let source = self.1.clone();
            gpui::list(self.0.clone(), move |_, _, _| {
                div().w_full().child(MarkdownView::new("lazy-long", source.clone()).chat(true)).into_any_element()
            })
            .w_full()
            .h_full()
        }
    }

    /// #EXP-1546: a long body builds every child on its first (measuring)
    /// frame, then only the ones crossing the viewport, at the same height.
    #[gpui::test]
    fn a_long_body_lays_out_only_what_shows(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let source = (0..200).map(|n| format!("Line {n} of a long plan.")).collect::<Vec<_>>().join("\n\n");
        let cx = cx.add_empty_window();
        let height = |eager: bool, cx: &mut gpui::VisualTestContext| {
            FORCE_EAGER.with(|force| force.set(eager));
            let list = ListState::new(1, ListAlignment::Top, px(0.));
            let mut built = Vec::new();
            for _ in 0..3 {
                BUILT.with(|count| count.set(0));
                let (state, source) = (list.clone(), source.clone());
                cx.draw(point(px(0.), px(0.)), size(px(400.), px(300.)), move |_, cx| {
                    cx.new(|_| Long(state, source)).into_any_element()
                });
                built.push(BUILT.with(std::cell::Cell::get));
            }
            FORCE_EAGER.with(|force| force.set(false));
            (list.bounds_for_item(0).map(|bounds| bounds.size.height), built)
        };
        let (eager, _) = height(true, cx);
        let (lazy, built) = height(false, cx);
        assert_eq!(eager, lazy, "the lazy body keeps its height");
        // The eager pass left the width known: the lazy pass's first frame
        // measures every line, the next ones build only what shows.
        assert!(built.contains(&200), "one frame at a known width measures every line: {built:?}");
        let last = built[built.len() - 1];
        assert!(last > 0 && last < 40, "then only the lines on screen build: {built:?}");
    }
}
