//! #EXP-1752 — an OPEN `PointerMenu` follows its rows' data: the composer's
//! "+" menu opened before the team's machines arrived showed an empty Device
//! list until it was reopened. With `.live(key)` the open menu rebuilds in
//! place when the key changes, the hovered row (and so its open submenu) kept.

use gpui::{
    div, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, Modifiers, ParentElement as _, Render, Styled as _,
    VisualTestContext, Window,
};
use gpui_component::button::Button;
use gpui_component::menu::PopupMenuItem;

/// A "+" whose menu lists `devices` — the composer's Device rows.
struct Scene {
    devices: Vec<&'static str>,
    live: bool,
}

fn device_row(name: &'static str) -> PopupMenuItem {
    PopupMenuItem::element(move |_, _| div().debug_selector(move || format!("device-{name}")).child(name))
}

fn key(devices: &[&'static str]) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    devices.hash(&mut hasher);
    hasher.finish()
}

impl Render for Scene {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "Scene").entered();
        let devices = self.devices.clone();
        let menu = crate::controls::PointerMenu::new(
            "plus-menu",
            Button::new("plus").label("+"),
            move |menu, _, _| devices.iter().fold(menu.label("Device"), |menu, name| menu.item(device_row(name))),
        );
        let menu = if self.live { menu.live(key(&self.devices)) } else { menu };
        div().size_full().p_4().child(div().debug_selector(|| "plus".into()).child(menu))
    }
}

struct Harness<'a> {
    scene: Entity<Scene>,
    cx: &'a mut VisualTestContext,
}

fn harness(live: bool, cx: &mut gpui::TestAppContext) -> Harness<'_> {
    cx.update(|cx| {
        gpui_component::init(cx);
        theme::init(cx);
    });
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_root, cx) = cx.add_window_view({
        let slot = slot.clone();
        move |window, cx| {
            let scene = cx.new(|_| Scene { devices: Vec::new(), live });
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

    fn open(&mut self) {
        let at = self.cx.debug_bounds("plus").expect("the + paints").center();
        self.cx.simulate_click(at, Modifiers::default());
        self.settle();
    }

    fn shows(&mut self, name: &str) -> bool {
        self.cx.debug_bounds(Box::leak(format!("device-{name}").into_boxed_str())).is_some()
    }

    /// The machines land (the `devices` shape, the relay's config).
    fn arrive(&mut self, devices: Vec<&'static str>) {
        self.scene.update(self.cx, |scene, cx| {
            scene.devices = devices;
            cx.notify();
        });
        self.settle();
    }
}

#[gpui::test]
fn an_open_menu_shows_devices_that_arrive_after_it_opened(cx: &mut gpui::TestAppContext) {
    let mut h = harness(true, cx);
    h.open();
    assert!(!h.shows("macbook"), "nothing has arrived yet");
    h.arrive(vec!["macbook", "mint"]);
    assert!(h.shows("macbook") && h.shows("mint"), "the open menu lists the machines that arrived");
    h.arrive(vec!["mint"]);
    assert!(!h.shows("macbook") && h.shows("mint"), "and drops the one that left");
}

#[gpui::test]
fn without_a_live_key_the_open_menu_keeps_its_rows(cx: &mut gpui::TestAppContext) {
    // The pre-fix behaviour, kept for menus with static rows: proves the
    // case above exercises the rebuild, not a reopen.
    let mut h = harness(false, cx);
    h.open();
    h.arrive(vec!["macbook"]);
    assert!(!h.shows("macbook"));
}
