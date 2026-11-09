//! VAPP-217 (eApps U2): `record-table` (General components): THE Exponential UI
//! RecordTable macro, painted by the gpui painter from the catalog specimen
//! (`packages/exponential-ui/fixtures/specimens.json` `exponential-ui-record-table`),
//! so the twin cannot drift from the web entry
//! (`apps/styleguide/src/entries/record-table.tsx`).

use gpui::{div, App, Div, ParentElement as _, Styled as _, Window};

use crate::exponential_ui_screen::{specimen_node, styleguide_surface};

pub(crate) const ID: &str = "record-table";
pub(crate) const OWNER: &str = "VAPP-217";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let tree = specimen_node("exponential-ui-record-table", "specimen_record_table").expect("the RecordTable specimen");
    div().w_full().child(styleguide_surface("sg-record-table", tree, window, cx))
}
