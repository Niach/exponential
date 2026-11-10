//! EXP-1513: a Local desktop without a usable CLI sets Claude Code up from
//! the chat's empty state (`chat_no_cli` in ede/ui/chat.json): Anthropic's
//! own installer when `claude` is missing ([`eagent::install`]), then the
//! CLI's OWN sign-in (`claude auth login`, which opens the browser itself).
//! Nothing here reads or writes a credential; the CLI keeps its login
//! wherever it always does.
//!
//! One background thread per setup reports [`Step`]s on a channel the main
//! loop drains; `Ready` restarts the desktop's agent so it picks the CLI up.

use std::time::{Duration, Instant};

use eagent::agent::CodingAgent;
use eagent::detect::{check_tool, is_signed_in, Tool};
use eagent::login::LocalLogin;
use serde_json::{json, Value};

/// How long the sign-in may wait for the person in the browser.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// How often the CLI is asked whether it is signed in yet.
const SIGN_IN_POLL: Duration = Duration::from_secs(2);

/// Where a setup is. `Idle` = none ran (or the last one finished).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Idle,
    Installing,
    /// The CLI's sign-in is open; `url` once its screen showed the link.
    SigningIn { url: Option<String> },
    Ready,
    Failed(String),
}

impl Step {
    /// `/chatting/setup`: what the empty state binds (`state` + its words).
    pub fn to_value(&self) -> Value {
        match self {
            Step::Idle | Step::Ready => json!({"state": "idle", "message": "", "url": ""}),
            Step::Installing => json!({"state": "installing", "message": "", "url": ""}),
            Step::SigningIn { url } => json!({"state": "signingIn", "message": "", "url": url.clone().unwrap_or_default()}),
            Step::Failed(message) => json!({"state": "failed", "message": message, "url": ""}),
        }
    }

    /// Whether a setup is running (a second press is ignored).
    pub fn running(&self) -> bool {
        matches!(self, Step::Installing | Step::SigningIn { .. })
    }
}

/// Start one setup on its own thread; its steps arrive on the receiver.
pub fn start() -> flume::Receiver<Step> {
    let (tx, rx) = flume::unbounded();
    let spawned = std::thread::Builder::new().name("ede-cli-setup".into()).spawn(move || {
        let last = run(&|step| {
            let _ = tx.send(step);
        });
        let _ = tx.send(last);
    });
    if let Err(e) = spawned {
        let (tx, rx) = flume::unbounded();
        let _ = tx.send(Step::Failed(format!("Could not start the setup: {e}")));
        return rx;
    }
    rx
}

/// The setup itself: install if needed, then sign in. Returns the last step.
fn run(report: &dyn Fn(Step)) -> Step {
    let agent = CodingAgent::Claude;
    let path = eagent::spawn::login_path();
    let mut program = eagent::install::resolve_program(agent, &path);
    if !check_tool(Tool::Claude, &program, &path).ok {
        report(Step::Installing);
        match eagent::install::install(agent, &path) {
            Ok(installed) => program = installed.program,
            Err(message) => return Step::Failed(message),
        }
    }
    if is_signed_in(agent, &program, &path) == Some(true) {
        return Step::Ready;
    }
    report(Step::SigningIn { url: None });
    let mut login = match LocalLogin::start(agent, &program, &path, false) {
        Ok(login) => login,
        Err(e) => return Step::Failed(format!("Could not start the Claude Code sign-in: {e}")),
    };
    let deadline = Instant::now() + SIGN_IN_TIMEOUT;
    let mut url: Option<String> = None;
    loop {
        std::thread::sleep(SIGN_IN_POLL);
        if let Some(progress) = login.poll() {
            match progress.url {
                Some(seen) if url.as_deref() != Some(seen.as_str()) => {
                    url = Some(seen);
                    report(Step::SigningIn { url: url.clone() });
                }
                Some(_) => {}
                None => return Step::Failed(progress.message.unwrap_or_else(|| "The Claude Code sign-in stopped. Try again.".into())),
            }
        }
        if is_signed_in(agent, &program, &path) == Some(true) {
            return Step::Ready;
        }
        if Instant::now() >= deadline {
            return Step::Failed("The sign-in timed out. Try again.".into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_bind_one_state_word() {
        assert_eq!(Step::Idle.to_value()["state"], "idle");
        assert_eq!(Step::Ready.to_value()["state"], "idle", "a finished setup leaves the chat to the agent");
        assert_eq!(Step::Installing.to_value()["state"], "installing");
        let signing = Step::SigningIn { url: Some("https://claude.ai/oauth".into()) }.to_value();
        assert_eq!((signing["state"].as_str(), signing["url"].as_str()), (Some("signingIn"), Some("https://claude.ai/oauth")));
        let failed = Step::Failed("Claude Code install failed: no network".into()).to_value();
        assert_eq!(failed["message"], "Claude Code install failed: no network");
        assert!(Step::Installing.running() && !Step::Failed(String::new()).running());
    }
}
