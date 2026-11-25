//! #EXP-2027 (sidebar-tree.json): a DEVICE's page and its SETTINGS page, the
//! two screens a device row under the sidebar's Devices opens (the Devices
//! page and its card grid are gone).
//!
//! [`DevicePageView`] = `Screen::Device`: the header (glyph, name, version,
//! Online / Offline, Settings), the device's logins with their two limit
//! bars (`Accounts`), every session running on it across the member's teams
//! with each row's team mark on its lead (`Running`); on THIS device also the four
//! This-device blocks that left Settings (settings-nav.json
//! `nav.desktopNavs.device`: Tools, Coding CLIs, Worktrees, Sessions),
//! stacked under the sections.
//!
//! [`DeviceSettingsPage`] = `Screen::DeviceSettings`: the old Device settings
//! dialog's body ([`crate::device_settings::DeviceSettingsView`]) as a page.
//!
//! Every read is the synced rows: `devices` (the logins and their usage, as
//! `machines.rs` reads them), `sessions` (`device_id` = the steer id of the
//! host), `teams`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, AnyElement, App, AppContext as _, Div, Entity, InteractiveElement as _, IntoElement,
    ParentElement, Render, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled,
    Subscription, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::rows::{DeviceRow, SessionRow};
use domain::session_tree::SessionState;

use crate::controls::PointerMenuButtonExt as _;
use crate::icons::registry;
use crate::navigation::{navigate, Screen};
use crate::usage_bar::{AgentProfileUsageRow, ChipAction, MiniWindow};

/// Online-ness is a clock question (`last_seen_at` against the contract
/// window): the page re-renders on this tick as well as on deltas.
const LIVENESS_TICK: std::time::Duration = std::time::Duration::from_secs(10);
/// One limit bar's width (label over the meter).
const LIMIT_BAR_W: f32 = 150.;
const LIMIT_TRACK_H: f32 = 4.;
const MARK_PX: f32 = 16.;
const HEADER_GLYPH_PX: f32 = 24.;

const COPY_ACCOUNTS: &str = "Accounts";
const COPY_RUNNING: &str = "Running";
const COPY_CODING_CLIS: &str = "Coding CLIs";
const COPY_ONLINE: &str = "Online";
const COPY_THIS_DEVICE: &str = "This device";
const COPY_DEVICE: &str = "Device";
const COPY_ACCOUNT_ACTIONS: &str = "Account actions";

// ---------------------------------------------------------------------------
// Resolving the device
// ---------------------------------------------------------------------------

/// This machine's steer device id (`steer::persistent_device_id`), read once
/// per data dir. `None` without a signed-in app context (tests).
fn own_steer_id(cx: &App) -> Option<String> {
    thread_local! {
        static OWN: RefCell<Option<(PathBuf, String)>> = const { RefCell::new(None) };
    }
    let data_dir = cx.try_global::<crate::session::AuthContext>()?.data_dir.clone();
    if let Some(hit) = OWN.with(|own| own.borrow().as_ref().filter(|(dir, _)| *dir == data_dir).map(|(_, id)| id.clone())) {
        return Some(hit);
    }
    let id = steer::persistent_device_id(&data_dir);
    OWN.with(|own| *own.borrow_mut() = Some((data_dir, id.clone())));
    Some(id)
}

/// The synced `devices` row of this machine, `None` while it has not synced.
pub(crate) fn this_device_row_id(cx: &App) -> Option<String> {
    let own = own_steer_id(cx)?;
    let store = sync::Store::try_global(cx)?;
    let devices = store.collections().devices.read(cx);
    devices
        .iter()
        .find(|row| row.device_id.as_deref() == Some(own.as_str()))
        .map(|row| row.id.clone())
}

/// The row a screen's `device_id` names: empty = this machine's; else the
/// `devices` row id (a steer device id resolves too).
fn resolve_row(device_id: &str, cx: &App) -> Option<DeviceRow> {
    if device_id.is_empty() {
        let id = this_device_row_id(cx)?;
        return crate::entity_preview::find_device(&id, cx);
    }
    crate::entity_preview::find_device(device_id, cx)
}

/// Whether `device_id` (empty = this one) is the machine this app runs on.
fn is_this_device(device_id: &str, row: Option<&DeviceRow>, cx: &App) -> bool {
    if device_id.is_empty() {
        return true;
    }
    let Some(own) = own_steer_id(cx) else { return false };
    device_id == own || row.and_then(|row| row.device_id.as_deref()) == Some(own.as_str())
}

/// A device's display name: its label, else its steer id.
fn row_name(row: &DeviceRow) -> String {
    row.label
        .as_deref()
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(str::to_string)
        .or_else(|| row.device_id.clone())
        .unwrap_or_else(|| COPY_DEVICE.to_string())
}

/// The device's name (the tab title, the page heading); `This device` while
/// this machine's row has not synced, `Device` for any other unsynced id.
pub(crate) fn device_title(device_id: &str, cx: &App) -> SharedString {
    if let Some(row) = resolve_row(device_id, cx) {
        return row_name(&row).into();
    }
    if is_this_device(device_id, None, cx) {
        COPY_THIS_DEVICE.into()
    } else {
        COPY_DEVICE.into()
    }
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

/// One tree running on a device ([`running_on_device`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RunningRoot {
    pub(crate) root_id: String,
    pub(crate) team_id: Option<String>,
    /// The root's agent CLI (the run mark).
    pub(crate) cli: Option<String>,
    /// `Working`, or `NeedsYou` when one of its sessions there waits on a
    /// person.
    pub(crate) state: SessionState,
    created_at: Option<String>,
}

/// The trees with a live session HOSTED on `device_id` (the steer id) that
/// works (queued or busy) or waits on a person, across every team the rows
/// come from; newest root first, ties by id (a heartbeat never reorders).
pub(crate) fn running_on_device(sessions: &[&SessionRow], device_id: &str) -> Vec<RunningRoot> {
    if device_id.is_empty() {
        return Vec::new();
    }
    let by_id: HashMap<&str, &SessionRow> = sessions.iter().map(|row| (row.id.as_str(), *row)).collect();
    let mut roots: Vec<RunningRoot> = Vec::new();
    for session in sessions.iter().copied() {
        if session.device_id.as_deref() != Some(device_id) || session.is_ended() {
            continue;
        }
        let state = if domain::session_tree::session_asks(session) {
            SessionState::NeedsYou
        } else if session.is_queued() || session.busy {
            SessionState::Working
        } else {
            continue;
        };
        let root_id = session.root_id_or_self();
        let root = by_id.get(root_id).copied().unwrap_or(session);
        match roots.iter_mut().find(|row| row.root_id == root_id) {
            Some(row) => {
                if state == SessionState::NeedsYou {
                    row.state = state;
                }
            }
            None => roots.push(RunningRoot {
                root_id: root_id.to_string(),
                team_id: root.team_id.clone().or_else(|| session.team_id.clone()),
                cli: root.cli.clone(),
                state,
                created_at: root.created_at.clone(),
            }),
        }
    }
    roots.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| a.root_id.cmp(&b.root_id)));
    roots
}

/// The two limit bars a login row draws: its `session` (5h) and `weekly`
/// windows, else the first two it reported.
pub(crate) fn limit_windows(usage: &coding::agent_usage::AgentUsage) -> Vec<MiniWindow> {
    let windows = crate::usage_bar::mini_windows(usage);
    let pair: Vec<MiniWindow> = windows
        .iter()
        .filter(|window| window.key == "session" || window.key == "weekly")
        .cloned()
        .collect();
    if pair.is_empty() {
        windows.into_iter().take(2).collect()
    } else {
        pair
    }
}

/// `<CLI> · <plan>` under a login's email (the CLI alone without a plan).
pub(crate) fn login_subtitle(agent: &str, plan: Option<&str>) -> String {
    let cli = coding::CodingAgent::parse(agent).map(|agent| agent.label().to_string()).unwrap_or_else(|| agent.to_string());
    match plan.map(str::trim).filter(|plan| !plan.is_empty()) {
        Some(plan) => format!("{cli} · {plan}"),
        None => cli,
    }
}

// ---------------------------------------------------------------------------
// Shared pieces
// ---------------------------------------------------------------------------

/// A heading line: the page title at the settings pages' size.
fn page_title(title: SharedString, cx: &App) -> Div {
    div()
        .min_w_0()
        .truncate()
        .text_2xl()
        .font_weight(gpui::FontWeight::BOLD)
        .text_color(cx.theme().foreground)
        .child(title)
}

/// One row of a section group (the settings rows' rhythm).
fn group_row() -> Div {
    h_flex().w_full().min_w_0().items_center().gap_3().px_4().py_3()
}

// ---------------------------------------------------------------------------
// The device page
// ---------------------------------------------------------------------------

/// The This-device blocks (settings-nav.json `nav.desktopNavs.device`).
struct ThisDevicePanes {
    tools: Entity<crate::settings::tools::ToolsPane>,
    clis: Entity<crate::settings::agents::AgentsPane>,
    worktrees: Entity<crate::settings::local_repos::LocalReposPane>,
    sessions: Entity<crate::settings::sessions::SessionsPane>,
}

/// `Screen::Device` (#EXP-2027).
pub(crate) struct DevicePageView {
    device_id: String,
    scroll: ScrollHandle,
    /// Built the first time this machine's page shows.
    panes: Option<ThisDevicePanes>,
    _subscriptions: Vec<Subscription>,
}

impl DevicePageView {
    pub(crate) fn new(_window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let mut subscriptions = crate::session_data::observe_session_rows(cx);
        if let Some(store) = sync::Store::try_global(cx) {
            let collections = store.collections().clone();
            subscriptions.push(cx.observe(&collections.devices, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&collections.teams, |_, _, cx| cx.notify()));
        }
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(LIVENESS_TICK).await;
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                return;
            }
        })
        .detach();
        Self { device_id: String::new(), scroll: ScrollHandle::new(), panes: None, _subscriptions: subscriptions }
    }

    /// Point the page at `device_id` (empty = this device); the same id is a
    /// no-op.
    pub(crate) fn set_device(&mut self, device_id: String, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.device_id == device_id {
            return;
        }
        self.device_id = device_id;
        self.scroll.set_offset(gpui::point(px(0.), px(0.)));
        cx.notify();
    }

    fn ensure_panes(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.panes.is_some() {
            return;
        }
        self.panes = Some(ThisDevicePanes {
            tools: cx.new(|cx| crate::settings::tools::ToolsPane::new(window, cx)),
            clis: cx.new(|cx| crate::settings::agents::AgentsPane::new(window, cx)),
            worktrees: cx.new(|cx| crate::settings::local_repos::LocalReposPane::new(window, cx)),
            sessions: cx.new(|cx| crate::settings::sessions::SessionsPane::new(window, cx)),
        });
    }

    fn render_header(&self, row: Option<&DeviceRow>, online: bool, mine: bool, cx: &App) -> Div {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let server = row.is_some_and(|row| row.is_server());
        let glyph = crate::icons::device_icon(row.and_then(|row| row.icon.as_deref()), server);
        let status = h_flex()
            .flex_shrink_0()
            .items_center()
            .gap_1p5()
            .text_sm()
            .text_color(if online { theme.foreground.opacity(0.85) } else { muted })
            .child(crate::surface::live_dot(if online { theme::tokens::GREEN.to_hsla() } else { muted }, false))
            .child(if online { COPY_ONLINE } else { crate::machines::COPY_OFFLINE });
        let settings = (mine && row.is_some()).then(|| {
            let row_id = row.map(|row| row.id.clone()).unwrap_or_default();
            crate::surface::glass_pill_button("device-page-settings", crate::surface::PillSize::Md, cx)
                .icon(registry::NAV_SETTINGS)
                .label(domain::sidebar_tree::sidebar_tree_copy().settings.clone())
                .on_click(move |_, window, cx| {
                    navigate(window, cx, Screen::DeviceSettings { device_id: row_id.clone() });
                })
        });
        h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_3()
            .child(Icon::new(glyph).size(px(HEADER_GLYPH_PX)).flex_shrink_0().text_color(theme.foreground.opacity(0.85)))
            .child(page_title(device_title(&self.device_id, cx), cx))
            .when_some(row.and_then(|row| row.version.clone()), |this, version| {
                this.child(div().flex_shrink_0().text_sm().text_color(muted).child(SharedString::from(format!("v{version}"))))
            })
            .when(row.is_some(), |this| this.child(status))
            .child(div().flex_1())
            .children(settings)
    }

    fn render_accounts(&self, row: &DeviceRow, online: bool, mine: bool, own: bool, cx: &App) -> Div {
        let me = crate::queries::active_account(cx).map(|account| account.user_id).unwrap_or_default();
        let now_ms = chrono::Utc::now().timestamp_millis();
        let logins = crate::usage_bar::sort_device_logins(crate::usage_bar::agent_profile_usage_rows(
            std::slice::from_ref(row),
            &me,
            |last_seen| crate::device_settings::row_is_online(last_seen, now_ms),
        ));
        let actionable = mine && (own || online);
        let installed = !row.agent_ids().is_empty() || !row.unauthed_agent_ids().is_empty();
        let device_id = row.device_id.clone().unwrap_or_default();
        let label: SharedString = row_name(row).into();
        let add = (actionable && installed).then(|| {
            let device_id = device_id.clone();
            crate::surface::glass_pill_button("device-page-add-account", crate::surface::PillSize::Sm, cx)
                .icon(registry::UI_ADD)
                .label(crate::machines::COPY_ADD_ACCOUNT)
                .on_click(move |_, window, cx| {
                    crate::agent_login::open_add_account_dialog_for(device_id.clone(), window, cx);
                })
                .into_any_element()
        });
        let muted = cx.theme().muted_foreground;
        let rows: Vec<Div> = if logins.is_empty() {
            let reported = row.agent_accounts.as_ref().is_some_and(|value| !value.is_null());
            vec![group_row().text_sm().text_color(muted).child(if reported {
                crate::machines::COPY_NO_ACCOUNT
            } else {
                crate::machines::COPY_CHECKING
            })]
        } else {
            logins
                .iter()
                .enumerate()
                .map(|(ix, login)| {
                    login_row(ix, login, LoginTarget { device_id: device_id.clone(), label: label.clone(), own, actionable }, cx)
                })
                .collect()
        };
        v_flex()
            .w_full()
            .min_w_0()
            .child(crate::surface::glass_section_header(COPY_ACCOUNTS, add, cx))
            .child(crate::surface::glass_group_rows(rows))
    }

    fn render_running(&self, row: &DeviceRow, cx: &mut App) -> Div {
        let Some(store) = sync::Store::try_global(cx) else { return div() };
        let collections = store.collections().clone();
        let running = {
            let sessions = collections.sessions.read(cx);
            let rows: Vec<&SessionRow> = sessions.iter().collect();
            running_on_device(&rows, row.device_id.as_deref().unwrap_or_default())
        };
        let teams: HashMap<String, (String, Option<String>)> = collections
            .teams
            .read(cx)
            .iter()
            .map(|team| (team.id.clone(), (team.name.clone(), team.icon_url.clone())))
            .collect();
        let count = div()
            .flex_shrink_0()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(SharedString::from(running.len().to_string()))
            .into_any_element();
        let mut rows: Vec<Div> = Vec::new();
        for (ix, root) in running.iter().enumerate() {
            let title = crate::session_data::session_title_of(&root.root_id, cx).unwrap_or_default();
            let team = root.team_id.as_deref().and_then(|id| teams.get(id)).cloned();
            let row_team = team.as_ref().map(|(name, icon)| crate::user_avatar::RowTeam { name: name.clone().into(), image: crate::user_avatar::team_image(icon.as_deref(), cx) });
            let theme = cx.theme();
            let (muted, hover) = (theme.muted_foreground, theme.list_hover);
            let root_id = root.root_id.clone();
            let trailing = team.map(|(name, _)| {
                div().flex_shrink_0().max_w(px(220.)).truncate().text_sm().text_color(muted).child(SharedString::from(name))
            });
            rows.push(
                div().w_full().child(
                    group_row()
                        .id(("device-page-running", ix))
                        .role(gpui::Role::Link)
                        .aria_label(SharedString::from(title.clone()))
                        .py_2p5()
                        .cursor_pointer()
                        .hover(move |style| style.bg(hover))
                        .on_click(move |_, window, cx| {
                            let screen = crate::navigation::session_screen_for(&root_id, cx);
                            navigate(window, cx, screen);
                        })
                        .child(crate::user_avatar::with_team_mark(
                            crate::run_rows::run_mark(crate::run_rows::run_agent(root.cli.as_deref()), root.state, cx),
                            domain::list_item::MARK,
                            row_team.as_ref(),
                            crate::user_avatar::card_ground(),
                            cx,
                        ))
                        .child(div().flex_1().min_w_0().truncate().text_sm().child(SharedString::from(title)))
                        .children(trailing),
                ),
            );
        }
        v_flex()
            .w_full()
            .min_w_0()
            .child(crate::surface::glass_section_header(COPY_RUNNING, Some(count), cx))
            .when(!rows.is_empty(), |this| this.child(crate::surface::glass_group_rows(rows)))
    }
}

/// What a login row's actions act on.
struct LoginTarget {
    device_id: String,
    label: SharedString,
    own: bool,
    actionable: bool,
}

/// ONE login: the provider mark, the email over `<CLI> · <plan>`, its two
/// limit bars; its actions behind a hover `⋯` (the old account panel's).
fn login_row(ix: usize, login: &AgentProfileUsageRow, target: LoginTarget, cx: &App) -> Div {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let mark = match coding::CodingAgent::parse(&login.agent) {
        Some(agent) => crate::coding_selects::agent_mark(agent),
        None => Icon::new(registry::SETTINGS_AGENTS),
    };
    let title = login.email.clone().unwrap_or_else(|| crate::usage_bar::login_label(login));
    let subtitle = login_subtitle(&login.agent, login.plan.as_deref());
    let usage = login.usage.as_ref().filter(|usage| {
        crate::usage_bar::usage_state(login.signed_in, login.unmonitored, Some(usage)) == crate::usage_bar::UsageState::Ready
    });
    let now_epoch = chrono::Utc::now().timestamp();
    let bars = usage.map(|usage| {
        let stale = crate::usage_bar::usage_age(Some(usage), now_epoch).is_some();
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap_6()
            .when(stale, |this| this.opacity(0.5))
            .children(limit_windows(usage).into_iter().map(|window| {
                v_flex()
                    .w(px(LIMIT_BAR_W))
                    .gap_1p5()
                    .child(
                        div()
                            .w_full()
                            .text_right()
                            .text_xs()
                            .text_color(muted)
                            .child(SharedString::from(format!("{} · {}%", window.label, window.percent))),
                    )
                    .child(crate::usage_bar::meter(window.percent, crate::usage_bar::severity(window.percent), LIMIT_TRACK_H, cx))
            }))
    });
    let actions = if target.actionable {
        crate::usage_bar::chip_actions(
            login.signed_in,
            login.health,
            (login.agent == coding::CodingAgent::Claude.id())
                .then_some(login.token_expires_at.as_deref().is_some_and(|at| !at.is_empty())),
        )
    } else {
        Vec::new()
    };
    let group: SharedString = format!("device-page-login-{ix}").into();
    let menu = (!actions.is_empty()).then(|| {
        let handlers: Vec<(ChipAction, Option<Rc<dyn Fn(&mut Window, &mut App)>>)> =
            actions.iter().map(|action| (*action, account_action(*action, &target, login))).collect();
        div()
            .flex_shrink_0()
            .invisible()
            .group_hover(group.clone(), |style| style.visible())
            .child(
                crate::controls::ghost_icon_button(("device-page-login-actions", ix), Icon::new(registry::UI_MORE), cx)
                    .tooltip(COPY_ACCOUNT_ACTIONS)
                    .pointer_menu(move |menu, _window, cx| {
                        let mut menu = menu;
                        for (action, handler) in handlers.iter().cloned() {
                            menu = menu.item((if action.destructive() {
                                crate::controls::danger_menu_item(action.label(), Icon::new(action.icon()), cx)
                            } else {
                                crate::controls::pointer_label_item(action.label(), false).icon(Icon::new(action.icon()))
                            })
                            .on_click(move |_: &gpui::ClickEvent, window, cx| {
                                if let Some(handler) = handler.as_ref() {
                                    handler(window, cx);
                                }
                            }));
                        }
                        menu
                    }),
            )
    });
    group_row()
        .group(group)
        .child(mark.size(px(MARK_PX)).flex_shrink_0().text_color(muted))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .child(div().min_w_0().truncate().text_sm().text_color(theme.foreground).child(SharedString::from(title)))
                .child(
                    h_flex()
                        .min_w_0()
                        .items_center()
                        .gap_2()
                        .child(div().min_w_0().truncate().text_xs().text_color(muted).child(SharedString::from(subtitle)))
                        .children(crate::usage_bar::health_badge(login.health, cx)),
                ),
        )
        .children(bars)
        .children(menu)
}

/// What one login action runs (the old account panel's `account_action`).
fn account_action(
    action: ChipAction,
    target: &LoginTarget,
    login: &AgentProfileUsageRow,
) -> Option<Rc<dyn Fn(&mut Window, &mut App)>> {
    let agent = coding::CodingAgent::parse(&login.agent)?;
    let device_id = target.device_id.clone();
    let device_label = target.label.clone();
    let own = target.own;
    let profile_id = login.profile_id.clone();
    let account_label = SharedString::from(crate::usage_bar::login_label(login));
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

impl Render for DevicePageView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "DevicePageView").entered();
        let row = resolve_row(&self.device_id, cx);
        let this_device = is_this_device(&self.device_id, row.as_ref(), cx);
        let now_ms = chrono::Utc::now().timestamp_millis();
        let online = row
            .as_ref()
            .is_some_and(|row| crate::device_settings::row_is_online(row.last_seen_at.as_deref(), now_ms));
        let me = crate::queries::active_account(cx).map(|account| account.user_id);
        let mine = row.as_ref().and_then(|row| row.user_id.as_deref()).is_some_and(|user| me.as_deref() == Some(user));

        let mut column = v_flex().gap_6().child(self.render_header(row.as_ref(), online, mine, cx));
        if let Some(row) = row.as_ref() {
            column = column
                .child(self.render_accounts(row, online, mine, this_device, cx))
                .child(self.render_running(row, cx));
        }
        if this_device {
            self.ensure_panes(window, cx);
        }
        if let Some(panes) = self.panes.as_ref().filter(|_| this_device) {
            column = column
                .child(panes.tools.clone())
                .child(
                    v_flex()
                        .w_full()
                        .min_w_0()
                        .child(crate::surface::glass_section_header(COPY_CODING_CLIS, None, cx))
                        .child(panes.clis.clone()),
                )
                .child(panes.worktrees.clone())
                .child(panes.sessions.clone());
        }
        crate::scroll_pane::page_scaffold(
            "device-page-scroll",
            &self.scroll,
            // The settings pages' 16px rem (EXP-1557): the This-device blocks
            // are those panes, and the sections around them match.
            div().child(crate::controls::web_rem(column.w_full().min_w_0())),
            window,
            cx,
        )
    }
}

// ---------------------------------------------------------------------------
// The device settings page
// ---------------------------------------------------------------------------

/// `Screen::DeviceSettings` (#EXP-2027): the Device settings body as a page.
pub(crate) struct DeviceSettingsPage {
    device_id: String,
    /// The body, keyed by the `devices` row id it edits.
    body: Option<(String, Entity<crate::device_settings::DeviceSettingsView>)>,
    scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl DeviceSettingsPage {
    pub(crate) fn new(_window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let mut subscriptions = Vec::new();
        if let Some(store) = sync::Store::try_global(cx) {
            let devices = store.collections().devices.clone();
            subscriptions.push(cx.observe(&devices, |_, _, cx| cx.notify()));
        }
        Self { device_id: String::new(), body: None, scroll: ScrollHandle::new(), _subscriptions: subscriptions }
    }

    /// Point the page at `device_id` (empty = this device); the same id is a
    /// no-op.
    pub(crate) fn set_device(&mut self, device_id: String, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.device_id == device_id {
            return;
        }
        self.device_id = device_id;
        self.scroll.set_offset(gpui::point(px(0.), px(0.)));
        cx.notify();
    }
}

impl Render for DeviceSettingsPage {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "DeviceSettingsPage").entered();
        let row_id = resolve_row(&self.device_id, cx).map(|row| row.id);
        match row_id {
            Some(row_id) if self.body.as_ref().is_none_or(|(at, _)| *at != row_id) => {
                let view = cx.new(|cx| crate::device_settings::DeviceSettingsView::new_page(row_id.clone(), window, cx));
                self.body = Some((row_id, view));
            }
            Some(_) => {}
            None => self.body = None,
        }
        let device_id = self.device_id.clone();
        let back = crate::session_screen::back_link_row(
            "device-settings-back",
            device_title(&self.device_id, cx).to_string(),
            move |window, cx| crate::navigation::go_back_to(window, cx, Screen::Device { device_id: device_id.clone() }),
            cx,
        );
        let body: AnyElement = match self.body.as_ref() {
            Some((_, view)) => view.clone().into_any_element(),
            None => crate::settings::settings_skeleton("device-settings-skeleton", cx),
        };
        let column = v_flex()
            .w_full()
            .min_w_0()
            .gap_4()
            .child(back)
            .child(page_title(device_title(&self.device_id, cx), cx))
            .child(body);
        crate::scroll_pane::page_scaffold(
            "device-settings-page-scroll",
            &self.scroll,
            div().child(crate::controls::web_rem(column)),
            window,
            cx,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn session(value: serde_json::Value) -> SessionRow {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn running_rows_gather_busy_trees_on_the_device_across_teams() {
        let rows = [
            session(json!({"id": "r1", "team_id": "t1", "root_id": "r1", "device_id": "d1", "cli": "claude", "status": "running", "busy": true, "created_at": "2026-11-20T09:00:00.000Z"})),
            // A child on the device keeps its idle root (another device) listed.
            session(json!({"id": "r2", "team_id": "t2", "root_id": "r2", "device_id": "d2", "status": "running", "created_at": "2026-11-20T10:00:00.000Z"})),
            session(json!({"id": "c2", "team_id": "t2", "root_id": "r2", "parent_id": "r2", "device_id": "d1", "status": "queued"})),
            // Idle, ended, or elsewhere: not running here.
            session(json!({"id": "r3", "team_id": "t1", "root_id": "r3", "device_id": "d1", "status": "running", "created_at": "2026-11-20T11:00:00.000Z"})),
            session(json!({"id": "r4", "team_id": "t1", "root_id": "r4", "device_id": "d1", "status": "ended", "busy": true})),
            session(json!({"id": "r5", "team_id": "t1", "root_id": "r5", "device_id": "d2", "status": "running", "busy": true})),
        ];
        let refs: Vec<&SessionRow> = rows.iter().collect();
        let running = running_on_device(&refs, "d1");
        let ids: Vec<(&str, Option<&str>)> = running.iter().map(|row| (row.root_id.as_str(), row.team_id.as_deref())).collect();
        assert_eq!(ids, vec![("r2", Some("t2")), ("r1", Some("t1"))]);
        assert!(running.iter().all(|row| row.state == SessionState::Working));
        assert!(running_on_device(&refs, "").is_empty());
    }

    #[test]
    fn a_tree_waiting_on_a_person_reads_needs_you() {
        let rows = [
            session(json!({"id": "r1", "root_id": "r1", "device_id": "d1", "status": "running", "busy": true})),
            session(json!({"id": "c1", "root_id": "r1", "parent_id": "r1", "device_id": "d1", "status": "running", "needs_input": true})),
        ];
        let refs: Vec<&SessionRow> = rows.iter().collect();
        let running = running_on_device(&refs, "d1");
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].state, SessionState::NeedsYou);
    }

    #[test]
    fn a_login_subtitle_names_the_cli_then_the_plan() {
        assert_eq!(login_subtitle("claude", Some("Max")), "Claude Code · Max");
        assert_eq!(login_subtitle("codex", Some(" ")), "Codex");
        assert_eq!(login_subtitle("other", None), "other");
    }

    #[test]
    fn limit_bars_are_the_five_hour_and_weekly_windows() {
        let usage = crate::usage_bar::parse_agent_usage(&json!({
            "fetchedAt": "2026-11-20T10:00:00.000Z",
            "windows": [
                {"key": "session", "label": "5h", "percent": 42},
                {"key": "weekly", "label": "Week", "percent": 86},
                {"key": "model:fable", "label": "Fable", "percent": 10}
            ]
        }))
        .unwrap();
        let labels: Vec<(String, u8)> = limit_windows(&usage).into_iter().map(|window| (window.label, window.percent)).collect();
        assert_eq!(labels, vec![("5h".to_string(), 42), ("Week".to_string(), 86)]);
        // The amber tone past the shared threshold.
        assert_eq!(crate::usage_bar::severity(86), crate::usage_bar::Severity::Warning);
        assert_eq!(crate::usage_bar::severity(42), crate::usage_bar::Severity::Normal);
    }
}
