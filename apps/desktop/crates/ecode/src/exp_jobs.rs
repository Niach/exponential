//! The git jobs a team device runs for eCode's own forge (#EXP-1616): the
//! server queues a `device_commands` row of kind `ecode_job`, the daemon
//! hands its `args` here with a cache directory, and completes the row with
//! the JSON this returns (`{ok: true, result}` or `{ok: false, reason,
//! message}`). A job only WRITES OBJECTS (a squash or rebase commit goes up
//! as a pack, a backup as a bundle); every ref move stays the server's CAS.
//!
//! The cache is a bare repository per repo, filled from the bucket by
//! [`ExpGit::fetch`] and its refs mirrored from the server before each job.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{json, Value};

use crate::exp_git::{git_at, mirror_refs, scratch, ExpGit};
use crate::{Credentials, Error, GitRemote, Identity, LocalRepo, PushSpec, Remote, Result};

/// The committer of the commits a job makes (squash, rebase).
fn job_identity() -> Identity {
    Identity { name: "Exponential".into(), email: "noreply@exponential.at".into() }
}

/// Opens (or creates) the job cache at `cache` and brings it up to the
/// remote: every object, and refs equal to the server's.
pub fn sync_cache(git: &ExpGit, cache: &Path) -> Result<(LocalRepo, BTreeMap<String, String>)> {
    let repo = if cache.join("HEAD").exists() { LocalRepo::open(cache)? } else { LocalRepo::init_bare(cache)? };
    let refs = git.refs()?;
    let wanted: Vec<&str> = refs.refs.values().map(String::as_str).collect();
    git.fetch_wanted(cache, &wanted)?;
    mirror_refs(cache, &refs.refs)?;
    Ok((repo.with_identity(job_identity()), refs.refs))
}

fn arg<'a>(args: &'a Value, name: &str) -> Result<&'a str> {
    args[name].as_str().ok_or_else(|| Error::Invalid(format!("the job has no `{name}`")))
}

/// `git diff --numstat` of `from..to`: path → (additions, deletions).
fn numstat(cache: &Path, from: Option<&str>, to: &str) -> BTreeMap<String, (u64, u64)> {
    let empty_tree = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
    let out = git_at(cache).args(["diff", "--numstat", "--no-renames", from.unwrap_or(empty_tree), to]).run().unwrap_or_default();
    out.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            let (a, d, path) = (parts.next()?, parts.next()?, parts.next()?);
            Some((path.to_string(), (a.parse().unwrap_or(0), d.parse().unwrap_or(0))))
        })
        .collect()
}

/// One job. `cache` = this repo's bare cache directory.
pub fn run_job(git: &ExpGit, cache: &Path, args: &Value) -> Result<Value> {
    let op = arg(args, "op")?;
    let (repo, refs) = sync_cache(git, cache)?;
    match op {
        "mergeability" => {
            let (check, _) = repo.mergeability(arg(args, "base")?, arg(args, "head")?)?;
            Ok(serde_json::to_value(check).unwrap_or(Value::Null))
        }
        "squash" => {
            let sha = repo.squash_commit(arg(args, "base")?, arg(args, "head")?, arg(args, "message")?)?;
            git.send_objects(cache, std::slice::from_ref(&sha))?;
            Ok(json!({ "sha": sha }))
        }
        "rebase_onto" => {
            let sha = repo.rebase_commits(arg(args, "tip")?, arg(args, "onto")?, arg(args, "upstream")?)?;
            git.send_objects(cache, std::slice::from_ref(&sha))?;
            Ok(json!({ "sha": sha }))
        }
        "files" => {
            let (base, head) = (arg(args, "base")?, arg(args, "head")?);
            let origin = repo.merge_base(base, head)?;
            let stats = numstat(cache, origin.as_deref(), head);
            let files: Vec<Value> = repo
                .diff(origin.as_deref(), head)?
                .into_iter()
                .map(|file| {
                    let (additions, deletions) = stats.get(&file.path).copied().unwrap_or((0, 0));
                    json!({ "path": file.path, "previousPath": null, "change": file.change, "additions": additions, "deletions": deletions, "patch": null })
                })
                .collect();
            Ok(json!({ "files": files }))
        }
        "is_ancestor" => Ok(json!({ "yes": repo.is_ancestor(arg(args, "ancestor")?, arg(args, "of")?)? })),
        "backup" => backup(git, cache, &repo, &refs),
        "restore" => restore(git, arg(args, "bundle")?),
        "repack" => repack(git, cache),
        "mirror" => mirror(&repo, &refs, args),
        other => Err(Error::Invalid(format!("unknown eCode job `{other}`"))),
    }
}

/// A bundle snapshot of every ref into the bucket.
fn backup(git: &ExpGit, cache: &Path, repo: &LocalRepo, refs: &BTreeMap<String, String>) -> Result<Value> {
    if refs.is_empty() {
        return Ok(json!({ "skipped": "empty repository" }));
    }
    let path = scratch("backup.bundle");
    let names: Vec<&str> = refs.keys().map(String::as_str).collect();
    let created = repo.bundle_create(&path, &names, &[]);
    let result = created.and_then(|info| {
        let key = git.upload("bundle", &path)?;
        let held: BTreeMap<String, String> = info.refs.into_iter().collect();
        git.record_bundle(&key, &held)?;
        Ok(json!({ "key": key, "refs": held.len() }))
    });
    let _ = std::fs::remove_file(&path);
    let _ = cache;
    result
}

/// A backup becomes the repo again: its objects as ONE pack, its refs as
/// every ref (the server swaps both in one transaction).
fn restore(git: &ExpGit, bundle_key: &str) -> Result<Value> {
    let bundle = scratch("restore.bundle");
    let fresh = scratch("restore.git");
    let pack = scratch("restore.pack");
    let result = (|| {
        git.download(&git.object_url(bundle_key)?, &bundle)?;
        let repo = LocalRepo::init_bare(&fresh)?;
        let info = repo.bundle_verify(&bundle)?;
        if !info.prerequisites.is_empty() {
            return Err(Error::Invalid("the backup is not a full bundle".into()));
        }
        repo.git().args(["fetch", "--quiet", "--no-tags"]).arg(bundle.as_os_str()).arg("+refs/*:refs/*").run()?;
        let refs: BTreeMap<String, String> = info.refs.into_iter().filter(|(name, _)| name.starts_with("refs/")).collect();
        git_at(&fresh).args(["pack-objects", "--all", "--stdout", "--quiet"]).run_files(None, Some(&pack))?;
        let key = git.upload("pack", &pack)?;
        git.record_restore(&key, &refs)?;
        Ok(json!({ "key": key, "refs": refs.len() }))
    })();
    let _ = std::fs::remove_file(&bundle);
    let _ = std::fs::remove_file(&pack);
    let _ = std::fs::remove_dir_all(&fresh);
    result
}

/// Every live pack becomes one (every object kept, reachable or not: a
/// squash commit uploaded a moment before its ref moves must survive).
fn repack(git: &ExpGit, cache: &Path) -> Result<Value> {
    let packs = git.packs(0)?;
    if packs.len() < 2 {
        return Ok(json!({ "skipped": "nothing to repack" }));
    }
    let fresh = scratch("repack.git");
    let list = scratch("repack.objects");
    let pack = scratch("repack.pack");
    let result = (|| {
        LocalRepo::init_bare(&fresh)?;
        for remote in &packs {
            let file = scratch("repack-in.pack");
            git.download(&remote.url, &file)?;
            let indexed = git_at(&fresh).args(["index-pack", "--stdin"]).run_files(Some(&file), None);
            let _ = std::fs::remove_file(&file);
            indexed?;
        }
        git_at(&fresh)
            .args(["cat-file", "--batch-all-objects", "--batch-check=%(objectname)"])
            .run_files(None, Some(&list))?;
        git_at(&fresh).args(["pack-objects", "--stdout", "--quiet"]).run_files(Some(&list), Some(&pack))?;
        let key = git.upload("pack", &pack)?;
        let replaces: Vec<String> = packs.iter().map(|p| p.id.clone()).collect();
        git.record_repack(&key, &replaces)?;
        Ok(json!({ "key": key, "replaced": replaces.len() }))
    })();
    for path in [&list, &pack] {
        let _ = std::fs::remove_file(path);
    }
    let _ = std::fs::remove_dir_all(&fresh);
    let _ = cache;
    result
}

/// Every branch and tag to the mirror, forced (it equals the bucket).
fn mirror(repo: &LocalRepo, refs: &BTreeMap<String, String>, args: &Value) -> Result<Value> {
    let url = arg(args, "url")?;
    let specs: Vec<PushSpec> = refs
        .iter()
        .filter(|(name, _)| name.starts_with("refs/heads/") || name.starts_with("refs/tags/"))
        .map(|(name, sha)| PushSpec { src: sha.clone(), dst: name.clone(), force: true })
        .collect();
    if specs.is_empty() {
        return Ok(json!({ "skipped": "empty repository" }));
    }
    match (args["username"].as_str(), args["token"].as_str()) {
        (Some(username), Some(token)) => GitRemote::new(Credentials::new(url, username, token, None)?)?.push(&repo.git_dir(), &specs)?,
        _ => GitRemote::anonymous(url).push(&repo.git_dir(), &specs)?,
    }
    Ok(json!({ "pushed": specs.len() }))
}

/// The answer a job row carries: `{ok, result}` or `{ok: false, reason, message}`.
pub fn job_answer(outcome: Result<Value>) -> (bool, String) {
    match outcome {
        Ok(result) => (true, json!({ "ok": true, "result": result }).to_string()),
        Err(error) => {
            let message = error.to_string();
            let answer = match error.reason() {
                Some(reason) => json!({ "ok": false, "reason": reason.as_str(), "message": message }),
                None => json!({ "ok": false, "message": message }),
            };
            (false, answer.to_string())
        }
    }
}
