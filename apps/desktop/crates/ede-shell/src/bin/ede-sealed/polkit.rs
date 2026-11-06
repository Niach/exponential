//! The polkit authentication agent (#EDE-17): registered with polkitd for
//! this session; `BeginAuthentication` opens the sealed prompt and the
//! answer goes through `polkit-agent-helper-1` (setuid, speaks PAM for
//! the identity polkit offers; its line protocol: the cookie, then the
//! password on each `PAM_PROMPT_ECHO_OFF`, `SUCCESS` or `FAILURE` at the
//! end). `CancelAuthentication` closes the prompt.

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    sync::mpsc,
};

use zbus::zvariant::{OwnedValue, Value};

pub const HELPER: &str = "/usr/lib/polkit-1/polkit-agent-helper-1";
pub const AGENT_PATH: &str = "/at/exponential/ede/PolkitAgent";

/// One request from polkitd.
#[derive(Debug, Clone)]
pub struct Request {
    pub cookie: String,
    pub message: String,
    pub action_id: String,
    /// The unix user names polkit offers (the first is the default).
    pub users: Vec<String>,
}

/// What the prompt answers.
#[derive(Debug)]
pub enum Answer {
    Password(String),
    Cancel,
}

/// What the prompt hears from the agent.
#[derive(Debug)]
pub enum UiEvent {
    /// Show the prompt; answers go back over the sender.
    Begin(Request, mpsc::Sender<Answer>),
    /// The password was wrong (or the helper failed): say so, keep the prompt.
    Failed(String, String),
    /// polkit is done with this cookie (authenticated, or cancelled): close the prompt.
    Done(String),
}

/// The agent object: requests go to the prompt over `to_ui`; the method
/// blocks on the answers (polkit waits for it).
pub struct Agent {
    pub to_ui: mpsc::Sender<UiEvent>,
}

#[zbus::interface(name = "org.freedesktop.PolicyKit1.AuthenticationAgent")]
impl Agent {
    #[allow(clippy::too_many_arguments)]
    fn begin_authentication(&self, action_id: String, message: String, _icon_name: String, _details: HashMap<String, String>, cookie: String, identities: Vec<(String, HashMap<String, OwnedValue>)>) -> zbus::fdo::Result<()> {
        let users: Vec<String> = identities
            .iter()
            .filter(|(kind, _)| kind == "unix-user")
            .filter_map(|(_, details)| details.get("uid").and_then(|v| u32::try_from(v.clone()).ok()))
            .filter_map(user_name)
            .collect();
        if users.is_empty() {
            return Err(zbus::fdo::Error::Failed("no unix user to authenticate as".into()));
        }
        let (answer_tx, answer_rx) = mpsc::channel();
        let request = Request { cookie: cookie.clone(), message, action_id, users: users.clone() };
        self.to_ui.send(UiEvent::Begin(request, answer_tx)).map_err(|_| zbus::fdo::Error::Failed("the prompt is gone".into()))?;
        // The person may try again until the helper says yes or they cancel.
        let result = loop {
            match answer_rx.recv() {
                Ok(Answer::Password(password)) => match run_helper(&users[0], &cookie, &password) {
                    Ok(()) => break Ok(()),
                    Err(e) => {
                        tracing::info!("polkit helper: {e}");
                        let _ = self.to_ui.send(UiEvent::Failed(cookie.clone(), e));
                    }
                },
                Ok(Answer::Cancel) | Err(_) => break Err(zbus::fdo::Error::Failed("cancelled".into())),
            }
        };
        let _ = self.to_ui.send(UiEvent::Done(cookie));
        result
    }

    fn cancel_authentication(&self, cookie: String) {
        let _ = self.to_ui.send(UiEvent::Done(cookie));
    }
}

/// `polkit-agent-helper-1 <user>` with the cookie and the password.
pub fn run_helper(user: &str, cookie: &str, password: &str) -> Result<(), String> {
    let mut child = Command::new(HELPER).arg(user).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| format!("{HELPER}: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    writeln!(stdin, "{cookie}").map_err(|e| e.to_string())?;
    let mut result = Err("no answer from the helper".to_string());
    for line in BufReader::new(stdout).lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.starts_with("PAM_PROMPT_ECHO_OFF") || line.starts_with("PAM_PROMPT_ECHO_ON") {
            if writeln!(stdin, "{password}").is_err() {
                break;
            }
        } else if line.starts_with("SUCCESS") {
            result = Ok(());
        } else if line.starts_with("FAILURE") {
            result = Err("wrong password".into());
        } else if let Some(msg) = line.strip_prefix("PAM_ERROR_MSG ") {
            result = Err(msg.to_string());
        }
    }
    let _ = child.wait();
    result
}

fn user_name(uid: u32) -> Option<String> {
    // SAFETY: getpwuid returns a static buffer or null; we copy the name out.
    let pw = unsafe { libc::getpwuid(uid) };
    if pw.is_null() {
        return None;
    }
    Some(unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) }.to_string_lossy().into_owned())
}

/// Registers with polkitd for this session; `None` without polkit.
pub fn register(conn: &zbus::blocking::Connection) -> Result<(), String> {
    let session = std::env::var("XDG_SESSION_ID").ok();
    let mut subject: HashMap<String, Value<'_>> = HashMap::new();
    let kind = match session {
        Some(id) => {
            subject.insert("session-id".into(), Value::from(id));
            "unix-session"
        }
        None => {
            subject.insert("pid".into(), Value::from(std::process::id()));
            subject.insert("start-time".into(), Value::from(0u64));
            "unix-process"
        }
    };
    let locale = std::env::var("LANG").unwrap_or_else(|_| "en_US.UTF-8".into());
    conn.call_method(Some("org.freedesktop.PolicyKit1"), "/org/freedesktop/PolicyKit1/Authority", Some("org.freedesktop.PolicyKit1.Authority"), "RegisterAuthenticationAgent", &((kind, subject), locale, AGENT_PATH))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_helper_is_where_polkit_ships_it_or_absent() {
        // Present on every polkit install; the path is the distro's.
        let _ = std::path::Path::new(super::HELPER).exists();
    }
}
