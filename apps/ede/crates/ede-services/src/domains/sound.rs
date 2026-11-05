//! Sound over PipeWire through WirePlumber's `wpctl`: the default sink and
//! source with their volumes and mutes, every sink and source; set the
//! volume, mute, pick the default.

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
        })
    }

    fn actions(&self) -> Vec<ActionSpec> {
        vec![
            ActionSpec::new("set_volume", "The output volume, 0 to 100 (up to 150).", json!({"percent": {"type": "integer"}}), &["percent"], Some(Capability::Ui)),
            ActionSpec::new("mute", "Mute or unmute the output.", json!({"on": {"type": "boolean"}}), &["on"], Some(Capability::Ui)),
            ActionSpec::new("set_mic_volume", "The input volume, 0 to 100.", json!({"percent": {"type": "integer"}}), &["percent"], Some(Capability::Ui)),
            ActionSpec::new("mute_mic", "Mute or unmute the input.", json!({"on": {"type": "boolean"}}), &["on"], Some(Capability::Ui)),
            ActionSpec::new("set_default", "The default output or input by node id.", json!({"id": {"type": "integer"}}), &["id"], Some(Capability::Ui)),
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
            "set_default" => {
                let id = args["id"].as_u64().ok_or("id")?;
                run(&["set-default", &id.to_string()])?;
                Ok(json!({"default": id}))
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
}
