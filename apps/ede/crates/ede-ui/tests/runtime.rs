//! The runtime against a real directory and the real git CLI.

use std::{
    fs,
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};

use ede_ui::{Body, Change, EditError, Runtime, THEME_FILE};
use serde_json::{json, Value};

/// A brand theme from the eUI package: `extends: exponential`.
const ORANGE: &str = include_str!("../../../../../packages/exponential-ui/themes/orange.theme.json");

fn panel(text: &str) -> Value {
    json!({
        "$comment": "the top panel",
        "surface": {"anchor": ["top", "left", "right"], "size": {"height": 40}, "exclusiveZone": 40},
        "root": {
            "id": "panel",
            "component": "Box",
            "props": {"style": {"display": "flex", "gap": "$spacing.md"}},
            "children": [{"id": "clock", "component": "Text", "props": {"text": text}}]
        }
    })
}

fn broken() -> Value {
    json!({"surface": {"size": {"width": 10, "height": 10}}, "root": {"id": "panel", "component": "Nope"}})
}

fn orange() -> Value {
    serde_json::from_str(ORANGE).unwrap()
}

fn text_of(ui: &Runtime, name: &str) -> String {
    let doc = ui.document(name).expect("a good version");
    doc.tree.children[0].props["text"].as_str().unwrap().to_string()
}

fn summaries(ui: &Runtime) -> Vec<String> {
    ui.log(50).unwrap().into_iter().map(|c| c.summary).collect()
}

fn open(dir: &std::path::Path) -> (Runtime, Vec<Change>) {
    Runtime::open(dir).unwrap()
}

#[test]
fn a_fresh_directory_is_a_repository_with_the_builtin_theme() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("ede/ui");
    let (ui, changes) = open(&dir);
    assert!(dir.join(".git").is_dir());
    assert_eq!(summaries(&ui), ["ede-ui: history starts"]);
    assert_eq!(changes, [Change::ThemeApplied { id: "exponential".into(), builtin: true, commit: None }]);
    assert_eq!(ui.theme().id, "exponential");
    assert_eq!(ui.documents().count(), 0);
    // A second open changes nothing.
    let (ui, _) = open(&dir);
    assert_eq!(summaries(&ui).len(), 1);
}

#[test]
fn an_edit_validates_writes_and_commits() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut ui, _) = open(tmp.path());
    let changes = ui.write("panel", &panel("12:00"), "Agent: add the clock").unwrap();
    let [Change::Applied { name, commit: Some(commit) }] = changes.as_slice() else { panic!("{changes:?}") };
    assert_eq!(name, "panel");
    assert_eq!(commit, &ui.log(1).unwrap()[0].id);
    assert_eq!(summaries(&ui)[0], "Agent: add the clock");
    assert_eq!(text_of(&ui, "panel"), "12:00");
    assert!(matches!(ui.document("panel").unwrap().body, Body::Nested(_)));

    // Refused: nothing written, nothing committed, the good version stays.
    let Err(EditError::Invalid(issues)) = ui.write("panel", &broken(), "Agent: break it") else { panic!() };
    assert_eq!(issues[0].path, "#panel");
    assert!(issues[0].message.contains("unknown component Nope"), "{issues:?}");
    assert_eq!(text_of(&ui, "panel"), "12:00");
    assert_eq!(summaries(&ui).len(), 2);
    let on_disk: Value = serde_json::from_slice(&fs::read(tmp.path().join("panel.json")).unwrap()).unwrap();
    assert_eq!(on_disk, panel("12:00"));

    assert!(matches!(ui.write("../etc", &panel("x"), "m"), Err(EditError::Name(_))));
    assert!(matches!(ui.write("theme", &panel("x"), "m"), Err(EditError::Name(_))));
}

#[test]
fn hand_edits_commit_once_valid_and_a_broken_one_falls_back() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut ui, _) = open(tmp.path());
    ui.write("panel", &panel("12:00"), "Set panel").unwrap();

    fs::write(tmp.path().join("panel.json"), serde_json::to_vec(&panel("13:00")).unwrap()).unwrap();
    let changes = ui.reload().unwrap();
    assert!(matches!(changes.as_slice(), [Change::Applied { commit: Some(_), .. }]), "{changes:?}");
    assert_eq!(summaries(&ui)[0], "Edit panel.json");
    assert_eq!(text_of(&ui, "panel"), "13:00");
    // Nothing changed since: nothing to report.
    assert_eq!(ui.reload().unwrap(), []);

    fs::write(tmp.path().join("panel.json"), b"{\"root\": ").unwrap();
    let changes = ui.reload().unwrap();
    let [Change::Rejected { name, issues, fallback: true }] = changes.as_slice() else { panic!("{changes:?}") };
    assert_eq!(name, "panel");
    assert!(issues[0].path.starts_with("line 1:"), "{issues:?}");
    assert_eq!(text_of(&ui, "panel"), "13:00");
    assert!(ui.rejected().contains_key("panel"));
    assert_eq!(summaries(&ui).len(), 3, "a refused edit is never committed");

    // After a restart the last good version comes from git.
    drop(ui);
    let (ui, changes) = open(tmp.path());
    assert!(changes.iter().any(|c| matches!(c, Change::Rejected { fallback: true, .. })), "{changes:?}");
    assert_eq!(text_of(&ui, "panel"), "13:00");

    // Discard puts the file back.
    let (mut ui, _) = open(tmp.path());
    ui.discard("panel").unwrap();
    assert!(ui.rejected().is_empty());
    let on_disk: Value = serde_json::from_slice(&fs::read(tmp.path().join("panel.json")).unwrap()).unwrap();
    assert_eq!(on_disk, panel("13:00"));
}

#[test]
fn a_broken_file_with_no_history_is_just_refused() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("dock.json"), serde_json::to_vec(&broken()).unwrap()).unwrap();
    // Not documents: hidden files, other extensions, bad names.
    fs::write(tmp.path().join(".dock.json.swp"), b"x").unwrap();
    fs::write(tmp.path().join("notes.txt"), b"x").unwrap();
    fs::write(tmp.path().join("Bad Name.json"), b"x").unwrap();
    let (ui, changes) = open(tmp.path());
    assert!(changes.iter().any(|c| matches!(c, Change::Rejected { name, fallback: false, .. } if name == "dock")), "{changes:?}");
    assert_eq!(changes.len(), 2, "{changes:?}");
    assert!(ui.document("dock").is_none());
}

#[test]
fn documents_are_checked_for_shape() {
    let issues = |v: Value| ede_ui::validate_document("d", &v).unwrap_err();
    assert_eq!(issues(json!([1]))[0].path, "file");
    assert_eq!(issues(json!({"surface": panel("x")["surface"], "root": panel("x")["root"], "colour": 1}))[0].path, "colour");
    assert_eq!(issues(json!({}))[0].path, "surface");
    assert_eq!(issues(json!({"surface": panel("x")["surface"]}))[0].path, "file");
    assert_eq!(issues(json!({"surface": panel("x")["surface"], "root": panel("x")["root"], "components": []}))[0].path, "file");
    assert_eq!(issues(json!({"surface": panel("x")["surface"], "catalogId": "https://example.com/c", "root": panel("x")["root"]}))[0].path, "catalogId");
    assert_eq!(issues(json!({"surface": panel("x")["surface"], "root": panel("x")["root"], "data": 3}))[0].path, "data");
    let props = issues(json!({"surface": panel("x")["surface"], "root": {"id": "t", "component": "Text", "props": {"text": 3}}}));
    assert_eq!(props[0].path, "#t");
    let style = issues(json!({"surface": panel("x")["surface"], "root": {"id": "b", "component": "Box", "props": {"style": {"gap": "$spacing.nope"}}}}));
    assert_eq!(style[0].path, "#b", "{style:?}");
}

#[test]
fn the_flat_form_and_data_build_a_surface() {
    let source = json!({
        "surface": {"size": {"width": 320, "height": 200}},
        "components": [
            {"id": "root", "component": "Box", "children": ["title"]},
            {"id": "title", "component": "Text", "text": {"path": "/title"}}
        ],
        "data": {"title": "Files"}
    });
    let doc = ede_ui::validate_document("bar", &source).unwrap();
    assert!(matches!(doc.body, Body::Flat(_)));
    assert_eq!(doc.tree.id, "root");
    let no_root = json!({"surface": panel("x")["surface"], "components": [{"id": "bar", "component": "Box"}]});
    assert_eq!(ede_ui::validate_document("bar", &no_root).unwrap_err()[0].path, "#root");
    let mut surface = doc.surface(eui_core::themes::default_theme());
    assert_eq!(surface.get_data("/title"), Some(&json!("Files")));
    assert!(surface.issues().is_empty(), "{:?}", surface.issues());
    assert_eq!(surface.root().unwrap().children[0].id, "title");
}

#[test]
fn the_theme_applies_falls_back_and_resets() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut ui, _) = open(tmp.path());
    let changes = ui.write_theme(&orange(), "Orange").unwrap();
    assert!(matches!(changes.as_slice(), [Change::ThemeApplied { builtin: false, commit: Some(_), .. }]), "{changes:?}");
    assert_eq!(ui.theme().id, "orange");

    let mut bad = orange();
    bad["modes"]["dark"]["color"]["primary"] = json!("orange-ish");
    assert!(matches!(ui.write_theme(&bad, "m"), Err(EditError::Invalid(_))));
    fs::write(tmp.path().join(THEME_FILE), serde_json::to_vec(&bad).unwrap()).unwrap();
    let changes = ui.reload().unwrap();
    assert!(matches!(changes.as_slice(), [Change::ThemeRejected { fallback, .. }] if fallback == "orange"), "{changes:?}");
    assert_eq!(ui.theme().id, "orange");
    drop(ui);
    let (mut ui, _) = open(tmp.path());
    assert_eq!(ui.theme().id, "orange", "the last good theme from git");

    ui.discard("theme").unwrap();
    let changes = ui.remove("theme", "Back to the default").unwrap();
    assert!(matches!(changes.as_slice(), [Change::ThemeApplied { builtin: true, commit: Some(_), .. }]), "{changes:?}");
    assert_eq!(ui.theme().id, "exponential");
    assert!(!tmp.path().join(THEME_FILE).exists());
}

#[test]
fn undo_reverts_the_newest_edit() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut ui, _) = open(tmp.path());
    assert!(ui.undo().unwrap().is_none(), "the first commit is never undone");
    ui.write("panel", &panel("12:00"), "Clock").unwrap();
    ui.write("panel", &panel("13:00"), "Later").unwrap();
    ui.write("dock", &panel("dock"), "Dock").unwrap();

    let (commit, changes) = ui.undo().unwrap().unwrap();
    assert_eq!(commit.summary, "Undo: Dock");
    assert!(matches!(changes.as_slice(), [Change::Removed { name, .. }] if name == "dock"), "{changes:?}");
    assert!(ui.document("dock").is_none());

    let (commit, _) = ui.undo().unwrap().unwrap();
    assert_eq!(commit.summary, "Undo: Undo: Dock", "an undo is undoable");
    assert!(ui.document("dock").is_some());

    // A pending hand edit in a file the newest edit touched blocks the undo.
    ui.write("panel", &panel("14:00"), "Even later").unwrap();
    fs::write(tmp.path().join("panel.json"), b"oops").unwrap();
    ui.reload().unwrap();
    let err = ui.undo().unwrap_err();
    assert!(err.to_string().contains("panel.json has an uncommitted edit"), "{err}");
    ui.discard("panel").unwrap();

    let (commit, changes) = ui.undo().unwrap().unwrap();
    assert_eq!(commit.summary, "Undo: Even later");
    assert!(matches!(changes.as_slice(), [Change::Applied { name, commit: None }] if name == "panel"), "{changes:?}");
    assert_eq!(text_of(&ui, "panel"), "13:00");
}

#[test]
fn the_watcher_reloads_on_change() {
    let tmp = tempfile::tempdir().unwrap();
    let (ui, _) = open(tmp.path());
    let ui = Arc::new(Mutex::new(ui));
    let (tx, rx) = mpsc::channel();
    let _watcher = ede_ui::watch(ui.clone(), move |changes| tx.send(changes).unwrap()).unwrap();

    fs::write(tmp.path().join("panel.json"), serde_json::to_vec(&panel("12:00")).unwrap()).unwrap();
    let changes = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(matches!(changes.as_slice(), [Change::Applied { commit: Some(_), .. }]), "{changes:?}");
    assert_eq!(text_of(&ui.lock().unwrap(), "panel"), "12:00");

    fs::write(tmp.path().join("panel.json"), serde_json::to_vec(&broken()).unwrap()).unwrap();
    let changes = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(matches!(changes.as_slice(), [Change::Rejected { fallback: true, .. }]), "{changes:?}");
    assert_eq!(text_of(&ui.lock().unwrap(), "panel"), "12:00");

    // An edit through the API reloads once, here; the watcher sees nothing new.
    ui.lock().unwrap().write("panel", &panel("13:00"), "Set").unwrap();
    assert!(rx.recv_timeout(Duration::from_millis(600)).is_err());

    fs::remove_file(tmp.path().join("panel.json")).unwrap();
    let changes = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(matches!(changes.as_slice(), [Change::Removed { commit: Some(_), .. }]), "{changes:?}");
}

#[test]
fn changes_serialize_as_events() {
    let c = Change::Rejected { name: "panel".into(), issues: vec![ede_ui::Issue::new("#x", "bad")], fallback: true };
    assert_eq!(c.kind(), "ui.rejected");
    assert_eq!(
        serde_json::to_value(&c).unwrap(),
        json!({"kind": "ui.rejected", "name": "panel", "issues": [{"path": "#x", "message": "bad"}], "fallback": true})
    );
}
