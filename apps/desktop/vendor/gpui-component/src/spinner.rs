use std::rc::Rc;

use crate::{Icon, IconName, Sizable, Size};
use gpui::{
    Animation, AnimationExt as _, AnyElement, App, Global, Hsla, IntoElement, ParentElement,
    Pixels, RenderOnce, Styled as _, Transformation, Window, div, linear, percentage,
    prelude::FluentBuilder as _, rems,
};
use instant::Duration;

/// Exponential patch (#EXP-1629): the host's spinner glyph at a size in
/// `color`. Registered (`set_spinner_glyph`), it draws every [`Spinner`]
/// whose icon is the default, so the IDE has ONE arc: genUI's
/// (`eui_gpui::paint::natives::spinner_glyph`).
#[derive(Clone)]
pub struct SpinnerGlyph(pub Rc<dyn Fn(Pixels, Hsla, &mut Window, &mut App) -> AnyElement>);

impl Global for SpinnerGlyph {}

/// Register the host's spinner glyph (see [`SpinnerGlyph`]).
pub fn set_spinner_glyph(cx: &mut App, glyph: impl Fn(Pixels, Hsla, &mut Window, &mut App) -> AnyElement + 'static) {
    cx.set_global(SpinnerGlyph(Rc::new(glyph)));
}

/// A cycling loading spinner.
#[derive(IntoElement)]
pub struct Spinner {
    size: Size,
    icon: Icon,
    /// The icon was set by the caller ([`Spinner::icon`]): the host's
    /// [`SpinnerGlyph`] does not replace it.
    custom_icon: bool,
    speed: Duration,
    easing: Box<dyn Fn(f32) -> f32>,
    color: Option<Hsla>,
}

impl Spinner {
    /// Create a new loading spinner.
    ///
    /// Exponential patch (#EXP-1629): the arc (`LoaderCircle`) on one linear
    /// turn per 0.8 s, genUI's `Spinner` and the IDE's `controls::spinner`,
    /// instead of upstream's spoked `Loader` easing in and out.
    pub fn new() -> Self {
        Self {
            size: Size::Medium,
            speed: Duration::from_secs_f64(0.8),
            easing: Box::new(linear),
            icon: Icon::new(IconName::LoaderCircle),
            custom_icon: false,
            color: None,
        }
    }

    /// Set specified icon for the spinner.
    ///
    /// Default is [`IconName::LoaderCircle`].
    ///
    /// Please ensure the icon used is suitable for a loading spinner.
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = icon.into();
        self.custom_icon = true;
        self
    }

    /// Set the icon color.
    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    /// Set the easing function.
    pub fn ease(mut self, easing: impl Fn(f32) -> f32 + 'static) -> Self {
        self.easing = Box::new(easing);
        self
    }
}

impl Sizable for Spinner {
    fn with_size(mut self, size: impl Into<Size>) -> Self {
        self.size = size.into();
        self
    }
}

impl RenderOnce for Spinner {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if let Some(glyph) = cx.try_global::<SpinnerGlyph>().filter(|_| !self.custom_icon).map(|g| g.0.clone()) {
            // `Icon::with_size`'s ladder in px.
            let size = match self.size {
                Size::Size(px) => px,
                Size::XSmall => rems(0.75).to_pixels(window.rem_size()),
                Size::Small => rems(0.875).to_pixels(window.rem_size()),
                Size::Medium => rems(1.).to_pixels(window.rem_size()),
                Size::Large => rems(1.5).to_pixels(window.rem_size()),
            };
            let color = self.color.unwrap_or_else(|| window.text_style().color);
            return div().child(glyph(size, color, window, cx)).into_any_element();
        }
        div()
            .child(
                self.icon
                    .with_size(self.size)
                    .when_some(self.color, |this, color| this.text_color(color))
                    .with_animation(
                        "circle",
                        Animation::new(self.speed).repeat().with_easing(self.easing),
                        |this, delta| this.transform(Transformation::rotate(percentage(delta))),
                    ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Render, TestAppContext, px, size};

    struct SpinnerHost;

    impl Render for SpinnerHost {
        fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
            Spinner::new()
        }
    }

    #[gpui::test]
    fn reduced_motion_spinner_is_static_and_requests_no_frame(cx: &mut TestAppContext) {
        cx.update(|cx| cx.set_reduce_motion(true));
        let window = cx.open_window(size(px(100.), px(100.)), |_, _| SpinnerHost);
        cx.run_until_parked();

        assert_eq!(
            window
                .update(cx, |_, window, cx| window.simulate_next_frame(cx))
                .unwrap(),
            0
        );
    }
}
