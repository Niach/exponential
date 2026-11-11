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
//!
//! #EXP-1629: ONE document on eUI's native host binding: the Lead Assistant
//! as a `row` Select, the two bands with their skeleton and empty states.

use std::rc::Rc;

use eui_gpui::{binding::Binding, host::HostPlugin, view::SurfaceView};
use gpui::{div, App, AppContext as _, Entity, IntoElement, ParentElement as _, Render, SharedString, Styled as _, Subscription, Window};
use gpui_component::ActiveTheme as _;
use serde_json::{json, Value};
use sync::Store;

use domain::agent_settings::{lead_options, LEAD, LEAD_NONE};
use domain::rows::EventRow;
use domain::session_tree::AgentRow;

use crate::icons::registry;
use crate::navigation::{active_team_id, navigate, Navigation, Screen};
use crate::sidebar::select_settings_section;

use super::doc::{empty_state, hex, path, IdeToasts, Ink, SM};
use super::SettingsSection;

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

/// A state the pictures show (`settings_bench`; headless has no team,
/// membership or synced rows).
#[cfg(test)]
pub(crate) struct AgentsIndexShot {
    /// `None` = no team selected.
    pub team: Option<String>,
    pub owner: bool,
    /// `agents.lead`: `None` = not read yet, `Some(None)` = no lead.
    pub lead: Option<Option<String>>,
    /// The team's members, `(id, display name)`.
    pub members: Vec<(String, String)>,
    pub agents: Vec<domain::session_tree::AgentRow>,
    pub agents_ready: bool,
    pub events: Vec<EventRow>,
    pub events_ready: bool,
    /// The emitters' session titles and person names, by id.
    pub titles: std::collections::HashMap<String, String>,
    pub names: std::collections::HashMap<String, String>,
    /// The Lead Assistant menu, opened by a press on its trigger.
    pub open_lead: bool,
}

/// The data-model path the projection is written at.
const PATH: &str = "/pane";

/// The Lead Assistant menu's width (web Picker `width="md"`, 16rem).
const LEAD_POPUP_WIDTH: f32 = 256.;

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
    binding: Entity<Binding<Value>>,
    view: Entity<SurfaceView>,
    /// The bench's state.
    #[cfg(test)]
    shot: Option<AgentsIndexShot>,
    /// The bench's frames to wait before its press on the lead trigger.
    #[cfg(test)]
    shot_press: Option<u8>,
    _subscriptions: Vec<Subscription>,
}

impl AgentsIndexPane {
    pub fn new(nav: Entity<Navigation>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
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
        let pane = cx.weak_entity();
        let handle = window.window_handle();
        let binding = cx.new(|_| {
            let mut binding = Binding::new(PATH, Value::Null, |state: &Value, _| state.clone());
            binding.set_toasts(Rc::new(IdeToasts));
            for name in ["new", "open", "lead"] {
                let pane = pane.clone();
                binding.on_action(name, move |_, event, cx| {
                    let key = event.context.get("key").and_then(Value::as_str).unwrap_or_default().to_string();
                    let value = event.payload.as_ref().and_then(|p| p.get("value")).and_then(Value::as_str).unwrap_or_default().to_string();
                    let pane = pane.clone();
                    // Deferred: the binding is mid-update.
                    cx.defer(move |cx| {
                        let _ = handle.update(cx, |_, window, cx| match name {
                            "new" => {
                                select_settings_section(window, cx, SettingsSection::NewAgent);
                                navigate(window, cx, Screen::Settings);
                            }
                            "open" => {
                                select_settings_section(window, cx, SettingsSection::Agent(key));
                                navigate(window, cx, Screen::Settings);
                            }
                            _ => {
                                let _ = pane.update(cx, |pane, cx| pane.pick_lead(value, cx));
                            }
                        });
                    });
                    None
                });
            }
            binding
        });
        let (list_hover, skeleton_fill) = (hex(cx.theme().list_hover), hex(cx.theme().muted_foreground.opacity(0.18)));
        let view = super::doc::mount_with("settings-agents-index", document(&Ink::of(cx), &list_hover, &skeleton_fill), &binding, Rc::new(PanePlugin { inner: super::doc::ide_plugin() }), true, window, cx);
        Self {
            nav,
            lead: None,
            lead_key: None,
            lead_busy: false,
            binding,
            view,
            #[cfg(test)]
            shot: None,
            #[cfg(test)]
            shot_press: None,
            _subscriptions: subscriptions,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new(crate::navigation::nav_for_window(window, cx), window, cx)
    }

    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: AgentsIndexShot, cx: &mut gpui::Context<Self>) {
        self.lead = shot.lead.clone();
        self.shot_press = shot.open_lead.then_some(2);
        self.shot = Some(shot);
        cx.notify();
    }

    /// The bench's press once the trigger is laid out; the headless clock
    /// stands still, so the popup's enter fade is run out by hand.
    #[cfg(test)]
    fn press_shot(&mut self, cx: &mut gpui::Context<Self>) {
        match self.shot_press.take() {
            Some(0) => self.view.update(cx, |view, cx| {
                let index = view.index_of("lead.trigger").expect("the lead trigger");
                view.fire(index, "press", None, cx);
            }),
            Some(wait) => self.shot_press = Some(wait - 1),
            None => {
                if !self.view.read(cx).layers().is_empty() {
                    self.view.update(cx, |view, cx| view.advance_clock(std::time::Duration::from_millis(500), cx));
                }
                return;
            }
        }
        cx.notify();
    }

    /// The pane's team (the bench's, under test).
    fn team_id(&self, cx: &App) -> Option<String> {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.team.clone();
        }
        active_team_id(&self.nav, cx)
    }

    fn owner(&self, team_id: &str, cx: &App) -> bool {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.owner;
        }
        super::is_owner(cx, team_id)
    }

    /// The team's members as `(id, display name)`.
    fn members(&self, team_id: &str, cx: &App) -> Vec<(String, String)> {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.members.clone();
        }
        crate::queries::team_users(cx, team_id)
            .iter()
            .map(|user| (user.id.clone(), crate::issue_draft::display_name(user)))
            .collect()
    }

    /// The team's agents and whether their shape is ready.
    fn agents(&self, team_id: &str, cx: &App) -> (Vec<AgentRow>, bool) {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return (shot.agents.clone(), shot.agents_ready);
        }
        (
            crate::session_data::agents_in_team(team_id, cx),
            Store::global(cx).collections().agents.read(cx).is_ready(),
        )
    }

    /// The Events band's rows and whether their shape is ready.
    fn events(&self, team_id: &str, cx: &App) -> (Vec<EventRow>, bool) {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return (team_events(shot.events.iter(), team_id), shot.events_ready);
        }
        let collections = Store::global(cx).collections();
        let collection = collections.events.read(cx);
        (team_events(collection.iter(), team_id), collection.is_ready())
    }

    /// A person's name.
    fn member_name(&self, user_id: &str, cx: &App) -> String {
        #[cfg(test)]
        if let Some(shot) = &self.shot {
            return shot.names.get(user_id).cloned().unwrap_or_else(|| user_id.to_string());
        }
        crate::session_chat::member_name(Some(user_id), cx)
    }

    /// An event row's caption.
    fn emitter(&self, event: &EventRow, cx: &App) -> String {
        event_emitter_label(
            event,
            |session_id| {
                #[cfg(test)]
                if let Some(shot) = &self.shot {
                    return shot.titles.get(session_id).cloned();
                }
                crate::session_data::session_title_of(session_id, cx)
            },
            |user_id| self.member_name(user_id, cx),
        )
    }

    /// #EXP-1735: re-read `agents.lead` when the pane first shows a team, when
    /// the synced `teams.lead_assistant_id` moves (another owner's pick, a
    /// deleted Assistant) and after a save. Called from `render`; the key
    /// is stamped before the fetch, so a failure never loops.
    fn ensure_lead(&mut self, team_id: &str, cx: &mut gpui::Context<Self>) {
        #[cfg(test)]
        if self.shot.is_some() {
            return;
        }
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

    /// The lead's value: the member's id, or `LEAD_NONE`.
    fn lead_value(&self) -> String {
        self.lead.clone().flatten().unwrap_or_else(|| LEAD_NONE.to_string())
    }

    /// The Select's pick (saved at once; re-picking the value is a no-op).
    fn pick_lead(&mut self, pick: String, cx: &mut gpui::Context<Self>) {
        if pick.is_empty() || pick == self.lead_value() {
            return;
        }
        let Some(team_id) = self.team_id(cx) else { return };
        self.set_lead(team_id, pick, cx);
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
        self.lead = Some((pick != LEAD_NONE).then(|| pick.clone()));
        cx.notify();
        let view = cx.entity().downgrade();
        cx.spawn(async move |this, cx| {
            let (id, user) = (team_id.clone(), pick.clone());
            let result = cx
                .background_executor()
                .spawn(async move {
                    let user = (user != LEAD_NONE).then_some(user);
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

    /// #EXP-1735 (agent-settings.json `lead`, web `LeadAssistantRow`): the
    /// value = the lead member's display name or `copy.none`; options = None,
    /// then `lead_options` (the team's members by display name, then id).
    /// Owners pick; members see it disabled.
    fn project_lead(&self, team_id: &str, owner: bool, cx: &App) -> Value {
        let current = self.lead_value();
        let mut options = vec![json!({"label": LEAD.copy.none, "value": LEAD_NONE})];
        options.extend(
            lead_options(self.members(team_id, cx), |member| (member.0.as_str(), member.1.as_str()))
                .into_iter()
                .map(|(id, name)| json!({"label": name, "value": id})),
        );
        // A lead outside the member list still reads as its name.
        if !options.iter().any(|option| option["value"] == current.as_str()) {
            options.push(json!({"label": self.member_name(&current, cx), "value": current}));
        }
        json!({"value": current, "options": options, "disabled": !owner || self.lead_busy || self.lead.is_none()})
    }

    /// The state as the document binds it.
    fn project(&self, team_id: Option<&str>, cx: &App) -> Value {
        let Some(team_id) = team_id else {
            return json!({"noTeam": true, "hasTeam": false, "owner": false, "lead": {"value": "", "options": [], "disabled": true},
                "agents": {"rows": [], "hasRows": false, "loading": false, "empty": false},
                "events": {"rows": [], "hasRows": false, "loading": false, "empty": false}});
        };
        let owner = self.owner(team_id, cx);
        let (agents, agents_ready) = self.agents(team_id, cx);
        let (events, events_ready) = self.events(team_id, cx);
        let foreground = hex(cx.theme().foreground);
        let agent_rows: Vec<Value> = agents
            .iter()
            .map(|agent| {
                // `icons::agent_row_icon`: the Assistant is the brand mark in
                // the row's own ink, an agent its tinted curated glyph.
                let (icon, color) = if agent.assistant {
                    (ASSISTANT_MARK.to_string(), foreground.clone())
                } else {
                    (agent_icon_name(&agent.icon), hex(crate::icons::agent_tint(Some(&agent.color))))
                };
                json!({"key": agent.agent_id, "name": agent.name, "icon": icon, "color": color})
            })
            .collect();
        let event_rows: Vec<Value> = events
            .iter()
            .map(|event| {
                json!({
                    "key": event.id,
                    "name": event.name.clone().unwrap_or_default(),
                    "emitter": self.emitter(event, cx),
                    "at": event.created_at.as_deref().map(crate::inbox::relative_time).unwrap_or_default(),
                })
            })
            .collect();
        let band = |rows: Vec<Value>, ready: bool| {
            let empty = rows.is_empty();
            json!({"rows": rows, "hasRows": !empty, "loading": empty && !ready, "empty": empty && ready})
        };
        json!({
            "noTeam": false,
            "hasTeam": true,
            "owner": owner,
            "lead": self.project_lead(team_id, owner, cx),
            "agents": band(agent_rows, agents_ready),
            "events": band(event_rows, events_ready),
        })
    }
}

impl Render for AgentsIndexPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "AgentsIndexPane").entered();
        let team_id = self.team_id(cx);
        if let Some(team_id) = &team_id {
            self.ensure_lead(team_id, cx);
        }
        let projection = self.project(team_id.as_deref(), cx);
        self.binding.update(cx, |binding, cx| binding.update_state(|state| *state = projection, cx));
        #[cfg(test)]
        self.press_shot(cx);
        div().w_full().min_w_0().child(self.view.clone())
    }
}

/// The Assistant row's glyph name.
const ASSISTANT_MARK: &str = "brand-exponential";

/// `icons::agent_glyph` as a name: the stored curated icon, else the
/// fixture's default, else the agent concept.
fn agent_icon_name(icon: &str) -> String {
    let default = &domain::session_tree::CHATS_LIST.agent_defaults.icon;
    if registry::icon_by_name(icon).is_some() {
        icon.to_string()
    } else if registry::icon_by_name(default).is_some() {
        default.clone()
    } else {
        "ui-agent-source".to_string()
    }
}

/// The pane's plugin: the IDE's, and the Assistant row's mark (the IDE's
/// own logo, `icons::agent_row_icon`).
struct PanePlugin {
    inner: crate::exponential_ui_host::IdePlugin,
}

impl HostPlugin for PanePlugin {
    fn icon(&self, name: &str) -> Option<SharedString> {
        self.inner.icon(name)
    }

    fn icon_element(&self, name: &str, size: f32, color: gpui::Hsla, cx: &App) -> Option<gpui::AnyElement> {
        if name == ASSISTANT_MARK {
            return Some(gpui_component::Icon::from(crate::icons::ExpIcon::Logo).size(gpui::px(size)).text_color(color).into_any_element());
        }
        self.inner.icon_element(name, size, color, cx)
    }

    fn on_unknown(&self, node: &eui_core::surface::PlacedNode) {
        self.inner.on_unknown(node)
    }
}

/// A flat row's frame (`surface::flat_row`, `px_3` / `py_2`, `gap_3`).
fn row_style(ink: &Ink) -> Value {
    json!({"display": "flex", "flexDirection": "row", "alignItems": "center", "width": "100%", "minWidth": 0, "gap": ink.r(0.75), "paddingHorizontal": ink.r(0.75), "paddingVertical": ink.r(0.5), "borderRadius": ink.radius_md})
}

/// One `text_sm` run on one line, truncating.
fn line(ink: &Ink, id: &str, text: Value, color: &str, weight: Option<u32>) -> Value {
    let mut node = ink.text(id, text, SM, color);
    node["props"]["lines"] = json!(1);
    node["style"]["minWidth"] = json!(0);
    node["style"]["flexShrink"] = json!(1);
    if let Some(weight) = weight {
        node["style"]["fontWeight"] = json!(weight);
    }
    node
}

/// A band over its rows, the skeleton and the empty state (`section`:
/// `gap_2`).
#[allow(clippy::too_many_arguments)]
fn band_section(ink: &Ink, id: &str, title: &str, trailing: Option<Value>, data: &str, empty_icon: &str, empty_title: &str, skeleton_fill: &str, row: Value) -> Value {
    let row_wrap = format!("{id}_row_wrap");
    let rows = json!({"id": format!("{id}_rows"), "component": "List", "visible": path(&format!("/pane/{data}/hasRows")), "props": {"gap": "none"},
        "template": {"component": row_wrap, "path": format!("/pane/{data}/rows"), "key": "key"},
        "children": [{"id": row_wrap, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0}, "children": [row]}]});
    json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%", "minWidth": 0},
        "children": [
            ink.band(&format!("{id}_band"), title, trailing),
            empty_state(ink, &format!("{id}_empty"), empty_icon, empty_title, path(&format!("/pane/{data}/empty"))),
            ink.skeleton_rows(&format!("{id}_skeleton"), skeleton_fill, path(&format!("/pane/{data}/loading"))),
            rows,
        ]})
}

/// The pane's tree (`gap_6`): the Lead Assistant `row` Select in its group,
/// the agents band (`New agent` for owners), the Events band; no team = the
/// no-team state.
fn document(ink: &Ink, list_hover: &str, skeleton_fill: &str) -> Value {
    let glyph = ink.r(SM);
    // Owners only: the pill's own `visible` (it is never disabled).
    let [mut new_agent, _] = ink.pill_button("new", Some("ui-add"), json!(domain::agent_settings::AGENT_SETTINGS.copy.new_agent), json!(false), json!({"name": "new"}));
    new_agent["visible"] = path("/pane/owner");
    // gpui centres the label's line half a point lower than the flex centre.
    new_agent["children"][1]["style"]["paddingTop"] = json!(1);

    let lead = json!({"id": "lead", "component": "Select",
        "props": {"label": LEAD.copy.label, "name": "lead", "triggerVariant": "row", "popupWidth": LEAD_POPUP_WIDTH, "value": path("/pane/lead/value"), "options": path("/pane/lead/options"), "disabled": path("/pane/lead/disabled")},
        "on": {"change": {"event": {"name": "lead"}}}});

    let mut agent_name = line(ink, "agent_name", path("name"), &ink.foreground, Some(500));
    agent_name["style"]["flexGrow"] = json!(1);
    let mut agent_row_style = row_style(ink);
    agent_row_style["cursor"] = json!("pointer");
    agent_row_style[":hover"] = json!({"backgroundColor": list_hover});
    let agent_row = json!({"id": "agent_row", "component": "Box", "props": {"pressable": true}, "accessibility": {"label": path("name")}, "on": {"press": {"event": {"name": "open", "context": {"key": path("key")}}}},
        "style": agent_row_style,
        "children": [
            {"id": "agent_icon", "component": "Icon", "props": {"name": path("icon")}, "style": {"width": glyph, "height": glyph, "color": path("color"), "flexShrink": 0}},
            agent_name,
        ]});

    let mut emitter = line(ink, "event_emitter", path("emitter"), &ink.muted, None);
    emitter["style"]["flexGrow"] = json!(1);
    emitter["style"]["flexBasis"] = json!(0);
    let mut at = ink.text("event_at", path("at"), SM, &ink.muted);
    at["style"]["flexShrink"] = json!(0);
    // Web `tabular-nums`: the times line up.
    at["style"]["fontVariantNumeric"] = json!("tabular-nums");
    let event_row = json!({"id": "event_row", "component": "Box", "style": row_style(ink),
        "children": [
            {"id": "event_icon", "component": "Icon", "props": {"name": "trigger-event"}, "style": {"width": glyph, "height": glyph, "color": ink.muted, "flexShrink": 0}},
            line(ink, "event_name", path("name"), &ink.foreground, Some(500)),
            emitter,
            at,
        ]});

    let page = json!({"id": "page", "component": "Box", "visible": path("/pane/hasTeam"), "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(1.5), "width": "100%", "minWidth": 0},
        "children": [
            ink.group("lead_group", vec![lead]),
            band_section(ink, "agents", super::AGENTS_GROUP, Some(new_agent), "agents", "settings-agents", AGENTS_EMPTY, skeleton_fill, agent_row),
            band_section(ink, "events", EVENTS_TITLE, None, "events", "trigger-event", EVENTS_EMPTY, skeleton_fill, event_row),
        ]});
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%", "minWidth": 0},
        "children": [page, empty_state(ink, "no_team", "ui-team", crate::empty_state_copy::NO_TEAM_SELECTED, path("/pane/noTeam"))]})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ink() -> Ink {
        Ink {
            rem: 14.,
            primary: "#fafafaff".into(),
            foreground: "#ffffffff".into(),
            hint: "#ffffffff".into(),
            value: "#ffffffff".into(),
            muted: "#a1a1aaff".into(),
            muted_glyph: "#a1a1aaff".into(),
            fill_row: "#ffffff0a".into(),
            stroke_row: "#ffffff14".into(),
            fill_card: "#ffffff0f".into(),
            stroke_card: "#ffffff1a".into(),
            code: "#27272aff".into(),
            radius_lg: 12.,
            radius_md: 8.,
            fill_section: "#ffffff0a".into(),
            fill_active: "#ffffff1a".into(),
            mono: "$type.family.mono".into(),
            pill_button_fill: "#ffffff0f".into(),
            pill_button_fill_disabled: "#ffffff0b".into(),
            primary_button_fill: "#f4f4f5ff".into(),
            primary_button_hover: "#e4e4e7ff".into(),
            primary_button_fill_disabled: "#f4f4f580".into(),
            primary_button_fg: "#18181bff".into(),
        }
    }

    /// #EXP-1629: the document raises no issue a debug build's mount check
    /// would refuse (only the IDE's literal chrome and the bound values).
    #[test]
    fn the_document_raises_no_unexpected_issue() {
        let tree = document(&ink(), "#ffffff0a", "#a1a1aa2e");
        let bound = super::super::doc::bound_values(&tree);
        let mut surface = eui_core::surface::Surface::new("settings-agents-index", Default::default());
        let outcome = surface.set_nested(serde_json::from_value(tree).expect("parses"));
        let issues = super::super::doc::unexpected_issues(&bound, &outcome.issues);
        assert!(issues.is_empty(), "{issues:?}");
    }

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
