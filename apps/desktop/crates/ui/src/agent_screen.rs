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

use std::collections::HashSet;
use std::time::Instant;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, AnyElement, App, AppContext as _, ClickEvent, Entity, FocusHandle,
    Focusable, FontWeight, InteractiveElement as _, IntoElement, ParentElement, Render,
    ScrollHandle, SharedString, Styled, Subscription, Window,
};
use gpui_component::input::{InputEvent, TextareaState};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _};
use sync::Store;

use domain::rows::{AgentDefRow, AppRow, MessageRow, SessionRow};
use domain::session_tree::SessionThreadRow;

use crate::composer_images::{self, PendingImages};
use crate::icons::registry;
use crate::markdown::ImageCache;

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

/// The page whose agent is not (or no longer) there (web `$agentId.tsx`).
const AGENT_NOT_FOUND: &str = "Not found";

/// The composer's placeholder: `Message <name>`.
pub(crate) fn message_placeholder(name: &str) -> String {
    format!("Message {name}")
}

/// The agent's display name: the `agents` row's `name`, `Untitled` while it
/// is blank (EXP-1341's ladder bottom).
pub(crate) fn agent_def_name(agent: &AgentDefRow) -> String {
    domain::session_title::title_from_parts(agent.name.as_deref(), None, &[], None, None)
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
    rows: Vec<SessionThreadRow>,
    messages: std::collections::HashMap<String, MessageRow>,
    images: Entity<ImageCache>,
    /// THE session composer, targeting the conversation root (keyed by its
    /// id); `None` until the conversation exists.
    composer: Option<(String, Entity<crate::steer_viewer::SteerSessionView>)>,
    first: FirstComposer,
    /// THE ask dock's state (the focused card, the swap animation).
    /// N-12: THE Dock's state (the fan, the open sheet, the band height).
    dock: Entity<crate::dock::DockState>,
    /// The name the placeholder was built for.
    placeholder_name: Option<String>,
    scroll: ScrollHandle,
    /// EXP-1316 (1n): the channel follows its tail like the session feed.
    follow: crate::controls::TailFollow,
    opened_at: Instant,
    focus_handle: FocusHandle,
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
                this.refresh(window, cx)
            }));
            subscriptions.push(cx.observe_in(&collections.apps, window, |this, _, window, cx| {
                this.refresh(window, cx)
            }));
            subscriptions.push(cx.observe_in(&collections.devices, window, |this, _, window, cx| {
                this.refresh(window, cx)
            }));
            subscriptions.push(cx.observe_in(&collections.sessions, window, |this, _, window, cx| {
                this.refresh(window, cx)
            }));
            subscriptions.push(cx.observe_in(&collections.messages, window, |this, _, window, cx| {
                this.refresh(window, cx)
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
            rows: Vec::new(),
            messages: std::collections::HashMap::new(),
            images,
            composer: None,
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
            scroll: ScrollHandle::new(),
            follow: crate::controls::TailFollow::default(),
            opened_at: Instant::now(),
            focus_handle: cx.focus_handle(),
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
            ChannelSubject::App(_) => domain::apps::APPS.copy.composer_placeholder.clone(),
        }
    }

    fn refresh(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match self.subject.clone() {
            ChannelSubject::Agent(requested) => {
                let agent_id = resolve_agent_id(&requested, cx);
                self.agent = crate::session_data::agent_def(&agent_id, cx);
                self.conversation = crate::session_data::my_conversation(&agent_id, cx);
            }
            ChannelSubject::App(app_id) => {
                self.app = Store::try_global(cx)
                    .and_then(|store| store.collections().apps.read(cx).get(&app_id).cloned());
                self.conversation = crate::session_data::my_editor_session(&app_id, cx);
                self.device_id = crate::queries::settled_device(&crate::queries::launch_devices(cx), None, None);
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
            }
            Some(_) => {}
            None => self.composer = None,
        }
        let conversation_id = self.conversation.as_ref().map(|row| row.id.clone());
        // The conversation's own card leads the contract's rows; the header
        // already names the agent.
        let editor = matches!(self.subject, ChannelSubject::App(_));
        self.rows = conversation_id
            .as_deref()
            .and_then(|id| {
                let now = crate::session_data::now_ms();
                // VAPP-110: an editor's chat is its whole tree (a
                // revived root keeps its rows, FEED-134).
                if editor {
                    crate::session_data::editor_thread_view(id, now, cx)
                } else {
                    crate::session_data::session_thread_view(id, None, now, cx)
                }
                .map(|view| view.rows.into_iter().filter(|row| row.id() != id).collect())
            })
            .unwrap_or_default();
        self.messages = conversation_id
            .as_deref()
            .map(|id| {
                if editor {
                    crate::session_data::tree_messages(id, cx)
                } else {
                    crate::session_data::messages_of_session(id, cx)
                }
            })
            .unwrap_or_default()
            .into_iter()
            .map(|message| (message.id.clone(), message))
            .collect();
        let _ = name;
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
            let Ok(input) = this.update(cx, |this, _| {
                this.first.images.note_uploaded(&resolved);
                let parsed = domain::image_message::parse_steer_message(&this.first.images.message(&text));
                let mut attachment_ids = parsed.attachment_ids;
                attachment_ids.extend(parsed.files.into_iter().map(|file| file.id));
                match &app_device {
                    Some(device_id) => crate::chat_launch::app_send_input(
                        &team_id,
                        &subject_id,
                        device_id.as_deref(),
                        &parsed.text,
                        attachment_ids,
                    ),
                    None => crate::chat_launch::agent_send_input(&team_id, &subject_id, &parsed.text, attachment_ids),
                }
            }) else {
                return;
            };
            let result = cx
                .background_executor()
                .spawn(async move { api::sessions::send(&trpc, &input) })
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
                .tool(
                    crate::composer::composer_tool("agent-attach", registry::UI_ATTACH, cx)
                        .tooltip("Attach files")
                        .disabled(self.first.sending)
                        .on_click(cx.listener(|_, _: &ClickEvent, window, cx| {
                            composer_images::pick_attachment_files(window, cx, |this: &mut Self, read, window, cx| {
                                this.stage_first_files(read, window, cx)
                            });
                        })),
                ),
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
        let SessionThreadRow::Message {
            id,
            author,
            author_id,
            body,
            ..
        } = row
        else {
            return crate::thread_screen::card_row(row, &conversation_id, &crate::thread_screen::ThreadText::team(team_id.as_deref()), &self.images, window, cx)
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
        // THE comment card (the run thread's), whole: no clamp, no toggle.
        crate::comments::thread_comment_card(
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
        )
    }

    fn render_header(&self, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let icon = self.subject_icon();
        let name = self.subject_name();
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
                            .min_w_0()
                            .truncate()
                            .text_2xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.foreground)
                            .child(SharedString::from(name)),
                    ),
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
                let pending = domain::ask_dock::thread_pending_asks(&rows_snapshot, |_| None);
                let docked: HashSet<String> = pending.iter().map(|ask| ask.id.clone()).collect();
                let (band, sheet) = crate::thread_screen::root_dock(
                    "agent",
                    &rows_snapshot,
                    &pending,
                    None,
                    &self.dock,
                    &composer,
                    &text,
                    &self.images,
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
        let rows: Vec<AnyElement> = rows_snapshot
            .iter()
            .filter(|row| !docked.contains(row.id()))
            .map(|row| self.render_row(row, window, cx))
            .collect();
        // VAPP-110: an editor chat with nothing in it yet = the fixture's
        // `chatEmpty` (the list-size empty state).
        let chat_empty = (editor && rows.is_empty()).then(|| {
            crate::controls::empty_state_list(
                Icon::new(registry::NAV_APPS),
                domain::apps::APPS.copy.chat_empty.clone(),
                cx,
            )
            .py_12()
        });
        let column = v_flex()
            .w_full()
            .max_w(px(CHANNEL_W))
            .mx_auto()
            .px(px(crate::work_header::WORK_GUTTER))
            .pt_8()
            .pb(px(band_height.max(16.) + crate::dock::DOCK_GAP))
            .gap_4()
            .children(rows)
            .children(chat_empty);
        v_flex()
            .track_focus(&self.focus_handle)
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .when(!editor, |this| this.child(self.render_header(cx)))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(self.follow.scroller(
                        if editor { "app-editor-chat-scroll" } else { "agent-conversation-scroll" },
                        &self.scroll,
                        column,
                        cx.entity_id(),
                        cx,
                    ))
                    .children(band)
                    .children(sheet),
            )
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
