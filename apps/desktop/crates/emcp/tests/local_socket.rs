//! #EDE-172 (P8): the host's local socket against real processes. The host
//! is this test process; a run's "agent" is a `sh` it spawns and binds
//! ([`emcp::local::bind_process`]); the peers are this same test binary run
//! again as [`local_peer_helper`], started where each case needs it: as the
//! agent's child (admitted as its run, then MCP `initialize`, `tools/list`
//! and `tools/call` over the socket), as the agent's sibling, under an
//! agent nobody bound, under another run's agent, and double-forked out of
//! the agent's tree. A reused pid is the ancestry walk's own unit test
//! (`src/local/ancestry.rs`): no test can make the kernel hand out a pid.

#![cfg(any(target_os = "linux", target_os = "macos"))]

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use emcp::{Caller, Server, ServerInfo, SessionId, ToolProvider};
use serde_json::{json, Value};

const T: Duration = Duration::from_secs(20);

/// What the helper does: `<socket>\t<server>\t<out file>\t<mode>`, the mode
/// `call`, `wait:<flag file>` (pause after `tools/list` until the flag
/// exists) or `orphan` (connect only once reparented).
const HELPER_ENV: &str = "EMCP_LOCAL_TEST_HELPER";

/// The agent stand-ins, each waiting for the test's go line (the run is
/// bound by then). `"$@"` is the helper.
const AS_CHILD: &str = r#"read go; "$@"; exit $?"#;
const IDLE: &str = "read go; sleep 30";
const DOUBLE_FORK: &str = r#"read go; ( "$@" & sleep 1 ); sleep 30"#;

/// One tool that says which run called it.
struct WhoAmI;

impl ToolProvider for WhoAmI {
    fn tools(&self) -> Result<Vec<Value>, String> {
        Ok(vec![json!({ "name": "whoami", "description": "The calling run.", "inputSchema": { "type": "object" } })])
    }

    fn call(&self, caller: &Caller, _name: &str, _arguments: Value) -> Value {
        json!({
            "content": [{ "type": "text", "text": caller.session_id.to_string() }],
            "structuredContent": { "run": caller.session_id.to_string(), "label": caller.label },
        })
    }
}

fn serve(name: &str) -> Server {
    Server::serve(ServerInfo { name: name.to_string(), version: "1.0.0".into(), instructions: None }, Arc::new(WhoAmI)).unwrap()
}

fn id(run: &str) -> SessionId {
    SessionId::new(run).unwrap()
}

fn socket() -> PathBuf {
    emcp::local::listen().expect("the local socket opens")
}

fn out_file(case: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("emcp-local-{}-{case}.json", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}

fn spec(server: &str, out: &Path, mode: &str) -> String {
    format!("{}\t{server}\t{}\t{mode}", socket().display(), out.display())
}

fn helper_args() -> [&'static str; 4] {
    ["--exact", "local_peer_helper", "--nocapture", "--test-threads=1"]
}

/// A stand-in agent running `script` with the helper as `"$@"`.
fn agent(script: &str, helper: &str) -> Child {
    Command::new("/bin/sh")
        .arg("-c")
        .arg(script)
        .arg("sh")
        .arg(std::env::current_exe().unwrap())
        .args(helper_args())
        .env(HELPER_ENV, helper)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

fn go(agent: &mut Child) {
    agent.stdin.as_mut().unwrap().write_all(b"go\n").unwrap();
}

fn end(mut agent: Child) {
    let _ = agent.kill();
    let _ = agent.wait();
}

fn wait_for(path: &Path) {
    let deadline = Instant::now() + T;
    while !path.exists() {
        assert!(Instant::now() < deadline, "{} never appeared", path.display());
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// What the helper wrote to `out`.
fn result(out: &Path) -> Value {
    wait_for(out);
    serde_json::from_str(&std::fs::read_to_string(out).unwrap()).unwrap()
}

/// Why the host refused, when it did.
fn refusal(result: &Value) -> &str {
    result["answer"].as_str().and_then(|answer| answer.strip_prefix("refused: ")).unwrap_or_default()
}

const OUTSIDE: &str = "not started by an agent run of this host";

/// The agent's child: admitted as its run, and MCP works over the socket.
#[test]
fn a_child_of_a_bound_agent_is_admitted_as_its_run() {
    let server = serve("local-admit");
    let run = id("run-admit");
    server.grant(&run, "EXP-1");
    let out = out_file("admit");
    let mut agent = agent(AS_CHILD, &spec("local-admit", &out, "call"));
    emcp::local::bind_process(&run, agent.id()).unwrap();
    go(&mut agent);
    let result = result(&out);
    assert_eq!(result["answer"], "ok", "{result}");
    assert_eq!(result["server"], "local-admit");
    assert_eq!(result["tools"], json!(["whoami"]));
    assert_eq!(result["call"]["structuredContent"], json!({ "run": "run-admit", "label": "EXP-1" }), "{result}");
    assert_eq!(agent.wait().unwrap().code(), Some(0));
}

/// The agent's sibling (a process the host started beside it) is no run.
#[test]
fn a_sibling_of_the_agent_is_refused() {
    let server = serve("local-sibling");
    let run = id("run-sibling");
    server.grant(&run, "EXP-2");
    let mut bound = agent(IDLE, "");
    emcp::local::bind_process(&run, bound.id()).unwrap();
    go(&mut bound);
    let out = out_file("sibling");
    let mut sibling = Command::new(std::env::current_exe().unwrap())
        .args(helper_args())
        .env(HELPER_ENV, spec("local-sibling", &out, "call"))
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let result = result(&out);
    assert_eq!(refusal(&result), OUTSIDE, "{result}");
    let _ = sibling.wait();
    end(bound);
}

/// A process of another bound run is that run, never this one: refused
/// while its run holds no grant here, and once it holds one, admitted as
/// itself.
#[test]
fn a_process_of_another_bound_run_is_not_this_run() {
    let server = serve("local-other");
    let (mine, other) = (id("run-mine"), id("run-other"));
    server.grant(&mine, "EXP-3");
    let mut own = agent(IDLE, "");
    emcp::local::bind_process(&mine, own.id()).unwrap();
    go(&mut own);

    let out = out_file("other-refused");
    let mut theirs = agent(AS_CHILD, &spec("local-other", &out, "call"));
    emcp::local::bind_process(&other, theirs.id()).unwrap();
    go(&mut theirs);
    let first = result(&out);
    assert_eq!(refusal(&first), r#"this run holds no grant on "local-other""#, "{first}");
    let _ = theirs.wait();

    server.grant(&other, "EXP-4");
    let out = out_file("other-admitted");
    let mut theirs = agent(AS_CHILD, &spec("local-other", &out, "call"));
    emcp::local::bind_process(&other, theirs.id()).unwrap();
    go(&mut theirs);
    let second = result(&out);
    assert_eq!(second["call"]["structuredContent"]["run"], "run-other", "{second}");
    let _ = theirs.wait();
    end(own);
}

/// A run that holds a grant but whose agent nobody bound admits nobody.
#[test]
fn a_run_nobody_bound_admits_nobody() {
    let server = serve("local-unbound");
    server.grant(&id("run-unbound"), "EXP-5");
    let out = out_file("unbound");
    let mut unbound = agent(AS_CHILD, &spec("local-unbound", &out, "call"));
    go(&mut unbound);
    let result = result(&out);
    assert_eq!(refusal(&result), OUTSIDE, "{result}");
    let _ = unbound.wait();
}

/// A process that double-forked out of the agent's tree lost the run.
#[test]
fn a_double_forked_orphan_is_refused() {
    let server = serve("local-orphan");
    let run = id("run-orphan");
    server.grant(&run, "EXP-6");
    let out = out_file("orphan");
    let mut bound = agent(DOUBLE_FORK, &spec("local-orphan", &out, "orphan"));
    emcp::local::bind_process(&run, bound.id()).unwrap();
    go(&mut bound);
    let result = result(&out);
    assert_eq!(refusal(&result), OUTSIDE, "{result}");
    // The agent is still there and bound: the orphan was refused for being
    // out of its tree.
    assert!(bound.try_wait().unwrap().is_none());
    end(bound);
}

/// A server the run holds no grant on, or none of that name, refuses even
/// the run's own child.
#[test]
fn the_run_must_hold_a_grant_on_the_named_server() {
    let granted = serve("local-granted");
    let _ungranted = serve("local-ungranted");
    let run = id("run-named");
    granted.grant(&run, "EXP-7");
    for (case, name) in [("ungranted", "local-ungranted"), ("unknown", "local-nothing")] {
        let out = out_file(case);
        let mut child = agent(AS_CHILD, &spec(name, &out, "call"));
        emcp::local::bind_process(&run, child.id()).unwrap();
        go(&mut child);
        let result = result(&out);
        assert_eq!(refusal(&result), format!("this run holds no grant on {name:?}"), "{case}: {result}");
        let _ = child.wait();
    }
}

/// #EXP-1918 over the socket: a run revoked while connected reads why on
/// its next call.
#[test]
fn a_revoked_run_reads_why_on_its_next_call() {
    let server = serve("local-revoke");
    let run = id("run-revoke");
    server.grant(&run, "EXP-8");
    let out = out_file("revoke");
    let flag = out_file("revoke-flag");
    let mut agent = agent(AS_CHILD, &spec("local-revoke", &out, &format!("wait:{}", flag.display())));
    emcp::local::bind_process(&run, agent.id()).unwrap();
    go(&mut agent);
    wait_for(&out.with_extension("listed"));
    server.revoke(&run);
    std::fs::write(&flag, "").unwrap();
    let result = result(&out);
    assert_eq!(result["call"]["isError"], true, "{result}");
    let text = result["call"]["content"][0]["text"].as_str().unwrap_or_default();
    assert!(text.contains("ended this session's access"), "{text}");
    let _ = agent.wait();
}

/// The socket is the host's alone: a private directory, a private socket,
/// named by the host's pid.
#[test]
fn the_socket_is_private_and_names_its_host() {
    use std::os::unix::fs::PermissionsExt as _;
    let path = socket();
    assert_eq!(path.file_name().unwrap().to_str().unwrap(), format!("mcp-{}.sock", std::process::id()));
    let dir = std::fs::metadata(path.parent().unwrap()).unwrap();
    assert_eq!(dir.permissions().mode() & 0o777, 0o700);
    assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(emcp::local::socket_path(), Some(path));
    let identity = emcp::local::host_identity().unwrap();
    assert!(identity.starts_with(&format!("{}:", std::process::id())), "{identity}");
    assert!(emcp::local::bind_process(&id("run-self"), std::process::id()).is_err(), "the host is never a run's process");
}

/// Not a test of its own: the peer the tests above start (this binary run
/// again with [`HELPER_ENV`]); without it, nothing.
#[test]
fn local_peer_helper() {
    let Ok(spec) = std::env::var(HELPER_ENV) else { return };
    if spec.is_empty() {
        return;
    }
    let parts: Vec<&str> = spec.split('\t').collect();
    let (socket, server, out, mode) = (Path::new(parts[0]), parts[1], parts[2], parts[3]);
    if mode == "orphan" {
        wait_until_reparented();
    }
    let answer = talk(socket, server, out, mode);
    std::fs::write(format!("{out}.tmp"), answer.to_string()).unwrap();
    std::fs::rename(format!("{out}.tmp"), out).unwrap();
    std::process::exit(0);
}

/// Until the subshell that started this process is gone.
fn wait_until_reparented() {
    let first = std::os::unix::process::parent_id();
    let deadline = Instant::now() + T;
    while std::os::unix::process::parent_id() == first && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn talk(socket: &Path, server: &str, out: &str, mode: &str) -> Value {
    let mut stream = match UnixStream::connect(socket) {
        Ok(stream) => stream,
        Err(err) => return json!({ "answer": format!("connect: {err}") }),
    };
    stream.set_read_timeout(Some(T)).unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let _ = writeln!(stream, "{server}");
    let mut answer = String::new();
    let _ = reader.read_line(&mut answer);
    let answer = answer.trim().to_string();
    if answer != "ok" {
        return json!({ "answer": answer });
    }
    let init = rpc(
        &mut stream,
        &mut reader,
        1,
        "initialize",
        json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "helper", "version": "1" } }),
    );
    let _ = writeln!(stream, "{}", json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
    let list = rpc(&mut stream, &mut reader, 2, "tools/list", json!({}));
    if let Some(flag) = mode.strip_prefix("wait:") {
        std::fs::write(Path::new(out).with_extension("listed"), "").unwrap();
        wait_for(Path::new(flag));
    }
    let call = rpc(&mut stream, &mut reader, 3, "tools/call", json!({ "name": "whoami", "arguments": {} }));
    let tools: Vec<Value> = list["result"]["tools"].as_array().into_iter().flatten().map(|tool| tool["name"].clone()).collect();
    json!({ "answer": answer, "server": init["result"]["serverInfo"]["name"], "tools": tools, "call": call["result"] })
}

/// One request; the answer with its id (notifications skipped).
fn rpc(stream: &mut UnixStream, reader: &mut BufReader<UnixStream>, id: u64, method: &str, params: Value) -> Value {
    let _ = writeln!(stream, "{}", json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return json!({ "eof": method });
        }
        if let Ok(value) = serde_json::from_str::<Value>(&line) {
            if value["id"] == json!(id) {
                return value;
            }
        }
    }
}
