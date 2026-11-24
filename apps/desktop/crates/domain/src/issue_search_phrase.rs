//! #NEW-133: the natural-language PHRASE GRAMMAR of the issue search. A
//! sentence (`offene Issues von Lukas zum Editor aus letzter Woche`) becomes
//! the structured query the engine already runs (`is:open by:lukas created:7d
//! editor`, [`crate::issue_search`]). Deterministic, no model, German +
//! English. Hand-mirrored from web `translateIssueSearchPhrase`
//! (`apps/web/src/lib/issue-search-phrase.ts`, which owns the WORD LISTS,
//! byte for byte the same here), byte-locked by
//! `packages/domain-contract/fixtures/issue-search-phrase.json` (its
//! `$comment` is the spec; read it before changing a rule).

use crate::issue_search::{parse_search_date, words, DAY_MS};

/// One team member: a name after `von`/`by` that is one of them turns into
/// `by:`.
#[derive(Debug, Clone, Copy, Default)]
pub struct PhraseMember<'a> {
    pub name: Option<&'a str>,
    pub email: Option<&'a str>,
}

const FILTER_PREFIXES: [&str; 6] = ["by:", "is:", "status:", "created:", "updated:", "similar:"];
const STRIP: &str = ".,;:!?\"'()";

const OPEN: &str = "open offen offene offenen offener offenes unresolved ungelöst ungelöste ungelösten unfinished unerledigt unerledigte unerledigten";
const CLOSED: &str = "closed geschlossen geschlossene geschlossenen resolved gelöst gelöste gelösten";
const STATUS_WORDS: [(&str, &str); 4] = [
    ("backlog", "backlog"),
    (
        "done",
        "done erledigt erledigte erledigten fertig fertige fertigen completed finished abgeschlossen abgeschlossene abgeschlossenen",
    ),
    (
        "cancelled",
        "cancelled canceled abgebrochen abgebrochene abgebrochenen storniert stornierte stornierten",
    ),
    ("duplicate", "duplicate duplicates duplikat duplikate duplikaten"),
];
/// Two-word statuses: `in progress`, `in arbeit`, `in review`.
const STATUS_PAIRS: [(&str, &str, &str); 5] = [
    ("in", "progress", "progress"),
    ("in", "arbeit", "progress"),
    ("in", "bearbeitung", "progress"),
    ("in", "review", "review"),
    ("im", "review", "review"),
];
const CREATOR_LEAD: &str = "von vom by from";
/// A creator lead-in that is dropped even when no member follows.
const CREATOR_DROP: &str = "von vom by";
const DATE_LEAD: &str =
    "aus seit since from in im during within während innerhalb von vom der dem den des the";
const MONTH_LEAD: &str = "in im from since seit aus during während";
const LAST: &str = "letzter letzten letzte letztem letztes last past previous vergangene vergangenen vergangener vergangenes";
const THIS: &str = "diese dieser diesen diesem dieses this current aktuelle aktuellen aktueller";
const UNITS: [(&str, &str); 4] = [
    ("d", "tag tage tagen day days"),
    ("w", "woche wochen week weeks"),
    ("m", "monat monate monaten month months"),
    ("y", "jahr jahre jahren year years"),
];
const TODAY: &str = "heute today";
const YESTERDAY: &str = "gestern yesterday";
const FROM: &str = "seit since ab after nach from";
const UNTIL: &str = "vor before bis until";
const MONTHS: [&str; 12] = [
    "januar january",
    "februar february",
    "märz march",
    "april",
    "mai may",
    "juni june",
    "juli july",
    "august",
    "september",
    "oktober october",
    "november",
    "dezember december",
];
const UPDATED: &str = "updated changed modified edited geändert geänderte geänderten aktualisiert aktualisierte aktualisierten bearbeitet bearbeitete bearbeiteten";
const NOUNS: &str = "issue issues ticket tickets aufgabe aufgaben task tasks vorgang vorgänge";
const NOISE: &str = concat!(
    "created erstellt erstellte erstellten angelegt angelegte angelegten filed opened eröffnet gemeldet gemeldete gemeldeten reported ",
    "alle all zeige zeig show me mir find finde suche such search list liste the der die das den dem des a an ein eine einen einem einer ",
    "that which were was wurden wurde sind are is ist hat haben have has been worden und and mit with zum zur zu über about regarding on ",
    "for für wegen bezüglich in im aus from of to welche welcher bitte please give gib any es gibt there",
);

/// Is `token` one of the space-separated `words`?
fn has(words: &str, token: &str) -> bool {
    words.split(' ').any(|word| word == token)
}

fn unit_of(token: Option<&str>) -> Option<&'static str> {
    let token = token?;
    UNITS.iter().find(|(_, words)| has(words, token)).map(|(unit, _)| *unit)
}

/// 1..=12, or 0 when `token` is no month name.
fn month_of(token: Option<&str>) -> i64 {
    let Some(token) = token else { return 0 };
    MONTHS.iter().position(|names| has(names, token)).map_or(0, |ix| ix as i64 + 1)
}

fn is_year(token: Option<&str>) -> bool {
    token.is_some_and(|t| t.len() == 4 && t.bytes().all(|b| b.is_ascii_digit()))
}

/// A count `1..=9999` written in 1 to 4 ASCII digits.
fn count_of(token: Option<&str>) -> Option<i64> {
    let token = token?;
    if !(1..=4).contains(&token.len()) || !token.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    token.parse::<i64>().ok().filter(|n| *n >= 1)
}

/// The UTC civil date of a day number (Howard Hinnant's civil_from_days).
pub(crate) fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month, doy - (153 * mp + 2) / 5 + 1)
}

/// A date phrase starting at `tokens[i]`: its `created:`/`updated:` value
/// and how many tokens it spans, or `None`.
fn date_at(tokens: &[String], i: usize, today: i64) -> Option<(String, usize)> {
    let at = |ix: usize| tokens.get(ix).map(String::as_str);
    let t = tokens[i].as_str();
    let next = at(i + 1);
    if has(TODAY, t) {
        return Some(("today".to_string(), 1));
    }
    if has(YESTERDAY, t) {
        return Some(("yesterday".to_string(), 1));
    }
    if has(LAST, t) {
        if let Some(unit) = unit_of(next) {
            let value = if unit == "w" { "7d".to_string() } else { format!("1{unit}") };
            return Some((value, 2));
        }
        if let (Some(count), Some(unit)) = (count_of(next), unit_of(at(i + 2))) {
            return Some((format!("{count}{unit}"), 3));
        }
        return None;
    }
    if has(THIS, t) {
        let (year, month, _) = civil_from_days(today);
        return match unit_of(next) {
            Some("d") => Some(("today".to_string(), 2)),
            Some("w") => {
                // Monday of the current UTC week (1970-01-01 was a Thursday).
                let (y, m, d) = civil_from_days(today - (today + 3).rem_euclid(7));
                Some((format!(">={y:04}-{m:02}-{d:02}"), 2))
            }
            Some("m") => Some((format!("{year:04}-{month:02}"), 2)),
            Some("y") => Some((format!(">={year:04}-01-01"), 2)),
            _ => None,
        };
    }
    if let Some(next) = next.filter(|next| parse_search_date(next).is_some()) {
        if has(FROM, t) {
            return Some((format!(">={next}"), 2));
        }
        if has(UNTIL, t) {
            return Some((format!("<{next}"), 2));
        }
    }
    if parse_search_date(t).is_some() {
        return Some((t.to_string(), 1));
    }
    // A month name counts only after a lead-in (`im Mai`, `in may`) or before
    // a year (`mai 2026`): `may` and `march` are words too.
    let month = month_of(next);
    if has(MONTH_LEAD, t) && month > 0 {
        if is_year(at(i + 2)) {
            return Some((format!("{}-{month:02}", tokens[i + 2]), 3));
        }
        let (year, now_month, _) = civil_from_days(today);
        let year = if month > now_month { year - 1 } else { year };
        return Some((format!("{year:04}-{month:02}"), 2));
    }
    let bare = month_of(Some(t));
    if bare > 0 && is_year(next) {
        return Some((format!("{}-{bare:02}", tokens[i + 1]), 2));
    }
    None
}

fn add(list: &mut Vec<String>, value: &str) {
    if !list.iter().any(|have| have == value) {
        list.push(value.to_string());
    }
}

/// The structured query a sentence means, or `None` when the grammar does not
/// recognise one (plain words, a query that already carries a filter token,
/// or nothing that would change). `now` = epoch ms.
pub fn translate(text: &str, members: &[PhraseMember<'_>], now: i64) -> Option<String> {
    let today = now.div_euclid(DAY_MS);

    let lowered = text.to_lowercase();
    let mut tokens: Vec<String> = Vec::new();
    for raw in lowered.split_whitespace() {
        let token = raw.trim_matches(|c: char| STRIP.contains(c));
        if token.is_empty() {
            continue;
        }
        if FILTER_PREFIXES.iter().any(|prefix| token.starts_with(prefix)) {
            return None;
        }
        tokens.push(token.to_string());
    }

    // Each member's name words and email local part, lowercased.
    struct Member {
        words: Vec<String>,
        local: String,
    }
    let members: Vec<Member> = members
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
        .collect();
    let is_of = |member: &Member, token: &str| {
        member.words.iter().any(|word| word == token) || member.local == token
    };

    let mut states: Vec<String> = Vec::new();
    let mut statuses: Vec<String> = Vec::new();
    let mut creators: Vec<String> = Vec::new();
    let mut dates: Vec<String> = Vec::new();
    let mut plain: Vec<String> = Vec::new();
    let mut updated = false;
    let mut noun = false;
    let mut strong = 0;
    let mut weak = 0;

    let mut i = 0;
    while i < tokens.len() {
        let t = tokens[i].as_str();
        let next = tokens.get(i + 1).map(String::as_str);

        // 1. A date phrase, behind up to two lead-in words (`aus der letzten Woche`).
        let mut lead = 0;
        let mut date = date_at(&tokens, i, today);
        while date.is_none() && lead < 2 && i + lead < tokens.len() && has(DATE_LEAD, &tokens[i + lead]) {
            lead += 1;
            if i + lead < tokens.len() {
                date = date_at(&tokens, i + lead, today);
            }
        }
        if let Some((value, length)) = date {
            add(&mut dates, &value);
            strong += 1;
            i += lead + length;
            continue;
        }

        // 2. A two-word status, then the one-word states and statuses.
        if let Some((_, _, status)) =
            STATUS_PAIRS.iter().find(|(first, second, _)| *first == t && Some(*second) == next)
        {
            add(&mut statuses, status);
            weak += 1;
            i += 2;
            continue;
        }
        if has(OPEN, t) {
            add(&mut states, "open");
            weak += 1;
            i += 1;
            continue;
        }
        if has(CLOSED, t) {
            add(&mut states, "closed");
            weak += 1;
            i += 1;
            continue;
        }
        if let Some((status, _)) = STATUS_WORDS.iter().find(|(_, names)| has(names, t)) {
            add(&mut statuses, status);
            weak += 1;
            i += 1;
            continue;
        }

        // 3. `von|by|from <member>`: every following word that is a name word of
        //    the same member joins (`von Lukas Mai`).
        if has(CREATOR_LEAD, t) {
            let mut matched: Vec<&Member> = match next {
                Some(next) => members.iter().filter(|member| is_of(*member, next)).collect::<Vec<&Member>>(),
                None => Vec::new(),
            };
            if !matched.is_empty() {
                let mut j = i + 1;
                while j < tokens.len() {
                    let narrowed: Vec<&Member> =
                        matched.iter().copied().filter(|member| is_of(*member, &tokens[j])).collect();
                    if narrowed.is_empty() {
                        break;
                    }
                    matched = narrowed;
                    add(&mut creators, &tokens[j]);
                    j += 1;
                }
                strong += 1;
                i = j;
                continue;
            }
            if has(CREATOR_DROP, t) {
                i += 1;
                continue;
            }
        }

        // 4. The date field, the nouns and the filler words.
        if has(UPDATED, t) {
            updated = true;
        } else if has(NOUNS, t) {
            noun = true;
        } else if !has(NOISE, t) {
            plain.push(t.to_string());
        }
        i += 1;
    }

    // A date or a member is a sentence on its own; a state or status word
    // needs the noun beside it (`open issues`, not `open file dialog`).
    if strong == 0 && !(weak > 0 && noun) {
        return None;
    }
    let field = if updated { "updated:" } else { "created:" };
    let query = states
        .iter()
        .map(|state| format!("is:{state}"))
        .chain(statuses.iter().map(|status| format!("status:{status}")))
        .chain(creators.iter().map(|creator| format!("by:{creator}")))
        .chain(dates.iter().map(|date| format!("{field}{date}")))
        .chain(plain)
        .collect::<Vec<_>>()
        .join(" ");
    (query != tokens.join(" ")).then_some(query)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::issue_search::parse_instant;
    use serde::Deserialize;

    /// The ONE contract fixture, replayed on every client (web
    /// `issue-search-phrase.test.ts`, iOS `IssueSearchPhraseTests`, Android
    /// `IssueSearchPhraseTest`): the SAME case names everywhere.
    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/issue-search-phrase.json");

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
        phrase: String,
        expected: Option<String>,
    }

    #[derive(Deserialize)]
    struct Fixture {
        cases: Vec<FixtureCase>,
    }

    #[test]
    fn issue_search_phrase_contract_fixture() {
        let fixture: Fixture =
            serde_json::from_str(FIXTURE).expect("the issue-search-phrase fixture parses");
        assert!(!fixture.cases.is_empty());
        for case in &fixture.cases {
            let members: Vec<PhraseMember<'_>> = case
                .members
                .iter()
                .map(|m| PhraseMember { name: m.name.as_deref(), email: m.email.as_deref() })
                .collect();
            let now = parse_instant(&case.now).expect("the case's now parses");
            assert_eq!(
                translate(&case.phrase, &members, now),
                case.expected,
                "fixture case: {}",
                case.name
            );
        }
    }
}
