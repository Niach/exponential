//! VAPP-91 — the Devices template through the IDE's Exponential UI host,
//! DEV-ONLY (`EXP_DEV_SCREEN=exponential-ui-devices`,
//! [`Screen::ExponentialUiDevices`]).
//!
//! The desktop twin of the web route `/exponential-ui-devices` (view
//! `exponential-ui-devices`): the IDE host ([`crate::exponential_ui_host`])
//! installs the app's Devices package, applies its `devices` template
//! (`applyTemplate`, the same message the web sends), binds `exp:devices`
//! over the synced store and paints the surface with the app extension's
//! `IconDisc` + `LiveDot` painters, in the IDE's theme mode.

use exponential_ui::theme::Mode;
use gpui::{
    div, prelude::FluentBuilder as _, px, Context, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, ScrollHandle, SharedString, Styled as _, Window,
};
use gpui_component::ActiveTheme as _;
use serde_json::json;

use crate::exponential_ui_host::IdeHost;
use crate::navigation::Screen;

/// The surface id and template id, the web route's.
const SURFACE_ID: &str = "devices";
const TEMPLATE_ID: &str = "devices";

/// The web page's `max-w-3xl` column.
const COLUMN_MAX_W: f32 = 768.;

fn ide_mode(cx: &gpui::App) -> Mode {
    if cx.theme().mode.is_dark() {
        Mode::Dark
    } else {
        Mode::Light
    }
}

pub struct ExponentialUiDevices {
    ide: IdeHost,
    scroll: ScrollHandle,
    _subscriptions: Vec<gpui::Subscription>,
}

impl ExponentialUiDevices {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let ide = IdeHost::new(ide_mode(cx), cx);
        ide.host.update(cx, |host, cx| {
            let ops = host.receive(
                &json!({"version": "v0.9", "applyTemplate": {"surfaceId": SURFACE_ID, "templateId": TEMPLATE_ID}}),
                cx,
            );
            log::info!("[exponential-ui devices] applyTemplate → {} ops", ops.len());
        });
        let subscriptions = vec![
            cx.observe(&ide.host, |_, _, cx| cx.notify()),
            // The IDE's theme mode follows into the surface.
            cx.observe_global::<gpui_component::Theme>(|this, cx| {
                let mode = ide_mode(cx);
                this.ide.host.update(cx, |host, cx| host.set_mode(mode, cx));
            }),
        ];
        Self { ide, scroll: ScrollHandle::new(), _subscriptions: subscriptions }
    }
}

impl Render for ExponentialUiDevices {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let _ = Screen::ExponentialUiDevices;
        let host = self.ide.host.read(cx);
        let surface = host.surface(SURFACE_ID);
        // The catalog-update banner (the web's `HostBanner`); a local host has
        // no transport, so no offline state.
        let banner = host.unsupported_catalog().map(|_| {
            SharedString::from("This surface needs a newer Exponential. Update the app to show it.")
        });
        let amber = theme::tokens::YELLOW.to_hsla();
        div().id("exponential-ui-devices").size_full().flex().flex_col().child(crate::scroll_pane::v_scroll_pane(
            "exponential-ui-devices-scroll",
            &self.scroll,
            div().w_full().flex().justify_center().child(
                div()
                    .w_full()
                    .max_w(px(COLUMN_MAX_W))
                    .px_4()
                    .py_4()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .when_some(banner, |col, text| {
                        col.child(
                            div()
                                .rounded_md()
                                .border_1()
                                .border_color(amber.opacity(0.4))
                                .bg(amber.opacity(0.1))
                                .px_3()
                                .py_2()
                                .text_sm()
                                .text_color(amber)
                                .child(text),
                        )
                    })
                    .children(surface),
            ),
        ))
    }
}
