//! VAPP-162: `nav-rail` (General components): THE navigation rail, the
//! Exponential UI Segmented `variant: bar, orientation: vertical` with
//! count badges, painted by the gpui painter from the catalog specimen's
//! rail case (`fixtures/specimens.json` `exponential-ui-segmented`, case
//! `orientation · vertical · bar (the rail) · counts`: Chats 3, Voices,
//! Library 12, Friends).

use gpui::{div, App, Div, ParentElement as _, Styled as _, Window};

use crate::exponential_ui_screen::{specimen_node, styleguide_surface};

pub(crate) const ID: &str = "nav-rail";
pub(crate) const OWNER: &str = "VAPP-162";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let case = specimen_node("exponential-ui-segmented", "specimen-segmented-case-8").expect("the rail case");
    let tree = serde_json::json!({"id": "sg-nav-rail", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "padding": "$spacing.xl"}, "children": [case]});
    div().w_full().child(styleguide_surface("sg-nav-rail", tree, window, cx))
}
