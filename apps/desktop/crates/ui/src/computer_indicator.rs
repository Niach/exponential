//! EXP-1196/1273 — the IDE's side of the device's Computer use switch, and
//! THE banner (`computer-use-banner.json`).
//!
//! The switch's edge: each time the device's switch reads ON after reading
//! off (the app starting with it on, a toggle here, a synced one from another
//! client), the OS permissions are asked NOW, not mid-run; each time it reads
//! off, every grant ends and the person's input is theirs again.
//!
//! The banner: while a session drives the screen the person's keyboard and
//! pointer are held (`computer::set_presence_observer` says when); a small
//! top-most window floating over the bottom of the primary display shows
//! the bar ([`banner_bar`], the bulk bar's capsule) for exactly that long.
//! The window is exactly the bar's size (EXP-1372: a fixed 560 x 64 cut a
//! long label off on both sides and showed a strip above the bar): the
//! words are shaped before the window opens ([`banner_size`]).
//! It takes no focus and no click: Esc is the way out, and the hold ends
//! on the key itself.

use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use etalk_gpui::bar::{
    item, rem_size, shaped_width, value, BAR_GAP_REMS, BAR_SIDE_REMS, BAR_SLACK, BAR_STROKE, BAR_TOP_REMS, ITEM_SIDE, LABEL_SIDE,
};
use gpui::{
    div, point, px, size, App, AppContext as _, Bounds, Context, Div, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, SharedString, Size, Stateful, Styled as _, Window, WindowBounds, WindowKind, WindowOptions,
};
use gpui_component::{ActiveTheme as _, Icon, Sizable as _};

use crate::icons::registry;
use crate::surface::glass_bar;

/// How often the switch (and the presence channel) is read.
const TICK: Duration = Duration::from_millis(400);

/// The banner's window floats 16 px above the display's visible bottom
/// (the bulk bar's place over a list), and never wider than the display
/// less that air on both sides.
const BANNER_BOTTOM: f32 = 16.;

/// The bar's boxes (`etalk_gpui::bar`, THE bar's parts the voice overlay
/// shares): the item's height, gaps, the key chip's sides and the glyph.
const ITEM_HEIGHT: f32 = 32.;
const ITEM_GAP: f32 = 6.;
const KEY_SIDE: f32 = 6.;
const GLYPH: f32 = 16.;

/// X11 without a screen layout (no RandR answer): the toolkit's display is
/// the whole X screen, every monitor side by side, so its width is no cap a
/// banner may take; this one keeps a long label off the monitors' seam.
const X11_FALLBACK_ROOM: f32 = 1200.;

/// The bar's height at `rem`: one item between the padding and the
/// stroke, whole pixels up.
fn bar_height(rem: f32) -> f32 {
    (ITEM_HEIGHT + 2. * (BAR_TOP_REMS * rem + BAR_STROKE)).ceil()
}

/// The shaped widths of the bar's words, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Words {
    title: f32,
    label: Option<f32>,
    key: f32,
    abort: f32,
}

impl Words {
    /// The bar's width around these words at `rem`, whole pixels up.
    fn bar_width(&self, rem: f32) -> f32 {
        let gap = BAR_GAP_REMS * rem;
        let title = ITEM_SIDE + GLYPH + ITEM_GAP + self.title + ITEM_SIDE;
        let label = self.label.map_or(0., |label| gap + LABEL_SIDE + label + LABEL_SIDE);
        let key = gap + ITEM_SIDE + (KEY_SIDE + self.key + KEY_SIDE + 2. * BAR_STROKE) + ITEM_GAP + self.abort + ITEM_SIDE;
        (2. * (BAR_SIDE_REMS * rem + BAR_STROKE) + title + label + key).ceil()
    }
}

/// The banner window's size: the bar around these words plus
/// [`BAR_SLACK`], capped at `max_width` (a longer label ellipsizes).
pub(crate) fn banner_size(label: Option<&str>, max_width: f32, cx: &App) -> Size<Pixels> {
    let words = copy();
    let rem = rem_size(cx);
    // `text_sm` and `text_xs`.
    let (sm, xs) = (rem * 0.875, rem * 0.75);
    let measured = Words {
        title: shaped_width(&words.title, sm, FontWeight::NORMAL, cx),
        label: label.map(|label| shaped_width(label, sm, FontWeight::SEMIBOLD, cx)),
        key: shaped_width(&words.key, xs, FontWeight::NORMAL, cx),
        abort: shaped_width(&words.abort, sm, FontWeight::NORMAL, cx),
    };
    let rem = f32::from(rem);
    size(px((measured.bar_width(rem) + BAR_SLACK).min(max_width.max(0.))), px(bar_height(rem)))
}

const FIXTURE: &str =
    include_str!("../../../../../packages/domain-contract/fixtures/computer-use-banner.json");

/// The fixture's words.
struct Copy {
    title: String,
    key: String,
    abort: String,
}

fn copy() -> &'static Copy {
    static COPY: OnceLock<Copy> = OnceLock::new();
    COPY.get_or_init(|| {
        let fixture: serde_json::Value = serde_json::from_str(FIXTURE).expect("computer-use-banner.json parses");
        let word = |key: &str| fixture["copy"][key].as_str().expect("banner copy").to_string();
        Copy { title: word("title"), key: word("key"), abort: word("abort") }
    })
}

/// THE bar (the bulk bar's capsule): the glyph and the title as one item,
/// the session's label as the bold value, then the key and the way out.
pub(crate) fn banner_bar(label: Option<SharedString>, cx: &App) -> Stateful<Div> {
    let words = copy();
    let foreground = cx.theme().foreground;
    let hairline = cx.theme().border;
    let mut bar = glass_bar(cx).id("computer-use-banner").flex_shrink_0().max_w_full().justify_center().child(
        item(cx)
            .child(Icon::new(registry::NAV_COMPUTER).with_size(px(16.)))
            .child(SharedString::from(words.title.clone())),
    );
    if let Some(label) = label {
        bar = bar.child(value(label, cx));
    }
    bar.child(
        item(cx)
            .child(
                div()
                    .h(px(20.))
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(hairline)
                    .text_xs()
                    .text_color(foreground)
                    .child(SharedString::from(words.key.clone())),
            )
            .child(SharedString::from(words.abort.clone())),
    )
}

/// The banner window's one view.
struct Banner {
    label: Option<SharedString>,
}

impl Render for Banner {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The window has no `Root`: its rem and font are set here, the ones
        // [`banner_size`] shaped the words in.
        window.set_rem_size(rem_size(cx));
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .font_family(cx.theme().font_family.clone())
            // The bar is the window: its air (`BAR_SLACK`) stays inside the
            // capsule, whatever ground the window has.
            .child(banner_bar(self.label.clone(), cx).w_full())
    }
}

/// Where the lock's thread leaves what it saw; the foreground loop drains it.
static PRESENCE: OnceLock<Mutex<std::sync::mpsc::Receiver<Option<String>>>> = OnceLock::new();

/// NEW-59: the toolkit display the epilot crate's `target` is (`None` =
/// no counterpart, e.g. X11's one whole-screen display).
fn toolkit_display(target: &coding::computer::Display, cx: &App) -> Option<std::rc::Rc<dyn gpui::PlatformDisplay>> {
    let screens = coding::computer::displays();
    // #FEED-161: by the stable id, the same display however the list moved.
    let index = screens.iter().position(|screen| screen.id == target.id)?;
    let displays = cx.displays();
    let map = etalk_gpui::push_to_talk::overlay::display_map(&displays, &screens);
    map.iter().position(|mapped| *mapped == Some(index)).and_then(|ix| displays.get(ix).cloned())
}

/// NEW-59: on X11 the banner's monitor is the run's work screen when it has
/// one (the same scaling as the primary's, `ScreenLayout::banner_origin`).
fn on_work_screen(layout: coding::computer::ScreenLayout, target: Option<&coding::computer::Display>) -> coding::computer::ScreenLayout {
    match target {
        Some(display) => coding::computer::ScreenLayout {
            primary: coding::computer::ScreenRect {
                x: display.x,
                y: display.y,
                width: display.width,
                height: display.height,
            },
            ..layout
        },
        None => layout,
    }
}

fn banner_options(label: Option<&str>, cx: &App) -> WindowOptions {
    // NEW-59: the banner floats on the run's work screen (its current
    // display), else the primary as before.
    let target = coding::computer::banner_display();
    let display = target
        .as_ref()
        .and_then(|target| toolkit_display(target, cx))
        .or_else(|| cx.primary_display());
    // The visible bounds leave the panel or dock out, like the bulk bar
    // floats above the list's bottom, not over the composer.
    let display_bounds = display.as_ref().map(|display| display.visible_bounds());
    // On X11 the toolkit's display is the whole X screen (every monitor
    // side by side): the primary monitor and the panel come from the
    // server, scaled by the root's logical width (EXP-1273).
    let layout = coding::computer::screen_layout().map(|layout| on_work_screen(layout, target.as_ref()));
    let room = display_bounds
        .map(|bounds| {
            let logical_root_width = f32::from(bounds.size.width);
            match layout.and_then(|layout| layout.primary_logical_width(logical_root_width)) {
                Some(width) => width,
                None if cfg!(target_os = "linux") => logical_root_width.min(X11_FALLBACK_ROOM),
                None => logical_root_width,
            }
        })
        .unwrap_or(f32::MAX);
    let banner = banner_size(label, room - 2. * BANNER_BOTTOM, cx);
    let (width, height) = (f32::from(banner.width), f32::from(banner.height));
    let on_primary_monitor = display_bounds.and_then(|bounds| {
        let (x, y) = layout?.banner_origin(f32::from(bounds.size.width), width, height, BANNER_BOTTOM)?;
        Some(point(px(x), px(y)))
    });
    let origin = on_primary_monitor
        .or_else(|| {
            display_bounds.map(|bounds| {
                point(
                    bounds.origin.x + (bounds.size.width - banner.width) / 2.,
                    bounds.origin.y + bounds.size.height - banner.height - px(BANNER_BOTTOM),
                )
            })
        })
        .unwrap_or_else(|| point(px(0.), px(0.)));
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds { origin, size: banner })),
        titlebar: None,
        focus: false,
        show: true,
        // Above every other window, never activated: a notification, not a
        // dialog (X11: override-redirect; macOS: a floating panel).
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        display_id: display.map(|display| display.id()),
        window_background: gpui::WindowBackgroundAppearance::Transparent,
        app_id: Some(crate::undock::CHANNEL_APP_ID.to_string()),
        #[cfg(target_os = "linux")]
        window_decorations: Some(gpui::WindowDecorations::Client),
        ..Default::default()
    }
}

/// Start the watcher: one foreground loop for the app's lifetime. The read
/// is a settings field; nothing starts until the switch is on.
pub(crate) fn init(cx: &mut App) {
    let (tx, rx) = std::sync::mpsc::channel::<Option<String>>();
    let _ = PRESENCE.set(Mutex::new(rx));
    // DEV-ONLY (the EXP_DEV_* family): `EXP_DEV_COMPUTER_BANNER=<label>`
    // shows the banner from the start, as if a session held the input
    // (sent after the observer's own first word, the release).
    let dev_banner = std::env::var("EXP_DEV_COMPUTER_BANNER").ok().map(|label| (tx.clone(), label));
    let tx = Mutex::new(tx);
    coding::computer::set_presence_observer(move |label| {
        if let Ok(tx) = tx.lock() {
            let _ = tx.send(label);
        }
    });
    if let Some((tx, label)) = dev_banner {
        let _ = tx.send(Some(label));
    }
    cx.spawn(async move |cx| {
        let mut switch_on = false;
        let mut banner: Option<gpui::WindowHandle<Banner>> = None;
        loop {
            cx.background_executor().timer(TICK).await;
            let on = cx.update(|cx| {
                crate::coding_flow::CodingHub::global_ref(cx)
                    .is_some_and(|hub| hub.read(cx).settings.computer_use)
            });
            if on && !switch_on {
                coding::computer::prepare_in_background();
            } else if !on && switch_on {
                cx.background_executor().spawn(async { coding::computer::shutdown() }).detach();
            }
            switch_on = on;

            // The last word of the hold wins: engaged (with its label) or
            // released.
            let mut latest: Option<Option<String>> = None;
            if let Some(rx) = PRESENCE.get() {
                if let Ok(rx) = rx.lock() {
                    while let Ok(label) = rx.try_recv() {
                        latest = Some(label);
                    }
                }
            }
            let Some(presence) = latest else { continue };
            let _ = cx.update(|cx| match presence {
                Some(label) => {
                    let label = (!label.is_empty()).then(|| SharedString::from(label));
                    // The window is the bar's size, so another label is
                    // another window (no move in the toolkit).
                    let same = banner.as_ref().is_some_and(|handle| {
                        handle.read_with(cx, |view, _| view.label == label).unwrap_or(false)
                    });
                    if same {
                        return;
                    }
                    if let Some(handle) = banner.take() {
                        let _ = handle.update(cx, |_, window, _| window.remove_window());
                    }
                    let options = banner_options(label.as_deref(), cx);
                    match cx.open_window(options, |_, cx| cx.new(|_| Banner { label })) {
                        Ok(handle) => banner = Some(handle),
                        Err(err) => log::warn!("[computer] the banner window did not open: {err}"),
                    }
                }
                None => {
                    if let Some(handle) = banner.take() {
                        let _ = handle.update(cx, |_, window, _| window.remove_window());
                    }
                }
            });
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gpui::{IntoElement, ParentElement as _, Styled as _};

    use super::*;

    /// The label of EXP-1372's picture, cut off on both sides by the old
    /// fixed 560 px window.
    const LONG: &str = "Razer Synapse: Basilisk V3 Pro mappings (retry)";

    /// Labels whose words shape to fractions: the first window (the bare
    /// sum, whole pixels up once) was 1 px narrower than the drawn bar for
    /// these, and the label ellipsized. Under the test platform's
    /// NoopTextSystem every glyph is 0.6 em whatever its weight, so here they
    /// only exercise the box arithmetic (rounding, slack, sides), not the
    /// real semibold-vs-normal measurement.
    const FRACTIONS: [&str; 3] = ["iii", "Razer Synapse: Basilisk", "WWWW mmmm"];

    /// NEW-59 (X11): the banner centres on the run's work screen, not the
    /// primary, through the same scaling.
    #[test]
    fn the_x11_banner_floats_on_the_work_screen() {
        use coding::computer::{Display, ScreenLayout, ScreenRect};
        let layout = ScreenLayout {
            root: (5120, 1440),
            primary: ScreenRect { x: 0, y: 0, width: 2560, height: 1440 },
            work_area: None,
        };
        let work = Display { id: "edid:0123456789abcdef".into(), number: 2, x: 2560, y: 0, width: 2560, height: 1440, primary: false };
        let (x, _) = on_work_screen(layout, Some(&work)).banner_origin(5120., 400., 40., 24.).unwrap();
        assert_eq!(x, 2560. + 1280. - 200.);
        let (x, _) = on_work_screen(layout, None).banner_origin(5120., 400., 40., 24.).unwrap();
        assert_eq!(x, 1280. - 200.);
    }

    /// The banner's view with a probe on the bar: the drawn bar's size.
    struct Probe {
        label: Option<SharedString>,
        drawn: Rc<Cell<(f32, f32)>>,
    }

    impl Render for Probe {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            window.set_rem_size(rem_size(cx));
            let drawn = self.drawn.clone();
            // Unconstrained (an 8000 px row), so the bar takes its own width.
            div().w(px(8000.)).flex().items_start().font_family(cx.theme().font_family.clone()).child(
                div().relative().flex_shrink_0().child(banner_bar(self.label.clone(), cx)).child(
                    gpui::canvas(
                        move |bounds, _, _| drawn.set((f32::from(bounds.size.width), f32::from(bounds.size.height))),
                        |_, _: (), _, _| {},
                    )
                    .absolute()
                    .size_full(),
                ),
            )
        }
    }

    #[test]
    fn the_bar_width_adds_every_box_around_the_words() {
        let words = Words { title: 100., label: None, key: 20., abort: 40. };
        // At 16: 22 bar sides + 138 title item + 100 key item (4 gap + 8
        // + 34 chip + 6 + 40 + 8).
        assert_eq!(words.bar_width(16.), 22. + 138. + 100.);
        assert_eq!(Words { label: Some(50.), ..words }.bar_width(16.), 260. + 62.);
        assert_eq!(Words { title: 100.2, ..words }.bar_width(16.), 261.);
        // At 14 the bar's own spacing shrinks: 19.5 sides, a 3.5 gap.
        assert_eq!(words.bar_width(14.), (19.5 + 138. + 99.5_f32).ceil());
        assert_eq!(bar_height(16.), 50.);
        assert_eq!(bar_height(14.), 48.);
    }

    /// EXP-1372: the window holds the bar the banner draws (never narrower,
    /// at most the slack wider), for no label, a short one, the long one
    /// of the issue's picture and the fractional ones.
    #[gpui::test]
    async fn the_banner_window_holds_the_drawn_bar(cx: &mut gpui::TestAppContext) {
        // NOTE: gpui's test platform shapes with the NoopTextSystem (every
        // glyph 0.6 em, semibold or normal alike), so this checks the box
        // arithmetic around the measured words, never real font metrics.
        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let labels = [None, Some("EXP-1273"), Some(LONG)].into_iter().chain(FRACTIONS.map(Some));
        for label in labels {
            let drawn = Rc::new(Cell::new((0., 0.)));
            let probe = Probe { label: label.map(SharedString::from), drawn: drawn.clone() };
            let (_view, window) = cx.add_window_view(|_, _| probe);
            window.update(|window, cx| window.draw(cx).clear(cx));
            let size = window.update(|_, cx| banner_size(label, f32::MAX, cx));
            let (width, height) = drawn.get();
            assert!(width > 0., "{label:?}: the bar draws");
            let window_width = f32::from(size.width);
            assert!(
                window_width >= width && window_width <= width + BAR_SLACK + 1.,
                "{label:?}: the window is {window_width} wide, the bar {width}"
            );
            assert_eq!(f32::from(size.height), height, "{label:?}: the window is the bar's height");
        }
        // The old fixed window could not hold the issue's label.
        let long = cx.update(|cx| banner_size(Some(LONG), f32::MAX, cx));
        assert!(f32::from(long.width) > 560., "{}", f32::from(long.width));
        // The capped window leaves the label to ellipsize.
        let capped = cx.update(|cx| banner_size(Some(LONG), 300., cx));
        assert_eq!(f32::from(capped.width), 300.);
    }
}
