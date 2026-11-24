//! #EXP-2124: the sidebar menus (web md+ and the desktop IDE), the Rust twin
//! of `packages/domain-contract/src/sidebar-menus.ts` (fixture
//! `sidebar-menus.json`, whose `$comment` is the spec; replayed whole by
//! `tests/sidebar_menus.rs`).
//!
//! ONE menu table for every sidebar row (`kinds`): five groups in a fixed
//! order, a separator between the non-empty ones; THE New list (`newKinds`)
//! every team creation uses; the tab strip's new-tab menu.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/sidebar-menus.json");

/// The separator between two non-empty groups.
pub const SIDEBAR_MENU_SEPARATOR: &str = "separator";

/// What decides a kind's conditional rows (`when`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SidebarMenuInput {
    /// The viewer owns the item's team.
    pub owner: bool,
    /// The viewer's own device.
    pub mine: bool,
    /// chats.json `menuCases` delete rule.
    pub can_delete: bool,
}

impl SidebarMenuInput {
    fn flag(&self, name: &str) -> bool {
        match name {
            "owner" => self.owner,
            "mine" => self.mine,
            "canDelete" => self.can_delete,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum TableEntry {
    Id(String),
    When { id: String, when: String },
}

#[derive(Debug, Clone, Deserialize)]
pub struct SidebarShortcut {
    pub apple: String,
    pub other: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Table {
    groups: Vec<String>,
    copy: BTreeMap<String, String>,
    icons: BTreeMap<String, String>,
    shortcuts: BTreeMap<String, SidebarShortcut>,
    kinds: BTreeMap<String, BTreeMap<String, Vec<TableEntry>>>,
    new_kinds: Vec<String>,
    new_owner_only: Vec<String>,
    new_copy: BTreeMap<String, String>,
    new_icons: BTreeMap<String, String>,
    tabs: Tabs,
}

#[derive(Debug, Deserialize)]
struct Tabs {
    pages: Vec<String>,
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| serde_json::from_str(FIXTURE).expect("sidebar-menus.json"))
}

/// Every kind of the table (`assistant`, `home`, … `getting-started`).
pub fn sidebar_menu_kinds() -> Vec<&'static str> {
    table().kinds.keys().map(String::as_str).collect()
}

/// A verb's label (`copy`; also `newTab`, `linkCopied`, `top-level`).
pub fn sidebar_menu_copy(id: &str) -> &'static str {
    table().copy.get(id).map(String::as_str).unwrap_or_default()
}

/// A verb's concept icon.
pub fn sidebar_menu_icon(id: &str) -> &'static str {
    table().icons.get(id).map(String::as_str).unwrap_or_default()
}

/// A verb's shortcut as this platform writes it (only New chat has one).
pub fn sidebar_menu_shortcut(id: &str) -> Option<&'static str> {
    let shortcut = table().shortcuts.get(id)?;
    Some(if cfg!(target_os = "macos") { shortcut.apple.as_str() } else { shortcut.other.as_str() })
}

/// A `danger` row of `kind`: drawn destructive, asks first.
pub fn sidebar_menu_destructive(kind: &str, id: &str) -> bool {
    table().kinds.get(kind).and_then(|groups| groups.get("danger")).is_some_and(|entries| {
        entries.iter().any(|entry| match entry {
            TableEntry::Id(entry) => entry == id,
            TableEntry::When { id: entry, .. } => entry == id,
        })
    })
}

/// A kind's menu for the viewer: the groups in order, `separator` between
/// the non-empty ones. An unknown kind has no menu.
pub fn sidebar_item_menu(kind: &str, input: SidebarMenuInput) -> Vec<&'static str> {
    let table = table();
    let Some(groups) = table.kinds.get(kind) else { return Vec::new() };
    let mut out = Vec::new();
    for group in &table.groups {
        let rows: Vec<&'static str> = groups
            .get(group)
            .into_iter()
            .flatten()
            .filter_map(|entry| match entry {
                TableEntry::Id(id) => Some(id.as_str()),
                TableEntry::When { id, when } => input.flag(when).then_some(id.as_str()),
            })
            .collect();
        if rows.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(SIDEBAR_MENU_SEPARATOR);
        }
        out.extend(rows);
    }
    out
}

/// THE New list for the viewer: a chat for every member, the rest owners'.
pub fn sidebar_new_kinds(owner: bool) -> Vec<&'static str> {
    let table = table();
    table.new_kinds.iter().filter(|kind| owner || !table.new_owner_only.contains(kind)).map(String::as_str).collect()
}

/// A New entry's label (`Chat`, `Agent`, `App`, `Board`).
pub fn sidebar_new_copy(kind: &str) -> &'static str {
    table().new_copy.get(kind).map(String::as_str).unwrap_or_default()
}

/// A New entry's concept icon.
pub fn sidebar_new_icon(kind: &str) -> &'static str {
    table().new_icons.get(kind).map(String::as_str).unwrap_or_default()
}

/// The sidebar kinds whose page opens as a work tab (`open-new-tab`).
pub fn sidebar_tab_pages() -> Vec<&'static str> {
    table().tabs.pages.iter().map(String::as_str).collect()
}

/// One team of the new-tab menu (already in sidebar order).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewTabTeam {
    pub id: String,
    pub owner: bool,
}

/// One row of the new-tab menu: `assistant`, `separator` or `team:<id>`
/// with THE New list as its submenu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewTabMenuRow {
    pub id: String,
    pub children: Vec<String>,
}

/// THE new-tab menu (the tab strip's +): the Assistant, then one submenu
/// per team holding THE New list.
pub fn new_tab_menu(assistant: bool, teams: &[NewTabTeam]) -> Vec<NewTabMenuRow> {
    let rows = teams.iter().map(|team| NewTabMenuRow {
        id: format!("team:{}", team.id),
        children: sidebar_new_kinds(team.owner).into_iter().map(str::to_string).collect(),
    });
    if !assistant {
        return rows.collect();
    }
    let mut out = vec![NewTabMenuRow { id: "assistant".into(), children: Vec::new() }];
    if !teams.is_empty() {
        out.push(NewTabMenuRow { id: SIDEBAR_MENU_SEPARATOR.into(), children: Vec::new() });
    }
    out.extend(rows);
    out
}

/// THE genUI `menuItem`s of a kind's menu: labels and icons from the
/// table, `danger` rows destructive, the shortcut where one exists.
/// `submenu(id)` gives a verb's submenu rows (New ›, Move to folder ›).
pub fn sidebar_menu_items(kind: &str, input: SidebarMenuInput, submenu: impl Fn(&str) -> Option<serde_json::Value>) -> Vec<serde_json::Value> {
    use serde_json::json;
    sidebar_item_menu(kind, input)
        .into_iter()
        .map(|id| {
            if id == SIDEBAR_MENU_SEPARATOR {
                return json!({"kind": "separator"});
            }
            let mut item = json!({"label": sidebar_menu_copy(id), "value": id, "icon": sidebar_menu_icon(id)});
            if let Some(items) = submenu(id) {
                item = json!({"kind": "submenu", "label": sidebar_menu_copy(id), "icon": sidebar_menu_icon(id), "items": items});
            }
            if sidebar_menu_destructive(kind, id) {
                item["destructive"] = json!(true);
            }
            if let Some(shortcut) = sidebar_menu_shortcut(id) {
                item["shortcut"] = json!(shortcut);
            }
            item
        })
        .collect()
}
