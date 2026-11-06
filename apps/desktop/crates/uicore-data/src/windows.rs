//! The compositor's window list as shell events: the host asks
//! ede-protocol for the whole list on every `window.*` / `workspace.*`
//! event, and [`window_events`] turns the difference from the last list
//! into the [`ShellEvent`]s the reducer takes, so the model never drifts
//! from what the compositor shows.

use ede_protocol::{WindowInfo, WorkspaceInfo};
use uicore_model::{ShellEvent, ShellState};

/// The events that take `model` from `prev` to `next` (protected windows
/// are not shell windows: they never appear), in an order the reducer
/// accepts: closed, opened, then the changed ones, then the focus.
pub fn window_events(model: &ShellState, next: &[WindowInfo]) -> Vec<ShellEvent> {
    let next: Vec<&WindowInfo> = next.iter().filter(|w| !w.protected).collect();
    let mut out = Vec::new();
    for w in &model.windows {
        if !next.iter().any(|n| n.id.to_string() == w.id) {
            out.push(ShellEvent::WindowClosed { id: w.id.clone() });
        }
    }
    for n in &next {
        let id = n.id.to_string();
        let title = n.title.clone().unwrap_or_default();
        let app = n.app_id.clone().unwrap_or_default();
        match model.windows.iter().find(|w| w.id == id) {
            None => out.push(ShellEvent::WindowOpened { id: id.clone(), app, title, workspace: n.workspace }),
            Some(w) => {
                if w.title != title {
                    out.push(ShellEvent::WindowTitled { id: id.clone(), title });
                }
                if w.workspace != n.workspace {
                    out.push(ShellEvent::WindowMoved { id: id.clone(), workspace: n.workspace as i64 });
                }
                if w.minimized != minimized(n) {
                    out.push(if minimized(n) { ShellEvent::WindowMinimized { id: id.clone() } } else { ShellEvent::WindowRestored { id: id.clone() } });
                }
            }
        }
    }
    for n in &next {
        let id = n.id.to_string();
        if n.focused && !model.windows.iter().any(|w| w.id == id && w.focused) {
            out.push(ShellEvent::WindowFocused { id });
        }
    }
    out
}

/// `WindowInfo.minimized` arrives with the compositor's minimize (#EDE-48);
/// until the protocol carries it, no listed window is minimized.
fn minimized(_w: &WindowInfo) -> bool {
    false
}

/// The workspace events that take `model` to the compositor's list.
pub fn workspace_events(model: &ShellState, workspaces: &[WorkspaceInfo]) -> Vec<ShellEvent> {
    let mut out = Vec::new();
    let count = workspaces.len().max(1);
    if model.workspaces.count != count {
        out.push(ShellEvent::WorkspaceCount { count: count as i64 });
    }
    if let Some(active) = workspaces.iter().find(|w| w.active).map(|w| w.index) {
        if model.workspaces.active != active {
            out.push(ShellEvent::WorkspaceSwitched { active: active as i64 });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ede_protocol::WindowId;
    use uicore_model::reduce;

    fn info(id: &str, app: &str, title: &str, focused: bool, workspace: usize) -> WindowInfo {
        WindowInfo { id: WindowId::new(id).unwrap(), title: Some(title.into()), app_id: Some(app.into()), x: 0, y: 0, width: 10, height: 10, focused, fullscreen: false, protected: false, x11: false, workspace }
    }

    fn apply(model: &ShellState, events: &[ShellEvent]) -> ShellState {
        events.iter().fold(model.clone(), |s, e| reduce(&s, e))
    }

    #[test]
    fn a_fresh_list_opens_every_window_and_focuses_the_focused_one() {
        let model = ShellState::default();
        let list = vec![info("w1", "firefox", "Firefox", false, 0), info("w2", "terminal", "Terminal", true, 0)];
        let events = window_events(&model, &list);
        assert_eq!(events.len(), 3);
        let after = apply(&model, &events);
        assert_eq!(after.windows.len(), 2);
        assert_eq!(after.mru, vec!["w2", "w1"]);
        assert!(after.windows[1].focused);
    }

    #[test]
    fn the_difference_closes_retitles_moves_and_refocuses() {
        let model = apply(&ShellState::default(), &window_events(&ShellState::default(), &[info("w1", "firefox", "A", true, 0), info("w2", "terminal", "T", false, 0)]));
        let model = reduce(&model, &ShellEvent::WorkspaceCount { count: 2 });
        let next = vec![info("w2", "terminal", "T2", true, 1)];
        let events = window_events(&model, &next);
        assert!(matches!(events[0], ShellEvent::WindowClosed { ref id } if id == "w1"));
        assert!(events.iter().any(|e| matches!(e, ShellEvent::WindowTitled { title, .. } if title == "T2")));
        assert!(events.iter().any(|e| matches!(e, ShellEvent::WindowMoved { workspace: 1, .. })));
        assert!(matches!(events.last(), Some(ShellEvent::WindowFocused { id }) if id == "w2"));
        let after = apply(&model, &events);
        assert_eq!(after.windows.len(), 1);
        assert_eq!(after.windows[0].workspace, 1);
        // Nothing changed: no events.
        assert!(window_events(&after, &next).is_empty());
    }

    #[test]
    fn workspaces_follow_the_compositor() {
        let model = ShellState::default();
        let list = vec![WorkspaceInfo { index: 0, active: false, windows: vec![] }, WorkspaceInfo { index: 1, active: true, windows: vec![] }];
        let events = workspace_events(&model, &list);
        let after = apply(&model, &events);
        assert_eq!((after.workspaces.count, after.workspaces.active), (2, 1));
        assert!(workspace_events(&after, &list).is_empty());
    }
}
