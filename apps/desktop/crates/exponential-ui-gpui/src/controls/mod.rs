//! # The generic glass controls (VAPP-90)
//!
//! The controls the Exponential IDE (`apps/desktop/crates/ui`) and the SDK's
//! catalog painters share: buttons, fields, the pill, the glass rows, bands,
//! cards and groups, the meter and the context ring, the read-only markdown
//! view. They moved here out of the IDE's `controls.rs` / `surface.rs` /
//! `usage_bar.rs` / `usage_sheet.rs`; the IDE re-exports every one under its
//! old name, so a doc comment's EXP-… rationale still reads as it did there.
//!
//! Every token value comes from the host's [`crate::chrome::Chrome`]:
//! builders that take a `cx` read [`crate::chrome::Chrome::global`], the pure
//! `Div` recipes (no `cx`) take the `&Chrome` as their first argument. The
//! SIZE ladder ([`CTL_LG_H`] / [`CTL_MD_H`] / [`CTL_SM_H`], [`WebControl`],
//! [`PillSize`]) is deliberately not part of the chrome: it is the shadcn
//! sizing layer every client shares and it rides `Styled` extension methods
//! that have no `cx` to read a global from.
//!
//! Colours that ARE the gpui-component theme (foreground, muted, primary,
//! danger, popover, list hover) keep coming from `cx.theme()`.

mod buttons;
mod fades;
mod feedback;
mod inputs;
pub mod markdown;
mod meter;
mod pills;
mod rows;
mod sizing;

pub use buttons::{
    custom_variant_fill, ghost_icon_button, glass_icon_button, text_button, web_switch,
    TextButtonVariant, CUSTOM_VARIANT_ALPHA_FACTOR,
};
pub use fades::{edge_fade_bottom, edge_fade_bottom_soft, edge_fade_top};
pub use feedback::{
    alert, alert_title, empty_state, skeleton, AlertVariant, Skeleton, SKELETON_PULSE,
};
pub use inputs::{
    checkbox, glass_input, search_field, web_textarea, CheckState, SearchFieldSize,
    CHECKBOX_PX, SEARCH_FIELD_SM_H,
};
pub use markdown::{bare_code_markdown_style, markdown_style, markdown_view};
pub use meter::{context_ring, meter};
pub use pills::{
    count_badge, glass_pill, glass_pill_button, glass_pill_button_primary, live_dot, pill_dot,
    BadgeTone, PillMode, PillSize, COUNT_BADGE_MAX, LIVE_DOT_PX,
};
pub use rows::{
    bare_row_shell, disclosure_header, flat_row, flat_row_compact, glass_bar, glass_card,
    glass_group, glass_group_rows, glass_group_rows_bare, glass_input_row, glass_picker_row,
    glass_picker_select, glass_row_card, picker_row_chevron, glass_row_divider, glass_row_input, glass_row_shell,
    glass_section_band, glass_section_band_fold, glass_section_header, glass_tab_item,
    glass_tabs_row, glass_toggle_row, list_row, list_row_divider, picker_value_label,
    property_row, segmented, segmented_item, ChevronSide, FLAT_ROW_COMPACT_H,
    ROW_DESCRIPTION_MAX_W,
};
pub use sizing::{WebControl, WebText, CTL_LG_H, CTL_MD_H, CTL_SM_H};
