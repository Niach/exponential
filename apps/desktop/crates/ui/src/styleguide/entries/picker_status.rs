//! EXP-1021 — the `picker-status` styleguide entry: the status picker — the team's rows, each by its glyph in its colour.
//!
//! Filled by EXP-1021; never edits `entries/mod.rs` nor the index.

use gpui::{App, Div, Window};

use super::picker::{chip, column, demo, inert};

pub(crate) const ID: &str = "picker-status";
pub(crate) const OWNER: &str = "EXP-1021";

pub(crate) fn render(_window: &mut Window, _cx: &mut App) -> Div {
    column(vec![
        demo("status — single; vocabulary order, Backlog leads (EXP-448)", |window, cx| {
            let statuses = domain::statuses::default_resolved_statuses();
            crate::picker::status_picker::status_picker(
                &statuses,
                crate::picker::PickerMode::Single,
                vec![statuses[0].group_key.clone()],
                chip("sg-picker-status", "Backlog", cx),
                inert(),
            )
            .id("sg-picker-status-surface")
            .render(window, cx)
        }),
        demo("status — multi (a filter)", |window, cx| {
            let statuses = domain::statuses::default_resolved_statuses();
            crate::picker::status_picker::status_picker(
                &statuses,
                crate::picker::PickerMode::Multi,
                vec![statuses[1].group_key.clone()],
                chip("sg-picker-statuses", "1 status", cx),
                inert(),
            )
            .id("sg-picker-statuses-surface")
            .render(window, cx)
        }),
        // N-6: the INLINE presentation — what every picker inside a dialog
        // window wears (the host rule): the same rows in the host's flow
        // under the trigger, disclosed here.
        demo("status — inline (inside a dialog the host decides)", |window, cx| {
            let statuses = domain::statuses::default_resolved_statuses();
            crate::picker::status_picker::status_picker(
                &statuses,
                crate::picker::PickerMode::Single,
                vec![statuses[1].group_key.clone()],
                chip("sg-picker-status-inline", "In Progress", cx),
                inert(),
            )
            .id("sg-picker-status-inline-surface")
            .inline()
            .open(std::rc::Rc::new(|_, _| {}))
            .render(window, cx)
        }),
        // N-14 (picker-presentation.json): THE composer options row as a row
        // host. Collapsed, the pins are the pins; open, the device pin's
        // rows sit UNDER THE WHOLE ROW and no pin moves (the web twin's
        // options-row specimen).
        demo("options row — inline pins, collapsed (N-14)", |window, cx| {
            options_row_specimen("sg-options-row-collapsed", false, window, cx)
        }),
        demo("options row — the Device pin open: rows under the whole row (N-14)", |window, cx| {
            options_row_specimen("sg-options-row-open", true, window, cx)
        }),
    ])
}

/// N-14: Device · Account · Model · Plan as the launch dialog draws them,
/// in an [`crate::picker::InlinePickerRow`]; `open` discloses the Device pin.
fn options_row_specimen(
    id: &'static str,
    open: bool,
    window: &mut Window,
    cx: &mut App,
) -> gpui::AnyElement {
    use gpui::{IntoElement as _, ParentElement as _, Styled as _};
    use crate::picker::device_picker::{device_picker, DevicePickerDevice};
    let device = |id: &str, name: &str, server: bool| DevicePickerDevice {
        id: id.to_string(),
        name: name.to_string(),
        icon: None,
        server,
        description: None,
        disabled: false,
    };
    let devices = [
        device("mbp", "Danny's MacBook Pro", false),
        device("mini", "Mac mini", false),
        device("build", "Build server", true),
    ];
    let pin = |pin: &'static str, label: &str, cx: &App| {
        crate::launch_options::inline_pin_trigger(pin.into(), label.to_string(), cx).into_any_element()
    };
    let mut picker = device_picker(
        &devices,
        Some("mbp".to_string()),
        crate::launch_options::inline_pin_trigger_with(
            "sg-options-row-device".into(),
            Some(crate::icons::registry::UI_DEVICE),
            "Danny's MacBook Pro".to_string(),
            cx,
        )
        .into_any_element(),
        inert(),
    )
    .id(gpui::ElementId::Name(format!("{id}-device").into()))
    .inline();
    if open {
        picker = picker.open(std::rc::Rc::new(|_, _| {}));
    }
    let row = gpui_component::h_flex()
        .w_full()
        .min_w_0()
        .flex_wrap()
        .gap_1()
        .items_center()
        .px_1()
        .text_xs()
        .child(picker.render(window, cx))
        .child(pin("sg-options-row-account", "danny@example.com", cx))
        .child(pin("sg-options-row-model", "Opus", cx))
        .child(pin("sg-options-row-plan", "Plan", cx));
    gpui_component::v_flex()
        .w(gpui::px(560.))
        .child(crate::picker::InlinePickerRow::new(id, row))
        .into_any_element()
}
