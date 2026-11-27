//! #EDE-92, #EDE-258: the start menu's keyboard (`apps/ede/ui/start-menu.express`),
//! Cinnamon's. The menu is `/shell/startMenu/nav` (uicore shell.json
//! `startMenuNav`): the categories and the rows of the selected one, or the
//! ranked results while the search has text.
//!
//! - **No text:** one column has the keyboard, the categories (where the
//!   menu opens) or the contents. Up and Down in the categories switch the
//!   category at once (as hovering does); in the contents they move the
//!   selected row, skipping rows that open nothing (a folder) and stopping
//!   at the ends. Right enters the contents on their first row, Left goes
//!   back. Enter on a category opens its page (Home, Chats, the
//!   Assistant…) or, with none (a team, Devices), enters its contents;
//!   Enter on a row opens it.
//! - **Text:** one result row is selected, the first by default and again
//!   after every change of the text; Up and Down move it and stop at the
//!   ends; Enter opens it. Left and Right stay the box's (its caret).
//!
//! Every other key (the text, Tab, Escape) is the focused component's.

use serde_json::{json, Value};

/// The column the keyboard is in while the box is empty.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Column {
    #[default]
    Categories,
    Contents,
}

/// The keyboard's place in the menu.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    /// The text the place belongs to (a changed text = the first result).
    query: String,
    column: Column,
    /// The selected row among the SELECTABLE rows (those with a target).
    index: usize,
}

/// A key the menu takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
}

/// What a key did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Moved {
    /// Switch to this category (the shell dispatches it).
    Category(String),
    /// The selected row moved (or the column changed): redraw, reveal it.
    Row(Option<usize>),
}

/// What Enter opens.
#[derive(Debug, Clone, PartialEq)]
pub enum Activation {
    /// A row's `target` (`{page}`, `{app}`, `{settings}`, `{issue, …}`, `{local, board}`).
    Target(Value),
    /// A category: its page, if it has one (the shell knows its key).
    Category(String),
}

fn query_of(nav: &Value, query: &str) -> bool {
    let _ = nav;
    !query.trim().is_empty()
}

fn rows(nav: &Value) -> &[Value] {
    nav["rows"].as_array().map_or(&[], Vec::as_slice)
}

fn selectable(nav: &Value) -> Vec<usize> {
    rows(nav).iter().enumerate().filter(|(_, r)| r["target"].is_object()).map(|(i, _)| i).collect()
}

fn categories(nav: &Value) -> Vec<String> {
    nav["categories"].as_array().into_iter().flatten().filter_map(|c| c["id"].as_str().map(String::from)).collect()
}

impl Selection {
    /// The selected row of `nav` (an index into its rows) for the text
    /// `query`: with text the first selectable by default; without, only in
    /// the contents column.
    pub fn row(&self, nav: &Value, query: &str) -> Option<usize> {
        let picks = selectable(nav);
        if picks.is_empty() {
            return None;
        }
        let searching = query_of(nav, query);
        if !searching && self.column == Column::Categories {
            return None;
        }
        let index = if searching && self.query != query { 0 } else { self.index.min(picks.len() - 1) };
        Some(picks[index])
    }

    pub fn column(&self) -> Column {
        self.column
    }

    /// A key pressed with `query` in the box over `nav`; `None` = not the
    /// menu's key (it goes on to the focused component).
    pub fn key(&mut self, nav: &Value, query: &str, key: Key) -> Option<Moved> {
        let picks = selectable(nav).len();
        if query_of(nav, query) {
            let step = match key {
                Key::Up => -1,
                Key::Down => 1,
                Key::Left | Key::Right => return None,
            };
            if picks == 0 {
                return None;
            }
            let at = if self.query == query { self.index.min(picks - 1) } else { 0 } as i64;
            *self = Selection { query: query.to_string(), column: Column::Contents, index: (at + step).clamp(0, picks as i64 - 1) as usize };
            return Some(Moved::Row(self.row(nav, query)));
        }
        self.query.clear();
        match (self.column, key) {
            (Column::Categories, Key::Up | Key::Down) => {
                let ids = categories(nav);
                let current = nav["category"].as_str().unwrap_or_default();
                let at = ids.iter().position(|id| id == current)? as i64;
                let next = (at + if key == Key::Up { -1 } else { 1 }).clamp(0, ids.len() as i64 - 1) as usize;
                (next as i64 != at).then(|| Moved::Category(ids[next].clone())).or(Some(Moved::Row(None)))
            }
            (Column::Categories, Key::Right) => {
                if picks == 0 {
                    return Some(Moved::Row(None));
                }
                self.column = Column::Contents;
                self.index = 0;
                Some(Moved::Row(self.row(nav, query)))
            }
            (Column::Categories, Key::Left) => Some(Moved::Row(None)),
            (Column::Contents, Key::Left) => {
                self.column = Column::Categories;
                Some(Moved::Row(None))
            }
            (Column::Contents, Key::Right) => Some(Moved::Row(self.row(nav, query))),
            (Column::Contents, Key::Up | Key::Down) => {
                if picks == 0 {
                    return Some(Moved::Row(None));
                }
                let at = self.index.min(picks - 1) as i64;
                self.index = (at + if key == Key::Up { -1 } else { 1 }).clamp(0, picks as i64 - 1) as usize;
                Some(Moved::Row(self.row(nav, query)))
            }
        }
    }

    /// Enter in the box.
    pub fn activation(&self, nav: &Value, query: &str) -> Option<Activation> {
        if let Some(row) = self.row(nav, query) {
            return Some(Activation::Target(rows(nav)[row]["target"].clone()));
        }
        if query_of(nav, query) {
            return None;
        }
        nav["category"].as_str().filter(|c| !c.is_empty()).map(|c| Activation::Category(c.to_string()))
    }

    /// Enter on a category without a page: the keyboard goes into its
    /// contents (their first row).
    pub fn enter_contents(&mut self, nav: &Value) -> bool {
        if selectable(nav).is_empty() {
            return false;
        }
        *self = Selection { query: String::new(), column: Column::Contents, index: 0 };
        true
    }

    /// After the reducer ran `event` on a menu whose text was `before`: the
    /// menu opened, closed or its text really changed = back to the start
    /// (the first result, the categories column); a new category keeps the
    /// column, on its first row. Enter's flush of the same text changes
    /// nothing.
    pub fn follow(&mut self, event: &uicore_model::ShellEvent, before: &str, open: bool) {
        use uicore_model::ShellEvent::*;
        let changed = matches!(event, StartMenuQuery { query } if query != before);
        if !open || changed || matches!(event, StartMenuOpen | StartMenuClose | StartMenuToggle) {
            *self = Selection::default();
        } else if matches!(event, StartMenuCategory { .. }) {
            self.index = 0;
        }
    }
}

/// Every row of `nav` says whether it is the selected one, every category
/// whether it has the keyboard's focus (the document binds `selected`).
pub fn mark(nav: &mut Value, selection: &Selection, query: &str) {
    let at = selection.row(nav, query);
    for (i, row) in nav["rows"].as_array_mut().into_iter().flatten().enumerate() {
        row["selected"] = json!(at == Some(i));
    }
}

/// The row with this id (a press on it).
pub fn row_target(nav: &Value, id: &str) -> Option<Value> {
    rows(nav).iter().find(|r| r["id"] == id).map(|r| r["target"].clone()).filter(Value::is_object)
}

/// The menu's key among a keystroke: arrows without a modifier.
pub fn key_of(key: &str, modified: bool) -> Option<Key> {
    match key {
        _ if modified => None,
        "down" => Some(Key::Down),
        "up" => Some(Key::Up),
        "left" => Some(Key::Left),
        "right" => Some(Key::Right),
        _ => None,
    }
}

/// #EDE-258: the Settings modules the search finds (settings.express's
/// device and personal pages: id, title, icon, keywords).
const SETTINGS_MODULES: &[(&str, &str, &str, &str)] = &[
    ("display", "Display", "ede-brightness", "screen monitor brightness resolution scale"),
    ("sound", "Sound", "media-volume-up", "audio volume speaker microphone"),
    ("network", "Network", "ede-wifi", "wifi wireless ethernet internet vpn"),
    ("bluetooth", "Bluetooth", "ede-bluetooth", "devices pair headphones"),
    ("power", "Power", "ede-battery", "battery suspend sleep"),
    ("themes", "Themes", "editor-text-format", "appearance dark light colours fonts"),
    ("keyboard", "Keyboard", "coding-command", "layout shortcuts keys"),
    ("mouse", "Mouse & Touchpad", "ui-device", "pointer trackpad scroll"),
    ("datetime", "Date & Time", "ui-clock", "clock timezone"),
    ("users", "Users", "settings-members", "accounts people"),
    ("notifications", "Notifications", "settings-notifications", "alerts do not disturb"),
    ("workspaces", "Workspaces", "ede-workspaces", "desktops virtual"),
    ("windows", "Windows", "ede-window", "tiling titlebar focus"),
    ("startup", "Startup apps", "action-run", "autostart login"),
    ("defaults", "Default apps", "nav-apps", "browser mail preferred applications"),
    ("tools", "Tools", "settings-tools", "coding"),
    ("coding-clis", "Coding CLIs", "settings-agents", "claude codex agents"),
    ("worktrees", "Worktrees", "settings-local-repos", "git repositories"),
    ("sessions", "Sessions", "settings-sessions", "agents running"),
    ("account", "Account", "settings-account", "sign in sign out exponential"),
    ("devices", "Devices", "nav-devices", "computers servers"),
    ("security", "Security", "settings-api", "passkeys tokens"),
    ("whats-new", "What's new", "nav-changelog", "changelog release"),
    ("about", "About", "settings-about", "version system"),
];

/// The Settings modules as the start menu's search takes them.
pub fn settings_modules() -> &'static [uicore_model::StartMenuSetting] {
    static MODULES: std::sync::OnceLock<Vec<uicore_model::StartMenuSetting>> = std::sync::OnceLock::new();
    MODULES.get_or_init(|| {
        SETTINGS_MODULES
            .iter()
            .map(|(id, title, icon, keywords)| uicore_model::StartMenuSetting { id: id.to_string(), title: title.to_string(), icon: icon.to_string(), keywords: keywords.to_string() })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, target: bool) -> Value {
        json!({"id": id, "title": id, "target": if target { json!({"page": id}) } else { Value::Null }})
    }

    /// A team's contents: a folder (opens nothing), two boards, an agent.
    fn team() -> Value {
        json!({"category": "team:t1", "categories": [{"id": "home"}, {"id": "chats"}, {"id": "team:t1"}, {"id": "local"}], "rows": [row("folder:f1", false), row("board:b1", true), row("board:b2", true), row("agent:a1", true)]})
    }

    fn results() -> Value {
        json!({"category": "team:t1", "categories": [{"id": "team:t1"}], "rows": [row("board:b1", true), row("app:firefox", true), row("issue:i1", true)]})
    }

    fn selected(nav: &Value, s: &Selection, q: &str) -> Option<String> {
        let mut nav = nav.clone();
        mark(&mut nav, s, q);
        nav["rows"].as_array().unwrap().iter().find(|r| r["selected"] == true).map(|r| r["id"].as_str().unwrap().to_string())
    }

    #[test]
    fn the_menu_opens_in_the_categories_and_arrows_switch_them() {
        let nav = team();
        let mut s = Selection::default();
        assert_eq!(selected(&nav, &s, ""), None, "no row while the categories have the keyboard");
        assert_eq!(s.key(&nav, "", Key::Down), Some(Moved::Category("local".into())));
        assert_eq!(s.key(&nav, "", Key::Up), Some(Moved::Category("chats".into())));
        let first = json!({"category": "home", "categories": [{"id": "home"}, {"id": "chats"}], "rows": []});
        assert_eq!(s.key(&first, "", Key::Up), Some(Moved::Row(None)), "the first stays");
    }

    #[test]
    fn right_enters_the_contents_and_left_leaves_them() {
        let nav = team();
        let mut s = Selection::default();
        assert_eq!(s.key(&nav, "", Key::Right), Some(Moved::Row(Some(1))), "the folder opens nothing: the first board");
        assert_eq!(selected(&nav, &s, "").as_deref(), Some("board:b1"));
        assert_eq!(s.key(&nav, "", Key::Down), Some(Moved::Row(Some(2))));
        assert_eq!(s.key(&nav, "", Key::Down), Some(Moved::Row(Some(3))));
        assert_eq!(s.key(&nav, "", Key::Down), Some(Moved::Row(Some(3))), "the last stays");
        assert_eq!(s.activation(&nav, ""), Some(Activation::Target(json!({"page": "agent:a1"}))));
        assert_eq!(s.key(&nav, "", Key::Left), Some(Moved::Row(None)));
        assert_eq!(s.column(), Column::Categories);
        assert_eq!(s.activation(&nav, ""), Some(Activation::Category("team:t1".into())), "Enter on the category");
    }

    #[test]
    fn enter_on_a_category_without_a_page_goes_into_it() {
        let nav = team();
        let mut s = Selection::default();
        assert!(s.enter_contents(&nav));
        assert_eq!(selected(&nav, &s, "").as_deref(), Some("board:b1"));
        let empty = json!({"category": "drafts", "categories": [], "rows": []});
        assert!(!Selection::default().enter_contents(&empty));
    }

    #[test]
    fn typing_selects_the_first_result_and_enter_opens_it() {
        let nav = results();
        let mut s = Selection::default();
        assert_eq!(selected(&nav, &s, "desk").as_deref(), Some("board:b1"));
        assert_eq!(s.activation(&nav, "desk"), Some(Activation::Target(json!({"page": "board:b1"}))));
        assert_eq!(s.key(&nav, "desk", Key::Down), Some(Moved::Row(Some(1))));
        assert_eq!(s.key(&nav, "desk", Key::Down), Some(Moved::Row(Some(2))));
        assert_eq!(s.key(&nav, "desk", Key::Down), Some(Moved::Row(Some(2))), "the last stays");
        assert_eq!(s.key(&nav, "desk", Key::Left), None, "the caret's");
        assert_eq!(selected(&nav, &s, "deskt").as_deref(), Some("board:b1"), "new text: the first again");
    }

    #[test]
    fn no_result_no_enter() {
        let nav = json!({"category": "team:t1", "categories": [], "rows": []});
        assert_eq!(Selection::default().activation(&nav, "zzz"), None);
        assert_eq!(Selection::default().key(&nav, "zzz", Key::Down), None);
    }

    #[test]
    fn a_press_finds_its_row() {
        assert_eq!(row_target(&team(), "board:b2"), Some(json!({"page": "board:b2"})));
        assert_eq!(row_target(&team(), "folder:f1"), None);
    }

    #[test]
    fn every_settings_module_is_a_page_of_settings() {
        let source = uicore_docs::doc("settings").expect("settings").source;
        for (id, title, ..) in SETTINGS_MODULES {
            assert!(source.contains(&format!("value=\"{id}\"")), "{id} is no page of settings.express");
            assert!(source.contains(&format!("title=\"{title}\"")), "{title}");
        }
    }

    #[test]
    fn only_bare_arrows_are_the_menus() {
        assert_eq!(key_of("left", false), Some(Key::Left));
        assert_eq!(key_of("down", true), None);
        assert_eq!(key_of("enter", false), None);
    }
}
