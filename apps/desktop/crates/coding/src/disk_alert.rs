//! #EXP-2154: a device low on disk tells a team's agents.
//!
//! The doctor's `disk` row only warns whoever looks (and below 10 GB, when a
//! build already fails). `settings.json` `diskAlert` (off without it) names a
//! threshold and a team: below it the row is the warning, and the host's beat
//! emits the team event [`EVENT`] (`exponential_events {op: "emit"}`, the
//! same stream an agent's event trigger listens on):
//!
//! ```json
//! { "diskAlert": { "freeGb": 40, "teamId": "<team uuid>" } }
//! ```
//!
//! Opt-in per machine: every member of the team reads its events.
//! Once per episode, again every [`REPEAT`] while it lasts; it is over
//! [`REARM_GB`] above the threshold (a disk hovering at it is one episode).

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// The team event's name (an agent's event trigger listens on it).
pub const EVENT: &str = "device.disk_low";

/// While the disk stays low, the event again this often.
pub const REPEAT: Duration = Duration::from_secs(6 * 60 * 60);

/// An episode ends this far above the threshold (decimal GB).
pub const REARM_GB: u64 = 5;

/// A failed emit is not tried again before this.
const RETRY: Duration = Duration::from_secs(10 * 60);

const SETTINGS_KEY: &str = "diskAlert";
const RECORD_FILE: &str = "disk-alert.json";
const GB: u64 = 1_000_000_000;

/// `settings.json` → `diskAlert` (missing or broken = no alert).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    /// Alert below this many free GB (decimal, the `disk` row's unit).
    pub free_gb: u64,
    /// The team whose event stream gets the event; none = only the row.
    #[serde(default)]
    pub team_id: Option<String>,
}

impl Config {
    pub fn load(data_dir: &Path) -> Option<Config> {
        let raw = std::fs::read_to_string(crate::Settings::default_path(data_dir)).ok()?;
        let root: serde_json::Value = serde_json::from_str(&raw).ok()?;
        serde_json::from_value(root.get(SETTINGS_KEY)?.clone()).ok()
    }
}

/// Below how many free bytes the doctor's `disk` row warns on this machine:
/// [`crate::worktree_lifecycle::LOW_DISK_BYTES`], or `diskAlert` when higher.
pub fn low_bytes(data_dir: &Path) -> u64 {
    let own = Config::load(data_dir).map_or(0, |config| config.free_gb.saturating_mul(GB));
    own.max(crate::worktree_lifecycle::LOW_DISK_BYTES)
}

/// The running episode: when its last event went out (unix seconds).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Episode {
    pub sent_at: Option<i64>,
    pub tried_at: Option<i64>,
}

/// What a reading of `free` bytes means at `now` (pure): emit, and the
/// episode to keep.
pub fn decide(free: u64, free_gb: u64, episode: Episode, now: i64) -> (bool, Episode) {
    if free >= free_gb.saturating_add(REARM_GB).saturating_mul(GB) {
        return (false, Episode::default());
    }
    if free >= free_gb.saturating_mul(GB) {
        return (false, episode);
    }
    // A clock set back never silences it: a stamp in the future is none.
    let waits = |at: Option<i64>, gap: Duration| at.is_some_and(|at| (0..gap.as_secs() as i64).contains(&(now - at)));
    if waits(episode.sent_at, REPEAT) || waits(episode.tried_at, RETRY) {
        return (false, episode);
    }
    (true, Episode { tried_at: Some(now), ..episode })
}

/// The episode in this process: the record below may not be writable on the
/// very disk it is about.
static CURRENT: Mutex<Option<Episode>> = Mutex::new(None);

/// The host's beat: emits the event when due. Blocking (one request at most).
pub fn beat(trpc: &api::TrpcClient, data_dir: &Path, repos_root: &Path, device_id: &str) {
    let Some(config) = Config::load(data_dir) else { return };
    let (Some(team), Some(free)) = (config.team_id.as_deref(), crate::worktree_lifecycle::free_bytes(repos_root)) else {
        return;
    };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs() as i64);
    let mut current = CURRENT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let arguments = event_arguments(team, device_id, &api::users::hostname(), free, config.free_gb);
    let episode = current.unwrap_or_else(|| load(data_dir));
    let next = step(data_dir, free, config.free_gb, episode, now, || {
        api::mcp_tools::call_tool(trpc, "exponential_events_write", &arguments).map(|_| ()).map_err(|err| err.to_string())
    });
    *current = Some(next);
}

/// [`beat`]'s decision and bookkeeping around one `emit`.
fn step(
    data_dir: &Path,
    free: u64,
    free_gb: u64,
    episode: Episode,
    now: i64,
    emit: impl FnOnce() -> Result<(), String>,
) -> Episode {
    let (due, mut next) = decide(free, free_gb, episode, now);
    if due {
        let size = crate::worktree_lifecycle::format_free(free);
        match emit() {
            Ok(()) => {
                log::warn!("disk alert: {size} free, below {free_gb} GB: {EVENT} emitted");
                next.sent_at = Some(now);
            }
            Err(err) => log::warn!("disk alert: {size} free, below {free_gb} GB: {EVENT} not emitted: {err}"),
        }
    }
    if next != episode {
        if let Ok(rendered) = serde_json::to_string(&next) {
            if let Err(err) = api::atomic_file::write_atomic(&data_dir.join(RECORD_FILE), &rendered) {
                log::warn!("disk alert: couldn't record it: {err}");
            }
        }
    }
    next
}

fn load(data_dir: &Path) -> Episode {
    std::fs::read_to_string(data_dir.join(RECORD_FILE)).ok().and_then(|raw| serde_json::from_str(&raw).ok()).unwrap_or_default()
}

fn event_arguments(team: &str, device_id: &str, host: &str, free: u64, free_gb: u64) -> serde_json::Value {
    api::mcp_tools::with_op(
        "emit",
        serde_json::json!({
            "teamId": team,
            "name": EVENT,
            "payload": {
                "deviceId": device_id,
                "device": host,
                "free": crate::worktree_lifecycle::format_free(free),
                "below": format!("{free_gb} GB"),
            },
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("exp-disk-alert-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn one_event_an_episode_and_again_while_it_lasts() {
        let dir = temp("episode");
        let hour = 3_600;
        let mut episode = Episode::default();
        let mut emitted = Vec::new();
        // (hours, free GB): low, lower, hovering back, low, still low, over, low.
        let readings = [(0, 100), (1, 39), (2, 22), (3, 41), (4, 38), (6, 20), (8, 20), (9, 46), (10, 39)];
        for (hours, free) in readings {
            let mut sent = false;
            episode = step(&dir, free * GB, 40, episode, hours * hour, || {
                sent = true;
                Ok(())
            });
            if sent {
                emitted.push(hours);
            }
            assert_eq!(load(&dir), episode, "a restart continues the episode");
        }
        assert_eq!(emitted, [1, 8, 10], "once, repeated after 6 h, again after it was over");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_emit_is_tried_again_and_a_full_disk_never_floods() {
        let dir = temp("retry");
        let fail = |episode, now| step(&dir, 3 * GB, 40, episode, now, || Err("offline".into()));
        let first = fail(Episode::default(), 1_000);
        assert_eq!(first, Episode { sent_at: None, tried_at: Some(1_000) });
        assert!(!decide(3 * GB, 40, first, 1_000 + 9 * 60).0);
        assert!(decide(3 * GB, 40, first, 1_000 + 10 * 60).0);
        // The record cannot be written: the episode still comes back.
        let gone = dir.join("not-there/at-all");
        std::fs::write(dir.join("not-there"), "a file, so no dir can be made").unwrap();
        let sent = step(&gone, 3 * GB, 40, Episode::default(), 50, || Ok(()));
        assert_eq!(sent.sent_at, Some(50));
        assert!(!decide(3 * GB, 40, sent, 60 * 60).0);
        // A clock set back: the stamp is in the future, so it alerts.
        assert!(decide(3 * GB, 40, Episode { sent_at: Some(9_000_000), tried_at: None }, 60).0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn it_is_off_without_the_setting_and_raises_the_rows_threshold_with_it() {
        let dir = temp("config");
        let settings = crate::Settings::default_path(&dir);
        std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
        assert_eq!(Config::load(&dir), None);
        assert_eq!(low_bytes(&dir), 10 * GB);
        std::fs::write(&settings, r#"{"diskAlert":{"freeGb":40,"teamId":"t"}}"#).unwrap();
        assert_eq!(Config::load(&dir), Some(Config { free_gb: 40, team_id: Some("t".into()) }));
        assert_eq!(low_bytes(&dir), 40 * GB);
        std::fs::write(&settings, r#"{"diskAlert":{"freeGb":2}}"#).unwrap();
        assert_eq!(low_bytes(&dir), 10 * GB, "never below the default");
        std::fs::write(&settings, r#"{"diskAlert":"yes"}"#).unwrap();
        assert_eq!(Config::load(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_event_names_the_device_and_the_reading() {
        let arguments = event_arguments("team", "dev-1", "mint", 38_400_000_000, 40);
        assert_eq!(
            arguments,
            serde_json::json!({
                "op": "emit", "teamId": "team", "name": "device.disk_low",
                "payload": { "deviceId": "dev-1", "device": "mint", "free": "38 GB", "below": "40 GB" },
            })
        );
    }
}
