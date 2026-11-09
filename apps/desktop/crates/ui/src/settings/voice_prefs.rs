//! NEW-67 (voice plan v3, V1) — Settings → Personal → Voice
//! (`fixtures/voice.json`): the member's ONE voice settings row, read with
//! `voice.get`, written with `voice.update` (server-only, like the
//! notification prefs). Group 1: Voice device (the member's desktops), Agent,
//! Language (S-3: replies to the voice are always spoken, no Speak replies
//! row). While a voice device is picked, group 2 under "On {device} (tray)":
//! Push to talk (S-3: the key binding as keycaps; on the voice device's own
//! IDE THE recorder, `capture_step` over the key events; read-only
//! elsewhere), Wake word (its detail = `wake_detail`), Wake phrase. The wake
//! word reads off and is disabled while the device or the agent is None
//! (clearOnNone). A save hands the stored settings to `speak_replies` at
//! once, so this device starts or stops listening without waiting for the
//! next heartbeat.
//!
//! #EXP-1629: the pane is a document on eUI's native host binding
//! (`eui_gpui::binding`): [`Voice`] is the state, [`project`] its data model
//! at `/pane`, [`document`] the tree (the glass rows' geometry and colours,
//! the IDE's chrome written in). Push to talk is genUI's `KeyRecorder` (the
//! bound keys a `KbdGroup`): the component captures and shows; THIS adapter
//! keeps Exponential's grammar (`capture_step` over its `key` events) and
//! pauses the global push-to-talk grab between `start` and the end. The
//! styleguide twin (`styleguide/entries/voice_settings.rs`) mounts the same
//! document over static values ([`VoicePrefsPane::twin`]).

use std::rc::Rc;

use eui_gpui::{
    binding::{Binding, Mutation},
    host::{ActionEvent, InputEvent},
    view::SurfaceView,
};
use gpui::{div, App, AppContext as _, Entity, IntoElement, ParentElement as _, Render, Styled as _, Subscription, WeakEntity, Window};
use gpui_component::ActiveTheme as _;
use serde_json::{json, Value};

use api::voice::VoiceUpdate;
use etalk::voice::{
    agent_options, capture_pending, capture_start, capture_step, language_label, shortcut_keycaps, shortcut_reason, tray_header,
    voice_device_options, voice_text_input, wake_phrase_error, wake_phrase_placeholder, CaptureEvent, CaptureState, CaptureStatus,
    VoiceAgent, VoiceDevice, VoiceOption, VoiceSettings, VOICE,
};

use etalk::meeting::{app_options, apps_value, desktop_audio_options, AudioApp, DesktopAudio, DesktopAudioCapability, MEETING};

use super::doc::{and, faded, hex, mount, not, path, skeleton, IdeToasts, Ink, HINT_ALPHA, SM, VALUE_ALPHA, XS};
use crate::queries;

/// The member's agents (every team's), by id: the pickers' options and the
/// bare names the wake phrase and the tray use.
pub(crate) fn voice_agents(cx: &App) -> Vec<VoiceAgent> {
    let Some(store) = sync::Store::try_global(cx) else { return Vec::new() };
    let collections = store.collections();
    let teams = collections.teams.read(cx);
    collections
        .agents
        .read(cx)
        .iter()
        .map(|agent| VoiceAgent {
            id: agent.id.clone(),
            name: agent.name.clone().unwrap_or_default(),
            team: agent
                .team_id
                .as_deref()
                .and_then(|team_id| teams.get(team_id))
                .map(|team| team.name.clone())
                .unwrap_or_default(),
        })
        .collect()
}

/// The member's devices and agents as the pickers offer them.
pub(crate) fn voice_choices(cx: &App) -> (Vec<VoiceOption>, Vec<VoiceOption>) {
    let Some(store) = sync::Store::try_global(cx) else {
        return (voice_device_options(&[]), agent_options(&[]));
    };
    let me = queries::active_account(cx).map(|account| account.user_id);
    let collections = store.collections();
    let devices: Vec<VoiceDevice> = collections
        .devices
        .read(cx)
        .iter()
        .filter(|row| me.is_some() && row.user_id == me)
        .filter_map(|row| {
            Some(VoiceDevice {
                device_id: row.device_id.clone()?,
                name: row.label.clone().filter(|label| !label.trim().is_empty()).or_else(|| row.device_id.clone())?,
                kind: row.kind.clone().unwrap_or_default(),
            })
        })
        .collect();
    (voice_device_options(&devices), agent_options(&voice_agents(cx)))
}

/// A picker's value: the option's label; a stored id whose row is not
/// synced here (the shape not ready, the row gone) reads `unsynced`, never
/// its raw id nor None (#NEW-71, web `voice-settings-card`).
fn label_of(options: &[VoiceOption], id: Option<&str>, unsynced: &str) -> String {
    options
        .iter()
        .find(|option| option.id.as_deref() == id)
        .map(|option| option.label.clone())
        .or_else(|| id.map(|_| unsynced.to_string()))
        .unwrap_or_default()
}

/// S-3: the Wake word row's detail on a client that is NOT the voice
/// device: the voice device's doctor row `wake` verbatim (voice.json WAKE
/// DETAIL), after `wake_detail`'s own noAgent / offline.
fn remote_wake_detail(settings: &VoiceSettings, device_name: &str, cx: &App) -> Option<String> {
    let id = settings.voice_device_id.as_deref()?;
    let row = sync::Store::try_global(cx)
        .and_then(|store| store.collections().devices.read(cx).iter().find(|row| row.device_id.as_deref() == Some(id)).cloned());
    let online = row.as_ref().is_some_and(|row| {
        crate::device_settings::row_is_online(row.last_seen_at.as_deref(), chrono::Utc::now().timestamp_millis())
    });
    let reported = row
        .as_ref()
        .and_then(|row| crate::device_readiness::parse(row.doctor.as_ref()))
        .and_then(|doctor| {
            doctor
                .items
                .into_iter()
                .find(|item| item.key == coding::device_doctor::KEY_WAKE && item.group == coding::device_doctor::DoctorGroup::Voice)
        })
        .and_then(|item| item.detail);
    let context = etalk::voice::WakeDetailContext {
        wake_word: settings.wake_word,
        agent: settings.agent_id.is_some(),
        online,
        device: device_name,
        state: None,
    };
    let detail = etalk::voice::wake_detail(&context)?;
    // Listening / muted / … come from the device itself; `starting` until
    // its doctor says.
    Some(if online && settings.agent_id.is_some() { reported.unwrap_or(detail) } else { detail })
}

/// S-3: this desktop IS the voice device.
pub(crate) fn is_voice_device(settings: &VoiceSettings, cx: &App) -> bool {
    #[cfg(test)]
    if let Some(here) = SHOT_HERE.with(|here| here.get()) {
        return here;
    }
    cx.try_global::<crate::session::AuthContext>()
        .is_some_and(|auth| settings.voice_device_id.as_deref() == Some(steer::persistent_device_id(&auth.data_dir).as_str()))
}

#[cfg(test)]
thread_local! {
    /// The bench's "this desktop is the voice device".
    pub(crate) static SHOT_HERE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// A state the pictures show (`settings_bench`).
#[cfg(test)]
pub(crate) struct VoiceShot {
    pub settings: VoiceSettings,
    pub capture: Option<CaptureState>,
}

/// The picked agent's bare name (`Hey Studio`, never `Hey Studio · Acme`);
/// the fixture's agent label while none is picked, as on web.
pub(crate) fn agent_name(settings: &VoiceSettings, agents: &[VoiceAgent]) -> String {
    settings
        .agent_id
        .as_deref()
        .and_then(|id| agents.iter().find(|agent| agent.id == id))
        .map(|agent| agent.name.clone())
        .unwrap_or_else(|| VOICE.copy.agent.clone())
}

/// The wake phrase's placeholder for the picked agent.
pub(crate) fn wake_placeholder(settings: &VoiceSettings, agents: &[VoiceAgent]) -> String {
    wake_phrase_placeholder(&agent_name(settings, agents))
}

/// The data-model path the pane's projection is written at.
const PATH: &str = "/pane";

/// A picker's value for None (a menu item's value is a string).
const NONE: &str = "none";

enum Load {
    Idle,
    Loading,
    Ready(VoiceSettings),
    Error,
}

/// The styleguide twin's static world: the options, the agents and the
/// meeting capability it draws over (no store, no writes, read-only keys).
#[derive(Clone)]
pub(crate) struct Fixed {
    pub devices: Vec<VoiceOption>,
    pub agents: Vec<VoiceOption>,
    pub agent_rows: Vec<VoiceAgent>,
    pub capability: DesktopAudioCapability,
}

/// The pane's state.
pub(crate) struct Voice {
    load: Load,
    generation: u64,
    account_id: Option<String>,
    /// Last write wins: only the newest write may adopt or revert.
    write: u64,
    /// Writes not answered yet (a stale page is not refetched over them).
    in_flight: u32,
    /// The speaker's settings as last seen: a change there (a heartbeat, a
    /// tray toggle) drops the cache.
    seen: VoiceSettings,
    /// #NEW-56: a too-short wake phrase, never sent: its line shows under
    /// the tray group until the next commit (`wakePhraseError`).
    wake_phrase_invalid: Option<String>,
    /// S-3: the push-to-talk recorder while it records.
    capture: Option<CaptureState>,
    /// S-3: drawn as the Assistant page's Voice section (no Voice device /
    /// Agent pickers).
    assistant_block: bool,
    /// The styleguide twin.
    fixed: Option<Fixed>,
}

impl Voice {
    fn ready(&self) -> Option<&VoiceSettings> {
        match &self.load {
            Load::Ready(settings) => Some(settings),
            _ => None,
        }
    }
}

/// What the tree is built over (its menus and placeholders): the document
/// is rebuilt when it changes (`VoicePrefsPane::render`).
#[derive(Clone, PartialEq)]
struct Options {
    devices: Vec<(String, String)>,
    agents: Vec<(String, String)>,
    languages: Vec<(String, String)>,
    auto_stop: Vec<(String, String)>,
    desktop_audio: Vec<(String, String)>,
    apps: Vec<(String, String)>,
    wake_placeholder: String,
}

fn pairs(list: &[VoiceOption]) -> Vec<(String, String)> {
    list.iter().map(|option| (option.id.clone().unwrap_or_else(|| NONE.to_string()), option.label.clone())).collect()
}

fn choices(state: &Voice, cx: &App) -> (Vec<VoiceOption>, Vec<VoiceOption>, Vec<VoiceAgent>) {
    match &state.fixed {
        Some(fixed) => (fixed.devices.clone(), fixed.agents.clone(), fixed.agent_rows.clone()),
        None => {
            let (devices, agents) = voice_choices(cx);
            (devices, agents, voice_agents(cx))
        }
    }
}

fn audio(state: &Voice, settings: &VoiceSettings, cx: &App) -> (DesktopAudioCapability, Vec<AudioApp>) {
    match &state.fixed {
        Some(fixed) => (fixed.capability, Vec::new()),
        None => crate::meeting_mode::desktop_audio_of(settings, cx),
    }
}

fn options(state: &Voice, cx: &App) -> Options {
    let settings = state.ready().cloned().unwrap_or_default();
    let (devices, agents, agent_rows) = choices(state, cx);
    let device_name = settings.voice_device_id.as_deref().map(|id| label_of(&devices, Some(id), &VOICE.copy.ellipsis));
    let (capability, live) = audio(state, &settings, cx);
    Options {
        devices: pairs(&devices),
        agents: pairs(&agents),
        // Every language carries its GPU note (#NEW-74), in the row and in the menu.
        languages: VOICE.languages.iter().map(|entry| (entry.value.clone(), language_label(&entry.value, device_name.as_deref()))).collect(),
        auto_stop: etalk::meeting::auto_stop_options().into_iter().map(|option| (option.value.to_string(), option.label)).collect(),
        desktop_audio: desktop_audio_options(capability).into_iter().map(|value| (value.key().to_string(), value.label().to_string())).collect(),
        apps: app_options(&live, &settings.desktop_audio_apps).into_iter().map(|option| (option.value, option.label)).collect(),
        wake_placeholder: wake_placeholder(&settings, &agent_rows),
    }
}

/// The state as the document binds it.
fn project(state: &Voice, cx: &App) -> Value {
    let copy = &VOICE.copy;
    let Some(settings) = state.ready() else {
        return json!({"loading": matches!(state.load, Load::Idle | Load::Loading), "failed": matches!(state.load, Load::Error), "ready": false, "hasTray": false});
    };
    let (devices, agents, _) = choices(state, cx);
    let live = state.fixed.is_none();
    let ready = settings.voice_device_id.is_some() && settings.agent_id.is_some();
    // voice.json: while the voice downloads (only ever on the voice device,
    // i.e. here) a row's detail reads `downloadProgress`; S-3 removed the
    // Speak replies row, so the Language row (the voice) carries it.
    let downloading = (live && ready).then(|| etalk_gpui::speak_replies::download_detail(cx)).flatten();
    // The stored device names the tray group even while its row is not
    // synced here (its label, else `copy.ellipsis`), as on web.
    let device_name = settings.voice_device_id.as_deref().map(|id| label_of(&devices, Some(id), &copy.ellipsis));
    let language = settings.language().to_string();

    let mut tray = Value::Null;
    if let Some(device_name) = device_name.as_deref() {
        // S-3: on the voice device itself the Push to talk row is THE
        // recorder; a binding that can't be held system-wide says why under
        // its label (#NEW-54). Everywhere else it is read-only.
        let here = live && is_voice_device(settings, cx);
        let status = if here && settings.push_to_talk() { etalk_gpui::push_to_talk::status(cx) } else { Default::default() };
        let reason = status.reason().map(|reason| shortcut_reason(reason, &settings.ptt_shortcut));
        // S-3 (WAKE DETAIL ×4): why the wake word is or is not listening:
        // here from the worker, elsewhere from the voice device's doctor.
        let wake_detail = if here {
            etalk_gpui::push_to_talk::own_wake_detail(settings, device_name, cx)
        } else if live {
            remote_wake_detail(settings, device_name, cx)
        } else {
            None
        };
        let capture = state.capture.as_ref().filter(|capture| here && capture.status == CaptureStatus::Recording);
        // The held modifiers' keycaps (`capture_pending` without its `…`).
        let separator = &VOICE.shortcut_grammar.separator;
        let pending: Vec<String> = capture
            .map(capture_pending)
            .filter(|pending| !pending.is_empty())
            .map(|pending| pending.trim_end_matches('…').trim_end_matches(separator.as_str()).split(separator.as_str()).map(str::to_string).collect())
            .unwrap_or_default();
        let keys = shortcut_keycaps(&settings.ptt_shortcut);
        let meeting_on = settings.meeting();
        let (capability, _) = audio(state, settings, cx);
        let apps_shown = settings.desktop_audio.picks_apps() && desktop_audio_options(capability).contains(&settings.desktop_audio);
        let hours = settings.auto_stop_hours();
        tray = json!({
            "header": tray_header(device_name),
            "ptt": {
                "here": here,
                "readOnly": !here,
                "keys": keys,
                "bound": !keys.is_empty(),
                "unbound": keys.is_empty(),
                "recording": capture.is_some(),
                "pending": pending,
                "clear": here && !keys.is_empty() && capture.is_none(),
                "reason": reason,
                "hint": reason.is_some(),
                "error": capture.and_then(|capture| capture.error.clone()),
                "hasError": capture.is_some_and(|capture| capture.error.is_some()),
            },
            "wake": {"checked": settings.wake_word && ready, "disabled": !ready, "detail": wake_detail, "hint": wake_detail.is_some()},
            "meeting": {
                "checked": meeting_on && ready,
                "disabled": !ready,
                "detail": meeting_on.then(|| etalk::meeting::who(settings.desktop_audio, &settings.desktop_audio_apps, capability)),
                "hint": meeting_on,
            },
            "autoStop": {"value": hours.to_string(), "label": etalk::meeting::auto_stop_option(hours), "disabled": !ready},
            "desktopAudio": {"value": settings.desktop_audio.key(), "label": settings.desktop_audio.label()},
            "apps": {"shown": apps_shown, "value": settings.desktop_audio_apps.iter().map(|app| app.id.as_str()).collect::<Vec<_>>().join(",")},
            "wakeInvalid": state.wake_phrase_invalid,
            "hasWakeInvalid": state.wake_phrase_invalid.is_some(),
        });
    }
    json!({
        "loading": false,
        "failed": false,
        "ready": true,
        "pickers": !state.assistant_block,
        // #NEW-96: a device row not (yet) synced leaves the value empty, as
        // web's picker row does (never `…`); web's agent picker trails nothing
        // for an agent not synced here.
        "device": {"value": settings.voice_device_id.clone().unwrap_or_else(|| NONE.to_string()), "label": label_of(&devices, settings.voice_device_id.as_deref(), "")},
        "agent": {"value": settings.agent_id.clone().unwrap_or_else(|| NONE.to_string()), "label": label_of(&agents, settings.agent_id.as_deref(), "")},
        "language": {"value": language, "label": language_label(&language, device_name.as_deref()), "detail": downloading, "hint": downloading.is_some()},
        "fields": {"wakePhrase": settings.wake_phrase, "voiceFile": settings.voice_file, "voiceTranscript": settings.voice_transcript},
        "hasTray": !tray.is_null(),
        "tray": tray,
    })
}

/// What a Voice tree writes in: the IDE's chrome and the danger ink.
struct Inks {
    ink: Ink,
    danger: String,
}

impl Inks {
    fn of(cx: &App) -> Self {
        Self { ink: Ink::of(cx), danger: hex(cx.theme().danger) }
    }
}

/// A row's label column (`glass_picker_row` / `glass_toggle_row`): the
/// `text_sm` label over its `text_xs` hint at 50% (`gap_0p5`, the hint
/// capped at `ROW_DESCRIPTION_MAX_W`); without a hint a picker's label
/// never shrinks, a toggle's grows.
fn label_column(ink: &Ink, id: &str, label: &str, hint: Option<Value>, grow: bool) -> Value {
    let mut children = vec![ink.text(&format!("{id}_label"), json!(label), SM, &ink.foreground)];
    let has_hint = hint.is_some();
    if let Some(hint) = hint {
        let mut hint = faded(ink.text(&format!("{id}_hint"), hint, XS, &ink.hint), HINT_ALPHA);
        hint["style"]["maxWidth"] = json!(eui_gpui::controls::ROW_DESCRIPTION_MAX_W);
        children.push(hint);
    }
    let shrink = if grow || has_hint { 1 } else { 0 };
    json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.125), "minWidth": 0, "flexGrow": if grow { 1 } else { 0 }, "flexShrink": shrink}, "children": children})
}

/// `glass_picker_row`'s trailing column: `flex_1`, at least 200 wide, its
/// control at the end.
fn control_column(id: &str, children: Vec<Value>) -> Value {
    json!({"id": id, "component": "Box", "style": {"display": "flex", "flexDirection": "row", "justifyContent": "flex-end", "flexGrow": 1, "minWidth": 200}, "children": children})
}

/// A row's Select options.
fn select_options(list: &[(String, String)]) -> Vec<Value> {
    list.iter().map(|(value, label)| json!({"label": label, "value": value})).collect()
}

/// The hinted row's Menu entries: plain items, so a pick closes it.
fn menu_items(list: &[(String, String)]) -> Vec<Value> {
    list.iter().map(|(value, label)| json!({"label": label, "value": value})).collect()
}

/// A picker row (`glass_picker_row` + the IDE's Picker popup): genUI's
/// `Select` with the `row` trigger; a pick closes it (`multiple` stays open).
fn select_row(id: &str, visible: Value, label: &str, value: Value, action: &str, options: &[(String, String)], disabled: bool) -> Value {
    json!({"id": id, "component": "Select", "visible": visible,
        "props": {"triggerVariant": "row", "label": label, "name": id, "value": value, "options": select_options(options), "disabled": disabled},
        "on": {"change": {"event": {"name": action}}}})
}

/// A picker row's trigger: the value at 70% (40% while `dim`) with its
/// chevron, opening `items` as the single-select Menu that fires `action`.
fn picker_trigger(ink: &Ink, id: &str, value: Value, action: &str, items: Vec<Value>, dim: bool) -> Value {
    let glyph = ink.r(1.);
    let alpha = if dim { 0.4 } else { VALUE_ALPHA };
    let mut text = faded(ink.text(&format!("{id}_value"), value, SM, &ink.value), alpha);
    text["props"]["lines"] = json!(1);
    text["style"]["maxWidth"] = json!(240);
    // The Button sets its value half a point higher than the box centre.
    text["style"]["marginTop"] = json!(-0.5);
    text["style"]["marginBottom"] = json!(0.5);
    let trigger = json!({"id": format!("{id}_trigger"), "component": "Box", "props": {"pressable": !dim},
        // A Medium ghost Button's content: `gap_2` between the value and its chevron.
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.5), "cursor": "pointer"},
        "children": [
            text,
            {"id": format!("{id}_chevron"), "component": "Icon", "props": {"name": "ui-chevron-right"}, "style": {"width": glyph, "height": glyph, "color": ink.value, "opacity": alpha}},
        ]});
    if dim {
        return trigger;
    }
    json!({"id": format!("{id}_menu"), "component": "Menu", "props": {"items": items, "align": "end"}, "on": {"select": {"event": {"name": action}}}, "children": [trigger]})
}

/// `<base>/hint` picks the row drawn with its hint line (or without).
fn hint_pick(base: &str, hint: bool) -> Value {
    if hint {
        path(&format!("{base}/hint"))
    } else {
        not(path(&format!("{base}/hint")))
    }
}

/// One picker row: a Select row, and (while `<base>/hint`) the IDE row
/// with its hint line under the label, which a Select row has no part for.
#[allow(clippy::too_many_arguments)]
fn picker_rows(ink: &Ink, id: &str, visible: Value, label: &str, base: &str, action: &str, options: &[(String, String)], dim: Option<Value>) -> Vec<Value> {
    let items = menu_items(options);
    let variants: Vec<(String, Value, bool)> = match &dim {
        Some(disabled) => vec![(id.to_string(), not(disabled.clone()), false), (format!("{id}_dim"), disabled.clone(), true)],
        None => vec![(id.to_string(), json!(true), false)],
    };
    let mut rows = Vec::new();
    for (vid, shown, dim) in variants {
        rows.push(select_row(&vid, and(visible.clone(), and(shown.clone(), hint_pick(base, false))), label, path(&format!("{base}/value")), action, options, dim));
        for hint in [true] {
            let rid = format!("{vid}_hinted");
            let column = label_column(ink, &format!("{rid}_text"), label, hint.then(|| path(&format!("{base}/detail"))), false);
            let trigger = picker_trigger(ink, &rid, path(&format!("{base}/label")), action, items.clone(), dim);
            rows.push(ink.shell(&rid, false, and(visible.clone(), and(shown.clone(), hint_pick(base, hint))), vec![column, control_column(&format!("{rid}_control"), vec![trigger])]));
        }
    }
    rows
}

/// A switch row (`glass_toggle_row`), once per hint variant and per
/// `<base>/disabled` (a Switch's `disabled` is not bindable).
fn switch_rows(ink: &Ink, id: &str, label: &str, base: &str, action: &str) -> Vec<Value> {
    let mut rows = Vec::new();
    for disabled in [false, true] {
        for hint in [false, true] {
            let rid = format!("{id}{}{}", if hint { "_hinted" } else { "" }, if disabled { "_off" } else { "" });
            let column = label_column(ink, &format!("{rid}_text"), label, hint.then(|| path(&format!("{base}/detail"))), true);
            let switch = json!({"id": format!("{rid}_switch"), "component": "Switch", "props": {"label": "", "name": rid, "checked": path(&format!("{base}/checked")), "disabled": disabled},
                "on": {"change": {"event": {"name": action}}}});
            let off = path(&format!("{base}/disabled"));
            let shown = and(hint_pick(base, hint), if disabled { off } else { not(off) });
            rows.push(ink.shell(&rid, false, shown, vec![column, switch]));
        }
    }
    rows
}

/// A text-field row (`glass_input_row`): genUI's row Input (the label
/// leading, the bare value end-aligned at 70%; the exponential theme's row
/// is the IDE's at its 14 px rem).
fn input_row(id: &str, visible: Value, label: &str, name: &str, placeholder: &str) -> Value {
    json!({"id": id, "component": "Input", "visible": visible, "props": {"variant": "row", "label": label, "name": name, "placeholder": placeholder, "value": path(&format!("/pane/fields/{name}"))}})
}

/// `action_error::field_error`: the 13 px line in the danger ink, in a box
/// of `padding` (horizontal, bottom).
fn field_error(inks: &Inks, id: &str, text: Value, visible: Value, padding: (f32, f32)) -> Value {
    let line = json!({"id": format!("{id}_text"), "component": "Text", "props": {"text": text}, "style": {"fontSize": crate::action_error::FIELD_ERROR_PX, "lineHeight": 20, "color": inks.danger}});
    json!({"id": id, "component": "Box", "visible": visible, "style": {"display": "flex", "flexDirection": "column", "paddingHorizontal": padding.0, "paddingBottom": padding.1}, "children": [line]})
}

/// Every id in `node` gets `suffix` (a subtree drawn twice).
fn rename(node: &mut Value, suffix: &str) {
    if let Some(id) = node.get("id").and_then(Value::as_str).map(str::to_string) {
        node["id"] = json!(format!("{id}{suffix}"));
    }
    if let Some(children) = node.get_mut("children").and_then(Value::as_array_mut) {
        for child in children {
            rename(child, suffix);
        }
    }
}

/// The Push to talk row: genUI's KeyRecorder on the voice device (the
/// Clear button beside it while a key is bound), the bound keys as a
/// KbdGroup elsewhere; the capture error under the row.
fn push_to_talk(inks: &Inks) -> Value {
    let ink = &inks.ink;
    let copy = &VOICE.copy;
    let base = "/pane/tray/ptt";
    let at = |key: &str| path(&format!("{base}/{key}"));
    let recorder = json!({"id": "ptt_recorder", "component": "KeyRecorder",
        "props": {"label": copy.push_to_talk, "value": at("keys"), "recording": at("recording"), "pending": at("pending"), "placeholder": copy.shortcut_record},
        "on": {"start": {"event": {"name": "pttStart"}}, "stop": {"event": {"name": "pttStop"}}, "key": {"event": {"name": "pttKey"}}}});
    // gpui-component's XSmall ghost Button: `h_5` / `px_1`, the `text_xs`
    // label at line height 1 (half a point under the box centre).
    let size = ink.r(XS);
    let clear = json!({"id": "ptt_clear", "component": "Box", "visible": at("clear"), "props": {"pressable": true}, "accessibility": {"label": copy.shortcut_clear},
        "on": {"press": {"event": {"name": "pttClear"}}},
        "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "justifyContent": "center", "flexShrink": 0, "height": ink.r(1.25), "paddingHorizontal": ink.r(0.25), "cursor": "pointer"},
        "children": [{"id": "ptt_clear_text", "component": "Text", "props": {"text": copy.shortcut_clear}, "style": {"fontSize": size, "lineHeight": size, "color": ink.foreground, "marginTop": 0.5, "marginBottom": -0.5}}]});
    let here = json!({"id": "ptt_here", "component": "Box", "visible": at("here"), "style": {"display": "flex", "flexDirection": "row", "alignItems": "center", "gap": ink.r(0.25)}, "children": [recorder, clear]});
    let keys = json!({"id": "ptt_keys", "component": "KbdGroup", "visible": and(at("readOnly"), at("bound")), "props": {"keys": at("keys")}});
    let mut none = ink.text("ptt_none", json!(copy.shortcut_none), SM, &ink.muted);
    none["visible"] = and(at("readOnly"), at("unbound"));
    let mut rows = Vec::new();
    for hint in [false, true] {
        let rid = if hint { "ptt_row_hinted" } else { "ptt_row" };
        let column = label_column(ink, &format!("{rid}_text"), &copy.push_to_talk, hint.then(|| at("reason")), false);
        let mut controls = vec![here.clone(), keys.clone(), none.clone()];
        if hint {
            controls.iter_mut().for_each(|node| rename(node, "_h"));
        }
        rows.push(ink.shell(rid, false, hint_pick(base, hint), vec![column, control_column(&format!("{rid}_control"), controls)]));
    }
    // feedback-states.json `errors`: invalid input inline under its field.
    rows.push(field_error(inks, "ptt_error", at("error"), at("hasError"), (ink.r(1.), ink.r(0.75))));
    json!({"id": "ptt", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "width": "100%"}, "children": rows})
}

/// One group of rows (`glass_group_rows`): each row with the hairline it
/// draws above itself where `divider` says.
fn group(ink: &Ink, id: &str, visible: Value, rows: Vec<(Vec<Value>, Option<Value>)>) -> Value {
    let mut children = Vec::new();
    for (ix, (nodes, divider)) in rows.into_iter().enumerate() {
        if let Some(divider) = divider {
            children.push(ink.divider(&format!("{id}_divider_{ix}"), divider));
        }
        children.extend(nodes);
    }
    let mut node = ink.group(id, children);
    node["visible"] = visible;
    node
}

/// The pane's tree.
fn document(inks: &Inks, options: &Options) -> Value {
    let ink = &inks.ink;
    let copy = &VOICE.copy;
    let words = &MEETING.copy;
    let pickers = path("/pane/pickers");
    let tray = path("/pane/hasTray");

    let device = picker_rows(ink, "device", pickers.clone(), &copy.voice_device, "/pane/device", "device", &&options.devices, None);
    let agent = picker_rows(ink, "agent", pickers.clone(), &copy.agent, "/pane/agent", "agent", &&options.agents, None);
    let language = picker_rows(ink, "language", json!(true), &copy.language, "/pane/language", "language", &&options.languages, None);
    let first = group(ink, "first", path("/pane/ready"), vec![
        (device, None),
        (agent, Some(pickers.clone())),
        (language, Some(pickers.clone())),
        (vec![input_row("voice_file", tray.clone(), &copy.voice_file, "voiceFile", &copy.voice_file_placeholder)], Some(tray.clone())),
        (vec![input_row("voice_transcript", tray.clone(), &copy.voice_transcript, "voiceTranscript", &copy.voice_transcript_placeholder)], Some(tray.clone())),
    ]);

    let mut header = ink.band("tray_header", "", None);
    header["children"][0]["props"]["text"] = path("/pane/tray/header");
    header["visible"] = tray.clone();

    let tray_group = group(ink, "tray", tray.clone(), vec![
        (vec![push_to_talk(inks)], None),
        (switch_rows(ink, "wake", &copy.wake_word, "/pane/tray/wake", "wake"), Some(json!(true))),
        (vec![input_row("wake_phrase", json!(true), &copy.wake_phrase, "wakePhrase", &options.wake_placeholder)], Some(json!(true))),
    ]);

    // #NEW-56: the too-short wake phrase's line, under the tray group (`px_1`).
    let invalid = field_error(inks, "wake_invalid", path("/pane/tray/wakeInvalid"), and(tray.clone(), path("/pane/tray/hasWakeInvalid")), (ink.r(0.25), 0.));

    // #NEW-57: the Apps row: a multiple Select row (it stays open while
    // apps are toggled), "None" muted while no app is picked.
    let mut apps = select_row("apps", path("/pane/tray/apps/shown"), &words.apps, path("/pane/tray/apps/value"), "app", &options.apps, false);
    apps["props"]["multiple"] = json!(true);
    apps["props"]["placeholder"] = json!(apps_value(&[]));
    let meeting = group(ink, "meeting", tray.clone(), vec![
        (switch_rows(ink, "meeting", &words.meeting, "/pane/tray/meeting", "meeting"), None),
        (picker_rows(ink, "auto_stop", json!(true), &words.auto_stop_after, "/pane/tray/autoStop", "autoStop", &&options.auto_stop, Some(path("/pane/tray/autoStop/disabled"))), Some(json!(true))),
        (picker_rows(ink, "desktop_audio", json!(true), &words.desktop_audio, "/pane/tray/desktopAudio", "desktopAudio", &&options.desktop_audio, None), Some(json!(true))),
        (vec![apps], Some(path("/pane/tray/apps/shown"))),
    ]);

    // `settings::section`: `gap_2`; a failed load kept the skeleton.
    let skeleton = skeleton("skeleton", json!({"@call": "or", "args": {"values": [path("/pane/loading"), path("/pane/failed")]}}));
    json!({"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "gap": ink.r(0.5), "width": "100%"},
        "children": [skeleton, first, header, tray_group, invalid, meeting]})
}

fn selected(event: &ActionEvent) -> Option<String> {
    event.payload.as_ref().and_then(|p| p.get("value")).and_then(Value::as_str).map(str::to_string)
}

fn checked(event: &ActionEvent) -> Option<bool> {
    event.payload.as_ref().and_then(|p| p.get("checked")).and_then(Value::as_bool)
}

/// The id a picker sent (`NONE` = None).
fn picked(event: &ActionEvent) -> Option<Option<String>> {
    selected(event).map(|value| Some(value).filter(|value| value != NONE))
}

/// Which one-line input.
#[derive(Clone, Copy)]
enum Field {
    WakePhrase,
    VoiceFile,
    VoiceTranscript,
}

impl Field {
    fn of(name: &str) -> Option<Field> {
        match name {
            "wakePhrase" => Some(Field::WakePhrase),
            "voiceFile" => Some(Field::VoiceFile),
            "voiceTranscript" => Some(Field::VoiceTranscript),
            _ => None,
        }
    }

    fn update(self, value: String) -> VoiceUpdate {
        match self {
            Field::WakePhrase => VoiceUpdate { wake_phrase: Some(value), ..Default::default() },
            Field::VoiceFile => VoiceUpdate { voice_file: Some(value), ..Default::default() },
            Field::VoiceTranscript => VoiceUpdate { voice_transcript: Some(value), ..Default::default() },
        }
    }

    fn limit(self) -> usize {
        match self {
            Field::WakePhrase => VOICE.limits.wake_phrase,
            Field::VoiceFile => VOICE.limits.voice_file,
            Field::VoiceTranscript => VOICE.limits.voice_transcript,
        }
    }

    /// #NEW-75: THE input rule ×4 (`voice_text_input`), then trimmed: the
    /// value as sent and shown.
    fn normalized(self, raw: &str) -> String {
        voice_text_input(raw, self.limit()).trim().to_string()
    }
}

/// Optimistic: apply locally (clearOnNone included), send the partial,
/// adopt what the server stored; a refusal reverts and toasts.
fn apply(binding: &mut Binding<Voice>, update: VoiceUpdate, cx: &mut gpui::Context<Binding<Voice>>) {
    if binding.state().fixed.is_some() {
        return;
    }
    let Some(previous) = binding.state().ready().cloned() else { return };
    let mut next = previous.clone();
    if let Some(value) = update.voice_device_id.clone() {
        next.voice_device_id = value;
    }
    if let Some(value) = update.agent_id.clone() {
        next.agent_id = value;
    }
    if let Some(value) = update.ptt_shortcut.clone() {
        next.ptt_shortcut = value;
    }
    if let Some(value) = update.wake_word {
        next.wake_word = value;
    }
    if let Some(value) = update.wake_phrase.clone() {
        next.wake_phrase = value.trim().to_string();
    }
    if let Some(value) = update.language.clone() {
        next.language = value;
    }
    if let Some(value) = update.voice_file.clone() {
        next.voice_file = value.trim().to_string();
    }
    if let Some(value) = update.voice_transcript.clone() {
        next.voice_transcript = value.trim().to_string();
    }
    if let Some(value) = update.desktop_audio {
        next.desktop_audio = value;
    }
    if let Some(value) = update.desktop_audio_apps.clone() {
        next.desktop_audio_apps = value;
    }
    if let Some(value) = update.meeting_auto_stop_hours {
        next.meeting_auto_stop_hours = value;
    }
    let next = next.clear_on_none();
    if next == previous {
        return;
    }
    let Some(trpc) = queries::trpc_client(cx) else { return };
    let mut write = 0;
    binding.update_state(
        |state| {
            state.load = Load::Ready(next);
            // Last write wins (the tray's pattern): overlapping saves never
            // clobber each other, only the newest adopts the stored row or reverts.
            state.write += 1;
            state.in_flight += 1;
            write = state.write;
        },
        cx,
    );
    cx.spawn(async move |this, cx| {
        let result = cx.background_executor().spawn(async move { api::voice::voice_update(&trpc, &update) }).await;
        let _ = this.update(cx, |binding, cx| {
            let newest = binding.state().write == write;
            match result {
                Ok(stored) => {
                    if newest {
                        crate::talk_host::set_voice_settings(stored.clone(), cx);
                    }
                    binding.update_state(
                        |state| {
                            state.in_flight = state.in_flight.saturating_sub(1);
                            if newest {
                                state.seen = stored.clone();
                                state.load = Load::Ready(stored);
                            }
                        },
                        cx,
                    );
                }
                Err(err) => {
                    binding.update_state(
                        |state| {
                            state.in_flight = state.in_flight.saturating_sub(1);
                            if newest {
                                state.load = Load::Ready(previous);
                            }
                        },
                        cx,
                    );
                    crate::action_error::failed_action_err("save", "the voice settings", &err, cx);
                }
            }
        });
    })
    .detach();
}

/// S-3: the recorder events that bring `capture`'s held modifiers to the
/// flags a `key` event carries (`meta` = the grammar's `super`).
fn modifier_events(capture: &CaptureState, flags: &Value) -> Vec<CaptureEvent> {
    let on = |name: &str| flags.get(name).and_then(Value::as_bool).unwrap_or(false);
    [("ctrl", on("ctrl")), ("alt", on("alt")), ("shift", on("shift")), ("super", on("meta"))]
        .into_iter()
        .filter_map(|(name, down)| {
            let held = capture.held.iter().any(|held| held == name);
            match (held, down) {
                (false, true) => Some(CaptureEvent::Down(name.to_string())),
                (true, false) => Some(CaptureEvent::Up(name.to_string())),
                _ => None,
            }
        })
        .collect()
}

/// S-3 (voice.json `captureCases`): one KeyRecorder `key` payload as the
/// grammar's events: the held modifiers first, so `ctrl-k` records
/// `ctrl+k` even when its modifier change was not seen.
fn key_events(capture: &CaptureState, payload: &Value) -> Vec<CaptureEvent> {
    let mut events = modifier_events(capture, payload);
    match payload.get("type").and_then(Value::as_str) {
        Some("down") => {
            if let Some(key) = payload.get("key").and_then(Value::as_str) {
                events.push(CaptureEvent::Down(key.to_string()));
            }
        }
        Some("mouse") => {
            if let Some(button) = payload.get("button").and_then(Value::as_u64) {
                events.push(CaptureEvent::Mouse(button as u32));
            }
        }
        _ => {}
    }
    events
}

/// S-3: the recorder ends (done, cancelled, blurred): the grab is taken again.
fn stop_recording(binding: &mut Binding<Voice>, cx: &mut gpui::Context<Binding<Voice>>) {
    if binding.state().capture.is_none() {
        return;
    }
    binding.update_state(|state| state.capture = None, cx);
    etalk_gpui::push_to_talk::set_recording(false, cx);
}

/// A `key` event while it records.
fn on_key(binding: &mut Binding<Voice>, payload: Value, cx: &mut gpui::Context<Binding<Voice>>) {
    let Some(mut capture) = binding.state().capture.clone() else { return };
    for event in key_events(&capture, &payload) {
        capture = capture_step(&capture, &event);
    }
    match capture.status {
        CaptureStatus::Recording => binding.update_state(|state| state.capture = Some(capture), cx),
        CaptureStatus::Cancelled => stop_recording(binding, cx),
        CaptureStatus::Done => {
            stop_recording(binding, cx);
            apply(binding, VoiceUpdate { ptt_shortcut: Some(capture.value), ..Default::default() }, cx);
        }
    }
}

/// Runs `edit` on the binding after the event that asked for it (the
/// binding is mid-update while its handler runs).
fn later(binding: &WeakEntity<Binding<Voice>>, cx: &mut App, edit: impl FnOnce(&mut Binding<Voice>, &mut gpui::Context<Binding<Voice>>) + 'static) -> Option<Mutation<Voice>> {
    let binding = binding.clone();
    cx.defer(move |cx| {
        let _ = binding.update(cx, |binding, cx| edit(binding, cx));
    });
    None
}

/// A picker's action: its value as the update `make` builds.
fn on_pick(binding: &mut Binding<Voice>, me: &WeakEntity<Binding<Voice>>, name: &str, make: fn(Option<String>) -> VoiceUpdate) {
    let me = me.clone();
    binding.on_action(name, move |_, event, cx| {
        let update = make(picked(event)?);
        later(&me, cx, move |binding, cx| apply(binding, update, cx))
    });
}

fn wire(binding: &mut Binding<Voice>, me: WeakEntity<Binding<Voice>>) {
    binding.set_toasts(Rc::new(IdeToasts));
    on_pick(binding, &me, "device", |value| VoiceUpdate { voice_device_id: Some(value), ..Default::default() });
    on_pick(binding, &me, "agent", |value| VoiceUpdate { agent_id: Some(value), ..Default::default() });
    on_pick(binding, &me, "language", |value| VoiceUpdate { language: value, ..Default::default() });
    on_pick(binding, &me, "autoStop", |value| VoiceUpdate { meeting_auto_stop_hours: value.as_deref().and_then(|value| value.parse().ok()), ..Default::default() });
    on_pick(binding, &me, "desktopAudio", |value| VoiceUpdate { desktop_audio: value.as_deref().map(DesktopAudio::from_key), ..Default::default() });
    let me_wake = me.clone();
    binding.on_action("wake", move |_, event, cx| {
        let on = checked(event)?;
        later(&me_wake, cx, move |binding, cx| apply(binding, VoiceUpdate { wake_word: Some(on), ..Default::default() }, cx))
    });
    // #NEW-57: on = THE consent dialog (its Start writes), off = stop at once.
    let me_meeting = me.clone();
    binding.on_action("meeting", move |state, event, cx| {
        if state.fixed.is_some() {
            return None;
        }
        if checked(event)? {
            crate::meeting_mode::prompt_consent(cx);
            // The switch shows what is stored until the dialog's Start.
            let me = me_meeting.clone();
            return later(&me, cx, |binding, cx| {
                binding.update_state(|_| {}, cx);
            });
        }
        crate::meeting_mode::stop(cx);
        Some(Mutation::local(|state: &mut Voice| {
            if let Load::Ready(settings) = &mut state.load {
                settings.meeting_started_at = None;
            }
        }))
    });
    let me_app = me.clone();
    binding.on_action("app", move |state, event, cx| {
        // The Select's value: the picked ids, comma-joined.
        let ids: Vec<String> = selected(event)?.split(',').filter(|id| !id.is_empty()).map(str::to_string).collect();
        let settings = state.ready()?;
        let (_, live) = audio(state, settings, cx);
        let options = app_options(&live, &settings.desktop_audio_apps);
        let apps: Vec<AudioApp> = ids
            .iter()
            .filter_map(|id| options.iter().find(|option| &option.value == id))
            .map(|option| AudioApp { id: option.value.clone(), name: option.label.chars().take(MEETING.limits.app_name).collect() })
            .take(MEETING.limits.apps)
            .collect();
        later(&me_app, cx, move |binding, cx| apply(binding, VoiceUpdate { desktop_audio_apps: Some(apps), ..Default::default() }, cx))
    });
    // S-3: the recorder. `start` (the core already wrote recording true):
    // the capture over the current binding, and the IDE lets its own grab go.
    let me_start = me.clone();
    binding.on_action("pttStart", move |state, _, cx| {
        let current = state.ready()?.ptt_shortcut.clone();
        if state.capture.is_some() || state.fixed.is_some() {
            return None;
        }
        etalk_gpui::push_to_talk::set_recording(true, cx);
        later(&me_start, cx, move |binding, cx| binding.update_state(|state| state.capture = Some(capture_start(&current)), cx))
    });
    let me_stop = me.clone();
    binding.on_action("pttStop", move |_, _, cx| later(&me_stop, cx, stop_recording));
    let me_key = me.clone();
    binding.on_action("pttKey", move |_, event, cx| {
        let payload = event.payload.clone()?;
        later(&me_key, cx, move |binding, cx| on_key(binding, payload, cx))
    });
    let me_clear = me.clone();
    binding.on_action("pttClear", move |_, _, cx| {
        later(&me_clear, cx, |binding, cx| {
            stop_recording(binding, cx);
            apply(binding, VoiceUpdate { ptt_shortcut: Some(String::new()), ..Default::default() }, cx);
        })
    });
    for name in ["wakePhrase", "voiceFile", "voiceTranscript"] {
        let me_input = me.clone();
        binding.on_input(name, move |_, event: &InputEvent, cx| {
            let field = Field::of(&event.name)?;
            let value = field.normalized(event.value.as_str().unwrap_or_default());
            later(&me_input, cx, move |binding, cx| {
                if let Some(invalid) = matches!(field, Field::WakePhrase).then(|| wake_phrase_error(&value)).flatten() {
                    binding.update_state(|state| state.wake_phrase_invalid = Some(invalid), cx);
                    return;
                }
                binding.update_state(|state| state.wake_phrase_invalid = None, cx);
                apply(binding, field.update(value), cx);
            })
        });
    }
}

pub struct VoicePrefsPane {
    view: Entity<SurfaceView>,
    binding: Entity<Binding<Voice>>,
    /// What the mounted tree was built over.
    options: Options,
    _subscriptions: Vec<Subscription>,
}

impl VoicePrefsPane {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::build(false, None, window, cx)
    }

    /// S-3: the pane as the Assistant page's Voice block.
    pub(crate) fn assistant(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::build(true, None, window, cx)
    }

    /// The styleguide twin: `settings` over `fixed`, inert.
    pub(crate) fn twin(settings: VoiceSettings, fixed: Fixed, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let mut pane = Self::build(false, Some(fixed), window, cx);
        pane.binding.update(cx, |binding, cx| binding.update_state(|state| state.load = Load::Ready(settings), cx));
        pane.options = options(pane.binding.read(cx).state(), cx);
        let tree: eui_core::NestedNode = serde_json::from_value(document(&Inks::of(cx), &pane.options)).expect("the voice document parses");
        pane.view.update(cx, |view, cx| {
            view.set_nested(tree, cx);
        });
        pane
    }

    fn build(assistant_block: bool, fixed: Option<Fixed>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let live = fixed.is_none();
        let state = Voice {
            load: Load::Idle,
            generation: 0,
            account_id: None,
            write: 0,
            in_flight: 0,
            seen: etalk_gpui::speak_replies::settings(cx),
            wake_phrase_invalid: None,
            capture: None,
            assistant_block,
            fixed,
        };
        let binding = cx.new(|cx| {
            let mut binding = Binding::new(PATH, state, project);
            wire(&mut binding, cx.weak_entity());
            if live {
                // S-3: the Wake word row follows the microphone, Mute and
                // the voice device's doctor; the Language row the download.
                if let Some(push_to_talk) = etalk_gpui::push_to_talk::entity(cx) {
                    binding.observe(&push_to_talk, cx);
                }
                if let Some(tray) = etalk_gpui::voice_tray::entity(cx) {
                    binding.observe(&tray, cx);
                }
                if let Some(store) = sync::Store::try_global(cx) {
                    let collections = store.collections().clone();
                    binding.observe(&collections.devices, cx);
                    binding.observe(&collections.agents, cx);
                }
            }
            binding
        });
        let options = options(binding.read(cx).state(), cx);
        let view = mount("settings-voice", document(&Inks::of(cx), &options), &binding, window, cx);
        let mut subscriptions = Vec::new();
        if live {
            // A change another client made (a tray toggle, a heartbeat) reaches
            // the speaker first; the open pane follows it.
            if let Some(speaker) = etalk_gpui::speak_replies::entity(cx) {
                subscriptions.push(cx.observe(&speaker, |this: &mut Self, _, cx| this.on_speaker(cx)));
            }
        }
        // F42: a pane dropped while it records gives the push-to-talk grab
        // back (only the recorder's end would otherwise).
        subscriptions.push(cx.on_release(|this: &mut Self, cx| {
            if this.binding.read(cx).state().capture.is_some() {
                etalk_gpui::push_to_talk::set_recording(false, cx);
            }
        }));
        Self { view, binding, options, _subscriptions: subscriptions }
    }

    #[cfg(test)]
    pub(crate) fn for_shots(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self::new(window, cx)
    }

    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: VoiceShot, cx: &mut gpui::Context<Self>) {
        SHOT_HERE.with(|here| here.set(Some(true)));
        let account_id = sync::Store::global(cx).session(cx).account_id().map(str::to_string);
        self.binding.update(cx, |binding, cx| {
            binding.update_state(
                |state| {
                    state.account_id = account_id;
                    state.load = Load::Ready(shot.settings);
                    state.capture = shot.capture;
                },
                cx,
            )
        });
        cx.notify();
    }

    fn on_speaker(&mut self, cx: &mut gpui::Context<Self>) {
        let now = etalk_gpui::speak_replies::settings(cx);
        let stale = self.binding.update(cx, |binding, cx| {
            if binding.state().seen == now {
                // The download's progress (the Language row's detail).
                binding.push(cx);
                return false;
            }
            let idle = binding.state().in_flight == 0;
            binding.update_state(|state| state.seen = now, cx);
            idle
        });
        if stale {
            self.mark_stale(cx);
        }
    }

    /// Drop the cached settings so the next render refetches (a change made
    /// on another client).
    pub fn mark_stale(&mut self, cx: &mut gpui::Context<Self>) {
        self.binding.update(cx, |binding, cx| {
            if binding.state().fixed.is_none() && matches!(binding.state().load, Load::Ready(_)) {
                binding.update_state(|state| state.load = Load::Idle, cx);
            }
        });
        cx.notify();
    }

    fn ensure_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        if self.binding.read(cx).state().fixed.is_some() {
            return;
        }
        let account_id = sync::Store::global(cx).session(cx).account_id().map(str::to_string);
        let trpc = queries::trpc_client(cx);
        self.binding.update(cx, |binding, cx| {
            if binding.state().account_id != account_id {
                binding.update_state(
                    |state| {
                        state.account_id = account_id;
                        state.load = Load::Idle;
                    },
                    cx,
                );
            }
            if !matches!(binding.state().load, Load::Idle) {
                return;
            }
            let Some(trpc) = trpc else { return };
            let mut generation = 0;
            binding.update_state(
                |state| {
                    state.load = Load::Loading;
                    state.wake_phrase_invalid = None;
                    state.generation += 1;
                    generation = state.generation;
                },
                cx,
            );
            cx.spawn(async move |this, cx| {
                let result = cx.background_executor().spawn(async move { api::voice::voice_get(&trpc) }).await;
                let _ = this.update(cx, |binding, cx| {
                    if binding.state().generation != generation {
                        return;
                    }
                    let load = match result {
                        Ok(settings) => Load::Ready(settings),
                        Err(err) => {
                            crate::action_error::failed_load("your voice settings", false, crate::action_error::action_error_description(&err), cx);
                            Load::Error
                        }
                    };
                    binding.update_state(|state| state.load = load, cx);
                });
            })
            .detach();
        });
    }
}

impl Render for VoicePrefsPane {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "VoicePrefsPane").entered();
        self.ensure_loaded(cx);
        // The menus and the wake placeholder are the tree's: a new device,
        // agent or app rebuilds it.
        let now = options(self.binding.read(cx).state(), cx);
        if now != self.options {
            let tree: eui_core::NestedNode = serde_json::from_value(document(&Inks::of(cx), &now)).expect("the voice document parses");
            self.view.update(cx, |view, cx| {
                view.set_nested(tree, cx);
            });
            self.options = now;
        }
        div().w_full().min_w_0().child(self.view.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::{document, key_events, label_of, modifier_events, Field, Inks, Options};
    use etalk::voice::{capture_pending, capture_start, capture_step, voice_device_options, CaptureEvent, CaptureStatus, VoiceDevice};
    use serde_json::json;

    /// #NEW-125 (voice.json): the Voice device row reads the picked option's
    /// label: None while none is picked, EMPTY while the stored device is not
    /// (yet) among the options, never `…`.
    #[test]
    fn voice_device_value_is_none_or_empty() {
        let mint = VoiceDevice { device_id: "d-mint".into(), name: "mint".into(), kind: "desktop".into() };
        let options = voice_device_options(&[mint]);
        assert_eq!(label_of(&options, None, ""), "None");
        assert_eq!(label_of(&options, Some("d-mint"), ""), "mint");
        assert_eq!(label_of(&options, Some("d-unsynced"), ""), "");
        assert_eq!(label_of(&voice_device_options(&[]), None, ""), "None");
    }

    /// S-3: KeyRecorder's `key` payloads fold into the grammar like the
    /// fixture's `down` / `up` events.
    #[test]
    fn key_payloads_feed_the_recorder() {
        let held = json!({"type": "modifiers", "ctrl": true, "shift": true, "alt": false, "meta": false});
        let mut capture = capture_start("ctrl+space");
        for event in key_events(&capture, &held) {
            capture = capture_step(&capture, &event);
        }
        assert_eq!(capture_pending(&capture), "Ctrl + Shift + …");
        assert!(modifier_events(&capture, &held).is_empty(), "nothing changed");
        let events = key_events(&capture, &json!({"type": "modifiers", "ctrl": true}));
        assert_eq!(events, vec![CaptureEvent::Up("shift".into())]);
        let mut capture = events.iter().fold(capture, |capture, event| capture_step(&capture, event));
        for event in key_events(&capture, &json!({"type": "down", "key": "k", "ctrl": true})) {
            capture = capture_step(&capture, &event);
        }
        assert_eq!((capture.status, capture.value.as_str()), (CaptureStatus::Done, "ctrl+k"));
        let platform = modifier_events(&capture_start(""), &json!({"meta": true}));
        assert_eq!(platform, vec![CaptureEvent::Down("super".into())]);
        assert_eq!(key_events(&capture_start(""), &json!({"type": "mouse", "button": 4})), vec![CaptureEvent::Mouse(4)]);
    }

    #[test]
    fn a_commit_is_cleaned_and_trimmed() {
        assert_eq!(Field::WakePhrase.normalized(" ab\u{7}cd "), "abcd");
        let raw = "x".repeat(Field::WakePhrase.limit() + 10);
        assert_eq!(Field::WakePhrase.normalized(&raw).len(), Field::WakePhrase.limit());
    }

    #[test]
    fn the_document_parses() {
        let ink = super::super::doc::Ink {
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
            mono: "Lilex".into(),
            pill_button_fill: "#ffffff0f".into(),
            pill_button_fill_disabled: "#ffffff0b".into(),
        };
        let options = Options {
            devices: vec![(String::new(), "None".into()), ("d-mint".into(), "mint".into())],
            agents: vec![(String::new(), "None".into())],
            languages: vec![("en".into(), "English".into())],
            auto_stop: vec![("4".into(), "4 hours".into())],
            desktop_audio: vec![("off".into(), "Off".into())],
            apps: vec![("us.zoom.xos".into(), "Zoom".into())],
            wake_placeholder: "Hey Studio".into(),
        };
        let tree: eui_core::NestedNode = serde_json::from_value(document(&Inks { ink, danger: "#ff6467ff".into() }, &options)).expect("parses");
        assert_eq!(tree.id, "root");
    }
}
