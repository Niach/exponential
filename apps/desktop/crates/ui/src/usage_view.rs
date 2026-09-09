//! The Usage center screen (EXP-807): the desktop twin of the web
//! `t/$teamSlug/usage` page (`components/agent-usage-page.tsx`, the spec this
//! file mirrors) — every machine's agent usage on ONE page instead of one
//! device dialog at a time.
//!
//! One row per device × agent PROFILE off the synced `devices` rows (own
//! machines plus the servers teammates shared with the active team), grouped
//! by agent in contract order, ATTENTION FIRST: signed-out rows lead, then
//! anything at or over the danger threshold, then the rest. The windows are
//! the same [`crate::usage_bar::render_usage_cards`] the device dialog and the
//! settings panes draw; stale numbers keep the dimmed "as of …" treatment.
//!
//! The page owns no model of its own: rows, ordering and the refresh floor
//! live in [`crate::usage_bar`] beside the ×4 card rules (the twin of the web
//! `agent-usage.ts` page section), so this file is layout, the refresh
//! round-trip and nothing else.
//!
//! "Refresh" queues an `agent_usage_refresh` command on one of MY machines
//! that runs it (cap `agent-usage-refresh`), never more often than the
//! device's own rate-limit floor — the button greys out and names the next
//! allowed time instead of queueing a no-op.

use std::collections::HashMap;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, ClickEvent, Entity, InteractiveElement as _, IntoElement, ParentElement, Render,
    ScrollHandle, SharedString, Styled, Subscription, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Disableable as _, Icon};

use coding::CodingAgent;

use crate::actions_view::page_scaffold;
use crate::icons::registry;
use crate::navigation::{active_team_id, nav_for_window, navigate, Navigation, Screen};
use crate::queries;
use crate::usage_bar::{
    agent_profile_usage_rows, as_of_label, is_fresh, refresh_allowed_at, render_usage_cards,
    sort_attention_first, AgentProfileUsageRow,
};

/// How long a queued refresh shows as in flight before this page gives up on
/// the machine answering (it answers by re-reporting on its next beat — the
/// synced stamp moving is what really clears the spinner). Web parity
/// (`REFRESH_PENDING_MS`).
const REFRESH_PENDING_SECS: i64 = 45;

/// The device cap a machine must advertise before a forced refresh is offered
/// (`coding::doctor::DEVICE_CAPS`, web `deviceCanRefreshUsage`).
const REFRESH_CAP: &str = "agent-usage-refresh";

/// `14:32` in this machine's own local time — the web tooltip's
/// `toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })`.
fn local_clock(epoch_secs: i64) -> String {
    chrono::DateTime::from_timestamp(epoch_secs, 0)
        .map(|at| at.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or_default()
}

/// One refresh in flight, keyed by [`AgentProfileUsageRow::key`].
struct RefreshMark {
    /// The usage stamp the row carried when the refresh was queued: the
    /// device's re-report MOVES it, and that is what clears the spinner.
    fetched_at: Option<String>,
    /// Epoch second it was queued — the [`REFRESH_PENDING_SECS`] give-up clock.
    at: i64,
}

pub struct UsageView {
    nav: Entity<Navigation>,
    scroll: ScrollHandle,
    refreshing: HashMap<String, RefreshMark>,
    /// The last failed queue attempt, rendered under the header. tRPC-only:
    /// nothing about a refused command reaches the synced row.
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
        Self {
            nav,
            scroll: ScrollHandle::new(),
            refreshing: HashMap::new(),
            error: None,
            _subscriptions: subscriptions,
        }
    }

    // -- data ----------------------------------------------------------------

    /// The page's rows, already attention-sorted, plus the caps of the machine
    /// each row sits on. `None` while the `devices` shape is still Waiting —
    /// the page renders its loading note rather than "nothing reported yet".
    #[allow(clippy::type_complexity)] // rows + one caps lookup, one call site
    fn rows(
        &self,
        cx: &gpui::App,
    ) -> Option<(Vec<AgentProfileUsageRow>, HashMap<String, Vec<String>>)> {
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
        let caps = rows
            .iter()
            .map(|row| {
                (
                    row.device_id.clone().unwrap_or_default(),
                    row.cap_ids(),
                )
            })
            .collect();
        let now_ms = chrono::Utc::now().timestamp_millis();
        Some((
            sort_attention_first(agent_profile_usage_rows(&rows, &me, |seen| {
                crate::device_settings::row_is_online(seen, now_ms)
            })),
            caps,
        ))
    }

    /// Agent groups in CONTRACT order (`CodingAgent::ALL`), anything else
    /// after it alphabetically — an agent group only exists once a machine has
    /// reported it.
    fn groups(rows: Vec<AgentProfileUsageRow>) -> Vec<(String, Vec<AgentProfileUsageRow>)> {
        let mut groups: Vec<(String, Vec<AgentProfileUsageRow>)> = Vec::new();
        for row in rows {
            match groups.iter_mut().find(|(agent, _)| agent == &row.agent) {
                Some((_, list)) => list.push(row),
                None => groups.push((row.agent.clone(), vec![row])),
            }
        }
        let rank = |agent: &str| {
            CodingAgent::ALL
                .iter()
                .position(|known| known.id() == agent)
                .unwrap_or(usize::MAX)
        };
        groups.sort_by(|(a, _), (b, _)| rank(a).cmp(&rank(b)).then_with(|| a.cmp(b)));
        groups
    }

    /// Drop the in-flight marks the synced rows have answered (the stamp
    /// moved) or that have waited past [`REFRESH_PENDING_SECS`]. The web page's
    /// `useEffect` in render position — a pure prune, so it never notifies.
    fn prune_refreshing(&mut self, rows: &[AgentProfileUsageRow], now_epoch: i64) {
        self.refreshing.retain(|key, mark| {
            let Some(row) = rows.iter().find(|row| &row.key == key) else {
                return false;
            };
            let stamp = row
                .usage
                .as_ref()
                .map(|usage| usage.fetched_at.clone())
                .filter(|stamp| !stamp.is_empty());
            stamp == mark.fetched_at && now_epoch - mark.at <= REFRESH_PENDING_SECS
        });
    }

    /// Whether this row's ▻ refresh is offered at all: one of MY machines,
    /// online, on a build that runs the command (web `deviceCanRefreshUsage`).
    fn can_refresh(row: &AgentProfileUsageRow, caps: &HashMap<String, Vec<String>>) -> bool {
        row.mine
            && row.online
            && caps
                .get(&row.device_id)
                .is_some_and(|caps| caps.iter().any(|cap| cap == REFRESH_CAP))
    }

    /// Queue `agent_usage_refresh` on the row's machine. The answer arrives as
    /// a synced `agent_usage` write, never as a command result — so the only
    /// local state is the in-flight mark.
    fn refresh(&mut self, row: &AgentProfileUsageRow, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let key = row.key.clone();
        self.error = None;
        self.refreshing.insert(
            key.clone(),
            RefreshMark {
                fetched_at: row
                    .usage
                    .as_ref()
                    .map(|usage| usage.fetched_at.clone())
                    .filter(|stamp| !stamp.is_empty()),
                at: chrono::Utc::now().timestamp(),
            },
        );
        cx.notify();
        let device_id = row.device_id.clone();
        let agent = row.agent.clone();
        let profile_id = row.profile_id.clone();
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
                        this.error = Some(err.user_message().into());
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
    /// the Devices page's header opens it, web parity) and the title.
    fn render_header(&self, cx: &gpui::App) -> gpui::AnyElement {
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
            .into_any_element()
    }

    fn render_row(
        &self,
        index: usize,
        row: &AgentProfileUsageRow,
        caps: &HashMap<String, Vec<String>>,
        now_epoch: i64,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let fresh = row
            .usage
            .as_ref()
            .is_some_and(|usage| is_fresh(&usage.fetched_at, now_epoch));
        let has_windows = row
            .usage
            .as_ref()
            .is_some_and(|usage| !usage.windows.is_empty());
        let next_allowed = refresh_allowed_at(row.usage.as_ref(), now_epoch);
        let refreshing = self.refreshing.contains_key(&row.key);
        // The "as of …" fallback: the numbers' own stamp, else when the
        // machine last probed the account.
        let as_of = row
            .usage
            .as_ref()
            .map(|usage| usage.fetched_at.clone())
            .filter(|stamp| !stamp.is_empty())
            .or_else(|| row.checked_at.clone())
            .map(|stamp| as_of_label(&stamp, now_epoch))
            .filter(|line| !line.is_empty());
        // EXP-694: the identity alone — the row's own context already names
        // the machine and the agent.
        let caption: SharedString = if !row.signed_in {
            "Not signed in".into()
        } else {
            row.email
                .clone()
                .or_else(|| row.plan.clone())
                .unwrap_or_else(|| "signed in".to_string())
                .into()
        };
        let plan_tail = row
            .signed_in
            .then(|| match (row.email.as_ref(), row.plan.as_ref()) {
                (Some(_), Some(plan)) => Some(SharedString::from(format!(" · {plan}"))),
                _ => None,
            })
            .flatten();

        let refresh_button = Self::can_refresh(row, caps).then(|| {
            let tooltip: SharedString = if refreshing {
                "Waiting for the machine…".into()
            } else if let Some(at) = next_allowed {
                format!("Refreshed recently. Next refresh at {}.", local_clock(at)).into()
            } else {
                "Re-read this profile's usage on the machine".into()
            };
            let target = row.clone();
            crate::controls::glass_icon_button(
                ("usage-refresh", index),
                Icon::new(registry::UI_REFRESH),
                cx,
            )
            .tooltip(tooltip)
            .loading(refreshing)
            .disabled(refreshing || next_allowed.is_some())
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.refresh(&target, cx);
            }))
        });

        // `glass_row_card` is a bare div (row-flex by default): the usage cards
        // stack UNDER the identity line, so the card is an explicit column.
        crate::surface::glass_row_card()
            .id(SharedString::from(format!("usage-row-{}", row.key)))
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .gap_2()
            .px_3()
            .py_2p5()
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_2()
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
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_0p5()
                            .child(
                                h_flex()
                                    .w_full()
                                    .min_w_0()
                                    .items_center()
                                    .gap_1p5()
                                    .overflow_hidden()
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .text_sm()
                                            .whitespace_nowrap()
                                            .child(SharedString::from(
                                                if row.device_label.trim().is_empty() {
                                                    row.device_id.clone()
                                                } else {
                                                    row.device_label.clone()
                                                },
                                            )),
                                    )
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .text_xs()
                                            .text_color(muted)
                                            .child(SharedString::from(row.profile_label.clone())),
                                    )
                                    // The profile this machine runs by default.
                                    .when(row.active, |this| {
                                        this.child(
                                            crate::surface::glass_pill(
                                                ("usage-active", index),
                                                crate::surface::PillSize::Sm,
                                                crate::surface::PillMode::Readonly,
                                                cx,
                                            )
                                            .child("active"),
                                        )
                                    }),
                            )
                            .child(
                                h_flex()
                                    .w_full()
                                    .min_w_0()
                                    .items_center()
                                    .text_xs()
                                    .text_color(if row.signed_in {
                                        muted
                                    } else {
                                        theme::tokens::YELLOW.to_hsla()
                                    })
                                    .child(div().min_w_0().truncate().child(caption))
                                    .children(plan_tail.map(|tail| {
                                        div()
                                            .flex_shrink_0()
                                            .text_color(muted.opacity(0.6))
                                            .child(tail)
                                    })),
                            ),
                    )
                    .children(refresh_button),
            )
            .when_some(
                row.usage.as_ref().filter(|_| has_windows).cloned(),
                |this, usage| {
                    let agent = CodingAgent::parse(&row.agent).unwrap_or_default();
                    this.child(
                        v_flex()
                            .w_full()
                            .min_w_0()
                            .gap_1()
                            // Stale numbers dim rather than claim to be current.
                            .when(!fresh, |this| this.opacity(0.5))
                            .child(render_usage_cards(agent, &usage, now_epoch, true, cx))
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
        let loaded = self.rows(cx);
        if let Some((rows, _)) = loaded.as_ref() {
            self.prune_refreshing(rows, now_epoch);
        }

        let mut column = v_flex().min_w_0().child(self.render_header(cx));
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
            Some((rows, _)) if rows.is_empty() => {
                column = column.child(crate::controls::empty_state(
                    Icon::new(registry::UI_DEVICE_OFFLINE),
                    "No agent accounts yet",
                    "No machine has reported an agent account yet.",
                    cx,
                ));
            }
            Some((rows, caps)) => {
                let mut index = 0;
                for (agent, group) in Self::groups(rows) {
                    let label = CodingAgent::parse(&agent)
                        .map(|agent| agent.label().to_string())
                        .unwrap_or(agent);
                    let mut block = v_flex()
                        .min_w_0()
                        .pb_2()
                        .child(crate::surface::glass_section_header(label, None, cx));
                    let mut list = v_flex().min_w_0().gap_2();
                    for row in &group {
                        list = list.child(self.render_row(index, row, &caps, now_epoch, cx));
                        index += 1;
                    }
                    block = block.child(list);
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
    use crate::usage_bar::SYSTEM_PROFILE_ID;

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

    /// The agent headings follow the CONTRACT order, never the row order or
    /// the alphabet; an agent this build has no name for still gets its own
    /// group, after the known ones.
    #[test]
    fn groups_follow_the_contract_agent_order() {
        let groups = UsageView::groups(vec![
            row("pi"),
            row("zed"),
            row("codex"),
            row("claude"),
            row("codex"),
        ]);
        assert_eq!(
            groups
                .iter()
                .map(|(agent, rows)| (agent.as_str(), rows.len()))
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
}
