//! The ONE agent/model/effort options cluster (EXP-615) — the web
//! `LaunchOptionsPane` twin, shared by every desktop surface that pins how an
//! agent run starts:
//!
//! - [`Variant::Launch`] — the Start-coding dialog and the create-action
//!   dialog: the doctor-filtered agent pill strip, the per-agent Model /
//!   Effort selects and the capability-gated toggles (ultracode, plan
//!   mode). This is [`LaunchOptionsSection`], which OWNS that
//!   state and hands out a [`LaunchOptions`] snapshot.
//! - [`Variant::Automation`] — [`crate::automation_editor`]'s launch PINS:
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
    div, App, Context, Div, InteractiveElement as _, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement as _, Stateful, Styled, Window,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_component::switch::Switch;
use gpui_component::{select::Select, v_flex, ActiveTheme as _, Icon};

use coding::{CodingAgent, LaunchOptions};

use crate::coding_selects::{
    agent_icon, choice_select, effort_choices_for, model_choices_for, selected, ChoiceSelect,
};
use crate::icons::ExpIcon;
use crate::surface;

/// Which surface the cluster is rendering for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Variant {
    /// A run that starts NOW: every field resolves to a concrete value.
    Launch,
    /// An automation's pins: every field may stay unset ("Device default"),
    /// and the bound machine's own launch defaults fill the gaps.
    Automation,
}

/// The label the model/effort pickers show while nothing is pinned — the
/// run then follows the agent CLI's own defaults, same wording as launch.
pub(crate) const CLI_DEFAULT_LABEL: &str = "CLI default";

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
/// claude+pi since EXP-441). EXP-206: ONE set of defaults — a single-issue
/// run and a multi-issue batch run seed identically, and plan mode is a
/// per-AGENT setting. EXP-690 retired the skip-permissions toggle (every
/// run bypasses).
pub(crate) fn agent_defaults(settings: &coding::Settings, agent: CodingAgent) -> (bool, bool) {
    (
        settings.claude_ultracode && agent.supports_ultracode(),
        settings.plan_mode_for(agent) && agent.supports_plan_mode(),
    )
}

/// The LAUNCH strip's pills for `agents` (the doctor's pickable list).
pub(crate) fn launch_pills(
    agents: &[CodingAgent],
    unauthed: &[CodingAgent],
) -> Vec<AgentPill> {
    agents
        .iter()
        .map(|agent| AgentPill {
            label: SharedString::from(agent.label()),
            icon: Some(agent_icon(*agent)),
            dimmed: unauthed.contains(agent),
            note: unauthed
                .contains(agent)
                .then(|| SharedString::from("not signed in")),
        })
        .collect()
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

/// A labeled field column with an optional muted hint under the control.
pub(crate) fn labeled_field(
    label: impl Into<SharedString>,
    field: gpui::AnyElement,
    hint: Option<&'static str>,
    cx: &App,
) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    v_flex()
        .flex_1()
        .gap_1()
        .child(div().text_xs().text_color(muted).child(label.into()))
        .child(field)
        .when_some(hint, |this, hint| {
            this.child(div().text_xs().text_color(muted.opacity(0.7)).child(hint))
        })
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

/// One [`Variant::Automation`] choice pin as a GROUPED picker row (EXP-694,
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
            Switch::new(self.id)
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
/// capsule. The Start-coding cluster ([`LaunchOptionsSection::render`]), the
/// Device settings dialog and Settings → Agents all build the same group
/// through this builder; only the STATE behind the selects differs (one
/// launch draft here, a per-agent defaults map there), plus the rows each
/// surface splices in:
///
/// - [`Self::leading`] — between the tabs and Model (the CLI-path row).
/// - [`Self::after_effort`] — between the effort row and the toggles (the
///   Start-coding resume row).
/// - [`Self::trailing`] — under the toggles (the account + usage rows).
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
    after_effort: Vec<Div>,
    trailing: Vec<Div>,
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
            after_effort: Vec::new(),
            trailing: Vec::new(),
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

    pub(crate) fn after_effort(mut self, rows: Vec<Div>) -> Self {
        self.after_effort = rows;
        self
    }

    pub(crate) fn trailing(mut self, rows: Vec<Div>) -> Self {
        self.trailing = rows;
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
            after_effort,
            trailing,
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
        rows.extend(after_effort);
        for toggle in toggles {
            rows.push(toggle.row(cx));
        }
        rows.extend(trailing);
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
    pub(crate) settings: coding::Settings,
}

/// The [`Variant::Launch`] cluster's own state: which agent runs, its
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
    /// Native plan mode (`--permission-mode plan`; pi via its extension).
    pub(crate) plan_mode: bool,
    /// EXP-696: `Some` while the run targets another machine — its agents
    /// and its published defaults replace the local doctor + hub everywhere.
    remote: Option<RemoteDefaults>,
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
        }
    }

    /// The whole [`Variant::Launch`] cluster as ONE inset-grouped stack
    /// (EXP-694 S4): the SHARED [`AgentDefaultsGroup`] — the embedded agent
    /// tabs row, the Model and effort picker rows, the optional `resume_row`,
    /// then the capability-gated toggles — every one a hairline-divided row of
    /// a single [`crate::surface::glass_group`], no loose controls, no
    /// free-floating capsule, no checkboxes. `hide_plan_mode` drops the
    /// Plan-mode row (the resume case — [`Self::options`] clamps it off
    /// regardless).
    pub(crate) fn render<V: Render>(
        &self,
        prefix: &'static str,
        access: fn(&mut V) -> &mut Self,
        resume_row: Option<Div>,
        hide_plan_mode: bool,
        cx: &mut Context<V>,
    ) -> gpui::AnyElement {
        let report = crate::coding_flow::CodingHub::global(cx)
            .read(cx)
            .doctor
            .report
            .clone();
        let pickable = self.pickable(cx);
        // EXP-696: a remote machine advertises only what it can RUN — its
        // signed-out CLIs never reach the picker, so nothing there is dimmed.
        let unauthed: Vec<CodingAgent> = match self.remote {
            Some(_) => Vec::new(),
            None => report
                .as_ref()
                .map(|report| report.unauthed_agents())
                .unwrap_or_default(),
        };
        let active_ix = pickable
            .iter()
            .position(|agent| *agent == self.agent)
            .unwrap_or(0);
        let click_agents = pickable.clone();
        let agent = self.agent;
        let effort_disabled = self.ultracode && agent.supports_ultracode();

        let mut group = AgentDefaultsGroup::new(
            prefix,
            agent,
            launch_pills(&pickable, &unauthed),
            Some(active_ix),
            move |view: &mut V, ix, window, cx| {
                if let Some(agent) = click_agents.get(ix).copied() {
                    access(view).set_agent(agent, window, cx);
                    cx.notify();
                }
            },
            self.model.clone(),
            self.effort.clone(),
        )
        .effort_disabled(effort_disabled);
        // EXP-698: the resume row arrives ALREADY on the group's row rhythm
        // (a `glass_toggle_row`), so it is spliced in verbatim — wrapping it
        // in a second `glass_row_shell` would double the row's padding.
        if let Some(resume_row) = resume_row {
            group = group.after_effort(vec![resume_row]);
        }
        // The capability-gated toggles (EXP-201; hint-free since EXP-206) —
        // switches on the group's row rhythm since EXP-694.
        if agent.supports_ultracode() {
            group = group.toggle(DefaultsToggle::new(
                format!("{prefix}-ultracode"),
                "Ultracode",
                self.ultracode,
                move |view: &mut V, on, _| access(view).ultracode = on,
            ));
        }
        if agent.supports_plan_mode() && !hide_plan_mode {
            group = group.toggle(DefaultsToggle::new(
                format!("{prefix}-plan-mode"),
                "Plan mode",
                self.plan_mode,
                move |view: &mut V, on, _| access(view).plan_mode = on,
            ));
        }
        group.render(cx).into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            settled_agent(
                &[CodingAgent::Claude, CodingAgent::Codex],
                CodingAgent::Pi,
                CodingAgent::Codex
            ),
            CodingAgent::Codex
        );
        // Neither runnable → the machine's first agent.
        assert_eq!(
            settled_agent(&[CodingAgent::Pi], CodingAgent::Claude, CodingAgent::Codex),
            CodingAgent::Pi
        );
        // Nothing advertised at all → the standing pick stands (the launch
        // blocker names the reason instead of the picker going blank).
        assert_eq!(
            settled_agent(&[], CodingAgent::Claude, CodingAgent::Codex),
            CodingAgent::Codex
        );
    }
}
