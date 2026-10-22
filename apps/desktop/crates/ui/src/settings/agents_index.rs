//! NEW-6: Settings → Agents, the index (web `settings/agents/index.tsx` +
//! `AgentsListBand` / `AgentEventsBand` of `components/team/agents-section.tsx`).
//! The Agents group's LABEL opens it, like the web nav.
//!
//! Two bands: the team's agents ("New agent" the owner-only trailing pill;
//! a row = the agent's tinted icon + name, opening its settings page), over
//! the team's Events: the synced `events` stream newest first (`created_at`
//! then `id`, descending), capped at [`AGENT_EVENTS_CAP`], one flat row per
//! event = the trigger-event glyph, the name, who emitted it (the emitting
//! session's title, the person's name, or the server) and a compact relative
//! time. Member-readable.

use gpui::{
    div, App, ClickEvent, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription,
    Window,
};
use gpui_component::{v_flex, ActiveTheme as _, Icon, Sizable as _};
use sync::Store;

use domain::rows::EventRow;

use crate::icons::registry;
use crate::navigation::{active_team_id, navigate, Navigation, Screen};
use crate::sidebar::select_settings_section;

use super::{section, SettingsSection};

/// The newest events the Events band lists (web `AGENT_EVENTS_CAP`).
pub(crate) const AGENT_EVENTS_CAP: usize = 50;
/// The Events band's label.
pub(crate) const EVENTS_TITLE: &str = "Events";
/// The Events band's empty state (web).
pub(crate) const EVENTS_EMPTY: &str = "No events yet";
/// The agents band's empty state (web).
pub(crate) const AGENTS_EMPTY: &str = "No agents yet";

/// Who emitted an event, as its row's caption (web `eventEmitterLabel`):
/// the session's title, the person's name, or the server.
pub(crate) fn event_emitter_label(
    event: &EventRow,
    session_title: impl Fn(&str) -> Option<String>,
    user_name: impl Fn(&str) -> String,
) -> String {
    match event.emitted_by.as_deref() {
        Some("session") => event
            .session_id
            .as_deref()
            .and_then(session_title)
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| "Session".to_string()),
        Some("user") => event
            .user_id
            .as_deref()
            .map(user_name)
            .unwrap_or_else(|| "Someone".to_string()),
        _ => "Exponential".to_string(),
    }
}

/// The team's events, newest first (`created_at` desc, then `id` desc),
/// capped at [`AGENT_EVENTS_CAP`] (web `AgentEventsBand`'s live query).
pub(crate) fn team_events<'a>(events: impl Iterator<Item = &'a EventRow>, team_id: &str) -> Vec<EventRow> {
    let mut rows: Vec<EventRow> = events
        .filter(|event| event.team_id.as_deref() == Some(team_id))
        .cloned()
        .collect();
    rows.sort_by(|a, b| {
        b.created_at
            .as_deref()
            .unwrap_or("")
            .cmp(a.created_at.as_deref().unwrap_or(""))
            .then_with(|| b.id.cmp(&a.id))
    });
    rows.truncate(AGENT_EVENTS_CAP);
    rows
}

/// The Agents index pane.
pub struct AgentsIndexPane {
    nav: Entity<Navigation>,
    _subscriptions: Vec<Subscription>,
}

impl AgentsIndexPane {
    pub fn new(nav: Entity<Navigation>, cx: &mut gpui::Context<Self>) -> Self {
        let mut subscriptions = vec![cx.observe(&nav, |_, _, cx| cx.notify())];
        if let Some(store) = Store::try_global(cx) {
            let collections = store.collections().clone();
            subscriptions.push(cx.observe(&collections.agents, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&collections.events, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&collections.sessions, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&collections.users, |_, _, cx| cx.notify()));
            subscriptions.push(cx.observe(&collections.team_members, |_, _, cx| cx.notify()));
        }
        Self {
            nav,
            _subscriptions: subscriptions,
        }
    }

    /// The agents band (web `AgentsListBand`).
    fn render_agents(&self, team_id: &str, owner: bool, cx: &App) -> gpui::AnyElement {
        let agents = crate::session_data::agents_in_team(team_id, cx);
        let ready = Store::global(cx).collections().agents.read(cx).is_ready();
        let trailing = owner.then(|| {
            crate::surface::glass_pill_button("agents-new", crate::surface::PillSize::Sm, cx)
                .icon(registry::UI_ADD)
                .label(domain::agent_settings::AGENT_SETTINGS.copy.new_agent.clone())
                .on_click(|_, window, cx| {
                    select_settings_section(window, cx, SettingsSection::NewAgent);
                    navigate(window, cx, Screen::Settings);
                })
                .into_any_element()
        });
        let body = section(cx).child(crate::surface::glass_section_header(
            super::AGENTS_GROUP,
            trailing,
            cx,
        ));
        if agents.is_empty() {
            return body
                .child(if ready {
                    crate::controls::empty_state_list(Icon::new(registry::SETTINGS_AGENTS), AGENTS_EMPTY, cx)
                        .into_any_element()
                } else {
                    super::settings_skeleton("agents-skeleton", cx)
                })
                .into_any_element();
        }
        let row_hover = cx.theme().list_hover;
        let mut list = v_flex().w_full().min_w_0();
        for agent in agents {
            let open_id = agent.agent_id.clone();
            list = list.child(crate::surface::list_row(
                crate::surface::flat_row()
                    .id(SharedString::from(format!("agents-list-row-{}", agent.agent_id)))
                    .flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .text_sm()
                    .cursor_pointer()
                    .hover(move |this| this.bg(row_hover))
                    .on_click(move |_: &ClickEvent, window, cx| {
                        select_settings_section(window, cx, SettingsSection::Agent(open_id.clone()));
                        navigate(window, cx, Screen::Settings);
                    })
                    .child(crate::icons::agent_row_icon(&agent).small().flex_shrink_0())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .child(SharedString::from(agent.name.clone())),
                    ),
            ));
        }
        body.child(list).into_any_element()
    }

    /// The Events band (web `AgentEventsBand`).
    fn render_events(&self, team_id: &str, cx: &App) -> gpui::AnyElement {
        let collections = Store::global(cx).collections();
        let collection = collections.events.read(cx);
        let ready = collection.is_ready();
        let events = team_events(collection.iter(), team_id);
        let body = section(cx).child(crate::surface::glass_section_header(EVENTS_TITLE, None, cx));
        if events.is_empty() {
            return body
                .child(if ready {
                    crate::controls::empty_state_list(Icon::new(registry::TRIGGER_EVENT), EVENTS_EMPTY, cx)
                        .into_any_element()
                } else {
                    super::settings_skeleton("agent-events-skeleton", cx)
                })
                .into_any_element();
        }
        let muted = cx.theme().muted_foreground;
        let mut list = v_flex().w_full().min_w_0();
        for event in events {
            let emitter = event_emitter_label(
                &event,
                |session_id| crate::session_data::session_title_of(session_id, cx),
                |user_id| crate::thread_screen::member_name(Some(user_id), cx),
            );
            let at = event
                .created_at
                .as_deref()
                .map(crate::inbox::relative_time)
                .unwrap_or_default();
            list = list.child(crate::surface::list_row(
                crate::surface::flat_row()
                    .id(SharedString::from(format!("agent-event-{}", event.id)))
                    .flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .text_sm()
                    .child(
                        Icon::new(registry::TRIGGER_EVENT)
                            .small()
                            .flex_shrink_0()
                            .text_color(muted),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .child(SharedString::from(event.name.clone().unwrap_or_default())),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(muted)
                            .child(SharedString::from(emitter)),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(muted)
                            .child(SharedString::from(at)),
                    ),
            ));
        }
        body.child(list).into_any_element()
    }
}

impl Render for AgentsIndexPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return v_flex().child(crate::controls::empty_state_list(
                Icon::new(registry::UI_TEAM),
                crate::empty_state_copy::NO_TEAM_SELECTED,
                cx,
            ));
        };
        let owner = super::is_owner(cx, &team_id);
        v_flex()
            .w_full()
            .gap_6()
            .child(self.render_agents(&team_id, owner, cx))
            .child(self.render_events(&team_id, cx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(value: serde_json::Value) -> EventRow {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn the_emitter_is_the_session_the_person_or_the_server() {
        let title = |id: &str| (id == "s1").then(|| "Fix the build".to_string());
        let name = |id: &str| format!("user {id}");
        let session = event(serde_json::json!({"id": "e", "emitted_by": "session", "session_id": "s1"}));
        assert_eq!(event_emitter_label(&session, title, name), "Fix the build");
        let unknown = event(serde_json::json!({"id": "e", "emitted_by": "session", "session_id": "s9"}));
        assert_eq!(event_emitter_label(&unknown, title, name), "Session");
        let user = event(serde_json::json!({"id": "e", "emitted_by": "user", "user_id": "u1"}));
        assert_eq!(event_emitter_label(&user, title, name), "user u1");
        let nobody = event(serde_json::json!({"id": "e", "emitted_by": "user"}));
        assert_eq!(event_emitter_label(&nobody, title, name), "Someone");
        let server = event(serde_json::json!({"id": "e", "emitted_by": "server"}));
        assert_eq!(event_emitter_label(&server, title, name), "Exponential");
    }

    #[test]
    fn events_list_newest_first_for_the_team_capped() {
        let mut rows = vec![
            event(serde_json::json!({"id": "a", "team_id": "t", "created_at": "2026-10-21T10:00:00Z"})),
            event(serde_json::json!({"id": "b", "team_id": "t", "created_at": "2026-10-21T11:00:00Z"})),
            event(serde_json::json!({"id": "c", "team_id": "t", "created_at": "2026-10-21T11:00:00Z"})),
            event(serde_json::json!({"id": "d", "team_id": "other", "created_at": "2026-10-21T12:00:00Z"})),
        ];
        let ids: Vec<String> = team_events(rows.iter(), "t").into_iter().map(|row| row.id).collect();
        assert_eq!(ids, vec!["c", "b", "a"]);
        for n in 0..60 {
            rows.push(event(serde_json::json!({"id": format!("x{n:02}"), "team_id": "t"})));
        }
        assert_eq!(team_events(rows.iter(), "t").len(), AGENT_EVENTS_CAP);
    }
}
