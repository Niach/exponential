//! Settings → Appearance (EXP-85) — the desktop-only UI-scale slider.
//!
//! One card, one control: a discrete 80%–150% slider over
//! [`crate::ui_scale`]. Dragging applies live (theme font size → window rem
//! size → every rem-sized element), release persists to the local
//! per-install `settings.json`. Never synced, no owner gate.

use gpui::{AppContext as _, Entity, IntoElement, ParentElement, Render, Styled, Subscription};
use gpui_component::{
    h_flex,
    slider::{Slider, SliderEvent, SliderState},
    v_flex, ActiveTheme as _,
};

use crate::ui_scale;

use super::{card, card_header};

pub struct AppearancePane {
    slider: Entity<SliderState>,
    _subscriptions: Vec<Subscription>,
}

impl AppearancePane {
    pub fn new(cx: &mut gpui::Context<Self>) -> Self {
        // The slider works in whole percent (discrete 5% steps); the persisted
        // value is the factor. Seed from the LIVE theme, not the file — the
        // theme is what the user is currently looking at.
        let percent = (ui_scale::current(cx) * 100.).round();
        let slider = cx.new(|_| {
            SliderState::new()
                .min(ui_scale::MIN_SCALE * 100.)
                .max(ui_scale::MAX_SCALE * 100.)
                .step(ui_scale::SCALE_STEP * 100.)
                .default_value(percent)
        });

        let subscriptions = vec![cx.subscribe(&slider, |_, _, event: &SliderEvent, cx| {
            match event {
                // Live preview while dragging — apply redraws every window.
                SliderEvent::Change(value) => {
                    ui_scale::apply(value.start() / 100., cx);
                    cx.notify();
                }
                // Persist once, on release.
                SliderEvent::Release(value) => {
                    let scale = ui_scale::normalize(value.start() / 100.);
                    if let Err(err) = ui_scale::save(&ui_scale::settings_file(cx), scale) {
                        log::warn!("[ui] saving uiScale failed: {err}");
                    }
                }
            }
        })];

        Self {
            slider,
            _subscriptions: subscriptions,
        }
    }
}

impl Render for AppearancePane {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let percent = (ui_scale::current(cx) * 100.).round() as i32;
        card(cx)
            .child(card_header(
                "Appearance",
                "UI scale for this device — text and controls zoom together.",
                cx,
            ))
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(v_flex().flex_1().child(Slider::new(&self.slider)))
                    .child(
                        gpui::div()
                            .w_12()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{percent}%")),
                    ),
            )
    }
}
