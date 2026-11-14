//! #EXP-1846 (agent-settings.json `page` › `triggers`, `checkIn`): THE
//! trigger list of an agent, ONE piece every document showing an agent's
//! triggers mounts (the agent page, the Assistant's, an app's Events tab,
//! #EXP-1829): the "Triggers" band with its `+ Add` pill (editors), then one
//! group: the Check in row (THE row Select over the presets,
//! `check_in_of` / `with_check_in`) and a row per `listed_triggers`
//! (sentence · Switch · ⋯ Edit/Delete; the row opens the editor).
//!
//! A host document mounts [`section`], writes [`project`] (a synced agent)
//! or [`project_list`] (the New agent page's draft) at `/pane/triggerList`,
//! registers [`ACTIONS`] and routes them to [`handle`] with its
//! [`ListTarget`].
//! Every write is the WHOLE array, each trigger kept as stored (its `preset`
//! included).

use gpui::{App, Context, Window};
use serde_json::{json, Value};

use domain::agent_settings::{check_in_of, listed_triggers, trigger_summary, with_check_in, AGENT_SETTINGS, ASSISTANT};
use domain::rows::AgentDefRow;

use crate::native_dialog::{self, AlertSpec};
use crate::queries;
use crate::trigger_dialog::{self, DraftTriggers};
use crate::trigger_editor;

use super::agent_doc::{select_row, Chrome};
use super::doc::{path, Ink, SM};

/// The data-model path the projection is written at.
pub(crate) const BASE: &str = "/pane/triggerList";

/// The document actions the list raises; a host registers them all and
/// routes them to [`handle`].
pub(crate) const ACTIONS: [&str; 5] = ["checkIn", "triggerAdd", "triggerOpen", "triggerToggle", "triggerMenu"];

fn at(rel: &str) -> Value {
    path(&format!("{BASE}/{rel}"))
}

/// Where the list's writes land: a synced agent (`agents.update`, whole
/// array) or the New agent page's local draft (sent with `agents.create`).
#[derive(Clone)]
pub(crate) enum ListTarget {
    Agent(String),
    Draft(DraftTriggers),
}

impl ListTarget {
    fn current(&self, cx: &App) -> Vec<Value> {
        match self {
            ListTarget::Agent(agent_id) => trigger_editor::agent_triggers(agent_id, cx),
            ListTarget::Draft(draft) => (draft.read)(cx),
        }
    }

    fn open_new(&self, window: &mut Window, cx: &mut App) {
        match self {
            ListTarget::Agent(agent_id) => trigger_dialog::open_new(window, cx, agent_id.clone()),
            ListTarget::Draft(draft) => trigger_dialog::open_new_draft(window, cx, draft.clone()),
        }
    }

    fn open_edit(&self, window: &mut Window, cx: &mut App, key: String) {
        match self {
            ListTarget::Agent(agent_id) => trigger_dialog::open_edit(window, cx, agent_id.clone(), key),
            ListTarget::Draft(draft) => trigger_dialog::open_edit_draft(window, cx, draft.clone(), key),
        }
    }
}

/// The list's own state on its host: a whole-array write in flight (the
/// next one starts from this one's array, so one at a time).
#[derive(Default)]
pub(crate) struct AgentTriggers {
    writing: bool,
}

/// Who edits an agent's triggers (`page` EDITORS): a team agent's are its
/// team's owners', the Assistant's its member's.
pub(crate) fn editor_of(agent: &AgentDefRow, team_id: &str, cx: &App) -> bool {
    if agent.is_assistant() {
        let me = queries::active_account(cx).map(|account| account.user_id);
        return me.is_some() && agent.owner_id == me;
    }
    super::is_owner(cx, team_id)
}

/// The machine's IANA zone (a NEW Check in takes it; a stored one keeps its own).
fn local_zone() -> Option<String> {
    iana_time_zone::get_timezone().ok().map(|zone| zone.trim().to_string()).filter(|zone| !zone.is_empty())
}

/// A synced agent's list projection (written at [`BASE`]).
pub(crate) fn project(state: &AgentTriggers, agent_id: &str, editor: bool, cx: &App) -> Value {
    project_list(&trigger_editor::agent_triggers(agent_id, cx), editor, state.writing)
}

/// The list's projection over `triggers` (written at [`BASE`]).
pub(crate) fn project_list(triggers: &[Value], editor: bool, writing: bool) -> Value {
    let rows: Vec<Value> = listed_triggers(triggers)
        .into_iter()
        .filter_map(|(_, trigger)| {
            let id = trigger_editor::trigger_id(trigger)?.to_string();
            let sentence = trigger_summary(trigger);
            Some(json!({
                "key": id,
                "summary": sentence,
                "enabled": trigger_editor::trigger_enabled(trigger),
                "menuLabel": format!("Trigger menu for {sentence}"),
                "items": [
                    {"label": "Edit", "value": "edit", "icon": "ui-edit"},
                    {"label": "Delete", "value": "delete", "icon": "ui-delete", "destructive": true},
                ],
            }))
        })
        .collect();
    json!({
        "editor": editor,
        "readOnly": !editor,
        "canAdd": editor && triggers.len() < domain::contract::AGENT_TRIGGER_MAX_PER_AGENT,
        // A member's switch reads; an editor's waits for the write in flight.
        "busy": writing || !editor,
        "checkIn": {"value": check_in_of(triggers), "options": ASSISTANT.schedules.iter().map(|preset| json!({"label": preset.label, "value": preset.id})).collect::<Vec<_>>()},
        "hasTriggers": !rows.is_empty(),
        "triggers": rows,
    })
}

/// The section's tree: the "Triggers" band (`+ Add` on its trailing edge
/// for editors) over ONE group: Check in, then the listed triggers.
pub(super) fn section(ink: &Ink, chrome: &Chrome) -> Value {
    let copy = &AGENT_SETTINGS.copy;
    let [mut add, _] = ink.pill_button("trigger_add", Some("ui-add"), json!(copy.add_trigger.clone()), json!(false), json!({"name": "triggerAdd"}));
    add["visible"] = at("canAdd");
    // gpui centres the label's line half a point lower than the flex centre.
    add["children"][1]["style"]["paddingTop"] = json!(1);
    let band = ink.band("triggers_band", &copy.triggers, Some(add));
    let mut check_in = select_row("check_in_select", &copy.check_in, "triggerList/checkIn", "checkIn", json!({}));
    check_in["props"]["disabled"] = at("readOnly");
    // One trigger row: `picker_value_label` (one line, at most 240 px).
    let mut summary = ink.text("trigger_summary", path("summary"), SM, &ink.foreground);
    summary["props"]["lines"] = json!(1);
    summary["style"]["minWidth"] = json!(0);
    summary["style"]["maxWidth"] = json!(240);
    // The ghost Button's run sits on its own line box, under the compact
    // switch's 16 px: a member's row (no ⋯) is the switch's height.
    summary["style"]["lineHeight"] = json!(16);
    // gpui snaps the Button's run half a pixel up inside its 32 px row.
    summary["style"]["position"] = json!("relative");
    summary["style"]["top"] = json!(-0.5);
    let summary = json!({"id": "trigger_open", "component": "Box", "props": {"pressable": true}, "on": {"press": {"event": {"name": "triggerOpen", "context": {"key": path("key")}}}},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "flexGrow": 1, "flexShrink": 1, "minWidth": 0, "cursor": "pointer"}, "children": [summary]});
    let switch = json!({"id": "trigger_switch", "component": "Switch", "props": {"label": "", "name": "enabled", "size": "compact", "checked": path("enabled"), "disabled": at("busy")},
        "on": {"change": {"event": {"name": "triggerToggle", "context": {"key": path("key")}}}}});
    // a11y: named by the caller ("Trigger menu for {sentence}").
    let mut more = ink.ghost_icon_button("trigger_more", "ui-more", path("menuLabel"), Value::Null, &chrome.list_hover);
    more.as_object_mut().expect("the trigger").remove("on");
    let menu = json!({"id": "trigger_menu", "component": "Menu", "visible": at("editor"), "props": {"items": path("items"), "align": "start"}, "on": {"select": {"event": {"name": "triggerMenu", "context": {"key": path("key")}}}}, "children": [more]});
    let trailing = json!({"id": "trigger_trailing", "component": "Box", "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "flexShrink": 0, "gap": ink.r(0.25)}, "children": [switch, menu]});
    let row = ink.shell("trigger", false, json!(true), vec![summary, trailing]);
    let row = json!({"id": "trigger_wrap", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"}, "children": [ink.divider("trigger_divider", json!(true)), row]});
    let list = json!({"id": "triggers", "component": "Box", "visible": at("hasTriggers"), "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "template": {"component": "trigger_wrap", "path": format!("{BASE}/triggers"), "key": "key"}, "children": [row]});
    json!({"id": "triggers_section", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"},
        "children": [band, ink.group("triggers_group", vec![check_in, list])]})
}

/// One of [`ACTIONS`] on `target`'s list. `state` reaches the list's
/// [`AgentTriggers`] on the host. A member's actions do nothing.
#[allow(clippy::too_many_arguments)]
pub(crate) fn handle<V: 'static>(
    host: &mut V,
    state: fn(&mut V) -> &mut AgentTriggers,
    name: &str,
    target: ListTarget,
    editor: bool,
    context: &Value,
    payload: &Value,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    if !editor {
        return;
    }
    let value = super::doc::picked_value(payload).unwrap_or_default().to_string();
    let key = context.get("key").and_then(Value::as_str).unwrap_or_default().to_string();
    match name {
        // A pick writes the WHOLE array with the preset trigger replaced in
        // front (its id and zone kept) or removed for `off`.
        "checkIn" if !value.is_empty() => {
            let triggers = target.current(cx);
            if check_in_of(&triggers) == value {
                return;
            }
            let next = with_check_in(&triggers, &value, local_zone().as_deref())
                .into_iter()
                .map(|mut trigger| {
                    // A new Check in mints its id here (as web does), so a
                    // write racing its echo keeps the same trigger.
                    if trigger_editor::trigger_id(&trigger).is_none() {
                        trigger["id"] = json!(uuid::Uuid::new_v4().to_string());
                    }
                    trigger
                })
                .collect();
            commit(host, state, &target, next, "change", "the check-in", cx);
        }
        "triggerAdd" => target.open_new(window, cx),
        "triggerOpen" if !key.is_empty() => target.open_edit(window, cx, key),
        "triggerToggle" if !key.is_empty() => {
            if let Some(on) = payload.get("checked").and_then(Value::as_bool) {
                let triggers = trigger_editor::triggers_with_enabled(&target.current(cx), &key, on);
                commit(host, state, &target, triggers, if on { "resume" } else { "pause" }, "the trigger", cx);
            }
        }
        "triggerMenu" if !key.is_empty() => match value.as_str() {
            "edit" => target.open_edit(window, cx, key),
            "delete" => {
                let view = cx.entity().downgrade();
                let spec = AlertSpec::from_prompt("Delete trigger", &domain::prompts::delete_trigger()).on_ok(move |_, cx| {
                    let (target, key) = (target.clone(), key.clone());
                    let _ = view.update(cx, |host, cx| {
                        let triggers = trigger_editor::triggers_without(&target.current(cx), &key);
                        commit(host, state, &target, triggers, "delete", "the trigger", cx);
                    });
                    true
                });
                native_dialog::open_alert(window, cx, spec);
            }
            _ => {}
        },
        _ => {}
    }
}

/// The whole array lands on `target`: the agent's write, or the draft's
/// (deferred: the draft repaints its host, which is mid-update here).
fn commit<V: 'static>(
    host: &mut V,
    state: fn(&mut V) -> &mut AgentTriggers,
    target: &ListTarget,
    triggers: Vec<Value>,
    verb: &'static str,
    thing: &'static str,
    cx: &mut Context<V>,
) {
    match target {
        ListTarget::Agent(agent_id) => write(host, state, agent_id.clone(), triggers, verb, thing, cx),
        ListTarget::Draft(draft) => {
            let draft = draft.clone();
            cx.defer(move |cx| (draft.write)(triggers, cx));
        }
    }
}

/// One whole-array `agents.update({triggers})`; a failure toasts `verb`.
fn write<V: 'static>(
    host: &mut V,
    state: fn(&mut V) -> &mut AgentTriggers,
    agent_id: String,
    triggers: Vec<Value>,
    verb: &'static str,
    thing: &'static str,
    cx: &mut Context<V>,
) {
    if state(host).writing {
        return;
    }
    let Some(trpc) = queries::trpc_client(cx) else {
        return;
    };
    state(host).writing = true;
    cx.notify();
    let input = api::agents::UpdateInput { triggers: Some(triggers.clone()), ..api::agents::UpdateInput::new(agent_id.clone()) };
    cx.spawn(async move |this, cx| {
        let result = cx.background_executor().spawn(async move { api::agents::update(&trpc, &input) }).await;
        let _ = this.update(cx, |host, cx| {
            state(host).writing = false;
            match result {
                Ok(_) => trigger_editor::note_triggers_written(&agent_id, triggers),
                Err(err) => crate::action_error::failed_action_err(verb, thing, &err, cx),
            }
            cx.notify();
        });
    })
    .detach();
}
