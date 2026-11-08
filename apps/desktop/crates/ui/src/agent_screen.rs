//! EXP-1294 / NEW-6 — an AGENT's page ([`Screen::Agent`], web
//! `agents/$agentId.tsx`): the screen id IS the `agents` row; the page shows
//! the CALLER's own conversation with it (a root session with that
//! `agent_id` and `user_id` = me). No conversation yet = the same screen,
//! empty, whose composer opens one with `sessions.send({agentId})`. The
//! header is the `agents` row's tinted icon and name; the conversation is
//! its messages on THE chat's painters (web `ChatConversation`: a
//! person's bubble right with `name · HH:MM`, the agent's plain prose; an
//! agent conversation = the chat bubbles), every `thread_started` (wire kind) / `session` card THE session
//! card (a started chat opens as the chat, a session as the session
//! view), question and plan cards answered in place; under it THE session
//! composer of the session view targeting the conversation root
//! (`session_screen::root_composer_view`: model pin, context ring, `/` menu,
//! Send⇄Stop over the tree, the queue strip), `Message <name>`.
//!
//! VAPP-110: the same channel is an app EDITOR's chat
//! ([`ChannelSubject::App`], embedded in [`crate::app_screen`] without the
//! header): the member's editor session (a root with that `app_id` and
//! `user_id` = me), the app's glyph as the agent's mark, the first send
//! `sessions.send({appId, text, deviceId})` on EDITOR DEVICE (#FEED-204).
//! VAPP-176: a DRAFT's empty chat = the page-size EmptyState `starterTitle`
//! whose `starters` pills SEND their text through that same first send (and,
//! VAPP-234, a `Start from a template` pill opens the import dialog), the
//! composer focused with `draftPlaceholder`.

use std::collections::HashSet;
use std::time::Instant;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, AnyElement, App, AppContext as _, ClickEvent, Entity, FocusHandle,
    Focusable, FollowMode, FontWeight, InteractiveElement as _, IntoElement, ListAlignment, ListState,
    ParentElement, Render, SharedString, Styled, Subscription, Window,
};
use gpui_component::input::{InputEvent, TextareaState};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _};
use sync::Store;

use domain::rows::{AgentDefRow, AppRow, MessageRow, SessionRow};
use domain::session_tree::SessionChatRow;

use crate::composer_images::{self, PendingImages};
use crate::icons::registry;
use crate::markdown::ImageCache;
use crate::transcript_rows::{plan_list_sync, ListOp, RowKey, WORKING_ROW_ID};

/// The channel's reading column: THE chat column (#EXP-1547), the Dock's.
const CHANNEL_W: f32 = crate::work_header::CHAT_COLUMN_W;
/// The header's side inset.
const HEADER_INSET: f32 = 44.;
/// The header's lead disc and the glyph in it (web `disc(`size-8`, `size-4`)`).
const HEADER_DISC_PX: f32 = 32.;
const HEADER_GLYPH_PX: f32 = 16.;

/// #EXP-1380: how far beyond the viewport the conversation renders and
/// measures rows (the chat's `CONVERSATION_OVERDRAW`).
const CHANNEL_OVERDRAW: gpui::Pixels = px(1024.);

/// #EXP-1380: the conversation list's projection (the chat screen's
/// recipe): the window over the rows, what the list was last told.
#[derive(Default)]
struct ChannelRows {
    /// The window's first row in the model's rows.
    start: usize,
    /// Whether the Working… tail is the last list row.
    tail: bool,
    keys: Vec<RowKey>,
    window: crate::chat_transcript::TranscriptWindow,
    /// The indexes (into the rows) of WAITING plan / question cards.
    waiting: Vec<usize>,
    /// What the keys were built for (rows generation, tail, start).
    synced: Option<(u64, bool, usize)>,
    /// #EXP-1736: the window's rows under transcript-grouping.json.
    layouts: Vec<domain::transcript_grouping::TranscriptRowLayout>,
}

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
fn agent_header_disc(agent: Option<&AgentDefRow>, cx: &App) -> AnyElement {
    let glyph = match agent {
        Some(agent) if agent.is_assistant() => Icon::from(crate::icons::ExpIcon::Logo)
            .with_size(px(HEADER_GLYPH_PX))
            .text_color(cx.theme().foreground)
            .into_any_element(),
        _ => agent_def_icon(agent).with_size(px(HEADER_GLYPH_PX)).into_any_element(),
    };
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

/// The agent's `…` (`agentMenuEntries`); none without entries.
fn agent_header_menu(agent_id: &str, cx: &App) -> Option<AnyElement> {
    if crate::chat_actions::agent_entries(agent_id, cx).is_empty() {
        return None;
    }
    let agent_id = agent_id.to_string();
    let trigger = crate::controls::ghost_icon_button("agent-header-more", Icon::new(registry::UI_MORE), cx)
        .accessibility_label("More");
    Some(
        div()
            .flex_shrink_0()
            .mr(px(-crate::work_header::GHOST_ICON_HANG))
            .child(crate::controls::PointerMenu::for_button(trigger, move |menu, _, cx| {
                crate::chat_actions::agent_menu_items(menu, &agent_id, cx)
            }))
            .into_any_element(),
    )
}

/// NEW-55 (voice-call.json): THE telephone, left of `…`, the header's icon
/// button: disabled with the reason as its tooltip while a call cannot start
/// here (no voice device, not this machine, no voice yet, no usable GPU, a
/// call this client does not run). S-3: while THIS client runs the call with
/// this agent it stays enabled and focuses the call panel; a new call focuses
/// it too (Esc there ends it).
fn agent_header_call(agent_id: &str, name: &str, panel: FocusHandle, cx: &App) -> AnyElement {
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

/// EXP-1686 (chats.json AGENT CHAT): THE agent chat header, conversation or
/// not yet: the lead disc, the agent's NAME, then `merge` (a conversation's
/// open PR), the call button and the agent menu; `back` over it when a session
/// link opened the chat (chats.json THE WAY BACK); `strip` only while the chat
/// holds a plan.
pub(crate) fn agent_header(
    agent: Option<&AgentDefRow>,
    merge: Option<AnyElement>,
    back: Option<AnyElement>,
    strip: Option<AnyElement>,
    call_panel: FocusHandle,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let name = agent.map(agent_def_name).unwrap_or_default();
    let call = agent.map(|agent| agent_header_call(&agent.id, &name, call_panel, cx));
    let menu = agent.and_then(|agent| agent_header_menu(&agent.id, cx));
    // The rule spans the content card (web `border-b`), like the chat's.
    v_flex()
        .w_full()
        .child(
            v_flex()
                .w_full()
                .px(px(HEADER_INSET))
                .pt_6()
                .pb_3()
                .gap_4()
                .child(
                    v_flex()
                        .w_full()
                        .min_w_0()
                        .gap_1()
                        .children(back)
                        .child(
                            h_flex()
                                .w_full()
                                .min_w_0()
                                .gap_3()
                                .items_center()
                                .child(agent_header_disc(agent, cx))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_2xl()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(theme.foreground)
                                        .child(SharedString::from(name)),
                                )
                                .children(merge)
                                .children(call)
                                .children(menu),
                        ),
                )
                .children(strip),
        )
        .child(div().w_full().h(px(1.)).bg(theme.border))
        .into_any_element()
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
    /// #EXP-1380: shared, never deep-cloned per frame.
    rows: std::rc::Rc<Vec<SessionChatRow>>,
    /// Bumped on every [`Self::refresh`]: what the list sync diffs on.
    rows_gen: u64,
    messages: std::collections::HashMap<String, MessageRow>,
    images: Entity<ImageCache>,
    /// THE session composer, targeting the conversation root (keyed by its
    /// id); `None` until the conversation exists.
    composer: Option<(String, Entity<crate::steer_viewer::SteerSessionView>)>,
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
    /// #EXP-1380 PERF: the virtualised conversation (`gpui::list` renders
    /// and measures only the rows in and around the viewport);
    /// [`FollowMode::Tail`] = EXP-1316 (1n)'s follow (opens on the newest
    /// row, pinned until the reader scrolls up).
    list: ListState,
    conv: ChannelRows,
    opened_at: Instant,
    focus_handle: FocusHandle,
    /// S-3: the in-window call panel over the conversation's body (empty
    /// unless THIS client runs a call with the agent).
    call_panel: Entity<etalk_gpui::voice_call::CallPanel>,
    /// #EXP-1714: the window's navigation, whose ACTIVE team scopes a global
    /// Assistant's prompts, issue refs and mentions.
    nav: Entity<crate::navigation::Navigation>,
    _subscriptions: Vec<Subscription>,
}

impl AgentScreenView {
    pub fn new(agent_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::for_subject(ChannelSubject::Agent(agent_id), window, cx)
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
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        )];
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
        }
        let transport = crate::queries::attachment_transport(cx);
        let images = cx.new(|_| ImageCache::new(transport));
        // Picture cards repaint when their bytes land (the cache notifies).
        subscriptions.push(cx.observe(&images, |_, _, cx| cx.notify()));
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
            rows: Default::default(),
            rows_gen: 0,
            messages: std::collections::HashMap::new(),
            images,
            composer: None,
            chat: None,
            first: FirstComposer {
                input,
                mention,
                images: PendingImages::default(),
                sending: false,
                notice: None,
                mention_team: None,
                teams_picked: None,
            },
            dock: cx.new(|_| crate::dock::DockState::default()),
            placeholder_name: None,
            draft_focused: false,
            refresh_queued: false,
            list: {
                let list = ListState::new(0, ListAlignment::Top, CHANNEL_OVERDRAW);
                list.set_follow_mode(FollowMode::Tail);
                list
            },
            conv: ChannelRows::default(),
            opened_at: Instant::now(),
            focus_handle: cx.focus_handle(),
            call_panel: cx.new(etalk_gpui::voice_call::CallPanel::new),
            nav: crate::navigation::nav_for_window(window, cx),
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
        let name = self.subject_name();
        let placeholder = self.placeholder();
        // THE session composer of the conversation, built once it exists
        // (and rebuilt should it ever be another row).
        match &self.conversation {
            Some(conversation)
                if self.composer.as_ref().map(|(id, _)| id.as_str()) != Some(conversation.id.as_str()) =>
            {
                let composer = crate::session_screen::root_composer_view(
                    &conversation.id,
                    placeholder.clone(),
                    crate::steer_viewer::ComposerIds::new(
                        "agent-send",
                        "agent-attach",
                        "agent-model",
                        "agent-context",
                    ),
                    window,
                    cx,
                );
                // EXP-1650: the editor's chat shows only "Session ended".
                if matches!(self.subject, ChannelSubject::App(_)) {
                    composer.update(cx, |composer, cx| composer.hide_empty_replay(cx));
                }
                self._subscriptions.push(cx.observe(&composer, |_, _, cx| cx.notify()));
                self.composer = Some((conversation.id.clone(), composer));
                // #EXP-1380 WINDOW: another conversation opens at its newest
                // window.
                self.conv.window = Default::default();
            }
            Some(_) => {}
            None => self.composer = None,
        }
        if let Some((_, composer)) = &self.composer {
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
        let conversation_id = self.conversation.as_ref().map(|row| row.id.clone());
        // The conversation's own card leads the contract's rows; the header
        // already names the agent.
        let editor = matches!(self.subject, ChannelSubject::App(_));
        // #EXP-1380: the rows and the messages off ONE scan.
        let (rows, messages) = conversation_id
            .as_deref()
            .and_then(|id| {
                let now = crate::session_data::now_ms();
                // VAPP-110: an editor's chat is its whole tree (a
                // revived root keeps its rows, FEED-134).
                crate::session_data::chat_view_and_messages(id, None, now, editor, cx).map(|(view, messages)| {
                    (view.rows.into_iter().filter(|row| row.id() != id).collect(), messages)
                })
            })
            .unwrap_or_default();
        self.rows = std::rc::Rc::new(rows);
        self.conv.waiting = crate::chat_transcript::waiting_indexes(self.rows.iter());
        self.rows_gen = self.rows_gen.wrapping_add(1);
        self.messages = messages.into_iter().map(|message| (message.id.clone(), message)).collect();
        let _ = name;
        // Session 58cb32df: a surface card arriving while the chat is open
        // plays its entrance once.
        crate::surface_card::sync_rows(cx.entity_id(), &self.rows, cx);
        if self.has_subject() && self.placeholder_name.as_deref() != Some(placeholder.as_str()) {
            if let Some((_, composer)) = &self.composer {
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
    pub(crate) fn editor_header(&self) -> Option<domain::apps::EditorHeader> {
        if !matches!(self.subject, ChannelSubject::App(_)) {
            return None;
        }
        let root = self.conversation.as_ref().map(|root| domain::apps::EditorHeaderRoot {
            status: root.status.as_deref().unwrap_or_default(),
            busy: root.busy,
            needs_input: root.needs_input,
        });
        let device = self.editor_device.as_ref().and_then(|device| device.header_device());
        Some(domain::apps::editor_header(root, device.as_ref().map(|(name, online)| (name.as_str(), *online))))
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
                        composer_images::settle_sent(&this.first.input, &mut this.first.images, &sent, window, cx);
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
        let conditions = ComposerMenuConditions {
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
            hasher.finish()
        };
        let teams_view = view.clone();
        let teams_change = self.teams_change(cx);
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

    fn render_row(
        &self,
        row: &SessionChatRow,
        layout: Option<&domain::transcript_grouping::TranscriptRowLayout>,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let team_id = self.subject_team_id(cx);
        let conversation_id = self.conversation.as_ref().map(|row| row.id.clone()).unwrap_or_default();
        // FEED-140: the inline cards answer while the reader may reply.
        let answer = self.composer.as_ref().is_some_and(|(_, composer)| composer.read(cx).may_message());
        let text = crate::session_chat::ChatText::team(team_id.as_deref()).answering(answer);
        // #EXP-1736: ONE transcript for every chat: a message (or a
        // report) = THE bubble under its sender, every card as it was.
        let at = self.messages.get(row.id()).and_then(|message| message.created_at.as_deref());
        let content = match crate::session_chat::transcript_message(row, layout, "agent-message", at, &text, &self.images, cx) {
            Some(bubble) => bubble,
            None => crate::session_chat::card_row(row, &conversation_id, &self.rows, &text, &self.images, window, cx)
                .unwrap_or_else(|| div().into_any_element()),
        };
        crate::session_chat::sender_row(layout, content, cx)
    }

    /// Session 58cb32df (chats.json `agentMenuCases`): the agent chat
    /// header's `…` = THE agent menu (Compact conversation, New
    /// conversation, Stop while busy, Agent settings for owners).
    fn render_header(&self, window: &Window, cx: &App) -> AnyElement {
        let panel = self.call_panel.read(cx).focus_handle(cx);
        let back = crate::session_screen::session_link_back(window, cx);
        agent_header(self.agent.as_ref(), None, back, None, panel, cx)
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
        let editor = matches!(self.subject, ChannelSubject::App(_));
        // S-3: one session screen. An agent's conversation renders on THE
        // chat screen (chat bubbles, its header, composer with the "+",
        // selection, Dock); this view keeps only the "no conversation yet"
        // page, whose first send = sessions.send({agentId}).
        if !editor {
            if let Some(root_id) = self.conversation.as_ref().map(|row| row.id.clone()) {
                if self.chat.as_ref().map(|(id, _)| id.as_str()) != Some(root_id.as_str()) {
                    let id = root_id.clone();
                    let view = cx.new(|cx| crate::session_chat::SessionChatScreenView::new(id, window, cx));
                    self.chat = Some((root_id, view));
                }
                if let Some((_, chat)) = self.chat.as_ref() {
                    return v_flex().track_focus(&self.focus_handle).size_full().min_h_0().child(chat.clone());
                }
            } else {
                self.chat = None;
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
        // N-12: THE Dock (dock.json ×4) floats over the conversation's
        // bottom edge: its waiting cards, the root's Queue, queued messages
        // and banners, then THE composer; a reader who may not reply keeps
        // the cards inline. No conversation yet = the same composer, whose
        // send opens it.
        let team_id = self.subject_team_id(cx);
        let composer_view = self.composer.as_ref().map(|(_, composer)| composer.clone());
        let text = crate::session_chat::ChatText::team(team_id.as_deref());
        let rows_snapshot = self.rows.clone();
        let (band, sheet, docked) = match composer_view.filter(|composer| composer.read(cx).may_message()) {
            Some(composer) => {
                // S-3: a waiting plan / question stays inline in
                // the conversation (with its actions), never docked.
                let docked: HashSet<String> = HashSet::new();
                let (band, sheet) = crate::session_chat::root_dock(
                    "agent",
                    &rows_snapshot,
                    None,
                    &self.dock,
                    &composer,
                    None,
                    &text,
                    window,
                    cx,
                );
                (band, sheet, docked)
            }
            None if self.composer.is_none() => {
                let card = self.render_first_composer(cx);
                let band = crate::dock::band(
                    &self.dock,
                    crate::dock::Band { id: "agent".into(), rows: Vec::new(), actions: Vec::new(), composer: Some(card) },
                    window,
                    cx,
                );
                (band, None, HashSet::new())
            }
            None => (None, None, HashSet::new()),
        };
        let band_height = if band.is_some() { self.dock.read(cx).band_height() } else { 0. };
        let _ = docked;
        self.sync_list(cx);
        // VAPP-110: an editor chat with nothing in it yet = the fixture's
        // `chatEmpty` (the list-size empty state).
        // VAPP-176: a draft's = the starters, the composer focused once.
        let starters = editor && rows_snapshot.is_empty() && self.is_empty_draft();
        if starters && !self.draft_focused {
            self.draft_focused = true;
            self.first.input.update(cx, |input, cx| input.focus(window, cx));
        }
        let chat_empty = (editor && rows_snapshot.is_empty()).then(|| {
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
        let conversation: AnyElement = match chat_empty {
            // EXP-1557 (web parity): the empty state sits CENTRED in the
            // pane above the floating Dock, never pinned to its top.
            Some(empty) => div()
                .flex_1()
                .min_h_0()
                .w_full()
                .flex()
                .flex_col()
                .justify_center()
                .pt_8()
                .pb(px(band_height.max(16.) + crate::dock::DOCK_GAP))
                .child(empty)
                .into_any_element(),
            None => {
                let list = gpui::list(
                    self.list.clone(),
                    cx.processor(|this, ix: usize, window, cx| this.render_list_row(ix, window, cx)),
                )
                .pt_8()
                .pb(px(band_height.max(16.) + crate::dock::DOCK_GAP));
                // EXP-1228: the transcript's right-click menu (Copy).
                crate::steer_viewer::with_transcript_menu(crate::scroll_pane::v_list_pane(list, &self.list))
                    .into_any_element()
            }
        };
        v_flex()
            .track_focus(&self.focus_handle)
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .when(!editor, |this| this.child(self.render_header(window, cx)))
            .child(
                div()
                    .id(if editor { "app-editor-chat-scroll" } else { "agent-conversation-scroll" })
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(conversation)
                    .children(band)
                    .child(crate::chat_transcript::jump_overlay(
                        "agent-conversation-scroll-jump-to-bottom",
                        &self.list,
                        self.conv.keys.len(),
                        band_height,
                        cx.entity_id(),
                        cx,
                    ))
                    .children(sheet)
                    // S-3: the call panel covers the body while the call runs.
                    .child(self.call_panel.clone()),
            )
    }
}

impl AgentScreenView {
    /// #EXP-1380: tell the list what changed — only when the rows, the
    /// Working… tail or the window moved, never per frame. Rows key by
    /// ABSOLUTE position with a content fingerprint, so an append is one
    /// splice, `Load earlier` a front splice, and a changed row re-measures.
    fn sync_list(&mut self, cx: &App) {
        let tail = self.conversation.as_ref().is_some_and(|row| !row.is_ended()) && self.composer.is_some();
        let total = self.rows.len();
        let start = self.conv.window.start(total, &self.conv.waiting);
        // #EXP-1464: an expanded / collapsed session row re-measures.
        let inputs = (self.rows_gen ^ crate::session_chat::chat_row_generation(cx).rotate_left(32), tail, start);
        if self.conv.synced == Some(inputs) {
            return;
        }
        self.conv.synced = Some(inputs);
        self.conv.start = start;
        self.conv.tail = tail;
        let root_id = self.conversation.as_ref().map(|row| row.id.clone()).unwrap_or_default();
        // #EXP-1736: ONE transcript: who sent each row, the groups, the gaps.
        let title = self.agent.as_ref().map(agent_def_name).unwrap_or_default();
        let agent_id = self
            .conversation
            .as_ref()
            .and_then(|row| row.agent_id.clone())
            .or_else(|| self.agent.as_ref().map(|agent| agent.id.clone()));
        let mut input = crate::session_chat::conversation_input(
            self.rows[start..].iter(),
            &root_id,
            &title,
            agent_id.as_deref(),
            None,
            start == 0,
            cx,
        );
        input.items = self.rows[start..].iter().map(crate::session_chat::transcript_item).collect();
        self.conv.layouts = domain::transcript_grouping::transcript_layout(&input);
        let layouts = &self.conv.layouts;
        let mut keys: Vec<RowKey> = self.rows[start..]
            .iter()
            .enumerate()
            .map(|(offset, row)| {
                let at = match row {
                    SessionChatRow::Message { id, .. } => {
                        self.messages.get(id).and_then(|message| message.created_at.as_deref())
                    }
                    _ => None,
                };
                let mut fingerprint = crate::session_chat::conversation_fingerprint(row, at, 0)
                    ^ crate::session_chat::row_state_fingerprint(row, &root_id, cx);
                if offset == 0 && start > 0 {
                    fingerprint = fingerprint.rotate_left(1) ^ 1;
                }
                if let Some(layout) = layouts.get(offset) {
                    fingerprint ^= crate::session_chat::layout_fingerprint(layout);
                }
                RowKey { id: (start + offset) as u64, fingerprint }
            })
            .collect();
        if tail {
            keys.push(RowKey { id: WORKING_ROW_ID, fingerprint: 0 });
        }
        let ops = plan_list_sync(&self.conv.keys, &keys);
        // #EXP-1380 FOLLOW: an arrival keeps a reader inside the slack on
        // the tail.
        crate::chat_transcript::rearm_on_arrival(&self.list, &ops);
        for op in ops {
            match op {
                ListOp::Splice { range, count } => self.list.splice(range, count),
                ListOp::Remeasure(range) => self.list.remeasure_items(range),
            }
        }
        self.conv.keys = keys;
        let _ = cx;
    }

    /// One conversation list row: the reading column, the row gap above
    /// every row but the first; `Load earlier` over the window's first row;
    /// the last row = the run view's ONE Working… line while the chat works
    /// (#EXP-1380 WORKING).
    fn render_list_row(&mut self, ix: usize, window: &mut Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        let rows = self.rows.clone();
        // #EXP-1736: the grouping's gap above every row (the Working… tail
        // starts a group of its own).
        let layout = self.conv.layouts.get(ix).cloned();
        let gap = match (&layout, ix) {
            (_, 0) => 0.,
            (Some(layout), _) => layout.gap,
            (None, _) => crate::session_chat::transcript_sizes().gap_between,
        };
        let content = match rows.get(self.conv.start + ix) {
            Some(row) if ix < self.conv.keys.len() && self.conv.keys[ix].id != WORKING_ROW_ID => {
                Some(self.render_row(row, layout.as_ref(), window, cx))
            }
            _ => self
                .composer
                .as_ref()
                .map(|(_, composer)| composer.clone())
                .and_then(|composer| composer.update(cx, |composer, cx| composer.live_tail(window, cx)))
                .map(|tail| div().w_full().child(tail).into_any_element()),
        };
        let Some(content) = content else {
            return div().into_any_element();
        };
        let earlier = (ix == 0 && self.conv.start > 0).then(|| {
            crate::chat_transcript::load_earlier_button(
                cx.listener(|this, _: &ClickEvent, _window, cx| {
                    this.conv.window.load_earlier();
                    this.list.pause_following_tail();
                    cx.notify();
                }),
                cx,
            )
        });
        div()
            .w_full()
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(CHANNEL_W))
                    .mx_auto()
                    .px(px(crate::work_header::WORK_GUTTER))
                    .pt(px(gap))
                    .children(earlier)
                    .child(content),
            )
            .into_any_element()
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
