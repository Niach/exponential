//! EXP-863/EXP-877/EXP-909 — the per-run usage sheet (the run's account, its
//! rate-limit windows, this run's context window and the other accounts on
//! the host) that the transcript composer's context ring opens. Moved out of
//! `session_screen` so the viewer's footer can host it; the SAME layout the
//! web popover, the iOS sheet and the Android sheet build, section for
//! section.

use gpui::{div, px, AnyElement, App, IntoElement, ParentElement, SharedString, Styled};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, progress::ProgressCircle, v_flex, ActiveTheme as _, Sizable as _,
};

use crate::controls::{WebControl as _, WebText as _};

/// EXP-863/EXP-909 — the usage sheet, the SAME layout on all four clients:
///
/// 1. the run's ACCOUNT, once: the agent's brand mark, the login's caption,
///    and either its health badge or its plan (muted) — never both, never a
///    second time further down;
/// 2. that account's rate-limit windows, two lines each, dimming to an
///    `as of …` caption once they are no longer current;
/// 3. "Context" — this run's live token window, one line plus the meter;
/// 4. "Accounts" — ONLY the OTHER logins on the host, each an identity line
///    with an icon-only switch, a mini usage line and its own refusal;
/// 5. ONE footer note: the global blocker, else the one-time switch cost.
///
/// Sections are separated by hairlines and no sentence appears twice. The
/// windows and the context block are two different quantities (the machine's
/// rate limits, up to a heartbeat stale, vs. this conversation's tokens on
/// the wire), which is why they stay two sections and not one merged list.
///
/// `windows` is the FALLBACK report — the caller's best guess for the agent
/// on that machine. The run's own login wins whenever its row carries
/// numbers: those are the limits this run actually spends (EXP-875 §2).
pub(crate) fn render_usage_sheet(
    agent: Option<coding::CodingAgent>,
    usage: Option<&steer::SessionUsage>,
    windows: Option<&coding::agent_usage::AgentUsage>,
    switch: Option<crate::account_switch::SwitchContext>,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let now_epoch = chrono::Utc::now().timestamp();
    let resolved = switch
        .as_ref()
        .and_then(|switch| switch.resolve(cx));
    let (targets, blocker) = match resolved.as_ref() {
        Some((targets, blocker)) => (targets.as_slice(), blocker.as_ref()),
        None => (&[][..], None),
    };
    let current = targets.iter().find(|target| target.current);
    let mut sections: Vec<AnyElement> = Vec::new();

    // 1. The run's account, once: brand mark · caption · badge-or-plan.
    if let Some(current) = current {
        let badge = crate::usage_bar::health_badge(current.health, cx);
        // The plan says itself only BEHIND an email — a caption that already
        // IS the plan must not print it twice — and never beside a badge:
        // a broken login's plan is not the thing to read first.
        let plan = current
            .plan
            .clone()
            .filter(|plan| badge.is_none() && plan != &current.caption);
        sections.push(
            h_flex()
                .w_full()
                .min_w_0()
                .items_center()
                .gap_1p5()
                .children(agent.map(|agent| {
                    crate::coding_selects::agent_mark(agent)
                        .with_size(px(crate::surface::PillSize::Sm.glyph()))
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_xs()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(SharedString::from(current.caption.clone())),
                )
                .children(badge)
                .children(plan.map(|plan| {
                    div()
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(muted)
                        .child(SharedString::from(plan))
                }))
                .into_any_element(),
        );
    }

    // 2. That account's windows — its OWN row's numbers, the caller's
    //    fallback only when the row carries none (a machine that reported
    //    just the top-level map, or a device row that has not synced).
    let run_windows = current
        .and_then(|target| target.usage.as_ref())
        .filter(|usage| !usage.windows.is_empty())
        .or_else(|| windows.filter(|usage| !usage.windows.is_empty()));
    match run_windows {
        Some(run_windows) => sections.push(crate::usage_bar::render_usage_windows(
            run_windows,
            now_epoch,
            cx,
        )),
        None => {
            // EXP-862: a live run's login IS signed in, so windows nothing has
            // read yet are "Checking…" (the ×4 `usage_caption` rule) IN PLACE
            // of the block — never a machine that looks broken, and never a
            // silently missing section.
            let caption =
                crate::usage_bar::usage_caption(crate::usage_bar::UsageState::Checking, None)
                    .unwrap_or_default();
            sections.push(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(caption)
                    .into_any_element(),
            );
        }
    }

    // 3. Context.
    sections.extend(crate::usage_bar::render_context_block(usage, cx));

    // 4. The OTHER accounts.
    if let Some(rows) = switch
        .as_ref()
        .and_then(|switch| switch.render_account_rows(targets, cx))
    {
        sections.push(
            v_flex()
                .w_full()
                .min_w_0()
                .gap_2()
                .child(crate::usage_bar::sheet_section_title(
                    crate::account_switch::SECTION_TITLE,
                    cx,
                ))
                .child(rows)
                .into_any_element(),
        );
    }

    // 5. One footer note.
    if let Some(note) = crate::account_switch::footer_note(targets, blocker) {
        sections.push(
            div()
                .text_2xs()
                .text_color(muted.opacity(0.8))
                .child(note)
                .into_any_element(),
        );
    }

    let mut sheet = v_flex().w(px(320.)).min_w_0();
    for (index, section) in sections.into_iter().enumerate() {
        let mut slot = div().w_full().min_w_0().py_2();
        if index > 0 {
            slot = slot
                .border_t_1()
                .border_color(theme::tokens::glass::STROKE_ROW.to_hsla());
        }
        sheet = sheet.child(slot.child(section));
    }
    sheet.into_any_element()
}

/// EXP-877 — the composer footer's CONTEXT RING: a 16px radial meter of how
/// full the run's context window is, in the usual severity tone (muted, amber
/// from 75 %, destructive from 95 % — [`crate::usage_bar::severity`]). It is
/// the trigger for [`render_usage_sheet`], so it is a `Button`, and it says
/// the numbers in its tooltip rather than beside itself: a percentage printed
/// next to a meter of the same percentage is the same fact twice.
///
/// A run with no window to report renders NO ring at all — that is the
/// caller's check ([`crate::usage_bar::context_percent`] is `None`).
pub(crate) fn context_ring(
    id: impl Into<gpui::ElementId>,
    percent: u8,
    usage: Option<&steer::SessionUsage>,
    cx: &App,
) -> Button {
    let tone = crate::usage_bar::severity_color(crate::usage_bar::severity(percent), cx);
    Button::new(id)
        .ghost()
        .web_icon_xs()
        .icon(
            // `Size::Size(s)` renders at `s * 0.75`, so the ring is asked for
            // the size that lands ON 16px.
            gpui_component::Sizable::with_size(
                ProgressCircle::new("session-context-ring")
                    .value(f32::from(percent))
                    .color(tone),
                px(16. / 0.75),
            ),
        )
        .tooltip(crate::usage_bar::format_context_usage(usage))
}

/// EXP-909 — the windows of ONE login on a machine, off that machine's
/// reported accounts: the profile's own `usage` row, falling back to the
/// top-level `agentUsage[agent]` map only when the resolved profile is the
/// machine's ACTIVE login or no profile could be resolved at all (the device
/// only ever puts the active login's numbers there).
///
/// `account` is the run's account. Resolution is the ×4 rule (§1): the named
/// profile, else the login whose email the machine reports at the top level,
/// else the active one — never `system` by default.
pub(crate) fn account_windows(
    accounts: &coding::agent_accounts::AgentAccounts,
    usage: &coding::agent_usage::AgentUsageMap,
    agent: coding::CodingAgent,
    account: Option<&str>,
) -> Option<coding::agent_usage::AgentUsage> {
    let ambient = || usage.get(agent.id()).cloned();
    let Some(entry) = accounts.get(agent.id()) else {
        return ambient();
    };
    let named = account
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .and_then(|id| entry.profiles.iter().find(|profile| profile.id == id));
    let by_email = || {
        let email = entry.email.as_deref().map(str::trim).filter(|e| !e.is_empty())?;
        entry
            .profiles
            .iter()
            .find(|profile| profile.email.as_deref().map(str::trim) == Some(email))
    };
    let Some(profile) = named
        .or_else(by_email)
        .or_else(|| entry.profiles.iter().find(|profile| profile.active))
    else {
        return ambient();
    };
    profile
        .usage
        .clone()
        .or_else(|| profile.active.then(ambient).flatten())
}

/// Another machine's per-agent windows, off the synced `devices` row — the
/// same jsonb every client reads (the device reports it on heartbeat).
pub(crate) fn device_usage(
    device_id: Option<&str>,
    agent: coding::CodingAgent,
    cx: &App,
) -> Option<coding::agent_usage::AgentUsage> {
    let device_id = device_id?;
    let store = sync::Store::try_global(cx)?;
    let devices = store.collections().devices.read(cx);
    let row = devices
        .iter()
        .find(|row| row.device_id.as_deref() == Some(device_id))?;
    crate::device_settings::parse_agent_map::<coding::agent_usage::AgentUsage>(
        row.agent_usage.as_ref(),
    )
    .remove(agent.id())
}
