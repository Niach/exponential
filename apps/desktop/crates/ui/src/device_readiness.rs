//! EXP-1196/1218/1219 — THE device readiness block, the desktop's renderer of
//! `packages/domain-contract/fixtures/device-doctor.json`.
//!
//! The device builds the block (`coding::device_doctor`); this module turns
//! one into rows and draws them. Two halves:
//!
//! - **The row model** ([`sections`], [`row_for`], [`blocking_row`]) — pure,
//!   fixture-locked below: groups as bands with their tag, one row per item
//!   (state glyph + tone, label, the device-written detail, at most ONE
//!   action pill), the `computer_use` item as the switch row, permission rows
//!   indented and hidden while their parent is off. `local` decides which
//!   actions are offered: on the device itself every one, for ANOTHER device
//!   only the fixture's `remote` ones.
//! - **The view** ([`render_sections`], [`render_row`]) on the shared
//!   `surface::` recipes (the group band, flat rows, glass pills, the toggle
//!   row with no description). No subtitles, no footers: label + state + one
//!   action.
//!
//! Hosts: Settings → Tools and the first-run devices step
//! (`settings::doctor_section::DoctorPanel`, THIS device), the device
//! settings dialog (that device, local or remote), and the Agent composer
//! (the ONE failing row under it, [`blocking_row`]).

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui::{
    div, px, AnyElement, App, Div, IntoElement, ParentElement, SharedString, Styled, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _};

use coding::device_doctor::{
    self, DeviceDoctor, DoctorAction, DoctorGroup, DoctorItem, DoctorState, KEY_COMPUTER_USE,
    KEY_GIT,
};
use coding::CodingAgent;

use crate::icons::registry;
use crate::surface;

/// The fixture's `copy.localTitle` — the first-run step's title.
pub(crate) const LOCAL_TITLE: &str = "Set up this device";
/// The fixture's `copy.skip`.
pub(crate) const SKIP: &str = "Skip for now";
/// The fixture's `copy.continue`: the first-run footer's filled pill once
/// [`first_run_ready`].
pub(crate) const CONTINUE: &str = "Continue";
/// The fixture's `copy.recheck`.
pub(crate) const RECHECK: &str = "Check again";
/// The install row's second pill (THIS device only): reveals the CLI path
/// field for a binary off PATH.
pub(crate) const CUSTOM_PATH: &str = "Custom path";
pub(crate) const SAVE_PATH: &str = "Save path";

// ---------------------------------------------------------------------------
// The row model (pure)
// ---------------------------------------------------------------------------

/// The fixture's `states[].tone`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tone {
    Success,
    Warning,
    Muted,
    Destructive,
}

/// The fixture's `states[].glyph`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Glyph {
    Check,
    Alert,
    Dash,
    X,
}

/// The fixture's `states` table.
pub(crate) fn state_glyph(state: DoctorState) -> (Glyph, Tone) {
    match state {
        DoctorState::Ok => (Glyph::Check, Tone::Success),
        DoctorState::Action => (Glyph::Alert, Tone::Warning),
        DoctorState::Missing | DoctorState::Off => (Glyph::Dash, Tone::Muted),
        DoctorState::Error => (Glyph::X, Tone::Destructive),
    }
}

/// A detail reads warning for `action`, destructive for `error`, muted
/// otherwise.
pub(crate) fn detail_tone(state: DoctorState) -> Tone {
    match state {
        DoctorState::Action => Tone::Warning,
        DoctorState::Error => Tone::Destructive,
        _ => Tone::Muted,
    }
}

/// The ONE trailing pill a row may carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RowPill {
    pub action: DoctorAction,
    /// The block's filled pill (at most one per block).
    pub primary: bool,
}

/// One rendered row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReadinessRow {
    pub key: String,
    pub label: String,
    pub state: DoctorState,
    /// `None` on the switch row (label + switch, no glyph).
    pub glyph: Option<(Glyph, Tone)>,
    /// The device-written detail; never on the switch row.
    pub detail: Option<String>,
    pub detail_tone: Tone,
    /// `Some(on)` = THE computer-use switch row.
    pub switch: Option<bool>,
    /// A permission row under its parent.
    pub indent: bool,
    pub pill: Option<RowPill>,
}

/// One group band and its rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReadinessSection {
    pub group: DoctorGroup,
    pub label: &'static str,
    pub tag: Option<&'static str>,
    pub rows: Vec<ReadinessRow>,
}

/// Lenient decode of a synced `devices.doctor` value: an item this build
/// cannot read (a newer device's state or group) is dropped, never the whole
/// block. `None` = no block (null, or nothing readable).
pub(crate) fn parse(value: Option<&serde_json::Value>) -> Option<DeviceDoctor> {
    let value = value?;
    let items: Vec<DoctorItem> = value
        .get("items")?
        .as_array()?
        .iter()
        .filter_map(|item| serde_json::from_value(item.clone()).ok())
        .collect();
    if items.is_empty() {
        return None;
    }
    Some(DeviceDoctor {
        checked_at: value
            .get("checkedAt")
            .and_then(|at| at.as_str())
            .unwrap_or_default()
            .to_string(),
        items,
    })
}

/// The first-run footer rule: `continue` (filled) instead of `skip` once Git
/// is ok and at least one agent is runnable.
pub(crate) fn first_run_ready(doctor: Option<&DeviceDoctor>) -> bool {
    doctor.is_some_and(|doctor| !device_doctor::runnable_agents(doctor).is_empty())
}

/// The bare Computer use switch row for a device with no doctor (an older
/// build): no block, but the switch stays reachable (the fixture's last
/// rule). Label + switch, nothing else.
pub(crate) fn bare_switch_row(on: bool) -> ReadinessRow {
    ReadinessRow {
        key: KEY_COMPUTER_USE.to_string(),
        label: device_doctor::label(KEY_COMPUTER_USE).to_string(),
        state: if on { DoctorState::Ok } else { DoctorState::Off },
        glyph: None,
        detail: None,
        detail_tone: Tone::Muted,
        switch: Some(on),
        indent: false,
        pill: None,
    }
}

/// Whether `action` is offered here (the fixture's `remote` rule).
pub(crate) fn offered(action: DoctorAction, local: bool) -> bool {
    local || action.remote()
}

/// The pill a row carries in this mode, if any.
fn row_action(item: &DoctorItem, local: bool) -> Option<DoctorAction> {
    item.action.filter(|action| offered(*action, local))
}

/// The row whose pill is the block's PRIMARY one: the first `action`/`error`
/// row (`device_doctor::first_action`, the fixture's `localPrimary`) that
/// carries a pill here — the switch row never does, and for another device
/// a non-remote action renders none, so the filled pill passes down.
pub(crate) fn primary_key(doctor: &DeviceDoctor, local: bool) -> Option<&str> {
    doctor
        .items
        .iter()
        .find(|item| {
            matches!(item.state, DoctorState::Action | DoctorState::Error)
                && row_action(item, local).is_some()
        })
        .map(|item| item.key.as_str())
}

fn build_row(
    item: &DoctorItem,
    local: bool,
    primary: Option<&str>,
    switch_on: Option<bool>,
) -> ReadinessRow {
    let is_switch = item.key == KEY_COMPUTER_USE;
    let pill = row_action(item, local).map(|action| RowPill {
        action,
        primary: primary == Some(item.key.as_str()),
    });
    ReadinessRow {
        key: item.key.clone(),
        label: device_doctor::label(&item.key).to_string(),
        state: item.state,
        glyph: (!is_switch).then(|| state_glyph(item.state)),
        detail: if is_switch { None } else { item.detail.clone() },
        detail_tone: detail_tone(item.state),
        switch: is_switch.then(|| switch_on.unwrap_or(item.state != DoctorState::Off)),
        indent: item.parent.is_some(),
        pill: if is_switch { None } else { pill },
    }
}

/// THE block as bands of rows. `switch_on` overrides the computer-use
/// switch with a value the host already wrote but the device has not echoed
/// yet (`None` = the block's own state); children hide while it is off.
pub(crate) fn sections(
    doctor: &DeviceDoctor,
    local: bool,
    switch_on: Option<bool>,
) -> Vec<ReadinessSection> {
    let primary = primary_key(doctor, local);
    let parent_on = |parent: &str| {
        let switch = (parent == KEY_COMPUTER_USE).then_some(switch_on).flatten();
        switch.unwrap_or_else(|| {
            doctor
                .items
                .iter()
                .find(|item| item.key == parent)
                .is_some_and(|item| item.state != DoctorState::Off)
        })
    };
    DoctorGroup::ALL
        .into_iter()
        .filter_map(|group| {
            let rows: Vec<ReadinessRow> = doctor
                .items
                .iter()
                .filter(|item| item.group == group)
                .filter(|item| item.parent.as_deref().is_none_or(parent_on))
                .map(|item| build_row(item, local, primary, switch_on))
                .collect();
            (!rows.is_empty()).then(|| ReadinessSection {
                group,
                label: group.label(),
                tag: group.tag(),
                rows,
            })
        })
        .collect()
}

/// ONE row alone (the composer's single-row mode): its pill, if offered,
/// is the primary one — it is the only action on screen.
pub(crate) fn row_for(doctor: &DeviceDoctor, key: &str, local: bool) -> Option<ReadinessRow> {
    let item = doctor.items.iter().find(|item| item.key == key)?;
    let mut row = build_row(item, local, Some(key), None);
    row.indent = false;
    Some(row)
}

/// The row that keeps `agent` from running on this device: Git when Git is
/// the failure, else that agent's row while it is not `ok`. `None` = the
/// block has nothing against it.
pub(crate) fn blocking_row(
    doctor: &DeviceDoctor,
    agent: CodingAgent,
    local: bool,
) -> Option<ReadinessRow> {
    let failing = |key: &str| {
        doctor
            .items
            .iter()
            .any(|item| item.key == key && item.state != DoctorState::Ok)
    };
    if failing(KEY_GIT) {
        return row_for(doctor, KEY_GIT, local);
    }
    if failing(agent.id()) {
        return row_for(doctor, agent.id(), local);
    }
    None
}

/// A row as one tooltip line: `Claude Code · 2.1.222 · needs 2.1.263`.
pub(crate) fn summary(row: &ReadinessRow) -> String {
    match &row.detail {
        Some(detail) if !detail.is_empty() => format!("{} · {detail}", row.label),
        _ => row.label.clone(),
    }
}

/// Why no agent can run, as a row: the first agent row that has a fix in
/// reach (`action`), else `None` (every agent simply missing — the hosts'
/// install copy says it better).
pub(crate) fn no_agent_row(doctor: &DeviceDoctor, local: bool) -> Option<ReadinessRow> {
    if !device_doctor::runnable_agents(doctor).is_empty() {
        return None;
    }
    if let Some(git) = blocking_git(doctor, local) {
        return Some(git);
    }
    let item = doctor
        .items
        .iter()
        .find(|item| item.group == DoctorGroup::Agents && item.state == DoctorState::Action)?;
    row_for(doctor, &item.key, local)
}

fn blocking_git(doctor: &DeviceDoctor, local: bool) -> Option<ReadinessRow> {
    doctor
        .items
        .iter()
        .any(|item| item.key == KEY_GIT && item.state != DoctorState::Ok)
        .then(|| row_for(doctor, KEY_GIT, local))
        .flatten()
}

/// The agent a row's key names (`claude`/`codex`).
pub(crate) fn agent_for(key: &str) -> Option<CodingAgent> {
    CodingAgent::ALL.into_iter().find(|agent| agent.id() == key)
}

// ---------------------------------------------------------------------------
// The view
// ---------------------------------------------------------------------------

/// A pill click: the row's key and its action.
pub(crate) type ActionHandler = Rc<dyn Fn(&str, DoctorAction, &mut Window, &mut App)>;
/// The computer-use switch.
pub(crate) type ToggleHandler = Rc<dyn Fn(bool, &mut Window, &mut App)>;

/// What a host hangs on a row beyond the model: a second trailing control
/// (the install row's Custom path pill) and a line under it (the path field,
/// a command's outcome).
#[derive(Default)]
pub(crate) struct RowExtras {
    pub trailing: Option<AnyElement>,
    pub below: Option<AnyElement>,
}

/// The host's wiring. No handler = inert (the styleguide).
#[derive(Default)]
pub(crate) struct BlockProps {
    /// Element-id prefix (two blocks may share a window).
    pub id: SharedString,
    pub on_action: Option<ActionHandler>,
    pub on_toggle: Option<ToggleHandler>,
    /// Row keys whose action is running (the pill shows its spinner).
    pub busy: HashSet<String>,
    pub extras: HashMap<String, RowExtras>,
}

fn tone_color(tone: Tone, cx: &App) -> gpui::Hsla {
    let theme = cx.theme();
    match tone {
        Tone::Success => theme.success,
        Tone::Warning => theme.warning,
        Tone::Muted => theme.muted_foreground,
        Tone::Destructive => theme.danger,
    }
}

fn glyph_icon(glyph: Glyph) -> crate::icons::ExpIcon {
    match glyph {
        Glyph::Check => registry::UI_CHECK,
        Glyph::Alert => registry::UI_WARNING,
        Glyph::Dash => registry::UI_MINUS,
        Glyph::X => registry::UI_CLOSE,
    }
}

/// The band's trailing tag (`optional`): plain muted text, like the web's.
fn tag_pill(_id: SharedString, tag: &'static str, cx: &App) -> AnyElement {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(tag)
        .into_any_element()
}

/// ONE row: glyph, label, detail, extras, the pill — or the switch row.
pub(crate) fn render_row(
    row: &ReadinessRow,
    props: &mut BlockProps,
    cx: &App,
) -> AnyElement {
    let id = props.id.clone();
    let mut extras = props.extras.remove(&row.key).unwrap_or_default();
    let line: Div = if let Some(on) = row.switch {
        let mut switch = crate::controls::web_switch(SharedString::from(format!(
            "{id}-switch-{}",
            row.key
        )))
        .checked(on)
        .disabled(props.on_toggle.is_none());
        if let Some(on_toggle) = props.on_toggle.clone() {
            switch = switch.on_click(move |checked: &bool, window, cx| {
                on_toggle(*checked, window, cx);
            });
        }
        // The fixture's switch row: label + switch, NO description.
        surface::glass_toggle_row(row.label.clone(), None, switch.into_any_element(), cx)
            .px_3()
            .py_2()
    } else {
        let mut line = surface::flat_row()
            .flex()
            .flex_row()
            .w_full()
            .min_w_0()
            .items_center()
            .gap_2()
            .px_3()
            .py_2();
        if let Some((glyph, tone)) = row.glyph {
            line = line.child(
                Icon::new(glyph_icon(glyph))
                    .small()
                    .flex_shrink_0()
                    .text_color(tone_color(tone, cx)),
            );
        }
        line = line
            .child(
                div()
                    .flex_shrink_0()
                    .text_sm()
                    .text_color(cx.theme().foreground)
                    .child(SharedString::from(row.label.clone())),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_sm()
                    .text_color(tone_color(row.detail_tone, cx))
                    .child(SharedString::from(row.detail.clone().unwrap_or_default())),
            )
            .children(extras.trailing.take());
        if let Some(pill) = row.pill {
            let pill_id = SharedString::from(format!("{id}-action-{}", row.key));
            let busy = props.busy.contains(&row.key);
            let mut button = if pill.primary {
                surface::glass_pill_button_primary(pill_id, surface::PillSize::Sm)
            } else {
                surface::glass_pill_button(pill_id, surface::PillSize::Sm, cx)
            }
            .label(pill.action.label())
            .loading(busy)
            .disabled(busy || props.on_action.is_none());
            if let Some(on_action) = props.on_action.clone() {
                let key = row.key.clone();
                let action = pill.action;
                button = button.on_click(move |_, window, cx| on_action(&key, action, window, cx));
            }
            line = line.child(button);
        }
        line
    };
    let line = if row.indent { line.pl(px(36.)) } else { line };
    match extras.below.take() {
        Some(below) => v_flex()
            .w_full()
            .min_w_0()
            .child(line)
            .child(div().w_full().px_3().pb_2().pl(px(36.)).child(below))
            .into_any_element(),
        None => line.into_any_element(),
    }
}

/// THE block: one band per group (its tag trailing), the group's rows as one
/// hairline ladder under it.
pub(crate) fn render_sections(
    sections: &[ReadinessSection],
    mut props: BlockProps,
    cx: &App,
) -> Div {
    let mut block = v_flex().w_full().min_w_0().gap_4();
    for section in sections {
        let tag = section.tag.map(|tag| {
            tag_pill(
                SharedString::from(format!("{}-tag-{:?}", props.id, section.group)),
                tag,
                cx,
            )
        });
        let rows: Vec<AnyElement> = section
            .rows
            .iter()
            .map(|row| render_row(row, &mut props, cx))
            .collect();
        block = block.child(
            v_flex()
                .w_full()
                .min_w_0()
                .child(surface::glass_section_header(section.label, tag, cx))
                .child(
                    v_flex().w_full().min_w_0().children(
                        rows.into_iter()
                            .enumerate()
                            .map(|(index, row)| surface::list_row(row, index)),
                    ),
                ),
        );
    }
    block
}

/// The block's loading shape (no report yet): two skeleton rungs.
pub(crate) fn render_loading() -> Div {
    v_flex()
        .gap_2()
        .child(crate::controls::skeleton().h_4().w_64())
        .child(crate::controls::skeleton().h_4().w_56())
}

/// The ONE row under the composer (single-row mode): the failing row with
/// its action, no band.
pub(crate) fn render_single(row: &ReadinessRow, props: BlockProps, cx: &App) -> Div {
    let mut props = props;
    h_flex().w_full().min_w_0().child(render_row(row, &mut props, cx))
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

/// Where an install row sends the person (the CLI install docs, never a
/// product page: a fresh machine needs the CLI).
pub(crate) fn install_url(key: &str) -> Option<&'static str> {
    match key {
        KEY_GIT => Some("https://git-scm.com/downloads"),
        "claude" => Some("https://code.claude.com/docs/en/quickstart#step-1-install-claude-code"),
        // EXP-1232: codex is a managed download — its row never offers Install.
        _ => None,
    }
}

/// The OS privacy pane a permission row's grant opens (macOS only; the
/// Wayland portal asks through its own dialog).
pub(crate) fn privacy_pane_url(key: &str) -> Option<&'static str> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    match key {
        "screen_recording" => {
            Some("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
        }
        "accessibility" => {
            Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        }
        _ => None,
    }
}

/// Which local agent updates are running right now (so every host's Update
/// pill spins for the same run).
#[derive(Default)]
struct LocalUpdates(HashSet<String>);

impl gpui::Global for LocalUpdates {}

/// The row keys whose LOCAL action is running.
pub(crate) fn local_busy(cx: &App) -> HashSet<String> {
    cx.try_global::<LocalUpdates>()
        .map(|updates| updates.0.clone())
        .unwrap_or_default()
}

fn open_external(url: &'static str, cx: &mut App) {
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = api::opener::open_in_browser(url) {
                log::warn!("[ui] device readiness: opening {url} failed: {err}");
            }
        })
        .detach();
}

/// A pill on THIS device.
pub(crate) fn run_local_action(key: &str, action: DoctorAction, _window: &mut Window, cx: &mut App) {
    match action {
        DoctorAction::Install => {
            if let Some(url) = install_url(key) {
                crate::settings::open_url(cx, url.to_string());
            }
        }
        DoctorAction::Update => {
            if let Some(agent) = agent_for(key) {
                run_local_update(agent, cx);
            }
        }
        DoctorAction::SignIn => {
            if let Some(agent) = agent_for(key) {
                crate::agent_login::open_login_tab(agent, false, cx);
            }
        }
        DoctorAction::Grant => {
            if let Some(url) = privacy_pane_url(key) {
                open_external(url, cx);
            }
            coding::computer::prepare_in_background();
        }
    }
}

/// `claude update` / `codex update` right here (`coding::update_agent`, the
/// `agent_update` command's runner), then a fresh doctor run.
pub(crate) fn run_local_update(agent: CodingAgent, cx: &mut App) {
    let key = agent.id().to_string();
    if !cx.default_global::<LocalUpdates>().0.insert(key.clone()) {
        return; // already running
    }
    let hub = crate::coding_flow::CodingHub::global(cx);
    let settings = hub.read(cx).settings.clone();
    let data_dir = crate::coding_flow::coding_data_dir(cx);
    hub.update(cx, |_, cx| cx.notify());
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { coding::update_agent(&settings, &data_dir, agent) })
            .await;
        let _ = cx.update(|cx| {
            cx.default_global::<LocalUpdates>().0.remove(&key);
            let toast = match result {
                Ok(outcome) => crate::toast::Toast::success(outcome.message()),
                Err(message) => crate::toast::Toast::error(message),
            };
            crate::toast::show_in_active_window(toast, cx);
            crate::coding_flow::CodingHub::refresh_doctor(&hub, cx);
        });
    })
    .detach();
}

/// A pill for ANOTHER device (only `remote` actions get here): Update queues
/// the `agent_update` device command, Sign in runs the remote login flow.
pub(crate) fn run_remote_action(
    device_id: String,
    device_label: SharedString,
    key: &str,
    action: DoctorAction,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(agent) = agent_for(key) else {
        return;
    };
    match action {
        DoctorAction::Update => {
            let Some(trpc) = crate::queries::trpc_client(cx) else {
                return;
            };
            cx.spawn(async move |cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        api::devices::create_agent_update_command(&trpc, &device_id, agent.id())
                    })
                    .await;
                let _ = cx.update(|cx| {
                    let toast = match result {
                        Ok(_) => crate::toast::Toast::info(format!(
                            "Updating {} on {device_label}…",
                            agent.label()
                        )),
                        Err(err) => crate::toast::Toast::error(err.user_message()),
                    };
                    crate::toast::show_in_active_window(toast, cx);
                });
            })
            .detach();
        }
        DoctorAction::SignIn => crate::agent_login::sign_in_on_device(
            device_id,
            device_label,
            false,
            agent,
            coding::agent_login::LoginTarget::System,
            window,
            cx,
        ),
        DoctorAction::Install | DoctorAction::Grant => {}
    }
}

/// THIS device's computer-use switch: `Settings.computer_use` through the
/// hub's save (which pushes the launch defaults and re-runs the doctor; the
/// computer indicator asks the OS permissions when it reads on).
pub(crate) fn set_local_computer_use(on: bool, cx: &mut App) {
    let hub = crate::coding_flow::CodingHub::global(cx);
    let mut settings = hub.read(cx).settings.clone();
    if settings.computer_use == on {
        return;
    }
    settings.computer_use = on;
    if let Err(err) = crate::coding_flow::CodingHub::save_settings(&hub, settings, cx) {
        log::warn!("[ui] device readiness: saving computer use failed: {err}");
    }
}

/// The local block's standard wiring (every action + the switch).
pub(crate) fn local_props(id: impl Into<SharedString>, cx: &App) -> BlockProps {
    BlockProps {
        id: id.into(),
        on_action: Some(Rc::new(run_local_action)),
        on_toggle: Some(Rc::new(|on, _window, cx| set_local_computer_use(on, cx))),
        busy: local_busy(cx),
        extras: HashMap::new(),
    }
}

/// A remote block's wiring for `device_id` (remote actions only; the switch
/// is the host's, it owns the launch-defaults write).
pub(crate) fn remote_props(
    id: impl Into<SharedString>,
    device_id: String,
    device_label: SharedString,
) -> BlockProps {
    BlockProps {
        id: id.into(),
        on_action: Some(Rc::new(move |key, action, window, cx| {
            run_remote_action(device_id.clone(), device_label.clone(), key, action, window, cx)
        })),
        ..BlockProps::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/device-doctor.json");

    fn cases() -> Vec<Value> {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        fixture["cases"].as_array().unwrap().clone()
    }

    fn doctor(case: &Value) -> DeviceDoctor {
        parse(Some(&case["doctor"])).expect("every fixture block parses")
    }

    fn all_rows(sections: &[ReadinessSection]) -> Vec<&ReadinessRow> {
        sections.iter().flat_map(|section| section.rows.iter()).collect()
    }

    /// The row model per fixture case, local AND remote: bands + tags, the
    /// switch row, glyph tones, detail tones, the pill rules.
    #[test]
    fn every_fixture_case_models_exactly() {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        let tones: HashMap<String, String> = fixture["states"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, spec)| (key.clone(), spec["tone"].as_str().unwrap().to_string()))
            .collect();
        let glyphs: HashMap<String, String> = fixture["states"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, spec)| (key.clone(), spec["glyph"].as_str().unwrap().to_string()))
            .collect();
        for case in cases() {
            let name = case["name"].as_str().unwrap();
            let doctor = doctor(&case);
            let items = case["doctor"]["items"].as_array().unwrap();
            for local in [true, false] {
                let sections = sections(&doctor, local, None);
                // Bands: the fixture's groups, in order, with their tags.
                for section in &sections {
                    let spec = fixture["groups"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|group| {
                            serde_json::from_value::<DoctorGroup>(group["key"].clone()).unwrap()
                                == section.group
                        })
                        .unwrap();
                    assert_eq!(section.label, spec["label"].as_str().unwrap(), "{name}");
                    assert_eq!(section.tag, spec["tag"].as_str(), "{name}");
                }
                let rows = all_rows(&sections);
                // Every item is a row (no parent is off in the fixture).
                assert_eq!(rows.len(), items.len(), "{name}");
                let mut primaries = 0;
                for (row, item) in rows.iter().zip(items) {
                    let state = item["state"].as_str().unwrap();
                    assert_eq!(row.key, item["key"].as_str().unwrap(), "{name}");
                    assert_eq!(
                        row.label,
                        fixture["labels"][row.key.as_str()].as_str().unwrap(),
                        "{name}"
                    );
                    assert_eq!(row.indent, item.get("parent").is_some(), "{name}: {}", row.key);
                    if row.key == KEY_COMPUTER_USE {
                        // The switch row: no glyph, no detail, no pill.
                        assert_eq!(row.switch, Some(state != "off"), "{name}");
                        assert!(row.glyph.is_none() && row.detail.is_none() && row.pill.is_none());
                        continue;
                    }
                    assert_eq!(row.switch, None);
                    let (glyph, _) = row.glyph.unwrap();
                    let glyph = match glyph {
                        Glyph::Check => "check",
                        Glyph::Alert => "alert",
                        Glyph::Dash => "dash",
                        Glyph::X => "x",
                    };
                    let tone = |tone: Tone| match tone {
                        Tone::Success => "success",
                        Tone::Warning => "warning",
                        Tone::Muted => "muted",
                        Tone::Destructive => "destructive",
                    };
                    assert_eq!(glyph, glyphs[state], "{name}: {}", row.key);
                    assert_eq!(tone(row.glyph.unwrap().1), tones[state], "{name}: {}", row.key);
                    let expected_detail_tone = match state {
                        "action" => "warning",
                        "error" => "destructive",
                        _ => "muted",
                    };
                    assert_eq!(tone(row.detail_tone), expected_detail_tone, "{name}");
                    assert_eq!(row.detail.as_deref(), item["detail"].as_str(), "{name}");
                    // Pills: every action locally; remote = the fixture's
                    // `remotePills` exactly.
                    let expected: Option<DoctorAction> = if local {
                        item.get("action")
                            .map(|action| serde_json::from_value(action.clone()).unwrap())
                    } else {
                        case["remotePills"]
                            .get(row.key.as_str())
                            .map(|action| serde_json::from_value(action.clone()).unwrap())
                    };
                    assert_eq!(row.pill.map(|pill| pill.action), expected, "{name}: {}", row.key);
                    if row.pill.is_some_and(|pill| pill.primary) {
                        primaries += 1;
                        if local {
                            assert_eq!(
                                Some(row.key.as_str()),
                                case["localPrimary"].as_str(),
                                "{name}"
                            );
                        }
                    }
                }
                assert!(primaries <= 1, "{name}: one primary pill at most");
                if local {
                    assert_eq!(primaries, case["localPrimary"].as_str().map_or(0, |_| 1), "{name}");
                }
            }
        }
    }

    #[test]
    fn children_hide_while_the_switch_is_off() {
        let case = &cases()[1];
        let doctor = doctor(case);
        let rows = |switch| {
            sections(&doctor, true, switch)
                .into_iter()
                .find(|section| section.group == DoctorGroup::ComputerUse)
                .unwrap()
                .rows
        };
        assert_eq!(rows(None).len(), 3);
        let off = rows(Some(false));
        assert_eq!(off.len(), 1);
        assert_eq!(off[0].switch, Some(false));
    }

    /// The composer's single row: Git when Git fails, else the picked
    /// agent's row; its pill is primary (the only action there), and a
    /// non-remote action shows none for another device.
    #[test]
    fn the_composer_row_is_the_failing_one() {
        let cases = cases();
        // Case 1: everything claude needs is fine.
        assert!(blocking_row(&doctor(&cases[0]), CodingAgent::Claude, true).is_none());
        // EXP-1232: the managed codex with no login — Sign in, remote too.
        let codex = blocking_row(&doctor(&cases[0]), CodingAgent::Codex, true).unwrap();
        assert_eq!(summary(&codex), "Codex · Signed out");
        assert_eq!(
            blocking_row(&doctor(&cases[0]), CodingAgent::Codex, false)
                .unwrap()
                .pill
                .map(|pill| pill.action),
            Some(DoctorAction::SignIn)
        );
        // Case 2: claude too old — Update, remote too.
        let claude = blocking_row(&doctor(&cases[1]), CodingAgent::Claude, false).unwrap();
        assert_eq!(summary(&claude), "Claude Code · 2.1.222 · needs 2.1.263");
        assert_eq!(
            claude.pill,
            Some(RowPill { action: DoctorAction::Update, primary: true })
        );
        // Case 3: Git is the failure, whatever the agent.
        let git = blocking_row(&doctor(&cases[2]), CodingAgent::Codex, true).unwrap();
        assert_eq!(git.key, KEY_GIT);
        assert_eq!(git.pill.map(|pill| pill.action), Some(DoctorAction::Install));
        assert!(no_agent_row(&doctor(&cases[0]), true).is_none());
        assert_eq!(no_agent_row(&doctor(&cases[1]), true).unwrap().key, "claude");
        assert_eq!(no_agent_row(&doctor(&cases[2]), true).unwrap().key, KEY_GIT);
    }

    #[test]
    fn parse_is_lenient() {
        assert!(parse(None).is_none());
        assert!(parse(Some(&Value::Null)).is_none());
        let value = serde_json::json!({
            "checkedAt": "2026-10-05T19:00:00.000Z",
            "items": [
                { "key": "git", "group": "required", "state": "ok", "detail": "2.55.0" },
                { "key": "future", "group": "someday", "state": "quantum" }
            ]
        });
        let doctor = parse(Some(&value)).unwrap();
        assert_eq!(doctor.items.len(), 1);
    }

    /// The first-run footer: Continue exactly when the case has a runnable
    /// agent (git ok + an ok agent row); no block yet = Skip.
    #[test]
    fn the_first_run_footer_continues_once_an_agent_is_runnable() {
        for case in cases() {
            let runnable = !case["runnable"].as_array().unwrap().is_empty();
            assert_eq!(first_run_ready(Some(&doctor(&case))), runnable, "{}", case["name"]);
        }
        assert!(!first_run_ready(None));
    }

    #[test]
    fn a_device_with_no_doctor_keeps_the_bare_switch_row() {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        let row = bare_switch_row(true);
        assert_eq!(row.label, fixture["labels"]["computer_use"].as_str().unwrap());
        assert_eq!(row.switch, Some(true));
        assert!(row.glyph.is_none() && row.detail.is_none() && row.pill.is_none());
        assert_eq!(bare_switch_row(false).switch, Some(false));
    }

    #[test]
    fn copy_matches_the_fixture() {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        assert_eq!(fixture["copy"]["localTitle"], LOCAL_TITLE);
        assert_eq!(fixture["copy"]["skip"], SKIP);
        assert_eq!(fixture["copy"]["recheck"], RECHECK);
        assert_eq!(fixture["copy"]["continue"], CONTINUE);
        for key in ["git", "claude"] {
            assert!(install_url(key).unwrap().starts_with("https://"));
        }
        // EXP-1232: codex is a managed download — nothing to install by hand.
        assert_eq!(install_url("codex"), None);
    }
}
