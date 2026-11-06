//! The [`Feed`]: every top-level path a document may bind with its JSON,
//! and the pure shaping that fills the shared ones from the compositor's
//! window list and from the shell reducer.

use std::collections::BTreeMap;

use serde_json::{json, Value};
use uicore_model::{alt_tab_order, start_menu_apps, start_menu_categories, taskbar_buttons, workspace_badges, ShellState, StartMenuApp};

/// The data every open surface sees: `path` (`/windows`) → value. A host
/// sets what changed and pushes the whole feed into its views.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Feed {
    entries: BTreeMap<String, Value>,
}

impl Feed {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets `path` (with its leading slash) to `value`; `true` when it changed.
    pub fn set(&mut self, path: &str, value: Value) -> bool {
        let path = if path.starts_with('/') { path.to_string() } else { format!("/{path}") };
        if self.entries.get(&path) == Some(&value) {
            return false;
        }
        self.entries.insert(path, value);
        true
    }

    pub fn get(&self, path: &str) -> Option<&Value> {
        self.entries.get(path)
    }

    /// Every path with its value, in path order.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// `/windows` as the taskbar binds it: the active workspace's windows in
/// the compositor's order (bottom to top), protected ones without their
/// title, each with the app it belongs to.
pub fn windows_feed(windows: &[ede_protocol::WindowInfo], active_workspace: usize) -> Vec<Value> {
    windows
        .iter()
        .filter(|w| !w.protected && w.workspace == active_workspace)
        .map(|w| {
            let app = w.app_id.clone().unwrap_or_default();
            let title = w.title.clone().unwrap_or_else(|| if app.is_empty() { "Window".into() } else { app.clone() });
            json!({"id": w.id, "title": title, "app": app, "focused": w.focused, "workspace": w.workspace, "minimized": w.minimized})
        })
        .collect()
}

/// `/shell`: the reducer's state as the documents read it, with the
/// selectors resolved (the taskbar's buttons, the workspace badges, the
/// start menu's categories and apps, the Alt-Tab order).
pub fn shell_feed(state: &ShellState, apps: &[StartMenuApp], favourites: &[String]) -> Value {
    json!({
        "rail": state.rail,
        "sidebar": state.sidebar,
        "switcher": state.switcher,
        "workspaces": {
            "active": state.workspaces.active,
            "count": state.workspaces.count,
            "badges": workspace_badges(state),
        },
        "taskbar": {
            "pinned": state.taskbar.pinned,
            "hover": state.taskbar.hover,
            "previews": state.taskbar.previews,
            "menu": state.taskbar.menu,
            "buttons": taskbar_buttons(state),
        },
        "startMenu": {
            "open": state.start_menu.open,
            "query": state.start_menu.query,
            "category": state.start_menu.category,
            "categories": start_menu_categories(apps, favourites),
            "apps": start_menu_apps(state, apps, favourites),
        },
        "chat": state.chat,
        "altTab": {
            "open": state.alt_tab.open,
            "selected": state.alt_tab.selected,
            "order": alt_tab_order(state),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ede_protocol::{WindowId, WindowInfo};

    fn window(id: &str, app: Option<&str>, workspace: usize, focused: bool) -> WindowInfo {
        WindowInfo { id: WindowId::new(id).unwrap(), title: Some(format!("{id} title")), app_id: app.map(String::from), x: 0, y: 0, width: 100, height: 100, focused, fullscreen: false, protected: false, x11: false, workspace, minimized: false }
    }

    #[test]
    fn the_feed_reports_what_changed() {
        let mut f = Feed::new();
        assert!(f.set("time", json!("12:00")));
        assert!(!f.set("/time", json!("12:00")));
        assert!(f.set("/time", json!("12:01")));
        assert_eq!(f.get("/time"), Some(&json!("12:01")));
        assert_eq!(f.len(), 1);
    }

    #[test]
    fn windows_feed_keeps_the_active_workspace_in_order_with_the_app() {
        let list = vec![window("w1", Some("firefox"), 0, false), window("w2", None, 1, false), window("w3", Some("org.gnome.Nautilus"), 0, true)];
        let feed = windows_feed(&list, 0);
        assert_eq!(feed.len(), 2);
        assert_eq!(feed[0]["app"], "firefox");
        assert_eq!(feed[1]["id"], "w3");
        assert_eq!(feed[1]["focused"], true);
        assert_eq!(feed[1]["minimized"], false);
    }

    #[test]
    fn shell_feed_resolves_the_selectors() {
        let mut state = ShellState::default();
        state.taskbar.pinned.push("firefox".into());
        let apps = vec![StartMenuApp { id: "firefox".into(), name: "Firefox".into(), comment: "Browse".into(), categories: vec!["Network".into()], keywords: String::new() }];
        let v = shell_feed(&state, &apps, &["firefox".into()]);
        assert_eq!(v["taskbar"]["buttons"][0]["app"], "firefox");
        assert_eq!(v["taskbar"]["buttons"][0]["running"], false);
        assert_eq!(v["workspaces"]["badges"][0]["active"], true);
        assert_eq!(v["startMenu"]["categories"][0]["id"], "all");
        assert_eq!(v["startMenu"]["apps"][0]["category"], "internet");
    }
}
