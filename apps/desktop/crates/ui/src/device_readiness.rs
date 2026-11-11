//! EXP-1196/1218/1219 — THE device readiness block, the desktop's renderer of
//! `packages/domain-contract/fixtures/device-doctor.json`.
//!
//! The device builds the block (`coding::device_doctor`); this module turns
//! one into rows and draws them. Two halves:
//!
//! - **The row model** ([`sections`], [`row_for`], [`blocking_row`]) — pure,
//!   fixture-locked below: groups as bands with their tag, one row per item
//!   (state glyph + tone, label, the device-written detail, at most ONE
//!   action pill), the `computer_use` item as the switch row, permission rows
//!   indented and hidden while their parent is off. `local` decides which
//!   actions are offered: on the device itself every one, for ANOTHER device
//!   only the fixture's `remote` ones.
//! - **The view** is ONE eUI document for every host (#EXP-1770,
//!   `settings::readiness`): the group band, flat rows, glass pills, the
//!   toggle row with no description. No subtitles, no footers: label +
//!   state + one action.
//!
//! Hosts: Settings → Tools and the first-run devices step
//! (`settings::doctor_section::DoctorPanel`, THIS device), the device
//! settings dialog (that device, local or remote), and the Agent composer
//! (the ONE failing row under it, [`blocking_row`]).

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui::{App, SharedString, Window};

use coding::device_doctor::{
    self, DeviceDoctor, DoctorAction, DoctorGroup, DoctorItem, DoctorState, KEY_COMPUTER_USE,
    KEY_DESKTOP_AUDIO, KEY_GIT, KEY_MICROPHONE,
};
use coding::CodingAgent;


/// The fixture's `copy.localTitle` — the first-run step's title.
pub(crate) const LOCAL_TITLE: &str = "Set up this device";
/// The fixture's `copy.skip`.
pub(crate) const SKIP: &str = "Skip for now";
/// The fixture's `copy.continue`: the first-run footer's filled pill once
/// [`first_run_ready`].
pub(crate) const CONTINUE: &str = "Continue";
/// The fixture's `copy.recheck`.
pub(crate) const RECHECK: &str = "Check again";
/// The install row's second pill (THIS device only): reveals the CLI path
/// field for a binary off PATH.
pub(crate) const CUSTOM_PATH: &str = "Custom path";
pub(crate) const SAVE_PATH: &str = "Save path";

// ---------------------------------------------------------------------------
// The row model (pure)
// ---------------------------------------------------------------------------

/// The fixture's `states[].tone`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tone {
    Success,
    Warning,
    Muted,
    Destructive,
}

/// The fixture's `states[].glyph`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Glyph {
    Check,
    Alert,
    Dash,
    X,
}

/// The fixture's `states` table.
pub(crate) fn state_glyph(state: DoctorState) -> (Glyph, Tone) {
    match state {
        DoctorState::Ok => (Glyph::Check, Tone::Success),
        DoctorState::Action => (Glyph::Alert, Tone::Warning),
        DoctorState::Missing | DoctorState::Off => (Glyph::Dash, Tone::Muted),
        DoctorState::Error => (Glyph::X, Tone::Destructive),
    }
}

/// A detail reads warning for `action`, destructive for `error`, muted
/// otherwise.
pub(crate) fn detail_tone(state: DoctorState) -> Tone {
    match state {
        DoctorState::Action => Tone::Warning,
        DoctorState::Error => Tone::Destructive,
        _ => Tone::Muted,
    }
}

/// The ONE trailing pill a row may carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RowPill {
    pub action: DoctorAction,
    /// The block's filled pill (at most one per block).
    pub primary: bool,
}

/// One rendered row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReadinessRow {
    pub key: String,
    pub label: String,
    pub state: DoctorState,
    /// `None` on the switch row (label + switch, no glyph).
    pub glyph: Option<(Glyph, Tone)>,
    /// The device-written detail; never on the switch row.
    pub detail: Option<String>,
    pub detail_tone: Tone,
    /// `Some(on)` = THE computer-use switch row.
    pub switch: Option<bool>,
    /// A permission row under its parent.
    pub indent: bool,
    pub pill: Option<RowPill>,
}

/// One group band and its rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReadinessSection {
    pub group: DoctorGroup,
    pub label: &'static str,
    pub tag: Option<&'static str>,
    pub rows: Vec<ReadinessRow>,
}

/// Lenient decode of a synced `devices.doctor` value: an item this build
/// cannot read (a newer device's state or group) is dropped, never the whole
/// block. `None` = no block (null, or nothing readable).
pub(crate) fn parse(value: Option<&serde_json::Value>) -> Option<DeviceDoctor> {
    let value = value?;
    let items: Vec<DoctorItem> = value
        .get("items")?
        .as_array()?
        .iter()
        .filter_map(|item| serde_json::from_value(item.clone()).ok())
        .collect();
    if items.is_empty() {
        return None;
    }
    Some(DeviceDoctor {
        checked_at: value
            .get("checkedAt")
            .and_then(|at| at.as_str())
            .unwrap_or_default()
            .to_string(),
        items,
        // NEW-59: lenient like the items: an unreadable entry is dropped.
        displays: value
            .get("displays")
            .and_then(|displays| displays.as_array())
            .map(|displays| {
                displays
                    .iter()
                    .filter_map(|display| serde_json::from_value(display.clone()).ok())
                    .collect()
            })
            .unwrap_or_default(),
        // NEW-57: lenient too: an unreadable object reads as absent.
        desktop_audio: value.get("desktopAudio").and_then(|audio| serde_json::from_value(audio.clone()).ok()),
    })
}

/// DEV-ONLY `EXP_DEV_DOCTOR_CASE=<case name>` (EXP-1654): THIS device's
/// readiness block (Settings → Tools, the first-run devices step) reads that
/// case of the contract fixture `device-doctor.json` instead of the capture
/// Mac's own doctor run (its git, its agent logins, its disk are no fact of
/// the product). An unknown name = the live block. Unset in every real build.
pub(crate) fn dev_doctor_case() -> Option<DeviceDoctor> {
    static CASE: std::sync::OnceLock<Option<DeviceDoctor>> = std::sync::OnceLock::new();
    CASE.get_or_init(|| {
        let name = std::env::var("EXP_DEV_DOCTOR_CASE").ok()?;
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/device-doctor.json"
        ))
        .ok()?;
        let case = fixture["cases"]
            .as_array()?
            .iter()
            .find(|case| case["name"].as_str() == Some(name.trim()))?;
        parse(Some(&case["doctor"]))
    })
    .clone()
}

/// NEW-78 / NEW-55: the voice rows THIS device adds, read on the main thread.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct VoiceRows {
    /// A bound key or the wake word listens here: the `microphone` row.
    pub listening: bool,
    /// THIS device is the voice device and the worker answered: the
    /// `speech` row (S-3: speak-back is always on).
    pub speech: Option<device_doctor::SpeechState>,
    /// S-3: THIS device is the voice device with the wake word on: the
    /// `wake` row.
    pub wake: Option<WakeRow>,
    /// NEW-57: THIS device is the voice device: `doctor.desktopAudio`.
    pub voice_device: bool,
    /// NEW-57: ... with Desktop audio not off: the `desktop_audio` row.
    pub desktop_audio: bool,
    /// NEW-57: `doctor.desktopAudio.apps` names what plays right now
    /// ([`reports_apps`]); else `[]`.
    pub apps: bool,
}

/// S-3: what the `wake` row reads (voice.json WAKE DETAIL), sampled on the
/// main thread.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct WakeRow {
    /// An agent is picked.
    pub agent: bool,
    /// `push_to_talk::wake_state` (None = not reported yet).
    pub state: Option<etalk::voice::WakeState>,
}

/// S-3: the `wake` row's state and detail: ok while listening, else off;
/// the detail is the device's own `wake_detail` (always online here).
pub(crate) fn wake_row(row: WakeRow) -> (bool, String) {
    let context =
        etalk::voice::WakeDetailContext { wake_word: true, agent: row.agent, online: true, device: "", state: row.state };
    let listening = row.agent && row.state == Some(etalk::voice::WakeState::Listening);
    (listening, etalk::voice::wake_detail(&context).unwrap_or_default())
}

/// meeting.json: the live app names ride the heartbeat only on the voice
/// device, while Desktop audio picks apps (`include`/`exclude`) AND the box
/// is shared with no team (teammates read a shared box's doctor).
pub(crate) fn reports_apps(voice_device: bool, desktop_audio: etalk::meeting::DesktopAudio, shared: bool) -> bool {
    voice_device && desktop_audio.picks_apps() && !shared
}

/// THIS device's own row is shared with a team; unknown (not synced yet,
/// no row) reads as shared, so nothing leaks before the row is known.
fn own_device_shared(device_id: &str, cx: &gpui::App) -> bool {
    let Some(store) = sync::Store::try_global(cx) else { return true };
    let Some(me) = crate::queries::active_account(cx).map(|account| account.user_id) else { return true };
    let devices = store.collections().devices.read(cx);
    devices
        .iter()
        .find(|row| row.device_id.as_deref() == Some(device_id) && row.user_id.as_deref() == Some(me.as_str()))
        .is_none_or(|row| !row.shared_team_ids.is_empty())
}

pub(crate) fn voice_rows(cx: &gpui::App) -> VoiceRows {
    let settings = etalk_gpui::speak_replies::settings(cx);
    let device_id = cx
        .try_global::<crate::session::AuthContext>()
        .map(|auth| steer::persistent_device_id(&auth.data_dir));
    let voice_device = device_id.as_deref().is_some_and(|id| settings.voice_device_id.as_deref() == Some(id));
    let shared = device_id.as_deref().is_none_or(|id| own_device_shared(id, cx));
    let wake_word = settings.clone().clear_on_none().wake_word;
    VoiceRows {
        listening: device_id.as_deref().is_some_and(|id| etalk_gpui::push_to_talk::listens_here(&settings, id)),
        speech: crate::talk_host::speech_state(cx).filter(|_| voice_device),
        wake: (voice_device && settings.wake_word).then(|| WakeRow {
            agent: settings.agent_id.is_some(),
            state: if wake_word { etalk_gpui::push_to_talk::wake_state(cx) } else { None },
        }),
        voice_device,
        desktop_audio: voice_device && settings.desktop_audio != etalk::meeting::DesktopAudio::Off,
        apps: reports_apps(voice_device, settings.desktop_audio, shared),
    }
}

/// Every voice row; background executor only (the microphone probe, the
/// desktop audio listing).
pub(crate) fn add_voice_rows(doctor: &mut DeviceDoctor, rows: VoiceRows) {
    add_microphone_row(doctor, rows.listening);
    if let Some(speech) = rows.speech {
        device_doctor::add_speech(doctor, speech);
    }
    if let Some(wake) = rows.wake {
        let (listening, detail) = wake_row(wake);
        device_doctor::add_wake(doctor, listening, &detail);
    }
    add_desktop_audio(doctor, rows);
}

/// NEW-57 (`meeting.json` DOCTOR): on the voice device the doctor carries
/// `desktopAudio {capability, apps}` (the apps playing now, every beat), and
/// with Desktop audio not off the voice group's `desktop_audio` row. The
/// mapping lives here because coding never depends on talk.
pub(crate) fn add_desktop_audio(doctor: &mut DeviceDoctor, rows: VoiceRows) {
    if !rows.voice_device {
        return;
    }
    let capability = match etalk::apps::capability() {
        etalk::apps::Capability::PerApp => etalk::meeting::DesktopAudioCapability::PerApp,
        etalk::apps::Capability::System => etalk::meeting::DesktopAudioCapability::System,
        etalk::apps::Capability::None => etalk::meeting::DesktopAudioCapability::None,
    };
    let apps = if rows.apps {
        etalk::apps::live()
            .into_iter()
            .map(|app| etalk::meeting::AudioApp { id: app.id, name: app.name })
            .collect()
    } else {
        Vec::new()
    };
    doctor.desktop_audio = Some(device_doctor::DoctorDesktopAudio { capability, apps });
    if !rows.desktop_audio {
        return;
    }
    let state = match capability {
        _ if etalk::apps::permission() == Some(false) => device_doctor::DesktopAudioState::NotGranted,
        etalk::meeting::DesktopAudioCapability::PerApp => device_doctor::DesktopAudioState::PerApp,
        etalk::meeting::DesktopAudioCapability::System => device_doctor::DesktopAudioState::System,
        etalk::meeting::DesktopAudioCapability::None => device_doctor::DesktopAudioState::None,
    };
    device_doctor::add_desktop_audio(doctor, state);
}

/// NEW-78: the IDE's `microphone` row, appended only while THIS device
/// listens (`push_to_talk::listens_here`); the CLI daemon never adds it.
/// Probes the OS (`etalk::microphone`, a `pactl` run on Linux): background
/// executor only. The mapping lives here because coding never depends on talk.
pub(crate) fn add_microphone_row(doctor: &mut DeviceDoctor, listening: bool) {
    if !listening {
        return;
    }
    let state = match etalk::microphone::probe() {
        etalk::microphone::Microphone::Granted => device_doctor::MicrophoneState::Granted,
        etalk::microphone::Microphone::NotGranted => device_doctor::MicrophoneState::NotGranted,
        etalk::microphone::Microphone::NoDevice => device_doctor::MicrophoneState::NoDevice,
        etalk::microphone::Microphone::NeedsRuntime => device_doctor::MicrophoneState::NeedsRuntime,
    };
    device_doctor::add_microphone(doctor, state);
}

/// The first-run footer rule: `continue` (filled) instead of `skip` once Git
/// is ok and at least one agent is runnable.
pub(crate) fn first_run_ready(doctor: Option<&DeviceDoctor>) -> bool {
    doctor.is_some_and(|doctor| !device_doctor::runnable_agents(doctor).is_empty())
}

/// The bare Computer use switch row for a device with no doctor (an older
/// build): no block, but the switch stays reachable (the fixture's last
/// rule). Label + switch, nothing else.
pub(crate) fn bare_switch_row(on: bool) -> ReadinessRow {
    ReadinessRow {
        key: KEY_COMPUTER_USE.to_string(),
        label: device_doctor::label(KEY_COMPUTER_USE).to_string(),
        state: if on { DoctorState::Ok } else { DoctorState::Off },
        glyph: None,
        detail: None,
        detail_tone: Tone::Muted,
        switch: Some(on),
        indent: false,
        pill: None,
    }
}

/// Whether `action` is offered here (the fixture's `remote` rule).
pub(crate) fn offered(action: DoctorAction, local: bool) -> bool {
    local || action.remote()
}

/// The pill a row carries in this mode, if any.
fn row_action(item: &DoctorItem, local: bool) -> Option<DoctorAction> {
    item.action.filter(|action| offered(*action, local))
}

/// The row whose pill is the block's PRIMARY one: the first `action`/`error`
/// row (`device_doctor::first_action`, the fixture's `localPrimary`) that
/// carries a pill here — the switch row never does, and for another device
/// a non-remote action renders none, so the filled pill passes down.
pub(crate) fn primary_key(doctor: &DeviceDoctor, local: bool) -> Option<&str> {
    doctor
        .items
        .iter()
        .find(|item| {
            matches!(item.state, DoctorState::Action | DoctorState::Error)
                && row_action(item, local).is_some()
        })
        .map(|item| item.key.as_str())
}

fn build_row(
    item: &DoctorItem,
    local: bool,
    primary: Option<&str>,
    switch_on: Option<bool>,
) -> ReadinessRow {
    let is_switch = item.key == KEY_COMPUTER_USE;
    let pill = row_action(item, local).map(|action| RowPill {
        action,
        primary: primary == Some(item.key.as_str()),
    });
    ReadinessRow {
        key: item.key.clone(),
        label: device_doctor::item_label(item).to_string(),
        state: item.state,
        glyph: (!is_switch).then(|| state_glyph(item.state)),
        detail: if is_switch { None } else { item.detail.clone() },
        detail_tone: detail_tone(item.state),
        switch: is_switch.then(|| switch_on.unwrap_or(item.state != DoctorState::Off)),
        indent: item.parent.is_some(),
        pill: if is_switch { None } else { pill },
    }
}

/// THE block as bands of rows. `switch_on` overrides the computer-use
/// switch with a value the host already wrote but the device has not echoed
/// yet (`None` = the block's own state); children hide while it is off.
pub(crate) fn sections(
    doctor: &DeviceDoctor,
    local: bool,
    switch_on: Option<bool>,
) -> Vec<ReadinessSection> {
    let primary = primary_key(doctor, local);
    let parent_on = |parent: &str| {
        let switch = (parent == KEY_COMPUTER_USE).then_some(switch_on).flatten();
        switch.unwrap_or_else(|| {
            doctor
                .items
                .iter()
                .find(|item| item.key == parent)
                .is_some_and(|item| item.state != DoctorState::Off)
        })
    };
    DoctorGroup::ALL
        .into_iter()
        .filter_map(|group| {
            let rows: Vec<ReadinessRow> = doctor
                .items
                .iter()
                .filter(|item| item.group == group)
                .filter(|item| item.parent.as_deref().is_none_or(parent_on))
                .map(|item| build_row(item, local, primary, switch_on))
                .collect();
            (!rows.is_empty()).then(|| ReadinessSection {
                group,
                label: group.label(),
                tag: group.tag(),
                rows,
            })
        })
        .collect()
}

/// ONE row alone (the composer's single-row mode): its pill, if offered,
/// is the primary one — it is the only action on screen.
pub(crate) fn row_for(doctor: &DeviceDoctor, key: &str, local: bool) -> Option<ReadinessRow> {
    let item = doctor.items.iter().find(|item| item.key == key)?;
    let mut row = build_row(item, local, Some(key), None);
    row.indent = false;
    Some(row)
}

/// The row that keeps `agent` from running on this device: Git when Git is
/// the failure, else that agent's row while it is not `ok`. `None` = the
/// block has nothing against it.
pub(crate) fn blocking_row(
    doctor: &DeviceDoctor,
    agent: CodingAgent,
    local: bool,
) -> Option<ReadinessRow> {
    let failing = |key: &str| {
        doctor
            .items
            .iter()
            .any(|item| item.key == key && item.state != DoctorState::Ok)
    };
    if failing(KEY_GIT) {
        return row_for(doctor, KEY_GIT, local);
    }
    if failing(agent.id()) {
        return row_for(doctor, agent.id(), local);
    }
    None
}

/// EXP-1375: the row the composer shows when nothing blocks the start but
/// the device is low on disk (its `disk` row is `action`): a warning with no
/// pill; the start stays enabled.
pub(crate) fn warning_row(doctor: &DeviceDoctor, local: bool) -> Option<ReadinessRow> {
    device_doctor::low_disk(doctor)
        .then(|| row_for(doctor, device_doctor::KEY_DISK, local))
        .flatten()
}

/// A row as one tooltip line: `Claude Code · 2.1.222 · needs 2.1.263`.
pub(crate) fn summary(row: &ReadinessRow) -> String {
    match &row.detail {
        Some(detail) if !detail.is_empty() => format!("{} · {detail}", row.label),
        _ => row.label.clone(),
    }
}

/// Why no agent can run, as a row: the first agent row that has a fix in
/// reach (`action`), else `None` (every agent simply missing — the hosts'
/// install copy says it better).
pub(crate) fn no_agent_row(doctor: &DeviceDoctor, local: bool) -> Option<ReadinessRow> {
    if !device_doctor::runnable_agents(doctor).is_empty() {
        return None;
    }
    if let Some(git) = blocking_git(doctor, local) {
        return Some(git);
    }
    let item = doctor
        .items
        .iter()
        .find(|item| item.group == DoctorGroup::Agents && item.state == DoctorState::Action)?;
    row_for(doctor, &item.key, local)
}

fn blocking_git(doctor: &DeviceDoctor, local: bool) -> Option<ReadinessRow> {
    doctor
        .items
        .iter()
        .any(|item| item.key == KEY_GIT && item.state != DoctorState::Ok)
        .then(|| row_for(doctor, KEY_GIT, local))
        .flatten()
}

/// The agent a row's key names (`claude`/`codex`).
pub(crate) fn agent_for(key: &str) -> Option<CodingAgent> {
    CodingAgent::ALL.into_iter().find(|agent| agent.id() == key)
}

// ---------------------------------------------------------------------------
// The view
// ---------------------------------------------------------------------------

/// A pill click: the row's key and its action.
pub(crate) type ActionHandler = Rc<dyn Fn(&str, DoctorAction, &mut Window, &mut App)>;
/// The computer-use switch.
pub(crate) type ToggleHandler = Rc<dyn Fn(bool, &mut Window, &mut App)>;
/// #EXP-1771: one of the Computer use band's picker rows
/// (`settings::readiness::TAIL_SLOTS`): the picked value and the choices
/// (label, value) its Select offers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TailPicker {
    pub value: String,
    pub options: Vec<(String, String)>,
}
/// A pick in one of them: its slot (`TAIL_SLOTS` index) and the value.
pub(crate) type PickHandler = Rc<dyn Fn(usize, String, &mut Window, &mut App)>;

/// The host's wiring. No handler = inert (the styleguide).
#[derive(Default)]
pub(crate) struct BlockProps {
    pub on_action: Option<ActionHandler>,
    pub on_toggle: Option<ToggleHandler>,
    /// Row keys whose action is running (the pill shows its spinner).
    pub busy: HashSet<String>,
    /// A line under a row, by its key (a finished command's outcome).
    pub notes: HashMap<String, String>,
    /// EXP-1236: picker rows appended to the Computer use band, under the
    /// switch and its permission rows (the Computer use model picker, NEW-59
    /// the Work screen picker under it). The host hands them over only while
    /// the switch is on; with no Computer use band to hang them on (an older
    /// device's doctor) they are dropped. The bare switch row (no doctor)
    /// carries them too.
    pub computer_use_tail: Vec<TailPicker>,
    /// A pick in one of [`Self::computer_use_tail`].
    pub on_pick: Option<PickHandler>,
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

/// Where an install row sends the person (the CLI install docs, never a
/// product page: a fresh machine needs the CLI).
pub(crate) fn install_url(key: &str) -> Option<&'static str> {
    match key {
        KEY_GIT => Some("https://git-scm.com/downloads"),
        "claude" => Some("https://code.claude.com/docs/en/quickstart#step-1-install-claude-code"),
        // NEW-54: the Visual C++ runtime the Windows CPU transcriber needs.
        KEY_MICROPHONE => Some(etalk::microphone::VC_RUNTIME_URL),
        // EXP-1232: codex is a managed download — its row never offers Install.
        _ => None,
    }
}

/// The OS privacy pane a permission row's grant opens (macOS only; the
/// Wayland portal asks through its own dialog).
pub(crate) fn privacy_pane_url(key: &str) -> Option<&'static str> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    match key {
        "screen_recording" => {
            Some("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
        }
        "accessibility" => {
            Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        }
        KEY_MICROPHONE => {
            Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
        }
        // NEW-57: process taps' Audio Recording permission.
        KEY_DESKTOP_AUDIO => {
            Some("x-apple.systempreferences:com.apple.preference.security?Privacy_AudioCapture")
        }
        _ => None,
    }
}

/// Which local agent updates are running right now (so every host's Update
/// pill spins for the same run).
#[derive(Default)]
struct LocalUpdates(HashSet<String>);

impl gpui::Global for LocalUpdates {}

/// The row keys whose LOCAL action is running.
pub(crate) fn local_busy(cx: &App) -> HashSet<String> {
    cx.try_global::<LocalUpdates>()
        .map(|updates| updates.0.clone())
        .unwrap_or_default()
}

fn open_external(url: &'static str, cx: &mut App) {
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = api::opener::open_in_browser(url) {
                log::warn!("[ui] device readiness: opening {url} failed: {err}");
            }
        })
        .detach();
}

/// A pill on THIS device.
pub(crate) fn run_local_action(key: &str, action: DoctorAction, _window: &mut Window, cx: &mut App) {
    match action {
        // EXP-1513: a missing Claude Code installs in place with Anthropic's
        // own installer (`agent_update` installs it), never a docs hop.
        DoctorAction::Install if agent_for(key) == Some(CodingAgent::Claude) => {
            run_local_agent_action(CodingAgent::Claude, "install", cx);
        }
        DoctorAction::Install => {
            if let Some(url) = install_url(key) {
                crate::settings::open_url(cx, url.to_string());
            }
        }
        DoctorAction::Update => {
            if let Some(agent) = agent_for(key) {
                run_local_update(agent, cx);
            }
        }
        DoctorAction::SignIn => {
            if let Some(agent) = agent_for(key) {
                crate::agent_login::open_login_tab(agent, None, cx);
            }
        }
        DoctorAction::Grant if key == KEY_MICROPHONE => grant_microphone(cx),
        DoctorAction::Grant if key == KEY_DESKTOP_AUDIO => grant_desktop_audio(cx),
        DoctorAction::Grant => {
            if let Some(url) = privacy_pane_url(key) {
                open_external(url, cx);
            }
            coding::computer::prepare_in_background();
        }
    }
}

/// NEW-78: the microphone row's Grant on macOS: the OS prompt while it has
/// not asked yet (the doctor re-runs on the answer), else System Settings →
/// Privacy → Microphone.
fn grant_microphone(cx: &mut App) {
    let (answered, answer) = std::sync::mpsc::channel::<bool>();
    let asked = etalk::microphone::request_access(move |granted| {
        let _ = answered.send(granted);
    });
    if !asked {
        if let Some(url) = privacy_pane_url(KEY_MICROPHONE) {
            open_external(url, cx);
        }
        return;
    }
    cx.spawn(async move |cx| {
        let answer = cx.background_executor().spawn(async move { answer.recv() }).await;
        if answer.is_ok() {
            let _ = cx.update(|cx| {
                if let Some(hub) = crate::coding_flow::CodingHub::global_ref(cx) {
                    crate::coding_flow::CodingHub::refresh_doctor(&hub, cx);
                }
            });
        }
    })
    .detach();
}

/// NEW-57: the desktop audio row's Grant (macOS): ask for the Audio
/// Recording permission process taps need, then a fresh doctor run;
/// still not granted = System Settings → Privacy → Audio Recording.
fn grant_desktop_audio(cx: &mut App) {
    cx.spawn(async move |cx| {
        let granted = cx.background_executor().spawn(async move { etalk::apps::request_permission() }).await;
        let _ = cx.update(|cx| {
            if granted == Some(false) {
                if let Some(url) = privacy_pane_url(KEY_DESKTOP_AUDIO) {
                    open_external(url, cx);
                }
            }
            if let Some(hub) = crate::coding_flow::CodingHub::global_ref(cx) {
                crate::coding_flow::CodingHub::refresh_doctor(&hub, cx);
            }
        });
    })
    .detach();
}

/// `claude update` / `codex update` right here (`coding::update_agent`, the
/// `agent_update` command's runner), then a fresh doctor run.
pub(crate) fn run_local_update(agent: CodingAgent, cx: &mut App) {
    run_local_agent_action(agent, "update", cx);
}

/// [`run_local_update`] under `verb` (the failure toast's word): the same
/// `coding::update_agent`, which installs a missing Claude Code (EXP-1513).
fn run_local_agent_action(agent: CodingAgent, verb: &'static str, cx: &mut App) {
    let key = agent.id().to_string();
    if !cx.default_global::<LocalUpdates>().0.insert(key.clone()) {
        return; // already running
    }
    let hub = crate::coding_flow::CodingHub::global(cx);
    let settings = hub.read(cx).settings.clone();
    let data_dir = crate::coding_flow::coding_data_dir(cx);
    hub.update(cx, |_, cx| cx.notify());
    cx.spawn(async move |cx| {
        let result = cx
            .background_executor()
            .spawn(async move { coding::update_agent(&settings, &data_dir, agent) })
            .await;
        let _ = cx.update(|cx| {
            cx.default_global::<LocalUpdates>().0.remove(&key);
            let toast = match result {
                Ok(outcome) => crate::toast::Toast::success(outcome.message()),
                Err(message) => crate::action_error::failed_toast(verb, agent.label(), Some(message)),
            };
            crate::toast::show_in_active_window(toast, cx);
            crate::coding_flow::CodingHub::refresh_doctor(&hub, cx);
        });
    })
    .detach();
}

/// A pill for ANOTHER device (only `remote` actions get here): Update queues
/// the `agent_update` device command, Sign in runs the remote login flow.
pub(crate) fn run_remote_action(
    device_id: String,
    device_label: SharedString,
    key: &str,
    action: DoctorAction,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(agent) = agent_for(key) else {
        return;
    };
    match action {
        DoctorAction::Update => {
            let Some(trpc) = crate::queries::trpc_client(cx) else {
                return;
            };
            cx.spawn(async move |cx| {
                let result = cx
                    .background_executor()
                    .spawn(async move {
                        api::devices::create_agent_update_command(&trpc, &device_id, agent.id())
                    })
                    .await;
                let _ = cx.update(|cx| {
                    let toast = match result {
                        Ok(_) => crate::toast::Toast::info(format!(
                            "Updating {} on {device_label}…",
                            agent.label()
                        )),
                        Err(err) => crate::action_error::failed_toast(
                            "update",
                            agent.label(),
                            crate::action_error::action_error_description(&err),
                        ),
                    };
                    crate::toast::show_in_active_window(toast, cx);
                });
            })
            .detach();
        }
        DoctorAction::SignIn => crate::agent_login::sign_in_on_device(
            device_id,
            device_label,
            false,
            agent,
            None,
            window,
            cx,
        ),
        DoctorAction::Install | DoctorAction::Grant => {}
    }
}

/// THIS device's computer-use switch: `Settings.computer_use` through the
/// hub's save (which pushes the launch defaults and re-runs the doctor; the
/// computer indicator asks the OS permissions when it reads on).
pub(crate) fn set_local_computer_use(on: bool, cx: &mut App) {
    let hub = crate::coding_flow::CodingHub::global(cx);
    let mut settings = hub.read(cx).settings.clone();
    if settings.computer_use == on {
        return;
    }
    settings.computer_use = on;
    if let Err(err) = crate::coding_flow::CodingHub::save_settings(&hub, settings, cx) {
        log::warn!("[ui] device readiness: saving computer use failed: {err}");
    }
}

/// The local block's standard wiring (every action + the switch).
pub(crate) fn local_props(cx: &App) -> BlockProps {
    BlockProps {
        on_action: Some(Rc::new(run_local_action)),
        on_toggle: Some(Rc::new(|on, _window, cx| set_local_computer_use(on, cx))),
        busy: local_busy(cx),
        ..BlockProps::default()
    }
}

/// A remote block's wiring for `device_id` (remote actions; the switch is the
/// host's, it owns the launch-defaults write).
pub(crate) fn remote_props(device_id: String, device_label: SharedString) -> BlockProps {
    BlockProps {
        on_action: Some(Rc::new(move |key, action, window, cx| {
            run_remote_action(device_id.clone(), device_label.clone(), key, action, window, cx)
        })),
        ..BlockProps::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/device-doctor.json");

    fn cases() -> Vec<Value> {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        fixture["cases"].as_array().unwrap().clone()
    }

    fn doctor(case: &Value) -> DeviceDoctor {
        parse(Some(&case["doctor"])).expect("every fixture block parses")
    }

    fn all_rows(sections: &[ReadinessSection]) -> Vec<&ReadinessRow> {
        sections.iter().flat_map(|section| section.rows.iter()).collect()
    }

    /// The row model per fixture case, local AND remote: bands + tags, the
    /// switch row, glyph tones, detail tones, the pill rules.
    #[test]
    fn every_fixture_case_models_exactly() {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        let tones: HashMap<String, String> = fixture["states"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, spec)| (key.clone(), spec["tone"].as_str().unwrap().to_string()))
            .collect();
        let glyphs: HashMap<String, String> = fixture["states"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, spec)| (key.clone(), spec["glyph"].as_str().unwrap().to_string()))
            .collect();
        for case in cases() {
            let name = case["name"].as_str().unwrap();
            let doctor = doctor(&case);
            let items = case["doctor"]["items"].as_array().unwrap();
            for local in [true, false] {
                let sections = sections(&doctor, local, None);
                // Bands: the fixture's groups, in order, with their tags.
                for section in &sections {
                    let spec = fixture["groups"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|group| {
                            serde_json::from_value::<DoctorGroup>(group["key"].clone()).unwrap()
                                == section.group
                        })
                        .unwrap();
                    assert_eq!(section.label, spec["label"].as_str().unwrap(), "{name}");
                    assert_eq!(section.tag, spec["tag"].as_str(), "{name}");
                }
                let rows = all_rows(&sections);
                // Every item is a row (no parent is off in the fixture).
                assert_eq!(rows.len(), items.len(), "{name}");
                let mut primaries = 0;
                for (row, item) in rows.iter().zip(items) {
                    let state = item["state"].as_str().unwrap();
                    assert_eq!(row.key, item["key"].as_str().unwrap(), "{name}");
                    // EXP-1375: a kept worktree row carries its own label.
                    let label = item["label"]
                        .as_str()
                        .or_else(|| fixture["labels"][row.key.as_str()].as_str())
                        .unwrap();
                    assert_eq!(row.label, label, "{name}");
                    assert_eq!(row.indent, item.get("parent").is_some(), "{name}: {}", row.key);
                    if row.key == KEY_COMPUTER_USE {
                        // The switch row: no glyph, no detail, no pill.
                        assert_eq!(row.switch, Some(state != "off"), "{name}");
                        assert!(row.glyph.is_none() && row.detail.is_none() && row.pill.is_none());
                        continue;
                    }
                    assert_eq!(row.switch, None);
                    let (glyph, _) = row.glyph.unwrap();
                    let glyph = match glyph {
                        Glyph::Check => "check",
                        Glyph::Alert => "alert",
                        Glyph::Dash => "dash",
                        Glyph::X => "x",
                    };
                    let tone = |tone: Tone| match tone {
                        Tone::Success => "success",
                        Tone::Warning => "warning",
                        Tone::Muted => "muted",
                        Tone::Destructive => "destructive",
                    };
                    assert_eq!(glyph, glyphs[state], "{name}: {}", row.key);
                    assert_eq!(tone(row.glyph.unwrap().1), tones[state], "{name}: {}", row.key);
                    let expected_detail_tone = match state {
                        "action" => "warning",
                        "error" => "destructive",
                        _ => "muted",
                    };
                    assert_eq!(tone(row.detail_tone), expected_detail_tone, "{name}");
                    assert_eq!(row.detail.as_deref(), item["detail"].as_str(), "{name}");
                    // Pills: every action locally; remote = the fixture's
                    // `remotePills` exactly.
                    let expected: Option<DoctorAction> = if local {
                        item.get("action")
                            .map(|action| serde_json::from_value(action.clone()).unwrap())
                    } else {
                        case["remotePills"]
                            .get(row.key.as_str())
                            .map(|action| serde_json::from_value(action.clone()).unwrap())
                    };
                    assert_eq!(row.pill.map(|pill| pill.action), expected, "{name}: {}", row.key);
                    if row.pill.is_some_and(|pill| pill.primary) {
                        primaries += 1;
                        if local {
                            assert_eq!(
                                Some(row.key.as_str()),
                                case["localPrimary"].as_str(),
                                "{name}"
                            );
                        }
                    }
                }
                assert!(primaries <= 1, "{name}: one primary pill at most");
                if local {
                    assert_eq!(primaries, case["localPrimary"].as_str().map_or(0, |_| 1), "{name}");
                }
            }
        }
    }

    /// meeting.json: live app names reach the server only from the voice
    /// device, while Desktop audio picks apps, on a box shared with no team.
    #[test]
    fn app_names_ride_the_heartbeat_only_unshared_and_picking() {
        use etalk::meeting::DesktopAudio;
        assert!(reports_apps(true, DesktopAudio::Include, false));
        assert!(reports_apps(true, DesktopAudio::Exclude, false));
        assert!(!reports_apps(true, DesktopAudio::Include, true), "a shared box never names its apps");
        assert!(!reports_apps(true, DesktopAudio::All, false), "Everything needs no names");
        assert!(!reports_apps(true, DesktopAudio::Off, false));
        assert!(!reports_apps(false, DesktopAudio::Include, false), "not the voice device");
    }

    /// S-3: the doctor's `wake` row: ok while listening, the device's own
    /// wake detail as the text every other client shows.
    #[test]
    fn the_wake_row_carries_the_devices_wake_detail() {
        use etalk::voice::WakeState;
        assert_eq!(wake_row(WakeRow { agent: true, state: Some(WakeState::Listening) }), (true, "Listening".into()));
        assert_eq!(wake_row(WakeRow { agent: true, state: Some(WakeState::Muted) }), (false, "Microphone muted".into()));
        assert_eq!(wake_row(WakeRow { agent: true, state: None }), (false, "Starting…".into()));
        assert_eq!(wake_row(WakeRow { agent: false, state: None }), (false, "Pick an agent".into()));
        let mut doctor = DeviceDoctor { checked_at: String::new(), items: Vec::new(), displays: Vec::new(), desktop_audio: None };
        let rows = VoiceRows { wake: Some(WakeRow { agent: true, state: Some(WakeState::MicBusy) }), ..Default::default() };
        add_voice_rows(&mut doctor, rows);
        let wake = doctor.items.iter().find(|item| item.key == "wake").unwrap();
        assert_eq!((wake.state, wake.detail.as_deref()), (DoctorState::Off, Some("A call is using the microphone")));
    }

    #[test]
    fn children_hide_while_the_switch_is_off() {
        let case = &cases()[1];
        let doctor = doctor(case);
        let rows = |switch| {
            sections(&doctor, true, switch)
                .into_iter()
                .find(|section| section.group == DoctorGroup::ComputerUse)
                .unwrap()
                .rows
        };
        assert_eq!(rows(None).len(), 3);
        let off = rows(Some(false));
        assert_eq!(off.len(), 1);
        assert_eq!(off[0].switch, Some(false));
    }

    /// The composer's single row: Git when Git fails, else the picked
    /// agent's row; its pill is primary (the only action there), and a
    /// non-remote action shows none for another device.
    #[test]
    fn the_composer_row_is_the_failing_one() {
        let cases = cases();
        // Case 1: everything claude needs is fine.
        assert!(blocking_row(&doctor(&cases[0]), CodingAgent::Claude, true).is_none());
        // EXP-1232: the managed codex with no login — Sign in, remote too.
        let codex = blocking_row(&doctor(&cases[0]), CodingAgent::Codex, true).unwrap();
        assert_eq!(summary(&codex), "Codex · Signed out");
        assert_eq!(
            blocking_row(&doctor(&cases[0]), CodingAgent::Codex, false)
                .unwrap()
                .pill
                .map(|pill| pill.action),
            Some(DoctorAction::SignIn)
        );
        // Case 2: claude too old — Update, remote too.
        let claude = blocking_row(&doctor(&cases[1]), CodingAgent::Claude, false).unwrap();
        assert_eq!(summary(&claude), "Claude Code · 2.1.222 · needs 2.1.263");
        assert_eq!(
            claude.pill,
            Some(RowPill { action: DoctorAction::Update, primary: true })
        );
        // Case 3: Git is the failure, whatever the agent.
        let git = blocking_row(&doctor(&cases[2]), CodingAgent::Codex, true).unwrap();
        assert_eq!(git.key, KEY_GIT);
        assert_eq!(git.pill.map(|pill| pill.action), Some(DoctorAction::Install));
        assert!(no_agent_row(&doctor(&cases[0]), true).is_none());
        assert_eq!(no_agent_row(&doctor(&cases[1]), true).unwrap().key, "claude");
        assert_eq!(no_agent_row(&doctor(&cases[2]), true).unwrap().key, KEY_GIT);
    }

    /// EXP-1375: the low-disk row is the composer's warning, never a blocker.
    #[test]
    fn the_low_disk_row_is_a_warning_only() {
        for case in cases() {
            let name = case["name"].as_str().unwrap();
            let doctor = doctor(&case);
            let warning = warning_row(&doctor, false);
            assert_eq!(
                warning.as_ref().map(|row| row.key.as_str()),
                case["composerWarning"].as_str(),
                "{name}"
            );
            if let Some(row) = warning {
                assert!(row.pill.is_none() && !row.indent, "{name}");
                assert!(blocking_row(&doctor, CodingAgent::Claude, false).is_none(), "{name}");
            }
        }
    }

    #[test]
    fn parse_is_lenient() {
        assert!(parse(None).is_none());
        assert!(parse(Some(&Value::Null)).is_none());
        let value = serde_json::json!({
            "checkedAt": "2026-10-05T19:00:00.000Z",
            "items": [
                { "key": "git", "group": "required", "state": "ok", "detail": "2.55.0" },
                { "key": "future", "group": "someday", "state": "quantum" }
            ]
        });
        let doctor = parse(Some(&value)).unwrap();
        assert_eq!(doctor.items.len(), 1);
    }

    /// The first-run footer: Continue exactly when the case has a runnable
    /// agent (git ok + an ok agent row); no block yet = Skip.
    #[test]
    fn the_first_run_footer_continues_once_an_agent_is_runnable() {
        for case in cases() {
            let runnable = !case["runnable"].as_array().unwrap().is_empty();
            assert_eq!(first_run_ready(Some(&doctor(&case))), runnable, "{}", case["name"]);
        }
        assert!(!first_run_ready(None));
    }

    #[test]
    fn a_device_with_no_doctor_keeps_the_bare_switch_row() {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        let row = bare_switch_row(true);
        assert_eq!(row.label, fixture["labels"]["computer_use"].as_str().unwrap());
        assert_eq!(row.switch, Some(true));
        assert!(row.glyph.is_none() && row.detail.is_none() && row.pill.is_none());
        assert_eq!(bare_switch_row(false).switch, Some(false));
    }

    #[test]
    fn copy_matches_the_fixture() {
        let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
        assert_eq!(fixture["copy"]["localTitle"], LOCAL_TITLE);
        assert_eq!(fixture["copy"]["skip"], SKIP);
        assert_eq!(fixture["copy"]["recheck"], RECHECK);
        assert_eq!(fixture["copy"]["continue"], CONTINUE);
        assert!(fixture["actions"].get("import").is_none(), "EXP-1532: no import");
        for key in ["git", "claude"] {
            assert!(install_url(key).unwrap().starts_with("https://"));
        }
        // EXP-1232: codex is a managed download — nothing to install by hand.
        assert_eq!(install_url("codex"), None);
    }
}
