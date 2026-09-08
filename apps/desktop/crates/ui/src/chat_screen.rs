//! EXP-772 — the Chat page (`Screen::Chat`), the desktop twin of the web
//! `t/$teamSlug/chat` route.
//!
//! An essentially empty page in the "Ask Linear" shape: one wide rounded
//! prompt box, vertically centred, with a single subtle row of small inline
//! pickers under it — machine, agent, model, effort, plan. No cards, no
//! headings, no sections.
//!
//! The session bar's Chat button used to launch a promptless run on the spot;
//! it opens this page instead, and the run starts with the first message. The
//! options are the ONE launch model every desktop surface uses
//! ([`coding::LaunchOptions`], seeded by
//! [`coding::LaunchOptions::defaults_for`] and named from the
//! [`crate::coding_selects`] vocabularies) — with **plan mode OFF** by
//! default, whatever the agent's setting says: a chat is a conversation, not a
//! planning run.
//!
//! The machine is a LABEL, not a picker: a desktop chat runs on the machine
//! you are sitting at. Remote chats start from web/mobile.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, AnyElement, App, AppContext as _, ClickEvent, Entity, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString, Styled, Subscription,
    Window,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{InputEvent, Textarea, TextareaState};
use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_component::switch::Switch;
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use coding::CodingAgent;

use crate::coding_selects::{effort_choices_for, model_choices_for};
use crate::icons::registry;
use crate::launch_options::{agent_label, pickable_agents, CLI_DEFAULT_LABEL};
use crate::navigation::{self, Navigation};

/// The page's one field: wide, rounded, Enter sends and Shift+Enter breaks a
/// line — the steer composer's rhythm, on a page with nothing else on it.
const PROMPT_MAX_W: f32 = 640.;

pub(crate) struct ChatScreenView {
    nav: Entity<Navigation>,
    input: Entity<TextareaState>,
    /// The agent the run starts on. `None` while the doctor found nothing
    /// runnable — the page still renders, and sending says so.
    agent: Option<CodingAgent>,
    /// `LaunchOptions.model` / `.effort` — blank is the CLI's own default,
    /// which is a CHOICE and not a missing answer.
    model: String,
    effort: String,
    /// EXP-772: OFF by default for a chat, whatever the agent's setting says.
    plan: bool,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl ChatScreenView {
    pub(crate) fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = navigation::nav_for_window(window, cx);
        let input = cx.new(|cx| {
            crate::controls::web_textarea(2, 8, window, cx)
                .submit_on_enter(true)
                .placeholder("Ask your agent anything…")
        });
        let mut subscriptions = vec![cx.subscribe_in(
            &input,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { shift: false, .. }) {
                    this.send(window, cx);
                }
            },
        )];
        subscriptions.push(cx.observe(&nav, |_, _, cx| cx.notify()));
        // The doctor report lands after the window does — re-seed when it
        // does, or the page would sit on "no agent" for the first seconds.
        if let Some(hub) = crate::coding_flow::CodingHub::global_ref(cx) {
            subscriptions.push(cx.observe(&hub, |this: &mut Self, _, cx| {
                this.reconcile_agent(cx);
                cx.notify();
            }));
        }
        let mut this = Self {
            nav,
            input,
            agent: None,
            model: String::new(),
            effort: String::new(),
            plan: false,
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        };
        this.reconcile_agent(cx);
        this
    }

    /// The agents this machine can run — the same doctor-filtered list every
    /// launch picker offers.
    fn agents(cx: &App) -> Vec<CodingAgent> {
        let Some(hub) = crate::coding_flow::CodingHub::global_ref(cx) else {
            return Vec::new();
        };
        pickable_agents(hub.read(cx).doctor.report.as_ref())
    }

    /// This machine's launch defaults — the hub's settings, or the plain
    /// defaults while it has not been built yet (the page can render before
    /// the coding hub exists).
    fn settings(cx: &App) -> coding::Settings {
        crate::coding_flow::CodingHub::global_ref(cx)
            .map(|hub| hub.read(cx).settings.clone())
            .unwrap_or_default()
    }

    /// Keep the pick on a runnable agent, and re-seed model/effort from that
    /// agent's own defaults. Plan stays where the user left it (OFF to start).
    fn reconcile_agent(&mut self, cx: &App) {
        let agents = Self::agents(cx);
        let settings = Self::settings(cx);
        let next = match self.agent {
            Some(agent) if agents.contains(&agent) => return,
            _ => agents
                .contains(&settings.default_agent)
                .then_some(settings.default_agent)
                .or_else(|| agents.first().copied()),
        };
        let Some(agent) = next else {
            self.agent = None;
            return;
        };
        self.set_agent(agent, cx);
    }

    fn set_agent(&mut self, agent: CodingAgent, cx: &App) {
        let (model, effort, plan) = chat_seed(&Self::settings(cx), agent);
        self.agent = Some(agent);
        self.model = model;
        self.effort = effort;
        self.plan = plan;
    }

    /// The options the run launches with.
    fn options(&self, agent: CodingAgent) -> coding::LaunchOptions {
        chat_options(agent, &self.model, &self.effort, self.plan)
    }

    /// Start the chat run with the typed prompt. The launcher navigates to
    /// the run's session screen itself once the agent is up
    /// (`coding_flow`'s `open_session`), so this only hands the work over.
    ///
    /// The draft is cleared on the way out, which is also the double-send
    /// guard: an empty draft never launches.
    fn send(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(agent) = self.agent else {
            return;
        };
        let prompt = self.input.read(cx).value().to_string();
        if prompt.trim().is_empty() {
            return;
        }
        let Some(host) = crate::session_bar::host_for_window(window, cx) else {
            return;
        };
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
        let options = self.options(agent);
        host.update(cx, |host, cx| {
            host.launch_chat_run(options, None, Some(prompt), window, cx);
        });
        cx.notify();
    }

    // ── The picker row ────────────────────────────────────────────────────

    /// One inline picker: muted label, the value at 70%, a caret. Deliberately
    /// chrome-less — this row must read as a caption under the field, not as a
    /// toolbar.
    fn choice_picker(
        &self,
        key: &'static str,
        choices: &'static [(&'static str, &'static str)],
        picked: &str,
        write: fn(&mut Self) -> &mut String,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let label = choices
            .iter()
            .find(|(_, value)| *value == picked)
            .map(|(label, _)| (*label).to_string())
            .unwrap_or_else(|| CLI_DEFAULT_LABEL.to_string());
        let current = picked.to_string();
        let view = cx.entity().downgrade();
        Button::new(SharedString::from(format!("chat-pick-{key}")))
            .ghost()
            .cursor_pointer()
            .h_auto()
            .px_1()
            .py_0()
            .text_color(cx.theme().muted_foreground)
            .dropdown_caret(true)
            .child(div().text_xs().child(SharedString::from(label)))
            .dropdown_menu(move |mut menu, _window, _cx| {
                for (label, value) in choices {
                    let view = view.clone();
                    let value = (*value).to_string();
                    let checked = current == value;
                    menu = menu.item(PopupMenuItem::new(*label).checked(checked).on_click(
                        move |_, _, cx| {
                            if let Some(view) = view.upgrade() {
                                let value = value.clone();
                                view.update(cx, |view, cx| {
                                    *write(view) = value;
                                    cx.notify();
                                });
                            }
                        },
                    ));
                }
                menu
            })
            .into_any_element()
    }

    fn agent_picker(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let agents = Self::agents(cx);
        let label = match self.agent {
            Some(agent) => agent.label().to_string(),
            None => crate::coding_flow::NO_AGENT_COPY.to_string(),
        };
        let current = self.agent;
        let view = cx.entity().downgrade();
        Button::new("chat-pick-agent")
            .ghost()
            .cursor_pointer()
            .h_auto()
            .px_1()
            .py_0()
            .text_color(cx.theme().muted_foreground)
            .dropdown_caret(true)
            .child(div().text_xs().child(SharedString::from(label)))
            .dropdown_menu(move |mut menu, _window, _cx| {
                for agent in &agents {
                    let view = view.clone();
                    let agent = *agent;
                    menu = menu.item(
                        PopupMenuItem::new(agent_label(agent.id()))
                            .checked(current == Some(agent))
                            .on_click(move |_, _, cx| {
                                if let Some(view) = view.upgrade() {
                                    view.update(cx, |view, cx| {
                                        view.set_agent(agent, cx);
                                        cx.notify();
                                    });
                                }
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }

    /// The machine: a LABEL on the desktop — a chat here runs here.
    fn machine_label(&self, cx: &mut App) -> SharedString {
        let own = navigation::active_team_id(&self.nav, cx)
            .is_some()
            .then(|| crate::queries::launch_devices(cx))
            .unwrap_or_default();
        own.into_iter()
            .find(|device| device.is_own)
            .map(|device| SharedString::from(device.label))
            .unwrap_or_else(|| SharedString::from("This device"))
    }

    fn render_options_row(&mut self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let machine = self.machine_label(cx);
        let agent = self.agent;
        let picked_model = self.model.clone();
        let picked_effort = self.effort.clone();
        let model = self.choice_picker(
            "model",
            agent.map(model_choices_for).unwrap_or(&[]),
            &picked_model,
            |this| &mut this.model,
            cx,
        );
        let effort = self.choice_picker(
            "effort",
            agent.map(effort_choices_for).unwrap_or(&[]),
            &picked_effort,
            |this| &mut this.effort,
            cx,
        );
        let plan_supported = agent.is_some_and(CodingAgent::supports_plan_mode);
        h_flex()
            .w_full()
            .min_w_0()
            .flex_wrap()
            .gap_1()
            .items_center()
            .px_1()
            .text_xs()
            .text_color(muted)
            .child(div().px_1().child(machine))
            .child(self.agent_picker(cx))
            .children(agent.map(|_| model))
            .children(agent.map(|_| effort))
            .when(plan_supported, |this| {
                this.child(
                    h_flex()
                        .gap_1p5()
                        .items_center()
                        .px_1()
                        .child("Plan")
                        .child(
                            Switch::new("chat-plan")
                                .checked(self.plan)
                                .on_click(cx.listener(|this, on: &bool, _, cx| {
                                    this.plan = *on;
                                    cx.notify();
                                })),
                        ),
                )
            })
            .into_any_element()
    }
}

/// EXP-772 — the chat page's picks as launch options. Pure, so the two rules
/// that are easy to get wrong are testable without a window:
///
/// - **ultracode is never on**: it is a coding posture, not a conversational
///   one, and the page does not offer it;
/// - **plan mode is capability-clamped**, so a switch left on while the agent
///   changes to one without a plan mode cannot leak into the argv.
pub(crate) fn chat_options(
    agent: CodingAgent,
    model: &str,
    effort: &str,
    plan: bool,
) -> coding::LaunchOptions {
    coding::LaunchOptions {
        agent,
        model: model.to_string(),
        effort: effort.to_string(),
        ultracode: false,
        plan_mode: plan && agent.supports_plan_mode(),
        external: None,
    }
}

/// EXP-772 — what the pickers start on for `agent`: that agent's own model and
/// effort defaults, and plan mode **OFF** whatever the setting says. A chat is
/// a conversation; parking it in plan mode is the surprise this avoids.
pub(crate) fn chat_seed(settings: &coding::Settings, agent: CodingAgent) -> (String, String, bool) {
    let defaults = coding::LaunchOptions::defaults_for(settings, agent);
    (defaults.model, defaults.effort, false)
}

impl Focusable for ChatScreenView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ChatScreenView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let can_send = self.agent.is_some();
        let options = self.render_options_row(cx);
        let composer = crate::composer::GlassComposer::new(
            div()
                .w_full()
                .min_w_0()
                .child(Textarea::new(&self.input).w_full().appearance(false))
                .into_any_element(),
        )
        .submit(
            crate::composer::composer_submit("chat-send", registry::UI_SEND, !can_send, cx)
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.send(window, cx);
                })),
        );
        // Vertically centred, one column, nothing else on the page.
        v_flex()
            .size_full()
            .min_h_0()
            .items_center()
            .justify_center()
            .p_6()
            .track_focus(&self.focus_handle)
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(PROMPT_MAX_W))
                    .min_w_0()
                    .gap_2()
                    .child(crate::composer::glass_composer(composer))
                    .child(options),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The chat page seeds off the AGENT's own defaults — and never off its
    /// plan-mode setting: a chat starts in build mode, always.
    #[test]
    fn a_chat_seeds_from_the_agent_defaults_with_plan_off() {
        let mut settings = coding::Settings::default();
        settings.claude_plan_mode = true;
        settings.claude_model = "opus".to_string();
        settings.codex_model = "gpt-5.6-terra".to_string();
        settings.codex_effort = "xhigh".to_string();

        let (model, effort, plan) = chat_seed(&settings, CodingAgent::Claude);
        assert_eq!(model, "opus");
        assert!(!plan, "a chat never starts in plan mode");
        let _ = effort;

        // Switching the agent re-seeds from THAT agent's defaults.
        let (model, effort, plan) = chat_seed(&settings, CodingAgent::Codex);
        assert_eq!((model.as_str(), effort.as_str()), ("gpt-5.6-terra", "xhigh"));
        assert!(!plan);
    }

    /// Ultracode is never on, and plan mode is clamped to what the agent can
    /// actually do.
    #[test]
    fn chat_options_never_ultracode_and_clamp_plan_mode() {
        let claude = chat_options(CodingAgent::Claude, "opus", "high", true);
        assert!(!claude.ultracode);
        assert!(claude.plan_mode);
        assert_eq!((claude.model.as_str(), claude.effort.as_str()), ("opus", "high"));
        assert!(claude.external.is_none());

        // Codex has no plan mode — a switch left on cannot reach the argv.
        assert!(!CodingAgent::Codex.supports_plan_mode());
        assert!(!chat_options(CodingAgent::Codex, "", "", true).plan_mode);

        // Off is off.
        assert!(!chat_options(CodingAgent::Claude, "opus", "", false).plan_mode);
    }
}
