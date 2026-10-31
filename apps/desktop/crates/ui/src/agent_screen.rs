//! EXP-1294 / NEW-6 — an AGENT's page ([`Screen::Agent`], web
//! `agents/$agentId.tsx`): the screen id IS the `agents` row; the page shows
//! the CALLER's own conversation with it (a root session with that
//! `agent_id` and `user_id` = me). No conversation yet = the same screen,
//! empty, whose composer opens one with `sessions.send({agentId})`. The
//! header is the `agents` row's tinted icon and name; the conversation is
//! its messages as COMMENT ROWS
//! ([`crate::comments::message_row`]: avatar or the agent's mark · author ·
//! `HH:MM` · markdown), every `thread_started` / `session` card THE session
//! card (a started thread opens as the thread, a session as the session
//! view), question and plan cards answered in place; under it THE session
//! composer of the session view targeting the conversation root
//! (`session_screen::root_composer_view`: model pin, context ring, `/` menu,
//! Send⇄Stop over the tree, the queue strip), `Message <name>`.
//!
//! VAPP-110: the same channel is an app EDITOR's chat
//! ([`ChannelSubject::App`], embedded in [`crate::app_screen`] without the
//! header): the member's editor session (a root with that `app_id` and
//! `user_id` = me), the app's glyph as the agent's mark, the first send
//! `sessions.send({appId, text, deviceId})` on the caller's default device.
//! VAPP-176: a DRAFT's empty chat = the page-size EmptyState `starterTitle`
//! whose `starters` pills SEND their text through that same first send, the
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
use domain::session_tree::SessionThreadRow;

use crate::composer_images::{self, PendingImages};
use crate::icons::registry;
use crate::markdown::ImageCache;
use crate::transcript_rows::{plan_list_sync, ListOp, RowKey, WORKING_ROW_ID};

/// The channel's reading column.
const CHANNEL_W: f32 = 1040.;
/// The header's side inset.
const HEADER_INSET: f32 = 44.;
/// The header icon.
const HEADER_ICON_PX: f32 = 28.;
/// A message row's marker.
const MARKER_PX: f32 = 24.;
/// A card row lines up with the comment cards: the timeline gutter (28px)
/// plus its gap.
const CARD_INSET: f32 = 36.;

/// #EXP-1380: how far beyond the viewport the conversation renders and
/// measures rows (the thread's `CONVERSATION_OVERDRAW`).
const CHANNEL_OVERDRAW: gpui::Pixels = px(1024.);

/// #EXP-1380: the conversation list's projection (the thread screen's
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
}

pub struct AgentScreenView {
    subject: ChannelSubject,
    agent: Option<AgentDefRow>,
    /// VAPP-110: the app an editor chat belongs to.
    app: Option<AppRow>,
    /// VAPP-110: the device an editor chat's first send runs on (the
    /// caller's default launch device).
    device_id: Option<String>,
    /// The caller's own conversation session with the agent (a root with
    /// that `agent_id` and `user_id` = me); `None` before the first message.
    conversation: Option<SessionRow>,
    /// #EXP-1380: shared, never deep-cloned per frame.
    rows: std::rc::Rc<Vec<SessionThreadRow>>,
    /// Bumped on every [`Self::refresh`]: what the list sync diffs on.
    rows_gen: u64,
    messages: std::collections::HashMap<String, MessageRow>,
    images: Entity<ImageCache>,
    /// THE session composer, targeting the conversation root (keyed by its
    /// id); `None` until the conversation exists.
    composer: Option<(String, Entity<crate::steer_viewer::SteerSessionView>)>,
    first: FirstComposer,
    /// S-3: an agent's conversation, once it exists, IS a thread: THE thread
    /// screen over its root (chat mode `bubbles`); this view is then only
    /// its wrapper. Keyed by the root id.
    thread: Option<(String, Entity<crate::thread_screen::ThreadScreenView>)>,
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
            // NEW-8: a message's session is its conversation's.
            subscriptions.push(cx.observe_in(&collections.conversations, window, |this, _, window, cx| {
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
                .timer(crate::thread_screen::MISSING_GRACE)
                .await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
        let mut this = Self {
            subject,
            agent: None,
            app: None,
            device_id: None,
            conversation: None,
            rows: Default::default(),
            rows_gen: 0,
            messages: std::collections::HashMap::new(),
            images,
            composer: None,
            thread: None,
            first: FirstComposer {
                input,
                mention,
                images: PendingImages::default(),
                sending: false,
                notice: None,
                mention_team: None,
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

    fn subject_team_id(&self) -> Option<String> {
        match &self.app {
            Some(app) => app.team_id.clone(),
            None => self.agent.as_ref().and_then(|agent| agent.team_id.clone()),
        }
    }

    /// The subject's glyph (the agent's tinted icon, the app's glyph).
    fn subject_icon(&self) -> Icon {
        match &self.app {
            Some(app) => crate::icons::app_icon(app.icon.as_deref(), app.color.as_deref()),
            None => agent_def_icon(self.agent.as_ref()),
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
        crate::controls::empty_state(Icon::new(registry::NAV_APPS), copy.starter_title.clone(), None, cx)
            .child(crate::controls::empty_state_actions().children(pills))
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
                let team_id = self.app.as_ref().and_then(|app| app.team_id.clone());
                self.device_id = crate::queries::settled_device(
                    &crate::queries::launch_devices(team_id.as_deref(), cx),
                    None,
                    None,
                );
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
                self._subscriptions.push(cx.observe(&composer, |_, _, cx| cx.notify()));
                self.composer = Some((conversation.id.clone(), composer));
                // #EXP-1380 WINDOW: another conversation opens at its newest
                // window.
                self.conv.window = Default::default();
            }
            Some(_) => {}
            None => self.composer = None,
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
                crate::session_data::thread_view_and_messages(id, None, now, editor, cx).map(|(view, messages)| {
                    (view.rows.into_iter().filter(|row| row.id() != id).collect(), messages)
                })
            })
            .unwrap_or_default();
        self.rows = std::rc::Rc::new(rows);
        self.conv.waiting = crate::chat_transcript::waiting_indexes(self.rows.iter());
        self.rows_gen = self.rows_gen.wrapping_add(1);
        self.messages = messages.into_iter().map(|message| (message.id.clone(), message)).collect();
        let _ = name;
        // Thread 58cb32df: a surface card arriving while the chat is open
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
        let team_id = self.subject_team_id();
        if team_id != self.first.mention_team {
            self.first.mention_team = team_id.clone();
            self.first.mention.update(cx, |mention, _| {
                mention.set_source(team_id.map(crate::markdown::store_completion_source));
            });
        }
        cx.notify();
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
        let Some(team_id) = self.subject_team_id() else {
            return;
        };
        let app_device = matches!(self.subject, ChannelSubject::App(_)).then(|| self.device_id.clone());
        let text = self.first.input.read(cx).value().to_string();
        let jobs = self.first.images.jobs();
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
        self.first.sending = true;
        self.first.notice = None;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let upload_team = team_id.clone();
            let uploaded = match transport {
                None => Ok(Vec::new()),
                Some(transport) => {
                    cx.background_executor()
                        .spawn(async move {
                            composer_images::upload_all(jobs, |filename, content_type, bytes| {
                                transport.upload_team_session_file(&upload_team, filename, content_type, bytes)
                            })
                        })
                        .await
                }
            };
            let resolved = match uploaded {
                Ok(resolved) => resolved,
                Err((resolved, error)) => {
                    let _ = this.update_in(cx, |this, _window, cx| {
                        this.first.sending = false;
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
                        cx.notify();
                    });
                    return;
                }
            };
            let Ok((input, plan)) = this.update(cx, |this, cx| {
                this.first.images.note_uploaded(&resolved);
                let parsed = domain::image_message::parse_steer_message(&this.first.images.message(&text));
                let mut attachment_ids = parsed.attachment_ids;
                attachment_ids.extend(parsed.files.into_iter().map(|file| file.id));
                let input = match &app_device {
                    Some(device_id) => crate::chat_launch::app_send_input(
                        &team_id,
                        &subject_id,
                        device_id.as_deref(),
                        &parsed.text,
                        attachment_ids,
                    ),
                    None => crate::chat_launch::agent_send_input(&team_id, &subject_id, &parsed.text, attachment_ids),
                };
                let plan = crate::sealing::plan_send(&input, cx);
                (input, plan)
            }) else {
                return;
            };
            let result = cx
                .background_executor()
                .spawn(async move { crate::sealing::send_blocking(&trpc, &input, &plan) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.first.sending = false;
                match result {
                    Ok(_) => {
                        this.first.images.clear();
                        this.first
                            .input
                            .update(cx, |state, cx| state.set_value("", window, cx));
                    }
                    Err(err) => crate::action_error::failed_action_err("send", "the message", &err, cx),
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
        let team_id = self.subject_team_id();
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
        let conditions = ComposerMenuConditions { prompts: !prompts.is_empty(), ..ComposerMenuConditions::default() };
        let trigger = crate::composer::composer_tool("agent-plus", registry::UI_ADD, cx)
            .tooltip(crate::chat_screen::PLUS_TOOLTIP)
            .disabled(self.first.sending);
        crate::controls::PointerMenu::for_button(trigger, move |mut menu, window, cx| {
            let root = cx.entity().downgrade();
            menu = crate::picker::search_list::search_submenu_parent(menu.min_w(px(260.)));
            for row in composer_menu_rows(ComposerSurface::Agent, conditions) {
                menu = match row {
                    Row::Separator => menu.separator(),
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
        .into_any_element()
    }

    /// The agent's mark in the 24px gutter disc.
    fn agent_marker(&self, cx: &App) -> AnyElement {
        let icon = self.subject_icon();
        div()
            .size(px(MARKER_PX))
            .flex_shrink_0()
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(cx.theme().secondary)
            .child(icon.with_size(px(14.)))
            .into_any_element()
    }

    fn render_row(&self, row: &SessionThreadRow, window: &Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        let team_id = self.subject_team_id();
        let conversation_id = self.conversation.as_ref().map(|row| row.id.clone()).unwrap_or_default();
        // FEED-140: the inline cards answer while the reader may reply.
        let answer = self.composer.as_ref().is_some_and(|(_, composer)| composer.read(cx).may_message());
        let SessionThreadRow::Message {
            id,
            author,
            author_id,
            body,
            xref,
            ..
        } = row
        else {
            let text = crate::thread_screen::ThreadText::team(team_id.as_deref()).answering(answer);
            return crate::thread_screen::card_row(row, &conversation_id, &self.rows, &text, &self.images, window, cx)
                .map(|card| div().w_full().pl(px(CARD_INSET)).child(card).into_any_element())
                .unwrap_or_else(|| div().into_any_element());
        };
        let at = self.messages.get(id).and_then(|message| message.created_at.as_deref());
        let (marker, name) = match author.as_str() {
            domain::contract::MESSAGE_AUTHOR_KIND_USER => {
                let name = crate::thread_screen::member_name(author_id.as_deref(), cx);
                let image = author_id.as_deref().and_then(|user_id| {
                    Store::try_global(cx)
                        .and_then(|store| store.collections().users.read(cx).get(user_id).cloned())
                        .and_then(|user| user.image)
                });
                let avatar = crate::user_avatar::user_avatar(
                    author_id.as_deref().unwrap_or(&name),
                    &name,
                    image.as_deref(),
                    gpui_component::Size::Small,
                    cx,
                );
                (avatar, name)
            }
            domain::contract::MESSAGE_AUTHOR_KIND_AGENT => (self.agent_marker(cx), self.subject_name()),
            _ => {
                return crate::thread_screen::muted_line(registry::UI_INFO, body.clone(), cx);
            }
        };
        // THE comment card (the run thread's), whole: no clamp, no toggle;
        // S-3: another session's message carries its link header above.
        let card = crate::comments::thread_comment_card(
            crate::comments::ThreadCommentProps {
                id: SharedString::from(format!("agent-message-{id}")),
                mark: marker,
                author: Some(name),
                meta: crate::thread_screen::clock(at),
                body: body.clone(),
                team_id,
                bare_refs: true,
                expanded: true,
                on_toggle: None,
                on_open: None,
                images: Some(self.images.clone()),
            },
            cx,
        );
        crate::thread_screen::with_xref(id, xref.as_ref(), domain::contract::MESSAGE_AUTHOR_KIND_AGENT, card, cx)
    }

    /// Thread 58cb32df (chats.json `agentMenuCases`): the agent chat
    /// header's `…` = THE agent menu (Compact conversation, New
    /// conversation, Stop while busy, Agent settings for owners).
    fn header_menu(&self, cx: &App) -> Option<AnyElement> {
        let agent_id = self.agent.as_ref()?.id.clone();
        if crate::chat_actions::agent_entries(&agent_id, cx).is_empty() {
            return None;
        }
        let trigger = crate::controls::ghost_icon_button("agent-header-more", Icon::new(registry::UI_MORE), cx);
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

    /// NEW-55 (voice-call.json): THE telephone, left of `…`, the header's
    /// icon button: disabled with the reason as its tooltip while a call
    /// cannot start here (no voice device, not this machine, no voice yet,
    /// no usable GPU, a call this client does not run). S-3: while THIS
    /// client runs the call with this agent it stays enabled and focuses
    /// the call panel; a new call focuses it too (Esc there ends it).
    fn header_call(&self, cx: &App) -> Option<AnyElement> {
        let agent_id = self.agent.as_ref()?.id.clone();
        let copy = &etalk::call::VOICE_CALL.copy;
        let here = etalk_gpui::voice_call::runs_for(&agent_id, cx);
        let reason = (!here).then(|| etalk_gpui::voice_call::unavailable(cx)).flatten();
        let tooltip = reason.clone().unwrap_or_else(|| copy.call.replace("{agent}", &self.subject_name()));
        let panel = self.call_panel.read(cx).focus_handle(cx);
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
        Some(div().flex_shrink_0().child(button).into_any_element())
    }

    fn render_header(&self, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let icon = self.subject_icon();
        let name = self.subject_name();
        let call = self.header_call(cx);
        let menu = self.header_menu(cx);
        v_flex()
            .w_full()
            .px(px(HEADER_INSET))
            .pt_6()
            .gap_4()
            .child(
                h_flex()
                    .min_w_0()
                    .gap_3()
                    .items_center()
                    .child(icon.with_size(px(HEADER_ICON_PX)))
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
                    .children(call)
                    .children(menu),
            )
            .child(div().w_full().h(px(1.)).bg(theme.border))
            .into_any_element()
    }
}

impl Focusable for AgentScreenView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for AgentScreenView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let editor = matches!(self.subject, ChannelSubject::App(_));
        // S-3: one session screen. An agent's conversation renders on THE
        // thread screen (chat bubbles, its header, composer with the "+",
        // selection, Dock); this view keeps only the "no conversation yet"
        // page, whose first send = sessions.send({agentId}).
        if !editor {
            if let Some(root_id) = self.conversation.as_ref().map(|row| row.id.clone()) {
                if self.thread.as_ref().map(|(id, _)| id.as_str()) != Some(root_id.as_str()) {
                    let id = root_id.clone();
                    let view = cx.new(|cx| crate::thread_screen::ThreadScreenView::new(id, window, cx));
                    self.thread = Some((root_id, view));
                }
                if let Some((_, thread)) = self.thread.as_ref() {
                    return v_flex().track_focus(&self.focus_handle).size_full().min_h_0().child(thread.clone());
                }
            } else {
                self.thread = None;
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
            if crate::thread_screen::thread_not_found(ready, false, self.opened_at.elapsed()) {
                return v_flex()
                    .track_focus(&self.focus_handle)
                    .size_full()
                    .px_4()
                    .items_center()
                    .justify_center()
                    .child(crate::controls::empty_state(
                        Icon::new(registry::SETTINGS_AGENTS),
                        AGENT_NOT_FOUND,
                        Some(crate::empty_state_copy::THREAD_NOT_FOUND_DESCRIPTION.into()),
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
        let team_id = self.subject_team_id();
        let composer_view = self.composer.as_ref().map(|(_, composer)| composer.clone());
        let text = crate::thread_screen::ThreadText::team(team_id.as_deref());
        let rows_snapshot = self.rows.clone();
        let (band, sheet, docked) = match composer_view.filter(|composer| composer.read(cx).may_message()) {
            Some(composer) => {
                // S-3: a waiting plan / question is a story, inline in
                // the conversation (with its actions), never docked.
                let docked: HashSet<String> = HashSet::new();
                let (band, sheet) = crate::thread_screen::root_dock(
                    "agent",
                    &rows_snapshot,
                    None,
                    &self.dock,
                    &composer,
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
            Some(empty) => div().flex_1().min_h_0().w_full().pt_8().child(empty).into_any_element(),
            None => {
                let list = gpui::list(
                    self.list.clone(),
                    cx.processor(|this, ix: usize, window, cx| this.render_list_row(ix, window, cx)),
                )
                .pt_8()
                .pb(px(band_height.max(16.) + crate::dock::DOCK_GAP));
                crate::scroll_pane::v_list_pane(list, &self.list).into_any_element()
            }
        };
        v_flex()
            .track_focus(&self.focus_handle)
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .when(!editor, |this| this.child(self.render_header(cx)))
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
        let inputs = (self.rows_gen ^ crate::thread_screen::thread_row_generation(cx).rotate_left(32), tail, start);
        if self.conv.synced == Some(inputs) {
            return;
        }
        self.conv.synced = Some(inputs);
        self.conv.start = start;
        self.conv.tail = tail;
        let root_id = self.conversation.as_ref().map(|row| row.id.clone()).unwrap_or_default();
        let mut keys: Vec<RowKey> = self.rows[start..]
            .iter()
            .enumerate()
            .map(|(offset, row)| {
                let at = match row {
                    SessionThreadRow::Message { id, .. } => {
                        self.messages.get(id).and_then(|message| message.created_at.as_deref())
                    }
                    _ => None,
                };
                let mut fingerprint = crate::thread_screen::conversation_fingerprint(row, at, 0)
                    ^ crate::thread_screen::row_state_fingerprint(row, &root_id, cx);
                if offset == 0 && start > 0 {
                    fingerprint = fingerprint.rotate_left(1) ^ 1;
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
        let content = match rows.get(self.conv.start + ix) {
            Some(row) if ix < self.conv.keys.len() && self.conv.keys[ix].id != WORKING_ROW_ID => {
                Some(self.render_row(row, window, cx))
            }
            _ => self
                .composer
                .as_ref()
                .map(|(_, composer)| composer.clone())
                .and_then(|composer| composer.update(cx, |composer, cx| composer.live_tail(window, cx)))
                .map(|tail| div().w_full().pl(px(CARD_INSET)).child(tail).into_any_element()),
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
                    .when(ix > 0, |this| this.pt_4())
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
