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
//! [`voice_rows`] draws the rows from plain values; the pane and the
//! styleguide twin (`styleguide/entries/voice_settings.rs`) both call it.

use std::rc::Rc;

use gpui::{
    div, prelude::FluentBuilder as _, AnyElement, App, AppContext as _, Entity, FocusHandle, InteractiveElement as _,
    IntoElement, Modifiers, MouseButton, NavigationDirection, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    input::{InputEvent, InputState},
    h_flex, v_flex, ActiveTheme as _, Disableable as _, Sizable as _,
};

use api::voice::VoiceUpdate;
use etalk::voice::{
    agent_options, capture_pending, capture_start, capture_step, language_label, shortcut_reason, tray_header,
    voice_device_options, voice_text_input, wake_phrase_error, wake_phrase_placeholder, CaptureEvent, CaptureState, CaptureStatus,
    VoiceAgent, VoiceDevice, VoiceOption, VoiceSettings, VOICE,
};

use etalk::meeting::{app_options, apps_value, desktop_audio_options, AudioApp, DesktopAudioCapability, MEETING};

use crate::controls::{glass_input, kbd, kbd_group, shortcut_keys, PointerMenuButtonExt as _};
use crate::picker::{Picker, PickerItem};
use crate::queries;
use crate::surface::{glass_group_rows, glass_input_row, glass_picker_row, glass_toggle_row, picker_value_label};

use super::{section, settings_skeleton};

/// A change from one row: the partial update to send.
pub(crate) type OnVoiceChange = Rc<dyn Fn(VoiceUpdate, &mut App)>;

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
fn label_of(options: &[VoiceOption], id: Option<&str>, unsynced: &str) -> SharedString {
    options
        .iter()
        .find(|option| option.id.as_deref() == id)
        .map(|option| option.label.clone())
        .or_else(|| id.map(|_| unsynced.to_string()))
        .unwrap_or_default()
        .into()
}

/// One picker row: the label, the current value, a single-select menu.
fn picker_row(
    id: &'static str,
    label: &str,
    detail: Option<SharedString>,
    value: SharedString,
    entries: Vec<(Option<String>, String)>,
    current: Option<String>,
    on_pick: Option<Rc<dyn Fn(Option<String>, &mut App)>>,
    cx: &App,
) -> gpui::Div {
    enabled_picker_row(id, label, detail, value, entries, current, true, on_pick, cx)
}

/// [`picker_row`] that can be disabled (#FEED-163: Stop after while the
/// device or the agent is None, like the toggles).
#[allow(clippy::too_many_arguments)]
fn enabled_picker_row(
    id: &'static str,
    label: &str,
    detail: Option<SharedString>,
    value: SharedString,
    entries: Vec<(Option<String>, String)>,
    current: Option<String>,
    enabled: bool,
    on_pick: Option<Rc<dyn Fn(Option<String>, &mut App)>>,
    cx: &App,
) -> gpui::Div {
    let value_color = cx.theme().foreground.opacity(if enabled { 0.7 } else { 0.4 });
    let on_pick = on_pick.filter(|_| enabled);
    let button = Button::new(id)
        .ghost()
        .disabled(!enabled)
        .cursor_pointer()
        .h_auto()
        .px_0()
        .py_0()
        .text_color(value_color)
        .child(picker_value_label(value))
        .child(crate::surface::picker_row_chevron(cx))
        .pointer_menu(move |mut menu, _, _| {
            menu = crate::pickers::single_select_menu(menu);
            for (entry_id, entry_label) in entries.clone() {
                let on_pick = on_pick.clone();
                let checked = entry_id == current;
                menu = menu.item(
                    crate::controls::pointer_label_item(entry_label, false)
                        .checked(checked)
                        .on_click(move |_, _, cx| {
                            if let Some(on_pick) = on_pick.as_ref() {
                                on_pick(entry_id.clone(), cx);
                            }
                        }),
                );
            }
            menu
        });
    glass_picker_row(label.to_string(), detail, button.into_any_element(), cx)
}

fn switch_row(
    id: &'static str,
    label: &str,
    detail: Option<SharedString>,
    checked: bool,
    enabled: bool,
    on_toggle: Option<Rc<dyn Fn(bool, &mut App)>>,
    cx: &App,
) -> gpui::Div {
    let switch = crate::controls::web_switch(id).checked(checked && enabled).disabled(!enabled);
    let switch = match on_toggle {
        Some(on_toggle) => switch.on_click(move |on: &bool, _, cx| on_toggle(*on, cx)),
        None => switch,
    };
    glass_toggle_row(label.to_string(), detail, switch.into_any_element(), cx)
}

/// S-3: what the recorder hands the pane.
pub(crate) enum RecorderInput {
    /// A click on the row: start recording.
    Start,
    /// A mouse button pressed over the recorder while it records (gpui's
    /// numbering: 1 left, 2 right, 3 middle, 4 back, 5 forward).
    Mouse(u32),
    /// The held modifiers changed while it records.
    Modifiers(Modifiers),
    /// The clear button (testId `shortcutClear`): no key = push to talk off.
    Clear,
}

/// S-3: the Push to talk row (voice.json PUSH TO TALK = A KEY BINDING).
#[derive(Default)]
pub(crate) struct ShortcutRecorder {
    /// The capture while recording (None = idle).
    pub capture: Option<CaptureState>,
    /// The recorder's key focus (the pane's).
    pub focus: Option<FocusHandle>,
    /// None = READ-ONLY (another device, the styleguide twin): the label.
    pub on_input: Option<Rc<dyn Fn(RecorderInput, &mut Window, &mut App)>>,
}

/// S-3: the recorder's control: idle = the keycaps (the clear button while
/// a key is bound, on the voice device); recording = the held modifiers as
/// pending keycaps or `copy.shortcutRecord`, focused so the key events land.
fn recorder_control(value: &str, recorder: &ShortcutRecorder, cx: &App) -> AnyElement {
    let Some(on_input) = recorder.on_input.clone() else {
        return shortcut_keys(value, cx).into_any_element();
    };
    let recording = recorder.capture.as_ref().filter(|capture| capture.status == CaptureStatus::Recording);
    let field = div()
        .id("settings-voice-shortcut")
        .cursor_pointer()
        .px_2()
        .py_1()
        .rounded_md()
        .border_1()
        .border_color(if recording.is_some() { cx.theme().ring } else { gpui::transparent_black() })
        .when_some(recorder.focus.as_ref(), |field, focus| field.track_focus(focus));
    let field = match recording {
        Some(capture) => {
            let pending = capture_pending(capture);
            let on_mouse = on_input.clone();
            let on_modifiers = on_input.clone();
            let inner = if pending.is_empty() {
                div().text_sm().text_color(cx.theme().muted_foreground).child(VOICE.copy.shortcut_record.clone())
            } else {
                // The held modifiers' keycaps, then the trailing `…`.
                let separator = &VOICE.shortcut_grammar.separator;
                let held = pending.trim_end_matches('…').trim_end_matches(separator.as_str());
                kbd_group(held.split(separator.as_str()).map(|cap| kbd(cap.to_string(), cx)))
                    .child(div().text_sm().text_color(cx.theme().muted_foreground).child(VOICE.copy.ellipsis.clone()))
            };
            field
                .on_any_mouse_down(move |event, window, cx| {
                    let button = match event.button {
                        MouseButton::Left => 1,
                        MouseButton::Right => 2,
                        MouseButton::Middle => 3,
                        MouseButton::Navigate(NavigationDirection::Back) => 4,
                        MouseButton::Navigate(NavigationDirection::Forward) => 5,
                    };
                    on_mouse(RecorderInput::Mouse(button), window, cx);
                    cx.stop_propagation();
                })
                .on_modifiers_changed(move |event, window, cx| on_modifiers(RecorderInput::Modifiers(event.modifiers), window, cx))
                .child(inner)
        }
        None => {
            let on_start = on_input.clone();
            field.on_click(move |_, window, cx| on_start(RecorderInput::Start, window, cx)).child(shortcut_keys(value, cx))
        }
    };
    let clear = (!value.is_empty() && recording.is_none()).then(|| {
        let on_clear = on_input.clone();
        Button::new("settings-voice-shortcut-clear")
            .ghost()
            .xsmall()
            .cursor_pointer()
            .label(VOICE.copy.shortcut_clear.clone())
            .on_click(move |_, window, cx| on_clear(RecorderInput::Clear, window, cx))
    });
    h_flex().gap_1().child(field).children(clear).into_any_element()
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

/// The page's three one-line inputs (the pane's live fields, or the twin's
/// static ones), already wrapped in `glass_row_input`.
pub(crate) struct VoiceInputs {
    pub wake_phrase: AnyElement,
    pub voice_file: AnyElement,
    pub voice_transcript: AnyElement,
}

/// #NEW-57: the meeting rows' inputs (`meeting.json` UI).
pub(crate) struct MeetingRows {
    /// The voice device's capability (`doctor.desktopAudio`).
    pub capability: DesktopAudioCapability,
    /// The Apps row's control ([`apps_control`]).
    pub apps: AnyElement,
    /// The Meeting mode toggle: on = THE consent dialog, off = stop at once.
    /// None = inert (the styleguide twin).
    pub on_meeting: Option<Rc<dyn Fn(bool, &mut App)>>,
}

/// #NEW-57: the Apps row's multi picker over `appOptions(live, stored)`;
/// `on_change` None = the trigger alone (the styleguide twin).
pub(crate) fn apps_control(
    settings: &VoiceSettings,
    live: &[AudioApp],
    on_change: Option<OnVoiceChange>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let stored = settings.desktop_audio_apps.clone();
    let empty = stored.is_empty();
    let trigger = Button::new("settings-voice-apps")
        .ghost()
        .cursor_pointer()
        .h_auto()
        .px_0()
        .py_0()
        .text_color(if empty { cx.theme().muted_foreground } else { cx.theme().foreground.opacity(0.7) })
        .child(picker_value_label(apps_value(&stored)))
        .child(crate::surface::picker_row_chevron(cx));
    let Some(on_change) = on_change else { return trigger.into_any_element() };
    let options = app_options(live, &stored);
    let items: Vec<PickerItem<String>> = options.iter().map(|option| PickerItem::new(option.value.clone(), option.label.clone())).collect();
    let value: Vec<String> = stored.iter().map(|app| app.id.clone()).collect();
    Picker::multi(
        items,
        value,
        trigger.into_any_element(),
        Rc::new(move |next: Vec<String>, _window, cx: &mut App| {
            let apps: Vec<AudioApp> = next
                .iter()
                .filter_map(|id| options.iter().find(|option| &option.value == id))
                .map(|option| AudioApp { id: option.value.clone(), name: option.label.chars().take(MEETING.limits.app_name).collect() })
                .take(MEETING.limits.apps)
                .collect();
            on_change(VoiceUpdate { desktop_audio_apps: Some(apps), ..Default::default() }, cx);
        }),
    )
    .id("settings-voice-apps-picker")
    .render(window, cx)
}

/// THE Voice rows (`voice.json`): group 1, then the tray group while a voice
/// device is picked. `wake_input` = the wake phrase's input element (the
/// pane's live field, or the twin's static one); `on_change` = None draws
/// the rows inert but in their live look (the styleguide twin).
pub(crate) fn voice_rows(
    settings: &VoiceSettings,
    devices: &[VoiceOption],
    agents: &[VoiceOption],
    inputs: VoiceInputs,
    meeting: MeetingRows,
    recorder: ShortcutRecorder,
    on_change: Option<OnVoiceChange>,
    assistant_block: bool,
    cx: &App,
) -> Vec<AnyElement> {
    let MeetingRows { capability, apps: apps_control, on_meeting } = meeting;
    let VoiceInputs { wake_phrase: wake_input, voice_file, voice_transcript } = inputs;
    let copy = &VOICE.copy;
    let ready = settings.voice_device_id.is_some() && settings.agent_id.is_some();
    // voice.json: while the voice downloads (only ever on the voice device,
    // i.e. here) a row's detail reads `downloadProgress`; S-3 removed the
    // Speak replies row, so the Language row (the voice) carries it.
    let downloading = (on_change.is_some() && ready).then(|| etalk_gpui::speak_replies::download_detail(cx)).flatten();
    let send = |make: Rc<dyn Fn(Option<String>) -> VoiceUpdate>| -> Option<Rc<dyn Fn(Option<String>, &mut App)>> {
        on_change.clone().map(|on_change| {
            Rc::new(move |value: Option<String>, cx: &mut App| on_change(make(value), cx)) as Rc<dyn Fn(Option<String>, &mut App)>
        })
    };
    let toggle = |make: Rc<dyn Fn(bool) -> VoiceUpdate>| -> Option<Rc<dyn Fn(bool, &mut App)>> {
        on_change.clone().map(|on_change| Rc::new(move |on: bool, cx: &mut App| on_change(make(on), cx)) as Rc<dyn Fn(bool, &mut App)>)
    };
    let options = |list: &[VoiceOption]| list.iter().map(|option| (option.id.clone(), option.label.clone())).collect::<Vec<_>>();

    // The stored device names the tray group even while its row is not
    // synced here (its label, else `copy.ellipsis`), as on web.
    let device_name = settings.voice_device_id.as_deref().map(|id| label_of(devices, Some(id), &copy.ellipsis).to_string());

    let language = settings.language().to_string();
    // Every language carries its GPU note (#NEW-74), in the row and in the menu.
    let languages = VOICE
        .languages
        .iter()
        .map(|entry| (Some(entry.value.clone()), language_label(&entry.value, device_name.as_deref())))
        .collect();

    let mut out: Vec<AnyElement> = Vec::new();
    // S-3 (agent-settings.json `assistant`): on the Assistant page the
    // block drops the "Voice device" and "Agent" pickers (the device is Runs
    // on's, the agent is this Assistant).
    let mut first: Vec<gpui::Div> = Vec::new();
    if !assistant_block {
        first.push(picker_row(
                "settings-voice-device",
                &copy.voice_device,
                None,
                label_of(devices, settings.voice_device_id.as_deref(), &copy.ellipsis),
                options(devices),
                settings.voice_device_id.clone(),
                send(Rc::new(|value| VoiceUpdate { voice_device_id: Some(value), ..Default::default() })),
                cx,
            ));
        first.push(picker_row(
                "settings-voice-agent",
                &copy.agent,
                None,
                // Web's picker trails nothing for an agent not synced here.
                label_of(agents, settings.agent_id.as_deref(), ""),
                options(agents),
                settings.agent_id.clone(),
                send(Rc::new(|value| VoiceUpdate { agent_id: Some(value), ..Default::default() })),
                cx,
            ));
    }
    first.extend([
            // #NEW-73: the spoken-reply language, then the custom voice.
            picker_row(
                "settings-voice-language",
                &copy.language,
                downloading.map(SharedString::from),
                SharedString::from(language_label(&language, device_name.as_deref())),
                languages,
                Some(language),
                send(Rc::new(|value| VoiceUpdate { language: value, ..Default::default() })),
                cx,
            ),
    ]);
    if device_name.is_some() {
        first.push(glass_input_row(copy.voice_file.clone(), voice_file, cx));
        first.push(glass_input_row(copy.voice_transcript.clone(), voice_transcript, cx));
    }
    out.push(glass_group_rows(first).into_any_element());
    if let Some(device_name) = device_name {
        // S-3: on the voice device itself the Push to talk row is THE
        // recorder; a binding that can't be held system-wide says why under
        // its label (#NEW-54). Everywhere else it is read-only.
        let here = on_change.is_some()
            && cx.try_global::<crate::session::AuthContext>().is_some_and(|auth| {
                settings.voice_device_id.as_deref() == Some(steer::persistent_device_id(&auth.data_dir).as_str())
            });
        let status = if here && settings.push_to_talk() { etalk_gpui::push_to_talk::status(cx) } else { Default::default() };
        let reason = status.reason().map(|reason| SharedString::from(shortcut_reason(reason, &settings.ptt_shortcut)));
        // S-3 (WAKE DETAIL ×4): why the wake word is or is not listening:
        // here from the worker, elsewhere from the voice device's doctor.
        let wake_detail = if here {
            etalk_gpui::push_to_talk::own_wake_detail(settings, &device_name, cx)
        } else if on_change.is_some() {
            remote_wake_detail(settings, &device_name, cx)
        } else {
            None
        };
        let recorder = if here { recorder } else { ShortcutRecorder::default() };
        let error = recorder.capture.as_ref().and_then(|capture| capture.error.clone());
        let shortcut_row = v_flex()
            .w_full()
            .child(glass_picker_row(copy.push_to_talk.clone(), reason, recorder_control(&settings.ptt_shortcut, &recorder, cx), cx))
            // feedback-states.json `errors`: invalid input inline under its field.
            .when_some(error, |row, error| row.child(crate::action_error::field_error(error, cx).px_4().pb_3()));
        out.push(crate::surface::glass_section_header(tray_header(&device_name), None, cx).into_any_element());
        let tray_rows = {
            let tray = vec![
                shortcut_row,
                switch_row(
                    "settings-voice-wake-word",
                    &copy.wake_word,
                    wake_detail.map(SharedString::from),
                    settings.wake_word,
                    ready,
                    toggle(Rc::new(|on| VoiceUpdate { wake_word: Some(on), ..Default::default() })),
                    cx,
                ),
                glass_input_row(copy.wake_phrase.clone(), wake_input, cx),
            ];
            tray
        };
        out.push(glass_group_rows(tray_rows).into_any_element());
        // #NEW-57 (`meeting.json` UI): its OWN group under the tray group, no
        // header: Meeting mode (its who on the second line while on),
        // #FEED-163 Stop after, Desktop audio, and Apps while it picks apps
        // the device can pick.
        let meeting_rows = {
            let mut tray: Vec<gpui::Div> = Vec::new();
            let words = &MEETING.copy;
            let meeting_on = settings.meeting();
            let who = meeting_on.then(|| etalk::meeting::who(settings.desktop_audio, &settings.desktop_audio_apps, capability));
            tray.push(switch_row(
                "settings-voice-meeting",
                &words.meeting,
                who.map(SharedString::from),
                meeting_on,
                ready,
                on_meeting,
                cx,
            ));
            let hours = settings.auto_stop_hours();
            tray.push(enabled_picker_row(
                "settings-voice-auto-stop",
                &words.auto_stop_after,
                None,
                SharedString::from(etalk::meeting::auto_stop_option(hours)),
                etalk::meeting::auto_stop_options().into_iter().map(|option| (Some(option.value.to_string()), option.label)).collect(),
                Some(hours.to_string()),
                ready,
                send(Rc::new(|value| VoiceUpdate {
                    meeting_auto_stop_hours: value.as_deref().and_then(|value| value.parse().ok()),
                    ..Default::default()
                })),
                cx,
            ));
            tray.push(picker_row(
                "settings-voice-desktop-audio",
                &words.desktop_audio,
                None,
                SharedString::from(settings.desktop_audio.label()),
                desktop_audio_options(capability).into_iter().map(|value| (Some(value.key().to_string()), value.label().to_string())).collect(),
                Some(settings.desktop_audio.key().to_string()),
                send(Rc::new(|value| VoiceUpdate {
                    desktop_audio: value.as_deref().map(etalk::meeting::DesktopAudio::from_key),
                    ..Default::default()
                })),
                cx,
            ));
            if settings.desktop_audio.picks_apps() && desktop_audio_options(capability).contains(&settings.desktop_audio) {
                tray.push(glass_picker_row(words.apps.clone(), None, apps_control, cx));
            }
            tray
        };
        out.push(glass_group_rows(meeting_rows).into_any_element());
    }
    out
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

enum Load {
    Idle,
    Loading,
    Ready(VoiceSettings),
    Error,
}

pub struct VoicePrefsPane {
    load: Load,
    generation: u64,
    account_id: Option<String>,
    /// The one-line inputs, built lazily from the loaded value.
    inputs: [Option<Entity<InputState>>; 3],
    _subscriptions: Vec<gpui::Subscription>,
    /// Last write wins: only the newest write may adopt or revert.
    write: u64,
    /// Writes not answered yet (a stale page is not refetched over them).
    in_flight: u32,
    /// The speaker's settings as last seen: a change there (a heartbeat, a
    /// tray toggle) drops the cache.
    seen: VoiceSettings,
    /// #NEW-56: a too-short wake phrase, never sent: its line shows under
    /// the field until the next edit (`wakePhraseError`).
    wake_phrase_invalid: Option<SharedString>,
    /// S-3: the wake word's state as last drawn (the Wake word row's
    /// detail); push to talk notifies far more often than that.
    wake_drawn: Option<etalk::voice::WakeState>,
    /// S-3: the push-to-talk recorder while it records.
    capture: Option<CaptureState>,
    /// S-3: the recorder's key focus.
    recorder_focus: Option<FocusHandle>,
    /// S-3: the key-down interceptor while recording (bindings never fire).
    _intercept: Option<gpui::Subscription>,
    _recorder_blur: Option<gpui::Subscription>,
    /// S-3: drawn as the Assistant page's Voice section (no Voice device /
    /// Agent pickers).
    assistant_block: bool,
}

/// Which one-line input (`VoicePrefsPane::inputs` index).
#[derive(Clone, Copy)]
enum Field {
    WakePhrase,
    VoiceFile,
    VoiceTranscript,
}

impl Field {
    fn value(self, settings: &VoiceSettings) -> String {
        match self {
            Field::WakePhrase => settings.wake_phrase.clone(),
            Field::VoiceFile => settings.voice_file.clone(),
            Field::VoiceTranscript => settings.voice_transcript.clone(),
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

    /// #NEW-75: THE input rule ×4 (`voice_text_input`), applied as you type.
    fn typed(self, raw: &str) -> String {
        voice_text_input(raw, self.limit())
    }

    /// FEED-160: where the caret lands in `typed` (= `self.typed(raw)`)
    /// for a caret at byte `cursor` of `raw`: the rule streams left to
    /// right, so the cleaned text before the caret is a prefix of `typed`.
    fn typed_cursor(self, raw: &str, cursor: usize, typed: &str) -> usize {
        let mut cursor = cursor.min(raw.len());
        while !raw.is_char_boundary(cursor) {
            cursor -= 1;
        }
        let mut at = self.typed(&raw[..cursor]).len().min(typed.len());
        while !typed.is_char_boundary(at) {
            at -= 1;
        }
        at
    }

    /// The value as sent and shown: the input rule, then trimmed.
    fn normalized(self, raw: &str) -> String {
        self.typed(raw).trim().to_string()
    }
}

impl VoicePrefsPane {
    pub fn new(cx: &mut gpui::Context<Self>) -> Self {
        // A change another client made (a tray toggle, a heartbeat) reaches
        // the speaker first; the open pane follows it.
        if let Some(speaker) = etalk_gpui::speak_replies::entity(cx) {
            cx.observe(&speaker, |this: &mut Self, _, cx| this.on_speaker(cx)).detach();
        }
        // S-3: on another desktop the Wake word row reads the voice
        // device's doctor (`wake`).
        if let Some(store) = sync::Store::try_global(cx) {
            cx.observe(&store.collections().devices.clone(), |_: &mut Self, _, cx| cx.notify()).detach();
        }
        // #NEW-86: the Wake word row's detail follows the microphone and Mute.
        if let Some(push_to_talk) = etalk_gpui::push_to_talk::entity(cx) {
            cx.observe(&push_to_talk, |this: &mut Self, _, cx| this.on_wake(cx)).detach();
        }
        if let Some(tray) = etalk_gpui::voice_tray::entity(cx) {
            cx.observe(&tray, |this: &mut Self, _, cx| this.on_wake(cx)).detach();
        }
        // F42: a pane dropped while it records gives the push-to-talk grab
        // back (only stop_recording would otherwise).
        cx.on_release(|this: &mut Self, cx| {
            if this.capture.is_some() {
                etalk_gpui::push_to_talk::set_recording(false, cx);
            }
        })
        .detach();
        Self {
            load: Load::Idle,
            generation: 0,
            account_id: None,
            inputs: [None, None, None],
            _subscriptions: Vec::new(),
            write: 0,
            in_flight: 0,
            seen: etalk_gpui::speak_replies::settings(cx),
            wake_phrase_invalid: None,
            wake_drawn: etalk_gpui::push_to_talk::wake_state(cx),
            capture: None,
            recorder_focus: None,
            _intercept: None,
            _recorder_blur: None,
            assistant_block: false,
        }
    }

    /// S-3: the pane as the Assistant page's Voice block.
    pub(crate) fn assistant_block(mut self) -> Self {
        self.assistant_block = true;
        self
    }

    fn on_wake(&mut self, cx: &mut gpui::Context<Self>) {
        let now = etalk_gpui::push_to_talk::wake_state(cx);
        if now != self.wake_drawn {
            self.wake_drawn = now;
            cx.notify();
        }
    }

    /// S-3: one recorder input (voice.json `captureCases`).
    fn on_recorder(&mut self, input: RecorderInput, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Load::Ready(settings) = &self.load else { return };
        let current = settings.ptt_shortcut.clone();
        match input {
            RecorderInput::Start => {
                if self.capture.is_some() {
                    return;
                }
                let mut capture = capture_start(&current);
                // Modifiers already held when the click lands count.
                for event in modifier_events(&capture, window.modifiers()) {
                    capture = capture_step(&capture, &event);
                }
                self.capture = Some(capture);
                // While it records the IDE lets its own grab go.
                etalk_gpui::push_to_talk::set_recording(true, cx);
                let focus = self.recorder_focus.get_or_insert_with(|| cx.focus_handle()).clone();
                window.focus(&focus, cx);
                self._recorder_blur = Some(cx.on_blur(&focus, window, |this, _, cx| this.stop_recording(cx)));
                let entity = cx.entity().downgrade();
                self._intercept = Some(cx.intercept_keystrokes(move |event, window, cx| {
                    let Some(entity) = entity.upgrade() else { return };
                    if entity.read(cx).capture.is_none() {
                        return;
                    }
                    cx.stop_propagation();
                    let keystroke = event.keystroke.clone();
                    entity.update(cx, |this, cx| this.on_keystroke(&keystroke, window, cx));
                }));
                cx.notify();
            }
            RecorderInput::Mouse(button) => self.capture_events(vec![CaptureEvent::Mouse(button)], cx),
            RecorderInput::Modifiers(modifiers) => {
                let Some(capture) = self.capture.as_ref() else { return };
                let events = modifier_events(capture, modifiers);
                self.capture_events(events, cx);
            }
            RecorderInput::Clear => {
                self.stop_recording(cx);
                self.apply(VoiceUpdate { ptt_shortcut: Some(String::new()), ..Default::default() }, cx);
            }
        }
    }

    /// S-3: a key-down while recording, as the grammar's key (the held
    /// modifiers first, so `ctrl-k` records `ctrl+k` even when its
    /// modifier change was not seen).
    fn on_keystroke(&mut self, keystroke: &gpui::Keystroke, _window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(capture) = self.capture.as_ref() else { return };
        let Some((key, shifted)) = etalk_gpui::push_to_talk::hotkey::grammar_key(&keystroke.key) else { return };
        let mut modifiers = keystroke.modifiers;
        modifiers.shift |= shifted;
        let mut events = modifier_events(capture, modifiers);
        events.push(CaptureEvent::Down(key));
        self.capture_events(events, cx);
    }

    fn capture_events(&mut self, events: Vec<CaptureEvent>, cx: &mut gpui::Context<Self>) {
        let Some(mut capture) = self.capture.take() else { return };
        for event in &events {
            capture = capture_step(&capture, event);
        }
        match capture.status {
            CaptureStatus::Recording => self.capture = Some(capture),
            CaptureStatus::Cancelled => self.stop_recording(cx),
            CaptureStatus::Done => {
                self.stop_recording(cx);
                self.apply(VoiceUpdate { ptt_shortcut: Some(capture.value), ..Default::default() }, cx);
            }
        }
        cx.notify();
    }

    /// S-3: the recorder ends (done, cancelled, blurred): the grab is taken
    /// again.
    fn stop_recording(&mut self, cx: &mut gpui::Context<Self>) {
        self.capture = None;
        self._intercept = None;
        self._recorder_blur = None;
        etalk_gpui::push_to_talk::set_recording(false, cx);
        cx.notify();
    }

    fn on_speaker(&mut self, cx: &mut gpui::Context<Self>) {
        let now = etalk_gpui::speak_replies::settings(cx);
        if now == self.seen {
            return;
        }
        self.seen = now;
        if self.in_flight == 0 {
            self.mark_stale(cx);
        }
    }

    /// Drop the cached settings so the next render refetches (a change made
    /// on another client).
    pub fn mark_stale(&mut self, cx: &mut gpui::Context<Self>) {
        if matches!(self.load, Load::Ready(_)) {
            self.load = Load::Idle;
            cx.notify();
        }
    }

    fn ensure_loaded(&mut self, cx: &mut gpui::Context<Self>) {
        let account_id = sync::Store::global(cx).session(cx).account_id().map(str::to_string);
        if account_id != self.account_id {
            self.account_id = account_id;
            self.load = Load::Idle;
        }
        if !matches!(self.load, Load::Idle) {
            return;
        }
        let Some(trpc) = queries::trpc_client(cx) else { return };
        self.load = Load::Loading;
        // The input fields are rebuilt from the fresh values.
        self.inputs = [None, None, None];
        self.wake_phrase_invalid = None;
        self._subscriptions.clear();
        self.generation += 1;
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { api::voice::voice_get(&trpc) }).await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.load = match result {
                    Ok(settings) => Load::Ready(settings),
                    Err(err) => {
                        crate::action_error::failed_load(
                            "your voice settings",
                            false,
                            crate::action_error::action_error_description(&err),
                            cx,
                        );
                        Load::Error
                    }
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// Optimistic: apply locally (clearOnNone included), send the partial,
    /// adopt what the server stored; a refusal reverts and toasts.
    fn apply(&mut self, update: VoiceUpdate, cx: &mut gpui::Context<Self>) {
        let Load::Ready(current) = &self.load else { return };
        let previous = current.clone();
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
        self.load = Load::Ready(next);
        cx.notify();
        let Some(trpc) = queries::trpc_client(cx) else { return };
        // Last write wins (the tray's pattern): overlapping saves never
        // clobber each other, only the newest adopts the stored row or reverts.
        self.write += 1;
        let write = self.write;
        self.in_flight += 1;
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { api::voice::voice_update(&trpc, &update) }).await;
            let _ = this.update(cx, |this, cx| {
                this.in_flight = this.in_flight.saturating_sub(1);
                let newest = this.write == write;
                match result {
                    Ok(stored) => {
                        if newest {
                            this.seen = stored.clone();
                            crate::talk_host::set_voice_settings(stored.clone(), cx);
                            this.load = Load::Ready(stored);
                        }
                    }
                    Err(err) => {
                        if newest {
                            this.load = Load::Ready(previous);
                            // A refused text shows the stored value again
                            // (else every blur would send it again).
                            this.inputs = [None, None, None];
                            this._subscriptions.clear();
                        }
                        crate::action_error::failed_action_err("save", "the voice settings", &err, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn input(&mut self, field: Field, settings: &VoiceSettings, placeholder: &str, window: &mut Window, cx: &mut gpui::Context<Self>) -> AnyElement {
        let input = match self.inputs[field as usize].clone() {
            Some(input) => input,
            None => {
                let value = field.value(settings);
                let placeholder = placeholder.to_string();
                let input = cx.new(|cx| {
                    let mut state = InputState::new(window, cx).placeholder(placeholder);
                    state.set_value(value, window, cx);
                    state
                });
                self._subscriptions.push(cx.subscribe_in(&input, window, move |this, input, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Change) {
                        // #NEW-75: an over-long paste or a tab never waits for the
                        // server to refuse it (`set_value` emits no Change).
                        let raw = input.read(cx).value().to_string();
                        let typed = field.typed(&raw);
                        if typed != raw {
                            // FEED-160: `set_value` parks the caret at the end;
                            // keep it where the user was instead.
                            let at = field.typed_cursor(&raw, input.read(cx).cursor(), &typed);
                            input.update(cx, |state, cx| {
                                state.set_value(typed, window, cx);
                                state.set_selected_range(at..at, cx);
                            });
                        }
                        if matches!(field, Field::WakePhrase) && this.wake_phrase_invalid.take().is_some() {
                            cx.notify();
                        }
                    }
                    if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                        let raw = input.read(cx).value().to_string();
                        let value = field.normalized(&raw);
                        if value != raw {
                            // What is sent is what shows.
                            input.update(cx, |state, cx| state.set_value(value.clone(), window, cx));
                        }
                        if let Some(invalid) = matches!(field, Field::WakePhrase).then(|| wake_phrase_error(&value)).flatten() {
                            this.wake_phrase_invalid = Some(invalid.into());
                            cx.notify();
                            return;
                        }
                        this.apply(field.update(value), cx);
                    }
                }));
                self.inputs[field as usize] = Some(input.clone());
                input
            }
        };
        crate::surface::glass_row_input(glass_input(&input, window, cx)).into_any_element()
    }
}

impl Render for VoicePrefsPane {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.ensure_loaded(cx);
        let mut body = section(cx);
        match &self.load {
            Load::Idle | Load::Loading => {
                body = body.child(settings_skeleton("voice-prefs-skeleton", cx));
            }
            Load::Error => {
                body = body.child(settings_skeleton("voice-prefs-error", cx));
            }
            Load::Ready(settings) => {
                let settings = settings.clone();
                let (devices, agents) = voice_choices(cx);
                let placeholder = wake_placeholder(&settings, &voice_agents(cx));
                let inputs = VoiceInputs {
                    wake_phrase: self.input(Field::WakePhrase, &settings, &placeholder, window, cx),
                    voice_file: self.input(Field::VoiceFile, &settings, &VOICE.copy.voice_file_placeholder, window, cx),
                    voice_transcript: self.input(Field::VoiceTranscript, &settings, &VOICE.copy.voice_transcript_placeholder, window, cx),
                };
                let entity = cx.entity();
                let on_change: OnVoiceChange = Rc::new(move |update, cx: &mut App| {
                    entity.update(cx, |this, cx| this.apply(update, cx));
                });
                let (capability, live) = crate::meeting_mode::desktop_audio_of(&settings, cx);
                let apps = apps_control(&settings, &live, Some(on_change.clone()), window, cx);
                let entity = cx.entity();
                let on_meeting: Rc<dyn Fn(bool, &mut App)> = Rc::new(move |on, cx: &mut App| {
                    if on {
                        // Never writes by itself: THE consent dialog's Start does.
                        entity.update(cx, |_, cx| cx.notify());
                        crate::meeting_mode::prompt_consent(cx);
                    } else {
                        entity.update(cx, |this, cx| {
                            if let Load::Ready(settings) = &mut this.load {
                                settings.meeting_started_at = None;
                            }
                            cx.notify();
                        });
                        crate::meeting_mode::stop(cx);
                    }
                });
                let meeting = MeetingRows { capability, apps, on_meeting: Some(on_meeting) };
                let entity = cx.entity();
                let recorder_focus = self.recorder_focus.get_or_insert_with(|| cx.focus_handle()).clone();
                let recorder = ShortcutRecorder {
                    capture: self.capture.clone(),
                    focus: Some(recorder_focus),
                    on_input: Some(Rc::new(move |input, window, cx: &mut App| {
                        entity.update(cx, |this, cx| this.on_recorder(input, window, cx));
                    })),
                };
                let rows = voice_rows(&settings, &devices, &agents, inputs, meeting, recorder, Some(on_change), self.assistant_block, cx);
                let count = rows.len();
                for (index, row) in rows.into_iter().enumerate() {
                    // The wake phrase closes the tray group (the meeting
                    // group, #NEW-57, is last): its line goes under it.
                    if index + 1 == count {
                        if let Some(message) = self.wake_phrase_invalid.clone().filter(|_| settings.voice_device_id.is_some()) {
                            body = body.child(crate::action_error::field_error(message, cx).px_1());
                        }
                    }
                    body = body.child(row);
                }
            }
        }
        v_flex().child(body)
    }
}

/// S-3: the recorder events that bring `capture`'s held modifiers to
/// `modifiers` (gpui's `platform` = the grammar's `super`).
fn modifier_events(capture: &CaptureState, modifiers: Modifiers) -> Vec<CaptureEvent> {
    [("ctrl", modifiers.control), ("alt", modifiers.alt), ("shift", modifiers.shift), ("super", modifiers.platform)]
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

#[cfg(test)]
mod tests {
    use super::{modifier_events, Field};
    use etalk::voice::{capture_pending, capture_start, capture_step, CaptureEvent, CaptureStatus};

    /// S-3: gpui's modifier changes fold into the recorder like the
    /// fixture's `down` / `up` events.
    #[test]
    fn modifier_changes_feed_the_recorder() {
        let held = gpui::Modifiers { control: true, shift: true, ..Default::default() };
        let mut capture = capture_start("ctrl+space");
        for event in modifier_events(&capture, held) {
            capture = capture_step(&capture, &event);
        }
        assert_eq!(capture_pending(&capture), "Ctrl + Shift + …");
        assert!(modifier_events(&capture, held).is_empty(), "nothing changed");
        let events = modifier_events(&capture, gpui::Modifiers { control: true, ..Default::default() });
        assert_eq!(events, vec![CaptureEvent::Up("shift".into())]);
        let mut capture = events.iter().fold(capture, |capture, event| capture_step(&capture, event));
        capture = capture_step(&capture, &CaptureEvent::Down("k".into()));
        assert_eq!((capture.status, capture.value.as_str()), (CaptureStatus::Done, "ctrl+k"));
        let platform = modifier_events(&capture_start(""), gpui::Modifiers { platform: true, ..Default::default() });
        assert_eq!(platform, vec![CaptureEvent::Down("super".into())]);
    }

    #[test]
    fn typed_cursor_stays_where_the_user_was() {
        // A control character pasted mid-text: the caret sits after the
        // paste, not at the end.
        let raw = "ab\u{7}cd";
        let typed = Field::WakePhrase.typed(raw);
        assert_eq!(typed, "abcd");
        assert_eq!(Field::WakePhrase.typed_cursor(raw, 3, &typed), 2);
        // Whatever a whitespace control becomes, the caret follows it.
        let raw = "ab\tcd";
        let typed = Field::VoiceFile.typed(raw);
        let at = Field::VoiceFile.typed_cursor(raw, 3, &typed);
        assert_eq!(&typed[..at], Field::VoiceFile.typed("ab\t"));
        assert_eq!(&typed[at..], "cd");
        // An over-long paste: the caret clamps to the cut value.
        let raw = "x".repeat(Field::WakePhrase.limit() + 10);
        let typed = Field::WakePhrase.typed(&raw);
        assert_eq!(Field::WakePhrase.typed_cursor(&raw, raw.len(), &typed), typed.len());
        // A caret inside a multi-byte char never splits it.
        let raw = "é\u{7}";
        let typed = Field::WakePhrase.typed(raw);
        assert_eq!(Field::WakePhrase.typed_cursor(raw, 1, &typed), 0);
    }
}
