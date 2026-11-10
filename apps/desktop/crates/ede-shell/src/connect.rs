//! Connect Exponential (#EDE-56): the IDE's sign-in, from the shell. The
//! browser flow (PKCE: `mobile-oauth-start` → the browser → the
//! `exponential://oauth-return?code=…` callback) when THIS shell is the
//! registered handler of the scheme (it registers itself only when nobody
//! else, the IDE above all, claims it: `exponential-shell connect <url>` is
//! the handler's command, forwarding the URL over a socket to the running
//! shell), else the device-code flow (RFC 8628: a code and a page; the
//! shell polls). Either way the account lands in the shared `accounts.json`
//! through `api::AuthStore::sign_in`, so the IDE sees it too.

use std::os::unix::net::UnixDatagram;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use api::accounts::Account;
use api::login::{AuthClient, DevicePoll, OAuthCallback, OAUTH_CALLBACK_SCHEME};
use api::AuthStore;
use serde_json::{json, Value};

const HANDLER_DESKTOP: &str = "at.exponential.ede-connect.desktop";
const SOCKET_NAME: &str = "exponential-shell-url.sock";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    Browser,
    Code { code: String, uri: String, complete: Option<String> },
    Done { email: String },
    Failed { error: String },
}

/// What the background threads report.
pub enum Event {
    Code { code: String, uri: String, complete: Option<String> },
    Signed(Account),
    Failed(String),
}

pub struct Connect {
    pub state: State,
    pub instance: String,
    auth: Arc<AuthStore>,
    client: Arc<AuthClient>,
    tx: mpsc::Sender<Event>,
    verifier: Option<String>,
    cancel: Arc<AtomicBool>,
}

impl Connect {
    pub fn new(auth: Arc<AuthStore>, tx: mpsc::Sender<Event>) -> Self {
        Self { state: State::Idle, instance: String::new(), auth, client: Arc::new(AuthClient::new()), tx, verifier: None, cancel: Arc::new(AtomicBool::new(false)) }
    }

    /// `/connecting`.
    pub fn feed(&self) -> Value {
        let (state, code, uri, email, error) = match &self.state {
            State::Idle => ("idle", "", "", "", ""),
            State::Browser => ("browser", "", "", "", ""),
            State::Code { code, uri, .. } => ("code", code.as_str(), uri.as_str(), "", ""),
            State::Done { email } => ("done", "", "", email.as_str(), ""),
            State::Failed { error } => ("failed", "", "", "", error.as_str()),
        };
        json!({"state": state, "instance": self.instance, "code": code, "uri": uri, "email": email, "error": error})
    }

    fn begin(&mut self, instance: &str) {
        self.cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.instance = api::login::normalize_instance_url(if instance.trim().is_empty() { "https://app.exponential.at" } else { instance.trim() });
    }

    /// The browser flow when the shell is the scheme's handler, else the code.
    pub fn start(&mut self, instance: &str) {
        self.begin(instance);
        if !ensure_handler() {
            tracing::info!("another app handles exponential:// links: the code flow instead");
            return self.start_code(&self.instance.clone());
        }
        let pkce = api::login::generate_pkce();
        let url = api::login::browser_login_start_url(&self.instance, &pkce.challenge);
        self.verifier = Some(pkce.verifier);
        open_url(&url);
        self.state = State::Browser;
    }

    /// The device-code flow: the code arrives as an event, then the poll.
    pub fn start_code(&mut self, instance: &str) {
        self.begin(instance);
        self.verifier = None;
        self.state = State::Browser;
        let (client, auth, tx, cancel, instance) = (self.client.clone(), self.auth.clone(), self.tx.clone(), self.cancel.clone(), self.instance.clone());
        std::thread::Builder::new()
            .name("ede-shell-connect".into())
            .spawn(move || {
                let grant = match client.request_device_code(&instance) {
                    Ok(grant) => grant,
                    Err(e) => {
                        let _ = tx.send(Event::Failed(e.user_message()));
                        return;
                    }
                };
                let _ = tx.send(Event::Code { code: grant.user_code.clone(), uri: grant.verification_uri.clone(), complete: grant.verification_uri_complete.clone() });
                let mut interval = Duration::from_secs(grant.interval.max(1));
                let deadline = std::time::Instant::now() + Duration::from_secs(grant.expires_in.max(30));
                loop {
                    std::thread::sleep(interval);
                    if cancel.load(Ordering::Relaxed) {
                        return;
                    }
                    if std::time::Instant::now() > deadline {
                        let _ = tx.send(Event::Failed("The code expired. Try again.".into()));
                        return;
                    }
                    match client.poll_device_token(&instance, &grant.device_code) {
                        Ok(DevicePoll::Authorized { token }) => {
                            let _ = tx.send(sign_in(&client, &auth, &instance, &token));
                            return;
                        }
                        Ok(DevicePoll::Pending) => {}
                        Ok(DevicePoll::SlowDown) => interval += Duration::from_secs(5),
                        Ok(DevicePoll::Expired) => {
                            let _ = tx.send(Event::Failed("The code expired. Try again.".into()));
                            return;
                        }
                        Ok(DevicePoll::Denied) => {
                            let _ = tx.send(Event::Failed("The sign-in was refused.".into()));
                            return;
                        }
                        Err(e) => {
                            let _ = tx.send(Event::Failed(e.user_message()));
                            return;
                        }
                    }
                }
            })
            .expect("the connect thread");
    }

    /// The code page, in the browser.
    pub fn open_page(&self) {
        if let State::Code { uri, complete, .. } = &self.state {
            open_url(complete.as_deref().unwrap_or(uri));
        }
    }

    pub fn cancel(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.verifier = None;
        self.state = State::Idle;
    }

    /// An `exponential://` URL the handler forwarded: the browser flow's end.
    pub fn callback(&mut self, url: &str) {
        let Some(callback) = api::login::parse_oauth_callback(url) else {
            tracing::info!(url, "not an OAuth callback");
            return;
        };
        let Some(verifier) = self.verifier.take() else {
            tracing::info!("an OAuth callback with no sign-in pending");
            return;
        };
        let (client, auth, tx, instance) = (self.client.clone(), self.auth.clone(), self.tx.clone(), self.instance.clone());
        std::thread::Builder::new()
            .name("ede-shell-oauth".into())
            .spawn(move || {
                let token = match callback {
                    OAuthCallback::Token(token) => Ok(token),
                    OAuthCallback::Code(code) => client.exchange_oauth_code(&instance, &code, &verifier),
                    OAuthCallback::Error(reason) => Err(api::ApiError::Decode(reason)),
                    OAuthCallback::Linked(_) => Err(api::ApiError::Decode("not a sign-in".into())),
                };
                let event = match token {
                    Ok(token) => sign_in(&client, &auth, &instance, &token),
                    Err(e) => Event::Failed(e.user_message()),
                };
                let _ = tx.send(event);
            })
            .expect("the oauth thread");
    }

    /// A background event into the state; `Some(account)` when one signed in.
    pub fn apply(&mut self, event: Event) -> Option<Account> {
        match event {
            Event::Code { code, uri, complete } => {
                if matches!(self.state, State::Browser | State::Code { .. }) {
                    self.state = State::Code { code, uri, complete };
                }
                None
            }
            Event::Signed(account) => {
                self.state = State::Done { email: account.email.clone() };
                Some(account)
            }
            Event::Failed(error) => {
                self.state = State::Failed { error };
                None
            }
        }
    }
}

fn sign_in(client: &AuthClient, auth: &AuthStore, instance: &str, token: &str) -> Event {
    match client.fetch_session(instance, token) {
        Ok(Some(user)) => match auth.sign_in(instance, token, &user) {
            Ok(account) => Event::Signed(account),
            Err(e) => Event::Failed(format!("Could not store the session: {}", e.user_message())),
        },
        Ok(None) => Event::Failed("The sign-in did not stick. Try again.".into()),
        Err(e) => Event::Failed(e.user_message()),
    }
}

pub(crate) fn open_url(url: &str) {
    match std::process::Command::new("xdg-open").arg(url).spawn() {
        Ok(_) => tracing::info!(url, "opened in the browser"),
        Err(e) => tracing::warn!(url, "xdg-open: {e}"),
    }
}

fn socket_path() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()).map(|dir| PathBuf::from(dir).join(SOCKET_NAME))
}

/// `exponential-shell connect <url>`: hands the URL to the running shell.
pub fn forward_url(url: &str) -> bool {
    let Some(path) = socket_path() else { return false };
    let Ok(sock) = UnixDatagram::unbound() else { return false };
    match sock.send_to(url.as_bytes(), &path) {
        Ok(_) => true,
        Err(e) => {
            eprintln!("exponential-shell: no running shell to hand {url} to: {e}");
            false
        }
    }
}

/// The running shell's end: every URL the handler forwards.
pub fn listen_urls(tx: mpsc::Sender<String>) {
    let Some(path) = socket_path() else { return };
    let _ = std::fs::remove_file(&path);
    let sock = match UnixDatagram::bind(&path) {
        Ok(sock) => sock,
        Err(e) => {
            tracing::warn!(path = %path.display(), "url socket: {e}");
            return;
        }
    };
    std::thread::Builder::new()
        .name("ede-shell-urls".into())
        .spawn(move || {
            let mut buf = vec![0u8; 8192];
            while let Ok(n) = sock.recv(&mut buf) {
                let url = String::from_utf8_lossy(&buf[..n]).trim().to_string();
                if !url.is_empty() && tx.send(url).is_err() {
                    return;
                }
            }
        })
        .expect("the url thread");
}

fn applications_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let data_home = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local").join("share"));
    dirs.push(data_home.join("applications"));
    let data_dirs = std::env::var_os("XDG_DATA_DIRS").filter(|v| !v.is_empty()).map(|v| std::env::split_paths(&v).collect::<Vec<_>>()).unwrap_or_else(|| vec![PathBuf::from("/usr/local/share"), PathBuf::from("/usr/share")]);
    dirs.extend(data_dirs.into_iter().map(|d| d.join("applications")));
    dirs
}

fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"))
}

/// Another app's desktop entry claiming the scheme (the IDE's, above all).
fn other_handler() -> Option<PathBuf> {
    let needle = format!("x-scheme-handler/{OAUTH_CALLBACK_SCHEME}");
    for dir in applications_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.file_name().and_then(|n| n.to_str()) == Some(HANDLER_DESKTOP) || path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            if std::fs::read_to_string(&path).is_ok_and(|text| text.lines().any(|l| l.starts_with("MimeType=") && l.contains(&needle))) {
                return Some(path);
            }
        }
    }
    None
}

/// The shell as the scheme's handler, unless another app is one already.
/// Returns whether the browser's callback will reach this shell.
pub fn ensure_handler() -> bool {
    if let Some(other) = other_handler() {
        tracing::info!(handler = %other.display(), "exponential:// has a handler");
        return false;
    }
    let Ok(exe) = std::env::current_exe() else { return false };
    let dir = applications_dirs().into_iter().next().unwrap_or_default();
    let contents = format!(
        "[Desktop Entry]\nType=Application\nName=Exponential Desktop\nComment=Connect Exponential (the sign-in's return)\nExec={} connect %u\nNoDisplay=true\nTerminal=false\nMimeType=x-scheme-handler/{OAUTH_CALLBACK_SCHEME};\n",
        exe.display()
    );
    let path = dir.join(HANDLER_DESKTOP);
    if std::fs::read_to_string(&path).ok().as_deref() != Some(contents.as_str()) {
        if let Err(e) = std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&path, &contents)) {
            tracing::warn!(path = %path.display(), "handler: {e}");
            return false;
        }
        let _ = std::process::Command::new("update-desktop-database").arg(&dir).status();
    }
    set_default_handler().is_ok()
}

/// `mimeapps.list`: the scheme's default = the shell's entry (in place, so
/// it works without xdg-utils; other lines kept).
fn set_default_handler() -> std::io::Result<()> {
    let path = config_home().join("mimeapps.list");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let key = format!("x-scheme-handler/{OAUTH_CALLBACK_SCHEME}");
    let line = format!("{key}={HANDLER_DESKTOP}");
    let mut out = String::new();
    let mut in_defaults = false;
    let mut written = false;
    let mut had_section = false;
    for raw in existing.lines() {
        let trimmed = raw.trim();
        if trimmed.starts_with('[') {
            if in_defaults && !written {
                out.push_str(&line);
                out.push('\n');
                written = true;
            }
            in_defaults = trimmed == "[Default Applications]";
            had_section |= in_defaults;
        } else if in_defaults && trimmed.starts_with(&format!("{key}=")) {
            if !written {
                out.push_str(&line);
                out.push('\n');
                written = true;
            }
            continue;
        }
        out.push_str(raw);
        out.push('\n');
    }
    if !written {
        if !had_section {
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("[Default Applications]\n");
        }
        out.push_str(&line);
        out.push('\n');
    }
    if out != existing {
        std::fs::create_dir_all(config_home())?;
        std::fs::write(&path, out)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_feed_follows_the_state() {
        let (tx, _rx) = mpsc::channel();
        let auth = AuthStore::load(tempfile::tempdir().unwrap().path().to_path_buf());
        let mut connect = Connect::new(auth, tx);
        assert_eq!(connect.feed()["state"], "idle");
        connect.state = State::Code { code: "ABCD-EFGH".into(), uri: "https://x/device".into(), complete: None };
        assert_eq!(connect.feed()["code"], "ABCD-EFGH");
        connect.apply(Event::Failed("nope".into()));
        assert_eq!(connect.feed()["state"], "failed");
        assert_eq!(connect.feed()["error"], "nope");
        connect.cancel();
        assert_eq!(connect.state, State::Idle);
    }
}
