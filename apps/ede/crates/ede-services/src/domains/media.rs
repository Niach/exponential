//! Media players over MPRIS on the session bus (#EDE-43's sound applet:
//! the player card with art, title, artist, the transport buttons and the
//! seek bar). Every `org.mpris.MediaPlayer2.*` name is a player; `active`
//! is the one playing, else the first. Play/pause, next, previous, stop
//! and seek go to the named player or the active one. No player at all is
//! an empty list with `available: true`: the bus is there, nobody plays.

use std::collections::HashMap;

use ede_protocol::Capability;
use serde_json::{json, Value};
use zbus::{blocking::Connection, zvariant::OwnedValue};

use super::{prop, session_bus, str_of};
use crate::{unavailable, ActionSpec, Service};

const PREFIX: &str = "org.mpris.MediaPlayer2.";
const PATH: &str = "/org/mpris/MediaPlayer2";
const ROOT: &str = "org.mpris.MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";

pub struct Media;

/// MPRIS's `PlaybackStatus` as the applet shows it.
pub fn status_of(playback_status: &str) -> &'static str {
    match playback_status {
        "Playing" => "playing",
        "Paused" => "paused",
        _ => "stopped",
    }
}

/// Microseconds (MPRIS) → seconds, as the seek bar binds them.
pub fn seconds(micros: i64) -> f64 {
    micros.max(0) as f64 / 1_000_000.0
}

/// A `mpris:length`, which players send as `x`, `t` or `u`.
fn length_of(metadata: &HashMap<String, OwnedValue>) -> i64 {
    let Some(v) = metadata.get("mpris:length") else { return 0 };
    i64::try_from(v.clone()).ok().or_else(|| u64::try_from(v.clone()).ok().map(|u| u as i64)).or_else(|| u32::try_from(v.clone()).ok().map(i64::from)).or_else(|| i32::try_from(v.clone()).ok().map(i64::from)).unwrap_or(0)
}

/// `xesam:artist` (`as`), joined with `, `; some players send one string.
fn artist_of(metadata: &HashMap<String, OwnedValue>) -> String {
    let Some(v) = metadata.get("xesam:artist") else { return String::new() };
    if let Ok(list) = Vec::<String>::try_from(v.clone()) {
        return list.join(", ");
    }
    String::try_from(v.clone()).unwrap_or_default()
}

/// One player's row of the snapshot, from what the bus said.
pub fn player_json(name: &str, identity: &str, playback_status: &str, metadata: &HashMap<String, OwnedValue>, position_micros: i64, can_go_next: bool, can_go_previous: bool, can_seek: bool) -> Value {
    json!({
        "name": name,
        "identity": identity,
        "status": status_of(playback_status),
        "title": str_of(metadata, "xesam:title"),
        "artist": artist_of(metadata),
        "album": str_of(metadata, "xesam:album"),
        "artUrl": str_of(metadata, "mpris:artUrl"),
        "position": seconds(position_micros),
        "length": seconds(length_of(metadata)),
        "canGoNext": can_go_next,
        "canGoPrevious": can_go_previous,
        "canSeek": can_seek,
    })
}

/// The index of the active player: the first playing one, else the first.
pub fn active_index(players: &[Value]) -> Option<usize> {
    players.iter().position(|p| p["status"] == "playing").or_else(|| (!players.is_empty()).then_some(0))
}

fn player_names(conn: &Connection) -> Result<Vec<String>, String> {
    let names: Vec<String> = conn.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "ListNames", &()).and_then(|r| r.body().deserialize()).map_err(|e| e.to_string())?;
    let mut players: Vec<String> = names.into_iter().filter(|n| n.starts_with(PREFIX)).collect();
    players.sort();
    Ok(players)
}

fn player_row(conn: &Connection, name: &str) -> Value {
    let identity = prop::<String>(conn, name, PATH, ROOT, "Identity").unwrap_or_else(|| name.trim_start_matches(PREFIX).to_string());
    let status = prop::<String>(conn, name, PATH, PLAYER, "PlaybackStatus").unwrap_or_default();
    let metadata: HashMap<String, OwnedValue> = prop(conn, name, PATH, PLAYER, "Metadata").unwrap_or_default();
    let position = prop::<i64>(conn, name, PATH, PLAYER, "Position").unwrap_or(0);
    let flag = |p: &str| prop::<bool>(conn, name, PATH, PLAYER, p).unwrap_or(false);
    player_json(name, &identity, &status, &metadata, position, flag("CanGoNext"), flag("CanGoPrevious"), flag("CanSeek"))
}

impl Service for Media {
    fn name(&self) -> &'static str {
        "media"
    }
    fn description(&self) -> &'static str {
        "Media players (MPRIS): what plays, the track, the transport controls."
    }

    fn snapshot(&self) -> Value {
        let Ok(conn) = session_bus() else { return unavailable("no session bus") };
        let names = match player_names(&conn) {
            Ok(n) => n,
            Err(e) => return unavailable(format!("session bus: {e}")),
        };
        let players: Vec<Value> = names.iter().map(|n| player_row(&conn, n)).collect();
        let active = active_index(&players).map(|i| players[i].clone()).unwrap_or(Value::Null);
        json!({"available": true, "players": players, "active": active})
    }

    fn actions(&self) -> Vec<ActionSpec> {
        let name = json!({"name": {"type": "string", "description": "The player's bus name from the snapshot; unset = the active player."}});
        vec![
            ActionSpec::new("play_pause", "Play or pause a player.", name.clone(), &[], Some(Capability::Ui)),
            ActionSpec::new("next", "The next track.", name.clone(), &[], Some(Capability::Ui)),
            ActionSpec::new("previous", "The previous track.", name.clone(), &[], Some(Capability::Ui)),
            ActionSpec::new("stop", "Stop a player.", name.clone(), &[], Some(Capability::Ui)),
            ActionSpec::new("seek", "Jump to a position in the current track, in seconds.", json!({"name": name["name"], "position": {"type": "number"}}), &["position"], Some(Capability::Ui)),
        ]
    }

    fn call(&self, action: &str, args: &Value) -> Result<Value, String> {
        let conn = session_bus()?;
        let name = match args["name"].as_str().filter(|n| !n.is_empty()) {
            Some(n) => n.to_string(),
            None => {
                let snapshot = self.snapshot();
                snapshot["active"]["name"].as_str().map(String::from).ok_or("no media player is running")?
            }
        };
        if !name.starts_with(PREFIX) {
            return Err(format!("{name} is not an MPRIS player"));
        }
        let simple = |method: &str| conn.call_method(Some(name.as_str()), PATH, Some(PLAYER), method, &()).map(|_| ()).map_err(|e| e.to_string());
        match action {
            "play_pause" | "next" | "previous" | "stop" => {
                let method = match action {
                    "play_pause" => "PlayPause",
                    "next" => "Next",
                    "previous" => "Previous",
                    _ => "Stop",
                };
                simple(method)?;
                Ok(json!({"name": name, "action": action}))
            }
            "seek" => {
                let position = args["position"].as_f64().ok_or("position")?.max(0.0);
                let micros = (position * 1_000_000.0).round() as i64;
                let metadata: HashMap<String, OwnedValue> = prop(&conn, &name, PATH, PLAYER, "Metadata").unwrap_or_default();
                let track = metadata.get("mpris:trackid").and_then(|v| zbus::zvariant::OwnedObjectPath::try_from(v.clone()).ok());
                match track {
                    // SetPosition is absolute but needs the track it refers to.
                    Some(track) => conn.call_method(Some(name.as_str()), PATH, Some(PLAYER), "SetPosition", &(track, micros)).map(|_| ()).map_err(|e| e.to_string())?,
                    // Without a track id: a relative seek from where it is.
                    None => {
                        let now = prop::<i64>(&conn, &name, PATH, PLAYER, "Position").unwrap_or(0);
                        conn.call_method(Some(name.as_str()), PATH, Some(PLAYER), "Seek", &(micros - now,)).map(|_| ()).map_err(|e| e.to_string())?;
                    }
                }
                Ok(json!({"name": name, "position": position}))
            }
            other => Err(format!("no action {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Value as ZValue;

    fn metadata() -> HashMap<String, OwnedValue> {
        let mut m = HashMap::new();
        m.insert("xesam:title".to_string(), OwnedValue::try_from(ZValue::from("Blue Train")).unwrap());
        m.insert("xesam:artist".to_string(), OwnedValue::try_from(ZValue::from(vec!["John Coltrane", "Lee Morgan"])).unwrap());
        m.insert("xesam:album".to_string(), OwnedValue::try_from(ZValue::from("Blue Train")).unwrap());
        m.insert("mpris:artUrl".to_string(), OwnedValue::try_from(ZValue::from("file:///tmp/art.jpg")).unwrap());
        m.insert("mpris:length".to_string(), OwnedValue::try_from(ZValue::from(642_000_000i64)).unwrap());
        m
    }

    #[test]
    fn a_player_row_reads_mpris_metadata() {
        let row = player_json("org.mpris.MediaPlayer2.spotify", "Spotify", "Playing", &metadata(), 61_500_000, true, false, true);
        assert_eq!(row["status"], "playing");
        assert_eq!(row["title"], "Blue Train");
        assert_eq!(row["artist"], "John Coltrane, Lee Morgan");
        assert_eq!(row["artUrl"], "file:///tmp/art.jpg");
        assert_eq!((row["position"].as_f64(), row["length"].as_f64()), (Some(61.5), Some(642.0)));
        assert_eq!((row["canGoNext"].as_bool(), row["canGoPrevious"].as_bool(), row["canSeek"].as_bool()), (Some(true), Some(false), Some(true)));
        // A length as an unsigned, an artist as one string, a status that is neither.
        let mut odd = HashMap::new();
        odd.insert("mpris:length".to_string(), OwnedValue::try_from(ZValue::from(5_000_000u64)).unwrap());
        odd.insert("xesam:artist".to_string(), OwnedValue::try_from(ZValue::from("Solo")).unwrap());
        let row = player_json("org.mpris.MediaPlayer2.vlc", "VLC", "Stopped", &odd, -3, false, false, false);
        assert_eq!((row["status"].as_str(), row["artist"].as_str(), row["title"].as_str()), (Some("stopped"), Some("Solo"), Some("")));
        assert_eq!((row["position"].as_f64(), row["length"].as_f64()), (Some(0.0), Some(5.0)));
    }

    #[test]
    fn the_active_player_is_the_playing_one_else_the_first() {
        let paused = json!({"name": "a", "status": "paused"});
        let playing = json!({"name": "b", "status": "playing"});
        assert_eq!(active_index(&[paused.clone(), playing.clone()]), Some(1));
        assert_eq!(active_index(&[paused.clone()]), Some(0));
        assert_eq!(active_index(&[]), None);
        assert_eq!(status_of("Paused"), "paused");
    }

    #[test]
    fn the_actions_take_an_optional_player_and_seek_takes_seconds() {
        let actions = Media.actions();
        let names: Vec<&str> = actions.iter().map(|a| a.name).collect();
        assert_eq!(names, ["play_pause", "next", "previous", "stop", "seek"]);
        assert!(actions.iter().all(|a| a.capability == Some(Capability::Ui)));
        assert_eq!(actions[0].schema["required"], json!([]));
        assert_eq!(actions[4].schema["required"], json!(["position"]));
    }
}
