//! The window menu and the keyboard applet (#EDE-253), Cinnamon's
//! windowMenu.js and keyboard@cinnamon.org as genUI Menu entries.
//!
//! The compositor's `window.menu` {window, x, y, …} opens
//! `window-menu.express` (an on-demand overlay layer of one pixel at the
//! point, its Menu's `open` bound to `/windowMenu/open`); a pick comes back
//! as `ede_window_menu {value}` and goes out as ede-protocol's
//! `window_action`, `move_to_workspace` or `close_window`. The keyboard
//! applet shows only with more than one layout (`/keyboard/show`), the
//! active one's short label, a menu of the layouts (a pick switches:
//! `set_keyboard_layout`) and Keyboard settings.

use ede_protocol::{KeyboardLayouts, WindowAction, WindowMenu};
use serde_json::{json, Value};

/// The document the window menu is.
pub const DOCUMENT: &str = "window-menu";

/// The entries in Cinnamon's order: Minimize; Maximize | Unmaximize; Move;
/// Resize; Move Titlebar Onscreen (only off screen); separator; Always on
/// Top; with more than one workspace Always on Visible Workspace / Only on
/// This Workspace (one checked) and Move to Another Workspace ▸ (the
/// current one insensitive); separator; Close.
pub fn items(menu: &WindowMenu) -> Value {
    let mut items = vec![
        json!({"label": "Minimize", "value": "minimize"}),
        if menu.maximized { json!({"label": "Unmaximize", "value": "unmaximize"}) } else { json!({"label": "Maximize", "value": "maximize", "disabled": !menu.resizable}) },
        json!({"label": "Move", "value": "move"}),
        json!({"label": "Resize", "value": "resize", "disabled": !menu.resizable}),
    ];
    if menu.offscreen {
        items.push(json!({"label": "Move Titlebar Onscreen", "value": "onscreen"}));
    }
    items.push(json!({"kind": "separator"}));
    items.push(json!({"kind": "checkbox", "label": "Always on Top", "value": "above", "checked": menu.above}));
    if menu.workspaces > 1 {
        items.push(json!({"kind": "checkbox", "label": "Always on Visible Workspace", "value": "stick", "checked": menu.sticky}));
        items.push(json!({"kind": "checkbox", "label": "Only on This Workspace", "value": "unstick", "checked": !menu.sticky}));
        let workspaces: Vec<Value> = (0..menu.workspaces).map(|i| json!({"label": format!("Workspace {}", i + 1), "value": format!("workspace:{i}"), "disabled": i == menu.workspace})).collect();
        items.push(json!({"kind": "submenu", "label": "Move to Another Workspace", "value": "workspaces", "items": workspaces, "disabled": menu.sticky}));
    }
    items.push(json!({"kind": "separator"}));
    items.push(json!({"label": "Close", "value": "close"}));
    Value::Array(items)
}

/// What a pick of the window menu does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    Action(WindowAction),
    Workspace(usize),
    Close,
    /// The menu closed without a pick (Escape, a click outside).
    Dismiss,
}

/// The pick `value` stands for, on a window whose menu said `menu`.
pub fn pick(value: &str, menu: &WindowMenu) -> Option<Pick> {
    Some(match value {
        "minimize" => Pick::Action(WindowAction::Minimize),
        "maximize" => Pick::Action(WindowAction::Maximize),
        "unmaximize" => Pick::Action(WindowAction::Unmaximize),
        "move" => Pick::Action(WindowAction::Move),
        "resize" => Pick::Action(WindowAction::Resize),
        "onscreen" => Pick::Action(WindowAction::MoveTitlebarOnscreen),
        "above" => Pick::Action(if menu.above { WindowAction::NotAbove } else { WindowAction::Above }),
        "stick" => Pick::Action(WindowAction::Stick),
        "unstick" => Pick::Action(WindowAction::Unstick),
        "close" => Pick::Close,
        "dismiss" => Pick::Dismiss,
        other => Pick::Workspace(other.strip_prefix("workspace:")?.parse().ok()?),
    })
}

/// The Menus whose pick rides an Event: genUI hands a `select`'s payload
/// (`{value}`) to an Event's context, never to a function's args, so a
/// Menu's `on.select` is `Event("<host function>", {…})` and the shell runs
/// that function with the picked value under the arg it reads.
pub const MENU_PICKS: [(&str, &str); 8] = [("ede_cu", "action"), ("ede_sessions", "menu"), ("ede_window_menu", "value"), ("ede_keyboard_layout", "value"), ("ede_panel_menu", "value"), ("ede_taskbar_menu", "value"), ("ede_tray_menu", "id"), ("ede_power", "action")];

/// The host function's args for Menu pick `name` with event `context`
/// (the Menu's own context plus the payload's `value`); `None` = not a
/// Menu pick, or no value picked.
pub fn menu_pick_args(name: &str, context: &Value) -> Option<Value> {
    let (_, arg) = MENU_PICKS.iter().find(|(n, _)| *n == name)?;
    let mut args = context.as_object()?.clone();
    let value = args.remove("value").filter(|v| !v.is_null())?;
    args.remove("checked");
    args.insert(arg.to_string(), value);
    Some(Value::Object(args))
}

/// `/windowMenu`: open, and the entries.
pub fn feed(menu: Option<&WindowMenu>, open: bool) -> Value {
    json!({"open": open && menu.is_some(), "items": menu.map(items).unwrap_or_else(|| json!([]))})
}

/// The point `(x, y)` (global logical) inside the output it falls on: the
/// layer's top and left margins there. Off every output: the first one's
/// corner.
pub fn margin_on(outputs: &[(i32, i32, i32, i32)], x: i32, y: i32) -> (u32, u32) {
    let on = outputs.iter().find(|&&(ox, oy, w, h)| x >= ox && y >= oy && x < ox + w && y < oy + h).or(outputs.first());
    match on {
        Some(&(ox, oy, w, h)) => ((x - ox).clamp(0, (w - 1).max(0)) as u32, (y - oy).clamp(0, (h - 1).max(0)) as u32),
        None => (x.max(0) as u32, y.max(0) as u32),
    }
}

/// `/keyboard`: shown with more than one layout, the active one's short
/// label (Cinnamon's, the `[keyboard] layout` name: `us`, `de`), the
/// layouts (the active one checked), a separator and Keyboard settings.
pub fn keyboard_feed(layouts: &KeyboardLayouts) -> Value {
    let short = |i: usize| layouts.short.get(i).cloned().filter(|s| !s.is_empty()).unwrap_or_else(|| layouts.layouts.get(i).map(|l| l.chars().take(2).collect::<String>().to_lowercase()).unwrap_or_default());
    let mut items: Vec<Value> = layouts.layouts.iter().enumerate().map(|(i, name)| json!({"kind": "checkbox", "label": name, "value": i.to_string(), "checked": i == layouts.index})).collect();
    items.push(json!({"kind": "separator"}));
    items.push(json!({"label": "Keyboard settings", "value": "settings", "icon": "nav-settings"}));
    json!({"show": layouts.layouts.len() > 1, "open": false, "label": short(layouts.index), "name": layouts.layouts.get(layouts.index).cloned().unwrap_or_default(), "items": items})
}

/// The goldens' sample (`EDE_SHELL_DEMO`): the Terminal's menu near the
/// top-left of the screen.
pub fn demo_menu() -> WindowMenu {
    WindowMenu { window: ede_protocol::WindowId::new("w2").unwrap(), x: 240, y: 160, maximized: false, above: true, sticky: false, offscreen: false, resizable: true, workspace: 0, workspaces: 4 }
}

/// The goldens' keyboard (`EDE_SHELL_DEMO_KEYBOARD=1`, `open` with its menu open): two layouts.
pub fn demo_keyboard() -> KeyboardLayouts {
    KeyboardLayouts { index: 0, layouts: vec!["English (US)".into(), "German".into()], short: vec!["us".into(), "de".into()] }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu() -> WindowMenu {
        WindowMenu { window: ede_protocol::WindowId::new("w1").unwrap(), x: 10, y: 20, maximized: false, above: false, sticky: false, offscreen: false, resizable: true, workspace: 1, workspaces: 4 }
    }

    fn labels(v: &Value) -> Vec<String> {
        v.as_array().unwrap().iter().map(|i| i.get("label").and_then(Value::as_str).unwrap_or("—").to_string()).collect()
    }

    /// Cinnamon's order; Move Titlebar Onscreen only off screen; Unmaximize
    /// on a maximized window; the workspaces past one.
    #[test]
    fn the_entries_follow_cinnamon() {
        let v = items(&menu());
        assert_eq!(labels(&v), ["Minimize", "Maximize", "Move", "Resize", "—", "Always on Top", "Always on Visible Workspace", "Only on This Workspace", "Move to Another Workspace", "—", "Close"]);
        let sub = &v[8]["items"];
        assert_eq!(sub.as_array().unwrap().len(), 4);
        assert_eq!(sub[1]["disabled"], true, "the current workspace is insensitive");
        assert_eq!(v[7]["checked"], true, "only on this workspace");
        let v = items(&WindowMenu { maximized: true, offscreen: true, above: true, workspaces: 1, ..menu() });
        assert_eq!(labels(&v), ["Minimize", "Unmaximize", "Move", "Resize", "Move Titlebar Onscreen", "—", "Always on Top", "—", "Close"]);
        assert_eq!(v[6]["checked"], true);
        let v = items(&WindowMenu { resizable: false, ..menu() });
        assert_eq!((v[1]["disabled"].clone(), v[3]["disabled"].clone()), (json!(true), json!(true)), "a fixed-size window neither maximizes nor resizes");
    }

    #[test]
    fn picks_map_to_the_protocol() {
        let m = menu();
        assert_eq!(pick("above", &m), Some(Pick::Action(WindowAction::Above)));
        assert_eq!(pick("above", &WindowMenu { above: true, ..menu() }), Some(Pick::Action(WindowAction::NotAbove)), "the check toggles");
        assert_eq!(pick("workspace:2", &m), Some(Pick::Workspace(2)));
        assert_eq!(pick("close", &m), Some(Pick::Close));
        assert_eq!(pick("dismiss", &m), Some(Pick::Dismiss));
        assert_eq!(pick("nonsense", &m), None);
    }

    #[test]
    fn the_layer_stands_at_the_point_on_its_output() {
        let outputs = [(0, 0, 1920, 1080), (1920, 0, 2560, 1440)];
        assert_eq!(margin_on(&outputs, 100, 50), (100, 50));
        assert_eq!(margin_on(&outputs, 2000, 70), (80, 70), "on the second output");
        assert_eq!(margin_on(&outputs, -5, 3000), (0, 1079), "off every output: the first one, kept on it");
    }

    /// The factory documents' Menus: a press on an entry reaches the
    /// host function with the entry's value (genUI's real event path: the
    /// Menu opened, its item pressed, the Event it fires, the shell's
    /// bridge). A `fn(value=$value)` select resolved `$value` without the
    /// payload, so every pick arrived empty.
    #[test]
    fn a_menu_pick_reaches_its_host_function_with_the_value() {
        use eui_core::measure::FixedMeasure;
        use eui_core::surface::{OutEvent, Surface, SurfaceOptions};
        use eui_core::types::NestedNode;

        fn node<'a>(n: &'a Value, id: &str) -> Option<&'a Value> {
            if n.get("id").and_then(Value::as_str) == Some(id) {
                return Some(n);
            }
            match n {
                Value::Object(m) => m.values().find_map(|v| node(v, id)),
                Value::Array(a) => a.iter().find_map(|v| node(v, id)),
                _ => None,
            }
        }
        // (document, Menu, its data scope, the entry pressed: kind, value; the function, its args)
        let cases = [
            ("window-menu", "root", json!({}), "item", json!("maximize"), "ede_window_menu", json!({"value": "maximize"})),
            ("window-menu", "root", json!({}), "checkbox", json!("above"), "ede_window_menu", json!({"value": "above"})),
            ("taskbar", "applet_keyboard", json!({}), "checkbox", json!("1"), "ede_keyboard_layout", json!({"value": "1"})),
            ("taskbar", "start_menu", json!({}), "item", json!("history"), "ede_panel_menu", json!({"value": "history"})),
            ("taskbar", "session", json!({}), "item", json!("datetime"), "ede_panel_menu", json!({"value": "datetime"})),
            ("taskbar", "group", json!({"app": "org.gnome.Ptyxis"}), "item", json!("close"), "ede_taskbar_menu", json!({"app": "org.gnome.Ptyxis", "value": "close"})),
            ("taskbar", "group", json!({"app": "org.gnome.Ptyxis"}), "checkbox", json!("pin"), "ede_taskbar_menu", json!({"app": "org.gnome.Ptyxis", "value": "pin"})),
            ("taskbar", "tray_menu", json!({"service": ":1.42/StatusNotifierItem"}), "item", json!("7"), "ede_tray_menu", json!({"service": ":1.42/StatusNotifierItem", "id": "7"})),
            ("taskbar", "tray_menu_press", json!({"service": ":1.42/StatusNotifierItem"}), "item", json!("7"), "ede_tray_menu", json!({"service": ":1.42/StatusNotifierItem", "id": "7"})),
            ("start-menu", "power", json!({}), "item", json!("suspend"), "ede_power", json!({"action": "suspend"})),
        ];
        for (doc, menu_id, scope, kind, value, function, want) in cases {
            let name = format!("{doc}/{menu_id}/{value}");
            let factory = uicore_docs::doc(doc).unwrap_or_else(|| panic!("{name}: no factory document")).json();
            let on = node(&factory["root"], menu_id).unwrap_or_else(|| panic!("{name}: no Menu"))["on"].clone();
            let functions: Vec<&str> = factory["surface"]["functions"].as_array().or_else(|| factory["surface"]["extensions"]["ede"]["functions"].as_array()).map(|f| f.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
            assert!(functions.contains(&function), "{name}: {function} is not among the document's functions");
            let item = json!({"kind": kind, "label": "Entry", "value": value});
            let tree: NestedNode = serde_json::from_value(json!({"id": "m", "component": "Menu", "props": {"label": "Open", "items": [item]}, "on": on})).expect("tree");
            let mut s = Surface::new(doc, SurfaceOptions::default());
            s.set_nested(tree);
            for (k, v) in scope.as_object().unwrap() {
                s.set_data(&format!("/{k}"), Some(v.clone())).unwrap();
            }
            s.set_viewport(400.0, 0.0, None);
            let mut m = FixedMeasure { sizes: Default::default(), wrap: true };
            let _ = s.set_open("m", true);
            s.layout(&mut m);
            let at = s.nodes().iter().position(|n| n.id == "m.item.0").unwrap_or_else(|| panic!("{name}: no entry")) as u32;
            let out = s.event(at, "press", None);
            let calls: Vec<(String, Value)> = out
                .iter()
                .filter_map(|e| match e {
                    OutEvent::Action { name, context, .. } => menu_pick_args(name, context).map(|args| (name.clone(), args)),
                    // The window menu's close (`change`) says `dismiss` on its own.
                    OutEvent::FunctionCall { name, args, .. } if name == function && args["value"] != "dismiss" => Some((name.clone(), args.clone())),
                    _ => None,
                })
                .collect();
            assert_eq!(calls, vec![(function.to_string(), want)], "{name}: {out:?}");
        }
    }

    #[test]
    fn the_keyboard_applet_shows_past_one_layout() {
        let one = KeyboardLayouts { index: 0, layouts: vec!["English (US)".into()], short: vec!["us".into()] };
        assert_eq!(keyboard_feed(&one)["show"], false);
        let two = KeyboardLayouts { index: 1, ..demo_keyboard() };
        let v = keyboard_feed(&two);
        assert_eq!((v["show"].clone(), v["label"].clone()), (json!(true), json!("de")));
        assert_eq!(v["items"][1]["checked"], true);
        assert_eq!(labels(&v["items"]), ["English (US)", "German", "—", "Keyboard settings"]);
    }
}
