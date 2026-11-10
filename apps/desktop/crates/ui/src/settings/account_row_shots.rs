//! #EXP-1764: the bench's two Account rows (`settings_bench`
//! `settings_account_row_*`), closed: today's IDE row
//! (`agent_detail::account_row`: `picker_trigger_bare` with the agent's
//! brand mark, the email and the row chevron, in a `glass_picker_row`) and
//! genUI's `row` Select with `triggerIcon: true` whose chosen option carries
//! `icon: brand-claude`, judged pixel for pixel.
use std::rc::Rc;

use gpui::{div, prelude::*, App, Entity, SharedString, Window};
use serde_json::json;

use eui_gpui::view::SurfaceView;

use super::doc::Ink;

const EMAIL: &str = "ada@example.com";

/// Today's Agent detail Account row with a Claude login chosen.
pub(crate) struct IdeAccountRow;

impl IdeAccountRow {
    pub(crate) fn for_shots(_window: &mut Window, _cx: &mut gpui::Context<Self>) -> Self {
        Self
    }
}

impl Render for IdeAccountRow {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let trigger = crate::trigger_editor::picker_trigger_bare("bench-account".into(), SharedString::from(EMAIL), cx)
            .icon(crate::coding_selects::agent_mark(coding::CodingAgent::Claude))
            .child(crate::surface::picker_row_chevron(cx));
        div().w_full().min_w_0().child(crate::surface::glass_group_rows(vec![crate::surface::glass_picker_row("Account", None, trigger.into_any_element(), cx)]))
    }
}

/// The same row as a genUI document: a `glass_group` holding the `row`
/// Select with `triggerIcon`.
pub(crate) struct EuiAccountRow {
    view: Entity<SurfaceView>,
}

fn document(ink: &Ink) -> serde_json::Value {
    let select = json!({"id": "account", "component": "Select",
        "props": {"triggerVariant": "row", "triggerIcon": true, "label": "Account", "name": "account", "value": "claude",
            "options": [{"label": EMAIL, "value": "claude", "icon": "brand-claude"}, {"label": "ada@example.org", "value": "codex", "icon": "brand-codex"}]}});
    ink.group("account_group", vec![select])
}

impl EuiAccountRow {
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self { view: mount(document(&Ink::of(cx)), window, cx) }
    }
}

fn mount(tree: serde_json::Value, window: &mut Window, cx: &mut App) -> Entity<SurfaceView> {
    let options = eui_gpui::view::SurfaceViewOptions {
        surface_id: "bench-account".into(),
        theme: eui_core::themes::builtin_theme(crate::exponential_ui_host::THEME_ID),
        mode: eui_core::theme::Mode::Dark,
        host: Rc::new(super::doc::ide_plugin()),
        lift_layers: true,
        settings: Some(eui_core::surface::SurfaceSettings { mode: eui_core::theme::ModeSetting::Dark, reduced_motion: theme::motion::reduce_motion(), ..Default::default() }),
        ..Default::default()
    };
    let tree: eui_core::NestedNode = serde_json::from_value(tree).expect("the account document parses");
    cx.new(|cx| {
        let mut view = SurfaceView::new(options, window, cx);
        view.set_nested(tree, cx);
        view
    })
}

impl Render for EuiAccountRow {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div().w_full().min_w_0().child(self.view.clone())
    }
}
