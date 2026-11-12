//! #EXP-1739 — an anchored overlay closes when its source moves, scrolls or
//! leaves the tree (`gpui_component::overlay_anchor`, swept by `Root`).
//!
//! Each case opens one overlay on a row inside a scroller under the real
//! `Root`, then scrolls, moves or removes the row, and checks the overlay is
//! gone: the IDE's context menu, a menu opened on chat text, the managed
//! tooltip (buttons) and a popover.

use std::time::Duration;

use gpui::{
    div, point, px, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, Modifiers, MouseButton,
    ParentElement as _, Render, ScrollHandle, ScrollEvent, StatefulInteractiveElement as _, Styled as _,
    VisualTestContext, Window,
};
use gpui_component::button::Button;
use gpui_component::menu::PopupMenuItem;
use gpui_component::popover::Popover;

use crate::controls::PointerContextMenuExt as _;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    ContextMenu,
    TextMenu,
    Tooltip,
    Popover,
}

/// A scroller (400px tall) holding a spacer and the source row.
struct Scene {
    kind: Kind,
    scroll: ScrollHandle,
    /// Pushes the row down (it MOVES without a scroll).
    offset: f32,
    row: bool,
    /// The chat text inside the row (the row itself stays).
    text: bool,
}

const MENU_ROW: &str = "overlay-menu-row";
const POPOVER_BODY: &str = "overlay-popover-body";

fn menu_row() -> PopupMenuItem {
    PopupMenuItem::element(|_, _| div().debug_selector(|| MENU_ROW.into()).child("Copy"))
}

impl Render for Scene {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "Scene").entered();
        let row: gpui::AnyElement = match self.kind {
            Kind::ContextMenu => div()
                .id("source")
                .debug_selector(|| "source".into())
                .h(px(40.))
                .w(px(240.))
                .child("Right-click me")
                .pointer_context_menu(|menu, _, _| menu.item(menu_row()))
                .into_any_element(),
            Kind::TextMenu => div()
                .id("source")
                .debug_selector(|| "source".into())
                .w(px(600.))
                .min_h(px(40.))
                .children(self.text.then(|| crate::markdown::MarkdownView::new("overlay-text", "Install with `curl -fsSL x | sh` today.").selectable(true)))
                .pointer_context_menu(|menu, _, _| menu.item(menu_row()))
                .into_any_element(),
            Kind::Tooltip => div()
                .debug_selector(|| "source".into())
                .child(Button::new("source-button").label("Hover me").tooltip("A tooltip"))
                .into_any_element(),
            Kind::Popover => div()
                .debug_selector(|| "source".into())
                .child(
                    Popover::new("source-popover")
                        .trigger(Button::new("source-trigger").label("Open"))
                        .content(|_, _, _| div().debug_selector(|| POPOVER_BODY.into()).child("Body")),
                )
                .into_any_element(),
        };
        div().size_full().child(
            div()
                .id("scroller")
                .h(px(400.))
                .w(px(800.))
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .child(div().h(px(self.offset + 100.)))
                .children(self.row.then_some(row))
                .child(div().h(px(2000.))),
        )
    }
}

struct Harness<'a> {
    scene: Entity<Scene>,
    cx: &'a mut VisualTestContext,
}

fn harness(kind: Kind, cx: &mut gpui::TestAppContext) -> Harness<'_> {
    cx.update(|cx| {
        gpui_component::init(cx);
        theme::init(cx);
    });
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_root, cx) = cx.add_window_view({
        let slot = slot.clone();
        move |window, cx| {
            let scene = cx.new(|_| Scene { kind, scroll: ScrollHandle::new(), offset: 0., row: true, text: true });
            *slot.borrow_mut() = Some(scene.clone());
            gpui_component::Root::new(scene, window, cx)
        }
    });
    let scene = slot.borrow().clone().expect("the scene was built");
    let mut harness = Harness { scene, cx };
    harness.settle();
    harness
}

impl Harness<'_> {
    fn settle(&mut self) {
        for _ in 0..3 {
            self.cx.update(|window, _| window.refresh());
            self.cx.run_until_parked();
        }
    }

    fn source(&mut self) -> gpui::Bounds<gpui::Pixels> {
        self.cx.debug_bounds("source").expect("the source paints")
    }

    fn shows(&mut self, selector: &'static str) -> bool {
        self.cx.debug_bounds(selector).is_some()
    }

    /// Opens the overlay; returns what probes it.
    fn open(&mut self) -> &'static str {
        let center = self.source().center();
        let kind = self.scene.read_with(self.cx, |scene, _| scene.kind);
        match kind {
            Kind::ContextMenu | Kind::TextMenu => {
                let at = if kind == Kind::TextMenu { self.source().origin + point(px(20.), px(10.)) } else { center };
                self.cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::default());
                self.cx.simulate_mouse_up(at, MouseButton::Right, Modifiers::default());
                self.settle();
                MENU_ROW
            }
            Kind::Tooltip => {
                let button = self.source().origin + point(px(20.), px(10.));
                self.cx.simulate_mouse_move(button, None, Modifiers::default());
                self.cx.executor().advance_clock(Duration::from_millis(800));
                self.settle();
                "tooltip"
            }
            Kind::Popover => {
                let button = self.source().origin + point(px(20.), px(10.));
                self.cx.simulate_click(button, Modifiers::default());
                self.settle();
                POPOVER_BODY
            }
        }
    }

    fn scroll_by_wheel(&mut self, dy: f32) {
        let at = point(px(700.), px(300.));
        self.cx.simulate_event(ScrollEvent {
            position: at,
            delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-dy))),
            modifiers: Modifiers::default(),
            touch_phase: gpui::TouchPhase::Moved,
            source: gpui::ScrollSource::Pointer,
        });
        self.settle();
    }

    fn scroll_by_content(&mut self, dy: f32) {
        self.scene.update(self.cx, |scene, cx| {
            scene.scroll.set_offset(point(px(0.), px(-dy)));
            cx.notify();
        });
        self.settle();
    }

    fn update(&mut self, f: impl FnOnce(&mut Scene)) {
        self.scene.update(self.cx, |scene, cx| {
            f(scene);
            cx.notify();
        });
        self.settle();
    }
}

fn open_then(kind: Kind, cx: &mut gpui::TestAppContext, lose: impl FnOnce(&mut Harness<'_>)) {
    let mut h = harness(kind, cx);
    let probe = h.open();
    assert!(h.shows(probe), "the overlay opens");
    // Nothing moved: it stays.
    h.update(|_| {});
    assert!(h.shows(probe), "an unrelated frame keeps it open");
    lose(&mut h);
    assert!(!h.shows(probe), "the overlay closes once its source is lost");
}

#[gpui::test]
#[cfg_attr(not(debug_assertions), ignore = "debug_bounds reads gpui debug selectors (debug_assertions only)")]
fn a_context_menu_closes_when_its_row_scrolls(cx: &mut gpui::TestAppContext) {
    open_then(Kind::ContextMenu, cx, |h| h.scroll_by_content(60.));
}

#[gpui::test]
#[cfg_attr(not(debug_assertions), ignore = "debug_bounds reads gpui debug selectors (debug_assertions only)")]
fn a_context_menu_closes_when_its_row_moves(cx: &mut gpui::TestAppContext) {
    open_then(Kind::ContextMenu, cx, |h| h.update(|scene| scene.offset = 30.));
}

#[gpui::test]
#[cfg_attr(not(debug_assertions), ignore = "debug_bounds reads gpui debug selectors (debug_assertions only)")]
fn a_context_menu_closes_when_its_row_is_removed(cx: &mut gpui::TestAppContext) {
    open_then(Kind::ContextMenu, cx, |h| h.update(|scene| scene.row = false));
}

#[gpui::test]
#[cfg_attr(not(debug_assertions), ignore = "debug_bounds reads gpui debug selectors (debug_assertions only)")]
fn a_menu_on_chat_text_closes_when_the_text_scrolls(cx: &mut gpui::TestAppContext) {
    open_then(Kind::TextMenu, cx, |h| h.scroll_by_content(60.));
}

/// The message goes (deleted, re-rendered) while the transcript stays.
#[gpui::test]
#[cfg_attr(not(debug_assertions), ignore = "debug_bounds reads gpui debug selectors (debug_assertions only)")]
fn a_menu_on_chat_text_closes_when_the_text_is_removed(cx: &mut gpui::TestAppContext) {
    open_then(Kind::TextMenu, cx, |h| h.update(|scene| scene.text = false));
}

#[gpui::test]
#[cfg_attr(not(debug_assertions), ignore = "debug_bounds reads gpui debug selectors (debug_assertions only)")]
fn a_tooltip_closes_when_its_button_scrolls(cx: &mut gpui::TestAppContext) {
    open_then(Kind::Tooltip, cx, |h| h.scroll_by_wheel(60.));
}

#[gpui::test]
#[cfg_attr(not(debug_assertions), ignore = "debug_bounds reads gpui debug selectors (debug_assertions only)")]
fn a_tooltip_closes_when_its_button_is_removed(cx: &mut gpui::TestAppContext) {
    open_then(Kind::Tooltip, cx, |h| h.update(|scene| scene.row = false));
}

#[gpui::test]
#[cfg_attr(not(debug_assertions), ignore = "debug_bounds reads gpui debug selectors (debug_assertions only)")]
fn a_popover_closes_when_its_trigger_scrolls(cx: &mut gpui::TestAppContext) {
    open_then(Kind::Popover, cx, |h| h.scroll_by_content(60.));
}

#[gpui::test]
#[cfg_attr(not(debug_assertions), ignore = "debug_bounds reads gpui debug selectors (debug_assertions only)")]
fn a_popover_closes_when_its_trigger_is_removed(cx: &mut gpui::TestAppContext) {
    open_then(Kind::Popover, cx, |h| h.update(|scene| scene.row = false));
}

/// The sweep is mounted by `Root` itself, so no window can forget it.
#[test]
fn root_mounts_the_overlay_sweep() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/gpui-component/src/root.rs");
    let text = std::fs::read_to_string(root).expect("the vendored Root is readable");
    assert!(text.contains("overlay_anchor::sweep_layer()"), "Root mounts the overlay sweep (#EXP-1739)");
}

/// Every raw `anchored()` overlay in the desktop crates either goes through
/// `overlay_anchor` or is listed here with why it needs no watch, so a new
/// call site cannot quietly hang an overlay off nothing.
#[test]
fn every_raw_anchored_overlay_is_watched_or_exempt() {
    /// Files whose `anchored()` is no overlay off a source, or one that
    /// re-resolves its position from its source every frame.
    const EXEMPT: &[(&str, &str)] = &[
        ("ui/src/dock.rs", "a full-window backdrop at the origin"),
        ("ui/src/wysiwyg/toolbar.rs", "the format rail follows the live selection every frame"),
        ("ui/src/picker/search_list.rs", "the typeahead follows the caret every frame (EXP-1649)"),
        ("ui/src/menu_window.rs", "the menu layer's own window; its trigger is watched in controls.rs"),
        ("ui/src/shell_regions.rs", "the start menu follows the start button's painted bounds every frame"),
        ("ui/src/chat_tab_composer.rs", "the MCP picker's anchor follows the +'s painted bounds every frame (#EXP-1793)"),
        ("eui-gpui/src/controls/zoomable_image.rs", "a control's own list, inside its control"),
    ];
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut unwatched = Vec::new();
    for root in ["ui/src", "eui-gpui/src", "uicore-gpui/src"] {
        let mut stack = vec![crates.join(root)];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries {
                let path = entry.expect("a readable dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|ext| ext.to_str()) != Some("rs") || path.ends_with("overlay_anchor_tests.rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).expect("a readable source file");
                let code: String = text.lines().filter(|line| !line.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
                if !code.contains("anchored()") || code.contains("overlay_anchor") {
                    continue;
                }
                let name = path.strip_prefix(&crates).unwrap().to_string_lossy().replace('\\', "/");
                if !EXEMPT.iter().any(|(file, _)| *file == name) {
                    unwatched.push(name);
                }
            }
        }
    }
    assert!(
        unwatched.is_empty(),
        "anchored overlays outside `gpui_component::overlay_anchor` (#EXP-1739): {unwatched:?} — watch the source (report_anchor / anchor_at + watch_anchor) or exempt the file with its reason"
    );
}
