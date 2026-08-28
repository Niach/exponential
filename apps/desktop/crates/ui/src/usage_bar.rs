//! EXP-484 (B1, desktop): how a machine's per-agent rate-limit usage is
//! PRESENTED — the selected window, the three tones, the countdown wording,
//! the stale dimming, and the two elements every surface reuses.
//!
//! The device collects the numbers locally (it never holds, copies or
//! refreshes a credential) and ships them on register/heartbeat into
//! `devices.agent_usage`; every client then renders the same bar off the
//! synced row.
//!
//! Hand-mirrored ×4 against the same fixture and the same test names:
//!   web      apps/web/src/lib/agent-usage.ts
//!   iOS      apps/ios/ExpCore/Sources/Domain/AgentUsagePresentation.swift
//!   Android  apps/android/.../domain/AgentUsagePresentation.kt
//! Changing a rule or a string here means changing it in all four.
//!
//! The selected window is a per-client PREFERENCE (`Settings.usage_window`,
//! keyed by agent id), never server state: which window a person cares about
//! is a local reading habit, and the device rewrites the row every few
//! minutes.

use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, App, Hsla, InteractiveElement, IntoElement,
    ParentElement, SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use coding::agent_usage::{AgentUsage, UsageWindow};

use crate::icons::registry;

/// Numbers older than this are STALE: the bar dims and captions itself
/// `as of <relative>` instead of claiming to be current. Fails closed — a
/// missing or unparsable `fetchedAt` is never fresh.
pub(crate) const USAGE_FRESH_SECS: i64 = 15 * 60;

/// ≥ this percent reads as warning (amber), ≥ [`DANGER_PERCENT`] as danger.
pub(crate) const WARNING_PERCENT: u8 = 75;
pub(crate) const DANGER_PERCENT: u8 = 95;

/// The width of the label column in an expanded row.
const LABEL_W: f32 = 56.;
/// The percent column (right-aligned, tabular).
const PERCENT_W: f32 = 32.;
/// The countdown column.
const COUNTDOWN_W: f32 = 112.;

/// The tone a window's fill takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Severity {
    Normal,
    Warning,
    Danger,
}

/// Tone thresholds — the same three everywhere.
pub(crate) fn severity(percent: u8) -> Severity {
    if percent >= DANGER_PERCENT {
        Severity::Danger
    } else if percent >= WARNING_PERCENT {
        Severity::Warning
    } else {
        Severity::Normal
    }
}

/// The fill color for a severity: muted for normal, the shared YELLOW token
/// for warning (the web's `bg-amber-500`), the theme's danger for danger.
pub(crate) fn severity_color(severity: Severity, cx: &App) -> Hsla {
    match severity {
        Severity::Normal => cx.theme().muted_foreground,
        Severity::Warning => theme::tokens::YELLOW.to_hsla(),
        Severity::Danger => cx.theme().danger,
    }
}

/// Fresh = fetched within [`USAGE_FRESH_SECS`]. FAIL-CLOSED: a missing or
/// unparsable `fetched_at` is never fresh; a stamp in the future (the
/// machine's clock runs ahead) is. The device's own `stale` flag is a
/// SEPARATE dimming input the view ORs in — it does not decide freshness.
pub(crate) fn is_fresh(fetched_at: &str, now_epoch: i64) -> bool {
    let Some(fetched) = crate::comments::parse_epoch(fetched_at) else {
        return false;
    };
    now_epoch - fetched < USAGE_FRESH_SECS
}

/// The window a bar shows: the reader's pinned key when the device still
/// reports it, else the fullest window (ties keep report order). `None` when
/// the agent reports no windows at all.
pub(crate) fn select_window<'a>(
    windows: &'a [UsageWindow],
    preferred: Option<&str>,
) -> Option<&'a UsageWindow> {
    if windows.is_empty() {
        return None;
    }
    if let Some(key) = preferred.filter(|key| !key.is_empty()) {
        if let Some(pinned) = windows.iter().find(|window| window.key == key) {
            return Some(pinned);
        }
    }
    let mut best = &windows[0];
    for window in windows {
        if window.percent > best.percent {
            best = window;
        }
    }
    Some(best)
}

/// `resets in 45m` / `resets in 2h 10m` / `resets in 3d 14h`, and
/// `resets soon` inside the last minute or once the stamp has passed. `None`
/// when the window carries no reset (the device could not read one).
pub(crate) fn format_reset_countdown(resets_at: Option<&str>, now_epoch: i64) -> Option<String> {
    let at = crate::comments::parse_epoch(resets_at?)?;
    let secs = at - now_epoch;
    if secs < 60 {
        return Some("resets soon".to_string());
    }
    let minutes = secs / 60;
    if minutes < 60 {
        return Some(format!("resets in {minutes}m"));
    }
    let hours = minutes / 60;
    if hours < 24 {
        let rest = minutes % 60;
        return Some(if rest == 0 {
            format!("resets in {hours}h")
        } else {
            format!("resets in {hours}h {rest}m")
        });
    }
    let days = hours / 24;
    let rest = hours % 24;
    Some(if rest == 0 {
        format!("resets in {days}d")
    } else {
        format!("resets in {days}d {rest}h")
    })
}

/// `as of 8 minutes ago` — the stale caption (and the offline "when was this
/// probed" line in the device dialog). Empty on an unparsable stamp, like
/// every other `relative_time` caller.
pub(crate) fn as_of_label(stamp: &str, now_epoch: i64) -> String {
    let relative = crate::comments::relative_time(stamp, now_epoch);
    if relative.is_empty() {
        String::new()
    } else {
        format!("as of {relative}")
    }
}

/// Tolerant parse of ONE agent's usage entry off a synced jsonb column:
/// malformed windows drop instead of failing the whole row, and anything
/// that isn't an object at all yields `None`. Never panics — a client must
/// not brick on a newer device's payload.
pub(crate) fn parse_agent_usage(value: &serde_json::Value) -> Option<AgentUsage> {
    let object = value.as_object()?;
    let mut usage = AgentUsage {
        fetched_at: object
            .get("fetchedAt")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string(),
        stale: object
            .get("stale")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        windows: Vec::new(),
    };
    if let Some(windows) = object.get("windows").and_then(|value| value.as_array()) {
        for entry in windows.iter().take(coding::agent_usage::MAX_WINDOWS) {
            if let Some(window) = parse_window(entry) {
                usage.windows.push(window);
            }
        }
    }
    Some(usage)
}

fn parse_window(value: &serde_json::Value) -> Option<UsageWindow> {
    let object = value.as_object()?;
    let key = object.get("key")?.as_str()?;
    let label = object.get("label")?.as_str()?;
    if key.is_empty() || label.is_empty() {
        return None;
    }
    let percent = object
        .get("percent")
        .and_then(|value| value.as_f64())
        .unwrap_or(0.)
        .round()
        .clamp(0., 100.) as u8;
    Some(UsageWindow {
        key: key.to_string(),
        label: label.to_string(),
        percent,
        resets_at: object
            .get("resetsAt")
            .and_then(|value| value.as_str())
            .map(str::to_string),
    })
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

/// Everything one usage bar renders from.
pub(crate) struct UsageBarProps<'a> {
    /// Element-id prefix — one bar per surface per agent.
    pub id: SharedString,
    pub usage: &'a AgentUsage,
    /// The pinned window key (`Settings.usage_window[agent]`).
    pub selected: Option<&'a str>,
    /// Whether the per-window rows are showing.
    pub expanded: bool,
    pub now_epoch: i64,
}

/// `5h · 78%` — the collapsed track's tooltip; stale numbers say when they
/// were taken.
fn summary(props: &UsageBarProps, window: &UsageWindow) -> SharedString {
    let mut text = format!("{} · {}%", window.label, window.percent);
    if props.usage.stale || !is_fresh(&props.usage.fetched_at, props.now_epoch) {
        let as_of = as_of_label(&props.usage.fetched_at, props.now_epoch);
        if !as_of.is_empty() {
            text.push_str(&format!(" · {as_of}"));
        }
    }
    SharedString::from(text)
}

/// The bar: a collapsed 3px track (click toggles), plus the per-window rows
/// while `expanded`. Renders nothing when the agent reports no windows.
///
/// `on_toggle` flips the caller's expanded state; `on_select` receives the
/// picked window KEY (the caller persists it as the agent's preference and
/// collapses).
///
/// Surfaces that need the two halves in DIFFERENT layout slots (the terminal
/// dock's toolbar puts the track in the 24px row and the rows underneath it)
/// call [`render_usage_track`] and [`render_usage_windows`] directly.
pub(crate) fn render_usage_bar(
    props: UsageBarProps,
    on_toggle: impl Fn(&mut Window, &mut App) + 'static,
    on_select: impl Fn(String, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let expanded = props.expanded;
    let track = render_usage_track(&props, on_toggle, cx);
    if !expanded {
        return track;
    }
    v_flex()
        .w_full()
        .gap_1()
        .child(track)
        .child(render_usage_windows(&props, on_select, cx))
        .into_any_element()
}

/// The collapsed track alone: one hairline filled to the selected window's
/// percent, in a 14px hit area with the `"<label> · NN%"` tooltip.
pub(crate) fn render_usage_track(
    props: &UsageBarProps,
    on_toggle: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let Some(selected) = select_window(&props.usage.windows, props.selected) else {
        return div().into_any_element();
    };
    let stale = props.usage.stale;
    let tooltip = summary(props, selected);
    let track = div()
        .id(SharedString::from(format!("{}-track", props.id)))
        .flex()
        .items_center()
        .w_full()
        .h(px(14.))
        .cursor_pointer()
        .when(stale, |this| this.opacity(0.55))
        .tooltip(move |window, cx| {
            gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
        })
        .on_click(move |_, window, cx| on_toggle(window, cx))
        .child(
            div()
                .w_full()
                .h(px(3.))
                .rounded_full()
                .bg(cx.theme().border.opacity(0.6))
                .child(
                    div()
                        .h_full()
                        .rounded_full()
                        .w(gpui::relative(selected.percent as f32 / 100.))
                        .bg(severity_color(severity(selected.percent), cx)),
                ),
        );

    track.into_any_element()
}

/// Every window the machine reported: label · bar · percent · countdown,
/// with the pinned one marked. Clicking a row pins it.
pub(crate) fn render_usage_windows(
    props: &UsageBarProps,
    on_select: impl Fn(String, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let Some(selected) = select_window(&props.usage.windows, props.selected) else {
        return div().into_any_element();
    };
    let stale = props.usage.stale;
    let on_select = std::rc::Rc::new(on_select);
    let mut rows = v_flex().w_full().gap_0p5().when(stale, |this| this.opacity(0.55));
    for (index, window) in props.usage.windows.iter().enumerate() {
        let active = window.key == selected.key;
        let marker = if active {
            registry::UI_SELECTED
        } else {
            registry::UI_UNSELECTED
        };
        let countdown = format_reset_countdown(window.resets_at.as_deref(), props.now_epoch)
            .unwrap_or_default();
        let key = window.key.clone();
        let on_select = on_select.clone();
        rows = rows.child(
            h_flex()
                .id(SharedString::from(format!("{}-window-{index}", props.id)))
                .w_full()
                .items_center()
                .gap_2()
                .py_0p5()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .cursor_pointer()
                .on_click(move |_, window, cx| on_select(key.clone(), window, cx))
                .child(Icon::new(marker).xsmall().flex_shrink_0())
                .child(
                    div()
                        .w(px(LABEL_W))
                        .flex_shrink_0()
                        .truncate()
                        .child(SharedString::from(window.label.clone())),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .h(px(4.))
                        .rounded_full()
                        .bg(cx.theme().border.opacity(0.6))
                        .child(
                            div()
                                .h_full()
                                .rounded_full()
                                .w(gpui::relative(window.percent as f32 / 100.))
                                .bg(severity_color(severity(window.percent), cx)),
                        ),
                )
                .child(
                    div()
                        .w(px(PERCENT_W))
                        .flex_shrink_0()
                        .text_right()
                        .child(SharedString::from(format!("{}%", window.percent))),
                )
                .child(
                    div()
                        .w(px(COUNTDOWN_W))
                        .flex_shrink_0()
                        .truncate()
                        .text_right()
                        .child(SharedString::from(countdown)),
                ),
        );
    }
    if stale {
        let as_of = as_of_label(&props.usage.fetched_at, props.now_epoch);
        if !as_of.is_empty() {
            rows = rows.child(
                div()
                    .pl(px(20.))
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(as_of)),
            );
        }
    }
    rows.into_any_element()
}

/// The agent's pinned window key out of the local preference map.
pub(crate) fn preferred_window(settings: &coding::Settings, agent: coding::CodingAgent) -> Option<String> {
    settings.usage_window.get(agent.id()).cloned()
}

/// Persist the agent's pinned window key (a UI pref — no doctor re-run, and
/// deliberately outside the launch-defaults wire).
pub(crate) fn persist_window(agent: coding::CodingAgent, key: String, cx: &mut App) {
    let hub = crate::coding_flow::CodingHub::global(cx);
    let mut settings = hub.read(cx).settings.clone();
    if settings.usage_window.get(agent.id()) == Some(&key) {
        return;
    }
    settings.usage_window.insert(agent.id().to_string(), key);
    crate::coding_flow::CodingHub::save_ui_prefs(&hub, settings, cx);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(key: &str, label: &str, percent: u8, resets_at: Option<&str>) -> UsageWindow {
        UsageWindow {
            key: key.to_string(),
            label: label.to_string(),
            percent,
            resets_at: resets_at.map(str::to_string),
        }
    }

    /// The three tones cross at 75 and 95 — the same thresholds on every
    /// client.
    #[test]
    fn severity_thresholds() {
        assert_eq!(severity(0), Severity::Normal);
        assert_eq!(severity(74), Severity::Normal);
        assert_eq!(severity(75), Severity::Warning);
        assert_eq!(severity(94), Severity::Warning);
        assert_eq!(severity(95), Severity::Danger);
        assert_eq!(severity(100), Severity::Danger);
    }

    /// The pin wins while the device still reports it; otherwise the fullest
    /// window does (ties keep report order).
    #[test]
    fn select_window_prefers_persisted_key_then_highest_percent() {
        let windows = vec![
            window("session", "5h", 12, None),
            window("weekly", "Week", 80, None),
            window("model:fable", "Fable", 80, None),
        ];
        assert_eq!(
            select_window(&windows, Some("session")).map(|w| w.key.as_str()),
            Some("session")
        );
        // A pin the device no longer reports falls back to the fullest one,
        // and the FIRST of two equal ones wins.
        assert_eq!(
            select_window(&windows, Some("credits")).map(|w| w.key.as_str()),
            Some("weekly")
        );
        assert_eq!(
            select_window(&windows, None).map(|w| w.key.as_str()),
            Some("weekly")
        );
        // An empty pin is no pin, and no windows is no selection.
        assert_eq!(
            select_window(&windows, Some("")).map(|w| w.key.as_str()),
            Some("weekly")
        );
        assert!(select_window(&[], Some("session")).is_none());
    }

    /// The countdown wording, verbatim across the four clients — including
    /// the "drop a zero smaller unit" rule and the past/soon collapse.
    #[test]
    fn format_reset_countdown_hours_minutes_days_and_past() {
        let now = 1_756_000_000_i64;
        let at = |offset: i64| {
            chrono::DateTime::from_timestamp(now + offset, 0)
                .unwrap()
                .to_rfc3339()
        };
        let go = |offset: i64| format_reset_countdown(Some(&at(offset)), now);

        assert_eq!(go(45 * 60).as_deref(), Some("resets in 45m"));
        assert_eq!(go(2 * 3_600 + 10 * 60).as_deref(), Some("resets in 2h 10m"));
        // A zero smaller unit is dropped, never rendered as `2h 0m`.
        assert_eq!(go(2 * 3_600).as_deref(), Some("resets in 2h"));
        assert_eq!(
            go(3 * 86_400 + 14 * 3_600).as_deref(),
            Some("resets in 3d 14h")
        );
        assert_eq!(go(3 * 86_400).as_deref(), Some("resets in 3d"));
        // Inside the last minute, and once the stamp has passed.
        assert_eq!(go(30).as_deref(), Some("resets soon"));
        assert_eq!(go(-600).as_deref(), Some("resets soon"));
        // No stamp / an unusable one renders no countdown at all.
        assert_eq!(format_reset_countdown(None, now), None);
        assert_eq!(format_reset_countdown(Some("nonsense"), now), None);
    }

    /// Freshness fails closed: 15 minutes is the line, and a stamp that
    /// cannot be read is never fresh.
    #[test]
    fn stale_usage_older_than_fifteen_minutes_is_not_fresh() {
        let now = 1_756_000_000_i64;
        let at = |offset: i64| {
            chrono::DateTime::from_timestamp(now + offset, 0)
                .unwrap()
                .to_rfc3339()
        };
        assert!(is_fresh(&at(-60), now));
        assert!(is_fresh(&at(-(USAGE_FRESH_SECS - 1)), now));
        assert!(!is_fresh(&at(-USAGE_FRESH_SECS), now));
        assert!(!is_fresh(&at(-3_600), now));
        // A machine whose clock runs ahead still reads fresh.
        assert!(is_fresh(&at(300), now));
        // Unparsable / empty stamps never do.
        assert!(!is_fresh("", now));
        assert!(!is_fresh("garbage", now));
    }

    /// The synced jsonb is parsed tolerantly: bad windows drop, percent
    /// clamps, and a non-object yields nothing to render.
    #[test]
    fn parse_agent_usage_tolerates_garbage() {
        let value = serde_json::json!({
            "fetchedAt": "2026-08-28T10:00:00.000Z",
            "stale": true,
            "windows": [
                { "key": "session", "label": "5h", "percent": 142.6, "resetsAt": null },
                { "key": "", "label": "Week", "percent": 10 },
                { "label": "no key", "percent": 10 },
                { "key": "weekly", "label": "Week", "percent": 33.4,
                  "resetsAt": "2026-08-29T10:00:00.000Z" },
            ],
        });
        let usage = parse_agent_usage(&value).expect("object parses");
        assert!(usage.stale);
        assert_eq!(usage.fetched_at, "2026-08-28T10:00:00.000Z");
        assert_eq!(usage.windows.len(), 2);
        assert_eq!(usage.windows[0].percent, 100, "percent clamps to 100");
        assert_eq!(usage.windows[1].percent, 33, "percent rounds");
        assert_eq!(
            usage.windows[1].resets_at.as_deref(),
            Some("2026-08-29T10:00:00.000Z")
        );
        assert!(parse_agent_usage(&serde_json::json!("nope")).is_none());
    }
}
