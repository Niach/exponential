//! EXP-1074 — `menu` (2 General components): the one floating-menu surface
//! and its row recipe, in two densities (`theme::tokens::menu::{pointer,
//! touch}`).
//!
//! The IDE's menus are gpui-component's `PopupMenu` (`.context_menu(…)` on
//! the issue rows, the row/tab overflow menus): their geometry — 26px rows,
//! 8px side padding — is the crate's, not ours; only `min_w`/`max_w` are.
//! The tokens record the pointer density every other pointer surface (web at
//! md+) draws, 36 / 8 / 8, so the IDE's row height is a LEFTOVER by decision:
//! the IDE reads well at its size and patching the vendored crate is its own
//! issue.
//!
//! EXP-1092: the demo is a REAL `PopupMenu`, built once and kept alive by
//! `window.use_keyed_state` (a menu is an entity), drawn open in place: the
//! header label, plain rows with their glyphs, a checked row, a submenu
//! trigger, a disabled row and the red destructive row
//! (`controls::danger_menu_item`) with no divider above it (EXP-697).
//!
//! EXP-1185: the description editor's right-click menu (Cut · Copy · Paste ·
//! Delete, then Insert ›) is NOT this component: it lives inside the vendored
//! `gpui-markdown-editor` and draws that crate's own menu recipe
//! (`theme.dimensions.menu_*`, dialog colours), as its Insert menu always did.
//!
//! EXP-1249: THE pointer menu. Every row points: rows built with the
//! `controls::pointer_*` helpers fill their item under `cursor_pointer`, and
//! a `controls::PointerMenu` lays the pointer layer over the whole surface
//! (the `Submenu` rows have no element form). The second specimen is the
//! composer's "+" menu, drawn by the REAL builder (`chat_screen::plus_menu`)
//! over a sample of the launch options: Implement issue ›, Run action ›, Add
//! file or image | Effort ›, Subagents ›, Ultracode | MCP servers ›,
//! Computer use. The issue specimen keeps the crate's plain rows: it mirrors
//! the issue list's context menu, which has not moved onto the helpers yet.
//!
//! EXP-1228: the run view's text menus are not this component either: they are
//! the OS's own menu (gpui-component `NativeMenu`). The composer's is the
//! `Textarea` built-in (Cut · Copy · Paste · Select All), and the transcript's
//! is Copy alone (`steer_viewer::on_feed_context_menu`). An image or media
//! tile inside the transcript keeps its `PopupMenu` (`controls::claim_right_click`).

use gpui::{div, App, Context, Div, Entity, IntoElement, ParentElement as _, Render, Styled as _, Window};
use gpui_component::{
    menu::PopupMenu,
    Icon, Side,
};

use crate::icons::{registry, ExpIcon};

pub(crate) const ID: &str = "menu";
pub(crate) const OWNER: &str = "EXP-1074";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let demo = window.use_keyed_state("sg-menu", cx, MenuDemo::new);
    let composer = window.use_keyed_state("sg-menu-composer", cx, ComposerMenuDemo::new);
    div()
        .flex()
        .flex_row()
        .items_start()
        .gap_6()
        .child(crate::controls::with_pointer_layer(demo))
        .child(crate::controls::with_pointer_layer(composer))
}

/// The composer "+" menu (EXP-1249), built by the real row builder over a
/// sample of the launch options (the composer's own view is not here, so
/// its rows act on nothing).
struct ComposerMenuDemo {
    menu: Entity<PopupMenu>,
}

impl ComposerMenuDemo {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let options = composer_menu_sample();
        let menu = PopupMenu::build(window, cx, move |menu, window, cx| {
            crate::chat_screen::plus_menu(
                menu,
                &gpui::WeakEntity::new_invalid(),
                Some(&options),
                window,
                cx,
            )
        });
        Self { menu }
    }
}

impl Render for ComposerMenuDemo {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.menu.clone()
    }
}

/// The draft's state: High effort, Opus subagents, Ultracode off, two MCP
/// servers picked, Computer use on.
pub(crate) fn composer_menu_sample() -> crate::launch_options::ComposerMenu {
    let server = |id: &str, name: &str| crate::launch_options::McpServerOption {
        id: id.into(),
        name: name.into(),
        ..Default::default()
    };
    crate::launch_options::ComposerMenu {
        effort_label: coding::CodingAgent::Claude.effort_label(),
        effort_choices: crate::coding_selects::effort_choices_for(coding::CodingAgent::Claude),
        effort_picked: "high".into(),
        effort_locked: false,
        subagent_picked: Some("opus".into()),
        ultracode: Some(false),
        mcp_servers: vec![server("linear", "Linear"), server("sentry", "Sentry")],
        mcp_selected: vec!["linear".into(), "sentry".into()],
        computer_use: Some(true),
    }
}

/// Holds the menu entity so it survives frames; draws it as-is.
struct MenuDemo {
    menu: Entity<PopupMenu>,
}

impl MenuDemo {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let menu = PopupMenu::build(window, cx, |menu, window, cx| {
            menu.check_side(Side::Right)
                .label("EXP-42")
                .item(crate::controls::pointer_label_item("Open issue", false).icon(Icon::from(ExpIcon::Pencil)))
                .item(crate::controls::pointer_label_item("Copy issue ID", false).icon(Icon::from(ExpIcon::Copy)))
                .item(
                    crate::controls::pointer_label_item("Show sub-issues", false)
                        .icon(Icon::new(registry::UI_ASSIGNEE))
                        .checked(true),
                )
                .submenu_with_icon(
                    Some(Icon::from(ExpIcon::Tag)),
                    "Labels",
                    window,
                    cx,
                    |menu, _, _| {
                        menu.check_side(Side::Right)
                            .item(crate::controls::pointer_label_item("Bug", false).checked(true))
                            .item(crate::controls::pointer_label_item("Feature", false))
                    },
                )
                .item(
                    crate::controls::pointer_label_item("Move to board", true)
                        .icon(Icon::from(ExpIcon::SquareKanban)),
                )
                .item(crate::controls::danger_menu_item(
                    "Delete issue",
                    Icon::new(registry::UI_DELETE),
                    cx,
                ))
        });
        Self { menu }
    }
}

impl Render for MenuDemo {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.menu.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The specimen's sample reads like the draft: every row of the "+"
    /// menu shows, with the draft's values.
    #[test]
    fn the_composer_specimen_shows_every_row() {
        let sample = composer_menu_sample();
        assert_eq!(sample.effort_value(), "High");
        assert_eq!(sample.subagent_value().as_deref(), Some("Opus"));
        assert_eq!(sample.mcp_value().as_deref(), Some("2"));
        assert_eq!(sample.computer_use, Some(true));
        assert_eq!(sample.ultracode, Some(false));
    }
}
