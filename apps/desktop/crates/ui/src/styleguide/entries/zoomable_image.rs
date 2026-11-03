//! EXP-1586 — `zoomable-image` (General components): THE lightbox picture,
//! the eUI zoom view (`eui_gpui::controls::ZoomableImage`, catalog
//! `zoomableImage`) every desktop lightbox draws (`crate::image_preview`).
//! The same two specimens as the web entry: a 1440×900 picture at rest with
//! the pill shown, and the same picture zoomed to 200%. The picture is the
//! plan card's local stand-in (no backend).

use std::rc::Rc;
use std::sync::Arc;

use eui_gpui::controls::{ZoomControls, ZoomableImage, ZoomableImageOptions};
use gpui::{div, px, App, AppContext as _, Div, Entity, ImageSource, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{v_flex, ActiveTheme as _};

pub(crate) const ID: &str = "zoomable-image";
pub(crate) const OWNER: &str = "EXP-1586";

/// The specimen picture's natural size.
const NATURAL: (u32, u32) = (1440, 900);

/// The box each specimen draws in.
const BOX: (f32, f32) = (640., 400.);

fn specimen(key: &str, scale: Option<f64>, window: &mut Window, cx: &mut App) -> Entity<ZoomableImage> {
    window
        .use_keyed_state(SharedString::from(format!("sg-zoomable-image-{key}")), cx, move |_, cx| {
            let (w, h) = NATURAL;
            let png = super::plan_card::picture_png([0x1e, 0x3a, 0x5f], [0x3b, 0x82, 0xf6], w, h);
            let image = ImageSource::from(Arc::new(gpui::Image::from_bytes(gpui::ImageFormat::Png, png)));
            let options = ZoomableImageOptions {
                alt: "Specimen".into(),
                controls: ZoomControls::Always,
                theme: eui_core::themes::builtin_theme(crate::exponential_ui_host::THEME_ID),
                mode: eui_core::theme::Mode::Dark,
                host: Rc::new(crate::exponential_ui_host::IdePlugin { media_origin: String::new(), uploads: None }),
                ..ZoomableImageOptions::default()
            };
            cx.new(|cx| {
                let mut zoom = ZoomableImage::new(format!("sg-{key}"), Some(image), Some((w as f64, h as f64)), options, cx);
                zoom.set_viewport_hint(BOX.0, BOX.1);
                zoom.set_scale(scale, cx);
                zoom
            })
        })
        .read(cx)
        .clone()
}

fn demo(label: &str, zoom: Entity<ZoomableImage>, cx: &App) -> Div {
    v_flex()
        .gap_2()
        .child(div().text_xs().text_color(cx.theme().muted_foreground).child(SharedString::from(label.to_string())))
        .child(div().w(px(BOX.0)).h(px(BOX.1)).child(div().size_full().child(zoom)))
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let rest = specimen("rest", None, window, cx);
    let zoomed = specimen("zoomed", Some(2.0), window, cx);
    v_flex().gap_6().child(demo("Fit", rest, cx)).child(demo("200%", zoomed, cx))
}
