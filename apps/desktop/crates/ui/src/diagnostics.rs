//! #LOGS-2: the gpui half of `observability`.
//!
//! - the connected account's server + user id → `observability::connect`
//!   (fetches the server's monitoring config, starts/stops the client);
//! - the main-thread heartbeat (`lag::tick` every `lag::TICK`) the hang
//!   watchdog listens for;
//! - gpui's own per-draw timings (`Window::draw` start/end) →
//!   `lag::record_frame`.
//!
//! Idle while nothing listens: one check every few seconds, no timer at
//! display rate, frame tracing left off.

use std::time::Duration;

use gpui::App;
use observability::lag;

/// While nothing listens: how often to look again.
const IDLE_RECHECK: Duration = Duration::from_secs(5);
/// Frame timings are drained this often (gpui buffers them).
const DRAIN_EVERY: u32 = 5;

pub fn install(cx: &mut App) {
    // Connect whenever the signed-in account changes.
    let state = sync::Store::global(cx).state();
    let mut connected: Option<String> = None;
    let mut connect = move |cx: &mut App| {
        let account_id = sync::Store::global(cx).session(cx).account_id().map(str::to_string);
        if account_id == connected {
            return;
        }
        connected = account_id.clone();
        let account = account_id.and_then(|id| cx.try_global::<crate::AuthContext>()?.auth.account(&id));
        if let Some(account) = account {
            observability::connect(&account.instance_url, Some(&account.user_id));
        }
    };
    connect(cx);
    cx.observe(&state, move |_, cx| connect(cx)).detach();

    cx.spawn(async move |cx| {
        #[cfg(feature = "frame-stats")]
        let mut frames: Option<gpui::FrameTimingCollector> = None;
        let mut beat = 0u32;
        loop {
            if !lag::active() {
                cx.background_executor().timer(IDLE_RECHECK).await;
                continue;
            }
            #[cfg(feature = "frame-stats")]
            if frames.is_none() {
                gpui::set_trace_enabled(true);
                frames = Some(gpui::FrameTimingCollector::new());
            }
            cx.background_executor().timer(lag::TICK).await;
            let foreground = cx.update(|cx| cx.active_window().is_some());
            lag::tick(foreground);
            beat += 1;
            #[cfg(feature = "frame-stats")]
            if beat % DRAIN_EVERY == 0 {
                if let Some(collector) = frames.as_mut() {
                    for event in collector.collect_unseen() {
                        if let gpui::FrameEvent::Draw(frame) = event {
                            lag::record_frame(frame.draw_start, frame.draw_end, frame.invalidations);
                        }
                    }
                }
            }
        }
    })
    .detach();
}

/// Settings › Account › Privacy: the "Send diagnostics" switch.
pub fn enabled() -> bool {
    observability::diagnostics_enabled()
}

/// Flip it off the UI thread: closing the client flushes (bounded).
pub fn set_enabled(enabled: bool, cx: &mut App) {
    cx.background_executor()
        .spawn(async move { observability::set_diagnostics_enabled(enabled) })
        .detach();
}
