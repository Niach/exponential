//! EXP-772 — the Chat page (`Screen::Chat`), the desktop twin of the web
//! `t/$teamSlug/chat` route.
//!
//! An essentially empty page in the "Ask Linear" shape: one wide rounded
//! prompt box, vertically centred, with a single subtle row of small inline
//! pickers under it — machine, agent, plan. No cards, no headings, no
//! sections.
//!
//! EXP-790: the box is the mention field (`@` members, `#` issue refs, `:`
//! emoji — the comment composer's widget), model and effort stay the
//! machine's defaults (they left the row with the session composer's
//! pickers), and three suggestion chips sit over the EMPTY field, inserting a
//! `#` so the issue picker opens. The chip strings are byte-identical to the
//! web page's `CHAT_SUGGESTIONS` (locked below).
//!
//! The rail's Agent entry opens this page, and the run starts with the first
//! message. The options are the ONE launch model every desktop surface uses
//! ([`coding::LaunchOptions`], seeded by
//! [`coding::LaunchOptions::defaults_for`]) — with **plan mode OFF** by
//! default, whatever the agent's setting says: a chat is a conversation, not a
//! planning run.
//!
//! The machine is a LABEL, not a picker: a desktop chat runs on the machine
//! you are sitting at. Remote chats start from web/mobile.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, AnyElement, App, AppContext as _, ClickEvent, Entity, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{InputEvent, TextareaState};
use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_component::switch::Switch;
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use coding::CodingAgent;

use crate::icons::registry;
use crate::launch_options::{agent_label, pickable_agents};
use crate::mention_input::MentionInput;
use crate::navigation::{self, Navigation};
use crate::surface::{glass_pill, PillMode, PillSize};

/// The page's one field: wide, rounded, Enter sends and Shift+Enter breaks a
/// line — the steer composer's rhythm, on a page with nothing else on it.
const PROMPT_MAX_W: f32 = 640.;

/// EXP-790: the chips over an empty prompt. Each ends in `#` so the issue
/// picker opens the moment it lands — the desktop twin of the web page's
/// `CHAT_SUGGESTIONS` (`routes/t/$teamSlug/chat.tsx`), byte-identical.
pub(crate) const CHAT_SUGGESTIONS: [&str; 3] = ["Fix #", "Explain #", "Review #"];

pub(crate) struct ChatScreenView {
    nav: Entity<Navigation>,
    input: Entity<TextareaState>,
    /// EXP-790: the completion overlay (`@` / `#` / `:`) over `input`; the
    /// composer card draws the chrome, so the widget draws none of its own.
    mention: Entity<MentionInput>,
    /// The team the completion source was last pointed at.
    mention_team: Option<String>,
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
        let mention = cx.new(|cx| {
            let mut mention = MentionInput::new(input.clone(), cx);
            mention.set_appearance(false);
            mention
        });
        let mut subscriptions = vec![cx.subscribe_in(
            &input,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { shift: false, .. } => this.send(window, cx),
                // The suggestion chips are a function of the draft being empty.
                InputEvent::Change => cx.notify(),
                _ => {}
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
            mention,
            mention_team: None,
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

    /// EXP-790: point the completion at the active team — the same `#`-issue
    /// / `@`-member source the comment composer uses. No team = plain input.
    fn sync_mention_source(&mut self, cx: &mut gpui::Context<Self>) {
        let team_id = navigation::active_team_id(&self.nav, cx);
        if team_id == self.mention_team {
            return;
        }
        self.mention_team = team_id.clone();
        self.mention.update(cx, |mention, _| {
            mention.set_source(team_id.map(crate::markdown::store_completion_source));
        });
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

    /// EXP-790: machine → agent → plan. Model and effort are the machine's
    /// defaults for the picked agent and never shown here.
    fn render_options_row(&mut self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let machine = self.machine_label(cx);
        let agent = self.agent;
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

    /// EXP-790: the suggestion chips, shown over the EMPTY field only. A click
    /// inserts the text through the mention widget so its trailing `#` opens
    /// the issue picker, exactly as typing it would.
    fn render_suggestions(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        if !self.input.read(cx).value().trim().is_empty() {
            return None;
        }
        let chips = CHAT_SUGGESTIONS.iter().enumerate().map(|(index, suggestion)| {
            let text: &'static str = suggestion;
            glass_pill(("chat-suggestion", index), PillSize::Sm, PillMode::Action, cx)
                .cursor_pointer()
                .child(div().text_xs().child(text))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.mention.update(cx, |mention, cx| mention.insert_text(text, window, cx));
                    cx.notify();
                }))
        });
        Some(
            h_flex()
                .w_full()
                .min_w_0()
                .flex_wrap()
                .gap_1()
                .px_1()
                .children(chips)
                .into_any_element(),
        )
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
        mcp_server_ids: Vec::new(),
        account: None,
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
        self.sync_mention_source(cx);
        let can_send = self.agent.is_some();
        let options = self.render_options_row(cx);
        let suggestions = self.render_suggestions(cx);
        let composer = crate::composer::GlassComposer::new(
            div()
                .w_full()
                .min_w_0()
                .child(self.mention.clone())
                .into_any_element(),
        )
        .submit(
            // EXP-790: one circled arrow on every composer (`ui-submit`).
            crate::composer::composer_submit("chat-send", registry::UI_SUBMIT, !can_send, cx)
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
                    .children(suggestions)
                    .child(crate::composer::glass_composer(composer))
                    .child(options),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-790: the chips are the web page's `CHAT_SUGGESTIONS`, byte for
    /// byte, and each ends in the `#` that opens the issue picker.
    #[test]
    fn chat_suggestions_mirror_the_web_page() {
        assert_eq!(CHAT_SUGGESTIONS, ["Fix #", "Explain #", "Review #"]);
        for suggestion in CHAT_SUGGESTIONS {
            assert!(suggestion.ends_with('#'), "{suggestion:?} must open the issue picker");
        }
    }

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
