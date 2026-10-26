//! NEW-53: exponentialTalk's voice settings and spoken-reply rules, the
//! desktop mirror of `packages/domain-contract/src/voice.ts`, byte-locked by
//! `fixtures/voice.json` (every case list replays below).

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
    pub downloading: String,
    pub download_failed: String,
    pub unavailable: String,
    pub speaking: String,
    pub stop: String,
    pub link: String,
    pub ellipsis: String,
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

/// The input rows' length caps (`limits`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceLimits {
    pub voice_file: usize,
    pub voice_transcript: usize,
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceSpec {
    pub copy: VoiceCopy,
    pub max_chars: usize,
    pub test_ids: VoiceTestIds,
    pub shortcuts: Vec<VoiceShortcut>,
    pub languages: Vec<VoiceLanguage>,
    pub defaults: VoiceSettings,
    pub limits: VoiceLimits,
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

/// #NEW-73: the Language row's trailing detail; German speaks on the voice
/// device's GPU only. None without a voice device or for English.
pub fn language_note(language: &str, device_name: Option<&str>) -> Option<String> {
    let device_name = device_name.filter(|_| language == "de")?;
    Some(VOICE.copy.language_note.replace("{device}", device_name))
}

/// The tray group's header for the voice device.
pub fn tray_header(device_name: &str) -> String {
    VOICE.copy.tray_header.replace("{device}", device_name)
}

/// The wake phrase's placeholder (and its value when empty).
pub fn wake_phrase_placeholder(agent_name: &str) -> String {
    VOICE.copy.wake_phrase_placeholder.replace("{agent}", agent_name)
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
    /// The languages THIS device has a voice for.
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

fn digits_words(digits: &str) -> String {
    digits
        .chars()
        .filter_map(|c| c.to_digit(10))
        .map(|d| ONES[d as usize])
        .collect::<Vec<_>>()
        .join(" ")
}

/// English cardinal words for a number written with digits (a decimal part
/// reads `point` and then digit by digit).
pub fn number_words(text: &str) -> String {
    let (whole, fraction) = match text.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (text, None),
    };
    let mut words = if whole.len() > 9 {
        digits_words(whole)
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
        words.push_str(" point ");
        words.push_str(&digits_words(fraction));
    }
    words
}

/// How people say an issue number (see `voice.json`).
pub fn identifier_number_words(digits: &str) -> String {
    if digits.len() <= 3 {
        return number_words(digits);
    }
    if digits.len() > 4 {
        return digits_words(digits);
    }
    let n: u64 = digits.parse().unwrap_or(0);
    if n % 1000 == 0 {
        return number_words(digits);
    }
    let high = n / 100;
    let low = n % 100;
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

/// What a voice says for a reply's Markdown body (`voice.json` rules).
pub fn spoken_text(body: &str, max_chars: usize) -> String {
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
        format!("{}{tail}", VOICE.copy.link)
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
        format!("{} {}", spell(&caps[1]), identifier_number_words(&caps[2]))
    });
    let text = ACRONYM.replace_all(&text, |caps: &Captures<'_>| spell(&caps[0]));
    let text = PERCENT.replace_all(&text, |caps: &Captures<'_>| format!("{} percent", number_words(&caps[1])));
    let text = NUMBER.replace_all(&text, |caps: &Captures<'_>| number_words(&caps[0]));
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
            assert_eq!(spoken_text(case["body"].as_str().unwrap(), max), case["spoken"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn number_words_match_the_fixture() {
        let fixture = fixture();
        for case in fixture["numberWordsCases"].as_array().unwrap() {
            assert_eq!(number_words(case["n"].as_str().unwrap()), case["words"].as_str().unwrap());
        }
        for case in fixture["identifierNumberCases"].as_array().unwrap() {
            assert_eq!(identifier_number_words(case["n"].as_str().unwrap()), case["words"].as_str().unwrap());
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
}
