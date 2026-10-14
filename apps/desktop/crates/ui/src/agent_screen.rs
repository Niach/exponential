//! EXP-1294 — an AGENT CHANNEL ([`Screen::Agent`]): a team agent is a root
//! session of kind `agent` that stays open. The header is the agent's
//! tinted icon and name; the conversation is its messages as COMMENT ROWS
//! ([`crate::comments::message_row`]: avatar or the agent's mark · author ·
//! `HH:MM` · markdown), every `thread_started` / `session` card THE session
//! card (a started thread opens as the thread, a session as the session
//! view), question and plan cards answered in place; under it THE session
//! composer box, `Message <name>`, sending `sessions.send({sessionId:
//! agent})`.

use gpui::{
    div, px, AnyElement, App, AppContext as _, Entity, FocusHandle,
    Focusable, FontWeight, InteractiveElement as _, IntoElement, ParentElement, Render,
    ScrollHandle, SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    h_flex,
    input::{InputEvent, TextareaState},
    v_flex, ActiveTheme as _, Sizable as _,
};
use sync::Store;

use domain::rows::{MessageRow, SessionRow};
use domain::session_tree::SessionThreadRow;

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

/// The composer's placeholder: `Message <name>`.
pub(crate) fn message_placeholder(name: &str) -> String {
    format!("Message {name}")
}

/// The agent's display name: its `name`, else the session copy.
pub(crate) fn agent_name(agent: &SessionRow) -> String {
    agent
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| domain::session_tree::SESSION_CARD.copy.session.clone())
}

pub struct AgentScreenView {
    agent_id: String,
    agent: Option<SessionRow>,
    rows: Vec<SessionThreadRow>,
    messages: std::collections::HashMap<String, MessageRow>,
    images: Entity<ImageCache>,
    input: Entity<TextareaState>,
    mention: Entity<crate::mention_input::MentionInput>,
    mention_team: Option<String>,
    /// The name the placeholder was built for.
    placeholder_name: Option<String>,
    /// The composer's staged images and their notice.
    pending: crate::composer_images::PendingImages,
    notice: Option<SharedString>,
    sending: bool,
    scroll: ScrollHandle,
    /// EXP-1316 (1n): the channel follows its tail like the session feed.
    follow: crate::controls::TailFollow,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl crate::composer::SessionReply for AgentScreenView {
    fn reply(&mut self) -> crate::composer::ReplyParts<'_> {
        crate::composer::ReplyParts {
            input: &self.input,
            images: &mut self.pending,
            notice: &mut self.notice,
            sending: &mut self.sending,
        }
    }
}

impl AgentScreenView {
    pub fn new(agent_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let name = crate::session_data::session(&agent_id, cx).map(|agent| agent_name(&agent));
        let (input, mention) = crate::composer::session_composer_field(
            message_placeholder(name.as_deref().unwrap_or_default()),
            window,
            cx,
        );
        let mut subscriptions = vec![cx.subscribe_in(
            &input,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { shift: false, .. } => this.send(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            },
        )];
        if let Some(store) = Store::try_global(cx) {
            let collections = store.collections().clone();
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
        let mut this = Self {
            agent_id,
            agent: None,
            rows: Vec::new(),
            messages: std::collections::HashMap::new(),
            images,
            input,
            mention,
            mention_team: None,
            placeholder_name: name,
            pending: Default::default(),
            notice: None,
            sending: false,
            scroll: ScrollHandle::new(),
            follow: crate::controls::TailFollow::default(),
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        };
        this.refresh(window, cx);
        this
    }

    fn refresh(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.agent = crate::session_data::session(&self.agent_id, cx);
        // The channel's own card leads the contract's rows; the header
        // already names the agent.
        let rows: Vec<SessionThreadRow> =
            crate::session_data::session_thread_view(&self.agent_id, None, crate::session_data::now_ms(), cx)
                .map(|view| {
                    view.rows
                        .into_iter()
                        .filter(|row| row.id() != self.agent_id)
                        .collect()
                })
                .unwrap_or_default();
        self.rows = rows;
        self.messages = crate::session_data::messages_of_session(&self.agent_id, cx)
            .into_iter()
            .map(|message| (message.id.clone(), message))
            .collect();
        if let Some(agent) = self.agent.clone() {
            let team = agent.team_id.clone();
            if team != self.mention_team {
                self.mention_team = team.clone();
                self.mention.update(cx, |mention, _| {
                    mention.set_source(team.map(crate::markdown::store_completion_source));
                });
            }
            let name = agent_name(&agent);
            if self.placeholder_name.as_deref() != Some(name.as_str()) {
                self.input.update(cx, |input, cx| {
                    input.set_placeholder(message_placeholder(&name), window, cx);
                });
                self.placeholder_name = Some(name);
            }
        }
        cx.notify();
    }

    /// `sessions.send({sessionId: agent, text})`.
    fn send(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let agent_id = self.agent_id.clone();
        crate::composer::send_session_reply(self, &agent_id, window, cx);
    }

    /// The agent's mark in the 24px gutter disc.
    fn agent_marker(&self, cx: &App) -> AnyElement {
        let icon = match &self.agent {
            Some(agent) => crate::icons::agent_session_icon(agent),
            None => crate::icons::agent_icon(None, None),
        };
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
        let team_id = self.agent.as_ref().and_then(|agent| agent.team_id.clone());
        let SessionThreadRow::Message {
            id,
            author,
            author_id,
            body,
            ..
        } = row
        else {
            return crate::thread_screen::card_row(row, &self.agent_id, team_id.as_deref(), &self.images, window, cx)
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
            domain::contract::MESSAGE_AUTHOR_KIND_AGENT => (
                self.agent_marker(cx),
                self.agent.as_ref().map(agent_name).unwrap_or_default(),
            ),
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
            },
            cx,
        )
    }

    fn render_header(&self, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let (icon, name) = match &self.agent {
            Some(agent) => (crate::icons::agent_session_icon(agent), agent_name(agent)),
            None => (crate::icons::agent_icon(None, None), String::new()),
        };
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
        if self.agent.is_none() {
            // Skeletons, never "Loading…" (feedback-states.json).
            return v_flex()
                .track_focus(&self.focus_handle)
                .size_full()
                .px(px(HEADER_INSET))
                .pt_6()
                .child(crate::feedback::skeleton_rows("agent-loading", 3, 36., cx));
        }
        let rows: Vec<AnyElement> = self
            .rows
            .clone()
            .iter()
            .map(|row| self.render_row(row, window, cx))
            .collect();
        let column = v_flex()
            .w_full()
            .max_w(px(CHANNEL_W))
            .mx_auto()
            .px(px(crate::work_header::WORK_GUTTER))
            .pt_8()
            .pb_8()
            .gap_4()
            .children(rows);
        let empty = !crate::composer::reply_has_content(self, cx);
        let this = cx.entity().downgrade();
        let submit = crate::composer::SessionComposerSubmit::send("agent-send", empty, move |_, window, cx| {
            let _ = this.update(cx, |this, cx| this.send(window, cx));
        })
        .loading(self.sending);
        let paste = cx.listener(crate::composer::on_reply_paste::<Self>);
        let composer = crate::composer::session_composer_box(
            crate::composer::SessionComposerBox::new(self.mention.clone(), submit)
                .strip(crate::composer::reply_strip(self, "agent-pending-remove", cx))
                .tool(crate::composer::reply_attach_tool("agent-attach", self.sending, cx))
                .footer(crate::composer::reply_notice(self, cx))
                .card_hook(move |card| card.capture_action(paste)),
            cx,
        );
        v_flex()
            .track_focus(&self.focus_handle)
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .child(self.render_header(cx))
            .child(self.follow.scroller(
                "agent-conversation-scroll",
                &self.scroll,
                column,
                cx.entity_id(),
                cx,
            ))
            .child(div().w_full().pb_2().child(composer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_placeholder_names_the_agent() {
        assert_eq!(message_placeholder("Triage"), "Message Triage");
        let agent: SessionRow = serde_json::from_value(serde_json::json!({"id": "a", "name": " Triage "})).unwrap();
        assert_eq!(agent_name(&agent), "Triage");
        let unnamed: SessionRow = serde_json::from_value(serde_json::json!({"id": "a"})).unwrap();
        assert_eq!(agent_name(&unnamed), domain::session_tree::SESSION_CARD.copy.session);
    }
}
