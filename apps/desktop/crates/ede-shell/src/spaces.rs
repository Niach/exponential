//! #EXP-2024 (6b): the live view of the agent spaces on this machine, the
//! shell's side: what the compositor lists, the titles the accounts hold,
//! the pictures the capture loop takes, the ONE pinned view (by session)
//! and what the capture loop photographs. `/spaces` feeds the taskbar's
//! Agent spaces button (shown while `count` is above 0) and its hosted
//! Popover list; `/spaceView` the pinned view (`space-view.json`). The rows
//! and the view's box are the `uicore-model` folds (`fixtures/spaces.json`).
//!
//! View only: nothing here acts inside a space; the list's Close ends a
//! space (`DestroySpace`), never its session.

use std::collections::HashMap;

use ede_protocol::SpaceInfo;
use serde_json::{json, Value};
use uicore_model::spaces::{space_rows, space_view, Display, Extent, SpaceInput, SpaceRow, SpaceSession, SpaceView};

use crate::captures::{Plan, Shot};

/// The taskbar's hosted Popover that lists the spaces (`taskbar.json`).
pub const LIST: &str = "agent_spaces";
/// The pinned view's on-demand layer document.
pub const VIEW: &str = "space-view";
/// A list thumbnail's box in logical px (the Row's leading picture).
pub const THUMB: (u32, u32) = (96, 60);

/// The pinned view: its session (the pin follows a session whose space is
/// made anew) and its size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pinned {
    pub session: String,
    pub large: bool,
    /// A capture found its space gone: the next list says whether the
    /// session has another one (followed) or none (the view closes).
    pub checking: bool,
}

/// What the shell knows of the spaces.
#[derive(Debug, Default)]
pub struct Spaces {
    /// The compositor's spaces (the capture loop's ListSpaces).
    pub list: Vec<SpaceInfo>,
    /// The spaces' sessions as the shell's accounts hold them (titles).
    pub sessions: Vec<SpaceSession>,
    /// Each space's latest picture: the list's thumbnail (`false`) and the
    /// view's (`true`); "" = Image's fallback (captures failed for 5 s).
    pub pictures: HashMap<(String, bool), String>,
    pub pinned: Option<Pinned>,
    /// The view's layer size as last opened or resized.
    pub shown_size: Option<(u32, u32)>,
    /// `EDE_SHELL_DEMO_SPACES` (the goldens): sample spaces nothing lists or captures.
    pub demo: bool,
}

/// Why a space's picture cannot be seen now.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pause {
    /// The screen is locked: no capture at all (the windows' neither).
    pub locked: bool,
    /// A fullscreen window over the first display: the compositor draws
    /// no top layer and none of their popups, so neither the view nor the
    /// list shows.
    pub fullscreen: bool,
    /// The chat or Settings opened after the view, over it (a later top
    /// layer is drawn above an earlier one). The list, a popup of the
    /// taskbar, is drawn above every top layer: never covered by them.
    pub view_covered: bool,
}

impl Spaces {
    fn inputs(&self) -> Vec<SpaceInput> {
        self.list.iter().map(|s| SpaceInput { id: s.id.to_string(), session: s.session_id.to_string() }).collect()
    }

    /// The list's rows (the fold).
    pub fn rows(&self) -> Vec<SpaceRow> {
        space_rows(&self.inputs(), &self.sessions)
    }

    /// The session ids whose rows the titles need: the spaces' sessions.
    pub fn wanted_sessions(&self) -> Vec<String> {
        self.list.iter().map(|s| s.session_id.to_string()).collect()
    }

    /// `/spaces`: the count (the button) and the rows with their pictures.
    pub fn feed(&self) -> Value {
        let rows: Vec<Value> = self
            .rows()
            .into_iter()
            .map(|row| {
                let picture = self.pictures.get(&(row.id.clone(), false)).cloned().unwrap_or_default();
                json!({"id": row.id, "session": row.session, "title": row.title, "subtitle": row.subtitle, "picture": picture})
            })
            .collect();
        json!({"count": self.list.len(), "rows": rows})
    }

    /// The pinned session's space (its lowest number when it has two).
    pub fn pinned_space(&self) -> Option<&SpaceInfo> {
        let pinned = self.pinned.as_ref()?;
        let rows = self.rows();
        let id = rows.iter().find(|r| r.session == pinned.session)?.id.clone();
        self.list.iter().find(|s| s.id.as_str() == id)
    }

    /// The pinned view's box on `display` (`None` = nothing pinned, or its
    /// session has no space now).
    pub fn view(&self, display: Display) -> Option<(&SpaceInfo, SpaceView)> {
        let pinned = self.pinned.as_ref()?;
        let space = self.pinned_space()?;
        let size = Extent { width: space.width.max(0) as u32, height: space.height.max(0) as u32 };
        Some((space, space_view(size, display, pinned.large)))
    }

    /// Whether the view's layer should be open: pinned, and its session has
    /// a space (or the loop is checking a gone one).
    pub fn view_wanted(&self) -> bool {
        self.pinned.as_ref().is_some_and(|p| p.checking || self.pinned_space().is_some())
    }

    /// `/spaceView`: the pinned view's title, picture and box.
    pub fn view_feed(&self, display: Display) -> Value {
        let Some(pinned) = self.pinned.as_ref() else { return json!({"open": false}) };
        let Some((space, view)) = self.view(display) else { return json!({"open": self.view_wanted(), "session": pinned.session, "title": "", "picture": ""}) };
        let row = self.rows().into_iter().find(|r| r.id == space.id.as_str());
        let picture = self.pictures.get(&(space.id.to_string(), true)).cloned().unwrap_or_default();
        json!({
            "open": true,
            "session": pinned.session,
            "space": space.id.as_str(),
            "title": row.as_ref().map(|r| r.title.clone()).unwrap_or_default(),
            "subtitle": row.and_then(|r| r.subtitle),
            "picture": picture,
            "large": view.large,
            "canGrow": view.can_grow,
            "size": {"width": view.width, "height": view.height},
            "pictureSize": view.picture,
        })
    }

    /// The compositor's new list: the pictures of spaces that went go, the
    /// pin follows its session's (new) space or ends with it.
    pub fn listed(&mut self, list: Vec<SpaceInfo>) {
        if self.demo {
            return;
        }
        self.list = list;
        let ids: Vec<String> = self.list.iter().map(|s| s.id.to_string()).collect();
        self.pictures.retain(|(space, _), _| ids.contains(space));
        if self.pinned_space().is_none() {
            self.pinned = None;
        } else if let Some(p) = self.pinned.as_mut() {
            p.checking = false;
        }
    }

    /// A capture found `space` gone: it leaves the list at once; a pinned
    /// view on it waits for the next list.
    pub fn gone(&mut self, space: &str) {
        self.list.retain(|s| s.id.as_str() != space);
        self.pictures.retain(|(s, _), _| s != space);
        if self.pinned.is_some() && self.pinned_space().is_none() {
            if let Some(p) = self.pinned.as_mut() {
                p.checking = true;
            }
        }
    }

    /// `ede_space {session | null, size?}`: pins a session's view (one at a
    /// time: another session replaces it), sets its size, or unpins.
    pub fn pin(&mut self, session: Option<&str>, large: Option<bool>) {
        self.pinned = match session {
            None => None,
            Some(s) => match self.pinned.take() {
                Some(p) if p.session == s => Some(Pinned { large: large.unwrap_or(p.large), ..p }),
                _ => Some(Pinned { session: s.to_string(), large: large.unwrap_or(false), checking: false }),
            },
        };
        if self.pinned.is_some() && !self.view_wanted() {
            self.pinned = None;
        }
    }

    /// A picture arrived (or the fallback, "").
    pub fn picture(&mut self, space: &str, view: bool, url: String) {
        if self.list.iter().any(|s| s.id.as_str() == space) {
            self.pictures.insert((space.to_string(), view), url);
        }
    }

    /// What the capture loop photographs of the spaces: the open list's
    /// thumbnails and the open view's space, at the display's scale;
    /// nothing while paused.
    pub fn targets(&self, view_open: bool, list_open: bool, pause: Pause, display: Display) -> (Option<Shot>, Vec<Shot>) {
        if pause.locked || pause.fullscreen || self.demo {
            return (None, Vec::new());
        }
        let view = (view_open && !pause.view_covered).then(|| self.view(display)).flatten().map(|(space, view)| Shot { space: space.id.to_string(), pixels: (view.pixels.width, view.pixels.height) });
        let device = |v: u32| (v as f64 * display.scale).round() as u32;
        let thumbs = if list_open { self.rows().into_iter().map(|r| Shot { space: r.id, pixels: (device(THUMB.0), device(THUMB.1)) }).collect() } else { Vec::new() };
        (view, thumbs)
    }
}

/// `EDE_SHELL_DEMO_SPACES=list|view` (the goldens, with `EDE_SHELL_DEMO`):
/// three sample spaces (a chat's, a child's under its chat, one no account
/// holds) with drawn pictures; `view` pins the first.
pub fn demo_mode() -> Option<String> {
    std::env::var("EDE_SHELL_DEMO_SPACES").ok().filter(|v| v == "list" || v == "view")
}

impl Spaces {
    pub fn demo(mode: &str) -> Self {
        let space = |id: &str, session: &str| SpaceInfo { id: ede_protocol::SpaceId::new(id).unwrap(), session_id: ede_protocol::SessionId::new(session).unwrap(), output: id.into(), width: 1280, height: 800, wayland_display: String::new() };
        let session = |id: &str, title: &str, parent: Option<&str>, root: &str| SpaceSession { id: id.into(), title: title.into(), parent_id: parent.map(String::from), root_id: Some(root.into()) };
        let mut s = Spaces {
            list: vec![space("space-1", "demo-chat"), space("space-2", "demo-child"), space("space-4", "exponential-ctl-4242")],
            sessions: vec![
                session("demo-chat", "Fix the sign-in page", None, "demo-chat"),
                session("demo-root", "Release 1.4", None, "demo-root"),
                session("demo-child", "Check the store pictures", Some("demo-root"), "demo-root"),
            ],
            demo: true,
            ..Default::default()
        };
        for (i, id) in ["space-1", "space-2", "space-4"].into_iter().enumerate() {
            s.pictures.insert((id.to_string(), false), demo_picture(THUMB.0, THUMB.1, i).unwrap_or_default());
        }
        if mode == "view" {
            s.pin(Some("demo-chat"), None);
            s.pictures.insert(("space-1".to_string(), true), demo_picture(640, 400, 0).unwrap_or_default());
        }
        s
    }
}

/// A drawn sample of a space's screen: its background, a window with a
/// title bar and lines of text, in one of three tones.
fn demo_picture(w: u32, h: u32, tone: usize) -> Option<String> {
    let ground = [[30u8, 41, 59], [39, 39, 42], [20, 48, 46]][tone % 3];
    let accent = [[59u8, 130, 246], [234, 179, 8], [16, 185, 129]][tone % 3];
    let (wx0, wy0, wx1, wy1) = (w / 8, h / 8, w * 7 / 8, h * 7 / 8);
    let bar = wy0 + (h / 14).max(2);
    let (step, thick) = ((h / 10).max(3), (h / 40).max(1));
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let inside = x >= wx0 && x < wx1 && y >= wy0 && y < wy1;
            // Lines of text, each its own length.
            let row = y.saturating_sub(bar + step / 2);
            let end = wx1 - (wx1 - wx0) * [10u32, 30, 18, 42, 24, 36][(row / step) as usize % 6] / 100;
            let line = inside && y > bar + step / 2 && row % step < thick && x > wx0 + w / 20 && x < end;
            let px = match (inside, y < bar, line) {
                (false, _, _) => ground,
                (true, true, _) => accent,
                (true, false, true) => [113, 113, 122],
                (true, false, false) => [244, 244, 245],
            };
            rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
        }
    }
    crate::thumbs::rgba_data_url(w, h, &rgba)
}

/// THE capture loop's plan: the windows (the switcher, the previews, Expo)
/// unless the screen is locked, and the spaces' targets.
pub fn plan(windows: Vec<String>, spaces: &Spaces, view_open: bool, list_open: bool, pause: Pause, display: Display) -> Plan {
    let (view, thumbs) = spaces.targets(view_open, list_open, pause, display);
    Plan { windows: if pause.locked { Vec::new() } else { windows }, view, thumbs }
}

/// A fullscreen person window, not minimised, on the active workspace,
/// over the first display (`output` = its x, y, width, height): the view
/// on that display is not drawn (the compositor draws no top layer over a
/// fullscreen window), so it is not captured.
pub fn fullscreen_over(windows: &[ede_protocol::WindowInfo], active: usize, output: Option<(i32, i32, i32, i32)>) -> bool {
    windows.iter().filter(|w| w.fullscreen && !w.minimized && w.workspace == active && w.is_persons()).any(|w| match output {
        Some((x, y, width, height)) => w.x < x + width && x < w.x + w.width && w.y < y + height && y < w.y + w.height,
        None => true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ede_protocol::{SessionId, SpaceId, WindowId, WindowInfo};

    fn space(id: &str, session: &str) -> SpaceInfo {
        SpaceInfo { id: SpaceId::new(id).unwrap(), session_id: SessionId::new(session).unwrap(), output: id.into(), width: 1280, height: 800, wayland_display: format!("/run/user/1000/{id}") }
    }

    const DISPLAY: Display = Display { width: 1920, height: 1080, scale: 1.0 };

    fn spaces() -> Spaces {
        let mut s = Spaces { list: vec![space("space-1", "s1"), space("space-2", "s2")], ..Default::default() };
        s.sessions = vec![SpaceSession { id: "s1".into(), title: "Fix the login page".into(), parent_id: None, root_id: Some("s1".into()) }];
        s
    }

    fn open_everything() -> Spaces {
        let mut s = spaces();
        s.pin(Some("s1"), None);
        s
    }

    fn targets(s: &Spaces, list_open: bool, pause: Pause) -> (Option<String>, Vec<String>) {
        let (view, thumbs) = s.targets(true, list_open, pause, DISPLAY);
        (view.map(|v| v.space), thumbs.into_iter().map(|t| t.space).collect())
    }

    /// The feed: the count, the rows in order, a picture or "" (the fallback).
    #[test]
    fn the_feed_counts_the_spaces_and_carries_their_pictures() {
        let mut s = spaces();
        s.picture("space-2", false, "data:image/png;base64,AA".into());
        s.picture("space-9", false, "data:image/png;base64,BB".into());
        assert_eq!(
            s.feed(),
            json!({"count": 2, "rows": [
                {"id": "space-1", "session": "s1", "title": "Fix the login page", "subtitle": null, "picture": ""},
                {"id": "space-2", "session": "s2", "title": "Agent space 2", "subtitle": null, "picture": "data:image/png;base64,AA"}
            ]})
        );
        assert_eq!(Spaces::default().feed(), json!({"count": 0, "rows": []}), "no space: no button");
    }

    /// Section 8, 6b: every close path ends the space targets.
    #[test]
    fn every_close_path_ends_the_space_targets() {
        let s = open_everything();
        assert_eq!(targets(&s, true, Pause::default()), (Some("space-1".into()), vec!["space-1".into(), "space-2".into()]));
        // The list's Popover closed: its thumbnails stop, the view stays.
        assert_eq!(targets(&s, false, Pause::default()), (Some("space-1".into()), vec![]));
        // The view's close button.
        let mut closed = open_everything();
        closed.pin(None, None);
        assert_eq!(targets(&closed, false, Pause::default()), (None, vec![]));
        // The view's layer not open (the document missing, the layer gone).
        assert_eq!(s.targets(false, false, Pause::default(), DISPLAY), (None, vec![]));
        // Locked; under a fullscreen window.
        assert_eq!(targets(&s, true, Pause { locked: true, ..Pause::default() }), (None, vec![]));
        assert_eq!(targets(&s, true, Pause { fullscreen: true, ..Pause::default() }), (None, vec![]));
        // The chat or Settings opened over the view: the view stops, the
        // list (a popup above every top layer) does not.
        assert_eq!(targets(&s, true, Pause { view_covered: true, ..Pause::default() }), (None, vec!["space-1".into(), "space-2".into()]));
        // The pinned session's space ended (Close on its row, its session's end).
        let mut ended = open_everything();
        ended.listed(vec![space("space-2", "s2")]);
        assert_eq!(ended.pinned, None);
        assert_eq!(targets(&ended, false, Pause::default()), (None, vec![]));
        // The last space ended: nothing to list.
        let mut none = open_everything();
        none.listed(vec![]);
        assert_eq!(targets(&none, true, Pause::default()), (None, vec![]));
        assert_eq!(none.feed()["count"], 0);
    }

    /// Closing the switcher, the previews and Expo empties the windows'
    /// targets and keeps a pinned view's; the lock empties both.
    #[test]
    fn closing_the_switcher_previews_and_expo_keeps_a_pinned_views_target() {
        let s = open_everything();
        let open = plan(vec!["w1".into(), "w2".into()], &s, true, false, Pause::default(), DISPLAY);
        assert_eq!(open.windows.len(), 2);
        let closed = plan(Vec::new(), &s, true, false, Pause::default(), DISPLAY);
        assert!(closed.windows.is_empty());
        assert_eq!(closed.view, open.view, "the pinned view goes on");
        assert!(closed.view.is_some());
        let locked = plan(vec!["w1".into()], &s, true, true, Pause { locked: true, ..Pause::default() }, DISPLAY);
        assert_eq!(locked, Plan::default(), "no capture between lock and unlock");
        let fullscreen = plan(vec!["w1".into()], &s, true, true, Pause { fullscreen: true, ..Pause::default() }, DISPLAY);
        assert_eq!((fullscreen.windows.len(), fullscreen.view, fullscreen.thumbs.len()), (1, None, 0), "under a fullscreen window: the spaces stop, Alt+Tab's windows not");
    }

    /// The pin is by session: a session whose space id changes is followed;
    /// a space that is gone (`not_found`) closes its view at the next list
    /// unless its session has another one.
    #[test]
    fn a_session_whose_space_id_changes_is_followed_and_not_found_closes_the_view() {
        let mut s = open_everything();
        s.listed(vec![space("space-2", "s2"), space("space-3", "s1")]);
        assert_eq!(s.pinned_space().map(|p| p.id.to_string()), Some("space-3".into()), "followed");
        assert_eq!(targets(&s, false, Pause::default()).0, Some("space-3".into()));
        s.gone("space-3");
        assert!(s.view_wanted(), "the view waits for the next list");
        assert_eq!(targets(&s, false, Pause::default()).0, None, "nothing to capture meanwhile");
        s.listed(vec![space("space-2", "s2"), space("space-4", "s1")]);
        assert_eq!(s.pinned_space().map(|p| p.id.to_string()), Some("space-4".into()), "made anew: followed");
        s.gone("space-4");
        s.listed(vec![space("space-2", "s2")]);
        assert_eq!(s.pinned, None, "gone for good: the view closes");
        assert!(!s.view_wanted());
    }

    /// One view at a time: another row replaces it; the size button keeps
    /// the session; a session without a space pins nothing.
    #[test]
    fn one_pinned_view_by_session() {
        let mut s = spaces();
        s.pin(Some("s1"), None);
        s.pin(Some("s1"), Some(true));
        assert_eq!(s.pinned, Some(Pinned { session: "s1".into(), large: true, checking: false }));
        s.pin(Some("s2"), None);
        assert_eq!(s.pinned, Some(Pinned { session: "s2".into(), large: false, checking: false }), "replaced");
        s.pin(Some("nobody"), None);
        assert_eq!(s.pinned, None);
        let mut big = Spaces { list: vec![space("space-1", "s1")], ..Default::default() };
        big.pin(Some("s1"), Some(true));
        let wide = Display { width: 2560, height: 1440, scale: 2.0 };
        let feed = big.view_feed(wide);
        assert_eq!((feed["size"].clone(), feed["pictureSize"].clone(), feed["large"].clone()), (json!({"width": 1282, "height": 842}), json!({"width": 1280, "height": 800}), json!(true)));
        let (view, thumbs) = big.targets(true, true, Pause::default(), wide);
        assert_eq!(view.map(|v| v.pixels), Some((1280, 800)), "at scale 2, never above the space");
        assert_eq!(thumbs[0].pixels, (192, 120), "the thumbnails at the display's scale");
    }

    fn window(fullscreen: bool, minimized: bool, workspace: usize, x: i32) -> WindowInfo {
        WindowInfo { id: WindowId::new("w1").unwrap(), title: None, app_id: Some("mpv".into()), x, y: 0, width: 1920, height: 1080, focused: true, fullscreen, protected: false, x11: false, workspace, minimized, space: None }
    }

    #[test]
    fn a_fullscreen_window_over_the_first_display_covers_the_view() {
        let first = Some((0, 0, 1920, 1080));
        assert!(fullscreen_over(&[window(true, false, 0, 0)], 0, first));
        assert!(!fullscreen_over(&[window(false, false, 0, 0)], 0, first), "not fullscreen");
        assert!(!fullscreen_over(&[window(true, true, 0, 0)], 0, first), "minimised");
        assert!(!fullscreen_over(&[window(true, false, 1, 0)], 0, first), "another workspace");
        assert!(!fullscreen_over(&[window(true, false, 0, 1920)], 0, first), "the second display");
    }
}
