//! #EDE-202: a document an older release committed that today's checks
//! refuse is set aside once, the factory copy takes its place, and the shell
//! hears it once; an unedited factory copy follows every factory update.

use std::{fs, path::Path, process::Command};

use ede_ui::{take_notices, Change, ResetNotice, Runtime, NOTICES_FILE, VERSION};
use serde_json::{json, Value};

fn panel(text: &str) -> Value {
    json!({
        "surface": {"anchor": ["top", "left", "right"], "size": {"height": 40}, "exclusiveZone": 40},
        "root": {"id": "panel", "component": "Box", "children": [{"id": "clock", "component": "Text", "props": {"text": text}}]}
    })
}

/// What an older release accepted: a function its header never lists (as
/// the `ede.*` → `ede_*` rename of #VAPP-289 left them).
fn old_release(name: &str) -> Vec<u8> {
    let mut doc = panel("old");
    doc["root"]["on"] = json!({"press": {"functionCall": {"@call": "ede_logout"}}});
    bytes(name, &doc)
}

/// A document as its Express file (#EDE-216).
fn bytes(name: &str, doc: &Value) -> Vec<u8> {
    ede_ui::decompile_document(name, doc).unwrap().into_bytes()
}

fn ship(factory: &Path, name: &str, doc: &Value) {
    fs::write(factory.join(format!("{name}.express")), bytes(name, doc)).unwrap();
}

/// A commit an older release made (its checks let `bytes` in).
fn commit_as_older_release(dir: &Path, file: &str, bytes: &[u8]) {
    fs::write(dir.join(file), bytes).unwrap();
    for args in [vec!["add", "--", file], vec!["commit", "--quiet", "-m", &format!("Edit {file}")]] {
        let ok = Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=t", "-c", "user.email=t@localhost", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null"]).args(&args).status().unwrap().success();
        assert!(ok, "git {args:?}");
    }
}

fn setup() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let (dir, factory) = (tmp.path().join("ui"), tmp.path().join("factory"));
    fs::create_dir_all(&factory).unwrap();
    ship(&factory, "taskbar", &panel("factory"));
    (tmp, dir, factory)
}

fn summaries(ui: &Runtime) -> Vec<String> {
    ui.log(50).unwrap().into_iter().map(|c| c.summary).collect()
}

fn refused(changes: &[Change]) -> bool {
    changes.iter().any(|c| matches!(c, Change::Rejected { .. }))
}

#[test]
fn a_refused_document_from_an_older_release_is_set_aside_for_the_factory_copy() {
    let (_tmp, dir, factory) = setup();
    drop(Runtime::open_with_factory(&dir, &factory).unwrap());
    commit_as_older_release(&dir, "taskbar.express", &old_release("taskbar"));

    let (ui, changes) = Runtime::open_with_factory(&dir, &factory).unwrap();
    assert!(!refused(&changes), "{changes:?}");
    let backup = format!("taskbar.express.refused-{VERSION}");
    assert_eq!(fs::read(dir.join(&backup)).unwrap(), old_release("taskbar"), "the person's file, kept next to it");
    assert_eq!(fs::read(dir.join("taskbar.express")).unwrap(), fs::read(factory.join("taskbar.express")).unwrap());
    assert_eq!(ui.document("taskbar").unwrap().tree.children[0].props["text"], "factory");
    assert_eq!(summaries(&ui)[0], "Factory taskbar");
    assert!(ui.rejected().is_empty());
    drop(ui);

    // Once: the next start refuses nothing and sets nothing aside.
    let (ui, changes) = Runtime::open_with_factory(&dir, &factory).unwrap();
    assert!(!refused(&changes));
    assert_eq!(summaries(&ui)[0], "Factory taskbar");
    assert!(!dir.join(format!("{backup}-2")).exists());
}

#[test]
fn the_shell_hears_a_reset_once() {
    let (_tmp, dir, factory) = setup();
    drop(Runtime::open_with_factory(&dir, &factory).unwrap());
    commit_as_older_release(&dir, "taskbar.express", &old_release("taskbar"));
    drop(Runtime::open_with_factory(&dir, &factory).unwrap());

    let notices = take_notices(&dir);
    let backup = format!("taskbar.express.refused-{VERSION}");
    assert_eq!(notices, vec![ResetNotice { name: "taskbar".into(), backup: backup.clone() }]);
    let (title, body) = notices[0].toast();
    assert_eq!(title, "Your taskbar.express was reset");
    assert!(body.ends_with(&format!("Your copy is kept as {backup}.")), "{body}");
    assert!(!dir.join(NOTICES_FILE).exists());
    assert!(take_notices(&dir).is_empty(), "taken once");

    // Another start has nothing to tell.
    drop(Runtime::open_with_factory(&dir, &factory).unwrap());
    assert!(take_notices(&dir).is_empty());
}

#[test]
fn a_refused_document_the_factory_no_longer_ships_is_set_aside_and_removed() {
    let (_tmp, dir, factory) = setup();
    drop(Runtime::open_with_factory(&dir, &factory).unwrap());
    // `panel` and `menu` before the IDE-look shell (#2164) renamed them.
    commit_as_older_release(&dir, "panel.express", &old_release("panel"));

    let (ui, changes) = Runtime::open_with_factory(&dir, &factory).unwrap();
    assert!(!refused(&changes), "{changes:?}");
    assert!(!dir.join("panel.express").exists());
    assert_eq!(fs::read(dir.join(format!("panel.express.refused-{VERSION}"))).unwrap(), old_release("panel"));
    assert!(ui.document("panel").is_none());
    assert_eq!(summaries(&ui)[0], format!("Set aside panel.express: refused by eDE {VERSION}"));
    assert_eq!(take_notices(&dir).len(), 1);
    drop(ui);

    let (_ui, changes) = Runtime::open_with_factory(&dir, &factory).unwrap();
    assert!(changes.is_empty() || !refused(&changes));
    assert!(take_notices(&dir).is_empty(), "once");
}

#[test]
fn a_broken_edit_nobody_committed_stays_the_persons_to_fix() {
    let (_tmp, dir, factory) = setup();
    drop(Runtime::open_with_factory(&dir, &factory).unwrap());
    fs::write(dir.join("taskbar.express"), old_release("taskbar")).unwrap();

    let (ui, changes) = Runtime::open_with_factory(&dir, &factory).unwrap();
    assert!(refused(&changes));
    assert_eq!(fs::read(dir.join("taskbar.express")).unwrap(), old_release("taskbar"), "untouched");
    assert_eq!(ui.document("taskbar").unwrap().tree.children[0].props["text"], "factory", "the last good version stays");
    assert!(take_notices(&dir).is_empty());
    assert!(fs::read_dir(&dir).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().contains(".refused-")));
}

#[test]
fn an_unedited_older_factory_copy_follows_the_newest_factory_document() {
    let (_tmp, dir, factory) = setup();
    drop(Runtime::open_with_factory(&dir, &factory).unwrap());
    ship(&factory, "taskbar", &panel("v2"));
    drop(Runtime::open_with_factory(&dir, &factory).unwrap());
    // The file back at the first factory copy, not by a factory commit (a
    // rollback to what was drawn before the update): still never edited.
    let (mut ui, _) = Runtime::open_with_factory(&dir, &factory).unwrap();
    ui.write("taskbar", &panel("factory"), "Undo: Edit taskbar.express, rolled back").unwrap();
    drop(ui);

    ship(&factory, "taskbar", &panel("v3"));
    let (ui, _) = Runtime::open_with_factory(&dir, &factory).unwrap();
    assert_eq!(ui.document("taskbar").unwrap().tree.children[0].props["text"], "v3", "{:?}", summaries(&ui));
}

#[test]
fn an_undone_factory_update_and_an_edit_stay() {
    let (_tmp, dir, factory) = setup();
    drop(Runtime::open_with_factory(&dir, &factory).unwrap());
    ship(&factory, "taskbar", &panel("v2"));
    let (mut ui, _) = Runtime::open_with_factory(&dir, &factory).unwrap();
    assert_eq!(summaries(&ui)[0], "Factory taskbar");
    ui.undo().unwrap().expect("the update undone");
    assert_eq!(summaries(&ui)[0], "Undo: Factory taskbar");
    drop(ui);

    ship(&factory, "taskbar", &panel("v3"));
    let (mut ui, _) = Runtime::open_with_factory(&dir, &factory).unwrap();
    assert_eq!(ui.document("taskbar").unwrap().tree.children[0].props["text"], "factory", "the person's undo stays");

    ui.write("taskbar", &panel("mine"), "Edit taskbar.express").unwrap();
    drop(ui);
    ship(&factory, "taskbar", &panel("v4"));
    let (ui, _) = Runtime::open_with_factory(&dir, &factory).unwrap();
    assert_eq!(ui.document("taskbar").unwrap().tree.children[0].props["text"], "mine");
}
