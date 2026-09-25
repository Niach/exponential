//! EXP-1029 contract — `IconPicker(set)` under the shared picker API: the
//! icon SET is the parameter (the 96 board/action glyphs or the six device
//! glyphs, `icons.json` `pickable` / `devicePickable`). `board_form::
//! icon_picker` (EXP-575, the square trigger over the swatch grid) moves onto
//! [`Picker`] in EXP-1021 with the grid as the surface's body.

use gpui::AnyElement;

use super::{OnPickerChange, Picker, PickerItem};

/// Which registry set the grid offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IconSet {
    Board,
    Device,
}

pub(crate) fn icon_items(set: IconSet) -> Vec<PickerItem<String>> {
    let names: &[&str] = match set {
        IconSet::Board => domain::contract::BOARD_ICON_VALUES,
        IconSet::Device => domain::contract::DEVICE_ICON_VALUES,
    };
    names
        .iter()
        .map(|name| PickerItem::new((*name).to_string(), *name))
        .collect()
}

pub(crate) fn icon_picker(
    set: IconSet,
    value: Option<String>,
    trigger: AnyElement,
    on_change: OnPickerChange<String>,
) -> Picker<String> {
    Picker::single(icon_items(set), value, trigger, on_change)
}
