//! #NEW-136: the MODEL FALLBACK of the issue search, its pure half. When the
//! phrase grammar ([`crate::issue_search_phrase`]) does not recognise a
//! sentence, the desktop app and iOS 26 offer `Translate to a search`: ONE
//! short call to a model on the user's own device or login (never typing,
//! never our server). This module decides when the row shows ([`wants`]),
//! builds the ONE prompt both callers send ([`prompt`]) and turns the model's
//! answer into a query the engine accepts ([`sanitize`]: the output is never
//! trusted). The call itself is `coding::search_translate`.
//!
//! Hand-mirrored from web `apps/web/src/lib/issue-search-translation.ts`
//! (the reference) and iOS `IssueSearchTranslation.swift`, byte-locked by
//! `packages/domain-contract/fixtures/issue-search-translation.json` (its
//! `$comment` is the spec; read it before changing a rule).

use std::sync::LazyLock;

use serde::Deserialize;

use crate::issue_search::{parse_range, tokens, words, DAY_MS};
use crate::issue_search_phrase::{civil_from_days, translate, PhraseMember};

const FIXTURE: &str =
    include_str!("../../../../../packages/domain-contract/fixtures/issue-search-translation.json");

/// The fixture's `limits`: the numbers every caller shares.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationLimits {
    pub min_words: usize,
    pub max_text_length: usize,
    pub max_tokens: usize,
    pub max_words: usize,
    pub max_word_length: usize,
    pub max_names: usize,
    /// The call's hard deadline.
    pub timeout_seconds: u64,
    /// The claude model alias of the desktop call.
    pub model: String,
}

/// The fixture's `hints`: the words that say a sentence names a TIME or a
/// STATE (space-separated). A date or a state the model answers without one
/// is dropped.
#[derive(Debug, Deserialize)]
struct Hints {
    time: String,
    state: String,
}

#[derive(Debug, Deserialize)]
struct Spec {
    system: String,
    hints: Hints,
    limits: TranslationLimits,
}

static SPEC: LazyLock<Spec> =
    LazyLock::new(|| serde_json::from_str(FIXTURE).expect("the issue-search-translation fixture parses"));

pub fn limits() -> &'static TranslationLimits {
    &SPEC.limits
}

/// The instruction line the model gets beside the prompt.
pub fn system() -> &'static str {
    &SPEC.system
}

const FILTER_PREFIXES: [&str; 6] = ["by:", "is:", "status:", "created:", "updated:", "similar:"];
/// What a model wraps its answer in: quotes, backticks, emphasis.
const WRAP: &str = "`\"'*“”„‘’«»";
const LABELS: [&str; 2] = ["search:", "query:"];
const STRIP: &str = ".,;:!?\"'()";

fn strip_wrap(text: &str) -> &str {
    text.trim_matches(|c: char| WRAP.contains(c) || c.is_whitespace())
}

/// A token without its leading and trailing punctuation (the grammar's).
fn strip_punctuation(token: &str) -> &str {
    token.trim_matches(|c: char| STRIP.contains(c))
}

/// Whether `value` is one word of letters and digits (the engine's words).
fn is_word(value: &str) -> bool {
    words(value) == [value]
}

/// One line of the model's answer without its fence, quotes and label.
fn clean_line(line: &str) -> &str {
    let trimmed = line.trim();
    // A code fence line (``` or ```text) is not the answer.
    if trimmed.starts_with("```") && !(trimmed.len() > 3 && trimmed.ends_with('`')) {
        return "";
    }
    let bare = strip_wrap(trimmed);
    for label in LABELS {
        if bare.get(..label.len()).is_some_and(|head| head.eq_ignore_ascii_case(label)) {
            return strip_wrap(&bare[label.len()..]);
        }
    }
    bare
}

/// Each member's name words and email local part, lowercased.
struct Member {
    words: Vec<String>,
    local: String,
}

fn members_of(members: &[PhraseMember<'_>]) -> Vec<Member> {
    members
        .iter()
        .map(|member| {
            let email = member.email.unwrap_or("").to_lowercase();
            let local = email.split_once('@').map_or(email.as_str(), |(local, _)| local);
            Member {
                words: words(&member.name.unwrap_or("").to_lowercase())
                    .into_iter()
                    .map(str::to_string)
                    .collect(),
                local: local.to_string(),
            }
        })
        .collect()
}

fn add(list: &mut Vec<String>, value: String) {
    if !list.contains(&value) {
        list.push(value);
    }
}

/// Whether the global search offers `Translate to a search` for `text`: the
/// text has `min_words` or more words, is at most `max_text_length` long,
/// carries no filter token, and the phrase grammar does not recognise it.
/// `now` = epoch ms.
pub fn wants(text: &str, members: &[PhraseMember<'_>], now: i64) -> bool {
    let limits = limits();
    if text.trim().chars().count() > limits.max_text_length {
        return false;
    }
    let lowered = text.to_lowercase();
    let mut count = 0;
    for raw in lowered.split_whitespace() {
        let token = strip_punctuation(raw);
        if FILTER_PREFIXES.iter().any(|prefix| token.starts_with(prefix)) {
            return false;
        }
        if !words(token).is_empty() {
            count += 1;
        }
    }
    count >= limits.min_words && translate(text, members, now).is_none()
}

/// The query a model's answer means, or `None`. Only tokens the engine
/// accepts AND the typed text backs survive (a person, a word, a time or a
/// state the sentence does not name is the model's invention), in the phrase
/// grammar's order (is:, status:, by:, the dates, the words); `None` when
/// nothing is left or nothing changed against `text`. `now` = epoch ms.
pub fn sanitize(output: &str, text: &str, members: &[PhraseMember<'_>], now: i64) -> Option<String> {
    let limits = limits();
    let members = members_of(members);
    let is_member = |word: &str| {
        members
            .iter()
            .any(|member| member.words.iter().any(|w| w == word) || (!member.local.is_empty() && member.local == word))
    };

    // What the typed text names: its words, a time, a state.
    let lowered = text.to_lowercase();
    let text_words = words(&lowered);
    let names = |hints: &str, digits: bool| {
        lowered.split_whitespace().map(strip_punctuation).any(|token| {
            (digits && token.bytes().any(|b| b.is_ascii_digit())) || hints.split(' ').any(|hint| hint == token)
        })
    };
    let names_time = names(&SPEC.hints.time, true);
    let names_state = names(&SPEC.hints.state, false);
    // A word of the answer is the text's when one starts with the other
    // (`crash` for `crashes`); a text word under three letters is no stem.
    let in_text = |token: &str| {
        let parts = words(token);
        !parts.is_empty()
            && parts.iter().all(|word| {
                text_words
                    .iter()
                    .any(|typed| typed.starts_with(word) || (typed.chars().count() >= 3 && word.starts_with(typed)))
            })
    };

    let line = output.split('\n').map(clean_line).find(|line| !line.is_empty()).unwrap_or("");

    let mut states: Vec<String> = Vec::new();
    let mut statuses: Vec<String> = Vec::new();
    let mut creators: Vec<String> = Vec::new();
    let mut dates: Vec<String> = Vec::new();
    let mut plain: Vec<String> = Vec::new();
    let mut add_plain = |token: &str| {
        let word = strip_punctuation(token);
        if !in_text(word) || word.chars().count() > limits.max_word_length {
            return;
        }
        if plain.len() >= limits.max_words && !plain.iter().any(|w| w == word) {
            return;
        }
        add(&mut plain, word.to_string());
    };

    for token in tokens(line).iter().take(limits.max_tokens) {
        if let Some(value) = token.strip_prefix("is:") {
            // One state at most: two would never match together.
            if (value == "open" || value == "closed") && names_state && states.is_empty() {
                states.push(value.to_string());
            }
        } else if let Some(value) = token.strip_prefix("status:") {
            let valid = is_word(value) && value.chars().count() <= limits.max_word_length;
            if valid && names_state && statuses.is_empty() {
                statuses.push(value.to_string());
            }
        } else if let Some(value) = token.strip_prefix("by:") {
            let value = value.strip_prefix('@').unwrap_or(value);
            // A name that is no member is a plain word, like in the grammar.
            if is_member(value) && in_text(value) {
                add(&mut creators, value.to_string());
            } else if !value.contains(':') {
                add_plain(value);
            }
        } else if let Some(value) = token.strip_prefix("created:") {
            if names_time && parse_range(value, now).is_some() {
                add(&mut dates, token.clone());
            }
        } else if let Some(value) = token.strip_prefix("updated:") {
            if names_time && parse_range(value, now).is_some() {
                add(&mut dates, token.clone());
            }
        } else if !token.contains(':') {
            add_plain(token);
        }
    }

    let query = states
        .iter()
        .map(|state| format!("is:{state}"))
        .chain(statuses.iter().map(|status| format!("status:{status}")))
        .chain(creators.iter().map(|creator| format!("by:{creator}")))
        .chain(dates)
        .chain(plain)
        .collect::<Vec<_>>()
        .join(" ");
    if query.is_empty() || query == tokens(text).join(" ") {
        return None;
    }
    Some(query)
}

/// The members' FIRST names (lowercased, no repeats, at most `max_names`):
/// all the prompt tells a model about the team. Never an email.
fn first_names(members: &[PhraseMember<'_>]) -> Vec<String> {
    let limits = limits();
    let mut names: Vec<String> = Vec::new();
    for member in members_of(members) {
        let Some(first) = member.words.into_iter().next() else { continue };
        if first.chars().count() > limits.max_word_length || names.contains(&first) {
            continue;
        }
        if names.len() >= limits.max_names {
            break;
        }
        names.push(first);
    }
    names
}

/// THE prompt (desktop and iOS send this exact string): the token syntax, the
/// members' first names, today's UTC date, "one line, the query only", German
/// and English examples, then the sentence (whitespace collapsed, capped).
/// `now` = epoch ms.
pub fn prompt(text: &str, members: &[PhraseMember<'_>], now: i64) -> String {
    let (year, month, day) = civil_from_days(now.div_euclid(DAY_MS));
    let names = first_names(members);
    let sentence: String = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limits().max_text_length)
        .collect();
    let mut lines: Vec<String> = vec![
        "Translate the sentence into one issue search query.".into(),
        "Tokens:".into(),
        "is:open or is:closed, at most one".into(),
        "status:<word>, at most one of backlog, progress, review, done, cancelled, duplicate".into(),
    ];
    if !names.is_empty() {
        lines.push(format!(
            "by:<name> for who created the issue, only when the sentence names one of {}",
            names.join(", ")
        ));
    }
    lines.extend([
        "created:<range> or updated:<range>, only when the sentence names a time; a range is 7d, 2w, 1m, 1y, today, yesterday, YYYY-MM-DD, YYYY-MM, >=YYYY-MM-DD, <YYYY-MM-DD or YYYY-MM-DD..YYYY-MM-DD".to_string(),
        "any other word is a plain search word: keep only the words that name the topic, as the sentence spells them".to_string(),
        format!("Use no other tokens. Never add a person, a status or a time the sentence does not name. Today is {year:04}-{month:02}-{day:02} (UTC)."),
        "Answer with one line, the query only.".to_string(),
        "Examples:".to_string(),
        "offene Issues zum Editor aus letzter Woche -> is:open created:7d editor".to_string(),
        "which crashes did we close since yesterday -> is:closed updated:yesterday crashes".to_string(),
        "was wurde im letzten Monat zur Abrechnung erledigt -> status:done updated:1m abrechnung".to_string(),
        "everything in review about the login page -> status:review login page".to_string(),
        "why is the export to csv so slow -> export csv slow".to_string(),
        "welche Fehler beim Drucken von Rechnungen sind noch offen -> is:open fehler drucken rechnungen".to_string(),
    ]);
    if let Some(first) = names.first() {
        lines.push(format!("what did {first} file about sync this year -> by:{first} created:1y sync"));
    }
    lines.push(format!("Sentence: {sentence}"));
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::issue_search::parse_instant;

    /// The ONE contract fixture, replayed on the clients that carry the
    /// fallback (web `issue-search-translation.test.ts`, iOS
    /// `IssueSearchTranslationTests`): the SAME case names everywhere.
    #[derive(Deserialize)]
    struct FixtureMember {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        email: Option<String>,
    }

    #[derive(Deserialize)]
    struct FixtureCase {
        name: String,
        now: String,
        members: Vec<FixtureMember>,
        text: String,
        #[serde(default)]
        output: Option<String>,
        expected: serde_json::Value,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Fixture {
        wants_cases: Vec<FixtureCase>,
        sanitize_cases: Vec<FixtureCase>,
        prompt_cases: Vec<FixtureCase>,
    }

    fn fixture() -> Fixture {
        serde_json::from_str(FIXTURE).expect("the issue-search-translation fixture parses")
    }

    fn replay(cases: &[FixtureCase], run: impl Fn(&FixtureCase, &[PhraseMember<'_>], i64) -> serde_json::Value) {
        assert!(!cases.is_empty());
        for case in cases {
            let members: Vec<PhraseMember<'_>> = case
                .members
                .iter()
                .map(|m| PhraseMember { name: m.name.as_deref(), email: m.email.as_deref() })
                .collect();
            let now = parse_instant(&case.now).expect("the case's now parses");
            assert_eq!(run(case, &members, now), case.expected, "fixture case: {}", case.name);
        }
    }

    #[test]
    fn issue_search_translation_wants_cases() {
        replay(&fixture().wants_cases, |case, members, now| wants(&case.text, members, now).into());
    }

    #[test]
    fn issue_search_translation_sanitize_cases() {
        replay(&fixture().sanitize_cases, |case, members, now| {
            sanitize(case.output.as_deref().expect("a sanitize case has an output"), &case.text, members, now).into()
        });
    }

    #[test]
    fn issue_search_translation_prompt_cases() {
        replay(&fixture().prompt_cases, |case, members, now| prompt(&case.text, members, now).into());
    }

    #[test]
    fn the_limits_are_the_decided_ones() {
        assert_eq!(limits().min_words, 4);
        assert_eq!(limits().timeout_seconds, 15);
        assert_eq!(limits().model, "haiku");
        assert!(!system().is_empty());
    }
}
