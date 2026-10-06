//! EXP-1196 — the IDE's side of the device's Computer use switch.
//!
//! The visible "an agent is driving" signal is cua's own agent cursor,
//! drawn by the worker over every action on every OS; the IDE draws nothing
//! of its own. What it does own is the switch's edge: each time the device's
//! switch reads ON after reading off (the app starting with it on, a toggle
//! here, a synced one from another client), the OS permissions are asked
//! and the worker brought up NOW, not mid-run; each time it reads off, the
//! worker stops and its cursor goes with it.

use std::time::Duration;

use gpui::App;

/// How often the switch is read.
const TICK: Duration = Duration::from_millis(400);

/// Start the watcher: one foreground loop for the app's lifetime. The read
/// is a settings field; nothing starts until the switch is on.
pub(crate) fn init(cx: &mut App) {
    cx.spawn(async move |cx| {
        let mut switch_on = false;
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
        }
    })
    .detach();
}
