//! EXP-772 — the Chat page (`Screen::Chat`), the desktop twin of the web
//! `t/$teamSlug/agent` route (`routes/t/$teamSlug/agent.tsx`).
//!
//! An essentially empty page in the "Ask Linear" shape: one wide rounded
//! prompt box, vertically centred, with a single subtle row of small inline
//! pickers under it — machine, agent, plan. No cards, no headings, no
//! sections.
//!
//! EXP-790: the box is the mention field (`@` members, `#` issue refs, `:`
//! emoji — the comment composer's widget), model and effort stay the
//! machine's defaults (they left the row with the session composer's
//! pickers), and suggestion chips sit over the EMPTY field — EXP-820: four
//! drawn once per page from a pool of sixteen; one ending in `#` opens the
//! issue picker. The pool is byte-identical to the web page's
//! `CHAT_SUGGESTIONS` (`lib/chat-suggestions.ts`, locked below).
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
use crate::launch_options::{
    agent_label, mcp_pick_summary, mcp_pick_popover, pickable_agents, McpServerOption,
};
use crate::mention_input::MentionInput;
use crate::navigation::{self, Navigation};
use crate::surface::{glass_pill, PillMode, PillSize};

/// The page's one field: wide, rounded, Enter sends and Shift+Enter breaks a
/// line — the steer composer's rhythm, on a page with nothing else on it.
const PROMPT_MAX_W: f32 = 640.;

/// EXP-790/EXP-820: the suggestion POOL over an empty prompt — the desktop
/// twin of the web page's `CHAT_SUGGESTIONS` (`lib/chat-suggestions.ts`,
/// rendered by `routes/t/$teamSlug/agent.tsx`), byte-identical and in the
/// same order. A suggestion ending in `#` opens the issue picker the moment it
/// lands; the others are plain text. The page shows [`CHAT_SUGGESTION_COUNT`]
/// of them, picked once per page ([`pick_chat_suggestions`]).
pub(crate) const CHAT_SUGGESTIONS: [&str; 16] = [
    "Fix #",
    "Explain #",
    "Review #",
    "Split # into sub-issues",
    "Label every issue in the backlog",
    "Set a priority on every unprioritized issue",
    "Find duplicate issues and link them",
    "Do a code review of the open PRs and file the findings on a new board",
    "Create an automation that labels new issues",
    "Set up a weekly standup digest automation",
    "Draft release notes from the issues completed this month",
    "Summarize what changed across the boards this week",
    "Start a session for # on my other machine",
    "Move stale in-progress issues back to the backlog",
    "Comment a plan on #",
    "Which issues are blocked, and by what?",
];

/// How many of the pool a page shows.
pub(crate) const CHAT_SUGGESTION_COUNT: usize = 4;

/// EXP-820: `CHAT_SUGGESTION_COUNT` DISTINCT indices into
/// [`CHAT_SUGGESTIONS`] — a partial Fisher-Yates over the pool driven by a
/// tiny xorshift on `seed`, so the page needs no `rand` dependency. Pure, so
/// the distinctness is testable; the caller seeds it once per page (the chips
/// must not reshuffle on every frame).
pub(crate) fn pick_chat_suggestions(seed: u64) -> Vec<usize> {
    let mut state = seed | 1; // xorshift needs a non-zero state
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut indices: Vec<usize> = (0..CHAT_SUGGESTIONS.len()).collect();
    let count = CHAT_SUGGESTION_COUNT.min(indices.len());
    for at in 0..count {
        let swap = at + (next() as usize) % (indices.len() - at);
        indices.swap(at, swap);
    }
    indices.truncate(count);
    indices
}

/// The seed a page picks its chips with: the clock's nanoseconds.
fn suggestion_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15)
}

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
    /// EXP-792: the team's MCP servers, resolved against THIS machine (a
    /// desktop chat runs here — there is no device pick to re-resolve
    /// against). Empty hides the pill, like the web's `mcp.servers.length`
    /// guard on the same row.
    mcp_servers: Vec<McpServerOption>,
    /// The picked server ids, seeded from `enabled_by_default`.
    mcp_selected: Vec<String>,
    /// The team the server list belongs to — the page outlives a team
    /// switch, so a switch has to refetch and re-seed.
    mcp_team: Option<String>,
    /// EXP-820: the chips this page shows — indices into
    /// [`CHAT_SUGGESTIONS`], drawn once when the page was built.
    suggestions: Vec<usize>,
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
            mcp_servers: Vec::new(),
            mcp_selected: Vec::new(),
            mcp_team: None,
            suggestions: pick_chat_suggestions(suggestion_seed()),
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
        chat_options(
            agent,
            &self.model,
            &self.effort,
            self.plan,
            self.mcp_selected.clone(),
        )
    }

    /// EXP-792: the team's MCP servers for the chat pill. One fetch per
    /// team (`mcpServers.list` is server-only), with THIS machine's
    /// readiness read from the local store alongside it — the page targets
    /// this machine and nothing else, so there is no matrix to consult.
    fn ensure_mcp_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        let team_id = navigation::active_team_id(&self.nav, cx);
        if team_id == self.mcp_team {
            return;
        }
        self.mcp_team = team_id.clone();
        self.mcp_servers = Vec::new();
        self.mcp_selected = Vec::new();
        let (Some(team), Some(trpc), Some(account)) = (
            team_id,
            crate::queries::trpc_client(cx),
            crate::queries::active_account(cx),
        ) else {
            return;
        };
        let data_dir = crate::session::AuthContext::global(cx).data_dir.clone();
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async move {
                    let servers = api::mcp_servers::list(&trpc, &team)
                        .inspect_err(|err| log::debug!("[ui] mcpServers.list for chat: {err}"))
                        .ok()?;
                    let configs: Vec<api::mcp_servers::McpServerConfig> =
                        servers.iter().map(|entry| entry.config.clone()).collect();
                    let local = coding::mcp_servers::readiness(
                        &data_dir,
                        &account.id,
                        &configs,
                        crate::settings::mcp_servers::now_secs(),
                    );
                    Some((team, servers, local))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let Some((team, servers, local)) = loaded else {
                    return;
                };
                // A team switch under the fetch wins — never seed the pill
                // from the team the person just left.
                if this.mcp_team.as_deref() != Some(team.as_str()) {
                    return;
                }
                let now = chrono::Utc::now();
                this.mcp_servers = servers
                    .iter()
                    .map(|entry| McpServerOption {
                        id: entry.config.id.clone(),
                        name: entry.config.name.clone(),
                        blocked: crate::launch_options::mcp_block_reason(
                            &entry.config.auth,
                            local
                                .iter()
                                .find(|row| row.server_id == entry.config.id)
                                .map(crate::settings::mcp_servers::Readiness::from),
                            None,
                            now,
                        ),
                        enabled_by_default: entry.config.enabled_by_default,
                    })
                    .collect();
                this.mcp_selected =
                    crate::launch_options::mcp_default_ids(&this.mcp_servers);
                cx.notify();
            });
        })
        .detach();
    }

    fn toggle_mcp_server(&mut self, id: &str) {
        if let Some(at) = self.mcp_selected.iter().position(|picked| picked == id) {
            self.mcp_selected.remove(at);
            return;
        }
        self.mcp_selected.push(id.to_string());
        let order: Vec<&str> = self.mcp_servers.iter().map(|s| s.id.as_str()).collect();
        self.mcp_selected.sort_by_key(|id| {
            order.iter().position(|known| known == id).unwrap_or(usize::MAX)
        });
    }

    /// EXP-792: the inline MCP pill — the web chat page's `McpServerPicker`
    /// on the same row, summarised the same way.
    fn mcp_picker(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let trigger = Button::new("chat-pick-mcp")
            .ghost()
            .cursor_pointer()
            .h_auto()
            .px_1()
            .py_0()
            .text_color(cx.theme().muted_foreground)
            .dropdown_caret(true)
            .child(div().text_xs().child(SharedString::from(format!(
                "MCP: {}",
                mcp_pick_summary(&self.mcp_servers, &self.mcp_selected)
            ))));
        mcp_pick_popover(
            "chat",
            trigger,
            &self.mcp_servers,
            &self.mcp_selected,
            |view: &mut Self, id: &str| view.toggle_mcp_server(id),
            cx,
        )
        .into_any_element()
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
            .when(!self.mcp_servers.is_empty(), |this| {
                this.child(self.mcp_picker(cx))
            })
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
    /// inserts the text through the mention widget so a trailing `#` opens
    /// the issue picker, exactly as typing it would. EXP-820: the page's own
    /// draw of [`CHAT_SUGGESTION_COUNT`] from the pool, fixed for its life.
    fn render_suggestions(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        if !self.input.read(cx).value().trim().is_empty() {
            return None;
        }
        let chips = self.suggestions.iter().enumerate().map(|(index, &pick)| {
            let text: &'static str = CHAT_SUGGESTIONS[pick];
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
    mcp_server_ids: Vec<String>,
) -> coding::LaunchOptions {
    coding::LaunchOptions {
        agent,
        model: model.to_string(),
        effort: effort.to_string(),
        ultracode: false,
        plan_mode: plan && agent.supports_plan_mode(),
        // EXP-792: the row's own pick, seeded from `enabled_by_default`.
        mcp_server_ids,
        // EXP-747 B7: the machine's ambient login. The chat row is
        // deliberately three controls wide (machine, agent, plan) — model
        // and effort are not on it either — so there is no account picker
        // here; the Start-coding dialog is where a run picks a profile.
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
        self.ensure_mcp_loaded(cx);
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

    /// EXP-790/EXP-820: the pool is the web page's `CHAT_SUGGESTIONS`
    /// (`lib/chat-suggestions.ts`), byte for byte and in the same order.
    #[test]
    fn chat_suggestions_mirror_the_web_page() {
        assert_eq!(
            CHAT_SUGGESTIONS,
            [
                "Fix #",
                "Explain #",
                "Review #",
                "Split # into sub-issues",
                "Label every issue in the backlog",
                "Set a priority on every unprioritized issue",
                "Find duplicate issues and link them",
                "Do a code review of the open PRs and file the findings on a new board",
                "Create an automation that labels new issues",
                "Set up a weekly standup digest automation",
                "Draft release notes from the issues completed this month",
                "Summarize what changed across the boards this week",
                "Start a session for # on my other machine",
                "Move stale in-progress issues back to the backlog",
                "Comment a plan on #",
                "Which issues are blocked, and by what?",
            ]
        );
    }

    /// EXP-820: a page's draw is four DISTINCT pool entries, whatever the
    /// seed (including the degenerate zero).
    #[test]
    fn a_page_draws_four_distinct_suggestions() {
        for seed in [0u64, 1, 42, u64::MAX, suggestion_seed()] {
            let picks = pick_chat_suggestions(seed);
            assert_eq!(picks.len(), CHAT_SUGGESTION_COUNT, "seed {seed}");
            let mut sorted = picks.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), CHAT_SUGGESTION_COUNT, "seed {seed}: {picks:?}");
            for pick in picks {
                assert!(
                    pick < CHAT_SUGGESTIONS.len(),
                    "seed {seed}: index {pick} outside the pool"
                );
            }
        }
        // Different seeds do reach different draws — it is a shuffle, not a
        // fixed prefix.
        let draws: std::collections::HashSet<Vec<usize>> =
            (1..64u64).map(|seed| pick_chat_suggestions(seed * 7919)).collect();
        assert!(draws.len() > 1);
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
        let claude = chat_options(CodingAgent::Claude, "opus", "high", true, Vec::new());
        assert!(!claude.ultracode);
        assert!(claude.plan_mode);
        assert_eq!((claude.model.as_str(), claude.effort.as_str()), ("opus", "high"));
        assert!(claude.external.is_none());

        // Codex has no plan mode — a switch left on cannot reach the argv.
        assert!(!CodingAgent::Codex.supports_plan_mode());
        assert!(!chat_options(CodingAgent::Codex, "", "", true, Vec::new()).plan_mode);

        // Off is off.
        assert!(!chat_options(CodingAgent::Claude, "opus", "", false, Vec::new()).plan_mode);
    }

    /// EXP-792: the row's MCP pick reaches the launch options verbatim (it
    /// was hardcoded empty until EXP-807), and the account stays the
    /// machine's ambient login — the chat row has no profile picker.
    #[test]
    fn chat_options_carry_the_mcp_pick() {
        let options = chat_options(
            CodingAgent::Claude,
            "opus",
            "",
            false,
            vec!["srv-1".to_string(), "srv-2".to_string()],
        );
        assert_eq!(options.mcp_server_ids, ["srv-1".to_string(), "srv-2".to_string()]);
        assert_eq!(options.account, None);
    }
}
