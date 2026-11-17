//! The polkit authentication agent (#EDE-17): registered with polkitd for
//! the login session; `BeginAuthentication` opens the sealed prompt and the
//! answer goes to polkit's helper, which speaks PAM for the identity polkit
//! offers: `/run/polkit/agent-helper.socket` where polkit has it (socket
//! activated, polkit ≥ 124: the user name, then the cookie), else the setuid
//! `polkit-agent-helper-1 <user>` (the cookie). Then the same line protocol
//! both ways: the password on each `PAM_PROMPT_ECHO_OFF`, `SUCCESS` or
//! `FAILURE` at the end. `CancelAuthentication` closes the prompt.

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    path::Path,
    process::{Command, Stdio},
    sync::mpsc,
};

use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

pub const HELPER: &str = "/usr/lib/polkit-1/polkit-agent-helper-1";
pub const HELPER_SOCKET: &str = "/run/polkit/agent-helper.socket";
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
/// waits for the answers (polkit waits for it).
pub struct Agent {
    pub to_ui: mpsc::Sender<UiEvent>,
}

#[zbus::interface(name = "org.freedesktop.PolicyKit1.AuthenticationAgent")]
impl Agent {
    #[allow(clippy::too_many_arguments)]
    async fn begin_authentication(&self, action_id: String, message: String, _icon_name: String, _details: HashMap<String, String>, cookie: String, identities: Vec<(String, HashMap<String, OwnedValue>)>) -> zbus::fdo::Result<()> {
        let users: Vec<String> = identities
            .iter()
            .filter(|(kind, _)| kind == "unix-user")
            .filter_map(|(_, details)| details.get("uid").and_then(|v| u32::try_from(v.clone()).ok()))
            .filter_map(user_name)
            .collect();
        if users.is_empty() {
            return Err(zbus::fdo::Error::Failed("no unix user to authenticate as".into()));
        }
        let request = Request { cookie, message, action_id, users };
        // The wait on its own thread (#EDE-87): the bus keeps serving, so
        // polkitd's `CancelAuthentication` (Ctrl+C on pkexec) closes the prompt.
        let to_ui = self.to_ui.clone();
        blocking::unblock(move || authenticate(&to_ui, request)).await
    }

    fn cancel_authentication(&self, cookie: String) {
        let _ = self.to_ui.send(UiEvent::Done(cookie));
    }
}

/// Shows the prompt; the person may try again until the helper says yes or they cancel.
fn authenticate(to_ui: &mpsc::Sender<UiEvent>, request: Request) -> zbus::fdo::Result<()> {
    let (answer_tx, answer_rx) = mpsc::channel();
    let (cookie, user) = (request.cookie.clone(), request.users[0].clone());
    to_ui.send(UiEvent::Begin(request, answer_tx)).map_err(|_| zbus::fdo::Error::Failed("the prompt is gone".into()))?;
    let result = loop {
        match answer_rx.recv() {
            Ok(Answer::Password(password)) => match run_helper(&user, &cookie, &password) {
                Ok(()) => break Ok(()),
                Err(e) => {
                    tracing::info!("polkit helper: {e}");
                    let _ = to_ui.send(UiEvent::Failed(cookie.clone(), e));
                }
            },
            Ok(Answer::Cancel) | Err(_) => break Err(zbus::fdo::Error::Failed("cancelled".into())),
        }
    };
    let _ = to_ui.send(UiEvent::Done(cookie));
    result
}

/// polkit's helper with the cookie and the password.
pub fn run_helper(user: &str, cookie: &str, password: &str) -> Result<(), String> {
    run_helper_with(Path::new(HELPER_SOCKET), Path::new(HELPER), user, cookie, password)
}

/// What polkit's own agent library does (`polkitagentsession.c`): the
/// helper socket when it exists and answers, the setuid helper otherwise
/// (on polkit 127 that binary is no longer setuid and says FAILURE to every
/// password, #EDE-87).
pub fn run_helper_with(socket: &Path, setuid: &Path, user: &str, cookie: &str, password: &str) -> Result<(), String> {
    if socket.exists() {
        match UnixStream::connect(socket) {
            Ok(stream) => {
                let reader = stream.try_clone().map_err(|e| e.to_string())?;
                return converse(reader, stream, &[user, cookie], password);
            }
            Err(e) => tracing::warn!("{}: {e}; the setuid helper instead", socket.display()),
        }
    }
    let mut child = Command::new(setuid).arg(user).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| format!("{}: {e}", setuid.display()))?;
    let stdin = child.stdin.take().ok_or("no stdin")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let result = converse(stdout, stdin, &[cookie], password);
    let _ = child.wait();
    result
}

/// The helper's line protocol: `first` lines, then the password on each prompt.
fn converse(reader: impl Read, mut writer: impl Write, first: &[&str], password: &str) -> Result<(), String> {
    for line in first {
        writeln!(writer, "{line}").map_err(|e| e.to_string())?;
    }
    let mut result = Err("no answer from the helper".to_string());
    for line in BufReader::new(reader).lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.starts_with("PAM_PROMPT_ECHO_OFF") || line.starts_with("PAM_PROMPT_ECHO_ON") {
            if writeln!(writer, "{password}").is_err() {
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

/// The login session polkit asks this agent for (#EDE-87): the agent runs
/// as a user unit, in no session of its own, so `XDG_SESSION_ID` when the
/// unit has it, else logind's graphical session of this user. Never the
/// agent's own process: polkitd would ask it for nobody.
pub fn session_for(env: Option<String>, logind: impl FnOnce() -> Option<String>) -> Result<String, String> {
    env.filter(|id| !id.is_empty())
        .or_else(logind)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| "no login session: no XDG_SESSION_ID, and logind names no graphical session for this user".into())
}

/// logind's graphical session of this user (`User.Display`).
fn logind_session(conn: &zbus::blocking::Connection) -> Option<String> {
    let login1 = Some("org.freedesktop.login1");
    let user = conn.call_method(login1, "/org/freedesktop/login1", Some("org.freedesktop.login1.Manager"), "GetUser", &(unsafe { libc::getuid() },)).ok()?;
    let path: OwnedObjectPath = user.body().deserialize().ok()?;
    let display = conn.call_method(login1, path.as_str(), Some("org.freedesktop.DBus.Properties"), "Get", &("org.freedesktop.login1.User", "Display")).ok()?;
    let value: OwnedValue = display.body().deserialize().ok()?;
    let (id, _): (String, OwnedObjectPath) = value.try_into().ok()?;
    Some(id)
}

/// Registers with polkitd for the login session; the session's id.
pub fn register(conn: &zbus::blocking::Connection) -> Result<String, String> {
    let session = session_for(std::env::var("XDG_SESSION_ID").ok(), || logind_session(conn))?;
    let subject: HashMap<String, Value<'_>> = HashMap::from([("session-id".to_string(), Value::from(session.clone()))]);
    let locale = std::env::var("LANG").unwrap_or_else(|_| "en_US.UTF-8".into());
    conn.call_method(Some("org.freedesktop.PolicyKit1"), "/org/freedesktop/PolicyKit1/Authority", Some("org.freedesktop.PolicyKit1.Authority"), "RegisterAuthenticationAgent", &(("unix-session", subject), locale, AGENT_PATH))
        .map(|_| session)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{os::unix::net::UnixStream, time::Duration};

    #[test]
    fn the_helper_is_where_polkit_ships_it_or_absent() {
        // Present on every polkit install; the path is the distro's.
        let _ = std::path::Path::new(super::HELPER).exists();
    }

    /// The agent served on one end of a socket pair, polkitd's side on the other.
    fn agent_on_a_pair() -> (zbus::blocking::Connection, zbus::blocking::Connection, mpsc::Receiver<UiEvent>) {
        let (a, b) = UnixStream::pair().unwrap();
        let (to_ui, ui) = mpsc::channel();
        let server = std::thread::spawn(move || {
            zbus::blocking::connection::Builder::unix_stream(a).server(zbus::Guid::generate()).unwrap().p2p().serve_at(AGENT_PATH, Agent { to_ui }).unwrap().build().unwrap()
        });
        let polkitd = zbus::blocking::connection::Builder::unix_stream(b).p2p().build().unwrap();
        (server.join().unwrap(), polkitd, ui)
    }

    /// #EDE-87 (Ctrl+C on pkexec): polkitd's `CancelAuthentication` for an
    /// open prompt is heard and closes it, and `BeginAuthentication` answers.
    #[test]
    fn a_cancel_is_heard_while_a_prompt_is_open() {
        let (_agent, polkitd, ui) = agent_on_a_pair();
        let uid = unsafe { libc::getuid() };
        let identity: HashMap<String, Value<'_>> = HashMap::from([("uid".to_string(), Value::from(uid))]);
        let (begun_tx, begun) = mpsc::channel();
        let caller = polkitd.clone();
        std::thread::spawn(move || {
            let body = ("org.example.act", "Do it", "", HashMap::<String, String>::new(), "cookie-1", vec![("unix-user", identity)]);
            let r = caller.call_method(None::<&str>, AGENT_PATH, Some("org.freedesktop.PolicyKit1.AuthenticationAgent"), "BeginAuthentication", &body);
            let _ = begun_tx.send(r.is_ok());
        });
        let Ok(UiEvent::Begin(request, answer)) = ui.recv_timeout(Duration::from_secs(5)) else { panic!("no prompt") };
        assert_eq!(request.cookie, "cookie-1");
        let (cancelled_tx, cancelled) = mpsc::channel();
        std::thread::spawn(move || {
            let r = polkitd.call_method(None::<&str>, AGENT_PATH, Some("org.freedesktop.PolicyKit1.AuthenticationAgent"), "CancelAuthentication", &("cookie-1",));
            let _ = cancelled_tx.send(r.is_ok());
        });
        assert_eq!(cancelled.recv_timeout(Duration::from_secs(5)), Ok(true), "CancelAuthentication answered while the prompt is open");
        assert!(matches!(ui.recv_timeout(Duration::from_secs(5)), Ok(UiEvent::Done(c)) if c == "cookie-1"), "the prompt closes");
        drop(answer); // what the prompt does when it closes
        assert_eq!(begun.recv_timeout(Duration::from_secs(5)), Ok(false), "BeginAuthentication ends as cancelled");
    }

    /// #EDE-87 P1: a user unit has no `XDG_SESSION_ID`; the agent then
    /// registers for logind's session, and without one it fails instead of
    /// registering for its own process.
    #[test]
    fn the_agent_registers_for_the_login_session_never_its_own_process() {
        assert_eq!(session_for(Some("5".into()), || panic!("logind is not asked")), Ok("5".into()));
        assert_eq!(session_for(None, || Some("3".into())), Ok("3".into()));
        assert_eq!(session_for(Some(String::new()), || Some("3".into())), Ok("3".into()));
        assert!(session_for(None, || None).is_err());
        assert!(session_for(None, || Some(String::new())).is_err());
    }

    /// A helper on a Unix socket that knows one user and one password: it
    /// reads the user, then the cookie (polkit's socket protocol).
    fn fake_socket_helper(path: &Path) -> std::thread::JoinHandle<Vec<String>> {
        let listener = std::os::unix::net::UnixListener::bind(path).unwrap();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut lines = BufReader::new(stream.try_clone().unwrap()).lines();
            let mut out = stream;
            let user = lines.next().unwrap().unwrap();
            let cookie = lines.next().unwrap().unwrap();
            writeln!(out, "PAM_PROMPT_ECHO_OFF Password: ").unwrap();
            let password = lines.next().unwrap().unwrap();
            writeln!(out, "{}", if user == "alice" && cookie == "cookie-1" && password == "right" { "SUCCESS" } else { "FAILURE" }).unwrap();
            vec![user, cookie, password]
        })
    }

    /// The setuid helper's protocol: the user as its argument, the cookie on stdin.
    fn fake_setuid_helper(dir: &Path) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("polkit-agent-helper-1");
        std::fs::write(&path, "#!/bin/sh\nread cookie\necho 'PAM_PROMPT_ECHO_OFF Password: '\nread pw\nif [ \"$1\" = alice ] && [ \"$cookie\" = cookie-1 ] && [ \"$pw\" = right ]; then echo SUCCESS; else echo FAILURE; fi\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    /// #EDE-87 P2: polkit 127's helper is the socket; the setuid helper is
    /// only for a polkit without it (or a socket nobody answers).
    #[test]
    fn the_helper_socket_when_polkit_has_one() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("agent-helper.socket");
        // A setuid helper that is not setuid (polkit 127) says no to everything.
        let refusing = dir.path().join("refusing");
        std::fs::write(&refusing, "#!/bin/sh\nread cookie\necho FAILURE\n").unwrap();
        std::fs::set_permissions(&refusing, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let helper = fake_socket_helper(&socket);
        assert_eq!(run_helper_with(&socket, &refusing, "alice", "cookie-1", "right"), Ok(()));
        assert_eq!(helper.join().unwrap(), ["alice", "cookie-1", "right"]);
        let again = dir.path().join("again.socket");
        let helper = fake_socket_helper(&again);
        assert_eq!(run_helper_with(&again, &refusing, "alice", "cookie-1", "wrong"), Err("wrong password".into()));
        helper.join().unwrap();
    }

    #[test]
    fn the_setuid_helper_without_the_socket() {
        let dir = tempfile::tempdir().unwrap();
        let setuid = fake_setuid_helper(dir.path());
        let absent = dir.path().join("agent-helper.socket");
        assert_eq!(run_helper_with(&absent, &setuid, "alice", "cookie-1", "right"), Ok(()));
        assert_eq!(run_helper_with(&absent, &setuid, "alice", "cookie-1", "wrong"), Err("wrong password".into()));
        // A socket file nobody listens on: the setuid helper, as polkit's library does.
        drop(std::os::unix::net::UnixListener::bind(&absent).unwrap());
        assert!(absent.exists());
        assert_eq!(run_helper_with(&absent, &setuid, "alice", "cookie-1", "right"), Ok(()));
    }
}
