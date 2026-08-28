//! The ONE agent/model/effort options cluster (EXP-615) — the web
//! `LaunchOptionsPane` twin, shared by every desktop surface that pins how an
//! agent run starts:
//!
//! - [`Variant::Launch`] — the Start-coding dialog and the create-action
//!   dialog: the doctor-filtered agent pill strip, the per-agent Model /
//!   Effort selects and the capability-gated toggles (ultracode, plan mode,
//!   skip permissions). This is [`LaunchOptionsSection`], which OWNS that
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
//! The state-owning half follows the [`crate::automation_editor`] idiom: the
//! host keeps a plain field and passes a `fn(&mut V) -> &mut Self` accessor,
//! so the callbacks reach back into it without a second entity.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, App, Context, InteractiveElement as _, IntoElement, ParentElement, Render,
    SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    checkbox::Checkbox, h_flex, select::Select, v_flex, ActiveTheme as _, Icon,
};

use coding::{CodingAgent, LaunchOptions};

use crate::coding_selects::{
    agent_icon, choice_select, effort_choices_for, model_choices_for, selected, ChoiceSelect,
};
use crate::controls::WebControl as _;
use crate::icons::ExpIcon;

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

/// The `(ultracode, plan_mode, skip_permissions)` settings defaults for
/// `agent`, capability-masked (EXP-201: ultracode is Claude-only, plan mode
/// is claude+pi since EXP-441, skip does not exist for pi). EXP-206: ONE set
/// of defaults — a single-issue run and a multi-issue batch run seed
/// identically, and plan/skip are per-AGENT settings.
pub(crate) fn agent_defaults(
    settings: &coding::Settings,
    agent: CodingAgent,
) -> (bool, bool, bool) {
    (
        settings.claude_ultracode && agent.supports_ultracode(),
        settings.plan_mode_for(agent) && agent.supports_plan_mode(),
        settings.skip_permissions_for(agent) && agent.supports_skip_permissions(),
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

/// The ONE agent strip (EXP-201, shared since EXP-615): the same web-capsule
/// tabs on every surface — launch dialogs and automation editors alike.
/// `active` is `None` only while an automation has no device bound yet
/// (nothing highlighted).
pub(crate) fn agent_tabs<V: Render>(
    id: &'static str,
    pills: Vec<AgentPill>,
    active: Option<usize>,
    on_select: impl Fn(&mut V, usize, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    // The web TabsList capsule: full width, equal segments — the same
    // primitive the subject tabs use, not a loose centered pill row.
    let on_select = std::rc::Rc::new(on_select);
    crate::controls::segmented(cx).children(pills.into_iter().enumerate().map(|(ix, pill)| {
        let on_select = on_select.clone();
        crate::controls::segmented_item(active == Some(ix), cx)
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
    }))
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

/// One [`Variant::Automation`] choice pin: "Device default" + the agent's own
/// choice list, writing through the `pick` accessor on the host's state `S`.
#[allow(clippy::too_many_arguments)] // two call sites, one per pin
pub(crate) fn choice_pin<V: Render, S: 'static>(
    prefix: &'static str,
    key: &'static str,
    choices: &'static [(&'static str, &'static str)],
    picked: Option<&str>,
    pick: fn(&mut S) -> &mut Option<String>,
    access: fn(&mut V) -> &mut S,
    cx: &mut Context<V>,
) -> impl IntoElement {
    use gpui_component::button::Button;
    use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};

    let label = picked
        .and_then(|value| {
            choices
                .iter()
                .find(|(_, choice)| *choice == value)
                .map(|(label, _)| (*label).to_string())
        })
        .unwrap_or_else(|| CLI_DEFAULT_LABEL.to_string());
    let current = picked.map(str::to_string);
    let view = cx.entity().downgrade();
    Button::new(SharedString::from(format!("{prefix}-pin-{key}")))
        .outline()
        .cursor_pointer()
        .web_input_sm()
        .w_full()
        .label(SharedString::from(label))
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
    /// Full permission bypass (claude/codex; pi has no permission system).
    pub(crate) skip_permissions: bool,
}

impl LaunchOptionsSection {
    /// Seed from the settings defaults for the settings' default agent.
    pub(crate) fn new(window: &mut Window, cx: &mut App) -> Self {
        let settings = crate::coding_flow::CodingHub::global(cx).read(cx).settings.clone();
        let agent = settings.default_agent;
        let (ultracode, plan_mode, skip_permissions) = agent_defaults(&settings, agent);
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
            skip_permissions,
        }
    }

    /// Switch the agent tab (EXP-201): rebuild the model/effort selects from
    /// the agent's own choice lists + settings defaults and re-seed the
    /// toggles (capability-masked).
    pub(crate) fn set_agent(&mut self, agent: CodingAgent, window: &mut Window, cx: &mut App) {
        if self.agent == agent {
            return;
        }
        self.agent = agent;
        let settings = crate::coding_flow::CodingHub::global(cx).read(cx).settings.clone();
        self.model = choice_select(model_choices_for(agent), settings.model_for(agent), window, cx);
        self.effort = choice_select(
            effort_choices_for(agent),
            settings.effort_for(agent),
            window,
            cx,
        );
        (self.ultracode, self.plan_mode, self.skip_permissions) = agent_defaults(&settings, agent);
    }

    /// Keep the selection on a RUNNABLE agent: when the doctor report
    /// (re)lands and the selected agent has no tab anymore — or turned out
    /// signed out (EXP-409) while a runnable sibling exists — hop to the
    /// first runnable agent (mirrors the remote pickers, which only offer the
    /// device's advertised agents).
    pub(crate) fn reconcile_agent(&mut self, window: &mut Window, cx: &mut App) {
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
            skip_permissions: self.skip_permissions && self.agent.supports_skip_permissions(),
        }
    }

    /// The whole [`Variant::Launch`] cluster: agent strip · Model/Effort ·
    /// [`resume_row`] · toggles. `hide_plan_mode` drops the Plan-mode row
    /// (the resume case — [`Self::options`] clamps it off regardless).
    pub(crate) fn render<V: Render>(
        &self,
        prefix: &'static str,
        access: fn(&mut V) -> &mut Self,
        resume_row: Option<gpui::AnyElement>,
        hide_plan_mode: bool,
        cx: &mut Context<V>,
    ) -> gpui::AnyElement {
        let report = crate::coding_flow::CodingHub::global(cx)
            .read(cx)
            .doctor
            .report
            .clone();
        let pickable = pickable_agents(report.as_ref());
        let unauthed: Vec<CodingAgent> = report
            .as_ref()
            .map(|report| report.unauthed_agents())
            .unwrap_or_default();
        let active_ix = pickable
            .iter()
            .position(|agent| *agent == self.agent)
            .unwrap_or(0);
        let click_agents = pickable.clone();
        let strip = agent_tabs(
            prefix,
            launch_pills(&pickable, &unauthed),
            Some(active_ix),
            move |view: &mut V, ix, window, cx| {
                if let Some(agent) = click_agents.get(ix).copied() {
                    access(view).set_agent(agent, window, cx);
                    cx.notify();
                }
            },
            cx,
        );

        let agent = self.agent;
        let ultracode = self.ultracode;
        let effort_hint =
            (ultracode && agent.supports_ultracode()).then_some("ultracode sets effort");
        let choices = h_flex()
            .gap_3()
            .w_full()
            // Top-align (h_flex centers): the Effort column grows an
            // "ultracode sets effort" hint line, which would otherwise sink
            // the Model label below the shared baseline.
            .items_start()
            .child(labeled_field(
                "Model",
                Select::new(&self.model).web_input_sm().into_any_element(),
                None,
                cx,
            ))
            .child(labeled_field(
                agent.effort_label(),
                Select::new(&self.effort)
                    .web_input_sm()
                    .disabled(ultracode && agent.supports_ultracode())
                    .into_any_element(),
                effort_hint,
                cx,
            ));

        let mut section = v_flex().w_full().gap_3().child(strip).child(choices);
        if let Some(resume_row) = resume_row {
            section = section.child(resume_row);
        }
        // The capability-gated toggles (EXP-201; hint-free since EXP-206).
        let show_plan = agent.supports_plan_mode() && !hide_plan_mode;
        if agent.supports_ultracode() || show_plan || agent.supports_skip_permissions() {
            let mut toggles = v_flex().gap_2();
            if agent.supports_ultracode() {
                toggles = toggles.child(
                    Checkbox::new(SharedString::from(format!("{prefix}-ultracode")))
                        .label("Dynamic workflows (ultracode)")
                        .checked(self.ultracode)
                        .on_click(cx.listener(move |view: &mut V, on: &bool, _, cx| {
                            access(view).ultracode = *on;
                            cx.notify();
                        }))
                        .into_any_element(),
                );
            }
            if show_plan {
                toggles = toggles.child(
                    Checkbox::new(SharedString::from(format!("{prefix}-plan-mode")))
                        .label("Plan mode")
                        .checked(self.plan_mode)
                        .on_click(cx.listener(move |view: &mut V, on: &bool, _, cx| {
                            access(view).plan_mode = *on;
                            cx.notify();
                        }))
                        .into_any_element(),
                );
            }
            if agent.supports_skip_permissions() {
                toggles = toggles.child(
                    Checkbox::new(SharedString::from(format!("{prefix}-skip-permissions")))
                        .label("Skip permissions")
                        .checked(self.skip_permissions)
                        .on_click(cx.listener(move |view: &mut V, on: &bool, _, cx| {
                            access(view).skip_permissions = *on;
                            cx.notify();
                        }))
                        .into_any_element(),
                );
            }
            section = section.child(toggles);
        }
        section.into_any_element()
    }
}
