//! EXP-484 — reading codex's account and rate limits through its OWN
//! surface: `codex app-server --listen stdio://`, a line-delimited JSON-RPC
//! channel.
//!
//! `~/.codex/auth.json` is deliberately never read. Codex owns its
//! credential (refresh included); we ask the CLI what it knows and take the
//! answer. The whole exchange is four messages:
//!
//! ```text
//! → {"id":1,"method":"initialize","params":{"clientInfo":{…}}}
//! ← {"id":1,"result":{…}}
//! → {"method":"initialized"}                (notification, no id)
//! → {"id":2,"method":"account/read","params":{"refreshToken":false}}
//! ← {"id":2,"result":{"account":{…}}}
//! → {"id":3,"method":"account/rateLimits/read","params":{}}
//! ← {"id":3,"result":{"rateLimits":{…}}}
//! ```
//!
//! `refreshToken: false` is load-bearing: this probe must never cause a
//! token refresh. The whole exchange runs under ONE deadline, and the child
//! is killed and reaped by a Drop guard on every path — a wedged app-server
//! must not outlive the probe (the daemon runs this on a cadence).

use std::io::{BufRead, BufReader, Write};
use std::process::Stdio;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use terminal::process::background_command;

/// The whole exchange's budget — the same 10 s the doctor's probes get.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// What the app-server answered. Either half may be `None` on a build that
/// does not implement that method — the caller keeps whatever it had.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CodexProbe {
    /// The `account/read` result (`{"account": {…}|null}`).
    pub account: Option<Value>,
    /// The `account/rateLimits/read` result (`{"rateLimits": {…}}`).
    pub rate_limits: Option<Value>,
}

/// One request line (line-delimited JSON-RPC, no `Content-Length` framing).
pub fn rpc_request(id: u64, method: &str, params: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()
}

/// One notification line (no id, no answer expected).
pub fn rpc_notification(method: &str, params: Value) -> String {
    json!({"jsonrpc": "2.0", "method": method, "params": params}).to_string()
}

/// Classify one line from the child. `Some((id, Some(result)))` is an
/// answer, `Some((id, None))` an error answer, `None` anything else (a
/// server-initiated request, a log line, junk) — the reader must never
/// mistake those for the answer it waits on.
pub fn route_line(line: &str) -> Option<(u64, Option<Value>)> {
    let value: Value = serde_json::from_str(line.trim()).ok()?;
    if value.get("method").is_some() {
        // A request or notification FROM the server, not our answer.
        return None;
    }
    let id = value.get("id")?.as_u64()?;
    match value.get("result") {
        Some(result) => Some((id, Some(result.clone()))),
        None => value.get("error").map(|_| (id, None)),
    }
}

/// Kills and reaps the child on every exit path, including a panic.
struct ChildGuard(std::process::Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            // Own process group (set at spawn): a wedged app-server may
            // have children of its own holding the pipes.
            libc::killpg(self.0.id() as i32, libc::SIGKILL);
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Run the exchange. Blocking; `Err` = the CLI never answered (not
/// installed, an older build without `app-server`, or a wedged process
/// killed at the deadline).
pub fn probe(program: &str, path_env: &str, timeout: Duration) -> std::io::Result<CodexProbe> {
    let mut cmd = background_command(program);
    cmd.env("PATH", path_env)
        .args(["app-server", "--listen", "stdio://"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn()?;
    let stdout = child
        .stdout
        .take()
        .expect("stdout piped above");
    let mut stdin = child.stdin.take().expect("stdin piped above");
    let _guard = ChildGuard(child);

    let (sender, receiver) = channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if let Some(message) = route_line(&line) {
                if sender.send(message).is_err() {
                    break;
                }
            }
        }
    });

    let deadline = Instant::now() + timeout;
    writeln!(
        stdin,
        "{}",
        rpc_request(
            1,
            "initialize",
            json!({"clientInfo": {"name": "exponential", "version": env!("CARGO_PKG_VERSION")}}),
        )
    )?;
    stdin.flush()?;
    await_answer(&receiver, 1, deadline)?;

    writeln!(stdin, "{}", rpc_notification("initialized", json!({})))?;
    writeln!(
        stdin,
        "{}",
        rpc_request(2, "account/read", json!({"refreshToken": false}))
    )?;
    stdin.flush()?;
    let account = await_answer(&receiver, 2, deadline).ok().flatten();

    writeln!(
        stdin,
        "{}",
        rpc_request(3, "account/rateLimits/read", json!({}))
    )?;
    stdin.flush()?;
    let rate_limits = await_answer(&receiver, 3, deadline).ok().flatten();

    Ok(CodexProbe {
        account: account.filter(|value| !value.is_null()),
        rate_limits: rate_limits.filter(|value| !value.is_null()),
    })
}

/// Wait for exactly `id`, discarding answers to anything else, until the
/// shared deadline.
fn await_answer(
    receiver: &Receiver<(u64, Option<Value>)>,
    id: u64,
    deadline: Instant,
) -> std::io::Result<Option<Value>> {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("codex app-server did not answer id {id}"),
            ));
        }
        match receiver.recv_timeout(remaining) {
            Ok((answered, result)) if answered == id => return Ok(result),
            Ok(_) => continue,
            Err(RecvTimeoutError::Timeout) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("codex app-server did not answer id {id}"),
                ))
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "codex app-server exited",
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_are_line_delimited_json_rpc() {
        assert_eq!(
            rpc_request(2, "account/read", json!({"refreshToken": false})),
            r#"{"jsonrpc":"2.0","id":2,"method":"account/read","params":{"refreshToken":false}}"#
        );
        assert!(!rpc_notification("initialized", json!({})).contains("\"id\""));
    }

    #[test]
    fn route_line_only_accepts_our_answers() {
        assert_eq!(
            route_line(r#"{"jsonrpc":"2.0","id":1,"result":{"ok":true}}"#),
            Some((1, Some(json!({"ok": true}))))
        );
        // An error answer still UNBLOCKS its waiter (with no result).
        assert_eq!(
            route_line(r#"{"jsonrpc":"2.0","id":3,"error":{"code":-32601}}"#),
            Some((3, None))
        );
        // A server-initiated request must never be mistaken for an answer.
        assert_eq!(route_line(r#"{"jsonrpc":"2.0","id":9,"method":"elicit"}"#), None);
        assert_eq!(route_line(r#"{"jsonrpc":"2.0","method":"log"}"#), None);
        assert_eq!(route_line("plain log output"), None);
        assert_eq!(route_line(""), None);
    }

    #[cfg(unix)]
    fn write_stub(tag: &str, body: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "exp-codex-app-server-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("codex");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    /// The happy path against a stub app-server: both answers ride back and
    /// the child is gone when the probe returns.
    #[cfg(unix)]
    #[test]
    fn probe_reads_the_account_and_the_rate_limits() {
        let stub = write_stub(
            "ok",
            r#"while IFS= read -r line; do
  case "$line" in
    *'"id":1'*) echo '{"jsonrpc":"2.0","id":1,"result":{"userAgent":"codex"}}' ;;
    *'"id":2'*) echo '{"jsonrpc":"2.0","id":2,"result":{"account":{"type":"chatgpt","email":"dev@acme.test","planType":"pro"}}}' ;;
    *'"id":3'*) echo '{"jsonrpc":"2.0","id":3,"result":{"rateLimits":{"primary":{"usedPercent":23,"windowDurationMins":300}}}}' ;;
  esac
done"#,
        );
        // Retry the transient ETXTBSY race a concurrent fork can cause
        // (same rule as the doctor's stub tests).
        let mut probed = probe(&stub.to_string_lossy(), "", Duration::from_secs(10));
        for _ in 0..20 {
            if probed.is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
            probed = probe(&stub.to_string_lossy(), "", Duration::from_secs(10));
        }
        let probed = probed.expect("stub app-server must answer");
        assert_eq!(
            probed.account.as_ref().unwrap()["account"]["email"],
            "dev@acme.test"
        );
        assert_eq!(
            probed.rate_limits.as_ref().unwrap()["rateLimits"]["primary"]["windowDurationMins"],
            300
        );
        let _ = std::fs::remove_dir_all(stub.parent().unwrap());
    }

    /// A silent (or wedged) app-server is killed at the deadline instead of
    /// stalling the collector.
    #[cfg(unix)]
    #[test]
    fn probe_gives_up_on_a_silent_server() {
        // A shell BUILTIN loop: the stub reads every request and answers
        // none, holding its stdout open (an external `sleep` would not even
        // resolve under the empty PATH this probe injects).
        let stub = write_stub("silent", "while IFS= read -r line; do :; done");
        let started = Instant::now();
        let err = probe(&stub.to_string_lossy(), "", Duration::from_millis(300)).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "must return at the deadline"
        );
        let _ = std::fs::remove_dir_all(stub.parent().unwrap());
    }

    #[test]
    fn probe_reports_a_missing_binary() {
        let err = probe(
            "definitely-not-a-real-binary-exp",
            "",
            Duration::from_millis(200),
        )
        .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }
}
