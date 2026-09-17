//! EXP-892 — the ONE issue-search engine every client runs, the desktop twin
//! of web `apps/web/src/lib/issue-search.ts` (iOS `IssueSearch.swift`,
//! Android `IssueSearch.kt`), byte-locked by
//! `packages/domain-contract/fixtures/issue-search.json` — same cases, same
//! test names, every platform.
//!
//! Every desktop surface that searches issues — the ⌘K dialog, the `#`
//! autocomplete, the composer's issue picker and the duplicate/relation
//! picker — ranks the locally synced rows with [`rank`] and then splices the
//! server's full-text hits (`issues.search`: title + description + comment
//! bodies, stemmed) in behind them with [`merge_server_hits`].
//!
//! Ranking, per query token (every token must match SOMEWHERE — and
//! semantics; a row's score is the sum of its tokens' best field score):
//!
//!   identifier exact (whole identifier or its number)       100
//!   identifier prefix (whole identifier or its number)       80
//!   identifier substring                                     60
//!   title word prefix                                        50
//!   title substring                                          40
//!   description word prefix                                  20
//!   description substring                                    15
//!
//! Ties order by `updated_at` desc, then `created_at` desc, then identifier
//! number desc, then identifier string desc. An EMPTY query lists the newest
//! CREATED first (the `#` menu's "recent work" list). Queries and tokens drop
//! one leading `#` so `#87` and `fix #87` both find EXP-87.
//!
//! gpui-free, like the rest of `domain`.

use std::cell::OnceCell;
use std::collections::HashSet;

const SCORE_IDENTIFIER_EXACT: i32 = 100;
const SCORE_IDENTIFIER_PREFIX: i32 = 80;
const SCORE_IDENTIFIER_CONTAINS: i32 = 60;
const SCORE_TITLE_WORD_PREFIX: i32 = 50;
const SCORE_TITLE_CONTAINS: i32 = 40;
const SCORE_DESCRIPTION_WORD_PREFIX: i32 = 20;
const SCORE_DESCRIPTION_CONTAINS: i32 = 15;

/// The `limit` every consumer gets without asking (web
/// `ISSUE_SEARCH_DEFAULT_LIMIT`).
pub const DEFAULT_LIMIT: usize = 30;

/// A BORROWED view of one searchable row — the engine never owns or clones a
/// row, so every consumer keeps its own snapshot type (`Issue`, the composer's
/// `IssueRow`, a server hit) and hands the engine a view of it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SearchRow<'a> {
    pub id: &'a str,
    pub identifier: &'a str,
    pub title: &'a str,
    /// The markdown body, when the consumer has it synced.
    pub description: Option<&'a str>,
    /// ISO-8601 / Electric timestamptz text; unparseable or missing = oldest.
    pub created_at: Option<&'a str>,
    pub updated_at: Option<&'a str>,
}

impl<'a> SearchRow<'a> {
    /// The minimum a row needs (a server hit that never synced carries no
    /// description and no timestamps).
    pub fn new(id: &'a str, identifier: &'a str, title: &'a str) -> Self {
        Self {
            id,
            identifier,
            title,
            description: None,
            created_at: None,
            updated_at: None,
        }
    }

    pub fn with_description(mut self, description: Option<&'a str>) -> Self {
        self.description = description;
        self
    }

    pub fn with_times(mut self, created_at: Option<&'a str>, updated_at: Option<&'a str>) -> Self {
        self.created_at = created_at;
        self.updated_at = updated_at;
        self
    }
}

/// The synced row's view — every surface that searches the `issues`
/// collection ranks through this, so identifier, title, description and both
/// timestamps are always in play.
impl<'a> From<&'a crate::rows::Issue> for SearchRow<'a> {
    fn from(issue: &'a crate::rows::Issue) -> Self {
        Self {
            id: &issue.id,
            identifier: &issue.identifier,
            title: &issue.title,
            description: issue.description.as_deref(),
            created_at: issue.created_at.as_deref(),
            updated_at: issue.updated_at.as_deref(),
        }
    }
}

/// Trim, drop ONE leading `#`, lowercase. Empty = "recent work".
pub fn normalize_query(query: &str) -> String {
    let trimmed = query.trim();
    let trimmed = trimmed.strip_prefix('#').map(str::trim).unwrap_or(trimmed);
    trimmed.to_lowercase()
}

/// Whitespace-separated tokens of the normalized query, each shorn of a
/// leading `#`, empties dropped.
pub fn tokens(query: &str) -> Vec<String> {
    normalize_query(query)
        .split_whitespace()
        .map(|token| token.strip_prefix('#').unwrap_or(token))
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

/// Web `words()` — split on every non-alphanumeric char, empties dropped.
fn words(text: &str) -> Vec<&str> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect()
}

fn is_digits(token: &str) -> bool {
    !token.is_empty() && token.bytes().all(|b| b.is_ascii_digit())
}

/// The digits after the identifier's last `-` (`EXP-87` → `87`), or `None`.
fn identifier_tail(identifier: &str) -> Option<&str> {
    let dash = identifier.rfind('-')?;
    let tail = &identifier[dash + 1..];
    is_digits(tail).then_some(tail)
}

/// The numeric tail as a number for ordering (`EXP-87` → 87), -1 when none.
pub fn identifier_number(identifier: &str) -> i64 {
    identifier_tail(identifier)
        .and_then(|tail| tail.parse::<i64>().ok())
        .unwrap_or(-1)
}

/// A row lowercased + word-split once, so an N-token query costs one pass.
/// The DESCRIPTION is lowercased and word-split LAZILY (the Android mirror's
/// rule): a token that already matched the identifier or the title never
/// touches it, and a description is the only unbounded field a ranked pool
/// carries — every keystroke prepares the whole pool.
struct Prepared<'a> {
    identifier: String,
    number: Option<String>,
    title: String,
    title_words: Vec<String>,
    raw_description: &'a str,
    description: OnceCell<String>,
    description_words: OnceCell<Vec<String>>,
}

impl Prepared<'_> {
    fn description(&self) -> &str {
        self.description.get_or_init(|| self.raw_description.to_lowercase())
    }

    fn description_words(&self) -> &[String] {
        self.description_words
            .get_or_init(|| words(self.description()).into_iter().map(str::to_string).collect())
    }
}

fn prepare<'a>(row: &SearchRow<'a>) -> Prepared<'a> {
    let title = row.title.to_lowercase();
    Prepared {
        identifier: row.identifier.to_lowercase(),
        number: identifier_tail(row.identifier).map(str::to_string),
        title_words: words(&title).into_iter().map(str::to_string).collect(),
        title,
        raw_description: row.description.unwrap_or(""),
        description: OnceCell::new(),
        description_words: OnceCell::new(),
    }
}

/// The best field score of `token` against a prepared row, or `None` when the
/// token matches nothing.
fn token_score(row: &Prepared<'_>, token: &str) -> Option<i32> {
    let number = row.number.as_deref();
    if row.identifier == token || number == Some(token) {
        return Some(SCORE_IDENTIFIER_EXACT);
    }
    if row.identifier.starts_with(token)
        || (is_digits(token) && number.is_some_and(|number| number.starts_with(token)))
    {
        return Some(SCORE_IDENTIFIER_PREFIX);
    }
    if row.identifier.contains(token) {
        return Some(SCORE_IDENTIFIER_CONTAINS);
    }
    if row.title_words.iter().any(|word| word.starts_with(token)) {
        return Some(SCORE_TITLE_WORD_PREFIX);
    }
    if row.title.contains(token) {
        return Some(SCORE_TITLE_CONTAINS);
    }
    if row
        .description_words()
        .iter()
        .any(|word| word.starts_with(token))
    {
        return Some(SCORE_DESCRIPTION_WORD_PREFIX);
    }
    if row.description().contains(token) {
        return Some(SCORE_DESCRIPTION_CONTAINS);
    }
    None
}

/// The row's total score for `query_tokens`, or `None` when any token misses.
pub fn score(row: &SearchRow<'_>, query_tokens: &[String]) -> Option<i32> {
    let prepared = prepare(row);
    let mut total = 0;
    for token in query_tokens {
        total += token_score(&prepared, token)?;
    }
    Some(total)
}

/// Rank the locally synced `rows` for `query` and return their INDICES into
/// `rows` — the scored, ordered, capped list described at the top of this
/// file. Stable for equal keys. `exclude` holds ids that never appear;
/// `limit` caps the result (`usize::MAX` = uncapped).
pub fn rank(
    rows: &[SearchRow<'_>],
    query: &str,
    limit: usize,
    exclude: &HashSet<String>,
) -> Vec<usize> {
    let query_tokens = tokens(query);
    let pool = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| !exclude.contains(row.id));
    if query_tokens.is_empty() {
        let mut ordered: Vec<(usize, &SearchRow<'_>)> = pool.collect();
        ordered.sort_by(|(_, a), (_, b)| compare_created(a, b));
        ordered.truncate(limit);
        return ordered.into_iter().map(|(ix, _)| ix).collect();
    }
    let mut scored: Vec<(usize, &SearchRow<'_>, i32)> = pool
        .filter_map(|(ix, row)| score(row, &query_tokens).map(|score| (ix, row, score)))
        .collect();
    scored.sort_by(|(_, a, a_score), (_, b, b_score)| {
        b_score
            .cmp(a_score)
            .then_with(|| compare_recency(a, b))
    });
    scored.truncate(limit);
    scored.into_iter().map(|(ix, _, _)| ix).collect()
}

/// Splice the server's full-text hits in behind the locally ranked rows:
/// local order first, then every hit not already listed, in the server's
/// relevance order, deduped by id. `resolve` turns a hit into a renderable
/// row — the synced row when the id is local, a stand-in built from the hit's
/// own fields where the consumer can render one, or `None` to drop it (a
/// picker whose pool is a subset of the team's issues never widens). The
/// `limit` spans both halves.
pub fn merge_server_hits<T, H>(
    local: Vec<T>,
    hits: &[H],
    limit: usize,
    exclude: &HashSet<String>,
    local_id: impl Fn(&T) -> &str,
    hit_id: impl Fn(&H) -> &str,
    mut resolve: impl FnMut(&H) -> Option<T>,
) -> Vec<T> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut merged: Vec<T> = Vec::new();
    for row in local {
        let id = local_id(&row).to_string();
        if exclude.contains(&id) || seen.contains(&id) {
            continue;
        }
        seen.insert(id);
        merged.push(row);
        if merged.len() >= limit {
            return merged;
        }
    }
    for hit in hits {
        let id = hit_id(hit);
        if exclude.contains(id) || seen.contains(id) {
            continue;
        }
        let Some(row) = resolve(hit) else {
            continue;
        };
        seen.insert(id.to_string());
        merged.push(row);
        if merged.len() >= limit {
            break;
        }
    }
    merged
}

// ---------------------------------------------------------------------------
// Ordering
// ---------------------------------------------------------------------------

fn compare_recency(a: &SearchRow<'_>, b: &SearchRow<'_>) -> std::cmp::Ordering {
    instant_of(b.updated_at)
        .cmp(&instant_of(a.updated_at))
        .then_with(|| instant_of(b.created_at).cmp(&instant_of(a.created_at)))
        .then_with(|| identifier_number(b.identifier).cmp(&identifier_number(a.identifier)))
        .then_with(|| b.identifier.cmp(a.identifier))
}

fn compare_created(a: &SearchRow<'_>, b: &SearchRow<'_>) -> std::cmp::Ordering {
    instant_of(b.created_at)
        .cmp(&instant_of(a.created_at))
        .then_with(|| identifier_number(b.identifier).cmp(&identifier_number(a.identifier)))
        .then_with(|| b.identifier.cmp(a.identifier))
}

/// The web's `Date.parse` in epoch milliseconds — missing or unparseable is
/// the OLDEST possible instant (web `Number.NEGATIVE_INFINITY`).
fn instant_of(value: Option<&str>) -> i64 {
    value.and_then(parse_instant).unwrap_or(i64::MIN)
}

/// `YYYY-MM-DD[(T| )hh:mm[:ss[.fff]][Z|±hh[:mm]]]` → epoch millis. Covers both
/// ISO-8601 (`…T…Z`) and Electric's timestamptz text (`… …+00`), the two
/// forms a row's timestamps ever arrive in.
fn parse_instant(value: &str) -> Option<i64> {
    let value = value.trim();
    let year: i64 = value.get(0..4)?.parse().ok()?;
    if value.get(4..5)? != "-" {
        return None;
    }
    let month: i64 = value.get(5..7)?.parse().ok()?;
    if value.get(7..8)? != "-" {
        return None;
    }
    let day: i64 = value.get(8..10)?.parse().ok()?;
    let mut millis = days_from_civil(year, month, day) * 86_400_000;

    let Some(rest) = value.get(10..).filter(|rest| !rest.is_empty()) else {
        return Some(millis);
    };
    if !matches!(rest.get(0..1)?, "T" | "t" | " ") {
        return None;
    }
    let rest = rest.get(1..)?;
    let hour: i64 = rest.get(0..2)?.parse().ok()?;
    if rest.get(2..3)? != ":" {
        return None;
    }
    let minute: i64 = rest.get(3..5)?.parse().ok()?;
    millis += hour * 3_600_000 + minute * 60_000;
    let mut rest = rest.get(5..)?;

    if rest.starts_with(':') {
        let second: i64 = rest.get(1..3)?.parse().ok()?;
        millis += second * 1_000;
        rest = rest.get(3..)?;
        if let Some(fraction) = rest.strip_prefix('.') {
            let digits: String = fraction.chars().take_while(char::is_ascii_digit).collect();
            if digits.is_empty() {
                return None;
            }
            let mut frac = digits.clone();
            frac.truncate(3);
            while frac.len() < 3 {
                frac.push('0');
            }
            millis += frac.parse::<i64>().ok()?;
            rest = &rest[1 + digits.len()..];
        }
    }

    if rest.is_empty() || rest.eq_ignore_ascii_case("z") {
        return Some(millis);
    }
    // An offset moves the instant the OTHER way (`+02:00` = 2h earlier UTC).
    let sign: i64 = match rest.get(0..1)? {
        "+" => -1,
        "-" => 1,
        _ => return None,
    };
    let rest = rest.get(1..)?;
    let offset_hours: i64 = rest.get(0..2)?.parse().ok()?;
    let offset_minutes: i64 = match rest.get(2..3) {
        Some(":") => rest.get(3..5).and_then(|m| m.parse().ok()).unwrap_or(0),
        Some(_) => rest.get(2..4).and_then(|m| m.parse().ok()).unwrap_or(0),
        None => 0,
    };
    Some(millis + sign * (offset_hours * 3_600_000 + offset_minutes * 60_000))
}

/// Days since the Unix epoch for a civil date (Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    /// The ONE contract fixture, replayed byte-for-byte on every client (web
    /// `issue-search.test.ts`, iOS `IssueSearchTests`, Android
    /// `IssueSearchTest`) — the SAME case names everywhere.
    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/issue-search.json");

    #[derive(Deserialize, Clone)]
    #[serde(rename_all = "camelCase")]
    struct FixtureRow {
        id: String,
        identifier: String,
        title: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        created_at: Option<String>,
        #[serde(default)]
        updated_at: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureHit {
        id: String,
        identifier: String,
        title: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureCase {
        name: String,
        rows: Vec<FixtureRow>,
        query: String,
        #[serde(default)]
        exclude: Option<Vec<String>>,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        server_hits: Option<Vec<FixtureHit>>,
        #[serde(default)]
        allow_unsynced: bool,
        expected: Vec<String>,
    }

    fn view(row: &FixtureRow) -> SearchRow<'_> {
        SearchRow {
            id: &row.id,
            identifier: &row.identifier,
            title: &row.title,
            description: row.description.as_deref(),
            created_at: row.created_at.as_deref(),
            updated_at: row.updated_at.as_deref(),
        }
    }

    /// Every fixture case, replayed with its own name in the failure message
    /// (a Rust test name cannot be a sentence — the fixture's name IS the
    /// case identity across the four clients).
    #[test]
    fn issue_search_contract_fixture() {
        let cases: Vec<FixtureCase> =
            serde_json::from_str(FIXTURE).expect("the issue-search fixture parses");
        assert!(!cases.is_empty());
        for case in &cases {
            let exclude: HashSet<String> =
                case.exclude.clone().unwrap_or_default().into_iter().collect();
            let limit = case.limit.unwrap_or(DEFAULT_LIMIT);
            let views: Vec<SearchRow<'_>> = case.rows.iter().map(view).collect();
            let local: Vec<FixtureRow> = rank(&views, &case.query, limit, &exclude)
                .into_iter()
                .map(|ix| case.rows[ix].clone())
                .collect();
            let merged = match &case.server_hits {
                Some(hits) => merge_server_hits(
                    local,
                    hits,
                    limit,
                    &exclude,
                    |row: &FixtureRow| row.id.as_str(),
                    |hit: &FixtureHit| hit.id.as_str(),
                    |hit: &FixtureHit| {
                        case.rows
                            .iter()
                            .find(|row| row.id == hit.id)
                            .cloned()
                            .or_else(|| {
                                case.allow_unsynced.then(|| FixtureRow {
                                    id: hit.id.clone(),
                                    identifier: hit.identifier.clone(),
                                    title: hit.title.clone(),
                                    description: None,
                                    created_at: None,
                                    updated_at: None,
                                })
                            })
                    },
                ),
                None => local,
            };
            let ids: Vec<&str> = merged.iter().map(|row| row.id.as_str()).collect();
            assert_eq!(ids, case.expected, "case: {}", case.name);
        }
    }

    #[test]
    fn normalizes_a_query() {
        assert_eq!(normalize_query("  #EXP-87  "), "exp-87");
        assert_eq!(normalize_query("#"), "");
    }

    #[test]
    fn tokenizes_on_whitespace_and_drops_per_token_hashes() {
        assert_eq!(tokens("Fix  #87 login"), vec!["fix", "87", "login"]);
        assert!(tokens("#").is_empty());
    }

    #[test]
    fn is_stable_for_equal_keys() {
        let rows = [
            SearchRow::new("a", "X-1", "same"),
            SearchRow::new("b", "X-1", "same"),
        ];
        assert_eq!(
            rank(&rows, "same", DEFAULT_LIMIT, &HashSet::new()),
            vec![0, 1]
        );
    }

    #[test]
    fn parses_both_timestamp_forms_to_the_same_instant() {
        let iso = parse_instant("2026-01-01T00:00:00Z").expect("iso");
        let electric = parse_instant("2026-01-01 00:00:00+00").expect("electric");
        assert_eq!(iso, electric);
        // 1767225600000 = 2026-01-01T00:00:00Z.
        assert_eq!(iso, 1_767_225_600_000);
        // Offsets move the instant; fractions count.
        assert_eq!(
            parse_instant("2026-01-01T02:00:00+02:00").expect("offset"),
            iso
        );
        assert_eq!(
            parse_instant("2026-01-01T00:00:00.250Z").expect("fraction"),
            iso + 250
        );
        // A date alone is midnight UTC; garbage is unparseable.
        assert_eq!(parse_instant("2026-01-01").expect("date"), iso);
        assert!(parse_instant("not a date").is_none());
    }

    #[test]
    fn missing_timestamps_sort_oldest() {
        let rows = [
            SearchRow::new("undated", "EXP-9", "login"),
            SearchRow::new("dated", "EXP-1", "login")
                .with_times(Some("2020-01-01T00:00:00Z"), Some("2020-01-01T00:00:00Z")),
        ];
        assert_eq!(rank(&rows, "login", DEFAULT_LIMIT, &HashSet::new()), vec![1, 0]);
    }

    #[test]
    fn descriptions_are_searched_and_excluded_ids_drop_out() {
        let rows = [
            SearchRow::new("a", "EXP-1", "Nothing").with_description(Some("a tsvector index")),
            SearchRow::new("b", "EXP-2", "tsvector drift"),
        ];
        let excluded: HashSet<String> = ["b".to_string()].into_iter().collect();
        assert_eq!(
            rank(&rows, "tsvector", DEFAULT_LIMIT, &HashSet::new()),
            vec![1, 0]
        );
        assert_eq!(rank(&rows, "tsvector", DEFAULT_LIMIT, &excluded), vec![0]);
    }
}
