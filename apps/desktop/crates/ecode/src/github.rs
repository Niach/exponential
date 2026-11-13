//! The `github` provider, server-free (#RUST-9): REST with a token (a
//! personal, OAuth or installation token) or one from the OAuth device flow
//! ([`DeviceFlow`]); no webhooks and no GitHub App, so events come from
//! polling the PR list with an ETag ([`Forge::events`]).
//!
//! Every op keeps `forge.json`'s refusals: the spec's checks run client-side
//! first (unknown branch, same branch, a second live PR, a loop), GitHub's
//! answers map onto the same reasons. What GitHub does not offer is refused
//! as `unsupported` (`rebase_onto`, its capability is off); `mergeability`
//! reports no conflict paths (the API has none).

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use ecode_core::{ForgeReason, Mergeability, PrState, Provider, PullRequest};
use reqwest::Method;
use serde_json::{json, Value};

use crate::http::{client, send, Response};
use crate::{
    Change, Credentials, Error, Event, Events, FileChange, Forge, MergeCheck, OpenPr, PushSource, Result,
};

const API: &str = "https://api.github.com";
const WEB: &str = "https://github.com";

/// What the poller last saw of a PR.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Seen {
    state: PrState,
    base: String,
    head: String,
    head_sha: String,
}

#[derive(Default)]
struct Poller {
    etag: Option<String>,
    seen: Option<BTreeMap<i64, Seen>>,
    log: Vec<Event>,
}

/// A GitHub repository as a [`Forge`].
pub struct GithubForge {
    client: reqwest::blocking::Client,
    api: String,
    /// `owner/name`.
    repo: String,
    token: String,
    default_branch: Mutex<Option<String>>,
    poller: Mutex<Poller>,
    /// How long `mergeability` waits for GitHub to compute it.
    mergeable_wait: Duration,
}

fn refuse<T>(reason: ForgeReason) -> Result<T> {
    Err(Error::refused(reason))
}

/// Standard base64 (blob uploads of non-UTF-8 bytes).
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn pr_from(value: &Value) -> PullRequest {
    let s = |p: &str| value.pointer(p).and_then(Value::as_str).map(str::to_string);
    let merged = value.get("merged").and_then(Value::as_bool).unwrap_or(false) || s("/merged_at").is_some();
    let state = if merged {
        PrState::Merged
    } else if s("/state").as_deref() == Some("closed") {
        PrState::Closed
    } else if value.get("draft").and_then(Value::as_bool).unwrap_or(false) {
        PrState::Draft
    } else {
        PrState::Open
    };
    PullRequest {
        number: value.get("number").and_then(Value::as_i64).unwrap_or_default(),
        head: s("/head/ref").unwrap_or_default(),
        base: s("/base/ref").unwrap_or_default(),
        title: s("/title").unwrap_or_default(),
        head_sha: if state.is_live() { None } else { s("/head/sha") },
        merged_sha: if merged { s("/merge_commit_sha") } else { None },
        state,
    }
}

impl GithubForge {
    /// `full_name` = `owner/name`; `token` from the user, the device flow
    /// or an installation.
    pub fn new(full_name: &str, token: impl Into<String>) -> Result<Self> {
        Ok(Self {
            client: client("ecode")?,
            api: API.into(),
            repo: full_name.into(),
            token: token.into(),
            default_branch: Mutex::new(None),
            poller: Mutex::new(Poller::default()),
            mergeable_wait: Duration::from_secs(10),
        })
    }

    /// Another API root (GitHub Enterprise, a test server).
    pub fn with_api(mut self, api: &str) -> Self {
        self.api = api.trim_end_matches('/').into();
        self
    }

    /// How long `mergeability` waits for GitHub's background computation.
    pub fn with_mergeable_wait(mut self, wait: Duration) -> Self {
        self.mergeable_wait = wait;
        self
    }

    /// The git credentials for this repository (for a [`crate::GitRemote`]).
    pub fn credentials(&self) -> Credentials {
        Credentials::github(&self.repo, self.token.clone())
    }

    fn call(&self, method: Method, path: &str, body: Option<Value>, etag: Option<&str>) -> Result<Response> {
        let url = format!("{}/repos/{}{path}", self.api, self.repo);
        let mut request = self
            .client
            .request(method, url)
            .bearer_auth(&self.token)
            .header("accept", "application/vnd.github+json")
            .header("x-github-api-version", "2022-11-28");
        if let Some(etag) = etag {
            request = request.header("if-none-match", etag);
        }
        if let Some(body) = body {
            request = crate::http::with_json(request, &body);
        }
        send(request, &[&self.token])
    }

    fn get(&self, path: &str) -> Result<Value> {
        self.call(Method::GET, path, None, None)?.ok(&[&self.token])
    }

    fn default_branch(&self) -> Result<String> {
        let mut cached = self.default_branch.lock().unwrap();
        if let Some(branch) = cached.as_ref() {
            return Ok(branch.clone());
        }
        let repo = self.call(Method::GET, "", None, None)?.ok(&[&self.token])?;
        let branch = repo.get("default_branch").and_then(Value::as_str).unwrap_or("main").to_string();
        *cached = Some(branch.clone());
        Ok(branch)
    }

    fn open_prs(&self) -> Result<Vec<PullRequest>> {
        let mut out = vec![];
        for page in 1.. {
            let list = self.get(&format!("/pulls?state=open&per_page=100&page={page}"))?;
            let items = list.as_array().cloned().unwrap_or_default();
            out.extend(items.iter().map(pr_from));
            if items.len() < 100 {
                break;
            }
        }
        Ok(out)
    }

    fn live(&self, number: i64) -> Result<PullRequest> {
        match self.pr(number)? {
            None => refuse(ForgeReason::Pr),
            Some(pr) if !pr.state.is_live() => refuse(ForgeReason::NotOpen),
            Some(pr) => Ok(pr),
        }
    }

    fn need_branch(&self, name: &str, reason: ForgeReason) -> Result<String> {
        self.branch(name)?.map_or_else(|| refuse(reason), Ok)
    }

    fn set_ref(&self, branch: &str, sha: &str, force: bool, exists: bool) -> Result<()> {
        let response = if exists {
            self.call(Method::PATCH, &format!("/git/refs/heads/{branch}"), Some(json!({ "sha": sha, "force": force })), None)?
        } else {
            self.call(Method::POST, "/git/refs", Some(json!({ "ref": format!("refs/heads/{branch}"), "sha": sha })), None)?
        };
        match response.status {
            200 | 201 => Ok(()),
            422 if response.message().to_lowercase().contains("fast forward") => refuse(ForgeReason::NonFastForward),
            422 => refuse(ForgeReason::Commit),
            _ => response.ok(&[&self.token]).map(|_| ()),
        }
    }

    fn write_commit(&self, parent: Option<&str>, changes: &BTreeMap<String, Option<Vec<u8>>>, message: &str) -> Result<String> {
        let base_tree = match parent {
            Some(parent) => {
                let response = self.call(Method::GET, &format!("/git/commits/{parent}"), None, None)?;
                if response.status == 404 || response.status == 422 {
                    return refuse(ForgeReason::Parent);
                }
                response.ok(&[&self.token])?.pointer("/tree/sha").and_then(Value::as_str).map(str::to_string)
            }
            None => None,
        };
        let mut entries = vec![];
        for (path, content) in changes {
            entries.push(match content {
                Some(bytes) => {
                    let blob = self.call(
                        Method::POST,
                        "/git/blobs",
                        Some(json!({ "content": base64(bytes), "encoding": "base64" })),
                        None,
                    )?;
                    let sha = blob.ok(&[&self.token])?.get("sha").cloned().unwrap_or(Value::Null);
                    json!({ "path": path, "mode": "100644", "type": "blob", "sha": sha })
                }
                None => json!({ "path": path, "mode": "100644", "type": "blob", "sha": null }),
            });
        }
        let mut tree_body = json!({ "tree": entries });
        if let Some(base) = base_tree {
            tree_body["base_tree"] = json!(base);
        }
        let tree = self.call(Method::POST, "/git/trees", Some(tree_body), None)?.ok(&[&self.token])?;
        let commit = self
            .call(
                Method::POST,
                "/git/commits",
                Some(json!({ "message": message, "tree": tree["sha"], "parents": parent.into_iter().collect::<Vec<_>>() })),
                None,
            )?
            .ok(&[&self.token])?;
        Ok(commit.get("sha").and_then(Value::as_str).unwrap_or_default().to_string())
    }

    /// One poll of the PR list: the events since the last one.
    fn poll(&self, poller: &mut Poller) -> Result<()> {
        let response = self.call(
            Method::GET,
            "/pulls?state=all&sort=updated&direction=desc&per_page=100",
            None,
            poller.etag.as_deref(),
        )?;
        if response.status == 304 {
            return Ok(());
        }
        let etag = response.etag.clone();
        let list = response.ok(&[&self.token])?;
        let mut now = BTreeMap::new();
        for item in list.as_array().into_iter().flatten() {
            let pr = pr_from(item);
            let head_sha = item.pointer("/head/sha").and_then(Value::as_str).unwrap_or_default().to_string();
            now.insert(pr.number, (Seen { state: pr.state, base: pr.base.clone(), head: pr.head.clone(), head_sha }, pr));
        }
        if let Some(seen) = &mut poller.seen {
            for (number, (current, pr)) in &now {
                let number = *number;
                match seen.get(&number) {
                    None => {
                        poller.log.push(Event::PrOpened { number });
                        if pr.state == PrState::Merged {
                            poller.log.push(Event::PrMerged { number, sha: pr.merged_sha.clone().unwrap_or_default() });
                        } else if pr.state == PrState::Closed {
                            poller.log.push(Event::PrClosed { number });
                        }
                    }
                    Some(before) => {
                        if before.head_sha != current.head_sha && current.state.is_live() {
                            poller.log.push(Event::Pushed { branch: current.head.clone(), sha: current.head_sha.clone(), forced: false });
                        }
                        if before.base != current.base {
                            poller.log.push(Event::PrRetargeted { number, from: before.base.clone(), to: current.base.clone() });
                        }
                        if before.state != current.state {
                            match current.state {
                                PrState::Merged => poller.log.push(Event::PrMerged { number, sha: pr.merged_sha.clone().unwrap_or_default() }),
                                PrState::Closed => poller.log.push(Event::PrClosed { number }),
                                _ => {}
                            }
                        }
                    }
                }
            }
            for (number, (current, _)) in now {
                seen.insert(number, current);
            }
        } else {
            poller.seen = Some(now.into_iter().map(|(n, (seen, _))| (n, seen)).collect());
        }
        poller.etag = etag;
        Ok(())
    }
}

impl Forge for GithubForge {
    fn provider(&self) -> Provider {
        Provider::Github
    }

    fn branch(&self, name: &str) -> Result<Option<String>> {
        let response = self.call(Method::GET, &format!("/git/ref/heads/{name}"), None, None)?;
        if response.status == 404 {
            return Ok(None);
        }
        let body = response.ok(&[&self.token])?;
        Ok(body.pointer("/object/sha").and_then(Value::as_str).map(str::to_string))
    }

    fn pr(&self, number: i64) -> Result<Option<PullRequest>> {
        let response = self.call(Method::GET, &format!("/pulls/{number}"), None, None)?;
        if response.status == 404 {
            return Ok(None);
        }
        Ok(Some(pr_from(&response.ok(&[&self.token])?)))
    }

    fn push(&self, branch: &str, source: PushSource, force: bool) -> Result<String> {
        let current = self.branch(branch)?;
        let sha = match source {
            PushSource::Sha(sha) => sha,
            PushSource::Commit { parent, changes, message } => self.write_commit(parent.as_deref(), &changes, &message)?,
        };
        self.set_ref(branch, &sha, force, current.is_some())?;
        Ok(sha)
    }

    fn open(&self, pr: OpenPr) -> Result<i64> {
        self.need_branch(&pr.head, ForgeReason::Head)?;
        self.need_branch(&pr.base, ForgeReason::Base)?;
        if pr.head == pr.base {
            return refuse(ForgeReason::SameBranch);
        }
        let open = self.open_prs()?;
        if open.iter().any(|o| o.head == pr.head) {
            return refuse(ForgeReason::Exists);
        }
        if crate::closes_loop(&open, &pr.head, &pr.base) {
            return refuse(ForgeReason::Cycle);
        }
        let response = self.call(
            Method::POST,
            "/pulls",
            Some(json!({ "head": pr.head, "base": pr.base, "title": pr.title, "draft": pr.draft })),
            None,
        )?;
        if response.status == 422 && response.message().contains("already exists") {
            return refuse(ForgeReason::Exists);
        }
        let body = response.ok(&[&self.token])?;
        Ok(body.get("number").and_then(Value::as_i64).unwrap_or_default())
    }

    fn list_by_base(&self, base: &str) -> Result<Vec<i64>> {
        let mut numbers: Vec<i64> = self.open_prs()?.iter().filter(|pr| pr.base == base).map(|pr| pr.number).collect();
        numbers.sort_unstable();
        Ok(numbers)
    }

    fn retarget(&self, number: i64, base: &str) -> Result<()> {
        let pr = self.live(number)?;
        self.need_branch(base, ForgeReason::Base)?;
        if base == pr.head {
            return refuse(ForgeReason::SameBranch);
        }
        if base == pr.base {
            return Ok(());
        }
        if crate::closes_loop(&self.open_prs()?, &pr.head, base) {
            return refuse(ForgeReason::Cycle);
        }
        self.call(Method::PATCH, &format!("/pulls/{number}"), Some(json!({ "base": base })), None)?.ok(&[&self.token])?;
        Ok(())
    }

    fn mergeability(&self, number: i64) -> Result<MergeCheck> {
        self.live(number)?;
        let started = std::time::Instant::now();
        loop {
            let pr = self.get(&format!("/pulls/{number}"))?;
            match pr.get("mergeable").and_then(Value::as_bool) {
                Some(mergeable) => {
                    let dirty = !mergeable || pr.get("mergeable_state").and_then(Value::as_str) == Some("dirty");
                    let mergeability = if dirty { Mergeability::Dirty } else { Mergeability::Clean };
                    return Ok(MergeCheck { mergeability, conflicts: vec![] });
                }
                None if started.elapsed() < self.mergeable_wait => std::thread::sleep(Duration::from_millis(500)),
                None => return Err(Error::Http { status: 202, detail: "GitHub is still computing mergeability".into() }),
            }
        }
    }

    fn squash_merge(&self, number: i64, expected_head: Option<&str>) -> Result<String> {
        let pr = self.live(number)?;
        if pr.state == PrState::Draft {
            return refuse(ForgeReason::Draft);
        }
        let head = self.need_branch(&pr.head, ForgeReason::Head)?;
        if expected_head.is_some_and(|expected| expected != head) {
            return refuse(ForgeReason::HeadMoved);
        }
        let title = if pr.title.is_empty() { pr.head.clone() } else { pr.title.clone() };
        let response = self.call(
            Method::PUT,
            &format!("/pulls/{number}/merge"),
            Some(json!({ "merge_method": "squash", "sha": head, "commit_title": format!("{title} (#{number})") })),
            None,
        )?;
        let sha = match response.status {
            200 => response.body.get("sha").and_then(Value::as_str).unwrap_or_default().to_string(),
            405 => return refuse(ForgeReason::Dirty),
            409 => return refuse(ForgeReason::HeadMoved),
            _ => return response.ok(&[&self.token]).map(|_| String::new()),
        };
        // The PR tree: children move onto the base BEFORE the head branch
        // goes (GitHub closes PRs whose base disappears).
        for child in self.list_by_base(&pr.head)? {
            self.call(Method::PATCH, &format!("/pulls/{child}"), Some(json!({ "base": pr.base })), None)?
                .ok(&[&self.token])?;
        }
        let other_live = self.open_prs()?.iter().any(|o| o.head == pr.head);
        if pr.head != self.default_branch()? && !other_live {
            // A repository that deletes merged heads itself answers 422: fine.
            let response = self.call(Method::DELETE, &format!("/git/refs/heads/{}", pr.head), None, None)?;
            if !matches!(response.status, 204 | 422 | 404) {
                response.ok(&[&self.token])?;
            }
        }
        Ok(sha)
    }

    fn close(&self, number: i64) -> Result<()> {
        self.live(number)?;
        self.call(Method::PATCH, &format!("/pulls/{number}"), Some(json!({ "state": "closed" })), None)?
            .ok(&[&self.token])?;
        Ok(())
    }

    fn files(&self, number: i64) -> Result<Vec<FileChange>> {
        self.live(number)?;
        let mut out = vec![];
        for page in 1.. {
            let list = self.get(&format!("/pulls/{number}/files?per_page=100&page={page}"))?;
            let items = list.as_array().cloned().unwrap_or_default();
            for item in &items {
                let path = item.get("filename").and_then(Value::as_str).unwrap_or_default().to_string();
                match item.get("status").and_then(Value::as_str).unwrap_or_default() {
                    "added" | "copied" => out.push(FileChange { path, change: Change::Added }),
                    "removed" => out.push(FileChange { path, change: Change::Removed }),
                    "renamed" => {
                        if let Some(previous) = item.get("previous_filename").and_then(Value::as_str) {
                            out.push(FileChange { path: previous.into(), change: Change::Removed });
                        }
                        out.push(FileChange { path, change: Change::Added });
                    }
                    _ => out.push(FileChange { path, change: Change::Modified }),
                }
            }
            if items.len() < 100 {
                break;
            }
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    fn rebase_onto(&self, _branch: &str, _onto: &str, _upstream: &str) -> Result<String> {
        refuse(ForgeReason::Unsupported)
    }

    fn events(&self, cursor: Option<&str>) -> Result<Events> {
        let mut poller = self.poller.lock().unwrap();
        self.poll(&mut poller)?;
        let from = cursor.and_then(|c| c.parse::<usize>().ok()).unwrap_or(0).min(poller.log.len());
        Ok(Events { events: poller.log[from..].to_vec(), cursor: poller.log.len().to_string() })
    }
}

/// A device-flow code to show the person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceCode {
    pub device_code: String,
    /// What the person types at `verification_uri`.
    pub user_code: String,
    pub verification_uri: String,
    /// Seconds between polls.
    pub interval: u64,
    pub expires_in: u64,
}

/// One poll's answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DevicePoll {
    Pending,
    /// Poll slower: the new interval in seconds.
    SlowDown(u64),
    Token(String),
    Denied,
    Expired,
}

/// GitHub's OAuth device flow (no redirect, no server): start, show the
/// code, poll until the person approved.
pub struct DeviceFlow {
    client: reqwest::blocking::Client,
    web: String,
    client_id: String,
}

impl DeviceFlow {
    /// `client_id` of an OAuth or GitHub App with the device flow enabled.
    pub fn new(client_id: impl Into<String>) -> Result<Self> {
        Ok(Self { client: client("ecode")?, web: WEB.into(), client_id: client_id.into() })
    }

    /// Another web root (GitHub Enterprise, a test server).
    pub fn with_web(mut self, web: &str) -> Self {
        self.web = web.trim_end_matches('/').into();
        self
    }

    fn post(&self, path: &str, form: Value) -> Result<Value> {
        let request = self.client.post(format!("{}{path}", self.web)).header("accept", "application/json");
        let request = crate::http::with_json(request, &form);
        send(request, &[])?.ok(&[])
    }

    /// Asks for a code (`scope` e.g. `repo`).
    pub fn start(&self, scope: &str) -> Result<DeviceCode> {
        let body = self.post("/login/device/code", json!({ "client_id": self.client_id, "scope": scope }))?;
        let s = |k: &str| body.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
        let n = |k: &str, d: u64| body.get(k).and_then(Value::as_u64).unwrap_or(d);
        Ok(DeviceCode {
            device_code: s("device_code"),
            user_code: s("user_code"),
            verification_uri: s("verification_uri"),
            interval: n("interval", 5),
            expires_in: n("expires_in", 900),
        })
    }

    /// One poll.
    pub fn poll(&self, code: &DeviceCode) -> Result<DevicePoll> {
        let body = self.post(
            "/login/oauth/access_token",
            json!({
                "client_id": self.client_id,
                "device_code": code.device_code,
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
            }),
        )?;
        if let Some(token) = body.get("access_token").and_then(Value::as_str) {
            return Ok(DevicePoll::Token(token.into()));
        }
        Ok(match body.get("error").and_then(Value::as_str).unwrap_or_default() {
            "authorization_pending" => DevicePoll::Pending,
            "slow_down" => DevicePoll::SlowDown(body.get("interval").and_then(Value::as_u64).unwrap_or(code.interval + 5)),
            "access_denied" => DevicePoll::Denied,
            "expired_token" => DevicePoll::Expired,
            other => return Err(Error::Http { status: 200, detail: format!("device flow: {other}") }),
        })
    }

    /// Polls at the server's pace until a token, a refusal or the expiry;
    /// `sleep` is the host's (a test passes a no-op).
    pub fn wait(&self, code: &DeviceCode, mut sleep: impl FnMut(Duration)) -> Result<String> {
        let mut interval = code.interval;
        let mut waited = 0;
        loop {
            sleep(Duration::from_secs(interval));
            waited += interval;
            match self.poll(code)? {
                DevicePoll::Token(token) => return Ok(token),
                DevicePoll::Pending if waited < code.expires_in => {}
                DevicePoll::SlowDown(next) if waited < code.expires_in => interval = next,
                DevicePoll::Denied => return Err(Error::Http { status: 403, detail: "the person denied access".into() }),
                _ => return Err(Error::Http { status: 410, detail: "the device code expired".into() }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::http::mock::MockServer;

    #[test]
    fn base64_matches_the_rfc_vectors() {
        for (input, out) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foobar", "Zm9vYmFy")] {
            assert_eq!(base64(input.as_bytes()), out);
        }
    }

    #[test]
    fn device_flow_polls_until_the_token() {
        let polls = Arc::new(Mutex::new(0));
        let count = polls.clone();
        let server = MockServer::start(move |_, path, body| match path {
            "/login/device/code" => {
                assert_eq!(body["client_id"], "Iv1.abc");
                (200, json!({ "device_code": "dc", "user_code": "ABCD-1234", "verification_uri": "https://github.com/login/device", "interval": 5, "expires_in": 900 }))
            }
            "/login/oauth/access_token" => {
                let mut n = count.lock().unwrap();
                *n += 1;
                match *n {
                    1 => (200, json!({ "error": "authorization_pending" })),
                    2 => (200, json!({ "error": "slow_down", "interval": 10 })),
                    _ => (200, json!({ "access_token": "gho_token", "token_type": "bearer" })),
                }
            }
            _ => (404, Value::Null),
        });
        let flow = DeviceFlow::new("Iv1.abc").unwrap().with_web(&server.url());
        let code = flow.start("repo").unwrap();
        assert_eq!(code.user_code, "ABCD-1234");
        let mut slept = vec![];
        let token = flow.wait(&code, |d| slept.push(d.as_secs())).unwrap();
        assert_eq!(token, "gho_token");
        assert_eq!(slept, [5, 5, 10]);
    }

    /// A tiny GitHub: two branches, PR #1 root → main, #2 child → root.
    fn fake_github() -> MockServer {
        let state = Arc::new(Mutex::new(json!({
            "1": { "number": 1, "state": "open", "draft": false, "title": "Root", "head": { "ref": "root", "sha": "c2" }, "base": { "ref": "main" } },
            "2": { "number": 2, "state": "open", "draft": false, "title": "Child", "head": { "ref": "child", "sha": "c3" }, "base": { "ref": "root" } }
        })));
        MockServer::start(move |method, path, body| {
            let mut prs = state.lock().unwrap();
            let path = path.strip_prefix("/repos/acme/web").unwrap_or(path);
            let open = || {
                let list: Vec<Value> = prs.as_object().unwrap().values().filter(|p| p["state"] == "open").cloned().collect();
                (200, Value::Array(list))
            };
            match (method, path) {
                ("GET", "") => (200, json!({ "default_branch": "main" })),
                ("GET", p) if p.starts_with("/git/ref/heads/") => {
                    let name = &p["/git/ref/heads/".len()..];
                    match name {
                        "main" => (200, json!({ "object": { "sha": "c1" } })),
                        "root" => (200, json!({ "object": { "sha": "c2" } })),
                        "child" => (200, json!({ "object": { "sha": "c3" } })),
                        _ => (404, json!({ "message": "Not Found" })),
                    }
                }
                ("GET", p) if p.starts_with("/pulls?state=open") => open(),
                ("GET", p) if p.starts_with("/pulls?state=all") => {
                    let list: Vec<Value> = prs.as_object().unwrap().values().cloned().collect();
                    (200, Value::Array(list))
                }
                ("GET", p) if p.starts_with("/pulls/") && !p.contains("/files") => {
                    let n = &p["/pulls/".len()..];
                    match prs.get(n) {
                        Some(pr) => (200, json!({ "mergeable": true, "mergeable_state": "clean" }).as_object().unwrap().clone().into_iter().chain(pr.as_object().unwrap().clone()).collect::<serde_json::Map<_, _>>().into()),
                        None => (404, json!({ "message": "Not Found" })),
                    }
                }
                ("PUT", "/pulls/1/merge") => {
                    assert_eq!(body["merge_method"], "squash");
                    assert_eq!(body["sha"], "c2");
                    prs["1"]["state"] = json!("closed");
                    prs["1"]["merged"] = json!(true);
                    prs["1"]["merged_at"] = json!("2026-11-13T00:00:00Z");
                    prs["1"]["merge_commit_sha"] = json!("m1");
                    (200, json!({ "sha": "m1", "merged": true }))
                }
                ("PATCH", p) if p.starts_with("/pulls/") => {
                    let n = p["/pulls/".len()..].to_string();
                    if let Some(base) = body.get("base") {
                        prs[&n]["base"]["ref"] = base.clone();
                    }
                    if let Some(state) = body.get("state") {
                        prs[&n]["state"] = state.clone();
                    }
                    (200, prs[&n].clone())
                }
                ("DELETE", "/git/refs/heads/root") => (204, Value::Null),
                ("POST", "/pulls") => (201, json!({ "number": 3 })),
                _ => (404, json!({ "message": format!("unexpected {method} {path}") })),
            }
        })
    }

    #[test]
    fn squash_merge_retargets_children_then_deletes_the_head() {
        let server = fake_github();
        let forge = GithubForge::new("acme/web", "ghp_secret").unwrap().with_api(&server.url());
        assert!(!forge.capabilities(ecode_core::Capability::RebaseOnto));
        assert_eq!(forge.events(None).unwrap().events, []);
        assert_eq!(forge.list_by_base("root").unwrap(), [2]);
        assert_eq!(forge.mergeability(1).unwrap().mergeability, Mergeability::Clean);
        assert_eq!(forge.squash_merge(1, Some("c1")).unwrap_err().reason(), Some(ForgeReason::HeadMoved));
        assert_eq!(forge.squash_merge(1, Some("c2")).unwrap(), "m1");
        assert_eq!(forge.pr(2).unwrap().unwrap().base, "main");
        let requests = server.requests();
        let patch = requests.iter().position(|r| r.starts_with("PATCH /repos/acme/web/pulls/2")).unwrap();
        let delete = requests.iter().position(|r| r.starts_with("DELETE /repos/acme/web/git/refs/heads/root")).unwrap();
        assert!(patch < delete, "children move before the head branch goes");
        assert!(requests.iter().all(|r| !r.contains("ghp_secret") || r.to_lowercase().contains("authorization: bearer ghp_secret")));
        let events = forge.events(Some("0")).unwrap().events;
        assert!(events.contains(&Event::PrMerged { number: 1, sha: "m1".into() }));
        assert!(events.contains(&Event::PrRetargeted { number: 2, from: "root".into(), to: "main".into() }));
        assert_eq!(forge.rebase_onto("child", "main", "c2").unwrap_err().reason(), Some(ForgeReason::Unsupported));
        // The spec's client-side refusals.
        assert_eq!(forge.open(OpenPr { head: "nope".into(), base: "main".into(), ..Default::default() }).unwrap_err().reason(), Some(ForgeReason::Head));
        assert_eq!(forge.open(OpenPr { head: "child".into(), base: "main".into(), ..Default::default() }).unwrap_err().reason(), Some(ForgeReason::Exists));
    }
}
