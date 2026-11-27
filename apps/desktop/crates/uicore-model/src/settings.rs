//! #EDE-273: System Settings on Exponential Desktop (Mint's
//! cinnamon-settings): the module catalogue, the grid's live search and the
//! window's navigation (open, back, Enter, `~/settings/<module>` links).
//! Locked by `packages/uicore/fixtures/system-settings.json` (its `$comment`
//! is the spec; `settingsGrid` and `settingsNavigate` in
//! `packages/uicore/src/settings.ts`).

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

const SOURCE: &str = include_str!("../../../../../packages/uicore/fixtures/system-settings.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SettingsKind {
    Module,
    Later,
    Launch,
}

/// One module of the catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsModule {
    pub id: String,
    pub name: String,
    pub category: String,
    pub icon: String,
    pub keywords: String,
    pub kind: SettingsKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opens: Option<String>,
    /// #EDE-274: the module's stack switcher (its pages, in order).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tabs: Vec<SettingsTab>,
}

/// One page of a module's stack switcher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsTab {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsCategoryDef {
    pub id: String,
    pub title: String,
}

#[derive(Deserialize)]
struct Catalogue {
    title: String,
    categories: Vec<SettingsCategoryDef>,
    modules: Vec<SettingsModule>,
    aliases: BTreeMap<String, String>,
}

fn catalogue() -> &'static Catalogue {
    static C: OnceLock<Catalogue> = OnceLock::new();
    C.get_or_init(|| serde_json::from_str(SOURCE).expect("system-settings.json"))
}

/// The window's title on the grid.
pub fn settings_title() -> &'static str {
    &catalogue().title
}

/// The catalogue, in the grid's order.
pub fn settings_modules() -> &'static [SettingsModule] {
    &catalogue().modules
}

/// A tile of the grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsTile {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub kind: SettingsKind,
    pub later: bool,
    pub installed: bool,
}

/// One section of the grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsCategory {
    pub id: String,
    pub title: String,
    pub modules: Vec<SettingsTile>,
}

/// The window's place: `module` empty = the grid; `query` as typed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsState {
    pub module: String,
    pub query: String,
    /// #EDE-274: the open module's page of its stack (`` without one).
    #[serde(default)]
    pub tab: String,
}

/// What the window was asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SettingsEvent {
    Open { id: String },
    Back,
    Query { text: String },
    Submit,
    Link { target: String },
    /// #EDE-274: the stack switcher.
    Tab { value: String },
    #[serde(other)]
    Other,
}

/// One step: the new place, a command to launch, the window's title.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsNavigation {
    pub state: SettingsState,
    pub launch: Option<String>,
    pub title: String,
}

fn matches(m: &SettingsModule, q: &str) -> bool {
    m.name.to_lowercase().contains(q) || m.keywords.to_lowercase().contains(q)
}

/// The grid for `query`; `installed` = the launch modules whose tool the host finds.
pub fn settings_grid(query: &str, installed: &[String]) -> Vec<SettingsCategory> {
    let q = query.trim().to_lowercase();
    let c = catalogue();
    c.categories
        .iter()
        .map(|cat| SettingsCategory {
            id: cat.id.clone(),
            title: cat.title.clone(),
            modules: c
                .modules
                .iter()
                .filter(|m| m.category == cat.id && (q.is_empty() || matches(m, &q)))
                .map(|m| SettingsTile {
                    id: m.id.clone(),
                    name: m.name.clone(),
                    icon: m.icon.clone(),
                    kind: m.kind,
                    later: m.kind == SettingsKind::Later,
                    installed: m.kind != SettingsKind::Launch || installed.contains(&m.id),
                })
                .collect(),
        })
        .filter(|cat| !cat.modules.is_empty())
        .collect()
}

fn module(id: &str) -> Option<&'static SettingsModule> {
    settings_modules().iter().find(|m| m.id == id)
}

fn resolve(id: &str) -> Option<&'static SettingsModule> {
    let m = module(id)?;
    match &m.opens {
        Some(target) => module(target),
        None => Some(m),
    }
}

fn result(state: SettingsState, launch: Option<String>) -> SettingsNavigation {
    let title = module(&state.module).map(|m| m.name.clone()).unwrap_or_else(|| settings_title().to_string());
    SettingsNavigation { state, launch, title }
}

fn open(state: SettingsState, id: &str, installed: &[String]) -> SettingsNavigation {
    let Some(m) = resolve(id) else { return result(state, None) };
    if m.kind == SettingsKind::Launch && installed.contains(&m.id) {
        return result(state, m.command.clone());
    }
    result(SettingsState { module: m.id.clone(), query: state.query, tab: first_tab(m) }, None)
}

fn first_tab(m: &SettingsModule) -> String {
    m.tabs.first().map(|t| t.value.clone()).unwrap_or_default()
}

/// One navigation step of the window.
pub fn settings_navigate(state: &SettingsState, event: &SettingsEvent, installed: &[String]) -> SettingsNavigation {
    let s = state.clone();
    match event {
        SettingsEvent::Open { id } => open(s, id, installed),
        SettingsEvent::Back => result(SettingsState { module: String::new(), tab: String::new(), ..s }, None),
        SettingsEvent::Query { text } => {
            if s.module.is_empty() {
                result(SettingsState { query: text.clone(), ..s }, None)
            } else {
                result(s, None)
            }
        }
        SettingsEvent::Submit => {
            if !s.module.is_empty() || s.query.trim().is_empty() {
                return result(s, None);
            }
            match settings_grid(&s.query, installed).first().and_then(|c| c.modules.first()).map(|t| t.id.clone()) {
                Some(first) => open(s, &first, installed),
                None => result(s, None),
            }
        }
        SettingsEvent::Link { target } => {
            let t = target.strip_prefix("~/settings").unwrap_or(target).trim_matches('/');
            let t = catalogue().aliases.get(t).map(String::as_str).unwrap_or(t);
            let grid = SettingsState::default();
            let Some(m) = (!t.is_empty()).then(|| resolve(t)).flatten() else { return result(grid, None) };
            if m.kind == SettingsKind::Launch && installed.contains(&m.id) {
                return result(grid, m.command.clone());
            }
            result(SettingsState { module: m.id.clone(), query: String::new(), tab: first_tab(m) }, None)
        }
        SettingsEvent::Tab { value } => match module(&s.module) {
            Some(m) if m.tabs.iter().any(|t| &t.value == value) => result(SettingsState { tab: value.clone(), ..s }, None),
            _ => result(s, None),
        },
        SettingsEvent::Other => result(s, None),
    }
}
