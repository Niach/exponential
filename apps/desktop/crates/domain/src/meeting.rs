//! NEW-57: meeting mode on the member's voice device, the desktop mirror of
//! `packages/domain-contract/src/meeting.ts`, byte-locked by
//! `fixtures/meeting.json` (its `$comment` is the spec; every case list
//! replays below).

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

const MEETING_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/meeting.json");

/// How many days the server keeps a meeting transcript batch.
pub const RETENTION_DAYS: i64 = crate::contract::MEETING_RETENTION_DAYS;
/// How many days the voice device keeps its raw segments (`meeting_segments`).
pub const LOCAL_RETENTION_DAYS: i64 = crate::contract::MEETING_LOCAL_RETENTION_DAYS;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingCopy {
    pub meeting: String,
    pub desktop_audio: String,
    pub apps: String,
    pub apps_none: String,
    pub you: String,
    pub desktop: String,
    pub who_all: String,
    pub who_except: String,
    pub and: String,
    pub consent_title: String,
    pub consent_body: String,
    pub start: String,
    pub cancel: String,
    pub pause: String,
    pub doctor_label: String,
    pub doctor_per_app: String,
    pub doctor_system: String,
    pub doctor_none: String,
    pub message_header: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingTestIds {
    pub meeting: String,
    pub desktop_audio: String,
    pub apps: String,
    pub consent_start: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DesktopAudioValue {
    pub value: DesktopAudio,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingLimits {
    pub apps: usize,
    pub app_name: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingFlush {
    pub max_segments: usize,
    pub max_age_ms: i64,
    pub turn_timeout_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MeetingPause {
    /// The fixed hotkey (`ctrl+alt+p`).
    pub shortcut: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingSpec {
    pub copy: MeetingCopy,
    pub test_ids: MeetingTestIds,
    pub desktop_audio_values: Vec<DesktopAudioValue>,
    pub limits: MeetingLimits,
    pub flush: MeetingFlush,
    pub pause: MeetingPause,
}

/// `fixtures/meeting.json` minus its cases.
pub static MEETING: LazyLock<MeetingSpec> =
    LazyLock::new(|| serde_json::from_str(MEETING_JSON).expect("meeting.json parses"));

/// The member's Desktop audio setting (`desktopAudioValues`); an unknown
/// value reads as `off`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase", from = "String")]
pub enum DesktopAudio {
    #[default]
    Off,
    All,
    Include,
    Exclude,
}

impl From<String> for DesktopAudio {
    fn from(key: String) -> Self {
        DesktopAudio::from_key(&key)
    }
}

impl From<String> for DesktopAudioCapability {
    fn from(key: String) -> Self {
        DesktopAudioCapability::from_key(&key)
    }
}

impl DesktopAudio {
    pub fn key(self) -> &'static str {
        match self {
            DesktopAudio::Off => "off",
            DesktopAudio::All => "all",
            DesktopAudio::Include => "include",
            DesktopAudio::Exclude => "exclude",
        }
    }

    pub fn from_key(key: &str) -> Self {
        match key {
            "all" => DesktopAudio::All,
            "include" => DesktopAudio::Include,
            "exclude" => DesktopAudio::Exclude,
            _ => DesktopAudio::Off,
        }
    }

    /// The picker's label (`desktopAudioValues`).
    pub fn label(self) -> &'static str {
        MEETING.desktop_audio_values.iter().find(|entry| entry.value == self).map(|entry| entry.label.as_str()).unwrap_or("")
    }

    /// Whether the Apps row shows (`include` / `exclude`).
    pub fn picks_apps(self) -> bool {
        matches!(self, DesktopAudio::Include | DesktopAudio::Exclude)
    }
}

/// What the voice device can capture (`doctor.desktopAudio.capability`); an
/// unknown value reads as `none`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", from = "String")]
pub enum DesktopAudioCapability {
    #[default]
    None,
    System,
    PerApp,
}

impl DesktopAudioCapability {
    pub fn from_key(key: &str) -> Self {
        match key {
            "system" => DesktopAudioCapability::System,
            "per_app" => DesktopAudioCapability::PerApp,
            _ => DesktopAudioCapability::None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            DesktopAudioCapability::None => "none",
            DesktopAudioCapability::System => "system",
            DesktopAudioCapability::PerApp => "per_app",
        }
    }
}

/// One app with an audio stream (`{id, name}`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct AudioApp {
    pub id: String,
    pub name: String,
}

/// One picker option (`{value, label}`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AppOption {
    pub value: String,
    pub label: String,
}

/// The Desktop audio row's values for the voice device's capability.
pub fn desktop_audio_options(capability: DesktopAudioCapability) -> Vec<DesktopAudio> {
    match capability {
        DesktopAudioCapability::PerApp => {
            vec![DesktopAudio::Off, DesktopAudio::All, DesktopAudio::Include, DesktopAudio::Exclude]
        }
        DesktopAudioCapability::System => vec![DesktopAudio::Off, DesktopAudio::All],
        DesktopAudioCapability::None => vec![DesktopAudio::Off],
    }
}

/// The Apps row's options: the live apps by name, then stored ones no longer
/// playing, in their stored order.
pub fn app_options(live: &[AudioApp], stored: &[AudioApp]) -> Vec<AppOption> {
    let mut by_name: Vec<&AudioApp> = live.iter().collect();
    by_name.sort_by(|a, b| a.name.cmp(&b.name));
    let mut out: Vec<AppOption> =
        by_name.into_iter().map(|app| AppOption { value: app.id.clone(), label: app.name.clone() }).collect();
    for app in stored {
        if !out.iter().any(|option| option.value == app.id) {
            out.push(AppOption { value: app.id.clone(), label: app.name.clone() });
        }
    }
    out
}

/// The Apps row's value: the picked names joined, or `copy.appsNone`.
pub fn apps_value(apps: &[AudioApp]) -> String {
    if apps.is_empty() {
        MEETING.copy.apps_none.clone()
    } else {
        apps.iter().map(|app| app.name.as_str()).collect::<Vec<_>>().join(", ")
    }
}

/// The setting as the device can honour it: no capability = off, system
/// only = include/exclude read as all.
fn effective(desktop_audio: DesktopAudio, capability: DesktopAudioCapability) -> DesktopAudio {
    match capability {
        DesktopAudioCapability::None => DesktopAudio::Off,
        DesktopAudioCapability::System if desktop_audio != DesktopAudio::Off => DesktopAudio::All,
        _ => desktop_audio,
    }
}

fn join_names(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => one.to_string(),
        [head @ .., last] => format!("{} {} {last}", head.join(", "), MEETING.copy.and),
    }
}

/// Who a meeting transcribes, in words.
pub fn who(desktop_audio: DesktopAudio, apps: &[AudioApp], capability: DesktopAudioCapability) -> String {
    let copy = &MEETING.copy;
    let you = copy.you.as_str();
    let names: Vec<&str> = apps.iter().map(|app| app.name.as_str()).collect();
    match effective(desktop_audio, capability) {
        DesktopAudio::All => join_names(&[you, &copy.who_all]),
        DesktopAudio::Exclude => {
            if names.is_empty() {
                join_names(&[you, &copy.who_all])
            } else {
                let except = copy.who_except.replace("{apps}", &join_names(&names));
                join_names(&[you, &except])
            }
        }
        DesktopAudio::Include => {
            let mut all = vec![you];
            all.extend(names);
            join_names(&all)
        }
        DesktopAudio::Off => you.to_string(),
    }
}

/// THE consent dialog's body.
pub fn consent_body(who: &str, agent: &str, days: i64) -> String {
    MEETING.copy.consent_body.replace("{who}", who).replace("{agent}", agent).replace("{days}", &days.to_string())
}

/// One channel a meeting listens on.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MeetingChannel {
    pub channel: String,
    pub label: String,
}

/// The channels a meeting listens on: the microphone, then the apps.
pub fn channels(
    desktop_audio: DesktopAudio,
    apps: &[AudioApp],
    capability: DesktopAudioCapability,
    playing: &[AudioApp],
) -> Vec<MeetingChannel> {
    let copy = &MEETING.copy;
    let mut out = vec![MeetingChannel { channel: "you".into(), label: copy.you.clone() }];
    let mode = effective(desktop_audio, capability);
    if mode == DesktopAudio::Off {
        return out;
    }
    if capability == DesktopAudioCapability::System {
        out.push(MeetingChannel { channel: "desktop".into(), label: copy.desktop.clone() });
        return out;
    }
    let picked = |id: &str| apps.iter().any(|app| app.id == id);
    for app in playing {
        let listened = match mode {
            DesktopAudio::All => true,
            DesktopAudio::Include => picked(&app.id),
            _ => !picked(&app.id),
        };
        if listened {
            out.push(MeetingChannel { channel: app.id.clone(), label: app.name.clone() });
        }
    }
    out
}

/// One flush event (`flushCases`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FlushEvent {
    Segment,
    Tick,
    Wake,
    Ptt,
    TurnDone,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlushState {
    /// `at` of every queued segment, oldest first.
    pub queued: Vec<i64>,
    pub in_flight_since: Option<i64>,
    /// A wake/ptt arrived while a turn was in flight.
    pub urgent: bool,
}

/// One flush step: how many queued segments go out now (None = nothing
/// sent); `state` moves on in place.
pub fn flush_step(state: &mut FlushState, event: FlushEvent, at: i64) -> Option<usize> {
    let flush = MEETING.flush;
    if state.in_flight_since.is_some_and(|since| at - since >= flush.turn_timeout_ms) {
        state.in_flight_since = None;
    }
    match event {
        FlushEvent::Segment => state.queued.push(at),
        FlushEvent::TurnDone => state.in_flight_since = None,
        FlushEvent::Wake | FlushEvent::Ptt if !state.queued.is_empty() => state.urgent = true,
        _ => {}
    }
    if state.in_flight_since.is_some() || state.queued.is_empty() {
        return None;
    }
    let due = state.urgent
        || state.queued.len() >= flush.max_segments
        || at - state.queued[0] >= flush.max_age_ms;
    if !due {
        return None;
    }
    let send = state.queued.len();
    state.queued.clear();
    state.urgent = false;
    state.in_flight_since = Some(at);
    Some(send)
}

/// One finalized segment as the message reads it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingSegment {
    pub channel: String,
    pub label: String,
    pub text: String,
    /// The first word's start, epoch ms.
    pub start_ms: i64,
}

/// (year, month, day, hour, minute, second) of epoch ms, UTC.
fn utc_parts(ms: i64) -> (i64, u32, u32, u32, u32, u32) {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400) as u32;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day, rem / 3600, rem % 3600 / 60, rem % 60)
}

/// The message a flush sends: a header and one fenced line per segment.
pub fn meeting_message(segments: &[MeetingSegment]) -> String {
    let mut ordered: Vec<&MeetingSegment> = segments.iter().collect();
    ordered.sort_by(|a, b| {
        a.start_ms.cmp(&b.start_ms).then_with(|| match (a.channel == "you", b.channel == "you") {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            (true, true) => std::cmp::Ordering::Equal,
            (false, false) => a.label.cmp(&b.label),
        })
    });
    let (year, month, day, ..) = utc_parts(ordered.first().map_or(0, |segment| segment.start_ms));
    let mut lines = vec![MEETING.copy.message_header.replace("{date}", &format!("{year:04}-{month:02}-{day:02}")), "```text".to_string()];
    for segment in ordered {
        let (.., hour, minute, second) = utc_parts(segment.start_ms);
        lines.push(format!("{hour:02}:{minute:02}:{second:02} {}: {}", segment.label, segment.text));
    }
    lines.push("```".to_string());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn fixture() -> Value {
        serde_json::from_str(MEETING_JSON).unwrap()
    }

    fn apps(value: &Value) -> Vec<AudioApp> {
        serde_json::from_value(value.clone()).unwrap()
    }

    fn capability(value: &Value) -> DesktopAudioCapability {
        DesktopAudioCapability::from_key(value.as_str().unwrap())
    }

    fn mode(value: &Value) -> DesktopAudio {
        DesktopAudio::from_key(value.as_str().unwrap())
    }

    #[test]
    fn the_spec_parses() {
        assert_eq!(MEETING.copy.meeting, "Meeting mode");
        assert_eq!(MEETING.pause.shortcut, "ctrl+alt+p");
        assert_eq!(DesktopAudio::Include.label(), "Pick apps");
        assert_eq!(RETENTION_DAYS, 7);
        assert_eq!(LOCAL_RETENTION_DAYS, 30);
        let unknown: DesktopAudio = serde_json::from_str("\"loud\"").unwrap();
        assert_eq!(unknown, DesktopAudio::Off);
        let unknown: DesktopAudioCapability = serde_json::from_str("\"sometimes\"").unwrap();
        assert_eq!(unknown, DesktopAudioCapability::None);
    }

    #[test]
    fn desktop_audio_options_match_the_fixture() {
        for case in fixture()["desktopAudioOptionCases"].as_array().unwrap() {
            let got: Vec<&str> = desktop_audio_options(capability(&case["capability"])).into_iter().map(DesktopAudio::key).collect();
            let expect: Vec<&str> = case["expect"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
            assert_eq!(got, expect, "{}", case["name"]);
        }
    }

    #[test]
    fn app_options_match_the_fixture() {
        for case in fixture()["appOptionCases"].as_array().unwrap() {
            let expect: Vec<AppOption> = serde_json::from_value(case["expect"].clone()).unwrap();
            assert_eq!(app_options(&apps(&case["live"]), &apps(&case["stored"])), expect, "{}", case["name"]);
        }
    }

    #[test]
    fn who_matches_the_fixture() {
        for case in fixture()["whoCases"].as_array().unwrap() {
            let got = who(mode(&case["desktopAudio"]), &apps(&case["apps"]), capability(&case["capability"]));
            assert_eq!(got, case["expect"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn consent_matches_the_fixture() {
        for case in fixture()["consentCases"].as_array().unwrap() {
            let got = consent_body(case["who"].as_str().unwrap(), case["agent"].as_str().unwrap(), case["days"].as_i64().unwrap());
            assert_eq!(got, case["expect"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn channels_match_the_fixture() {
        for case in fixture()["channelCases"].as_array().unwrap() {
            let got = channels(mode(&case["desktopAudio"]), &apps(&case["apps"]), capability(&case["capability"]), &apps(&case["playing"]));
            let expect: Vec<MeetingChannel> = serde_json::from_value(case["expect"].clone()).unwrap();
            assert_eq!(got, expect, "{}", case["name"]);
        }
    }

    #[test]
    fn flush_matches_the_fixture() {
        for case in fixture()["flushCases"].as_array().unwrap() {
            let mut state = FlushState::default();
            for step in case["steps"].as_array().unwrap() {
                let event: FlushEvent = serde_json::from_value(step["event"].clone()).unwrap();
                let send = flush_step(&mut state, event, step["at"].as_i64().unwrap());
                assert_eq!(send.map(|n| n as u64), step["send"].as_u64(), "{} {step}", case["name"]);
            }
        }
    }

    #[test]
    fn message_matches_the_fixture() {
        for case in fixture()["messageCases"].as_array().unwrap() {
            let segments: Vec<MeetingSegment> = serde_json::from_value(case["segments"].clone()).unwrap();
            assert_eq!(meeting_message(&segments), case["expect"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn utc_parts_reads_epoch_and_leap_days() {
        assert_eq!(utc_parts(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(utc_parts(951_782_400_000), (2000, 2, 29, 0, 0, 0));
    }
}
