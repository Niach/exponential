//! EXP-746 — replay one ended ACP run's transcript through the engine, the
//! way the desktop's Past row does (`EngineSession::open_transcript`), and
//! print every local feed event. A debugging aid for the replay path, which
//! has no CLI surface of its own.
//!
//! `cargo run -p engine --example exp746_replay -- <claude|codex|pi> <acp session id> <cwd>`

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

struct StderrLog;

impl log::Log for StderrLog {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        eprintln!("[{}] {}", record.level(), record.args());
    }
    fn flush(&self) {}
}

static LOGGER: StderrLog = StderrLog;

fn main() {
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Debug);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [agent, session_id, cwd] = args.as_slice() else {
        eprintln!("usage: exp746_replay <claude|codex|pi> <acp session id> <cwd>");
        std::process::exit(2);
    };
    let agent = match agent.as_str() {
        "claude" => coding::CodingAgent::Claude,
        "codex" => coding::CodingAgent::Codex,
        "pi" => coding::CodingAgent::Pi,
        other => {
            eprintln!("unknown agent {other}");
            std::process::exit(2);
        }
    };
    let runtime = steer::SteerRuntime::new().expect("a steer runtime");
    // What the desktop's Past row hands over: pi is file-keyed, and a claude
    // or codex run's ACP id doubles as its native handle (D8).
    let native = match agent {
        coding::CodingAgent::Pi => engine::ResumeHandle::PiSessionFile(PathBuf::from(session_id)),
        _ => engine::ResumeHandle::Acp(session_id.clone()),
    };
    let session = engine::EngineSession::open_transcript(engine::OpenTranscript {
        runtime,
        data_dir: std::env::temp_dir().join("exp746-replay"),
        personal_key: None,
        handle: engine::HistoryHandle {
            agent: coding::AgentKind::Builtin(agent),
            cwd: PathBuf::from(cwd),
            acp_session_id: Some(session_id.clone()),
            native,
        },
        local_sink: Arc::new(|_| {}),
    })
    .unwrap_or_else(|error| {
        eprintln!("open_transcript failed: {error:?}");
        std::process::exit(1);
    });
    let events = session.subscribe();
    let started = std::time::Instant::now();
    while let Ok(event) = events.recv_timeout(Duration::from_secs(30)) {
        println!("{:?}", event);
        if matches!(event, engine::LocalFeedEvent::Phase(engine::EnginePhase::Ended)) {
            break;
        }
    }
    println!("done after {:?}; exit: {:?}", started.elapsed(), session.wait_timeout(Duration::from_secs(5)).map(|exit| (exit.outcome, exit.error)));
}
