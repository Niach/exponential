//! #EDE-65: the start menu's issue search. Every connected account's synced
//! teams (the shell's per-account stores, `issues` + `boards` + `teams`) and
//! every active Local desktop's store make ONE pool, indexed when the menu
//! opens (off the main loop) and ranked per query by the issue-search engine
//! every client runs (`domain::issue_search`, fixture-locked), so `EXP-42`
//! finds EXP-42 first whatever team it is in. The menu shows the hits under
//! the apps (`uicore_model::start_menu_issues`, packages/uicore's
//! shell.json); opening one hands it to the Exponential app until the shell
//! shows issues itself.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{Map, Value};
use uicore_model::{StartMenuIssue, SwitcherKind, START_MENU_ISSUE_LIMIT};

/// One searchable issue with where it lives.
#[derive(Debug, Clone, PartialEq)]
pub struct Indexed {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub description: Option<String>,
    pub status: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub source: SwitcherKind,
    pub source_id: String,
    pub source_name: String,
    pub board: String,
}

/// What an index build reads: the accounts' stores and the Local desktops.
pub enum Source {
    Account(Arc<sync::store::ShapeStore>),
    Local { id: String, name: String, path: PathBuf },
}

type Row = Map<String, Value>;

fn text(row: &Row, key: &str) -> Option<String> {
    row.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string)
}

/// One store's issues, each with its board and its source (`None` from
/// `source_of` = a team this store does not name: skipped).
pub fn index_rows(issues: &[Row], boards: &[Row], source_of: impl Fn(&str) -> Option<(SwitcherKind, String, String)>) -> Vec<Indexed> {
    let boards: HashMap<String, (String, String)> = boards.iter().filter_map(|b| Some((text(b, "id")?, (text(b, "name").unwrap_or_default(), text(b, "team_id")?)))).collect();
    issues
        .iter()
        .filter_map(|row| {
            let (board, team) = boards.get(&text(row, "board_id")?)?;
            let (source, source_id, source_name) = source_of(team)?;
            Some(Indexed {
                id: text(row, "id")?,
                identifier: text(row, "identifier")?,
                title: text(row, "title").unwrap_or_default(),
                description: text(row, "description"),
                status: text(row, "status"),
                created_at: text(row, "created_at"),
                updated_at: text(row, "updated_at"),
                source,
                source_id,
                source_name,
                board: board.clone(),
            })
        })
        .collect()
}

fn shape_rows(store: &sync::store::ShapeStore, name: &str) -> Vec<Row> {
    sync::shapes::shape_by_name(name).and_then(|spec| store.read_all(spec).ok()).unwrap_or_default()
}

/// The pool: blocking (SQLite reads), so the shell runs it off the main loop.
pub fn build(sources: Vec<Source>) -> Vec<Indexed> {
    let mut out = Vec::new();
    for source in sources {
        match source {
            Source::Account(store) => {
                let teams: HashMap<String, String> = shape_rows(&store, "teams").iter().filter_map(|t| Some((text(t, "id")?, text(t, "name").unwrap_or_default()))).collect();
                out.extend(index_rows(&shape_rows(&store, "issues"), &shape_rows(&store, "boards"), |team| teams.get(team).map(|name| (SwitcherKind::Team, team.to_string(), name.clone()))));
            }
            Source::Local { id, name, path } => {
                if !path.exists() {
                    continue;
                }
                let store = match elocal::LocalStore::open(&path) {
                    Ok(store) => store,
                    Err(e) => {
                        tracing::warn!(desktop = %id, "issue search: {e}");
                        continue;
                    }
                };
                let rows = |table: &str| store.rows(table).unwrap_or_default();
                out.extend(index_rows(&rows("issues"), &rows("boards"), |_| Some((SwitcherKind::Local, id.clone(), name.clone()))));
            }
        }
    }
    out
}

/// The engine's ranking over the whole pool, capped at the menu's rows.
pub fn search(index: &[Indexed], query: &str) -> Vec<StartMenuIssue> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    let rows: Vec<domain::issue_search::SearchRow<'_>> = index
        .iter()
        .map(|i| {
            domain::issue_search::SearchRow::new(&i.id, &i.identifier, &i.title)
                .with_description(i.description.as_deref())
                .with_times(i.created_at.as_deref(), i.updated_at.as_deref())
                .with_status(i.status.as_deref())
        })
        .collect();
    domain::issue_search::rank(&rows, query, START_MENU_ISSUE_LIMIT, &HashSet::new())
        .into_iter()
        .map(|ix| {
            let i = &index[ix];
            StartMenuIssue {
                id: i.id.clone(),
                identifier: i.identifier.clone(),
                title: i.title.clone(),
                status: i.status.clone().unwrap_or_default(),
                source: i.source.clone(),
                source_id: i.source_id.clone(),
                source_name: i.source_name.clone(),
                board: i.board.clone(),
            }
        })
        .collect()
}

/// The deep link that opens an issue in the Exponential app.
/// #EDE-65 J4: where a start-menu issue lives (`ede_issue {source, sourceId}`).
pub fn issue_source(args: &Value) -> Option<(SwitcherKind, String)> {
    let kind = serde_json::from_value::<SwitcherKind>(args.get("source")?.clone()).ok()?;
    let id = args.get("sourceId")?.as_str().filter(|id| !id.is_empty())?;
    Some((kind, id.to_string()))
}

pub fn issue_deep_link(identifier: &str) -> String {
    format!("{}://issue/{identifier}", api::login::OAUTH_CALLBACK_SCHEME)
}

/// The demo feed's issues (the goldens): the demo switcher's desktops and teams.
pub fn demo_index() -> Vec<Indexed> {
    let issue = |id: &str, identifier: &str, title: &str, status: &str, source: SwitcherKind, source_id: &str, source_name: &str, board: &str| Indexed {
        id: id.into(),
        identifier: identifier.into(),
        title: title.into(),
        description: None,
        status: Some(status.into()),
        created_at: Some(format!("2026-11-0{}T10:00:00Z", 1 + id.len() % 7)),
        updated_at: None,
        source,
        source_id: source_id.into(),
        source_name: source_name.into(),
        board: board.into(),
    };
    vec![
        issue("d1", "FB-12", "Search issues from the start menu", "in_progress", SwitcherKind::Team, "t-feedback", "Feedback", "Shell"),
        issue("d2", "FB-9", "The switcher's pick lands", "done", SwitcherKind::Team, "t-feedback", "Feedback", "Shell"),
        issue("d3", "SP-4", "Search the side project's backlog", "backlog", SwitcherKind::Team, "t-side", "Side project", "Ideas"),
        issue("d4", "LOC-3", "Search the Local desktop too", "in_review", SwitcherKind::Local, "local", "Local", "Issues"),
        issue("d5", "WRK-7", "Quarterly report", "backlog", SwitcherKind::Local, "work", "Work", "Issues"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// #EDE-65 J4: a hit names its team or Local desktop (the row's `source`, `sourceId`).
    #[test]
    fn a_hit_names_where_it_lives() {
        assert_eq!(issue_source(&json!({"identifier": "GEN-1", "source": "local", "sourceId": "d1"})), Some((SwitcherKind::Local, "d1".into())));
        assert_eq!(issue_source(&json!({"identifier": "EXP-1", "source": "team", "sourceId": "t1"})), Some((SwitcherKind::Team, "t1".into())));
        assert_eq!(issue_source(&json!({"identifier": "EXP-1"})), None);
        assert_eq!(issue_source(&json!({"identifier": "EXP-1", "source": "team", "sourceId": ""})), None);
        // The row the document binds carries both under those names.
        let row = serde_json::to_value(uicore_model::StartMenuIssueRow { id: "i".into(), identifier: "GEN-1".into(), title: "t".into(), status: "backlog".into(), icon: String::new(), source: SwitcherKind::Local, source_id: "d1".into(), context: String::new() }).unwrap();
        assert_eq!(issue_source(&row), Some((SwitcherKind::Local, "d1".into())));
    }

    fn row(value: Value) -> Row {
        value.as_object().unwrap().clone()
    }

    fn pool() -> Vec<Indexed> {
        let boards = [row(json!({"id": "b1", "team_id": "t1", "name": "Engineering"})), row(json!({"id": "b2", "team_id": "t2", "name": "Ideas"}))];
        let team = |t: &str| match t {
            "t1" => Some((SwitcherKind::Team, "t1".into(), "Acme".into())),
            _ => None,
        };
        let mut index = index_rows(
            &[
                row(json!({"id": "a", "board_id": "b1", "identifier": "EXP-42", "title": "Search issues", "status": "done", "created_at": "2026-11-01T00:00:00Z"})),
                row(json!({"id": "b", "board_id": "b1", "identifier": "EXP-7", "title": "Fix search ranking", "status": "in_progress", "created_at": "2026-11-02T00:00:00Z"})),
                row(json!({"id": "c", "board_id": "b2", "identifier": "SIDE-1", "title": "Search the other team", "status": "backlog"})),
                row(json!({"id": "d", "board_id": "gone", "identifier": "EXP-9", "title": "Search without a board"})),
            ],
            &boards,
            team,
        );
        index.extend(index_rows(
            &[row(json!({"id": "l", "board_id": "lb", "identifier": "LOC-42", "title": "Search the Local desktop", "status": "backlog", "created_at": "2026-11-03T00:00:00Z"}))],
            &[row(json!({"id": "lb", "team_id": "lt", "name": "Issues"}))],
            |_| Some((SwitcherKind::Local, "local".into(), "Local".into())),
        ));
        index
    }

    #[test]
    fn a_store_indexes_its_issues_with_board_and_source() {
        let index = pool();
        // SIDE-1's team is not this store's, EXP-9's board is gone.
        assert_eq!(index.iter().map(|i| i.identifier.as_str()).collect::<Vec<_>>(), ["EXP-42", "EXP-7", "LOC-42"]);
        assert_eq!((index[0].source_name.as_str(), index[0].board.as_str()), ("Acme", "Engineering"));
        assert_eq!((index[2].source.clone(), index[2].source_id.as_str()), (SwitcherKind::Local, "local"));
    }

    /// One pool across teams and Local: an exact identifier first whatever
    /// its state, then open work before done (the engine's rules), the engine's score after.
    #[test]
    fn the_pool_ranks_as_one_across_teams_and_local() {
        let index = pool();
        let ids = |q: &str| search(&index, q).into_iter().map(|h| h.identifier).collect::<Vec<_>>();
        assert_eq!(ids("EXP-42"), ["EXP-42"]);
        // Both numbers are exact: the open one first.
        assert_eq!(ids("42"), ["LOC-42", "EXP-42"]);
        // Equal scores: open before done, then the newer first, across sources.
        assert_eq!(ids("search"), ["LOC-42", "EXP-7", "EXP-42"]);
        assert_eq!(ids("  "), Vec::<String>::new());
        assert_eq!(ids("nothing like it"), Vec::<String>::new());
    }

    #[test]
    fn the_hits_cap_at_the_menus_rows() {
        let many: Vec<Indexed> = (0..30).map(|n| Indexed { id: format!("i{n}"), identifier: format!("EXP-{n}"), title: "Search".into(), ..pool()[0].clone() }).collect();
        assert_eq!(search(&many, "search").len(), START_MENU_ISSUE_LIMIT);
    }

    /// The hand-off: the app's socket when it runs, else a cold start with
    /// the link as the entry's `%U`, so the issue is not lost.
    #[test]
    fn an_issue_opens_through_the_apps_link_even_on_a_cold_start() {
        assert_eq!(issue_deep_link("EXP-42"), "exponential://issue/EXP-42");
        let app = crate::apps::App {
            id: "at.exponential.desktop".into(),
            name: "Exponential".into(),
            comment: String::new(),
            exec: "exponential-desktop".into(),
            exec_url: Some("exponential-desktop %U".into()),
            icon: None,
            keywords: String::new(),
            categories: vec![],
        };
        assert_eq!(crate::ide_command(&app, Some(&issue_deep_link("LOC-3"))), "exponential-desktop 'exponential://issue/LOC-3'");
    }
}
