//! #EDE-92: the start menu's keyboard selection (`apps/ede/ui/start-menu.json`).
//! While the search has text, one result row is the selected one: the first
//! by default, Up and Down move it and stop at the ends, Enter in the search
//! box runs it (an app launches, an issue opens) as a press on it would. The
//! rows are the apps, then the issues, as the menu lists them; the selection
//! belongs to its query, so a changed query selects the first row again.
//! Without text the menu is as it was: no row selected, Enter does nothing.

use serde_json::{json, Value};

/// The selected result of one query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    query: String,
    index: usize,
}

impl Selection {
    /// The selected row among `rows` results of `query`: none without text
    /// or results, the first for a query this selection is not of, never
    /// past the last (the issues arrive after the apps and may shrink).
    pub fn index(&self, query: &str, rows: usize) -> Option<usize> {
        if query.trim().is_empty() || rows == 0 {
            return None;
        }
        Some(if self.query == query { self.index.min(rows - 1) } else { 0 })
    }

    /// Up (`-1`) or Down (`1`): the row it lands on, stopping at the ends;
    /// `None` when there is nothing to select (the key is not the menu's).
    pub fn step(&mut self, query: &str, rows: usize, step: i64) -> Option<usize> {
        let at = self.index(query, rows)? as i64;
        let next = (at + step).clamp(0, rows as i64 - 1) as usize;
        *self = Selection { query: query.to_string(), index: next };
        Some(next)
    }
}

fn len(start_menu: &Value, list: &str) -> usize {
    start_menu[list].as_array().map_or(0, Vec::len)
}

/// The result rows of `/shell/startMenu`: its apps, then its issues.
pub fn rows(start_menu: &Value) -> usize {
    len(start_menu, "apps") + len(start_menu, "issues")
}

/// The selected row of `/shell/startMenu` for `selection`.
pub fn selected(start_menu: &Value, selection: &Selection) -> Option<usize> {
    selection.index(start_menu["query"].as_str().unwrap_or_default(), rows(start_menu))
}

/// Every app and issue row of `/shell/startMenu` says whether it is the
/// selected one (the document's rows bind `selected`).
pub fn mark(start_menu: &mut Value, selection: &Selection) {
    let at = selected(start_menu, selection);
    let apps = len(start_menu, "apps");
    for (list, offset) in [("apps", 0), ("issues", apps)] {
        for (i, row) in start_menu[list].as_array_mut().into_iter().flatten().enumerate() {
            row["selected"] = json!(at == Some(offset + i));
        }
    }
}

/// Enter in the search box: the host function a press on the selected row
/// calls, with its args (`ede_launch {app}` or `ede_issue {identifier,
/// source, sourceId}`); `None` with no row selected.
pub fn activation(start_menu: &Value) -> Option<(&'static str, Value)> {
    let is_selected = |row: &&Value| row["selected"] == json!(true);
    if let Some(app) = start_menu["apps"].as_array().into_iter().flatten().find(is_selected) {
        return Some(("ede_launch", json!({"app": app["id"]})));
    }
    let issue = start_menu["issues"].as_array().into_iter().flatten().find(is_selected)?;
    Some(("ede_issue", json!({"identifier": issue["identifier"], "source": issue["source"], "sourceId": issue["sourceId"]})))
}

/// The selection's step for a key pressed in the menu: Down and Up without
/// a modifier; every other key (Enter, Tab, Escape, the text) stays the
/// focused component's.
pub fn key_step(key: &str, modified: bool) -> Option<i64> {
    match key {
        _ if modified => None,
        "down" => Some(1),
        "up" => Some(-1),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `/shell/startMenu` as the shell feeds it: `term` finds three apps and two issues.
    fn menu(query: &str, apps: &[&str], issues: &[&str]) -> Value {
        json!({
            "open": true,
            "query": query,
            "apps": apps.iter().map(|id| json!({"id": id, "name": id})).collect::<Vec<_>>(),
            "issues": issues.iter().map(|id| json!({"id": id, "identifier": id, "source": "team", "sourceId": "t1", "title": "An issue"})).collect::<Vec<_>>(),
        })
    }

    fn term() -> Value {
        menu("term", &["org.gnome.Terminal", "uxterm", "xterm"], &["EDE-1", "EDE-2"])
    }

    fn marked(mut m: Value, selection: &Selection) -> Value {
        mark(&mut m, selection);
        m
    }

    fn selected_rows(m: &Value) -> Vec<String> {
        ["apps", "issues"].iter().flat_map(|l| m[*l].as_array().unwrap().iter()).filter(|r| r["selected"] == json!(true)).map(|r| r["id"].as_str().unwrap().to_string()).collect()
    }

    #[test]
    fn a_typed_query_selects_the_first_result() {
        let m = marked(term(), &Selection::default());
        assert_eq!(selected_rows(&m), ["org.gnome.Terminal"]);
        assert!(m["apps"].as_array().unwrap().iter().chain(m["issues"].as_array().unwrap()).all(|r| r["selected"].is_boolean()), "every row says so: {m}");
        // Issues only: the first of them.
        assert_eq!(selected_rows(&marked(menu("#12", &[], &["EDE-12", "EDE-120"]), &Selection::default())), ["EDE-12"]);
    }

    #[test]
    fn without_text_no_row_is_selected() {
        for query in ["", "  "] {
            let m = marked(menu(query, &["firefox", "nemo"], &[]), &Selection::default());
            assert!(selected_rows(&m).is_empty(), "{query:?}: {m}");
            assert_eq!(activation(&m), None);
        }
    }

    #[test]
    fn down_and_up_move_the_selection_and_stop_at_the_ends() {
        let m = term();
        let mut s = Selection::default();
        let mut step = |by: i64| {
            s.step("term", rows(&m), by).expect("a row");
            selected_rows(&marked(m.clone(), &s))
        };
        assert_eq!(step(-1), ["org.gnome.Terminal"], "Up on the first row stays");
        assert_eq!(step(1), ["uxterm"]);
        assert_eq!(step(1), ["xterm"]);
        assert_eq!(step(1), ["EDE-1"], "the issues follow the apps");
        assert_eq!(step(1), ["EDE-2"]);
        assert_eq!(step(1), ["EDE-2"], "Down on the last row stays");
        assert_eq!(step(-1), ["EDE-1"]);
        assert_eq!(step(-1), ["xterm"]);
    }

    #[test]
    fn a_changed_query_selects_the_first_result_again() {
        let mut s = Selection::default();
        s.step("term", 5, 1);
        s.step("term", 5, 1);
        assert_eq!(s.index("term", 5), Some(2));
        assert_eq!(selected_rows(&marked(menu("termi", &["org.gnome.Terminal", "terminator"], &[]), &s)), ["org.gnome.Terminal"]);
        // Back at the old text after another one: the first again.
        s.step("termi", 2, 1);
        assert_eq!(s.index("term", 5), Some(0));
    }

    #[test]
    fn a_shorter_list_keeps_a_row_selected() {
        let mut s = Selection::default();
        for _ in 0..4 {
            s.step("term", 5, 1);
        }
        // The issue search answered with fewer hits.
        assert_eq!(selected_rows(&marked(menu("term", &["org.gnome.Terminal", "uxterm", "xterm"], &[]), &s)), ["xterm"]);
    }

    #[test]
    fn enter_runs_the_selected_row() {
        let m = term();
        let mut s = Selection::default();
        assert_eq!(activation(&marked(m.clone(), &s)), Some(("ede_launch", json!({"app": "org.gnome.Terminal"}))));
        s.step("term", rows(&m), 1);
        assert_eq!(activation(&marked(m.clone(), &s)), Some(("ede_launch", json!({"app": "uxterm"}))));
        s.step("term", rows(&m), 1);
        s.step("term", rows(&m), 1);
        assert_eq!(activation(&marked(m.clone(), &s)), Some(("ede_issue", json!({"identifier": "EDE-1", "source": "team", "sourceId": "t1"}))));
    }

    #[test]
    fn with_no_result_enter_does_nothing() {
        let m = marked(menu("zzzz", &[], &[]), &Selection::default());
        assert_eq!(activation(&m), None);
        let mut s = Selection::default();
        assert_eq!(s.step("zzzz", rows(&m), 1), None, "and the arrows are not the menu's");
        // A menu that was never marked (an older shell's feed) runs nothing either.
        assert_eq!(activation(&term()), None);
    }

    #[test]
    fn only_plain_up_and_down_are_the_selections() {
        assert_eq!((key_step("down", false), key_step("up", false)), (Some(1), Some(-1)));
        for key in ["enter", "tab", "escape", "left", "right", "home", "end", "pageup", "pagedown", "space", "a"] {
            assert_eq!(key_step(key, false), None, "{key} stays the focused component's");
        }
        assert_eq!((key_step("down", true), key_step("up", true)), (None, None), "a modifier keeps the key out");
    }
}
