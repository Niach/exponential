//! Settings → About (EXP-262): version, project links, and the third-party
//! licence notice every distributed build must reproduce.
//!
//! The notice is [`crate::licenses::NOTICES`] — the same `include_str!` blob
//! whose embedding EXP-375/EXP-376 gated — rendered through the one
//! virtualized long-text primitive in the crate (`TextView::markdown` with
//! `scrollable`, the `file_viewer` path, `gpui::list`-backed). It sits in a
//! FIXED-height box on purpose: `SettingsView::render` wraps every pane in
//! `overflow_y_scroll`, so the inner list needs a bounded viewport to
//! virtualize against.

use std::sync::OnceLock;

use gpui::{
    div, App, InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    Styled, Window, px,
};
use gpui_component::{
    button::Button, h_flex, text::TextView, v_flex, ActiveTheme as _, Icon, Sizable as _,
};

use crate::icons::registry;
use crate::licenses;

use super::{card_header, open_url, row_stroke, section};

const SOURCE_URL: &str = "https://github.com/Niach/exponential";
const LICENSE_URL: &str = "https://github.com/Niach/exponential/blob/master/LICENSE";

/// The locked cross-client blurb — same wording on web, iOS and Android.
const LICENSES_BLURB: &str = "Exponential is built with open-source software. \
     These licenses cover the components bundled in this build.";

/// The notice, fenced ONCE per process: `fence_code` scans the whole ~1 MB
/// blob for backtick runs, which must not run per frame.
fn fenced_notices() -> &'static SharedString {
    static FENCED: OnceLock<SharedString> = OnceLock::new();
    FENCED.get_or_init(|| crate::file_viewer::fence_code(licenses::NOTICES, "text").into())
}

/// The About pane (`SettingsSection::About`) — stateless; everything it shows
/// is compiled in.
pub struct AboutPane;

impl AboutPane {
    fn link_button(id: &'static str, label: &'static str, url: &'static str) -> Button {
        Button::new(id)
            .outline()
            .xsmall()
            .icon(Icon::new(registry::UI_EXTERNAL_LINK))
            .label(label)
            .on_click(|_, _, cx| open_url(cx, url.to_string()))
    }

    fn render_about(&self, cx: &App) -> impl IntoElement {
        section(cx)
            .child(card_header(
                "About",
                format!(
                    "Exponential · Version {}",
                    domain::client_version::current_version()
                ),
                cx,
            ))
            .child(
                h_flex()
                    .gap_2()
                    .child(Self::link_button("about-source", "Source code", SOURCE_URL))
                    .child(Self::link_button(
                        "about-license",
                        "License (Apache-2.0)",
                        LICENSE_URL,
                    )),
            )
    }

    fn render_licenses(&self, cx: &App) -> impl IntoElement {
        section(cx)
            .child(card_header("Third-party licenses", LICENSES_BLURB, cx))
            .child(
                div()
                    .id("about-notices")
                    .h(px(480.))
                    .w_full()
                    .overflow_hidden()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_color(row_stroke(cx))
                    .px_3()
                    .py_2()
                    .child(
                        TextView::markdown("about-notices-text", fenced_notices().clone())
                            .selectable(true)
                            .scrollable(true)
                            .style(crate::surface::bare_code_markdown_style()),
                    ),
            )
    }
}

impl Render for AboutPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_6()
            .child(self.render_about(cx))
            .child(self.render_licenses(cx))
    }
}
