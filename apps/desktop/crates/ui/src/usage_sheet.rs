//! EXP-863/EXP-877 — the per-run usage sheet (context window, the active
//! account's rate-limit windows, the other accounts on the host) that the
//! transcript composer's context ring opens. Moved out of `session_screen`
//! so the viewer's footer can host it; the SAME structure the web popover
//! builds.

use gpui::{div, px, AnyElement, App, IntoElement, ParentElement, SharedString, Styled};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use crate::controls::WebText as _;

/// EXP-863 — the usage sheet, the SAME structure the web popover builds:
///
/// 1. the ACTIVE account's caption once (the only place it appears);
/// 2. "Context" — the run's live window, cost right-aligned, then the meter;
/// 3. the active account's rate-limit windows (the three cards as today);
/// 4. "Accounts" — ONLY the other accounts on the host, each with Switch,
///    dense cards and an account-level refusal; omitted with no other one;
/// 5. ONE footer note: the global blocker, else the one-time cost.
///
/// Sections are separated by hairlines; no sentence appears twice. The
/// context block and the windows are two different quantities (this run's
/// tokens on the wire vs. the machine's rate limits, up to a heartbeat
/// stale), which is why they stay two sections and not one merged list.
pub(crate) fn render_usage_sheet(
    agent: Option<coding::CodingAgent>,
    usage: Option<&steer::SessionUsage>,
    windows: Option<&coding::agent_usage::AgentUsage>,
    switch: Option<crate::account_switch::SwitchContext>,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let resolved = switch
        .as_ref()
        .and_then(|switch| switch.resolve(cx));
    let (targets, blocker) = match resolved.as_ref() {
        Some((targets, blocker)) => (targets.as_slice(), blocker.as_ref()),
        None => (&[][..], None),
    };
    let mut sections: Vec<AnyElement> = Vec::new();

    // 1. The active account, once.
    if let Some(current) = targets.iter().find(|target| target.current) {
        sections.push(
            h_flex()
                .w_full()
                .min_w_0()
                .items_center()
                .gap_1p5()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_xs()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(SharedString::from(current.caption.clone())),
                )
                .children(crate::usage_bar::health_badge(current.health, cx))
                .into_any_element(),
        );
    }

    // 2. Context.
    let context = crate::usage_bar::render_context_block(usage, cx);
    let has_context = context.is_some();
    sections.extend(context);

    // 3. The active account's windows.
    let has_windows = windows.is_some_and(|windows| !windows.windows.is_empty());
    if let (Some(agent), Some(windows)) = (agent, windows.filter(|_| has_windows)) {
        sections.push(crate::usage_bar::render_usage_cards(
            agent,
            windows,
            chrono::Utc::now().timestamp(),
            true,
            cx,
        ));
    }
    if !has_context && !has_windows {
        // EXP-862: a live run's login IS signed in, so windows that have not
        // been read yet are "Checking…" (the ×4 `usage_caption` rule), never
        // a machine that looks broken.
        let caption = crate::usage_bar::usage_caption(crate::usage_bar::UsageState::Checking, None)
            .unwrap_or_default();
        sections.push(div().text_xs().text_color(muted).child(caption).into_any_element());
    }

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
