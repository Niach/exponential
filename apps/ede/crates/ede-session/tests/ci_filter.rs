//! #EDE-140: test.yml's `ede` path filter runs this workspace's tests when a
//! PR touches anything it builds from outside apps/ede: every path package
//! (emcp, crash-monitor, ede-protocol, ...) and every file a source names by
//! a relative path (`include_str!`, `include_bytes!`, a fixture macro). A
//! moved design token or a changed emcp signature otherwise went green and
//! broke the next eDE PR. The filter is matched with `grep -E`, as the
//! `changes` job matches it.

use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..").canonicalize().unwrap()
}

/// The `ede='^(...)'` line of test.yml's `changes` job.
fn filter(repo: &Path) -> String {
    let yml = std::fs::read_to_string(repo.join(".github/workflows/test.yml")).unwrap();
    let line = yml.lines().map(str::trim).find(|line| line.starts_with("ede='")).expect("test.yml has an `ede='...'` filter");
    line["ede='".len()..line.len() - 1].to_string()
}

/// The paths `regex` does not match, through `grep -vE`.
fn uncovered(regex: &str, paths: &[String]) -> Vec<String> {
    let mut grep = Command::new("grep").args(["-vE", regex]).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().unwrap();
    grep.stdin.take().unwrap().write_all(paths.join("\n").as_bytes()).unwrap();
    let out = grep.wait_with_output().unwrap();
    assert!(out.status.code().is_some_and(|code| code <= 1), "grep failed on the filter: {regex}");
    String::from_utf8(out.stdout).unwrap().lines().map(str::to_string).collect()
}

/// Every `.rs` file under `dir`.
fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// What a source names by a relative path that leaves its crate: a string
/// literal starting `../` (from the file) or `/../` (after
/// `CARGO_MANIFEST_DIR`), that exists. `tests_too` = a workspace member
/// (`cargo test --workspace` builds its tests); a path dependency's own
/// tests are not built here, so its files stop at `#[cfg(test)]`.
fn named_files(krate: &Path, file: &Path, tests_too: bool) -> Vec<PathBuf> {
    let text = std::fs::read_to_string(file).unwrap();
    let mut found = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if !tests_too && line.starts_with("#[cfg(test)]") {
            break;
        }
        if line.starts_with("//") {
            continue;
        }
        for literal in line.split('"').skip(1).step_by(2) {
            let candidates = if literal.starts_with("../") {
                vec![file.parent().unwrap().join(literal), krate.join(literal)]
            } else if let Some(rest) = literal.strip_prefix('/').filter(|rest| rest.starts_with("../")) {
                vec![krate.join(rest)]
            } else {
                continue;
            };
            found.extend(candidates.into_iter().filter(|path| path.exists()).filter_map(|path| path.canonicalize().ok()));
        }
    }
    found
}

#[test]
fn the_ede_path_filter_covers_what_the_workspace_builds_from_outside() {
    let repo = repo();
    let workspace = repo.join("apps/ede");
    let metadata = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--locked", "--offline"])
        .current_dir(&workspace)
        .output()
        .unwrap();
    assert!(metadata.status.success(), "cargo metadata: {}", String::from_utf8_lossy(&metadata.stderr));
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let members: Vec<&str> = metadata["workspace_members"].as_array().unwrap().iter().map(|id| id.as_str().unwrap()).collect();

    let relative = |path: &Path| path.strip_prefix(&repo).unwrap().to_string_lossy().into_owned();
    let mut inputs = Vec::new();
    for package in metadata["packages"].as_array().unwrap() {
        if !package["source"].is_null() {
            continue;
        }
        let krate = Path::new(package["manifest_path"].as_str().unwrap()).parent().unwrap().canonicalize().unwrap();
        let member = members.contains(&package["id"].as_str().unwrap());
        if !krate.starts_with(&workspace) {
            inputs.push(format!("{}/Cargo.toml", relative(&krate)));
        }
        let mut files = Vec::new();
        for dir in if member { &["src", "tests", "examples", "benches"][..] } else { &["src"][..] } {
            sources(&krate.join(dir), &mut files);
        }
        files.push(krate.join("build.rs"));
        for file in files.iter().filter(|file| file.exists()) {
            for named in named_files(&krate, file, member) {
                if !named.starts_with(&workspace) && !workspace.starts_with(&named) {
                    inputs.push(if named.is_dir() { format!("{}/", relative(&named)) } else { relative(&named) });
                }
            }
        }
    }
    inputs.sort();
    inputs.dedup();
    assert!(inputs.iter().any(|path| path == "apps/desktop/crates/ede-protocol/Cargo.toml"), "the scan found the shared crates: {inputs:?}");
    let missing = uncovered(&filter(&repo), &inputs);
    assert!(missing.is_empty(), "test.yml's `ede` filter misses what apps/ede builds from (add each to it): {missing:#?}");
}
