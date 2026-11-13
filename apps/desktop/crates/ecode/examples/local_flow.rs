//! The Rerusted proof (#RUST-9): a whole PR-tree flow with NO server. A
//! repository is created, two branches stack on each other, a PR opens on a
//! non-default base, gets retargeted, and both land by an ordered merge on
//! the `local` provider, its PRs in a file Store.
//!
//! `cargo run -p ecode --example local_flow [dir]`; CI runs it as the test
//! `tests/local_flow.rs`.

use std::collections::BTreeMap;
use std::path::Path;

use ecode::{Event, FileStore, Forge, Identity, LocalForge, LocalRepo, OpenPr, PushSource};

/// What the flow ended with.
#[derive(Debug)]
pub struct Summary {
    pub main: String,
    pub merged: Vec<(i64, String)>,
    pub events: Vec<Event>,
    pub log: Vec<String>,
}

fn change(path: &str, content: &str) -> BTreeMap<String, Option<Vec<u8>>> {
    BTreeMap::from([(path.to_string(), Some(content.as_bytes().to_vec()))])
}

/// Runs the flow in `dir` (created; must be empty).
pub fn run(dir: &Path) -> ecode::Result<Summary> {
    // init: a bare repository and a file Store beside it.
    let repo = LocalRepo::init_bare(dir.join("repo.git"))?
        .with_identity(Identity { name: "Rerusted".into(), email: "agent@rerusted.local".into() });
    let store = FileStore::open(dir.join("store"))?;
    let forge = LocalForge::new(repo, store, "main");

    // main, then `api` on main and `ui` stacked on `api`.
    let root = forge.push("main", PushSource::Commit { parent: None, changes: change("README.md", "hello\n"), message: "init".into() }, false)?;
    let api = forge.push("api", PushSource::Commit { parent: Some(root.clone()), changes: change("api.rs", "fn api() {}\n"), message: "api".into() }, false)?;
    forge.push("ui", PushSource::Commit { parent: Some(api.clone()), changes: change("ui.rs", "fn ui() {}\n"), message: "ui".into() }, false)?;
    // A scratch branch to show retargeting.
    forge.push("scratch", PushSource::Sha(root.clone()), false)?;

    // Open the PR tree: `api` on main, `ui` on a NON-default base first.
    let first = forge.open(OpenPr { head: "api".into(), base: "main".into(), title: "API".into(), draft: false })?;
    let second = forge.open(OpenPr { head: "ui".into(), base: "scratch".into(), title: "UI".into(), draft: false })?;
    // Retarget it onto its real parent.
    forge.retarget(second, "api")?;
    assert_eq!(forge.list_by_base("api")?, [second]);

    // Ordered merge: the root first, then its child (moved onto main by the first merge).
    let out = forge.merge_ordered(&[first, second])?;
    if let Some(failed) = out.failed {
        return Err(ecode::Error::Refused(failed.failure));
    }
    let main = forge.branch("main")?.expect("main exists");
    let log = forge.repo().log("main", 10)?.into_iter().map(|e| e.message.lines().next().unwrap_or_default().to_string()).collect();
    Ok(Summary {
        main,
        merged: out.merged.into_iter().map(|m| (m.number, m.sha)).collect(),
        events: forge.events(None)?.events,
        log,
    })
}

#[allow(dead_code)]
fn main() {
    let dir = std::env::args().nth(1).map(std::path::PathBuf::from).unwrap_or_else(|| {
        std::env::temp_dir().join(format!("ecode-local-flow-{}", std::process::id()))
    });
    std::fs::create_dir_all(&dir).expect("create the directory");
    let summary = run(&dir).expect("the flow runs");
    println!("main = {}", summary.main);
    for (number, sha) in &summary.merged {
        println!("merged #{number} as {sha}");
    }
    println!("log: {:?}", summary.log);
    println!("{} events", summary.events.len());
}
