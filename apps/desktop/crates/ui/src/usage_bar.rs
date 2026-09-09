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
//! EXP-807: the cross-device USAGE PAGE's model lives here too — the twin of
//! the web `agentProfileUsageRows` / `peakPercent` / `attentionRank` /
//! `sortAttentionFirst` / `refreshAllowedAt` (`agent-usage.ts`, bottom
//! section). It is a web+desktop pair, not a ×4 rule: iOS and Android ship no
//! usage PAGE (`packages/view-catalog/views.json` says so), only the per-run
//! sheet the cards above feed.
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
        // EXP-698: a NORMAL bar is not a "muted" thing, it is the glass
        // foreground at 30% over the track — the web/mobile meter fill.
        Severity::Normal => cx.theme().foreground.opacity(0.30),
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
    /// EXP-694: EMPTY means the group renders WITHOUT a heading — the weekly
    /// group's cards ("All models", "<Model> only") already name themselves,
    /// and a "Weekly limits" line above them was one label too many.
    /// Renderers skip an empty title.
    pub title: &'static str,
    pub cards: Vec<UsageCard>,
}

/// The whole report, grouped the way Claude's own app groups it:
///
/// * `session` → **Current session** (title "Current session");
/// * `weekly` + `model:*` → the UNTITLED weekly group (EXP-694), the
///   all-models window first ("All models") then the per-model ones in report
///   order ("Fable only");
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
        ("weekly", "", weekly),
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

/// EXP-804: `coding_sessions.blocked` — the agent's usage wall as row state.
/// Every field is optional for the same reason the server's zod mirror is
/// `.nullish()` throughout: a newer device naming a window this build has no
/// name for must degrade that field, never fail the whole parse and leave a
/// walled run rendering healthy.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct CodingSessionBlocked {
    pub kind: Option<String>,
    pub agent: Option<String>,
    pub window: Option<String>,
    pub resets_at: Option<String>,
    pub since: Option<String>,
}

/// Tolerant parse of a run's `blocked` jsonb column. `None` on absent or
/// unusable JSON — a run is then simply not shown as blocked, never guessed.
pub(crate) fn parse_blocked(value: Option<&serde_json::Value>) -> Option<CodingSessionBlocked> {
    let object = value?.as_object()?;
    let string = |key: &str| {
        object
            .get(key)
            .and_then(|value| value.as_str())
            .map(str::to_string)
    };
    Some(CodingSessionBlocked {
        kind: string("kind"),
        agent: string("agent"),
        window: string("window"),
        resets_at: string("resetsAt"),
        since: string("since"),
    })
}

/// EXP-804: the one-line badge for a run's usage wall — `Rate limited ·
/// resets in 2h`, or bare `Rate limited` when the agent named no reset time.
/// `None` when the run is not blocked.
///
/// The wall is ORTHOGONAL to the session state: a blocked run still reads
/// `running`, so this NEVER replaces the display state — it renders beside
/// it. An unrecognised `kind` still gets a badge (`Blocked`): a future device
/// reporting a wall this build has no name for must not render silent.
/// Locked ×4 (web `blockedBadgeLabel`).
pub(crate) fn blocked_badge_label(
    blocked: Option<&CodingSessionBlocked>,
    now_epoch: i64,
) -> Option<String> {
    let blocked = blocked?;
    let label = match blocked.kind.as_deref().unwrap_or("rate_limit") {
        "rate_limit" => "Rate limited",
        _ => "Blocked",
    };
    match format_reset_countdown(blocked.resets_at.as_deref(), now_epoch) {
        Some(countdown) => Some(format!("{label} · {countdown}")),
        None => Some(label.to_string()),
    }
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

/// Every window the machine reported, under its group heading (a group with
/// an empty title renders none, and the session group's single card is
/// already titled "Current session"). Renders nothing when the agent reports
/// no windows.
///
/// `compact` is the grouped-stack arm (EXP-694, ×4 `compact`): the windows
/// sit INSIDE the agent's own glass group in the device editor and the
/// settings panes, so each one is a PLAIN row — no nested `glass_card()`,
/// because the group around them already draws the surface. `false` keeps
/// the standalone carded look for any surface that renders them on their own.
pub(crate) fn render_usage_cards(
    agent: coding::CodingAgent,
    usage: &AgentUsage,
    now_epoch: i64,
    compact: bool,
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
        .gap(px(if compact { 12. } else { 16. }))
        .when(usage.stale, |this| this.opacity(0.55));
    for group in groups {
        let mut column = v_flex().w_full().gap_2();
        if !group.title.is_empty() && group.key != "session" {
            column = column.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(group.title)),
            );
        }
        for card in group.cards {
            column = column.child(render_usage_card(&card, compact, cx));
        }
        body = body.child(column);
    }
    body.into_any_element()
}

fn render_usage_card(card: &UsageCard, compact: bool, cx: &App) -> gpui::Div {
    let muted = cx.theme().muted_foreground;
    let mut body = if compact {
        v_flex().gap_1p5()
    } else {
        crate::surface::glass_card().gap_1p5().px_2p5().py_2()
    }
    .w_full()
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
                // EXP-698: the track is the glass strong stroke, not a
                // dimmed chrome border.
                .bg(theme::tokens::glass::STROKE_STRONG.to_hsla())
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

// ---------------------------------------------------------------------------
// EXP-746: the per-SESSION context meter
// ---------------------------------------------------------------------------
//
// A different quantity from everything above: the windows are this MACHINE's
// rate limits, up to a heartbeat stale, while this is the live conversation's
// token budget on the wire (`ActivityEvent::Usage`). It renders BESIDE
// `usage_groups`, never inside it — that grouping is fixture-locked ×4, and a
// token count has no reset window to belong to.
//
// Mirrored ×4 with the same test names:
//   web      apps/web/src/lib/agent-usage.ts
//   iOS      AgentUsagePresentation.swift
//   Android  domain/AgentUsagePresentation.kt

/// The context block's heading. Byte-identical ×4.
pub(crate) const CONTEXT_SECTION_TITLE: &str = "Context";

/// How full the context is, 0-100 and FLOORED. `None` when the run reports no
/// window at all (an agent that never sent a `usage` frame, or a zero size —
/// dividing by which is the one thing worse than showing nothing).
pub(crate) fn context_percent(usage: Option<&steer::SessionUsage>) -> Option<u8> {
    let usage = usage?;
    if usage.context_size <= 0 {
        return None;
    }
    let used = usage.context_used.max(0);
    Some(((used.saturating_mul(100) / usage.context_size).min(100)) as u8)
}

/// `124k / 200k (62%)` — thousands collapsed to `k` from 1000 up, no
/// decimals, the percent floored. Empty when there is nothing to report.
pub(crate) fn format_context_usage(usage: Option<&steer::SessionUsage>) -> String {
    let Some(percent) = context_percent(usage) else {
        return String::new();
    };
    let usage = match usage {
        Some(usage) => usage,
        None => return String::new(),
    };
    format!(
        "{} / {} ({percent}%)",
        format_tokens(usage.context_used.max(0)),
        format_tokens(usage.context_size)
    )
}

/// `124000` → `124k`, `999` → `999`. Rounded, never truncated: `1500` reads
/// `2k`, because a token count is an estimate and a floor would report less
/// than was spent.
fn format_tokens(tokens: i64) -> String {
    if tokens < 1000 {
        return tokens.to_string();
    }
    format!("{}k", (tokens + 500) / 1000)
}

/// `$1.24`, or `None` under half a cent — a run that cost a fraction of a
/// cent reads as free, and `$0.00` would be a lie in the other direction.
pub(crate) fn format_usage_cost(usage: Option<&steer::SessionUsage>) -> Option<String> {
    let cost = usage?.cost_usd?;
    if !cost.is_finite() || cost < 0.005 {
        return None;
    }
    Some(format!("${cost:.2}"))
}

/// The "Context" block of the session usage sheet: the heading, the
/// used/size line with the same meter the windows draw, and the cost when
/// there is one. `None` when the run has no usage to show.
pub(crate) fn render_context_block(
    usage: Option<&steer::SessionUsage>,
    cx: &App,
) -> Option<AnyElement> {
    let percent = context_percent(usage)?;
    let muted = cx.theme().muted_foreground;
    let mut block = v_flex()
        .w_full()
        .gap_2()
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child(SharedString::from(CONTEXT_SECTION_TITLE)),
        )
        .child(
            div()
                .text_xs()
                .child(SharedString::from(format_context_usage(usage))),
        )
        .child(
            div()
                .w_full()
                .h(px(TRACK_H))
                .rounded_full()
                .bg(theme::tokens::glass::STROKE_STRONG.to_hsla())
                .child(
                    div()
                        .h_full()
                        .rounded_full()
                        .w(gpui::relative(percent as f32 / 100.))
                        .bg(severity_color(severity(percent), cx)),
                ),
        );
    if let Some(cost) = format_usage_cost(usage) {
        block = block.child(
            div()
                .text_xs()
                .text_color(muted)
                .child(SharedString::from(cost)),
        );
    }
    Some(block.into_any_element())
}

// ---------------------------------------------------------------------------
// EXP-807 (EXP-792 C1-C4): the cross-device usage PAGE's model
// ---------------------------------------------------------------------------
//
// One row per device × agent PROFILE off the synced `devices` rows (mine +
// the servers shared with the team). Profiles (`agentAccounts[agent].profiles`,
// EXP-747 B5) carry their own usage; a device that reports none (an older
// build) falls back to the top-level account + `agentUsage[agent]` as the
// single `system` row, so the page never goes blank on a pre-profile machine.
//
// Mirrored with the web `agent-usage.ts` bottom section — same field names,
// same fallbacks, same ordering — so a rule changed on one side is greppable
// from the other. [`crate::usage_view`] is the only renderer.

/// The ambient login's profile id — the local constant the launcher already
/// uses, byte-identical with the web's `SYSTEM_PROFILE_ID`.
pub(crate) const SYSTEM_PROFILE_ID: &str = coding::SYSTEM_PROFILE;

/// A forced usage refresh (`agent_usage_refresh`) is refused while the last
/// fetch is younger than this: the device never hits the agent's usage
/// endpoint more often (its own [`coding::usage_cache::RATE_LIMITED_FLOOR_SECS`]),
/// so the button greys out and names the next allowed time instead of
/// queueing a no-op. The twin of the web's `RATE_LIMITED_FLOOR_MS`.
pub(crate) const RATE_LIMITED_FLOOR_SECS: i64 =
    coding::usage_cache::RATE_LIMITED_FLOOR_SECS as i64;

/// One rendered line of the usage page: a machine, an agent, and ONE of that
/// agent's account profiles on it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AgentProfileUsageRow {
    /// `<deviceId>:<agent>:<profileId>` — stable enough to key a list with,
    /// and the key the in-flight refresh map uses.
    pub key: String,
    pub device_id: String,
    pub device_label: String,
    /// Whether the row is one of the caller's OWN machines (a refresh is only
    /// ever queued on those).
    pub mine: bool,
    pub online: bool,
    pub agent: String,
    pub profile_id: String,
    /// The profile's label (`Default` for the system profile when the device
    /// sent none).
    pub profile_label: String,
    pub active: bool,
    pub signed_in: bool,
    pub email: Option<String>,
    pub plan: Option<String>,
    pub usage: Option<AgentUsage>,
    /// The "as of …" fallback when the usage is stale or absent.
    pub checked_at: Option<String>,
}

/// `Some(trimmed)` for a non-blank string — the wire uses "absent" and "empty
/// string" interchangeably (a `#[serde(default)]` String is `""`), and every
/// caption here treats both as nothing to say.
fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// The rows the usage page renders for `devices`, grouped by agent later.
///
/// `is_online` is injected rather than re-derived (the desktop's
/// `device_settings::row_is_online` needs a clock) so the derivation stays a
/// pure function of the rows — the web passes `deviceRowIsOnline` the same
/// way. Nothing is sorted here: [`sort_attention_first`] owns the order.
pub(crate) fn agent_profile_usage_rows(
    devices: &[domain::rows::DeviceRow],
    current_user_id: &str,
    is_online: impl Fn(Option<&str>) -> bool,
) -> Vec<AgentProfileUsageRow> {
    let mut out = Vec::new();
    for device in devices {
        let accounts = crate::device_settings::parse_agent_map::<coding::AgentAccount>(
            device.agent_accounts.as_ref(),
        );
        let usage_map: std::collections::BTreeMap<String, AgentUsage> = device
            .agent_usage
            .as_ref()
            .and_then(|value| value.as_object())
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|(agent, entry)| {
                        parse_agent_usage(entry).map(|usage| (agent.clone(), usage))
                    })
                    .collect()
            })
            .unwrap_or_default();
        // The union of "has an account" and "reported usage": a machine that
        // only managed one of the two still gets its row.
        let mut agents: std::collections::BTreeSet<&str> =
            accounts.keys().map(String::as_str).collect();
        agents.extend(usage_map.keys().map(String::as_str));

        let device_id = device.device_id.clone().unwrap_or_default();
        let device_label = device.label.clone().unwrap_or_default();
        let mine = device.user_id.as_deref() == Some(current_user_id);
        let online = is_online(device.last_seen_at.as_deref());
        for agent in agents {
            let account = accounts.get(agent);
            let base = |profile_id: &str| AgentProfileUsageRow {
                key: format!("{device_id}:{agent}:{profile_id}"),
                device_id: device_id.clone(),
                device_label: device_label.clone(),
                mine,
                online,
                agent: agent.to_string(),
                profile_id: profile_id.to_string(),
                profile_label: String::new(),
                active: false,
                signed_in: false,
                email: None,
                plan: None,
                usage: None,
                checked_at: None,
            };
            let profiles = account.map(|account| account.profiles.as_slice()).unwrap_or(&[]);
            if profiles.is_empty() {
                out.push(AgentProfileUsageRow {
                    profile_label: coding::agent_profiles::SYSTEM_LABEL.to_string(),
                    active: true,
                    signed_in: account.is_some_and(|account| account.signed_in),
                    email: account.and_then(|account| non_empty(account.email.as_deref())),
                    plan: account.and_then(|account| non_empty(account.plan.as_deref())),
                    usage: usage_map.get(agent).cloned(),
                    checked_at: account
                        .and_then(|account| non_empty(Some(&account.checked_at)))
                        .or_else(|| non_empty(device.agent_usage_at.as_deref())),
                    ..base(SYSTEM_PROFILE_ID)
                });
                continue;
            }
            for profile in profiles {
                // The active profile's numbers ride BOTH the profile entry and
                // the pre-profile `agentUsage[agent]` slot; prefer the
                // profile's own and fall back for a device that only
                // populated the old slot.
                let usage = profile.usage.clone().or_else(|| {
                    profile
                        .active
                        .then(|| usage_map.get(agent).cloned())
                        .flatten()
                });
                out.push(AgentProfileUsageRow {
                    profile_label: non_empty(profile.label.as_deref()).unwrap_or_else(|| {
                        if profile.id == SYSTEM_PROFILE_ID {
                            coding::agent_profiles::SYSTEM_LABEL.to_string()
                        } else {
                            profile.id.clone()
                        }
                    }),
                    active: profile.active,
                    signed_in: profile.signed_in,
                    email: non_empty(profile.email.as_deref()),
                    plan: non_empty(profile.plan.as_deref()),
                    usage,
                    checked_at: non_empty(Some(&profile.checked_at))
                        .or_else(|| account.and_then(|a| non_empty(Some(&a.checked_at)))),
                    ..base(&profile.id)
                });
            }
        }
    }
    out
}

/// The fullest window's percent, or 0 for a row with no usage at all.
pub(crate) fn peak_percent(usage: Option<&AgentUsage>) -> u8 {
    usage
        .map(|usage| {
            usage
                .windows
                .iter()
                .map(|window| window.percent)
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0)
}

/// Attention-first bucket: signed-out rows lead (there is something to do),
/// then rows at or over [`DANGER_PERCENT`], then everything else.
pub(crate) fn attention_rank(row: &AgentProfileUsageRow) -> u8 {
    if !row.signed_in {
        0
    } else if peak_percent(row.usage.as_ref()) >= DANGER_PERCENT {
        1
    } else {
        2
    }
}

/// [`attention_rank`] first, then the fuller row, then device label, agent and
/// profile — so a heartbeat can never shuffle two otherwise equal rows.
pub(crate) fn sort_attention_first(
    rows: Vec<AgentProfileUsageRow>,
) -> Vec<AgentProfileUsageRow> {
    let mut rows = rows;
    rows.sort_by(|a, b| {
        attention_rank(a)
            .cmp(&attention_rank(b))
            .then_with(|| peak_percent(b.usage.as_ref()).cmp(&peak_percent(a.usage.as_ref())))
            .then_with(|| a.device_label.cmp(&b.device_label))
            .then_with(|| a.agent.cmp(&b.agent))
            .then_with(|| a.profile_id.cmp(&b.profile_id))
    });
    rows
}

/// When a forced refresh is next allowed for `usage`, as an epoch SECOND.
/// `None` = right now (no fetch on record, an unreadable stamp, or a last
/// fetch older than the floor). A stamp in the future (the machine's clock
/// runs ahead) is treated as "just fetched".
pub(crate) fn refresh_allowed_at(usage: Option<&AgentUsage>, now_epoch: i64) -> Option<i64> {
    let usage = usage?;
    let fetched = crate::comments::parse_epoch(&usage.fetched_at)?;
    let next = fetched + RATE_LIMITED_FLOOR_SECS;
    (next > now_epoch).then_some(next)
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
    /// two caption forms. EXP-694: the weekly group carries NO title.
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
                // Untitled since EXP-694: its cards name themselves.
                ("weekly", ""),
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
    fn blocked_badge_names_the_wall_and_counts_down() {
        // ×4-locked strings (web `blockedBadgeLabel`). NOW = 2026-08-28T12:00Z.
        let now = crate::comments::parse_epoch("2026-08-28T12:00:00.000Z").unwrap();
        let blocked = |kind: &str, resets_at: Option<&str>| CodingSessionBlocked {
            kind: Some(kind.to_string()),
            agent: Some("claude".to_string()),
            window: Some("session".to_string()),
            resets_at: resets_at.map(str::to_string),
            since: Some("2026-08-28T11:30:00.000Z".to_string()),
        };
        assert_eq!(
            blocked_badge_label(
                Some(&blocked("rate_limit", Some("2026-08-28T14:00:00.000Z"))),
                now
            )
            .as_deref(),
            Some("Rate limited · resets in 2h")
        );
        // No reset time: the badge still names the wall.
        assert_eq!(
            blocked_badge_label(Some(&blocked("rate_limit", None)), now).as_deref(),
            Some("Rate limited")
        );
        assert_eq!(
            blocked_badge_label(Some(&blocked("rate_limit", Some("later"))), now).as_deref(),
            Some("Rate limited")
        );
        // A wall kind this build has no name for must not render silent.
        assert_eq!(
            blocked_badge_label(
                Some(&blocked("quota", Some("2026-08-28T14:00:00.000Z"))),
                now
            )
            .as_deref(),
            Some("Blocked · resets in 2h")
        );
        assert_eq!(blocked_badge_label(None, now), None);
    }

    #[test]
    fn parse_blocked_tolerates_garbage() {
        assert_eq!(parse_blocked(None), None);
        assert_eq!(parse_blocked(Some(&serde_json::json!(null))), None);
        assert_eq!(parse_blocked(Some(&serde_json::json!("nope"))), None);
        // A payload missing every field still parses — the badge falls back.
        assert_eq!(
            parse_blocked(Some(&serde_json::json!({}))),
            Some(CodingSessionBlocked::default())
        );
        let parsed = parse_blocked(Some(&serde_json::json!({
            "kind": "rate_limit",
            "agent": "claude",
            "window": "weekly",
            "resetsAt": "2026-08-28T14:00:00.000Z",
            "since": "2026-08-28T11:30:00.000Z",
        })))
        .unwrap();
        assert_eq!(parsed.window.as_deref(), Some("weekly"));
        assert_eq!(parsed.resets_at.as_deref(), Some("2026-08-28T14:00:00.000Z"));
    }

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

    // ── EXP-746: the per-session context meter (×4 test names) ────────────

    fn usage(used: i64, size: i64, cost: Option<f64>) -> steer::SessionUsage {
        steer::SessionUsage {
            context_used: used,
            context_size: size,
            cost_usd: cost,
        }
    }

    #[test]
    fn context_usage_reads_used_over_size_with_a_percent() {
        assert_eq!(
            format_context_usage(Some(&usage(124_000, 200_000, None))),
            "124k / 200k (62%)"
        );
        // Under a thousand prints verbatim; the percent floors.
        assert_eq!(
            format_context_usage(Some(&usage(999, 4_000, None))),
            "999 / 4k (24%)"
        );
        assert_eq!(context_percent(Some(&usage(1, 3, None))), Some(33));
        // The fixtures that catch ×4 arithmetic drift, one per trap:
        // an exact fraction (the percent must be `used * 100 / size`)...
        assert_eq!(
            format_context_usage(Some(&usage(116_000, 200_000, None))),
            "116k / 200k (58%)"
        );
        assert_eq!(context_percent(Some(&usage(116_000, 200_000, None))), Some(58));
        // ...and counts that are not round thousands: `k` ROUNDS, so this is
        // 125k and not 124k, and 1500 is 2k.
        assert_eq!(
            format_context_usage(Some(&usage(124_600, 200_000, None))),
            "125k / 200k (62%)"
        );
        assert_eq!(
            format_context_usage(Some(&usage(1_500, 200_000, None))),
            "2k / 200k (0%)"
        );
        // No window, no line — never a division by zero, never "0 / 0".
        assert_eq!(format_context_usage(Some(&usage(10, 0, None))), "");
        assert_eq!(format_context_usage(None), "");
        assert_eq!(context_percent(Some(&usage(10, 0, None))), None);
        // A report past its own window still reads as full, not 120%.
        assert_eq!(context_percent(Some(&usage(300, 200, None))), Some(100));
        assert_eq!(
            format_context_usage(Some(&usage(300, 200, None))),
            "300 / 200 (100%)"
        );
    }

    #[test]
    fn a_cost_under_half_a_cent_renders_nothing() {
        assert_eq!(
            format_usage_cost(Some(&usage(1, 2, Some(1.239)))),
            Some("$1.24".to_string())
        );
        assert_eq!(
            format_usage_cost(Some(&usage(1, 2, Some(0.005)))),
            Some("$0.01".to_string())
        );
        assert_eq!(format_usage_cost(Some(&usage(1, 2, Some(0.004)))), None);
        assert_eq!(format_usage_cost(Some(&usage(1, 2, Some(0.)))), None);
        assert_eq!(format_usage_cost(Some(&usage(1, 2, None))), None);
        assert_eq!(format_usage_cost(None), None);
    }

    #[test]
    fn the_context_heading_is_the_shared_wording() {
        assert_eq!(CONTEXT_SECTION_TITLE, "Context");
    }

    // ── EXP-807: the usage PAGE's model ───────────────────────────────────
    //
    // The web twin (`agentProfileUsageRows` & co) carries no tests of its own
    // yet, so these names are the ones a web mirror should take verbatim:
    // "rows fall back to the system profile", "rows read every profile",
    // "peak percent is the fullest window", "attention first leads with the
    // signed-out rows" and "a refresh inside the floor is refused".

    /// The synced wire, hydrated exactly the way the shape does it.
    fn device_row(value: serde_json::Value) -> domain::rows::DeviceRow {
        serde_json::from_value(value).expect("device row parses")
    }

    fn usage_json(fetched_at: &str, key: &str, percent: u8) -> serde_json::Value {
        serde_json::json!({
            "fetchedAt": fetched_at,
            "stale": false,
            "windows": [{ "key": key, "label": "Week", "percent": percent }],
        })
    }

    /// A machine that reports no PROFILES (an older build, or a single-login
    /// install) still gets exactly one row per agent: the ambient `system`
    /// profile, labelled "Default", carrying the top-level account and the
    /// pre-profile `agentUsage` slot. An agent that reported ONLY usage — no
    /// account at all — still gets its row, signed out, dated by the row's
    /// `agent_usage_at`.
    #[test]
    fn agent_profile_usage_rows_fall_back_to_the_system_profile() {
        let row = device_row(serde_json::json!({
            "id": "row-1",
            "device_id": "dev-1",
            "label": "Studio",
            "user_id": "me",
            "last_seen_at": "2026-08-28T11:59:00.000Z",
            "agent_accounts": {
                "claude": {
                    "signedIn": true,
                    "email": "dev@acme.test",
                    "plan": "max",
                    "checkedAt": "2026-08-28T11:00:00.000Z",
                },
            },
            "agent_usage": {
                "claude": usage_json("2026-08-28T11:55:00.000Z", "session", 42),
                "codex": usage_json("2026-08-28T11:55:00.000Z", "weekly", 8),
            },
            "agent_usage_at": "2026-08-28T11:30:00.000Z",
        }));
        let rows = sort_attention_first(agent_profile_usage_rows(&[row], "me", |_| true));

        // Signed-out rows lead: codex reported numbers but no account.
        assert_eq!(
            rows.iter().map(|row| row.key.as_str()).collect::<Vec<_>>(),
            vec!["dev-1:codex:system", "dev-1:claude:system"]
        );
        let claude = &rows[1];
        assert_eq!(claude.agent, "claude");
        assert_eq!(claude.profile_id, SYSTEM_PROFILE_ID);
        assert_eq!(claude.profile_label, "Default");
        assert!(claude.active, "the ambient login is always the active one");
        assert!(claude.mine);
        assert!(claude.online);
        assert!(claude.signed_in);
        assert_eq!(claude.device_label, "Studio");
        assert_eq!(claude.email.as_deref(), Some("dev@acme.test"));
        assert_eq!(claude.plan.as_deref(), Some("max"));
        assert_eq!(peak_percent(claude.usage.as_ref()), 42);
        // The account's own probe stamp wins over the row's usage stamp.
        assert_eq!(claude.checked_at.as_deref(), Some("2026-08-28T11:00:00.000Z"));

        let codex = &rows[0];
        assert!(!codex.signed_in);
        assert_eq!(codex.email, None);
        assert_eq!(peak_percent(codex.usage.as_ref()), 8);
        // No account to date it: the device's `agent_usage_at` is the fallback.
        assert_eq!(codex.checked_at.as_deref(), Some("2026-08-28T11:30:00.000Z"));

        // A teammate's shared machine is never "mine", and the online-ness is
        // the caller's to decide.
        let theirs = device_row(serde_json::json!({
            "id": "row-2",
            "device_id": "dev-2",
            "label": "Server",
            "user_id": "someone-else",
            "shared_team_id": "team-1",
            "agent_accounts": { "claude": { "signedIn": true, "checkedAt": "" } },
        }));
        let rows = agent_profile_usage_rows(&[theirs], "me", |_| false);
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].mine);
        assert!(!rows[0].online);
        // An empty `checkedAt` is nothing to say, never an "as of " with no date.
        assert_eq!(rows[0].checked_at, None);

        // A machine that reported nothing at all contributes no rows.
        let quiet = device_row(serde_json::json!({ "id": "row-3", "device_id": "dev-3" }));
        assert!(agent_profile_usage_rows(&[quiet], "me", |_| true).is_empty());
    }

    /// EXP-747 B5: with profiles the page renders ONE row each — the profile's
    /// own label/identity/usage — and only the ACTIVE profile falls back to the
    /// pre-profile `agentUsage` slot (those numbers are its, not the others').
    #[test]
    fn agent_profile_usage_rows_read_every_profile() {
        let row = device_row(serde_json::json!({
            "id": "row-1",
            "device_id": "dev-1",
            "label": "Studio",
            "user_id": "me",
            "agent_accounts": {
                "claude": {
                    "signedIn": true,
                    "email": "work@acme.test",
                    "checkedAt": "2026-08-28T11:00:00.000Z",
                    "profiles": [
                        {
                            "id": "system",
                            "signedIn": true,
                            "email": "work@acme.test",
                            "active": true,
                            "checkedAt": "2026-08-28T11:00:00.000Z",
                        },
                        {
                            "id": "personal",
                            "label": "Personal",
                            "signedIn": false,
                            "active": false,
                            "checkedAt": "",
                            "usage": usage_json("2026-08-28T11:50:00.000Z", "weekly", 96),
                        },
                    ],
                },
            },
            "agent_usage": { "claude": usage_json("2026-08-28T11:55:00.000Z", "session", 42) },
        }));
        let rows = agent_profile_usage_rows(&[row], "me", |_| true);
        assert_eq!(
            rows.iter().map(|row| row.key.as_str()).collect::<Vec<_>>(),
            vec!["dev-1:claude:system", "dev-1:claude:personal"]
        );
        let system = &rows[0];
        // No label on the wire: the system profile is "Default" everywhere.
        assert_eq!(system.profile_label, "Default");
        assert!(system.active);
        assert!(system.signed_in);
        // The active profile inherits the pre-profile slot.
        assert_eq!(peak_percent(system.usage.as_ref()), 42);

        let personal = &rows[1];
        assert_eq!(personal.profile_label, "Personal");
        assert!(!personal.active);
        assert!(!personal.signed_in);
        // Its OWN numbers — never the active profile's.
        assert_eq!(peak_percent(personal.usage.as_ref()), 96);
        // A blank profile stamp degrades to the account's.
        assert_eq!(
            personal.checked_at.as_deref(),
            Some("2026-08-28T11:00:00.000Z")
        );
    }

    /// A row's attention weight is the FULLEST window, not the first or the
    /// last one; nothing reported is 0 rather than a missing value.
    #[test]
    fn peak_percent_is_the_fullest_window() {
        let now = 1_756_000_000_i64;
        let usage = AgentUsage {
            fetched_at: chrono::DateTime::from_timestamp(now, 0).unwrap().to_rfc3339(),
            stale: false,
            windows: vec![
                window("session", "5h", 12, None),
                window("weekly", "Week", 96, None),
                window("model:fable", "Fable", 40, None),
            ],
        };
        assert_eq!(peak_percent(Some(&usage)), 96);
        assert_eq!(peak_percent(Some(&AgentUsage::default())), 0);
        assert_eq!(peak_percent(None), 0);
    }

    /// The page's order: signed-out rows first (there is something to DO),
    /// then anything at or over the danger threshold, then the rest — the
    /// fuller row ahead inside a bucket, and label/agent/profile after that so
    /// a heartbeat cannot reshuffle equal rows.
    #[test]
    fn attention_first_leads_with_the_signed_out_rows() {
        let row = |device: &str, agent: &str, profile: &str, signed_in: bool, percent: u8| {
            AgentProfileUsageRow {
                key: format!("{device}:{agent}:{profile}"),
                device_id: device.to_string(),
                device_label: device.to_string(),
                mine: true,
                online: true,
                agent: agent.to_string(),
                profile_id: profile.to_string(),
                profile_label: profile.to_string(),
                active: true,
                signed_in,
                email: None,
                plan: None,
                usage: Some(AgentUsage {
                    fetched_at: String::new(),
                    stale: false,
                    windows: vec![window("weekly", "Week", percent, None)],
                }),
                checked_at: None,
            }
        };
        let rows = sort_attention_first(vec![
            row("Studio", "claude", "system", true, 10),
            row("Studio", "codex", "system", true, DANGER_PERCENT),
            row("Air", "claude", "system", false, 100),
            row("Air", "claude", "personal", true, 60),
            // Same bucket AND the same fill: label, then agent, then profile.
            row("Air", "codex", "system", true, 10),
        ]);
        assert_eq!(
            rows.iter().map(|row| row.key.as_str()).collect::<Vec<_>>(),
            vec![
                // Signed out leads even at 100% — it is the actionable one.
                "Air:claude:system",
                // Then the danger bucket.
                "Studio:codex:system",
                // Then the rest, fullest first, ties by label/agent/profile.
                "Air:claude:personal",
                "Air:codex:system",
                "Studio:claude:system",
            ]
        );
        // The buckets themselves, spelled out.
        assert_eq!(attention_rank(&row("Air", "claude", "system", false, 0)), 0);
        assert_eq!(
            attention_rank(&row("Air", "claude", "system", true, DANGER_PERCENT)),
            1
        );
        assert_eq!(
            attention_rank(&row("Air", "claude", "system", true, DANGER_PERCENT - 1)),
            2
        );
    }

    /// The button greys out for the device's OWN 429 floor (5 minutes) and
    /// names the moment it comes back; anything older, or undated, refreshes
    /// right now.
    #[test]
    fn a_refresh_inside_the_floor_is_refused() {
        let now = 1_756_000_000_i64;
        let at = |offset: i64| {
            chrono::DateTime::from_timestamp(now + offset, 0)
                .unwrap()
                .to_rfc3339()
        };
        let usage = |fetched_at: String| AgentUsage {
            fetched_at,
            stale: false,
            windows: Vec::new(),
        };
        assert_eq!(RATE_LIMITED_FLOOR_SECS, 300);
        // Fetched a minute ago: refused until the floor runs out.
        assert_eq!(
            refresh_allowed_at(Some(&usage(at(-60))), now),
            Some(now - 60 + RATE_LIMITED_FLOOR_SECS)
        );
        // Exactly on the floor, and past it: allowed now.
        assert_eq!(
            refresh_allowed_at(Some(&usage(at(-RATE_LIMITED_FLOOR_SECS))), now),
            None
        );
        assert_eq!(refresh_allowed_at(Some(&usage(at(-3_600))), now), None);
        // A machine whose clock runs ahead counts as "just fetched".
        assert_eq!(
            refresh_allowed_at(Some(&usage(at(120))), now),
            Some(now + 120 + RATE_LIMITED_FLOOR_SECS)
        );
        // No fetch on record, an unreadable stamp, or no usage at all.
        assert_eq!(refresh_allowed_at(Some(&usage(String::new())), now), None);
        assert_eq!(refresh_allowed_at(Some(&usage("garbage".into())), now), None);
        assert_eq!(refresh_allowed_at(None, now), None);
    }
}
