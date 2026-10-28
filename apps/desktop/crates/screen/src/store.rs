//! STORE + RETENTION: kept frames as lossy WebP files in one directory per
//! UTC hour (`<data dir>/screen/<yyyyMMdd-HH>/<at ms>-<display>.webp`),
//! indexed in `<data dir>/screen.sqlite` (`screen_frames`). The sweep drops
//! rows older than `retentionMs` with their files and empty hour
//! directories; at start-up files without a row (and rows without a file)
//! go too.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use rusqlite::{params, Connection};

use crate::ring;

pub(crate) const FRAMES_DIR: &str = "screen";
pub(crate) const INDEX_FILE: &str = "screen.sqlite";

/// A rect within the frame, physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// One `screen_frames` row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Row {
    /// Unix ms.
    pub at: i64,
    /// 1-based, the computer crate's display order.
    pub display: u32,
    pub app: String,
    pub title: String,
    /// 16 lowercase hex digits.
    pub hash: String,
    /// Relative to the data dir, `/`-separated.
    pub path: String,
    pub width: u32,
    pub height: u32,
    /// The focused window's rect when it lies on this display.
    pub focus: Option<Rect>,
}

pub(crate) struct Store {
    data_dir: PathBuf,
    conn: Connection,
}

/// `screen/<yyyyMMdd-HH>/<at>-<display>.webp`, the hour in UTC.
pub(crate) fn relative_path(at: i64, display: u32) -> String {
    let hour = Utc.timestamp_millis_opt(at).single().unwrap_or_default().format("%Y%m%d-%H");
    format!("{FRAMES_DIR}/{hour}/{at}-{display}.webp")
}

fn sql(err: rusqlite::Error) -> String {
    format!("screen memory index: {err}")
}

fn io(err: std::io::Error) -> String {
    format!("screen memory store: {err}")
}

impl Store {
    pub(crate) fn open(data_dir: &Path) -> Result<Store, String> {
        fs::create_dir_all(data_dir.join(FRAMES_DIR)).map_err(io)?;
        let conn = Connection::open(data_dir.join(INDEX_FILE)).map_err(sql)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;
             CREATE TABLE IF NOT EXISTS screen_frames (
                 at INTEGER NOT NULL,
                 display INTEGER NOT NULL,
                 app TEXT NOT NULL,
                 title TEXT NOT NULL,
                 hash TEXT NOT NULL,
                 path TEXT NOT NULL PRIMARY KEY,
                 width INTEGER NOT NULL,
                 height INTEGER NOT NULL,
                 focus_x INTEGER,
                 focus_y INTEGER,
                 focus_width INTEGER,
                 focus_height INTEGER
             );
             CREATE INDEX IF NOT EXISTS screen_frames_at ON screen_frames (at);",
        )
        .map_err(sql)?;
        Ok(Store { data_dir: data_dir.to_path_buf(), conn })
    }

    pub(crate) fn file(&self, row: &Row) -> PathBuf {
        self.data_dir.join(&row.path)
    }

    /// Write the frame's file, then its row.
    pub(crate) fn insert(&self, row: &Row, webp: &[u8]) -> Result<(), String> {
        let file = self.file(row);
        if let Some(dir) = file.parent() {
            fs::create_dir_all(dir).map_err(io)?;
        }
        fs::write(&file, webp).map_err(io)?;
        let focus = row.focus;
        self.conn
            .execute(
                "INSERT OR REPLACE INTO screen_frames
                 (at, display, app, title, hash, path, width, height, focus_x, focus_y, focus_width, focus_height)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    row.at,
                    row.display,
                    row.app,
                    row.title,
                    row.hash,
                    row.path,
                    row.width,
                    row.height,
                    focus.map(|rect| rect.x),
                    focus.map(|rect| rect.y),
                    focus.map(|rect| rect.width),
                    focus.map(|rect| rect.height),
                ],
            )
            .map_err(sql)?;
        Ok(())
    }

    /// The rows with from <= at <= to, by time.
    pub(crate) fn frames(&self, from: i64, to: i64) -> Result<Vec<Row>, String> {
        let mut statement = self
            .conn
            .prepare_cached(
                "SELECT at, display, app, title, hash, path, width, height, focus_x, focus_y, focus_width, focus_height
                 FROM screen_frames WHERE at >= ?1 AND at <= ?2 ORDER BY at, display",
            )
            .map_err(sql)?;
        let rows = statement
            .query_map(params![from, to], |row| {
                let focus = match (row.get::<_, Option<i32>>(8)?, row.get::<_, Option<i32>>(9)?, row.get::<_, Option<u32>>(10)?, row.get::<_, Option<u32>>(11)?) {
                    (Some(x), Some(y), Some(width), Some(height)) => Some(Rect { x, y, width, height }),
                    _ => None,
                };
                Ok(Row {
                    at: row.get(0)?,
                    display: row.get(1)?,
                    app: row.get(2)?,
                    title: row.get(3)?,
                    hash: row.get(4)?,
                    path: row.get(5)?,
                    width: row.get(6)?,
                    height: row.get(7)?,
                    focus,
                })
            })
            .map_err(sql)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(sql)
    }

    /// RETENTION: rows with at < now - `retentionMs` go with their files;
    /// hour directories left empty are removed. Returns how many went.
    pub(crate) fn sweep(&self, now: i64) -> Result<usize, String> {
        let cutoff = ring::cutoff(now);
        let paths: Vec<String> = {
            let mut statement = self.conn.prepare_cached("SELECT path FROM screen_frames WHERE at < ?1").map_err(sql)?;
            let rows = statement.query_map(params![cutoff], |row| row.get(0)).map_err(sql)?;
            rows.collect::<Result<_, _>>().map_err(sql)?
        };
        if paths.is_empty() {
            return Ok(0);
        }
        for path in &paths {
            let _ = fs::remove_file(self.data_dir.join(path));
        }
        self.conn.execute("DELETE FROM screen_frames WHERE at < ?1", params![cutoff]).map_err(sql)?;
        self.remove_empty_hours();
        Ok(paths.len())
    }

    fn remove_empty_hours(&self) {
        let Ok(hours) = fs::read_dir(self.data_dir.join(FRAMES_DIR)) else { return };
        for hour in hours.flatten() {
            let path = hour.path();
            if path.is_dir() && fs::read_dir(&path).is_ok_and(|mut entries| entries.next().is_none()) {
                let _ = fs::remove_dir(&path);
            }
        }
    }

    /// Start-up: files without a row are deleted, rows without a file too.
    pub(crate) fn remove_orphans(&self) -> Result<(), String> {
        let known: HashSet<String> = {
            let mut statement = self.conn.prepare("SELECT path FROM screen_frames").map_err(sql)?;
            let rows = statement.query_map([], |row| row.get(0)).map_err(sql)?;
            rows.collect::<Result<_, _>>().map_err(sql)?
        };
        let mut present = HashSet::new();
        if let Ok(hours) = fs::read_dir(self.data_dir.join(FRAMES_DIR)) {
            for hour in hours.flatten() {
                let hour_path = hour.path();
                let Ok(files) = fs::read_dir(&hour_path) else {
                    // A stray file at the top level.
                    let _ = fs::remove_file(&hour_path);
                    continue;
                };
                for file in files.flatten() {
                    let relative = format!("{FRAMES_DIR}/{}/{}", hour.file_name().to_string_lossy(), file.file_name().to_string_lossy());
                    if known.contains(&relative) {
                        present.insert(relative);
                    } else if file.path().is_dir() {
                        let _ = fs::remove_dir_all(file.path());
                    } else {
                        let _ = fs::remove_file(file.path());
                    }
                }
            }
        }
        for missing in known.difference(&present) {
            self.conn.execute("DELETE FROM screen_frames WHERE path = ?1", params![missing]).map_err(sql)?;
        }
        self.remove_empty_hours();
        Ok(())
    }
}

/// Every frame and index row under `data_dir` (sign out, another voice
/// device or account). Call with the store closed.
pub(crate) fn forget(data_dir: &Path) {
    let _ = fs::remove_dir_all(data_dir.join(FRAMES_DIR));
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let _ = fs::remove_file(data_dir.join(format!("{INDEX_FILE}{suffix}")));
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::fixture::fixture;

    /// A fresh data dir under the system temp dir, removed on drop.
    pub(crate) struct TempDir(pub PathBuf);

    impl TempDir {
        pub(crate) fn new() -> TempDir {
            let dir = std::env::temp_dir().join(format!("exp-screen-test-{}", uuid::Uuid::new_v4().simple()));
            fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn row(at: i64, display: u32) -> Row {
        Row {
            at,
            display,
            app: "Zed".into(),
            title: "main.rs".into(),
            hash: "0f0f0f0f0f0f0f0f".into(),
            path: relative_path(at, display),
            width: 4,
            height: 2,
            focus: Some(Rect { x: 1, y: 0, width: 2, height: 2 }),
        }
    }

    #[test]
    fn paths_are_per_utc_hour() {
        // 2026-10-28T10:00:20Z.
        assert_eq!(relative_path(1_793_181_620_000, 2), "screen/20261028-10/1793181620000-2.webp");
    }

    #[test]
    fn the_sweep_replays_the_retention_cases_on_disk() {
        for case in fixture()["retentionCases"].as_array().unwrap() {
            let dir = TempDir::new();
            let store = Store::open(&dir.0).unwrap();
            let now = case["now"].as_i64().unwrap();
            let ats: Vec<i64> = case["frames"].as_array().unwrap().iter().map(|at| at.as_i64().unwrap()).collect();
            for at in &ats {
                store.insert(&row(*at, 1), b"webp").unwrap();
            }
            store.sweep(now).unwrap();
            let left: Vec<i64> = store.frames(i64::MIN, i64::MAX).unwrap().iter().map(|row| row.at).collect();
            let expect: Vec<i64> = ats
                .iter()
                .zip(case["kept"].as_array().unwrap())
                .filter(|(_, kept)| kept.as_bool().unwrap())
                .map(|(at, _)| *at)
                .collect();
            assert_eq!(left, expect, "{}", case["name"]);
            for at in &ats {
                assert_eq!(dir.0.join(relative_path(*at, 1)).exists(), expect.contains(at), "{} file {at}", case["name"]);
            }
        }
    }

    #[test]
    fn an_hour_left_empty_goes_and_orphans_go_at_start() {
        let dir = TempDir::new();
        let store = Store::open(&dir.0).unwrap();
        let old = 1_793_170_000_000; // an hour earlier than `now`
        let now = 1_793_181_620_000;
        store.insert(&row(old, 1), b"old").unwrap();
        store.insert(&row(now, 1), b"new").unwrap();
        let old_hour = dir.0.join(relative_path(old, 1)).parent().unwrap().to_path_buf();
        assert!(old_hour.exists());
        store.sweep(now).unwrap();
        assert!(!old_hour.exists());
        // A file without a row, and a row without a file.
        let stray = dir.0.join(relative_path(now + 1, 2));
        fs::write(&stray, b"stray").unwrap();
        store.insert(&row(now + 2, 1), b"x").unwrap();
        fs::remove_file(dir.0.join(relative_path(now + 2, 1))).unwrap();
        drop(store);
        let store = Store::open(&dir.0).unwrap();
        store.remove_orphans().unwrap();
        assert!(!stray.exists());
        let rows = store.frames(i64::MIN, i64::MAX).unwrap();
        assert_eq!(rows, vec![row(now, 1)]);
        drop(store);
        forget(&dir.0);
        assert!(!dir.0.join(FRAMES_DIR).exists());
        assert!(!dir.0.join(INDEX_FILE).exists());
    }
}
