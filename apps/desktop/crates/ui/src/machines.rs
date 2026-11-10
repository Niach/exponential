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
//! logins beneath itself, each with the ×4 actions (`usage_bar::chip_actions`):
//! sign in, sign out, remove this device's copy of the login
//! (`agent_profile_remove`). EXP-1158: there is no "default" to set — every
//! start runs on the login last used on that device. A team device's logins are read-only: they belong
//! to their owner.
//!
//! EXP-1560: a device card carries ONE control — the `⋯` overflow, opening
//! THE menu: the update line, `Update` (a server's), and Device settings
//! ([`crate::device_settings`]). No ▶ (a run starts from the Agent page
//! composer, which preselects a device).
//!
//! FEED-149: a teammate's shared card carries no control, except for an
//! OWNER of the current team: its `⋯` then holds Remove, which drops that
//! member's share of the machine from this team (`devices.dropShare`).
//!
//! EXP-909 folded the cross-device Accounts section (EXP-818) INTO these rows.
//! One account held by three machines is three logins on three machines —
//! which is what a person repairing one needs to see — so the email-merged
//! groups, their per-agent tabs and their device chips are gone, and with them
//! the page's only place where a login was not under its machine. The 30 s
//! refresh round the section ran came along.
//!
//! EXP-1560 (`fixtures/devices-list.json`, one spec ×4; was A-16's one-line
//! row): each section is a GRID of device CARDS ([`device_card_columns`]).
//! A card's top row is the device's own icon, the name block and the `⋯`; its
//! bottom row the logins as brand-mark ICONS ([`device_card_account_icons`]).
//! A login with a problem (dead, signed out, at its limit) wears THE run-mark
//! badge; the update line leads the `⋯` menu. Only login PROFILES count: an
//! agent CLI installed but never signed in badges nothing
//! ([`device_attention`]). Online cards lead, the offline ones follow dimmed
//! under a muted "Offline" label.
//!
//! #EXP-1643 THE ACCOUNT PANEL: nothing floats any more. A press on an icon
//! expands THAT card with [`account_panel`] (one open per device, remembered
//! per device on this client in [`crate::ui_prefs`], never synced); a hover is
//! only the tooltip with the email.

use crate::controls::PointerMenuButtonExt as _;
use gpui::prelude::FluentBuilder as _;
use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    div, px, App, AppContext as _, Entity, InteractiveElement as _, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    button::{ButtonCustomVariant, ButtonVariants as _},
    ActiveTheme as _, Icon, IconNamed as _, Sizable as _,
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

/// EXP-1560: the card grid's geometry (`fixture.card`), byte-locked by
/// `device_card_geometry_matches_the_fixture`.
pub(crate) const CARD_MIN_W: f32 = 320.;
pub(crate) const CARD_GAP: f32 = 12.;
pub(crate) const CARD_PADDING: f32 = 12.;
pub(crate) const CARD_ICON_SIZE: f32 = 28.;
pub(crate) const CARD_MARK_SIZE: f32 = 16.;
pub(crate) const CARD_MAX_ACCOUNT_ICONS: usize = 5;

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

    /// A-16: the card's [`device_attention`] input. `latest` is the instance's
    /// `CLIENT_LATEST_VERSION_*` for this device's kind.
    pub(crate) fn list_input(&self, latest: Option<&str>) -> DeviceListInput {
        let now = chrono::Utc::now();
        let now_epoch = now.timestamp();
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
                    limit_hit: limit_hit(login.usage.as_ref(), now_epoch),
                    token_expires_at: login.token_expires_at.clone(),
                })
                .collect(),
            now,
        }
    }
}

/// EXP-1560 — `limitHit`: a FRESH usage window ([`crate::usage_bar::
/// is_fresh`]) at ≥ 100 %. Old numbers never badge: the limit may have
/// reset since.
pub(crate) fn limit_hit(usage: Option<&coding::agent_usage::AgentUsage>, now_epoch: i64) -> bool {
    usage.is_some_and(|usage| {
        crate::usage_bar::is_fresh(&usage.fetched_at, now_epoch)
            && usage.windows.iter().any(|window| window.percent >= 100)
    })
}

// -- A-16: the row's attention, pure --------------------------------------

/// The fixture's `copy`, byte-locked by `device_list_copy_matches_the_fixture`.
pub(crate) const COPY_MINE: &str = "My devices";
pub(crate) const COPY_TEAM: &str = "Team devices";
pub(crate) const COPY_OFFLINE: &str = "Offline";
pub(crate) const COPY_NO_ACCOUNT: &str = "No account";
pub(crate) const COPY_CHECKING: &str = "Checking…";
pub(crate) const COPY_ADD_ACCOUNT: &str = "Add account";
const COPY_UPDATE_AVAILABLE: &str = "Update available · v{version}";
const COPY_UPDATE_QUEUED: &str = "Update queued: restarts within 10 minutes.";
const COPY_NEEDS_RELOGIN: &str = "{label} · Needs re-login";
const COPY_SIGNED_OUT: &str = "{label} · Signed out";
const COPY_LIMIT_REACHED: &str = "{label} · Limit reached";
pub(crate) const COPY_UPDATE: &str = "Update";
#[cfg(test)]
pub(crate) const COPY_SIGN_IN: &str = "Sign in";
/// EXP-1627: the sign-in dialog's title (never the agent's name) and its
/// ONE waiting line (`{device}` = the device label, `{agent}` = the agent).
pub(crate) const COPY_SIGN_IN_TITLE: &str = "Sign in";
pub(crate) const COPY_SIGN_IN_WAITING: &str = "Waiting for {device} to publish the {agent} sign-in link…";
const COPY_STATUS_NEEDS_RELOGIN: &str = "Needs re-login";
const COPY_STATUS_SIGNED_OUT: &str = "Signed out";
const COPY_STATUS_LIMIT: &str = "Limit reached";
/// EXP-1626: Persist login, an option on an existing claude login (the
/// fixture's copy): the panel's pill, its renewal (the account panel only,
/// EXP-1626: no renewal reminder), the dialog's one muted line, and the
/// panel's line saying which login is active.
pub(crate) const COPY_PERSIST_LOGIN: &str = "Persist login";
pub(crate) const COPY_RENEW_PERSISTED_LOGIN: &str = "Renew persisted login";
pub(crate) const COPY_PERSIST_NOTE: &str =
    "Persist login keeps this account's runs signed in for 1 year, even if its browser login expires.";
pub(crate) const COPY_PERSISTED_BROWSER: &str = "Persisted · usage from the browser login";
pub(crate) const COPY_PERSISTED_TOKEN: &str = "Persisted · browser login expired, weekly usage only";
/// An older device's refusal of a long-lived login with no profile (its
/// `LONG_LIVED_NEEDS_PROFILE`). Fixture-locked only: no client falls back
/// on it any more (EXP-1626: no client queues a long-lived Add).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const COPY_LONG_LIVED_REFUSED: &str = "A long-lived login needs a Claude account on this machine.";
pub(crate) const COPY_DEVICE_ACTIONS: &str = "Device actions";
pub(crate) const COPY_SETTINGS: &str = DEVICE_SETTINGS;
pub(crate) const COPY_REMOVE_FROM_TEAM: &str = "Remove from this team";
pub(crate) const COPY_MORE_ACCOUNTS: &str = "More accounts";

/// EXP-1626 — the account panel's persisted line: `None` for a login with
/// no token; else which login is active (`usageSource: "token"` = the
/// browser login cannot serve the usage, the token's probe does).
pub(crate) fn persisted_line(login: &AgentProfileUsageRow) -> Option<&'static str> {
    login.token_expires_at.as_ref()?;
    Some(match login.usage_from_token {
        true => COPY_PERSISTED_TOKEN,
        false => COPY_PERSISTED_BROWSER,
    })
}

/// One login PROFILE a device reported (web `DeviceListLogin`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DeviceListLogin {
    pub(crate) agent: String,
    pub(crate) profile_id: String,
    /// [`crate::usage_bar::login_label`] — the email, else the plan.
    pub(crate) label: String,
    pub(crate) health: coding::agent_accounts::Health,
    /// EXP-1560: a fresh usage window at ≥ 100 % ([`limit_hit`]).
    pub(crate) limit_hit: bool,
    /// EXP-1600: when the profile's long-lived claude login ends (ISO);
    /// `None` = it holds none. EXP-1626: never an item (no renewal
    /// reminder); kept as the fixture's input.
    #[allow(dead_code)]
    pub(crate) token_expires_at: Option<String>,
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
    /// The fixture's clock (EXP-1626: no renewal reminder reads it).
    #[allow(dead_code)]
    pub(crate) now: chrono::DateTime<chrono::Utc>,
}

/// EXP-1560: a login item's `status` — the panel's warning line
/// (`copy.status`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LoginStatus {
    NeedsRelogin,
    SignedOut,
    Limit,
}

impl LoginStatus {
    /// The fixture's wire value (the replay's).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn wire(self) -> &'static str {
        match self {
            LoginStatus::NeedsRelogin => "needs_relogin",
            LoginStatus::SignedOut => "signed_out",
            LoginStatus::Limit => "limit",
        }
    }

    /// `copy.status[wire]`.
    pub(crate) fn label(self) -> &'static str {
        match self {
            LoginStatus::NeedsRelogin => COPY_STATUS_NEEDS_RELOGIN,
            LoginStatus::SignedOut => COPY_STATUS_SIGNED_OUT,
            LoginStatus::Limit => COPY_STATUS_LIMIT,
        }
    }
}

/// What one attention item is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AttentionKind {
    Login {
        agent: String,
        profile_id: String,
        status: LoginStatus,
    },
    Update,
    UpdateQueued,
}

/// The one pill an attention line may carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AttentionAction {
    SignIn,
    Update,
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

/// A-16 / EXP-1560 — THE card derivation, mirrored ×4 (web `deviceListRow`,
/// iOS / Android `DevicesList`) and replayed against every case of
/// `devices-list.json`. Items in order: every `needs_relogin` login, every
/// `signed_out` login, every OTHER login at its limit (never an action),
/// then the update line (the caller's own device only). EXP-1626: a
/// persisted login is never an item (no renewal reminder). A login yields at
/// most one item; an agent with no login PROFILE is not an item: it is not
/// in `logins` at all.
pub(crate) fn device_attention(input: &DeviceListInput) -> DeviceListRow {
    use coding::agent_accounts::Health;
    let mut attention = Vec::new();
    let can_sign_in = input.mine && input.online;
    for (health, template, status) in [
        (Health::NeedsRelogin, COPY_NEEDS_RELOGIN, LoginStatus::NeedsRelogin),
        (Health::SignedOut, COPY_SIGNED_OUT, LoginStatus::SignedOut),
    ] {
        for login in input.logins.iter().filter(|login| login.health == health) {
            attention.push(AttentionItem {
                kind: AttentionKind::Login {
                    agent: login.agent.clone(),
                    profile_id: login.profile_id.clone(),
                    status,
                },
                text: template.replace("{label}", &login.label),
                action: can_sign_in.then_some(AttentionAction::SignIn),
            });
        }
    }
    for login in input.logins.iter().filter(|login| {
        login.limit_hit && !matches!(login.health, Health::NeedsRelogin | Health::SignedOut)
    }) {
        attention.push(AttentionItem {
            kind: AttentionKind::Login {
                agent: login.agent.clone(),
                profile_id: login.profile_id.clone(),
                status: LoginStatus::Limit,
            },
            text: COPY_LIMIT_REACHED.replace("{label}", &login.label),
            action: None,
        });
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

/// EXP-1560 — grid columns for a section `width` px wide (web
/// `deviceCardColumns`): as many `CARD_MIN_W` cards as fit with `CARD_GAP`
/// between them, never fewer than one.
pub(crate) fn device_card_columns(width: f32) -> usize {
    (((width + CARD_GAP) / (CARD_MIN_W + CARD_GAP)).floor().max(1.)) as usize
}

/// EXP-1560 — how many account icons a card shows, and whether the `⋯`
/// "More accounts" button follows them (web `deviceCardAccountIcons`,
/// `fixture.accountIcons`).
pub(crate) fn device_card_account_icons(count: usize, expanded: bool) -> (usize, bool) {
    if expanded || count <= CARD_MAX_ACCOUNT_ICONS {
        (count, false)
    } else {
        (CARD_MAX_ACCOUNT_ICONS - 1, true)
    }
}

/// #EXP-1643 — what a press on the icon of `key` remembers: the open icon
/// closes (by hand, so it stays closed), any other opens.
pub(crate) fn toggled_account<'a>(open: Option<&str>, key: &'a str) -> Option<&'a str> {
    (open != Some(key)).then_some(key)
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
    /// EXP-1560: the section's content width, probed at prepaint — what
    /// [`device_card_columns`] reads.
    measured_width: Rc<std::cell::Cell<gpui::Pixels>>,
    /// EXP-1560: the `devices` row ids whose `⋯` "More accounts" was pressed
    /// (revealed in place for the section's lifetime).
    expanded_accounts: std::collections::HashSet<String>,
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
            measured_width: Rc::new(std::cell::Cell::new(px(0.))),
            expanded_accounts: std::collections::HashSet::new(),
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

    /// EXP-1560: one card's [`DeviceCardProps`] — the derivation
    /// ([`device_attention`]) split onto the logins it badges and the update
    /// line the `⋯` leads with, the open account ([`crate::ui_prefs`]), plus
    /// the handlers its icons, its Update item, "More accounts" and the
    /// owner's Remove run.
    fn card_props(
        &self,
        index: usize,
        device: &DeviceCard,
        owned_team_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) -> DeviceCardProps {
        // Informational only: the card nudges, the device decides.
        let latest = if device.server {
            self.latest.cli.clone()
        } else {
            self.latest.desktop.clone()
        };
        let derived = device_attention(&device.list_input(latest.as_deref()));
        let view = cx.entity().downgrade();
        let mut login_flags: Vec<Option<LoginStatus>> = vec![None; device.logins.len()];
        let mut update_line = None;
        for item in &derived.attention {
            match &item.kind {
                AttentionKind::Login {
                    agent,
                    profile_id,
                    status,
                } => {
                    if let Some(slot) = device
                        .logins
                        .iter()
                        .position(|login| &login.agent == agent && &login.profile_id == profile_id)
                    {
                        login_flags[slot] = Some(*status);
                    }
                }
                // `update` is a SERVER's only (devices-list.json): the CLI
                // daemon acts on the request; a desktop app updates itself
                // and its line carries no item.
                AttentionKind::Update | AttentionKind::UpdateQueued => {
                    let update = item.action.map(|_| {
                        let view = view.clone();
                        let device_id = device.device_id.clone();
                        AttentionPill {
                            label: COPY_UPDATE,
                            busy: device.update_requested || self.updating.contains(&device.device_id),
                            on_click: Some(Rc::new(move |_: &mut Window, cx: &mut App| {
                                let device_id = device_id.clone();
                                let _ = view.update(cx, |this, cx| this.request_update(device_id, cx));
                            })),
                        }
                    });
                    update_line = Some(UpdateLine {
                        text: SharedString::from(item.text.clone()),
                        update,
                    });
                }
            }
        }
        // FEED-149: a team OWNER's Remove on a teammate's card shared into
        // the current team.
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
        // #EXP-1643: the open account, remembered per device on this client.
        let open_login = {
            let keys: Vec<&str> = device.logins.iter().map(|login| login.key.as_str()).collect();
            let remembered = crate::ui_prefs::device_account(&device.device_id);
            domain::devices_list::open_device_account(
                &keys,
                remembered.as_ref().map(|key| key.as_deref()),
            )
            .map(str::to_string)
        };
        let on_toggle_login = {
            let view = view.clone();
            let device_id = device.device_id.clone();
            let open = open_login.clone();
            Some(Rc::new(move |key: &str, _: &mut Window, cx: &mut App| {
                crate::ui_prefs::set_device_account(&device_id, toggled_account(open.as_deref(), key));
                let _ = view.update(cx, |_, cx| cx.notify());
            }) as LoginHandler)
        };
        let on_expand_accounts = {
            let row_id = device.row_id.clone();
            Some(Rc::new(move |_: &mut Window, cx: &mut App| {
                let row_id = row_id.clone();
                let _ = view.update(cx, |this, cx| {
                    this.expanded_accounts.insert(row_id);
                    cx.notify();
                });
            }) as RowHandler)
        };
        DeviceCardProps {
            index,
            device: device.clone(),
            latest,
            login_flags,
            update_line,
            no_account: derived.no_account,
            inert: false,
            open_login,
            on_toggle_login,
            accounts_expanded: self.expanded_accounts.contains(&device.row_id),
            on_expand_accounts,
            on_drop_share,
        }
    }

    /// EXP-1560: one headed group — the online cards in the list's own order
    /// as one grid, then the muted "Offline" label over the offline ones in
    /// their own grid (dimmed by the card).
    fn render_group(
        &self,
        offset: usize,
        devices: &[DeviceCard],
        owned_team_id: Option<&str>,
        columns: usize,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Div {
        let muted = cx.theme().muted_foreground;
        let mut online = Vec::new();
        let mut offline = Vec::new();
        for (index, device) in devices.iter().enumerate() {
            let card = device_card(self.card_props(offset + index, device, owned_team_id, cx), cx);
            if device.online {
                online.push(card);
            } else {
                offline.push(card);
            }
        }
        gpui_component::v_flex()
            .min_w_0()
            .gap(px(CARD_GAP))
            .when(!online.is_empty(), |this| this.child(card_grid(online, columns)))
            .when(!offline.is_empty(), |this| {
                this.child(
                    gpui_component::v_flex()
                        .min_w_0()
                        .gap_2()
                        .child(div().text_xs().text_color(muted).child(COPY_OFFLINE))
                        .child(card_grid(offline, columns)),
                )
            })
    }
}

/// EXP-1560 — `cards` laid out `columns` wide, `CARD_GAP` both ways; the
/// last row keeps its empty cells so every card is one column wide. #EXP-1643:
/// a row aligns to the top, a card with its panel open never stretches its
/// neighbour.
pub(crate) fn card_grid(cards: Vec<gpui::AnyElement>, columns: usize) -> gpui::Div {
    let columns = columns.max(1);
    let mut grid = gpui_component::v_flex().w_full().min_w_0().gap(px(CARD_GAP));
    let mut cards = cards.into_iter().peekable();
    while cards.peek().is_some() {
        let mut row = gpui_component::h_flex()
            .w_full()
            .min_w_0()
            .items_start()
            .gap(px(CARD_GAP));
        for _ in 0..columns {
            let cell = gpui_component::v_flex().flex_1().min_w_0();
            row = row.child(match cards.next() {
                Some(card) => cell.child(card),
                None => cell,
            });
        }
        grid = grid.child(row);
    }
    grid
}

/// A card control's click, built where the state lives and run by the card.
pub(crate) type RowHandler = Rc<dyn Fn(&mut Window, &mut App)>;

/// #EXP-1643: a press on one login's icon, by its [`AgentProfileUsageRow::key`].
pub(crate) type LoginHandler = Rc<dyn Fn(&str, &mut Window, &mut App)>;

/// The Update item a card's menu offers. `on_click: None` = inert (the gallery).
#[derive(Clone)]
pub(crate) struct AttentionPill {
    pub(crate) label: &'static str,
    /// The request is already in flight: the control disables.
    pub(crate) busy: bool,
    pub(crate) on_click: Option<RowHandler>,
}

/// EXP-1560: the update line the `⋯` menu leads with, and its Update item.
#[derive(Clone)]
pub(crate) struct UpdateLine {
    pub(crate) text: SharedString,
    pub(crate) update: Option<AttentionPill>,
}

/// EXP-1560: everything [`device_card`] draws. A plain value, so the
/// styleguide's `device-attention` entry draws the REAL card.
pub(crate) struct DeviceCardProps {
    pub(crate) index: usize,
    pub(crate) device: DeviceCard,
    /// This kind's `CLIENT_LATEST_VERSION_*` (the version's tooltip).
    pub(crate) latest: Option<String>,
    /// One per `device.logins`, in that order: its attention status, if any
    /// (the icon's badge, the panel's warning line).
    pub(crate) login_flags: Vec<Option<LoginStatus>>,
    pub(crate) update_line: Option<UpdateLine>,
    pub(crate) no_account: bool,
    /// The gallery: every control drawn, none wired (no menus, no dialogs).
    pub(crate) inert: bool,
    /// #EXP-1643: the [`AgentProfileUsageRow::key`] of the login whose panel
    /// the card shows ([`domain::devices_list::open_device_account`]).
    pub(crate) open_login: Option<String>,
    pub(crate) on_toggle_login: Option<LoginHandler>,
    /// "More accounts" was pressed: every login icon shows.
    pub(crate) accounts_expanded: bool,
    pub(crate) on_expand_accounts: Option<RowHandler>,
    /// FEED-149: a team owner's Remove, the team card's only menu item.
    pub(crate) on_drop_share: Option<RowHandler>,
}

/// EXP-1560 — THE device card: the glass card surface, two rows. Top: the
/// device's own icon, name · version · default star over the status line and
/// the This device / Shared tags, the `⋯` overflow right. Bottom: the logins as
/// brand-mark icons (badged when they need attention), the `⋯` "More accounts"
/// reveal and the ghost `+`, then #EXP-1643 the open login's [`account_panel`].
/// An offline card is dimmed.
pub(crate) fn device_card(props: DeviceCardProps, cx: &App) -> gpui::AnyElement {
    let DeviceCardProps {
        index,
        device,
        latest,
        login_flags,
        update_line,
        no_account,
        inert,
        open_login,
        on_toggle_login,
        accounts_expanded,
        on_expand_accounts,
        on_drop_share,
    } = props;
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    // A-16: only the caller's OWN device wears the outdated tone — a
    // teammate's version is theirs to keep.
    let outdated = device.mine && update_available(device.version.as_deref(), latest.as_deref());
    // EXP-909: the request lives on the SYNCED row; a QUEUED one is the
    // menu's update line, an unblocked one is simply under way.
    let updating = device.update_requested && !device.update_queued();

    let name_line = gpui_component::h_flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap_1p5()
        .child(
            div()
                .id(("machine-name", index))
                .min_w_0()
                .truncate()
                .text_sm()
                .text_color(theme.foreground)
                .when_some(device.owner_tooltip.clone(), |this, owner| {
                    this.tooltip(move |window, cx| {
                        gpui_component::tooltip::Tooltip::new(owner.clone()).build(window, cx)
                    })
                })
                .child(device.label.clone()),
        )
        .when_some(device.version.clone(), |this, version| {
            // Stateful: the outdated hint rides a tooltip.
            let hint: SharedString =
                format!("Update available: v{}", latest.clone().unwrap_or_default()).into();
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
                            gpui_component::tooltip::Tooltip::new(hint.clone()).build(window, cx)
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
        });
    let status = gpui_component::h_flex()
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
        .child(div().min_w_0().truncate().child(SharedString::from(status_line(&device))))
        .when(updating, |this| this.child(div().flex_shrink_0().child("Updating…")))
        // EXP-696: the device this app IS.
        .when(device.own, |this| this.child(row_tag("This device", cx)))
        // EXP-642 (web parity): an own device shared with a team.
        .when(device.shared, |this| this.child(row_tag("Shared", cx)));

    let overflow = device_menu(index, &device, update_line, on_drop_share, inert, cx);
    let top = gpui_component::h_flex()
        .w_full()
        .min_w_0()
        .items_start()
        .gap_2()
        // #EXP-1643: the device's own icon leads the card.
        .child(
            Icon::new(crate::icons::device_icon(device.icon.as_deref(), device.server))
                .size(px(domain::devices_list::CARD_DEVICE_ICON_SIZE))
                .flex_shrink_0()
                .mt(px(1.))
                .text_color(muted),
        )
        .child(
            gpui_component::v_flex()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .child(name_line)
                .child(status),
        )
        .child(overflow);

    // The logins as ICONS, `sortDeviceLogins` order (the derivation's);
    // "No account" / "Checking…" muted in their slot; own online cards end
    // with the ghost `+` (Add account).
    let mut accounts = gpui_component::h_flex()
        .w_full()
        .min_w_0()
        .flex_wrap()
        .items_center()
        .gap_1();
    if device.logins.is_empty() {
        accounts = accounts.child(
            div()
                .h(px(CARD_ICON_SIZE))
                .flex()
                .items_center()
                .text_xs()
                .text_color(muted)
                .child(if no_account { COPY_NO_ACCOUNT } else { COPY_CHECKING }),
        );
    }
    let open_slot = open_login
        .as_deref()
        .and_then(|key| device.logins.iter().position(|login| login.key == key));
    // An open login folded behind the `⋯` reveals the folded icons.
    let folded_visible = device_card_account_icons(device.logins.len(), false).0;
    let revealed = accounts_expanded || open_slot.is_some_and(|slot| slot >= folded_visible);
    let (visible, more) = device_card_account_icons(device.logins.len(), revealed);
    for (slot, login) in device.logins.iter().enumerate().take(visible) {
        let badged = login_flags.get(slot).copied().flatten().is_some();
        let on_toggle = on_toggle_login.clone().map(|on_toggle| {
            let key = login.key.clone();
            Rc::new(move |window: &mut Window, cx: &mut App| on_toggle(&key, window, cx)) as RowHandler
        });
        accounts = accounts.child(account_icon(
            index * 64 + slot,
            login,
            badged,
            open_slot == Some(slot),
            on_toggle,
            cx,
        ));
    }
    if more {
        accounts = accounts.child(
            crate::controls::ghost_icon_button(
                ("machine-more-accounts", index),
                Icon::new(registry::UI_MORE),
                cx,
            )
            .size(px(CARD_ICON_SIZE))
            .tooltip(COPY_MORE_ACCOUNTS)
            .on_click(move |_, window, cx| {
                if let Some(on_click) = on_expand_accounts.as_ref() {
                    on_click(window, cx);
                }
            }),
        );
    }
    let installed = !device.agents.is_empty() || !device.unauthed_agents.is_empty();
    if device.actionable() && installed {
        let device_id = device.device_id.clone();
        accounts = accounts.child(
            crate::controls::ghost_icon_button(
                ("machine-add-account", index),
                Icon::new(registry::UI_ADD),
                cx,
            )
            .size(px(CARD_ICON_SIZE))
            .tooltip(COPY_ADD_ACCOUNT)
            .on_click(move |_, window, cx| {
                if inert {
                    return;
                }
                crate::agent_login::open_add_account_dialog_for(device_id.clone(), window, cx);
            }),
        );
    }

    let panel = open_slot.map(|slot| {
        let status = login_flags.get(slot).copied().flatten();
        account_panel(index * 64 + slot, &device, &device.logins[slot], status, inert, cx)
    });

    crate::surface::glass_card()
        .w_full()
        .min_w_0()
        .p(px(CARD_PADDING))
        .gap_3()
        // EXP-1560: the offline group is dimmed.
        .when(!device.online, |this| this.opacity(0.6))
        .child(top)
        .child(
            gpui_component::v_flex()
                .w_full()
                .min_w_0()
                .gap(px(domain::devices_list::CARD_PANEL_GAP))
                .child(accounts)
                .children(panel),
        )
        .into_any_element()
}

/// EXP-1560 — the card's `⋯` (`copy.deviceActions`) opening THE menu: the
/// update line (disabled, muted), `Update` (a server's; busy while
/// requested), Device settings (my devices). A team device's menu holds
/// only a team owner's Remove; a card with no item keeps the slot empty.
fn device_menu(
    index: usize,
    device: &DeviceCard,
    update_line: Option<UpdateLine>,
    on_drop_share: Option<RowHandler>,
    inert: bool,
    cx: &App,
) -> gpui::AnyElement {
    if !device.mine && on_drop_share.is_none() {
        return div().size(px(CARD_ICON_SIZE)).flex_shrink_0().into_any_element();
    }
    let button = crate::controls::ghost_icon_button(
        ("machine-actions", index),
        Icon::new(registry::UI_MORE),
        cx,
    )
    .size(px(CARD_ICON_SIZE))
    .tooltip(COPY_DEVICE_ACTIONS);
    if inert {
        return button.into_any_element();
    }
    let mine = device.mine;
    let settings_row_id = device.row_id.clone();
    div()
        .flex_shrink_0()
        .child(button.pointer_menu(move |menu, _window, cx| {
            let mut menu = menu;
            if let Some(line) = update_line.as_ref() {
                menu = menu.item(crate::controls::pointer_label_item(line.text.clone(), true));
                if let Some(pill) = line.update.clone() {
                    let on_click = pill.on_click.clone();
                    menu = menu.item(
                        crate::controls::pointer_label_item(pill.label, pill.busy)
                            .icon(Icon::new(registry::UI_UPDATE))
                            .on_click(move |_: &gpui::ClickEvent, window, cx| {
                                if let Some(on_click) = on_click.as_ref() {
                                    on_click(window, cx);
                                }
                            }),
                    );
                }
            }
            if mine {
                let row_id = settings_row_id.clone();
                menu = menu.item(
                    crate::controls::pointer_label_item(COPY_SETTINGS, false)
                        .icon(Icon::new(registry::NAV_SETTINGS))
                        .on_click(move |_: &gpui::ClickEvent, window, cx| {
                            crate::device_settings::open(window, cx, row_id.clone());
                        }),
                );
            }
            if let Some(on_drop_share) = on_drop_share.clone() {
                menu = menu.item(
                    crate::controls::danger_menu_item(
                        COPY_REMOVE_FROM_TEAM,
                        Icon::new(registry::UI_DELETE),
                        cx,
                    )
                    .on_click(move |_: &gpui::ClickEvent, window, cx| on_drop_share(window, cx)),
                );
            }
            menu
        }))
        .into_any_element()
}

/// The muted outlined tag the status line wears ("Shared", "This device").
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

/// #EXP-1643 — THE account panel a press on an icon expands the card with
/// (`devices-list.json`; web `DeviceAccountPanel`): a hairline, the email (else
/// [`crate::usage_bar::login_label`]) + the icon-only Sign out and Remove, the
/// plan + the one action pill, the persisted line, the status in warning tone,
/// the usage windows. A team or offline device: the same panel, no actions.
fn account_panel(
    id: usize,
    device: &DeviceCard,
    login: &AgentProfileUsageRow,
    status: Option<LoginStatus>,
    inert: bool,
    cx: &App,
) -> gpui::Div {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let label = crate::usage_bar::login_label(login);
    let title = login.email.clone().unwrap_or_else(|| label.clone());
    // The plan rides UNDER an address only: a title that already is the plan
    // must not print it twice.
    let plan = login.email.as_ref().and(login.plan.clone());
    let actions = if device.actionable() {
        chip_actions(
            login.signed_in,
            login.health,
            (login.agent == coding::CodingAgent::Claude.id()).then_some(
                login.token_expires_at.as_deref().is_some_and(|at| !at.is_empty()),
            ),
        )
    } else {
        Vec::new()
    };
    let handler = |action: ChipAction| (!inert).then(|| account_action(action, device, login, &label)).flatten();

    let mut name_line = gpui_component::h_flex()
        .w_full()
        .min_w_0()
        .min_h(px(CARD_ICON_SIZE))
        .items_center()
        .gap_0p5()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_sm()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(theme.foreground)
                .child(SharedString::from(title)),
        );
    for action in actions.iter().copied().filter(|action| action.destructive()) {
        name_line = name_line.child(panel_icon_button((id, action), handler(action), cx));
    }
    let pill = actions.iter().copied().find(|action| !action.destructive()).map(|action| {
        let on_click = handler(action);
        div().flex_shrink_0().child(
            crate::surface::glass_pill_button(
                ("machine-account-action", id),
                crate::surface::PillSize::Sm,
                cx,
            )
            .icon(action.icon())
            .label(action.label())
            .on_click(move |_, window, cx| {
                if let Some(on_click) = on_click.as_ref() {
                    on_click(window, cx);
                }
            }),
        )
    });
    let usage = login.usage.as_ref().filter(|usage| {
        crate::usage_bar::usage_state(login.signed_in, login.unmonitored, Some(usage))
            == crate::usage_bar::UsageState::Ready
    });
    let now_epoch = chrono::Utc::now().timestamp();

    gpui_component::v_flex()
        .w_full()
        .min_w_0()
        .gap_3()
        .border_t_1()
        .border_color(theme.border)
        .pt(px(domain::devices_list::CARD_PANEL_GAP))
        .child(
            gpui_component::v_flex()
                .w_full()
                .min_w_0()
                .gap_0p5()
                .child(name_line)
                .when(plan.is_some() || pill.is_some(), |this| {
                    this.child(
                        gpui_component::h_flex()
                            .w_full()
                            .min_w_0()
                            .min_h(px(CARD_ICON_SIZE))
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_xs()
                                    .text_color(muted)
                                    .children(plan.map(SharedString::from)),
                            )
                            .children(pill),
                    )
                })
                .when_some(persisted_line(login), |this, line| {
                    this.child(div().text_xs().text_color(muted).child(line))
                })
                .when_some(status, |this, status| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(theme::tokens::YELLOW.to_hsla())
                            .child(status.label()),
                    )
                }),
        )
        .when_some(usage, |this, usage| {
            this.child(crate::usage_bar::render_usage_windows(usage, now_epoch, cx))
        })
}

/// #EXP-1643 — one icon-only panel action (`card.iconSize`, named by its
/// tooltip, the destructive tone on hover only). A plain element because the
/// `Button` painter has no hover foreground.
fn panel_icon_button(
    (id, action): (usize, ChipAction),
    on_click: Option<RowHandler>,
    cx: &App,
) -> gpui::AnyElement {
    const GROUP: &str = "machine-account-icon-action";
    let theme = cx.theme();
    let hover = theme.list_hover;
    let danger = theme.danger;
    let label = SharedString::from(action.label());
    let element_id = match action {
        ChipAction::Remove => ("machine-account-remove", id),
        _ => ("machine-account-sign-out", id),
    };
    div()
        .id(element_id)
        .role(gpui::Role::Button)
        .aria_label(label.clone())
        .group(GROUP)
        .size(px(CARD_ICON_SIZE))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(crate::controls::ide_chrome().radius_md))
        .cursor_pointer()
        .hover(move |this| this.bg(hover))
        // The web row's `-mr-1`: the last glyph sits on the card's padding.
        .when(action == ChipAction::Remove, |this| this.mr(px(-4.)))
        .tooltip(move |window, cx| {
            gpui_component::tooltip::Tooltip::new(label.clone()).build(window, cx)
        })
        .on_click(move |_, window, cx| {
            if let Some(on_click) = on_click.as_ref() {
                on_click(window, cx);
            }
        })
        .child(
            gpui::svg()
                .path(action.icon().path())
                .size(px(CARD_MARK_SIZE))
                .flex_shrink_0()
                .text_color(theme.muted_foreground)
                .group_hover(GROUP, move |this| this.text_color(danger)),
        )
        .into_any_element()
}

/// #EXP-1643 — what a panel action runs: the sign-in flows, or the confirmed
/// Sign out / Remove. `None` for an agent this build does not know.
fn account_action(
    action: ChipAction,
    device: &DeviceCard,
    login: &AgentProfileUsageRow,
    label: &str,
) -> Option<RowHandler> {
    let agent = coding::CodingAgent::parse(&login.agent)?;
    let device_id = device.device_id.clone();
    let device_label = device.label.clone();
    let own = device.own;
    let profile_id = login.profile_id.clone();
    let account_label = SharedString::from(label.to_string());
    Some(Rc::new(move |window: &mut Window, cx: &mut App| match action {
        ChipAction::SignIn => crate::agent_login::sign_in_on_device(
            device_id.clone(),
            device_label.clone(),
            own,
            agent,
            Some(profile_id.clone()),
            window,
            cx,
        ),
        // EXP-1626: persist (or renew) THIS profile, in place.
        ChipAction::Persist { .. } => crate::agent_login::persist_login_on_device(
            device_id.clone(),
            device_label.clone(),
            own,
            profile_id.clone(),
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
    }))
}

/// EXP-1560 — ONE login as an icon: a ghost icon button holding the agent's
/// brand mark, THE run-mark badge (`needs_input` amber) when the login has an
/// attention item. #EXP-1643: a press opens or closes the card's
/// [`account_panel`], the open icon carries the glass-active wash, and a hover
/// is only the tooltip with the email (else the label).
fn account_icon(
    id: usize,
    login: &AgentProfileUsageRow,
    badged: bool,
    open: bool,
    on_toggle: Option<RowHandler>,
    cx: &App,
) -> gpui::AnyElement {
    let mark = match coding::CodingAgent::parse(&login.agent) {
        Some(agent) => crate::coding_selects::agent_mark(agent),
        None => Icon::new(registry::SETTINGS_AGENTS),
    };
    let tooltip = SharedString::from(
        login
            .email
            .clone()
            .unwrap_or_else(|| crate::usage_bar::login_label(login)),
    );
    // The button sizes its icon at 3/4 of a `Size::Size`: that is the 16px
    // mark; the box itself is the card's 28px icon square.
    let mut button = crate::controls::ghost_icon_button(("machine-login", id), mark, cx)
        .with_size(gpui_component::Size::Size(px(CARD_MARK_SIZE / 0.75)))
        .size(px(CARD_ICON_SIZE))
        .tooltip(tooltip);
    if open {
        // The hover is unmixed, the rest goes through the variant's mix
        // (`ghost_icon_button`); both read the one wash.
        let wash = crate::controls::ide_chrome().fill_active;
        button = button.custom(
            ButtonCustomVariant::new(cx)
                .color(crate::surface::custom_variant_fill(wash))
                .hover(wash)
                .active(wash)
                .foreground(cx.theme().foreground.opacity(0.7)),
        );
    }
    div()
        .relative()
        .flex_shrink_0()
        .child(button.on_click(move |_, window, cx| {
            if let Some(on_toggle) = on_toggle.as_ref() {
                on_toggle(window, cx);
            }
        }))
        .when(badged, |this| {
            this.child(
                crate::run_rows::mark_badge(
                    domain::session_tree::SessionState::NeedsYou.tone(),
                    crate::controls::ide_chrome().fill_card_opaque,
                    cx,
                )
                .top(px(4. - crate::run_rows::RUN_BADGE_RING_PX))
                .right(px(4. - crate::run_rows::RUN_BADGE_RING_PX)),
            )
        })
        .into_any_element()
}

/// Where the desktop app's builds live — the desktop card's "All platforms".
pub(crate) const DESKTOP_RELEASES_URL: &str = "https://github.com/Niach/exponential/releases/latest";

/// The desktop card's primary download (web `desktopDownloadHref`): THIS
/// OS's asset through the redirect endpoint (EXP-1693: the newest desktop-v*
/// release that has it), the releases page on anything else.
pub(crate) fn desktop_download_url() -> String {
    match std::env::consts::OS {
        os @ ("macos" | "windows" | "linux") => {
            format!("https://app.exponential.at/api/download/desktop/{os}")
        }
        _ => DESKTOP_RELEASES_URL.to_string(),
    }
}

/// The instance the install one-liner points the daemon at.
pub(crate) fn server_install_origin(cx: &gpui::App) -> String {
    queries::active_account(cx)
        .map(|account| queries::display_instance_url(&account.instance_url).trim_end_matches('/').to_string())
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
    // P33: no subtitle (the cards say it). EXP-1557: the web dialog's
    // shape: no footer (the ✕, Escape and the scrim close it), web's
    // `max-w-lg`, and the two cards' own height (#EXP-1588: the panel
    // hugs them; the body scrolls past the window cap).
    let spec = AlertSpec::new("Add device", "", "Done")
        .without_cancel()
        .without_footer()
        .width(px(512.))
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
            token_expires_at: None,
            usage_from_token: false,
        }
    }

    /// EXP-1626: no profile names — a login's label is its email, else the
    /// no-email fallback, whatever an older device still reports as `name`;
    /// the device's `usageSource: "token"` reaches the row.
    #[test]
    fn a_login_is_labelled_by_its_email_and_carries_its_usage_source() {
        let mut login = login_row(true, true);
        assert_eq!(crate::usage_bar::login_label(&login), coding::agent_accounts::NO_EMAIL_LABEL);
        login.email = Some("dev@acme.test".into());
        assert_eq!(crate::usage_bar::login_label(&login), "dev@acme.test");
        let row: domain::rows::DeviceRow = serde_json::from_value(serde_json::json!({
            "id": "row-1",
            "device_id": "dev-1",
            "user_id": "me",
            "agent_accounts": {"claude": {"signedIn": true, "checkedAt": "", "profiles": [
                {"id": "0a1b2c3d", "signedIn": true, "checkedAt": "", "name": "Work",
                 "tokenExpiresAt": "2027-11-02T00:00:00.000Z", "usageSource": "token"},
                {"id": "0a1b2c3e", "signedIn": true, "checkedAt": "", "email": "b@x.at",
                 "tokenExpiresAt": "2027-11-02T00:00:00.000Z"},
                {"id": "0a1b2c3f", "signedIn": true, "checkedAt": "", "email": "c@x.at"}
            ]}}
        }))
        .unwrap();
        let rows = crate::usage_bar::agent_profile_usage_rows(&[row], "me", |_| true);
        let by_id = |id: &str| rows.iter().find(|row| row.profile_id == id).unwrap();
        assert_eq!(crate::usage_bar::login_label(by_id("0a1b2c3d")), coding::agent_accounts::NO_EMAIL_LABEL);
        assert!(by_id("0a1b2c3d").usage_from_token);
        assert!(!by_id("0a1b2c3e").usage_from_token);
        // The panel's persisted line (`copy.persistedToken` /
        // `copy.persistedBrowser`), none without a token.
        assert_eq!(persisted_line(by_id("0a1b2c3d")), Some(COPY_PERSISTED_TOKEN));
        assert_eq!(persisted_line(by_id("0a1b2c3e")), Some(COPY_PERSISTED_BROWSER));
        assert_eq!(persisted_line(by_id("0a1b2c3f")), None);
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
            ("updateAvailable", COPY_UPDATE_AVAILABLE),
            ("updateQueued", COPY_UPDATE_QUEUED),
            ("needsRelogin", COPY_NEEDS_RELOGIN),
            ("signedOut", COPY_SIGNED_OUT),
            ("update", COPY_UPDATE),
            ("signIn", COPY_SIGN_IN),
            ("signInTitle", COPY_SIGN_IN_TITLE),
            ("signInWaiting", COPY_SIGN_IN_WAITING),
            ("limitReached", COPY_LIMIT_REACHED),
            ("deviceActions", COPY_DEVICE_ACTIONS),
            ("settings", COPY_SETTINGS),
            ("removeFromTeam", COPY_REMOVE_FROM_TEAM),
            ("moreAccounts", COPY_MORE_ACCOUNTS),
            ("longLivedRefused", COPY_LONG_LIVED_REFUSED),
            ("persistLogin", COPY_PERSIST_LOGIN),
            ("renewPersistedLogin", COPY_RENEW_PERSISTED_LOGIN),
            ("persistNote", COPY_PERSIST_NOTE),
            ("persistedBrowser", COPY_PERSISTED_BROWSER),
            ("persistedToken", COPY_PERSISTED_TOKEN),
        ] {
            assert_eq!(copy[key].as_str(), Some(ours), "copy.{key}");
        }
        for status in [
            LoginStatus::NeedsRelogin,
            LoginStatus::SignedOut,
            LoginStatus::Limit,
        ] {
            assert_eq!(
                copy["status"][status.wire()].as_str(),
                Some(status.label()),
                "copy.status.{}",
                status.wire()
            );
        }
        assert_eq!(copy["status"].as_object().map(|status| status.len()), Some(3));
        assert_eq!(copy.as_object().map(|copy| copy.len()), Some(26), "a new copy key");
        assert_eq!(COPY_LONG_LIVED_REFUSED, coding::agent_login::LONG_LIVED_NEEDS_PROFILE);
        // EXP-1626: the account panel reads the fixture's copy.
        assert_eq!(crate::usage_bar::ChipAction::Persist { renew: false }.label(), COPY_PERSIST_LOGIN);
        assert_eq!(crate::usage_bar::ChipAction::Persist { renew: true }.label(), COPY_RENEW_PERSISTED_LOGIN);
        assert!(fixture.get("keepSignedIn").is_none(), "EXP-1626: no renewal reminder");
    }

    /// EXP-1560: the card grid's geometry is the fixture's `card`.
    #[test]
    fn device_card_geometry_matches_the_fixture() {
        let fixture = fixture();
        let card = &fixture["card"];
        for (key, ours) in [
            ("minWidth", CARD_MIN_W),
            ("gap", CARD_GAP),
            ("padding", CARD_PADDING),
            ("iconSize", CARD_ICON_SIZE),
            ("markSize", CARD_MARK_SIZE),
            ("maxAccountIcons", CARD_MAX_ACCOUNT_ICONS as f32),
            ("deviceIconSize", domain::devices_list::CARD_DEVICE_ICON_SIZE),
            ("panelGap", domain::devices_list::CARD_PANEL_GAP),
        ] {
            assert_eq!(card[key].as_f64(), Some(ours as f64), "card.{key}");
        }
        assert_eq!(card.as_object().map(|card| card.len()), Some(8), "a new card key");
        // The `$comment`'s examples: a phone in portrait gets 1 column,
        // wider sections 2 or more.
        assert_eq!(device_card_columns(358.), 1);
        assert_eq!(device_card_columns(1100.), 3);
        assert_eq!(device_card_columns(0.), 1);
        assert_eq!(device_card_columns(651.), 1);
        assert_eq!(device_card_columns(652.), 2);
    }

    /// EXP-1560: [`device_card_account_icons`] replays `fixture.accountIcons`.
    #[test]
    fn device_card_account_icons_replay_the_fixture() {
        let fixture = fixture();
        let cases = fixture["accountIcons"].as_array().expect("accountIcons");
        assert!(!cases.is_empty());
        for case in cases {
            let name = case["name"].as_str().unwrap_or_default();
            let count = case["input"]["count"].as_u64().expect("count") as usize;
            let expanded = case["input"]["expanded"].as_bool().expect("expanded");
            let (visible, more) = device_card_account_icons(count, expanded);
            assert_eq!(case["expect"]["visible"].as_u64(), Some(visible as u64), "{name}: visible");
            assert_eq!(case["expect"]["more"].as_bool(), Some(more), "{name}: more");
        }
    }

    /// #EXP-1643: pressing the open icon closes (by hand); any other opens.
    #[test]
    fn pressing_an_account_icon_toggles_that_login() {
        assert_eq!(toggled_account(None, "a"), Some("a"));
        assert_eq!(toggled_account(Some("a"), "a"), None);
        assert_eq!(toggled_account(Some("a"), "b"), Some("b"));
    }

    /// EXP-1560: `limitHit` = a FRESH window at ≥ 100 %; old numbers never
    /// badge.
    #[test]
    fn limit_hit_needs_a_fresh_full_window() {
        let now = 1_800_000_000;
        let usage = |age: i64, percent: u8| coding::agent_usage::AgentUsage {
            fetched_at: chrono::DateTime::from_timestamp(now - age, 0)
                .expect("stamp")
                .to_rfc3339(),
            stale: false,
            windows: vec![
                coding::agent_usage::UsageWindow {
                    key: "session".to_string(),
                    label: "Session".to_string(),
                    percent: 12,
                    resets_at: None,
                },
                coding::agent_usage::UsageWindow {
                    key: "weekly".to_string(),
                    label: "Weekly".to_string(),
                    percent,
                    resets_at: None,
                },
            ],
        };
        assert!(limit_hit(Some(&usage(60, 100)), now));
        assert!(!limit_hit(Some(&usage(60, 99)), now));
        assert!(!limit_hit(Some(&usage(crate::usage_bar::USAGE_FRESH_SECS + 1, 100)), now));
        assert!(!limit_hit(None, now));
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
                            limit_hit: login["limitHit"].as_bool().unwrap_or(false),
                            token_expires_at: login["tokenExpiresAt"].as_str().map(str::to_string),
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
                now: chrono::DateTime::parse_from_rfc3339(
                    input["now"].as_str().unwrap_or_else(|| panic!("{name}: now")),
                )
                .unwrap_or_else(|_| panic!("{name}: now parses"))
                .with_timezone(&chrono::Utc),
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
                        AttentionKind::Login {
                            agent,
                            profile_id,
                            status,
                        } => serde_json::json!({
                            "kind": "login",
                            "profileId": profile_id,
                            "agent": agent,
                            "text": item.text,
                            "action": action,
                            "status": status.wire(),
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
        let _span = tracing::trace_span!("render", view = "MachinesSection").entered();
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
        // EXP-1560: the grid's columns from the section's probed width.
        let columns = device_card_columns(f32::from(self.measured_width.get()));
        let mine_group = self.render_group(0, mine, None, columns, cx);
        // Offset the element ids so the two groups can never collide.
        let team_group =
            self.render_group(mine.len(), team, owned_team_id.as_deref(), columns, cx);
        let measured_width = self.measured_width.clone();
        let entity = cx.entity().downgrade();

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
            .relative()
            .child(
                // Width probe (the issue list's): re-render once when a
                // resize moves the grid's column count. The notify is
                // deferred, this view is leased while it prepaints.
                gpui::canvas(
                    move |bounds, _, cx| {
                        measured_width.set(bounds.size.width);
                        if device_card_columns(f32::from(bounds.size.width)) != columns {
                            let entity = entity.clone();
                            cx.defer(move |cx| {
                                entity.update(cx, |_, cx| cx.notify()).ok();
                            });
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
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
