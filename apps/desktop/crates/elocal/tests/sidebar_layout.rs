//! #EXP-1832: a Local desktop keeps the sidebar folders like the server
//! (sidebar-folders.json, local-parity.json `sidebarLayout`): the desktop's
//! own path (`domain::sidebar_folders` → the `api` wrappers) over the local
//! executor, the rows in the store's `sidebar_layout` table with the shape's
//! columns, read back the way the sidebar reads the synced shape.

use api::sidebar_layout::{sidebar_layout_remove, sidebar_layout_rename, sidebar_layout_save, SidebarLayoutSaveInput};
use domain::rows::SidebarLayoutRow;
use domain::sidebar_folders::{
    sidebar_entries, sidebar_move, sidebar_tree, sidebar_ungroup, SidebarEntryKind, SidebarEntryWrite, SidebarMove, SidebarNode,
    SidebarSection, SidebarWrites,
};
use elocal::{Desktops, LocalDesktop};
use serde_json::{json, Value};

fn desktop() -> (tempfile::TempDir, LocalDesktop) {
    let dir = tempfile::tempdir().unwrap();
    let desktops = Desktops::open(dir.path()).unwrap();
    let id = desktops.current().unwrap().to_string();
    let desktop = desktops.open_desktop(&id).unwrap();
    (dir, desktop)
}

fn rows(desktop: &LocalDesktop) -> Vec<SidebarLayoutRow> {
    desktop.store.rows("sidebar_layout").unwrap().into_iter().map(|row| serde_json::from_value(Value::Object(row)).unwrap()).collect()
}

fn status(result: Result<impl std::fmt::Debug, api::ApiError>) -> u16 {
    match result {
        Err(api::ApiError::Http { status, .. }) => status,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn the_shape_table_carries_the_proxy_columns() {
    let spec = sync::shapes::shape_by_name("sidebar_layout").unwrap();
    assert_eq!(
        spec.columns,
        &["id", "user_id", "team_id", "section", "kind", "target_id", "parent_id", "name", "sort_order", "created_at", "updated_at"]
    );
}

#[test]
fn moves_rename_and_ungroup_round_trip_through_the_store() {
    let (_dir, desktop) = desktop();
    let client = desktop.client();
    let team_id = desktop.executor.team_id().to_string();
    let items: Vec<String> = ["b-1", "b-2", "b-3"].map(String::from).to_vec();
    let section = SidebarSection::Board;
    let save = |writes: &SidebarWrites| sidebar_layout_save(&client, &SidebarLayoutSaveInput::new(&team_id, section, writes)).unwrap();
    let tree = || sidebar_tree(&items, &sidebar_entries(&rows(&desktop), &team_id, section));

    // b-3 into b-1: a new folder at b-1's place holding b-1 then b-3.
    let into = SidebarMove { id: "b-3".into(), into: Some("b-1".into()), ..Default::default() };
    let writes = sidebar_move(&items, &[], &into, "f-new").unwrap();
    assert_eq!(writes.created_folder_id.as_deref(), Some("f-new"));
    assert!(save(&writes).tx_id.is_some());
    assert_eq!(
        tree(),
        vec![
            SidebarNode::Folder { id: "f-new".into(), name: "New folder".into(), children: vec!["b-1".into(), "b-3".into()] },
            SidebarNode::Item { id: "b-2".into() },
        ]
    );
    let user = desktop.executor.identity().user_id;
    assert!(rows(&desktop).iter().all(|row| row.user_id.as_deref() == Some(user.as_str())));

    // Rename saves the trimmed name; an unknown folder is a 404.
    assert_eq!(sidebar_layout_rename(&client, "f-new", "  Ops ").unwrap().name, "Ops");
    assert_eq!(status(sidebar_layout_rename(&client, "f-nope", "x")), 404);

    // A second move re-uses the item rows (upserted by target, no duplicates).
    let entries = sidebar_entries(&rows(&desktop), &team_id, section);
    let before = SidebarMove { id: "b-2".into(), before: Some("f-new".into()), ..Default::default() };
    save(&sidebar_move(&items, &entries, &before, "f-unused").unwrap());
    assert_eq!(rows(&desktop).len(), 4);
    assert_eq!(tree()[0], SidebarNode::Item { id: "b-2".into() });

    // Ungroup: the folder goes, its items take its place.
    let entries = sidebar_entries(&rows(&desktop), &team_id, section);
    save(&sidebar_ungroup(&items, &entries, "f-new").unwrap());
    assert_eq!(tree(), ["b-2", "b-1", "b-3"].map(|id| SidebarNode::Item { id: id.into() }).to_vec());
    assert!(rows(&desktop).iter().all(|row| row.kind.as_deref() == Some("item")));
}

#[test]
fn the_servers_checks_refuse_a_broken_layout() {
    let (_dir, desktop) = desktop();
    let client = desktop.client();
    let team_id = desktop.executor.team_id().to_string();
    let item = |parent: Option<&str>| SidebarEntryWrite {
        id: None,
        kind: SidebarEntryKind::Item,
        target_id: Some("b-1".into()),
        parent_id: parent.map(String::from),
        name: None,
        sort_order: 1.0,
    };
    let save = |team: &str, upserts: Vec<SidebarEntryWrite>| {
        let writes = SidebarWrites { upserts, ..Default::default() };
        sidebar_layout_save(&client, &SidebarLayoutSaveInput::new(team, SidebarSection::Board, &writes))
    };
    // An item's parent must be a folder of the section.
    assert_eq!(status(save(&team_id, vec![item(Some("f-missing"))])), 400);
    // A folder carries an id.
    let folder = SidebarEntryWrite { kind: SidebarEntryKind::Folder, target_id: None, ..item(None) };
    assert_eq!(status(save(&team_id, vec![folder])), 400);
    // Another team is not this desktop's.
    assert_eq!(status(save("t-other", vec![item(None)])), 404);
    assert!(rows(&desktop).is_empty());
}

/// #EXP-2027 (sidebar-tree.json, local-parity.json `sidebarLayout`): the
/// `team` list places only the team's boards, agents (role agent), apps and
/// root sessions the identity started or that are shared; `remove` drops the
/// identity's item entry for a target.
#[test]
fn the_team_section_checks_its_targets_and_remove_drops_an_entry() {
    let (_dir, desktop) = desktop();
    let client = desktop.client();
    let team_id = desktop.executor.team_id().to_string();
    let user = desktop.executor.identity().user_id;
    let write = |table: &str, row: Value| desktop.store.write(table, row.as_object().unwrap().clone()).unwrap();
    let board = desktop.store.rows("boards").unwrap()[0]["id"].as_str().unwrap().to_string();
    write("agents", json!({"id": "a-1", "team_id": team_id, "name": "Ops", "role": "agent", "sort_order": 1}));
    write("agents", json!({"id": "a-x", "team_id": team_id, "name": "Assistant", "role": "assistant", "sort_order": 0}));
    write("apps", json!({"id": "p-1", "team_id": team_id, "slug": "crm", "name": "crm"}));
    write("sessions", json!({"id": "s-mine", "team_id": team_id, "parent_id": null, "user_id": user, "shared": false, "depth": 0}));
    write("sessions", json!({"id": "s-shared", "team_id": team_id, "parent_id": null, "user_id": "u-other", "shared": true, "depth": 0}));
    write("sessions", json!({"id": "s-private", "team_id": team_id, "parent_id": null, "user_id": "u-other", "shared": false, "depth": 0}));
    write("sessions", json!({"id": "s-child", "team_id": team_id, "parent_id": "s-mine", "user_id": user, "shared": false, "depth": 1}));
    write("sessions", json!({"id": "s-elsewhere", "team_id": "t-other", "parent_id": null, "user_id": user, "shared": false, "depth": 0}));

    let item = |target: &str, sort_order: f64| SidebarEntryWrite {
        id: None,
        kind: SidebarEntryKind::Item,
        target_id: Some(target.into()),
        parent_id: None,
        name: None,
        sort_order,
    };
    let save = |upserts: Vec<SidebarEntryWrite>| {
        let writes = SidebarWrites { upserts, ..Default::default() };
        sidebar_layout_save(&client, &SidebarLayoutSaveInput::new(&team_id, SidebarSection::Team, &writes))
    };
    let targets = [board.as_str(), "a-1", "p-1", "s-mine", "s-shared"];
    save(targets.iter().enumerate().map(|(at, id)| item(id, (at + 1) as f64)).collect()).unwrap();
    let entries = sidebar_entries(&rows(&desktop), &team_id, SidebarSection::Team);
    assert_eq!(entries.len(), 5);

    for refused in ["a-x", "s-private", "s-child", "s-elsewhere", "nope"] {
        match save(vec![item(refused, 9.0)]) {
            Err(api::ApiError::Http { status: 400, .. }) => {}
            other => panic!("{refused}: expected a 400, got {other:?}"),
        }
    }
    assert_eq!(rows(&desktop).len(), 5);

    // Remove from sidebar: the session's entry goes, the rest stay; a second
    // remove deletes nothing and is fine.
    assert!(sidebar_layout_remove(&client, "s-mine").unwrap().tx_id.is_some());
    sidebar_layout_remove(&client, "s-mine").unwrap();
    let left: Vec<String> = rows(&desktop).into_iter().filter_map(|row| row.target_id).collect();
    assert_eq!(left.len(), 4);
    assert!(!left.iter().any(|id| id == "s-mine"));
}
