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
//! menu): sign in, sign out, remove this device's copy of the login
//! (`agent_profile_remove`). EXP-1158: there is no "default" to set — every
//! start runs on the login last used on that device. A team device's logins are read-only: they belong
//! to their owner.
//!
//! EXP-909: a device row carries ONE control — a settings GEAR revealed on
//! hover, opening [`crate::device_settings`]. No ▶ (a run starts from the
//! Agent page composer, which preselects a device), no ⋯ row menu, and no
//! inline update controls: Update and Remove are sections of that dialog.
//!
//! FEED-149: a teammate's shared row carries no control, except for an OWNER
//! of the current team: the gear's slot then holds Remove, which drops that
//! member's share of the machine from this team (`devices.dropShare`).
//!
//! EXP-909 folded the cross-device Accounts section (EXP-818) INTO these rows.
//! One account held by three machines is three logins on three machines —
//! which is what a person repairing one needs to see — so the email-merged
//! groups, their per-agent tabs and their device chips are gone, and with them
//! the page's only place where a login was not under its machine. The 30 s
//! refresh round the section ran came along.
//!
//! A-16 (`fixtures/devices-list.json`, one spec ×4): the row is ONE line and
//! nothing folds. The logins are CHIPS on that line (brand mark + identity,
//! the account picker's limits preview on hover, the login menu on click);
//! nothing warns inline — the problems (dead or signed-out logins, an update)
//! ride ONE attention icon whose hover card lists them, each with its one
//! pill action. Only login PROFILES count: an agent CLI installed but never
//! signed in badges nothing ([`device_attention`]). Online rows lead, the
//! offline ones follow dimmed under a muted "Offline" label.

use crate::controls::PointerMenuButtonExt as _;
use gpui::prelude::FluentBuilder as _;
use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    div, px, App, AppContext as _, Entity, InteractiveElement as _, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    menu::{PopupMenuItem},
    ActiveTheme as _, Disableable as _, Icon, Sizable as _,
};

use crate::agent_account_actions::DEVICE_SETTINGS;
use crate::icons::registry;
use crate::native_dialog::{self, AlertSpec};
use crate::navigation::{nav_for_window, Navigation};
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

/// A-16: the widest the chip cluster grows before it wraps onto a second
/// line, so a machine with many logins never squeezes its NAME out.
const CHIPS_MAX_W: f32 = 600.;

/// A-16: the attention card's width (web `w-80`, minus its padding).
const ATTENTION_CARD_W: f32 = 300.;

/// A-16: the attention card's hover delays (web `HoverCard` 100 / 150 ms).
const ATTENTION_OPEN_DELAY: std::time::Duration = std::time::Duration::from_millis(100);
const ATTENTION_CLOSE_DELAY: std::time::Duration = std::time::Duration::from_millis(150);

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
pub(crate) struct DeviceCard {
    /// The `devices` ROW id — what the Device settings dialog is keyed by.
    pub(crate) row_id: String,
    pub(crate) device_id: String,
    /// The name, falling back to the id for a device that never sent one.
    pub(crate) label: SharedString,
    /// The headless daemon (the only kind that self-updates).
    pub(crate) server: bool,
    /// EXP-924: the owner-picked glyph, verbatim — resolved at render time
    /// through [`crate::icons::device_icon`] (NULL = the kind default).
    pub(crate) icon: Option<String>,
    pub(crate) online: bool,
    /// This very install.
    pub(crate) own: bool,
    /// Mine, as opposed to a teammate's shared server.
    pub(crate) mine: bool,
    /// EXP-525: a teammate's row keeps the attribution in its tooltip.
    pub(crate) owner_tooltip: Option<SharedString>,
    /// FEED-149: the teams a TEAMMATE's row is shared into — what gates a
    /// team owner's Remove. Empty on my own rows (their sharing lives in
    /// Device settings).
    pub(crate) shared_team_ids: Vec<String>,
    /// An OWN device shared with at least one team.
    pub(crate) shared: bool,
    pub(crate) is_default: bool,
    pub(crate) version: Option<String>,
    pub(crate) update_requested: bool,
    pub(crate) update_blocked: bool,
    pub(crate) last_seen_at: Option<String>,
    /// Runnable agents (signed in) and installed-but-signed-out ones — the
    /// Accounts sub-rows read both (EXP-909 retired the row's ▶ gate).
    pub(crate) agents: Vec<String>,
    pub(crate) unauthed_agents: Vec<String>,
    /// EXP-909: the logins this device holds, one chip each, already in
    /// the ×4 order ([`crate::usage_bar::sort_device_logins`]).
    pub(crate) logins: Vec<AgentProfileUsageRow>,
    /// Whether the device has reported its `agentAccounts` map AT ALL. An
    /// empty map means "no login here" ([`crate::usage_bar::
    /// NO_LOGIN_REPORTED`]); a missing one means the device simply has not
    /// answered yet ("Checking…"), and the two must not read the same.
    pub(crate) accounts_reported: bool,
}

impl DeviceCard {
    /// The same gate the login menus apply: mine, and either this very
    /// install (the login runs in a tab right here) or an online device.
    fn actionable(&self) -> bool {
        self.mine && (self.own || self.online)
    }

    /// A-16: the update request is parked behind live sessions (EXP-411).
    fn update_queued(&self) -> bool {
        self.update_requested && self.update_blocked
    }

    /// A-16: the row's [`device_attention`] input. `latest` is the instance's
    /// `CLIENT_LATEST_VERSION_*` for this device's kind.
    pub(crate) fn list_input(&self, latest: Option<&str>) -> DeviceListInput {
        DeviceListInput {
            online: self.online,
            mine: self.mine,
            server: self.server,
            // Every row of mine came off the `devices` shape: registered.
            registered: self.mine,
            outdated: update_available(self.version.as_deref(), latest),
            latest: latest.map(str::to_string),
            update_queued: self.update_queued(),
            accounts_reported: self.accounts_reported,
            logins: self
                .logins
                .iter()
                .map(|login| DeviceListLogin {
                    agent: login.agent.clone(),
                    profile_id: login.profile_id.clone(),
                    label: crate::usage_bar::login_label(login),
                    health: login.health,
                })
                .collect(),
        }
    }
}

// -- A-16: the row's attention, pure --------------------------------------

/// The fixture's `copy`, byte-locked by `device_list_copy_matches_the_fixture`.
pub(crate) const COPY_MINE: &str = "My devices";
pub(crate) const COPY_TEAM: &str = "Team devices";
pub(crate) const COPY_OFFLINE: &str = "Offline";
pub(crate) const COPY_NO_ACCOUNT: &str = "No account";
pub(crate) const COPY_CHECKING: &str = "Checking…";
pub(crate) const COPY_ADD_ACCOUNT: &str = "Add account";
pub(crate) const COPY_ATTENTION: &str = "Needs attention";
const COPY_UPDATE_AVAILABLE: &str = "Update available · v{version}";
const COPY_UPDATE_QUEUED: &str = "Update queued: restarts within 10 minutes.";
const COPY_NEEDS_RELOGIN: &str = "{label} · Needs re-login";
const COPY_SIGNED_OUT: &str = "{label} · Signed out";
pub(crate) const COPY_UPDATE: &str = "Update";
pub(crate) const COPY_SIGN_IN: &str = "Sign in";

/// One login PROFILE a device reported (web `DeviceListLogin`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DeviceListLogin {
    pub(crate) agent: String,
    pub(crate) profile_id: String,
    /// [`crate::usage_bar::login_label`] — the email, else the plan.
    pub(crate) label: String,
    pub(crate) health: coding::agent_accounts::Health,
}

/// What [`device_attention`] reads off one row (web `DeviceListInput`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DeviceListInput {
    pub(crate) online: bool,
    /// The caller's own machine (not a teammate's shared server).
    pub(crate) mine: bool,
    /// A headless CLI server: the only kind that acts on an update request.
    pub(crate) server: bool,
    pub(crate) registered: bool,
    /// The client's own version compare said the device is behind `latest`.
    pub(crate) outdated: bool,
    pub(crate) latest: Option<String>,
    pub(crate) update_queued: bool,
    /// The machine sent its accounts at least once.
    pub(crate) accounts_reported: bool,
    /// Every login PROFILE it reported; an agent with none adds nothing.
    pub(crate) logins: Vec<DeviceListLogin>,
}

/// What one attention line is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AttentionKind {
    Login { agent: String, profile_id: String },
    Update,
    UpdateQueued,
}

/// The one pill an attention line may carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AttentionAction {
    SignIn,
    Update,
}

impl AttentionAction {
    pub(crate) fn label(self) -> &'static str {
        match self {
            AttentionAction::SignIn => COPY_SIGN_IN,
            AttentionAction::Update => COPY_UPDATE,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AttentionItem {
    pub(crate) kind: AttentionKind,
    pub(crate) text: String,
    pub(crate) action: Option<AttentionAction>,
}

/// Web `DeviceListRow`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DeviceListRow {
    /// The `online` group (else `offline`, under its muted label).
    pub(crate) online: bool,
    /// Reported accounts, holds no login: "No account" in the chip slot.
    pub(crate) no_account: bool,
    pub(crate) attention: Vec<AttentionItem>,
}

/// A-16 — THE row derivation, mirrored ×4 (web `deviceListRow`, iOS / Android
/// `DevicesList`) and replayed against every case of `devices-list.json`.
/// Items in order: every `needs_relogin` login, every `signed_out` login,
/// then the update line (the caller's own device only). An agent with no
/// login PROFILE is not an item: it is not in `logins` at all.
pub(crate) fn device_attention(input: &DeviceListInput) -> DeviceListRow {
    use coding::agent_accounts::Health;
    let mut attention = Vec::new();
    let can_sign_in = input.mine && input.online;
    for (health, template) in [
        (Health::NeedsRelogin, COPY_NEEDS_RELOGIN),
        (Health::SignedOut, COPY_SIGNED_OUT),
    ] {
        for login in input.logins.iter().filter(|login| login.health == health) {
            attention.push(AttentionItem {
                kind: AttentionKind::Login {
                    agent: login.agent.clone(),
                    profile_id: login.profile_id.clone(),
                },
                text: template.replace("{label}", &login.label),
                action: can_sign_in.then_some(AttentionAction::SignIn),
            });
        }
    }
    if input.mine {
        if input.update_queued {
            attention.push(AttentionItem {
                kind: AttentionKind::UpdateQueued,
                text: COPY_UPDATE_QUEUED.to_string(),
                action: None,
            });
        } else if let Some(latest) = input.latest.as_deref().filter(|_| input.outdated) {
            attention.push(AttentionItem {
                kind: AttentionKind::Update,
                text: COPY_UPDATE_AVAILABLE.replace("{version}", latest),
                action: (input.server && input.online && input.registered)
                    .then_some(AttentionAction::Update),
            });
        }
    }
    DeviceListRow {
        online: input.online,
        no_account: input.accounts_reported && input.logins.is_empty(),
        attention,
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
    /// A-16: the device ids whose `devices.requestUpdate` (the attention
    /// card's Update pill) is in flight.
    updating: std::collections::HashSet<String>,
    /// FEED-149: the window's navigation — the current team a team owner's
    /// Remove drops a share from.
    nav: Entity<Navigation>,
    /// FEED-149: the `devices` row ids whose `dropShare` is in flight.
    dropping: std::collections::HashSet<String>,
    _subscriptions: Vec<gpui::Subscription>,
}

impl MachinesSection {
    pub(crate) fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        // The rows stream over the devices shape — re-derive on every delta,
        // and notify only when the derivation actually moved.
        let devices_collection = sync::Store::global(cx).collections().devices.clone();
        // FEED-149: the owner's Remove reads the current team and my role in
        // it at render time, so a team switch or a role change repaints.
        let members_collection = sync::Store::global(cx).collections().team_members.clone();
        let subscriptions = vec![
            cx.observe(&devices_collection, |this: &mut Self, _, cx| {
                this.refresh_cards(cx);
            }),
            cx.observe(&nav, |_, _, cx| cx.notify()),
            cx.observe(&members_collection, |_, _, cx| cx.notify()),
        ];
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
            updating: std::collections::HashSet::new(),
            nav,
            dropping: std::collections::HashSet::new(),
            _subscriptions: subscriptions,
        }
    }

    /// FEED-149: the team a teammate's shared row can be removed from — the
    /// current team, when I OWN it. `None` = no row carries the control.
    fn owned_team_id(&self, cx: &App) -> Option<String> {
        crate::navigation::active_team_id(&self.nav, cx)
            .filter(|team_id| crate::settings::is_owner(cx, team_id))
    }

    /// FEED-149: Remove behind a confirm (the prompts fixture's
    /// `remove-device-share`).
    fn prompt_drop_share(
        &mut self,
        device: &DeviceCard,
        team_id: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let view = cx.entity().downgrade();
        let row_id = device.row_id.clone();
        let spec = AlertSpec::from_prompt(
            "Remove device",
            &domain::prompts::remove_device_share(&device.label),
        )
        .on_ok(move |_, cx| {
            if let Some(view) = view.upgrade() {
                let (row_id, team_id) = (row_id.clone(), team_id.clone());
                view.update(cx, |this, cx| this.drop_share(row_id, team_id, cx));
            }
            true
        });
        native_dialog::open_alert(window, cx, spec);
    }

    /// FEED-149: `devices.dropShare`. Success needs no handling of its own —
    /// the row leaves the Team devices list through the synced shape; a
    /// failure is the standard failed-action toast.
    fn drop_share(&mut self, row_id: String, team_id: String, cx: &mut gpui::Context<Self>) {
        if !self.dropping.insert(row_id.clone()) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            self.dropping.remove(&row_id);
            return;
        };
        cx.spawn(async move |this, cx| {
            let id = row_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move { api::devices::drop_share(&trpc, &id, &team_id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.dropping.remove(&row_id);
                if let Err(err) = result {
                    log::warn!("[ui] devices.dropShare failed: {err}");
                    crate::action_error::failed_action_err("remove", "the device", &err, cx);
                }
            });
        })
        .detach();
    }

    /// EXP-909 — whether a login's device may run the refresh at all: one of
    /// MY devices, online (web `deviceCanRefreshUsage`). Pure, so the rule is
    /// unit-tested rather than asserted by reading the render.
    fn can_refresh(row: &AgentProfileUsageRow) -> bool {
        row.mine && row.online
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
                if !Self::can_refresh(row) || self.refreshing.contains_key(&row.key) {
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
                shared_team_ids: if owned {
                    Vec::new()
                } else {
                    row.shared_team_ids.clone()
                },
                server: row.is_server(),
                icon: row.icon.clone(),
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

    // -- actions -------------------------------------------------------------

    /// A-16: the attention card's Update pill on a SERVER —
    /// `devices.requestUpdate` (the Device settings Update section's own
    /// request; the daemon picks it up off its next heartbeat). The synced
    /// `update_requested_at` is what the row reads afterwards; a failure is
    /// the standard failed-action toast.
    fn request_update(&mut self, device_id: String, cx: &mut gpui::Context<Self>) {
        if !self.updating.insert(device_id.clone()) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else {
            self.updating.remove(&device_id);
            return;
        };
        cx.notify();
        cx.spawn(async move |this, cx| {
            let id = device_id.clone();
            let result = cx
                .background_executor()
                .spawn(async move { api::devices::request_update(&trpc, &id, false) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.updating.remove(&device_id);
                if let Err(err) = result {
                    log::warn!("[ui] devices.requestUpdate failed: {err}");
                    crate::action_error::failed_action_err("update", "the device", &err, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    // -- render --------------------------------------------------------------

    /// A-16: one row's [`DeviceRowProps`] — the derivation ([`device_attention`])
    /// plus the handlers its attention pills and the owner's Remove run.
    fn row_props(
        &self,
        index: usize,
        device: &DeviceCard,
        owned_team_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) -> DeviceRowProps {
        // Informational only: the row nudges, the device decides.
        let latest = if device.server {
            self.latest.cli.clone()
        } else {
            self.latest.desktop.clone()
        };
        let derived = device_attention(&device.list_input(latest.as_deref()));
        let view = cx.entity().downgrade();
        let attention = derived
            .attention
            .iter()
            .map(|item| {
                let action = item.action.map(|action| {
                    let (busy, on_click): (bool, RowHandler) = match (action, &item.kind) {
                        (AttentionAction::SignIn, AttentionKind::Login { agent, profile_id }) => {
                            let agent = coding::CodingAgent::parse(agent);
                            let device_id = device.device_id.clone();
                            let device_label = device.label.clone();
                            let own = device.own;
                            let profile_id = profile_id.clone();
                            (
                                false,
                                Rc::new(move |window: &mut Window, cx: &mut App| {
                                    let Some(agent) = agent else {
                                        return;
                                    };
                                    crate::agent_login::sign_in_on_device(
                                        device_id.clone(),
                                        device_label.clone(),
                                        own,
                                        agent,
                                        Some(profile_id.clone()),
                                        window,
                                        cx,
                                    );
                                }),
                            )
                        }
                        // `update` is a SERVER's only (devices-list.json): the
                        // CLI daemon acts on the request (Device settings'
                        // Update); a desktop app updates itself and its line
                        // carries no pill.
                        _ => {
                            let view = view.clone();
                            let device_id = device.device_id.clone();
                            (
                                device.update_requested || self.updating.contains(&device.device_id),
                                Rc::new(move |_: &mut Window, cx: &mut App| {
                                    let device_id = device_id.clone();
                                    let _ = view.update(cx, |this, cx| this.request_update(device_id, cx));
                                }),
                            )
                        }
                    };
                    AttentionPill {
                        label: action.label(),
                        busy,
                        on_click: Some(on_click),
                    }
                });
                AttentionLine {
                    text: SharedString::from(item.text.clone()),
                    action,
                }
            })
            .collect();
        // FEED-149: a team OWNER's Remove on a teammate's row shared into the
        // current team.
        let on_drop_share = owned_team_id
            .filter(|team_id| !device.mine && device.shared_team_ids.iter().any(|id| id == team_id))
            .map(|team_id| {
                let team_id = team_id.to_string();
                let target = device.clone();
                let view = view.clone();
                Rc::new(move |window: &mut Window, cx: &mut App| {
                    let _ = view.update(cx, |this, cx| {
                        this.prompt_drop_share(&target, team_id.clone(), window, cx)
                    });
                }) as RowHandler
            });
        DeviceRowProps {
            index,
            device: device.clone(),
            latest,
            attention,
            no_account: derived.no_account,
            inert: false,
            on_drop_share,
        }
    }

    /// A-16: one headed group — the online rows in the list's own order, then
    /// the muted "Offline" label over the offline ones (dimmed by the row).
    fn render_group(
        &self,
        offset: usize,
        devices: &[DeviceCard],
        owned_team_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Div {
        let muted = cx.theme().muted_foreground;
        let mut online = Vec::new();
        let mut offline = Vec::new();
        for (index, device) in devices.iter().enumerate() {
            let row = device_row(self.row_props(offset + index, device, owned_team_id, cx), cx);
            if device.online {
                online.push(row);
            } else {
                offline.push(row);
            }
        }
        gpui_component::v_flex()
            .min_w_0()
            .children(online)
            .when(!offline.is_empty(), |this| {
                this.child(
                    div()
                        .px_3()
                        .pt_3()
                        .pb_1()
                        .text_xs()
                        .text_color(muted)
                        .child(COPY_OFFLINE),
                )
                .children(offline)
            })
    }
}

/// A row control's click, built where the state lives and run by the row.
pub(crate) type RowHandler = Rc<dyn Fn(&mut Window, &mut App)>;

/// A-16: one attention line's pill. `on_click: None` = inert (the gallery).
#[derive(Clone)]
pub(crate) struct AttentionPill {
    pub(crate) label: &'static str,
    /// The request is already in flight: the pill disables.
    pub(crate) busy: bool,
    pub(crate) on_click: Option<RowHandler>,
}

/// A-16: one line of the attention card — its text and at most one pill.
#[derive(Clone)]
pub(crate) struct AttentionLine {
    pub(crate) text: SharedString,
    pub(crate) action: Option<AttentionPill>,
}

/// A-16: everything [`device_row`] draws. A plain value, so the styleguide's
/// `device-attention` entry draws the REAL row.
pub(crate) struct DeviceRowProps {
    pub(crate) index: usize,
    pub(crate) device: DeviceCard,
    /// This kind's `CLIENT_LATEST_VERSION_*` (the version's tooltip).
    pub(crate) latest: Option<String>,
    pub(crate) attention: Vec<AttentionLine>,
    pub(crate) no_account: bool,
    /// The gallery: every control drawn, none wired (no login menu, no gear,
    /// no Add account dialog).
    pub(crate) inert: bool,
    /// FEED-149: a team owner's Remove, in the gear's slot.
    pub(crate) on_drop_share: Option<RowHandler>,
}

/// A-16 — THE Devices row: ONE line — kind glyph · name (version, default
/// star, This device / Shared tags) over the status line · the login chips ·
/// the attention icon · the hover-revealed gear. Nothing folds and nothing
/// warns inline; an offline row is dimmed.
pub(crate) fn device_row(props: DeviceRowProps, cx: &App) -> gpui::AnyElement {
    let DeviceRowProps {
        index,
        device,
        latest,
        attention,
        no_account,
        inert,
        on_drop_share,
    } = props;
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let label = device.label.clone();
    // EXP-924: the owner's pick when there is one, else the kind default
    // — the ONE device resolver, shared with the composer's device pin.
    let kind_icon = crate::icons::device_icon(device.icon.as_deref(), device.server);
    // A-16: only the caller's OWN device wears the outdated tone — a
    // teammate's version is theirs to keep.
    let outdated = device.mine && update_available(device.version.as_deref(), latest.as_deref());
    // EXP-909: the request lives on the SYNCED row; a QUEUED one is an
    // attention line (A-16), an unblocked one is simply under way.
    let updating = device.update_requested && !device.update_queued();
    // EXP-909: the row carries ONE control — the settings gear, revealed on
    // hover (the drafts list's idiom: the group lives on the row, the reveal
    // on the button). A teammate's shared row carries none.
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
                    if inert {
                        return;
                    }
                    crate::device_settings::open(window, cx, settings_row_id.clone());
                }),
            )
    });
    // FEED-149: a team OWNER's Remove on a teammate's row shared into the
    // current team — the gear's slot, painted and revealed the same way.
    let drop_share = on_drop_share.map(|on_click| {
        div()
            .invisible()
            .group_hover(MACHINE_ROW_GROUP, |style| style.visible())
            .flex_shrink_0()
            .child(
                crate::controls::ghost_icon_button(
                    ("machine-drop-share", index),
                    Icon::new(registry::UI_DELETE),
                    cx,
                )
                .tooltip("Remove from this team")
                .on_click(move |_, window, cx| on_click(window, cx)),
            )
    });

    // EXP-642: the web `GlassRow` two-line name block — name · version ·
    // default star · tags over the status line, `min_w_0` so only the NAME
    // gives way.
    let name_block = gpui_component::v_flex()
        .flex_1()
        .min_w_0()
        .gap_0p5()
        .child(
            gpui_component::h_flex()
                .w_full()
                .min_w_0()
                .items_center()
                .gap_1p5()
                // Web keeps the version ADJACENT to the name, so the name is
                // fit-content — a `truncate` here would render at the EXP-175
                // collapsed width; the cluster clips instead.
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
                .when_some(device.version.clone(), |this, version| {
                    // Stateful: the outdated hint rides a tooltip.
                    let hint: SharedString =
                        format!("Update available: v{}", latest.clone().unwrap_or_default())
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
                // EXP-622: the device every picker prefills.
                .when(device.is_default, |this| {
                    this.child(
                        Icon::new(registry::UI_DEVICE_DEFAULT)
                            .xsmall()
                            .flex_shrink_0()
                            .text_color(muted),
                    )
                })
                // EXP-642 (web parity): an own device shared with a team.
                .when(device.shared, |this| this.child(row_tag("Shared", cx)))
                // EXP-696: the device this app IS.
                .when(device.own, |this| this.child(row_tag("This device", cx))),
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
                .child(SharedString::from(status_line(&device)))
                .when(updating, |this| this.child(div().child("Updating…"))),
        );

    // A-16: the logins as CHIPS, `sortDeviceLogins` order (the derivation's);
    // "No account" / "Checking…" muted in their slot; own online rows end
    // with the ghost `+` (Add account).
    let mut chips = gpui_component::h_flex()
        .flex_shrink_0()
        .max_w(px(CHIPS_MAX_W))
        .flex_wrap()
        .justify_end()
        .items_center()
        .gap_1p5();
    if device.logins.is_empty() {
        chips = chips.child(
            div()
                .text_xs()
                .text_color(muted)
                .child(if no_account { COPY_NO_ACCOUNT } else { COPY_CHECKING }),
        );
    }
    for (slot, login) in device.logins.iter().enumerate() {
        chips = chips.child(login_chip(index * 64 + slot, &device, login, inert, cx));
    }
    let installed = !device.agents.is_empty() || !device.unauthed_agents.is_empty();
    if device.actionable() && installed {
        let device_id = device.device_id.clone();
        chips = chips.child(
            crate::controls::ghost_icon_button(
                ("machine-add-account", index),
                Icon::new(registry::UI_ADD),
                cx,
            )
            .tooltip(COPY_ADD_ACCOUNT)
            .on_click(move |_, window, cx| {
                if inert {
                    return;
                }
                crate::agent_login::open_add_account_dialog_for(device_id.clone(), window, cx);
            }),
        );
    }

    let attention_icon = (!attention.is_empty()).then(|| attention_icon(index, attention, cx));
    let row_hover = theme.list_hover;
    crate::surface::flat_row()
        .id(SharedString::from(format!("machine-{}", device.row_id)))
        .group(MACHINE_ROW_GROUP)
        .flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap_3()
        .px_3()
        .py_2p5()
        .hover(move |this| this.bg(row_hover))
        // A-16: the offline group is dimmed.
        .when(!device.online, |this| this.opacity(0.6))
        .child(
            div()
                .flex_shrink_0()
                .child(Icon::new(kind_icon).xsmall().text_color(muted)),
        )
        .child(name_block)
        .child(chips)
        .children(attention_icon)
        .children(gear)
        .children(drop_share)
        .into_any_element()
}

/// The muted outlined tag the name line wears ("Shared", "This device").
fn row_tag(label: &'static str, cx: &App) -> gpui::Div {
    div()
        .flex_shrink_0()
        .px_1()
        .rounded(px(theme::tokens::radius::SM))
        .border_1()
        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(label)
}

/// A-16 — ONE login as a chip: the brand mark, its identity
/// ([`crate::usage_bar::login_label`]) and ` · plan` behind an address. Hover
/// previews its limits exactly like the account picker's rows (EXP-992,
/// [`crate::usage_bar::render_account_limits`] in a tooltip); a click on one
/// of MY reachable devices opens the ×4 login menu ([`chip_actions`]) with
/// the same bars at its top. A teammate's chip only previews.
fn login_chip(
    id: usize,
    device: &DeviceCard,
    login: &AgentProfileUsageRow,
    inert: bool,
    cx: &App,
) -> gpui::AnyElement {
    let agent = coding::CodingAgent::parse(&login.agent);
    let label = SharedString::from(crate::usage_bar::login_label(login));
    // The plan rides BEHIND an address only: a label that already is the
    // plan must not print it twice.
    let plan_tail = match (login.email.as_ref(), login.plan.as_ref()) {
        (Some(_), Some(plan)) => Some(SharedString::from(format!(" · {plan}"))),
        _ => None,
    };
    let limits = login
        .usage
        .as_ref()
        .and_then(coding::account_option::account_limits);
    let actions = if device.actionable() && !inert {
        chip_actions(login.signed_in, login.health)
    } else {
        Vec::new()
    };
    let body = |cx: &App| {
        let mut parts: Vec<gpui::AnyElement> = Vec::new();
        if let Some(agent) = agent {
            parts.push(
                div()
                    .flex_shrink_0()
                    .child(
                        crate::coding_selects::agent_mark(agent)
                            .with_size(px(crate::surface::PillSize::Sm.glyph())),
                    )
                    .into_any_element(),
            );
        }
        parts.push(div().child(label.clone()).into_any_element());
        if let Some(tail) = plan_tail.clone() {
            parts.push(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(tail)
                    .into_any_element(),
            );
        }
        parts
    };
    let chip = match agent.filter(|_| !actions.is_empty()) {
        Some(agent) => {
            let device_id = device.device_id.clone();
            let device_label = device.label.clone();
            let own = device.own;
            let profile_id = login.profile_id.clone();
            let account_label = label.clone();
            let menu_limits = limits.clone();
            crate::surface::glass_pill_button(("machine-login", id), crate::surface::PillSize::Sm, cx)
                .children(body(cx))
                .pointer_menu(move |menu, _window, cx| {
                    let mut menu = menu;
                    // The same bars the hover previews, at the menu's top.
                    if let Some(limits) = menu_limits.clone() {
                        menu = menu
                            .item(
                                PopupMenuItem::element(move |_, cx| {
                                    div()
                                        .py_1()
                                        .child(crate::usage_bar::render_account_limits(&limits, cx))
                                })
                                .disabled(true),
                            )
                            .separator();
                    }
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
                                Some(profile_id.clone()),
                                window,
                                cx,
                            ),
                            ChipAction::SignOut => crate::agent_account_actions::sign_out_account(
                                device_id.clone(),
                                device_label.clone(),
                                own,
                                agent,
                                profile_id.clone(),
                                account_label.clone(),
                                window,
                                cx,
                            ),
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
                        menu = menu.item(if action.destructive() {
                            crate::controls::danger_menu_item(
                                action.label(),
                                Icon::new(action.icon()),
                                cx,
                            )
                            .on_click(click)
                        } else {
                            crate::controls::pointer_label_item(action.label(), false)
                                .icon(Icon::new(action.icon()))
                                .on_click(click)
                        });
                    }
                    menu
                })
                .into_any_element()
        }
        None => crate::surface::glass_pill(
            ("machine-login", id),
            crate::surface::PillSize::Sm,
            crate::surface::PillMode::Readonly,
            cx,
        )
        .children(body(cx))
        .into_any_element(),
    };
    // EXP-992's preview rides a TOOLTIP on the chip's wrapper — display-only,
    // so it can never swallow the click that opens the menu.
    div()
        .id(("machine-login-preview", id))
        .flex_shrink_0()
        .when_some(limits, |this, limits| {
            this.tooltip(move |window, cx| {
                let limits = limits.clone();
                gpui_component::tooltip::Tooltip::element(move |_, cx| {
                    crate::usage_bar::render_account_limits(&limits, cx)
                })
                .build(window, cx)
            })
        })
        .child(chip)
        .into_any_element()
}

/// A-16 — the attention icon (warning tone): its card opens on hover and
/// lists every item, text left, its pill right.
fn attention_icon(index: usize, lines: Vec<AttentionLine>, cx: &App) -> gpui::AnyElement {
    let amber = theme::tokens::YELLOW.to_hsla();
    let card_id = SharedString::from(format!("machine-attention-card-{index}"));
    div()
        .id(("machine-attention", index))
        .flex_shrink_0()
        // A control inside the row: a click never reaches the row.
        .on_click(|_, _, cx| cx.stop_propagation())
        .child(
            gpui_component::hover_card::HoverCard::new(("machine-attention-hover", index))
                .anchor(gpui::Anchor::TopRight)
                .open_delay(ATTENTION_OPEN_DELAY)
                .close_delay(ATTENTION_CLOSE_DELAY)
                .p_2p5()
                .trigger(
                    crate::controls::ghost_icon_button(
                        ("machine-attention-trigger", index),
                        Icon::new(registry::UI_WARNING).text_color(amber),
                        cx,
                    )
                    .text_color(amber),
                )
                .content(move |_, _, cx| attention_card(&card_id, &lines, cx)),
        )
        .into_any_element()
}

/// A-16 — the attention card's body: one line per item, the warning glyph and
/// its text left, at most one pill right (web `DeviceAttention`'s card).
pub(crate) fn attention_card(id: &SharedString, lines: &[AttentionLine], cx: &App) -> gpui::Div {
    let amber = theme::tokens::YELLOW.to_hsla();
    let mut card = gpui_component::v_flex().w(px(ATTENTION_CARD_W)).gap_1();
    for (slot, line) in lines.iter().enumerate() {
        let pill = line.action.clone().map(|pill| {
            let on_click = pill.on_click.clone();
            crate::surface::glass_pill_button(
                SharedString::from(format!("{id}-{slot}")),
                crate::surface::PillSize::Sm,
                cx,
            )
            .label(pill.label)
            .disabled(pill.busy)
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                if let Some(on_click) = on_click.as_ref() {
                    on_click(window, cx);
                }
            })
        });
        card = card.child(
            gpui_component::h_flex()
                .w_full()
                .min_w_0()
                .items_center()
                .gap_2()
                .py_0p5()
                .child(
                    Icon::new(registry::UI_WARNING)
                        .size(px(14.))
                        .flex_shrink_0()
                        .text_color(amber),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_xs()
                        .child(line.text.clone()),
                )
                .children(pill),
        );
    }
    card
}

/// Where the desktop app's builds live — the desktop card's "All platforms".
pub(crate) const DESKTOP_RELEASES_URL: &str = "https://github.com/Niach/exponential/releases/latest";

/// The desktop card's primary download (web `desktopDownloadHref`): THIS
/// OS's release asset (the names `updater::expected_asset_name` matches),
/// the releases page on anything else.
pub(crate) fn desktop_download_url() -> String {
    let asset = match std::env::consts::OS {
        "macos" => "Exponential-production.dmg",
        "windows" => "Exponential-production-x86_64-windows.exe",
        "linux" => "Exponential-production-x86_64.AppImage",
        _ => return DESKTOP_RELEASES_URL.to_string(),
    };
    format!("{DESKTOP_RELEASES_URL}/download/{asset}")
}

/// The instance the install one-liner points the daemon at.
pub(crate) fn server_install_origin(cx: &gpui::App) -> String {
    queries::active_account(cx)
        .map(|account| account.instance_url.trim_end_matches('/').to_string())
        .unwrap_or_else(|| "https://app.exponential.at".to_string())
}

/// The "Add device" dialog (EXP-697, one spec with the web twin): the
/// device-setup block's desktop card first (it is what actually runs coding
/// sessions), then its server card (EXP-1169, [`crate::device_setup`]): the
/// install command for the headless `exponential` CLI with a one-time install
/// token, and the field that approves the CLI's device code in place.
/// Shared (EXP-470): opened from this section's band and from the
/// Getting-started page's server card.
pub(crate) fn open_add_server_dialog(window: &mut Window, cx: &mut gpui::App) {
    // The card is a stateful view (token mint, code field): created on the
    // dialog's first frame, dropped with the window.
    let server_card: Rc<RefCell<Option<Entity<crate::device_setup::ServerCard>>>> =
        Rc::new(RefCell::new(None));
    // P33: no subtitle (the cards say it). An alert window cannot follow
    // its content, so the height is the two cards' own (the server card's
    // code field included) plus the footer, not a 600px slab.
    let spec = AlertSpec::new("Add device", "", "Done")
        .without_cancel()
        .height(px(500.))
        .content(move |window, cx| {
        let card = server_card
            .borrow_mut()
            .get_or_insert_with(|| {
                cx.new(|cx| crate::device_setup::ServerCard::new(window, cx))
            })
            .clone();
        gpui_component::v_flex()
            .gap_3()
            .child(crate::device_setup::desktop_card(cx))
            .child(card)
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
            icon: None,
            online,
            own: false,
            mine: true,
            owner_tooltip: None,
            shared_team_ids: Vec::new(),
            shared: false,
            is_default: false,
            version: None,
            update_requested: false,
            update_blocked: false,
            last_seen_at: last_seen.map(str::to_string),
            agents: Vec::new(),
            unauthed_agents: Vec::new(),
            logins: Vec::new(),
            accounts_reported: false,
        }
    }

    fn login_row(mine: bool, online: bool) -> AgentProfileUsageRow {
        AgentProfileUsageRow {
            key: "dev-1:claude:0a1b2c3d".to_string(),
            device_id: "dev-1".to_string(),
            device_label: "Studio".to_string(),
            mine,
            online,
            agent: "claude".to_string(),
            profile_id: "0a1b2c3d".to_string(),
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
    /// only ever queues on MY machine, and only while it is online (web
    /// `deviceCanRefreshUsage`).
    #[test]
    fn refresh_needs_my_own_online_machine() {
        assert!(MachinesSection::can_refresh(&login_row(true, true)));
        // A teammate's shared server is not mine to poll.
        assert!(!MachinesSection::can_refresh(&login_row(false, true)));
        // Offline: the command would sit queued until it came back.
        assert!(!MachinesSection::can_refresh(&login_row(true, false)));
    }

    /// A-16: THE contract fixture the four clients replay.
    const DEVICES_LIST: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/devices-list.json");

    fn fixture() -> serde_json::Value {
        serde_json::from_str(DEVICES_LIST).expect("devices-list.json parses")
    }

    /// A-16: every string the row renders is the fixture's `copy`, byte for
    /// byte.
    #[test]
    fn device_list_copy_matches_the_fixture() {
        let fixture = fixture();
        let copy = &fixture["copy"];
        for (key, ours) in [
            ("mine", COPY_MINE),
            ("team", COPY_TEAM),
            ("offline", COPY_OFFLINE),
            ("noAccount", COPY_NO_ACCOUNT),
            ("checking", COPY_CHECKING),
            ("addAccount", COPY_ADD_ACCOUNT),
            ("attention", COPY_ATTENTION),
            ("updateAvailable", COPY_UPDATE_AVAILABLE),
            ("updateQueued", COPY_UPDATE_QUEUED),
            ("needsRelogin", COPY_NEEDS_RELOGIN),
            ("signedOut", COPY_SIGNED_OUT),
            ("update", COPY_UPDATE),
            ("signIn", COPY_SIGN_IN),
        ] {
            assert_eq!(copy[key].as_str(), Some(ours), "copy.{key}");
        }
        assert_eq!(copy.as_object().map(|copy| copy.len()), Some(13), "a new copy key");
    }

    /// A-16: [`device_attention`] replays every case of `devices-list.json`
    /// (web `deviceListRow`) — among them the mint bug: an agent installed
    /// but never signed in badges nothing.
    #[test]
    fn device_attention_replays_the_fixture() {
        let fixture = fixture();
        let cases = fixture["cases"].as_array().expect("cases");
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap_or_default();
            let input = &case["input"];
            let flag = |key: &str| input[key].as_bool().unwrap_or_else(|| panic!("{name}: {key}"));
            let logins = input["logins"]
                .as_array()
                .map(|logins| {
                    logins
                        .iter()
                        .map(|login| DeviceListLogin {
                            agent: login["agent"].as_str().unwrap_or_default().to_string(),
                            profile_id: login["profileId"].as_str().unwrap_or_default().to_string(),
                            label: login["label"].as_str().unwrap_or_default().to_string(),
                            health: coding::agent_accounts::Health::parse(
                                login["health"].as_str().unwrap_or_default(),
                            ),
                        })
                        .collect()
                })
                .unwrap_or_default();
            let row = device_attention(&DeviceListInput {
                online: flag("online"),
                mine: flag("mine"),
                server: flag("server"),
                registered: flag("registered"),
                outdated: flag("outdated"),
                latest: input["latest"].as_str().map(str::to_string),
                update_queued: flag("updateQueued"),
                accounts_reported: flag("accountsReported"),
                logins,
            });
            let expect = &case["expect"];
            let group = if row.online { "online" } else { "offline" };
            assert_eq!(expect["group"].as_str(), Some(group), "{name}: group");
            assert_eq!(expect["noAccount"].as_bool(), Some(row.no_account), "{name}: noAccount");
            let got: Vec<serde_json::Value> = row
                .attention
                .iter()
                .map(|item| {
                    let action = match item.action {
                        Some(AttentionAction::SignIn) => serde_json::json!("sign_in"),
                        Some(AttentionAction::Update) => serde_json::json!("update"),
                        None => serde_json::Value::Null,
                    };
                    match &item.kind {
                        AttentionKind::Login { agent, profile_id } => serde_json::json!({
                            "kind": "login",
                            "profileId": profile_id,
                            "agent": agent,
                            "text": item.text,
                            "action": action,
                        }),
                        AttentionKind::Update => serde_json::json!({
                            "kind": "update",
                            "text": item.text,
                            "action": action,
                        }),
                        AttentionKind::UpdateQueued => serde_json::json!({
                            "kind": "update_queued",
                            "text": item.text,
                            "action": action,
                        }),
                    }
                })
                .collect();
            assert_eq!(
                serde_json::Value::Array(got),
                expect["attention"],
                "{name}: attention"
            );
        }
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

        let derived = self.derived.clone();
        let (mine, team) = match derived.as_ref() {
            Some(derived) => (derived.mine.as_slice(), derived.team.as_slice()),
            None => (&[][..], &[][..]),
        };
        let owned_team_id = self.owned_team_id(cx);
        let mine_group = self.render_group(0, mine, None, cx);
        // Offset the element ids so the two groups can never collide.
        let team_group = self.render_group(mine.len(), team, owned_team_id.as_deref(), cx);

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
                COPY_MINE,
                Some(add_server),
                cx,
            ))
            // EXP-1259 D8: the first load is skeleton rows.
            .when(derived.is_none(), |this| {
                this.child(crate::feedback::skeleton_rows(
                    "devices-skeleton",
                    crate::feedback::SKELETON_ROWS,
                    crate::settings::SETTINGS_ROW_H,
                    cx,
                ))
            })
            .when(derived.is_some() && mine.is_empty(), |this| {
                this.child(crate::controls::empty_state_list(
                    gpui_component::Icon::new(crate::icons::registry::NAV_DEVICES),
                    crate::empty_state_copy::DEVICES,
                    cx,
                ))
            })
            .child(mine_group)
            // EXP-432/642: teammates' shared devices get their OWN headed
            // section, exactly like the web page.
            .when(!team.is_empty(), |this| {
                this.child(
                    gpui_component::v_flex()
                        .min_w_0()
                        .pt_4()
                        .child(crate::surface::glass_section_header(
                            COPY_TEAM,
                            None,
                            cx,
                        ))
                        .child(team_group),
                )
            })
    }
}
