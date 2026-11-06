//! Sound over PipeWire through WirePlumber's `wpctl`: the default sink and
//! source with their volumes and mutes, every sink and source, and the
//! playback streams (the mixer, #EDE-43: one row per app); set the volume,
//! mute, pick the default output or input, a stream's volume and mute.

use ede_protocol::Capability;
use serde_json::{json, Value};

use super::command;
use crate::{unavailable, ActionSpec, Service};

pub struct Sound;

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: u32,
    pub name: String,
    pub volume: u32,
    pub muted: bool,
    pub default: bool,
}

/// The sinks and sources of `wpctl status` (its tree with `├─ Sinks:` and
/// `├─ Sources:` sections under `Audio`).
pub fn parse_status(text: &str) -> (Vec<Node>, Vec<Node>) {
    let mut sinks = Vec::new();
    let mut sources = Vec::new();
    let mut section: Option<&mut Vec<Node>> = None;
    let mut in_audio = false;
    for line in text.lines() {
        let trimmed = line.trim_start_matches(|c: char| c == ' ' || c == '│' || c == '├' || c == '└' || c == '─').trim();
        if line.starts_with("Audio") {
            in_audio = true;
            continue;
        }
        if line.starts_with("Video") || line.starts_with("Settings") {
            in_audio = false;
            section = None;
            continue;
        }
        if !in_audio {
            continue;
        }
        if trimmed.starts_with("Sinks:") {
            section = Some(&mut sinks);
            continue;
        }
        if trimmed.starts_with("Sources:") {
            section = Some(&mut sources);
            continue;
        }
        if trimmed.ends_with(':') {
            section = None;
            continue;
        }
        let Some(list) = section.as_deref_mut() else { continue };
        let default = trimmed.starts_with('*');
        let rest = trimmed.trim_start_matches('*').trim();
        let Some((id, after)) = rest.split_once('.') else { continue };
        let Ok(id) = id.trim().parse::<u32>() else { continue };
        let (name, tail) = match after.rfind("[vol:") {
            Some(i) => (after[..i].trim(), &after[i..]),
            None => (after.trim(), ""),
        };
        let vol = tail.trim_start_matches("[vol:").trim_end_matches(']').trim();
        let muted = vol.contains("MUTED");
        let volume = vol.split_whitespace().next().and_then(|v| v.parse::<f64>().ok()).map(|v| (v * 100.0).round() as u32).unwrap_or(0);
        list.push(Node { id, name: name.to_string(), volume, muted, default });
    }
    (sinks, sources)
}

/// A client's stream of `wpctl status`'s `Streams:` section.
#[derive(Debug, Clone, PartialEq)]
pub struct Stream {
    pub id: u32,
    /// The label `wpctl status` prints (the node name, mostly the app).
    pub label: String,
    /// Its ports read from a source (`input_FL < …`): a recording, not playback.
    pub capture: bool,
}

/// The streams of `wpctl status` (the `Streams:` section under `Audio`): a
/// stream line `63. Firefox` followed by its port lines (`64. output_FL >
/// Sink:playback_FL`, `<` for a capture stream's inputs).
pub fn parse_streams(text: &str) -> Vec<Stream> {
    let mut streams: Vec<Stream> = Vec::new();
    let mut in_audio = false;
    let mut in_streams = false;
    for line in text.lines() {
        let trimmed = line.trim_start_matches(|c: char| c == ' ' || c == '│' || c == '├' || c == '└' || c == '─').trim();
        if line.starts_with("Audio") {
            in_audio = true;
            continue;
        }
        if line.starts_with("Video") || line.starts_with("Settings") {
            in_audio = false;
            in_streams = false;
            continue;
        }
        if !in_audio {
            continue;
        }
        if trimmed.ends_with(':') {
            in_streams = trimmed == "Streams:";
            continue;
        }
        if !in_streams {
            continue;
        }
        let Some((id, after)) = trimmed.split_once('.') else { continue };
        let Ok(id) = id.trim().parse::<u32>() else { continue };
        let after = after.trim();
        if after.contains(" > ") || after.contains(" < ") {
            if let Some(last) = streams.last_mut() {
                last.capture |= after.contains(" < ");
            }
            continue;
        }
        streams.push(Stream { id, label: after.to_string(), capture: false });
    }
    streams
}

/// `wpctl get-volume <id>`: `Volume: 0.75` or `Volume: 0.75 [MUTED]` →
/// `(percent, muted)`.
pub fn parse_volume(text: &str) -> Option<(u32, bool)> {
    let rest = text.trim().strip_prefix("Volume:")?.trim();
    let volume = rest.split_whitespace().next()?.parse::<f64>().ok()?;
    Some(((volume * 100.0).round() as u32, rest.contains("MUTED")))
}

/// A property of `wpctl inspect <id>` (`    application.name = "Firefox"`).
pub fn inspect_prop(text: &str, key: &str) -> Option<String> {
    text.lines().map(|l| l.trim().trim_start_matches('*').trim()).find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == key).then(|| v.trim().trim_matches('"').to_string())
    })
}

fn stream_json(s: &Stream) -> Value {
    let (volume, muted) = command("wpctl", &["get-volume", &s.id.to_string()]).and_then(|t| parse_volume(&t)).unwrap_or((0, false));
    let inspect = command("wpctl", &["inspect", &s.id.to_string()]).unwrap_or_default();
    let app = inspect_prop(&inspect, "application.name").filter(|a| !a.is_empty()).unwrap_or_else(|| s.label.clone());
    let name = inspect_prop(&inspect, "media.name").filter(|n| !n.is_empty()).unwrap_or_else(|| s.label.clone());
    json!({"id": s.id, "app": app, "name": name, "volume": volume, "muted": muted})
}

fn node_json(n: &Node) -> Value {
    json!({"id": n.id, "name": n.name, "volume": n.volume, "muted": n.muted, "default": n.default})
}

impl Service for Sound {
    fn name(&self) -> &'static str {
        "sound"
    }
    fn description(&self) -> &'static str {
        "Output and input devices and volumes through PipeWire (WirePlumber)."
    }

    fn snapshot(&self) -> Value {
        let Some(text) = command("wpctl", &["status"]) else { return unavailable("wpctl (WirePlumber) is not available") };
        let (sinks, sources) = parse_status(&text);
        let streams: Vec<Value> = parse_streams(&text).iter().filter(|s| !s.capture).map(stream_json).collect();
        let sink = sinks.iter().find(|n| n.default).or(sinks.first());
        let source = sources.iter().find(|n| n.default).or(sources.first());
        json!({
            "available": true,
            "volume": sink.map(|n| n.volume).unwrap_or(0),
            "muted": sink.map(|n| n.muted).unwrap_or(false),
            "micVolume": source.map(|n| n.volume).unwrap_or(0),
            "micMuted": source.map(|n| n.muted).unwrap_or(false),
            "sinks": sinks.iter().map(node_json).collect::<Vec<_>>(),
            "sources": sources.iter().map(node_json).collect::<Vec<_>>(),
            "streams": streams,
        })
    }

    fn actions(&self) -> Vec<ActionSpec> {
        vec![
            ActionSpec::new("set_volume", "The output volume, 0 to 100 (up to 150).", json!({"percent": {"type": "integer"}}), &["percent"], Some(Capability::Ui)),
            ActionSpec::new("mute", "Mute or unmute the output.", json!({"on": {"type": "boolean"}}), &["on"], Some(Capability::Ui)),
            ActionSpec::new("set_mic_volume", "The input volume, 0 to 100.", json!({"percent": {"type": "integer"}}), &["percent"], Some(Capability::Ui)),
            ActionSpec::new("mute_mic", "Mute or unmute the input.", json!({"on": {"type": "boolean"}}), &["on"], Some(Capability::Ui)),
            ActionSpec::new("set_default", "The default output or input by node id.", json!({"id": {"type": "integer"}}), &["id"], Some(Capability::Ui)),
            ActionSpec::new("set_default_source", "The default input (microphone) by node id, from the snapshot's sources.", json!({"id": {"type": "integer"}}), &["id"], Some(Capability::Ui)),
            ActionSpec::new("set_stream_volume", "One app's playback volume, 0 to 100, by stream id from the snapshot's streams.", json!({"id": {"type": "integer"}, "percent": {"type": "integer"}}), &["id", "percent"], Some(Capability::Ui)),
            ActionSpec::new("mute_stream", "Mute or unmute one app's playback stream.", json!({"id": {"type": "integer"}, "on": {"type": "boolean"}}), &["id", "on"], Some(Capability::Ui)),
        ]
    }

    fn call(&self, action: &str, args: &Value) -> Result<Value, String> {
        let run = |a: &[&str]| command("wpctl", a).map(|_| ()).ok_or_else(|| format!("wpctl {} failed", a.join(" ")));
        match action {
            "set_volume" | "set_mic_volume" => {
                let percent = args["percent"].as_u64().ok_or("percent")?.min(150);
                let target = if action == "set_volume" { "@DEFAULT_AUDIO_SINK@" } else { "@DEFAULT_AUDIO_SOURCE@" };
                run(&["set-volume", "-l", "1.5", target, &format!("{}%", percent)])?;
                Ok(json!({"volume": percent}))
            }
            "mute" | "mute_mic" => {
                let on = args["on"].as_bool().unwrap_or(true);
                let target = if action == "mute" { "@DEFAULT_AUDIO_SINK@" } else { "@DEFAULT_AUDIO_SOURCE@" };
                run(&["set-mute", target, if on { "1" } else { "0" }])?;
                Ok(json!({"muted": on}))
            }
            "set_default" | "set_default_source" => {
                let id = args["id"].as_u64().ok_or("id")?;
                run(&["set-default", &id.to_string()])?;
                Ok(json!({"default": id}))
            }
            "set_stream_volume" => {
                let id = args["id"].as_u64().ok_or("id")?;
                let percent = args["percent"].as_u64().ok_or("percent")?.min(100);
                run(&["set-volume", "-l", "1.0", &id.to_string(), &format!("{percent}%")])?;
                Ok(json!({"id": id, "volume": percent}))
            }
            "mute_stream" => {
                let id = args["id"].as_u64().ok_or("id")?;
                let on = args["on"].as_bool().unwrap_or(true);
                run(&["set-mute", &id.to_string(), if on { "1" } else { "0" }])?;
                Ok(json!({"id": id, "muted": on}))
            }
            other => Err(format!("no action {other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUS: &str = "PipeWire 'pipewire-0' [1.6.2]\n └─ Clients:\n        33. WirePlumber\n\nAudio\n ├─ Devices:\n │      48. Internes Audio                      [alsa]\n │  \n ├─ Sinks:\n │  *   55. Internes Audio Analoges Stereo      [vol: 0.40]\n │      57. HDMI Out                            [vol: 1.00 MUTED]\n │  \n ├─ Sources:\n │  *   56. Internes Audio Analoges Stereo      [vol: 1.00]\n │  \n ├─ Filters:\n │  \n └─ Streams:\n        63. speech-dispatcher-dummy\n\nVideo\n ├─ Sinks:\n";

    #[test]
    fn wpctl_status_lists_the_sinks_and_sources() {
        let (sinks, sources) = parse_status(STATUS);
        assert_eq!(sinks.len(), 2);
        assert_eq!(sinks[0], Node { id: 55, name: "Internes Audio Analoges Stereo".into(), volume: 40, muted: false, default: true });
        assert_eq!((sinks[1].id, sinks[1].muted, sinks[1].volume), (57, true, 100));
        assert_eq!((sources.len(), sources[0].id, sources[0].default), (1, 56, true));
    }

    const STREAMS: &str = "Audio\n ├─ Sinks:\n │  *   55. Internes Audio Analoges Stereo      [vol: 0.40]\n │  \n ├─ Sources:\n │  *   56. Internes Audio Analoges Stereo      [vol: 1.00]\n │  \n └─ Streams:\n        63. Firefox                                                             \n             64. output_FL       > Internes Audio Analoges Stereo:playback_FL\t[active]\n             65. output_FR       > Internes Audio Analoges Stereo:playback_FR\t[active]\n        70. spotify\n             71. output_FL       > Internes Audio Analoges Stereo:playback_FL\t[active]\n        80. Chromium input\n             81. input_FL        < Internes Audio Analoges Stereo:capture_FL\t[active]\n        90. speech-dispatcher-dummy\n\nVideo\n ├─ Sinks:\n └─ Streams:\n        99. cheese\n";

    #[test]
    fn wpctl_status_lists_the_playback_streams_without_their_ports() {
        let streams = parse_streams(STREAMS);
        assert_eq!(streams.len(), 4, "video streams and port rows are not streams");
        assert_eq!(streams[0], Stream { id: 63, label: "Firefox".into(), capture: false });
        assert_eq!((streams[1].id, streams[1].label.as_str()), (70, "spotify"));
        assert!(streams[2].capture, "a recording stream reads from a source");
        assert_eq!((streams[3].id, streams[3].capture), (90, false));
        assert_eq!(parse_streams(STATUS).len(), 1);
    }

    #[test]
    fn stream_volume_and_properties_parse() {
        assert_eq!(parse_volume("Volume: 0.75\n"), Some((75, false)));
        assert_eq!(parse_volume("Volume: 1.00 [MUTED]\n"), Some((100, true)));
        assert_eq!(parse_volume("Node 63 not found\n"), None);
        let inspect = "id 63, type PipeWire:Interface:Node\n    application.name = \"Firefox\"\n    application.process.binary = \"firefox\"\n    media.name = \"AudioStream\"\n  * node.name = \"Firefox\"\n";
        assert_eq!(inspect_prop(inspect, "application.name").as_deref(), Some("Firefox"));
        assert_eq!(inspect_prop(inspect, "media.name").as_deref(), Some("AudioStream"));
        assert_eq!(inspect_prop(inspect, "node.name").as_deref(), Some("Firefox"), "a starred line is a property too");
        assert_eq!(inspect_prop(inspect, "media.title"), None);
    }
}
