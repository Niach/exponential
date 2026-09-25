//! EXP-1020 — `device-settings` (3 Special components): the per-machine
//! settings surface, ONE layout on all four clients.
//!
//! The IDE's implementation is `crate::device_settings` (the dialog the
//! machines row's gear opens) and `settings::agents` (the same agent-defaults
//! card for THIS install). Worktrees are not here: a machine's worktrees are
//! a local surface, Settings → Worktrees, which is also the only place that
//! cleans them.
//!
//! EXP-1063: the demo is drawn by the dialog's OWN parts, in the dialog's
//! order — the glass rows, the shared account picker over
//! `device_account_options` (no logins reported, so each agent contributes
//! its ambient row), `AgentDefaultsGroup` with real selects, the "Workflow
//! settings" `sub_shell_row` opening `device_settings::render_workflow_page`
//! inside a `SubShellHost`, then Update and the Remove row. The dialog view
//! itself is bound to a synced device row and the store, so the entry owns a
//! small demo view holding the same entities instead; nothing it draws is a
//! copy of a recipe.

use gpui::{
    div, px, App, AppContext as _, Context, Div, Entity, FocusHandle, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::InputState,
    v_flex, ActiveTheme as _, Icon,
};

use coding::CodingAgent;

use crate::coding_selects::{
    agent_icon, choice_select, effort_choices_for, model_choices_for, workflow_model_choices_for,
    AccountTrigger, ChoiceSelect, SUBAGENT_MODEL_CHOICES,
};
use crate::controls::{glass_input, WebControl as _};
use crate::device_settings::{choice_label, render_workflow_page, workflow_defaults_for};
use crate::icons::registry;
use crate::launch_options::{
    account_key, device_account_options, AgentDefaultsGroup, AgentPill, DefaultsToggle,
};
use crate::sub_shell::{
    focus_back_on_open, sub_shell_row, SubShellHost, SubShellNav, SubShellPage, SubShellProps,
};
use crate::surface;

pub(crate) const ID: &str = "device-settings";
pub(crate) const OWNER: &str = "EXP-1020";

const WORKFLOW_SETTINGS: &str = "Workflow settings";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let demo = window.use_keyed_state("sg-device-settings", cx, DeviceSettingsDemo::new);
    div().w(px(520.)).child(demo)
}

/// The dialog's state, minus the device row and the store behind it.
struct DeviceSettingsDemo {
    name_input: Entity<InputState>,
    is_default: bool,
    agent_tab: CodingAgent,
    default_agent: CodingAgent,
    model: ChoiceSelect,
    effort: ChoiceSelect,
    codex_model: ChoiceSelect,
    codex_effort: ChoiceSelect,
    subagent_model: ChoiceSelect,
    workflow_model: ChoiceSelect,
    workflow_strong_model: ChoiceSelect,
    ultracode: bool,
    plan_mode: bool,
    nav: SubShellNav,
    back_focus: FocusHandle,
}

impl DeviceSettingsDemo {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = coding::Settings::default();
        let name_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder("Machine name");
            state.set_value("Studio Mac", window, cx);
            state
        });
        let (workflow, workflow_strong) = workflow_defaults_for(settings.default_agent);
        let workflow_choices = workflow_model_choices_for(settings.default_agent);
        Self {
            name_input,
            is_default: true,
            agent_tab: settings.default_agent,
            default_agent: settings.default_agent,
            model: choice_select(
                model_choices_for(CodingAgent::Claude),
                &settings.claude_model,
                window,
                cx,
            ),
            effort: choice_select(
                effort_choices_for(CodingAgent::Claude),
                &settings.claude_effort,
                window,
                cx,
            ),
            codex_model: choice_select(
                model_choices_for(CodingAgent::Codex),
                &settings.codex_model,
                window,
                cx,
            ),
            codex_effort: choice_select(
                effort_choices_for(CodingAgent::Codex),
                &settings.codex_effort,
                window,
                cx,
            ),
            subagent_model: choice_select(
                &SUBAGENT_MODEL_CHOICES,
                &settings.claude_subagent_model,
                window,
                cx,
            ),
            workflow_model: choice_select(&workflow_choices, &workflow, window, cx),
            workflow_strong_model: choice_select(&workflow_choices, &workflow_strong, window, cx),
            ultracode: settings.claude_ultracode,
            plan_mode: settings.claude_plan_mode,
            nav: SubShellNav::new(),
            back_focus: cx.focus_handle(),
        }
    }

    fn account_row(&self, cx: &mut Context<Self>) -> Div {
        let settings = coding::Settings {
            default_agent: self.default_agent,
            ..coding::Settings::default()
        };
        let options = device_account_options(
            &Default::default(),
            &Default::default(),
            &settings,
            &CodingAgent::ALL,
        );
        let current = account_key(self.default_agent, None);
        let view = cx.entity().downgrade();
        surface::glass_group_rows(vec![surface::glass_picker_row(
            "Default account",
            None,
            crate::coding_selects::account_picker(
                "sg-device-default-account",
                &options,
                Some(current.as_str()),
                AccountTrigger::Row,
                move |option, _, cx| {
                    let agent = option.agent;
                    view.update(cx, |this, cx| {
                        this.default_agent = agent;
                        cx.notify();
                    })
                    .ok();
                },
                cx,
            ),
            cx,
        )])
    }

    fn defaults_group(&self, cx: &mut Context<Self>) -> Div {
        let tabs = CodingAgent::ALL.to_vec();
        let active = tabs.iter().position(|agent| *agent == self.agent_tab);
        let pills = tabs
            .iter()
            .map(|agent| AgentPill {
                label: SharedString::from(agent.label()),
                icon: Some(agent_icon(*agent)),
                dimmed: false,
                note: None,
            })
            .collect();
        let (model, effort) = match self.agent_tab {
            CodingAgent::Claude => (self.model.clone(), self.effort.clone()),
            CodingAgent::Codex => (self.codex_model.clone(), self.codex_effort.clone()),
        };
        let (workflow, workflow_strong) = workflow_defaults_for(self.default_agent);
        let choices = model_choices_for(self.default_agent);
        let summary = SharedString::from(format!(
            "{} · {}",
            choice_label(choices, &workflow),
            choice_label(choices, &workflow_strong),
        ));
        let mut group = AgentDefaultsGroup::new(
            "sg-device-defaults",
            self.agent_tab,
            pills,
            active,
            move |this: &mut Self, ix, _, cx| {
                if let Some(agent) = tabs.get(ix).copied() {
                    this.agent_tab = agent;
                    cx.notify();
                }
            },
            model,
            effort,
        )
        .effort_disabled(self.agent_tab == CodingAgent::Claude && self.ultracode)
        .trailing(vec![sub_shell_row(
            SubShellProps::new("sg-device-workflow-settings", WORKFLOW_SETTINGS)
                .icon(Icon::new(registry::NAV_WORKFLOWS))
                .value(summary),
            cx.listener(|this: &mut Self, _, window, cx| {
                this.nav.open(WORKFLOW_SETTINGS);
                focus_back_on_open(&this.back_focus, window, cx);
                cx.notify();
            }),
            cx,
        )]);
        if self.agent_tab.supports_subagent_model() {
            group = group.subagent(self.subagent_model.clone());
        }
        if self.agent_tab == CodingAgent::Claude {
            group = group
                .toggle(DefaultsToggle::new(
                    "sg-device-ultracode",
                    "Ultracode",
                    self.ultracode,
                    |this: &mut Self, on, _| this.ultracode = on,
                ))
                .toggle(DefaultsToggle::new(
                    "sg-device-plan",
                    "Plan mode",
                    self.plan_mode,
                    |this: &mut Self, on, _| this.plan_mode = on,
                ));
        }
        group.render(cx)
    }

    fn update_section(&self, cx: &mut Context<Self>) -> Div {
        let muted = cx.theme().muted_foreground;
        v_flex()
            .w_full()
            .gap_2()
            .child(surface::glass_section_header("Update", None, cx))
            .child(surface::glass_group_rows(vec![surface::glass_row_shell()
                .min_w_0()
                .gap_2()
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_0p5()
                        .child(div().text_sm().child("Exponential v0.18.75"))
                        .child(div().text_xs().text_color(muted).child("Up to date")),
                )]))
    }

    fn remove_row(&self) -> Div {
        surface::glass_group_rows(vec![surface::glass_row_shell().min_w_0().gap_2().child(
            Button::new("sg-device-remove")
                .danger()
                .web_sm()
                .icon(Icon::new(registry::UI_DELETE))
                .label("Remove device"),
        )])
    }
}

impl Render for DeviceSettingsDemo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let identity_row = surface::glass_row_shell()
            .gap_2()
            .child(
                h_flex()
                    .flex_shrink_0()
                    .size(px(crate::controls::CTL_MD_H))
                    .justify_center()
                    .child(
                        Icon::new(crate::icons::device_icon(None, false)).text_color(muted),
                    ),
            )
            .child(
                div().flex_1().min_w_0().child(
                    surface::glass_row_input(glass_input(&self.name_input, window, cx))
                        .text_left(),
                ),
            );
        let default_row = surface::glass_toggle_row(
            "Default device",
            None,
            crate::controls::web_switch("sg-device-default")
                .checked(self.is_default)
                .on_click(cx.listener(|this: &mut Self, on: &bool, _, cx| {
                    this.is_default = *on;
                    cx.notify();
                }))
                .into_any_element(),
            cx,
        );
        let body = v_flex()
            .w_full()
            .gap_2()
            .child(surface::glass_group_rows(vec![identity_row]))
            .child(surface::glass_group_rows(vec![default_row]))
            .child(self.account_row(cx))
            .child(self.defaults_group(cx))
            .child(self.update_section(cx))
            .child(self.remove_row());
        let mut host = SubShellHost::new(body);
        if self.nav.is_open() {
            let page = render_workflow_page(&self.workflow_model, &self.workflow_strong_model, cx);
            host = host.open(
                SubShellPage::new(WORKFLOW_SETTINGS, page),
                &self.back_focus,
                cx.listener(|this: &mut Self, _, _, cx| {
                    this.nav.back();
                    cx.notify();
                }),
            );
        }
        div().w_full().child(host.render(window, cx))
    }
}
