//! NEW-57 RESIDENCY: the raw meeting segments stay on the device, in
//! `<data dir>/meeting.sqlite` (`meeting_segments`: meeting id, channel,
//! label, start/end ms, text, words), for
//! `domain::meeting::LOCAL_RETENTION_DAYS` days, pruned at start and
//! hourly. Audio is never stored here.

use std::path::Path;

use rusqlite::{params, Connection};

/// The log's file under the IDE's data dir.
pub(crate) const FILE: &str = "meeting.sqlite";

/// One finalized segment as the log keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LoggedSegment {
    pub meeting_id: String,
    pub channel: String,
    pub label: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    /// The words with their timestamps, JSON (`[]` = the runtime gave none).
    pub words: String,
}

pub(crate) struct SegmentLog {
    conn: Connection,
}

fn sql(err: rusqlite::Error) -> String {
    format!("meeting log: {err}")
}

impl SegmentLog {
    pub(crate) fn open(data_dir: &Path) -> Result<SegmentLog, String> {
        std::fs::create_dir_all(data_dir).map_err(|err| format!("meeting log: {err}"))?;
        Self::with(Connection::open(data_dir.join(FILE)).map_err(sql)?)
    }

    fn with(conn: Connection) -> Result<SegmentLog, String> {
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=5000;
             CREATE TABLE IF NOT EXISTS meeting_segments (
                 meeting_id TEXT NOT NULL,
                 channel TEXT NOT NULL,
                 label TEXT NOT NULL,
                 start_ms INTEGER NOT NULL,
                 end_ms INTEGER NOT NULL,
                 text TEXT NOT NULL,
                 words TEXT NOT NULL DEFAULT '[]'
             );
             CREATE INDEX IF NOT EXISTS meeting_segments_end ON meeting_segments (end_ms);
             CREATE INDEX IF NOT EXISTS meeting_segments_meeting ON meeting_segments (meeting_id, start_ms);",
        )
        .map_err(sql)?;
        Ok(SegmentLog { conn })
    }

    pub(crate) fn insert(&self, segment: &LoggedSegment) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO meeting_segments (meeting_id, channel, label, start_ms, end_ms, text, words)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    segment.meeting_id,
                    segment.channel,
                    segment.label,
                    segment.start_ms,
                    segment.end_ms,
                    segment.text,
                    segment.words
                ],
            )
            .map(|_| ())
            .map_err(sql)
    }

    /// Drop every segment that ended more than `days` before `now_ms`;
    /// answers how many went.
    pub(crate) fn prune(&self, now_ms: i64, days: i64) -> Result<usize, String> {
        let cutoff = now_ms - days * 86_400_000;
        self.conn.execute("DELETE FROM meeting_segments WHERE end_ms < ?1", params![cutoff]).map_err(sql)
    }

    #[cfg(test)]
    fn all(&self) -> Vec<LoggedSegment> {
        let mut statement = self
            .conn
            .prepare("SELECT meeting_id, channel, label, start_ms, end_ms, text, words FROM meeting_segments ORDER BY start_ms")
            .unwrap();
        statement
            .query_map([], |row| {
                Ok(LoggedSegment {
                    meeting_id: row.get(0)?,
                    channel: row.get(1)?,
                    label: row.get(2)?,
                    start_ms: row.get(3)?,
                    end_ms: row.get(4)?,
                    text: row.get(5)?,
                    words: row.get(6)?,
                })
            })
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(start_ms: i64, text: &str) -> LoggedSegment {
        LoggedSegment {
            meeting_id: "2026-10-28T09:00:00Z".into(),
            channel: "you".into(),
            label: "You".into(),
            start_ms,
            end_ms: start_ms + 1_000,
            text: text.into(),
            words: "[]".into(),
        }
    }

    #[test]
    fn the_meeting_log_keeps_segments_for_30_days() {
        let log = SegmentLog::with(Connection::open_in_memory().unwrap()).unwrap();
        let day = 86_400_000;
        let now = 100 * day;
        log.insert(&segment(now - 31 * day, "too old")).unwrap();
        log.insert(&segment(now - 29 * day, "kept")).unwrap();
        log.insert(&segment(now - 1_000, "fresh")).unwrap();
        assert_eq!(log.prune(now, domain::meeting::LOCAL_RETENTION_DAYS).unwrap(), 1);
        let texts: Vec<String> = log.all().into_iter().map(|row| row.text).collect();
        assert_eq!(texts, ["kept", "fresh"]);
        assert_eq!(log.prune(now, domain::meeting::LOCAL_RETENTION_DAYS).unwrap(), 0);
    }

    #[test]
    fn the_meeting_log_lives_in_the_data_dir() {
        let dir = std::env::temp_dir().join(format!("meeting-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let log = SegmentLog::open(&dir).unwrap();
        log.insert(&segment(5, "hello")).unwrap();
        drop(log);
        let reopened = SegmentLog::open(&dir).unwrap();
        assert_eq!(reopened.all(), [segment(5, "hello")]);
        assert!(dir.join(FILE).is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
