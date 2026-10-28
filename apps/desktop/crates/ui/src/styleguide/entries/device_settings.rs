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
//! order — the glass rows, the readiness block (EXP-1236: its Computer use
//! band with the switch on and the Computer use model picker under it, NEW-59
//! the Work screen picker under that, off
//! the contract fixture's "computer use on" case), `AgentDefaultsGroup` with
//! real selects (Model, then Thread model, then claude's Subagent model —
//! every model row the ONE `coding_selects::model_select` over all the
//! agent's models, no default row), then Update and the Remove row. The dialog view
//! itself is bound to a synced device row and the store, so the entry owns a
//! small demo view holding the same entities instead; nothing it draws is a
//! copy of a recipe.

use std::rc::Rc;

use gpui::{
    div, px, App, AppContext as _, Context, Div, Entity, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::InputState,
    v_flex, ActiveTheme as _, Icon,
};

use coding::device_doctor::{DeviceDoctor, DoctorState, KEY_COMPUTER_USE};
use coding::CodingAgent;

use crate::coding_selects::{agent_icon, choice_select, effort_choices_for, model_select, ChoiceSelect};
use crate::controls::{glass_input, WebControl as _};
use crate::device_readiness;
use crate::icons::registry;
use crate::launch_options::{AgentDefaultsGroup, AgentPill, DefaultsToggle};
use crate::surface;

pub(crate) const ID: &str = "device-settings";
pub(crate) const OWNER: &str = "EXP-1020";

/// The ONE contract fixture the readiness block replays (`device-doctor.json`).
const DOCTOR_FIXTURE: &str =
    include_str!("../../../../../../../packages/domain-contract/fixtures/device-doctor.json");

/// EXP-1236: the fixture case whose Computer use switch is ON — the one that
/// shows the model picker under it. `None` only if the fixture has no such
/// case (the block is then left out, never faked).
pub(super) fn computer_use_on_doctor() -> Option<DeviceDoctor> {
    let fixture: serde_json::Value = serde_json::from_str(DOCTOR_FIXTURE).ok()?;
    fixture["cases"]
        .as_array()?
        .iter()
        .filter_map(|case| device_readiness::parse(Some(&case["doctor"])))
        .find(|doctor| {
            doctor
                .items
                .iter()
                .any(|item| item.key == KEY_COMPUTER_USE && item.state == DoctorState::Ok)
        })
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let demo = window.use_keyed_state("sg-device-settings", cx, DeviceSettingsDemo::new);
    div().w(px(520.)).child(demo)
}

/// The dialog's state, minus the device row and the store behind it.
struct DeviceSettingsDemo {
    name_input: Entity<InputState>,
    is_default: bool,
    agent_tab: CodingAgent,
    model: ChoiceSelect,
    effort: ChoiceSelect,
    codex_model: ChoiceSelect,
    codex_effort: ChoiceSelect,
    thread_model: ChoiceSelect,
    codex_thread_model: ChoiceSelect,
    subagent_model: ChoiceSelect,
    ultracode: bool,
    plan_mode: bool,
    /// EXP-1236: the readiness block's Computer use switch + the model
    /// picker it reveals.
    doctor: Option<DeviceDoctor>,
    computer_use: bool,
    computer_use_model: ChoiceSelect,
    /// NEW-59: the Work screen row under the model row (the `work-screen`
    /// entry's demo displays, Auto picked).
    work_screen: ChoiceSelect,
}

impl DeviceSettingsDemo {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = coding::Settings::default();
        let name_input = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder("Machine name");
            state.set_value("Studio Mac", window, cx);
            state
        });
        Self {
            name_input,
            is_default: true,
            agent_tab: settings.default_agent,
            model: model_select(CodingAgent::Claude, &settings.claude_model, window, cx),
            thread_model: model_select(CodingAgent::Claude, &settings.claude_thread_model, window, cx),
            effort: choice_select(
                effort_choices_for(CodingAgent::Claude),
                &settings.claude_effort,
                window,
                cx,
            ),
            codex_model: model_select(CodingAgent::Codex, &settings.codex_model, window, cx),
            codex_thread_model: model_select(
                CodingAgent::Codex,
                &settings.codex_thread_model,
                window,
                cx,
            ),
            codex_effort: choice_select(
                effort_choices_for(CodingAgent::Codex),
                &settings.codex_effort,
                window,
                cx,
            ),
            subagent_model: model_select(
                CodingAgent::Claude,
                &settings.claude_subagent_model,
                window,
                cx,
            ),
            ultracode: settings.claude_ultracode,
            plan_mode: settings.claude_plan_mode,
            doctor: computer_use_on_doctor(),
            computer_use: true,
            computer_use_model: model_select(
                CodingAgent::Claude,
                &settings.computer_use_model,
                window,
                cx,
            ),
            work_screen: crate::coding_selects::work_screen_select(
                &super::work_screen::demo_displays(),
                settings.computer_use_display_id.as_deref(),
                window,
                cx,
            ),
        }
    }

    /// The dialog's readiness block: the fixture's "computer use on" case,
    /// the switch live, the Computer use model row and (NEW-59) the Work
    /// screen row under it as the band's last rows while it is on (the
    /// dialog's own `device_settings::computer_use_rows`).
    fn readiness_block(&self, cx: &mut Context<Self>) -> Option<Div> {
        let doctor = self.doctor.as_ref()?;
        let view = cx.entity().downgrade();
        let props = device_readiness::BlockProps {
            id: "sg-device-settings-readiness".into(),
            on_toggle: Some(Rc::new(move |on, _window, cx| {
                let _ = view.update(cx, |this, cx| {
                    this.computer_use = on;
                    cx.notify();
                });
            })),
            computer_use_tail: if self.computer_use {
                crate::device_settings::computer_use_rows(&self.computer_use_model, &self.work_screen, cx)
            } else {
                Vec::new()
            },
            ..device_readiness::BlockProps::default()
        };
        Some(device_readiness::render_sections(
            &device_readiness::sections(doctor, false, Some(self.computer_use)),
            props,
            cx,
        ))
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
        let (model, effort, thread) = match self.agent_tab {
            CodingAgent::Claude => (self.model.clone(), self.effort.clone(), self.thread_model.clone()),
            CodingAgent::Codex => (
                self.codex_model.clone(),
                self.codex_effort.clone(),
                self.codex_thread_model.clone(),
            ),
        };
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
        .thread(thread);
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
        let readiness = self.readiness_block(cx);
        let body = v_flex()
            .w_full()
            .gap_2()
            .child(surface::glass_group_rows(vec![identity_row]))
            .child(surface::glass_group_rows(vec![default_row]))
            .children(readiness)
            .child(self.defaults_group(cx))
            .child(self.update_section(cx))
            .child(self.remove_row());
        div().w_full().child(body)
    }
}
