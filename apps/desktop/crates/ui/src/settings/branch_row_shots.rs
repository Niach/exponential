//! #EXP-1629: the bench's two Branch rows (`settings_bench`
//! `settings_branch_menu_*`): today's IDE row (`board_form::branch_value_menu`
//! in a `glass_group_rows`) and genUI's `Select` row whose `main` option
//! carries `trailing: "· default"`, each opened, so the popup row
//! `main · default` is judged pixel for pixel.
use std::collections::VecDeque;
use std::rc::Rc;

use gpui::{div, prelude::*, App, Entity, Window};
use serde_json::json;

use eui_gpui::view::SurfaceView;

use super::doc::Ink;
use crate::board_form::{branch_value_menu, BranchLoad};

/// Today's Board detail Branch row and, under it, its menu as the row's
/// `pointer_menu` builds it (`board_form::branch_dropdown`, branches
/// `main` + `develop` with `main` the repo default: `single_select_menu`
/// over `pointer_label_item("main · default")`, the shown one checked),
/// drawn in place (a headless frame never paints the trigger's popover),
/// its first row selected as the menu's dev open does (Down from nothing).
pub(crate) struct IdeBranchRow {
    menu: Option<Entity<gpui_component::menu::PopupMenu>>,
}

impl IdeBranchRow {
    pub(crate) fn for_shots(_window: &mut Window, _cx: &mut gpui::Context<Self>) -> Self {
        Self { menu: None }
    }
}

impl Render for IdeBranchRow {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        use gpui::{px, Focusable as _};
        let row = branch_value_menu::<Self>("bench-branch", "main", Some("main".into()), false, |_, _| Some(BranchLoad::Ready(vec!["main".into(), "develop".into()])), |_, _| {}, |_, _, _| {}, cx);
        let menu = match self.menu.clone() {
            Some(menu) => menu,
            None => {
                let menu = gpui_component::menu::PopupMenu::build(window, cx, |menu, _, _| {
                    let mut menu = crate::pickers::single_select_menu(menu.scrollable(true).max_h(px(320.)));
                    for (name, default) in [("main", true), ("develop", false)] {
                        let label = if default { format!("{name} \u{00b7} default") } else { name.to_string() };
                        menu = menu.item(crate::controls::pointer_label_item(gpui::SharedString::from(label), false).checked(name == "main"));
                    }
                    menu
                });
                menu.focus_handle(cx).focus(window, cx);
                let handle = menu.focus_handle(cx);
                window.on_next_frame(move |window, cx| handle.dispatch_action(&gpui_base::actions::SelectDown, window, cx));
                self.menu = Some(menu.clone());
                menu
            }
        };
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(40.))
            .child(crate::surface::glass_group_rows(vec![crate::surface::glass_picker_row("Branch", None, row, cx)]))
            .child(div().flex().child(menu))
    }
}

/// The same row as a genUI document: a `glass_group` holding the `row`
/// Select, opened by pressing its trigger.
pub(crate) struct EuiBranchRow {
    view: Entity<SurfaceView>,
    presses: (VecDeque<String>, u8),
}

fn document(ink: &Ink) -> serde_json::Value {
    let select = json!({"id": "branch", "component": "Select",
        "props": {"triggerVariant": "row", "label": "Branch", "name": "branch", "value": "main",
            "options": [{"label": "main", "value": "main", "trailing": "\u{00b7} default"}, {"label": "develop", "value": "develop"}]},
        "on": {"open": {"event": {"name": "branches"}}}});
    ink.group("branch_group", vec![select])
}

impl EuiBranchRow {
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self { view: mount(document(&Ink::of(cx)), window, cx), presses: (VecDeque::from(["branch.trigger".to_string()]), 2) }
    }

    /// The trigger's press once it is laid out (two painted frames in); an
    /// open popup's enter fade is run out by hand (the headless clock
    /// stands still).
    fn press(&mut self, cx: &mut gpui::Context<Self>) {
        let (queue, wait) = &mut self.presses;
        if queue.is_empty() {
            if !self.view.read(cx).layers().is_empty() {
                self.view.update(cx, |view, cx| view.advance_clock(std::time::Duration::from_millis(500), cx));
            }
            return;
        }
        if *wait > 0 {
            *wait -= 1;
            cx.notify();
            return;
        }
        let id = queue.pop_front().unwrap_or_default();
        self.view.update(cx, |view, cx| {
            if let Some(index) = view.index_of(&id) {
                view.fire(index, "press", None, cx);
            }
        });
        cx.notify();
    }
}

fn mount(tree: serde_json::Value, window: &mut Window, cx: &mut App) -> Entity<SurfaceView> {
    let options = eui_gpui::view::SurfaceViewOptions {
        surface_id: "bench-branch".into(),
        theme: eui_core::themes::builtin_theme(crate::exponential_ui_host::THEME_ID),
        mode: eui_core::theme::Mode::Dark,
        host: Rc::new(super::doc::ide_plugin()),
        lift_layers: true,
        settings: Some(eui_core::surface::SurfaceSettings { mode: eui_core::theme::ModeSetting::Dark, reduced_motion: theme::motion::reduce_motion(), ..Default::default() }),
        ..Default::default()
    };
    let tree: eui_core::NestedNode = serde_json::from_value(tree).expect("the branch document parses");
    cx.new(|cx| {
        let mut view = SurfaceView::new(options, window, cx);
        view.set_nested(tree, cx);
        view
    })
}

impl Render for EuiBranchRow {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.press(cx);
        div().w_full().min_w_0().child(self.view.clone())
    }
}
