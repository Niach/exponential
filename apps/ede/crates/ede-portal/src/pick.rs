//! What a ScreenCast (or RemoteDesktop) session shares: the chooser
//! command's picks, or (no chooser) the focused output/window. A restored
//! selection only preselects the chooser's rows (#EDE-172, D6 a).

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
    /// The last share's picks (its restore data): the chooser starts with
    /// them selected. Never an answer: only the chooser's own output
    /// shares, and without a chooser they are ignored.
    pub selected: &'a [Pick],
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
    // Asked for and still there (a window: not protected).
    let offered = |p: &Pick| p.source_type() & types != 0 && p.geometry(outputs, windows).is_some();
    let windows: Vec<&WindowInfo> = windows.iter().filter(|w| !w.protected).collect();
    let mut allow = false;
    let picks = if chooser.is_empty() {
        auto(types, outputs, &windows)
    } else {
        let mut selected: Vec<String> = ask.selected.iter().filter(|&p| offered(p)).map(Pick::encode).collect();
        if !ask.multiple {
            selected.truncate(1);
        }
        let mut names = Vec::new();
        if types & MONITOR != 0 {
            names.push("monitor");
        }
        if types & WINDOW != 0 {
            names.push("window");
        }
        // #EDE-200: only the rows of the types asked for (a screen picked
        // for a window-only ask would be dropped, answering as a cancel).
        let input = json!({
            "app_id": ask.app_id,
            "types": names,
            "multiple": ask.multiple,
            "remote_control": ask.remote_control,
            "outputs": if types & MONITOR != 0 { outputs } else { &[] },
            "windows": if types & WINDOW != 0 { windows.clone() } else { vec![] },
            "selected": selected,
        });
        let out = run(chooser, &input.to_string())?;
        allow = ask.remote_control && out.lines().any(|l| l.trim() == "allow");
        out.lines().filter_map(Pick::decode).filter(|p| offered(p)).collect()
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
        OutputInfo { name: name.into(), x, y: 0, width: 800, height: 600, scale: 1.0, modes: vec![], make: String::new(), model: String::new(), physical_mm: [0, 0], space: None }
    }

    fn window(id: &str, x: i32, focused: bool, protected: bool) -> WindowInfo {
        WindowInfo { id: WindowId::new(id).unwrap(), title: None, app_id: None, x, y: 10, width: 100, height: 100, focused, fullscreen: false, protected, x11: false, workspace: 0, minimized: false, space: None }
    }

    #[test]
    fn auto_shares_the_focused_output_or_window() {
        let outputs = [output("A", 0), output("B", 800)];
        let windows = [window("w1", 50, false, false), window("w2", 900, true, false)];
        let ask = Ask { app_id: "", types: MONITOR, multiple: false, remote_control: false, selected: &[] };
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
        let ask = Ask { app_id: "org.example", types: MONITOR | WINDOW, multiple: true, remote_control: false, selected: &[] };
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

        let control = Ask { app_id: "org.example", types: 0, multiple: false, remote_control: true, selected: &[] };
        assert_eq!(choose(&sh("cat >/dev/null; echo allow"), &control, &outputs, &windows).unwrap(), Some(Chosen { picks: vec![], allow: true }));
        assert_eq!(choose(&sh("cat >/dev/null"), &control, &outputs, &windows).unwrap(), None);
        assert_eq!(choose(&[], &control, &outputs, &windows).unwrap(), None, "remote control is never granted by default");
    }

    /// The chooser's answer to `ask` (a shell `answer` after reading the
    /// ask) and the rows it was told to start selected.
    fn asked(ask: &Ask, answer: &str, outputs: &[OutputInfo], windows: &[WindowInfo]) -> (Vec<String>, Option<Chosen>) {
        let dir = tempfile::tempdir().unwrap();
        let seen = dir.path().join("in.json");
        let chooser = vec!["sh".to_string(), "-c".to_string(), format!("cat > {}; {answer}", seen.display())];
        let chosen = choose(&chooser, ask, outputs, windows).unwrap();
        let input: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(seen).unwrap()).unwrap();
        (input["selected"].as_array().unwrap().iter().map(|p| p.as_str().unwrap().to_string()).collect(), chosen)
    }

    /// The rows the chooser was shown for `ask` (outputs by name, windows by
    /// id, the selected ones) and its answer `answer`'s share.
    fn shown(ask: &Ask, answer: &str, outputs: &[OutputInfo], windows: &[WindowInfo]) -> (Vec<String>, Vec<String>, Vec<String>, Option<Chosen>) {
        let dir = tempfile::tempdir().unwrap();
        let seen = dir.path().join("in.json");
        let chooser = vec!["sh".to_string(), "-c".to_string(), format!("cat > {}; {answer}", seen.display())];
        let chosen = choose(&chooser, ask, outputs, windows).unwrap();
        let input: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(seen).unwrap()).unwrap();
        let list = |key: &str, field: &str| input[key].as_array().unwrap_or_else(|| panic!("the ask names its {key}")).iter().map(|r| r[field].as_str().unwrap().to_string()).collect::<Vec<_>>();
        (list("outputs", "name"), list("windows", "id"), input["selected"].as_array().unwrap().iter().map(|p| p.as_str().unwrap().to_string()).collect(), chosen)
    }

    /// #EDE-200: the chooser lists only the types the app asked for: a
    /// window-only ask shows no screens (a screen picked there would answer
    /// as a cancel), a monitor-only one no windows, a combined one both.
    #[test]
    fn the_chooser_lists_only_the_types_asked_for() {
        let outputs = [output("A", 0), output("B", 800)];
        let windows = [window("w1", 50, true, false), window("w2", 60, false, true), window("w3", 900, false, false)];
        let w = |id: &str| Pick::Window(WindowId::new(id).unwrap());
        let strings = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let none = Vec::<String>::new();

        let window_only = Ask { app_id: "org.example", types: WINDOW, multiple: false, remote_control: false, selected: &[] };
        let (screens, rows, _, chosen) = shown(&window_only, "echo window:w3", &outputs, &windows);
        assert_eq!((screens, rows), (none.clone(), strings(&["w1", "w3"])), "a window-only ask shows no screens");
        assert_eq!(chosen.unwrap().picks, [w("w3")]);
        assert_eq!(shown(&window_only, "echo output:A", &outputs, &windows).3, None, "a screen is never shared for a window-only ask");

        let monitor_only = Ask { types: MONITOR, ..window_only };
        let (screens, rows, _, chosen) = shown(&monitor_only, "echo output:B", &outputs, &windows);
        assert_eq!((screens, rows), (strings(&["A", "B"]), none.clone()), "a monitor-only ask shows no windows");
        assert_eq!(chosen.unwrap().picks, [Pick::Output("B".into())]);

        let both = Ask { types: MONITOR | WINDOW, multiple: true, ..window_only };
        let (screens, rows, _, chosen) = shown(&both, "echo output:A; echo window:w1", &outputs, &windows);
        assert_eq!((screens, rows), (strings(&["A", "B"]), strings(&["w1", "w3"])), "a combined ask shows both");
        assert_eq!(chosen.unwrap().picks, [Pick::Output("A".into()), w("w1")]);

        // #EDE-172 (P11): a restored window share still opens with its
        // last window selected, and nothing else listed.
        let restored = [Pick::Output("A".into()), w("w3")];
        let restored_window = Ask { selected: &restored, ..window_only };
        let (screens, rows, selected, chosen) = shown(&restored_window, "echo window:w3", &outputs, &windows);
        assert_eq!((screens, rows, selected), (none, strings(&["w1", "w3"]), strings(&["window:w3"])));
        assert_eq!(chosen.unwrap().picks, [w("w3")]);
    }

    /// #EDE-172 (D6 a): a restored share's picks preselect the chooser's
    /// rows; only the chooser's answer shares.
    #[test]
    fn restored_picks_preselect_the_chooser_and_never_answer_it() {
        let outputs = [output("A", 0), output("B", 800)];
        let windows = [window("w1", 50, false, false), window("w2", 60, false, true), window("w3", 900, true, false)];
        let w = |id: &str| Pick::Window(WindowId::new(id).unwrap());
        let restored = [Pick::Output("gone".into()), w("w2"), Pick::Output("B".into()), w("w1"), w("w9")];
        let ask = Ask { app_id: "org.example", types: MONITOR | WINDOW, multiple: true, remote_control: false, selected: &restored };

        let (rows, chosen) = asked(&ask, "echo output:B; echo window:w1", &outputs, &windows);
        assert_eq!(rows, ["output:B", "window:w1"], "a gone output, a protected window and a closed one are never preselected");
        assert_eq!(chosen.unwrap().picks, [Pick::Output("B".into()), w("w1")]);
        let (_, chosen) = asked(&ask, "echo output:A", &outputs, &windows);
        assert_eq!(chosen.unwrap().picks, [Pick::Output("A".into())], "the person's pick, not the restored one");
        assert_eq!(asked(&ask, "exit 1", &outputs, &windows), (vec!["output:B".to_string(), "window:w1".to_string()], None), "a cancel shares nothing");
        assert_eq!(asked(&ask, "true", &outputs, &windows).1, None, "an empty answer shares nothing");

        let one_window = Ask { types: WINDOW, multiple: false, ..ask };
        assert_eq!(asked(&one_window, "exit 1", &outputs, &windows).0, ["window:w1"], "one pick: the first restored one asked for");
        let control = Ask { types: MONITOR, multiple: false, remote_control: true, ..ask };
        let (rows, chosen) = asked(&control, "echo output:B", &outputs, &windows);
        assert_eq!((rows, chosen.map(|c| c.allow)), (vec!["output:B".to_string()], Some(false)), "a preselected restore never allows remote control");

        let focused_on_a = [window("w1", 50, true, false)];
        let restored_b = [Pick::Output("B".into())];
        let no_chooser = Ask { types: MONITOR, multiple: false, selected: &restored_b, ..ask };
        assert_eq!(choose(&[], &no_chooser, &outputs, &focused_on_a).unwrap().unwrap().picks, [Pick::Output("A".into())], "without a chooser restore data picks nothing: the focused output, as for any ask");
    }
}
