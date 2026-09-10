//! EXP-818: the **Accounts** section of the Devices page — the Usage page
//! (EXP-807/817) folded into Devices, so ONE page says which machines exist
//! and which agent accounts are live on them.
//!
//! One ROW per ACCOUNT (an agent plus the login the machines named) off the
//! synced `devices` rows (own machines plus the servers teammates shared
//! with the active team), grouped by agent in contract order, ATTENTION
//! FIRST: signed-out accounts lead, then anything at or over the danger
//! threshold, then the rest. The machines holding the account are chips on
//! the row — a chip wears a CHECK when the account is the ACTIVE login on
//! that machine (`AgentProfileUsageRow::active`), and clicking a chip of one
//! of MY machines offers "Switch account" / "Sign in" on that machine
//! (`device_settings::login_affordance`, the ONE rule): the "Default" /
//! "active" chips the old page wore said nothing about where the account
//! was live. The numbers are the FRESHEST machine's report (they are the
//! account's limits, so every machine reads the same ones), rendered as
//! [`crate::usage_bar::render_usage_cards_dense`]; stale numbers keep the
//! dimmed "as of …" treatment.
//!
//! The section owns no model of its own: rows, groups, ordering and the
//! refresh floor live in [`crate::usage_bar`] beside the ×4 card rules (the
//! twin of the web `agent-usage.ts`), so this file is layout, the refresh
//! round-trip and the login menu.
//!
//! "Refresh" queues an `agent_usage_refresh` command on one of MY machines
//! that runs it (cap `agent-usage-refresh`), never more often than the
//! device's own rate-limit floor; while the section is on screen it does
//! that BY ITSELF for every account whose freshest report is past the floor
//! (EXP-817).

use std::collections::HashMap;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, App, ClickEvent, Entity, InteractiveElement as _, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    h_flex, menu::DropdownMenu as _, menu::PopupMenuItem, notification::Notification, v_flex,
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _,
};

use coding::CodingAgent;

use crate::icons::registry;
use crate::navigation::{active_team_id, nav_for_window, Navigation};
use crate::queries;
use crate::usage_bar::{
    account_usage_groups, agent_profile_usage_rows, as_of_label, is_fresh, refresh_allowed_at,
    render_usage_cards_dense, sort_account_groups_attention_first, AgentAccountUsageGroup,
    AgentProfileUsageRow, SYSTEM_PROFILE_ID,
};

/// How long a queued refresh shows as in flight before this section gives up
/// on the machine answering (it answers by re-reporting on its next beat —
/// the synced stamp moving is what really clears the spinner). Web parity
/// (`REFRESH_PENDING_MS`).
const REFRESH_PENDING_SECS: i64 = 45;

/// EXP-817: the section's own refresh never re-tries one account faster than
/// this — a queued command the machine has not answered yet is a CONFLICT on
/// the server, and hammering it buys nothing (web `AUTO_REFRESH_RETRY_MS`).
const AUTO_REFRESH_RETRY_SECS: i64 = 60;

/// The auto-refresh tick (web `useNow(30_000)`).
const AUTO_REFRESH_TICK_SECS: u64 = 30;

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

/// One refresh in flight, keyed by [`AgentAccountUsageGroup::key`].
struct RefreshMark {
    /// The usage stamp the account carried when the refresh was queued: the
    /// device's re-report MOVES it, and that is what clears the spinner.
    fetched_at: Option<String>,
    /// Epoch second it was queued — the [`REFRESH_PENDING_SECS`] give-up clock.
    at: i64,
}

pub struct AccountsSection {
    nav: Entity<Navigation>,
    refreshing: HashMap<String, RefreshMark>,
    /// EXP-817: when the section's OWN refresh last tried each account.
    auto_attempts: HashMap<String, i64>,
    /// The last failed MANUAL queue attempt, rendered under the header.
    /// tRPC-only: nothing about a refused command reaches the synced row.
    error: Option<SharedString>,
    _subscriptions: Vec<Subscription>,
}

impl AccountsSection {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        // Live section: every heartbeat is a `devices` delta, and a team
        // switch re-scopes which shared servers belong here.
        let devices = sync::Store::global(cx).collections().devices.clone();
        let subscriptions = vec![
            cx.observe(&nav, |_, _, cx| cx.notify()),
            cx.observe(&devices, |_, _, cx| cx.notify()),
        ];
        // EXP-817: the auto-refresh tick — the section looks at every account
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

    /// What hovering a chip says: this machine's OWN report age and state,
    /// and (EXP-818) whether the account is the ACTIVE login there.
    fn chip_tooltip(row: &AgentProfileUsageRow, now_epoch: i64) -> String {
        let mut parts = vec![if row.online { "online" } else { "offline" }.to_string()];
        if !row.signed_in {
            parts.push("not signed in".to_string());
        } else if row.active {
            parts.push("active here".to_string());
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

    /// EXP-818: one machine chip — the online dot, the machine (· profile),
    /// and a CHECK when the account is the active login on that machine.
    /// A chip of one of MY machines that can run a sign-in opens a menu:
    /// "Switch account on X" (signed in) or "Sign in on X"; every other chip
    /// is read-only with the tooltip.
    fn render_chip(
        index: usize,
        chip: usize,
        row: &AgentProfileUsageRow,
        own_device_id: &str,
        caps: &[String],
        now_epoch: i64,
        cx: &App,
    ) -> gpui::AnyElement {
        let muted = cx.theme().muted_foreground;
        let tooltip = SharedString::from(Self::chip_tooltip(row, now_epoch));
        let dot = div()
            .size_1p5()
            .flex_shrink_0()
            .rounded_full()
            .bg(if row.online {
                theme::tokens::GREEN.to_hsla()
            } else {
                muted.opacity(0.4)
            });
        let label = SharedString::from(Self::chip_label(row));
        let check = (row.signed_in && row.active).then(|| {
            Icon::new(registry::UI_CHECK)
                .with_size(px(crate::surface::PillSize::Sm.glyph()))
                .text_color(theme::tokens::GREEN.to_hsla())
        });
        let agent = CodingAgent::parse(&row.agent);
        let own = row.device_id == own_device_id;
        let affordance = agent.filter(|_| row.mine).and_then(|agent| {
            crate::device_settings::login_affordance(agent, own, row.online, caps, row.signed_in)
        });
        match (agent, affordance) {
            (Some(agent), Some(affordance)) => {
                let device_id = row.device_id.clone();
                let device_label = Self::chip_label(row);
                let switch = affordance.switch;
                let item_label = SharedString::from(format!(
                    "{} on {device_label}",
                    affordance.label
                ));
                crate::surface::glass_pill_button(
                    ("accounts-chip", index * 64 + chip),
                    crate::surface::PillSize::Sm,
                    cx,
                )
                .when(!row.online, |this| this.text_color(muted))
                .tooltip(tooltip)
                .child(dot)
                .child(label)
                .children(check)
                .dropdown_menu(move |menu, _window, _cx| {
                    let device_id = device_id.clone();
                    let device_label = device_label.clone();
                    menu.item(
                        PopupMenuItem::new(item_label.clone())
                            .icon(Icon::new(if switch {
                                registry::UI_SWAP
                            } else {
                                registry::UI_SIGN_IN
                            }))
                            .on_click(move |_, window, cx| {
                                start_login(
                                    device_id.clone(),
                                    device_label.clone(),
                                    own,
                                    agent,
                                    switch,
                                    window,
                                    cx,
                                );
                            }),
                    )
                })
                .into_any_element()
            }
            _ => crate::surface::glass_pill(
                ("accounts-chip", index * 64 + chip),
                crate::surface::PillSize::Sm,
                crate::surface::PillMode::Readonly,
                cx,
            )
            .when(!row.online, |this| this.text_color(muted))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
            })
            .child(dot)
            .child(label)
            .children(check)
            .into_any_element(),
        }
    }

    /// One account ROW (EXP-818: a flat row under the Accounts band, not a
    /// card in a grid): the identity line with the refresh button, the
    /// machine chips, the dense usage windows.
    #[allow(clippy::too_many_arguments)] // one call site; render facts
    fn render_row(
        &self,
        index: usize,
        group: &AgentAccountUsageGroup,
        own_device_id: &str,
        caps: &HashMap<String, Vec<String>>,
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
        // EXP-694: the identity alone — the agent label sits above the rows.
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
                ("accounts-refresh", index),
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
            let device_caps = caps.get(&row.device_id).map(Vec::as_slice).unwrap_or(&[]);
            chips = chips.child(Self::render_chip(
                index,
                chip,
                row,
                own_device_id,
                device_caps,
                now_epoch,
                cx,
            ));
        }

        crate::surface::flat_row()
            .id(SharedString::from(format!("accounts-row-{}", group.key)))
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .gap_1p5()
            .px_3()
            .py_2p5()
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

/// EXP-818: a chip's sign-in — this machine opens the login tab right here
/// (`agent_login::open_login_tab`, which confirms a destructive switch);
/// another of my machines gets an `agent_login` command on its heartbeat,
/// finished from that machine's Device settings (the code hand-back lives
/// there). Mirrors `device_settings::start_login` for the own/remote fork.
#[allow(clippy::too_many_arguments)]
fn start_login(
    device_id: String,
    device_label: String,
    own: bool,
    agent: CodingAgent,
    switch: bool,
    window: &mut Window,
    cx: &mut App,
) {
    if own {
        crate::agent_login::open_login_tab(agent, switch, cx);
        return;
    }
    let handle = window.window_handle();
    let queue = move |cx: &mut App| {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let device_id = device_id.clone();
        let device_label = device_label.clone();
        cx.spawn(async move |cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::devices::create_agent_login_command(&trpc, &device_id, agent.id(), switch)
                })
                .await;
            let _ = handle.update(cx, |_, window, cx| {
                match result {
                    Ok(_) => window.push_notification(
                        Notification::success(SharedString::from(format!(
                            "Sign-in sent to {device_label}. Finish it in that machine's settings."
                        ))),
                        cx,
                    ),
                    Err(err) => window.push_notification(
                        Notification::error(SharedString::from(err.user_message())),
                        cx,
                    ),
                }
            });
        })
        .detach();
    };
    if switch {
        crate::agent_login::confirm_switch_then(agent, cx, queue);
    } else {
        queue(cx);
    }
}

impl Render for AccountsSection {
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
        let own_device_id = queries::own_device_id(cx);
        // The machines' caps, for the chips' sign-in gate.
        let caps: HashMap<String, Vec<String>> = sync::Store::global(cx)
            .collections()
            .devices
            .read(cx)
            .iter()
            .map(|row| (row.device_id.clone().unwrap_or_default(), row.cap_ids()))
            .collect();

        let note = auto_refreshes.then(|| {
            div()
                .text_xs()
                .text_color(muted)
                .child("Refreshes every 5 minutes")
                .into_any_element()
        });
        // The section carries its OWN top spacing (the page column has no
        // `gap`), like the run sections it sits where.
        let mut column = v_flex()
            .min_w_0()
            .mt_6()
            .child(crate::surface::glass_section_header("Accounts", note, cx));
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
                        .child("Loading accounts…"),
                );
            }
            Some(groups) if groups.is_empty() => {
                column = column.child(
                    div()
                        .px_1()
                        .py_2()
                        .text_xs()
                        .text_color(muted)
                        .child("No machine has reported an agent account yet."),
                );
            }
            Some(groups) => {
                let mut index = 0;
                for (agent, section) in Self::sections(groups) {
                    let label = CodingAgent::parse(&agent)
                        .map(|agent| agent.label().to_string())
                        .unwrap_or(agent);
                    // The agent name as a muted sub-label over its rows.
                    let mut block = v_flex().min_w_0().pb_1().child(
                        div()
                            .px_3()
                            .pt_2()
                            .pb_1()
                            .text_xs()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(muted)
                            .child(SharedString::from(label)),
                    );
                    for group in &section {
                        block = block.child(self.render_row(
                            index,
                            group,
                            &own_device_id,
                            &caps,
                            now_epoch,
                            cx,
                        ));
                        index += 1;
                    }
                    column = column.child(block);
                }
            }
        }
        column
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
        let sections = AccountsSection::sections(vec![
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
        assert!(AccountsSection::can_refresh(&mine, &caps));

        let mut theirs = row("claude");
        theirs.mine = false;
        assert!(!AccountsSection::can_refresh(&theirs, &caps));

        let mut offline = row("claude");
        offline.online = false;
        assert!(!AccountsSection::can_refresh(&offline, &caps));

        // An older build that never advertised the cap.
        assert!(!AccountsSection::can_refresh(&mine, &HashMap::new()));
        let capless: HashMap<String, Vec<String>> =
            [("dev-1".to_string(), Vec::new())].into_iter().collect();
        assert!(!AccountsSection::can_refresh(&mine, &capless));
    }

    /// The chip names the machine, and the profile only when it is not the
    /// ambient login; hovering it says how old THAT machine's report is.
    #[test]
    fn chips_name_the_machine_and_a_named_profile() {
        let mut studio = row("claude");
        assert_eq!(AccountsSection::chip_label(&studio), "Studio");
        studio.profile_id = "0a1b2c3d".into();
        studio.profile_label = "Personal".into();
        assert_eq!(AccountsSection::chip_label(&studio), "Studio · Personal");
        studio.device_label = String::new();
        assert_eq!(AccountsSection::chip_label(&studio), "dev-1 · Personal");

        let now = crate::comments::parse_epoch("2026-08-28T12:00:00.000Z").unwrap();
        let mut quiet = row("claude");
        // EXP-818: the active login on that machine says so.
        assert_eq!(
            AccountsSection::chip_tooltip(&quiet, now),
            "online · active here · no usage reported"
        );
        quiet.active = false;
        assert_eq!(AccountsSection::chip_tooltip(&quiet, now), "online · no usage reported");
        quiet.online = false;
        quiet.signed_in = false;
        quiet.checked_at = Some("2026-08-28T10:00:00.000Z".into());
        assert_eq!(
            AccountsSection::chip_tooltip(&quiet, now),
            "offline · not signed in · as of 2 hours ago"
        );
    }
}
