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
    /// #EXP-1735: the team's lead member from `agents.lead`: `None` = not
    /// read yet (row disabled), `Some(None)` = no lead.
    lead: Option<Option<String>>,
    /// The `(team, synced teams.lead_assistant_id)` the lead was read for.
    lead_key: Option<(String, Option<String>)>,
    /// An `agents.setLeadAssistant` is in flight (row disabled).
    lead_busy: bool,
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
            subscriptions.push(cx.observe(&collections.teams, |_, _, cx| cx.notify()));
        }
        Self {
            nav,
            lead: None,
            lead_key: None,
            lead_busy: false,
            _subscriptions: subscriptions,
        }
    }

    /// #EXP-1735: re-read `agents.lead` when the pane first shows a team, when
    /// the synced `teams.lead_assistant_id` moves (another owner's pick, a
    /// deleted Assistant) and after a save. Called from `render`; the key
    /// is stamped before the fetch, so a failure never loops.
    fn ensure_lead(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) {
        let synced = Store::global(cx)
            .collections()
            .teams
            .read(cx)
            .get(team_id)
            .and_then(|team| team.lead_assistant_id.clone());
        let key = (team_id.to_string(), synced);
        if self.lead_key.as_ref() == Some(&key) {
            return;
        }
        if self.lead_key.as_ref().map(|(team, _)| team.as_str()) != Some(team_id) {
            self.lead = None;
        }
        self.lead_key = Some(key.clone());
        let Some(trpc) = crate::queries::trpc_client(cx) else { return };
        cx.spawn(async move |this, cx| {
            let team = key.0.clone();
            let result = cx
                .background_executor()
                .spawn(async move { api::agents::lead(&trpc, &team) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.lead_key.as_ref() != Some(&key) {
                    return;
                }
                match result {
                    Ok(lead) => this.lead = Some(lead.map(|lead| lead.user_id)),
                    Err(err) => log::warn!("[ui] agents.lead failed: {err}"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// #EXP-1735 (agent-settings.json `lead`, web `LeadAssistantRow`): THE
    /// settings Picker row (single select, where the Coordinator row was),
    /// value = the lead member's display name or `copy.none`; options =
    /// None, then `lead_options` (the team's members by display name, then
    /// id). Owners pick (saved at once); members see it disabled.
    fn render_lead(
        &self,
        team_id: &str,
        owner: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        use domain::agent_settings::{lead_options, LEAD, LEAD_NONE};
        let current = self
            .lead
            .clone()
            .flatten()
            .unwrap_or_else(|| LEAD_NONE.to_string());
        let members: Vec<(String, String)> = crate::queries::team_users(cx, team_id)
            .iter()
            .map(|user| (user.id.clone(), crate::issue_draft::display_name(user)))
            .collect();
        let items: Vec<crate::picker::PickerItem<String>> =
            std::iter::once(crate::picker::PickerItem::new(LEAD_NONE.to_string(), LEAD.copy.none.clone()))
                .chain(
                    lead_options(members, |member| (member.0.as_str(), member.1.as_str()))
                        .into_iter()
                        .map(|(id, name)| crate::picker::PickerItem::new(id, name)),
                )
                .collect();
        let label = items
            .iter()
            .find(|item| item.value == current)
            .map(|item| item.label.to_string())
            .unwrap_or_else(|| crate::session_chat::member_name(Some(current.as_str()), cx));
        use gpui_component::button::ButtonVariants as _;
        let trigger = gpui_component::button::Button::new(SharedString::from(LEAD.test_ids.row.clone()))
            .accessibility_label(LEAD.copy.label.clone())
            .ghost()
            .cursor_pointer()
            .h_auto()
            .px_0()
            .py_0()
            .text_color(cx.theme().foreground.opacity(0.7))
            .child(crate::surface::picker_value_label(label))
            .child(crate::surface::picker_row_chevron(cx));
        let team_id = team_id.to_string();
        let view = cx.entity().downgrade();
        let control = crate::picker::Picker::single(
            items,
            Some(current.clone()),
            trigger.into_any_element(),
            std::rc::Rc::new(move |next: Vec<String>, _window, cx: &mut App| {
                let Some(pick) = next.into_iter().next() else { return };
                if pick == current {
                    return;
                }
                let team_id = team_id.clone();
                let _ = view.update(cx, |this, cx| this.set_lead(team_id, pick, cx));
            }),
        )
        .disabled(!owner || self.lead_busy || self.lead.is_none())
        .id(SharedString::from(format!("{}-picker", LEAD.test_ids.row)))
        .render(window, cx);
        crate::surface::glass_group_rows(vec![crate::surface::glass_picker_row(
            LEAD.copy.label.clone(),
            None,
            control,
            cx,
        )])
        .into_any_element()
    }

    /// #EXP-1735: `agents.setLeadAssistant` (owner-only server-side). The
    /// pick shows at once; settled or failed, the row re-reads `agents.lead`
    /// (a failure toasts with Retry).
    fn set_lead(&mut self, team_id: String, pick: String, cx: &mut gpui::Context<Self>) {
        if self.lead_busy {
            return;
        }
        let Some(trpc) = crate::queries::trpc_client(cx) else {
            crate::action_error::failed_action("save", "the lead Assistant", None, cx);
            return;
        };
        self.lead_busy = true;
        self.lead = Some((pick != domain::agent_settings::LEAD_NONE).then(|| pick.clone()));
        cx.notify();
        let view = cx.entity().downgrade();
        cx.spawn(async move |this, cx| {
            let (id, user) = (team_id.clone(), pick.clone());
            let result = cx
                .background_executor()
                .spawn(async move {
                    let user = (user != domain::agent_settings::LEAD_NONE).then_some(user);
                    api::agents::set_lead_assistant(&trpc, &id, user.as_deref())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.lead_busy = false;
                // Re-read the lead on the next frame (the server's truth).
                this.lead_key = None;
                if let Err(err) = result {
                    log::warn!("[ui] agents.setLeadAssistant failed: {err}");
                    crate::action_error::failed_action_with_retry(
                        "save",
                        "the lead Assistant",
                        crate::action_error::action_error_description(&err),
                        move |_, cx| {
                            let (team_id, pick) = (team_id.clone(), pick.clone());
                            let _ = view.update(cx, |this, cx| this.set_lead(team_id, pick, cx));
                        },
                        cx,
                    );
                }
                cx.notify();
            });
        })
        .detach();
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
                |user_id| crate::session_chat::member_name(Some(user_id), cx),
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
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "AgentsIndexPane").entered();
        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return v_flex().child(crate::controls::empty_state_list(
                Icon::new(registry::UI_TEAM),
                crate::empty_state_copy::NO_TEAM_SELECTED,
                cx,
            ));
        };
        let owner = super::is_owner(cx, &team_id);
        // #EXP-1735: the Lead Assistant row tops the pane (where the
        // Coordinator row was).
        self.ensure_lead(&team_id, cx);
        let lead = self.render_lead(&team_id, owner, window, cx);
        v_flex()
            .w_full()
            .gap_6()
            .child(lead)
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
