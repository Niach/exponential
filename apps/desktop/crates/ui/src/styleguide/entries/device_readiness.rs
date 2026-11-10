//! EXP-1196/1218/1219: `device-readiness` (3 Special components): THE device
//! readiness block, the REAL `device_readiness` model + renderer over
//! every case of the contract fixture `device-doctor.json` (read here with
//! `include_str!`, nothing retyped), each on the device itself (every action
//! offered) and from another device (only Update / Sign in). Inert: a
//! gallery has nowhere to send a pill or a switch.

use gpui::prelude::FluentBuilder as _;
use gpui::{div, px, App, Div, IntoElement, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use crate::device_readiness::{self, BlockProps};
use crate::settings::readiness::{self, Shape};

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

/// One case's block (#EXP-1770: the shared readiness document), kept
/// across renders under `id`.
fn block(id: &str, shape: Shape, window: &mut Window, cx: &mut App) -> gpui::AnyElement {
    let block = readiness::keyed(id, window, cx);
    // A no-op action: the pills draw active, like web's specimen.
    let props = BlockProps { on_action: Some(std::rc::Rc::new(|_, _, _, _| {})), ..BlockProps::default() };
    block.update(cx, |block, cx| block.show(shape, props, cx));
    block.into_any_element()
}

fn specimen(
    index: usize,
    doctor: &coding::device_doctor::DeviceDoctor,
    local: bool,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let muted = cx.theme().muted_foreground;
    let sections = device_readiness::sections(doctor, local, None);
    let block = block(&format!("sg-device-readiness-{index}-{local}"), Shape::Sections(&sections), window, cx);
    v_flex()
        .w(px(WIDTH))
        .gap_2()
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child(if local { "This device" } else { "Another device" }),
        )
        .child(block)
}

/// EXP-1557: the view's SUBJECT first (web `DeviceReadinessSpecimen`, the
/// natives' specimen line for line): the fixture's low-disk case (its LAST)
/// from another device, then the composer's low-disk row, so the desktop
/// `device-readiness` capture shows the Storage band, not the first case.
fn low_disk_specimen(cases: &[(String, coding::device_doctor::DeviceDoctor)], window: &mut Window, cx: &mut App) -> Option<Div> {
    let (name, doctor) = cases.last()?;
    let muted = cx.theme().muted_foreground;
    let caption = |text: SharedString| div().pt_2p5().text_xs().text_color(muted).child(text);
    let sections = device_readiness::sections(doctor, false, None);
    let low_disk = block("sg-device-readiness-low-disk", Shape::Sections(&sections), window, cx);
    let composer = device_readiness::warning_row(doctor, false).map(|row| block("sg-device-readiness-composer", Shape::Single(&row), window, cx));
    Some(
        v_flex()
            .w(px(WIDTH))
            .gap_1p5()
            .child(caption(SharedString::from(name.clone())))
            .child(low_disk)
            .when_some(composer, |this, composer| this.child(caption("Composer".into())).child(composer)),
    )
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let foreground = cx.theme().foreground;
    let cases = cases();
    // EXP-1558 (DEV-ONLY, `EXP_DEV_SPECIMEN_ONLY=1`): the specimen alone,
    // what web's `?dev=device-readiness` draws (the `device-readiness`
    // capture); the entry itself goes on to every case.
    if std::env::var("EXP_DEV_SPECIMEN_ONLY").is_ok_and(|value| value == "1") {
        return div().children(low_disk_specimen(&cases, window, cx));
    }
    let mut column = v_flex().gap_8().children(low_disk_specimen(&cases, window, cx));
    for (index, (name, doctor)) in cases.iter().enumerate() {
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
                        .child(specimen(index, doctor, true, window, cx))
                        .child(specimen(index, doctor, false, window, cx)),
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
        assert_eq!(cases().len(), 14);
    }
}
