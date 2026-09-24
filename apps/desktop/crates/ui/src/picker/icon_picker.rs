//! EXP-1029 contract — `IconPicker(set)` under the shared picker API: the
//! icon SET is the parameter (the 96 board/action glyphs or the six device
//! glyphs, `icons.json` `pickable` / `devicePickable`). `board_form::
//! icon_picker` (EXP-575, the square trigger over the swatch grid) moves onto
//! [`Picker`] in EXP-1021 with the grid as the surface's body.

use gpui::{px, AnyElement, IntoElement as _, ParentElement as _, SharedString, Styled as _};
use gpui_component::v_flex;

use super::{OnPickerChange, Picker, PickerItem};

/// Which registry set the grid offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IconSet {
    Board,
    Device,
}

impl IconSet {
    /// The curated names this set offers, straight from the contract.
    pub(crate) fn names(self) -> &'static [&'static str] {
        match self {
            IconSet::Board => domain::contract::BOARD_ICON_VALUES,
            IconSet::Device => domain::contract::DEVICE_ICON_VALUES,
        }
    }

    /// Which set a caller's option list IS (`board_form::icon_picker` takes
    /// the list, not the set — one picker, two sets, never a fork).
    pub(crate) fn of(options: &[&str]) -> Self {
        if options.len() == domain::contract::DEVICE_ICON_VALUES.len()
            && options
                .iter()
                .all(|name| domain::contract::DEVICE_ICON_VALUES.contains(name))
        {
            IconSet::Device
        } else {
            IconSet::Board
        }
    }
}

pub(crate) fn icon_items(set: IconSet) -> Vec<PickerItem<String>> {
    set.names()
        .iter()
        .map(|name| {
            PickerItem::new((*name).to_string(), *name)
                .icon(crate::icons::board_icon_name_glyph(name))
        })
        .collect()
}

/// EXP-1021: the icon picker is the ONE picker whose surface body is a GRID
/// (96 board glyphs read as a swatch wall, never as 96 rows), so it hands
/// the primitive a `panel` — the popover, its trigger and its dismiss stay
/// the primitive's. `board_form::icon_picker` overrides this panel to add
/// its optional "No icon" reset.
pub(crate) fn icon_picker(
    set: IconSet,
    value: Option<String>,
    trigger: AnyElement,
    on_change: OnPickerChange<String>,
) -> Picker<String> {
    let names = set.names();
    let panel_change = on_change.clone();
    let selected = SharedString::from(value.clone().unwrap_or_default());
    Picker::single(icon_items(set), value, trigger, on_change)
        .empty_text("No icons")
        .panel(move |dismiss, _window, cx| {
            let on_change = panel_change.clone();
            v_flex()
                .w(px(crate::board_form::icon_grid_width(names.len())))
                .p_1()
                .child(crate::board_form::icon_swatch_grid(
                    SharedString::from("picker-icon"),
                    names,
                    &selected,
                    move |name, window, cx| {
                        on_change(vec![name.to_string()], window, cx);
                        dismiss(window, cx);
                    },
                    cx,
                ))
                .into_any_element()
        })
}
