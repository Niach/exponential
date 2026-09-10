//! The Usage center screen (EXP-807): the desktop twin of the web
//! `t/$teamSlug/usage` page (`components/agent-usage-page.tsx`, the spec this
//! file mirrors) — every machine's agent usage on ONE page instead of one
//! device dialog at a time.
//!
//! EXP-817: one card per ACCOUNT (an agent plus the login the machines
//! named) off the synced `devices` rows (own machines plus the servers
//! teammates shared with the active team), grouped by agent in contract
//! order, ATTENTION FIRST: signed-out cards lead, then anything at or over
//! the danger threshold, then the rest. The machines holding the account are
//! chips on the card; the numbers are the FRESHEST machine's report (they
//! are the account's limits, so every machine reads the same ones). The
//! windows are [`crate::usage_bar::render_usage_cards_dense`], laid out as a
//! wrapping grid so every account fits on one screen; stale numbers keep the
//! dimmed "as of …" treatment.
//!
//! The page owns no model of its own: rows, groups, ordering and the refresh
//! floor live in [`crate::usage_bar`] beside the ×4 card rules (the twin of
//! the web `agent-usage.ts` page section), so this file is layout, the
//! refresh round-trip and nothing else.
//!
//! "Refresh" queues an `agent_usage_refresh` command on one of MY machines
//! that runs it (cap `agent-usage-refresh`), never more often than the
//! device's own rate-limit floor — the button greys out and names the next
//! allowed time instead of queueing a no-op. EXP-817: while the page is open
//! it does that BY ITSELF for every account whose freshest report is past
//! the floor, so the numbers on screen are never older than ~5 minutes on a
//! machine that answers.

use std::collections::HashMap;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, ClickEvent, Entity, InteractiveElement as _, IntoElement, ParentElement, Render,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Disableable as _, Icon};

use coding::CodingAgent;

use crate::actions_view::page_scaffold;
use crate::icons::registry;
use crate::navigation::{active_team_id, nav_for_window, navigate, Navigation, Screen};
use crate::queries;
use crate::usage_bar::{
    account_usage_groups, agent_profile_usage_rows, as_of_label, is_fresh, refresh_allowed_at,
    render_usage_cards_dense, sort_account_groups_attention_first, AgentAccountUsageGroup,
    AgentProfileUsageRow, SYSTEM_PROFILE_ID,
};

/// How long a queued refresh shows as in flight before this page gives up on
/// the machine answering (it answers by re-reporting on its next beat — the
/// synced stamp moving is what really clears the spinner). Web parity
/// (`REFRESH_PENDING_MS`).
const REFRESH_PENDING_SECS: i64 = 45;

/// EXP-817: the page's own refresh never re-tries one account faster than
/// this — a queued command the machine has not answered yet is a CONFLICT on
/// the server, and hammering it buys nothing (web `AUTO_REFRESH_RETRY_MS`).
const AUTO_REFRESH_RETRY_SECS: i64 = 60;

/// The auto-refresh tick (web `useNow(30_000)`).
const AUTO_REFRESH_TICK_SECS: u64 = 30;

/// The device cap a machine must advertise before a forced refresh is offered
/// (`coding::doctor::DEVICE_CAPS`, web `deviceCanRefreshUsage`).
const REFRESH_CAP: &str = "agent-usage-refresh";

/// The account card's width in the wrapping grid: narrow enough for three
/// beside each other on a laptop, wide enough for an email and a chip row.
const CARD_MIN_W: f32 = 280.;
const CARD_MAX_W: f32 = 420.;

/// `14:32` in this machine's own local time — the web tooltip's
/// `toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })`.
fn local_clock(epoch_secs: i64) -> String {
    chrono::DateTime::from_timestamp(epoch_secs, 0)
        .map(|at| at.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or_default()
}

/// One refresh in flight, keyed by [`AgentAccountUsageGroup::key`].
struct RefreshMark {
    /// The usage stamp the account carried when the refresh was queued: the
    /// device's re-report MOVES it, and that is what clears the spinner.
    fetched_at: Option<String>,
    /// Epoch second it was queued — the [`REFRESH_PENDING_SECS`] give-up clock.
    at: i64,
}

pub struct UsageView {
    nav: Entity<Navigation>,
    scroll: ScrollHandle,
    refreshing: HashMap<String, RefreshMark>,
    /// EXP-817: when the page's OWN refresh last tried each account.
    auto_attempts: HashMap<String, i64>,
    /// The last failed MANUAL queue attempt, rendered under the header.
    /// tRPC-only: nothing about a refused command reaches the synced row.
    error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl UsageView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        // Live page: every heartbeat is a `devices` delta, and a team switch
        // re-scopes which shared servers belong here.
        let devices = sync::Store::global(cx).collections().devices.clone();
        let subscriptions = vec![
            cx.observe(&nav, |_, _, cx| cx.notify()),
            cx.observe(&devices, |_, _, cx| cx.notify()),
        ];
        // EXP-817: the auto-refresh tick — the page looks at every account
        // on a coarse clock (and once right away), like the web's `useNow`.
        cx.spawn(async move |this, cx| loop {
            if this
                .update(cx, |this, cx| {
                    this.auto_refresh(cx);
                    cx.notify();
                })
                .is_err()
            {
                return;
            }
            cx.background_executor()
                .timer(std::time::Duration::from_secs(AUTO_REFRESH_TICK_SECS))
                .await;
        })
        .detach();
        Self {
            nav,
            scroll: ScrollHandle::new(),
            refreshing: HashMap::new(),
            auto_attempts: HashMap::new(),
            error: None,
            _subscriptions: subscriptions,
        }
    }

    // -- data ----------------------------------------------------------------

    /// The page's account groups, already attention-sorted. `None` while the
    /// `devices` shape is still Waiting — the page renders its loading note
    /// rather than "nothing reported yet".
    fn groups(&self, cx: &gpui::App) -> Option<Vec<AgentAccountUsageGroup>> {
        let collections = sync::Store::global(cx).collections();
        let devices = collections.devices.read(cx);
        if !devices.is_ready() {
            return None;
        }
        // The USER id, not the per-instance account key (the EXP-642 trap).
        let me = queries::active_account(cx)?.user_id;
        let team = active_team_id(&self.nav, cx);
        // Web parity: my machines, plus the SERVERS shared with the team I am
        // looking at — a teammate's shared server belongs to its own team's
        // page, not to every team I am a member of.
        let rows: Vec<domain::rows::DeviceRow> = devices
            .iter()
            .filter(|row| {
                row.user_id.as_deref() == Some(me.as_str())
                    || (row.is_server()
                        && row.shared_team_id.is_some()
                        && row.shared_team_id == team)
            })
            .cloned()
            .collect();
        let caps: HashMap<String, Vec<String>> = rows
            .iter()
            .map(|row| {
                (
                    row.device_id.clone().unwrap_or_default(),
                    row.cap_ids(),
                )
            })
            .collect();
        let now_ms = chrono::Utc::now().timestamp_millis();
        let rows = agent_profile_usage_rows(&rows, &me, |seen| {
            crate::device_settings::row_is_online(seen, now_ms)
        });
        Some(sort_account_groups_attention_first(account_usage_groups(
            rows,
            |row| Self::can_refresh(row, &caps),
        )))
    }

    /// Agent sections in CONTRACT order (`CodingAgent::ALL`), anything else
    /// after it alphabetically — a section only exists once a machine has
    /// reported the agent.
    fn sections(
        groups: Vec<AgentAccountUsageGroup>,
    ) -> Vec<(String, Vec<AgentAccountUsageGroup>)> {
        let mut sections: Vec<(String, Vec<AgentAccountUsageGroup>)> = Vec::new();
        for group in groups {
            match sections.iter_mut().find(|(agent, _)| agent == &group.agent) {
                Some((_, list)) => list.push(group),
                None => sections.push((group.agent.clone(), vec![group])),
            }
        }
        let rank = |agent: &str| {
            CodingAgent::ALL
                .iter()
                .position(|known| known.id() == agent)
                .unwrap_or(usize::MAX)
        };
        sections.sort_by(|(a, _), (b, _)| rank(a).cmp(&rank(b)).then_with(|| a.cmp(b)));
        sections
    }

    /// Drop the in-flight marks the synced rows have answered (the stamp
    /// moved) or that have waited past [`REFRESH_PENDING_SECS`]. The web page's
    /// `useEffect` in render position — a pure prune, so it never notifies.
    fn prune_refreshing(&mut self, groups: &[AgentAccountUsageGroup], now_epoch: i64) {
        self.refreshing.retain(|key, mark| {
            let Some(group) = groups.iter().find(|group| &group.key == key) else {
                return false;
            };
            let stamp = group
                .usage
                .as_ref()
                .map(|usage| usage.fetched_at.clone())
                .filter(|stamp| !stamp.is_empty());
            stamp == mark.fetched_at && now_epoch - mark.at <= REFRESH_PENDING_SECS
        });
    }

    /// Whether a row's machine may run the refresh at all: one of MY machines,
    /// online, on a build that runs the command (web `deviceCanRefreshUsage`).
    fn can_refresh(row: &AgentProfileUsageRow, caps: &HashMap<String, Vec<String>>) -> bool {
        row.mine
            && row.online
            && caps
                .get(&row.device_id)
                .is_some_and(|caps| caps.iter().any(|cap| cap == REFRESH_CAP))
    }

    /// EXP-817: the page's own refresh round. Every account with an eligible
    /// machine and a freshest report past the floor gets ONE refresh queued
    /// — never while one is in flight, never twice inside
    /// [`AUTO_REFRESH_RETRY_SECS`]. The floor is the device's own 429 budget,
    /// so this can never out-poll what the machine allows itself.
    fn auto_refresh(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(groups) = self.groups(cx) else {
            return;
        };
        let now_epoch = chrono::Utc::now().timestamp();
        self.prune_refreshing(&groups, now_epoch);
        for group in &groups {
            if group.refresh_target.is_none() || self.refreshing.contains_key(&group.key) {
                continue;
            }
            if refresh_allowed_at(group.usage.as_ref(), now_epoch).is_some() {
                continue;
            }
            let last = self.auto_attempts.get(&group.key).copied().unwrap_or(i64::MIN);
            if now_epoch.saturating_sub(last) < AUTO_REFRESH_RETRY_SECS {
                continue;
            }
            self.auto_attempts.insert(group.key.clone(), now_epoch);
            self.refresh(group, true, cx);
        }
    }

    /// Queue `agent_usage_refresh` on the account's refresh target. The answer
    /// arrives as a synced `agent_usage` write, never as a command result — so
    /// the only local state is the in-flight mark. `silent` is the page's own
    /// round: a refusal (a command still queued from the last round is a
    /// CONFLICT) is swallowed, the next tick simply looks again.
    fn refresh(&mut self, group: &AgentAccountUsageGroup, silent: bool, cx: &mut gpui::Context<Self>) {
        let Some(target) = group.refresh_target.as_ref() else {
            return;
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let key = group.key.clone();
        if !silent {
            self.error = None;
        }
        self.refreshing.insert(
            key.clone(),
            RefreshMark {
                fetched_at: group
                    .usage
                    .as_ref()
                    .map(|usage| usage.fetched_at.clone())
                    .filter(|stamp| !stamp.is_empty()),
                at: chrono::Utc::now().timestamp(),
            },
        );
        cx.notify();
        let device_id = target.device_id.clone();
        let agent = target.agent.clone();
        let profile_id = target.profile_id.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::devices::create_agent_usage_refresh_command(
                        &trpc,
                        &device_id,
                        &agent,
                        &profile_id,
                    )
                })
                .await;
            let queued = this
                .update(cx, |this, cx| {
                    if let Err(err) = result {
                        this.refreshing.remove(&key);
                        if !silent {
                            this.error = Some(err.user_message().into());
                        }
                        cx.notify();
                        return false;
                    }
                    true
                })
                .unwrap_or(false);
            if !queued {
                return;
            }
            // The give-up tick: the mark is pruned on the next render past the
            // window, so the page needs ONE notify to get there even if no
            // heartbeat lands meanwhile.
            cx.background_executor()
                .timer(std::time::Duration::from_secs(
                    REFRESH_PENDING_SECS as u64 + 1,
                ))
                .await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    // -- render --------------------------------------------------------------

    /// The page header: the way back to Devices (this page has no rail entry —
    /// the Devices page's header opens it, web parity), the title, and the
    /// auto-refresh note when any account has a machine to ask.
    fn render_header(&self, auto_refreshes: bool, cx: &gpui::App) -> gpui::AnyElement {
        h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_2()
            .pb_2()
            .child(
                crate::controls::glass_icon_button(
                    "usage-back",
                    Icon::new(registry::UI_BACK),
                    cx,
                )
                .tooltip("Devices")
                .on_click(|_: &ClickEvent, window, cx| {
                    navigate(window, cx, Screen::Devices);
                }),
            )
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(cx.theme().foreground)
                    .child("Usage"),
            )
            .when(auto_refreshes, |this| {
                this.child(
                    div()
                        .ml_auto()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Refreshes every 5 minutes while this page is open"),
                )
            })
            .into_any_element()
    }

    /// The `Studio · Personal` chip text: the machine, plus the profile when
    /// it is not the ambient login.
    fn chip_label(row: &AgentProfileUsageRow) -> String {
        let device = if row.device_label.trim().is_empty() {
            row.device_id.as_str()
        } else {
            row.device_label.as_str()
        };
        if row.profile_id == SYSTEM_PROFILE_ID {
            device.to_string()
        } else {
            format!("{device} · {}", row.profile_label)
        }
    }

    /// What hovering a chip says: this machine's OWN report age and state.
    fn chip_tooltip(row: &AgentProfileUsageRow, now_epoch: i64) -> String {
        let mut parts = vec![if row.online { "online" } else { "offline" }.to_string()];
        if !row.signed_in {
            parts.push("not signed in".to_string());
        }
        let stamp = row
            .usage
            .as_ref()
            .map(|usage| usage.fetched_at.clone())
            .filter(|stamp| !stamp.is_empty())
            .or_else(|| row.checked_at.clone());
        match stamp {
            Some(stamp) => {
                let fresh = row
                    .usage
                    .as_ref()
                    .is_some_and(|usage| is_fresh(&usage.fetched_at, now_epoch));
                let relative = crate::comments::relative_time(&stamp, now_epoch);
                if !relative.is_empty() {
                    parts.push(if fresh {
                        format!("reported {relative}")
                    } else {
                        format!("as of {relative}")
                    });
                }
            }
            None => parts.push("no usage reported".to_string()),
        }
        parts.join(" · ")
    }

    fn render_chip(
        index: usize,
        chip: usize,
        row: &AgentProfileUsageRow,
        now_epoch: i64,
        cx: &gpui::App,
    ) -> gpui::AnyElement {
        let muted = cx.theme().muted_foreground;
        let tooltip = SharedString::from(Self::chip_tooltip(row, now_epoch));
        crate::surface::glass_pill(
            ("usage-chip", index * 64 + chip),
            crate::surface::PillSize::Sm,
            crate::surface::PillMode::Readonly,
            cx,
        )
        .when(!row.online, |this| this.text_color(muted))
        .tooltip(move |window, cx| {
            gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
        })
        .child(
            div()
                .size_1p5()
                .flex_shrink_0()
                .rounded_full()
                .bg(if row.online {
                    theme::tokens::GREEN.to_hsla()
                } else {
                    muted.opacity(0.4)
                }),
        )
        .child(SharedString::from(Self::chip_label(row)))
        .into_any_element()
    }

    fn render_card(
        &self,
        index: usize,
        group: &AgentAccountUsageGroup,
        now_epoch: i64,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let foreground = theme.foreground;
        let fresh = group
            .usage
            .as_ref()
            .is_some_and(|usage| is_fresh(&usage.fetched_at, now_epoch));
        let has_windows = group
            .usage
            .as_ref()
            .is_some_and(|usage| !usage.windows.is_empty());
        let next_allowed = refresh_allowed_at(group.usage.as_ref(), now_epoch);
        let refreshing = self.refreshing.contains_key(&group.key);
        // The "as of …" fallback: the numbers' own stamp, else when a machine
        // last probed the account.
        let as_of = group
            .usage
            .as_ref()
            .map(|usage| usage.fetched_at.clone())
            .filter(|stamp| !stamp.is_empty())
            .or_else(|| group.checked_at.clone())
            .map(|stamp| as_of_label(&stamp, now_epoch))
            .filter(|line| !line.is_empty());
        // EXP-694: the identity alone — the section already names the agent.
        let caption: SharedString = if !group.signed_in {
            "Not signed in".into()
        } else {
            group
                .email
                .clone()
                .or_else(|| group.plan.clone())
                .unwrap_or_else(|| "signed in".to_string())
                .into()
        };
        let plan_tail = group
            .signed_in
            .then(|| match (group.email.as_ref(), group.plan.as_ref()) {
                (Some(_), Some(plan)) => Some(SharedString::from(format!(" · {plan}"))),
                _ => None,
            })
            .flatten();

        let refresh_button = group.refresh_target.as_ref().map(|target| {
            let machine = if target.device_label.trim().is_empty() {
                "the machine".to_string()
            } else {
                target.device_label.clone()
            };
            let tooltip: SharedString = if refreshing {
                format!("Waiting for {machine}…").into()
            } else if let Some(at) = next_allowed {
                format!("Refreshed recently. Next refresh at {}.", local_clock(at)).into()
            } else {
                format!("Re-read this account's usage on {machine}").into()
            };
            let target_group = group.clone();
            crate::controls::glass_icon_button(
                ("usage-refresh", index),
                Icon::new(registry::UI_REFRESH),
                cx,
            )
            .tooltip(tooltip)
            .loading(refreshing)
            .disabled(refreshing || next_allowed.is_some())
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.refresh(&target_group, false, cx);
            }))
        });

        let mut chips = h_flex().w_full().min_w_0().flex_wrap().gap_1();
        for (chip, row) in group.rows.iter().enumerate() {
            chips = chips.child(Self::render_chip(index, chip, row, now_epoch, cx));
        }

        // `glass_row_card` is a bare div (row-flex by default): the usage cards
        // stack UNDER the identity line, so the card is an explicit column.
        crate::surface::glass_row_card()
            .id(SharedString::from(format!("usage-card-{}", group.key)))
            .flex()
            .flex_col()
            .flex_grow_1()
            .min_w(px(CARD_MIN_W))
            .max_w(px(CARD_MAX_W))
            .gap_1p5()
            .px_2p5()
            .py_2()
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_2()
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .items_center()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(if group.signed_in {
                                foreground
                            } else {
                                theme::tokens::YELLOW.to_hsla()
                            })
                            .child(div().min_w_0().truncate().child(caption))
                            .children(plan_tail.map(|tail| {
                                div()
                                    .flex_shrink_0()
                                    .font_weight(gpui::FontWeight::NORMAL)
                                    .text_color(muted.opacity(0.6))
                                    .child(tail)
                            })),
                    )
                    .children(refresh_button),
            )
            .child(chips)
            .when_some(
                group.usage.as_ref().filter(|_| has_windows).cloned(),
                |this, usage| {
                    let agent = CodingAgent::parse(&group.agent).unwrap_or_default();
                    this.child(
                        v_flex()
                            .w_full()
                            .min_w_0()
                            .gap_1()
                            // Stale numbers dim rather than claim to be current.
                            .when(!fresh, |this| this.opacity(0.5))
                            .child(render_usage_cards_dense(agent, &usage, now_epoch, cx))
                            .children((!fresh && !usage.stale).then(|| {
                                as_of.clone().map(|line| {
                                    div().text_xs().text_color(muted).child(SharedString::from(line))
                                })
                            }).flatten()),
                    )
                },
            )
            .when(!has_windows, |this| {
                this.child(
                    div().text_xs().text_color(muted).child(SharedString::from(
                        match as_of.clone() {
                            Some(line) => format!("No usage reported · {line}"),
                            None => "No usage reported".to_string(),
                        },
                    )),
                )
            })
            .into_any_element()
    }
}

impl Render for UsageView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let now_epoch = chrono::Utc::now().timestamp();
        let muted = cx.theme().muted_foreground;
        let loaded = self.groups(cx);
        if let Some(groups) = loaded.as_ref() {
            self.prune_refreshing(groups, now_epoch);
        }
        let auto_refreshes = loaded
            .as_ref()
            .is_some_and(|groups| groups.iter().any(|group| group.refresh_target.is_some()));

        let mut column = v_flex()
            .min_w_0()
            .child(self.render_header(auto_refreshes, cx));
        if let Some(error) = self.error.clone() {
            column = column.child(
                div()
                    .px_1()
                    .pb_2()
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .child(error),
            );
        }

        match loaded {
            None => {
                column = column.child(
                    div()
                        .px_1()
                        .py_2()
                        .text_xs()
                        .text_color(muted)
                        .child("Loading usage…"),
                );
            }
            Some(groups) if groups.is_empty() => {
                column = column.child(crate::controls::empty_state(
                    Icon::new(registry::UI_DEVICE_OFFLINE),
                    "No agent accounts yet",
                    "No machine has reported an agent account yet.",
                    cx,
                ));
            }
            Some(groups) => {
                let mut index = 0;
                for (agent, section) in Self::sections(groups) {
                    let label = CodingAgent::parse(&agent)
                        .map(|agent| agent.label().to_string())
                        .unwrap_or(agent);
                    let mut block = v_flex()
                        .min_w_0()
                        .pb_2()
                        .child(crate::surface::glass_section_header(label, None, cx));
                    // The wrapping grid: cards grow to share a row and wrap
                    // to the next when the window is too narrow for another.
                    let mut grid = h_flex().w_full().min_w_0().flex_wrap().items_start().gap_2();
                    for group in &section {
                        grid = grid.child(self.render_card(index, group, now_epoch, cx));
                        index += 1;
                    }
                    block = block.child(grid);
                    column = column.child(block);
                }
            }
        }

        page_scaffold("usage-screen-scroll", &self.scroll, column)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(agent: &str) -> AgentProfileUsageRow {
        AgentProfileUsageRow {
            key: format!("dev-1:{agent}:{SYSTEM_PROFILE_ID}"),
            device_id: "dev-1".into(),
            device_label: "Studio".into(),
            mine: true,
            online: true,
            agent: agent.into(),
            profile_id: SYSTEM_PROFILE_ID.into(),
            profile_label: "Default".into(),
            active: true,
            signed_in: true,
            email: None,
            plan: None,
            usage: None,
            checked_at: None,
        }
    }

    fn group(agent: &str, key: &str) -> AgentAccountUsageGroup {
        AgentAccountUsageGroup {
            key: key.to_string(),
            agent: agent.to_string(),
            signed_in: true,
            email: None,
            plan: None,
            rows: vec![row(agent)],
            usage: None,
            checked_at: None,
            refresh_target: None,
        }
    }

    /// The agent headings follow the CONTRACT order, never the group order or
    /// the alphabet; an agent this build has no name for still gets its own
    /// section, after the known ones.
    #[test]
    fn sections_follow_the_contract_agent_order() {
        let sections = UsageView::sections(vec![
            group("pi", "pi:a"),
            group("zed", "zed:a"),
            group("codex", "codex:a"),
            group("claude", "claude:a"),
            group("codex", "codex:b"),
        ]);
        assert_eq!(
            sections
                .iter()
                .map(|(agent, groups)| (agent.as_str(), groups.len()))
                .collect::<Vec<_>>(),
            vec![("claude", 1), ("codex", 2), ("pi", 1), ("zed", 1)]
        );
    }

    /// The refresh is offered only on MY machine, only while it is online, and
    /// only on a build that advertises the command (web `deviceCanRefreshUsage`).
    #[test]
    fn refresh_needs_my_own_online_machine_with_the_cap() {
        let caps: HashMap<String, Vec<String>> =
            [("dev-1".to_string(), vec![REFRESH_CAP.to_string()])]
                .into_iter()
                .collect();
        let mine = row("claude");
        assert!(UsageView::can_refresh(&mine, &caps));

        let mut theirs = row("claude");
        theirs.mine = false;
        assert!(!UsageView::can_refresh(&theirs, &caps));

        let mut offline = row("claude");
        offline.online = false;
        assert!(!UsageView::can_refresh(&offline, &caps));

        // An older build that never advertised the cap.
        assert!(!UsageView::can_refresh(&mine, &HashMap::new()));
        let capless: HashMap<String, Vec<String>> =
            [("dev-1".to_string(), Vec::new())].into_iter().collect();
        assert!(!UsageView::can_refresh(&mine, &capless));
    }

    /// The chip names the machine, and the profile only when it is not the
    /// ambient login; hovering it says how old THAT machine's report is.
    #[test]
    fn chips_name_the_machine_and_a_named_profile() {
        let mut studio = row("claude");
        assert_eq!(UsageView::chip_label(&studio), "Studio");
        studio.profile_id = "0a1b2c3d".into();
        studio.profile_label = "Personal".into();
        assert_eq!(UsageView::chip_label(&studio), "Studio · Personal");
        studio.device_label = String::new();
        assert_eq!(UsageView::chip_label(&studio), "dev-1 · Personal");

        let now = crate::comments::parse_epoch("2026-08-28T12:00:00.000Z").unwrap();
        let mut quiet = row("claude");
        assert_eq!(UsageView::chip_tooltip(&quiet, now), "online · no usage reported");
        quiet.online = false;
        quiet.signed_in = false;
        quiet.checked_at = Some("2026-08-28T10:00:00.000Z".into());
        assert_eq!(
            UsageView::chip_tooltip(&quiet, now),
            "offline · not signed in · as of 2 hours ago"
        );
    }
}
