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
    AnyElement, App, AppContext as _, Entity, IntoElement, ParentElement as _, Render, SharedString, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    input::{InputEvent, InputState},
    menu::DropdownMenu as _,
    v_flex, ActiveTheme as _, Disableable as _,
};

use api::voice::VoiceUpdate;
use domain::voice::{
    agent_options, tray_header, voice_device_options, wake_phrase_placeholder, VoiceAgent, VoiceDevice,
    VoiceOption, VoiceSettings, VOICE,
};

use crate::controls::glass_input;
use crate::queries;
use crate::surface::{glass_group_rows, glass_input_row, glass_picker_row, glass_toggle_row, picker_value_label};

use super::{section, settings_skeleton};

/// A change from one row: the partial update to send.
pub(crate) type OnVoiceChange = Rc<dyn Fn(VoiceUpdate, &mut App)>;

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
    let teams = collections.teams.read(cx);
    let agents: Vec<VoiceAgent> = collections
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
        .collect();
    (voice_device_options(&devices), agent_options(&agents))
}

fn label_of(options: &[VoiceOption], id: Option<&str>) -> SharedString {
    options
        .iter()
        .find(|option| option.id.as_deref() == id)
        .or_else(|| options.first())
        .map(|option| SharedString::from(option.label.clone()))
        .unwrap_or_default()
}

/// One picker row: the label, the current value, a single-select menu.
fn picker_row(
    id: &'static str,
    label: &str,
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
        .disabled(on_pick.is_none())
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
    glass_picker_row(label.to_string(), None, button.into_any_element(), cx)
}

fn switch_row(
    id: &'static str,
    label: &str,
    checked: bool,
    enabled: bool,
    on_toggle: Option<Rc<dyn Fn(bool, &mut App)>>,
    cx: &App,
) -> gpui::Div {
    let switch = crate::controls::web_switch(id).checked(checked && enabled).disabled(!enabled || on_toggle.is_none());
    let switch = match on_toggle {
        Some(on_toggle) => switch.on_click(move |on: &bool, _, cx| on_toggle(*on, cx)),
        None => switch,
    };
    glass_toggle_row(label.to_string(), None, switch.into_any_element(), cx)
}

/// THE Voice rows (`voice.json`): group 1, then the tray group while a voice
/// device is picked. `wake_input` = the wake phrase's input element (the
/// pane's live field, or the twin's static one); `on_change` = None draws
/// the rows inert (the styleguide twin).
pub(crate) fn voice_rows(
    settings: &VoiceSettings,
    devices: &[VoiceOption],
    agents: &[VoiceOption],
    wake_input: AnyElement,
    on_change: Option<OnVoiceChange>,
    cx: &App,
) -> Vec<AnyElement> {
    let copy = &VOICE.copy;
    let ready = settings.voice_device_id.is_some() && settings.agent_id.is_some();
    let send = |make: Rc<dyn Fn(Option<String>) -> VoiceUpdate>| -> Option<Rc<dyn Fn(Option<String>, &mut App)>> {
        on_change.clone().map(|on_change| {
            Rc::new(move |value: Option<String>, cx: &mut App| on_change(make(value), cx)) as Rc<dyn Fn(Option<String>, &mut App)>
        })
    };
    let toggle = |make: Rc<dyn Fn(bool) -> VoiceUpdate>| -> Option<Rc<dyn Fn(bool, &mut App)>> {
        on_change.clone().map(|on_change| Rc::new(move |on: bool, cx: &mut App| on_change(make(on), cx)) as Rc<dyn Fn(bool, &mut App)>)
    };
    let options = |list: &[VoiceOption]| list.iter().map(|option| (option.id.clone(), option.label.clone())).collect::<Vec<_>>();

    let device_name = devices
        .iter()
        .find(|option| option.id.is_some() && option.id == settings.voice_device_id)
        .map(|option| option.label.clone());

    let mut out: Vec<AnyElement> = Vec::new();
    out.push(
        glass_group_rows(vec![
            picker_row(
                "settings-voice-device",
                &copy.voice_device,
                label_of(devices, settings.voice_device_id.as_deref()),
                options(devices),
                settings.voice_device_id.clone(),
                send(Rc::new(|value| VoiceUpdate { voice_device_id: Some(value), ..Default::default() })),
                cx,
            ),
            picker_row(
                "settings-voice-agent",
                &copy.agent,
                label_of(agents, settings.agent_id.as_deref()),
                options(agents),
                settings.agent_id.clone(),
                send(Rc::new(|value| VoiceUpdate { agent_id: Some(value), ..Default::default() })),
                cx,
            ),
            switch_row(
                "settings-voice-speak-replies",
                &copy.speak_replies,
                settings.speak_replies,
                ready,
                toggle(Rc::new(|on| VoiceUpdate { speak_replies: Some(on), ..Default::default() })),
                cx,
            ),
        ])
        .into_any_element(),
    );
    if let Some(device_name) = device_name {
        let shortcut_label = VOICE
            .shortcuts
            .iter()
            .find(|shortcut| shortcut.value == settings.ptt_shortcut)
            .map(|shortcut| shortcut.label.clone())
            .unwrap_or_else(|| settings.ptt_shortcut.clone());
        let shortcuts = VOICE.shortcuts.iter().map(|shortcut| (Some(shortcut.value.clone()), shortcut.label.clone())).collect();
        out.push(crate::surface::glass_section_header(tray_header(&device_name), None, cx).into_any_element());
        out.push(
            glass_group_rows(vec![
                switch_row(
                    "settings-voice-push-to-talk",
                    &copy.push_to_talk,
                    settings.push_to_talk,
                    ready,
                    toggle(Rc::new(|on| VoiceUpdate { push_to_talk: Some(on), ..Default::default() })),
                    cx,
                ),
                picker_row(
                    "settings-voice-shortcut",
                    &copy.shortcut,
                    SharedString::from(shortcut_label),
                    shortcuts,
                    Some(settings.ptt_shortcut.clone()),
                    send(Rc::new(|value| VoiceUpdate { ptt_shortcut: value, ..Default::default() })),
                    cx,
                ),
                switch_row(
                    "settings-voice-wake-word",
                    &copy.wake_word,
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

/// The wake phrase's placeholder for the picked agent.
pub(crate) fn wake_placeholder(settings: &VoiceSettings, agents: &[VoiceOption]) -> String {
    let agent = agents
        .iter()
        .find(|option| option.id.is_some() && option.id == settings.agent_id)
        .map(|option| option.label.as_str())
        .unwrap_or("…");
    wake_phrase_placeholder(agent)
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
    wake_input: Option<Entity<InputState>>,
    _wake_subscription: Option<gpui::Subscription>,
}

impl VoicePrefsPane {
    pub fn new(_cx: &mut gpui::Context<Self>) -> Self {
        Self { load: Load::Idle, generation: 0, account_id: None, wake_input: None, _wake_subscription: None }
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
        // The wake phrase field is rebuilt from the fresh value.
        self.wake_input = None;
        self._wake_subscription = None;
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
        let next = next.clear_on_none();
        if next == previous {
            return;
        }
        self.load = Load::Ready(next);
        cx.notify();
        let Some(trpc) = queries::trpc_client(cx) else { return };
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { api::voice::voice_update(&trpc, &update) }).await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(stored) => {
                        crate::speak_replies::set_settings(stored.clone(), cx);
                        this.load = Load::Ready(stored);
                    }
                    Err(err) => {
                        this.load = Load::Ready(previous);
                        crate::action_error::failed_action_err("save", "the voice settings", &err, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn wake_input(&mut self, settings: &VoiceSettings, placeholder: &str, window: &mut Window, cx: &mut gpui::Context<Self>) -> Entity<InputState> {
        if let Some(input) = self.wake_input.clone() {
            return input;
        }
        let value = settings.wake_phrase.clone();
        let placeholder = placeholder.to_string();
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder(placeholder);
            state.set_value(value, window, cx);
            state
        });
        self._wake_subscription = Some(cx.subscribe(&input, |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                let phrase = input.read(cx).value().to_string();
                this.apply(VoiceUpdate { wake_phrase: Some(phrase), ..Default::default() }, cx);
            }
        }));
        self.wake_input = Some(input.clone());
        input
    }
}

impl Render for VoicePrefsPane {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.ensure_loaded(cx);
        let mut body = section(cx).child(crate::surface::glass_section_header(VOICE.copy.group.clone(), None, cx));
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
                let placeholder = wake_placeholder(&settings, &agents);
                let input = self.wake_input(&settings, &placeholder, window, cx);
                let wake = crate::surface::glass_row_input(glass_input(&input, window, cx)).into_any_element();
                let entity = cx.entity();
                let on_change: OnVoiceChange = Rc::new(move |update, cx: &mut App| {
                    entity.update(cx, |this, cx| this.apply(update, cx));
                });
                for row in voice_rows(&settings, &devices, &agents, wake, Some(on_change), cx) {
                    body = body.child(row);
                }
            }
        }
        v_flex().child(body)
    }
}
