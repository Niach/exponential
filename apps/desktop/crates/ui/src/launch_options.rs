//! The ONE agent/model/effort options cluster (EXP-615) — the web
//! `LaunchOptionsPane` twin, shared by every desktop surface that pins how an
//! agent run starts:
//!
//! - Launch: the Agent page composer (EXP-825) and, before it, the start-coding
//!   dialog: the doctor-filtered agent pill strip, the per-agent Model /
//!   Effort selects and the capability-gated toggles (ultracode, plan
//!   mode). This is [`LaunchOptionsSection`], which OWNS that
//!   state and hands out a [`LaunchOptions`] snapshot.
//! - Automation: [`crate::automation_editor`]'s launch PINS:
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
//! The state-owning half follows the [`crate::automation_editor`] idiom: the
//! host keeps a plain field and passes a `fn(&mut V) -> &mut Self` accessor,
//! so the callbacks reach back into it without a second entity.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, AnyElement, App, Context, Div, InteractiveElement as _, IntoElement, ParentElement,
    Render, SharedString, StatefulInteractiveElement as _, Stateful, Styled, Window,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_component::{h_flex, select::Select, v_flex, ActiveTheme as _, Disableable as _, Icon};

use coding::{CodingAgent, LaunchOptions};

use crate::coding_selects::{
    agent_icon, choice_select, effort_choices_for, model_choices_for, selected, ChoiceSelect,
};
use crate::icons::ExpIcon;
use crate::surface;

/// The label the model/effort pickers show while nothing is pinned — the
/// run then follows the agent CLI's own defaults, same wording as launch.
pub(crate) const CLI_DEFAULT_LABEL: &str = "CLI default";

// ---------------------------------------------------------------------------
// EXP-792 — the MCP servers multiselect
// ---------------------------------------------------------------------------

/// One team MCP server as the run multiselect offers it, ALREADY resolved
/// against the machine the run lands on: the caller (which is the only thing
/// that knows whether this is a local or a remote start) computes
/// [`Self::blocked`] once per target, so the row itself is pure.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct McpServerOption {
    pub(crate) id: String,
    pub(crate) name: String,
    /// Why the TARGET machine cannot satisfy it (web `serverBlockReason`);
    /// `None` = ready there. A blocked server is GREYED, never hidden and
    /// never unpickable — picking one anyway is how a person finds out what
    /// to fix, and the launch blocker then names it (web parity).
    pub(crate) blocked: Option<String>,
    /// Seeds the FIRST pick (`enabledByDefault`).
    pub(crate) enabled_by_default: bool,
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

/// The seed of a fresh multiselect: the servers flagged `enabledByDefault`
/// (web `preselectMcpServerIds` with no saved pick).
pub(crate) fn mcp_default_ids(servers: &[McpServerOption]) -> Vec<String> {
    servers
        .iter()
        .filter(|server| server.enabled_by_default)
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
        // reach the launcher as an "unknown server" blocker.
        selected.retain(|id| servers.iter().any(|server| &server.id == id));
    }
}

/// EXP-792 — why a machine cannot satisfy a server (web
/// `serverBlockReason`). `entry` is that machine's readiness row: the local
/// read for this install, the synced matrix row for a remote target.
///
/// The sentence is the LAUNCHER's own ([`coding::mcp_servers`]
/// `NOT_SIGNED_IN` / `sign_in_expired`), so the greyed picker row, the
/// dialog's launch blocker and the refused run all read identically. Those
/// sentences say "on this machine" because the DEVICE wrote them; naming a
/// remote target therefore SUBSTITUTES the label into them rather than
/// suffixing one, which is what would otherwise produce "not signed in on
/// this machine on the mini". `device_label: None` = this machine, and the
/// sentence stands as written.
pub(crate) fn mcp_block_reason(
    auth: &str,
    entry: Option<crate::settings::mcp_servers::Readiness<'_>>,
    device_label: Option<&str>,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<String> {
    use crate::settings::mcp_servers::ready_on;
    if ready_on(auth, entry, now) {
        return None;
    }
    let sentence = entry
        .filter(|entry| !entry.ready)
        .and_then(|entry| entry.error.map(str::trim).filter(|error| !error.is_empty()))
        .map(str::to_string)
        .unwrap_or_else(|| match auth {
            "oauth" => coding::mcp_servers::NOT_SIGNED_IN.to_string(),
            _ => "no value on this machine".to_string(),
        });
    Some(name_the_machine(sentence, device_label))
}

/// Point a device-written sentence at the machine it is ABOUT. Substituting
/// beats appending: `sign_in_expired` puts "on this machine" mid-sentence,
/// and a suffix would leave both in.
fn name_the_machine(sentence: String, device_label: Option<&str>) -> String {
    let Some(label) = device_label.map(str::trim).filter(|label| !label.is_empty()) else {
        return sentence;
    };
    let named = sentence.replace("this machine", label);
    if named != sentence {
        return named;
    }
    format!("{sentence} on {label}")
}

// ---------------------------------------------------------------------------
// EXP-747 B7 — the account picker
// ---------------------------------------------------------------------------

/// One agent ACCOUNT PROFILE the target machine holds, as the picker offers
/// it. `id` is [`coding::agent_profiles::SYSTEM_PROFILE`] for the ambient
/// login, which [`LaunchOptions::account`] carries as `None`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AccountOption {
    pub(crate) id: String,
    pub(crate) label: String,
    /// A profile whose CLI is signed out still shows (picking it is how you
    /// find out); the row says so rather than hiding the choice.
    pub(crate) signed_in: bool,
}

/// The account rows of one machine's `agent_accounts` entry for one agent.
///
/// A machine with a single login reports NO profiles (the pre-profile
/// payload is byte-identical), which is why an empty list here means "there
/// is nothing to pick" and the row hides rather than showing one dead
/// choice.
pub(crate) fn account_options(
    account: Option<&coding::agent_accounts::AgentAccount>,
) -> Vec<AccountOption> {
    let Some(account) = account else {
        return Vec::new();
    };
    if account.profiles.len() < 2 {
        return Vec::new();
    }
    account
        .profiles
        .iter()
        .map(|profile| AccountOption {
            label: profile
                .label
                .clone()
                .filter(|label| !label.trim().is_empty())
                .unwrap_or_else(|| {
                    if profile.id == coding::agent_profiles::SYSTEM_PROFILE {
                        coding::agent_profiles::SYSTEM_LABEL.to_string()
                    } else {
                        profile.id.clone()
                    }
                }),
            id: profile.id.clone(),
            signed_in: profile.signed_in,
        })
        .collect()
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

/// The AUTOMATION strip's pills: the agent ids the BOUND device advertises.
pub(crate) fn agent_id_pills(agent_ids: &[String]) -> Vec<AgentPill> {
    agent_ids
        .iter()
        .map(|id| AgentPill {
            label: SharedString::from(agent_label(id)),
            icon: CodingAgent::parse(id).map(agent_icon),
            dimmed: false,
            note: None,
        })
        .collect()
}

/// An agent id's display name — the brand casing every picker shows.
pub(crate) fn agent_label(id: &str) -> String {
    match CodingAgent::parse(id) {
        Some(agent) => agent.label().to_string(),
        // A newer contract value still renders readably.
        None => {
            let mut chars = id.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        }
    }
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
                .children(pill.icon.map(|icon| Icon::from(icon).size_3p5()))
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
        .dropdown_caret(true)
        // EXP-697: NOT `.label()` — upstream draws that in a `flex_none` box,
        // so a long model name wraps onto a second line.
        .child(surface::picker_value_label(SharedString::from(pin_label(
            choices, picked,
        ))));
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
    trigger
        .dropdown_menu(move |mut menu, _window, _cx| {
            let default_view = view.clone();
            let current = current.clone();
            menu = menu.item(
                PopupMenuItem::new(CLI_DEFAULT_LABEL)
                    .checked(current.is_none())
                    .on_click(move |_, _, cx| {
                        if let Some(view) = default_view.upgrade() {
                            view.update(cx, |view, cx| {
                                *pick(access(view)) = None;
                                cx.notify();
                            });
                        }
                    }),
            );
            for (label, value) in choices {
                // A blank value IS "leave it to the CLI" — the same thing
                // "Device default" already says.
                if value.is_empty() {
                    continue;
                }
                let view = view.clone();
                let value = (*value).to_string();
                let checked = current.as_deref() == Some(value.as_str());
                menu = menu.item(PopupMenuItem::new(*label).checked(checked).on_click(
                    move |_, _, cx| {
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
/// with the machine's kind and the account pin with the account mark.
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

/// EXP-862: the composer options row's ICON-ONLY control (the `⋯` that
/// opens more options) — the same muted ghost as the pins, a glyph instead of
/// a word and no caret, web `Button variant="ghost" size="icon-xs"`.
pub(crate) fn inline_icon_trigger(id: SharedString, icon: ExpIcon, cx: &App) -> Button {
    Button::new(id)
        .ghost()
        .cursor_pointer()
        .h_auto()
        .px_1()
        .py_0()
        .text_color(cx.theme().muted_foreground)
        .child(Icon::new(icon).size(gpui::px(14.)))
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

/// Hangs a model/effort choice menu off `trigger`, writing the pick into the
/// section's `ChoiceSelect` (the same entity the grouped cluster's select
/// row edits, so the two never disagree).
fn choice_menu<V: Render>(
    trigger: Button,
    choices: &'static [(&'static str, &'static str)],
    picked: String,
    select: fn(&LaunchOptionsSection) -> &ChoiceSelect,
    access: fn(&mut V) -> &mut LaunchOptionsSection,
    cx: &mut Context<V>,
) -> impl IntoElement {
    let view = cx.entity().downgrade();
    trigger.dropdown_menu(move |mut menu, _window, _cx| {
        for (label, value) in choices {
            let view = view.clone();
            let value = (*value).to_string();
            let checked = picked == value;
            menu = menu.item(PopupMenuItem::new(*label).checked(checked).on_click(
                move |_, window, cx| {
                    if let Some(view) = view.upgrade() {
                        let value = value.clone();
                        view.update(cx, |view, cx| {
                            let state = select(access(view)).clone();
                            state.update(cx, |state, cx| {
                                state.set_selected_value(&SharedString::from(value), window, cx)
                            });
                            cx.notify();
                        });
                    }
                },
            ));
        }
        menu
    })
}

/// The Account pin's label: the picked profile's, else the ambient login's.
fn account_pin_label(options: &[AccountOption], picked: Option<&str>) -> String {
    options
        .iter()
        .find(|option| Some(option.id.as_str()) == picked)
        .map(|option| option.label.clone())
        .unwrap_or_else(|| coding::agent_profiles::SYSTEM_LABEL.to_string())
}

/// Hangs the Account choice menu off an already-dressed `trigger` (EXP-825:
/// shared by the grouped row and the composer's `⋯` popover).
fn account_menu<V: Render>(
    trigger: Button,
    options: &[AccountOption],
    picked: Option<&str>,
    access: fn(&mut V) -> &mut LaunchOptionsSection,
    cx: &mut Context<V>,
) -> impl IntoElement {
    let current = picked.map(str::to_string);
    let options = options.to_vec();
    let view = cx.entity().downgrade();
    trigger
        .dropdown_menu(move |mut menu, _window, _cx| {
            for option in &options {
                let view = view.clone();
                // The ambient login is `None` on the wire, so the system
                // profile writes back as "nothing pinned" rather than as the
                // literal id — `LaunchOptions::with_account` folds the two
                // together anyway, and this keeps ONE representation.
                let value = (option.id != coding::agent_profiles::SYSTEM_PROFILE)
                    .then(|| option.id.clone());
                let checked = current.as_deref() == value.as_deref();
                let label = match option.signed_in {
                    true => option.label.clone(),
                    false => format!("{} \u{2014} signed out", option.label),
                };
                // EXP-862: the trigger leads with the account mark, so every
                // row carries it too (a value shown with an icon is picked
                // with that icon).
                menu = menu.item(PopupMenuItem::new(label)
                    .icon(Icon::new(crate::icons::registry::NAV_ACCOUNT))
                    .checked(checked)
                    .on_click(
                    move |_, _, cx| {
                        if let Some(view) = view.upgrade() {
                            let value = value.clone();
                            view.update(cx, |view, cx| {
                                access(view).account = value;
                                cx.notify();
                            });
                        }
                    },
                ));
            }
            menu
        })
}

/// EXP-792 — the MCP multiselect's POPOVER, hung off an already-dressed
/// `trigger` (the chat page's inline pill).
///
/// Not a `dropdown_menu`: `PopupMenu::confirm` dismisses UNCONDITIONALLY
/// after running an item's handler (gpui-component `popup_menu.rs`, both the
/// `Item` and the `ElementItem` arm), so a checkbox menu built out of
/// [`PopupMenuItem`] would close on every single toggle. The repo's own
/// no-close multiselect is the label picker's `Popover` of checkbox rows
/// ([`crate::pickers::label_picker_popover`]) — which is also the shape the
/// web twin wears (`mcp-server-picker.tsx`: a popover of `Checkbox` command
/// items) — so that is what this builds.
///
/// A server the target machine cannot satisfy is GREYED and carries its
/// reason, but stays pickable: the launch blocker then names it, which is a
/// far better answer than a row that silently refuses to tick.
pub(crate) fn mcp_pick_popover<V: Render>(
    prefix: &str,
    trigger: Button,
    servers: &[McpServerOption],
    selected: &[String],
    toggle: impl Fn(&mut V, &str) + 'static,
    cx: &mut Context<V>,
) -> gpui_component::popover::Popover {
    use gpui_component::checkbox::Checkbox;
    use gpui_component::popover::Popover;

    let servers = servers.to_vec();
    let selected = selected.to_vec();
    let view = cx.entity().downgrade();
    let toggle = std::rc::Rc::new(toggle);
    let rows_id: SharedString = format!("{prefix}-mcp-rows").into();
    let row_prefix = prefix.to_string();
    Popover::new(SharedString::from(format!("{prefix}-mcp-popover")))
        .p_1()
        .trigger(trigger)
        .content(move |_, _window, cx| {
            let muted = cx.theme().muted_foreground;
            // The registry grows with the team — cap + scroll, like every
            // other picker list (EXP-46a).
            let mut rows = v_flex()
                .id(rows_id.clone())
                .w_full()
                .min_w(gpui::px(220.))
                .max_h(gpui::px(240.))
                .overflow_y_scroll();
            for server in &servers {
                let checked = selected.iter().any(|id| id == &server.id);
                let id = server.id.clone();
                let view = view.clone();
                let toggle = toggle.clone();
                let blocked = server.blocked.clone();
                rows = rows.child(
                    crate::pickers::picker_row(
                        SharedString::from(format!("{row_prefix}-mcp-{}", server.id)),
                        cx,
                    )
                    .when(blocked.is_some(), |row| row.opacity(0.5))
                    // The ROW owns the click — a handler on the checkbox too
                    // would double-toggle (the label picker's rule).
                    .child(
                        Checkbox::new(SharedString::from(format!(
                            "{row_prefix}-mcp-check-{}",
                            server.id
                        )))
                        .checked(checked),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_0p5()
                            .child(
                                div()
                                    .truncate()
                                    .child(SharedString::from(server.name.clone())),
                            )
                            .children(blocked.map(|reason| {
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .truncate()
                                    .child(SharedString::from(reason))
                            })),
                    )
                    .on_click(move |_, _, cx| {
                        if let Some(view) = view.upgrade() {
                            let id = id.clone();
                            let toggle = toggle.clone();
                            view.update(cx, |view, cx| {
                                toggle(view, &id);
                                cx.notify();
                            });
                        }
                    }),
                );
            }
            rows
        })
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
        } = self;
        let mut rows: Vec<Div> = vec![agent_tabs_row(prefix, pills, active, on_select, cx)];
        rows.extend(leading);
        rows.push(surface::glass_picker_row(
            "Model",
            None,
            surface::glass_picker_select(Select::new(&model)).into_any_element(),
            cx,
        ));
        rows.push(surface::glass_picker_row(
            agent.effort_label(),
            // The hint the two-column layout carried under the Effort select
            // (EXP-206) becomes the row's own second line.
            effort_disabled.then(|| SharedString::from("ultracode sets effort")),
            surface::glass_picker_select(Select::new(&effort))
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
    /// beaten yet): the Account row then has nothing to offer and hides.
    pub(crate) accounts: coding::agent_accounts::AgentAccounts,
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
    /// EXP-747 B7: the agent account PROFILE the run signs in as on the
    /// target machine; `None` = its ambient login.
    account: Option<String>,
}

impl LaunchOptionsSection {
    /// Seed from the settings defaults for the settings' default agent.
    pub(crate) fn new(window: &mut Window, cx: &mut App) -> Self {
        let settings = crate::coding_flow::CodingHub::global(cx).read(cx).settings.clone();
        let agent = settings.default_agent;
        let (ultracode, plan_mode) = agent_defaults(&settings, agent);
        Self {
            agent,
            model: choice_select(model_choices_for(agent), settings.model_for(agent), window, cx),
            effort: choice_select(
                effort_choices_for(agent),
                settings.effort_for(agent),
                window,
                cx,
            ),
            ultracode,
            plan_mode,
            remote: None,
            mcp_servers: Vec::new(),
            mcp_selected: Vec::new(),
            mcp_seeded: false,
            account: None,
        }
    }

    /// EXP-792: offer these team MCP servers, already resolved against the
    /// machine the run lands on. Safe to call on every render: it re-points
    /// the blockers (which move with the device pick), clamps a pick whose
    /// row has gone, and seeds `enabled_by_default` exactly once.
    pub(crate) fn set_mcp_servers(&mut self, servers: Vec<McpServerOption>) {
        seed_or_clamp_mcp(&mut self.mcp_seeded, &mut self.mcp_selected, &servers);
        self.mcp_servers = servers;
    }

    /// Forget the seed: the owner switched TEAM, so the next non-empty list
    /// (the new team's servers) seeds `enabled_by_default` again instead of
    /// being clamped against the old team's ticks (which would land every
    /// new server OFF).
    pub(crate) fn reset_mcp_seed(&mut self) {
        self.mcp_seeded = false;
    }

    /// The picked server ids — what [`Self::options`] puts on the wire, and
    /// what a launch pre-check walks.
    pub(crate) fn mcp_server_ids(&self) -> &[String] {
        &self.mcp_selected
    }

    /// The offered rows, for a caller that wants to name a blocked pick.
    pub(crate) fn mcp_servers(&self) -> &[McpServerOption] {
        &self.mcp_servers
    }

    /// Tick/untick one server. Kept in REGISTRY order so the launcher's
    /// per-server env positions (`EXP_MCP_TOKEN_<n>`) follow the list the
    /// person sees, not the order they happened to click in.
    pub(crate) fn toggle_mcp_server(&mut self, id: &str) {
        if let Some(at) = self.mcp_selected.iter().position(|picked| picked == id) {
            self.mcp_selected.remove(at);
            return;
        }
        self.mcp_selected.push(id.to_string());
        let order: Vec<&str> = self.mcp_servers.iter().map(|s| s.id.as_str()).collect();
        self.mcp_selected
            .sort_by_key(|id| order.iter().position(|known| known == id).unwrap_or(usize::MAX));
    }

    /// EXP-747 B7: the account profiles the TARGET machine holds for the
    /// SELECTED agent — its own for a remote run, this install's for a local
    /// one. Fewer than two = nothing to pick, and the row hides.
    fn account_options(&self, cx: &mut App) -> Vec<AccountOption> {
        let agent = self.agent.id();
        match &self.remote {
            Some(remote) => account_options(remote.accounts.get(agent)),
            None => {
                let (accounts, _usage) = crate::device_settings::own_agent_status(cx);
                account_options(accounts.get(agent))
            }
        }
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
        // Same reason as an agent switch: profiles are per MACHINE too.
        self.account = None;
        let settings = self.seed_settings(cx);
        let available = self.pickable(cx);
        self.agent = settled_agent(&available, settings.default_agent, self.agent);
        self.reseed(&settings, window, cx);
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
            // EXP-792/747 B7: the run's own picks, no longer hardcoded — the
            // launcher resolves the ids against the device's secret store
            // and the account against its profile dirs.
            mcp_server_ids: self.mcp_selected.clone(),
            account: self.account.clone(),
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

    /// The Agent pin — EXP-862: the SHARED
    /// [`crate::coding_selects::agent_picker`], one component per client. The
    /// trigger is icon-only (the brand mark plus a caret, the label in its
    /// tooltip) and the menu rows carry the same mark, so the composer row
    /// keeps its width whichever agent is selected.
    pub(crate) fn agent_pin<V: Render>(
        &self,
        prefix: &'static str,
        access: fn(&mut V) -> &mut LaunchOptionsSection,
        cx: &mut Context<V>,
    ) -> AnyElement {
        let agents = self.pickable(cx);
        let view = cx.entity().downgrade();
        crate::coding_selects::agent_picker(
            SharedString::from(format!("{prefix}-agent")),
            &agents,
            self.agent,
            move |agent, window, cx| {
                if let Some(view) = view.upgrade() {
                    view.update(cx, |view, cx| {
                        access(view).set_agent(agent, window, cx);
                        cx.notify();
                    });
                }
            },
            cx,
        )
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

    /// The Effort pin (the `⋯` popover): the picked agent's effort list;
    /// while ultracode is on the level IS ultracode, so the pin reads so and
    /// takes no menu.
    pub(crate) fn effort_pin<V: Render>(
        &self,
        prefix: &'static str,
        access: fn(&mut V) -> &mut LaunchOptionsSection,
        cx: &mut Context<V>,
    ) -> AnyElement {
        if self.ultracode && self.agent.supports_ultracode() {
            return inline_pin_trigger(
                SharedString::from(format!("{prefix}-effort")),
                "Ultracode".to_string(),
                cx,
            )
            .disabled(true)
            .into_any_element();
        }
        let choices = effort_choices_for(self.agent);
        let picked = selected(&self.effort, cx);
        let label = pin_label(choices, Some(picked.as_str()).filter(|v| !v.is_empty()));
        let trigger = inline_pin_trigger(SharedString::from(format!("{prefix}-effort")), label, cx);
        choice_menu(trigger, choices, picked, |section| &section.effort, access, cx)
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

    /// The Ultracode switch (the `⋯` popover); `None` for an agent without
    /// it.
    pub(crate) fn ultracode_toggle<V: Render>(
        &self,
        prefix: &'static str,
        access: fn(&mut V) -> &mut LaunchOptionsSection,
        cx: &mut Context<V>,
    ) -> Option<AnyElement> {
        if !self.agent.supports_ultracode() {
            return None;
        }
        Some(inline_switch(
            SharedString::from(format!("{prefix}-ultracode")),
            "Ultracode",
            self.ultracode,
            move |view: &mut V, on| access(view).ultracode = on,
            cx,
        ))
    }

    /// The MCP servers pin (the `⋯` popover); `None` while the team offers
    /// no servers.
    pub(crate) fn mcp_pin<V: Render>(
        &self,
        prefix: &'static str,
        access: fn(&mut V) -> &mut LaunchOptionsSection,
        cx: &mut Context<V>,
    ) -> Option<AnyElement> {
        if self.mcp_servers.is_empty() {
            return None;
        }
        let trigger = inline_pin_trigger(
            SharedString::from(format!("{prefix}-mcp")),
            format!("MCP: {}", mcp_pick_summary(&self.mcp_servers, &self.mcp_selected)),
            cx,
        );
        Some(
            mcp_pick_popover(
                prefix,
                trigger,
                &self.mcp_servers,
                &self.mcp_selected,
                move |view: &mut V, id: &str| access(view).toggle_mcp_server(id),
                cx,
            )
            .into_any_element(),
        )
    }

    /// The Account pin (the `⋯` popover); `None` with fewer than two
    /// profiles on the target machine (nothing to pick).
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
        // EXP-862: the label is the PROFILE's (its name or email) behind the
        // account mark — the row no longer spends a word saying which kind of
        // pin it is.
        let trigger = inline_pin_trigger_with(
            SharedString::from(format!("{prefix}-account")),
            Some(crate::icons::registry::NAV_ACCOUNT),
            account_pin_label(&options, self.account.as_deref()),
            cx,
        );
        Some(account_menu(trigger, &options, self.account.as_deref(), access, cx).into_any_element())
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::mcp_servers::Readiness;

    fn server(id: &str, name: &str, on_by_default: bool) -> McpServerOption {
        McpServerOption {
            id: id.into(),
            name: name.into(),
            blocked: None,
            enabled_by_default: on_by_default,
        }
    }

    fn now() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339("2026-09-09T10:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc)
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

    /// The seed is `enabledByDefault` and nothing else (web
    /// `preselectMcpServerIds` with no saved pick).
    #[test]
    fn mcp_seed_is_the_default_enabled_set() {
        let servers = vec![
            server("a", "Linear", true),
            server("b", "Notion", false),
            server("c", "Sentry", true),
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

    /// EXP-792: the greyed row's reason IS the launcher's refusal sentence,
    /// pointed at the machine it is about.
    #[test]
    fn mcp_block_reason_names_the_target_machine() {
        let now = now();
        // A no-auth server needs nothing anywhere.
        assert_eq!(mcp_block_reason("none", None, Some("the mini"), now), None);
        // Ready → nothing to say.
        let ready = Readiness { ready: true, expires_at: None, error: None };
        assert_eq!(mcp_block_reason("secret", Some(ready), None, now), None);

        // LOCAL: the device wrote the sentence, so it stands as written —
        // no second "on this machine" tacked onto the end.
        let refused = Readiness {
            ready: false,
            expires_at: None,
            error: Some(coding::mcp_servers::NOT_SIGNED_IN),
        };
        assert_eq!(
            mcp_block_reason("oauth", Some(refused), None, now).as_deref(),
            Some("not signed in on this machine")
        );
        // REMOTE: the label is SUBSTITUTED into it, not appended.
        assert_eq!(
            mcp_block_reason("oauth", Some(refused), Some("the mini"), now).as_deref(),
            Some("not signed in on the mini")
        );
        // Mid-sentence too (`coding::mcp_servers::sign_in_expired`).
        let expired = coding::mcp_servers::sign_in_expired("Linear", None);
        let entry = Readiness { ready: false, expires_at: None, error: Some(&expired) };
        let named = mcp_block_reason("oauth", Some(entry), Some("the mini"), now).unwrap();
        assert!(named.starts_with("sign-in expired on the mini"), "{named}");
        assert!(!named.contains("this machine"), "{named}");

        // No report at all falls back to the per-auth sentence.
        assert_eq!(
            mcp_block_reason("oauth", None, None, now).as_deref(),
            Some("not signed in on this machine")
        );
        assert_eq!(
            mcp_block_reason("secret", None, Some("the mini"), now).as_deref(),
            Some("no value on the mini")
        );
        // A sentence that never mentions a machine still gets one.
        let odd = Readiness { ready: false, expires_at: None, error: Some("refresh failed") };
        assert_eq!(
            mcp_block_reason("oauth", Some(odd), Some("the mini"), now).as_deref(),
            Some("refresh failed on the mini")
        );
        // A blank/whitespace label is no label.
        assert_eq!(
            mcp_block_reason("oauth", Some(refused), Some("  "), now).as_deref(),
            Some("not signed in on this machine")
        );
    }

    /// EXP-747 B7: a single-login machine reports NO profiles, so there is
    /// nothing to pick and the row hides — the picker never shows one dead
    /// choice.
    #[test]
    fn account_options_appear_only_when_there_is_a_choice() {
        use coding::agent_accounts::{AgentAccount, AgentProfileEntry};
        let profile = |id: &str, label: Option<&str>, signed_in: bool| AgentProfileEntry {
            id: id.into(),
            label: label.map(str::to_string),
            signed_in,
            ..Default::default()
        };
        assert!(account_options(None).is_empty());
        assert!(account_options(Some(&AgentAccount::default())).is_empty());
        let one = AgentAccount {
            profiles: vec![profile("system", None, true)],
            ..Default::default()
        };
        assert!(account_options(Some(&one)).is_empty(), "one login is no choice");

        let two = AgentAccount {
            profiles: vec![
                profile("system", None, true),
                profile("work", Some("Work"), false),
                profile("bare", Some("  "), true),
            ],
            ..Default::default()
        };
        let options = account_options(Some(&two));
        assert_eq!(options.len(), 3);
        // The ambient login labels itself, an unlabelled profile falls back
        // to its id, and the signed-out flag rides along for the caption.
        assert_eq!(options[0].label, coding::agent_profiles::SYSTEM_LABEL);
        assert_eq!(options[1].label, "Work");
        assert!(!options[1].signed_in);
        assert_eq!(options[2].label, "bare");
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
