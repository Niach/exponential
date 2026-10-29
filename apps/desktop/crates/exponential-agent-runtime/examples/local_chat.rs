//! One turn with your own `claude` (or `codex`), no server (EXP-1426 part 5).
//!
//! ```sh
//! cargo run -p exponential-agent-runtime --example local_chat -- "reply with the word ok"
//! cargo run -p exponential-agent-runtime --example local_chat -- --agent codex "hi"
//! ```
//!
//! Detects the CLI, says whether it is signed in (and how to sign in when it
//! is not), runs ONE turn in the current directory printing the narration and
//! the tool rows, then lists the last 5 conversations of this directory.
//! A question or permission card is printed and left unanswered (Ctrl-C).

use std::io::Write;
use std::time::Duration;

use exponential_agent_runtime::agent::CodingAgent;
use exponential_agent_runtime::local::{LocalAgent, RunRequest};
use exponential_agent_runtime::wire::{ActivityEvent, TurnState};

fn main() {
    let mut agent = CodingAgent::Claude;
    let mut words = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--agent" {
            let name = args.next().unwrap_or_default();
            agent = CodingAgent::parse(&name).unwrap_or_else(|| {
                eprintln!("unknown agent {name:?} (claude | codex)");
                std::process::exit(2);
            });
        } else {
            words.push(arg);
        }
    }
    let prompt = if words.is_empty() { "reply with the word ok".to_string() } else { words.join(" ") };

    let local = LocalAgent::new(agent);
    let check = local.check();
    if !check.ok {
        eprintln!("{}: {}", agent.label(), check.error.unwrap_or_default());
        std::process::exit(1);
    }
    println!("{} {}", agent.label(), check.version.unwrap_or_default());
    match local.signed_in() {
        Some(true) => println!("signed in"),
        signed => {
            let state = if signed.is_none() { "sign-in status unknown" } else { "not signed in" };
            println!("{state}; sign in with: {}", local.login_command_line());
            std::process::exit(1);
        }
    }

    let cwd = std::env::current_dir().expect("a current directory");
    let run = match local.start(RunRequest::new(cwd.clone()).prompt(prompt)) {
        Ok(run) => run,
        Err(err) => {
            eprintln!("start failed: {err}");
            std::process::exit(1);
        }
    };
    let events = run.events().expect("the run's own channel");
    let mut started = false;
    loop {
        match events.recv_timeout(Duration::from_millis(250)) {
            Ok(ActivityEvent::Narration { text, .. }) => {
                print!("{text}");
                let _ = std::io::stdout().flush();
            }
            Ok(ActivityEvent::Tool { name, detail, .. }) => {
                println!("\n· {name} {}", detail.unwrap_or_default());
            }
            Ok(ActivityEvent::Question { text, id, .. }) => {
                println!("\n? {text} [{id}] (answer with LocalRun::answer)");
            }
            Ok(ActivityEvent::ApiError { .. }) => println!("\n! the agent reported an API error"),
            Ok(ActivityEvent::Turn { state: TurnState::Started, .. }) => started = true,
            Ok(ActivityEvent::Turn { state: TurnState::Ended, .. }) if started => break,
            Ok(_) => {}
            Err(flume::RecvTimeoutError::Timeout) if run.is_done() => break,
            Err(flume::RecvTimeoutError::Timeout) => {}
            Err(flume::RecvTimeoutError::Disconnected) => break,
        }
    }
    println!();
    if run.is_done() {
        let exit = run.wait();
        println!("the run ended: {} {}", exit.outcome, exit.error.unwrap_or_default());
    } else {
        if let Some(id) = run.native_session_id() {
            println!("session {id}");
        }
        run.stop();
    }

    println!("\nlast 5 conversations here:");
    for past in local.sessions(Some(&cwd), Some(5)) {
        println!("  {}  {}", past.id, past.title);
    }
}
