//! Share a Local desktop as a team (#EDE-57): the General page's flow over
//! elocal's uploader. The host feeds `/sharing` (the state the page shows);
//! the upload runs on a thread and reports over a channel; when it is done
//! the desktop is archived (its file stays 30 days) and the team takes its
//! place in the switcher once the account's teams shape carries it.

use std::sync::{mpsc, Arc};

use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Idle,
    NeedsAccount,
    Picking,
    Running { progress: u32, label: String },
    Done { team_name: String },
    Failed { error: String },
}

pub enum Event {
    Progress { progress: u32, label: String },
    Done(elocal::share::Shared),
    Failed(String),
}

pub struct Share {
    pub state: State,
    /// The desktop being shared and the account it goes to.
    pub desktop_id: Option<String>,
    pub account_id: Option<String>,
    pub accounts: Vec<(String, String)>,
    pub summary: String,
    tx: mpsc::Sender<Event>,
}

impl Share {
    pub fn new(tx: mpsc::Sender<Event>) -> Self {
        Self { state: State::Idle, desktop_id: None, account_id: None, accounts: Vec::new(), summary: String::new(), tx }
    }

    /// `/sharing`.
    pub fn feed(&self) -> Value {
        let accounts: Vec<Value> = self.accounts.iter().map(|(id, email)| json!({"label": email, "value": id})).collect();
        let (state, progress, label, team_name, error) = match &self.state {
            State::Idle => ("idle", 0, String::new(), String::new(), String::new()),
            State::NeedsAccount => ("needsAccount", 0, String::new(), String::new(), String::new()),
            State::Picking => ("picking", 0, String::new(), String::new(), String::new()),
            State::Running { progress, label } => ("running", *progress, label.clone(), String::new(), String::new()),
            State::Done { team_name } => ("done", 100, String::new(), team_name.clone(), String::new()),
            State::Failed { error } => ("failed", 0, String::new(), String::new(), error.clone()),
        };
        json!({"state": state, "accounts": accounts, "summary": self.summary, "progress": progress, "label": label, "teamName": team_name, "error": error})
    }

    /// Share as a team…: the accounts to pick from, or the nudge to connect.
    pub fn start(&mut self, desktop_id: &str, accounts: Vec<(String, String)>, summary: String) {
        self.desktop_id = Some(desktop_id.to_string());
        self.accounts = accounts;
        self.summary = summary;
        self.state = if self.accounts.is_empty() { State::NeedsAccount } else { State::Picking };
    }

    pub fn cancel(&mut self) {
        self.state = State::Idle;
    }

    /// The upload, on a thread; `client` talks to the picked account.
    pub fn run(&mut self, client: api::TrpcClient, desktop: Arc<elocal::LocalDesktop>, name: String, device_id: Option<String>) {
        self.state = State::Running { progress: 0, label: "Preparing".into() };
        let tx = self.tx.clone();
        std::thread::Builder::new()
            .name("ede-shell-share".into())
            .spawn(move || {
                let mut report = |p: elocal::share::Progress| {
                    let (progress, label) = match p {
                        elocal::share::Progress::Created { .. } => (5, "Uploading".to_string()),
                        elocal::share::Progress::Chunk { sent, total } => (5 + (60 * sent / total.max(1)) as u32, format!("Uploading {sent} of {total}")),
                        elocal::share::Progress::Attachment { sent, total } => (65 + (25 * sent / total.max(1)) as u32, format!("Uploading {sent} of {total} files")),
                        elocal::share::Progress::Committing => (92, "Making the team".to_string()),
                    };
                    let _ = tx.send(Event::Progress { progress, label });
                };
                let event = match elocal::share::run(&client, &desktop, &name, device_id.as_deref(), &mut report) {
                    Ok(shared) => Event::Done(shared),
                    Err(e) => Event::Failed(e.user_message()),
                };
                let _ = tx.send(event);
            })
            .expect("the share thread");
    }

    /// A thread's report into the state; the team when the share is done.
    pub fn apply(&mut self, event: Event) -> Option<elocal::share::Shared> {
        match event {
            Event::Progress { progress, label } => {
                if matches!(self.state, State::Running { .. }) {
                    self.state = State::Running { progress, label };
                }
                None
            }
            Event::Done(shared) => {
                self.state = State::Done { team_name: shared.name.clone() };
                Some(shared)
            }
            Event::Failed(error) => {
                self.state = State::Failed { error };
                None
            }
        }
    }
}

/// `12 issues, 31 comments, 4 sessions and 5 files (2.1 MB)`.
pub fn summary(desktop: &elocal::LocalDesktop) -> String {
    let count = |table: &str| desktop.store.rows(table).map(|r| r.len()).unwrap_or(0);
    let plural = |n: usize, one: &str, many: &str| if n == 1 { format!("{n} {one}") } else { format!("{n} {many}") };
    let bytes: u64 = desktop.store.rows("attachments").unwrap_or_default().iter().filter_map(|a| a.get("size_bytes").and_then(Value::as_u64)).sum();
    let size = if bytes >= 1_048_576 { format!(" ({:.1} MB)", bytes as f64 / 1_048_576.0) } else if bytes > 0 { format!(" ({} KB)", bytes.div_ceil(1024)) } else { String::new() };
    format!("{}, {}, {} and {}{size}", plural(count("issues"), "issue", "issues"), plural(count("comments"), "comment", "comments"), plural(count("sessions"), "session", "sessions"), plural(count("attachments"), "file", "files"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_feed_follows_the_state() {
        let (tx, _rx) = mpsc::channel();
        let mut share = Share::new(tx);
        assert_eq!(share.feed()["state"], "idle");
        share.start("d1", vec![], "nothing".into());
        assert_eq!(share.feed()["state"], "needsAccount");
        share.start("d1", vec![("a".into(), "a@x.com".into())], "1 issue".into());
        assert_eq!(share.feed()["state"], "picking");
        assert_eq!(share.feed()["accounts"][0]["label"], "a@x.com");
        share.state = State::Running { progress: 0, label: String::new() };
        share.apply(Event::Progress { progress: 40, label: "Uploading 2 of 5".into() });
        assert_eq!(share.feed()["progress"], 40);
        let done = share.apply(Event::Done(elocal::share::Shared { team_id: "t".into(), slug: "s".into(), name: "Team".into(), remapped: 0 }));
        assert!(done.is_some());
        assert_eq!(share.feed()["teamName"], "Team");
        share.apply(Event::Failed("nope".into()));
        assert_eq!(share.feed()["error"], "nope");
    }

    #[test]
    fn the_summary_counts() {
        let dir = tempfile::tempdir().unwrap();
        let desktops = elocal::Desktops::open(dir.path()).unwrap();
        let desktop = desktops.open_desktop(desktops.current().unwrap()).unwrap();
        assert_eq!(summary(&desktop), "0 issues, 0 comments, 0 sessions and 0 files");
    }
}
