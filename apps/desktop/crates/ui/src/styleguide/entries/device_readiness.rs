//! EXP-1196/1218/1219: `device-readiness` (3 Special components): THE device
//! readiness block, the REAL `device_readiness` model + renderer over the
//! three cases of the contract fixture `device-doctor.json` (read here with
//! `include_str!`, nothing retyped), each on the device itself (every action
//! offered) and from another device (only Update / Sign in). Inert: a
//! gallery has nowhere to send a pill or a switch.

use gpui::{div, px, App, Div, IntoElement, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use crate::device_readiness::{self, BlockProps};

pub(crate) const ID: &str = "device-readiness";
pub(crate) const OWNER: &str = "EXP-1196";

/// The ONE contract fixture the five clients replay.
const FIXTURE: &str =
    include_str!("../../../../../../../packages/domain-contract/fixtures/device-doctor.json");

/// One specimen column's width.
const WIDTH: f32 = 440.;

/// Every case: its name and its block.
fn cases() -> Vec<(String, coding::device_doctor::DeviceDoctor)> {
    let Ok(fixture) = serde_json::from_str::<serde_json::Value>(FIXTURE) else {
        return Vec::new();
    };
    fixture["cases"]
        .as_array()
        .map(|cases| {
            cases
                .iter()
                .filter_map(|case| {
                    let name = case["name"].as_str()?.to_string();
                    Some((name, device_readiness::parse(Some(&case["doctor"]))?))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn specimen(
    index: usize,
    doctor: &coding::device_doctor::DeviceDoctor,
    local: bool,
    cx: &App,
) -> Div {
    let muted = cx.theme().muted_foreground;
    let props = BlockProps {
        id: SharedString::from(format!("sg-device-readiness-{index}-{local}")),
        ..BlockProps::default()
    };
    v_flex()
        .w(px(WIDTH))
        .gap_2()
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child(if local { "This device" } else { "Another device" }),
        )
        .child(device_readiness::render_sections(
            &device_readiness::sections(doctor, local, None),
            props,
            cx,
        ))
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let foreground = cx.theme().foreground;
    let mut column = v_flex().gap_8();
    for (index, (name, doctor)) in cases().iter().enumerate() {
        column = column.child(
            v_flex()
                .gap_3()
                .child(
                    div()
                        .text_sm()
                        .text_color(foreground)
                        .child(SharedString::from(name.clone())),
                )
                .child(
                    h_flex()
                        .items_start()
                        .gap_8()
                        .child(specimen(index, doctor, true, cx))
                        .child(specimen(index, doctor, false, cx)),
                ),
        );
    }
    div().child(column.into_any_element())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fixture_case_draws() {
        assert_eq!(cases().len(), 3);
    }
}
