//! NEW-53: exponentialTalk's voice settings and spoken-reply rules, the
//! desktop mirror of `packages/domain-contract/src/voice.ts`, byte-locked by
//! `fixtures/voice.json` (every case list replays below).

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::{Captures, Regex};
use serde::Deserialize;

const VOICE_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/voice.json");

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceCopy {
    pub group: String,
    pub voice_device: String,
    pub voice_device_none: String,
    pub agent: String,
    pub agent_none: String,
    pub speak_replies: String,
    pub language: String,
    pub language_note: String,
    pub voice_file: String,
    pub voice_file_placeholder: String,
    pub voice_transcript: String,
    pub voice_transcript_placeholder: String,
    pub tray_header: String,
    pub push_to_talk: String,
    pub shortcut: String,
    pub wake_word: String,
    pub wake_phrase: String,
    pub wake_phrase_placeholder: String,
    pub tray_open: String,
    pub tray_call: String,
    pub tray_mute: String,
    pub tray_settings: String,
    pub tray_quit: String,
    pub downloading: String,
    pub downloading_progress: String,
    pub download_failed: String,
    pub unavailable: String,
    pub speaking: String,
    pub stop: String,
    pub link: String,
    pub ellipsis: String,
    pub wake_phrase_too_short: String,
}

/// #NEW-54: push to talk's words (`pttCopy`, desktop-only).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PttCopy {
    pub shortcut_unavailable: String,
    pub shortcut_reasons: ShortcutReasons,
    pub overlay_listening: String,
    pub overlay_heard: String,
    pub overlay_thinking: String,
    pub overlay_replied: String,
    pub overlay_muted: String,
    pub overlay_muted_line: String,
    pub overlay_unmute: String,
    pub overlay_not_sent: String,
    pub overlay_not_sent_line: String,
    pub listen_failed: ListenFailedCopy,
}

/// #NEW-54: why a push-to-talk shortcut can't be held (`copy.shortcutReasons`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutReasons {
    pub taken: String,
    pub no_portal: String,
    pub declined: String,
    pub mouse: String,
}

/// #NEW-54: the overlay's line per Listen failure (`copy.listenFailed`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ListenFailedCopy {
    pub no_microphone: String,
    pub not_granted: String,
    pub downloading: String,
    pub unavailable: String,
    pub failed: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceTestIds {
    pub page: String,
    pub voice_device: String,
    pub agent: String,
    pub speak_replies: String,
    pub language: String,
    pub voice_file: String,
    pub voice_transcript: String,
    pub push_to_talk: String,
    pub shortcut: String,
    pub wake_word: String,
    pub wake_phrase: String,
}

/// One push-to-talk shortcut (`shortcuts`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct VoiceShortcut {
    pub value: String,
    pub label: String,
}

/// #NEW-73: one spoken-reply language (`languages`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct VoiceLanguage {
    pub value: String,
    pub label: String,
}

/// #NEW-74: the words a spoken reply uses in one language (`spokenWords`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SpokenWords {
    pub link: String,
    pub point: String,
    pub percent: String,
}

/// The input rows' length caps (`limits`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceLimits {
    pub voice_file: usize,
    pub voice_transcript: usize,
    /// #NEW-56: the letters a stored wake phrase needs at least.
    pub wake_phrase_min_letters: usize,
}

/// The member's voice settings (`user_voice_settings`, the heartbeat's
/// `voice`, `voice.get`); `defaults` when the member has no row.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VoiceSettings {
    pub voice_device_id: Option<String>,
    pub agent_id: Option<String>,
    pub speak_replies: bool,
    pub push_to_talk: bool,
    pub ptt_shortcut: String,
    pub wake_word: bool,
    pub wake_phrase: String,
    /// #NEW-73: a `languages` value; empty reads as English.
    pub language: String,
    /// A reference WAV's path on the voice device; empty = the built-in voice.
    pub voice_file: String,
    pub voice_transcript: String,
}

impl VoiceSettings {
    /// The spoken-reply language (`en` while unset).
    pub fn language(&self) -> &str {
        if self.language.is_empty() { "en" } else { &self.language }
    }

    /// voice.json `clearOnNone`: with no voice device or no agent nothing
    /// listens and nothing speaks.
    pub fn clear_on_none(mut self) -> Self {
        if self.voice_device_id.is_none() || self.agent_id.is_none() {
            self.speak_replies = false;
            self.push_to_talk = false;
            self.wake_word = false;
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceSpec {
    pub copy: VoiceCopy,
    pub ptt_copy: PttCopy,
    pub max_chars: usize,
    /// Keyed by language (`en`, `de`).
    pub spoken_words: HashMap<String, SpokenWords>,
    pub test_ids: VoiceTestIds,
    pub shortcuts: Vec<VoiceShortcut>,
    pub languages: Vec<VoiceLanguage>,
    pub defaults: VoiceSettings,
    pub limits: VoiceLimits,
    /// #NEW-54: how long an overlay state stays before it fades (ms).
    pub overlay_hide_after_ms: HashMap<String, u64>,
    /// #NEW-54: the styleguide `voice-call-overlay` rows.
    pub overlay_specimens: Vec<OverlaySpecimen>,
}

/// #NEW-54: one styleguide capsule (`overlaySpecimens`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct OverlaySpecimen {
    pub activity: String,
    pub level: f32,
    pub bands: Vec<f32>,
    pub state: String,
    pub line: String,
    pub pill: String,
    pub primary: bool,
}

/// #NEW-73: what the Language row and its menu show for a language.
pub fn language_label(language: &str, device_name: Option<&str>) -> String {
    let label = VOICE
        .languages
        .iter()
        .find(|entry| entry.value == language)
        .map(|entry| entry.label.clone())
        .unwrap_or_else(|| language.to_string());
    match language_note(language, device_name) {
        Some(note) => format!("{label} · {note}"),
        None => label,
    }
}

/// #NEW-73: the Language row's trailing detail; every language speaks on the
/// voice device's GPU only (#NEW-74). None without a voice device.
pub fn language_note(_language: &str, device_name: Option<&str>) -> Option<String> {
    let device_name = device_name?;
    Some(VOICE.copy.language_note.replace("{device}", device_name))
}

/// The tray group's header for the voice device.
pub fn tray_header(device_name: &str) -> String {
    VOICE.copy.tray_header.replace("{device}", device_name)
}

/// The spoken words of a language; an unknown one reads as English.
pub fn spoken_words(language: &str) -> &'static SpokenWords {
    VOICE.spoken_words.get(language).or_else(|| VOICE.spoken_words.get("en")).expect("voice.json spokenWords.en")
}

const GB: u64 = 1_000_000_000;
const MB: u64 = 1_000_000;

/// One size for the download detail: MB below 1 GB, else GB with one
/// decimal; `down` rounds down (received never overstates progress).
fn download_size(bytes: u64, down: bool) -> String {
    if bytes < GB {
        let mb = if down { bytes / MB } else { (bytes + MB / 2) / MB };
        return format!("{mb} MB");
    }
    let tenth = GB / 10;
    let tenths = if down { bytes / tenth } else { (bytes + tenth / 2) / tenth };
    format!("{}.{} GB", tenths / 10, tenths % 10)
}

/// The Speak replies row's detail while the voice downloads; a total of 0
/// (unknown) reads `copy.downloading`.
pub fn download_progress(received: u64, total: u64) -> String {
    if total == 0 {
        return VOICE.copy.downloading.clone();
    }
    VOICE
        .copy
        .downloading_progress
        .replace("{received}", &download_size(received, true))
        .replace("{total}", &download_size(total, false))
}

/// The wake phrase's placeholder (and its value when empty).
pub fn wake_phrase_placeholder(agent_name: &str) -> String {
    VOICE.copy.wake_phrase_placeholder.replace("{agent}", agent_name)
}

/// #NEW-56: a stored wake phrase needs `limits.wakePhraseMinLetters`
/// letters (letters only), so a short word can't fire on every segment;
/// empty = the placeholder, valid (`wakePhraseError`). The refusal's text.
pub fn wake_phrase_error(text: &str) -> Option<String> {
    let trimmed = text.trim();
    let letters = trimmed.chars().filter(|ch| ch.is_alphabetic()).count();
    (!trimmed.is_empty() && letters < VOICE.limits.wake_phrase_min_letters).then(|| VOICE.copy.wake_phrase_too_short.clone())
}

/// #NEW-56: the phrase the wake word listens for: the member's, else
/// `Hey {agent}` (None while that needs an agent name nobody knows yet, or
/// for a too-short phrase stored before [`wake_phrase_error`] refused it).
pub fn wake_phrase(settings: &VoiceSettings, agent: Option<&str>) -> Option<String> {
    match settings.wake_phrase.trim() {
        "" => agent.map(wake_phrase_placeholder),
        phrase if wake_phrase_error(phrase).is_some() => None,
        phrase => Some(phrase.to_string()),
    }
}

/// #NEW-56: the tray icon (`trayIcon`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrayIcon {
    App,
    /// The app icon with the always-on microphone dot.
    Listening,
}

/// #NEW-56: the tray icon; None = no tray on this device.
pub fn tray_icon(device_is_voice_device: bool, wake_listening: bool, muted: bool) -> Option<TrayIcon> {
    if !device_is_voice_device {
        return None;
    }
    Some(if wake_listening && !muted { TrayIcon::Listening } else { TrayIcon::App })
}

/// #NEW-86: the IDE's Wake word row detail on the voice device itself
/// (`wakeDetail`): Microphone muted, Listening while the wake word holds the
/// microphone, else None. A tray host is not always there to show it.
pub fn wake_detail(here: bool, wake_word: bool, wake_listening: bool, muted: bool) -> Option<String> {
    if !here || !wake_word {
        return None;
    }
    if muted {
        return Some(VOICE.ptt_copy.overlay_muted.clone());
    }
    wake_listening.then(|| VOICE.ptt_copy.overlay_listening.clone())
}

/// #NEW-68: one tray menu entry's identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrayItemId {
    Open,
    Call,
    SpeakReplies,
    PushToTalk,
    WakeWord,
    Mute,
    Settings,
    Quit,
}

/// #NEW-68: one entry of the tray's OS-native menu.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TrayItem {
    Separator,
    Action { id: TrayItemId, label: String, enabled: bool },
    Check { id: TrayItemId, label: String, checked: bool, enabled: bool },
}

/// #NEW-68: what the tray menu reads besides the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrayContext<'a> {
    /// The voice agent's name (None = no agent).
    pub agent: Option<&'a str>,
    /// Mute microphone (IDE-local, never stored).
    pub muted: bool,
    /// A call can start (V4; false until then).
    pub call_available: bool,
    /// The microphone listens (V3; false until then): Mute shows.
    pub mic_available: bool,
    /// THIS device is the member's voice device.
    pub device_is_voice_device: bool,
    /// #NEW-54: the push-to-talk shortcut could not be grabbed.
    pub shortcut_unavailable: bool,
    /// #NEW-54: the portal's trigger description (Wayland).
    pub shortcut_label: Option<&'a str>,
}

/// #NEW-68: the tray's OS-native menu; None = no tray on this device.
pub fn tray_menu(settings: &VoiceSettings, context: &TrayContext<'_>) -> Option<Vec<TrayItem>> {
    if !context.device_is_voice_device {
        return None;
    }
    let copy = &VOICE.copy;
    let agent = context.agent.unwrap_or("…");
    let ready = settings.voice_device_id.is_some() && settings.agent_id.is_some();
    let label = context.shortcut_label.map(str::to_string).unwrap_or_else(|| shortcut_label(&settings.ptt_shortcut));
    let shortcut =
        if context.shortcut_unavailable { VOICE.ptt_copy.shortcut_unavailable.replace("{shortcut}", &label) } else { label };
    let phrase = wake_phrase(settings, Some(agent)).unwrap_or_default();
    let check = |id, label: String, on: bool| TrayItem::Check { id, label, checked: on && ready, enabled: ready };
    let mut menu = vec![TrayItem::Action { id: TrayItemId::Open, label: copy.tray_open.clone(), enabled: true }];
    if context.call_available {
        menu.push(TrayItem::Action { id: TrayItemId::Call, label: copy.tray_call.replace("{agent}", agent), enabled: ready });
    }
    menu.extend([
        TrayItem::Separator,
        check(TrayItemId::SpeakReplies, copy.speak_replies.clone(), settings.speak_replies),
        check(TrayItemId::PushToTalk, format!("{} · {shortcut}", copy.push_to_talk), settings.push_to_talk),
        check(TrayItemId::WakeWord, format!("{} · {phrase}", copy.wake_word), settings.wake_word),
    ]);
    if context.mic_available {
        menu.push(TrayItem::Check { id: TrayItemId::Mute, label: copy.tray_mute.clone(), checked: context.muted, enabled: true });
    }
    menu.extend([
        TrayItem::Separator,
        TrayItem::Action { id: TrayItemId::Settings, label: copy.tray_settings.clone(), enabled: true },
        TrayItem::Action { id: TrayItemId::Quit, label: copy.tray_quit.clone(), enabled: true },
    ]);
    Some(menu)
}

/// #NEW-54: a shortcut's label (`shortcuts`), the value itself when unknown.
pub fn shortcut_label(value: &str) -> String {
    VOICE.shortcuts.iter().find(|entry| entry.value == value).map(|entry| entry.label.clone()).unwrap_or_else(|| value.to_string())
}

/// #NEW-54: the voice device's OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceOs {
    Linux,
    Macos,
    Windows,
}

/// #NEW-54: the Linux display session (None elsewhere).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceSession {
    X11,
    Wayland,
}

/// #NEW-54: how the voice device holds a push-to-talk shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutSupport {
    /// Grabbed system-wide (X11, macOS, Windows).
    Global,
    /// Bound through the xdg GlobalShortcuts portal (Wayland).
    Portal,
    /// Only while an IDE window is focused (Wayland without the portal).
    FocusedOnly,
    /// Never (a mouse button on macOS or Wayland): hidden from the picker.
    Unavailable,
}

impl ShortcutSupport {
    pub fn key(self) -> &'static str {
        match self {
            ShortcutSupport::Global => "global",
            ShortcutSupport::Portal => "portal",
            ShortcutSupport::FocusedOnly => "focused-only",
            ShortcutSupport::Unavailable => "unavailable",
        }
    }
}

/// Whether a `shortcuts` value is a mouse button.
pub fn is_mouse_shortcut(shortcut: &str) -> bool {
    shortcut.starts_with("mouse")
}

/// #NEW-54: `shortcutSupport(os, session, shortcut, portal)`.
pub fn shortcut_support(os: VoiceOs, session: Option<VoiceSession>, shortcut: &str, portal: bool) -> ShortcutSupport {
    match os {
        VoiceOs::Windows => ShortcutSupport::Global,
        VoiceOs::Macos if is_mouse_shortcut(shortcut) => ShortcutSupport::Unavailable,
        VoiceOs::Macos => ShortcutSupport::Global,
        VoiceOs::Linux if session != Some(VoiceSession::Wayland) => ShortcutSupport::Global,
        VoiceOs::Linux if is_mouse_shortcut(shortcut) => ShortcutSupport::Unavailable,
        VoiceOs::Linux if portal => ShortcutSupport::Portal,
        VoiceOs::Linux => ShortcutSupport::FocusedOnly,
    }
}

/// #NEW-54: the Shortcut picker's values on the voice device.
pub fn shortcut_options(os: VoiceOs, session: Option<VoiceSession>) -> Vec<&'static VoiceShortcut> {
    VOICE
        .shortcuts
        .iter()
        .filter(|entry| shortcut_support(os, session, &entry.value, true) != ShortcutSupport::Unavailable)
        .collect()
}

/// #NEW-54: why the shortcut can't be held (`copy.shortcutReasons`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutReason {
    Taken,
    NoPortal,
    Declined,
    Mouse,
}

impl ShortcutReason {
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "taken" => Some(Self::Taken),
            "noPortal" => Some(Self::NoPortal),
            "declined" => Some(Self::Declined),
            "mouse" => Some(Self::Mouse),
            _ => None,
        }
    }
}

/// #NEW-54: the line under the Shortcut row.
pub fn shortcut_reason(reason: ShortcutReason, shortcut: &str) -> String {
    let reasons = &VOICE.ptt_copy.shortcut_reasons;
    let text = match reason {
        ShortcutReason::Taken => &reasons.taken,
        ShortcutReason::NoPortal => &reasons.no_portal,
        ShortcutReason::Declined => &reasons.declined,
        ShortcutReason::Mouse => &reasons.mouse,
    };
    text.replace("{shortcut}", &shortcut_label(shortcut))
}

/// #NEW-54: what a heard utterance sends: trimmed, None when blank.
pub fn utterance_text(heard: &str) -> Option<&str> {
    Some(heard.trim()).filter(|text| !text.is_empty())
}

/// #NEW-54: where an utterance goes (`utteranceInput`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UtteranceTarget {
    /// Into the member's conversation with the agent.
    Session { session_id: String, text: String },
    /// To the agent, which creates the conversation.
    Agent { team_id: String, agent_id: String, text: String },
}

pub fn utterance_input(heard: &str, team_id: &str, agent_id: Option<&str>, conversation_id: Option<&str>) -> Option<UtteranceTarget> {
    let text = utterance_text(heard)?.to_string();
    let agent_id = agent_id?;
    Some(match conversation_id {
        Some(session_id) => UtteranceTarget::Session { session_id: session_id.to_string(), text },
        None => UtteranceTarget::Agent { team_id: team_id.to_string(), agent_id: agent_id.to_string(), text },
    })
}

/// #NEW-54: a Listen failure (the wire's `ListenFailure`, snake_case).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListenFailure {
    NoMicrophone,
    NotGranted,
    Downloading,
    Unavailable,
    Failed,
}

impl ListenFailure {
    pub fn from_key(key: &str) -> Self {
        match key {
            "no_microphone" => Self::NoMicrophone,
            "not_granted" => Self::NotGranted,
            "downloading" => Self::Downloading,
            "unavailable" => Self::Unavailable,
            _ => Self::Failed,
        }
    }

    fn line(self) -> &'static str {
        let copy = &VOICE.ptt_copy.listen_failed;
        match self {
            Self::NoMicrophone => &copy.no_microphone,
            Self::NotGranted => &copy.not_granted,
            Self::Downloading => &copy.downloading,
            Self::Unavailable => &copy.unavailable,
            Self::Failed => &copy.failed,
        }
    }
}

/// #NEW-54: the push-to-talk phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PttPhase {
    #[default]
    Hidden,
    Listening,
    Heard,
    Thinking,
    Speaking,
    Spoken,
    Reply,
    Muted,
    NotSent,
    Failed(ListenFailure),
}

impl PttPhase {
    /// A phase by its fixture key (a failure reads `failed`).
    pub fn from_key(key: &str) -> Option<PttPhase> {
        Some(match key {
            "hidden" => PttPhase::Hidden,
            "listening" => PttPhase::Listening,
            "heard" => PttPhase::Heard,
            "thinking" => PttPhase::Thinking,
            "speaking" => PttPhase::Speaking,
            "spoken" => PttPhase::Spoken,
            "reply" => PttPhase::Reply,
            "muted" => PttPhase::Muted,
            "notSent" => PttPhase::NotSent,
            "failed" => PttPhase::Failed(ListenFailure::Failed),
            _ => return None,
        })
    }

    pub fn key(self) -> &'static str {
        match self {
            PttPhase::Hidden => "hidden",
            PttPhase::Listening => "listening",
            PttPhase::Heard => "heard",
            PttPhase::Thinking => "thinking",
            PttPhase::Speaking => "speaking",
            PttPhase::Spoken => "spoken",
            PttPhase::Reply => "reply",
            PttPhase::Muted => "muted",
            PttPhase::NotSent => "notSent",
            PttPhase::Failed(_) => "failed",
        }
    }
}

/// #NEW-54: what the overlay shows (`pttStep`'s state).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PttState {
    pub phase: PttPhase,
    pub line: String,
}

/// #NEW-54: a key or worker event (`pttFlowCases` events).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PttEvent {
    KeyDown { muted: bool },
    KeyUp,
    Partial(String),
    Heard(String),
    Empty,
    ListenFailed(ListenFailure),
    Sent,
    SendFailed,
    Replied { body: String, speaking: bool },
    SpeakFinished,
    Cancel,
    Hide,
}

fn quoted(text: &str) -> String {
    format!("\u{201c}{text}\u{201d}")
}

/// #NEW-54: one event folded into the push-to-talk state.
pub fn ptt_step(state: &PttState, event: &PttEvent) -> PttState {
    let at = |phase: PttPhase, line: String| PttState { phase, line };
    let keep = || state.clone();
    match event {
        PttEvent::KeyDown { muted: true } => at(PttPhase::Muted, VOICE.ptt_copy.overlay_muted_line.clone()),
        PttEvent::KeyDown { muted: false } => at(PttPhase::Listening, String::new()),
        PttEvent::KeyUp => keep(),
        PttEvent::Partial(text) if state.phase == PttPhase::Listening => at(PttPhase::Listening, quoted(text.trim())),
        PttEvent::Heard(text) if state.phase == PttPhase::Listening => match utterance_text(text) {
            Some(text) => at(PttPhase::Heard, quoted(text)),
            None => PttState::default(),
        },
        PttEvent::Empty if state.phase == PttPhase::Listening => PttState::default(),
        PttEvent::ListenFailed(reason) if state.phase == PttPhase::Listening => at(PttPhase::Failed(*reason), String::new()),
        PttEvent::Sent if state.phase == PttPhase::Heard => at(PttPhase::Thinking, state.line.clone()),
        PttEvent::SendFailed if state.phase == PttPhase::Heard => at(PttPhase::NotSent, VOICE.ptt_copy.overlay_not_sent_line.clone()),
        PttEvent::Replied { body, speaking } if state.phase == PttPhase::Thinking => {
            let spoken = spoken_text(body, VOICE.max_chars, "en");
            at(if *speaking { PttPhase::Speaking } else { PttPhase::Reply }, first_sentence(&spoken).to_string())
        }
        PttEvent::SpeakFinished if state.phase == PttPhase::Speaking => at(PttPhase::Spoken, state.line.clone()),
        PttEvent::Cancel | PttEvent::Hide => PttState::default(),
        _ => keep(),
    }
}

/// #NEW-54: the capsule for a push-to-talk state (None = hidden).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PttOverlay {
    /// The VoiceWave activity key (`idle`, `listening`, `thinking`, `speaking`, `muted`).
    pub activity: &'static str,
    pub state: String,
    pub line: String,
    pub pill: String,
    pub primary: bool,
    pub hide_after_ms: Option<u64>,
}

pub fn ptt_overlay(state: &PttState, agent: Option<&str>) -> Option<PttOverlay> {
    let copy = &VOICE.copy;
    let ptt = &VOICE.ptt_copy;
    let view = |activity: &'static str, label: &str| PttOverlay {
        activity,
        state: label.to_string(),
        line: state.line.clone(),
        pill: agent.unwrap_or("…").to_string(),
        primary: false,
        hide_after_ms: VOICE.overlay_hide_after_ms.get(state.phase.key()).copied(),
    };
    Some(match state.phase {
        PttPhase::Hidden => return None,
        PttPhase::Listening => view("listening", &ptt.overlay_listening),
        PttPhase::Heard => view("thinking", &ptt.overlay_heard),
        PttPhase::Thinking => view("thinking", &ptt.overlay_thinking),
        PttPhase::Speaking => PttOverlay { pill: copy.stop.clone(), primary: true, ..view("speaking", &copy.speaking) },
        PttPhase::Spoken | PttPhase::Reply => view("idle", &ptt.overlay_replied),
        PttPhase::Muted => PttOverlay { pill: ptt.overlay_unmute.clone(), ..view("muted", &ptt.overlay_muted) },
        PttPhase::NotSent => view("idle", &ptt.overlay_not_sent),
        PttPhase::Failed(reason) => view("idle", reason.line()),
    })
}

/// `fixtures/voice.json` minus its cases.
pub static VOICE: LazyLock<VoiceSpec> =
    LazyLock::new(|| serde_json::from_str(VOICE_JSON).expect("voice.json parses"));

/// The fields of a synced `messages` row [`should_speak`] reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VoiceMessage<'a> {
    pub author_kind: Option<&'a str>,
    pub body: Option<&'a str>,
    pub has_card: bool,
    pub author_session_id: Option<&'a str>,
    /// The session the row belongs to (through its conversation).
    pub session_id: Option<&'a str>,
    pub created_at: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceContext<'a> {
    /// The member's conversation session with the voice agent.
    pub agent_session_id: Option<&'a str>,
    /// THIS device is the member's voice device.
    pub is_voice_device: bool,
    pub speak_replies: bool,
    /// #NEW-73: the language replies are spoken in.
    pub language: &'a str,
    /// The languages THIS device's speech worker reports (empty without a
    /// usable GPU, #NEW-74).
    pub voices: &'a [&'a str],
    /// When the watcher armed; older rows are a backfill.
    pub armed_at: &'a str,
}

/// Whether a synced message row is read aloud.
pub fn should_speak(message: &VoiceMessage<'_>, context: &VoiceContext<'_>) -> bool {
    let Some(agent_session) = context.agent_session_id.filter(|_| context.speak_replies && context.is_voice_device) else {
        return false;
    };
    if !context.voices.contains(&context.language) {
        return false;
    }
    if message.author_kind != Some(crate::contract::MESSAGE_AUTHOR_KIND_AGENT) || message.has_card {
        return false;
    }
    if message.body.map_or(true, |body| body.trim().is_empty()) {
        return false;
    }
    if message.author_session_id.is_some_and(|id| !id.is_empty()) {
        return false;
    }
    if message.session_id != Some(agent_session) {
        return false;
    }
    match (parse_instant(message.created_at.unwrap_or("")), parse_instant(context.armed_at)) {
        (Some(created), Some(armed)) => created >= armed,
        _ => false,
    }
}

/// A timestamp as UTC milliseconds: RFC 3339 (the arm time, the fixture) or
/// the Postgres spelling the shapes deliver (`2026-10-25 10:00:05.123+00`);
/// zone-less means UTC. Same ladder as `ui::inbox::parse_timestamp`.
pub fn parse_instant(text: &str) -> Option<i64> {
    let text = text.trim();
    if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(text) {
        return Some(parsed.timestamp_millis());
    }
    for format in ["%Y-%m-%d %H:%M:%S%.f%#z", "%Y-%m-%dT%H:%M:%S%.f%#z", "%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.f"] {
        if let Ok(parsed) = chrono::DateTime::parse_from_str(text, format) {
            return Some(parsed.timestamp_millis());
        }
        if let Ok(parsed) = chrono::NaiveDateTime::parse_from_str(text, format) {
            return Some(parsed.and_utc().timestamp_millis());
        }
    }
    None
}

const ONES: [&str; 20] = [
    "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve",
    "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen",
];
const TENS: [&str; 10] = ["", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety"];

fn below_hundred(n: u64) -> String {
    if n < 20 {
        return ONES[n as usize].to_string();
    }
    let tens = TENS[(n / 10) as usize];
    if n % 10 == 0 {
        tens.to_string()
    } else {
        format!("{tens}-{}", ONES[(n % 10) as usize])
    }
}

fn below_thousand(n: u64) -> String {
    let hundreds = n / 100;
    let rest = n % 100;
    let mut parts = Vec::new();
    if hundreds > 0 {
        parts.push(format!("{} hundred", ONES[hundreds as usize]));
    }
    if rest > 0 || hundreds == 0 {
        parts.push(below_hundred(rest));
    }
    parts.join(" ")
}

const ONES_DE: [&str; 20] = [
    "null", "eins", "zwei", "drei", "vier", "fünf", "sechs", "sieben", "acht", "neun", "zehn", "elf", "zwölf",
    "dreizehn", "vierzehn", "fünfzehn", "sechzehn", "siebzehn", "achtzehn", "neunzehn",
];
const TENS_DE: [&str; 10] =
    ["", "", "zwanzig", "dreißig", "vierzig", "fünfzig", "sechzig", "siebzig", "achtzig", "neunzig"];

/// A German digit in front of `und`, `hundert` or `tausend` (`ein`, not `eins`).
fn unit_de(n: u64) -> &'static str {
    if n == 1 { "ein" } else { ONES_DE[n as usize] }
}

fn below_hundred_de(n: u64) -> String {
    if n < 20 {
        return ONES_DE[n as usize].to_string();
    }
    let tens = TENS_DE[(n / 10) as usize];
    if n % 10 == 0 {
        tens.to_string()
    } else {
        format!("{}und{tens}", unit_de(n % 10))
    }
}

/// One written compound: `einhunderteins`, `siebenhundertdreiundachtzig`.
fn below_thousand_de(n: u64) -> String {
    let hundreds = n / 100;
    let rest = n % 100;
    let mut words = if hundreds > 0 { format!("{}hundert", unit_de(hundreds)) } else { String::new() };
    if rest > 0 || hundreds == 0 {
        words.push_str(&below_hundred_de(rest));
    }
    words
}

/// A compound ending in `eins` with the ending a following noun wants.
fn with_ending(mut words: String, ending: &str) -> String {
    if words.ends_with("eins") {
        words.truncate(words.len() - 1);
        words.push_str(ending);
    }
    words
}

/// German cardinal words for an integer below a billion.
fn whole_words_de(whole: u64) -> String {
    if whole == 0 {
        return ONES_DE[0].to_string();
    }
    let millions = whole / 1_000_000;
    let thousands = whole % 1_000_000 / 1_000;
    let rest = whole % 1_000;
    let mut parts = Vec::new();
    if millions == 1 {
        parts.push("eine Million".to_string());
    } else if millions > 1 {
        parts.push(format!("{} Millionen", with_ending(below_thousand_de(millions), "e")));
    }
    let mut tail = String::new();
    if thousands > 0 {
        tail.push_str(&with_ending(below_thousand_de(thousands), ""));
        tail.push_str("tausend");
    }
    if rest > 0 {
        tail.push_str(&below_thousand_de(rest));
    }
    if !tail.is_empty() {
        parts.push(tail);
    }
    parts.join(" ")
}

fn digits_words(digits: &str, language: &str) -> String {
    let ones = if language == "de" { &ONES_DE } else { &ONES };
    digits
        .chars()
        .filter_map(|c| c.to_digit(10))
        .map(|d| ones[d as usize])
        .collect::<Vec<_>>()
        .join(" ")
}

/// Cardinal words for a number written with digits, in the language (`en`
/// or `de`; a decimal part reads `spokenWords.point` and then digit by digit).
pub fn number_words(text: &str, language: &str) -> String {
    let (whole, fraction) = match text.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (text, None),
    };
    let mut words = if whole.len() > 9 {
        digits_words(whole, language)
    } else if language == "de" {
        whole_words_de(whole.parse().unwrap_or(0))
    } else {
        let mut n: u64 = whole.parse().unwrap_or(0);
        if n == 0 {
            "zero".to_string()
        } else {
            let mut parts = Vec::new();
            let millions = n / 1_000_000;
            n %= 1_000_000;
            let thousands = n / 1_000;
            n %= 1_000;
            if millions > 0 {
                parts.push(format!("{} million", below_thousand(millions)));
            }
            if thousands > 0 {
                parts.push(format!("{} thousand", below_thousand(thousands)));
            }
            if n > 0 {
                parts.push(below_thousand(n));
            }
            parts.join(" ")
        }
    };
    if let Some(fraction) = fraction.filter(|fraction| !fraction.is_empty()) {
        words.push(' ');
        words.push_str(&spoken_words(language).point);
        words.push(' ');
        words.push_str(&digits_words(fraction, language));
    }
    words
}

/// How people say an issue number in the language (see `voice.json`).
pub fn identifier_number_words(digits: &str, language: &str) -> String {
    if digits.len() <= 3 {
        return number_words(digits, language);
    }
    if digits.len() > 4 {
        return digits_words(digits, language);
    }
    let n: u64 = digits.parse().unwrap_or(0);
    if n % 1000 == 0 {
        return number_words(digits, language);
    }
    let high = n / 100;
    let low = n % 100;
    if language == "de" {
        return if low == 0 {
            format!("{}hundert", below_hundred_de(high))
        } else if low < 10 {
            format!("{} null {}", below_hundred_de(high), ONES_DE[low as usize])
        } else {
            format!("{} {}", below_hundred_de(high), below_hundred_de(low))
        };
    }
    if low == 0 {
        format!("{} hundred", below_hundred(high))
    } else if low < 10 {
        format!("{} oh {}", below_hundred(high), ONES[low as usize])
    } else {
        format!("{} {}", below_hundred(high), below_hundred(low))
    }
}

fn spell(letters: &str) -> String {
    letters
        .to_uppercase()
        .chars()
        .map(String::from)
        .collect::<Vec<_>>()
        .join(" ")
}

fn clip(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let head: String = text.chars().take(max_chars).collect();
    if head.ends_with(['.', '!', '?']) {
        return head;
    }
    let sentence_end = [". ", "! ", "? "].iter().filter_map(|stop| head.rfind(stop)).max();
    if let Some(end) = sentence_end.filter(|end| *end > 0) {
        return head[..=end].to_string();
    }
    let cut = match head.rfind(' ') {
        Some(space) if space > 0 => &head[..space],
        _ => head.as_str(),
    };
    format!("{}{}", cut.trim_end(), VOICE.copy.ellipsis)
}

macro_rules! re {
    ($name:ident, $pattern:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($pattern).expect($pattern));
    };
}

re!(FENCE, r"(?s)```.*?(```|$)");
re!(IMAGE, r"!\[[^\]]*\]\([^)]*\)");
re!(LINK, r"\[([^\]]+)\]\([^)]*\)");
re!(URL, r"https?://[^\s)]+");
re!(HEADING, r"^\s{0,3}#{1,6}\s+");
re!(QUOTE, r"^\s*>\s?");
re!(BULLET, r"^\s*[-*+]\s+");
re!(NUMBERED, r"^\s*\d+[.)]\s+");
re!(INLINE_CODE, r"`([^`]*)`");
re!(STRONG_STAR, r"\*\*(.+?)\*\*");
re!(STRONG_UNDER, r"__(.+?)__");
re!(EM_STAR, r"\*(\S|\S[^\n]*?\S)\*");
re!(EM_UNDER, r"_(\S|\S[^\n]*?\S)_");
re!(STRIKE, r"~~(.+?)~~");
re!(MENTION, r"@([A-Za-z0-9._%+-]+)@[A-Za-z0-9.-]+\.[A-Za-z]{2,}");
re!(ISSUE_REF, r"#([A-Za-z]{2,10}-\d+)");
re!(IDENTIFIER, r"\b([A-Za-z]{2,10})-(\d+)\b");
re!(ACRONYM, r"\b[A-Z]{2,5}\b");
re!(PERCENT, r"(\d+(?:\.\d+)?)\s?%");
re!(NUMBER, r"\b\d+(?:\.\d+)?\b");
re!(SPACE, r"\s+");

/// What a voice says for a reply's Markdown body in the language
/// (`voice.json` rules; an unknown language reads as English).
pub fn spoken_text(body: &str, max_chars: usize, language: &str) -> String {
    let words = spoken_words(language);
    let text = body.replace("\r\n", "\n").replace('\r', "\n");
    let text = FENCE.replace_all(&text, "\n");
    let text: String = text
        .split('\n')
        .filter(|line| !line.trim_start().starts_with('|'))
        .collect::<Vec<_>>()
        .join("\n");
    let text = IMAGE.replace_all(&text, "");
    let text = LINK.replace_all(&text, "$1");
    let text = URL.replace_all(&text, |caps: &Captures<'_>| {
        // A trailing sentence mark belongs to the sentence, not the url.
        let url = &caps[0];
        let trimmed = url.trim_end_matches(['.', ',', ';', ':', '!', '?']);
        let tail = &url[trimmed.len()..];
        format!("{}{tail}", words.link)
    });
    let text: String = text
        .split('\n')
        .map(|line| {
            let line = HEADING.replace(line, "");
            let line = QUOTE.replace(&line, "");
            let line = BULLET.replace(&line, "");
            NUMBERED.replace(&line, "").into_owned()
        })
        .collect::<Vec<_>>()
        .join("\n");
    let text = INLINE_CODE.replace_all(&text, "$1");
    let text = STRONG_STAR.replace_all(&text, "$1");
    let text = STRONG_UNDER.replace_all(&text, "$1");
    let text = EM_STAR.replace_all(&text, "$1");
    let text = EM_UNDER.replace_all(&text, "$1");
    let text = STRIKE.replace_all(&text, "$1");
    let text = MENTION.replace_all(&text, "$1");
    let text = ISSUE_REF.replace_all(&text, "$1");
    let text = IDENTIFIER.replace_all(&text, |caps: &Captures<'_>| {
        format!("{} {}", spell(&caps[1]), identifier_number_words(&caps[2], language))
    });
    let text = ACRONYM.replace_all(&text, |caps: &Captures<'_>| spell(&caps[0]));
    let text = PERCENT.replace_all(&text, |caps: &Captures<'_>| format!("{} {}", number_words(&caps[1], language), words.percent));
    let text = NUMBER.replace_all(&text, |caps: &Captures<'_>| number_words(&caps[0], language));
    let text = SPACE.replace_all(&text, " ");
    clip(text.trim(), max_chars)
}

/// The first sentence of a spoken text (the Dock banner's title).
pub fn first_sentence(spoken: &str) -> &str {
    for (index, ch) in spoken.char_indices() {
        if matches!(ch, '.' | '!' | '?') && spoken[index + 1..].starts_with(' ') {
            return &spoken[..=index];
        }
    }
    spoken
}

/// One agent the Assistant picker offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceAgent {
    pub id: String,
    pub name: String,
    pub team: String,
}

/// One picker option; `id: None` = None.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceOption {
    pub id: Option<String>,
    pub label: String,
}

/// The Assistant picker's options: None first, then the agents by label
/// (`{name} · {team}` when two agents share a name).
pub fn agent_options(agents: &[VoiceAgent]) -> Vec<VoiceOption> {
    let mut options: Vec<VoiceOption> = agents
        .iter()
        .map(|agent| {
            let shared = agents.iter().filter(|other| other.name == agent.name).count() > 1;
            VoiceOption {
                id: Some(agent.id.clone()),
                label: if shared { format!("{} · {}", agent.name, agent.team) } else { agent.name.clone() },
            }
        })
        .collect();
    options.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()).then(a.label.cmp(&b.label)));
    let mut out = vec![VoiceOption { id: None, label: VOICE.copy.agent_none.clone() }];
    out.extend(options);
    out
}

/// One device the Voice device picker may offer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceDevice {
    pub device_id: String,
    pub name: String,
    /// Contract `deviceKind`; only `desktop` runs the voice.
    pub kind: String,
}

/// The Voice device picker's options: None first, then the member's
/// desktops by name.
pub fn voice_device_options(devices: &[VoiceDevice]) -> Vec<VoiceOption> {
    let mut options: Vec<VoiceOption> = devices
        .iter()
        .filter(|device| device.kind == "desktop")
        .map(|device| VoiceOption { id: Some(device.device_id.clone()), label: device.name.clone() })
        .collect();
    options.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()).then(a.label.cmp(&b.label)));
    let mut out = vec![VoiceOption { id: None, label: VOICE.copy.voice_device_none.clone() }];
    out.extend(options);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn fixture() -> Value {
        serde_json::from_str(VOICE_JSON).unwrap()
    }

    #[test]
    fn the_spec_parses() {
        assert_eq!(VOICE.copy.group, "Voice");
        assert_eq!(VOICE.copy.speak_replies, "Speak replies");
        assert_eq!(VOICE.test_ids.page, "settings-voice");
        assert_eq!(VOICE.defaults.ptt_shortcut, "ctrl+space");
        assert!(VOICE.shortcuts.iter().any(|shortcut| shortcut.value == VOICE.defaults.ptt_shortcut));
    }

    #[test]
    fn should_speak_matches_the_fixture() {
        for case in fixture()["shouldSpeakCases"].as_array().unwrap() {
            let m = &case["message"];
            let c = &case["context"];
            let message = VoiceMessage {
                author_kind: m["authorKind"].as_str(),
                body: m["body"].as_str(),
                has_card: !m["card"].is_null(),
                author_session_id: m["authorSessionId"].as_str(),
                session_id: m["sessionId"].as_str(),
                created_at: m["createdAt"].as_str(),
            };
            let voices: Vec<&str> = c["voices"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
            let context = VoiceContext {
                agent_session_id: c["agentSessionId"].as_str(),
                is_voice_device: c["isVoiceDevice"].as_bool().unwrap(),
                speak_replies: c["speakReplies"].as_bool().unwrap(),
                language: c["language"].as_str().unwrap(),
                voices: &voices,
                armed_at: c["armedAt"].as_str().unwrap(),
            };
            assert_eq!(should_speak(&message, &context), case["speak"].as_bool().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn spoken_text_matches_the_fixture() {
        let fixture = fixture();
        let default_max = fixture["maxChars"].as_u64().unwrap() as usize;
        for case in fixture["spokenTextCases"].as_array().unwrap() {
            let max = case["maxChars"].as_u64().map(|n| n as usize).unwrap_or(default_max);
            let language = case["language"].as_str().unwrap_or("en");
            assert_eq!(
                spoken_text(case["body"].as_str().unwrap(), max, language),
                case["spoken"].as_str().unwrap(),
                "{}",
                case["name"]
            );
        }
        assert_eq!(spoken_text("See https://example.com, 50% done.", default_max, "fr"), "See link, fifty percent done.");
    }

    #[test]
    fn number_words_match_the_fixture() {
        let fixture = fixture();
        for case in fixture["numberWordsCases"].as_array().unwrap() {
            let language = case["language"].as_str().unwrap_or("en");
            assert_eq!(number_words(case["n"].as_str().unwrap(), language), case["words"].as_str().unwrap());
        }
        for case in fixture["identifierNumberCases"].as_array().unwrap() {
            let language = case["language"].as_str().unwrap_or("en");
            assert_eq!(identifier_number_words(case["n"].as_str().unwrap(), language), case["words"].as_str().unwrap());
        }
    }

    #[test]
    fn download_progress_matches_the_fixture() {
        for case in fixture()["downloadProgressCases"].as_array().unwrap() {
            assert_eq!(
                download_progress(case["received"].as_u64().unwrap(), case["total"].as_u64().unwrap()),
                case["text"].as_str().unwrap(),
                "{}",
                case["name"]
            );
        }
    }

    #[test]
    fn agent_options_match_the_fixture() {
        for case in fixture()["agentOptionsCases"].as_array().unwrap() {
            let agents: Vec<VoiceAgent> = case["agents"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| VoiceAgent {
                    id: a["id"].as_str().unwrap().into(),
                    name: a["name"].as_str().unwrap().into(),
                    team: a["team"].as_str().unwrap().into(),
                })
                .collect();
            let expected: Vec<VoiceOption> = case["options"]
                .as_array()
                .unwrap()
                .iter()
                .map(|o| VoiceOption { id: o["id"].as_str().map(String::from), label: o["label"].as_str().unwrap().into() })
                .collect();
            assert_eq!(agent_options(&agents), expected, "{}", case["name"]);
        }
    }

    fn options(value: &Value) -> Vec<VoiceOption> {
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|o| VoiceOption { id: o["id"].as_str().map(String::from), label: o["label"].as_str().unwrap().into() })
            .collect()
    }

    #[test]
    fn voice_device_options_match_the_fixture() {
        for case in fixture()["voiceDeviceOptionsCases"].as_array().unwrap() {
            let devices: Vec<VoiceDevice> = case["devices"]
                .as_array()
                .unwrap()
                .iter()
                .map(|d| VoiceDevice {
                    device_id: d["deviceId"].as_str().unwrap().into(),
                    name: d["name"].as_str().unwrap().into(),
                    kind: d["kind"].as_str().unwrap().into(),
                })
                .collect();
            assert_eq!(voice_device_options(&devices), options(&case["options"]), "{}", case["name"]);
        }
    }

    #[test]
    fn clear_on_none_matches_the_fixture() {
        for case in fixture()["clearOnNoneCases"].as_array().unwrap() {
            let mut before = serde_json::to_value(VOICE.defaults.clone()).unwrap();
            for (key, value) in case["before"].as_object().unwrap().iter().chain(case["change"].as_object().unwrap()) {
                before[key] = value.clone();
            }
            let after: VoiceSettings = serde_json::from_value(before).unwrap();
            let after = serde_json::to_value(after.clear_on_none()).unwrap();
            for (key, value) in case["after"].as_object().unwrap() {
                assert_eq!(&after[key], value, "{} {key}", case["name"]);
            }
        }
    }

    #[test]
    fn headers_and_placeholders_match_the_fixture() {
        let fixture = fixture();
        for case in fixture["trayHeaderCases"].as_array().unwrap() {
            assert_eq!(tray_header(case["device"].as_str().unwrap()), case["header"].as_str().unwrap());
        }
        for case in fixture["wakePhrasePlaceholderCases"].as_array().unwrap() {
            assert_eq!(wake_phrase_placeholder(case["agent"].as_str().unwrap()), case["placeholder"].as_str().unwrap());
        }
    }

    #[test]
    fn wake_phrase_errors_match_the_fixture() {
        for case in fixture()["wakePhraseErrorCases"].as_array().unwrap() {
            assert_eq!(wake_phrase_error(case["in"].as_str().unwrap()).as_deref(), case["error"].as_str(), "{case}");
        }
        let settings = VoiceSettings { wake_phrase: "Ok".into(), ..Default::default() };
        assert_eq!(wake_phrase(&settings, Some("Studio")), None, "a short stored phrase is never listened for");
    }

    #[test]
    fn language_note_matches_the_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(VOICE_JSON).unwrap();
        for case in fixture["languageNoteCases"].as_array().unwrap() {
            assert_eq!(
                language_note(case["language"].as_str().unwrap(), case["device"].as_str()),
                case["note"].as_str().map(str::to_string),
                "{}",
                case["name"]
            );
        }
        for case in fixture["languageLabelCases"].as_array().unwrap() {
            assert_eq!(
                language_label(case["language"].as_str().unwrap(), case["device"].as_str()),
                case["label"].as_str().unwrap(),
                "{}",
                case["name"]
            );
        }
        assert_eq!(VOICE.languages.iter().map(|l| l.value.as_str()).collect::<Vec<_>>(), ["en", "de"]);
        assert_eq!(VOICE.defaults.language(), "en");
        for language in &VOICE.languages {
            assert!(VOICE.spoken_words.contains_key(&language.value), "spokenWords.{}", language.value);
        }
    }

    #[test]
    fn parse_instant_reads_both_spellings() {
        let iso = parse_instant("2026-10-25T10:00:05.123Z").unwrap();
        assert_eq!(parse_instant("2026-10-25 10:00:05.123+00"), Some(iso));
        assert_eq!(parse_instant("2026-10-25 10:00:05.123"), Some(iso));
    }

    #[test]
    fn first_sentence_cuts_at_the_first_stop() {
        assert_eq!(first_sentence("Done. The PR is open."), "Done.");
        assert_eq!(first_sentence("No stop here"), "No stop here");
        assert_eq!(first_sentence("Version 1.5 shipped. Next."), "Version 1.5 shipped.");
    }

    #[test]
    fn tray_menu_matches_the_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(VOICE_JSON).unwrap();
        for case in fixture["trayMenuCases"].as_array().unwrap() {
            let settings: VoiceSettings = serde_json::from_value(case["settings"].clone()).unwrap();
            let context = &case["context"];
            let menu = tray_menu(
                &settings,
                &TrayContext {
                    agent: context["agent"].as_str(),
                    muted: context["muted"].as_bool().unwrap(),
                    call_available: context["callAvailable"].as_bool().unwrap(),
                    mic_available: context["micAvailable"].as_bool().unwrap(),
                    device_is_voice_device: context["deviceIsVoiceDevice"].as_bool().unwrap(),
                    shortcut_unavailable: context["shortcutUnavailable"].as_bool().unwrap_or(false),
                    shortcut_label: context["shortcutLabel"].as_str(),
                },
            );
            let expected: Option<Vec<TrayItem>> = serde_json::from_value(case["menu"].clone()).unwrap();
            assert_eq!(menu, expected, "{}", case["name"]);
        }
    }

    #[test]
    fn tray_icon_matches_the_fixture() {
        for case in fixture()["trayIconCases"].as_array().unwrap() {
            let context = &case["context"];
            let icon = tray_icon(
                context["deviceIsVoiceDevice"].as_bool().unwrap(),
                context["wakeListening"].as_bool().unwrap(),
                context["muted"].as_bool().unwrap(),
            );
            let expected: Option<TrayIcon> = serde_json::from_value(case["icon"].clone()).unwrap();
            assert_eq!(icon, expected, "{}", case["name"]);
        }
    }

    #[test]
    fn wake_detail_matches_the_fixture() {
        for case in fixture()["wakeDetailCases"].as_array().unwrap() {
            let context = &case["context"];
            let detail = wake_detail(
                context["here"].as_bool().unwrap(),
                context["wakeWord"].as_bool().unwrap(),
                context["wakeListening"].as_bool().unwrap(),
                context["muted"].as_bool().unwrap(),
            );
            assert_eq!(detail.as_deref(), case["detail"].as_str(), "{}", case["name"]);
        }
    }

    #[test]
    fn the_wake_phrase_is_the_members_or_hey_agent() {
        let mut settings = VoiceSettings { wake_phrase: "  Computer ".into(), ..Default::default() };
        assert_eq!(wake_phrase(&settings, None).as_deref(), Some("Computer"));
        settings.wake_phrase.clear();
        assert_eq!(wake_phrase(&settings, Some("Studio")).as_deref(), Some("Hey Studio"));
        assert_eq!(wake_phrase(&settings, None), None, "never `Hey …`");
    }

    fn os_of(value: &Value) -> (VoiceOs, Option<VoiceSession>) {
        let os = match value["os"].as_str().unwrap() {
            "linux" => VoiceOs::Linux,
            "macos" => VoiceOs::Macos,
            _ => VoiceOs::Windows,
        };
        let session = match value["session"].as_str() {
            Some("x11") => Some(VoiceSession::X11),
            Some("wayland") => Some(VoiceSession::Wayland),
            _ => None,
        };
        (os, session)
    }

    #[test]
    fn shortcut_support_matches_the_fixture() {
        let fixture = fixture();
        for case in fixture["shortcutSupportCases"].as_array().unwrap() {
            let (os, session) = os_of(case);
            let support = shortcut_support(os, session, case["shortcut"].as_str().unwrap(), case["portal"].as_bool().unwrap());
            assert_eq!(support.key(), case["support"].as_str().unwrap(), "{case}");
        }
        for case in fixture["shortcutOptionsCases"].as_array().unwrap() {
            let (os, session) = os_of(case);
            let options: Vec<&str> = shortcut_options(os, session).iter().map(|entry| entry.value.as_str()).collect();
            let expected: Vec<&str> = case["options"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
            assert_eq!(options, expected, "{}", case["name"]);
        }
        for case in fixture["shortcutReasonCases"].as_array().unwrap() {
            let reason = ShortcutReason::from_key(case["reason"].as_str().unwrap()).unwrap();
            assert_eq!(shortcut_reason(reason, case["shortcut"].as_str().unwrap()), case["text"].as_str().unwrap());
        }
    }

    fn ptt_event(value: &Value) -> PttEvent {
        let text = || value["text"].as_str().unwrap().to_string();
        match value["t"].as_str().unwrap() {
            "keyDown" => PttEvent::KeyDown { muted: value["muted"].as_bool().unwrap() },
            "keyUp" => PttEvent::KeyUp,
            "partial" => PttEvent::Partial(text()),
            "heard" => PttEvent::Heard(text()),
            "empty" => PttEvent::Empty,
            "listenFailed" => PttEvent::ListenFailed(ListenFailure::from_key(value["reason"].as_str().unwrap())),
            "sent" => PttEvent::Sent,
            "sendFailed" => PttEvent::SendFailed,
            "replied" => PttEvent::Replied {
                body: value["body"].as_str().unwrap().to_string(),
                speaking: value["speaking"].as_bool().unwrap(),
            },
            "speakFinished" => PttEvent::SpeakFinished,
            "cancel" => PttEvent::Cancel,
            "hide" => PttEvent::Hide,
            other => panic!("unknown event {other}"),
        }
    }

    #[test]
    fn ptt_flow_matches_the_fixture() {
        for case in fixture()["pttFlowCases"].as_array().unwrap() {
            let state = case["events"].as_array().unwrap().iter().fold(PttState::default(), |state, event| ptt_step(&state, &ptt_event(event)));
            let overlay = ptt_overlay(&state, case["agent"].as_str());
            let expected = &case["overlay"];
            match overlay {
                None => assert!(expected.is_null(), "{}: hidden, expected {expected}", case["name"]),
                Some(overlay) => {
                    assert_eq!(overlay.activity, expected["activity"].as_str().unwrap(), "{}", case["name"]);
                    assert_eq!(overlay.state, expected["state"].as_str().unwrap(), "{}", case["name"]);
                    assert_eq!(overlay.line, expected["line"].as_str().unwrap(), "{}", case["name"]);
                    assert_eq!(overlay.pill, expected["pill"].as_str().unwrap(), "{}", case["name"]);
                    assert_eq!(overlay.primary, expected["primary"].as_bool().unwrap(), "{}", case["name"]);
                    assert_eq!(overlay.hide_after_ms, expected["hideAfterMs"].as_u64(), "{}", case["name"]);
                }
            }
        }
    }

    #[test]
    fn utterance_input_matches_the_fixture() {
        for case in fixture()["utteranceCases"].as_array().unwrap() {
            let input = utterance_input(
                case["heard"].as_str().unwrap(),
                case["teamId"].as_str().unwrap(),
                case["agentId"].as_str(),
                case["conversationId"].as_str(),
            );
            let expected = &case["input"];
            let actual = match input {
                None => Value::Null,
                Some(UtteranceTarget::Session { session_id, text }) => serde_json::json!({ "sessionId": session_id, "text": text }),
                Some(UtteranceTarget::Agent { team_id, agent_id, text }) => {
                    serde_json::json!({ "teamId": team_id, "agentId": agent_id, "text": text })
                }
            };
            assert_eq!(&actual, expected, "{}", case["name"]);
        }
    }

    #[test]
    fn the_overlay_specimens_read_the_overlay_copy() {
        let (copy, ptt) = (&VOICE.copy, &VOICE.ptt_copy);
        let states = [&ptt.overlay_listening, &ptt.overlay_heard, &ptt.overlay_thinking, &copy.speaking, &ptt.overlay_muted];
        assert_eq!(VOICE.overlay_specimens.len(), 4);
        for specimen in &VOICE.overlay_specimens {
            assert!(states.contains(&&specimen.state), "{}", specimen.state);
        }
    }
}
