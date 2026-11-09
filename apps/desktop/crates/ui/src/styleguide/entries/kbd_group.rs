//! EXP-1629: `kbd-group` (General components): THE Exponential UI Kbd and
//! KbdGroup macros, painted by the gpui painter from the catalog specimens
//! (`packages/exponential-ui/fixtures/specimens.json` `exponential-ui-kbd`
//! and `exponential-ui-kbd-group`), so the twin cannot drift from the web
//! entry (`apps/styleguide/src/entries/kbd-group.tsx`).

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};

use crate::exponential_ui_screen::{specimen_node, styleguide_surface};

pub(crate) const ID: &str = "kbd-group";
pub(crate) const OWNER: &str = "EXP-1629";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let kbd = specimen_node("exponential-ui-kbd", "specimen_kbd").expect("the Kbd specimen");
    let group = specimen_node("exponential-ui-kbd-group", "specimen_kbd_group").expect("the KbdGroup specimen");
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(16.))
        .child(styleguide_surface("sg-kbd", kbd, window, cx))
        .child(styleguide_surface("sg-kbd-group", group, window, cx))
}
