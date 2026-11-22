//! NEW-59: THE work screen (`fixtures/work-screen.json`): which displays
//! agents may drive, the `wrong_display` refusals the hub answers with,
//! the run's current display and where the voice overlay may open. The
//! pure rules replay the fixture; the OS reads go through the backend.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

use crate::backend::{self, DisplayInfo, Target, WindowInfo};
use crate::{Display, ScreenState};

const FIXTURE: &str = include_str!("../../../../../packages/domain-contract/fixtures/work-screen.json");

fn fixture() -> &'static Value {
    static FIXTURE_JSON: OnceLock<Value> = OnceLock::new();
    FIXTURE_JSON.get_or_init(|| serde_json::from_str(FIXTURE).expect("work-screen.json parses"))
}

/// The tools a fullscreen app on a gaming display refuses (`inputTools`).
fn input_tools() -> &'static [String] {
    static TOOLS: OnceLock<Vec<String>> = OnceLock::new();
    TOOLS.get_or_init(|| {
        fixture()["inputTools"]
            .as_array()
            .expect("work-screen inputTools")
            .iter()
            .map(|tool| tool.as_str().expect("an input tool name").to_string())
            .collect()
    })
}

/// The tools that press keys into the focused window (`keyTools`).
fn key_tools() -> &'static [String] {
    static TOOLS: OnceLock<Vec<String>> = OnceLock::new();
    TOOLS.get_or_init(|| {
        fixture()["keyTools"]
            .as_array()
            .expect("work-screen keyTools")
            .iter()
            .map(|tool| tool.as_str().expect("a key tool name").to_string())
            .collect()
    })
}

/// Whether `tool` acts on the foreground (`inputTools`).
pub(crate) fn is_input(tool: &str) -> bool {
    input_tools().iter().any(|input| input == tool)
}

/// #EXP-1979: whether `tool` moves or presses the pointer: an input tool
/// that neither presses keys (`keyTools`) nor activates a window.
pub(crate) fn is_pointer(tool: &str) -> bool {
    is_input(tool) && !is_key(tool) && tool != "activate_window"
}

/// #EXP-1898: whether `tool` DRIVES (`inputTools`, plus the clipboard's
/// `read_clipboard`): it takes the person's input for the call and queues
/// with the other driving calls. Looking and waiting (`screenshot`,
/// `list_windows`, `cursor_position`, `wait`) do neither.
pub(crate) fn drives(tool: &str) -> bool {
    is_input(tool) || tool == "read_clipboard"
}

/// Whether `tool` presses keys into the focused window (`keyTools`).
pub(crate) fn is_key(tool: &str) -> bool {
    key_tools().iter().any(|key| key == tool)
}

/// The refusal kinds (`copy.refusals` keys).
pub const NO_WORK_SCREEN: &str = "noWorkScreen";
pub const PICKED_GONE: &str = "pickedGone";
pub const NO_WINDOW_LIST: &str = "noWindowList";
pub const UNKNOWN_WINDOW: &str = "unknownWindow";
pub const NOT_WORK_SCREEN: &str = "notWorkScreen";
pub const WINDOW_ELSEWHERE: &str = "windowElsewhere";
pub const GAMING: &str = "gaming";
pub const KEYS_ELSEWHERE: &str = "keysElsewhere";
/// #EXP-1514: the pick is the agent space only (`spaceGateCases`).
pub const SPACE_ONLY: &str = "spaceOnly";

/// #EXP-1514: the reserved Work screen pick `space.value`: agents work in
/// an agent space only, never on this device's screen. No display id is
/// ever that bare word.
pub const SPACE_PICK: &str = "space";

/// The pick as the SCREEN readers take it (screen memory, the voice
/// overlay's screen state): `space` restricts driving only, so for them it
/// is Auto (C 11, `screen-memory.json` `spaceCases`).
pub(crate) fn display_pick(picked: Option<&str>) -> Option<&str> {
    picked.filter(|picked| *picked != SPACE_PICK)
}

/// What the gate reads (`gateCases[].state`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    pub count: usize,
    /// The 1-based number of the display whose id is stored, 0 when that
    /// id is not connected (#FEED-161); `None` = Auto.
    pub picked: Option<u32>,
    /// 0-based displays holding a fullscreen window.
    pub fullscreen: Vec<usize>,
    /// 0-based display of the focused window.
    pub active: Option<usize>,
    pub active_fullscreen: bool,
    /// The OS window list was read (`windowsRead`); `false` = input fails
    /// closed.
    pub windows_read: bool,
    /// #EXP-1514: the stored pick is [`SPACE_PICK`] (`spaceOnly`): nothing
    /// is done on this screen.
    pub space_only: bool,
}

/// One tool call as the gate sees it (`gateCases[].calls[].call`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Call<'a> {
    pub tool: &'a str,
    /// `activate_window`'s window id.
    pub id: Option<u64>,
    /// `screenshot`'s 0-based `display` argument.
    pub display: Option<usize>,
    /// `activate_window`: the 0-based display the LISTED window overlaps
    /// most ([`window_display`]); `None` = list_windows does not name the
    /// id or it lies on no display.
    pub window_display: Option<usize>,
}

/// The `wrong_display` refusal: one of `copy.refusals`, filled in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub kind: &'static str,
    pub text: String,
}

/// The 0-based displays agents may drive: the picked one while connected,
/// else every display without a fullscreen app.
pub fn work_displays(state: &State) -> Vec<usize> {
    if state.space_only {
        return Vec::new();
    }
    if let Some(picked) = state.picked {
        // A picked display that is gone = none: a hot-unplug never widens
        // the set the person allowed.
        let picked = picked as usize;
        return if picked >= 1 && picked <= state.count { vec![picked - 1] } else { Vec::new() };
    }
    (0..state.count).filter(|index| !state.fullscreen.contains(index)).collect()
}

/// The fields a refusal's copy fills in.
#[derive(Default)]
struct Fields {
    display: Option<usize>,
    id: Option<u64>,
}

fn refuse(kind: &'static str, fields: Fields, work: &[usize]) -> Refusal {
    let template = fixture()["copy"]["refusals"][kind].as_str().expect("work-screen refusal copy");
    let text_of = |value: Option<String>| value.unwrap_or_default();
    // NUMBERING: the tools' own 0-based display indexes.
    let work = work.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
    let text = template
        .replace("{display}", &text_of(fields.display.map(|display| display.to_string())))
        .replace("{id}", &text_of(fields.id.map(|id| id.to_string())))
        .replace("{work}", &work);
    Refusal { kind, text }
}

/// Whether the hub runs `call` now, else the `wrong_display` refusal.
pub fn gate(state: &State, call: Call<'_>) -> Result<(), Refusal> {
    let work = work_displays(state);
    if call.tool == "wait" {
        return Ok(());
    }
    // #EXP-1514: the pick Agent space only, checked first.
    if state.space_only {
        return Err(refuse(SPACE_ONLY, Fields::default(), &work));
    }
    if state.picked.is_some() && work.is_empty() {
        return Err(refuse(PICKED_GONE, Fields::default(), &work));
    }
    if work.is_empty() {
        return Err(refuse(NO_WORK_SCREEN, Fields::default(), &work));
    }
    let input = is_input(call.tool);
    // #FEED-161: Auto without a window list knows no display to be free
    // (no fullscreen reading); a connected pick still screenshots.
    if !state.windows_read && state.picked.is_none() {
        return Err(refuse(NO_WINDOW_LIST, Fields::default(), &work));
    }
    if !state.windows_read && (input || call.tool == "list_windows") {
        return Err(refuse(NO_WINDOW_LIST, Fields::default(), &work));
    }
    if call.tool == "screenshot" {
        if let Some(display) = call.display.filter(|display| !work.contains(display)) {
            return Err(refuse(NOT_WORK_SCREEN, Fields { display: Some(display), ..Fields::default() }, &work));
        }
    }
    if call.tool == "activate_window" {
        match call.window_display {
            None => return Err(refuse(UNKNOWN_WINDOW, Fields { id: call.id, ..Fields::default() }, &work)),
            Some(display) if !work.contains(&display) => {
                return Err(refuse(WINDOW_ELSEWHERE, Fields { display: Some(display), ..Fields::default() }, &work))
            }
            Some(_) => {}
        }
    }
    if let Some(active) = state.active.filter(|_| input) {
        if state.active_fullscreen && !work.contains(&active) {
            return Err(refuse(GAMING, Fields { display: Some(active), ..Fields::default() }, &work));
        }
        if key_tools().iter().any(|tool| tool == call.tool) && !work.contains(&active) {
            return Err(refuse(KEYS_ELSEWHERE, Fields { display: Some(active), ..Fields::default() }, &work));
        }
    }
    Ok(())
}

/// The run's current display: its previous pick while it stays a work
/// display, else the primary when it is one, else the first work display.
pub fn current(work: &[usize], primary: usize, previous: Option<usize>) -> Option<usize> {
    if let Some(previous) = previous.filter(|previous| work.contains(previous)) {
        return Some(previous);
    }
    if work.contains(&primary) {
        return Some(primary);
    }
    work.first().copied()
}

/// The display the voice overlay opens on (0-based), `None` = hidden
/// (`overlayCases`).
pub fn overlay_display(count: usize, pointer: usize, fullscreen: &[usize], work: &[usize]) -> Option<usize> {
    let free = |index: usize| !fullscreen.contains(&index);
    if pointer < count && free(pointer) {
        return Some(pointer);
    }
    if let Some(&index) = work.iter().find(|&&index| free(index)) {
        return Some(index);
    }
    (0..count).find(|&index| free(index))
}

/// The device's Work screen setting: the picked display's stable id
/// (#FEED-161); `None` = Auto. Resolved to a number on every call
/// ([`resolve`]), so a hot-plug or a reorder never moves it.
/// One per [`Target`] (#EXP-1924).
static PICKED: Mutex<Option<HashMap<Target, String>>> = Mutex::new(None);

pub(crate) fn set_picked(target: Target, id: Option<String>) {
    let mut picked = PICKED.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let picked = picked.get_or_insert_with(HashMap::new);
    match id.filter(|id| !id.is_empty()) {
        Some(id) => picked.insert(target, id),
        None => picked.remove(&target),
    };
}

pub(crate) fn picked(target: Target) -> Option<String> {
    PICKED.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).as_ref()?.get(&target).cloned()
}

/// The gate's `picked` off the stored id: the 1-based number of the display
/// carrying it ([`carried_index`]), 0 when none does; `None` = Auto.
pub(crate) fn resolve(displays: &[DisplayInfo], picked: Option<&str>) -> Option<u32> {
    let id = picked?;
    Some(carried_index(displays, id).map_or(0, |index| index as u32 + 1))
}

/// The 0-based display the hub reads a stored pick as (`pickCases[].hub`):
/// the one carrying it, else (#EXP-1919) for a `base@connector` the lone
/// display `base` on that connector, whose twin went. Never a guess: the
/// host rewrites any other form ([`pick_of`]).
pub(crate) fn carried_index(displays: &[DisplayInfo], stored: &str) -> Option<usize> {
    if let Some(index) = displays.iter().position(|display| display.id == stored) {
        return Some(index);
    }
    let (_, connector) = stored.rsplit_once('@')?;
    displays.iter().position(|display| display.connector.as_deref() == Some(connector) && backend::twin_id(&display.id, connector) == stored)
}

/// #EXP-1919 SAME MONITOR (`pickCases[].expect`): the 0-based display a
/// stored pick names for the host, which also knows `seen`, the
/// `base@connector` form of the display it last saw carrying the pick:
/// [`carried_index`], else for a bare id whose display got a twin the
/// display `seen` (not connected without it, never another twin); only a
/// pick never seen falls to the legacy rule ([`picked_index`]).
pub(crate) fn pick_index(displays: &[DisplayInfo], stored: &str, seen: Option<&str>) -> Option<usize> {
    if let Some(index) = carried_index(displays, stored) {
        return Some(index);
    }
    let own = |seen: &&str| seen.rsplit_once('@').is_some_and(|(_, connector)| backend::twin_id(stored, connector) == *seen);
    if let Some(seen) = seen.filter(own) {
        return displays.iter().position(|display| display.id == seen);
    }
    let ids: Vec<&str> = displays.iter().map(|display| display.id.as_str()).collect();
    picked_index(&ids, stored)
}

/// [`pick_index`] as the host stores it: the id that display carries now
/// and its `base@connector` form (`None` = no connector tells it apart).
pub(crate) fn pick_of(displays: &[DisplayInfo], stored: &str, seen: Option<&str>) -> Option<crate::PickedDisplay> {
    let display = &displays[pick_index(displays, stored, seen)?];
    let seen = display.connector.as_deref().map(|connector| {
        if display.id.ends_with(&format!("@{connector}")) {
            display.id.clone()
        } else {
            backend::twin_id(&display.id, connector)
        }
    });
    Some(crate::PickedDisplay { id: display.id.clone(), seen })
}

/// The 0-based display a stored pick names among `ids` (the OS's order):
/// the one carrying it, else (#FEED-164 TWINS UPGRADE, `legacyIdCases`) an
/// id stored before twins carried their connector: `base` or `base#n` = the
/// n-th (bare = the 1st) of the `base@<connector>` twins, the display the
/// old order-based scheme named.
pub(crate) fn picked_index(ids: &[&str], stored: &str) -> Option<usize> {
    if let Some(index) = ids.iter().position(|id| *id == stored) {
        return Some(index);
    }
    let (base, n) = match stored.rsplit_once('#') {
        Some((base, n)) if !n.is_empty() && n.bytes().all(|byte| byte.is_ascii_digit()) => (base, n.parse::<usize>().ok().filter(|n| *n >= 2)?),
        _ => (stored, 1),
    };
    // A twin id cut to fit 128 keeps only a prefix of its base.
    let twin_of = |id: &str| {
        id.match_indices('@').any(|(at, _)| at > 0 && (&id[..at] == base || (id.chars().count() == backend::ID_MAX && base.starts_with(&id[..at]))))
    };
    ids.iter().enumerate().filter(|(_, id)| twin_of(id)).map(|(index, _)| index).nth(n - 1)
}

/// The 0-based display `window` overlaps MOST (ties: the lower index;
/// #FEED-161, never its centre alone); `None` when it is on no display.
pub(crate) fn window_display(displays: &[DisplayInfo], window: &WindowInfo) -> Option<usize> {
    most_overlap(displays, window.x, window.y, window.width, window.height)
}

/// The 0-based display a rect overlaps most (ties: the lower index).
pub(crate) fn most_overlap(displays: &[DisplayInfo], x: i32, y: i32, width: u32, height: u32) -> Option<usize> {
    let mut best: Option<(usize, u64)> = None;
    for (index, display) in displays.iter().enumerate() {
        let area = display.overlap(x, y, width, height);
        if area > 0 && best.is_none_or(|(_, most)| area > most) {
            best = Some((index, area));
        }
    }
    best.map(|(index, _)| index)
}

/// The gate's state off the backend's displays and window list (top
/// first, each marked fullscreen per the OS's rule).
pub(crate) fn state_of(displays: &[DisplayInfo], windows: &[WindowInfo], picked: Option<u32>) -> State {
    let mut fullscreen: Vec<usize> = windows
        .iter()
        .filter(|window| window.fullscreen)
        .filter_map(|window| window_display(displays, window))
        .collect();
    fullscreen.sort_unstable();
    fullscreen.dedup();
    let active = windows.iter().find(|window| window.active);
    State {
        count: displays.len(),
        picked,
        fullscreen,
        active: active.and_then(|window| window_display(displays, window)),
        active_fullscreen: active.is_some_and(|window| window.fullscreen),
        windows_read: true,
        space_only: false,
    }
}

/// The display the banner floats on (0-based): the current display of the
/// last call that went through; cleared when the lock releases.
/// One per [`Target`] (#EXP-1924).
static BANNER: Mutex<Option<HashMap<Target, usize>>> = Mutex::new(None);

pub(crate) fn set_banner(target: Target, display: Option<usize>) {
    let mut banner = BANNER.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let banner = banner.get_or_insert_with(HashMap::new);
    match display {
        Some(display) => banner.insert(target, display),
        None => banner.remove(&target),
    };
}

fn public(index: usize, display: &DisplayInfo) -> Display {
    Display {
        id: display.id.clone(),
        number: index as u32 + 1,
        x: display.x,
        y: display.y,
        width: display.width,
        height: display.height,
        primary: display.primary,
    }
}

fn backend_displays() -> Option<(Box<dyn backend::Backend>, Vec<DisplayInfo>)> {
    let backend = backend::open(Target::Person, None).ok()?;
    let displays = backend.displays().ok()?;
    Some((backend, displays))
}

pub(crate) fn displays() -> Vec<Display> {
    match backend_displays() {
        Some((_, displays)) => displays.iter().enumerate().map(|(index, display)| public(index, display)).collect(),
        None => Vec::new(),
    }
}

pub(crate) fn screen_state() -> Option<ScreenState> {
    let (backend, displays) = backend_displays()?;
    let windows = backend.list_windows().unwrap_or_else(|reason| {
        log::debug!("[computer] no window list for the work screen ({reason})");
        Vec::new()
    });
    let picked = resolve(&displays, display_pick(picked(Target::Person).as_deref()));
    let state = state_of(&displays, &windows, picked);
    let work = work_displays(&state);
    Some(ScreenState {
        displays: displays.iter().enumerate().map(|(index, display)| public(index, display)).collect(),
        fullscreen: state.fullscreen,
        work,
    })
}

pub(crate) fn pick_now(stored: &str, seen: Option<&str>) -> Option<crate::PickedDisplay> {
    let (_, displays) = backend_displays()?;
    pick_of(&displays, stored, seen)
}

/// [`crate::ScreenView`] off one display set (with pixel sizes) and one window
/// list (top first); `app_id` names a pid's app (asked for the focused
/// window and each display's top window).
pub(crate) fn view_of(
    displays: &[(DisplayInfo, (u32, u32))],
    windows: &[WindowInfo],
    picked: Option<&str>,
    app_id: impl Fn(u32) -> Option<String>,
) -> crate::ScreenView {
    let infos: Vec<DisplayInfo> = displays.iter().map(|(display, _)| display.clone()).collect();
    let picked = resolve(&infos, display_pick(picked));
    let work = work_displays(&state_of(&infos, windows, picked)).into_iter().map(|index| index as u32 + 1).collect();
    let mut tops: Vec<crate::TopWindow> = Vec::new();
    for window in windows {
        if let Some(index) = window_display(&infos, window) {
            let display = index as u32 + 1;
            if !tops.iter().any(|top| top.display == display) {
                tops.push(crate::TopWindow {
                    display,
                    app: window.app.clone(),
                    app_id: window.pid.and_then(&app_id),
                    title: window.title.clone(),
                });
            }
        }
    }
    tops.sort_by_key(|top| top.display);
    let focused = windows.iter().find(|window| window.active).and_then(|window| {
        let index = window_display(&infos, window)?;
        let (display, pixels) = &displays[index];
        Some(crate::FocusedWindow {
            app: window.app.clone(),
            app_id: window.pid.and_then(&app_id),
            title: window.title.clone(),
            display: index as u32 + 1,
            rect: rect_in_pixels(display, *pixels, window)?,
        })
    });
    crate::ScreenView { focused, work, tops }
}

/// `window`'s rect within `display` (screen units) as `pixels` (the
/// display's physical size) counts it, clipped to the display; `None` when
/// nothing of it is left.
pub(crate) fn rect_in_pixels(display: &DisplayInfo, pixels: (u32, u32), window: &WindowInfo) -> Option<crate::ScreenRect> {
    if display.width == 0 || display.height == 0 {
        return None;
    }
    let scale = |value: i64, units: u32, pixels: u32| (value * i64::from(pixels) + i64::from(units) / 2) / i64::from(units);
    let left = i64::from(window.x - display.x).clamp(0, i64::from(display.width));
    let top = i64::from(window.y - display.y).clamp(0, i64::from(display.height));
    let right = (i64::from(window.x - display.x) + i64::from(window.width)).clamp(0, i64::from(display.width));
    let bottom = (i64::from(window.y - display.y) + i64::from(window.height)).clamp(0, i64::from(display.height));
    let (x0, x1) = (scale(left, display.width, pixels.0), scale(right, display.width, pixels.0));
    let (y0, y1) = (scale(top, display.height, pixels.1), scale(bottom, display.height, pixels.1));
    (x1 > x0 && y1 > y0).then(|| crate::ScreenRect { x: x0 as i32, y: y0 as i32, width: (x1 - x0) as u32, height: (y1 - y0) as u32 })
}

/// #EXP-1916: the 0-based display the banner floats on, without reading
/// the OS (the IDE asks every tick whether the run moved on).
pub(crate) fn banner_index(target: Target) -> Option<usize> {
    BANNER.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).as_ref()?.get(&target).copied()
}

pub(crate) fn banner_display(target: Target) -> Option<Display> {
    let index = banner_index(target)?;
    let (_, displays) = backend_displays()?;
    displays.get(index).map(|display| public(index, display))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn indexes(value: &Value) -> Vec<usize> {
        value.as_array().unwrap().iter().map(|index| index.as_u64().unwrap() as usize).collect()
    }

    fn index(value: &Value) -> Option<usize> {
        value.as_u64().map(|index| index as usize)
    }

    #[test]
    fn the_gate_replays_every_gate_case() {
        let cases = fixture()["gateCases"].as_array().unwrap().iter().chain(fixture()["spaceGateCases"].as_array().unwrap());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let raw = &case["state"];
            let state = State {
                count: raw["count"].as_u64().unwrap() as usize,
                picked: raw["picked"].as_u64().map(|picked| picked as u32),
                fullscreen: indexes(&raw["fullscreen"]),
                active: index(&raw["active"]),
                active_fullscreen: raw["activeFullscreen"].as_bool().unwrap(),
                windows_read: raw["windowsRead"].as_bool().unwrap_or(true),
                space_only: raw["spaceOnly"].as_bool().unwrap_or(false),
            };
            assert_eq!(work_displays(&state), indexes(&case["work"]), "{name}: work");
            for step in case["calls"].as_array().unwrap() {
                let call = Call {
                    tool: step["call"]["tool"].as_str().unwrap(),
                    id: step["call"]["id"].as_u64(),
                    display: index(&step["call"]["display"]),
                    window_display: index(&step["call"]["windowDisplay"]),
                };
                let got = gate(&state, call);
                match &step["expect"] {
                    Value::String(allow) => {
                        assert_eq!(allow, "allow");
                        assert_eq!(got, Ok(()), "{name}: {call:?}");
                    }
                    expect => {
                        let refusal = got.expect_err(&format!("{name}: {call:?} is refused"));
                        assert_eq!(refusal.kind, expect["kind"].as_str().unwrap(), "{name}: {call:?}");
                        assert_eq!(refusal.text, expect["text"].as_str().unwrap(), "{name}: {call:?}");
                    }
                }
            }
        }
    }

    /// #EXP-1514: the reserved pick is the fixture's, and only the driving
    /// side reads it: the screen readers take it as Auto.
    #[test]
    fn the_space_pick_restricts_driving_only() {
        assert_eq!(fixture()["space"]["value"], SPACE_PICK);
        assert_eq!(fixture()["pickerKeys"]["space"], SPACE_PICK);
        assert_eq!(display_pick(Some(SPACE_PICK)), None);
        assert_eq!(display_pick(Some("output:DP-1")), Some("output:DP-1"));
        assert_eq!(display_pick(None), None);
    }

    #[test]
    fn the_current_display_replays_every_current_case() {
        for case in fixture()["currentCases"].as_array().unwrap() {
            let got = current(&indexes(&case["work"]), index(&case["primary"]).unwrap(), index(&case["previous"]));
            assert_eq!(got, index(&case["expect"]), "{}", case["name"]);
        }
    }

    #[test]
    fn the_overlay_display_replays_every_overlay_case() {
        for case in fixture()["overlayCases"].as_array().unwrap() {
            let state = &case["state"];
            let got = overlay_display(
                state["count"].as_u64().unwrap() as usize,
                index(&state["pointer"]).unwrap(),
                &indexes(&state["fullscreen"]),
                &indexes(&state["work"]),
            );
            assert_eq!(got, index(&case["expect"]), "{}", case["name"]);
        }
    }

    #[test]
    fn the_focused_rect_is_in_the_displays_pixels_and_clipped_to_it() {
        // A Retina panel: 1512x982 points, 3024x1964 pixels, right of a 1920 one.
        let display = DisplayInfo { id: String::new(), x: 1920, y: 0, width: 1512, height: 982, primary: false, name: String::new(), connector: None };
        let window = |x, y, width, height| WindowInfo {
            id: 1,
            title: String::new(),
            app: String::new(),
            pid: None,
            x,
            y,
            width,
            height,
            active: true,
            fullscreen: false,
        };
        let rect = rect_in_pixels(&display, (3024, 1964), &window(1970, 25, 600, 400)).unwrap();
        assert_eq!(rect, crate::ScreenRect { x: 100, y: 50, width: 1200, height: 800 });
        // Hanging off the left edge onto the other display: clipped.
        let rect = rect_in_pixels(&display, (3024, 1964), &window(1820, 0, 300, 982)).unwrap();
        assert_eq!(rect, crate::ScreenRect { x: 0, y: 0, width: 400, height: 1964 });
        assert_eq!(rect_in_pixels(&display, (3024, 1964), &window(0, 0, 100, 100)), None);
    }

    #[test]
    fn the_state_reads_fullscreen_and_focus_off_the_windows_displays() {
        let display = |x, primary| DisplayInfo { id: format!("id-{x}"), x, y: 0, width: 1000, height: 1000, primary, name: String::new(), connector: None };
        let displays = [display(0, true), display(1000, false)];
        let window = |x, active, fullscreen| WindowInfo {
            id: 1,
            title: String::new(),
            app: String::new(),
            pid: None,
            x,
            y: 0,
            width: 1000,
            height: 1000,
            active,
            fullscreen,
        };
        // A focused game on display 2, a plain window on display 1, one off-screen.
        let windows = [window(1000, true, true), window(0, false, false), window(5000, false, true)];
        let state = state_of(&displays, &windows, None);
        assert_eq!(state, State { count: 2, picked: None, fullscreen: vec![1], active: Some(1), active_fullscreen: true, windows_read: true, space_only: false });
        assert_eq!(work_displays(&state), [0]);
        assert_eq!(state_of(&displays, &[], Some(2)).active, None);
    }

    /// #FEED-161: a window's display = the one its rect overlaps MOST
    /// (ties: the lower index), never its centre alone.
    #[test]
    fn a_window_belongs_to_the_display_it_overlaps_most() {
        // Display 0 is short (600 tall), display 1 is 1000.
        let displays = [
            DisplayInfo { id: "a".into(), x: 0, y: 0, width: 1000, height: 600, primary: true, name: String::new(), connector: None },
            DisplayInfo { id: "b".into(), x: 1000, y: 0, width: 1000, height: 1000, primary: false, name: String::new(), connector: None },
        ];
        let window = |x, y, width, height| WindowInfo {
            id: 1,
            title: String::new(),
            app: String::new(),
            pid: None,
            x,
            y,
            width,
            height,
            active: false,
            fullscreen: false,
        };
        // Centres on no display: the overlap still places them.
        assert_eq!(window_display(&displays, &window(200, 300, 1500, 800)), Some(1));
        assert_eq!(window_display(&displays, &window(0, 0, 1200, 1400)), Some(0));
        // Centre (950, 500) on display 0, most of it (200x1000 vs 300x600) on 1.
        assert_eq!(window_display(&displays, &window(700, 0, 500, 1000)), Some(1));
        // An exact tie: the lower index.
        assert_eq!(window_display(&displays, &window(800, 0, 400, 400)), Some(0));
        // Off every display.
        assert_eq!(window_display(&displays, &window(5000, 0, 100, 100)), None);
    }

    /// #FEED-161: the stored id names the display wherever it sits now; an
    /// id nobody carries = 0 (gone), never Auto.
    #[test]
    fn the_picked_id_resolves_to_its_display_after_a_reorder() {
        let display = |id: &str| DisplayInfo { id: id.into(), x: 0, y: 0, width: 10, height: 10, primary: false, name: String::new(), connector: None };
        assert_eq!(resolve(&[display("a"), display("b")], Some("b")), Some(2));
        assert_eq!(resolve(&[display("b"), display("a")], Some("b")), Some(1));
        assert_eq!(resolve(&[display("a")], Some("b")), Some(0));
        assert_eq!(resolve(&[display("a")], None), None);
        let state = State { count: 1, picked: resolve(&[display("a")], Some("b")), windows_read: true, ..State::default() };
        assert!(work_displays(&state).is_empty());
    }

    /// #FEED-164 TWINS UPGRADE: `work-screen.json` `legacyIdCases`.
    #[test]
    fn an_id_stored_before_connector_twins_names_the_same_twin() {
        for case in fixture()["legacyIdCases"].as_array().unwrap() {
            let ids: Vec<&str> = case["displays"].as_array().unwrap().iter().map(|id| id.as_str().unwrap()).collect();
            let got = picked_index(&ids, case["stored"].as_str().unwrap()).map(|index| ids[index]);
            assert_eq!(got, case["expect"].as_str(), "{}", case["name"]);
            let displays: Vec<DisplayInfo> =
                ids.iter().map(|id| DisplayInfo { id: id.to_string(), x: 0, y: 0, width: 10, height: 10, primary: false, name: String::new(), connector: None }).collect();
            // The host's rule for a pick it never saw on a display.
            assert_eq!(pick_index(&displays, case["stored"].as_str().unwrap(), None).map(|index| ids[index]), got, "{}", case["name"]);
        }
        // Every old id names its own twin under the new one, ids cut to 128 too.
        let long = "x".repeat(200);
        for base in ["edid:0123456789abcdef", long.as_str()] {
            let display = |id: &str| DisplayInfo { id: id.into(), x: 0, y: 0, width: 10, height: 10, primary: false, name: String::new(), connector: None };
            let mut old = [display(base), display("uuid-b"), display(base), display(base)];
            let mut new = old.clone();
            backend::finish_ids(&mut old, &[]);
            backend::finish_ids(&mut new, &[Some("HDMI-1".into()), Some("1".into()), Some("DP-1".into()), Some("DP-2".into())]);
            let ids: Vec<&str> = new.iter().map(|display| display.id.as_str()).collect();
            for (index, display) in old.iter().enumerate() {
                assert_eq!(picked_index(&ids, &display.id), Some(index), "{}", display.id);
            }
        }
    }

    /// #EXP-1919 SAME MONITOR: a display's id changes form with its twin
    /// (`base` alone, `base@connector` beside it); the pick stays on its
    /// monitor (`work-screen.json` `pickCases`).
    #[test]
    fn a_pick_stays_on_its_monitor_when_a_twin_goes_or_arrives() {
        for case in fixture()["pickCases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let displays: Vec<DisplayInfo> = case["displays"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| DisplayInfo {
                    id: row["id"].as_str().unwrap().into(),
                    x: 0,
                    y: 0,
                    width: 10,
                    height: 10,
                    primary: false,
                    name: String::new(),
                    connector: row["connector"].as_str().map(str::to_string),
                })
                .collect();
            let stored = case["stored"].as_str().unwrap();
            let id_of = |index: Option<usize>| index.map(|index| displays[index].id.as_str());
            assert_eq!(id_of(pick_index(&displays, stored, case["seen"].as_str())), case["expect"].as_str(), "{name}");
            assert_eq!(id_of(carried_index(&displays, stored)), case["hub"].as_str(), "{name}: hub");
        }
        // The round trip off real id lists, ids cut to 128 too: DP-1 is
        // picked beside its twin, the twin goes, comes back and sorts first.
        let long = "x".repeat(200);
        for base in ["edid:H", long.as_str()] {
            let display = |id: &str| DisplayInfo { id: id.into(), x: 0, y: 0, width: 10, height: 10, primary: false, name: String::new(), connector: None };
            let mut both = [display(base), display(base)];
            backend::finish_ids(&mut both, &[Some("HDMI-1".into()), Some("DP-1".into())]);
            let mut alone = [display(base)];
            backend::finish_ids(&mut alone, &[Some("DP-1".into())]);
            assert_eq!(resolve(&alone, Some(both[1].id.as_str())), Some(1), "the picked monitor is connected");
            let kept = pick_of(&alone, &both[1].id, None).unwrap();
            assert_eq!((kept.id.as_str(), kept.seen.as_deref()), (alone[0].id.as_str(), Some(both[1].id.as_str())));
            assert_eq!(resolve(&both, Some(kept.id.as_str())), Some(0), "the hub never guesses a twin");
            let back = pick_of(&both, &kept.id, kept.seen.as_deref()).unwrap();
            assert_eq!((back.id.as_str(), back.seen.as_deref()), (both[1].id.as_str(), Some(both[1].id.as_str())));
        }
        // Twins told apart by order carry no connector: nothing to remember.
        let display = |id: &str| DisplayInfo { id: id.into(), x: 0, y: 0, width: 10, height: 10, primary: false, name: String::new(), connector: None };
        let mut ordered = [display("uuid-a"), display("uuid-a")];
        backend::finish_ids(&mut ordered, &[Some("1".into()), None]);
        assert_eq!(pick_of(&ordered, "uuid-a#2", None), Some(crate::PickedDisplay { id: "uuid-a#2".into(), seen: None }));
    }

    /// NEW-58: screen memory's view: the focused window with its app id,
    /// the work displays 1-based, each display's top window.
    #[test]
    fn the_screen_view_reads_focus_work_and_top_windows_in_one_list() {
        let display = |x, primary| (DisplayInfo { id: format!("id-{x}"), x, y: 0, width: 1000, height: 1000, primary, name: String::new(), connector: None }, (2000, 2000));
        let displays = [display(0, true), display(1000, false)];
        let window = |app: &str, x, active, fullscreen| WindowInfo {
            id: 1,
            title: format!("{app} window"),
            app: app.into(),
            pid: Some(7),
            x,
            y: 0,
            width: 500,
            height: 500,
            active,
            fullscreen,
        };
        let windows = [window("Notes", 1100, false, false), window("Passwords", 100, true, false), window("Zed", 1200, false, false)];
        let view = view_of(&displays, &windows, None, |pid| Some(format!("app.{pid}")));
        let focused = view.focused.unwrap();
        assert_eq!((focused.app.as_str(), focused.app_id.as_deref(), focused.display), ("Passwords", Some("app.7"), 1));
        assert_eq!(focused.rect, crate::ScreenRect { x: 200, y: 0, width: 1000, height: 1000 });
        assert_eq!(view.work, [1, 2]);
        let tops: Vec<(u32, &str, Option<&str>)> =
            view.tops.iter().map(|top| (top.display, top.app.as_str(), top.app_id.as_deref())).collect();
        assert_eq!(tops, [(1, "Passwords", Some("app.7")), (2, "Notes", Some("app.7"))]);
        // A picked display that is gone: no work display; no focus = None.
        let view = view_of(&displays, &windows[..1], Some("id-unplugged"), |_| None);
        assert_eq!((view.focused, view.work), (None, vec![]));
    }
}
