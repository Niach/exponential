//! SLOP-7 (EXP-1121): `readiness-checklist` (3 Special components): the
//! "Ready to code?" checklist Start coding opens while a run cannot start.
//!
//! The IDE's popover (`coding_readiness::render_popover`) is drawn off a live
//! `StartCodingControl` and its `PopoverState`, neither of which a gallery
//! can own, so the demo draws the popover's header, three rows and fix pills
//! with the SAME recipes (the met tick, the amber current ring, the dashed
//! pending ring, `surface::glass_pill_button{,_primary}`) over the REAL model:
//! `domain::coding_readiness::coding_readiness` run on the contract fixture's
//! "repo missing" case (`coding-readiness.json`, byte-locked ×4), read here
//! with `include_str!`, and filtered by the popover's own
//! `coding_readiness::desktop_fixes`. Nothing is retyped.

use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, App, Div, FontWeight, IntoElement,
    ParentElement as _, SharedString, Styled as _, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::coding_readiness::{
    coding_readiness, copy, CodingReadiness, CodingReadinessInput, ReadinessFix, ReadinessStep,
    ReadinessStepKey, ReadinessStepState,
};

use crate::icons::registry;
use crate::surface::{glass_pill_button, glass_pill_button_primary, PillSize};

pub(crate) const ID: &str = "readiness-checklist";
pub(crate) const OWNER: &str = "SLOP-7";

/// The ONE contract fixture the four clients replay.
const FIXTURE: &str =
    include_str!("../../../../../../../packages/domain-contract/fixtures/coding-readiness.json");

/// The web specimen's case, by its fixture name.
const CASE: &str = "repo missing: GitHub met, repository current, device pending with last seen";

/// The popover's width (web `w-[23.5rem]`).
const WIDTH: f32 = 376.;

/// The case's input, run through the real model.
fn readiness() -> Option<CodingReadiness> {
    let fixture: serde_json::Value = serde_json::from_str(FIXTURE).ok()?;
    let case = fixture
        .get("cases")?
        .as_array()?
        .iter()
        .find(|case| case.get("name").and_then(|name| name.as_str()) == Some(CASE))?;
    let input: CodingReadinessInput = serde_json::from_value(case.get("input")?.clone()).ok()?;
    Some(coding_readiness(&input))
}

/// A step's glyph inside its ring (the popover's `step_glyph`).
fn step_glyph(key: ReadinessStepKey) -> crate::icons::ExpIcon {
    match key {
        ReadinessStepKey::Github => registry::UI_GITHUB,
        ReadinessStepKey::Repository => registry::UI_BRANCH,
        ReadinessStepKey::Device => registry::NAV_DEVICES,
    }
}

/// The 20px ring a row leads with: green tick, amber ring, dashed grey ring.
fn step_ring(step: &ReadinessStep, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let ring = div()
        .flex_shrink_0()
        .size(px(20.))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .border_1();
    match step.state {
        ReadinessStepState::Met => ring
            .border_color(theme.success.opacity(0.35))
            .bg(theme.success.opacity(0.15))
            .child(Icon::new(registry::UI_CHECK).size_3().text_color(theme.success)),
        ReadinessStepState::Current => ring
            .border_color(theme.warning)
            .child(Icon::new(step_glyph(step.key)).size_3().text_color(theme.warning)),
        ReadinessStepState::Pending => ring
            .border_dashed()
            .border_color(theme.muted_foreground.opacity(0.6))
            .child(Icon::new(step_glyph(step.key)).size_3().text_color(theme.muted_foreground)),
    }
    .into_any_element()
}

/// The three-slice progress strip under the summary.
fn progress(readiness: &CodingReadiness, cx: &App) -> AnyElement {
    let theme = cx.theme();
    h_flex()
        .w_full()
        .gap_1()
        .children(readiness.steps.iter().map(|step| {
            let color = match step.state {
                ReadinessStepState::Met => theme.success,
                ReadinessStepState::Current => theme.warning,
                ReadinessStepState::Pending => theme.border,
            };
            div().flex_1().h(px(3.)).rounded_full().bg(color)
        }))
        .into_any_element()
}

/// One fix pill: the first primary, the rest glass; Board settings trails a
/// chevron. Inert: a gallery has nowhere to send it.
fn fix_pill(fix: ReadinessFix, primary: bool, cx: &App) -> AnyElement {
    let id = SharedString::from(format!("sg-readiness-fix-{fix:?}"));
    let size = PillSize::Sm;
    let mut button = if primary {
        glass_pill_button_primary(id, size)
    } else {
        glass_pill_button(id, size, cx)
    };
    let glyph = match fix {
        ReadinessFix::ChooseRepository => Some(registry::UI_BRANCH),
        ReadinessFix::ConnectGithub => Some(registry::UI_GITHUB),
        ReadinessFix::OpenDevices => Some(registry::NAV_DEVICES),
        ReadinessFix::SetUpServer => Some(registry::UI_SERVER),
        _ => None,
    };
    if let Some(glyph) = glyph {
        button = button.icon(Icon::new(glyph).with_size(px(size.glyph())));
    }
    if fix == ReadinessFix::BoardSettings {
        button
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(fix.label())
                    .child(Icon::new(registry::UI_CHEVRON_RIGHT).size_3()),
            )
            .into_any_element()
    } else {
        button.label(fix.label()).into_any_element()
    }
}

/// One step row, the popover's `render_step` minus its live picker.
fn step_row(step: &ReadinessStep, cx: &App) -> AnyElement {
    let theme = cx.theme().clone();
    let (fg, muted, warning) = (theme.foreground, theme.muted_foreground, theme.warning);
    let mut row = h_flex()
        .w_full()
        .items_start()
        .gap_3()
        .px_4()
        .py_2p5()
        .child(step_ring(step, cx));
    match step.state {
        ReadinessStepState::Met => {
            row = row.items_center().child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .text_color(muted)
                            .child(SharedString::from(step.title.clone())),
                    )
                    .when_some(step.detail.clone(), |line, detail| {
                        line.child(
                            div()
                                .flex_shrink_0()
                                .max_w(px(160.))
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .text_xs()
                                .text_color(muted)
                                .child(SharedString::from(detail)),
                        )
                    }),
            );
        }
        ReadinessStepState::Current | ReadinessStepState::Pending => {
            let current = step.state == ReadinessStepState::Current;
            let mut column = v_flex()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .child(
                    div()
                        .text_sm()
                        .text_color(if current { fg } else { fg.opacity(0.8) })
                        .child(SharedString::from(step.title.clone())),
                )
                .when_some(step.body.clone(), |column, body| {
                    column.child(div().text_xs().text_color(muted).child(SharedString::from(body)))
                });
            if current {
                row = row.bg(warning.opacity(0.06));
                // The specimen is an owner's view, so Board settings shows.
                let fixes = crate::coding_readiness::desktop_fixes(step, true);
                if !fixes.is_empty() {
                    column = column.child(
                        h_flex().pt_2().gap_2().flex_wrap().children(
                            fixes
                                .into_iter()
                                .enumerate()
                                .map(|(index, fix)| fix_pill(fix, index == 0, cx)),
                        ),
                    );
                }
            }
            row = row.child(column);
        }
    }
    row.into_any_element()
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let Some(readiness) = readiness() else {
        return div();
    };
    let theme = cx.theme().clone();
    let header = v_flex()
        .gap_2()
        .px_4()
        .pt_3()
        .pb_3()
        .child(
            v_flex()
                .gap_0p5()
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.foreground)
                        .child(copy::TITLE),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(SharedString::from(readiness.summary.clone())),
                ),
        )
        .child(progress(&readiness, cx));
    let mut rows = v_flex().w_full().border_t_1().border_color(theme.border);
    for step in &readiness.steps {
        rows = rows.child(step_row(step, cx));
    }
    div().child(
        v_flex()
            .w(px(WIDTH))
            .rounded(px(8.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .overflow_hidden()
            .child(header)
            .child(rows),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The case still exists and still reads GitHub met, repository current
    /// (with its two fixes), device pending — the shape the demo documents.
    #[test]
    fn the_fixture_case_draws_met_current_pending() {
        let readiness = readiness().expect("the fixture case parses");
        let states: Vec<_> = readiness.steps.iter().map(|step| step.state).collect();
        assert_eq!(
            states,
            vec![
                ReadinessStepState::Met,
                ReadinessStepState::Current,
                ReadinessStepState::Pending
            ]
        );
        assert_eq!(
            readiness.steps[1].fixes,
            vec![ReadinessFix::ChooseRepository, ReadinessFix::BoardSettings]
        );
    }
}
