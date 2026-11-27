//! #EXP-2024: the live view of an agent's space on Exponential Desktop: the
//! taskbar's Agent spaces list (`/spaces`) and the pinned view's box
//! (`/spaceView`), pure projections of what the compositor lists and the
//! session rows the shell's accounts hold. Locked by
//! `packages/uicore/fixtures/spaces.json` (its `$comment` is the spec;
//! `spaceRows` and `spaceView` in `packages/uicore/src/spaces.ts`).

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::layout::shell_layout;

const SOURCE: &str = include_str!("../../../../../packages/uicore/fixtures/spaces.json");

fn fixture() -> &'static Value {
    static FIXTURE: OnceLock<Value> = OnceLock::new();
    FIXTURE.get_or_init(|| serde_json::from_str(SOURCE).expect("spaces.json"))
}

fn constant(key: &str) -> u32 {
    fixture()["constants"][key].as_u64().unwrap_or_else(|| panic!("spaces.json constants.{key}")) as u32
}

/// A space the compositor lists: its id (`space-<n>`) and its grant's session.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpaceInput {
    pub id: String,
    pub session: String,
}

/// A session row one of the shell's signed-in accounts holds, its title
/// resolved from what this machine can read; `None` = no title can be read
/// (#EDE-194: a sealed title no keyring here opens and nothing else that
/// yields one; never session-title.json's `Untitled`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceSession {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub root_id: Option<String>,
}

/// One row of the list: the space, its session, the compositor's number,
/// the session's title (or `Agent space n`) and a child's chat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpaceRow {
    pub id: String,
    pub session: String,
    pub number: Option<u64>,
    pub title: String,
    pub subtitle: Option<String>,
}

/// A space's number: the `n` of `space-<n>`, 1 to 15 digits (beyond that
/// it is no number: the TS twin's are exact to 15).
pub fn space_number(id: &str) -> Option<u64> {
    id.strip_prefix("space-").filter(|n| (1..=15).contains(&n.len()) && n.bytes().all(|b| b.is_ascii_digit())).and_then(|n| n.parse().ok())
}

/// Trimmed of white space as both twins read it (Unicode's White_Space and
/// JavaScript's: U+FEFF too).
fn trimmed(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

fn untitled(id: &str, number: Option<u64>) -> String {
    let copy = fixture()["copy"]["untitled"].as_str().expect("spaces.json copy.untitled");
    copy.replace("{n}", &number.map_or_else(|| id.to_string(), |n| n.to_string()))
}

/// The list's rows in the order of the spaces' numbers.
pub fn space_rows(spaces: &[SpaceInput], sessions: &[SpaceSession]) -> Vec<SpaceRow> {
    let by_id: HashMap<&str, &SpaceSession> = sessions.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut rows: Vec<SpaceRow> = spaces
        .iter()
        .map(|space| {
            let number = space_number(&space.id);
            let session = by_id.get(space.session.as_str());
            let title = session.and_then(|s| s.title.as_deref()).map(trimmed).filter(|t| !t.is_empty()).map_or_else(|| untitled(&space.id, number), str::to_string);
            let subtitle = session
                .filter(|s| s.parent_id.as_deref().is_some_and(|p| !p.is_empty()))
                .and_then(|s| s.root_id.as_deref())
                .and_then(|root| by_id.get(root))
                .and_then(|root| root.title.as_deref())
                .map(trimmed)
                .filter(|t| !t.is_empty())
                .map(str::to_string);
            SpaceRow { id: space.id.clone(), session: space.session.clone(), number, title, subtitle }
        })
        .collect();
    rows.sort_by(|a, b| match (a.number, b.number) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.id.cmp(&b.id),
    });
    rows
}

/// A width and a height in logical px (or, for `pixels`, device px).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extent {
    pub width: u32,
    pub height: u32,
}

/// The first display: its logical size and scale.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Display {
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

/// The pinned view's box: the whole layer (`width` × `height`: the header
/// on the picture inside the border), the picture's box, the pixels a capture is shrunk to,
/// whether it is the large size and whether the large size fits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceView {
    pub width: u32,
    pub height: u32,
    pub picture: Extent,
    pub pixels: Extent,
    pub large: bool,
    pub can_grow: bool,
}

/// The pinned view's box for a space of `space` on `display`.
pub fn space_view(space: Extent, display: Display, large: bool) -> SpaceView {
    let layout = shell_layout();
    let (header, border) = (constant("header"), constant("border"));
    let aspect = if space.width > 0 && space.height > 0 { space.height as f64 / space.width as f64 } else { 10.0 / 16.0 };
    let room_w = display.width.saturating_sub(2 * layout.panel_margin);
    let room_h = display.height.saturating_sub(layout.taskbar_height + 2 * layout.panel_margin);
    let picture_h = |w: u32| (w as f64 * aspect).round() as u32;
    let fits = |w: u32| header + picture_h(w) + 2 * border <= room_h;
    let mut base = constant("maxWidth").min(display.width / 3);
    if !fits(base) {
        base = ((room_h.saturating_sub(header + 2 * border)) as f64 / aspect).floor() as u32;
    }
    let can_grow = 2 * base + 2 * border <= room_w && fits(2 * base);
    let large = large && can_grow;
    let width = if large { 2 * base } else { base };
    let picture = Extent { width, height: picture_h(width) };
    let device = |v: u32, cap: u32| {
        let px = (v as f64 * display.scale).round() as u32;
        if cap > 0 { px.min(cap) } else { px }
    };
    let pixels = Extent { width: device(picture.width, space.width), height: device(picture.height, space.height) };
    SpaceView { width: width + 2 * border, height: header + picture.height + 2 * border, picture, pixels, large, can_grow }
}

/// #EDE-266: who drives a space (the compositor's `control`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpaceControl {
    #[default]
    Agent,
    Paused,
    Person,
}

impl SpaceControl {
    fn key(self) -> &'static str {
        match self {
            SpaceControl::Agent => "agent",
            SpaceControl::Paused => "paused",
            SpaceControl::Person => "person",
        }
    }
}

/// A trace line of the holder's session feed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceLine {
    pub title: String,
    pub icon: String,
    pub status: String,
}

/// An entry of the holder's `sessions` feed: the chat `id`, its working
/// sessions (`stop`) and their `trace`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraySession {
    pub id: String,
    #[serde(default)]
    pub stop: Vec<String>,
    #[serde(default)]
    pub trace: Vec<TraceLine>,
}

/// What the computer-use tray folds (`cuTray`'s input).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayInput {
    pub rows: Vec<SpaceRow>,
    #[serde(default)]
    pub selected: Option<String>,
    #[serde(default)]
    pub controls: HashMap<String, SpaceControl>,
    #[serde(default)]
    pub sessions: Vec<TraySession>,
    #[serde(default)]
    pub watching: Option<String>,
    /// #EDE-272: the sessions the person stopped (parked), oldest first.
    #[serde(default)]
    pub stopped: Vec<StoppedInput>,
}

/// A parked session as the shell resolved it (a row's title and chat).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoppedInput {
    pub session: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub subtitle: Option<String>,
}

/// A parked session's row in the tray, with Allow computer use again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayStopped {
    pub session: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub allow: TrayAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayAction {
    pub label: String,
    pub icon: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayState {
    pub label: String,
    pub icon: String,
    pub variant: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraySelected {
    pub id: String,
    pub session: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub control: SpaceControl,
    pub state: TrayState,
    pub now: Option<String>,
    pub trace: Vec<TraceLine>,
    pub watching: bool,
    pub watch: TrayAction,
    pub pause: Option<TrayAction>,
    pub take: TrayAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayRow {
    #[serde(flatten)]
    pub row: SpaceRow,
    pub selected: bool,
}

/// The computer-use tray: its popup and its applet menu (a Menu's items).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tray {
    pub count: usize,
    pub shown: bool,
    pub badge: String,
    pub list: bool,
    pub rows: Vec<TrayRow>,
    pub selected: Option<TraySelected>,
    pub menu: Vec<Value>,
    pub stopped: Vec<TrayStopped>,
}

fn copy(key: &str) -> String {
    fixture()["copy"][key].as_str().unwrap_or_else(|| panic!("spaces.json copy.{key}")).to_string()
}

fn tray_action(key: &str) -> TrayAction {
    let icon = fixture()["constants"]["trayIcons"][key].as_str().unwrap_or_else(|| panic!("spaces.json trayIcons.{key}"));
    TrayAction { label: copy(key), icon: icon.to_string(), value: key.to_string() }
}

/// #EDE-266: the computer-use tray (`cuTray`).
pub fn cu_tray(input: &TrayInput) -> Tray {
    let rows = &input.rows;
    let pick = rows.iter().find(|r| Some(&r.id) == input.selected.as_ref()).or(rows.first());
    let count = rows.len();
    let badge = match count {
        0 => String::new(),
        1 => copy("spaceOne"),
        n => copy("spaceMany").replace("{n}", &n.to_string()),
    };
    let selected = pick.map(|pick| {
        let control = input.controls.get(&pick.id).copied().unwrap_or_default();
        let entry = input.sessions.iter().find(|s| s.id == pick.session || s.stop.contains(&pick.session));
        let keep = constant("trayTrace") as usize;
        let trace: Vec<TraceLine> = entry.map(|e| e.trace[e.trace.len().saturating_sub(keep)..].to_vec()).unwrap_or_default();
        let now = trace.iter().rev().find(|line| line.status == "running").map(|line| line.title.clone());
        let watching = input.watching.as_deref() == Some(pick.session.as_str());
        let key = control.key();
        let constants = &fixture()["constants"];
        let state = TrayState {
            label: copy(key),
            icon: constants["trayIcons"][key].as_str().unwrap_or_default().to_string(),
            variant: constants["trayVariants"][key].as_str().unwrap_or_default().to_string(),
        };
        TraySelected {
            id: pick.id.clone(),
            session: pick.session.clone(),
            title: pick.title.clone(),
            subtitle: pick.subtitle.clone(),
            control,
            state,
            now,
            trace,
            watching,
            watch: tray_action(if watching { "unwatch" } else { "watch" }),
            pause: match control {
                SpaceControl::Agent => Some(tray_action("pause")),
                SpaceControl::Paused => Some(tray_action("resume")),
                SpaceControl::Person => None,
            },
            take: tray_action(if control == SpaceControl::Person { "handBack" } else { "takeOver" }),
        }
    });
    let item = |a: &TrayAction| serde_json::json!({"label": a.label, "value": a.value, "icon": a.icon});
    let mut menu = match &selected {
        None => Vec::new(),
        Some(s) => {
            let mut menu = vec![item(&s.watch)];
            menu.extend(s.pause.as_ref().map(item));
            menu.push(item(&s.take));
            menu.push(serde_json::json!({"kind": "separator"}));
            let mut stop = item(&tray_action("stop"));
            stop["destructive"] = Value::Bool(true);
            menu.push(stop);
            menu
        }
    };
    // #EDE-272: the parked sessions, each with Allow computer use again.
    let stopped: Vec<TrayStopped> = input
        .stopped
        .iter()
        .map(|s| {
            let title = s.title.as_deref().map(trimmed).filter(|t| !t.is_empty()).map_or_else(|| copy("stoppedUntitled"), str::to_string);
            TrayStopped { session: s.session.clone(), title, subtitle: s.subtitle.clone(), allow: tray_action("allow") }
        })
        .collect();
    if !stopped.is_empty() && !menu.is_empty() {
        menu.push(serde_json::json!({"kind": "separator"}));
    }
    for s in &stopped {
        menu.push(serde_json::json!({"label": s.allow.label, "value": format!("allow:{}", s.session), "icon": s.allow.icon, "detail": s.title}));
    }
    let rows = rows.iter().map(|r| TrayRow { row: r.clone(), selected: selected.as_ref().is_some_and(|s| s.id == r.id) }).collect();
    Tray { count, shown: count > 0 || !stopped.is_empty(), badge, list: count > 1, rows, selected, menu, stopped }
}
