//! EXP-1029 contract — the device picker: the machines a run may start on,
//! each by its device glyph (contract `deviceIcon`) + name, offline ones
//! disabled with the reason as the description. The composer, the
//! automation editor and the workflow runner row pick one.

use gpui::AnyElement;

use super::{OnPickerChange, Picker, PickerItem};

/// A device row as the picker reads it.
pub(crate) struct DevicePickerDevice {
    pub id: String,
    pub name: String,
    /// Contract `deviceIcon`; `None` = the kind default.
    pub icon: Option<String>,
    /// A muted reason under the name (`Offline`, `Update to run workflows`).
    pub description: Option<String>,
    pub disabled: bool,
}

pub(crate) fn device_items(devices: &[DevicePickerDevice]) -> Vec<PickerItem<String>> {
    devices
        .iter()
        .map(|device| {
            let mut item = PickerItem::new(device.id.clone(), device.name.clone())
                .disabled(device.disabled);
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
