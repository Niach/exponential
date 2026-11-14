//! EXP-1294 / NEW-6 — an AGENT's page ([`Screen::Agent`], web
//! `agents/$agentId.tsx`): the screen id IS the `agents` row; the page shows
//! the CALLER's own conversation with it (a root session with that
//! `agent_id` and `user_id` = me). #EXP-1732: once it exists the page IS THE
//! session screen ([`crate::session_chat::SessionChatScreenView`]); before,
//! the same chrome (THE header with THE `…`, THE Segmented, the beside rule)
//! over an empty chat whose composer opens one with
//! `sessions.send({agentId})`. The header is the `agents` row's tinted icon
//! and name. #EXP-1732: the Chat tab = the ROOT's own session view (its
//! raw work, its cards, THE Dock and THE composer, `host_root_composer`:
//! Send⇄Stop over the tree), `Message <name>`.
//!
//! VAPP-110: the same channel is an app EDITOR's chat
//! ([`ChannelSubject::App`], embedded in [`crate::app_screen`] without the
//! header): the member's editor session (a root with that `app_id` and
//! `user_id` = me; once it exists, its own session view), the app's glyph as the agent's mark, the first send
//! `sessions.send({appId, text, deviceId})` on EDITOR DEVICE (#FEED-204).
//! VAPP-176: a DRAFT's empty chat = the page-size EmptyState `starterTitle`
//! whose `starters` pills SEND their text through that same first send (and,
//! VAPP-234, a `Start from a template` pill opens the import dialog), the
//! composer focused with `draftPlaceholder`.

use std::rc::Rc;
use std::time::Instant;

use gpui::{
    div, px, AnyElement, App, AppContext as _, Entity, FocusHandle,
    Focusable, InteractiveElement as _, IntoElement,
    ParentElement, Render, SharedString, Styled, Subscription, Window,
};
use gpui_component::input::{InputEvent, TextareaState};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _};
use sync::Store;

use domain::rows::{AgentDefRow, AppRow, SessionRow};
use domain::session_screen::{
    session_screen, session_screen_kind, SessionScreen, SessionScreenCounts, SessionScreenLayout, SessionScreenOpen,
    SessionScreenTab,
};

use crate::composer_images::{self, PendingImages};
use crate::icons::registry;

/// The header's side inset.
const HEADER_INSET: f32 = 44.;
/// The header's lead disc and the glyph in it (web `disc(`size-8`, `size-4`)`).
const HEADER_DISC_PX: f32 = 32.;
const HEADER_GLYPH_PX: f32 = 16.;

/// The page whose agent is not (or no longer) there (web `$agentId.tsx`).
const AGENT_NOT_FOUND: &str = "Not found";

/// The composer's placeholder: `Message <name>`.
pub(crate) fn message_placeholder(name: &str) -> String {
    format!("Message {name}")
}

/// The agent's display name: the `agents` row's `name`, `Untitled` while it
/// is blank (EXP-1341's ladder bottom).
pub(crate) fn agent_def_name(agent: &AgentDefRow) -> String {
    domain::session_title::title_from_parts(agent.name.as_deref(), &[], None)
}

/// The agent's mark: its icon in its colour (web `BoardGlyph`).
pub(crate) fn agent_def_icon(agent: Option<&AgentDefRow>) -> Icon {
    crate::icons::agent_icon(
        agent.and_then(|agent| agent.icon.as_deref()),
        agent.and_then(|agent| agent.color.as_deref()),
    )
}

/// The header's lead disc (web `disc(`size-8`, `size-4`)`): the agent's
/// avatar on bg muted; the Assistant = the brand mark.
pub(crate) fn agent_header_disc(agent: Option<&AgentDefRow>, cx: &App) -> AnyElement {
    let glyph = match agent {
        Some(agent) if agent.is_assistant() => Icon::from(crate::icons::ExpIcon::Logo)
            .with_size(px(HEADER_GLYPH_PX))
            .text_color(cx.theme().foreground)
            .into_any_element(),
        _ => agent_def_icon(agent).with_size(px(HEADER_GLYPH_PX)).into_any_element(),
    };
    header_disc(glyph, cx)
}

/// THE header's lead disc (the chat header's `disc` on the muted fill)
/// around a glyph: an agent's avatar, the Assistant's mark, an app's
/// BoardGlyph (session-screen.json APP LEAD).
pub(crate) fn header_disc(glyph: AnyElement, cx: &App) -> AnyElement {
    div()
        .size(px(HEADER_DISC_PX))
        .flex_shrink_0()
        .rounded_full()
        .bg(cx.theme().muted)
        .flex()
        .items_center()
        .justify_center()
        .child(glyph)
        .into_any_element()
}

/// #FEED-239: the agent's `…` = THE session screen menu (kind agent /
/// assistant); its settings entry = `on_settings` (the in-screen pane).
pub(crate) fn agent_header_menu(
    agent_id: &str,
    on_settings: crate::session_chat::OnPress,
    link: Option<crate::session_chat::ChatMenuTarget>,
    cx: &App,
) -> Option<AnyElement> {
    let (kind, entries) = crate::chat_actions::agent_screen_entries(agent_id, link.is_some(), cx);
    let agent_id = agent_id.to_string();
    crate::chat_actions::screen_menu_button(
        !entries.is_empty(),
        move |menu, window, cx| {
            let id = agent_id.clone();
            let on_settings = on_settings.clone();
            let on_entry: crate::chat_actions::OnMenuEntry = Rc::new(move |entry, window, cx| {
                crate::chat_actions::run_agent_screen_entry(entry, &id, &*on_settings, window, cx)
            });
            crate::session_chat::link_then_menu_items(menu, kind.as_str(), &entries, on_entry, link.as_ref(), window, cx)
        },
        cx,
    )
}

/// #FEED-239: the agent whose chat opens next with its settings open (a
/// rail / chat-list row's `…` › settings), consumed by that agent's screen.
#[derive(Default)]
struct AgentSettingsRequest(Option<String>);

impl gpui::Global for AgentSettingsRequest {}

/// #FEED-239: open `agent_id`'s chat with its settings open (`open:
/// settings`), never the Settings screen.
pub(crate) fn open_agent_settings(window: &mut Window, cx: &mut App, agent_id: String) {
    cx.set_global(AgentSettingsRequest(Some(agent_id.clone())));
    crate::navigation::navigate(window, cx, crate::navigation::Screen::Agent { agent_id });
}

/// The pending settings request for `agent_id`, consumed.
fn take_settings_request(agent_id: &str, cx: &mut App) -> bool {
    let asked = cx.try_global::<AgentSettingsRequest>().and_then(|request| request.0.as_deref()) == Some(agent_id);
    if asked {
        cx.set_global(AgentSettingsRequest(None));
    }
    asked
}

/// NEW-55 (voice-call.json): THE telephone, left of `…`, the header's icon
/// button: disabled with the reason as its tooltip while a call cannot start
/// here (no voice device, not this machine, no voice yet, no usable GPU, a
/// call this client does not run). S-3: while THIS client runs the call with
/// this agent it stays enabled and focuses the call panel; a new call focuses
/// it too (Esc there ends it).
pub(crate) fn agent_header_call(agent_id: &str, name: &str, panel: FocusHandle, cx: &App) -> AnyElement {
    let copy = &etalk::call::VOICE_CALL.copy;
    let here = etalk_gpui::voice_call::runs_for(agent_id, cx);
    let reason = (!here).then(|| etalk_gpui::voice_call::unavailable(cx)).flatten();
    let tooltip = reason.clone().unwrap_or_else(|| copy.call.replace("{agent}", name));
    let agent_id = agent_id.to_string();
    let button = crate::controls::ghost_icon_button(
        etalk::call::VOICE_CALL.test_ids.call.as_str(),
        Icon::new(registry::VOICE_CALL),
        cx,
    )
    .tooltip(tooltip)
    .disabled(reason.is_some())
    .on_click(move |_, window, cx| {
        if !etalk_gpui::voice_call::runs_for(&agent_id, cx) {
            etalk_gpui::voice_call::start(agent_id.clone(), cx);
        }
        window.focus(&panel, cx);
    });
    div().flex_shrink_0().child(button).into_any_element()
}

/// NEW-6: the `agents` row an [`crate::navigation::Screen::Agent`] id names.
/// The id IS the agent row; a LEGACY link (an older build, a notification)
/// named the agent's conversation SESSION, whose `agent_id` resolves it
/// (migrated agents kept one id for both). Unknown ids stay as they are.
pub(crate) fn resolve_agent_id(id: &str, cx: &App) -> String {
    if crate::session_data::agent_def(id, cx).is_some() {
        return id.to_string();
    }
    crate::session_data::session(id, cx)
        .and_then(|row| row.agent_id)
        .filter(|agent_id| !agent_id.is_empty())
        .unwrap_or_else(|| id.to_string())
}

/// NEW-6: the tree root an agent screen's tab holds: the CALLER's own
/// conversation with the agent once it exists, else the agent id (the
/// placeholder root until the first message creates the conversation).
pub(crate) fn agent_screen_root(id: &str, cx: &App) -> String {
    let agent_id = resolve_agent_id(id, cx);
    crate::session_data::my_conversation(&agent_id, cx)
        .map(|row| row.id)
        .unwrap_or(agent_id)
}

/// What the channel talks to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ChannelSubject {
    /// An agent's page: the id it was opened with (the agent row, or a
    /// legacy conversation session id).
    Agent(String),
    /// VAPP-110: an app editor's chat (the `apps` row id).
    App(String),
}

/// THE composer before the conversation exists: the session composer box,
/// whose send opens the conversation with `sessions.send({agentId, text})`
/// (web `AgentComposer`).
struct FirstComposer {
    input: Entity<TextareaState>,
    mention: Entity<crate::mention_input::MentionInput>,
    images: PendingImages,
    sending: bool,
    notice: Option<SharedString>,
    mention_team: Option<String>,
    /// #EXP-1715 (composer-menu.json `teamsPick`): the Assistant's Teams
    /// pick for the first message (`None` = every team).
    teams_picked: crate::teams_pick::TeamsPick,
    /// #EXP-1793 (composer-menu.json `mcpPick`): the "+" › MCP servers pick
    /// for the first message (`None` = the agent's `mcpServerIds`, the
    /// seed); a pick that differs rides the send as `mcpServerIds`.
    mcp_picked: Option<Vec<String>>,
    /// #FEED-219: the synced draft (`agent:<id>` / `app:<id>`).
    draft: crate::composer_drafts::DraftField,
}

/// #EXP-1793: one team MCP server as the first composer's "+" lists it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct FirstMcpServer {
    id: String,
    name: String,
    url: Option<String>,
    command: Option<String>,
    /// The viewer has not connected it (a pick opens Settings › MCP).
    blocked: bool,
}

pub struct AgentScreenView {
    subject: ChannelSubject,
    agent: Option<AgentDefRow>,
    /// VAPP-110: the app an editor chat belongs to.
    app: Option<AppRow>,
    /// VAPP-110: the device an editor chat's first send runs on
    /// (#FEED-204: [`Self::editor_device`]'s).
    device_id: Option<String>,
    /// #FEED-204 (apps.json EDITOR DEVICE): the composer's Device row.
    editor_device: Option<crate::device_move::EditorDevice>,
    /// #EXP-1729 (apps.json `editorHeader`): the editor composer's held
    /// Account and Model picks, THE launch cluster seeded from the editor
    /// device's launch defaults (re-seeded when the device changes, held
    /// while the editor is open), and the device it was seeded on.
    editor_launch: Option<crate::launch_options::LaunchOptionsSection>,
    editor_launch_device: Option<String>,
    /// #EXP-1729: the ended root the picks were last seeded from (its own
    /// account and model, until a pick here).
    editor_root_seed: Option<String>,
    /// #EXP-1729: the "+" Account › / Model › rows off [`Self::editor_launch`]
    /// (`None` outside an app editor).
    editor_runner: Option<crate::launch_options::EditorRunner>,
    /// The caller's own conversation session with the agent (a root with
    /// that `agent_id` and `user_id` = me); `None` before the first message.
    conversation: Option<SessionRow>,
    /// #EXP-1732: an editor's chat once its root exists = THE root's own
    /// session view (its raw work, its cards, THE Dock and THE composer),
    /// keyed by the root id.
    root_view: Option<(String, Entity<crate::session_screen::SessionScreenView>)>,
    first: FirstComposer,
    /// S-3: an agent's conversation, once it exists, IS a chat: THE chat
    /// screen over its root (chat mode `bubbles`); this view is then only
    /// its wrapper. Keyed by the root id.
    chat: Option<(String, Entity<crate::session_chat::SessionChatScreenView>)>,
    /// THE ask dock's state (the focused card, the swap animation).
    /// N-12: THE Dock's state (the fan, the open sheet, the band height).
    dock: Entity<crate::dock::DockState>,
    /// The name the placeholder was built for.
    placeholder_name: Option<String>,
    /// VAPP-176: a draft's first composer took the focus once.
    draft_focused: bool,
    /// #EXP-1380: a [`Self::queue_refresh`] is pending.
    refresh_queued: bool,
    opened_at: Instant,
    focus_handle: FocusHandle,
    /// S-3: the in-window call panel over the conversation's body (empty
    /// unless THIS client runs a call with the agent).
    call_panel: Entity<etalk_gpui::voice_call::CallPanel>,
    /// #EXP-1714: the window's navigation, whose ACTIVE team scopes a global
    /// Assistant's prompts, issue refs and mentions.
    nav: Entity<crate::navigation::Navigation>,
    /// #EXP-1732: before the conversation exists: the header tab on show,
    /// the settings open (THE `…`'s entry), its page (built on first open)
    /// and scroll.
    tab: SessionScreenTab,
    settings_open: bool,
    settings: Option<Entity<crate::settings::agent_detail::AgentDetailPane>>,
    settings_scroll: gpui::ScrollHandle,
    /// #EXP-1793: the agent's team's MCP servers (`mcpServers.list`,
    /// server-only) for the first composer's "+", and the team asked for.
    mcp: Option<(String, Vec<FirstMcpServer>)>,
    mcp_requested: Option<String>,
    /// #EXP-1837: the Assistant widget's chat: the Chat tab's body alone
    /// (no header, no tabs).
    widget: bool,
    _subscriptions: Vec<Subscription>,
}

impl AgentScreenView {
    pub fn new(agent_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::for_subject(ChannelSubject::Agent(agent_id), window, cx)
    }

    /// #EXP-1837: the Assistant widget's chat (no header, no tabs; the
    /// widget draws its own header).
    pub(crate) fn for_widget(agent_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let mut this = Self::for_subject(ChannelSubject::Agent(agent_id), window, cx);
        this.widget = true;
        this
    }

    /// VAPP-110: an app editor's chat (no header; the editor draws it).
    pub(crate) fn for_app(app_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::for_subject(ChannelSubject::App(app_id), window, cx)
    }

    fn for_subject(subject: ChannelSubject, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let (input, mention) = crate::composer::session_composer_field(message_placeholder(""), window, cx);
        let mut subscriptions = vec![cx.subscribe_in(
            &input,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { shift: false, .. } => this.send_first(window, cx),
                InputEvent::Change => {
                    // #FEED-219: the person's edit, written synced.
                    let text = this.first.input.read(cx).value().to_string();
                    this.first.draft.edited(&text, cx);
                    cx.notify();
                }
                _ => {}
            },
        )];
        // #FEED-219: the view going away writes what is pending.
        subscriptions.push(cx.on_release(|this, cx| this.first.draft.leave(cx)));
        // #FEED-239: a row's `…` › settings asking this agent's chat.
        subscriptions.push(cx.observe_global::<AgentSettingsRequest>(|_, cx| cx.notify()));
        if let Some(store) = Store::try_global(cx) {
            let collections = store.collections().clone();
            subscriptions.push(cx.observe_in(&collections.agents, window, |this, _, window, cx| {
                this.queue_refresh(window, cx)
            }));
            subscriptions.push(cx.observe_in(&collections.apps, window, |this, _, window, cx| {
                this.queue_refresh(window, cx)
            }));
            subscriptions.push(cx.observe_in(&collections.devices, window, |this, _, window, cx| {
                this.queue_refresh(window, cx)
            }));
            subscriptions.push(cx.observe_in(&collections.sessions, window, |this, _, window, cx| {
                this.queue_refresh(window, cx)
            }));
            subscriptions.push(cx.observe_in(&collections.messages, window, |this, _, window, cx| {
                this.queue_refresh(window, cx)
            }));
            subscriptions.push(cx.observe(&collections.users, |_, _, cx| cx.notify()));
            // #FEED-219: an untouched first composer follows its draft.
            subscriptions.push(cx.observe_in(&collections.composer_drafts, window, |this, _, window, cx| {
                this.first.draft.reconcile(cx);
                if crate::composer_drafts::apply_follow(&this.first.draft, &this.first.input, window, cx) {
                    cx.notify();
                }
            }));
        }
        // The "Not found" verdict waits out the sync grace: one repaint then.
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(crate::session_chat::MISSING_GRACE)
                .await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
        let mut this = Self {
            subject,
            agent: None,
            app: None,
            device_id: None,
            editor_device: None,
            editor_launch: None,
            editor_launch_device: None,
            editor_root_seed: None,
            editor_runner: None,
            conversation: None,
            root_view: None,
            chat: None,
            first: FirstComposer {
                input,
                mention,
                images: PendingImages::default(),
                sending: false,
                notice: None,
                mention_team: None,
                teams_picked: None,
                mcp_picked: None,
                draft: Default::default(),
            },
            dock: cx.new(|_| crate::dock::DockState::default()),
            placeholder_name: None,
            draft_focused: false,
            refresh_queued: false,
            opened_at: Instant::now(),
            focus_handle: cx.focus_handle(),
            call_panel: cx.new(etalk_gpui::voice_call::CallPanel::new),
            nav: crate::navigation::nav_for_window(window, cx),
            tab: SessionScreenTab::Chat,
            settings_open: false,
            settings: None,
            settings_scroll: gpui::ScrollHandle::new(),
            mcp: None,
            mcp_requested: None,
            widget: false,
            _subscriptions: subscriptions,
        };
        this.refresh(window, cx);
        this
    }

    /// Whether the channel's subject (the agent, the app) has synced.
    fn has_subject(&self) -> bool {
        self.agent.is_some() || self.app.is_some()
    }

    /// The subject's name (the agent's, the app's).
    fn subject_name(&self) -> String {
        match &self.app {
            Some(app) => app.name.clone(),
            None => self.agent.as_ref().map(agent_def_name).unwrap_or_default(),
        }
    }

    /// #EXP-1715: an agent chat with the viewer's own Assistant (the Teams
    /// pick rides its sends).
    fn to_assistant(&self, cx: &App) -> bool {
        matches!(self.subject, ChannelSubject::Agent(_))
            && self.agent.as_ref().is_some_and(|agent| crate::teams_pick::is_my_assistant(agent, cx))
    }

    /// #EXP-1793: the agent's team (the servers are its own team's).
    fn mcp_team_id(&self) -> Option<String> {
        match &self.subject {
            ChannelSubject::Agent(_) => self.agent.as_ref().and_then(|agent| agent.team_id.clone()),
            ChannelSubject::App(_) => None,
        }
    }

    /// #EXP-1793: one `mcpServers.list` per team, before the conversation
    /// exists (the first composer's "+" › MCP servers).
    fn ensure_mcp_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(team_id) = self.mcp_team_id() else { return };
        if self.conversation.is_some() || self.mcp_requested.as_deref() == Some(team_id.as_str()) {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else { return };
        self.mcp_requested = Some(team_id.clone());
        cx.spawn(async move |this, cx| {
            let team = team_id.clone();
            let loaded = cx.background_executor().spawn(async move { api::mcp_servers::list(&trpc, &team) }).await;
            let _ = this.update(cx, |this, cx| {
                if this.mcp_requested.as_deref() != Some(team_id.as_str()) {
                    return;
                }
                match loaded {
                    Ok(entries) => {
                        let servers = entries
                            .into_iter()
                            .map(|entry| FirstMcpServer {
                                blocked: crate::launch_options::mcp_block_reason(&entry.connection).is_some(),
                                id: entry.config.id,
                                name: entry.config.name,
                                url: entry.config.url,
                                command: entry.config.command,
                            })
                            .collect();
                        this.mcp = Some((team_id, servers));
                    }
                    Err(err) => log::debug!("[ui] mcpServers.list for the agent composer: {err}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// #EXP-1793: the team's servers and the first composer's pick (the
    /// held one, else the agent's), team list order.
    fn first_mcp(&self) -> (Vec<FirstMcpServer>, Vec<String>) {
        let team = self.mcp_team_id();
        let servers = match (&self.mcp, team) {
            (Some((loaded, servers)), Some(team)) if *loaded == team => servers.clone(),
            _ => Vec::new(),
        };
        let seed = self.agent.as_ref().map(|agent| agent.mcp_server_ids.clone()).unwrap_or_default();
        let picked = self.first.mcp_picked.as_ref().unwrap_or(&seed);
        let picked = servers.iter().filter(|server| picked.contains(&server.id)).map(|server| server.id.clone()).collect();
        (servers, picked)
    }

    /// #EXP-1793: what the first send carries as `mcpServerIds`: the pick
    /// while it differs from the agent's (`[]` clears).
    fn first_mcp_send(&self) -> Option<Vec<String>> {
        let picked = self.first.mcp_picked.as_ref()?;
        let seed = self.agent.as_ref().map(|agent| agent.mcp_server_ids.as_slice()).unwrap_or_default();
        let same = picked.len() == seed.len() && picked.iter().all(|id| seed.contains(id));
        (!same).then(|| picked.clone())
    }

    /// #EXP-1715: a changed Teams pick lands on the first composer.
    fn teams_change(&self, cx: &gpui::Context<Self>) -> crate::teams_pick::OnTeamsPick {
        let view = cx.entity().downgrade();
        std::rc::Rc::new(move |next, _window, cx| {
            if let Some(view) = view.upgrade() {
                view.update(cx, |this, cx| {
                    this.first.teams_picked = next;
                    cx.notify();
                });
            }
        })
    }

    /// The team the subject's `/` prompts, issue refs, mentions and template
    /// starter read. #EXP-1714: a global Assistant (its team is none of the
    /// synced teams: the hidden personal team) borrows the window's ACTIVE
    /// team; `send_first` keeps the agent's own team.
    fn subject_team_id(&self, cx: &App) -> Option<String> {
        match &self.app {
            Some(app) => app.team_id.clone(),
            None => {
                let team_id = self.agent.as_ref().and_then(|agent| agent.team_id.clone());
                let synced = team_id.as_deref().is_some_and(|team_id| {
                    Store::try_global(cx).is_some_and(|store| store.collections().teams.read(cx).get(team_id).is_some())
                });
                if synced || self.agent.is_none() || Store::try_global(cx).is_none() {
                    team_id
                } else {
                    crate::navigation::active_team_id(&self.nav, cx)
                }
            }
        }
    }

    /// The composer's placeholder: `Message <name>`; an editor chat reads
    /// the fixture's `composerPlaceholder` (`noDevice` with no device).
    fn placeholder(&self) -> String {
        match &self.subject {
            ChannelSubject::Agent(_) => message_placeholder(&self.subject_name()),
            ChannelSubject::App(_) if self.device_id.is_none() && self.conversation.is_none() => {
                domain::apps::APPS.copy.no_device.clone()
            }
            ChannelSubject::App(_) if self.is_empty_draft() => domain::apps::APPS.copy.draft_placeholder.clone(),
            ChannelSubject::App(_) => domain::apps::APPS.copy.composer_placeholder.clone(),
        }
    }

    /// VAPP-176: an editor chat of a DRAFT (no version yet) with nothing
    /// sent (no editor session): the starters show.
    fn is_empty_draft(&self) -> bool {
        self.conversation.is_none()
            && self.app.as_ref().is_some_and(|app| domain::apps::is_draft(app.head_version.unwrap_or(0)))
    }

    /// VAPP-176: a starter pill SENDS its text as the first message, through
    /// the composer's own first send.
    fn send_starter(&mut self, text: String, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.first.sending || self.device_id.is_none() {
            return;
        }
        self.first.input.update(cx, |input, cx| input.set_value(text, window, cx));
        self.send_first(window, cx);
    }

    /// VAPP-176: the draft's page-size EmptyState (the app glyph disc,
    /// `starterTitle`) with one action Pill per `starters` entry, disabled
    /// like the composer without a device.
    fn render_starters(&self, cx: &mut gpui::Context<Self>) -> gpui::Div {
        let copy = &domain::apps::APPS.copy;
        let disabled = self.device_id.is_none() || self.first.sending;
        let pills = copy.starters.iter().enumerate().map(|(index, text)| {
            let starter = text.clone();
            crate::surface::glass_pill_button(
                SharedString::from(format!("{}-{index}", domain::apps::APPS.test_ids.starter)),
                crate::surface::PillSize::Md,
                cx,
            )
            .label(text.clone())
            .disabled(disabled)
            .on_click(cx.listener(move |this, _, window, cx| this.send_starter(starter.clone(), window, cx)))
        });
        // VAPP-234: one more pill opens the import dialog's templates step
        // (an import needs no device: never disabled).
        let team_id = self.subject_team_id(cx);
        let from_template = team_id.map(|team_id| {
            crate::surface::glass_pill_button(
                SharedString::from(domain::app_templates::APP_TEMPLATES_TEST_IDS.from_template.clone()),
                crate::surface::PillSize::Md,
                cx,
            )
            .label(domain::app_templates::APP_TEMPLATES_COPY.from_template.clone())
            .on_click(move |_, window, cx| crate::app_import_dialog::open(window, cx, team_id.clone()))
        });
        crate::controls::empty_state(Icon::new(registry::NAV_APPS), copy.starter_title.clone(), None, cx)
            .child(crate::controls::empty_state_actions().children(pills).children(from_template))
    }

    /// #EXP-1380: one [`Self::refresh`] per effect cycle, however many
    /// collections synced in it (a sync burst touched sessions, messages and
    /// conversations at once: three full rebuilds of a 1200-row chat).
    fn queue_refresh(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.refresh_queued {
            return;
        }
        self.refresh_queued = true;
        cx.defer_in(window, |this, window, cx| {
            this.refresh_queued = false;
            this.refresh(window, cx);
        });
    }

    /// #FEED-219: the first composer's draft key once the subject resolved
    /// (`agent:<agentId>`, `app:<appId>`) and the team its row files under.
    fn sync_first_draft(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        use domain::composer_draft::{Kind, Target};
        let (target, team_id) = match (&self.subject, &self.agent, &self.app) {
            (ChannelSubject::Agent(_), Some(agent), _) => {
                (Some(Target::new(Kind::Agent, agent.id.clone())), agent.team_id.clone())
            }
            (ChannelSubject::App(_), _, Some(app)) => (Some(Target::new(Kind::App, app.id.clone())), app.team_id.clone()),
            _ => (None, None),
        };
        if target.is_none() {
            return;
        }
        if self.first.draft.retarget(target, team_id, cx) {
            crate::composer_drafts::apply_follow(&self.first.draft, &self.first.input, window, cx);
        }
    }

    fn refresh(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match self.subject.clone() {
            ChannelSubject::Agent(requested) => {
                let agent_id = resolve_agent_id(&requested, cx);
                self.agent = crate::session_data::agent_def(&agent_id, cx);
                self.conversation = crate::session_data::my_conversation(&agent_id, cx);
                // S-3: the call panel draws this agent's call.
                let agent = self.agent.as_ref().map(|agent| agent.id.clone());
                self.call_panel.update(cx, |panel, cx| panel.set_agent(agent, cx));
            }
            ChannelSubject::App(app_id) => {
                self.app = Store::try_global(cx)
                    .and_then(|store| store.collections().apps.read(cx).get(&app_id).cloned());
                self.conversation = crate::session_data::my_editor_session(&app_id, cx);
                self.editor_device = self.editor_device_row(&app_id, cx);
                self.device_id = self.editor_device.as_ref().and_then(|device| device.device_id.clone());
                self.sync_editor_runner(window, cx);
            }
        }
        self.sync_first_draft(window, cx);
        let name = self.subject_name();
        let placeholder = self.placeholder();
        // #EXP-1732: an editor's chat = THE root's own session view, built
        // once the root exists (and rebuilt should it be another row); an
        // agent's conversation is THE session screen (`render`).
        let editor = matches!(self.subject, ChannelSubject::App(_));
        match self.conversation.as_ref().filter(|_| editor) {
            Some(conversation)
                if self.root_view.as_ref().map(|(id, _)| id.as_str()) != Some(conversation.id.as_str()) =>
            {
                let id = conversation.id.clone();
                let view = cx.new(|cx| crate::session_screen::SessionScreenView::new(id, window, cx));
                // THE root composer (Send⇄Stop over the tree); EXP-1650: the
                // editor's chat shows only "Session ended".
                let inner = view.read(cx).inner().clone();
                inner.update(cx, |composer, cx| {
                    composer.host_root_composer(
                        placeholder.clone(),
                        crate::steer_viewer::ComposerIds::new("agent-send", "agent-attach", "agent-model", "agent-context"),
                        window,
                        cx,
                    );
                    composer.hide_empty_replay(cx);
                });
                self._subscriptions.push(cx.observe(&view, |_, _, cx| cx.notify()));
                self.root_view = Some((conversation.id.clone(), view));
                self.placeholder_name = None;
            }
            Some(_) => {}
            None => self.root_view = None,
        }
        let root_composer = self.root_view.as_ref().map(|(_, view)| view.read(cx).inner().clone());
        if let Some(composer) = &root_composer {
            let device = self.editor_device.clone();
            // #EXP-1729: an ENDED root's picks ride its next send; a live
            // root's rows are the session's own switches.
            let ended = self.conversation.as_ref().is_some_and(|root| root.is_ended());
            let runner = self.editor_runner.clone().filter(|_| ended);
            composer.update(cx, |composer, cx| {
                composer.set_editor_device(device, cx);
                composer.set_editor_runner(runner, cx);
            });
        }
        let _ = name;
        if self.has_subject() && self.placeholder_name.as_deref() != Some(placeholder.as_str()) {
            if let Some(composer) = &root_composer {
                composer.update(cx, |composer, cx| {
                    composer.set_placeholder(placeholder.clone(), window, cx);
                });
            }
            self.first.input.update(cx, |input, cx| {
                input.set_placeholder(placeholder.clone(), window, cx)
            });
            self.placeholder_name = Some(placeholder);
        }
        let team_id = self.subject_team_id(cx);
        if team_id != self.first.mention_team {
            self.first.mention_team = team_id.clone();
            self.first.mention.update(cx, |mention, _| {
                mention.set_source(team_id.map(crate::markdown::store_completion_source));
            });
        }
        cx.notify();
    }

    /// #FEED-204 (apps.json EDITOR DEVICE): the Device row over the launch
    /// devices (with a root: the ones it may move to), the pick remembered
    /// per app, a live root on its own device until a pick moves it.
    fn editor_device_row(
        &self,
        app_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> Option<crate::device_move::EditorDevice> {
        let team_id = self.app.as_ref()?.team_id.clone()?;
        let launch = crate::queries::launch_devices(Some(team_id.as_str()), cx);
        let root = self.conversation.as_ref();
        let targets = match root {
            Some(root) => crate::device_move::candidates(root, cx),
            None => Vec::new(),
        };
        let move_session = root.map(|root| crate::device_move::move_session(root, cx));
        let current = root.and_then(|root| root.device_id.as_deref());
        let offered: Vec<_> = launch
            .iter()
            .filter(|device| match &move_session {
                None => true,
                Some(session) => {
                    current == Some(device.device_id.as_str())
                        || targets.iter().any(|target| {
                            target.device_id == device.device_id
                                && crate::device_move::refusal(target, session).is_none()
                        })
                }
            })
            .collect();
        let candidates: Vec<(&str, bool)> =
            offered.iter().map(|device| (device.device_id.as_str(), device.is_default)).collect();
        let remembered = crate::ui_prefs::editor_device(app_id);
        let choice = domain::apps::editor_device(
            root.map(|root| (root.status.as_deref().unwrap_or_default(), current)),
            &candidates,
            remembered.as_deref(),
        );
        let mut rows: Vec<(String, String)> =
            offered.iter().map(|device| (device.device_id.clone(), device.label.clone())).collect();
        if let Some(id) = choice.device_id.as_deref().filter(|id| !rows.iter().any(|(row, _)| row == id)) {
            // A root's own device off the launch list (offline).
            let label = targets.iter().find(|target| target.device_id == id).map(|target| target.label.clone());
            rows.insert(0, (id.to_string(), label.unwrap_or_else(|| id.to_string())));
        }
        let view = cx.entity().downgrade();
        let picked_app = app_id.to_string();
        let online = choice.device_id.as_deref().is_some_and(|id| launch.iter().any(|device| device.device_id == id));
        Some(crate::device_move::EditorDevice {
            app_id: app_id.to_string(),
            team_id,
            device_id: choice.device_id,
            rows,
            moves: choice.moves,
            online,
            on_pick: std::rc::Rc::new(move |device_id: String, window: &mut Window, cx: &mut App| {
                crate::ui_prefs::set_editor_device(&picked_app, &device_id);
                let _ = view.update(cx, |this, cx| {
                    this.move_live_root(&device_id, cx);
                    this.refresh(window, cx);
                });
            }),
        })
    }

    /// #EXP-1729: (re)seed the held Account / Model picks off the editor
    /// device's launch defaults (the launch composer's own seeding,
    /// [`crate::launch_options::LaunchOptionsSection::set_remote`]) whenever
    /// the composer's device changes to one that is online, then snapshot
    /// the rows.
    fn sync_editor_runner(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(team_id) = self.app.as_ref().and_then(|app| app.team_id.clone()) else {
            self.editor_runner = None;
            return;
        };
        if self.editor_launch.is_none() {
            self.editor_launch = Some(crate::launch_options::LaunchOptionsSection::new(window, cx));
            self.editor_launch_device = None;
        }
        if self.editor_launch_device != self.device_id {
            let device = self.device_id.as_deref().and_then(|device_id| {
                crate::queries::launch_devices(Some(team_id.as_str()), cx)
                    .into_iter()
                    .find(|device| device.device_id == device_id)
            });
            if let Some(device) = device {
                let remote = crate::chat_screen::remote_defaults(&device, cx);
                if let Some(launch) = self.editor_launch.as_mut() {
                    launch.set_remote(remote, window, cx);
                }
                // Landing on the ended root's own device keeps its picks.
                let root_device = self
                    .conversation
                    .as_ref()
                    .filter(|root| root.is_ended())
                    .and_then(|root| root.device_id.as_deref());
                if root_device == Some(device.device_id.as_str()) {
                    self.editor_root_seed = None;
                }
                self.editor_launch_device = Some(device.device_id);
            }
        }
        // An ENDED root's rows read its OWN agent, account and model until a
        // pick here (a device pick re-seeds off that device, above).
        let ended = self.conversation.as_ref().filter(|root| root.is_ended());
        let root_agent = ended.map(|root| {
            root.cli.as_deref().and_then(coding::CodingAgent::parse).unwrap_or(coding::CodingAgent::Claude)
        });
        if let (Some(root), Some(agent)) = (ended, root_agent) {
            if self.editor_root_seed.as_deref() != Some(root.id.as_str()) {
                let (account, model) = (root.account.clone(), root.model.clone());
                if let Some(launch) = self.editor_launch.as_mut() {
                    launch.seed_from_session(agent, account, model.as_deref(), window, cx);
                }
                self.editor_root_seed = Some(root.id.clone());
            }
        }
        let view = cx.entity().downgrade();
        let account_view = view.clone();
        let on_account: std::rc::Rc<dyn Fn(coding::AccountOption, &mut Window, &mut App)> =
            std::rc::Rc::new(move |option, window, cx| {
                let _ = account_view.update(cx, |this, cx| {
                    if let Some(launch) = this.editor_launch.as_mut() {
                        launch.set_account(&option, window, cx);
                    }
                    this.refresh(window, cx);
                });
            });
        let on_model: std::rc::Rc<dyn Fn(String, &mut Window, &mut App)> = std::rc::Rc::new(move |value, window, cx| {
            let _ = view.update(cx, |this, cx| {
                if let Some(launch) = this.editor_launch.as_mut() {
                    launch.pick_model(&value, window, cx);
                }
                this.refresh(window, cx);
            });
        });
        self.editor_runner = self.editor_launch.as_ref().map(|launch| {
            let mut runner = crate::launch_options::EditorRunner::from_section(launch, on_account, on_model, cx);
            // An ended root keeps its agent: only that agent's logins.
            if let Some(agent) = root_agent {
                runner.menu.accounts.retain(|option| option.agent == agent);
            }
            runner
        });
    }

    /// #EXP-1729 (apps.json `editorHeader`, `editorHeaderCases`): the
    /// editor header's `status · device` subtitle and its tone, off the
    /// member's editor root and the device the composer shows.
    /// With the header's DEVICE part alone (`editor_header` with no root:
    /// the name, else `no device online`; session-screen.json STATUS WORD).
    pub(crate) fn editor_header(&self) -> Option<(domain::apps::EditorHeader, String)> {
        if !matches!(self.subject, ChannelSubject::App(_)) {
            return None;
        }
        let root = self.conversation.as_ref().map(|root| domain::apps::EditorHeaderRoot {
            status: root.status.as_deref().unwrap_or_default(),
            busy: root.busy,
            needs_input: root.needs_input,
        });
        let device = self.editor_device.as_ref().and_then(|device| device.header_device());
        let device = device.as_ref().map(|(name, online)| (name.as_str(), *online));
        Some((domain::apps::editor_header(root, device), domain::apps::editor_header(None, device).subtitle))
    }

    /// #EXP-1738: a pick on a live editor root moves it at once (an ended
    /// root's next send does, [`crate::device_move::EditorDevice::move_target`]).
    fn move_live_root(&self, picked: &str, cx: &mut App) {
        let Some(root) = self.conversation.as_ref().filter(|root| root.is_live()) else {
            return;
        };
        if let Some(device_id) = crate::device_move::pick_outcome(picked, root.device_id.as_deref()) {
            crate::device_move::request_move(root.id.clone(), device_id, cx);
        }
    }

    // ── the first message (web `AgentComposer`) ──────────────────────────

    fn first_can_send(&self, cx: &App) -> bool {
        let has_content =
            !self.first.input.read(cx).value().trim().is_empty() || !self.first.images.is_empty();
        let device_ok = !matches!(self.subject, ChannelSubject::App(_)) || self.device_id.is_some();
        !self.first.sending && has_content && self.has_subject() && device_ok
    }

    /// Upload the staged files to the TEAM route (there is no session yet),
    /// then ONE `sessions.send({teamId, agentId, text, attachmentIds})`: the
    /// conversation is created and the screen fills from the sync.
    fn send_first(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if !self.first_can_send(cx) {
            return;
        }
        let subject_id = match (&self.subject, &self.agent, &self.app) {
            (ChannelSubject::Agent(_), Some(agent), _) => agent.id.clone(),
            (ChannelSubject::App(_), _, Some(app)) => app.id.clone(),
            _ => return,
        };
        // #EXP-1714: the send and its uploads go to the SUBJECT's own team
        // (a global Assistant's = the personal team; the server refuses any
        // other), never the active team `subject_team_id` borrows.
        let team_id = match &self.app {
            Some(app) => app.team_id.clone(),
            None => self.agent.as_ref().and_then(|agent| agent.team_id.clone()),
        };
        let Some(team_id) = team_id else {
            return;
        };
        let app_device = matches!(self.subject, ChannelSubject::App(_)).then(|| self.device_id.clone());
        // #EXP-1729: the held Account / Model picks ride the send.
        let picks = self.editor_runner.as_ref().map(|runner| runner.picks.clone());
        let text = self.first.input.read(cx).value().to_string();
        // EXP-1537: the field stays editable while the send is in flight;
        // what lands takes out only what was sent (`composer_send`).
        let sent = self.first.images.content(text.clone());
        let jobs = self.first.images.jobs();
        let file_keys = composer_images::file_keys(&jobs);
        let transport = if jobs.is_empty() {
            None
        } else {
            match crate::queries::attachment_transport(cx) {
                Some(transport) => Some(transport),
                None => {
                    crate::action_error::failed_action("upload", "the attachment", None, cx);
                    return;
                }
            }
        };
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            crate::action_error::failed_action("send", "the message", None, cx);
            return;
        };
        let upload_team = team_id.clone();
        // #EXP-1715: the Assistant's first turn carries its Teams pick.
        let scope = self.to_assistant(cx).then(|| self.first.teams_picked.clone());
        // #EXP-1793: a changed MCP servers pick rides it too.
        let mcp_ids = self.first_mcp_send();
        let send_input = move |text: &str, attachment_ids: Vec<String>| match &app_device {
            Some(device_id) => {
                let mut input =
                    crate::chat_launch::app_send_input(&team_id, &subject_id, device_id.as_deref(), text, attachment_ids);
                if let Some(picks) = &picks {
                    picks.apply(&mut input, true);
                }
                input
            }
            None => {
                let mut input = crate::chat_launch::agent_send_input(&team_id, &subject_id, text, attachment_ids);
                input.scope_team_ids = scope.clone();
                input.mcp_server_ids = mcp_ids.clone();
                input
            }
        };
        // The sealing facts are the text's and the subject's (the uploads
        // name no one): read now, so the send needs nothing of the screen
        // once the uploads land.
        let plan = crate::sealing::plan_send(&send_input(&text, Vec::new()), cx);
        self.first.sending = true;
        self.first.notice = None;
        cx.notify();
        // EXP-1537: the uploads AND the send run in one background job that
        // holds nothing of the screen, so leaving it never cancels either.
        cx.spawn_in(window, async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn(async move {
                    let resolved = match transport {
                        None => Vec::new(),
                        Some(transport) => composer_images::upload_all(jobs, |filename, content_type, bytes| {
                            transport.upload_team_session_file(&upload_team, filename, content_type, bytes)
                        })
                        .map_err(|(resolved, error)| composer_images::UploadSendError::Upload(resolved, error))?,
                    };
                    let message = composer_images::uploaded_message(&text, &file_keys, &resolved);
                    let parsed = domain::image_message::parse_steer_message(&message);
                    let mut attachment_ids = parsed.attachment_ids;
                    attachment_ids.extend(parsed.files.into_iter().map(|file| file.id));
                    let input = send_input(&parsed.text, attachment_ids);
                    match crate::sealing::send_blocking(&trpc, &input, &plan) {
                        Ok(_) => Ok(()),
                        Err(err) => Err(composer_images::UploadSendError::Send(resolved, err)),
                    }
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.first.sending = false;
                match outcome {
                    Ok(()) => {
                        // #EXP-1793: the conversation holds the pick now.
                        this.first.mcp_picked = None;
                        composer_images::settle_sent(&this.first.input, &mut this.first.images, &sent, window, cx);
                        // #FEED-219: the field the send leaves is written at once.
                        let left = this.first.input.read(cx).value().to_string();
                        this.first.draft.write_now(&left, cx);
                    }
                    Err(composer_images::UploadSendError::Upload(resolved, error)) => {
                        this.first.images.note_uploaded(&resolved);
                        log::warn!("[ui] agent composer upload failed: {error}");
                        let view = cx.entity().downgrade();
                        crate::action_error::failed_action_with_retry(
                            "upload",
                            "the attachment",
                            None,
                            move |window, cx| {
                                let _ = view.update(cx, |this, cx| this.send_first(window, cx));
                            },
                            cx,
                        );
                    }
                    Err(composer_images::UploadSendError::Send(resolved, err)) => {
                        this.first.images.note_uploaded(&resolved);
                        crate::action_error::failed_action_err("send", "the message", &err, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn stage_first_files(
        &mut self,
        files: Vec<composer_images::StagedFile>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.first.notice = self.first.images.stage(files, &self.first.input, window, cx);
        cx.notify();
    }

    fn on_first_paste(
        &mut self,
        _: &gpui_component::input::Paste,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let files = composer_images::clipboard_images(cx);
        if files.is_empty() {
            return;
        }
        cx.stop_propagation();
        self.stage_first_files(files, window, cx);
    }

    fn remove_first_file(&mut self, key: u64, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.first.images.remove(key, &self.first.input, window, cx);
        cx.notify();
    }

    /// THE session composer box, field + attach + Send: the first message.
    fn render_first_composer(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let disabled = !self.first_can_send(cx);
        let view = cx.entity().downgrade();
        let submit = crate::composer::SessionComposerSubmit::send("agent-send", disabled, move |_, window, cx| {
            let _ = view.update(cx, |this, cx| this.send_first(window, cx));
        })
        .loading(self.first.sending);
        let strip = (!self.first.images.is_empty()).then(|| {
            self.first
                .images
                .render_strip("agent-pending-remove", self.first.sending, Self::remove_first_file, cx)
        });
        // #EXP-1715: the Assistant's picked teams lead the strip.
        let chips = self.to_assistant(cx).then(|| {
            let teams = crate::teams_pick::pick_teams(cx);
            crate::teams_pick::chips_row(
                "agent-composer-chip",
                &teams,
                self.first.teams_picked.as_deref(),
                self.first.sending,
                self.teams_change(cx),
                cx,
            )
        });
        let strip = match (chips.flatten(), strip) {
            (None, strip) => strip,
            (Some(chips), strip) => {
                Some(v_flex().w_full().min_w_0().gap_1p5().child(chips).children(strip).into_any_element())
            }
        };
        let notice = self.first.notice.clone();
        let composer = crate::composer::session_composer_card(
            crate::composer::SessionComposerBox::new(self.first.mention.clone(), submit)
                .strip(strip)
                // S-3: THE "+" (composer-menu.json, `agent` surface: Photo or
                // file, Issue ›), never an inline paperclip.
                .tool(self.render_first_plus(cx)),
            cx,
        );
        // #EXP-1841: a paste or a drop stages like the "+" pick (Home's
        // launch composer's handlers).
        let composer = div()
            .w_full()
            .min_w_0()
            .child(composer)
            .capture_action(cx.listener(Self::on_first_paste))
            .on_drop(cx.listener(|this, paths: &gpui::ExternalPaths, window, cx| {
                this.stage_first_files(composer_images::dropped_files(paths), window, cx)
            }))
            .into_any_element();
        match notice {
            Some(notice) => v_flex()
                .w_full()
                .gap_1()
                .child(composer)
                .child(
                    div()
                        .w_full()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(notice),
                )
                .into_any_element(),
            None => composer,
        }
    }

    /// S-3: the first composer's "+" (the `agent` surface before the
    /// conversation exists: nothing to configure yet, so no Model row).
    fn render_first_plus(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        use crate::chat_screen::{composer_menu_rows, ComposerMenuConditions, ComposerMenuRow as Row, ComposerSurface};
        let view = cx.entity().downgrade();
        let input = self.first.input.clone();
        let team_id = self.subject_team_id(cx);
        // #EXP-1434: the team's prompts, the `/` menu's list.
        let prompts: Vec<crate::slash_commands::SlashPrompt> = team_id
            .as_deref()
            .filter(|_| Store::try_global(cx).is_some())
            .map(|team_id| {
                crate::queries::team_prompts(cx, team_id)
                    .0
                    .iter()
                    .map(crate::slash_commands::SlashPrompt::from)
                    .collect()
            })
            .unwrap_or_default();
        let assistant = self.to_assistant(cx);
        let editor_device = self.editor_device.clone();
        // #EXP-1729: and its Account and Model, always.
        let editor_runner = self.editor_runner.clone().filter(|_| editor_device.is_some());
        // #EXP-1793: an agent chat's MCP servers, seeded from the agent's.
        let (mcp_servers, mcp_picked) = self.first_mcp();
        let conditions = ComposerMenuConditions {
            mcp: !mcp_servers.is_empty(),
            mcp_pick: matches!(self.subject, ChannelSubject::Agent(_)),
            // #FEED-204: an app editor picks the device of its first send.
            device: editor_device.is_some(),
            account: editor_runner.is_some(),
            model: editor_runner.is_some(),
            prompts: !prompts.is_empty(),
            // #EXP-1715: the viewer's Assistant in more than one team.
            assistant,
            teams: assistant && crate::teams_pick::several_teams(cx),
            ..ComposerMenuConditions::default()
        };
        // #EXP-1752: the open menu follows the editor's devices live.
        let content_key = {
            use std::hash::{Hash as _, Hasher as _};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            if let Some(device) = &editor_device {
                device.device_id.hash(&mut hasher);
                device.rows.hash(&mut hasher);
            }
            (editor_runner.is_some(), prompts.len(), assistant, conditions.teams).hash(&mut hasher);
            (&mcp_servers, &mcp_picked).hash(&mut hasher);
            hasher.finish()
        };
        let teams_view = view.clone();
        let teams_change = self.teams_change(cx);
        let mcp_view = view.clone();
        let trigger = crate::composer::composer_tool("agent-plus", registry::UI_ADD, cx)
            .tooltip(crate::chat_screen::PLUS_TOOLTIP)
            .disabled(self.first.sending);
        crate::controls::PointerMenu::for_button(trigger, move |mut menu, window, cx| {
            let root = cx.entity().downgrade();
            menu = crate::picker::search_list::search_submenu_parent(menu.min_w(px(260.)));
            for row in composer_menu_rows(ComposerSurface::Agent, conditions) {
                menu = match row {
                    Row::Separator => menu.separator(),
                    // #EXP-1715: THE multi picker rows over the viewer's teams.
                    Row::Teams => {
                        let state_view = teams_view.clone();
                        crate::teams_pick::teams_submenu(
                            menu,
                            "agent-teams",
                            std::rc::Rc::new(move |cx: &App| {
                                let picked =
                                    state_view.upgrade().and_then(|view| view.read(cx).first.teams_picked.clone());
                                (crate::teams_pick::pick_teams(cx), picked)
                            }),
                            teams_change.clone(),
                            root.clone(),
                            window,
                            cx,
                        )
                    }
                    // #FEED-204 (apps.json EDITOR DEVICE).
                    Row::Device => match &editor_device {
                        Some(device) => device.menu_row(menu, row, window, cx),
                        None => menu,
                    },
                    // #EXP-1729: THE launch composer's rows; the pick rides
                    // the first send.
                    Row::Account => match &editor_runner {
                        Some(runner) => runner.account_row(menu, "agent-menu-account", window, cx),
                        None => menu,
                    },
                    Row::Model => match &editor_runner {
                        Some(runner) => runner.model_row(menu, window, cx),
                        None => menu,
                    },
                    // #EXP-1793: THE multi picker rows over the team's servers.
                    Row::McpServers => {
                        let state_view = mcp_view.clone();
                        let state: std::rc::Rc<dyn Fn(&App) -> (Vec<FirstMcpServer>, Vec<String>)> =
                            std::rc::Rc::new(move |cx: &App| {
                                state_view.upgrade().map(|view| view.read(cx).first_mcp()).unwrap_or_default()
                            });
                        let value = (!mcp_picked.is_empty()).then(|| mcp_picked.len().to_string());
                        let pick_view = mcp_view.clone();
                        let root = root.clone();
                        menu.submenu_with_value(
                            row.icon().map(Icon::new),
                            row.label(),
                            value,
                            window,
                            cx,
                            move |sub, window, cx| {
                                crate::picker::search_list::menu_search_list_submenu(
                                    "agent-mcp-servers",
                                    first_mcp_source(state.clone(), pick_view.clone()),
                                    sub,
                                    root.clone(),
                                    window,
                                    cx,
                                )
                            },
                        )
                    }
                    Row::AddFile => {
                        let view = view.clone();
                        menu.item(crate::controls::pointer_menu_item(row.icon(), row.label(), |_| None, move |window, cx| {
                            if let Some(view) = view.upgrade() {
                                view.update(cx, |_, cx| {
                                    composer_images::pick_attachment_files(window, cx, |this: &mut Self, read, window, cx| {
                                        this.stage_first_files(read, window, cx)
                                    });
                                });
                            }
                        }))
                    }
                    Row::ImplementIssue => {
                        let Some(team_id) = team_id.clone() else {
                            continue;
                        };
                        let input = input.clone();
                        let root = root.clone();
                        menu.submenu_with_icon(row.icon().map(Icon::new), row.label(), window, cx, move |sub, window, cx| {
                            let input = input.clone();
                            let insert: std::rc::Rc<dyn Fn(String, &mut Window, &mut App)> =
                                std::rc::Rc::new(move |text: String, window: &mut Window, cx: &mut App| {
                                    input.update(cx, |state, cx| state.insert(text, window, cx));
                                    input.read(cx).focus_handle(cx).focus(window, cx);
                                });
                            crate::picker::search_list::menu_search_list_submenu(
                                "agent-issue-ref",
                                crate::picker::search_list::issue_ref_source(team_id.clone(), insert),
                                sub,
                                root.clone(),
                                window,
                                cx,
                            )
                        })
                    }
                    // #EXP-1434: Prompts › — the `/` pick's body at the caret.
                    Row::Prompts => {
                        let input = input.clone();
                        let root = root.clone();
                        let prompts = prompts.clone();
                        menu.submenu_with_icon(row.icon().map(Icon::new), row.label(), window, cx, move |sub, window, cx| {
                            let input = input.clone();
                            let insert: std::rc::Rc<dyn Fn(String, &mut Window, &mut App)> =
                                std::rc::Rc::new(move |text: String, window: &mut Window, cx: &mut App| {
                                    input.update(cx, |state, cx| state.insert(text, window, cx));
                                    input.read(cx).focus_handle(cx).focus(window, cx);
                                });
                            let pick: std::rc::Rc<dyn Fn(crate::slash_commands::SlashPrompt, &mut Window, &mut App)> =
                                std::rc::Rc::new(move |prompt, window: &mut Window, cx: &mut App| {
                                    crate::picker::search_list::insert_prompt_body(prompt, insert.clone(), window, cx)
                                });
                            crate::picker::search_list::menu_search_list_submenu(
                                "agent-prompts",
                                crate::picker::search_list::prompt_source(prompts.clone(), pick),
                                sub,
                                root.clone(),
                                window,
                                cx,
                            )
                        })
                    }
                    _ => menu,
                };
            }
            menu
        })
        .anchor(gpui::Anchor::BottomLeft)
        .live(content_key)
        .into_any_element()
    }

    /// #EXP-1732: THE session screen of an agent with no conversation yet
    /// (no counts, the IDE is wide).
    fn screen(&self) -> SessionScreen {
        let agent = self.agent.as_ref();
        let assistant = agent.is_some_and(AgentDefRow::is_assistant);
        let kind = session_screen_kind(agent.map(|agent| agent.id.as_str()), None, assistant);
        let open = self.settings_open.then_some(SessionScreenOpen::Settings.as_str());
        session_screen(kind.as_str(), true, Some(self.tab.as_str()), open, SessionScreenCounts::default())
    }

    /// #EXP-1732 THE header (session-screen.json), the chat's own: the
    /// agent's disc, its NAME over the subtitle, the telephone, THE `…`
    /// (#FEED-239 `sessionScreenMenu`, no gear), THE Segmented on the row.
    fn render_header(&self, screen: &SessionScreen, window: &Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        use crate::session_chat::{screen_header, screen_tabs, OnPress, OnTabPick, ScreenHeader};
        let panel = self.call_panel.read(cx).focus_handle(cx);
        let back = crate::session_screen::session_link_back(window, cx);
        let agent = self.agent.as_ref();
        let name = agent.map(agent_def_name).unwrap_or_default();
        let call = agent.map(|agent| agent_header_call(&agent.id, &name, panel, cx));
        let this = cx.entity().downgrade();
        let on_settings: OnPress = Rc::new(move |_window, cx| {
            let _ = this.update(cx, |this, cx| {
                this.settings_open = true;
                cx.notify();
            });
        });
        let menu = agent.and_then(|agent| agent_header_menu(&agent.id, on_settings, None, cx));
        let this = cx.entity().downgrade();
        let on_tab: OnTabPick = Rc::new(move |tab, _window, cx| {
            let _ = this.update(cx, |this, cx| {
                this.settings_open = false;
                this.tab = tab;
                cx.notify();
            });
        });
        let tabs = screen_tabs(&screen.tabs, screen.active, on_tab, cx);
        let title = crate::session_chat::title_row(&name, false, cx).into_any_element();
        let subtitle = crate::session_chat::screen_subtitle(None, &[], agent, cx);
        screen_header(
            ScreenHeader {
                avatar: agent_header_disc(agent, cx),
                title,
                subtitle,
                subtitle_color: None,
                trailing: call.into_iter().collect(),
                menu,
                back,
                tabs,
            },
            cx,
        )
    }

    /// #EXP-1732: the tab pane before the conversation exists: the
    /// settings (the agent's settings page); the tabs have nothing to list
    /// yet.
    fn render_pane(&mut self, screen: &SessionScreen, window: &mut Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        if screen.open != Some(SessionScreenOpen::Settings) {
            return div().flex_1().into_any_element();
        }
        let Some(agent_id) = self.agent.as_ref().map(|agent| agent.id.clone()) else {
            return div().flex_1().into_any_element();
        };
        let pane = match self.settings.clone() {
            Some(pane) => pane,
            None => {
                let nav = self.nav.clone();
                let shared = crate::sidebar::rail_shared_for_window(window, cx);
                let pane = cx.new(|cx| crate::settings::agent_detail::AgentDetailPane::for_agent(agent_id, nav, shared, window, cx));
                self.settings = Some(pane.clone());
                pane
            }
        };
        crate::scroll_pane::v_scroll_pane(
            "agent-settings-scroll",
            &self.settings_scroll,
            div().w_full().px_6().py_4().child(pane),
        )
        .into_any_element()
    }
}

impl Focusable for AgentScreenView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl AgentScreenView {
    /// #FEED-191: the conversation's chat view (root id, view), once it has one.
    pub(crate) fn chat_view(&self) -> Option<&(String, Entity<crate::session_chat::SessionChatScreenView>)> {
        self.chat.as_ref()
    }
}

impl Render for AgentScreenView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "AgentScreenView").entered();
        let editor = matches!(self.subject, ChannelSubject::App(_));
        // #EXP-1793: the first composer's "+" › MCP servers.
        self.ensure_mcp_loaded(cx);
        // S-3: one session screen. An agent's conversation renders on THE
        // chat screen (chat bubbles, its header, composer with the "+",
        // selection, Dock); this view keeps only the "no conversation yet"
        // page, whose first send = sessions.send({agentId}).
        if !editor {
            // #FEED-239: a row's `…` › settings opened this agent's chat.
            let asked = self
                .agent
                .as_ref()
                .map(|agent| agent.id.clone())
                .is_some_and(|agent_id| take_settings_request(&agent_id, cx));
            if let Some(root_id) = self.conversation.as_ref().map(|row| row.id.clone()) {
                if self.chat.as_ref().map(|(id, _)| id.as_str()) != Some(root_id.as_str()) {
                    let id = root_id.clone();
                    let widget = self.widget;
                    let view = cx.new(|cx| {
                        let mut view = crate::session_chat::SessionChatScreenView::new(id, window, cx);
                        view.set_widget(widget);
                        view
                    });
                    self.chat = Some((root_id, view));
                }
                if let Some((_, chat)) = self.chat.as_ref() {
                    if asked {
                        chat.update(cx, |chat, cx| {
                            chat.open_item(crate::session_chat::ChatOpened::Settings, window, cx)
                        });
                    }
                    return v_flex().track_focus(&self.focus_handle).size_full().min_h_0().child(chat.clone());
                }
            } else {
                self.chat = None;
                if asked {
                    self.settings_open = true;
                }
            }
        }
        if !self.has_subject() {
            // VAPP-110: the editor screen owns the app's not-found verdict.
            if editor {
                return v_flex().track_focus(&self.focus_handle).size_full();
            }
            // The agents synced, this one is not among them, and the grace
            // passed (deleted, a lost membership, a foreign id).
            let ready = Store::try_global(cx).is_some_and(|store| store.collections().agents.read(cx).is_ready());
            if crate::session_chat::chat_not_found(ready, false, self.opened_at.elapsed()) {
                return v_flex()
                    .track_focus(&self.focus_handle)
                    .size_full()
                    .px_4()
                    .items_center()
                    .justify_center()
                    .child(crate::controls::empty_state(
                        Icon::new(registry::SETTINGS_AGENTS),
                        AGENT_NOT_FOUND,
                        Some(crate::empty_state_copy::CHAT_NOT_FOUND_DESCRIPTION.into()),
                        cx,
                    ));
            }
            // Skeletons, never "Loading…" (feedback-states.json).
            return v_flex()
                .track_focus(&self.focus_handle)
                .size_full()
                .px(px(HEADER_INSET))
                .pt_6()
                .child(crate::feedback::skeleton_rows("agent-loading", 3, 36., cx));
        }
        // #EXP-1732: an editor's chat once its root exists = THE root's own
        // session view (the app editor's chat pane, headerless).
        if let Some((_, view)) = self.root_view.clone() {
            return v_flex()
                .track_focus(&self.focus_handle)
                .size_full()
                .min_h_0()
                .overflow_hidden()
                .child(crate::session_chat::session_pane(&view, cx));
        }
        // No conversation yet: the empty chat over THE Dock with the first
        // composer, whose send opens the conversation.
        let card = self.render_first_composer(cx);
        let band = crate::dock::band(
            &self.dock,
            crate::dock::Band { id: "agent".into(), rows: Vec::new(), actions: Vec::new(), composer: Some(card) },
            window,
            cx,
        );
        // VAPP-110: an editor chat with nothing in it yet = the fixture's
        // `chatEmpty` (the list-size empty state).
        // VAPP-176: a draft's = the starters, the composer focused once.
        let starters = editor && self.is_empty_draft();
        if starters && !self.draft_focused {
            self.draft_focused = true;
            self.first.input.update(cx, |input, cx| input.focus(window, cx));
        }
        let chat_empty = editor.then(|| {
            if starters {
                return self.render_starters(cx);
            }
            crate::controls::empty_state_list(
                Icon::new(registry::NAV_APPS),
                domain::apps::APPS.copy.chat_empty.clone(),
                cx,
            )
            .py_12()
        });
        // EXP-1557 (web parity): the empty state sits CENTRED in the pane
        // above THE Dock, never pinned to its top. #EXP-1782: the band is a
        // flow child under it, so the pane ends at the band's top edge.
        let conversation = div()
            .flex_1()
            .min_h_0()
            .w_full()
            .flex()
            .flex_col()
            .justify_center()
            .pt_8()
            .pb(px(crate::dock::DOCK_GAP))
            .children(chat_empty)
            .into_any_element();
        let chat = div()
            .id(if editor { "app-editor-chat-scroll" } else { "agent-conversation-scroll" })
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(conversation)
            .children(band)
            // S-3: the call panel covers the body while the call runs.
            .child(self.call_panel.clone())
            .into_any_element();
        // VAPP-110: an editor's chat is the app editor's chat pane, headerless
        // (#EXP-1837: so is the Assistant widget's).
        if editor || self.widget {
            return v_flex().track_focus(&self.focus_handle).size_full().min_h_0().overflow_hidden().child(chat);
        }
        // #EXP-1732: THE session screen's chrome over the empty chat.
        let screen = self.screen();
        let header = self.render_header(&screen, window, cx);
        let body = match screen.layout {
            SessionScreenLayout::Chat => chat,
            _ => {
                let pane = self.render_pane(&screen, window, cx);
                // #FEED-239: THE collapse closes the right pane.
                let this = cx.entity().downgrade();
                let on_collapse: crate::session_chat::OnPress = Rc::new(move |_window, cx| {
                    let _ = this.update(cx, |this, cx| {
                        this.settings_open = false;
                        this.tab = SessionScreenTab::Chat;
                        cx.notify();
                    });
                });
                crate::session_chat::screen_beside(chat, pane, on_collapse, cx)
            }
        };
        v_flex()
            .track_focus(&self.focus_handle)
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .child(header)
            .child(body)
    }
}


/// #EXP-1793 (composer-menu.json `mcp-servers`): THE search list of multi
/// picker rows over the agent's team's servers (the server's mark, its
/// name, the check trailing); a server the viewer has not connected reads
/// so and opens Settings › MCP servers instead of toggling.
fn first_mcp_source(
    state: std::rc::Rc<dyn Fn(&App) -> (Vec<FirstMcpServer>, Vec<String>)>,
    view: gpui::WeakEntity<AgentScreenView>,
) -> crate::picker::search_list::MenuListSource {
    use crate::picker::search_list;
    use gpui::prelude::FluentBuilder as _;
    let rows_state = state.clone();
    search_list::MenuListSource {
        placeholder: "Search",
        rows: std::rc::Rc::new(move |query: &str, _window: &mut Window, cx: &mut App| {
            let (servers, picked) = rows_state(cx);
            let query = query.trim().to_lowercase();
            servers
                .iter()
                .filter(|server| query.is_empty() || server.name.to_lowercase().contains(&query))
                .map(|server| {
                    let on = picked.contains(&server.id);
                    let icon = crate::picker::mcp_server_picker::mcp_server_icon(server.url.as_deref(), server.command.as_deref());
                    // The launch picker's second line: the not-connected
                    // reason, else the host (or the command).
                    let caption = if server.blocked {
                        Some(crate::launch_options::MCP_CONNECT_FIRST.to_string())
                    } else {
                        crate::picker::mcp_server_picker::mcp_host(server.url.as_deref())
                            .or_else(|| server.command.clone().filter(|command| !command.is_empty()))
                    };
                    search_list::MenuListRow {
                        id: server.id.clone(),
                        body: h_flex()
                            .flex_1()
                            .min_w_0()
                            .items_center()
                            .gap_2()
                            .child(Icon::from(icon).size(px(16.)).flex_shrink_0().text_color(cx.theme().muted_foreground))
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .child(div().truncate().text_color(cx.theme().foreground).child(SharedString::from(server.name.clone())))
                                    .children(caption.map(|caption| div().truncate().text_xs().text_color(cx.theme().muted_foreground).child(caption))),
                            )
                            .when(on, |row| row.child(search_list::row_check(cx)))
                            // The launch picker dims a server not connected yet.
                            .when(server.blocked, |row| row.opacity(0.5))
                            .into_any_element(),
                        disabled: false,
                    }
                })
                .collect()
        }),
        pick: std::rc::Rc::new(move |id: &str, window: &mut Window, cx: &mut App| {
            let (servers, picked) = state(cx);
            let Some(server) = servers.iter().find(|server| server.id == id) else { return false };
            if server.blocked && !picked.contains(&server.id) {
                crate::navigation::navigate(window, cx, crate::navigation::Screen::Settings);
                crate::sidebar::select_settings_section(window, cx, crate::settings::SettingsSection::Mcp);
                return true;
            }
            let mut next = picked;
            match next.iter().position(|picked| picked == id) {
                Some(at) => {
                    next.remove(at);
                }
                None => next.push(id.to_string()),
            }
            let next: Vec<String> = servers.iter().filter(|server| next.contains(&server.id)).map(|server| server.id.clone()).collect();
            if let Some(view) = view.upgrade() {
                view.update(cx, |this, cx| {
                    this.first.mcp_picked = Some(next);
                    cx.notify();
                });
            }
            false
        }),
        empty: std::rc::Rc::new(|_query: &str, _cx: &App| "No servers found.".into()),
        loading: std::rc::Rc::new(|_: &App| false),
        footer: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_placeholder_names_the_agent() {
        assert_eq!(message_placeholder("Triage"), "Message Triage");
        let agent: AgentDefRow = serde_json::from_value(serde_json::json!({"id": "a", "name": " Triage "})).unwrap();
        assert_eq!(agent_def_name(&agent), "Triage");
        let unnamed: AgentDefRow = serde_json::from_value(serde_json::json!({"id": "a"})).unwrap();
        assert_eq!(agent_def_name(&unnamed), domain::session_title::copy::UNTITLED);
    }
}
