//! #EXP-2109: THE header actions, the Rust twin of
//! `packages/domain-contract/src/header-actions.ts` (fixture
//! `header-actions.json`, whose `$comment` is the spec; replayed whole by
//! `tests/header_actions.rs`): one order, one round button, + never inside
//! the `…`.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/header-actions.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HeaderAction {
    Status,
    Primary,
    Call,
    Page,
    Search,
    Inbox,
    Add,
    More,
}

/// A page's actions on each form; `None` = no header there.
#[derive(Debug, Clone, Deserialize)]
pub struct HeaderPage {
    pub phone: Option<Vec<HeaderAction>>,
    pub wide: Option<Vec<HeaderAction>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaderActions {
    /// Phones: the circle and its glyph.
    pub circle: f32,
    pub glyph: f32,
    /// Web md+ and the IDE.
    pub wide_circle: f32,
    pub wide_glyph: f32,
    pub gap: f32,
    pub order: Vec<HeaderAction>,
    pub icons: BTreeMap<HeaderAction, String>,
    pub adds: BTreeMap<String, String>,
    pub rows: BTreeMap<String, Vec<HeaderAction>>,
    pub menu_drops: Vec<String>,
    pub pages: BTreeMap<String, HeaderPage>,
}

pub static HEADER_ACTIONS: LazyLock<HeaderActions> = LazyLock::new(|| serde_json::from_str(FIXTURE).expect("header-actions.json"));

/// `actions` in THE order.
pub fn order_header_actions(actions: &[HeaderAction]) -> Vec<HeaderAction> {
    HEADER_ACTIONS.order.iter().copied().filter(|action| actions.contains(action)).collect()
}

/// A page's actions in order on a wide header (web md+, the IDE); `None` =
/// the page has no header there (or is unknown).
pub fn header_actions_wide(page: &str) -> Option<Vec<HeaderAction>> {
    HEADER_ACTIONS.pages.get(page)?.wide.as_deref().map(order_header_actions)
}

/// A page's actions in order on a phone header.
pub fn header_actions_phone(page: &str) -> Option<Vec<HeaderAction>> {
    HEADER_ACTIONS.pages.get(page)?.phone.as_deref().map(order_header_actions)
}

/// Whether the page's + does this menu entry (`menuDrops`).
pub fn header_menu_drops(entry: &str) -> bool {
    HEADER_ACTIONS.menu_drops.iter().any(|drop| drop == entry)
}

/// The `…` of a header: the page's menu (`id` names an entry) without what
/// the + does (and Open on the page itself), separators tidied; empty = no
/// button.
pub fn header_menu_by<T: Clone>(entries: &[T], id: impl Fn(&T) -> &str) -> Vec<T> {
    const SEPARATOR: &str = "separator";
    let kept: Vec<&T> = entries.iter().filter(|entry| !header_menu_drops(id(entry))).collect();
    kept.iter()
        .enumerate()
        .filter(|(ix, entry)| id(entry) != SEPARATOR || (*ix > 0 && ix + 1 < kept.len() && id(kept[ix - 1]) != SEPARATOR))
        .map(|(_, entry)| (*entry).clone())
        .collect()
}

pub fn header_menu<'a>(entries: &[&'a str]) -> Vec<&'a str> {
    header_menu_by(entries, |entry| entry)
}
