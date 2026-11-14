//! #EXP-1832: sidebar folders + drag reorder, the Rust twin of
//! `packages/domain-contract/src/sidebar-folders.ts` (fixture
//! `sidebar-folders.json`, whose `$comment` is the spec; replayed whole by
//! `tests/sidebar_folders.rs`).
//!
//! One synced `sidebar_layout` row per (member, team, section) node: a FOLDER
//! (`kind: folder`, a name, never navigable) or an ITEM entry placing one
//! board, agent or app (`target_id`), optionally inside a folder
//! (`parent_id`). Two layers max: a folder holds items only. A row with no
//! entry keeps the default order (the caller's `items`), after the placed
//! ones.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize, Serializer};
use serde_json::Value;

use crate::rows::SidebarLayoutRow;

pub const SIDEBAR_FOLDER_DEFAULT_NAME: &str = "New folder";
/// Nesting levels: top level + one folder level.
pub const SIDEBAR_MAX_DEPTH: u8 = 2;
/// A folder name's longest saved form (UTF-16 code units, like JS `slice`).
pub const SIDEBAR_FOLDER_NAME_MAX: usize = 80;

/// One sidebar group's rows (the `section` column).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SidebarSection {
    Board,
    Agent,
    App,
}

impl SidebarSection {
    pub const ALL: [SidebarSection; 3] = [SidebarSection::Board, SidebarSection::Agent, SidebarSection::App];

    pub fn as_str(self) -> &'static str {
        match self {
            SidebarSection::Board => "board",
            SidebarSection::Agent => "agent",
            SidebarSection::App => "app",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|section| section.as_str() == value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SidebarEntryKind {
    Folder,
    Item,
}

impl SidebarEntryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SidebarEntryKind::Folder => "folder",
            SidebarEntryKind::Item => "item",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "folder" => Some(SidebarEntryKind::Folder),
            "item" => Some(SidebarEntryKind::Item),
            _ => None,
        }
    }
}

/// A whole sort order goes out as an integer (`1`, not `1.0`), like JS.
fn number<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    if value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        serializer.serialize_i64(*value as i64)
    } else {
        serializer.serialize_f64(*value)
    }
}

/// One synced `sidebar_layout` row of a section (the shape's columns).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarEntry {
    pub id: String,
    pub kind: SidebarEntryKind,
    /// The board / agent / app an item places; `None` on a folder.
    #[serde(default)]
    pub target_id: Option<String>,
    /// The folder an item sits in; `None` = top level (always on a folder).
    #[serde(default)]
    pub parent_id: Option<String>,
    /// A folder's name; `None` on an item.
    #[serde(default)]
    pub name: Option<String>,
    #[serde(serialize_with = "number")]
    pub sort_order: f64,
}

impl SidebarEntry {
    /// A synced row as an entry; `None` = an unknown kind.
    pub fn from_row(row: &SidebarLayoutRow) -> Option<Self> {
        Some(Self {
            id: row.id.clone(),
            kind: SidebarEntryKind::parse(row.kind.as_deref()?)?,
            target_id: row.target_id.clone(),
            parent_id: row.parent_id.clone(),
            name: row.name.clone(),
            sort_order: row.sort_order.unwrap_or(0.0),
        })
    }
}

/// The entries of one team's section among the synced rows (any order).
pub fn sidebar_entries<'a>(
    rows: impl IntoIterator<Item = &'a SidebarLayoutRow>,
    team_id: &str,
    section: SidebarSection,
) -> Vec<SidebarEntry> {
    rows.into_iter()
        .filter(|row| row.team_id.as_deref() == Some(team_id) && row.section.as_deref() == Some(section.as_str()))
        .filter_map(SidebarEntry::from_row)
        .collect()
}

/// One node of the section as the sidebar shows it. An item's `id` is the
/// target id; a folder's is its entry id and its `children` are target ids
/// in order (never empty).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SidebarNode {
    Item { id: String },
    Folder { id: String, name: String, children: Vec<String> },
}

impl SidebarNode {
    pub fn id(&self) -> &str {
        match self {
            SidebarNode::Item { id } | SidebarNode::Folder { id, .. } => id,
        }
    }

    fn is_folder(&self) -> bool {
        matches!(self, SidebarNode::Folder { .. })
    }
}

/// A List `move` event: `id` lands before / after / into the named node
/// (exactly one of the three). Ids are node ids: a target id for an item,
/// the entry id for a folder.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SidebarMove {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub into: Option<String>,
}

impl SidebarMove {
    /// A genUI List `move` event's context `{id, before | after | into}`;
    /// `None` = no `id`. A non-string position counts as absent.
    pub fn from_context(context: &Value) -> Option<Self> {
        let field = |key: &str| context.get(key).and_then(Value::as_str).map(str::to_string);
        Some(Self { id: field("id")?, before: field("before"), after: field("after"), into: field("into") })
    }
}

/// One row to write. `id` `None` = a new ITEM entry (the server upserts it
/// by (user, section, target)); a new folder carries the caller's
/// `new_folder_id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarEntryWrite {
    pub id: Option<String>,
    pub kind: SidebarEntryKind,
    pub target_id: Option<String>,
    pub parent_id: Option<String>,
    pub name: Option<String>,
    #[serde(serialize_with = "number")]
    pub sort_order: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarWrites {
    pub upserts: Vec<SidebarEntryWrite>,
    /// Entry ids to delete (folders that became empty / were ungrouped).
    pub deletes: Vec<String>,
    /// The folder this move created: the client edits its name in place.
    pub created_folder_id: Option<String>,
}

fn compare_entries(a: &SidebarEntry, b: &SidebarEntry) -> Ordering {
    match a.sort_order.partial_cmp(&b.sort_order) {
        Some(Ordering::Equal) | None => a.id.cmp(&b.id),
        Some(order) => order,
    }
}

fn sorted(entries: &[SidebarEntry]) -> Vec<&SidebarEntry> {
    let mut out: Vec<&SidebarEntry> = entries.iter().collect();
    out.sort_by(|a, b| compare_entries(a, b));
    out
}

/// The section as the sidebar shows it. Placed nodes (folders and items
/// with an entry) first by `sort_order` (ties by entry id), then every item
/// with no entry in default order. An entry whose target is not in `items`
/// is skipped; an item whose `parent_id` names no folder sits at the top
/// level; a folder's `parent_id` is ignored (two layers max); a folder with
/// no visible item is not shown.
pub fn sidebar_tree(items: &[String], entries: &[SidebarEntry]) -> Vec<SidebarNode> {
    let visible: HashSet<&str> = items.iter().map(String::as_str).collect();
    // Insertion-ordered maps (a JS `Map`): a later folder with the same id
    // replaces the entry in place.
    let mut folders: Vec<&SidebarEntry> = Vec::new();
    let mut folder_index: HashMap<&str, usize> = HashMap::new();
    let mut item_entries: Vec<&SidebarEntry> = Vec::new();
    let mut placed: HashSet<&str> = HashSet::new();
    for entry in sorted(entries) {
        match entry.kind {
            SidebarEntryKind::Folder => match folder_index.get(entry.id.as_str()) {
                Some(&at) => folders[at] = entry,
                None => {
                    folder_index.insert(entry.id.as_str(), folders.len());
                    folders.push(entry);
                }
            },
            SidebarEntryKind::Item => {
                if let Some(target) = entry.target_id.as_deref() {
                    if visible.contains(target) && placed.insert(target) {
                        item_entries.push(entry);
                    }
                }
            }
        }
    }
    let mut children: HashMap<&str, Vec<&SidebarEntry>> = HashMap::new();
    let mut top: Vec<&SidebarEntry> = Vec::new();
    for entry in item_entries.iter().copied() {
        match entry.parent_id.as_deref() {
            Some(parent) if folder_index.contains_key(parent) => children.entry(parent).or_default().push(entry),
            _ => top.push(entry),
        }
    }
    for folder in folders.iter().copied() {
        if children.contains_key(folder.id.as_str()) {
            top.push(folder);
        }
    }
    top.sort_by(|a, b| compare_entries(a, b));
    let mut nodes: Vec<SidebarNode> = top
        .into_iter()
        .map(|entry| match entry.kind {
            SidebarEntryKind::Folder => {
                let mut list = children.remove(entry.id.as_str()).unwrap_or_default();
                list.sort_by(|a, b| compare_entries(a, b));
                SidebarNode::Folder {
                    id: entry.id.clone(),
                    name: entry.name.clone().unwrap_or_else(|| SIDEBAR_FOLDER_DEFAULT_NAME.to_string()),
                    children: list.into_iter().filter_map(|child| child.target_id.clone()).collect(),
                }
            }
            SidebarEntryKind::Item => SidebarNode::Item { id: entry.target_id.clone().unwrap_or_default() },
        })
        .collect();
    for item in items {
        if !placed.contains(item.as_str()) {
            nodes.push(SidebarNode::Item { id: item.clone() });
        }
    }
    nodes
}

/// Where a node sits: the holding folder's id (`None` = top level) and
/// whether it is a folder.
struct Located {
    folder: Option<String>,
    is_folder: bool,
}

fn locate(tree: &[SidebarNode], id: &str) -> Option<Located> {
    for node in tree {
        if node.id() == id {
            return Some(Located { folder: None, is_folder: node.is_folder() });
        }
        if let SidebarNode::Folder { id: folder_id, children, .. } = node {
            if children.iter().any(|child| child == id) {
                return Some(Located { folder: Some(folder_id.clone()), is_folder: false });
            }
        }
    }
    None
}

/// The children of the first folder node `folder_id` names.
fn folder_children<'a>(tree: &'a mut [SidebarNode], folder_id: &str) -> Option<&'a mut Vec<String>> {
    tree.iter_mut().find_map(|node| match node {
        SidebarNode::Folder { id, children, .. } if id.as_str() == folder_id => Some(children),
        _ => None,
    })
}

fn top_index(tree: &[SidebarNode], id: &str) -> Option<usize> {
    tree.iter().position(|node| node.id() == id)
}

/// Writes the tree as rows: every node's `sort_order` = its 1-based index in
/// its container; only rows that differ from their entry (or have none).
fn materialize(
    tree: &[SidebarNode],
    entries: &[SidebarEntry],
    deletes: Vec<String>,
    created_folder_id: Option<String>,
) -> SidebarWrites {
    let mut by_id: HashMap<&str, &SidebarEntry> = HashMap::new();
    for entry in entries {
        by_id.insert(entry.id.as_str(), entry);
    }
    let mut by_target: HashMap<&str, &SidebarEntry> = HashMap::new();
    for entry in sorted(entries) {
        if entry.kind == SidebarEntryKind::Item {
            if let Some(target) = entry.target_id.as_deref() {
                by_target.entry(target).or_insert(entry);
            }
        }
    }
    let mut upserts: Vec<SidebarEntryWrite> = Vec::new();
    let write_item = |upserts: &mut Vec<SidebarEntryWrite>, target: &str, parent: Option<&str>, sort_order: f64| {
        let entry = by_target.get(target);
        if let Some(entry) = entry {
            if entry.parent_id.as_deref() == parent && entry.sort_order == sort_order {
                return;
            }
        }
        upserts.push(SidebarEntryWrite {
            id: entry.map(|entry| entry.id.clone()),
            kind: SidebarEntryKind::Item,
            target_id: Some(target.to_string()),
            parent_id: parent.map(str::to_string),
            name: None,
            sort_order,
        });
    };
    for (index, node) in tree.iter().enumerate() {
        let sort_order = (index + 1) as f64;
        match node {
            SidebarNode::Item { id } => write_item(&mut upserts, id.as_str(), None, sort_order),
            SidebarNode::Folder { id, name, children } => {
                let entry = by_id.get(id.as_str());
                let same = entry.is_some_and(|entry| {
                    entry.sort_order == sort_order && entry.name.as_deref() == Some(name.as_str()) && entry.parent_id.is_none()
                });
                if !same {
                    upserts.push(SidebarEntryWrite {
                        id: Some(id.clone()),
                        kind: SidebarEntryKind::Folder,
                        target_id: None,
                        parent_id: None,
                        name: Some(name.clone()),
                        sort_order,
                    });
                }
                for (child_index, child) in children.iter().enumerate() {
                    write_item(&mut upserts, child.as_str(), Some(id.as_str()), (child_index + 1) as f64);
                }
            }
        }
    }
    SidebarWrites { upserts, deletes, created_folder_id }
}

/// Applies a List `move` to the section. `None` = not allowed (an unknown
/// id, a node onto itself, not exactly one position, a folder into anything
/// or beside a folder's item, an item into an item that sits in a folder).
/// `before`/`after` = beside the target in its container; `into` a folder =
/// appended to it; `into` an item = a NEW folder (`new_folder_id`, the
/// default name) at the item's place holding the item then the dragged one.
/// A folder the move empties is deleted.
pub fn sidebar_move(items: &[String], entries: &[SidebarEntry], mv: &SidebarMove, new_folder_id: &str) -> Option<SidebarWrites> {
    #[derive(PartialEq, Clone, Copy)]
    enum Position {
        Before,
        After,
        Into,
    }
    let positions: Vec<(Position, &String)> = [(Position::Before, &mv.before), (Position::After, &mv.after), (Position::Into, &mv.into)]
        .into_iter()
        .filter_map(|(position, target)| target.as_ref().map(|target| (position, target)))
        .collect();
    let [(position, target_id)] = positions.as_slice() else {
        return None;
    };
    let (position, target_id) = (*position, target_id.as_str());
    if target_id == mv.id {
        return None;
    }
    let mut tree = sidebar_tree(items, entries);
    let dragged = locate(&tree, &mv.id)?;
    let target = locate(&tree, target_id)?;
    if dragged.is_folder && (position == Position::Into || target.folder.is_some()) {
        return None;
    }
    if position == Position::Into && !target.is_folder && target.folder.is_some() {
        return None;
    }

    // Take the dragged node out.
    let dragged_node = match &dragged.folder {
        Some(source) => {
            let list = folder_children(&mut tree, source)?;
            let at = list.iter().position(|child| *child == mv.id)?;
            list.remove(at);
            SidebarNode::Item { id: mv.id.clone() }
        }
        None => {
            let at = top_index(&tree, &mv.id)?;
            let node = tree.remove(at);
            if dragged.is_folder {
                node
            } else {
                SidebarNode::Item { id: mv.id.clone() }
            }
        }
    };

    let mut created_folder_id = None;
    if position == Position::Into {
        if target.is_folder {
            folder_children(&mut tree, target_id)?.push(mv.id.clone());
        } else {
            created_folder_id = Some(new_folder_id.to_string());
            let at = top_index(&tree, target_id)?;
            tree[at] = SidebarNode::Folder {
                id: new_folder_id.to_string(),
                name: SIDEBAR_FOLDER_DEFAULT_NAME.to_string(),
                children: vec![target_id.to_string(), mv.id.clone()],
            };
        }
    } else {
        let offset = usize::from(position == Position::After);
        match &target.folder {
            Some(folder) => {
                let list = folder_children(&mut tree, folder)?;
                let at = list.iter().position(|child| child == target_id)?;
                list.insert(at + offset, mv.id.clone());
            }
            None => {
                let at = top_index(&tree, target_id)?;
                tree.insert(at + offset, dragged_node);
            }
        }
    }

    let mut deletes = Vec::new();
    if let Some(source) = &dragged.folder {
        let at = tree.iter().position(|node| matches!(node, SidebarNode::Folder { id, .. } if id == source));
        if let Some(at) = at {
            if matches!(&tree[at], SidebarNode::Folder { children, .. } if children.is_empty()) {
                tree.remove(at);
                deletes.push(source.clone());
            }
        }
    }
    Some(materialize(&tree, entries, deletes, created_folder_id))
}

/// Ungroup: the folder's items take its place at the top level, in order,
/// and the folder is deleted. `None` = no such (visible) folder.
pub fn sidebar_ungroup(items: &[String], entries: &[SidebarEntry], folder_id: &str) -> Option<SidebarWrites> {
    let mut tree = sidebar_tree(items, entries);
    let at = tree.iter().position(|node| matches!(node, SidebarNode::Folder { id, .. } if id == folder_id))?;
    let SidebarNode::Folder { children, .. } = tree.remove(at) else {
        return None;
    };
    for (offset, child) in children.into_iter().enumerate() {
        tree.insert(at + offset, SidebarNode::Item { id: child });
    }
    Some(materialize(&tree, entries, vec![folder_id.to_string()], None))
}

/// A folder name as saved: trimmed, at most 80 UTF-16 code units (JS
/// `slice`; a pair the cut would split is dropped whole); empty = the default
/// (Esc / an emptied field keeps "New folder").
pub fn sidebar_folder_name(input: &str) -> String {
    let trimmed = input.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    let mut units = 0;
    let name: String = trimmed
        .chars()
        .take_while(|c| {
            units += c.len_utf16();
            units <= SIDEBAR_FOLDER_NAME_MAX
        })
        .collect();
    if name.is_empty() {
        SIDEBAR_FOLDER_DEFAULT_NAME.to_string()
    } else {
        name
    }
}

/// The device-local expanded flag's key (the IDE settings file); a folder
/// with no stored flag is OPEN.
pub fn sidebar_folder_open_key(team_id: &str, folder_id: &str) -> String {
    format!("sidebar-folder-open:{team_id}:{folder_id}")
}

/// One List row: what the List template gets. A folder row carries
/// `group: true` and its item count; a closed folder's items are left out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SidebarRow {
    pub kind: SidebarEntryKind,
    /// Node id (target id for an item, entry id for a folder).
    pub id: String,
    /// 0 | 1.
    pub depth: u8,
    pub group: bool,
    /// Folders only.
    pub name: Option<String>,
    pub open: bool,
    pub count: usize,
}

pub fn sidebar_rows(tree: &[SidebarNode], is_open: impl Fn(&str) -> bool) -> Vec<SidebarRow> {
    let item = |id: &str, depth: u8| SidebarRow {
        kind: SidebarEntryKind::Item,
        id: id.to_string(),
        depth,
        group: false,
        name: None,
        open: false,
        count: 0,
    };
    let mut rows = Vec::new();
    for node in tree {
        match node {
            SidebarNode::Item { id } => rows.push(item(id, 0)),
            SidebarNode::Folder { id, name, children } => {
                let open = is_open(id);
                rows.push(SidebarRow {
                    kind: SidebarEntryKind::Folder,
                    id: id.clone(),
                    depth: 0,
                    group: true,
                    name: Some(name.clone()),
                    open,
                    count: children.len(),
                });
                if open {
                    rows.extend(children.iter().map(|child| item(child, 1)));
                }
            }
        }
    }
    rows
}
