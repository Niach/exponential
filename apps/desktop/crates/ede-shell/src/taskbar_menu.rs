//! #EDE-254: what a taskbar group's menu needs beyond the shell model
//! (`uicore_model::taskbar_menu`, shell.json): the app's recent files
//! (GtkRecentManager's `recently-used.xbel`, the files whose type the app
//! is the default for, most recent first, as grouped-window-list's Recent)
//! and its desktop entry's Actions.

use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::Mutex,
    time::SystemTime,
};

use uicore_model::{DesktopAction, RecentFile};

/// One `<bookmark>` of `recently-used.xbel`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bookmark {
    pub uri: String,
    pub mime: String,
    pub modified: String,
}

/// `$XDG_DATA_HOME/recently-used.xbel` (GTK's default recent file).
fn xbel_path() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .map(|d| d.join("recently-used.xbel"))
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let at = tag.find(&format!("{name}=\""))? + name.len() + 2;
    let end = tag[at..].find('"')? + at;
    Some(unescape(&tag[at..end]))
}

fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

/// The bookmarks of an xbel document, most recently modified first (ISO
/// times compare as text).
pub fn parse_xbel(text: &str) -> Vec<Bookmark> {
    let mut out: Vec<Bookmark> = text
        .split("<bookmark ")
        .skip(1)
        .filter_map(|chunk| {
            let head = &chunk[..chunk.find('>')?];
            let uri = attr(head, "href")?;
            let modified = attr(head, "modified").unwrap_or_default();
            let body = &chunk[..chunk.find("</bookmark>").unwrap_or(chunk.len())];
            let mime = body.find("<mime:mime-type ").and_then(|at| attr(&body[at..at + body[at..].find('>')?], "type")).unwrap_or_default();
            Some(Bookmark { uri, mime, modified })
        })
        .collect();
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    out
}

/// GtkRecentInfo's short name: the URI's last segment, percent-decoded.
pub fn short_name(uri: &str) -> String {
    let tail = uri.trim_end_matches('/').rsplit('/').next().unwrap_or(uri);
    let bytes = tail.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Some(b) = std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The default application of each MIME type: `mimeapps.list`'s pick,
/// else the first installed entry (by id) naming the type.
fn default_for(mime: &str, defaults: &BTreeMap<String, String>, entries: &[(String, crate::settings_pages::DesktopFile)]) -> Option<String> {
    defaults.get(mime).cloned().or_else(|| entries.iter().find(|(_, d)| d.mime.iter().any(|m| m == mime)).map(|(id, _)| id.clone()))
}

/// The recent files whose default application is `entry_id`
/// (`org.gnome.TextEditor.desktop`), most recent first.
pub fn recent_of(bookmarks: &[Bookmark], entry_id: &str, defaults: &BTreeMap<String, String>, entries: &[(String, crate::settings_pages::DesktopFile)]) -> Vec<RecentFile> {
    let mut cache: HashMap<&str, Option<String>> = HashMap::new();
    bookmarks
        .iter()
        .filter(|b| !b.mime.is_empty())
        .filter(|b| cache.entry(b.mime.as_str()).or_insert_with(|| default_for(&b.mime, defaults, entries)).as_deref() == Some(entry_id))
        .take(uicore_model::TASKBAR_RECENT_MAX)
        .map(|b| RecentFile { name: short_name(&b.uri), uri: b.uri.clone() })
        .collect()
}

struct RecentCache {
    stamp: Option<SystemTime>,
    bookmarks: Vec<Bookmark>,
    defaults: BTreeMap<String, String>,
    entries: Vec<(String, crate::settings_pages::DesktopFile)>,
}

static RECENT: Mutex<Option<RecentCache>> = Mutex::new(None);

/// `entry_id`'s recent files, read again when the xbel file changed.
pub fn recent(entry_id: &str) -> Vec<RecentFile> {
    let Some(path) = xbel_path() else { return Vec::new() };
    let stamp = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    let mut cache = RECENT.lock().unwrap_or_else(|p| p.into_inner());
    if cache.as_ref().is_none_or(|c| c.stamp != stamp) {
        let bookmarks = std::fs::read_to_string(&path).map(|t| parse_xbel(&t)).unwrap_or_default();
        *cache = Some(RecentCache { stamp, bookmarks, defaults: crate::settings_pages::mimeapps_defaults(), entries: crate::settings_pages::desktop_entries(&uicore_data::apps::data_dirs()) });
    }
    let c = cache.as_ref().expect("filled above");
    recent_of(&c.bookmarks, entry_id, &c.defaults, &c.entries)
}

/// One `[Desktop Action <id>]` of an entry, with its command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub id: String,
    pub name: String,
    pub exec: String,
}

/// The entry's `Actions=` in its order, each with its group's `Name` and
/// `Exec` (field codes stripped); an action without both is left out.
pub fn parse_actions(text: &str) -> Vec<Action> {
    let mut listed: Vec<String> = Vec::new();
    let mut groups: HashMap<String, (String, String)> = HashMap::new();
    let mut section = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].to_string();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let (key, value) = (key.trim(), value.trim());
        if section == "Desktop Entry" && key == "Actions" {
            listed = value.split(';').filter(|a| !a.is_empty()).map(String::from).collect();
        } else if let Some(id) = section.strip_prefix("Desktop Action ") {
            let g = groups.entry(id.to_string()).or_default();
            match key {
                "Name" => g.0 = value.to_string(),
                "Exec" => g.1 = uicore_data::apps::strip_field_codes(value),
                _ => {}
            }
        }
    }
    listed.into_iter().filter_map(|id| groups.get(&id).filter(|(n, e)| !n.is_empty() && !e.is_empty()).map(|(name, exec)| Action { id: id.clone(), name: name.clone(), exec: exec.clone() })).collect()
}

static ACTIONS: Mutex<Option<HashMap<String, Vec<Action>>>> = Mutex::new(None);

/// The actions of desktop entry `entry_id`, read once per entry.
pub fn actions(entry_id: &str) -> Vec<Action> {
    let mut cache = ACTIONS.lock().unwrap_or_else(|p| p.into_inner());
    let map = cache.get_or_insert_with(HashMap::new);
    map.entry(entry_id.to_string())
        .or_insert_with(|| {
            uicore_data::apps::data_dirs().into_iter().find_map(|d| std::fs::read_to_string(d.join("applications").join(entry_id)).ok()).map(|t| parse_actions(&t)).unwrap_or_default()
        })
        .clone()
}

/// The actions as the menu selector takes them.
pub fn menu_actions(actions: &[Action]) -> Vec<DesktopAction> {
    actions.iter().map(|a| DesktopAction { id: a.id.clone(), name: a.name.clone() }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const XBEL: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xbel version="1.0">
  <bookmark href="file:///home/a/old%20notes.txt" added="2026-11-01T10:00:00Z" modified="2026-11-01T10:00:00Z" visited="2026-11-01T10:00:00Z">
    <info><metadata owner="http://freedesktop.org"><mime:mime-type type="text/plain"/></metadata></info>
  </bookmark>
  <bookmark href="file:///home/a/page.html?a=1&amp;b=2" added="2026-11-02T10:00:00Z" modified="2026-11-03T10:00:00Z" visited="2026-11-02T10:00:00Z">
    <info><metadata owner="http://freedesktop.org"><mime:mime-type type="text/html"/></metadata></info>
  </bookmark>
  <bookmark href="file:///home/a/new.txt" added="2026-11-04T10:00:00Z" modified="2026-11-04T10:00:00Z" visited="2026-11-04T10:00:00Z">
    <info><metadata owner="http://freedesktop.org"><mime:mime-type type="text/plain"/></metadata></info>
  </bookmark>
</xbel>"#;

    #[test]
    fn recent_files_are_the_apps_defaults_newest_first() {
        let b = parse_xbel(XBEL);
        assert_eq!(b.iter().map(|b| b.modified.as_str()).collect::<Vec<_>>(), ["2026-11-04T10:00:00Z", "2026-11-03T10:00:00Z", "2026-11-01T10:00:00Z"]);
        assert_eq!(b[1].uri, "file:///home/a/page.html?a=1&b=2");
        let defaults = BTreeMap::from([("text/html".to_string(), "firefox.desktop".to_string())]);
        let entries = vec![("org.gnome.TextEditor.desktop".to_string(), crate::settings_pages::parse_desktop("[Desktop Entry]\nName=Text Editor\nMimeType=text/plain;\n"))];
        let editor = recent_of(&b, "org.gnome.TextEditor.desktop", &defaults, &entries);
        assert_eq!(editor, vec![RecentFile { name: "new.txt".into(), uri: "file:///home/a/new.txt".into() }, RecentFile { name: "old notes.txt".into(), uri: "file:///home/a/old%20notes.txt".into() }]);
        assert_eq!(recent_of(&b, "firefox.desktop", &defaults, &entries).len(), 1);
        assert!(recent_of(&b, "nemo.desktop", &defaults, &entries).is_empty());
    }

    #[test]
    fn actions_follow_the_actions_key() {
        let text = "[Desktop Entry]\nName=Firefox\nExec=firefox %u\nActions=new-window;new-private-window;gone;\n\n[Desktop Action new-private-window]\nName=New Private Window\nExec=firefox --private-window %u\n\n[Desktop Action new-window]\nName=New Window\nExec=firefox --new-window %u\n";
        let got = parse_actions(text);
        assert_eq!(got, vec![Action { id: "new-window".into(), name: "New Window".into(), exec: "firefox --new-window".into() }, Action { id: "new-private-window".into(), name: "New Private Window".into(), exec: "firefox --private-window".into() }]);
    }
}
