//! NEW-67 (voice plan v3, V1) — Settings → Personal → Voice
//! (`fixtures/voice.json`): the member's ONE voice settings row, read with
//! `voice.get`, written with `voice.update` (server-only, like the
//! notification prefs). Group 1: Voice device (the member's desktops), Agent,
//! Speak replies. While a voice device is picked, group 2 under "On {device}
//! (tray)": Push to talk, Shortcut, Wake word, Wake phrase. Every switch reads
//! off and is disabled while the device or the agent is None (clearOnNone).
//! A save hands the stored settings to `speak_replies` at once, so this
//! device starts or stops speaking without waiting for the next heartbeat.
//!
//! [`voice_rows`] draws the rows from plain values; the pane and the
//! styleguide twin (`styleguide/entries/voice_settings.rs`) both call it.

use std::rc::Rc;

use gpui::{
    AnyElement, App, AppContext as _, Entity, IntoElement, ParentElement as _, Render, SharedString, Styled as _, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    input::{InputEvent, InputState},
    menu::DropdownMenu as _,
    v_flex, ActiveTheme as _, Disableable as _,
};

use api::voice::VoiceUpdate;
use domain::voice::{
    agent_options, language_label, shortcut_label, shortcut_options, shortcut_reason, tray_header, voice_device_options, wake_phrase_placeholder, VoiceAgent, VoiceDevice,
    VoiceOption, VoiceSettings, VOICE,
};

use crate::controls::glass_input;
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
/// synced here (the shape not ready, the row gone) reads as the id itself,
/// never as None (web `voice-settings-card`).
fn label_of(options: &[VoiceOption], id: Option<&str>) -> SharedString {
    options
        .iter()
        .find(|option| option.id.as_deref() == id)
        .map(|option| option.label.clone())
        .or_else(|| id.map(str::to_string))
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
    let value_color = cx.theme().foreground.opacity(0.7);
    let button = Button::new(id)
        .ghost()
        .cursor_pointer()
        .h_auto()
        .px_0()
        .py_0()
        .text_color(value_color)
        .child(picker_value_label(value))
        .child(crate::surface::picker_row_chevron(cx))
        .dropdown_menu(move |mut menu, _, _| {
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

/// The page's three one-line inputs (the pane's live fields, or the twin's
/// static ones), already wrapped in `glass_row_input`.
pub(crate) struct VoiceInputs {
    pub wake_phrase: AnyElement,
    pub voice_file: AnyElement,
    pub voice_transcript: AnyElement,
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
    on_change: Option<OnVoiceChange>,
    cx: &App,
) -> Vec<AnyElement> {
    let VoiceInputs { wake_phrase: wake_input, voice_file, voice_transcript } = inputs;
    let copy = &VOICE.copy;
    let ready = settings.voice_device_id.is_some() && settings.agent_id.is_some();
    // voice.json: while the voice downloads (only ever on the voice device,
    // i.e. here) the Speak replies row's detail reads `downloadProgress`.
    let downloading = (on_change.is_some() && ready && settings.speak_replies)
        .then(|| crate::speak_replies::download_detail(cx))
        .flatten();
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
    // synced here (its label, else the id), as on web.
    let device_name = settings.voice_device_id.as_deref().map(|id| label_of(devices, Some(id)).to_string());

    let language = settings.language().to_string();
    // Every language carries its GPU note (#NEW-74), in the row and in the menu.
    let languages = VOICE
        .languages
        .iter()
        .map(|entry| (Some(entry.value.clone()), language_label(&entry.value, device_name.as_deref())))
        .collect();

    let mut out: Vec<AnyElement> = Vec::new();
    let mut first: Vec<gpui::Div> = vec![
            picker_row(
                "settings-voice-device",
                &copy.voice_device,
                None,
                label_of(devices, settings.voice_device_id.as_deref()),
                options(devices),
                settings.voice_device_id.clone(),
                send(Rc::new(|value| VoiceUpdate { voice_device_id: Some(value), ..Default::default() })),
                cx,
            ),
            picker_row(
                "settings-voice-agent",
                &copy.agent,
                None,
                label_of(agents, settings.agent_id.as_deref()),
                options(agents),
                settings.agent_id.clone(),
                send(Rc::new(|value| VoiceUpdate { agent_id: Some(value), ..Default::default() })),
                cx,
            ),
            switch_row(
                "settings-voice-speak-replies",
                &copy.speak_replies,
                downloading.map(SharedString::from),
                settings.speak_replies,
                ready,
                toggle(Rc::new(|on| VoiceUpdate { speak_replies: Some(on), ..Default::default() })),
                cx,
            ),
            // #NEW-73: the spoken-reply language, then the custom voice.
            picker_row(
                "settings-voice-language",
                &copy.language,
                None,
                SharedString::from(language_label(&language, device_name.as_deref())),
                languages,
                Some(language),
                send(Rc::new(|value| VoiceUpdate { language: value, ..Default::default() })),
                cx,
            ),
    ];
    if device_name.is_some() {
        first.push(glass_input_row(copy.voice_file.clone(), voice_file, cx));
        first.push(glass_input_row(copy.voice_transcript.clone(), voice_transcript, cx));
    }
    out.push(glass_group_rows(first).into_any_element());
    if let Some(device_name) = device_name {
        // #NEW-54: on the voice device itself the picker offers what this OS
        // can hold (no mouse buttons on macOS or Wayland), the portal's
        // trigger names the shortcut, and a shortcut that can't be held
        // system-wide says why under the row.
        let here = on_change.is_some()
            && cx.try_global::<crate::session::AuthContext>().is_some_and(|auth| {
                settings.voice_device_id.as_deref() == Some(steer::persistent_device_id(&auth.data_dir).as_str())
            });
        let status = if here && settings.push_to_talk { crate::push_to_talk::status(cx) } else { Default::default() };
        let shortcut_label = status.trigger().map(str::to_string).unwrap_or_else(|| shortcut_label(&settings.ptt_shortcut));
        let reason = status.reason().map(|reason| SharedString::from(shortcut_reason(reason, &settings.ptt_shortcut)));
        let shortcuts = if here {
            let (os, session) = crate::push_to_talk::platform();
            shortcut_options(os, session).into_iter().map(|shortcut| (Some(shortcut.value.clone()), shortcut.label.clone())).collect()
        } else {
            VOICE.shortcuts.iter().map(|shortcut| (Some(shortcut.value.clone()), shortcut.label.clone())).collect()
        };
        out.push(crate::surface::glass_section_header(tray_header(&device_name), None, cx).into_any_element());
        out.push(
            glass_group_rows(vec![
                switch_row(
                    "settings-voice-push-to-talk",
                    &copy.push_to_talk,
                    None,
                    settings.push_to_talk,
                    ready,
                    toggle(Rc::new(|on| VoiceUpdate { push_to_talk: Some(on), ..Default::default() })),
                    cx,
                ),
                picker_row(
                    "settings-voice-shortcut",
                    &copy.shortcut,
                    reason,
                    SharedString::from(shortcut_label),
                    shortcuts,
                    Some(settings.ptt_shortcut.clone()),
                    send(Rc::new(|value| VoiceUpdate { ptt_shortcut: value, ..Default::default() })),
                    cx,
                ),
                switch_row(
                    "settings-voice-wake-word",
                    &copy.wake_word,
                    None,
                    settings.wake_word,
                    ready,
                    toggle(Rc::new(|on| VoiceUpdate { wake_word: Some(on), ..Default::default() })),
                    cx,
                ),
                glass_input_row(copy.wake_phrase.clone(), wake_input, cx),
            ])
            .into_any_element(),
        );
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

/// The wake phrase's length limit (web `maxLength={64}`).
const WAKE_PHRASE_LIMIT: usize = 64;

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
            Field::WakePhrase => WAKE_PHRASE_LIMIT,
            Field::VoiceFile => VOICE.limits.voice_file,
            Field::VoiceTranscript => VOICE.limits.voice_transcript,
        }
    }

    /// The value as sent and shown: no control characters, trimmed, within
    /// the limit (the server checks the limit BEFORE trimming).
    fn normalized(self, raw: &str) -> String {
        let clean: String = raw.chars().filter(|ch| !ch.is_control()).collect();
        clean.trim().chars().take(self.limit()).collect::<String>().trim_end().to_string()
    }
}

impl VoicePrefsPane {
    pub fn new(cx: &mut gpui::Context<Self>) -> Self {
        // A change another client made (a tray toggle, a heartbeat) reaches
        // the speaker first; the open pane follows it.
        if let Some(speaker) = crate::speak_replies::entity(cx) {
            cx.observe(&speaker, |this: &mut Self, _, cx| this.on_speaker(cx)).detach();
        }
        Self {
            load: Load::Idle,
            generation: 0,
            account_id: None,
            inputs: [None, None, None],
            _subscriptions: Vec::new(),
            write: 0,
            in_flight: 0,
            seen: crate::speak_replies::settings(cx),
        }
    }

    fn on_speaker(&mut self, cx: &mut gpui::Context<Self>) {
        let now = crate::speak_replies::settings(cx);
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
        if let Some(value) = update.speak_replies {
            next.speak_replies = value;
        }
        if let Some(value) = update.push_to_talk {
            next.push_to_talk = value;
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
                            crate::speak_replies::set_settings(stored.clone(), cx);
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
                    if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                        let raw = input.read(cx).value().to_string();
                        let value = field.normalized(&raw);
                        if value != raw {
                            // What is sent is what shows.
                            input.update(cx, |state, cx| state.set_value(value.clone(), window, cx));
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
                for row in voice_rows(&settings, &devices, &agents, inputs, Some(on_change), cx) {
                    body = body.child(row);
                }
            }
        }
        v_flex().child(body)
    }
}
