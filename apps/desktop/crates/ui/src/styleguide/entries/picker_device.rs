//! EXP-1021 — the `picker-device` styleguide entry: the device picker — the device glyph, offline rows disabled with their reason.
//!
//! Filled by EXP-1021; never edits `entries/mod.rs` nor the index.

use gpui::{App, Div, Window};

use super::picker::{chip, column, demo, inert};

pub(crate) const ID: &str = "picker-device";
pub(crate) const OWNER: &str = "EXP-1021";

pub(crate) fn render(_window: &mut Window, _cx: &mut App) -> Div {
    column(vec![demo(
        "device — an offline machine still RENDERS; it just never picks",
        |window, cx| {
            let devices = vec![
                crate::picker::device_picker::DevicePickerDevice {
                    id: "device-1".to_string(),
                    name: "studio".to_string(),
                    icon: Some("laptop".to_string()),
                    server: false,
                    description: None,
                    disabled: false,
                },
                crate::picker::device_picker::DevicePickerDevice {
                    id: "device-2".to_string(),
                    name: "builder".to_string(),
                    icon: Some("server".to_string()),
                    server: true,
                    description: Some("Offline".to_string()),
                    disabled: true,
                },
            ];
            crate::picker::device_picker::device_picker(
                &devices,
                Some("device-1".to_string()),
                chip("sg-picker-device", "studio", cx),
                inert(),
            )
            .id("sg-picker-device-surface")
            .render(window, cx)
        },
    )])
}
