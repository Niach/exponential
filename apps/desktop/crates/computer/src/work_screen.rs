//! NEW-59: THE work screen (`fixtures/work-screen.json`): which displays
//! agents may drive, the `wrong_display` refusals the hub answers with,
//! the run's current display and where the voice overlay may open. The
//! pure rules replay the fixture; the OS reads go through the backend.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

use crate::backend::{self, DisplayInfo, WindowInfo};
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

/// Whether `tool` acts on the foreground (`inputTools`).
pub(crate) fn is_input(tool: &str) -> bool {
    input_tools().iter().any(|input| input == tool)
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
        if matches!(call.tool, "type" | "key") && !work.contains(&active) {
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
static PICKED: Mutex<Option<String>> = Mutex::new(None);

pub(crate) fn set_picked(id: Option<String>) {
    *PICKED.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = id.filter(|id| !id.is_empty());
}

pub(crate) fn picked() -> Option<String> {
    PICKED.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
}

/// The gate's `picked` off the stored id: the 1-based number of the display
/// carrying it, 0 when none does; `None` = Auto.
pub(crate) fn resolve(displays: &[DisplayInfo], picked: Option<&str>) -> Option<u32> {
    let id = picked?;
    Some(displays.iter().position(|display| display.id == id).map_or(0, |index| index as u32 + 1))
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
    }
}

/// The display the banner floats on (0-based): the current display of the
/// last call that went through; cleared when the lock releases.
const NO_BANNER: usize = usize::MAX;
static BANNER: AtomicUsize = AtomicUsize::new(NO_BANNER);

pub(crate) fn set_banner(display: Option<usize>) {
    BANNER.store(display.unwrap_or(NO_BANNER), Ordering::Release);
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
    let backend = backend::open().ok()?;
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
    let picked = resolve(&displays, picked().as_deref());
    let state = state_of(&displays, &windows, picked);
    let work = work_displays(&state);
    Some(ScreenState {
        displays: displays.iter().enumerate().map(|(index, display)| public(index, display)).collect(),
        fullscreen: state.fullscreen,
        work,
    })
}

/// The displays with their pixel sizes, re-read at most every
/// `DISPLAYS_TTL`: screen memory's 4 Hz focus poll would otherwise pay
/// two display enumerations per look (~1.5 ms on a Mac) for a layout that
/// changes when a monitor is plugged.
const DISPLAYS_TTL: std::time::Duration = std::time::Duration::from_secs(3);
type DisplayCache = (std::time::Instant, Vec<(DisplayInfo, (u32, u32))>);
static DISPLAY_CACHE: std::sync::Mutex<Option<DisplayCache>> = std::sync::Mutex::new(None);

pub(crate) fn screen_view() -> Option<crate::ScreenView> {
    let backend = backend::open().ok()?;
    let displays = {
        let mut cache = DISPLAY_CACHE.lock().unwrap();
        match cache.as_ref().filter(|(at, _)| at.elapsed() < DISPLAYS_TTL) {
            Some((_, displays)) => displays.clone(),
            None => {
                let displays: Vec<(DisplayInfo, (u32, u32))> = backend
                    .displays()
                    .ok()?
                    .into_iter()
                    .map(|display| {
                        let pixels = backend::pixel_size(display.x, display.y, display.width, display.height);
                        (display, pixels)
                    })
                    .collect();
                *cache = Some((std::time::Instant::now(), displays.clone()));
                displays
            }
        }
    };
    let windows = backend.list_windows().ok()?;
    Some(view_of(&displays, &windows, picked().as_deref(), backend::app_id))
}

/// [`screen_view`] off one display set (with pixel sizes) and one window
/// list (top first); `app_id` names a pid's app (asked for the focused
/// window only).
pub(crate) fn view_of(
    displays: &[(DisplayInfo, (u32, u32))],
    windows: &[WindowInfo],
    picked: Option<&str>,
    app_id: impl Fn(u32) -> Option<String>,
) -> crate::ScreenView {
    let infos: Vec<DisplayInfo> = displays.iter().map(|(display, _)| display.clone()).collect();
    let picked = resolve(&infos, picked);
    let work = work_displays(&state_of(&infos, windows, picked)).into_iter().map(|index| index as u32 + 1).collect();
    let mut tops: Vec<crate::TopWindow> = Vec::new();
    for window in windows {
        if let Some(index) = window_display(&infos, window) {
            let display = index as u32 + 1;
            if !tops.iter().any(|top| top.display == display) {
                tops.push(crate::TopWindow { display, app: window.app.clone(), title: window.title.clone() });
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

pub(crate) fn banner_display() -> Option<Display> {
    let index = BANNER.load(Ordering::Acquire);
    if index == NO_BANNER {
        return None;
    }
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
        for case in fixture()["gateCases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let raw = &case["state"];
            let state = State {
                count: raw["count"].as_u64().unwrap() as usize,
                picked: raw["picked"].as_u64().map(|picked| picked as u32),
                fullscreen: indexes(&raw["fullscreen"]),
                active: index(&raw["active"]),
                active_fullscreen: raw["activeFullscreen"].as_bool().unwrap(),
                windows_read: raw["windowsRead"].as_bool().unwrap_or(true),
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
        let display = DisplayInfo { id: String::new(), x: 1920, y: 0, width: 1512, height: 982, primary: false, name: String::new() };
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
        let display = |x, primary| DisplayInfo { id: format!("id-{x}"), x, y: 0, width: 1000, height: 1000, primary, name: String::new() };
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
        assert_eq!(state, State { count: 2, picked: None, fullscreen: vec![1], active: Some(1), active_fullscreen: true, windows_read: true });
        assert_eq!(work_displays(&state), [0]);
        assert_eq!(state_of(&displays, &[], Some(2)).active, None);
    }

    /// #FEED-161: a window's display = the one its rect overlaps MOST
    /// (ties: the lower index), never its centre alone.
    #[test]
    fn a_window_belongs_to_the_display_it_overlaps_most() {
        // Display 0 is short (600 tall), display 1 is 1000.
        let displays = [
            DisplayInfo { id: "a".into(), x: 0, y: 0, width: 1000, height: 600, primary: true, name: String::new() },
            DisplayInfo { id: "b".into(), x: 1000, y: 0, width: 1000, height: 1000, primary: false, name: String::new() },
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
        let display = |id: &str| DisplayInfo { id: id.into(), x: 0, y: 0, width: 10, height: 10, primary: false, name: String::new() };
        assert_eq!(resolve(&[display("a"), display("b")], Some("b")), Some(2));
        assert_eq!(resolve(&[display("b"), display("a")], Some("b")), Some(1));
        assert_eq!(resolve(&[display("a")], Some("b")), Some(0));
        assert_eq!(resolve(&[display("a")], None), None);
        let state = State { count: 1, picked: resolve(&[display("a")], Some("b")), windows_read: true, ..State::default() };
        assert!(work_displays(&state).is_empty());
    }

    /// NEW-58: screen memory's view: the focused window with its app id,
    /// the work displays 1-based, each display's top window.
    #[test]
    fn the_screen_view_reads_focus_work_and_top_windows_in_one_list() {
        let display = |x, primary| (DisplayInfo { id: format!("id-{x}"), x, y: 0, width: 1000, height: 1000, primary, name: String::new() }, (2000, 2000));
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
        let tops: Vec<(u32, &str)> = view.tops.iter().map(|top| (top.display, top.app.as_str())).collect();
        assert_eq!(tops, [(1, "Passwords"), (2, "Notes")]);
        // A picked display that is gone: no work display; no focus = None.
        let view = view_of(&displays, &windows[..1], Some("id-unplugged"), |_| None);
        assert_eq!((view.focused, view.work), (None, vec![]));
    }
}
