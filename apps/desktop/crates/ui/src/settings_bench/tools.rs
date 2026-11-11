use super::*;

fn tools_shot(doctor: Option<&'static str>) -> ToolsShot {
    ToolsShot { repos: "~/code/exp", prefix: "exp/", shell: None, typed: None, doctor, path_open: false }
}

fn tools(name: &str, shot: ToolsShot) {
    // Pinned before the first frame (the hub's own doctor run is superseded).
    shoot_pane(name, move |window, cx| {
        let mut pane = ToolsPane::for_shots(window, cx);
        pane.show(shot, window, cx);
        pane
    }, |_, _| {});
}

#[test]
#[ignore]
fn settings_tools_ready() {
    tools("tools-ready", tools_shot(Some("ready: git + claude, codex missing, computer use off")));
}

#[test]
#[ignore]
fn settings_tools_dirty() {
    tools("tools-dirty", ToolsShot { shell: Some("/bin/zsh"), typed: Some("feature/"), ..tools_shot(Some("git missing, claude signed out, codex ok, computer use on (no OS permissions here)")) });
}

#[test]
#[ignore]
fn settings_tools_doctor_running() {
    tools("tools-doctor-running", tools_shot(None));
}

#[test]
#[ignore]
fn settings_tools_path_open() {
    tools("tools-path-open", ToolsShot { path_open: true, ..tools_shot(Some("ready: git + claude, codex missing, computer use off")) });
}
