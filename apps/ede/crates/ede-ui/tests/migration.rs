//! #EDE-216: the one-time migration of `<name>.json` documents to
//! `<name>.express` (express.json's MIGRATION), and that `.json` documents
//! are never read again.

use std::{fs, path::Path, process::Command};

use ede_ui::{take_notices, Change, Runtime, MIGRATION_MESSAGE, VERSION};
use serde_json::{json, Value};

fn panel(text: &str) -> Value {
    json!({
        "$comment": "the top panel",
        "surface": {"anchor": ["top", "left", "right"], "size": {"height": 40}, "exclusiveZone": 40},
        "root": {"id": "panel", "component": "Box", "children": [{"id": "clock", "component": "Text", "props": {"text": text}}]}
    })
}

fn json_bytes(doc: &Value) -> Vec<u8> {
    serde_json::to_vec_pretty(doc).unwrap()
}

/// A commit as an older release made it.
fn commit(dir: &Path, file: &str, bytes: &[u8], message: &str) {
    fs::write(dir.join(file), bytes).unwrap();
    for args in [vec!["add", "--", file], vec!["commit", "--quiet", "-m", message]] {
        let ok = Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=t", "-c", "user.email=t@localhost", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null"]).args(&args).status().unwrap().success();
        assert!(ok, "git {args:?}");
    }
}

fn summaries(ui: &Runtime) -> Vec<String> {
    ui.log(50).unwrap().into_iter().map(|c| c.summary).collect()
}

fn text_of(ui: &Runtime, name: &str) -> String {
    ui.document(name).expect("applied").tree.children[0].props["text"].as_str().unwrap().to_string()
}

/// A ui directory as an older release left it: the factory taskbar never
/// touched, a panel the person edited, a dock that is not a document, the
/// theme; the factory ships Express now.
fn older_release() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let (dir, factory) = (tmp.path().join("ui"), tmp.path().join("factory"));
    fs::create_dir_all(&factory).unwrap();
    drop(Runtime::open(&dir).unwrap());
    commit(&dir, "taskbar.json", &json_bytes(&panel("old factory")), "Factory taskbar");
    commit(&dir, "panel.json", &json_bytes(&panel("factory")), "Factory panel");
    commit(&dir, "panel.json", &json_bytes(&panel("mine")), "Edit panel.json");
    commit(&dir, "dock.json", br#"{"root": {"id": "dock", "component": "Box"}, "components": []}"#, "Edit dock.json");
    fs::write(dir.join("theme.json"), b"{}").unwrap();
    for (name, text) in [("taskbar", "factory"), ("panel", "factory")] {
        fs::write(factory.join(format!("{name}.express")), ede_ui::decompile_document(name, &panel(text)).unwrap()).unwrap();
    }
    (tmp, dir, factory)
}

#[test]
fn json_documents_become_express_in_one_commit() {
    let (_tmp, dir, factory) = older_release();
    let (ui, changes) = Runtime::open_with_factory(&dir, &factory).unwrap();

    // No .json document is left; the theme stays JSON.
    for name in ["taskbar", "panel", "dock"] {
        assert!(!dir.join(format!("{name}.json")).exists(), "{name}.json");
    }
    assert!(dir.join("theme.json").exists());

    // The person's edit, decompiled: what they had, now Express.
    assert_eq!(fs::read_to_string(dir.join("panel.express")).unwrap(), ede_ui::decompile_document("panel", &panel("mine")).unwrap());
    assert_eq!(text_of(&ui, "panel"), "mine");
    assert!(fs::read_to_string(dir.join("panel.express")).unwrap().starts_with("# the top panel\n"));

    // The unedited factory copy: the shipped Express, as a factory commit.
    assert_eq!(fs::read(dir.join("taskbar.express")).unwrap(), fs::read(factory.join("taskbar.express")).unwrap());
    assert_eq!(text_of(&ui, "taskbar"), "factory");

    // One that does not convert is set aside, with a notice.
    let backup = format!("dock.json.refused-{VERSION}");
    assert!(dir.join(&backup).exists());
    assert!(ui.document("dock").is_none());
    let notices = take_notices(&dir);
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].toast().0, "Your dock.json was reset");

    let log = summaries(&ui);
    assert_eq!(log.iter().filter(|s| *s == MIGRATION_MESSAGE).count(), 1, "{log:?}");
    assert_eq!(&log[..2], ["Factory taskbar", MIGRATION_MESSAGE], "{log:?}");
    assert!(!changes.iter().any(|c| matches!(c, Change::Rejected { .. })), "{changes:?}");

    // The unedited copy keeps following the factory; the edited one stays the person's.
    drop(ui);
    for name in ["taskbar", "panel"] {
        fs::write(factory.join(format!("{name}.express")), ede_ui::decompile_document(name, &panel("v2")).unwrap()).unwrap();
    }
    let (ui, _) = Runtime::open_with_factory(&dir, &factory).unwrap();
    assert_eq!(text_of(&ui, "taskbar"), "v2");
    assert_eq!(text_of(&ui, "panel"), "mine");
    assert_eq!(summaries(&ui).iter().filter(|s| *s == MIGRATION_MESSAGE).count(), 1, "once");
}

#[test]
fn a_json_document_is_never_read_again() {
    let (_tmp, dir, factory) = older_release();
    let (mut ui, _) = Runtime::open_with_factory(&dir, &factory).unwrap();
    // A .json written beside the .express (an old tool, a stale copy) is ignored.
    fs::write(dir.join("panel.json"), json_bytes(&panel("stale"))).unwrap();
    assert_eq!(ui.reload().unwrap(), []);
    assert_eq!(text_of(&ui, "panel"), "mine");
    drop(ui);
    let (ui, _) = Runtime::open_with_factory(&dir, &factory).unwrap();
    assert_eq!(text_of(&ui, "panel"), "mine");
    assert!(dir.join("panel.json").exists(), "never migrated over an .express");
}

/// The shell's read-only runtime never migrates (the supervisor owns git).
#[test]
fn the_read_only_runtime_does_not_migrate() {
    let (_tmp, dir, _factory) = older_release();
    let (ui, _) = Runtime::open_readonly(&dir).unwrap();
    assert!(dir.join("panel.json").exists());
    assert!(ui.document("panel").is_none());
}

/// A bad Express file is refused with issues that name its lines.
#[test]
fn a_bad_express_file_gives_readable_issues() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut ui, _) = Runtime::open(tmp.path()).unwrap();
    ui.write("panel", &panel("12:00"), "Clock").unwrap();
    fs::write(
        tmp.path().join("panel.express"),
        "# the top panel\nsurface(\"panel\", extensions={ede: {anchor: [\"top\", \"left\", \"right\"], size: {height: 40}}})\nroot = Box(children=[clock])\nclock = Clock(format=\"HH:mm\")\n",
    )
    .unwrap();
    let changes = ui.reload().unwrap();
    let [Change::Rejected { issues, fallback: true, .. }] = changes.as_slice() else { panic!("{changes:?}") };
    assert_eq!(issues.iter().map(ToString::to_string).collect::<Vec<_>>(), ["line 4 #clock: unknown component Clock"]);
    assert_eq!(text_of(&ui, "panel"), "12:00", "the last good version stays");
}
