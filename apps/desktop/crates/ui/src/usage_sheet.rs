//! EXP-863/EXP-877/EXP-909/EXP-1051 — the per-run usage sheet (this run's
//! context window, the run's account, its rate-limit windows and the other
//! accounts on the host) that the transcript composer's context ring opens.
//! Moved out of `session_screen` so the viewer's footer can host it; the SAME
//! layout the web popover, the iOS sheet and the Android sheet build, section
//! for section.

use std::rc::Rc;

use gpui::{
    div, px, relative, size, AnyElement, App, AppContext as _, Hsla, InteractiveElement as _,
    IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled,
    Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    progress::ProgressCircle,
    text::TextView,
    v_flex, ActiveTheme as _, Icon, Sizable as _,
};

use crate::controls::{WebControl as _, WebText as _};
use crate::icons::registry;
use crate::native_dialog::{self, DialogContent, DialogSpec};

/// EXP-863/EXP-909/EXP-1051 — the usage sheet, the SAME layout on all four
/// clients:
///
/// 0. "Context window" — THIS run's own window, and the only place it is more
///    than one number: the headline (`65k / 200k (32%)`), the run's cost and
///    the segmented bar, with the LAYOUT legend one chevron away (one row per
///    layer, `Free` last). It LEADS the sheet because it is the run's own
///    number, and EXP-746's rule still holds — a token count has no percent
///    window of its own, so it never folds into the machine's windows below;
/// 1. the run's ACCOUNT, once: the agent's brand mark, the login's caption,
///    and either its health badge or its plan (muted) — never both, never a
///    second time further down;
/// 2. that account's rate-limit windows, two lines each, dimming to an
///    `as of …` caption once they are no longer current;
/// 3. "Accounts" — ONLY the OTHER logins on the host, each an identity line
///    with an icon-only switch and a mini usage line.
///
/// Sections are separated by hairlines and no sentence appears twice. The
/// windows and the context window are two different quantities (the machine's
/// rate limits, up to a heartbeat stale, vs. this conversation's tokens on
/// the wire), which is why they stay two sections and not one merged list.
///
/// EXP-1051 dropped the footer note: a row's refusal and the switch's one-time
/// cost both ride the switch control's OWN tooltip now, where the control is.
///
/// `windows` is the FALLBACK report — the caller's best guess for the agent
/// on that machine. The run's own login wins whenever its row carries
/// numbers: those are the limits this run actually spends (EXP-875 §2).
///
/// `layout` is the device's `context_layout` state (empty until it publishes
/// one), `legend_open` the viewer's own disclosure state and `on_toggle` the
/// flip — the sheet is a pure render, its state lives on the view that hosts
/// the popover.
pub(crate) fn render_usage_sheet(
    agent: Option<coding::CodingAgent>,
    usage: Option<&steer::SessionUsage>,
    layout: &[steer::ContextSegment],
    legend_open: bool,
    on_toggle: Rc<dyn Fn(&mut Window, &mut App)>,
    windows: Option<&coding::agent_usage::AgentUsage>,
    switch: Option<crate::account_switch::SwitchContext>,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let now_epoch = chrono::Utc::now().timestamp();
    let resolved = switch.as_ref().and_then(|switch| switch.resolve(cx));
    let targets = resolved.as_deref().unwrap_or(&[]);
    let current = targets.iter().find(|target| target.current);
    let mut sections: Vec<AnyElement> = Vec::new();

    // 0. This run's context window — the bar, and the layout behind it.
    sections.extend(render_context_window(
        usage,
        layout,
        legend_open,
        on_toggle,
        cx,
    ));

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

    // 3. The OTHER accounts.
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

// ---------------------------------------------------------------------------
// EXP-1051 — the context window block (section 0)
// ---------------------------------------------------------------------------

/// The read-only playbook viewer's title and blurb — the web's words
/// (`context-window-block.tsx`), so a person reading the same row on two
/// clients opens the same thing under the same name.
const PLAYBOOK_DIALOG_TITLE: &str = "Run playbook";
const PLAYBOOK_DIALOG_BLURB: &str = "Appended to every coding run's system prompt.";

/// The legend swatch's side, and the bar's height.
const SWATCH: f32 = 10.;
const BAR_H: f32 = 4.;

/// A segment tone → its fill. The hues are the AVATAR palette's (the web's
/// `--avatar-*`), so the bar shares the app's ONE set of identity colours
/// instead of inventing a seventh palette; `neutral` is the glass foreground
/// every meter fills with, and `track` is the bar's own background so a `Free`
/// swatch matches the part of the track nothing painted. An unknown tone reads
/// as `neutral` — an older client draws a layer it cannot colour rather than
/// dropping it out of the geometry.
fn tone_color(tone: &str, cx: &App) -> Hsla {
    use theme::tokens::avatar;
    match tone {
        "orange" => avatar::ORANGE.to_hsla(),
        "yellow" => avatar::YELLOW.to_hsla(),
        "green" => avatar::GREEN.to_hsla(),
        "teal" => avatar::TEAL.to_hsla(),
        "blue" => avatar::BLUE.to_hsla(),
        "violet" => avatar::VIOLET.to_hsla(),
        "pink" => avatar::PINK.to_hsla(),
        "red" => avatar::RED.to_hsla(),
        "track" => theme::tokens::glass::STROKE_STRONG.to_hsla(),
        _ => cx.theme().foreground.opacity(0.30),
    }
}

/// EXP-1051 — the sheet's first section: `Context window` + the headline + the
/// cost on one clickable row, the stacked bar under it, and (once open) the
/// legend. `None` when the run has published no window at all — there is no
/// scale to draw on, and a lone title would be the only thing left.
fn render_context_window(
    usage: Option<&steer::SessionUsage>,
    layout: &[steer::ContextSegment],
    legend_open: bool,
    on_toggle: Rc<dyn Fn(&mut Window, &mut App)>,
    cx: &App,
) -> Option<AnyElement> {
    let view = crate::context_layout::context_window_view(usage, Some(layout))?;
    let muted = cx.theme().muted_foreground;
    let cost = crate::usage_bar::format_usage_cost(usage);

    // The fold row. The chevron TRAILS the numbers: the headline is what the
    // eye goes to, the indicator parks after it (web `chevron="trailing"`).
    let header = h_flex()
        .id("context-window-toggle")
        .w_full()
        .min_w_0()
        .items_center()
        .gap_2()
        .cursor_pointer()
        .child(crate::usage_bar::sheet_section_title(
            domain::contract::CONTEXT_LAYOUT_TITLE,
            cx,
        ))
        .child(div().flex_1().min_w_0())
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .child(SharedString::from(view.headline.clone())),
        )
        .children(cost.map(|cost| {
            div()
                .flex_shrink_0()
                .text_2xs()
                .text_color(muted)
                .child(SharedString::from(cost))
        }))
        .child(
            Icon::new(if legend_open {
                registry::UI_CHEVRON_DOWN
            } else {
                registry::UI_CHEVRON_RIGHT
            })
            .xsmall()
            .text_color(muted),
        )
        .on_click(move |_, window, cx| on_toggle(window, cx));

    // The bar. The slices are NEVER scaled to fit: when the device's estimates
    // overshoot the measured total the track CLIPS (`overflow_hidden` over
    // non-shrinking children) rather than rescaling every layer to hide it.
    let mut track = h_flex()
        .relative()
        .w_full()
        .h(px(BAR_H))
        .rounded_full()
        .overflow_hidden()
        .bg(theme::tokens::glass::STROKE_STRONG.to_hsla());
    for slice in &view.bar {
        track = track.child(
            div()
                .h_full()
                .flex_shrink_0()
                .w(relative(slice.percent as f32 / 100.))
                .bg(tone_color(slice.tone, cx)),
        );
    }
    // The three marks the reader measures against: the compaction floor, then
    // the two usage thresholds.
    for tick in view.ticks {
        track = track.child(
            div()
                .absolute()
                .left(relative(f32::from(tick) / 100.))
                .w(px(1.))
                .h_full()
                .bg(cx.theme().background.opacity(0.8)),
        );
    }

    let mut block = v_flex()
        .w_full()
        .min_w_0()
        .gap_1p5()
        .child(header)
        .child(track);

    if legend_open {
        let mut rows = v_flex().w_full().min_w_0();
        for (index, row) in view.legend.iter().enumerate() {
            rows = rows.child(render_legend_row(index, row, cx));
        }
        block = block.child(rows);
    }
    Some(block.into_any_element())
}

/// One legend row: swatch · label · `≈`-prefixed tokens · share. Two of them
/// ACT rather than report — the playbook every run is handed (opened
/// read-only, rather than "somewhere in the repo") and the team prompt, the
/// one layer in here a person can shorten, which is Settings → General.
fn render_legend_row(index: usize, row: &crate::context_layout::ContextLegendRow, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let line = h_flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap_2()
        .text_xs()
        .child(
            div()
                .flex_shrink_0()
                .size(px(SWATCH))
                .rounded(px(3.))
                .bg(tone_color(row.tone, cx)),
        )
        .child(div().min_w_0().truncate().child(row.label))
        .child(div().flex_1().min_w_0())
        .child(
            div()
                .flex_shrink_0()
                .text_color(muted)
                .child(SharedString::from(format!(
                    "{}{}",
                    if row.estimated { "≈" } else { "" },
                    row.tokens
                ))),
        )
        .child(
            div()
                .flex_shrink_0()
                .w(px(44.))
                .text_right()
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(SharedString::from(row.percent.clone())),
        );
    // What the layer is MADE of, when the device named it — under the label,
    // never beside it: the 320px sheet has no room for a second column.
    let body = v_flex()
        .w_full()
        .min_w_0()
        .gap_0p5()
        .child(line)
        .children(row.detail.clone().map(|detail| {
            div()
                .w_full()
                .min_w_0()
                .truncate()
                .pl(px(SWATCH + 8.))
                .text_2xs()
                .text_color(muted.opacity(0.7))
                .child(SharedString::from(detail))
        }));
    let shell = div()
        .id(("context-legend-row", index))
        .w_full()
        .min_w_0()
        .px_1()
        .py_0p5()
        .rounded_sm();
    match row.key {
        "playbook" => shell
            .cursor_pointer()
            .hover(|this| this.bg(theme::tokens::glass::FILL_ACTIVE.to_hsla()))
            .on_click(|_, window, cx| open_playbook(window, cx))
            .child(body)
            .into_any_element(),
        "team" => shell
            .cursor_pointer()
            .hover(|this| this.bg(theme::tokens::glass::FILL_ACTIVE.to_hsla()))
            .on_click(|_, window, cx| {
                crate::navigation::navigate(window, cx, crate::navigation::Screen::Settings);
                crate::sidebar::select_settings_section(
                    window,
                    cx,
                    crate::settings::SettingsSection::General,
                );
            })
            .child(body)
            .into_any_element(),
        _ => shell.child(body).into_any_element(),
    }
}

/// The playbook, read-only: the 6 KiB `coding::skill::RUN_SKILL` every run's
/// system prompt is appended with, in the same GFM renderer the issue
/// description and What's new use.
fn open_playbook(window: &mut Window, cx: &mut App) {
    let spec = DialogSpec::new(PLAYBOOK_DIALOG_TITLE, size(px(640.), px(560.)))
        .resizable(size(px(360.), px(240.)));
    native_dialog::open_dialog_window(window, cx, spec, move |_window, cx| {
        DialogContent::new(cx.new(|_| PlaybookView))
    });
}

/// The playbook dialog's body — a pure read-only render of the shipped
/// playbook.
struct PlaybookView;

impl Render for PlaybookView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(PLAYBOOK_DIALOG_BLURB),
            )
            .child(
                div().flex_1().min_h_0().text_sm().child(
                    TextView::markdown("run-playbook-body", SharedString::from(coding::skill::RUN_SKILL))
                        .style(crate::surface::markdown_style())
                        .selectable(true)
                        .scrollable(true),
                ),
            )
    }
}
