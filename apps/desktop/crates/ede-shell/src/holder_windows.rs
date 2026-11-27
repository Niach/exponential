//! #EDE-237: what the Exponential app's windows are. In eDE mode the
//! holder hosts every page as a window of its own and tells the shell what
//! each one is over the session bus ([`api::bus::Request::Windows`], a long
//! poll): its kind, concept icon, title and team. The taskbar's grouped
//! Exponential button lists them as Cinnamon's thumbnails, each with its
//! page's icon and title.
//!
//! A toplevel is matched to its page by app id and title: the holder titles
//! each window after its page.

use std::{sync::mpsc, time::Duration};

use api::bus::{Client, Reply, Request, WindowMeta};

/// The holder's windows as last told.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HolderWindows {
    /// The app id the holder's toplevels carry.
    pub app_id: String,
    pub windows: Vec<WindowMeta>,
}

impl HolderWindows {
    /// The page a toplevel of `app` titled `title` holds.
    pub fn page(&self, app: &str, title: &str) -> Option<&WindowMeta> {
        if self.app_id.is_empty() || app != self.app_id {
            return None;
        }
        self.windows.iter().find(|meta| meta.title == title)
    }
}

/// How long the shell waits before asking a holder that is not there (or
/// went away) again.
const RETRY: Duration = Duration::from_secs(2);

/// How long it waits on a holder that does not know the op (an older one).
const RETRY_UNSUPPORTED: Duration = Duration::from_secs(60);

/// Follows the holder's windows on a thread of its own; every change is
/// sent on `tx` (an empty list while no holder answers).
pub fn follow(tx: mpsc::Sender<HolderWindows>) {
    let spawned = std::thread::Builder::new().name("ede-shell-holder-windows".into()).spawn(move || {
        let path = api::bus::socket_path(&api::default_data_dir());
        let mut last = HolderWindows::default();
        loop {
            let pause = match Client::connect(&path, api::bus::WINDOWS_TIMEOUT) {
                Ok(mut client) => {
                    let mut after = None;
                    loop {
                        match client.call(&Request::Windows { after }) {
                            Ok(Reply::Windows { generation, app_id, windows }) => {
                                after = Some(generation);
                                let now = HolderWindows { app_id, windows };
                                if now != last {
                                    last = now.clone();
                                    if tx.send(now).is_err() {
                                        return;
                                    }
                                }
                            }
                            Ok(_) => break RETRY_UNSUPPORTED,
                            Err(_) => break RETRY,
                        }
                    }
                }
                Err(_) => RETRY,
            };
            if last != HolderWindows::default() {
                last = HolderWindows::default();
                if tx.send(last.clone()).is_err() {
                    return;
                }
            }
            std::thread::sleep(pause);
        }
    });
    if let Err(e) = spawned {
        tracing::warn!("holder windows: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_toplevel_is_its_page_by_app_id_and_title() {
        let meta = |title: &str, icon: &str| WindowMeta { key: format!("k:{title}"), kind: "issue".into(), icon: icon.into(), title: title.into(), team: None, team_id: None };
        let holder = HolderWindows { app_id: "at.exponential".into(), windows: vec![meta("EXP-1 Crash", "ui-issue"), meta("Desktop", "nav-boards")] };
        assert_eq!(holder.page("at.exponential", "Desktop").map(|m| m.icon.as_str()), Some("nav-boards"));
        assert_eq!(holder.page("firefox", "Desktop"), None, "another app's window");
        assert_eq!(holder.page("at.exponential", "Exponential"), None, "no page by that title");
        assert_eq!(HolderWindows::default().page("", "Desktop"), None);
    }
}
