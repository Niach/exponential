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
//! It takes no focus and no click: Esc is the way out, and the hold ends
//! on the key itself.

use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use gpui::{
    div, point, px, size, App, AppContext as _, Bounds, Context, Div, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, SharedString, Stateful, Styled as _, Window, WindowBounds,
    WindowKind, WindowOptions,
};
use gpui_component::{ActiveTheme as _, Icon, Sizable as _};

use crate::icons::registry;
use crate::surface::glass_bar;

/// How often the switch (and the presence channel) is read.
const TICK: Duration = Duration::from_millis(400);

/// The banner's window: the bar plus air, 16 px above the display's
/// visible bottom (the bulk bar's place over a list).
const BANNER_WIDTH: f32 = 560.;
const BANNER_HEIGHT: f32 = 64.;
const BANNER_BOTTOM: f32 = 16.;

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

/// One `item` of the bar (the bulk bar's: 32 tall, 8 px sides, a glyph
/// and words in the muted foreground).
fn item(cx: &App) -> Div {
    div().flex().items_center().gap(px(6.)).h(px(32.)).px(px(8.)).text_sm().text_color(cx.theme().muted_foreground)
}

/// THE bar (the bulk bar's capsule): the glyph and the title as one item,
/// the session's label as the bold value, then the key and the way out.
pub(crate) fn banner_bar(label: Option<SharedString>, cx: &App) -> Stateful<Div> {
    let words = copy();
    let foreground = cx.theme().foreground;
    let hairline = cx.theme().border;
    let mut bar = glass_bar(cx).id("computer-use-banner").flex_shrink_0().child(
        item(cx)
            .child(Icon::new(registry::NAV_COMPUTER).with_size(px(16.)))
            .child(SharedString::from(words.title.clone())),
    );
    if let Some(label) = label {
        bar = bar.child(
            div().px(px(4.)).text_sm().font_weight(FontWeight::SEMIBOLD).text_color(foreground).child(label),
        );
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().flex().items_end().justify_center().child(banner_bar(self.label.clone(), cx))
    }
}

/// Where the lock's thread leaves what it saw; the foreground loop drains it.
static PRESENCE: OnceLock<Mutex<std::sync::mpsc::Receiver<Option<String>>>> = OnceLock::new();

fn banner_options(cx: &App) -> WindowOptions {
    let display = cx.primary_display();
    // The visible bounds leave the panel or dock out, like the bulk bar
    // floats above the list's bottom, not over the composer.
    let display_bounds = display.as_ref().map(|display| display.visible_bounds());
    let origin = display_bounds
        .map(|bounds| {
            point(
                bounds.origin.x + (bounds.size.width - px(BANNER_WIDTH)) / 2.,
                bounds.origin.y + bounds.size.height - px(BANNER_HEIGHT) - px(BANNER_BOTTOM),
            )
        })
        .unwrap_or_else(|| point(px(0.), px(0.)));
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds { origin, size: size(px(BANNER_WIDTH), px(BANNER_HEIGHT)) })),
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
    let tx = Mutex::new(tx);
    coding::computer::set_presence_observer(move |label| {
        if let Ok(tx) = tx.lock() {
            let _ = tx.send(label);
        }
    });
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
                    match banner.as_ref() {
                        Some(handle) => {
                            let _ = handle.update(cx, |view, _, cx| {
                                view.label = label;
                                cx.notify();
                            });
                        }
                        None => {
                            let options = banner_options(cx);
                            match cx.open_window(options, |_, cx| cx.new(|_| Banner { label })) {
                                Ok(handle) => banner = Some(handle),
                                Err(err) => log::warn!("[computer] the banner window did not open: {err}"),
                            }
                        }
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
