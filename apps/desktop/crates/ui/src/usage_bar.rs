//! EXP-484 (B1, desktop): how a machine's per-agent rate-limit usage is
//! PRESENTED — the grouping, the three tones, the countdown wording, the
//! stale dimming, and the cards every surface reuses.
//!
//! The device collects the numbers locally (it never holds, copies or
//! refreshes a credential) and ships them on register/heartbeat into
//! `devices.agent_usage`; every client then renders the same cards off the
//! synced row.
//!
//! Hand-mirrored ×4 against the same fixture and the same test names:
//!   web      apps/web/src/lib/agent-usage.ts
//!   iOS      apps/ios/ExpCore/Sources/Domain/AgentUsagePresentation.swift
//!   Android  apps/android/.../domain/AgentUsagePresentation.kt
//! Changing a rule or a string here means changing it in all four.
//!
//! EXP-688: there is no "pinned window" any more. Claude's own app shows
//! every window at once (Current session / All models / Fable only), so
//! [`usage_groups`] renders the machine's whole report as cards in three
//! fixed groups and the local `Settings.usage_window` preference is gone.

use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, App, Hsla, InteractiveElement as _,
    IntoElement, ParentElement, SharedString, Styled,
};
use gpui_component::{v_flex, ActiveTheme as _};

use coding::agent_usage::{AgentUsage, UsageWindow};

/// Numbers older than this are STALE: the cards dim and caption themselves
/// `as of <relative>` instead of claiming to be current. Fails closed — a
/// missing or unparsable `fetchedAt` is never fresh.
pub(crate) const USAGE_FRESH_SECS: i64 = 15 * 60;

/// ≥ this percent reads as warning (amber), ≥ [`DANGER_PERCENT`] as danger.
pub(crate) const WARNING_PERCENT: u8 = 75;
pub(crate) const DANGER_PERCENT: u8 = 95;

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

// ---------------------------------------------------------------------------
// Grouping (the ×4 contract)
// ---------------------------------------------------------------------------

/// One rendered limit: a window with its display title, tone and caption
/// already resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UsageCard {
    /// The wire window key it came from.
    pub key: String,
    pub title: String,
    pub percent: u8,
    pub severity: Severity,
    /// `resets in 2h 10m`, `Starts when a message is sent`, or empty.
    pub caption: String,
}

/// One titled group of cards. `key` is `session` / `weekly` / `other`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UsageGroup {
    pub key: &'static str,
    pub title: &'static str,
    pub cards: Vec<UsageCard>,
}

/// The whole report, grouped the way Claude's own app groups it:
///
/// * `session` → **Current session** (title "Current session");
/// * `weekly` + `model:*` → **Weekly limits**, the all-models window first
///   ("All models") then the per-model ones in report order ("Fable only");
/// * everything else (`credits`, codex's `43200`) → **Other**, in report
///   order, under its wire label.
///
/// Empty groups are omitted and the order is fixed. The caption is the reset
/// countdown when the window carries one; an idle session window at 0% with
/// no reset says so instead of rendering a blank line.
pub(crate) fn usage_groups(usage: &AgentUsage, now_epoch: i64) -> Vec<UsageGroup> {
    let mut session = Vec::new();
    let mut weekly = Vec::new();
    let mut models = Vec::new();
    let mut other = Vec::new();
    for window in &usage.windows {
        let card = usage_card(window, now_epoch);
        match window.key.as_str() {
            "session" => session.push(card),
            "weekly" => weekly.push(card),
            key if key.starts_with("model:") => models.push(card),
            _ => other.push(card),
        }
    }
    weekly.append(&mut models);
    [
        ("session", "Current session", session),
        ("weekly", "Weekly limits", weekly),
        ("other", "Other", other),
    ]
    .into_iter()
    .filter(|(_, _, cards)| !cards.is_empty())
    .map(|(key, title, cards)| UsageGroup { key, title, cards })
    .collect()
}

fn usage_card(window: &UsageWindow, now_epoch: i64) -> UsageCard {
    let title = match window.key.as_str() {
        "session" => "Current session".to_string(),
        "weekly" => "All models".to_string(),
        key if key.starts_with("model:") => format!("{} only", window.label),
        _ => window.label.clone(),
    };
    let caption = if window.resets_at.is_some() {
        format_reset_countdown(window.resets_at.as_deref(), now_epoch).unwrap_or_default()
    } else if window.key == "session" && window.percent == 0 {
        "Starts when a message is sent".to_string()
    } else {
        String::new()
    };
    UsageCard {
        key: window.key.clone(),
        title,
        percent: window.percent,
        severity: severity(window.percent),
        caption,
    }
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

/// The track's height — thick enough to read as a bar rather than the old
/// 3px hairline, which is what made the toolbar's usage strip unreadable.
const TRACK_H: f32 = 6.;

/// Every window the machine reported, as one glass card each under its
/// group heading (the session group needs none — its single card is titled
/// "Current session"). Renders nothing when the agent reports no windows.
pub(crate) fn render_usage_cards(
    agent: coding::CodingAgent,
    usage: &AgentUsage,
    now_epoch: i64,
    cx: &App,
) -> AnyElement {
    let groups = usage_groups(usage, now_epoch);
    if groups.is_empty() {
        return div().into_any_element();
    }
    let muted = cx.theme().muted_foreground;
    let mut body = v_flex()
        .id(SharedString::from(format!("usage-cards-{}", agent.id())))
        .w_full()
        .gap_2()
        .when(usage.stale, |this| this.opacity(0.55));
    for group in groups {
        if group.key != "session" {
            body = body.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(group.title)),
            );
        }
        for card in group.cards {
            body = body.child(render_usage_card(&card, cx));
        }
    }
    body.into_any_element()
}

fn render_usage_card(card: &UsageCard, cx: &App) -> gpui::Div {
    let muted = cx.theme().muted_foreground;
    let mut body = crate::surface::glass_card()
        .w_full()
        .gap_1p5()
        .px_2p5()
        .py_2()
        .child(
            gpui_component::h_flex()
                .w_full()
                .items_center()
                .gap_2()
                .text_xs()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(SharedString::from(card.title.clone())),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(muted)
                        .child(SharedString::from(format!("{}% used", card.percent))),
                ),
        )
        .child(
            div()
                .w_full()
                .h(px(TRACK_H))
                .rounded_full()
                .bg(cx.theme().border.opacity(0.6))
                .child(
                    div()
                        .h_full()
                        .rounded_full()
                        .w(gpui::relative(card.percent as f32 / 100.))
                        .bg(severity_color(card.severity, cx)),
                ),
        );
    if !card.caption.is_empty() {
        body = body.child(
            div()
                .text_xs()
                .text_color(muted)
                .child(SharedString::from(card.caption.clone())),
        );
    }
    body
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

    /// EXP-688, the ×4 grouping contract: three fixed groups in a fixed
    /// order, the all-models window ahead of the per-model ones, the titles
    /// ("All models" / "Fable only" / the wire label for the rest), and the
    /// two caption forms.
    #[test]
    fn usage_groups_split_current_weekly_and_other() {
        let now = 1_756_000_000_i64;
        let at = |offset: i64| {
            chrono::DateTime::from_timestamp(now + offset, 0)
                .unwrap()
                .to_rfc3339()
        };
        let usage = AgentUsage {
            fetched_at: at(-60),
            stale: false,
            windows: vec![
                // An idle machine: 0% and no reset yet.
                window("session", "5h", 0, None),
                window("weekly", "Week", 61, Some(&at(2 * 3_600 + 10 * 60))),
                window("model:fable", "Fable", 12, None),
                window("credits", "Credits", 16, None),
            ],
        };
        let groups = usage_groups(&usage, now);
        assert_eq!(
            groups
                .iter()
                .map(|group| (group.key, group.title))
                .collect::<Vec<_>>(),
            vec![
                ("session", "Current session"),
                ("weekly", "Weekly limits"),
                ("other", "Other"),
            ]
        );
        assert_eq!(
            groups
                .iter()
                .flat_map(|group| group.cards.iter())
                .map(|card| (card.key.as_str(), card.title.as_str(), card.percent))
                .collect::<Vec<_>>(),
            vec![
                ("session", "Current session", 0),
                ("weekly", "All models", 61),
                ("model:fable", "Fable only", 12),
                ("credits", "Credits", 16),
            ]
        );
        // Captions: the countdown when there is a reset, the idle-session
        // line at 0% without one, nothing otherwise.
        assert_eq!(groups[0].cards[0].caption, "Starts when a message is sent");
        assert_eq!(groups[1].cards[0].caption, "resets in 2h 10m");
        assert_eq!(groups[1].cards[1].caption, "");
        assert_eq!(groups[2].cards[0].caption, "");
        assert_eq!(groups[1].cards[0].severity, Severity::Normal);

        // A machine reporting nothing renders nothing.
        assert!(usage_groups(&AgentUsage::default(), now).is_empty());
        // A used session window captions the countdown, never the idle line.
        let busy = AgentUsage {
            fetched_at: at(-60),
            stale: false,
            windows: vec![window("session", "5h", 42, None)],
        };
        assert_eq!(usage_groups(&busy, now)[0].cards[0].caption, "");
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
