//! #NEW-79: the voice agent's launcher-enforced tool policy, byte-locked by
//! `fixtures/voice-policy.json` (its `_` is the spec; every case list
//! replays below). The engine's claude adapter classifies each
//! `can_use_tool` with [`tier`] and builds the proposal card with [`card`];
//! the IDE's push to talk answers waiting cards by voice with
//! [`match_answer`] before a transcript goes out as a message.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;
use serde_json::{Map, Value};

const VOICE_POLICY_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/voice-policy.json");

/// What happens to one tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum Tier {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "confirm")]
    Confirm,
    #[serde(rename = "confirm-strong")]
    ConfirmStrong,
    #[serde(rename = "deny")]
    Deny,
}

impl Tier {
    /// A proposal card asks first.
    pub fn asks(self) -> bool {
        matches!(self, Self::Confirm | Self::ConfirmStrong)
    }

    /// How much a call needs before it runs (`deny` = never).
    fn rank(self) -> u8 {
        match self {
            Self::Auto => 0,
            Self::Confirm => 1,
            Self::ConfirmStrong => 2,
            Self::Deny => 3,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyLimits {
    pub snippet: usize,
    pub value: usize,
    pub arguments: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PolicyOptions {
    pub allow: String,
    pub deny: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyCopy {
    pub header: String,
    pub header_strong: String,
    pub strong_tail: String,
    pub snippet: String,
    pub denied: String,
    pub rejected: String,
    pub replaced: String,
    pub expired: String,
    pub changed: String,
    pub changed_ask: String,
    pub answered: String,
}

#[derive(Debug, Clone, Deserialize)]
struct When {
    field: String,
    equals: Value,
    tier: Tier,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum OpRule {
    Plain(Tier),
    When { tier: Tier, when: Vec<When> },
}

impl OpRule {
    fn tier(&self, input: &Value) -> Tier {
        match self {
            Self::Plain(tier) => *tier,
            Self::When { tier, when } => when
                .iter()
                .find(|rule| input.get(&rule.field) == Some(&rule.equals))
                .map_or(*tier, |rule| rule.tier),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct NounRule {
    label: String,
    default: Tier,
    #[serde(default)]
    ops: HashMap<String, OpRule>,
}

#[derive(Debug, Clone, Deserialize)]
struct CardRule {
    label: String,
    #[serde(default)]
    target: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Phrases {
    all_allow: Vec<String>,
    all_deny: Vec<String>,
    allow: Vec<String>,
    deny: Vec<String>,
    yes: Vec<String>,
    no: Vec<String>,
    but: Vec<String>,
    numbers: HashMap<String, i64>,
    ordinals: HashMap<String, i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoicePolicy {
    exponential_prefix: String,
    pub expires_after_ms: u64,
    pub pre_approved_for_ms: u64,
    pub limits: PolicyLimits,
    pub options: PolicyOptions,
    pub copy: PolicyCopy,
    target_keys: Vec<String>,
    create_ops: Vec<String>,
    nouns: HashMap<String, NounRule>,
    tools: HashMap<String, Tier>,
    cards: HashMap<String, CardRule>,
    other_mcp: Tier,
    unknown: Tier,
    phrases: Phrases,
}

pub static VOICE_POLICY: LazyLock<VoicePolicy> =
    LazyLock::new(|| serde_json::from_str(VOICE_POLICY_JSON).expect("voice-policy.json parses"));

/// An Exponential tool's noun (`mcp__exponential__exponential_issues` →
/// `issues`).
fn noun_of(tool: &str) -> Option<&str> {
    tool.strip_prefix(VOICE_POLICY.exponential_prefix.as_str())
}

fn op_of(input: &Value) -> Option<&str> {
    input.get("op").and_then(Value::as_str).filter(|op| !op.is_empty())
}

/// The tier of one call (`classifyCases`).
pub fn tier(tool: &str, input: &Value) -> Tier {
    let policy = &*VOICE_POLICY;
    if let Some(noun) = noun_of(tool) {
        let Some(rule) = policy.nouns.get(noun) else { return policy.unknown };
        return op_of(input).and_then(|op| rule.ops.get(op)).map_or(rule.default, |op| op.tier(input));
    }
    if let Some(tier) = policy.tools.get(tool) {
        return *tier;
    }
    if tool.starts_with("mcp__") {
        return policy.other_mcp;
    }
    policy.unknown
}

/// One proposal card's words and keys (`cardCases`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub tier: Tier,
    pub header: String,
    pub question: String,
    /// Same key = the newer card withdraws the older one.
    pub replace_key: Option<String>,
    /// What a change pre-approves (`{tool}#{op}|{target}`): only a
    /// `confirm` card with a target has one; a confirm-strong call or a
    /// call without a target always asks again.
    pub approval_key: Option<String>,
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let head: String = text.chars().take(max).collect();
    format!("{}…", head.trim_end())
}

fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(one_line(text)).filter(|text| !text.is_empty()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

/// `statusId` / `file_path` → `status id` / `file path`.
fn key_words(key: &str) -> String {
    let mut words = String::new();
    for ch in key.chars() {
        if ch == '_' || ch == '-' {
            words.push(' ');
        } else if ch.is_uppercase() {
            if !words.is_empty() && !words.ends_with(' ') {
                words.push(' ');
            }
            words.extend(ch.to_lowercase());
        } else {
            words.push(ch);
        }
    }
    one_line(&words)
}

fn argument_value(value: &Value) -> Option<String> {
    let max = VOICE_POLICY.limits.value;
    match value {
        Value::Null => None,
        Value::String(_) => {
            let text = cut(&scalar(value)?, max);
            Some(if text.contains(' ') { format!("“{text}”") } else { text })
        }
        Value::Array(items) if items.is_empty() => None,
        Value::Array(items) => {
            let parts: Option<Vec<String>> = items.iter().map(scalar).collect();
            Some(parts.map_or_else(|| "…".to_string(), |parts| cut(&parts.join(", "), max)))
        }
        Value::Object(_) => Some("…".to_string()),
        _ => scalar(value),
    }
}

/// The card for a call the policy asks about. `snippet` = the person's
/// message the turn came from.
pub fn card(tool: &str, input: &Value, tier: Tier, snippet: &str) -> Card {
    let policy = &*VOICE_POLICY;
    let empty = Map::new();
    let fields = input.as_object().unwrap_or(&empty);
    let op = op_of(input);
    let (label, target_key) = if let Some(noun) = noun_of(tool) {
        let label = policy.nouns.get(noun).map_or_else(|| noun.to_string(), |rule| rule.label.clone());
        let label = match op {
            Some(op) => format!("{label} · {}", op.replace('_', " ")),
            None => label,
        };
        let key = policy.target_keys.iter().find(|key| fields.get(*key).and_then(scalar).is_some()).cloned();
        (label, key)
    } else if let Some(rule) = policy.cards.get(tool) {
        (rule.label.clone(), rule.target.clone().filter(|key| fields.get(key).and_then(scalar).is_some()))
    } else if let Some(rest) = tool.strip_prefix("mcp__") {
        let (server, name) = rest.split_once("__").unwrap_or((rest, ""));
        (one_line(&format!("{server} {name}")), None)
    } else {
        (tool.to_string(), None)
    };
    let target = target_key.as_ref().and_then(|key| fields.get(key)).and_then(scalar);

    let mut parts: Vec<String> = fields
        .iter()
        .filter(|(key, _)| key.as_str() != "op" && Some(key.as_str()) != target_key.as_deref())
        .filter_map(|(key, value)| Some(format!("{}: {}", key_words(key), argument_value(value)?)))
        .collect();
    let more = parts.len().saturating_sub(policy.limits.arguments);
    parts.truncate(policy.limits.arguments);

    let mut question = label;
    if let Some(target) = &target {
        question.push(' ');
        question.push_str(target);
    }
    if !parts.is_empty() {
        question.push_str(" — ");
        question.push_str(&parts.join(", "));
        if more > 0 {
            question.push_str(&format!(" and {more} more"));
        }
    }
    question.push('.');
    if tier == Tier::ConfirmStrong {
        question.push_str(&policy.copy.strong_tail);
    }
    let said = one_line(snippet);
    if !said.is_empty() {
        question.push_str("\n\n");
        question.push_str(&policy.copy.snippet.replace("{text}", &cut(&said, policy.limits.snippet)));
    }

    let creates = op.is_some_and(|op| policy.create_ops.iter().any(|create| create == op));
    let approval_key = target
        .as_ref()
        .filter(|_| tier == Tier::Confirm)
        .map(|target| format!("{tool}#{}|{target}", op.unwrap_or_default()));
    Card {
        tier,
        header: if tier == Tier::ConfirmStrong { policy.copy.header_strong.clone() } else { policy.copy.header.clone() },
        question,
        replace_key: target.filter(|_| !creates).map(|target| format!("{tool}|{target}")),
        approval_key,
    }
}

/// A typed or spoken answer to a card (`answerCases`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardAnswer {
    Allow,
    Deny,
    /// `yes, but …`: the person changed the call.
    Change(String),
    /// Anything else: not run, the words go to the model.
    Other(String),
}

/// What a card's answer means: blank or the deny label = deny, the allow
/// label = allow; else a leading no / deny phrase = deny, a bare yes /
/// allow phrase = allow, `[yes] [,] but …` = a change, anything else =
/// other (never a change: "don't delete it" must not pre-approve).
pub fn read_answer(answer: &str) -> CardAnswer {
    let policy = &*VOICE_POLICY;
    let phrases = &policy.phrases;
    let answer = answer.trim();
    if answer.is_empty() || answer.eq_ignore_ascii_case(&policy.options.deny) {
        return CardAnswer::Deny;
    }
    if answer.eq_ignore_ascii_case(&policy.options.allow) {
        return CardAnswer::Allow;
    }
    let words = tokens(answer);
    let plain: Vec<String> = words.iter().filter(|word| *word != ",").cloned().collect();
    let negative = [&phrases.no, &phrases.deny, &phrases.all_deny];
    if negative.iter().any(|list| take_phrase(&plain, list).is_some()) {
        return CardAnswer::Deny;
    }
    let lead = take_phrase(&words, &phrases.yes).or_else(|| take_phrase(&words, &phrases.allow));
    if lead.is_some_and(|taken| taken == plain.len()) {
        return CardAnswer::Allow;
    }
    if take_change(&words[lead.unwrap_or(0)..]).is_some() {
        return CardAnswer::Change(answer.to_string());
    }
    CardAnswer::Other(answer.to_string())
}

/// What the engine answers a call the policy asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    /// Denied with this message for the model.
    Deny(String),
}

/// #NEW-79: the voice conversation's open cards (by replace key) and the
/// redo a change pre-approved (`replaceCases`, `approvalCases`). Pure: the
/// engine passes its clock in milliseconds.
#[derive(Debug, Default)]
pub struct VoiceLedger {
    waiting: HashMap<String, String>,
    approved: HashMap<String, (u64, Tier)>,
}

impl VoiceLedger {
    /// A call about to show `card`: `true` = a change pre-approved exactly
    /// this tool, op and target at this tier or lower, so it runs now
    /// (once).
    pub fn take_approval(&mut self, card: &Card, now_ms: u64) -> bool {
        self.approved.retain(|_, (until, _)| *until > now_ms);
        let Some(key) = &card.approval_key else { return false };
        match self.approved.get(key) {
            Some((_, tier)) if card.tier.rank() <= tier.rank() => {
                self.approved.remove(key);
                true
            }
            _ => false,
        }
    }

    /// `card` (asked as `id`) is waiting: the id of the older card for the
    /// same target it withdraws.
    pub fn park(&mut self, card: &Card, id: &str) -> Option<String> {
        let key = card.replace_key.as_ref()?;
        self.waiting.insert(key.clone(), id.to_string()).filter(|older| older != id)
    }

    /// `card` stopped waiting (answered, replaced or expired).
    pub fn unpark(&mut self, card: &Card, id: &str) {
        if let Some(key) = &card.replace_key {
            if self.waiting.get(key).is_some_and(|waiting| waiting == id) {
                self.waiting.remove(key);
            }
        }
    }

    /// The person answered `card`.
    pub fn answered(&mut self, card: &Card, answer: &CardAnswer, now_ms: u64) -> Verdict {
        let copy = &VOICE_POLICY.copy;
        match answer {
            CardAnswer::Allow => Verdict::Allow,
            CardAnswer::Deny => Verdict::Deny(copy.rejected.clone()),
            CardAnswer::Other(words) => Verdict::Deny(copy.answered.replace("{answer}", words)),
            CardAnswer::Change(words) => match &card.approval_key {
                Some(key) => {
                    self.approved.insert(key.clone(), (now_ms + VOICE_POLICY.pre_approved_for_ms, card.tier));
                    Verdict::Deny(copy.changed.replace("{answer}", words))
                }
                None => Verdict::Deny(copy.changed_ask.replace("{answer}", words)),
            },
        }
    }
}

// ── Voice answers (`phraseCases`) ──────────────────────────────────────────

/// A waiting card, oldest first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaitingCard {
    pub id: String,
    pub strong: bool,
}

/// The voice conversation's waiting proposal cards, oldest first
/// (`waitingCardCases`); a card older than its expiry (the engine withdrew
/// it) or without a time is never answerable. `now_ms` = UTC milliseconds.
pub fn waiting_cards<'a>(
    messages: impl IntoIterator<Item = &'a crate::rows::MessageRow>,
    now_ms: i64,
) -> Vec<WaitingCard> {
    let policy = &*VOICE_POLICY;
    let strong_tail = policy.copy.strong_tail.trim();
    let expiry = i64::try_from(policy.expires_after_ms).unwrap_or(i64::MAX);
    let mut cards: Vec<(i64, &str, WaitingCard)> = messages
        .into_iter()
        .filter(|message| message.answered_at.as_deref().is_none_or(str::is_empty))
        .filter_map(|message| {
            let card = message.card.as_ref()?;
            if card.get("kind").and_then(Value::as_str) != Some("question") {
                return None;
            }
            card.get("questionId").and_then(Value::as_str).filter(|id| !id.is_empty())?;
            let options: Vec<&str> = card.get("options")?.as_array()?.iter().filter_map(Value::as_str).collect();
            if !options.contains(&policy.options.allow.as_str()) || !options.contains(&policy.options.deny.as_str()) {
                return None;
            }
            let question = card.get("question").and_then(Value::as_str).unwrap_or_default();
            let at = message.created_at.as_deref().and_then(crate::voice::parse_instant)?;
            if now_ms.saturating_sub(at) >= expiry {
                return None;
            }
            Some((at, message.id.as_str(), WaitingCard { id: message.id.clone(), strong: question.contains(strong_tail) }))
        })
        .collect();
    cards.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)));
    cards.into_iter().map(|(_, _, card)| card).collect()
}

/// One answer to post through `sessions.answer`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceAnswer {
    pub id: String,
    pub answer: String,
}

fn normalise(text: &str) -> Vec<String> {
    let mapped: String = text
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_alphanumeric() { ch } else { ' ' })
        .collect();
    mapped.split_whitespace().map(str::to_string).collect()
}

/// The words with commas kept as their own token (`two, but …`).
fn tokens(text: &str) -> Vec<String> {
    let spaced = text.replace(',', " , ");
    let mapped: String = spaced
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_alphanumeric() || ch == ' ' || ch == ',' { ch } else { ' ' })
        .collect();
    mapped.split_whitespace().map(str::to_string).collect()
}

/// Longest phrase of `list` at the head of `words`: how many words it took.
fn take_phrase(words: &[String], list: &[String]) -> Option<usize> {
    list.iter()
        .map(|phrase| normalise(phrase))
        .filter(|phrase| !phrase.is_empty() && words.len() >= phrase.len() && words[..phrase.len()] == phrase[..])
        .map(|phrase| phrase.len())
        .max()
}

/// A card number at the head of `words` (after an optional `card` /
/// `number`): (1-based index or -1 for the last, words taken).
fn take_number(words: &[String], after_verb: bool) -> Option<(i64, usize)> {
    let phrases = &VOICE_POLICY.phrases;
    let skip = usize::from(words.first().is_some_and(|word| word == "card" || word == "number"));
    let word = words.get(skip)?;
    let value = word
        .parse::<i64>()
        .ok()
        .filter(|n| *n > 0)
        .or_else(|| phrases.numbers.get(word).copied())
        .or_else(|| phrases.ordinals.get(word).copied())
        .or_else(|| (after_verb && skip == 0 && (word == "to" || word == "too")).then_some(2))?;
    Some((value, skip + 1))
}

fn pick<'a>(cards: &'a [WaitingCard], number: i64) -> Option<&'a WaitingCard> {
    if number == -1 {
        return cards.last();
    }
    cards.get(usize::try_from(number).ok()?.checked_sub(1)?)
}

/// `[,] {but} {change}` at the head of `words`: the change words.
fn take_change(words: &[String]) -> Option<String> {
    let rest = if words.first().is_some_and(|word| word == ",") { &words[1..] } else { words };
    let taken = take_phrase(rest, &VOICE_POLICY.phrases.but)?;
    let change: Vec<&str> = rest[taken..].iter().map(String::as_str).filter(|word| *word != ",").collect();
    (!change.is_empty()).then(|| change.join(" "))
}

fn change_answer(change: &str) -> String {
    format!("Yes, but {change}")
}

/// A finalized transcript as answers to the waiting cards, or `None` when it
/// is not one (it then goes out as a message).
pub fn match_answer(text: &str, cards: &[WaitingCard]) -> Option<Vec<VoiceAnswer>> {
    if cards.is_empty() {
        return None;
    }
    let policy = &*VOICE_POLICY;
    let phrases = &policy.phrases;
    let allow = || policy.options.allow.clone();
    let deny = || policy.options.deny.clone();
    let words = tokens(text);
    // Commas only matter before a `but`; anywhere else they are spaces.
    let plain: Vec<String> = words.iter().filter(|word| *word != ",").cloned().collect();
    let whole = |list: &[String]| take_phrase(&plain, list).is_some_and(|taken| taken == plain.len());

    if whole(&phrases.all_allow) {
        let answers: Vec<VoiceAnswer> = cards
            .iter()
            .filter(|card| !card.strong)
            .map(|card| VoiceAnswer { id: card.id.clone(), answer: allow() })
            .collect();
        return (!answers.is_empty()).then_some(answers);
    }
    if whole(&phrases.all_deny) {
        return Some(cards.iter().map(|card| VoiceAnswer { id: card.id.clone(), answer: deny() }).collect());
    }

    // `{allow|yes|deny}? {n} …`
    let (lead, taken) = if let Some(taken) = take_phrase(&words, &phrases.yes) {
        (Some("yes"), taken)
    } else if let Some(taken) = take_phrase(&words, &phrases.allow) {
        (Some("allow"), taken)
    } else if let Some(taken) = take_phrase(&words, &phrases.deny) {
        (Some("deny"), taken)
    } else {
        (None, 0)
    };
    if let Some((number, used)) = take_number(&words[taken..], lead.is_some()) {
        let card = pick(cards, number)?;
        let rest = &words[taken + used..];
        let one = |answer: String| Some(vec![VoiceAnswer { id: card.id.clone(), answer }]);
        if rest.is_empty() {
            return match lead {
                Some("yes") => one(allow()),
                Some("allow") if !card.strong => one(allow()),
                Some("deny") => one(deny()),
                _ => None,
            };
        }
        if lead == Some("deny") {
            return None;
        }
        let change = take_change(rest)?;
        if card.strong && lead != Some("yes") {
            return None;
        }
        return one(change_answer(&change));
    }

    let newest = cards.last()?;
    let one = |answer: String| Some(vec![VoiceAnswer { id: newest.id.clone(), answer }]);
    if let Some(taken) = take_phrase(&words, &phrases.yes) {
        if taken == words.len() {
            return one(allow());
        }
        return one(change_answer(&take_change(&words[taken..])?));
    }
    if whole(&phrases.no) {
        return one(deny());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> Value {
        serde_json::from_str(VOICE_POLICY_JSON).unwrap()
    }

    fn tier_of(name: &str) -> Tier {
        serde_json::from_value(Value::String(name.to_string())).unwrap()
    }

    #[test]
    fn classify_cases_replay() {
        for case in fixture()["classifyCases"].as_array().unwrap() {
            let tool = case["tool"].as_str().unwrap();
            assert_eq!(tier(tool, &case["input"]), tier_of(case["tier"].as_str().unwrap()), "{case}");
        }
    }

    #[test]
    fn card_cases_replay() {
        for case in fixture()["cardCases"].as_array().unwrap() {
            let tool = case["tool"].as_str().unwrap();
            let input = &case["input"];
            let built = card(tool, input, tier(tool, input), case["snippet"].as_str().unwrap());
            assert_eq!(built.header, case["header"].as_str().unwrap(), "{case}");
            assert_eq!(built.question, case["question"].as_str().unwrap(), "{case}");
            assert_eq!(built.replace_key.as_deref(), case["replaceKey"].as_str(), "{case}");
            assert_eq!(built.approval_key.as_deref(), case["approvalKey"].as_str(), "{case}");
        }
    }

    #[test]
    fn phrase_cases_replay() {
        for case in fixture()["phraseCases"].as_array().unwrap() {
            let cards: Vec<WaitingCard> = case["cards"]
                .as_array()
                .unwrap()
                .iter()
                .map(|card| WaitingCard { id: card["id"].as_str().unwrap().to_string(), strong: card["strong"].as_bool().unwrap() })
                .collect();
            let expected: Option<Vec<VoiceAnswer>> = case["answers"].as_array().map(|answers| {
                answers
                    .iter()
                    .map(|answer| VoiceAnswer {
                        id: answer["id"].as_str().unwrap().to_string(),
                        answer: answer["answer"].as_str().unwrap().to_string(),
                    })
                    .collect()
            });
            assert_eq!(match_answer(case["text"].as_str().unwrap(), &cards), expected, "{case}");
        }
    }

    #[test]
    fn waiting_card_cases_replay() {
        for case in fixture()["waitingCardCases"].as_array().unwrap() {
            let rows: Vec<crate::rows::MessageRow> = case["messages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| crate::rows::MessageRow {
                    id: m["id"].as_str().unwrap().to_string(),
                    card: Some(m["card"].clone()).filter(|card| !card.is_null()),
                    answered_at: m["answeredAt"].as_str().map(str::to_string),
                    created_at: m["createdAt"].as_str().map(str::to_string),
                    body: m["body"].as_str().map(str::to_string),
                    ..Default::default()
                })
                .collect();
            let expected: Vec<WaitingCard> = case["cards"]
                .as_array()
                .unwrap()
                .iter()
                .map(|card| WaitingCard { id: card["id"].as_str().unwrap().to_string(), strong: card["strong"].as_bool().unwrap() })
                .collect();
            let now = crate::voice::parse_instant(case["now"].as_str().unwrap()).unwrap();
            assert_eq!(waiting_cards(&rows, now), expected, "{case}");
        }
    }

    #[test]
    fn every_noun_of_the_surface_has_a_rule() {
        for noun in [
            "issues", "comments", "attachments", "boards", "teams", "people", "team_config", "apps", "sessions",
            "sessions_send", "session_self", "guide_write", "pr", "devices", "events", "notifications",
        ] {
            assert!(VOICE_POLICY.nouns.contains_key(noun), "{noun}");
        }
    }

    fn built(call: &Value) -> Card {
        let tool = call["tool"].as_str().unwrap();
        card(tool, &call["input"], tier(tool, &call["input"]), "")
    }

    #[test]
    fn answer_cases_replay() {
        for case in fixture()["answerCases"].as_array().unwrap() {
            let text = case["text"].as_str().unwrap();
            let kind = match read_answer(text) {
                CardAnswer::Allow => "allow",
                CardAnswer::Deny => "deny",
                CardAnswer::Change(words) => {
                    assert_eq!(words, text.trim(), "{case}");
                    "change"
                }
                CardAnswer::Other(_) => "other",
            };
            assert_eq!(kind, case["answer"].as_str().unwrap(), "{case}");
        }
    }

    /// The whole card path: the first call's card, the person's answer
    /// (what the model is told), the redo after `afterMs` and a second
    /// identical redo.
    #[test]
    fn approval_cases_replay() {
        let copy = &VOICE_POLICY.copy;
        for case in fixture()["approvalCases"].as_array().unwrap() {
            let mut ledger = VoiceLedger::default();
            let first = built(&case["first"]);
            assert!(first.tier.asks(), "{case}");
            assert!(!ledger.take_approval(&first, 0), "{case}");
            let answer = case["answer"].as_str().unwrap();
            let verdict = ledger.answered(&first, &read_answer(answer), 0);
            let expected = match case["reply"].as_str().unwrap() {
                "allow" => Verdict::Allow,
                "rejected" => Verdict::Deny(copy.rejected.clone()),
                "answered" => Verdict::Deny(copy.answered.replace("{answer}", answer)),
                "changed" => Verdict::Deny(copy.changed.replace("{answer}", answer)),
                "changedAsk" => Verdict::Deny(copy.changed_ask.replace("{answer}", answer)),
                other => panic!("unknown reply {other}"),
            };
            assert_eq!(verdict, expected, "{case}");
            let redo = built(&case["redo"]);
            let at = case["afterMs"].as_u64().unwrap();
            assert_eq!(ledger.take_approval(&redo, at), case["runs"].as_bool().unwrap(), "{case}");
            assert_eq!(ledger.take_approval(&redo, at + 1), case["thenAgain"].as_bool().unwrap(), "{case}");
        }
    }

    #[test]
    fn replace_cases_replay() {
        for case in fixture()["replaceCases"].as_array().unwrap() {
            let mut ledger = VoiceLedger::default();
            let replaced: Vec<Option<String>> = case["cards"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(index, call)| ledger.park(&built(call), &index.to_string()))
                .collect();
            let expected: Vec<Option<String>> = case["replaced"]
                .as_array()
                .unwrap()
                .iter()
                .map(|index| index.as_u64().map(|index| index.to_string()))
                .collect();
            assert_eq!(replaced, expected, "{case}");
        }
    }

    /// An answered card leaves the ledger: a later card for the same target
    /// replaces nothing.
    #[test]
    fn an_unparked_card_is_never_replaced() {
        let mut ledger = VoiceLedger::default();
        let first = built(&json!({ "tool": "mcp__exponential__exponential_issues", "input": { "op": "update", "id": "NEW-1" } }));
        assert_eq!(ledger.park(&first, "a"), None);
        ledger.unpark(&first, "a");
        assert_eq!(ledger.park(&first, "b"), None);
        ledger.unpark(&first, "a");
        assert_eq!(ledger.park(&first, "c"), Some("b".to_string()));
    }
}
