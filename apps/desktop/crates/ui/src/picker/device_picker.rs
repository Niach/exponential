//! EXP-1029 contract — the device picker: the machines a run may start on,
//! each by its device glyph (contract `deviceIcon`) + name, offline ones
//! disabled with the reason as the description. The composer, the
//! automation editor and the workflow runner row pick one.

use gpui::AnyElement;

use crate::icons::device_icon;

use super::{OnPickerChange, Picker, PickerItem};

/// A device row as the picker reads it.
pub(crate) struct DevicePickerDevice {
    pub id: String,
    pub name: String,
    /// Contract `deviceIcon`; `None` = the kind default.
    pub icon: Option<String>,
    /// Contract `deviceKind` = `server` (web `kind`), read ONLY for that
    /// default: a headless daemon falls back to the server glyph, every other
    /// machine to the device one.
    pub server: bool,
    /// A muted reason under the name (`Offline`, `Update to run workflows`).
    pub description: Option<String>,
    pub disabled: bool,
}

pub(crate) fn device_items(devices: &[DevicePickerDevice]) -> Vec<PickerItem<String>> {
    devices
        .iter()
        .map(|device| {
            let mut item = PickerItem::new(device.id.clone(), device.name.clone())
                .disabled(device.disabled)
                // EXP-924's resolver: the row's stored glyph when it names a
                // DEVICE icon, else the kind default.
                .icon(gpui_component::Icon::from(device_icon(
                    device.icon.as_deref(),
                    device.server,
                )));
            if let Some(description) = &device.description {
                item = item.description(description.clone());
            }
            item
        })
        .collect()
}

pub(crate) fn device_picker(
    devices: &[DevicePickerDevice],
    value: Option<String>,
    trigger: AnyElement,
    on_change: OnPickerChange<String>,
) -> Picker<String> {
    Picker::single(device_items(devices), value, trigger, on_change).empty_text("No devices")
}
