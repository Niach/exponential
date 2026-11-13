//! LocalRepo on a real repository: init/open, commits, log, diff, revert
//! (eDE's undo, #NEW-107), refs by compare-and-swap, mergeability, the argv
//! squash and bundles.

use std::collections::BTreeMap;
use std::path::PathBuf;

use ecode::core::Mergeability;
use ecode::{Change, Error, Expected, Identity, LocalRepo, Reverted, StoreError};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("ecode-repo-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn files(pairs: &[(&str, Option<&str>)]) -> BTreeMap<String, Option<Vec<u8>>> {
    pairs.iter().map(|(p, c)| (p.to_string(), c.map(|c| c.as_bytes().to_vec()))).collect()
}

fn me() -> Identity {
    Identity { name: "Test".into(), email: "test@ecode.test".into() }
}

#[test]
fn commit_log_diff_and_revert_as_undo() {
    let dir = TempDir::new("undo");
    let repo = LocalRepo::init(&dir.0).unwrap().with_identity(me());
    assert!(repo.work_dir().is_some());
    let first = repo.commit_on("main", &files(&[("shell.json", Some("{\"v\":1}")), ("nested/a.txt", Some("a"))]), "one").unwrap();
    let second = repo.commit_on("main", &files(&[("shell.json", Some("{\"v\":2}"))]), "two").unwrap();
    let third = repo.commit_on("main", &files(&[("nested/a.txt", None), ("b.txt", Some("b"))]), "three").unwrap();

    let log = repo.log("main", 10).unwrap();
    assert_eq!(log.iter().map(|e| e.sha.as_str()).collect::<Vec<_>>(), [&third, &second, &first]);
    assert_eq!(log[0].message, "three");
    assert_eq!(log[0].author, "Test");
    assert_eq!(log[1].parents, std::slice::from_ref(&first));

    let diff = repo.diff(Some(&second), &third).unwrap();
    let kinds: Vec<_> = diff.iter().map(|f| (f.path.as_str(), f.change)).collect();
    assert_eq!(kinds, [("b.txt", Change::Added), ("nested/a.txt", Change::Removed)]);

    // Undo the second edit: shell.json goes back to v1, the third stays.
    let Reverted::Commit(undo) = repo.revert("main", &second, "undo two").unwrap() else { panic!("conflict") };
    assert_eq!(repo.read_file(&undo, "shell.json").unwrap().unwrap(), b"{\"v\":1}");
    assert_eq!(repo.read_file(&undo, "b.txt").unwrap().unwrap(), b"b");
    assert_eq!(repo.branch("main").unwrap(), Some(undo.clone()));

    // A later edit of the same path makes the old revert conflict.
    repo.commit_on("main", &files(&[("shell.json", Some("{\"v\":3}"))]), "four").unwrap();
    assert_eq!(repo.revert("main", &second, "undo two again").unwrap(), Reverted::Conflicts(vec!["shell.json".into()]));

    // Reopen from disk.
    let reopened = LocalRepo::open(&dir.0).unwrap();
    assert_eq!(reopened.log("main", 1).unwrap()[0].message, "four");
}

#[test]
fn refs_move_by_compare_and_swap() {
    let dir = TempDir::new("cas");
    let repo = LocalRepo::init_bare(&dir.0).unwrap().with_identity(me());
    let a = repo.commit(None, &files(&[("a", Some("1"))]), "a").unwrap();
    let b = repo.commit(Some(&a), &files(&[("a", Some("2"))]), "b").unwrap();
    repo.set_branch("main", &a, Expected::Absent).unwrap();
    assert!(matches!(repo.set_branch("main", &b, Expected::Absent), Err(Error::Store(StoreError::Conflict { .. }))));
    assert!(matches!(repo.set_branch("main", &b, Expected::Is(&b)), Err(Error::Store(StoreError::Conflict { .. }))));
    repo.set_branch("main", &b, Expected::Is(&a)).unwrap();
    assert!(repo.is_ancestor(&a, &b).unwrap());
    assert!(!repo.is_ancestor(&b, &a).unwrap());
    repo.delete_branch("main", Expected::Is(&b)).unwrap();
    assert_eq!(repo.branch("main").unwrap(), None);
    assert_eq!(repo.resolve(&a).unwrap(), Some(a.clone()));
    assert_eq!(repo.resolve("nope").unwrap(), None);
}

#[test]
fn mergeability_squash_and_bundles() {
    let dir = TempDir::new("merge");
    let repo = LocalRepo::init_bare(dir.0.join("origin.git")).unwrap().with_identity(me());
    let base = repo.commit(None, &files(&[("a", Some("1")), ("b", Some("1"))]), "base").unwrap();
    let main = repo.commit(Some(&base), &files(&[("a", Some("main"))]), "main").unwrap();
    let clean = repo.commit(Some(&base), &files(&[("b", Some("feature"))]), "clean").unwrap();
    let dirty = repo.commit(Some(&base), &files(&[("a", Some("other"))]), "dirty").unwrap();

    let (check, _) = repo.mergeability(&main, &clean).unwrap();
    assert_eq!(check.mergeability, Mergeability::Clean);
    let (check, _) = repo.mergeability(&main, &dirty).unwrap();
    assert_eq!((check.mergeability, check.conflicts), (Mergeability::Dirty, vec!["a".to_string()]));

    let squash = repo.squash_commit(&main, &clean, "land").unwrap();
    assert_eq!(repo.log(&squash, 1).unwrap()[0].parents, std::slice::from_ref(&main));
    assert_eq!(repo.read_file(&squash, "a").unwrap().unwrap(), b"main");
    assert_eq!(repo.read_file(&squash, "b").unwrap().unwrap(), b"feature");
    assert_eq!(repo.squash_commit(&main, &dirty, "no").unwrap_err().reason(), Some(ecode::core::ForgeReason::Dirty));

    // A bundle of main past `base`, verified in a clone that has `base`.
    repo.set_branch("main", &squash, Expected::Any).unwrap();
    let bundle = dir.0.join("main.bundle");
    let info = repo.bundle_create(&bundle, &["main"], &[&base]).unwrap();
    assert_eq!(info.refs, [("refs/heads/main".to_string(), squash.clone())]);
    assert_eq!(info.prerequisites, std::slice::from_ref(&base));
    let other = LocalRepo::init_bare(dir.0.join("other.git")).unwrap();
    assert!(other.bundle_verify(&bundle).is_err(), "the prerequisite is missing");
    repo.bundle_create(&dir.0.join("base.bundle"), &["refs/heads/main"], &[]).unwrap();
    assert_eq!(repo.bundle_verify(&dir.0.join("base.bundle")).unwrap().prerequisites, Vec::<String>::new());
}
