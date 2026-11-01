//! What a ScreenCast (or RemoteDesktop) session shares: the chooser
//! command's picks, a restored selection, or the focused output/window.

use std::{
    io::Write,
    process::{Command, Stdio},
};

use ede_protocol::{CaptureTarget, OutputInfo, WindowId, WindowInfo};
use serde_json::json;

use crate::session::{MONITOR, WINDOW};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pick {
    Output(String),
    Window(WindowId),
}

impl Pick {
    /// `output:HDMI-A-1` / `window:w3` (the chooser's output, restore data).
    pub fn encode(&self) -> String {
        match self {
            Pick::Output(name) => format!("output:{name}"),
            Pick::Window(id) => format!("window:{}", id.as_str()),
        }
    }

    pub fn decode(s: &str) -> Option<Pick> {
        let (kind, rest) = s.trim().split_once(':')?;
        match kind {
            "output" if !rest.is_empty() => Some(Pick::Output(rest.into())),
            "window" => WindowId::new(rest).ok().map(Pick::Window),
            _ => None,
        }
    }

    pub fn source_type(&self) -> u32 {
        match self {
            Pick::Output(_) => MONITOR,
            Pick::Window(_) => WINDOW,
        }
    }

    pub fn target(&self) -> CaptureTarget {
        match self {
            Pick::Output(name) => CaptureTarget::Output { name: name.clone() },
            Pick::Window(id) => CaptureTarget::Window { window: id.clone() },
        }
    }

    /// Still there (and, for a window, not protected)?
    pub fn geometry(&self, outputs: &[OutputInfo], windows: &[WindowInfo]) -> Option<((i32, i32), (i32, i32))> {
        match self {
            Pick::Output(name) => outputs.iter().find(|o| &o.name == name).map(|o| ((o.x, o.y), (o.width, o.height))),
            Pick::Window(id) => windows.iter().find(|w| &w.id == id && !w.protected).map(|w| ((w.x, w.y), (w.width, w.height))),
        }
    }
}

pub struct Ask<'a> {
    pub app_id: &'a str,
    pub types: u32,
    pub multiple: bool,
    pub remote_control: bool,
}

/// The chooser's answer: what to share, and (a line `allow`) whether a
/// RemoteDesktop session may drive the keyboard and pointer.
#[derive(Debug, Default, PartialEq)]
pub struct Chosen {
    pub picks: Vec<Pick>,
    pub allow: bool,
}

/// `Ok(None)` = cancelled. `types` 0 = nothing to share (remote control
/// alone).
pub fn choose(chooser: &[String], ask: &Ask, outputs: &[OutputInfo], windows: &[WindowInfo]) -> Result<Option<Chosen>, String> {
    let types = ask.types;
    let windows: Vec<&WindowInfo> = windows.iter().filter(|w| !w.protected).collect();
    let mut allow = false;
    let picks = if chooser.is_empty() {
        auto(types, outputs, &windows)
    } else {
        let mut names = Vec::new();
        if types & MONITOR != 0 {
            names.push("monitor");
        }
        if types & WINDOW != 0 {
            names.push("window");
        }
        let input = json!({
            "app_id": ask.app_id,
            "types": names,
            "multiple": ask.multiple,
            "remote_control": ask.remote_control,
            "outputs": outputs,
            "windows": if types & WINDOW != 0 { windows.clone() } else { vec![] },
        });
        let out = run(chooser, &input.to_string())?;
        allow = ask.remote_control && out.lines().any(|l| l.trim() == "allow");
        out.lines()
            .filter_map(Pick::decode)
            .filter(|p| p.source_type() & types != 0)
            .filter(|p| p.geometry(outputs, &windows.iter().map(|w| (*w).clone()).collect::<Vec<_>>()).is_some())
            .collect()
    };
    let mut picks: Vec<Pick> = picks;
    if !ask.multiple {
        picks.truncate(1);
    }
    if picks.is_empty() && (types != 0 || !allow) {
        return Ok(None);
    }
    Ok(Some(Chosen { picks, allow }))
}

fn auto(types: u32, outputs: &[OutputInfo], windows: &[&WindowInfo]) -> Vec<Pick> {
    let focused = windows.iter().find(|w| w.focused);
    if types == 0 {
        return Vec::new();
    }
    if types & MONITOR != 0 {
        let under = focused.and_then(|w| {
            let (cx, cy) = (w.x + w.width / 2, w.y + w.height / 2);
            outputs.iter().find(|o| cx >= o.x && cx < o.x + o.width && cy >= o.y && cy < o.y + o.height)
        });
        return under.or(outputs.first()).map(|o| Pick::Output(o.name.clone())).into_iter().collect();
    }
    focused.or(windows.first()).map(|w| Pick::Window(w.id.clone())).into_iter().collect()
}

fn run(chooser: &[String], input: &str) -> Result<String, String> {
    let mut child = Command::new(&chooser[0])
        .args(&chooser[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("chooser {}: {e}", chooser[0]))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes());
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Ok(String::new());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(name: &str, x: i32) -> OutputInfo {
        OutputInfo { name: name.into(), x, y: 0, width: 800, height: 600, scale: 1.0 }
    }

    fn window(id: &str, x: i32, focused: bool, protected: bool) -> WindowInfo {
        WindowInfo { id: WindowId::new(id).unwrap(), title: None, app_id: None, x, y: 10, width: 100, height: 100, focused, fullscreen: false, protected, x11: false }
    }

    #[test]
    fn auto_shares_the_focused_output_or_window() {
        let outputs = [output("A", 0), output("B", 800)];
        let windows = [window("w1", 50, false, false), window("w2", 900, true, false)];
        let ask = Ask { app_id: "", types: MONITOR, multiple: false, remote_control: false };
        assert_eq!(choose(&[], &ask, &outputs, &windows).unwrap(), Some(Chosen { picks: vec![Pick::Output("B".into())], allow: false }));
        let ask = Ask { types: WINDOW, ..ask };
        assert_eq!(choose(&[], &ask, &outputs, &windows).unwrap().unwrap().picks, [Pick::Window(WindowId::new("w2").unwrap())]);
        let only_secret = [window("w9", 0, true, true)];
        assert_eq!(choose(&[], &ask, &outputs, &only_secret).unwrap(), None, "a protected window is never shared");
    }

    #[test]
    fn the_chooser_picks_and_can_cancel() {
        let outputs = [output("A", 0), output("B", 800)];
        let windows = [window("w1", 50, true, false), window("w2", 60, false, true)];
        let ask = Ask { app_id: "org.example", types: MONITOR | WINDOW, multiple: true, remote_control: false };
        let sh = |script: &str| vec!["sh".to_string(), "-c".to_string(), script.to_string()];
        let picks = choose(&sh("cat >/dev/null; printf 'output:B\\nwindow:w2\\nwindow:w1\\nbogus\\n'"), &ask, &outputs, &windows).unwrap();
        assert_eq!(picks.unwrap().picks, [Pick::Output("B".into()), Pick::Window(WindowId::new("w1").unwrap())]);
        assert_eq!(choose(&sh("cat >/dev/null; exit 1"), &ask, &outputs, &windows).unwrap(), None);
        let dir = tempfile::tempdir().unwrap();
        let seen = dir.path().join("in.json");
        let picks = choose(&sh(&format!("cat > {}; echo output:A", seen.display())), &ask, &outputs, &windows).unwrap();
        assert_eq!(picks.unwrap().picks, [Pick::Output("A".into())]);
        let input: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(seen).unwrap()).unwrap();
        assert_eq!((input["app_id"].as_str(), input["types"].as_array().unwrap().len()), (Some("org.example"), 2));
        let ids: Vec<_> = input["windows"].as_array().unwrap().iter().map(|w| w["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["w1"], "protected windows are never offered");

        let control = Ask { app_id: "org.example", types: 0, multiple: false, remote_control: true };
        assert_eq!(choose(&sh("cat >/dev/null; echo allow"), &control, &outputs, &windows).unwrap(), Some(Chosen { picks: vec![], allow: true }));
        assert_eq!(choose(&sh("cat >/dev/null"), &control, &outputs, &windows).unwrap(), None);
        assert_eq!(choose(&[], &control, &outputs, &windows).unwrap(), None, "remote control is never granted by default");
    }
}
