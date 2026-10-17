//! EXP-1294 — an AGENT CHANNEL ([`Screen::Agent`]): a team agent is a root
//! session of kind `agent` that stays open. The header is the agent's
//! tinted icon and name; the conversation is its messages as COMMENT ROWS
//! ([`crate::comments::message_row`]: avatar or the agent's mark · author ·
//! `HH:MM` · markdown), every `thread_started` / `session` card THE session
//! card (a started thread opens as the thread, a session as the session
//! view), question and plan cards answered in place; under it THE session
//! composer of the session view targeting the agent root
//! (`session_screen::root_composer_view`: model pin, context ring, `/` menu,
//! Send⇄Stop over the tree, the queue strip), `Message <name>`.

use gpui::{
    div, px, AnyElement, App, AppContext as _, Entity, FocusHandle,
    Focusable, FontWeight, InteractiveElement as _, IntoElement, ParentElement, Render,
    ScrollHandle, SharedString, Styled, Subscription, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Sizable as _};
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

/// The agent's display name: its `name`, else EXP-1341's title ladder
/// (`domain::session_title`; `Untitled` with nothing to go on).
pub(crate) fn agent_name(agent: &SessionRow) -> String {
    domain::session_title::title_from_parts(
        agent.name.as_deref(),
        agent.title.as_deref(),
        &[],
        agent.action_name.as_deref(),
        None,
    )
}

pub struct AgentScreenView {
    agent_id: String,
    agent: Option<SessionRow>,
    rows: Vec<SessionThreadRow>,
    messages: std::collections::HashMap<String, MessageRow>,
    images: Entity<ImageCache>,
    /// THE session composer, targeting the agent root.
    composer: Entity<crate::steer_viewer::SteerSessionView>,
    /// THE ask dock's state (the focused card, the swap animation).
    ask_dock: Entity<crate::ask_dock::AskDockState>,
    /// The name the placeholder was built for.
    placeholder_name: Option<String>,
    scroll: ScrollHandle,
    /// EXP-1316 (1n): the channel follows its tail like the session feed.
    follow: crate::controls::TailFollow,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl AgentScreenView {
    pub fn new(agent_id: String, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let name = crate::session_data::session(&agent_id, cx).map(|agent| agent_name(&agent));
        let composer = crate::session_screen::root_composer_view(
            &agent_id,
            message_placeholder(name.as_deref().unwrap_or_default()),
            crate::steer_viewer::ComposerIds::new(
                "agent-send",
                "agent-attach",
                "agent-model",
                "agent-context",
            ),
            window,
            cx,
        );
        let mut subscriptions = vec![cx.observe(&composer, |_, _, cx| cx.notify())];
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
            composer,
            ask_dock: cx.new(|_| crate::ask_dock::AskDockState::default()),
            placeholder_name: name,
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
            let name = agent_name(&agent);
            if self.placeholder_name.as_deref() != Some(name.as_str()) {
                self.composer.update(cx, |composer, cx| {
                    composer.set_placeholder(message_placeholder(&name), window, cx);
                });
                self.placeholder_name = Some(name);
            }
        }
        cx.notify();
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
            return crate::thread_screen::card_row(row, &self.agent_id, &crate::thread_screen::ThreadText::team(team_id.as_deref()), &self.images, window, cx)
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
                images: Some(self.images.clone()),
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
        // THE ask dock (ask-dock.json ×4): the channel's waiting cards take
        // the composer's slot; a reader who may not reply keeps them inline.
        let team_id = self.agent.as_ref().and_then(|agent| agent.team_id.clone());
        let dock = self
            .composer
            .read(cx)
            .may_message()
            .then(|| {
                crate::thread_screen::conversation_dock(
                    "agent",
                    &self.rows,
                    |_| None,
                    &self.ask_dock,
                    &crate::thread_screen::ThreadText::team(team_id.as_deref()),
                    &self.images,
                    window,
                    cx,
                )
            })
            .flatten();
        let docked = dock.as_ref().map(|(_, ids)| ids.clone()).unwrap_or_default();
        let rows: Vec<AnyElement> = self
            .rows
            .clone()
            .iter()
            .filter(|row| !docked.contains(row.id()))
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
        // THE session composer of the session view, targeting the agent; it
        // yields its slot to the dock.
        let composer = match dock {
            Some((dock, _)) => Some(
                div()
                    .w_full()
                    .max_w(px(CHANNEL_W))
                    .mx_auto()
                    .px(px(crate::work_header::WORK_GUTTER))
                    .child(dock)
                    .into_any_element(),
            ),
            None => self
                .composer
                .update(cx, |composer, cx| composer.render_session_composer(window, cx)),
        };
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
            .children(composer.map(|composer| div().w_full().pb_2().child(composer)))
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
        assert_eq!(agent_name(&unnamed), domain::session_title::copy::UNTITLED);
    }
}
