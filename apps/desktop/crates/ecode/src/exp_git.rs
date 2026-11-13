//! `exp://<team>/<repo>`: eCode's own git through an Exponential server
//! (#EXP-1616). The team bucket IS the remote: the server holds the refs (a
//! compare-and-swap row each) and hands out presigned URLs; packs move
//! straight between this device and the bucket. No git process runs on the
//! server.
//!
//! - fetch = download every live pack newer than this repo's cursor and
//!   `git index-pack` it (a repack or a restore makes one newer pack holding
//!   everything, so the cursor stays valid);
//! - push = `git pack-objects` of what the remote lacks (never thin), one
//!   presigned PUT, then ONE request moving every ref by CAS. A non-forced
//!   update must be a fast-forward of the remote ref (checked here, the
//!   server cannot run git); a ref that moved meanwhile is rejected whole.
//!
//! The wire is `/api/ecode/git/<op>` with a repo token as the bearer
//! (`repositories.credentials`, or a device job's token).

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

use crate::http::{client, send, with_json};
use crate::{Error, GitCommand, Result};

/// The refs the server holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RemoteRefs {
    /// The default branch as a full ref (`refs/heads/main`).
    pub head: Option<String>,
    /// Full ref name → sha.
    pub refs: BTreeMap<String, String>,
}

/// One update `git push` asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefPush {
    /// The local commit (`None` = delete the remote ref).
    pub src: Option<String>,
    /// The full remote ref.
    pub dst: String,
    pub force: bool,
}

/// A live pack of the remote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemotePack {
    pub id: String,
    pub seq: u64,
    pub bytes: u64,
    pub url: String,
}

/// The client of one repo on one server.
pub struct ExpGit {
    api: reqwest::blocking::Client,
    transfer: reqwest::blocking::Client,
    base: String,
    token: String,
}

/// The pack header's object count (`PACK`, version, count).
fn pack_objects_in(path: &Path) -> Result<u32> {
    let mut header = [0u8; 12];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut header))
        .map_err(|e| Error::git("read pack", e))?;
    if &header[..4] != b"PACK" {
        return Err(Error::git("read pack", "not a pack"));
    }
    Ok(u32::from_be_bytes([header[8], header[9], header[10], header[11]]))
}

static SCRATCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A scratch file path (removed by the caller).
pub(crate) fn scratch(name: &str) -> PathBuf {
    let n = SCRATCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::env::temp_dir().join(format!("ecode-{}-{n}-{name}", std::process::id()))
}

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

impl ExpGit {
    /// `server` = the instance (`https://app.exponential.at`), `token` = a
    /// repo token (`expg_…`).
    pub fn new(server: &str, token: impl Into<String>) -> Result<Self> {
        let transfer = {
            let _ = rustls::crypto::ring::default_provider().install_default();
            reqwest::blocking::Client::builder()
                .user_agent("git-remote-exp")
                // A pack of a big repo over a slow link.
                .timeout(Duration::from_secs(60 * 60))
                .build()
                .map_err(|e| Error::Http { status: 0, detail: e.to_string() })?
        };
        Ok(Self { api: client("git-remote-exp")?, transfer, base: server.trim_end_matches('/').to_string(), token: token.into() })
    }

    fn url(&self, op: &str) -> String {
        format!("{}/api/ecode/git/{op}", self.base)
    }

    fn get(&self, op: &str) -> Result<Value> {
        let request = self.api.get(self.url(op)).bearer_auth(&self.token);
        send(request, &[&self.token])?.ok(&[&self.token])
    }

    fn post(&self, op: &str, body: &Value) -> Result<crate::http::Response> {
        send(with_json(self.api.post(self.url(op)).bearer_auth(&self.token), body), &[&self.token])
    }

    /// The remote's refs.
    pub fn refs(&self) -> Result<RemoteRefs> {
        let body = self.get("refs")?;
        let refs = body["refs"]
            .as_object()
            .map(|m| m.iter().filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string()))).collect())
            .unwrap_or_default();
        Ok(RemoteRefs { head: body["head"].as_str().map(str::to_string), refs })
    }

    /// The live packs newer than `after` (oldest first).
    pub fn packs(&self, after: u64) -> Result<Vec<RemotePack>> {
        let body = self.get(&format!("packs?after={after}"))?;
        Ok(body["packs"]
            .as_array()
            .map(|packs| {
                packs
                    .iter()
                    .filter_map(|p| {
                        Some(RemotePack {
                            id: p["id"].as_str()?.to_string(),
                            seq: p["seq"].as_u64()?,
                            bytes: p["bytes"].as_u64().unwrap_or(0),
                            url: p["url"].as_str()?.to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Downloads a presigned URL into `path`.
    pub fn download(&self, url: &str, path: &Path) -> Result<()> {
        let mut response = self.transfer.get(url).send().map_err(|e| Error::Http { status: 0, detail: e.to_string() })?;
        if !response.status().is_success() {
            return Err(Error::Http { status: response.status().as_u16(), detail: "download from the bucket failed".into() });
        }
        let mut file = std::fs::File::create(path).map_err(|e| Error::git("write pack", e))?;
        response.copy_to(&mut file).map_err(|e| Error::Http { status: 0, detail: e.to_string() })?;
        Ok(())
    }

    /// Uploads `path` as a new `kind` (`pack` | `bundle`) object; its key.
    pub fn upload(&self, kind: &str, path: &Path) -> Result<String> {
        let bytes = std::fs::metadata(path).map_err(|e| Error::git("read upload", e))?.len();
        let grant = self.post("upload", &json!({ "kind": kind, "bytes": bytes }))?.ok(&[&self.token])?;
        let (Some(key), Some(url)) = (grant["key"].as_str(), grant["url"].as_str()) else {
            return Err(Error::Http { status: 200, detail: "upload: no key".into() });
        };
        let file = std::fs::File::open(path).map_err(|e| Error::git("read upload", e))?;
        let response = self
            .transfer
            .put(url)
            .header("content-type", "application/octet-stream")
            .header("content-length", bytes)
            .body(reqwest::blocking::Body::sized(file, bytes))
            .send()
            .map_err(|e| Error::Http { status: 0, detail: e.to_string() })?;
        if !response.status().is_success() {
            return Err(Error::Http { status: response.status().as_u16(), detail: "upload to the bucket failed".into() });
        }
        Ok(key.to_string())
    }

    /// The cursor file of `git_dir` for this server (the newest pack seq it holds).
    fn cursor_path(&self, git_dir: &Path) -> PathBuf {
        let tag = crate::content_id(format!("{}|{}", self.base, self.token.split('.').next().unwrap_or_default()).as_bytes());
        git_dir.join("ecode").join(format!("exp-{}.cursor", &tag[..16]))
    }

    fn read_cursor(&self, git_dir: &Path) -> u64 {
        std::fs::read_to_string(self.cursor_path(git_dir)).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0)
    }

    fn write_cursor(&self, git_dir: &Path, seq: u64) -> Result<()> {
        let path = self.cursor_path(git_dir);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| Error::git("write cursor", e))?;
        }
        std::fs::write(path, seq.to_string()).map_err(|e| Error::git("write cursor", e))
    }

    /// Brings every remote object into the repository at `git_dir`; the
    /// number of packs indexed. `from_start` ignores the cursor.
    pub fn fetch(&self, git_dir: &Path, from_start: bool) -> Result<usize> {
        let after = if from_start { 0 } else { self.read_cursor(git_dir) };
        let packs = self.packs(after)?;
        for pack in &packs {
            let file = Scratch(scratch("fetch.pack"));
            self.download(&pack.url, &file.0)?;
            git_at(git_dir).args(["index-pack", "--stdin", "--keep=ecode"]).run_files(Some(&file.0), None)?;
            self.write_cursor(git_dir, pack.seq)?;
        }
        Ok(packs.len())
    }

    /// Fetches until every sha in `wanted` is here (a pruned or lost local
    /// copy re-reads from the first pack).
    pub fn fetch_wanted(&self, git_dir: &Path, wanted: &[&str]) -> Result<()> {
        self.fetch(git_dir, false)?;
        if wanted.iter().all(|sha| has_object(git_dir, sha)) {
            return Ok(());
        }
        self.fetch(git_dir, true)?;
        match wanted.iter().find(|sha| !has_object(git_dir, sha)) {
            Some(sha) => Err(Error::git("fetch", format!("the remote does not hold {sha}"))),
            None => Ok(()),
        }
    }

    /// Builds a pack of `tips` minus everything the remote's `refs` reach
    /// (when here); `None` = nothing to send.
    fn pack_for(&self, git_dir: &Path, tips: &[String], remote: &RemoteRefs) -> Result<Option<Scratch>> {
        if tips.is_empty() {
            return Ok(None);
        }
        let input = Scratch(scratch("revs"));
        let mut lines: Vec<String> = tips.to_vec();
        lines.extend(remote.refs.values().filter(|sha| has_object(git_dir, sha)).map(|sha| format!("^{sha}")));
        std::fs::write(&input.0, lines.join("\n") + "\n").map_err(|e| Error::git("pack-objects", e))?;
        let pack = Scratch(scratch("push.pack"));
        git_at(git_dir).args(["pack-objects", "--stdout", "--revs", "--quiet"]).run_files(Some(&input.0), Some(&pack.0))?;
        Ok(if pack_objects_in(&pack.0)? == 0 { None } else { Some(pack) })
    }

    /// Uploads the objects of `tips` the remote lacks, moving no ref (a
    /// device job's squash or rebase commit). The pack's key, if any.
    pub fn send_objects(&self, git_dir: &Path, tips: &[String]) -> Result<Option<String>> {
        let remote = self.refs()?;
        let Some(pack) = self.pack_for(git_dir, tips, &remote)? else { return Ok(None) };
        let key = self.upload("pack", &pack.0)?;
        let answer = self.post("push", &json!({ "pack": key, "updates": [] }))?;
        answer.ok(&[&self.token])?;
        Ok(Some(key))
    }

    /// `git push`: per update, `Ok` or the reason it was refused. All
    /// accepted updates move together (one CAS); a moved remote ref fails
    /// them all with "fetch first".
    pub fn push(&self, git_dir: &Path, updates: &[RefPush]) -> Result<Vec<(String, std::result::Result<(), String>)>> {
        let remote = self.refs()?;
        let mut outcome = Vec::new();
        let mut accepted = Vec::new();
        for update in updates {
            let old = remote.refs.get(&update.dst).cloned();
            let refused = match (&update.src, &old) {
                (None, None) => Some("the remote ref does not exist".to_string()),
                (Some(new), Some(old)) if !update.force && new != old => {
                    if !has_object(git_dir, old) {
                        Some("fetch first".to_string())
                    } else if !is_ancestor(git_dir, old, new) {
                        Some("non-fast-forward".to_string())
                    } else {
                        None
                    }
                }
                _ => None,
            };
            match refused {
                Some(why) => outcome.push((update.dst.clone(), Err(why))),
                None => {
                    if update.src.as_deref() != old.as_deref() {
                        accepted.push(json!({ "ref": update.dst, "old": old, "new": update.src }));
                    }
                    outcome.push((update.dst.clone(), Ok(())));
                }
            }
        }
        if accepted.is_empty() {
            return Ok(outcome);
        }
        let tips: Vec<String> = accepted.iter().filter_map(|u| u["new"].as_str().map(str::to_string)).collect();
        let pack = self.pack_for(git_dir, &tips, &remote)?;
        let key = match &pack {
            Some(pack) => Some(self.upload("pack", &pack.0)?),
            None => None,
        };
        let answer = self.post("push", &json!({ "pack": key, "updates": accepted }))?;
        if answer.status == 409 {
            let moved = answer.body["rejected"].as_str().unwrap_or_default().to_string();
            for (dst, result) in &mut outcome {
                if result.is_ok() {
                    *result = Err(if *dst == moved { "fetch first".to_string() } else { "atomic push failed".to_string() });
                }
            }
            return Ok(outcome);
        }
        answer.ok(&[&self.token])?;
        Ok(outcome)
    }

    /// The presigned GET of one of the repo's objects (device jobs).
    pub fn object_url(&self, key: &str) -> Result<String> {
        let mut query = String::new();
        for byte in key.bytes() {
            if byte.is_ascii_alphanumeric() || b"-_./".contains(&byte) {
                query.push(byte as char);
            } else {
                query.push_str(&format!("%{byte:02X}"));
            }
        }
        let body = self.get(&format!("object?key={query}"))?;
        body["url"].as_str().map(str::to_string).ok_or_else(|| Error::Http { status: 200, detail: "object: no url".into() })
    }

    /// Records an uploaded bundle as a backup (device jobs).
    pub fn record_bundle(&self, key: &str, refs: &BTreeMap<String, String>) -> Result<()> {
        self.post("bundle", &json!({ "key": key, "refs": refs }))?.ok(&[&self.token]).map(|_| ())
    }

    /// One pack replaces `replaces` (device jobs).
    pub fn record_repack(&self, key: &str, replaces: &[String]) -> Result<()> {
        self.post("repack", &json!({ "pack": key, "replaces": replaces }))?.ok(&[&self.token]).map(|_| ())
    }

    /// One pack and `refs` replace every pack and ref (device jobs).
    pub fn record_restore(&self, key: &str, refs: &BTreeMap<String, String>) -> Result<()> {
        self.post("restore", &json!({ "pack": key, "refs": refs }))?.ok(&[&self.token]).map(|_| ())
    }
}

/// A git command on the repository at `git_dir`.
pub(crate) fn git_at(git_dir: &Path) -> GitCommand {
    GitCommand::new(None).arg(format!("--git-dir={}", git_dir.display()))
}

/// Whether `sha` is in the repository.
pub(crate) fn has_object(git_dir: &Path, sha: &str) -> bool {
    git_at(git_dir).args(["cat-file", "-e", &format!("{sha}^{{object}}")]).run().is_ok()
}

fn is_ancestor(git_dir: &Path, ancestor: &str, of: &str) -> bool {
    git_at(git_dir).args(["merge-base", "--is-ancestor", ancestor, of]).run().is_ok()
}

/// Makes the repository's refs equal `refs` (a job's cache mirrors the remote).
pub(crate) fn mirror_refs(git_dir: &Path, refs: &BTreeMap<String, String>) -> Result<()> {
    let existing = git_at(git_dir).args(["for-each-ref", "--format=%(refname)"]).run()?;
    let mut script = String::new();
    for name in existing.lines().filter(|n| !n.is_empty() && !refs.contains_key(*n)) {
        script.push_str(&format!("delete {name}\n"));
    }
    for (name, sha) in refs {
        script.push_str(&format!("update {name} {sha}\n"));
    }
    let input = Scratch(scratch("update-ref"));
    std::fs::write(&input.0, script).map_err(|e| Error::git("update-ref", e))?;
    git_at(git_dir).args(["update-ref", "--stdin"]).run_files(Some(&input.0), None)
}
