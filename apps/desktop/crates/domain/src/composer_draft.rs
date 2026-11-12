//! #FEED-219: every composer's synced DRAFT ×4. Pure and locked by
//! `packages/domain-contract/fixtures/composer-draft.json` (`keys`,
//! `unparsable`, `writes`, `fields`, `debounceMs`; its `$comment` holds the
//! rules); mirrors TS `src/composer-draft.ts`.
//!
//! [`DraftWriter`] is the desktop's writer state, clock-injected so the
//! debounce and the flushes are testable without gpui (web
//! `lib/composer-drafts.ts`'s twin): an edit is written [`DEBOUNCE`] after the
//! last keystroke, at once when the composer goes away or a send settles; its
//! text stays the key's LOCAL copy until the shape echoes it back.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// The fixture's `debounceMs`.
pub const DEBOUNCE_MS: u64 = 800;
pub const DEBOUNCE: Duration = Duration::from_millis(DEBOUNCE_MS);

/// The target a composer writes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// The issue's bottom comment composer (id = the issue).
    Comment,
    /// The reply composer under a comment card (id = the thread's top comment).
    Reply,
    /// A session's composer (a chat, an agent's conversation, an app editor's session).
    Session,
    /// An agent's composer before its conversation exists.
    Agent,
    /// An app editor's composer before its first session.
    App,
    /// The team's New chat composer (id = the team).
    NewChat,
}

impl Kind {
    pub const ALL: [Kind; 6] = [Kind::Comment, Kind::Reply, Kind::Session, Kind::Agent, Kind::App, Kind::NewChat];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Comment => "comment",
            Kind::Reply => "reply",
            Kind::Session => "session",
            Kind::Agent => "agent",
            Kind::App => "app",
            Kind::NewChat => "newChat",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Target {
    pub kind: Kind,
    pub id: String,
}

impl Target {
    pub fn new(kind: Kind, id: impl Into<String>) -> Self {
        Self { kind, id: id.into() }
    }

    /// The row's `key`: `<kind>:<id>`.
    pub fn key(&self) -> String {
        format!("{}:{}", self.kind.as_str(), self.id)
    }
}

/// `<kind>:<id>` back to its target; an unknown kind or a missing id = None
/// (a client ignores that row).
pub fn parse_key(key: &str) -> Option<Target> {
    let (kind, id) = key.split_once(':')?;
    if id.is_empty() {
        return None;
    }
    let kind = Kind::ALL.into_iter().find(|k| k.as_str() == kind)?;
    Some(Target::new(kind, id))
}

/// What writing the field's text does to the row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Write {
    Upsert(String),
    Delete,
}

/// Empty or whitespace-only deletes; anything else upserts verbatim.
pub fn write_for(text: &str) -> Write {
    if text.trim().is_empty() {
        Write::Delete
    } else {
        Write::Upsert(text.to_string())
    }
}

/// What the field shows: an edited field is the person's; else this client's
/// own unechoed write, else the synced row, else empty.
pub fn field(field: &str, edited: bool, local: Option<&str>, synced: Option<&str>) -> String {
    if edited {
        return field.to_string();
    }
    local.or(synced).unwrap_or_default().to_string()
}

/// One write the caller performs (`composerDrafts.upsert` / `.delete`), then
/// reports with [`DraftWriter::settled`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteJob {
    pub key: String,
    pub team_id: String,
    /// The field's text this job writes (what `settled` gets back).
    pub text: String,
    pub write: Write,
}

#[derive(Clone, Debug)]
struct Entry {
    team_id: String,
    /// The newest text for the key, written or not.
    text: String,
    /// When the debounced write falls due (None = nothing scheduled).
    due_at: Option<Instant>,
    /// A write is in flight.
    writing: bool,
    /// The text has not been written yet.
    dirty: bool,
}

/// Every composer's pending draft writes, per key.
#[derive(Clone, Debug, Default)]
pub struct DraftWriter {
    entries: HashMap<String, Entry>,
}

impl DraftWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// The key's own write not yet echoed back (the fixture's `local`).
    pub fn local(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(|e| e.text.as_str())
    }

    /// Record the field's text: written [`DEBOUNCE`] after `now`.
    pub fn edit(&mut self, key: &str, team_id: &str, text: &str, now: Instant) {
        let entry = self.entries.entry(key.to_string()).or_insert_with(|| Entry {
            team_id: team_id.to_string(),
            text: String::new(),
            due_at: None,
            writing: false,
            dirty: false,
        });
        entry.team_id = team_id.to_string();
        entry.text = text.to_string();
        entry.dirty = true;
        entry.due_at = Some(now + DEBOUNCE);
    }

    /// The earliest scheduled write (the caller's timer).
    pub fn next_due(&self) -> Option<Instant> {
        self.entries.values().filter(|e| !e.writing).filter_map(|e| e.due_at).min()
    }

    /// The writes whose debounce has run out by `now`.
    pub fn due(&mut self, now: Instant) -> Vec<WriteJob> {
        let keys: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, e)| !e.writing && e.due_at.is_some_and(|at| at <= now))
            .map(|(k, _)| k.clone())
            .collect();
        keys.iter().filter_map(|k| self.flush(k)).collect()
    }

    /// Write the key's pending text now (the composer goes away, a send
    /// settled). None = nothing due or a write already in flight (it is
    /// re-flushed when that one settles).
    pub fn flush(&mut self, key: &str) -> Option<WriteJob> {
        let entry = self.entries.get_mut(key)?;
        entry.due_at = None;
        if entry.writing || !entry.dirty {
            return None;
        }
        entry.dirty = false;
        entry.writing = true;
        Some(WriteJob {
            key: key.to_string(),
            team_id: entry.team_id.clone(),
            text: entry.text.clone(),
            write: write_for(&entry.text),
        })
    }

    /// Every pending write now (the window goes away).
    pub fn flush_all(&mut self) -> Vec<WriteJob> {
        let keys: Vec<String> = self.entries.keys().cloned().collect();
        keys.iter().filter_map(|k| self.flush(k)).collect()
    }

    /// A job came back. A failed write is silent: the text stays dirty and
    /// the next write retries. Returns the follow-up write when the person
    /// typed while this one was in flight and no debounce is pending.
    pub fn settled(&mut self, job: &WriteJob, ok: bool) -> Option<WriteJob> {
        let entry = self.entries.get_mut(&job.key)?;
        entry.writing = false;
        if !ok {
            entry.dirty = true;
            return None;
        }
        if entry.dirty && entry.due_at.is_none() {
            return self.flush(&job.key);
        }
        None
    }

    /// The shape's row for `key` now (None = no row). Once it matches the
    /// written text and nothing else is pending, the local copy is dropped.
    pub fn observe_synced(&mut self, key: &str, synced: Option<&str>) {
        let Some(entry) = self.entries.get(key) else {
            return;
        };
        if entry.writing || entry.dirty || entry.due_at.is_some() {
            return;
        }
        let echoed = match write_for(&entry.text) {
            Write::Upsert(text) => synced == Some(text.as_str()),
            Write::Delete => synced.is_none(),
        };
        if echoed {
            self.entries.remove(key);
        }
    }

    /// Keys holding a local copy (the caller re-checks them on a shape delta).
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/composer-draft.json");

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Fixture {
        debounce_ms: u64,
        keys: Vec<KeyCase>,
        unparsable: Vec<String>,
        writes: Vec<WriteCase>,
        fields: Vec<FieldCase>,
    }
    #[derive(Deserialize)]
    struct KeyCase {
        name: String,
        target: TargetJson,
        key: String,
    }
    #[derive(Deserialize)]
    struct TargetJson {
        kind: String,
        id: String,
    }
    #[derive(Deserialize)]
    struct WriteCase {
        name: String,
        text: String,
        expected: WriteJson,
    }
    #[derive(Deserialize)]
    struct WriteJson {
        op: String,
        #[serde(default)]
        text: Option<String>,
    }
    #[derive(Deserialize)]
    struct FieldCase {
        name: String,
        input: FieldInput,
        expected: String,
    }
    #[derive(Deserialize)]
    struct FieldInput {
        field: String,
        edited: bool,
        local: Option<String>,
        synced: Option<String>,
    }

    fn fixture() -> Fixture {
        serde_json::from_str(FIXTURE).unwrap()
    }

    #[test]
    fn replays_debounce() {
        assert_eq!(fixture().debounce_ms, DEBOUNCE_MS);
    }

    #[test]
    fn replays_keys() {
        for case in fixture().keys {
            let kind = Kind::ALL.into_iter().find(|k| k.as_str() == case.target.kind).unwrap();
            let target = Target::new(kind, &case.target.id);
            assert_eq!(target.key(), case.key, "{}", case.name);
            assert_eq!(parse_key(&case.key), Some(target), "{}", case.name);
        }
    }

    #[test]
    fn replays_unparsable() {
        for key in fixture().unparsable {
            assert_eq!(parse_key(&key), None, "{key:?}");
        }
    }

    #[test]
    fn replays_writes() {
        for case in fixture().writes {
            let expected = match case.expected.op.as_str() {
                "upsert" => Write::Upsert(case.expected.text.unwrap()),
                "delete" => Write::Delete,
                other => panic!("unknown op {other}"),
            };
            assert_eq!(write_for(&case.text), expected, "{}", case.name);
        }
    }

    #[test]
    fn replays_fields() {
        for case in fixture().fields {
            let i = &case.input;
            assert_eq!(
                field(&i.field, i.edited, i.local.as_deref(), i.synced.as_deref()),
                case.expected,
                "{}",
                case.name
            );
        }
    }

    const KEY: &str = "comment:i-1";

    #[test]
    fn writes_after_the_debounce_of_the_last_keystroke() {
        let t0 = Instant::now();
        let mut w = DraftWriter::new();
        w.edit(KEY, "t-1", "h", t0);
        w.edit(KEY, "t-1", "hi", t0 + Duration::from_millis(500));
        assert_eq!(w.next_due(), Some(t0 + Duration::from_millis(1300)));
        assert!(w.due(t0 + Duration::from_millis(1299)).is_empty());
        let jobs = w.due(t0 + Duration::from_millis(1300));
        assert_eq!(
            jobs,
            vec![WriteJob { key: KEY.into(), team_id: "t-1".into(), text: "hi".into(), write: Write::Upsert("hi".into()) }]
        );
        // In flight: nothing more is due, the local copy shows.
        assert!(w.due(t0 + Duration::from_secs(10)).is_empty());
        assert_eq!(w.local(KEY), Some("hi"));
    }

    #[test]
    fn flush_writes_at_once_and_an_empty_field_deletes() {
        let t0 = Instant::now();
        let mut w = DraftWriter::new();
        w.edit(KEY, "t-1", "  \n", t0);
        let job = w.flush(KEY).unwrap();
        assert_eq!(job.write, Write::Delete);
        assert_eq!(w.next_due(), None);
        // Nothing pending: a second flush writes nothing.
        assert_eq!(w.flush(KEY), None);
        assert_eq!(w.settled(&job, true), None);
        w.observe_synced(KEY, None);
        assert_eq!(w.local(KEY), None);
    }

    #[test]
    fn the_local_copy_stays_until_the_shape_echoes_it() {
        let t0 = Instant::now();
        let mut w = DraftWriter::new();
        w.edit(KEY, "t-1", "draft", t0);
        let job = w.flush(KEY).unwrap();
        w.settled(&job, true);
        w.observe_synced(KEY, Some("older"));
        assert_eq!(w.local(KEY), Some("draft"));
        w.observe_synced(KEY, Some("draft"));
        assert_eq!(w.local(KEY), None);
    }

    #[test]
    fn typing_during_a_write_writes_the_newer_text_after_it() {
        let t0 = Instant::now();
        let mut w = DraftWriter::new();
        w.edit(KEY, "t-1", "a", t0);
        let first = w.flush(KEY).unwrap();
        w.edit(KEY, "t-1", "ab", t0 + Duration::from_millis(10));
        // The flush while in flight defers to settle.
        assert_eq!(w.flush(KEY), None);
        let next = w.settled(&first, true).unwrap();
        assert_eq!(next.text, "ab");
    }

    #[test]
    fn a_failed_write_is_silent_and_retried_by_the_next_flush() {
        let t0 = Instant::now();
        let mut w = DraftWriter::new();
        w.edit(KEY, "t-1", "keep", t0);
        let job = w.flush(KEY).unwrap();
        assert_eq!(w.settled(&job, false), None);
        assert_eq!(w.local(KEY), Some("keep"));
        assert_eq!(w.flush(KEY).unwrap().text, "keep");
    }

    #[test]
    fn flush_all_writes_every_pending_key() {
        let t0 = Instant::now();
        let mut w = DraftWriter::new();
        w.edit("comment:a", "t-1", "x", t0);
        w.edit("session:b", "t-1", "y", t0);
        let mut keys: Vec<String> = w.flush_all().into_iter().map(|j| j.key).collect();
        keys.sort();
        assert_eq!(keys, vec!["comment:a".to_string(), "session:b".to_string()]);
    }
}
