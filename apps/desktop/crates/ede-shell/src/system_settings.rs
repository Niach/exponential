//! #EDE-273 (#EDE-241 P6.1): System Settings, Mint's cinnamon-settings: ONE
//! window of the shell (`settings.express`, a `window` surface: eDE's frame,
//! its own taskbar entry) with the category grid, the live search and the
//! modules in the same window. What it shows is uicore's
//! `system-settings.json` (the catalogue, `settings_grid`,
//! `settings_navigate`); this keeps the window's place and feeds `/settings`.

use serde_json::{json, Value};
use uicore_model::settings::{settings_grid, settings_modules, settings_navigate, SettingsEvent, SettingsKind, SettingsNavigation, SettingsState};

/// The settings window's document (and the start menu's own entry's id).
pub const DOCUMENT: &str = "settings";

/// The window's app id: the start menu's System Settings entry, so the
/// taskbar names the window's button.
pub const APP_ID: &str = "at.exponential.settings";

/// The window's place and the tools found on this machine.
#[derive(Debug, Clone, Default)]
pub struct Nav {
    pub state: SettingsState,
    /// The launch modules whose tool is installed (Mint's root tools, the printers' dialog).
    pub installed: Vec<String>,
}

impl Nav {
    /// The launch modules whose command is on `PATH` (the goldens: none, so
    /// a capture never starts a tool of the machine it runs on).
    pub fn probe(demo: bool) -> Self {
        let installed = settings_modules()
            .iter()
            .filter(|m| m.kind == SettingsKind::Launch)
            .filter(|m| !demo && m.command.as_deref().and_then(|c| c.split_whitespace().next()).is_some_and(on_path))
            .map(|m| m.id.clone())
            .collect();
        Self { state: SettingsState::default(), installed }
    }

    /// One step; the new place is kept, a command to launch handed back.
    pub fn step(&mut self, event: &SettingsEvent) -> SettingsNavigation {
        let next = settings_navigate(&self.state, event, &self.installed);
        self.state = next.state.clone();
        next
    }

    /// #EDE-275: the holder can open its pages (an account is signed in):
    /// the Exponential category's modules open the holder's windows, else
    /// their page in the window says Connect Exponential.
    pub fn set_holder(&mut self, signed_in: bool) {
        let pages: Vec<String> = settings_modules().iter().filter(|m| m.kind == SettingsKind::Page).map(|m| m.id.clone()).collect();
        self.installed.retain(|id| !pages.contains(id));
        if signed_in {
            self.installed.extend(pages);
        }
    }

    /// The window starts again on the grid (it was closed).
    pub fn reset(&mut self) {
        self.state = SettingsState::default();
    }

    /// `/settings`: the place, the window's title, the grid's categories by
    /// id (each `{title, modules}`; a category the search left out has none).
    pub fn feed(&self) -> Value {
        let nav = settings_navigate(&self.state, &SettingsEvent::Other, &self.installed);
        let open = settings_modules().iter().find(|m| m.id == self.state.module);
        let kind = open.map(|m| match m.kind {
            SettingsKind::Module => "module",
            SettingsKind::Later => "later",
            SettingsKind::Launch => "missing",
            SettingsKind::Page => "signedOut",
        });
        let mut grid = serde_json::Map::new();
        for c in ["appearance", "preferences", "hardware", "exponential", "administration"] {
            grid.insert(c.into(), json!({"modules": []}));
        }
        for category in settings_grid(&self.state.query, &self.installed) {
            grid.insert(category.id.clone(), json!({"title": category.title, "modules": category.modules}));
        }
        let command = open.and_then(|m| m.command.clone()).unwrap_or_default();
        json!({
            "module": self.state.module,
            "query": self.state.query,
            "tab": self.state.tab,
            "title": nav.title,
            "inModule": !self.state.module.is_empty(),
            "kind": kind.unwrap_or(""),
            "command": command,
            "empty": !self.state.query.trim().is_empty() && grid.values().all(|c| c["modules"].as_array().is_some_and(Vec::is_empty)),
            "grid": grid,
        })
    }
}

fn on_path(bin: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
}

/// `ede_settings_nav {action: open|back|query|submit|link|tab, module?, query?, target?, tab?}`.
pub fn event_of(args: &Value) -> Option<SettingsEvent> {
    let text = |k: &str| args.get(k).and_then(Value::as_str).map(String::from);
    Some(match args.get("action").and_then(Value::as_str)? {
        "open" => SettingsEvent::Open { id: text("module")? },
        "back" => SettingsEvent::Back,
        "query" => SettingsEvent::Query { text: text("query").unwrap_or_default() },
        "submit" => SettingsEvent::Submit,
        "link" => SettingsEvent::Link { target: text("target").unwrap_or_default() },
        "tab" => SettingsEvent::Tab { value: text("tab")? },
        _ => return None,
    })
}

/// #EDE-275: a holder page as the bus asks for it: an in-app link
/// (`~/settings/…`) is `open {link}`, a window key (`device:`) `page {key}`.
pub fn holder_request(page: &str, token: Option<String>) -> api::bus::Request {
    if page.starts_with("~/") {
        api::bus::Request::Open { link: Some(page.to_string()), activation_token: token }
    } else {
        api::bus::Request::Page { key: page.to_string(), activation_token: token }
    }
}

/// Alt+Left in the window: back, as cinnamon-settings' back button.
pub fn is_back_key(key: &str, alt: bool, others: bool) -> bool {
    key == "left" && alt && !others
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_feed_lists_every_category_and_the_place() {
        let mut nav = Nav { installed: vec!["printers".into()], ..Nav::default() };
        let feed = nav.feed();
        assert_eq!(feed["title"], "System Settings");
        assert_eq!(feed["inModule"], false);
        assert!(feed["grid"]["hardware"]["modules"].as_array().unwrap().iter().any(|m| m["id"] == "sound"));
        nav.step(&SettingsEvent::Query { text: "zzz".into() });
        let feed = nav.feed();
        assert_eq!(feed["grid"]["hardware"]["modules"], json!([]), "a category the search leaves out has none");
        assert_eq!(feed["empty"], true);
        nav.step(&SettingsEvent::Link { target: "~/settings/effects".into() });
        assert_eq!((nav.feed()["kind"].as_str(), nav.feed()["title"].as_str()), (Some("later"), Some("Effects")));
        nav.step(&SettingsEvent::Open { id: "firewall".into() });
        assert_eq!(nav.feed()["kind"], "missing", "a launch module without its tool");
        assert_eq!(nav.feed()["command"], "gufw");
        nav.set_holder(false);
        nav.step(&SettingsEvent::Open { id: "agents".into() });
        assert_eq!(nav.feed()["kind"], "signedOut", "signed out: Connect Exponential in the window");
        nav.set_holder(true);
        let opened = nav.step(&SettingsEvent::Link { target: "~/settings/account".into() });
        assert_eq!((opened.page.as_deref(), nav.feed()["inModule"].as_bool()), (Some("~/settings/account"), Some(false)), "signed in: the holder's window, the grid here");
        nav.set_holder(false);
        assert!(!nav.installed.iter().any(|id| id == "account"));
    }

    #[test]
    fn the_host_function_reads_its_actions() {
        assert_eq!(event_of(&json!({"action": "open", "module": "sound"})), Some(SettingsEvent::Open { id: "sound".into() }));
        assert_eq!(event_of(&json!({"action": "query", "query": "blu"})), Some(SettingsEvent::Query { text: "blu".into() }));
        assert_eq!(event_of(&json!({"action": "open"})), None);
        assert_eq!(event_of(&json!({"action": "tab", "tab": "input"})), Some(SettingsEvent::Tab { value: "input".into() }));
        assert_eq!(event_of(&json!({"action": "nope"})), None);
        assert_eq!(holder_request("~/settings/voice", None), api::bus::Request::Open { link: Some("~/settings/voice".into()), activation_token: None });
        assert_eq!(holder_request("device:", Some("t".into())), api::bus::Request::Page { key: "device:".into(), activation_token: Some("t".into()) });
        assert!(is_back_key("left", true, false));
        assert!(!is_back_key("left", false, false));
        assert!(!is_back_key("left", true, true));
    }
}
