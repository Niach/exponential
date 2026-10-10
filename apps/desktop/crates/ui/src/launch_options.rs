//! The ONE agent/model/effort options cluster (EXP-615) — the web
//! `LaunchOptionsPane` twin, shared by every desktop surface that pins how an
//! agent run starts:
//!
//! - Launch: the Agent page composer (EXP-825) and, before it, the start-coding
//!   dialog: the doctor-filtered agent pill strip, the per-agent Model /
//!   Effort selects and the capability-gated toggles (ultracode, plan
//!   mode). This is [`LaunchOptionsSection`], which OWNS that
//!   state and hands out a [`LaunchOptions`] snapshot.
//! - Automation: [`crate::trigger_editor`]'s launch PINS:
//!   the exact same strip (seeded to the bound device's default agent — no
//!   "Device default" pill since EXP-615), the same choice lists behind the
//!   launch "CLI default" sentinel, and NO toggles (an unattended run never
//!   parks on plan mode).
//!
//! The three dialogs drifted into three different agent pickers before this
//! module existed (a pill strip here, a dropdown there); everything visual
//! lives here now, so they cannot drift again.
//!
//! EXP-694: the cluster renders as ONE inset-grouped stack
//! ([`crate::surface::glass_group`]) — the tabs are its first ROW, the
//! model/effort selects are picker rows, the toggles are switch rows. The
//! capsule strip ([`agent_tabs`]) survives only for the surfaces that have not
//! moved onto a group yet.
//!
//! The state-owning half follows the [`crate::trigger_editor`] idiom: the
//! host keeps a plain field and passes a `fn(&mut V) -> &mut Self` accessor,
//! so the callbacks reach back into it without a second entity.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, AnyElement, App, Context, Div, InteractiveElement as _, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement as _, Stateful, Styled, Window,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::{h_flex, select::Select, ActiveTheme as _, Icon};

use coding::{CodingAgent, LaunchOptions};

use crate::coding_selects::{
    choice_select, effort_choices_for, model_choices_for, selected, ChoiceSelect,
    SUBAGENT_MODEL_CHOICES,
};
use crate::icons::ExpIcon;
use crate::surface;

/// The label the model/effort pickers show while nothing is pinned — the
/// run then follows the agent CLI's own defaults, same wording as launch.
pub(crate) const CLI_DEFAULT_LABEL: &str = "CLI default";

/// The unset SUBAGENT model's label ×4 (web `CLI_DEFAULT_MODEL`).
pub(crate) const SUBAGENT_DEFAULT_LABEL: &str = "Default";

// ---------------------------------------------------------------------------
// EXP-792 — the MCP servers multiselect
// ---------------------------------------------------------------------------

/// One team MCP server as the run multiselect offers it, ALREADY resolved
/// against the person's own connection (the server holds it, so it is the
/// same whichever machine the run lands on).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct McpServerOption {
    pub(crate) id: String,
    pub(crate) name: String,
    /// Why the person cannot use it yet ([`mcp_block_reason`]); `None` =
    /// ready. A blocked server is GREYED and never preselected, but stays
    /// pickable: the launch then starts without it (a warning, never a
    /// blocker), so connecting it in Settings before the start is enough.
    pub(crate) blocked: Option<String>,
    /// Seeds the FIRST pick (`enabledByDefault`) — ready servers only.
    pub(crate) enabled_by_default: bool,
    /// The row's glyph + second line come from these
    /// ([`crate::picker::mcp_server_picker::mcp_server_icon`]).
    pub(crate) url: Option<String>,
    pub(crate) command: Option<String>,
}

/// The options as the shared picker reads them
/// ([`crate::picker::mcp_server_picker::McpPickerServer`]): a blocked
/// server keeps its reason as the second line and stays PICKABLE (the
/// launch then starts without it), so `disabled` is never set — the row
/// only dims (the composer "+" menu's MCP servers picker).
pub(crate) fn mcp_picker_servers(
    servers: &[McpServerOption],
) -> Vec<crate::picker::mcp_server_picker::McpPickerServer> {
    servers
        .iter()
        .map(|server| crate::picker::mcp_server_picker::McpPickerServer {
            id: server.id.clone(),
            name: server.name.clone(),
            url: server.url.clone(),
            command: server.command.clone(),
            description: server.blocked.clone(),
            disabled: false,
        })
        .collect()
}

/// The multiselect's trailing value — web `mcpPickSummary`, string for
/// string: `None`, one or two names joined, else `"A, B +2"`.
pub(crate) fn mcp_pick_summary(servers: &[McpServerOption], selected: &[String]) -> String {
    let names: Vec<&str> = servers
        .iter()
        .filter(|server| selected.iter().any(|id| id == &server.id))
        .map(|server| server.name.as_str())
        .collect();
    match names.len() {
        0 => "None".to_string(),
        1 | 2 => names.join(", "),
        n => format!("{}, {} +{}", names[0], names[1], n - 2),
    }
}

/// The seed of a fresh multiselect: the READY servers flagged
/// `enabledByDefault` (web `preselectMcpServerIds` with no saved pick) — a
/// server you have not connected is never preselected.
pub(crate) fn mcp_default_ids(servers: &[McpServerOption]) -> Vec<String> {
    servers
        .iter()
        .filter(|server| server.enabled_by_default && server.blocked.is_none())
        .map(|server| server.id.clone())
        .collect()
}

/// One step of [`LaunchOptionsSection::set_mcp_servers`]: seed `selected`
/// from `enabled_by_default` on the first non-empty list (once per team,
/// `seeded` flips), otherwise clamp the ticks to the rows still offered.
fn seed_or_clamp_mcp(seeded: &mut bool, selected: &mut Vec<String>, servers: &[McpServerOption]) {
    if !*seeded && !servers.is_empty() {
        *selected = mcp_default_ids(servers);
        *seeded = true;
    } else {
        // A server removed on the web under an open dialog must not
        // reach the launcher as an "unknown server" skip.
        selected.retain(|id| servers.iter().any(|server| &server.id == id));
    }
}

/// The greyed row's caption for a server the person has not connected.
pub(crate) const MCP_CONNECT_FIRST: &str = "Connect first in Settings → MCP servers";
/// … and for one whose sign-in expired or whose refresh failed.
pub(crate) const MCP_RECONNECT_FIRST: &str = "Reconnect first in Settings → MCP servers";

/// EXP-792 — why the person cannot use a server yet (web `mcpNotReadyLabel`
/// plus where to go): `None` when their connection is usable (`connected`)
/// or none is needed (`not_needed`).
pub(crate) fn mcp_block_reason(connection: &api::mcp_servers::McpConnection) -> Option<String> {
    if connection.is_ready() {
        return None;
    }
    Some(match connection.status.as_str() {
        "expired" | "error" => MCP_RECONNECT_FIRST,
        _ => MCP_CONNECT_FIRST,
    }
    .to_string())
}

// ---------------------------------------------------------------------------
// EXP-872 — the ONE account picker
// ---------------------------------------------------------------------------

/// The account options the TARGET machine offers, already flattened across
/// its agents ([`coding::flatten_accounts`]) and clamped to the agents it can
/// actually run. A machine that reports no signed-in login still offers one
/// UNPINNED row per runnable agent (an empty id: the device resolves its
/// profile, or refuses with "Sign in to … first"), named by the agent — the
/// picker must never go empty while there is something to launch.
pub(crate) fn machine_account_options(
    accounts: &coding::agent_accounts::AgentAccounts,
    usage: &coding::agent_usage::AgentUsageMap,
    settings: &coding::Settings,
    available: &[CodingAgent],
) -> Vec<coding::AccountOption> {
    let mut options = coding::flatten_accounts(accounts, usage, Some(settings.default_agent.id()));
    options.retain(|option| available.contains(&option.agent));
    if !options.is_empty() {
        return options;
    }
    available
        .iter()
        .map(|agent| coding::AccountOption {
            id: String::new(),
            agent: *agent,
            email: agent.label().to_string(),
            is_last_used: *agent == settings.default_agent,
            health: coding::Health::Unknown,
            limits: None,
        })
        .collect()
}

/// The picker key for an (agent, account) pair — an unnamed account (`None`)
/// is the agent's unpinned row (an empty id).
pub(crate) fn account_key(agent: CodingAgent, account: Option<&str>) -> String {
    format!("{}:{}", agent.id(), account.unwrap_or_default())
}

/// One pill in the agent strip.
pub(crate) struct AgentPill {
    pub(crate) label: SharedString,
    /// The agent's brand mark; `None` for an unknown newer-contract id.
    pub(crate) icon: Option<ExpIcon>,
    /// EXP-409: installed but signed out — greyed, still clickable (picking
    /// it puts the sign-in fix in the footer blocker instead of dead UI).
    pub(crate) dimmed: bool,
    pub(crate) note: Option<SharedString>,
}

/// The agents a LAUNCH strip offers (EXP-206): the ones the doctor found
/// installed — including installed-but-signed-out ones (EXP-409). While the
/// report is pending — or when NOTHING is installed — every agent stays
/// visible, so the strip never goes empty and the footer blocker can name the
/// selected agent's failure.
pub(crate) fn pickable_agents(report: Option<&coding::DoctorReport>) -> Vec<CodingAgent> {
    let Some(report) = report else {
        return CodingAgent::ALL.to_vec();
    };
    let runnable = report.installed_agents();
    let unauthed = report.unauthed_agents();
    if runnable.is_empty() && unauthed.is_empty() {
        return CodingAgent::ALL.to_vec();
    }
    // Keep the canonical ALL order regardless of auth state.
    CodingAgent::ALL
        .into_iter()
        .filter(|agent| runnable.contains(agent) || unauthed.contains(agent))
        .collect()
}

/// The `(ultracode, plan_mode)` settings defaults for `agent`,
/// capability-masked (EXP-201: ultracode is Claude-only, plan mode is
/// claude-only). EXP-206: ONE set of defaults — a single-issue
/// run and a multi-issue batch run seed identically, and plan mode is a
/// per-AGENT setting. EXP-690 retired the skip-permissions toggle (every
/// run bypasses).
pub(crate) fn agent_defaults(settings: &coding::Settings, agent: CodingAgent) -> (bool, bool) {
    (
        settings.claude_ultracode && agent.supports_ultracode(),
        settings.plan_mode_for(agent) && agent.supports_plan_mode(),
    )
}

/// EXP-749: can `agent` NOT run a session on a machine advertising
/// `acp_agents`? Every machine says which of its agents speak ACP, so an
/// agent missing from the list cannot start a run there.
pub(crate) fn cannot_run_session(acp_agents: &[CodingAgent], agent: CodingAgent) -> bool {
    !acp_agents.contains(&agent)
}

/// The segments of an agent strip — the pills themselves, container-free, so
/// the free-floating capsule ([`agent_tabs`]) and the EMBEDDED group row
/// ([`agent_tabs_row`]) draw the exact same tabs.
fn agent_segments<V: Render>(
    id: &'static str,
    pills: Vec<AgentPill>,
    active: Option<usize>,
    on_select: impl Fn(&mut V, usize, &mut Window, &mut Context<V>) + 'static,
    embedded: bool,
    cx: &mut Context<V>,
) -> Vec<Stateful<Div>> {
    let muted = cx.theme().muted_foreground;
    let on_select = std::rc::Rc::new(on_select);
    pills
        .into_iter()
        .enumerate()
        .map(|(ix, pill)| {
            let on_select = on_select.clone();
            let selected = active == Some(ix);
            let segment = if embedded {
                surface::glass_tab_item(selected, cx)
            } else {
                crate::controls::segmented_item(selected, cx)
            };
            segment
                .id((id, ix))
                .when(pill.dimmed, |this| this.opacity(0.45))
                .children(pill.icon.map(|icon| {
                    crate::coding_selects::mark_icon(icon).size_3p5()
                }))
                .child(pill.label)
                .when_some(pill.note, |this, note| {
                    this.child(div().text_xs().text_color(muted).child(note))
                })
                .on_click(cx.listener(move |this, _: &gpui::ClickEvent, window, cx| {
                    on_select(this, ix, window, cx);
                }))
        })
        .collect()
}

/// EXP-694 — the same strip as the FIRST ROW of a
/// [`crate::surface::glass_group`]: no capsule of its own, 8px padding, the
/// hairline below drawn by the group. This is what every grouped agent picker
/// (launch, automation pins, device defaults) leads with.
pub(crate) fn agent_tabs_row<V: Render>(
    id: &'static str,
    pills: Vec<AgentPill>,
    active: Option<usize>,
    on_select: impl Fn(&mut V, usize, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> Div {
    surface::glass_tabs_row().children(agent_segments(id, pills, active, on_select, true, cx))
}

/// The label a pin shows for `picked` — the choice's own label, or the
/// "CLI default" sentinel while nothing is pinned.
fn pin_label(choices: &'static [(&'static str, &'static str)], picked: Option<&str>) -> String {
    picked
        .and_then(|value| {
            choices
                .iter()
                .find(|(_, choice)| *choice == value)
                .map(|(label, _)| (*label).to_string())
        })
        .unwrap_or_else(|| CLI_DEFAULT_LABEL.to_string())
}

/// One automation choice pin as a GROUPED picker row (EXP-694,
/// S2): the label leading, the pinned value trailing at 70% behind a caret,
/// no field chrome, and the "CLI default" sentinel while nothing is pinned.
/// Writes through the `pick` accessor on the host's state `S`.
#[allow(clippy::too_many_arguments)] // one per pin, same shape as `choice_pin`
pub(crate) fn choice_pin_row<V: Render, S: 'static>(
    label: impl Into<SharedString>,
    prefix: &'static str,
    key: &'static str,
    choices: &'static [(&'static str, &'static str)],
    picked: Option<&str>,
    pick: fn(&mut S) -> &mut Option<String>,
    access: fn(&mut V) -> &mut S,
    cx: &mut Context<V>,
) -> Div {
    let foreground = cx.theme().foreground;
    let trigger = Button::new(SharedString::from(format!("{prefix}-pin-{key}")))
        .ghost()
        .cursor_pointer()
        .h_auto()
        .px_0()
        .py_0()
        .text_color(foreground.opacity(0.7))
        // EXP-697: NOT `.label()` — upstream draws that in a `flex_none` box,
        // so a long model name wraps onto a second line.
        .child(surface::picker_value_label(SharedString::from(pin_label(
            choices, picked,
        ))))
        // A grouped picker row ends in chevron-right ×4, not a caret.
        .child(surface::picker_row_chevron(cx));
    let control = pin_menu(trigger, choices, picked, pick, access, cx).into_any_element();
    surface::glass_picker_row(label, None, control, cx)
}

/// Hangs a pin's choice menu off an already-dressed `trigger`.
fn pin_menu<V: Render, S: 'static>(
    trigger: Button,
    choices: &'static [(&'static str, &'static str)],
    picked: Option<&str>,
    pick: fn(&mut S) -> &mut Option<String>,
    access: fn(&mut V) -> &mut S,
    cx: &mut Context<V>,
) -> impl IntoElement {
    let current = picked.map(str::to_string);
    let view = cx.entity().downgrade();
    crate::controls::PointerMenu::for_button(trigger, move |mut menu, _window, _cx| {
            let default_view = view.clone();
            let current = current.clone();
            menu = menu.item(crate::controls::pointer_check_item(
                CLI_DEFAULT_LABEL,
                current.is_none(),
                move |_, cx| {
                    if let Some(view) = default_view.upgrade() {
                        view.update(cx, |view, cx| {
                            *pick(access(view)) = None;
                            cx.notify();
                        });
                    }
                },
            ));
            for (label, value) in choices {
                // A blank value IS "leave it to the CLI" — the same thing
                // "Device default" already says.
                if value.is_empty() {
                    continue;
                }
                let view = view.clone();
                let value = (*value).to_string();
                let checked = current.as_deref() == Some(value.as_str());
                menu = menu.item(crate::controls::pointer_check_item(
                    *label,
                    checked,
                    move |_, cx| {
                        if let Some(view) = view.upgrade() {
                            let value = value.clone();
                            view.update(cx, |view, cx| {
                                *pick(access(view)) = Some(value);
                                cx.notify();
                            });
                        }
                    },
                ));
            }
            menu
        })
}

/// EXP-825: the composer options row's pin — the chat page's muted `text_xs`
/// ghost with a caret, one word on the line under the card.
pub(crate) fn inline_pin_trigger(id: SharedString, label: String, cx: &App) -> Button {
    inline_pin_trigger_with(id, None, label, cx)
}

/// EXP-862: the same pin with a LEADING GLYPH — the picker rule ×4 is that a
/// value shown with an icon is picked with that icon, so the device pin leads
/// with the machine's glyph (EXP-924: its owner's pick, else its kind) and the
/// account pin with the account mark.
pub(crate) fn inline_pin_trigger_with(
    id: SharedString,
    icon: Option<ExpIcon>,
    label: String,
    cx: &App,
) -> Button {
    use gpui::prelude::FluentBuilder as _;
    Button::new(id)
        .ghost()
        .cursor_pointer()
        .h_auto()
        .px_1()
        .py_0()
        .text_color(cx.theme().muted_foreground)
        .dropdown_caret(true)
        .when_some(icon, |button, icon| {
            button.child(Icon::new(icon).size(gpui::px(12.)))
        })
        .child(div().text_xs().child(SharedString::from(label)))
}

/// EXP-1030 — the candidate machines as THE device picker
/// ([`crate::picker::device_picker`]) reads them: one row per machine, its
/// [`crate::icons::device_icon_name`] glyph (its owner's pick, else its kind
/// — resolved HERE, because the typed constructor takes no `&App` and the
/// kind lives on the synced row), and its picker line as the label. Every
/// launch surface that offers a machine — the composer's Device pin, an
/// action trigger's runner — builds its rows through this ONE function, so
/// they can never list the same fleet differently.
pub(crate) fn launch_device_rows(
    devices: &[crate::queries::LaunchDevice],
    cx: &App,
) -> Vec<crate::picker::device_picker::DevicePickerDevice> {
    devices
        .iter()
        .map(|device| crate::picker::device_picker::DevicePickerDevice {
            id: device.device_id.clone(),
            name: device.label.clone(),
            icon: Some(device_glyph_name(&device.device_id, cx).to_string()),
            // The glyph above is already resolved (EXP-924), so the kind
            // default the picker would derive from this is never consulted.
            server: false,
            description: None,
            // EXP-615: an offline-but-capable machine is not a lesser choice
            // (the run fires when it comes back), so no candidate is ever
            // greyed out here.
            disabled: false,
        })
        .collect()
}

/// The stored glyph NAME for one machine (EXP-924: its own `icon` when it
/// names a device icon, else its kind's default) — resolved off the synced
/// `devices` row, so a picker row and the Devices list wear the same mark.
pub(crate) fn device_glyph_name(device_id: &str, cx: &App) -> &'static str {
    let (icon, is_server) = sync::Store::try_global(cx)
        .and_then(|store| {
            store
                .collections()
                .devices
                .read(cx)
                .iter()
                .find(|row| row.device_id.as_deref() == Some(device_id))
                .map(|row| (row.icon.clone(), row.is_server()))
        })
        .unwrap_or((None, false));
    crate::icons::device_icon_name(icon.as_deref(), is_server)
}

/// EXP-825: a labelled switch on the composer options row.
fn inline_switch<V: Render>(
    id: SharedString,
    label: &'static str,
    on: bool,
    write: impl Fn(&mut V, bool) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    h_flex()
        .gap_1p5()
        .items_center()
        .px_1()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(label)
        .child(
            crate::controls::web_switch(id)
                .checked(on)
                .on_click(cx.listener(move |view, on: &bool, _, cx| {
                    write(view, *on);
                    cx.notify();
                })),
        )
        .into_any_element()
}

/// EXP-877 — THE choice pin: `trigger` with a checked-item menu of
/// `choices`, calling `on_pick` with the chosen VALUE. The one place a
/// (label, value) list becomes a dropdown, so the launch pins and the
/// transcript composer's model picker read the same and cannot drift apart.
///
/// `on_pick` takes the host view because every caller writes somewhere on it
/// — a `ChoiceSelect` entity here ([`choice_menu`]), a `/model <alias>`
/// message in the steer viewer.
pub(crate) fn choice_pin<V: Render>(
    trigger: Button,
    choices: &'static [(&'static str, &'static str)],
    picked: String,
    on_pick: impl Fn(&mut V, &str, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> impl IntoElement {
    choice_pin_for(trigger, choices, picked, cx.entity().downgrade(), on_pick)
}

/// [`choice_pin`] for a caller with no `Context<V>` at hand: the host's weak
/// handle is captured up front instead of derived from `cx`.
pub(crate) fn choice_pin_for<V: Render>(
    trigger: Button,
    choices: &'static [(&'static str, &'static str)],
    picked: String,
    view: gpui::WeakEntity<V>,
    on_pick: impl Fn(&mut V, &str, &mut Window, &mut Context<V>) + 'static,
) -> impl IntoElement {
    let on_pick = std::rc::Rc::new(on_pick);
    crate::controls::PointerMenu::for_button(trigger, move |mut menu, _window, _cx| {
        for (label, value) in choices {
            let view = view.clone();
            let on_pick = on_pick.clone();
            let value = (*value).to_string();
            let checked = picked == value;
            menu = menu.item(crate::controls::pointer_check_item(
                *label,
                checked,
                move |window, cx| {
                    if let Some(view) = view.upgrade() {
                        let value = value.clone();
                        let on_pick = on_pick.clone();
                        view.update(cx, |view, cx| on_pick(view, &value, window, cx));
                    }
                },
            ));
        }
        menu
    })
}

/// Hangs a model/effort choice menu off `trigger`, writing the pick into the
/// section's `ChoiceSelect` (the same entity the grouped cluster's select row
/// edits, so the two never disagree) — [`choice_pin`] with that write.
fn choice_menu<V: Render>(
    trigger: Button,
    choices: &'static [(&'static str, &'static str)],
    picked: String,
    select: fn(&LaunchOptionsSection) -> &ChoiceSelect,
    access: fn(&mut V) -> &mut LaunchOptionsSection,
    cx: &mut Context<V>,
) -> impl IntoElement {
    choice_menu_for(trigger, choices, picked, select, access, cx.entity().downgrade())
}

/// [`choice_menu`] against a captured weak handle.
fn choice_menu_for<V: Render>(
    trigger: Button,
    choices: &'static [(&'static str, &'static str)],
    picked: String,
    select: fn(&LaunchOptionsSection) -> &ChoiceSelect,
    access: fn(&mut V) -> &mut LaunchOptionsSection,
    view: gpui::WeakEntity<V>,
) -> impl IntoElement {
    choice_pin_for(
        trigger,
        choices,
        picked,
        view,
        move |view: &mut V, value: &str, window, cx| {
            let state = select(access(view)).clone();
            state.update(cx, |state, cx| {
                state.set_selected_value(&SharedString::from(value.to_string()), window, cx)
            });
            cx.notify();
        },
    )
}

/// One switch row of an [`AgentDefaultsGroup`]: the label, its state, and
/// what a flip writes back into the host view.
pub(crate) struct DefaultsToggle<V: Render> {
    id: SharedString,
    label: SharedString,
    checked: bool,
    #[allow(clippy::type_complexity)]
    on_click: Box<dyn Fn(&mut V, bool, &mut Context<V>) + 'static>,
}

impl<V: Render> DefaultsToggle<V> {
    pub(crate) fn new(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        checked: bool,
        on_click: impl Fn(&mut V, bool, &mut Context<V>) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            checked,
            on_click: Box::new(on_click),
        }
    }

    fn row(self, cx: &mut Context<V>) -> Div {
        let on_click = self.on_click;
        surface::glass_toggle_row(
            self.label,
            None,
            crate::controls::web_switch(self.id)
                .checked(self.checked)
                .on_click(cx.listener(move |view: &mut V, on: &bool, _, cx| {
                    on_click(view, *on, cx);
                    cx.notify();
                }))
                .into_any_element(),
            cx,
        )
    }
}

/// EXP-694 S4 — the ONE agent picker every desktop surface renders: a single
/// inset-grouped stack of `[embedded agent tabs] / Model / <effort> /
/// <toggles>`, hairline-divided, no loose controls and no free-floating
/// capsule. The Device settings dialog and Settings → Agents build the same
/// group through this builder; only the STATE behind the selects differs (a
/// per-device defaults map there, the ambient settings here), plus the one
/// row a surface splices in:
///
/// - [`Self::leading`] — between the tabs and Model (the CLI-path row).
/// - [`Self::subagent`] — EXP-1020, right under Model: claude's subagent
///   model, the row the device settings had on web and nowhere else.
///
/// EXP-862 dropped the trailing account + usage rows with the device-settings
/// dialog's account block; the composer's own options row is the only launch
/// surface now, and it pins its account inline.
pub(crate) struct AgentDefaultsGroup<V: Render> {
    prefix: &'static str,
    agent: CodingAgent,
    pills: Vec<AgentPill>,
    active: Option<usize>,
    #[allow(clippy::type_complexity)]
    on_select: Box<dyn Fn(&mut V, usize, &mut Window, &mut Context<V>) + 'static>,
    model: ChoiceSelect,
    effort: ChoiceSelect,
    effort_disabled: bool,
    toggles: Vec<DefaultsToggle<V>>,
    leading: Vec<Div>,
    subagent: Option<ChoiceSelect>,
}

impl<V: Render> AgentDefaultsGroup<V> {
    pub(crate) fn new(
        prefix: &'static str,
        agent: CodingAgent,
        pills: Vec<AgentPill>,
        active: Option<usize>,
        on_select: impl Fn(&mut V, usize, &mut Window, &mut Context<V>) + 'static,
        model: ChoiceSelect,
        effort: ChoiceSelect,
    ) -> Self {
        Self {
            prefix,
            agent,
            pills,
            active,
            on_select: Box::new(on_select),
            model,
            effort,
            effort_disabled: false,
            toggles: Vec::new(),
            leading: Vec::new(),
            subagent: None,
        }
    }

    /// Ultracode owns the effort level while it is on (EXP-206) — the row
    /// dims and says so instead of offering a pick the argv ignores.
    pub(crate) fn effort_disabled(mut self, disabled: bool) -> Self {
        self.effort_disabled = disabled;
        self
    }

    pub(crate) fn toggle(mut self, toggle: DefaultsToggle<V>) -> Self {
        self.toggles.push(toggle);
        self
    }

    pub(crate) fn leading(mut self, rows: Vec<Div>) -> Self {
        self.leading = rows;
        self
    }

    /// EXP-981/EXP-1020: the model claude's SUBAGENTS run on, directly under
    /// Model as on web. Callers pass it only where the agent supports one.
    pub(crate) fn subagent(mut self, select: ChoiceSelect) -> Self {
        self.subagent = Some(select);
        self
    }

    pub(crate) fn render(self, cx: &mut Context<V>) -> Div {
        let Self {
            prefix,
            agent,
            pills,
            active,
            on_select,
            model,
            effort,
            effort_disabled,
            toggles,
            leading,
            subagent,
        } = self;
        let mut rows: Vec<Div> = vec![agent_tabs_row(prefix, pills, active, on_select, cx)];
        rows.extend(leading);
        rows.push(surface::glass_picker_row(
            "Model",
            None,
            surface::glass_picker_select(Select::new(&model), cx).into_any_element(),
            cx,
        ));
        if let Some(subagent) = subagent {
            rows.push(surface::glass_picker_row(
                "Subagent model",
                None,
                surface::glass_picker_select(Select::new(&subagent), cx).into_any_element(),
                cx,
            ));
        }
        rows.push(surface::glass_picker_row(
            agent.effort_label(),
            // The hint the two-column layout carried under the Effort select
            // (EXP-206) becomes the row's own second line.
            effort_disabled.then(|| SharedString::from("ultracode sets effort")),
            surface::glass_picker_select(Select::new(&effort), cx)
                .disabled(effort_disabled)
                // `appearance(false)` drops the component's own disabled
                // dimming with the rest of the field chrome — put it back.
                .when(effort_disabled, |select| select.opacity(0.5))
                .into_any_element(),
            cx,
        ));
        for toggle in toggles {
            rows.push(toggle.row(cx));
        }
        surface::glass_group_rows(rows)
    }
}

/// EXP-696: which agent a device settle lands on, byte-for-byte the web
/// `use-launch-options.ts` rule: the machine's OWN default agent when it can
/// actually run there, else the standing pick when that machine can run it,
/// else its first agent. `available` empty (an old row advertising nothing)
/// keeps the standing pick — the blocker names the real reason.
pub(crate) fn settled_agent(
    available: &[CodingAgent],
    device_default: CodingAgent,
    current: CodingAgent,
) -> CodingAgent {
    if available.contains(&device_default) {
        return device_default;
    }
    if available.contains(&current) {
        return current;
    }
    available.first().copied().unwrap_or(current)
}

/// EXP-696: what a REMOTE target machine advertises — the agent CLIs it can
/// run and the launch defaults it published. A cluster carrying one of these
/// stops consulting the LOCAL doctor and the LOCAL `CodingHub`: neither says
/// anything about another machine (web `use-launch-options.ts`' device
/// settle).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RemoteDefaults {
    pub(crate) agents: Vec<CodingAgent>,
    /// EXP-749: which of those can run a SESSION there; the rest of `agents`
    /// carries the "can't run a session" note. Never a filter.
    pub(crate) acp_agents: Vec<CodingAgent>,
    pub(crate) settings: coding::Settings,
    /// EXP-484/747 B7: the machine's `agent_accounts` payload — WHICH login
    /// each agent CLI runs as there, and its account PROFILES. Empty for a
    /// row that never reported (an older build, or a machine that has not
    /// beaten yet): the account picker then falls back to one row per agent.
    pub(crate) accounts: coding::agent_accounts::AgentAccounts,
    /// EXP-992: the same machine's `agent_usage` payload — the picker's hover
    /// preview draws its bars off it (the active login's numbers ride here
    /// when its profile row carries none).
    pub(crate) usage: coding::agent_usage::AgentUsageMap,
    /// EXP-1249: the machine advertises `computer-use-run` — it reads a
    /// start's per-run `computerUse`. Without it the "+" menu offers no
    /// Computer use row (the run would silently take the machine's switch).
    pub(crate) computer_use_run: bool,
    /// The machine advertises `steer-files` — it localizes non-image FILE
    /// attachments. Without it the composer's pick takes images only (the
    /// server refuses a file to such a device anyway).
    pub(crate) steer_files: bool,
}

/// The launch cluster's own state: which agent runs, its
/// model/effort picks and the capability-gated toggles. Seeded from
/// [`coding::Settings`]' per-AGENT fields; switching the agent tab re-seeds
/// everything from that agent's own defaults.
pub(crate) struct LaunchOptionsSection {
    /// The selected agent CLI (EXP-201).
    pub(crate) agent: CodingAgent,
    model: ChoiceSelect,
    effort: ChoiceSelect,
    /// EXP-981: the model claude's SUBAGENTS run on
    /// (`CLAUDE_CODE_SUBAGENT_MODEL`). Claude-only, blank = the CLI's own
    /// default, so its picker offers the model list with a "CLI default"
    /// entry in front.
    subagent_model: ChoiceSelect,
    /// Dynamic workflows (`--effort ultracode`) — Claude-only, any model.
    pub(crate) ultracode: bool,
    /// Native plan mode (`--permission-mode plan`).
    pub(crate) plan_mode: bool,
    /// EXP-696: `Some` while the run targets another machine — its agents
    /// and its published defaults replace the local doctor + hub everywhere.
    remote: Option<RemoteDefaults>,
    /// EXP-792: the team's MCP servers, already resolved against the TARGET
    /// machine by whichever surface owns the device pick
    /// ([`Self::set_mcp_servers`]). EMPTY hides the row — a team with no
    /// servers gets no multiselect, exactly like the web's `mcpRow`.
    mcp_servers: Vec<McpServerOption>,
    /// The picked server ids, in registry order.
    mcp_selected: Vec<String>,
    /// Whether [`Self::mcp_selected`] has been seeded from
    /// `enabled_by_default` yet. The seed fires ONCE, on the first non-empty
    /// list: `set_mcp_servers` is called on every render (the blockers
    /// depend on the device pick, which moves), and re-seeding there would
    /// undo the person's ticks under their cursor.
    mcp_seeded: bool,
    /// FEED-73: the subject brings its OWN MCP list (a real action — the
    /// server sets the run's pick from it), so the row hides and no pick
    /// rides the launch. The pick itself is kept for the next subject.
    mcp_owned_by_subject: bool,
    /// EXP-747 B7: the agent account PROFILE the run signs in as on the
    /// target machine; `None` = unnamed, the machine's last used login
    /// (EXP-1158).
    account: Option<String>,
    /// EXP-1249: the run's own Computer use pick (the "+" menu toggle);
    /// `None` = the target machine's switch decides. Cleared on a device
    /// switch: another machine has another switch.
    computer_use: Option<bool>,
}

impl LaunchOptionsSection {
    /// Seed from this install's LAST USED login (EXP-1158) — which names the
    /// agent too — and that agent's settings defaults.
    pub(crate) fn new(window: &mut Window, cx: &mut App) -> Self {
        let settings = crate::coding_flow::CodingHub::global(cx).read(cx).settings.clone();
        let agent = settings.default_agent;
        let (ultracode, plan_mode) = agent_defaults(&settings, agent);
        let mut this = Self {
            agent,
            model: choice_select(model_choices_for(agent), settings.model_for(agent), window, cx),
            effort: choice_select(
                effort_choices_for(agent),
                settings.effort_for(agent),
                window,
                cx,
            ),
            subagent_model: choice_select(
                &SUBAGENT_MODEL_CHOICES,
                settings.subagent_model_for(agent),
                window,
                cx,
            ),
            ultracode,
            plan_mode,
            remote: None,
            mcp_servers: Vec::new(),
            mcp_selected: Vec::new(),
            mcp_seeded: false,
            mcp_owned_by_subject: false,
            // Settled just below against what the machine reports.
            account: None,
            computer_use: None,
        };
        // …then settle it against what this machine actually reports, so the
        // pin can never SHOW one login while the launch spends another.
        this.settle_account(window, cx);
        this
    }

    /// EXP-792: offer these team MCP servers, already resolved against the
    /// machine the run lands on. Safe to call on every render: it re-points
    /// the blockers (which move with the device pick), clamps a pick whose
    /// row has gone, and seeds `enabled_by_default` exactly once.
    pub(crate) fn set_mcp_servers(&mut self, servers: Vec<McpServerOption>) {
        seed_or_clamp_mcp(&mut self.mcp_seeded, &mut self.mcp_selected, &servers);
        self.mcp_servers = servers;
    }

    /// FEED-73: whether the composer's subject owns its MCP list
    /// ([`crate::chat_launch::subject_owns_mcp_servers`]). Safe on every
    /// render.
    pub(crate) fn set_mcp_owned_by_subject(&mut self, owned: bool) {
        self.mcp_owned_by_subject = owned;
    }

    /// Forget the seed: the owner switched TEAM, so the next non-empty list
    /// (the new team's servers) seeds `enabled_by_default` again instead of
    /// being clamped against the old team's ticks (which would land every
    /// new server OFF).
    pub(crate) fn reset_mcp_seed(&mut self) {
        self.mcp_seeded = false;
    }

    /// The picker's new set. Kept in REGISTRY order so the launcher's
    /// per-server env positions (`EXP_MCP_TOKEN_<n>`) follow the list the
    /// person sees, not the order they happened to click in; an id the
    /// list no longer offers is dropped.
    pub(crate) fn set_mcp_selected(&mut self, ids: Vec<String>) {
        self.mcp_selected = self
            .mcp_servers
            .iter()
            .filter(|server| ids.iter().any(|id| id == &server.id))
            .map(|server| server.id.clone())
            .collect();
    }

    /// EXP-872: every signed-in login the TARGET machine reports, ACROSS
    /// agents — its own for a remote run, this install's for a local one.
    /// The last used login leads ([`machine_account_options`]).
    pub(crate) fn account_options(&self, cx: &mut App) -> Vec<coding::AccountOption> {
        let available = self.pickable(cx);
        let settings = self.seed_settings(cx);
        match &self.remote {
            Some(remote) => machine_account_options(
                &remote.accounts,
                &remote.usage,
                &settings,
                &available,
            ),
            None => {
                let (accounts, usage) = crate::device_settings::own_agent_status(cx);
                machine_account_options(&accounts, &usage, &settings, &available)
            }
        }
    }

    /// The key the picker carries for the CURRENT pick.
    pub(crate) fn account_key(&self) -> String {
        account_key(self.agent, self.account.as_deref())
    }

    /// EXP-872: pick a login. It carries its agent, so this IS the agent
    /// switch — model, effort and the toggles re-seed from that agent's
    /// defaults on the target machine exactly as [`Self::set_agent`] does.
    pub(crate) fn set_account(
        &mut self,
        option: &coding::AccountOption,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.agent != option.agent {
            self.agent = option.agent;
            let settings = self.seed_settings(cx);
            self.reseed(&settings, window, cx);
        }
        self.account = option.wire_account();
    }

    /// EXP-1158: seed the pick from the machine's LAST USED login (which also
    /// decides the agent). Falls back to the settle rule when the machine
    /// offers nothing at all. Only a fresh composer and a device switch
    /// settle — an in-progress pick is never yanked.
    fn settle_account(&mut self, window: &mut Window, cx: &mut App) {
        let options = self.account_options(cx);
        match coding::last_used_account_option(&options).cloned() {
            Some(option) => {
                self.agent = option.agent;
                self.account = option.wire_account();
            }
            None => {
                let settings = self.seed_settings(cx);
                let available = self.pickable(cx);
                self.agent = settled_agent(&available, settings.default_agent, self.agent);
                self.account = None;
            }
        }
        let settings = self.seed_settings(cx);
        self.reseed(&settings, window, cx);
    }

    /// The settings the seeds come from: the TARGET machine's published
    /// defaults for a remote run, this install's own for a local one.
    fn seed_settings(&self, cx: &mut App) -> coding::Settings {
        match &self.remote {
            Some(remote) => remote.settings.clone(),
            None => crate::coding_flow::CodingHub::global(cx).read(cx).settings.clone(),
        }
    }

    /// The agents the strip may offer: exactly what a remote target
    /// advertises (EXP-201 — the server re-checks the same list), else the
    /// local doctor's pickable set.
    pub(crate) fn pickable(&self, cx: &mut App) -> Vec<CodingAgent> {
        match &self.remote {
            Some(remote) => remote.agents.clone(),
            None => pickable_agents(
                crate::coding_flow::CodingHub::global(cx)
                    .read(cx)
                    .doctor
                    .report
                    .as_ref(),
            ),
        }
    }

    /// EXP-696: point the cluster at a different machine (`None` = this one)
    /// and RE-SEED off it — the target's own default agent wins, else the
    /// current pick when it can run there, else its first agent; model,
    /// effort and the toggles follow that agent's defaults on that machine.
    /// The web twin is `use-launch-options.ts`' device-seed effect.
    pub(crate) fn set_remote(
        &mut self,
        remote: Option<RemoteDefaults>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.remote = remote;
        self.computer_use = None;
        // EXP-872: profiles are per MACHINE, so the pick cannot carry over —
        // the new machine's LAST USED login settles both the agent and the
        // login, and the model/effort seeds follow it.
        self.settle_account(window, cx);
    }

    /// Rebuild the model/effort selects + toggles for [`Self::agent`] from
    /// `settings`.
    fn reseed(&mut self, settings: &coding::Settings, window: &mut Window, cx: &mut App) {
        let agent = self.agent;
        self.model = choice_select(model_choices_for(agent), settings.model_for(agent), window, cx);
        self.effort = choice_select(
            effort_choices_for(agent),
            settings.effort_for(agent),
            window,
            cx,
        );
        self.subagent_model = choice_select(
            &SUBAGENT_MODEL_CHOICES,
            settings.subagent_model_for(agent),
            window,
            cx,
        );
        (self.ultracode, self.plan_mode) = agent_defaults(settings, agent);
    }

    /// Switch the agent tab (EXP-201): rebuild the model/effort selects from
    /// the agent's own choice lists + settings defaults and re-seed the
    /// toggles (capability-masked).
    pub(crate) fn set_agent(&mut self, agent: CodingAgent, window: &mut Window, cx: &mut App) {
        if self.agent == agent {
            return;
        }
        self.agent = agent;
        // A profile id belongs to ONE agent's config dir — carrying a claude
        // profile onto a codex launch would name a directory that does not
        // exist there.
        self.account = None;
        let settings = self.seed_settings(cx);
        self.reseed(&settings, window, cx);
    }

    /// Keep the selection on a RUNNABLE agent: when the doctor report
    /// (re)lands and the selected agent has no tab anymore — or turned out
    /// signed out (EXP-409) while a runnable sibling exists — hop to the
    /// first runnable agent (mirrors the remote pickers, which only offer the
    /// device's advertised agents).
    pub(crate) fn reconcile_agent(&mut self, window: &mut Window, cx: &mut App) {
        // EXP-696: a remote target's list is authoritative — the local
        // doctor knows nothing about that machine's CLIs.
        if let Some(remote) = self.remote.clone() {
            if !remote.agents.contains(&self.agent) {
                if let Some(&first) = remote.agents.first() {
                    self.set_agent(first, window, cx);
                }
            }
            return;
        }
        let report = crate::coding_flow::CodingHub::global(cx)
            .read(cx)
            .doctor
            .report
            .clone();
        let runnable = report
            .as_ref()
            .map(|report| report.installed_agents())
            .unwrap_or_default();
        let preferred = if runnable.is_empty() {
            pickable_agents(report.as_ref())
        } else {
            runnable
        };
        if !preferred.contains(&self.agent) {
            if let Some(&first) = preferred.first() {
                self.set_agent(first, window, cx);
            }
        }
    }

    /// The cluster's choices as launch options. `resume_active` clamps plan
    /// mode off (EXP-202: the plan already happened in the conversation being
    /// continued) — pass `false` on surfaces that cannot resume. EXP-662: a
    /// resume also keeps the RECORDED agent, so only `model`/`effort` from
    /// here reach it, and only while the picker sits on that same agent.
    pub(crate) fn options(&self, resume_active: bool, cx: &App) -> LaunchOptions {
        LaunchOptions {
            agent: self.agent,
            model: selected(&self.model, cx),
            // Ignored by the argv while ultracode is on (ultracode IS the
            // effort level); blank = omit the flag.
            effort: selected(&self.effort, cx),
            // Capability-clamped so a stale toggle can never leak onto an
            // agent that doesn't support it.
            ultracode: self.ultracode && self.agent.supports_ultracode(),
            plan_mode: self.plan_mode && self.agent.supports_plan_mode() && !resume_active,
            // EXP-981: claude-only, clamped like the toggles above.
            subagent_model: if self.agent.supports_subagent_model() {
                selected(&self.subagent_model, cx)
            } else {
                String::new()
            },
            // EXP-792/747 B7: the run's own picks, no longer hardcoded — the
            // launcher resolves the ids against the device's secret store
            // and the account against its profile dirs.
            // FEED-73: never for a subject that owns its list.
            mcp_server_ids: if self.mcp_owned_by_subject {
                Vec::new()
            } else {
                self.mcp_selected.clone()
            },
            account: self.account.clone(),
            // EXP-1249: only a pick the target can read rides the launch.
            computer_use: computer_use_pick(self.computer_use, self.computer_use_offered()),
        }
    }

    /// EXP-1249: whether the "+" menu offers Computer use for this target —
    /// always on this machine (this build reads its own pick), on a remote
    /// one only with the `computer-use-run` cap.
    pub(crate) fn computer_use_offered(&self) -> bool {
        self.remote.as_ref().is_none_or(|remote| remote.computer_use_run)
    }

    /// Whether the "Add file or image" pick may stage non-image FILES for
    /// this target — always on this machine (this build localizes them), on
    /// a remote one only with the `steer-files` cap.
    pub(crate) fn files_offered(&self) -> bool {
        files_offered_for(self.remote.as_ref().map(|remote| remote.steer_files))
    }

    /// EXP-1249: the target machine's own Computer use switch — what a run
    /// with no pick of its own gets.
    pub(crate) fn computer_use_default(&self, cx: &App) -> bool {
        match &self.remote {
            Some(remote) => remote.settings.computer_use,
            None => crate::coding_flow::CodingHub::global_ref(cx)
                .is_some_and(|hub| hub.read(cx).settings.computer_use),
        }
    }

    /// EXP-1249: the "+" menu's toggle flipped to `on`.
    pub(crate) fn set_computer_use(&mut self, on: bool) {
        self.computer_use = Some(on);
    }

    /// EXP-1249: the "+" menu's Effort pick, into the same select the
    /// grouped cluster edits.
    pub(crate) fn pick_effort(&mut self, value: &str, window: &mut Window, cx: &mut App) {
        set_choice(&self.effort, value, window, cx);
    }

    /// EXP-1249: the "+" menu's Subagents pick (claude only).
    pub(crate) fn pick_subagent_model(&mut self, value: &str, window: &mut Window, cx: &mut App) {
        set_choice(&self.subagent_model, value, window, cx);
    }

    /// EXP-792: the servers the "+" menu's MCP picker offers — none for a
    /// subject that owns its list (FEED-73) — and the current pick.
    pub(crate) fn mcp_menu(&self) -> (Vec<McpServerOption>, Vec<String>) {
        if self.mcp_owned_by_subject {
            (Vec::new(), Vec::new())
        } else {
            (self.mcp_servers.clone(), self.mcp_selected.clone())
        }
    }

    // ── EXP-825: the Agent page composer's inline options row ──────────────
    //
    // Variant B of the composer mockups: under the card ONE muted line —
    // Device (the composer's own), Agent, Model, Plan — and a `⋯` popover
    // holding Effort, Ultracode, MCP servers and Account. These are thin
    // views over the same state `render` shows as a grouped cluster, so the
    // two surfaces cannot disagree about what a run launches with.

    /// EXP-825: reseed plan mode when the composer's subject flips between
    /// "nothing picked" (a chat — a conversation, never a planning run, so
    /// OFF whatever the agent's setting says, EXP-772) and a picked subject
    /// (the agent's own default, EXP-206). `planModeOff` is read on open on
    /// the web too; the always-open composer needs the same reseed on the
    /// flip or a chat inherits the device default (the EXP-772 regression).
    pub(crate) fn reseed_plan_for_subject(&mut self, has_subject: bool, cx: &mut App) {
        self.plan_mode = if has_subject {
            agent_defaults(&self.seed_settings(cx), self.agent).1
        } else {
            false
        };
    }

    /// The Model pin over the picked agent's own model list.
    pub(crate) fn model_pin<V: Render>(
        &self,
        prefix: &'static str,
        access: fn(&mut V) -> &mut LaunchOptionsSection,
        cx: &mut Context<V>,
    ) -> AnyElement {
        let choices = model_choices_for(self.agent);
        let picked = selected(&self.model, cx);
        let label = pin_label(choices, Some(picked.as_str()).filter(|v| !v.is_empty()));
        let trigger = inline_pin_trigger(SharedString::from(format!("{prefix}-model")), label, cx);
        choice_menu(trigger, choices, picked, |section| &section.model, access, cx)
            .into_any_element()
    }

    /// The Plan switch; `None` for an agent without a plan mode.
    pub(crate) fn plan_toggle<V: Render>(
        &self,
        prefix: &'static str,
        access: fn(&mut V) -> &mut LaunchOptionsSection,
        cx: &mut Context<V>,
    ) -> Option<AnyElement> {
        if !self.agent.supports_plan_mode() {
            return None;
        }
        Some(inline_switch(
            SharedString::from(format!("{prefix}-plan")),
            "Plan",
            self.plan_mode,
            move |view: &mut V, on| access(view).plan_mode = on,
            cx,
        ))
    }

    /// EXP-1249: everything the composer's "+" menu draws, read off the
    /// section in ONE pass — the menu builder runs in the MENU's context, so
    /// it cannot reach back into the host view for a second look.
    pub(crate) fn composer_menu(&self, cx: &App) -> ComposerMenu {
        let ultracode_on = self.ultracode && self.agent.supports_ultracode();
        let (mcp_servers, mcp_selected) = self.mcp_menu();
        ComposerMenu {
            effort_label: self.agent.effort_label(),
            effort_choices: effort_choices_for(self.agent),
            effort_picked: selected(&self.effort, cx),
            effort_locked: ultracode_on,
            subagent_picked: self
                .agent
                .supports_subagent_model()
                .then(|| selected(&self.subagent_model, cx)),
            ultracode: self.agent.supports_ultracode().then_some(self.ultracode),
            mcp_servers,
            mcp_selected,
            computer_use: computer_use_shown(
                self.computer_use,
                self.computer_use_offered(),
                self.computer_use_default(cx),
            ),
        }
    }

    /// EXP-872 — THE account pin: ONE picker over every signed-in login the
    /// TARGET machine reports, across agents. It replaced the agent pin and
    /// the old account pin both — picking a login picks its agent — so it
    /// renders WHENEVER the machine offers anything, and collapses to plain
    /// text (no chevron) when there is a single login to spend.
    pub(crate) fn account_pin<V: Render>(
        &self,
        prefix: &'static str,
        access: fn(&mut V) -> &mut LaunchOptionsSection,
        cx: &mut Context<V>,
    ) -> Option<AnyElement> {
        let options = self.account_options(cx);
        if options.is_empty() {
            return None;
        }
        let current = self.account_key();
        let view = cx.entity().downgrade();
        Some(crate::coding_selects::account_picker(
            SharedString::from(format!("{prefix}-account")),
            &options,
            Some(current.as_str()),
            crate::coding_selects::AccountTrigger::Inline,
            move |option, window, cx| {
                if let Some(view) = view.upgrade() {
                    let option = option.clone();
                    view.update(cx, |view, cx| {
                        access(view).set_account(&option, window, cx);
                        cx.notify();
                    });
                }
            },
            cx,
        ))
    }
}

/// EXP-1249 — the composer "+" menu's run options, snapshotted off a
/// [`LaunchOptionsSection`] (the `⋯` overlay EXP-991 drew is gone: its rows
/// moved into the menu).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ComposerMenu {
    pub(crate) effort_label: &'static str,
    pub(crate) effort_choices: &'static [(&'static str, &'static str)],
    pub(crate) effort_picked: String,
    /// Ultracode IS the effort level while it is on, so the row says so and
    /// takes no submenu.
    pub(crate) effort_locked: bool,
    /// `None` for an agent whose subagents take no model pin.
    pub(crate) subagent_picked: Option<String>,
    /// `None` for an agent without dynamic workflows.
    pub(crate) ultracode: Option<bool>,
    /// Empty hides the MCP servers row.
    pub(crate) mcp_servers: Vec<McpServerOption>,
    pub(crate) mcp_selected: Vec<String>,
    /// The Computer use toggle's state; `None` hides the row.
    pub(crate) computer_use: Option<bool>,
}

impl ComposerMenu {
    /// The Effort row's trailing value.
    pub(crate) fn effort_value(&self) -> String {
        if self.effort_locked {
            "Ultracode".to_string()
        } else {
            pin_label(
                self.effort_choices,
                Some(self.effort_picked.as_str()).filter(|value| !value.is_empty()),
            )
        }
    }

    /// The Subagents row's trailing value (`None` = no row).
    pub(crate) fn subagent_value(&self) -> Option<String> {
        self.subagent_picked.as_deref().map(|picked| {
            Some(picked)
                .filter(|value| !value.is_empty())
                .map_or_else(
                    || SUBAGENT_DEFAULT_LABEL.to_string(),
                    |picked| pin_label(&SUBAGENT_MODEL_CHOICES, Some(picked)),
                )
        })
    }

    /// The MCP servers row's trailing value: how many are picked, `None` at
    /// zero (composer-menu.json: no value at zero; the row itself shows
    /// whenever the team has a server).
    pub(crate) fn mcp_value(&self) -> Option<String> {
        (!self.mcp_selected.is_empty()).then(|| self.mcp_selected.len().to_string())
    }
}

/// EXP-1249: the per-run pick a launch carries — only for a target that
/// reads it (`offered`), else `None` (its own switch decides).
pub(crate) fn computer_use_pick(explicit: Option<bool>, offered: bool) -> Option<bool> {
    explicit.filter(|_| offered)
}

/// EXP-1249: the toggle's state — the run's pick, else the machine's
/// switch; `None` (no row) for a target that cannot read a pick.
pub(crate) fn computer_use_shown(
    explicit: Option<bool>,
    offered: bool,
    device_default: bool,
) -> Option<bool> {
    offered.then(|| explicit.unwrap_or(device_default))
}

/// Write `value` into a model/effort select.
fn set_choice(select: &ChoiceSelect, value: &str, window: &mut Window, cx: &mut App) {
    select.update(cx, |state, cx| {
        state.set_selected_value(&SharedString::from(value.to_string()), window, cx)
    });
}

/// [`LaunchOptionsSection::files_offered`]'s pure rule: `None` = this
/// machine (always), `Some(cap)` = the remote target's `steer-files` cap.
fn files_offered_for(remote_steer_files: Option<bool>) -> bool {
    remote_steer_files.is_none_or(|steer_files| steer_files)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The file pick's target gate: the local machine always takes files, a
    /// remote one only with the `steer-files` cap.
    #[test]
    fn files_are_offered_locally_and_on_a_remote_with_the_cap() {
        assert!(files_offered_for(None));
        assert!(files_offered_for(Some(true)));
        assert!(!files_offered_for(Some(false)));
    }

    fn server(id: &str, name: &str, on_by_default: bool) -> McpServerOption {
        McpServerOption {
            id: id.into(),
            name: name.into(),
            blocked: None,
            enabled_by_default: on_by_default,
            ..Default::default()
        }
    }

    /// EXP-792: the shared picker's rows — a blocked server keeps its reason
    /// as the second line but is NEVER disabled (it still toggles; the
    /// launch merely starts without it), and the glyph inputs travel.
    #[test]
    fn the_computer_use_toggle_follows_the_device_until_picked() {
        // A target that cannot read a pick shows no row and sends nothing.
        assert_eq!(computer_use_shown(Some(true), false, true), None);
        assert_eq!(computer_use_pick(Some(true), false), None);
        // No pick: the row mirrors the machine's switch, the launch sends
        // nothing (the machine decides).
        assert_eq!(computer_use_shown(None, true, true), Some(true));
        assert_eq!(computer_use_shown(None, true, false), Some(false));
        assert_eq!(computer_use_pick(None, true), None);
        // A pick wins, both ways, and rides the launch.
        assert_eq!(computer_use_shown(Some(false), true, true), Some(false));
        assert_eq!(computer_use_pick(Some(false), true), Some(false));
        assert_eq!(computer_use_pick(Some(true), true), Some(true));
    }

    #[test]
    fn composer_menu_values_read_like_the_old_overlay() {
        let menu = ComposerMenu {
            effort_label: "Effort",
            effort_choices: &[("Low", "low"), ("High", "high")],
            effort_picked: "high".into(),
            effort_locked: false,
            subagent_picked: Some(String::new()),
            ultracode: Some(false),
            mcp_servers: vec![server("a", "Linear", true), server("b", "Sentry", false)],
            mcp_selected: vec!["a".into()],
            computer_use: Some(true),
        };
        assert_eq!(menu.effort_value(), "High");
        assert_eq!(menu.subagent_value().as_deref(), Some(SUBAGENT_DEFAULT_LABEL));
        assert_eq!(menu.mcp_value().as_deref(), Some("1"));
        let locked = ComposerMenu { effort_locked: true, ..menu.clone() };
        assert_eq!(locked.effort_value(), "Ultracode");
        let none_picked = ComposerMenu { mcp_selected: Vec::new(), ..menu.clone() };
        assert_eq!(none_picked.mcp_value(), None, "no value at zero");
        let bare = ComposerMenu { mcp_servers: Vec::new(), subagent_picked: None, ..menu };
        assert_eq!(bare.subagent_value(), None);
    }

    #[test]
    fn picker_rows_keep_a_blocked_server_pickable() {
        let servers = vec![
            McpServerOption {
                url: Some("https://mcp.linear.app/mcp".into()),
                ..server("a", "Linear", true)
            },
            McpServerOption {
                blocked: Some(MCP_CONNECT_FIRST.to_string()),
                command: Some("npx acme".into()),
                ..server("b", "Local", false)
            },
        ];
        let rows = mcp_picker_servers(&servers);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].url.as_deref(), Some("https://mcp.linear.app/mcp"));
        assert_eq!(rows[0].description, None);
        assert_eq!(rows[1].command.as_deref(), Some("npx acme"));
        assert_eq!(rows[1].description.as_deref(), Some(MCP_CONNECT_FIRST));
        assert!(rows.iter().all(|row| !row.disabled));
    }

    /// EXP-792: web `mcpPickSummary`, string for string — two names spell
    /// themselves out, three or more overflow into `+n`.
    #[test]
    fn mcp_summary_matches_the_web() {
        let servers = vec![
            server("a", "Linear", false),
            server("b", "Notion", false),
            server("c", "Sentry", false),
            server("d", "Figma", false),
        ];
        assert_eq!(mcp_pick_summary(&servers, &[]), "None");
        assert_eq!(mcp_pick_summary(&servers, &["a".into()]), "Linear");
        assert_eq!(
            mcp_pick_summary(&servers, &["a".into(), "b".into()]),
            "Linear, Notion"
        );
        assert_eq!(
            mcp_pick_summary(&servers, &["a".into(), "b".into(), "c".into()]),
            "Linear, Notion +1"
        );
        assert_eq!(
            mcp_pick_summary(&servers, &["a".into(), "b".into(), "c".into(), "d".into()]),
            "Linear, Notion +2"
        );
        // The summary follows the REGISTRY order, not the click order — the
        // launcher's env positions do too.
        assert_eq!(
            mcp_pick_summary(&servers, &["c".into(), "a".into()]),
            "Linear, Sentry"
        );
        // An id whose row has gone contributes nothing.
        assert_eq!(mcp_pick_summary(&servers, &["gone".into()]), "None");
    }

    /// The seed is `enabledByDefault` (web `preselectMcpServerIds` with no
    /// saved pick), minus every server the person has not connected.
    #[test]
    fn mcp_seed_is_the_default_enabled_set() {
        let mut unconnected = server("d", "Figma", true);
        unconnected.blocked = Some(MCP_CONNECT_FIRST.to_string());
        let servers = vec![
            server("a", "Linear", true),
            server("b", "Notion", false),
            server("c", "Sentry", true),
            unconnected,
        ];
        assert_eq!(mcp_default_ids(&servers), vec!["a".to_string(), "c".to_string()]);
        assert!(mcp_default_ids(&[]).is_empty());
    }

    /// The seed fires once per TEAM: renders in between only clamp (a tick
    /// under the cursor survives), a team switch resets it so the next
    /// team's `enabled_by_default` set lands ON instead of being filtered
    /// out against the old ticks (EXP-825 review).
    #[test]
    fn mcp_seed_resets_on_team_switch() {
        let team_a = vec![server("a", "Linear", true), server("b", "Notion", false)];
        let mut seeded = false;
        let mut selected = Vec::new();
        seed_or_clamp_mcp(&mut seeded, &mut selected, &team_a);
        assert_eq!(selected, vec!["a".to_string()]);
        // The person ticks Notion; the per-render call keeps it.
        selected.push("b".into());
        seed_or_clamp_mcp(&mut seeded, &mut selected, &team_a);
        assert_eq!(selected, vec!["a".to_string(), "b".to_string()]);

        // Team switch: the list is empty while the fetch is out, then the
        // new team's rows arrive.
        let team_b = vec![server("x", "Sentry", false), server("y", "Figma", true)];
        let mut stale = seeded;
        let mut stale_selected = selected.clone();
        seed_or_clamp_mcp(&mut stale, &mut stale_selected, &[]);
        seed_or_clamp_mcp(&mut stale, &mut stale_selected, &team_b);
        assert!(stale_selected.is_empty(), "without the reset every new server lands OFF");

        seeded = false; // `LaunchOptionsSection::reset_mcp_seed`
        seed_or_clamp_mcp(&mut seeded, &mut selected, &[]);
        assert!(selected.is_empty());
        seed_or_clamp_mcp(&mut seeded, &mut selected, &team_b);
        assert_eq!(selected, vec!["y".to_string()]);
        assert!(seeded);
    }

    /// EXP-792: only a usable connection (or none needed) is ready; every
    /// other status greys the row and says where to fix it.
    #[test]
    fn mcp_block_reason_reads_the_callers_connection() {
        let connection = |status: &str| api::mcp_servers::McpConnection {
            status: status.into(),
            expires_at: None,
            error: None,
        };
        assert_eq!(mcp_block_reason(&connection("connected")), None);
        assert_eq!(mcp_block_reason(&connection("not_needed")), None);
        for status in ["not_connected", "future"] {
            assert_eq!(
                mcp_block_reason(&connection(status)).as_deref(),
                Some(MCP_CONNECT_FIRST),
                "{status}"
            );
        }
        for status in ["expired", "error"] {
            assert_eq!(
                mcp_block_reason(&connection(status)).as_deref(),
                Some(MCP_RECONNECT_FIRST),
                "{status}"
            );
        }
    }

    /// EXP-872: the machine's logins flatten into ONE list across agents,
    /// the last used login first, clamped to the agents it can RUN. A machine
    /// that reports no login still offers one row per runnable agent, named
    /// by the agent — the picker never goes empty while there is something to
    /// launch.
    #[test]
    fn account_options_flatten_across_agents_and_never_go_empty() {
        use coding::agent_accounts::{AgentAccount, AgentAccounts, AgentProfileEntry};
        let mut settings = coding::Settings {
            default_agent: CodingAgent::Codex,
            ..coding::Settings::default()
        };
        let mut accounts = AgentAccounts::new();
        accounts.insert(
            "claude".into(),
            AgentAccount {
                signed_in: true,
                profiles: vec![
                    AgentProfileEntry {
                        id: "work".into(),
                        signed_in: true,
                        email: Some("work@x.test".into()),
                        active: true,
                        ..Default::default()
                    },
                    AgentProfileEntry {
                        id: "home".into(),
                        signed_in: true,
                        email: Some("home@x.test".into()),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
        );
        accounts.insert(
            "codex".into(),
            AgentAccount {
                signed_in: true,
                email: Some("codex@x.test".into()),
                profiles: vec![AgentProfileEntry {
                    id: "main".into(),
                    signed_in: true,
                    email: Some("codex@x.test".into()),
                    active: true,
                    ..Default::default()
                }],
                ..Default::default()
            },
        );
        let usage = coding::agent_usage::AgentUsageMap::new();
        let all = CodingAgent::ALL.to_vec();
        let options = machine_account_options(&accounts, &usage, &settings, &all);
        assert_eq!(
            options
                .iter()
                .map(|option| option.account_option_key())
                .collect::<Vec<_>>(),
            vec![
                "codex:main".to_string(),
                "claude:work".to_string(),
                "claude:home".to_string(),
            ]
        );
        assert!(options[0].is_last_used);
        // The last used agent's ACTIVE login leads.
        settings.default_agent = CodingAgent::Claude;
        let claude_last = machine_account_options(&accounts, &usage, &settings, &all);
        assert_eq!(claude_last[0].account_option_key(), "claude:work");

        // An agent the machine cannot RUN offers no login.
        let claude_only =
            machine_account_options(&accounts, &usage, &settings, &[CodingAgent::Claude]);
        assert!(claude_only.iter().all(|option| option.agent == CodingAgent::Claude));

        // No login reported at all: one UNPINNED row per runnable agent,
        // named by it (the device resolves the profile, or refuses).
        let bare = machine_account_options(
            &AgentAccounts::new(),
            &usage,
            &settings,
            &[CodingAgent::Claude, CodingAgent::Codex],
        );
        assert_eq!(
            bare.iter().map(|option| option.email.as_str()).collect::<Vec<_>>(),
            vec!["Claude Code", "Codex"]
        );
        assert!(bare[0].is_last_used, "the machine's last used agent leads");
        assert!(bare.iter().all(|option| option.id.is_empty()));
        // Nothing runnable at all is the one empty case (the launch blocker
        // names the reason instead of a dead row).
        assert!(machine_account_options(&AgentAccounts::new(), &usage, &settings, &[]).is_empty());

        // The picker key of an unnamed account is the agent's unpinned row.
        assert_eq!(account_key(CodingAgent::Claude, None), "claude:");
        assert_eq!(account_key(CodingAgent::Codex, Some("work")), "codex:work");
    }

    /// EXP-749: an agent a remote machine has installed but cannot speak ACP
    /// with cannot start a run there (the composer's blocker names it).
    #[test]
    fn an_agent_missing_from_the_acp_list_cannot_run_a_session() {
        assert!(cannot_run_session(&[], CodingAgent::Codex));
        assert!(!cannot_run_session(
            &[CodingAgent::Claude, CodingAgent::Codex],
            CodingAgent::Codex
        ));
    }

    /// EXP-696: the device settle's agent rule (web
    /// `use-launch-options.ts`), which is what keeps a remote start from
    /// asking a machine to run a CLI it does not have.
    #[test]
    fn device_settle_prefers_the_machines_own_default_agent() {
        let all = CodingAgent::ALL.to_vec();
        // The machine's default wins over the standing pick.
        assert_eq!(
            settled_agent(&all, CodingAgent::Codex, CodingAgent::Claude),
            CodingAgent::Codex
        );
        // A default the machine cannot run keeps a still-runnable pick.
        assert_eq!(
            settled_agent(&[CodingAgent::Codex], CodingAgent::Claude, CodingAgent::Codex),
            CodingAgent::Codex
        );
        // Neither runnable → the machine's first agent.
        assert_eq!(
            settled_agent(&[CodingAgent::Codex], CodingAgent::Claude, CodingAgent::Claude),
            CodingAgent::Codex
        );
        // Nothing advertised at all → the standing pick stands (the launch
        // blocker names the reason instead of the picker going blank).
        assert_eq!(
            settled_agent(&[], CodingAgent::Claude, CodingAgent::Codex),
            CodingAgent::Codex
        );
    }
}
