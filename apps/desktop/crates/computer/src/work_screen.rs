//! NEW-59: THE work screen (`fixtures/work-screen.json`): which displays
//! agents may drive, the `wrong_display` refusals the hub answers with,
//! the run's current display and where the voice overlay may open. The
//! pure rules replay the fixture; the OS reads go through the backend.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::OnceLock;

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
pub const NOT_WORK_SCREEN: &str = "notWorkScreen";
pub const WINDOW_ELSEWHERE: &str = "windowElsewhere";
pub const GAMING: &str = "gaming";
pub const KEYS_ELSEWHERE: &str = "keysElsewhere";

/// What the gate reads (`gateCases[].state`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    pub count: usize,
    /// 1-based; `None` = Auto.
    pub picked: Option<u32>,
    /// 0-based displays holding a fullscreen window.
    pub fullscreen: Vec<usize>,
    /// 0-based display of the focused window.
    pub active: Option<usize>,
    pub active_fullscreen: bool,
}

/// One tool call as the gate sees it (`gateCases[].calls[].call`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Call<'a> {
    pub tool: &'a str,
    /// `screenshot`'s 0-based `display` argument.
    pub display: Option<usize>,
    /// `activate_window`: the 0-based display holding the window's centre.
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
        let picked = picked as usize;
        if picked >= 1 && picked <= state.count {
            return vec![picked - 1];
        }
    }
    (0..state.count).filter(|index| !state.fullscreen.contains(index)).collect()
}

fn refuse(kind: &'static str, number: Option<usize>, work: &[usize]) -> Refusal {
    let template = fixture()["copy"]["refusals"][kind].as_str().expect("work-screen refusal copy");
    let work = work.iter().map(|index| (index + 1).to_string()).collect::<Vec<_>>().join(", ");
    let text = template
        .replace("{number}", &number.map(|number| number.to_string()).unwrap_or_default())
        .replace("{work}", &work);
    Refusal { kind, text }
}

/// Whether the hub runs `call` now, else the `wrong_display` refusal.
pub fn gate(state: &State, call: Call<'_>) -> Result<(), Refusal> {
    let work = work_displays(state);
    if call.tool == "wait" {
        return Ok(());
    }
    if work.is_empty() {
        return Err(refuse(NO_WORK_SCREEN, None, &work));
    }
    if call.tool == "screenshot" {
        if let Some(display) = call.display.filter(|display| !work.contains(display)) {
            return Err(refuse(NOT_WORK_SCREEN, Some(display + 1), &work));
        }
    }
    if call.tool == "activate_window" {
        if let Some(display) = call.window_display.filter(|display| !work.contains(display)) {
            return Err(refuse(WINDOW_ELSEWHERE, Some(display + 1), &work));
        }
    }
    if let Some(active) = state.active {
        if is_input(call.tool) {
            if state.active_fullscreen && !work.contains(&active) {
                return Err(refuse(GAMING, Some(active + 1), &work));
            }
            if matches!(call.tool, "type" | "key") && !work.contains(&active) {
                return Err(refuse(KEYS_ELSEWHERE, Some(active + 1), &work));
            }
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

/// The device's Work screen setting, 1-based; 0 = Auto.
static PICKED: AtomicU32 = AtomicU32::new(0);

pub(crate) fn set_picked(number: Option<u32>) {
    PICKED.store(number.unwrap_or(0), Ordering::Release);
}

pub(crate) fn picked() -> Option<u32> {
    Some(PICKED.load(Ordering::Acquire)).filter(|number| *number > 0)
}

/// The 0-based display holding the point; `None` off every display.
pub(crate) fn display_at(displays: &[DisplayInfo], x: i32, y: i32) -> Option<usize> {
    displays.iter().position(|display| display.contains(x, y))
}

/// The 0-based display holding `window`'s centre.
pub(crate) fn window_display(displays: &[DisplayInfo], window: &WindowInfo) -> Option<usize> {
    display_at(displays, window.x + window.width as i32 / 2, window.y + window.height as i32 / 2)
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
    let state = state_of(&displays, &windows, picked());
    let work = work_displays(&state);
    Some(ScreenState {
        displays: displays.iter().enumerate().map(|(index, display)| public(index, display)).collect(),
        fullscreen: state.fullscreen,
        work,
    })
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
            };
            assert_eq!(work_displays(&state), indexes(&case["work"]), "{name}: work");
            for step in case["calls"].as_array().unwrap() {
                let call = Call {
                    tool: step["call"]["tool"].as_str().unwrap(),
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
    fn the_state_reads_fullscreen_and_focus_off_the_window_centres() {
        let display = |x, primary| DisplayInfo { x, y: 0, width: 1000, height: 1000, primary, name: String::new() };
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
        assert_eq!(state, State { count: 2, picked: None, fullscreen: vec![1], active: Some(1), active_fullscreen: true });
        assert_eq!(work_displays(&state), [0]);
        assert_eq!(state_of(&displays, &[], Some(2)).active, None);
    }
}
