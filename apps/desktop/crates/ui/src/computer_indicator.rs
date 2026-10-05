//! EXP-1196 — the "an agent is driving this computer" pill.
//!
//! Computer use moves the person's OWN pointer and keyboard, usually in some
//! other app's window, so the notice cannot live inside the IDE: it is a
//! small always-on-top window at the top of the primary display, up while a
//! run acted within [`coding::computer::DRIVING_WINDOW`] and gone a moment
//! after the last action. It never takes focus (the agent's keys must keep
//! landing where they were going).
//!
//! The headless daemon has no window to draw; it posts one OS notification
//! per run instead (`crates/cli`).

use std::time::Duration;

use gpui::{
    div, px, size, AnyWindowHandle, App, AppContext as _, Bounds, Context, IntoElement,
    ParentElement as _, Point, Render, Styled as _, Window, WindowBackgroundAppearance,
    WindowBounds, WindowKind, WindowOptions,
};
use gpui_component::{ActiveTheme as _, Root};

use crate::surface::{self, PillMode, PillSize};

/// How often the pill checks whether a run is driving.
const TICK: Duration = Duration::from_millis(400);
/// Re-read the keyboard layout every this many ticks (~10 s): the person
/// may switch input sources while a run is driving.
const LAYOUT_REFRESH_TICKS: u32 = 25;
const PILL_W: f32 = 264.;
const PILL_H: f32 = 32.;
/// Below the menu bar / a top panel.
const TOP_INSET: f32 = 44.;

pub(crate) const DRIVING_LABEL: &str = "An agent is driving this computer";

struct DrivingPill;

impl Render for DrivingPill {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (ground, tone) = (cx.theme().background, cx.theme().success);
        // The one desktop capsule (`surface::glass_pill`) on an OPAQUE disc
        // of the page colour: glass alone would be unreadable over whatever
        // the desktop shows behind this window.
        // The pill FILLS the window, so the window's own edge is the pill's.
        div().size_full().rounded_full().bg(ground).child(
            surface::glass_pill("computer-driving", PillSize::Md, PillMode::Readonly, cx)
                .size_full()
                .justify_center()
                .child(surface::live_dot(tone, true))
                .child(DRIVING_LABEL),
        )
    }
}

fn open(cx: &mut App) -> Option<AnyWindowHandle> {
    let display = cx.primary_display()?;
    let area = display.bounds();
    let bounds = Bounds {
        origin: Point {
            x: area.origin.x + (area.size.width - px(PILL_W)) / 2.,
            y: area.origin.y + px(TOP_INSET),
        },
        size: size(px(PILL_W), px(PILL_H)),
    };
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: None,
        focus: false,
        show: true,
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        display_id: Some(display.id()),
        window_background: WindowBackgroundAppearance::Transparent,
        ..Default::default()
    };
    cx.open_window(options, |window, cx| {
        let pill = cx.new(|_| DrivingPill);
        // Root MUST be the first view of every window (§3.3).
        cx.new(|cx| Root::new(pill, window, cx).bg(gpui::transparent_black()))
    })
    .ok()
    .map(Into::into)
}

/// Start the watcher: one foreground loop for the app's lifetime. The check
/// is a lock-free read until the first run is granted computer use.
///
/// It also keeps the keyboard layout a chord's character keys resolve
/// against current: macOS only answers that on the main thread, which is
/// where this loop's `cx.update` runs and where the server's worker threads
/// never are.
///
/// And it asks the OS permissions up front: each time the device's switch
/// reads ON after reading off (the app starting with it on, a toggle here,
/// a synced one from another client), every dialog comes now, not mid-run.
pub(crate) fn init(cx: &mut App) {
    coding::computer::refresh_key_layout();
    cx.spawn(async move |cx| {
        let mut pill: Option<AnyWindowHandle> = None;
        let mut ticks = 0u32;
        let mut switch_on = false;
        loop {
            cx.background_executor().timer(TICK).await;
            ticks = ticks.wrapping_add(1);
            let on = cx.update(|cx| {
                crate::coding_flow::CodingHub::global_ref(cx)
                    .is_some_and(|hub| hub.read(cx).settings.computer_use)
            });
            if on && !switch_on {
                coding::computer::prepare_in_background();
            }
            switch_on = on;
            if ticks % LAYOUT_REFRESH_TICKS == 0 {
                cx.update(|_| coding::computer::refresh_key_layout());
            }
            let driving = coding::computer::driving().is_some();
            if driving == pill.is_some() {
                continue;
            }
            pill = cx.update(|cx| match pill.take() {
                Some(handle) => {
                    let _ = handle.update(cx, |_, window, _| window.remove_window());
                    None
                }
                None => open(cx),
            });
        }
    })
    .detach();
}
