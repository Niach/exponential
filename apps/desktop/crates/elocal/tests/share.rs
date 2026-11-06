//! #EDE-57: the uploader against a canned instance: the manifest, the
//! chunks in table order, the attachment bytes on the signed path, the
//! commit, and the team it answers.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use api::issues::IssuesCreateInput;
use elocal::share::{run, Progress};
use elocal::Desktops;
use serde_json::{json, Value};

struct Seen {
    method: String,
    path: String,
    body: Vec<u8>,
}

/// Answers every request by its path, one connection each (`Connection: close`).
fn instance(log: Arc<Mutex<Vec<Seen>>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buf = Vec::new();
            let mut chunk = [0u8; 65536];
            let (mut head_end, mut length) = (None, 0usize);
            loop {
                let n = match stream.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                buf.extend_from_slice(&chunk[..n]);
                if head_end.is_none() {
                    if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        head_end = Some(pos + 4);
                        let head = String::from_utf8_lossy(&buf[..pos]).to_string();
                        length = head.lines().find_map(|l| l.split_once(':').filter(|(k, _)| k.eq_ignore_ascii_case("content-length")).and_then(|(_, v)| v.trim().parse().ok())).unwrap_or(0);
                    }
                }
                if let Some(pos) = head_end {
                    if buf.len() >= pos + length {
                        break;
                    }
                }
            }
            let Some(pos) = head_end else { continue };
            let head = String::from_utf8_lossy(&buf[..pos]).to_string();
            let mut parts = head.split_whitespace();
            let method = parts.next().unwrap_or("").to_string();
            let path = parts.next().unwrap_or("").to_string();
            let body = buf[pos..pos + length].to_vec();
            let answer = match path.as_str() {
                "/api/trpc/imports.create" => json!({"result": {"data": {"importId": "imp-1", "teamId": "team-1", "slug": "local-1"}}}),
                p if p.starts_with("/api/trpc/imports.chunk") => {
                    let sent: Value = serde_json::from_slice(&body).unwrap();
                    let uploads: Vec<Value> = sent["rows"].as_array().unwrap().iter().filter(|r| r["table"] == "attachments").map(|r| json!({"id": r["id"], "serverId": r["id"], "uploadPath": format!("/api/attachment-uploads/tok-{}", r["id"].as_str().unwrap()), "expiresAt": "2026-11-07T01:00:00.000Z"})).collect();
                    json!({"result": {"data": {"counts": {}, "uploads": uploads}}})
                }
                p if p.starts_with("/api/attachment-uploads/") => json!({"ok": true}),
                "/api/trpc/imports.commit" => json!({"result": {"data": {"team": {"id": "team-1", "slug": "local-1", "name": "Shared"}, "remapped": 1, "counts": {}, "idMap": []}}}),
                _ => json!({"error": {"message": "nope"}}),
            };
            log.lock().unwrap().push(Seen { method, path: path.clone(), body });
            let text = answer.to_string();
            let status = if path.starts_with("/api/trpc/") || path.starts_with("/api/attachment-uploads/") { 200 } else { 404 };
            let _ = stream.write_all(format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len()).as_bytes());
        }
    });
    format!("http://127.0.0.1:{port}")
}

#[test]
fn uploads_a_desktop_as_a_team() {
    let dir = tempfile::tempdir().unwrap();
    let desktops = Desktops::open(dir.path()).unwrap();
    let id = desktops.current().unwrap().to_string();
    let desktop = desktops.open_desktop(&id).unwrap();
    let local = desktop.client();
    let board = desktop.store.rows("boards").unwrap().into_iter().next().unwrap();
    let issue = api::issues::issues_create(&local, &IssuesCreateInput::new(board["id"].as_str().unwrap(), "Ship")).unwrap();
    api::comments::comments_create(&local, &issue.issue.id, "note", None, None, None).unwrap();
    api::prompts::create(&local, desktop.executor.team_id(), "Review", None, "Look twice").unwrap();
    let attachment = desktop.executor.attachment_create("a.txt", "text/plain", b"bytes!", Some(&issue.issue.id)).unwrap();

    let log = Arc::new(Mutex::new(Vec::new()));
    let base = instance(log.clone());
    let client = api::TrpcClient::new(&base, Arc::new(api::StaticToken("tok".into())));
    let mut progress = Vec::new();
    let shared = run(&client, &desktop, "Shared", Some("dev-1"), &mut |p| progress.push(p)).unwrap();
    assert_eq!(shared.team_id, "team-1");
    assert_eq!(shared.name, "Shared");
    assert_eq!(shared.remapped, 1);
    assert_eq!(progress.first(), Some(&Progress::Created { team_id: "team-1".into(), slug: "local-1".into() }));
    assert_eq!(progress.last(), Some(&Progress::Committing));
    assert!(progress.contains(&Progress::Attachment { sent: 1, total: 1 }));

    let seen = log.lock().unwrap();
    assert_eq!(seen[0].path, "/api/trpc/imports.create");
    let create: Value = serde_json::from_slice(&seen[0].body).unwrap();
    assert_eq!(create["name"], "Shared");
    assert_eq!(create["manifest"]["version"], 1);
    assert_eq!(create["manifest"]["desktopId"], json!(id));
    assert_eq!(create["manifest"]["deviceId"], "dev-1");
    assert_eq!(create["manifest"]["identity"]["userId"], json!(desktop.executor.identity().user_id));
    assert_eq!(create["manifest"]["counts"]["issues"], 1);
    assert_eq!(create["manifest"]["counts"]["attachments"], 1);
    assert_eq!(create["manifest"]["attachmentBytes"], 6);
    let chunk: Value = serde_json::from_slice(&seen[1].body).unwrap();
    assert_eq!(chunk["importId"], "imp-1");
    assert_eq!(chunk["seq"], 0);
    let tables: Vec<&str> = chunk["rows"].as_array().unwrap().iter().map(|r| r["table"].as_str().unwrap()).collect();
    let first = |t: &str| tables.iter().position(|x| *x == t).unwrap();
    assert!(first("boards") < first("issues") && first("issues") < first("comments") && first("comments") < first("attachments"));
    let prompt = chunk["rows"].as_array().unwrap().iter().find(|r| r["table"] == "prompts").unwrap();
    assert_eq!(prompt["row"]["body"], "Look twice");
    let put = seen.iter().find(|s| s.method == "PUT").unwrap();
    assert_eq!(put.path, format!("/api/attachment-uploads/tok-{}", attachment["id"].as_str().unwrap()));
    assert_eq!(put.body, b"bytes!");
    assert_eq!(seen.last().unwrap().path, "/api/trpc/imports.commit");
}
