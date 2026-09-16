//! "My devices" (EXP-403): the signed-in user's registered devices —
//! desktops and headless `exponential` daemon servers — rendered as a
//! section on the Devices page, so all four clients surface the same device
//! registry.
//!
//! The rows come from the SYNCED `devices` shape and nothing else (EXP-485):
//! online-ness is `last_seen_at` freshness against the contract window — no
//! relay presence involved — and every mutation lands back through sync, so
//! there is no poll and no fallback list to keep in step.
//!
//! EXP-832: the shape deltas land every few seconds (one per heartbeat of
//! every device the caller can see), so the rows are DERIVED on the observer
//! and cached — jsonb account maps parsed once per change, not once per
//! frame — and a derivation equal to the one on screen never notifies.
//!
//! The one tRPC call left is `devices.latestVersions`, the informational
//! `CLIENT_LATEST_VERSION_*` pair behind the amber update nudge. That is
//! instance config, not device state: it is fetched ONCE per section
//! lifetime (the first render) rather than polled.
//!
//! EXP-849/EXP-862/EXP-909: this is THE surface for agent logins. Each device
//! row carries the worst health among the logins it reported and lists those
//! logins beneath itself, one flat sub-row each: the brand mark, the login's
//! identity (`usage_bar::login_label`), its health badge, its mini usage line
//! and a `⋯` menu — the ×4 rule (`usage_bar::chip_actions`, the shared login
//! menu): sign in, set as
//! default (`agent_profile_use`: a device-local pointer, never a logout and
//! never a credential copy), remove this device's copy of the login
//! (`agent_profile_remove`). A team device's logins are read-only: they belong
//! to their owner.
//!
//! EXP-909: a device row carries ONE control — a settings GEAR revealed on
//! hover, opening [`crate::device_settings`]. No ▶ (a run starts from the
//! Agent page composer, which preselects a device), no ⋯ row menu, and no
//! inline update controls: Update and Remove are sections of that dialog.
//!
//! EXP-909 folded the cross-device Accounts section (EXP-818) INTO these rows.
//! One account held by three machines is three logins on three machines —
//! which is what a person repairing one needs to see — so the email-merged
//! groups, their per-agent tabs and their device chips are gone, and with them
//! the page's only place where a login was not under its machine. The 30 s
//! refresh round the section ran came along.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, App, ClipboardItem, InteractiveElement as _, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    ActiveTheme as _, Icon, Sizable as _, WindowExt as _,
};

use crate::agent_account_actions::DEVICE_SETTINGS;
use crate::controls::{WebControl as _, WebText as _};
use crate::icons::registry;
use crate::native_dialog::{self, AlertSpec};
use crate::queries;
use crate::usage_bar::{chip_actions, AgentProfileUsageRow, ChipAction};

/// EXP-832: how often the section re-derives its rows without a shape delta.
/// Online-ness is a CLOCK question (`last_seen_at` against the contract
/// window), so a device that simply stops beating has to age out on a tick;
/// the derivation is compared before it notifies, so an unchanged list costs
/// one pass and no frame.
const LIVENESS_TICK: std::time::Duration = std::time::Duration::from_secs(10);

/// EXP-909: the hover group a device row shares with its ONE control — the
/// settings gear only renders under the pointer (`drafts_view`'s idiom).
const MACHINE_ROW_GROUP: &str = "machine-row";

/// EXP-909 (moved from the retired Accounts section): how long a queued usage
/// refresh shows as in flight before the list gives up on the device
/// answering — it answers by RE-REPORTING on its next beat, and the synced
/// stamp moving is what really clears the mark. Web parity
/// (`REFRESH_PENDING_MS`).
const REFRESH_PENDING_SECS: i64 = 45;

/// The list's own refresh never re-tries one login faster than this: a
/// command the device has not answered yet is a CONFLICT on the server, and
/// hammering it buys nothing (web `AUTO_REFRESH_RETRY_MS`).
const AUTO_REFRESH_RETRY_SECS: i64 = 60;

/// The auto-refresh tick (web `useNow(30_000)`).
const AUTO_REFRESH_TICK_SECS: u64 = 30;

/// One refresh in flight, keyed by [`AgentProfileUsageRow::key`].
struct RefreshMark {
    /// The usage stamp the login carried when the refresh was queued: the
    /// device's re-report MOVES it, and that is what clears the mark.
    fetched_at: Option<String>,
    /// Epoch second it was queued — the [`REFRESH_PENDING_SECS`] give-up clock.
    at: i64,
}

/// EXP-832 — one device row, fully derived: every string the row renders and
/// its logins already derived out of the row's `agent_accounts` jsonb.
/// `PartialEq` is what keeps a heartbeat that changed nothing off the frame.
#[derive(Clone, Debug, PartialEq)]
struct DeviceCard {
    /// The `devices` ROW id — what the Device settings dialog is keyed by.
    row_id: String,
    device_id: String,
    /// The name, falling back to the id for a device that never sent one.
    label: SharedString,
    /// The headless daemon (the only kind that self-updates).
    server: bool,
    online: bool,
    /// This very install.
    own: bool,
    /// Mine, as opposed to a teammate's shared server.
    mine: bool,
    /// EXP-525: a teammate's row keeps the attribution in its tooltip.
    owner_tooltip: Option<SharedString>,
    /// An OWN device shared with at least one team.
    shared: bool,
    is_default: bool,
    version: Option<String>,
    update_requested: bool,
    update_blocked: bool,
    last_seen_at: Option<String>,
    /// Runnable agents (signed in) and installed-but-signed-out ones — the
    /// Accounts sub-rows read both (EXP-909 retired the row's ▶ gate).
    agents: Vec<String>,
    unauthed_agents: Vec<String>,
    caps: Vec<String>,
    /// EXP-909: the logins this device holds, one sub-row each, already in
    /// the ×4 order ([`crate::usage_bar::sort_device_logins`]).
    logins: Vec<AgentProfileUsageRow>,
    /// Whether the device has reported its `agentAccounts` map AT ALL. An
    /// empty map means "no login here" ([`crate::usage_bar::
    /// NO_LOGIN_REPORTED`]); a missing one means the device simply has not
    /// answered yet ("Checking…"), and the two must not read the same.
    accounts_reported: bool,
    /// The WORST health among them — the row's badge.
    health: coding::agent_accounts::Health,
}

impl DeviceCard {
    fn has_cap(&self, cap: &str) -> bool {
        self.caps.iter().any(|have| have == cap)
    }

    /// The same gate the login menus apply: mine, and either this very
    /// install (the login runs in a tab right here) or an online device on a
    /// build that takes the command.
    fn actionable(&self) -> bool {
        self.mine && (self.own || (self.online && self.has_cap("agent-login")))
    }
}

/// The two groups the section renders, in the EXP-623 stable order.
#[derive(Clone, Debug, Default, PartialEq)]
struct DerivedDevices {
    mine: Vec<DeviceCard>,
    team: Vec<DeviceCard>,
}

pub(crate) struct MachinesSection {
    latest: api::devices::LatestVersions,
    /// The one-shot guard for [`Self::ensure_latest_loaded`].
    latest_requested: bool,
    /// EXP-832: the rows on screen. `None` while the shape is still Waiting.
    derived: Option<DerivedDevices>,
    /// EXP-909: the usage refreshes in flight, keyed by login row key.
    refreshing: std::collections::HashMap<String, RefreshMark>,
    /// When the list's OWN refresh last tried each login.
    auto_attempts: std::collections::HashMap<String, i64>,
    _subscriptions: Vec<gpui::Subscription>,
}

impl MachinesSection {
    pub(crate) fn new(_window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        // The rows stream over the devices shape — re-derive on every delta,
        // and notify only when the derivation actually moved.
        let devices_collection = sync::Store::global(cx).collections().devices.clone();
        let subscriptions = vec![cx.observe(&devices_collection, |this: &mut Self, _, cx| {
            this.refresh_cards(cx);
        })];
        // Online-ness ages out on a clock, not on a delta.
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(LIVENESS_TICK).await;
            if this.update(cx, |this, cx| this.refresh_cards(cx)).is_err() {
                return;
            }
        })
        .detach();
        // EXP-909: the usage refresh round the Accounts section used to run —
        // a coarse clock (and one pass right away), like the web's `useNow`.
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
            latest: api::devices::LatestVersions::default(),
            latest_requested: false,
            derived: None,
            refreshing: std::collections::HashMap::new(),
            auto_attempts: std::collections::HashMap::new(),
            _subscriptions: subscriptions,
        }
    }

    /// EXP-909 — whether a login's device may run the refresh at all: one of
    /// MY devices, online, on a build that runs the command (web
    /// `deviceCanRefreshUsage`). Pure, so the rule is unit-tested rather than
    /// asserted by reading the render.
    fn can_refresh(row: &AgentProfileUsageRow, caps: &[String]) -> bool {
        row.mine
            && row.online
            && caps
                .iter()
                .any(|cap| cap == crate::usage_bar::USAGE_REFRESH_CAP)
    }

    /// Drop the in-flight marks the synced rows have answered (the stamp
    /// moved) or that have waited past [`REFRESH_PENDING_SECS`]. A pure
    /// prune, so it never notifies.
    fn prune_refreshing(&mut self, logins: &[&AgentProfileUsageRow], now_epoch: i64) {
        self.refreshing.retain(|key, mark| {
            let Some(row) = logins.iter().find(|row| &row.key == key) else {
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

    /// EXP-909 (EXP-817's round, moved here with the logins): every login
    /// whose device may run the command and whose numbers are past the
    /// device's own floor gets ONE `agent_usage_refresh` queued — never while
    /// one is in flight, never twice inside [`AUTO_REFRESH_RETRY_SECS`]. The
    /// floor is the device's own 429 budget, so this can never out-poll what
    /// the device allows itself.
    fn auto_refresh(&mut self, cx: &mut gpui::Context<Self>) {
        if self.derived.is_none() {
            self.derived = Self::derive(cx);
        }
        let Some(derived) = self.derived.clone() else {
            return;
        };
        let cards: Vec<&DeviceCard> = derived.mine.iter().chain(derived.team.iter()).collect();
        let logins: Vec<&AgentProfileUsageRow> =
            cards.iter().flat_map(|card| card.logins.iter()).collect();
        let now_epoch = chrono::Utc::now().timestamp();
        self.prune_refreshing(&logins, now_epoch);
        for card in &cards {
            for row in &card.logins {
                if !Self::can_refresh(row, &card.caps) || self.refreshing.contains_key(&row.key) {
                    continue;
                }
                if crate::usage_bar::refresh_allowed_at(row.usage.as_ref(), now_epoch).is_some() {
                    continue;
                }
                let last = self
                    .auto_attempts
                    .get(&row.key)
                    .copied()
                    .unwrap_or(i64::MIN);
                if now_epoch.saturating_sub(last) < AUTO_REFRESH_RETRY_SECS {
                    continue;
                }
                self.auto_attempts.insert(row.key.clone(), now_epoch);
                self.refresh(row, cx);
            }
        }
    }

    /// Queue `agent_usage_refresh` on one login's device. The answer arrives
    /// as a synced `agent_usage` write, never as a command result — so the
    /// only local state is the in-flight mark. A refusal (a command still
    /// queued from the last round is a CONFLICT) is swallowed: nobody asked
    /// for this round, and the next tick simply looks again.
    fn refresh(&mut self, row: &AgentProfileUsageRow, cx: &mut gpui::Context<Self>) {
        let Some(trpc) = queries::trpc_client(cx) else {
            return;
        };
        let key = row.key.clone();
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
            // The give-up tick: the mark is pruned on the next render past
            // the window, so the list needs ONE notify to get there even if
            // no heartbeat lands meanwhile.
            cx.background_executor()
                .timer(std::time::Duration::from_secs(
                    REFRESH_PENDING_SECS as u64 + 1,
                ))
                .await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    // -- data ----------------------------------------------------------------

    /// EXP-832: re-derive the rows, and notify only if they CHANGED. Every
    /// heartbeat of every visible device is a delta, so the equality check is
    /// what keeps the page from repainting a dozen times a minute.
    fn refresh_cards(&mut self, cx: &mut gpui::Context<Self>) {
        let derived = Self::derive(cx);
        if derived == self.derived {
            return;
        }
        self.derived = derived;
        cx.notify();
    }

    /// Fetch `devices.latestVersions` once per section lifetime. The values
    /// are instance config (they change on a deploy, not on a heartbeat), so
    /// a failure simply means no update nudge until the section is rebuilt —
    /// far better than a render-driven retry storm.
    fn ensure_latest_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        if self.latest_requested {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            // No client yet — try again on a later render.
            return;
        };
        self.latest_requested = true;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { api::devices::latest_versions(&trpc) })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(latest) => this.latest = latest,
                    Err(err) => log::warn!("[ui] devices.latestVersions failed: {err}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// EXP-481/EXP-832: the rows from the SYNCED devices shape as
    /// [`DeviceCard`]s — `(mine, team)`, each group in the EXP-623 stable
    /// order (online-by-label first, so heartbeats can't reorder the list;
    /// offline rows don't beat, so last-seen desc is stable there). EXP-642
    /// keeps the two groups apart: they render as their own headed sections,
    /// web parity. `None` while the shape is Waiting (cold start / old
    /// server) — the section renders its loading note.
    fn derive(cx: &mut App) -> Option<DerivedDevices> {
        // Before the collections are borrowed: resolving it caches a global.
        let own_device_id = crate::queries::own_device_id(cx);
        let collections = sync::Store::global(cx).collections();
        let devices = collections.devices.read(cx);
        if !devices.is_ready() {
            return None;
        }
        // The USER id, not the account key: `devices.user_id` is the Better Auth
        // user id, `Account.id` is the per-instance account key (EXP-642 — the
        // old comparison never matched, which the My/Team split made visible).
        let me = crate::queries::active_account(cx)?.user_id;
        let users = collections.users.read(cx);
        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut derived = DerivedDevices::default();
        for row in devices.iter() {
            let owned = row.user_id.as_deref() == Some(me.as_str());
            let owner_tooltip = (!owned)
                .then(|| {
                    row.user_id
                        .as_deref()
                        .and_then(|user_id| users.get(user_id))
                        .and_then(|user| user.name.clone())
                })
                .flatten()
                .filter(|name| !name.is_empty())
                .map(|name| SharedString::from(format!("Shared by {name}")));
            let device_id = row.device_id.clone().unwrap_or_default();
            let label = row.label.clone().unwrap_or_default();
            let update_requested = row.update_requested_at.is_some();
            let accounts = crate::device_settings::parse_agent_map::<coding::AgentAccount>(
                row.agent_accounts.as_ref(),
            );
            let card = DeviceCard {
                row_id: row.id.clone(),
                label: SharedString::from(if label.trim().is_empty() {
                    device_id.clone()
                } else {
                    label
                }),
                own: device_id == own_device_id,
                mine: owned,
                owner_tooltip,
                server: row.is_server(),
                online: crate::device_settings::row_is_online(row.last_seen_at.as_deref(), now_ms),
                last_seen_at: row.last_seen_at.clone(),
                version: row.version.clone(),
                update_requested,
                update_blocked: update_requested && row.active_sessions.unwrap_or(0) > 0,
                // EXP-642: only an OWN row wears the chip — the Team devices
                // section is shared by definition.
                shared: owned && !row.shared_team_ids.is_empty(),
                // EXP-622: a teammate's flag is THEIR preference, never ours.
                is_default: owned && row.is_default.unwrap_or(false),
                agents: row.agent_ids(),
                unauthed_agents: row.unauthed_agent_ids(),
                caps: row.cap_ids(),
                health: coding::agent_accounts::worst_health(&accounts),
                // EXP-909: ONE device's logins, through the same per-profile
                // derivation the rest of the app reads (the usage rows), so a
                // login says the same thing here as in the run's own sheet.
                logins: crate::usage_bar::sort_device_logins(
                    crate::usage_bar::agent_profile_usage_rows(
                        std::slice::from_ref(row),
                        &me,
                        |last_seen| {
                            crate::device_settings::row_is_online(last_seen, now_ms)
                        },
                    ),
                ),
                accounts_reported: row
                    .agent_accounts
                    .as_ref()
                    .is_some_and(|value| !value.is_null()),
                device_id,
            };
            if owned {
                derived.mine.push(card);
            } else {
                derived.team.push(card);
            }
        }
        // Online first, then by label so heartbeats can't reorder the online
        // group; offline rows don't beat, so last-seen desc (ISO stamps order
        // lexicographically) is stable there.
        let stable_order = |a: &DeviceCard, b: &DeviceCard| {
            b.online
                .cmp(&a.online)
                .then_with(|| {
                    if a.online {
                        std::cmp::Ordering::Equal
                    } else {
                        b.last_seen_at.cmp(&a.last_seen_at)
                    }
                })
                .then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase()))
                .then_with(|| a.device_id.cmp(&b.device_id))
        };
        derived.mine.sort_by(stable_order);
        derived.team.sort_by(stable_order);
        Some(derived)
    }

    // -- render --------------------------------------------------------------

    fn render_row(
        &self,
        index: usize,
        device: &DeviceCard,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let label = device.label.clone();
        let kind_icon = if device.server {
            registry::UI_SERVER
        } else {
            registry::UI_DEVICE
        };
        // Informational only: the row nudges, the device decides.
        let latest = if device.server {
            self.latest.cli.as_deref()
        } else {
            self.latest.desktop.as_deref()
        };
        let outdated = update_available(device.version.as_deref(), latest);
        // EXP-909: the request lives on the SYNCED row (the dialog writes it);
        // the row only reports it.
        let updating = device.update_requested;
        // EXP-411: the request is parked behind live sessions on the device —
        // "Update queued" instead of an indefinite "Updating…".
        let queued = device.update_requested && device.update_blocked;
        // EXP-909: the row carries ONE control — the settings gear, revealed
        // on hover (the drafts list's idiom: the group lives on the row, the
        // reveal on the button). Starting a run on a device is the composer's
        // device picker, not a row affordance; a teammate's shared row has
        // nothing here to rename, remove or update, so it carries no control
        // at all — not even a spacer.
        let settings_row_id = device.row_id.clone();
        let gear = device.mine.then(|| {
            div()
                .invisible()
                .group_hover(MACHINE_ROW_GROUP, |style| style.visible())
                .flex_shrink_0()
                .child(
                    // EXP-862: a row action is GHOST chrome, never a circle.
                    crate::controls::ghost_icon_button(
                        ("machine-settings", index),
                        Icon::new(registry::NAV_SETTINGS),
                        cx,
                    )
                    .tooltip(DEVICE_SETTINGS)
                    .on_click(move |_, window, cx| {
                        crate::device_settings::open(window, cx, settings_row_id.clone());
                    }),
                )
        });

        // EXP-642: one row per device, the web `GlassRow` two-line shape —
        // icon · (name · version · default star · "Shared") over the status
        // line · the gear — `min_w_0` down the name side so only the NAME
        // gives way.
        let row_hover = theme.list_hover;
        let line = crate::surface::flat_row()
            .id(SharedString::from(format!("machine-{}", device.device_id)))
            .group(MACHINE_ROW_GROUP)
            .flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_3()
            .px_3()
            .py_2p5()
            .cursor_pointer()
            .hover(move |this| this.bg(row_hover))
            .child(
                div()
                    .flex_shrink_0()
                    .child(Icon::new(kind_icon).xsmall().text_color(muted)),
            )
            .child(
                gpui_component::v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .child(
                        gpui_component::h_flex()
                            .w_full()
                            .min_w_0()
                            .items_center()
                            .gap_1p5()
                            // Web keeps the version ADJACENT to the name, so the name
                            // is fit-content — a `truncate` here would render at the
                            // EXP-175 collapsed width (it needs `flex_1` on the
                            // ellipsis div itself); the cluster clips instead.
                            .overflow_hidden()
                            .child(
                                div()
                                    .id(("machine-name", index))
                                    .flex_shrink_0()
                                    .text_sm()
                                    .whitespace_nowrap()
                                    .text_color(theme.foreground)
                                    .when_some(device.owner_tooltip.clone(), |this, owner| {
                                        this.tooltip(move |window, cx| {
                                            gpui_component::tooltip::Tooltip::new(owner.clone())
                                                .build(window, cx)
                                        })
                                    })
                                    .child(label.clone()),
                            )
                            // EXP-696: the device this app IS — the same muted
                            // caption the "Shared" chip uses, so the row line
                            // keeps one rhythm.
                            .when(device.own, |this| {
                                this.child(
                                    div()
                                        .flex_shrink_0()
                                        .px_1()
                                        .rounded(px(theme::tokens::radius::SM))
                                        .border_1()
                                        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                                        .text_xs()
                                        .text_color(muted)
                                        .child("This device"),
                                )
                            })
                            // EXP-622: the device every picker prefills.
                            .when(device.is_default, |this| {
                                this.child(
                                    Icon::new(registry::UI_DEVICE_DEFAULT)
                                        .xsmall()
                                        .flex_shrink_0()
                                        .text_color(muted),
                                )
                            })
                            .when_some(device.version.clone(), |this, version| {
                                // Stateful: the outdated hint rides a tooltip, and
                                // `tooltip` lives on gpui's STATEFUL interactive trait.
                                let hint: SharedString =
                                    format!("Update available: v{}", latest.unwrap_or_default())
                                        .into();
                                this.child(
                                    div()
                                        .id(("machine-version", index))
                                        .flex_shrink_0()
                                        .text_xs()
                                        .text_color(if outdated {
                                            theme::tokens::YELLOW.to_hsla()
                                        } else {
                                            muted
                                        })
                                        .when(outdated, |this| {
                                            this.tooltip(move |window, cx| {
                                                gpui_component::tooltip::Tooltip::new(hint.clone())
                                                    .build(window, cx)
                                            })
                                        })
                                        .child(SharedString::from(format!("v{version}"))),
                                )
                            })
                            // EXP-642 (web parity): an own device shared with a team
                            // says so on the name line.
                            .when(device.shared, |this| {
                                this.child(
                                    div()
                                        .flex_shrink_0()
                                        .px_1()
                                        .rounded(px(theme::tokens::radius::SM))
                                        .border_1()
                                        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                                        .text_xs()
                                        .text_color(muted)
                                        .child("Shared"),
                                )
                            }),
                    )
                    .child(
                        gpui_component::h_flex()
                            .w_full()
                            .min_w_0()
                            .items_center()
                            .gap_1p5()
                            .text_xs()
                            .text_color(muted)
                            .when(device.online, |this| {
                                this.child(
                                    div()
                                        .size_1p5()
                                        .flex_shrink_0()
                                        .rounded_full()
                                        .bg(theme::tokens::GREEN.to_hsla()),
                                )
                            })
                            .child(SharedString::from(status_line(device)))
                            // EXP-849: the WORST health among the device's
                            // logins — a revoked credential leaves the device
                            // looking fine while every run on it fails at the
                            // first request. One badge, the ×4 strings; the
                            // repair is in the login row's own menu (EXP-909).
                            .children(crate::usage_bar::health_badge(device.health, cx))
                            .when(updating, |this| {
                                this.child(div().child(if queued {
                                    "Update queued"
                                } else {
                                    "Updating…"
                                }))
                            }),
                    ),
            )
            // EXP-909: ONE trailing control — the gear, on my own rows only.
            // The ▶/⋯ pair and the 32px placeholder that kept them lined up
            // are gone with them, so a team row ends at its status line.
            .children(gear);
        // EXP-909: the device's own logins hang UNDER its line, so a login is
        // always read beside the machine that holds it.
        gpui_component::v_flex()
            .w_full()
            .min_w_0()
            .child(line)
            .children(self.render_login_rows(index, device, cx))
            .into_any_element()
    }

    /// EXP-909 — the logins one device holds, as flat sub-rows under its
    /// device line: the agent's brand mark, the login's IDENTITY
    /// ([`crate::usage_bar::login_label`] — never its status), the plan
    /// behind it when both are known, the health badge, and the `⋯` menu
    /// carrying the ×4 rule ([`chip_actions`]). Under that, the mini usage
    /// line, or the caption ladder for a login with no numbers yet.
    ///
    /// A device that has not reported its accounts says "Checking…"; one that
    /// reported none says so ([`crate::usage_bar::NO_LOGIN_REPORTED`]) — the
    /// two are different facts and must not read alike. A teammate's shared
    /// device is READ-ONLY: its logins belong to its owner, so they carry no
    /// menu and no add row.
    fn render_login_rows(
        &self,
        index: usize,
        device: &DeviceCard,
        cx: &mut gpui::Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let muted = cx.theme().muted_foreground;
        let now_epoch = chrono::Utc::now().timestamp();
        // The sub-rows hang under the device NAME, not under its icon.
        let indent = |element: gpui::Div| element.w_full().min_w_0().pl_9().pr_3();
        if device.logins.is_empty() {
            let line = if device.accounts_reported {
                crate::usage_bar::NO_LOGIN_REPORTED
            } else {
                // The ×4 "signed in, nothing read yet" caption — a machine
                // still working must not read as a broken one.
                "Checking…"
            };
            let mut rows = vec![indent(div())
                .pb_1()
                .text_xs()
                .text_color(muted)
                .child(line)
                .into_any_element()];
            rows.extend(self.render_add_account_row(index, device, cx));
            return rows;
        }
        let actionable = device.actionable();
        let can_switch = device.own || device.has_cap(coding::doctor::ACCOUNT_SWITCH_CAP);
        let can_remove = device.has_cap(coding::doctor::ACCOUNT_REMOVE_CAP);
        let mut rows: Vec<gpui::AnyElement> = Vec::new();
        for (slot, login) in device.logins.iter().enumerate() {
            let Some(agent) = coding::CodingAgent::parse(&login.agent) else {
                continue;
            };
            let label = SharedString::from(crate::usage_bar::login_label(login));
            // The plan rides BEHIND an address only: a label that already is
            // the plan must not print it twice.
            let plan_tail = match (login.email.as_ref(), login.plan.as_ref()) {
                (Some(_), Some(plan)) => Some(SharedString::from(format!(" · {plan}"))),
                _ => None,
            };
            let badge = crate::usage_bar::health_badge(login.health, cx);
            let actions = if actionable {
                chip_actions(
                    login.signed_in,
                    login.health,
                    login.active,
                    &login.profile_id,
                    can_switch,
                    can_remove,
                )
            } else {
                Vec::new()
            };
            let menu = (!actions.is_empty()).then(|| {
                let device_id = device.device_id.clone();
                let device_label = device.label.clone();
                let own = device.own;
                let profile_id = login.profile_id.clone();
                let account_label = label.clone();
                crate::controls::ghost_icon_button(
                    ("machine-login-menu", index * 64 + slot),
                    Icon::new(registry::UI_MORE),
                    cx,
                )
                .dropdown_menu(move |menu, _window, cx| {
                    let mut menu = menu;
                    for action in &actions {
                        let action = *action;
                        let device_id = device_id.clone();
                        let device_label = device_label.clone();
                        let profile_id = profile_id.clone();
                        let account_label = account_label.clone();
                        let click = move |_: &gpui::ClickEvent,
                                          window: &mut Window,
                                          cx: &mut App| match action {
                            ChipAction::SignIn => crate::agent_login::sign_in_on_device(
                                device_id.clone(),
                                device_label.clone(),
                                own,
                                agent,
                                coding::agent_login::LoginTarget::Profile(profile_id.clone()),
                                window,
                                cx,
                            ),
                            ChipAction::SetDefault => {
                                crate::agent_account_actions::use_account_here(
                                    device_id.clone(),
                                    device_label.to_string(),
                                    own,
                                    agent,
                                    profile_id.clone(),
                                    window,
                                    cx,
                                )
                            }
                            ChipAction::Remove => crate::agent_account_actions::remove_account(
                                device_id.clone(),
                                device_label.clone(),
                                own,
                                agent,
                                profile_id.clone(),
                                account_label.clone(),
                                window,
                                cx,
                            ),
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
                    menu
                })
            });
            // The numbers: the mini line when there are any, else the ×4
            // caption ladder ("Checking…" for a login nothing has read yet,
            // "No usage reported · as of …" for one that reports none).
            let as_of = login
                .usage
                .as_ref()
                .map(|usage| usage.fetched_at.clone())
                .filter(|stamp| !stamp.is_empty())
                .or_else(|| login.checked_at.clone())
                .map(|stamp| crate::usage_bar::as_of_label(&stamp, now_epoch))
                .filter(|line| !line.is_empty());
            let state = crate::usage_bar::usage_state(
                login.signed_in,
                login.unmonitored,
                login.usage.as_ref(),
            );
            let caption = crate::usage_bar::usage_caption(state, as_of.as_deref());
            let age = crate::usage_bar::usage_age(login.usage.as_ref(), now_epoch);
            let mini = caption
                .is_none()
                .then(|| {
                    login
                        .usage
                        .as_ref()
                        .and_then(|usage| crate::usage_bar::render_usage_mini(usage, cx))
                })
                .flatten();
            let numbers = gpui_component::h_flex()
                .w_full()
                .min_w_0()
                .items_center()
                .gap_2()
                .when(age.is_some(), |this| this.opacity(0.5))
                .children(mini.map(|mini| div().flex_1().min_w_0().child(mini)))
                .children(caption.map(|line| {
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_xs()
                        .text_color(muted)
                        .child(SharedString::from(line))
                }))
                .children(age.map(|line| {
                    div()
                        .flex_shrink_0()
                        .text_2xs()
                        .text_color(muted)
                        .child(SharedString::from(line))
                }));
            rows.push(
                indent(gpui_component::h_flex())
                    .id(("machine-login", index * 64 + slot))
                    .items_center()
                    .gap_2()
                    .py_1()
                    .when(!device.online, |this| this.opacity(0.6))
                    .child(
                        div().flex_shrink_0().child(
                            crate::coding_selects::agent_mark(agent)
                                .with_size(px(crate::surface::PillSize::Sm.glyph())),
                        ),
                    )
                    .child(
                        gpui_component::v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_0p5()
                            .child(
                                gpui_component::h_flex()
                                    .w_full()
                                    .min_w_0()
                                    .items_center()
                                    .gap_1p5()
                                    .text_xs()
                                    .child(
                                        gpui_component::h_flex()
                                            .flex_1()
                                            .min_w_0()
                                            .items_center()
                                            .child(div().min_w_0().truncate().child(label))
                                            .children(plan_tail.map(|tail| {
                                                div()
                                                    .flex_shrink_0()
                                                    .text_color(muted)
                                                    .child(tail)
                                            })),
                                    )
                                    .children(badge),
                            )
                            .child(numbers),
                    )
                    .child(match menu {
                        Some(menu) => div().flex_shrink_0().child(menu),
                        None => div()
                            .flex_shrink_0()
                            .w(px(theme::tokens::size::CONTROL_MD)),
                    })
                    .into_any_element(),
            );
        }
        rows.extend(self.render_add_account_row(index, device, cx));
        rows
    }

    /// EXP-909 — "Add account" under a device's logins: sign in with ANOTHER
    /// account on THAT machine (the dialog is device-bound — it keeps the
    /// agent picker and drops the device picker).
    ///
    /// This REVERSES EXP-845's visible-but-disabled control: the row is only
    /// rendered where the sign-in can actually happen — one of MY machines,
    /// listening, on a build that runs `agent_login`, with at least one agent
    /// installed there — and is simply absent otherwise. A control that can
    /// only ever explain why it is dead is worse than no control; a
    /// teammate's shared device never gets one at all.
    fn render_add_account_row(
        &self,
        index: usize,
        device: &DeviceCard,
        _cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let installed = !device.agents.is_empty() || !device.unauthed_agents.is_empty();
        if !device.actionable() || !installed {
            return None;
        }
        let device_id = device.device_id.clone();
        Some(
            div()
                .w_full()
                .min_w_0()
                .pl_9()
                .pr_3()
                .pb_1()
                .child(
                    Button::new(("machine-add-account", index))
                        .ghost()
                        .web_xs()
                        .icon(Icon::new(registry::UI_ADD))
                        .label("Add account")
                        .on_click(move |_, window, cx| {
                            crate::agent_login::open_add_account_dialog_for(
                                device_id.clone(),
                                window,
                                cx,
                            );
                        }),
                )
                .into_any_element(),
        )
    }
}

/// Where the desktop app's builds live — the "Download desktop app" target.
const DESKTOP_RELEASES_URL: &str = "https://github.com/Niach/exponential/releases/latest";

/// The instance the install one-liner points the daemon at.
fn server_install_origin(cx: &gpui::App) -> String {
    queries::active_account(cx)
        .map(|account| account.instance_url.trim_end_matches('/').to_string())
        .unwrap_or_else(|| "https://app.exponential.at".to_string())
}

/// The headless-daemon install one-liner (web `buildServerInstallSnippet`) —
/// the Add-device dialog shows it, the Getting-started server card copies it.
pub(crate) fn server_install_snippet(cx: &gpui::App) -> String {
    let origin = server_install_origin(cx);
    format!("curl -fsSL https://exponential.at/install.sh | EXP_INSTANCE={origin} sh")
}

/// The install one-liner in its copyable box (EXP-725 — extracted so the
/// Add-device dialog and the onboarding wizard's devices step render the SAME
/// box). The clipboard gets the ONE-LINE command; the box shows it wrapped
/// over two lines so the snippet never needs a horizontal scroll.
pub(crate) fn server_install_snippet_box(
    id: impl Into<gpui::ElementId>,
    cx: &gpui::App,
) -> impl IntoElement {
    let origin = server_install_origin(cx);
    let snippet = server_install_snippet(cx);
    let line_two = SharedString::from(format!("  EXP_INSTANCE={origin} sh"));
    div()
        .relative()
        .p_2()
        .pr_8()
        .rounded(px(theme::tokens::radius::SM))
        .border_1()
        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
        .text_xs()
        .font_family(theme::terminal::FONT_FAMILY)
        .text_color(cx.theme().foreground)
        .child(
            gpui_component::v_flex()
                .child("curl -fsSL https://exponential.at/install.sh |")
                .child(line_two),
        )
        .child(
            div().absolute().top_1().right_1().child(
                // EXP-862: a copy glyph on a row is ghost chrome, not a circle.
                crate::controls::ghost_icon_button(id, Icon::new(registry::UI_COPY), cx)
                    .tooltip("Copy install command")
                    .on_click(move |_, window, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(snippet.clone()));
                        window
                            .push_notification(Notification::success("Copied install command"), cx);
                    }),
            ),
        )
}

/// The "Add device" dialog (EXP-697, one spec with the web twin): the desktop
/// app first — it is what actually runs coding sessions — then the install
/// one-liner for the headless `exponential` CLI as the always-on-server path.
/// The script is served by the CLOUD marketing site for every instance
/// (self-hosted ships no marketing pages), so the snippet always names the
/// target instance explicitly via `EXP_INSTANCE` — the web
/// `buildServerInstallSnippet` shape exactly. Shared (EXP-470): opened from
/// this section's band and from the Getting-started page's server card.
pub(crate) fn open_add_server_dialog(window: &mut Window, cx: &mut gpui::App) {
    let spec = AlertSpec::new(
        "Add device",
        "To run coding sessions, install the desktop app.",
        "Done",
    )
    .without_cancel()
    .height(px(320.))
    .content(move |_, cx| {
        gpui_component::v_flex()
            .gap_3()
            .child(
                gpui_component::h_flex().child(
                    Button::new("add-device-download")
                        .outline()
                        .web_sm()
                        .icon(Icon::new(registry::UI_DOWNLOAD))
                        .label("Download desktop app")
                        .on_click(|_, _, cx| {
                            crate::settings::open_url(cx, DESKTOP_RELEASES_URL.to_string());
                        }),
                ),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Or install the Exponential CLI on a server:"),
            )
            .child(server_install_snippet_box("add-device-copy", cx))
            .into_any_element()
    });
    native_dialog::open_alert(window, cx, spec);
}

/// `Online` / `Last seen 5m` / `Offline` — the web row's caption, in the
/// desktop's relative-time wording.
///
/// EXP-862: nothing about sign-ins any more. "claude not signed in" said what
/// the chip's own badge below already says, and it was the loudest thing on a
/// row that was otherwise fine.
fn status_line(device: &DeviceCard) -> String {
    if device.online {
        return "Online".to_string();
    }
    match device.last_seen_at.as_deref().map(crate::inbox::relative_time) {
        Some(seen) if !seen.is_empty() => format!("Last seen {seen}"),
        _ => "Offline".to_string(),
    }
}

/// The web row's amber nudge: this device's version compares below its
/// platform's `CLIENT_LATEST_VERSION_*`. Unknown on either side = no nudge.
pub(crate) fn update_available(version: Option<&str>, latest: Option<&str>) -> bool {
    match (version, latest) {
        (Some(version), Some(latest)) => crate::update::is_newer(latest, version),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(online: bool, last_seen: Option<&str>) -> DeviceCard {
        DeviceCard {
            row_id: "row-1".to_string(),
            device_id: "dev-1".to_string(),
            label: "Studio".into(),
            server: false,
            online,
            own: false,
            mine: true,
            owner_tooltip: None,
            shared: false,
            is_default: false,
            version: None,
            update_requested: false,
            update_blocked: false,
            last_seen_at: last_seen.map(str::to_string),
            agents: Vec::new(),
            unauthed_agents: Vec::new(),
            caps: Vec::new(),
            logins: Vec::new(),
            accounts_reported: false,
            health: coding::agent_accounts::Health::Unknown,
        }
    }

    fn login_row(mine: bool, online: bool) -> AgentProfileUsageRow {
        AgentProfileUsageRow {
            key: "dev-1:claude:system".to_string(),
            device_id: "dev-1".to_string(),
            device_label: "Studio".to_string(),
            mine,
            online,
            agent: "claude".to_string(),
            profile_id: crate::usage_bar::SYSTEM_PROFILE_ID.to_string(),
            profile_label: "Default".to_string(),
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

    #[test]
    fn status_line_reads_online_last_seen_or_offline() {
        assert_eq!(status_line(&device(true, None)), "Online");
        assert!(status_line(&device(false, Some("2020-01-01T00:00:00Z")))
            .starts_with("Last seen "));
        assert_eq!(status_line(&device(false, None)), "Offline");
        // A timestamp the parser rejects must never render a dangling
        // "Last seen " with no time after it.
        assert_eq!(status_line(&device(false, Some("garbage"))), "Offline");
    }

    /// EXP-862: the status line says nothing about sign-ins — a device whose
    /// only agent is signed out still reads "Online" (its chip carries the
    /// "Signed out" badge, once).
    #[test]
    fn status_line_never_names_signed_out_agents() {
        let mut nothing_runnable = device(true, None);
        nothing_runnable.unauthed_agents = vec!["claude".to_string()];
        assert_eq!(status_line(&nothing_runnable), "Online");

        let mut partly = device(true, None);
        partly.agents = vec!["codex".to_string()];
        partly.unauthed_agents = vec!["claude".to_string()];
        assert_eq!(status_line(&partly), "Online");

        let mut offline = device(false, None);
        offline.unauthed_agents = vec!["claude".to_string()];
        assert_eq!(status_line(&offline), "Offline");
    }

    #[test]
    fn update_nudge_needs_both_versions() {
        assert!(update_available(Some("0.4.1"), Some("0.5.0")));
        assert!(!update_available(Some("0.5.0"), Some("0.5.0")));
        // A device ahead of the informational value (a dev build) is fine.
        assert!(!update_available(Some("0.6.0"), Some("0.5.0")));
        assert!(!update_available(None, Some("0.5.0")));
        assert!(!update_available(Some("0.4.1"), None));
    }

    /// EXP-909 (ported from the retired Accounts section): the refresh round
    /// only ever queues on MY machine, only while it is online, and only on a
    /// build that advertises the command (web `deviceCanRefreshUsage`).
    #[test]
    fn refresh_needs_my_own_online_machine_with_the_cap() {
        let caps = vec![crate::usage_bar::USAGE_REFRESH_CAP.to_string()];
        assert!(MachinesSection::can_refresh(&login_row(true, true), &caps));
        // A teammate's shared server is not mine to poll.
        assert!(!MachinesSection::can_refresh(&login_row(false, true), &caps));
        // Offline: the command would sit queued until it came back.
        assert!(!MachinesSection::can_refresh(&login_row(true, false), &caps));
        // An older build that never advertised the cap.
        assert!(!MachinesSection::can_refresh(&login_row(true, true), &[]));
        assert!(!MachinesSection::can_refresh(
            &login_row(true, true),
            &["agent-login".to_string()]
        ));
    }

    /// EXP-832: a heartbeat that changes nothing must not repaint the list —
    /// the derivation compares equal.
    #[test]
    fn derived_rows_compare_equal_when_nothing_moved() {
        let one = DerivedDevices {
            mine: vec![device(true, None)],
            team: Vec::new(),
        };
        let two = DerivedDevices {
            mine: vec![device(true, None)],
            team: Vec::new(),
        };
        assert_eq!(one, two);
        let moved = DerivedDevices {
            mine: vec![device(false, None)],
            team: Vec::new(),
        };
        assert_ne!(one, moved);
    }
}

impl Render for MachinesSection {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.ensure_latest_loaded(cx);
        // First frame: the observer only fires on a DELTA, so the derivation
        // has to happen here once.
        if self.derived.is_none() {
            self.derived = Self::derive(cx);
        }

        let muted = cx.theme().muted_foreground;
        let derived = self.derived.clone();
        let (mine, team) = match derived.as_ref() {
            Some(derived) => (derived.mine.as_slice(), derived.team.as_slice()),
            None => (&[][..], &[][..]),
        };
        let mine_rows: Vec<gpui::AnyElement> = mine
            .iter()
            .enumerate()
            .map(|(index, device)| self.render_row(index, device, cx))
            .collect();
        let team_rows: Vec<gpui::AnyElement> = team
            .iter()
            .enumerate()
            // Offset the element ids so the two groups can never collide.
            .map(|(index, device)| self.render_row(index + mine.len(), device, cx))
            .collect();

        // EXP-642: the web `GlassSectionHeader` — a plain-text heading with
        // no count, the "Add device" control trailing (EXP-697: the dialog
        // leads with the desktop app, so the button no longer says server).
        let add_server = crate::surface::glass_pill_button(
            "machines-add-server",
            crate::surface::PillSize::Sm,
            cx,
        )
        .icon(registry::UI_ADD)
        .label("Add device")
        .on_click(|_: &gpui::ClickEvent, window, cx| {
            open_add_server_dialog(window, cx);
        })
        .into_any_element();

        // NO `w_full` (EXP-508): as a child of the page's centered column, a
        // percent width resolves against the UNCLAMPED ancestor available
        // width and shrink-wraps the section at wide windows (the EXP-436
        // leak). Auto width + the column's flex-col stretch size it to the
        // capped column width; the heading/rows below a stretch-sized parent
        // resolve their `w_full` correctly.
        gpui_component::v_flex()
            .min_w_0()
            .child(crate::surface::glass_section_header(
                "My devices",
                Some(add_server),
                cx,
            ))
            .when(derived.is_none(), |this| {
                this.child(
                    div()
                        .px_1()
                        .py_2()
                        .text_xs()
                        .text_color(muted)
                        .child("Loading devices…"),
                )
            })
            .when(derived.is_some() && mine.is_empty(), |this| {
                this.child(
                    div()
                        .px_1()
                        .py_2()
                        .text_xs()
                        .text_color(muted)
                        .child("No devices yet. Open the Exponential desktop app, or add a device."),
                )
            })
            .child(gpui_component::v_flex().min_w_0().children(mine_rows))
            // EXP-432/642: teammates' shared devices get their OWN headed
            // section, exactly like the web page.
            .when(!team.is_empty(), |this| {
                this.child(
                    gpui_component::v_flex()
                        .min_w_0()
                        .pt_4()
                        .child(crate::surface::glass_section_header(
                            "Team devices",
                            None,
                            cx,
                        ))
                        .child(gpui_component::v_flex().min_w_0().children(team_rows)),
                )
            })
    }
}
