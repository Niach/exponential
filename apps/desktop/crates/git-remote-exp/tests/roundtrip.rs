//! #EXP-1616: `git clone/fetch/push exp://…` end to end, with real git
//! driving this helper against a stand-in for the server (`/api/ecode/git/*`
//! with the same semantics: refs by compare-and-swap, packs in a bucket that
//! is a directory here, presigned URLs that point back at it). Then the
//! device jobs of eCode's forge on the same remote: mergeability, a squash
//! commit, files, a repack, and THE BACKUP TEST: back up, wipe, restore, and
//! verify the refs and every object.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

#[derive(Default)]
struct Remote {
    refs: BTreeMap<String, String>,
    /// (id, seq, key, superseded)
    packs: Vec<(String, u64, String, bool)>,
    bundles: Vec<(String, BTreeMap<String, String>)>,
    seq: u64,
}

struct Server {
    url: String,
    state: Arc<Mutex<Remote>>,
    bucket: PathBuf,
}

fn respond(stream: &mut std::net::TcpStream, status: u16, body: &[u8], kind: &str) {
    let _ = write!(stream, "HTTP/1.1 {status} X\r\ncontent-type: {kind}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", body.len());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

impl Server {
    fn start(root: &Path) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let state = Arc::new(Mutex::new(Remote::default()));
        let bucket = root.join("bucket");
        std::fs::create_dir_all(&bucket).unwrap();
        let (shared, dir, base) = (state.clone(), bucket.clone(), url.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let (shared, dir, base) = (shared.clone(), dir.clone(), base.clone());
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut head = String::new();
                    reader.read_line(&mut head).unwrap();
                    let mut length = 0usize;
                    let mut auth = String::new();
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                            break;
                        }
                        if let Some((name, value)) = line.split_once(':') {
                            if name.eq_ignore_ascii_case("content-length") {
                                length = value.trim().parse().unwrap_or(0);
                            }
                            if name.eq_ignore_ascii_case("authorization") {
                                auth = value.trim().to_string();
                            }
                        }
                    }
                    let mut body = vec![0; length];
                    reader.read_exact(&mut body).unwrap();
                    let mut parts = head.split_whitespace();
                    let (method, path) = (parts.next().unwrap().to_string(), parts.next().unwrap().to_string());
                    // The bucket: presigned URLs land here.
                    if let Some(key) = path.strip_prefix("/bucket/") {
                        let file = dir.join(key.replace('/', "__"));
                        if method == "PUT" {
                            std::fs::write(&file, &body).unwrap();
                            return respond(&mut stream, 200, b"", "text/plain");
                        }
                        return match std::fs::read(&file) {
                            Ok(bytes) => respond(&mut stream, 200, &bytes, "application/octet-stream"),
                            Err(_) => respond(&mut stream, 404, b"", "text/plain"),
                        };
                    }
                    if auth != "Bearer expg_test" {
                        return respond(&mut stream, 401, br#"{"error":"Unauthorized"}"#, "application/json");
                    }
                    let input: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                    let (op, query) = path.trim_start_matches("/api/ecode/git/").split_once('?').map_or((path.trim_start_matches("/api/ecode/git/").to_string(), String::new()), |(a, b)| (a.to_string(), b.to_string()));
                    let mut s = shared.lock().unwrap();
                    let answer: (u16, Value) = match op.as_str() {
                        "refs" => (200, json!({ "head": "refs/heads/main", "refs": s.refs })),
                        "packs" => {
                            let after: u64 = query.trim_start_matches("after=").parse().unwrap_or(0);
                            let packs: Vec<Value> = s
                                .packs
                                .iter()
                                .filter(|p| !p.3 && p.1 > after)
                                .map(|p| json!({ "id": p.0, "seq": p.1, "bytes": 1, "url": format!("{base}/bucket/{}", p.2) }))
                                .collect();
                            (200, json!({ "packs": packs }))
                        }
                        "upload" => {
                            s.seq += 1;
                            let key = format!("ecode/t/r/{}s/{}.{}", input["kind"].as_str().unwrap(), s.seq, input["kind"].as_str().unwrap());
                            (200, json!({ "key": key, "url": format!("{base}/bucket/{key}") }))
                        }
                        "push" => {
                            let updates = input["updates"].as_array().cloned().unwrap_or_default();
                            let moved = updates.iter().find(|u| s.refs.get(u["ref"].as_str().unwrap()).map(String::as_str) != u["old"].as_str());
                            if let Some(moved) = moved {
                                (409, json!({ "ok": false, "rejected": moved["ref"] }))
                            } else {
                                if let Some(key) = input["pack"].as_str() {
                                    s.seq += 1;
                                    let seq = s.seq;
                                    s.packs.push((format!("p{seq}"), seq, key.to_string(), false));
                                }
                                for u in updates {
                                    let name = u["ref"].as_str().unwrap().to_string();
                                    match u["new"].as_str() {
                                        Some(sha) => s.refs.insert(name, sha.to_string()),
                                        None => s.refs.remove(&name),
                                    };
                                }
                                (200, json!({ "ok": true }))
                            }
                        }
                        "object" => (200, json!({ "url": format!("{base}/bucket/{}", query.trim_start_matches("key=")) })),
                        "bundle" => {
                            let refs = serde_json::from_value(input["refs"].clone()).unwrap();
                            s.bundles.push((input["key"].as_str().unwrap().to_string(), refs));
                            (200, json!({ "ok": true }))
                        }
                        "repack" | "restore" => {
                            let replaces: Vec<String> = input["replaces"].as_array().map(|a| a.iter().map(|v| v.as_str().unwrap().to_string()).collect()).unwrap_or_default();
                            for pack in s.packs.iter_mut() {
                                if op == "restore" || replaces.contains(&pack.0) {
                                    pack.3 = true;
                                }
                            }
                            s.seq += 1;
                            let seq = s.seq;
                            s.packs.push((format!("p{seq}"), seq, input["pack"].as_str().unwrap().to_string(), false));
                            if op == "restore" {
                                s.refs = serde_json::from_value(input["refs"].clone()).unwrap();
                            }
                            (200, json!({ "ok": true }))
                        }
                        _ => (404, json!({ "error": "Not found" })),
                    };
                    drop(s);
                    respond(&mut stream, answer.0, answer.1.to_string().as_bytes(), "application/json");
                });
            }
        });
        Self { url, state, bucket }
    }
}

fn git(dir: &Path, args: &[&str], server: &str) -> (bool, String) {
    let helper = PathBuf::from(env!("CARGO_BIN_EXE_git-remote-exp"));
    let path = format!("{}:{}", helper.parent().unwrap().display(), std::env::var("PATH").unwrap_or_default());
    let out = Command::new("git")
        .current_dir(dir)
        .args(["-c", "user.name=T", "-c", "user.email=t@example.com", "-c", "init.defaultBranch=main", "-c", "core.hooksPath=/dev/null"])
        .args(args)
        .env("PATH", path)
        .env("EXP_GIT_SERVER", server)
        .env("EXP_GIT_TOKEN", "expg_test")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap();
    (out.status.success(), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

fn ok(dir: &Path, args: &[&str], server: &str) -> String {
    let (success, out) = git(dir, args, server);
    assert!(success, "git {args:?} failed:\n{out}");
    out.trim().to_string()
}

fn commit(dir: &Path, server: &str, file: &str, text: &str) -> String {
    std::fs::write(dir.join(file), text).unwrap();
    ok(dir, &["add", "-A"], server);
    ok(dir, &["commit", "-q", "-m", &format!("{file}: {text}")], server);
    ok(dir, &["rev-parse", "HEAD"], server)
}

fn tempdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("git-remote-exp-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn clone_fetch_push_jobs_and_a_restored_backup() {
    if !ecode::git_available() {
        eprintln!("skipped: no git");
        return;
    }
    let root = tempdir("roundtrip");
    let server = Server::start(&root);
    let url = "exp://team/app";

    // ── push: a first commit creates main, a second is a fast-forward ──
    let work = root.join("work");
    std::fs::create_dir_all(&work).unwrap();
    ok(&work, &["init", "-q"], &server.url);
    let c1 = commit(&work, &server.url, "a.txt", "one\n");
    ok(&work, &["remote", "add", "origin", url], &server.url);
    ok(&work, &["push", "-q", "origin", "main"], &server.url);
    let c2 = commit(&work, &server.url, "a.txt", "one\ntwo\n");
    ok(&work, &["push", "-q", "origin", "main"], &server.url);
    assert_eq!(server.state.lock().unwrap().refs.get("refs/heads/main"), Some(&c2));
    assert_eq!(server.state.lock().unwrap().packs.len(), 2, "one pack per push");

    // A topic branch for the forge jobs below.
    ok(&work, &["checkout", "-q", "-b", "topic"], &server.url);
    let t1 = commit(&work, &server.url, "b.txt", "topic\n");
    ok(&work, &["push", "-q", "origin", "topic"], &server.url);

    // ── clone: every object, HEAD on the default branch ──
    let clone = root.join("clone");
    ok(&root, &["clone", "-q", url, clone.to_str().unwrap()], &server.url);
    assert_eq!(ok(&clone, &["rev-parse", "HEAD"], &server.url), c2);
    assert_eq!(std::fs::read_to_string(clone.join("a.txt")).unwrap(), "one\ntwo\n");
    assert_eq!(ok(&clone, &["rev-parse", "origin/topic"], &server.url), t1);

    // ── a stale clone's push is not a fast-forward: refused ──
    ok(&work, &["checkout", "-q", "main"], &server.url);
    let c3 = commit(&work, &server.url, "c.txt", "three\n");
    ok(&work, &["push", "-q", "origin", "main"], &server.url);
    commit(&clone, &server.url, "d.txt", "stale\n");
    let (pushed, out) = git(&clone, &["push", "origin", "main"], &server.url);
    assert!(!pushed, "a non-fast-forward push must fail:\n{out}");
    assert_eq!(server.state.lock().unwrap().refs.get("refs/heads/main"), Some(&c3));
    // fetch brings the new commit; a forced push is allowed.
    ok(&clone, &["fetch", "-q", "origin"], &server.url);
    assert_eq!(ok(&clone, &["rev-parse", "origin/main"], &server.url), c3);

    // ── the forge's device jobs, on a job cache ──
    let remote = ecode::exp_git::ExpGit::new(&server.url, "expg_test").unwrap();
    let cache = root.join("cache.git");
    let run = |args: Value| ecode::exp_jobs::run_job(&remote, &cache, &args).unwrap();
    assert_eq!(run(json!({ "op": "mergeability", "base": c3, "head": t1 })), json!({ "mergeability": "clean", "conflicts": [] }));
    let files = run(json!({ "op": "files", "base": c3, "head": t1 }));
    assert_eq!(files["files"][0]["path"], "b.txt");
    assert_eq!(files["files"][0]["change"], "added");
    assert_eq!(files["files"][0]["additions"], 1);
    assert_eq!(run(json!({ "op": "is_ancestor", "ancestor": c1, "of": c3 })), json!({ "yes": true }));
    let squash = run(json!({ "op": "squash", "base": c3, "head": t1, "message": "Topic (#1)" }));
    let merged = squash["sha"].as_str().unwrap().to_string();
    // The job moved no ref: the server's CAS does (as ecodeForge would).
    {
        let mut s = server.state.lock().unwrap();
        assert_eq!(s.refs.get("refs/heads/main"), Some(&c3));
        s.refs.insert("refs/heads/main".into(), merged.clone());
    }
    ok(&clone, &["fetch", "-q", "origin"], &server.url);
    assert_eq!(ok(&clone, &["rev-parse", "origin/main^"], &server.url), c3, "the squash commit sits on the base");
    assert_eq!(ok(&clone, &["show", "origin/main:b.txt"], &server.url), "topic");
    // The mirror: every branch, forced, to another remote (a bare repo here).
    let mirror = root.join("mirror.git");
    ok(&root, &["init", "-q", "--bare", mirror.to_str().unwrap()], &server.url);
    assert_eq!(run(json!({ "op": "mirror", "url": mirror.to_str().unwrap() }))["pushed"], 2);
    assert_eq!(ok(&mirror, &["rev-parse", "main"], &server.url), merged);
    assert_eq!(ok(&mirror, &["rev-parse", "topic"], &server.url), t1);
    let repacked = run(json!({ "op": "repack" }));
    assert!(repacked["replaced"].as_u64().unwrap() >= 4, "{repacked}");
    assert_eq!(server.state.lock().unwrap().packs.iter().filter(|p| !p.3).count(), 1);

    // ── THE BACKUP: back up, wipe, restore, verify refs and objects ──
    let before = server.state.lock().unwrap().refs.clone();
    let backup = run(json!({ "op": "backup" }));
    let bundle_key = backup["key"].as_str().unwrap().to_string();
    assert_eq!(server.state.lock().unwrap().bundles[0].1, before, "the bundle holds every ref");
    {
        // The wipe: every ref and every pack (rows and bucket objects) gone;
        // only the bundle survives.
        let mut s = server.state.lock().unwrap();
        s.refs.clear();
        for pack in s.packs.drain(..) {
            let _ = std::fs::remove_file(server.bucket.join(pack.2.replace('/', "__")));
        }
    }
    let gone = root.join("gone");
    ok(&root, &["clone", "-q", url, gone.to_str().unwrap()], &server.url);
    assert!(git(&gone, &["rev-parse", "HEAD"], &server.url).0 == false, "the wiped remote is empty");
    let restore_cache = root.join("restore-cache.git");
    ecode::exp_jobs::run_job(&remote, &restore_cache, &json!({ "op": "restore", "bundle": bundle_key })).unwrap();
    assert_eq!(server.state.lock().unwrap().refs, before, "every ref restored");
    let restored = root.join("restored");
    ok(&root, &["clone", "-q", url, restored.to_str().unwrap()], &server.url);
    ok(&restored, &["fsck", "--full", "--strict"], &server.url);
    for (name, sha) in &before {
        let short = name.trim_start_matches("refs/heads/");
        assert_eq!(&ok(&restored, &["rev-parse", &format!("origin/{short}")], &server.url), sha, "{name}");
    }
    // Every object of the original history is there again.
    let objects = |dir: &Path, revs: &[&str]| {
        let mut args = vec!["rev-list", "--objects"];
        args.extend_from_slice(revs);
        ok(dir, &args, &server.url).lines().map(|l| l.split(' ').next().unwrap().to_string()).collect::<std::collections::BTreeSet<_>>()
    };
    let wanted = objects(&work, &["main", "topic"]);
    let have = objects(&restored, &["--all"]);
    assert!(wanted.is_subset(&have), "objects missing after the restore: {:?}", wanted.difference(&have).collect::<Vec<_>>());
    for sha in [&c1, &c2, &c3, &t1, &merged] {
        ok(&restored, &["cat-file", "-e", sha.as_str()], &server.url);
    }
    assert_eq!(std::fs::read_to_string(restored.join("b.txt")).unwrap(), "topic\n");
    let _ = std::fs::remove_dir_all(&root);
}
