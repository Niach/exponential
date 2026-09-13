//! EXP-818: the **Accounts** section of the Devices page — the Usage page
//! (EXP-807/817) folded into Devices, so ONE page says which devices exist
//! and which agent accounts are live on them.
//!
//! One ROW per ACCOUNT (an agent plus the login the devices named) off the
//! synced `devices` rows (own devices plus the servers teammates shared
//! with the active team), under one TAB per agent (EXP-849 — codex's rows used
//! to push claude's numbers below the fold), ATTENTION FIRST: signed-out and
//! revoked accounts lead, then anything at or over the danger threshold, then
//! the rest. The devices holding the account are chips on the row — a chip
//! wears a CHECK where the account is that device's ACTIVE login
//! (`AgentProfileUsageRow::active`) and the health badge where the credential
//! there is broken.
//!
//! EXP-862: a chip is also a CONTROL. It carries the same menu the Devices
//! rows' chips do — the one shared rule (`usage_bar::chip_actions`): a signed
//! out or expired login offers a sign-in, a healthy one "Set as default"
//! (`agent_profile_use`) and "Remove account" (`agent_profile_remove`, behind
//! the pinned confirm), plus the way to that device's settings. A person
//! looking at an account here should not have to go find the device row to fix
//! it. What went with EXP-862: the per-row refresh button and the "Refreshes
//! every 5 minutes" caption (the section refreshes itself, and a button for it
//! mostly said "too soon"), and the "Not signed in" title — the chip badge is
//! the ONE signed-out notice ×4.
//!
//! The numbers are the FRESHEST device's report (they are the account's
//! limits, so every device reads the same ones), rendered as
//! [`crate::usage_bar::render_usage_cards_dense`]; stale numbers keep the
//! dimmed "as of …" treatment, and a login nothing has probed yet says
//! "Checking…" rather than claiming to have nothing.
//!
//! "+ Add account" (the header) and a row's bare "+" chip are the page's two
//! writes, both [`crate::agent_login`]'s: pick a device, sign in there.
//!
//! The section owns no model of its own: rows, groups, ordering and the
//! refresh floor live in [`crate::usage_bar`] beside the ×4 card rules (the
//! twin of the web `agent-usage.ts`), so this file is layout and the account
//! writes.
//!
//! While the section is on screen it refreshes BY ITSELF (EXP-817): every
//! account whose freshest report is past the device's own rate-limit floor
//! gets one `agent_usage_refresh` queued on a device of mine that runs it.

use std::collections::HashMap;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, App, ClickEvent, Entity, InteractiveElement as _, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription,
    Window,
};
use gpui_component::{
    button::ButtonVariant, h_flex, menu::DropdownMenu as _, menu::PopupMenuItem,
    notification::Notification, v_flex, ActiveTheme as _, Icon, Sizable as _, WindowExt as _,
};

use coding::CodingAgent;

use crate::icons::registry;
use crate::navigation::{active_team_id, nav_for_window, Navigation};
use crate::queries;
use crate::usage_bar::{
    account_usage_groups, agent_profile_usage_rows, as_of_label, chip_actions, is_fresh,
    refresh_allowed_at, remove_account_confirm, render_usage_cards_dense,
    sort_account_groups_attention_first, usage_caption, usage_state, AgentAccountUsageGroup,
    AgentProfileUsageRow, ChipAction, SYSTEM_PROFILE_ID,
};

/// How long a queued refresh shows as in flight before this section gives up
/// on the device answering (it answers by re-reporting on its next beat —
/// the synced stamp moving is what really clears the mark). Web parity
/// (`REFRESH_PENDING_MS`).
const REFRESH_PENDING_SECS: i64 = 45;

/// EXP-817: the section's own refresh never re-tries one account faster than
/// this — a queued command the device has not answered yet is a CONFLICT on
/// the server, and hammering it buys nothing (web `AUTO_REFRESH_RETRY_MS`).
const AUTO_REFRESH_RETRY_SECS: i64 = 60;

/// The auto-refresh tick (web `useNow(30_000)`).
const AUTO_REFRESH_TICK_SECS: u64 = 30;

/// The device cap a device must advertise before a forced refresh is queued on
/// it (`coding::doctor::DEVICE_CAPS`, web `deviceCanRefreshUsage`).
const REFRESH_CAP: &str = "agent-usage-refresh";

/// One refresh in flight, keyed by [`AgentAccountUsageGroup::key`].
struct RefreshMark {
    /// The usage stamp the account carried when the refresh was queued: the
    /// device's re-report MOVES it, and that is what clears the mark.
    fetched_at: Option<String>,
    /// Epoch second it was queued — the [`REFRESH_PENDING_SECS`] give-up clock.
    at: i64,
}

/// EXP-862 — what a chip's menu has to know about the DEVICE it names: who
/// owns it, whether it is listening, and which of the three account commands
/// its build takes. Everything here comes off the synced row.
#[derive(Clone, Default)]
struct DeviceFacts {
    /// The `devices` ROW id — what the Device settings dialog is keyed by.
    row_id: String,
    label: String,
    online: bool,
    mine: bool,
    /// This very install: its writes happen here, not over a command.
    own: bool,
    can_login: bool,
    can_switch: bool,
    can_remove: bool,
}

impl DeviceFacts {
    /// The same gate the Devices rows apply: mine, listening, and advertising
    /// `agent-login` (this very install always can — the login runs in a tab
    /// right here).
    fn actionable(&self) -> bool {
        self.mine && (self.own || (self.online && self.can_login))
    }
}

pub struct AccountsSection {
    nav: Entity<Navigation>,
    refreshing: HashMap<String, RefreshMark>,
    /// EXP-817: when the section's OWN refresh last tried each account.
    auto_attempts: HashMap<String, i64>,
    /// EXP-849: the agent TAB on screen (`None` = the first section this
    /// client sees). Tabs, not stacked headings: the two agents' accounts
    /// have nothing to say to each other, and codex's rows pushed claude's
    /// numbers — the ones a reader opens this page for — below the fold.
    agent_tab: Option<String>,
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
            agent_tab: None,
            _subscriptions: subscriptions,
        }
    }

    // -- data ----------------------------------------------------------------

    /// The page's account groups (attention-sorted) and the devices behind
    /// their chips. `None` while the `devices` shape is still Waiting — the
    /// page renders its loading note rather than "nothing reported yet".
    fn loaded(
        &self,
        cx: &mut gpui::App,
    ) -> Option<(Vec<AgentAccountUsageGroup>, HashMap<String, DeviceFacts>)> {
        // Before the collections are borrowed: resolving it caches a global.
        let own_device_id = queries::own_device_id(cx);
        let collections = sync::Store::global(cx).collections();
        let devices = collections.devices.read(cx);
        if !devices.is_ready() {
            return None;
        }
        // The USER id, not the per-instance account key (the EXP-642 trap).
        let me = queries::active_account(cx)?.user_id;
        let team = active_team_id(&self.nav, cx);
        // Web parity: my devices, plus the SERVERS shared with the team I am
        // looking at — a teammate's shared server belongs to the pages of the
        // teams it is shared with (FEED-33: possibly several), not to every
        // team I am a member of.
        let rows: Vec<domain::rows::DeviceRow> = devices
            .iter()
            .filter(|row| {
                row.user_id.as_deref() == Some(me.as_str())
                    || (row.is_server()
                        && team
                            .as_deref()
                            .is_some_and(|team| row.is_shared_with(team)))
            })
            .cloned()
            .collect();
        let now_ms = chrono::Utc::now().timestamp_millis();
        let facts: HashMap<String, DeviceFacts> = rows
            .iter()
            .map(|row| {
                let caps = row.cap_ids();
                let device_id = row.device_id.clone().unwrap_or_default();
                let online = crate::device_settings::row_is_online(row.last_seen_at.as_deref(), now_ms);
                let facts = DeviceFacts {
                    row_id: row.id.clone(),
                    label: row.label.clone().unwrap_or_default(),
                    online,
                    mine: row.user_id.as_deref() == Some(me.as_str()),
                    own: device_id == own_device_id,
                    can_login: caps.iter().any(|cap| cap == "agent-login"),
                    can_switch: caps
                        .iter()
                        .any(|cap| cap == coding::doctor::ACCOUNT_SWITCH_CAP),
                    can_remove: caps
                        .iter()
                        .any(|cap| cap == coding::doctor::ACCOUNT_REMOVE_CAP),
                };
                (device_id, facts)
            })
            .collect();
        let caps: HashMap<String, Vec<String>> = rows
            .iter()
            .map(|row| (row.device_id.clone().unwrap_or_default(), row.cap_ids()))
            .collect();
        let rows = agent_profile_usage_rows(&rows, &me, |seen| {
            crate::device_settings::row_is_online(seen, now_ms)
        });
        let groups = sort_account_groups_attention_first(account_usage_groups(rows, |row| {
            Self::can_refresh(row, &caps)
        }));
        Some((groups, facts))
    }

    /// The groups alone — the refresh round has no use for the chips' devices.
    fn groups(&self, cx: &mut gpui::App) -> Option<Vec<AgentAccountUsageGroup>> {
        self.loaded(cx).map(|(groups, _)| groups)
    }

    /// Agent sections in CONTRACT order (`CodingAgent::ALL`), anything else
    /// after it alphabetically — a section only exists once a device has
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

    /// Whether a row's device may run the refresh at all: one of MY devices,
    /// online, on a build that runs the command (web `deviceCanRefreshUsage`).
    fn can_refresh(row: &AgentProfileUsageRow, caps: &HashMap<String, Vec<String>>) -> bool {
        row.mine
            && row.online
            && caps
                .get(&row.device_id)
                .is_some_and(|caps| caps.iter().any(|cap| cap == REFRESH_CAP))
    }

    /// EXP-817: the page's own refresh round. Every account with an eligible
    /// device and a freshest report past the floor gets ONE refresh queued
    /// — never while one is in flight, never twice inside
    /// [`AUTO_REFRESH_RETRY_SECS`]. The floor is the device's own 429 budget,
    /// so this can never out-poll what the device allows itself.
    ///
    /// EXP-862: this is the ONLY refresh there is — the per-row button went
    /// with the wave, because the answer it gave most often was "too soon".
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
            self.refresh(group, cx);
        }
    }

    /// Queue `agent_usage_refresh` on the account's refresh target. The answer
    /// arrives as a synced `agent_usage` write, never as a command result — so
    /// the only local state is the in-flight mark. A refusal (a command still
    /// queued from the last round is a CONFLICT) is swallowed: nobody asked
    /// for this round, and the next tick simply looks again.
    fn refresh(&mut self, group: &AgentAccountUsageGroup, cx: &mut gpui::Context<Self>) {
        let Some(target) = group.refresh_target.as_ref() else {
            return;
        };
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let key = group.key.clone();
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
                .update(cx, |this, _| {
                    if result.is_err() {
                        this.refreshing.remove(&key);
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

    /// The `Studio · Personal` chip text: the device, plus the profile when
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

    /// What hovering a chip says: this device's OWN report age and state,
    /// and (EXP-818) whether the account is the ACTIVE login there.
    fn chip_tooltip(row: &AgentProfileUsageRow, now_epoch: i64) -> String {
        use coding::agent_accounts::Health;
        let mut parts = vec![if row.online { "online" } else { "offline" }.to_string()];
        if !row.signed_in {
            parts.push("not signed in".to_string());
        } else {
            // EXP-849: a revoked credential is the loudest thing a chip can
            // say — it looks signed in everywhere else.
            if row.health == Health::NeedsRelogin {
                parts.push("needs re-login".to_string());
            }
            if row.active {
                parts.push("active here".to_string());
            }
        }
        if row.unmonitored {
            parts.push("usage not tracked here".to_string());
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

    /// EXP-849/EXP-862: one device chip — the online dot, the device
    /// (· profile), a CHECK where the account is that device's ACTIVE login,
    /// the health badge when the credential there is broken, and the shared
    /// chip MENU when this client can act on that device.
    ///
    /// The menu is the ×4 rule ([`chip_actions`]) plus the way to that
    /// device's settings, so an account can be repaired where it is read.
    fn render_chip(
        index: usize,
        chip: usize,
        row: &AgentProfileUsageRow,
        facts: Option<&DeviceFacts>,
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
        let badge = crate::usage_bar::health_badge(row.health, cx);
        let id = ("accounts-chip", index * 64 + chip);
        let facts = facts.cloned().unwrap_or_default();
        let actions = if facts.actionable() {
            chip_actions(
                row.signed_in,
                row.health,
                row.active,
                &row.profile_id,
                facts.own || facts.can_switch,
                facts.can_remove,
            )
        } else {
            Vec::new()
        };
        // A chip of a device I own is always a way INTO that device; a
        // teammate's shared server is a statement.
        if actions.is_empty() && !facts.mine {
            return crate::surface::glass_pill(
                id,
                crate::surface::PillSize::Sm,
                crate::surface::PillMode::Readonly,
                cx,
            )
            .when(!row.online, |this| this.text_color(muted))
            .tooltip({
                let tooltip = tooltip.clone();
                move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                }
            })
            .child(dot)
            .child(label)
            .children(check)
            .children(badge)
            .into_any_element();
        }
        let account_label = SharedString::from(
            row.email
                .clone()
                .or_else(|| row.plan.clone())
                .unwrap_or_else(|| row.profile_label.clone()),
        );
        let device_id = row.device_id.clone();
        let device_label = SharedString::from(if facts.label.trim().is_empty() {
            row.device_id.clone()
        } else {
            facts.label.clone()
        });
        let profile_id = row.profile_id.clone();
        let agent = CodingAgent::parse(&row.agent);
        let own = facts.own;
        let row_id = facts.row_id.clone();
        crate::surface::glass_pill_button(id, crate::surface::PillSize::Sm, cx)
            .when(!row.online, |this| this.text_color(muted))
            .tooltip(tooltip)
            .child(dot)
            .child(label)
            .children(check)
            .children(badge)
            .dropdown_menu(move |menu, _window, cx| {
                let mut menu = menu;
                for action in &actions {
                    let Some(agent) = agent else {
                        continue;
                    };
                    let action = *action;
                    let device_id = device_id.clone();
                    let device_label = device_label.clone();
                    let profile_id = profile_id.clone();
                    let account_label = account_label.clone();
                    let click = move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                        match action {
                            ChipAction::SignIn => crate::agent_login::sign_in_on_device(
                                device_id.clone(),
                                device_label.clone(),
                                own,
                                agent,
                                coding::agent_login::LoginTarget::Profile(profile_id.clone()),
                                window,
                                cx,
                            ),
                            ChipAction::SetDefault => use_account_here(
                                device_id.clone(),
                                device_label.to_string(),
                                own,
                                agent,
                                profile_id.clone(),
                                window,
                                cx,
                            ),
                            ChipAction::Remove => remove_account(
                                device_id.clone(),
                                device_label.clone(),
                                own,
                                agent,
                                profile_id.clone(),
                                account_label.clone(),
                                window,
                                cx,
                            ),
                        }
                    };
                    menu = menu.item(match action {
                        ChipAction::Remove => crate::controls::danger_menu_item(
                            action.label(),
                            Icon::new(action.icon()),
                            cx,
                        )
                        .on_click(click),
                        _ => PopupMenuItem::new(action.label())
                            .icon(Icon::new(action.icon()))
                            .on_click(click),
                    });
                }
                if facts.mine && !row_id.is_empty() {
                    let row_id = row_id.clone();
                    menu = menu.item(
                        PopupMenuItem::new(DEVICE_SETTINGS)
                            .icon(Icon::new(registry::NAV_SETTINGS))
                            .on_click(move |_, window, cx| {
                                crate::device_settings::open(window, cx, row_id.clone());
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }

    /// EXP-862 — the bare "+" chip: sign this SAME account in on another of my
    /// devices ("Sign in on <device>", ×4). Absent when every device that
    /// could take it already holds the account.
    fn render_add_chip(
        index: usize,
        group: &AgentAccountUsageGroup,
        cx: &mut App,
    ) -> Option<gpui::AnyElement> {
        let agent = CodingAgent::parse(&group.agent)?;
        let held: Vec<String> = group
            .rows
            .iter()
            .map(|row| row.device_id.clone())
            .collect();
        let targets = crate::agent_login::add_account_devices(Some(agent), &held, cx);
        if targets.is_empty() {
            return None;
        }
        Some(
            crate::surface::glass_pill_button(
                ("accounts-add-chip", index),
                crate::surface::PillSize::Sm,
                cx,
            )
            .child(
                Icon::new(registry::UI_ADD)
                    .with_size(px(crate::surface::PillSize::Sm.glyph())),
            )
            .dropdown_menu(move |menu, _window, _cx| {
                let mut menu = menu;
                for device in &targets {
                    let device = device.clone();
                    menu = menu.item(crate::pickers::option_item(
                        SharedString::from(format!("Sign in on {}", device.label)),
                        Icon::new(device.icon()),
                        false,
                        move |window, cx| {
                            let target = device.add_account_target(agent);
                            crate::agent_login::sign_in_on_device(
                                device.device_id.clone(),
                                device.label.clone(),
                                device.own,
                                agent,
                                target,
                                window,
                                cx,
                            );
                        },
                    ));
                }
                menu
            })
            .into_any_element(),
        )
    }

    /// One account ROW (EXP-818: a flat row under the Accounts band, not a
    /// card in a grid): the identity line with its health badge, the device
    /// chips, the dense usage windows.
    fn render_row(
        &self,
        index: usize,
        group: &AgentAccountUsageGroup,
        facts: &HashMap<String, DeviceFacts>,
        now_epoch: i64,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let foreground = theme.foreground;
        let row_hover = theme.list_hover;
        let fresh = group
            .usage
            .as_ref()
            .is_some_and(|usage| is_fresh(&usage.fetched_at, now_epoch));
        // The "as of …" fallback: the numbers' own stamp, else when a device
        // last probed the account.
        let as_of = group
            .usage
            .as_ref()
            .map(|usage| usage.fetched_at.clone())
            .filter(|stamp| !stamp.is_empty())
            .or_else(|| group.checked_at.clone())
            .map(|stamp| as_of_label(&stamp, now_epoch))
            .filter(|line| !line.is_empty());
        let caption = SharedString::from(group_caption(group));
        let plan_tail = match (group.email.as_ref(), group.plan.as_ref()) {
            (Some(_), Some(plan)) => Some(SharedString::from(format!(" · {plan}"))),
            _ => None,
        };
        // EXP-849 (interface A): HEALTH — "Needs re-login" for a credential
        // the provider revoked (the CLI still reports signed in), "Signed out"
        // for a login nobody made. The ×4 strings and the one colour rule live
        // in [`crate::usage_bar::health_badge`]; the repair is in the chip's
        // own menu.
        let health_badge = crate::usage_bar::health_badge(group.health, cx);
        // EXP-862: a signed-in login nobody has probed yet is CHECKING, not
        // "no usage reported" (the same four states ×4).
        let state = usage_state(
            group.signed_in,
            !group.rows.is_empty() && group.rows.iter().all(|row| row.unmonitored),
            group.usage.as_ref(),
        );
        let caption_line = usage_caption(state, as_of.as_deref());

        let mut chips = h_flex().w_full().min_w_0().flex_wrap().gap_1();
        for (chip, row) in group.rows.iter().enumerate() {
            chips = chips.child(Self::render_chip(
                index,
                chip,
                row,
                facts.get(&row.device_id),
                now_epoch,
                cx,
            ));
        }
        chips = chips.children(Self::render_add_chip(index, group, cx));

        crate::surface::flat_row()
            .id(SharedString::from(format!("accounts-row-{}", group.key)))
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .gap_1p5()
            .px_3()
            .py_2p5()
            .cursor_pointer()
            .hover(move |this| this.bg(row_hover))
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
                            .text_color(foreground)
                            .child(div().min_w_0().truncate().child(caption))
                            .children(plan_tail.map(|tail| {
                                div()
                                    .flex_shrink_0()
                                    .font_weight(gpui::FontWeight::NORMAL)
                                    .text_color(muted.opacity(0.6))
                                    .child(tail)
                            })),
                    )
                    .children(health_badge),
            )
            .child(chips)
            .when_some(
                group
                    .usage
                    .as_ref()
                    .filter(|_| caption_line.is_none())
                    .cloned(),
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
                            .children(
                                (!fresh && !usage.stale)
                                    .then(|| {
                                        as_of.clone().map(|line| {
                                            div()
                                                .text_xs()
                                                .text_color(muted)
                                                .child(SharedString::from(line))
                                        })
                                    })
                                    .flatten(),
                            ),
                    )
                },
            )
            .children(caption_line.map(|line| {
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(line))
            }))
            .into_any_element()
    }
}

/// EXP-862 — the account row's identity line: WHO the account is — the email,
/// else the plan (an agent may report a provider, never an address), else the
/// login's own label.
///
/// Never a STATUS. A signed-out account used to title itself "Not signed in",
/// which said the same thing as the chip's badge one line down and buried the
/// only identifying thing the row had; the chip badge is the single signed-out
/// notice ×4 (Android `AgentAccountsRows.caption`).
fn group_caption(group: &AgentAccountUsageGroup) -> String {
    group
        .email
        .clone()
        .or_else(|| group.plan.clone())
        .or_else(|| group.rows.first().map(|row| row.profile_label.clone()))
        .unwrap_or_else(|| coding::agent_profiles::SYSTEM_LABEL.to_string())
}

/// The device row's menu entry AND the chip menus' way into it — one string
/// ×4 (it replaced "Edit" with EXP-862).
pub(crate) const DEVICE_SETTINGS: &str = "Device settings";

/// EXP-849 — "Set as default": make this login the device's DEFAULT for its
/// agent. Non-destructive — it moves a device-local pointer and signs nobody
/// out, which is why it is offered beside (never instead of) the sign-in.
///
/// This device writes the pointer directly; another of mine gets the
/// `agent_profile_use` command on its heartbeat and answers by re-reporting,
/// so the CHECK moves on the next beat either way.
pub(crate) fn use_account_here(
    device_id: String,
    device_label: String,
    own: bool,
    agent: CodingAgent,
    profile_id: String,
    window: &mut Window,
    cx: &mut App,
) {
    if own {
        // The SAME body the `agent_profile_use` command runs
        // (`coding::use_profile`): same signed-in check, same sentences, same
        // re-probe — so "Set as default" means one thing whether it was asked
        // for on this device or from another client.
        match crate::device_sync::use_agent_profile_here(agent, &profile_id, cx) {
            Ok(()) => window.push_notification(
                Notification::success(SharedString::from(format!(
                    "{} now runs as this account here.",
                    agent.label()
                ))),
                cx,
            ),
            Err(err) => {
                window.push_notification(Notification::error(SharedString::from(err)), cx)
            }
        }
        return;
    }
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    let handle = window.window_handle();
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move {
                api::devices::create_agent_profile_use_command(
                    &trpc,
                    &device_id,
                    agent.id(),
                    &profile_id,
                )
            })
            .await;
        let _ = handle.update(cx, |_, window, cx| match result {
            Ok(_) => window.push_notification(
                Notification::success(SharedString::from(format!(
                    "{device_label} will run {} as this account.",
                    agent.label()
                ))),
                cx,
            ),
            Err(err) => window.push_notification(
                Notification::error(SharedString::from(err.user_message())),
                cx,
            ),
        });
    })
    .detach();
}

/// EXP-862 — "Remove account": delete the DEVICE's copy of a login (the
/// profile's config dir, credentials included, and its index row). The ACCOUNT
/// is untouched — the device never runs `codex logout`, which would revoke it
/// server-wide — which is exactly what the confirm says.
///
/// Destructive, so it confirms first with the pinned ×4 sentence
/// ([`remove_account_confirm`]). This device removes it directly; another of
/// mine gets the `agent_profile_remove` command on its heartbeat and answers
/// by re-reporting, so the chip goes on the next beat either way.
#[allow(clippy::too_many_arguments)]
pub(crate) fn remove_account(
    device_id: String,
    device_label: SharedString,
    own: bool,
    agent: CodingAgent,
    profile_id: String,
    account_label: SharedString,
    window: &mut Window,
    cx: &mut App,
) {
    let handle = window.window_handle();
    let spec = crate::native_dialog::AlertSpec::new(
        "Remove account",
        remove_account_confirm(&account_label, &device_label),
        "Remove",
    )
    .ok_variant(ButtonVariant::Danger)
    .on_ok(move |window, cx| {
        if own {
            match crate::device_sync::remove_agent_profile_here(agent, &profile_id, cx) {
                Ok(()) => window.push_notification(
                    Notification::success(SharedString::from(format!(
                        "{account_label} removed from this device."
                    ))),
                    cx,
                ),
                Err(err) => {
                    window.push_notification(Notification::error(SharedString::from(err)), cx)
                }
            }
            return true;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            return true;
        };
        let device_id = device_id.clone();
        let profile_id = profile_id.clone();
        let device_label = device_label.clone();
        cx.spawn(async move |cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::devices::create_agent_profile_remove_command(
                        &trpc,
                        &device_id,
                        agent.id(),
                        &profile_id,
                    )
                })
                .await;
            let _ = handle.update(cx, |_, window, cx| match result {
                Ok(_) => window.push_notification(
                    Notification::success(SharedString::from(format!(
                        "{device_label} will remove this login."
                    ))),
                    cx,
                ),
                Err(err) => window.push_notification(
                    Notification::error(SharedString::from(err.user_message())),
                    cx,
                ),
            });
        })
        .detach();
        true
    });
    crate::native_dialog::open_alert(window, cx, spec);
}

impl Render for AccountsSection {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let now_epoch = chrono::Utc::now().timestamp();
        let muted = cx.theme().muted_foreground;
        let loaded = self.loaded(cx);
        if let Some((groups, _)) = loaded.as_ref() {
            self.prune_refreshing(groups, now_epoch);
        }

        // EXP-862: "+ Add account" — the "+ Add device" twin on every client
        // (pick a device, pick an agent, sign in there). The per-agent context
        // menu it replaced could only ever add an account on THIS machine.
        let add_account = crate::surface::glass_pill_button(
            "accounts-add",
            crate::surface::PillSize::Sm,
            cx,
        )
        .icon(registry::UI_ADD)
        .label("Add account")
        .on_click(|_: &ClickEvent, window, cx| {
            crate::agent_login::open_add_account_dialog(window, cx);
        })
        .into_any_element();
        // The section carries its OWN top spacing (the page column has no
        // `gap`), like the run sections it sits where.
        let mut column = v_flex().min_w_0().mt_6().child(
            crate::surface::glass_section_header("Accounts", Some(add_account), cx),
        );

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
            Some((groups, _)) if groups.is_empty() => {
                column = column.child(
                    div()
                        .px_1()
                        .py_2()
                        .text_xs()
                        .text_color(muted)
                        .child("No device has reported an agent account yet."),
                );
            }
            Some((groups, facts)) => {
                let sections = Self::sections(groups);
                // EXP-849: one TAB per agent — and only when there is more
                // than one, so a client seeing a single agent gets no chrome
                // it cannot act on. EXP-862: the shared segmented strip, with
                // the agent's brand mark beside its name.
                let active = self
                    .agent_tab
                    .clone()
                    .filter(|agent| sections.iter().any(|(id, _)| id == agent))
                    .or_else(|| sections.first().map(|(agent, _)| agent.clone()));
                if sections.len() > 1 {
                    let mut tabs = crate::controls::segmented(cx);
                    for (agent, _) in sections.iter() {
                        let selected = Some(agent) == active.as_ref();
                        let id = agent.clone();
                        let known = CodingAgent::parse(agent);
                        let label = known
                            .map(|agent| agent.label().to_string())
                            .unwrap_or_else(|| agent.clone());
                        tabs = tabs.child(
                            crate::controls::segmented_item(selected, cx)
                                .id(SharedString::from(format!("accounts-agent-tab-{agent}")))
                                .children(known.map(|agent| {
                                    Icon::new(crate::coding_selects::agent_icon(agent))
                                        .with_size(px(crate::surface::PillSize::Sm.glyph()))
                                }))
                                .child(SharedString::from(label))
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.agent_tab = Some(id.clone());
                                    cx.notify();
                                })),
                        );
                    }
                    column = column.child(div().px_1().pt_2().pb_1().child(tabs));
                }
                let mut index = 0;
                for (agent, section) in &sections {
                    if Some(agent) != active.as_ref() {
                        // The index still advances: element ids must not move
                        // between tabs (a reused id carries stale state).
                        index += section.len();
                        continue;
                    }
                    let mut block = v_flex().min_w_0().pb_1();
                    for group in section {
                        block = block.child(self.render_row(index, group, &facts, now_epoch, cx));
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
            health: coding::agent_accounts::Health::Ok,
            unmonitored: false,
        }
    }

    fn group(agent: &str, key: &str) -> AgentAccountUsageGroup {
        AgentAccountUsageGroup {
            key: key.to_string(),
            agent: agent.to_string(),
            signed_in: true,
            email: None,
            plan: None,
            health: coding::agent_accounts::Health::Ok,
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
            group("zed", "zed:a"),
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
            vec![("claude", 1), ("codex", 2), ("zed", 2)]
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
    fn chips_name_the_device_and_a_named_profile() {
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

    /// EXP-862: the row TITLES the account, never its state — the email, else
    /// the bare plan, else the login's own label. A signed-out account keeps
    /// its identity; the chip's badge is what says "Signed out".
    #[test]
    fn the_row_caption_is_the_identity_never_the_status() {
        let mut named = group("claude", "claude:dev@acme.test");
        named.email = Some("dev@acme.test".into());
        named.plan = Some("max".into());
        assert_eq!(group_caption(&named), "dev@acme.test");

        let mut plan_only = group("codex", "codex:a");
        plan_only.plan = Some("plus".into());
        assert_eq!(group_caption(&plan_only), "plus");

        let mut signed_out = group("claude", "claude:b");
        signed_out.signed_in = false;
        signed_out.health = coding::agent_accounts::Health::SignedOut;
        signed_out.rows[0].signed_in = false;
        signed_out.rows[0].profile_label = "Work".into();
        assert_eq!(group_caption(&signed_out), "Work");
        // The badge is the notice, and it still has one.
        assert_eq!(signed_out.health.badge_label(), Some("Signed out"));

        // Nothing at all to go on: the ambient login's own label.
        let mut bare = group("claude", "claude:c");
        bare.rows.clear();
        assert_eq!(group_caption(&bare), coding::agent_profiles::SYSTEM_LABEL);
    }
}
