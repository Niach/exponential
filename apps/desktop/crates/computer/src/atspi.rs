//! AT-SPI over D-Bus (zbus, pure Rust): `read_ui` on Linux for X11 and
//! Wayland alike, plus the window list a Wayland session has no other way to
//! name. Raw method calls on the accessibility bus, no proxy crate.

use std::time::Duration;

use zbus::blocking::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

use crate::backend::{BackendResult, Rect, UiNode};
use crate::guard::UI_NODES_MAX;

const UI_DEPTH_MAX: usize = 14;
/// Elements visited per read, shown or not: a browser holds tens of
/// thousands, and every visit is a round trip.
const VISIT_MAX: usize = 4000;
const ROOT: &str = "/org/a11y/atspi/accessible/root";
const ACCESSIBLE: &str = "org.a11y.atspi.Accessible";
const STATE_ACTIVE: u32 = 1;
const STATE_SHOWING: u32 = 25;
/// Nameless containers: structure, not content (as on macOS).
const STRUCTURAL: &[&str] =
    &["panel", "filler", "section", "unknown", "redundant object", "layered pane", "grouping", "invalid"];
/// Roles whose text, not name, is what they say.
const TEXTUAL: &[&str] = &["entry", "text", "paragraph", "static", "label", "editbar", "spin button"];

/// One accessible object: its bus name and path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub bus: String,
    pub path: OwnedObjectPath,
}

/// A top-level frame of an app.
#[derive(Clone, Debug)]
pub struct Frame {
    pub node: Node,
    pub pid: u32,
    pub app: String,
    pub title: String,
    pub active: bool,
    pub rect: Option<Rect>,
}

fn dbus(err: zbus::Error) -> String {
    format!("Accessibility (AT-SPI) request failed: {err}")
}

pub struct Atspi {
    conn: Connection,
}

impl Atspi {
    /// Join the session's accessibility bus, switching accessibility on so
    /// toolkits that only build their tree for an assistive tool (Chromium,
    /// Electron, Qt) start exposing it.
    pub fn connect() -> BackendResult<Self> {
        let session = Connection::session().map_err(dbus)?;
        let _ = session.call_method(
            Some("org.a11y.Bus"),
            "/org/a11y/bus",
            Some("org.freedesktop.DBus.Properties"),
            "Set",
            &("org.a11y.Status", "IsEnabled", Value::from(true)),
        );
        let address: String = session
            .call_method(Some("org.a11y.Bus"), "/org/a11y/bus", Some("org.a11y.Bus"), "GetAddress", &())
            .map_err(|_| "This desktop runs no accessibility bus (at-spi2-core); use screenshot.".to_string())?
            .body()
            .deserialize()
            .map_err(dbus)?;
        let conn = zbus::blocking::connection::Builder::address(address.as_str())
            .map_err(dbus)?
            .method_timeout(Duration::from_secs(2))
            .build()
            .map_err(dbus)?;
        Ok(Self { conn })
    }

    fn call<B, R>(&self, node: &Node, iface: &str, method: &str, body: &B) -> Option<R>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
        R: for<'d> serde::Deserialize<'d> + zbus::zvariant::Type,
    {
        self.conn
            .call_method(Some(node.bus.as_str()), node.path.as_str(), Some(iface), method, body)
            .ok()?
            .body()
            .deserialize()
            .ok()
    }

    fn property(&self, node: &Node, iface: &str, name: &str) -> Option<OwnedValue> {
        self.call(node, "org.freedesktop.DBus.Properties", "Get", &(iface, name))
    }

    fn text_property(&self, node: &Node, name: &str) -> String {
        self.property(node, ACCESSIBLE, name)
            .and_then(|value| String::try_from(value).ok())
            .unwrap_or_default()
    }

    pub fn name(&self, node: &Node) -> String {
        self.text_property(node, "Name")
    }

    pub fn children(&self, node: &Node) -> Vec<Node> {
        self.call::<_, Vec<(String, OwnedObjectPath)>>(node, ACCESSIBLE, "GetChildren", &())
            .unwrap_or_default()
            .into_iter()
            .map(|(bus, path)| Node { bus, path })
            .collect()
    }

    fn role(&self, node: &Node) -> String {
        self.call(node, ACCESSIBLE, "GetRoleName", &()).unwrap_or_default()
    }

    fn has_state(&self, node: &Node, state: u32) -> bool {
        self.call::<_, Vec<u32>>(node, ACCESSIBLE, "GetState", &())
            .and_then(|words| words.get(state as usize / 32).copied())
            .is_some_and(|word| word & (1 << (state % 32)) != 0)
    }

    /// Screen coordinates; `None` when the toolkit does not know them (a
    /// Wayland client cannot) or the element has no size.
    pub fn extents(&self, node: &Node) -> Option<Rect> {
        let (x, y, width, height): (i32, i32, i32, i32) =
            self.call(node, "org.a11y.atspi.Component", "GetExtents", &0u32)?;
        (width > 0 && height > 0 && (x, y) != (i32::MIN, i32::MIN)).then(|| Rect {
            x: f64::from(x),
            y: f64::from(y),
            width: f64::from(width),
            height: f64::from(height),
        })
    }

    fn pid(&self, bus: &str) -> Option<u32> {
        self.conn
            .call_method(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
                "GetConnectionUnixProcessID",
                &bus,
            )
            .ok()?
            .body()
            .deserialize()
            .ok()
    }

    /// Every app's top-level frames that are on screen.
    pub fn frames(&self) -> Vec<Frame> {
        let root = Node { bus: "org.a11y.atspi.Registry".into(), path: OwnedObjectPath::try_from(ROOT).unwrap() };
        let mut frames = Vec::new();
        for app in self.children(&root) {
            let Some(pid) = self.pid(&app.bus) else { continue };
            let app_name = self.name(&app);
            for frame in self.children(&app) {
                if !self.has_state(&frame, STATE_SHOWING) {
                    continue;
                }
                frames.push(Frame {
                    pid,
                    app: app_name.clone(),
                    title: self.name(&frame),
                    active: self.has_state(&frame, STATE_ACTIVE),
                    rect: self.extents(&frame),
                    node: frame,
                });
            }
        }
        frames
    }

    /// The frame of process `pid` that shows `title` (else its active one,
    /// else its first).
    pub fn frame_for(&self, pid: u32, title: &str) -> Option<Frame> {
        let mut frames: Vec<Frame> = self.frames().into_iter().filter(|frame| frame.pid == pid).collect();
        let pick = frames
            .iter()
            .position(|frame| !frame.title.is_empty() && frame.title == title)
            .or_else(|| {
                frames.iter().position(|frame| {
                    !frame.title.is_empty() && (title.starts_with(&frame.title) || frame.title.starts_with(title))
                })
            })
            .or_else(|| frames.iter().position(|frame| frame.active))
            .unwrap_or(0);
        (pick < frames.len()).then(|| frames.swap_remove(pick))
    }

    /// The tree under `root`, shown elements only, in screen pixels.
    pub fn tree(&self, root: &Node, units: Units) -> Vec<UiNode> {
        let mut nodes = Vec::new();
        let mut visits = 0;
        self.walk(root, 0, false, units, &mut nodes, &mut visits);
        nodes
    }

    fn walk(
        &self,
        node: &Node,
        depth: usize,
        physical: bool,
        units: Units,
        nodes: &mut Vec<UiNode>,
        visits: &mut usize,
    ) {
        if nodes.len() > UI_NODES_MAX || depth > UI_DEPTH_MAX || *visits >= VISIT_MAX {
            return;
        }
        *visits += 1;
        if depth > 0 && !self.has_state(node, STATE_SHOWING) {
            return;
        }
        let role = self.role(node);
        let mut label = self.name(node);
        if label.trim().is_empty() {
            label = self.text_property(node, "Description");
        }
        // Text only from roles that show it as such; never `password text`.
        // A field shows its value after its name (a placeholder, a label).
        let field = matches!(role.as_str(), "entry" | "spin button");
        if (label.trim().is_empty() || field) && TEXTUAL.contains(&role.as_str()) {
            let text = self
                .call::<_, String>(node, "org.a11y.atspi.Text", "GetText", &(0i32, 400i32))
                .unwrap_or_default()
                .replace('\u{fffc}', "");
            if label.trim().is_empty() {
                label = text;
            } else if !text.trim().is_empty() && text != label {
                label = format!("{label}: {text}");
            }
        }
        let raw = self.extents(node);
        let physical = physical || (role.starts_with("document") && units.is_physical(raw));
        let structural = label.trim().is_empty() && STRUCTURAL.contains(&role.as_str());
        if !structural {
            let rect = raw.map(|rect| units.to_screen(rect, physical));
            nodes.push(UiNode { depth, role: role.clone(), label, rect });
        }
        let child_depth = if structural { depth } else { depth + 1 };
        for child in self.children(node) {
            self.walk(&child, child_depth, physical, units, nodes, visits);
        }
    }
}

/// How a toolkit's extents map onto screen pixels. A scaled toolkit (GTK at
/// 2x) reports logical points; Chromium reports its own UI in points but web
/// content in device pixels offset from the frame's logical origin.
#[derive(Clone, Copy, Debug)]
pub struct Units {
    scale: f64,
    frame: Option<Rect>,
    window: Option<Rect>,
}

impl Units {
    /// `window` = the window's rect in screen pixels, `frame` = what the
    /// toolkit reports for it.
    pub fn new(window: Option<Rect>, frame: Option<Rect>) -> Self {
        let scale = window.zip(frame).map_or(1.0, |(window, frame)| scale_factor(window, frame));
        Self { scale, frame, window }
    }

    /// A document wider than its own frame's logical size is in device pixels.
    fn is_physical(&self, rect: Option<Rect>) -> bool {
        self.scale > 1.0
            && self.window.is_some()
            && rect.zip(self.frame).is_some_and(|(rect, frame)| rect.width > frame.width * 1.1)
    }

    fn to_screen(&self, rect: Rect, physical: bool) -> Rect {
        match (physical, self.frame, self.window) {
            (true, Some(frame), Some(window)) => Rect {
                x: window.x + rect.x - frame.x,
                y: window.y + rect.y - frame.y,
                ..rect
            },
            _ => Rect {
                x: rect.x * self.scale,
                y: rect.y * self.scale,
                width: rect.width * self.scale,
                height: rect.height * self.scale,
            },
        }
    }
}

/// `read_ui` for the window of process `pid` titled `title`; `window` = its
/// rect in screen pixels when known.
pub fn read_ui(pid: u32, app: &str, title: &str, window: Option<Rect>) -> BackendResult<Vec<UiNode>> {
    let atspi = Atspi::connect()?;
    let empty = || {
        format!(
            "{app} exposes no accessibility tree; use screenshot. (Chromium and Electron apps \
             build one only when started with accessibility on, e.g. \
             --force-renderer-accessibility.)"
        )
    };
    let frame = atspi.frame_for(pid, title).ok_or_else(empty)?;
    let nodes = atspi.tree(&frame.node, Units::new(window, frame.rect));
    if nodes.len() <= 1 {
        return Err(empty());
    }
    Ok(nodes)
}

/// Screen pixels per reported unit: the window's width over its frame's,
/// snapped to a whole factor when close (client-side shadows make the two
/// differ a little), 1 when implausible.
fn scale_factor(window: Rect, frame: Rect) -> f64 {
    let ratio = window.width / frame.width;
    if !(0.9..=4.0).contains(&ratio) {
        return 1.0;
    }
    if (ratio - ratio.round()).abs() < 0.15 { ratio.round() } else { ratio }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(width: f64) -> Rect {
        Rect { x: 0.0, y: 0.0, width, height: 100.0 }
    }

    #[test]
    fn chromium_web_content_is_device_pixels_from_the_frame_origin() {
        let window = Rect { x: 20.0, y: 20.0, width: 2530.0, height: 2760.0 };
        let frame = Rect { x: 10.0, y: 10.0, width: 1265.0, height: 1380.0 };
        let units = Units::new(Some(window), Some(frame));
        let document = Rect { x: 52.0, y: 314.0, width: 2466.0, height: 2402.0 };
        assert!(units.is_physical(Some(document)));
        let button = Rect { x: 68.0, y: 446.0, width: 145.0, height: 44.0 };
        assert_eq!(units.to_screen(button, true), Rect { x: 78.0, y: 456.0, ..button });
        // The browser's own controls are logical points.
        let back = Rect { x: 32.0, y: 66.0, width: 28.0, height: 28.0 };
        assert!(!units.is_physical(Some(back)));
        assert_eq!(units.to_screen(back, false), Rect { x: 64.0, y: 132.0, width: 56.0, height: 56.0 });
    }

    #[test]
    fn a_scaled_toolkit_is_rescaled_by_a_whole_factor_when_close() {
        assert_eq!(scale_factor(rect(5120.0), rect(2560.0)), 2.0);
        assert_eq!(scale_factor(rect(5120.0), rect(2520.0)), 2.0);
        assert_eq!(scale_factor(rect(1500.0), rect(1000.0)), 1.5);
        assert_eq!(scale_factor(rect(1000.0), rect(1000.0)), 1.0);
        assert_eq!(scale_factor(rect(1000.0), rect(10.0)), 1.0);
    }
}
