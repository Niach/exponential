//! The polkit authentication agent (#EDE-17): registered with polkitd for
//! the login session; `BeginAuthentication` opens the sealed prompt and the
//! answer goes to polkit's helper, which speaks PAM for the identity polkit
//! offers: `/run/polkit/agent-helper.socket` where polkit has it (socket
//! activated, polkit ≥ 124: the user name, then the cookie), else the setuid
//! `polkit-agent-helper-1 <user>` (the cookie). Then the same line protocol
//! both ways: the password on each `PAM_PROMPT_ECHO_OFF`, `SUCCESS` or
//! `FAILURE` at the end. `CancelAuthentication` closes the prompt.
//!
//! #EDE-127: polkitd finds this agent by the login session, and what an
//! agent launched into an agent space runs in that session too. A request
//! whose subject or caller belongs to a space is refused before anything
//! maps: no prompt on the person's screen, their keyboard stays where it
//! is, the space's command fails.
//!
//! #EDE-138: what a space launches runs in a scope of the space's own
//! systemd slice, and that is looked at first: a process keeps its cgroup
//! when it starts a session of its own with an empty environment and lets
//! its parent die, which leaves every other sign behind.
//!
//! #EDE-143: a cgroup is no wall inside one user (a process may write
//! itself into another one's `cgroup.procs`), but the login session's scope
//! is root's: no space's process gets back into it. So three kinds of
//! request: (a) from the person's session (the compositor's own cgroup,
//! while no space has launches outside systemd scopes): the prompt as
//! always, with the keyboard; (b) from a space: refused; (c) from anywhere
//! else of the user (their services, snaps, Flatpaks, a coding session's
//! shell, a space's process that moved itself): the prompt shows WITHOUT
//! taking the keyboard and names the asking process; the person clicks
//! into it to type. (c) only while an agent space exists (one is open, a
//! closed one's slice is still stopping, or a launch is alive): with no
//! space every request is (a), as before #EDE-143.

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{mpsc, Arc},
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
    /// #EDE-143: the asking process, named on the prompt, when the request
    /// is not from the person's session ([`Asked::Elsewhere`]): then the
    /// prompt does not take the keyboard. `None` = the person's.
    pub asker: Option<String>,
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

/// Who asks: [`Asked`] for these processes (#EDE-127, #EDE-143).
pub type SpaceOf = Arc<dyn Fn(&[u32]) -> Asked + Send + Sync>;

/// Who a request comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Asked {
    /// An agent space's process (`space-n`): refused before anything maps.
    Space(String),
    /// The person's session: every process named runs in the compositor's
    /// own cgroup, and no space has launches outside systemd scopes; or no
    /// agent space exists at all. The prompt takes the keyboard.
    Person,
    /// Neither: the prompt shows without the keyboard and names the
    /// process. Also when the compositor cannot be asked.
    Elsewhere,
}

/// What the compositor says about a request's processes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Said {
    /// The space one of them belongs to.
    pub space: Option<String>,
    /// All of them run in the person's session and no space has launches
    /// outside systemd scopes ([`ede_protocol::ProcessAnswer`]).
    pub session: bool,
    /// No agent space exists: every request is the person's. `false` from
    /// a compositor that cannot be asked, or does not say.
    pub no_space: bool,
}

/// The processes polkitd names in a request's details: its subject's
/// (pkexec's parent, the client of a system service) and its caller's
/// (pkexec itself, the service).
pub const PIDS: [&str; 2] = ["polkit.subject-pid", "polkit.caller-pid"];

/// The processes polkitd names in a request's details, the subject's first.
pub fn named_pids(details: &HashMap<String, String>) -> Vec<u32> {
    PIDS.iter().filter_map(|key| details.get(*key)?.trim().parse().ok()).filter(|pid| *pid > 1).collect()
}

/// Who a request comes from, by its subject's and its caller's process.
/// One that names no process is asked about with none: nothing vouches for
/// it, so it is the person's only while no agent space exists.
pub fn asked_from(details: &HashMap<String, String>, space_of: &(dyn Fn(&[u32]) -> Asked + Send + Sync)) -> Asked {
    space_of(&named_pids(details))
}

/// A process as the prompt names it: its command line (shortened), and
/// its id.
pub fn describe(pid: u32) -> String {
    let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    let mut line = cmdline.split(|b| *b == 0).filter(|arg| !arg.is_empty()).map(|arg| String::from_utf8_lossy(arg).into_owned()).collect::<Vec<_>>().join(" ");
    if line.is_empty() {
        line = std::fs::read_to_string(format!("/proc/{pid}/comm")).map(|comm| comm.trim().to_string()).unwrap_or_else(|_| "a process that ended".into());
    }
    if line.chars().count() > 80 {
        line = line.chars().take(79).collect::<String>() + "…";
    }
    format!("{line} (process {pid})")
}

/// The space's number in a line for people: `space-3` is agent space 3.
fn number(space: &str) -> &str {
    space.strip_prefix("space-").unwrap_or(space)
}

/// The agent object: requests go to the prompt over `to_ui`; the method
/// waits for the answers (polkit waits for it).
pub struct Agent {
    pub to_ui: mpsc::Sender<UiEvent>,
    /// Whether a request comes from an agent space (#EDE-127).
    pub space_of: SpaceOf,
}

#[zbus::interface(name = "org.freedesktop.PolicyKit1.AuthenticationAgent")]
impl Agent {
    #[allow(clippy::too_many_arguments)]
    async fn begin_authentication(&self, action_id: String, message: String, _icon_name: String, details: HashMap<String, String>, cookie: String, identities: Vec<(String, HashMap<String, OwnedValue>)>) -> zbus::fdo::Result<()> {
        // On its own thread (#EDE-87): the bus keeps serving, so polkitd's
        // `CancelAuthentication` (Ctrl+C on pkexec) closes the prompt; the
        // space check reads /proc and may ask the compositor.
        let (to_ui, space_of) = (self.to_ui.clone(), self.space_of.clone());
        blocking::unblock(move || {
            // #EDE-127: first, before anything can map.
            let asker = match asked_from(&details, &*space_of) {
                Asked::Space(space) => {
                    tracing::info!("refused a polkit request from agent space {}: {action_id}", number(&space));
                    return Err(zbus::fdo::Error::Failed(format!("refused: asked from agent space {}", number(&space))));
                }
                Asked::Person => None,
                // #EDE-143: not the person's session: named, no keyboard.
                Asked::Elsewhere => {
                    let asker = named_pids(&details).first().map(|pid| describe(*pid)).unwrap_or_else(|| "a process polkit did not name".into());
                    tracing::info!("a polkit request from outside the person's session ({asker}): {action_id}");
                    Some(asker)
                }
            };
            let users: Vec<String> = identities
                .iter()
                .filter(|(kind, _)| kind == "unix-user")
                .filter_map(|(_, details)| details.get("uid").and_then(|v| u32::try_from(v.clone()).ok()))
                .filter_map(user_name)
                .collect();
            if users.is_empty() {
                return Err(zbus::fdo::Error::Failed("no unix user to authenticate as".into()));
            }
            authenticate(&to_ui, Request { cookie, message, action_id, users, asker })
        })
        .await
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

/// Who `pids` are (#EDE-127, #EDE-143): an agent space's by what their
/// cgroups say first (#EDE-138: a scope of a space's slice, of any
/// compositor of this user), then by what their environments, or those of
/// the processes they descend from, say about the compositor's spaces
/// under `spaces` (neither needs a round trip, and both hold while the
/// compositor cannot be asked), else by what `compositor` answers from
/// what it launched and what connected to a space's socket; the person's
/// when the compositor says they all run in the person's session, or that
/// no agent space exists; else [`Asked::Elsewhere`].
pub fn space_of_with(pids: &[u32], spaces: Option<&Path>, compositor: impl FnOnce(&[u32]) -> Said) -> Asked {
    space_of_by(pids, ede_protocol::process::cgroup, spaces, compositor)
}

/// [`space_of_with`] with the cgroups read through `cgroup_of` (a test's).
pub fn space_of_by(pids: &[u32], cgroup_of: impl Fn(u32) -> Option<String>, spaces: Option<&Path>, compositor: impl FnOnce(&[u32]) -> Said) -> Asked {
    if let Some((_, space, pid)) = pids.iter().find_map(|pid| ede_protocol::process::space_by_cgroup_with(*pid, &cgroup_of)) {
        tracing::debug!("process {pid} is agent space {}'s by its cgroup", number(space.as_str()));
        return Asked::Space(space.as_str().to_string());
    }
    if let Some(spaces) = spaces {
        if let Some((space, pid)) = pids.iter().find_map(|pid| ede_protocol::process::space_by_environment(*pid, spaces)) {
            tracing::debug!("process {pid} is agent space {}'s by its environment", number(space.as_str()));
            return Asked::Space(space.as_str().to_string());
        }
    }
    match compositor(pids) {
        Said { space: Some(space), .. } => Asked::Space(space),
        Said { session: true, .. } | Said { no_space: true, .. } => Asked::Person,
        Said { .. } => Asked::Elsewhere,
    }
}

/// [`space_of_with`] for this session: the compositor of `$WAYLAND_DISPLAY`.
pub fn space_of(pids: &[u32]) -> Asked {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    let display = std::env::var("WAYLAND_DISPLAY").ok();
    let spaces = runtime.as_deref().zip(display.as_deref()).map(|(runtime, display)| ede_protocol::spaces_path(runtime, display));
    space_of_with(pids, spaces.as_deref(), ask_compositor)
}

/// The compositor's answer for every process: a compositor that cannot be
/// asked (or is too old to know the op) leaves the space to the cgroups and
/// environments and vouches for nobody's session, nor says that no space
/// exists. For a request that names no process it is asked (about the
/// agent's own) only whether any space exists.
fn ask_compositor(pids: &[u32]) -> Said {
    let mut client = match ede_protocol::Client::connect_env() {
        Ok(client) => client,
        Err(e) => {
            tracing::warn!("polkit: the compositor cannot be asked whose processes these are: {e:?}");
            return Said::default();
        }
    };
    if pids.is_empty() {
        return match client.space_of_process(std::process::id()) {
            Ok(answer) => Said { no_space: answer.no_space, ..Said::default() },
            Err(e) => {
                tracing::warn!("polkit: the compositor could not say whether an agent space exists: {e:?}");
                Said::default()
            }
        };
    }
    let (mut session, mut no_space) = (true, true);
    for pid in pids {
        match client.space_of_process(*pid) {
            Ok(ede_protocol::ProcessAnswer { space: Some(found), .. }) => {
                tracing::debug!("process {pid} is agent space {}'s ({:?} of process {})", number(found.space.as_str()), found.by, found.pid);
                return Said { space: Some(found.space.as_str().to_string()), ..Said::default() };
            }
            Ok(answer) => {
                session &= answer.session && !answer.unscoped_launches;
                no_space &= answer.no_space;
            }
            Err(e) => {
                tracing::warn!("polkit: the compositor could not say whose process {pid} is: {e:?}");
                return Said::default();
            }
        }
    }
    Said { space: None, session, no_space }
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
        agent_on_a_pair_with(Arc::new(|_| Asked::Person))
    }

    fn agent_on_a_pair_with(space_of: SpaceOf) -> (zbus::blocking::Connection, zbus::blocking::Connection, mpsc::Receiver<UiEvent>) {
        let (a, b) = UnixStream::pair().unwrap();
        let (to_ui, ui) = mpsc::channel();
        let server = std::thread::spawn(move || {
            zbus::blocking::connection::Builder::unix_stream(a).server(zbus::Guid::generate()).unwrap().p2p().serve_at(AGENT_PATH, Agent { to_ui, space_of }).unwrap().build().unwrap()
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

    /// polkitd's `BeginAuthentication` as it sends it, naming the subject's
    /// and the caller's process; its answer arrives on the receiver.
    fn begin(polkitd: &zbus::blocking::Connection, cookie: &str, subject: &str, caller: &str) -> mpsc::Receiver<Result<(), String>> {
        let (polkitd, cookie) = (polkitd.clone(), cookie.to_string());
        let details = HashMap::from([("polkit.subject-pid".to_string(), subject.to_string()), ("polkit.caller-pid".to_string(), caller.to_string())]);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let identity: HashMap<String, Value<'_>> = HashMap::from([("uid".to_string(), Value::from(unsafe { libc::getuid() }))]);
            let body = ("org.freedesktop.policykit.exec", "Authentication is needed", "", details, cookie.as_str(), vec![("unix-user", identity)]);
            let r = polkitd.call_method(None::<&str>, AGENT_PATH, Some("org.freedesktop.PolicyKit1.AuthenticationAgent"), "BeginAuthentication", &body);
            let _ = tx.send(r.map(|_| ()).map_err(|e| e.to_string()));
        });
        rx
    }

    /// #EDE-127: a request whose subject or caller is an agent space's
    /// process is refused at once and the prompt never hears of it; the
    /// person's request opens the prompt as before.
    #[test]
    fn a_request_from_an_agent_space_is_refused_before_any_prompt() {
        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = asked.clone();
        let space_of: SpaceOf = Arc::new(move |pids: &[u32]| {
            log.lock().unwrap().push(pids.to_vec());
            if pids.contains(&4242) {
                Asked::Space("space-3".into())
            } else if pids.contains(&6161) {
                Asked::Elsewhere
            } else {
                Asked::Person
            }
        });
        let (_agent, polkitd, ui) = agent_on_a_pair_with(space_of);
        // The space's process is the subject (pkexec's parent)...
        let refused = begin(&polkitd, "c-space", "4242", "4243").recv_timeout(Duration::from_secs(5)).expect("answered at once");
        assert!(refused.as_ref().is_err_and(|e| e.contains("refused: asked from agent space 3")), "{refused:?}");
        // ...or the caller (a space's process asking about another).
        let refused = begin(&polkitd, "c-caller", "5151", "4242").recv_timeout(Duration::from_secs(5)).expect("answered at once");
        assert!(refused.is_err());
        assert!(ui.recv_timeout(Duration::from_millis(300)).is_err(), "the prompt heard of a space's request");
        assert_eq!(*asked.lock().unwrap(), [vec![4242, 4243], vec![5151, 4242]], "both processes are asked about");
        // The person's own: the prompt opens; cancelled, it ends as before.
        let reply = begin(&polkitd, "c-person", "5151", "5152");
        let Ok(UiEvent::Begin(request, answer)) = ui.recv_timeout(Duration::from_secs(5)) else { panic!("the person's request opened no prompt") };
        assert_eq!((request.cookie.as_str(), request.asker.as_deref()), ("c-person", None), "the person's prompt names nobody and takes the keyboard");
        answer.send(Answer::Cancel).unwrap();
        assert!(reply.recv_timeout(Duration::from_secs(5)).unwrap().is_err());
        assert!(matches!(ui.recv_timeout(Duration::from_secs(5)), Ok(UiEvent::Done(c)) if c == "c-person"), "the prompt closes");
        // #EDE-143: from outside the person's session: the prompt opens,
        // naming the subject's process (this test's own here).
        let me = std::process::id().to_string();
        let reply = begin(&polkitd, "c-elsewhere", &me, "6161");
        let Ok(UiEvent::Begin(request, answer)) = ui.recv_timeout(Duration::from_secs(5)) else { panic!("a request from elsewhere opened no prompt") };
        let asker = request.asker.clone().expect("the prompt names who asks");
        assert!(asker.ends_with(&format!("(process {me})")), "{asker}");
        answer.send(Answer::Cancel).unwrap();
        assert!(reply.recv_timeout(Duration::from_secs(5)).unwrap().is_err());
    }

    /// The processes asked about: the subject's and the caller's, never
    /// init or garbage; no process named = nothing to ask, and nobody
    /// vouches for it.
    #[test]
    fn a_request_names_its_subjects_and_its_callers_process() {
        let details = |pairs: &[(&str, &str)]| pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<HashMap<_, _>>();
        let seen = std::sync::Mutex::new(Vec::new());
        // What the chain answers for no process while a space exists.
        let space_of = |pids: &[u32]| {
            seen.lock().unwrap().push(pids.to_vec());
            if pids.is_empty() { Asked::Elsewhere } else { Asked::Person }
        };
        assert_eq!(asked_from(&details(&[("polkit.subject-pid", "10"), ("polkit.caller-pid", "11"), ("other", "12")]), &space_of), Asked::Person);
        assert_eq!(asked_from(&details(&[("polkit.subject-pid", " 20\n"), ("polkit.caller-pid", "1")]), &space_of), Asked::Person);
        // #EDE-143: a request that names no process is asked about with none.
        assert_eq!(asked_from(&details(&[("polkit.subject-pid", "x"), ("polkit.caller-pid", "0")]), &space_of), Asked::Elsewhere);
        assert_eq!(asked_from(&details(&[]), &space_of), Asked::Elsewhere);
        assert_eq!(*seen.lock().unwrap(), [vec![10, 11], vec![20], vec![], vec![]]);
        assert_eq!(asked_from(&details(&[("polkit.caller-pid", "7")]), &|pids: &[u32]| if pids == [7] { Asked::Space("space-1".into()) } else { Asked::Person }), Asked::Space("space-1".into()));
        assert_eq!(named_pids(&details(&[("polkit.caller-pid", "7"), ("polkit.subject-pid", "8")])), [8, 7], "the subject's first: the one the prompt names");
    }

    /// The environment answers first (no round trip); the compositor is
    /// asked about the rest, and its answer stands.
    #[test]
    fn the_environment_answers_first_and_the_compositor_for_the_rest() {
        let spaces = std::env::temp_dir().join(format!("ede-sealed-{}.spaces", std::process::id()));
        let mut in_space = Command::new("sleep").arg("30").env("XDG_RUNTIME_DIR", spaces.join("space-5")).spawn().unwrap();
        let mut person = Command::new("sleep").arg("30").spawn().unwrap();
        let space = |name: &str| Said { space: Some(name.into()), ..Said::default() };
        let session = Said { session: true, ..Said::default() };
        assert_eq!(space_of_with(&[person.id(), in_space.id()], Some(&spaces), |_| panic!("the environment said it")), Asked::Space("space-5".into()));
        assert_eq!(space_of_with(&[person.id()], Some(&spaces), |pids| if pids == [person.id()] { space("space-9") } else { Said::default() }), Asked::Space("space-9".into()), "what the compositor knows");
        assert_eq!(space_of_with(&[person.id()], Some(&spaces), |_| session.clone()), Asked::Person);
        // #EDE-143: outside the session, or a compositor that cannot say.
        assert_eq!(space_of_with(&[person.id()], Some(&spaces), |_| Said::default()), Asked::Elsewhere);
        // With no agent space every request is the person's, as before
        // #EDE-143; also one that names no process.
        let no_space = Said { no_space: true, ..Said::default() };
        assert_eq!(space_of_with(&[person.id()], Some(&spaces), |_| no_space.clone()), Asked::Person);
        assert_eq!(space_of_with(&[], Some(&spaces), |pids| if pids.is_empty() { no_space.clone() } else { panic!("{pids:?}") }), Asked::Person);
        assert_eq!(space_of_with(&[], Some(&spaces), |_| Said::default()), Asked::Elsewhere, "a space exists: nothing vouches for it");
        // A process the environment ties to a space is refused whatever the compositor says.
        assert_eq!(space_of_with(&[in_space.id()], Some(&spaces), |_| no_space.clone()), Asked::Space("space-5".into()));
        assert_eq!(space_of_with(&[in_space.id()], None, |_| session.clone()), Asked::Person, "no spaces directory: the compositor alone");
        for child in [&mut in_space, &mut person] {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// #EDE-138: the cgroup answers before the environment and before the
    /// compositor. A process in a scope of a space's slice is that
    /// space's with nothing else left: an empty environment, no spaces
    /// directory to compare with, a compositor that is not asked.
    #[test]
    fn the_cgroup_answers_before_the_environment_and_the_compositor() {
        let mut in_space = Command::new("sleep").arg("30").env_clear().spawn().unwrap();
        let mut person = Command::new("sleep").arg("30").spawn().unwrap();
        let scoped = in_space.id();
        let cgroup_of = move |pid: u32| {
            if pid == scoped {
                return Some("0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-exponential_p41.slice/app-exponential_p41-s7.slice/exponential-p41-s7-2.scope\n".to_string());
            }
            ede_protocol::process::cgroup(pid)
        };
        assert_eq!(space_of_by(&[person.id(), scoped], &cgroup_of, None, |_| panic!("the cgroup said it")), Asked::Space("space-7".into()));
        assert_eq!(space_of_by(&[person.id()], &cgroup_of, None, |pids| Said { space: (pids == [person.id()]).then(|| "space-9".into()), ..Said::default() }), Asked::Space("space-9".into()), "the person's cgroup names no space: the other signs are asked");
        assert_eq!(space_of_by(&[person.id()], &cgroup_of, None, |_| Said { session: true, ..Said::default() }), Asked::Person);
        for child in [&mut in_space, &mut person] {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// #EDE-143: a process is named by its command line and its id; a long
    /// one is shortened, one that ended is said to have.
    #[test]
    fn the_prompt_names_a_process_by_its_command_line() {
        let mut child = Command::new("sleep").arg("30").spawn().unwrap();
        // Its arguments are there a moment after the exec.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::fs::read(format!("/proc/{}/cmdline", child.id())).map_or(true, |line| line.is_empty()) {
            assert!(std::time::Instant::now() < deadline, "no command line");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(describe(child.id()), format!("sleep 30 (process {})", child.id()));
        let _ = child.kill();
        let _ = child.wait();
        let long = Command::new("sh").args(["-c", "sleep 30; true", &"x".repeat(200)]).spawn();
        if let Ok(mut long) = long {
            std::thread::sleep(Duration::from_millis(100));
            let named = describe(long.id());
            assert!(named.chars().count() < 110 && named.contains('…'), "{named}");
            let _ = long.kill();
            let _ = long.wait();
        }
        assert_eq!(describe(u32::MAX - 1), format!("a process that ended (process {})", u32::MAX - 1));
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
