//! `uicore-gpui` (#EDE-50): the shell's gpui host, shared by Exponential
//! Desktop's shell and the IDE. A [`ShellHost`] says how a
//! document's surface becomes a window: [`LayerShellHost`] = one
//! wlr-layer-shell surface per region (the sidebar, the taskbar, the ground),
//! a popup document and a hosted Popover / Menu layer (VAPP-281) as
//! `xdg_popup`s of their parent surface (Linux); [`WindowedHost`] = normal
//! windows and popup windows (`--windowed`, macOS).
//! [`Surfaces`] owns the open windows: it opens a layer document, toggles a
//! popup document under its anchor, mirrors a surface's hosted layers as
//! popup windows, feeds every open view from a [`uicore_data::Feed`] and
//! redraws a document after an edit. The host plugin (actions, host
//! functions, icons) stays the application's: it is handed in per surface.
//! [`inline`] mounts a document in place inside a host's own window instead
//! (the IDE's regions, #EXP-1725); [`glyphs`] paints the rail's glyphs both
//! hosts share (#EDE-65).

pub mod glyphs;
pub mod inline;

use std::{cell::RefCell, collections::HashMap, rc::Rc};

#[cfg(target_os = "linux")]
use ede_ui::surface::{Edge, Keyboard, Layer as DocLayer};
use ede_ui::{
    surface::{Placement, SurfaceHeader},
    Document,
};
use eui_core::{
    overlay::{OverlayAlign, OverlaySide},
    surface::{Layer, SurfaceCommand, SurfaceSettings},
    theme::{Mode, ModeSetting, ResolvedTheme},
};
use eui_gpui::host::HostPlugin;
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
#[cfg(target_os = "linux")]
use gpui::layer_shell::{Anchor, KeyboardInteractivity, Layer as ShellLayer, LayerShellOptions};
use gpui::{
    div, point, popup::{PopupAnchor, PopupConstraintAdjustment, PopupGravity, PopupOptions}, prelude::*, px, size, AnyWindowHandle, App, Bounds, Entity, Pixels, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use serde_json::Value;
use uicore_data::Feed;

/// A layer surface stretched on neither axis gets this size.
pub const DEFAULT_SIZE: (f32, f32) = (1280.0, 40.0);

/// The gap between a popup and its anchor.
pub const POPUP_GAP: f32 = 4.0;

/// What a window of the shell shows: a whole document, or one hosted
/// popup layer of a document (VAPP-281) at (0, 0).
pub struct SurfaceRoot {
    pub view: Entity<SurfaceView>,
    /// `Some(owner)` = the hosted Popover / Menu `owner` of `view`.
    pub hosted: Option<String>,
}

impl Render for SurfaceRoot {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        match self.hosted.clone() {
            None => div().size_full().child(self.view.clone()),
            Some(owner) => {
                let el = self.view.update(cx, |v, cx| v.hosted_popup_element(&owner, window, cx));
                div().size_full().children(el)
            }
        }
    }
}

/// One document on screen.
#[derive(Clone)]
pub struct Region {
    pub name: String,
    pub handle: WindowHandle<SurfaceRoot>,
    pub view: Entity<SurfaceView>,
    /// What the view was last given of the feed.
    pushed: Rc<RefCell<Pushed>>,
}

/// What one view was last given of the feed (#EDE-65): the data roots its
/// document binds and the value of every path pushed into it, so a refresh
/// gives each view only the paths it binds that moved, and an unchanged
/// view never lays out and redraws (every refresh redrew every bar).
#[derive(Default)]
pub struct Pushed {
    roots: std::collections::BTreeSet<String>,
    last: HashMap<String, Value>,
}

impl Pushed {
    /// For a document of `source` (its JSON as written).
    pub fn for_source(source: &Value) -> Self {
        Self { roots: bound_roots(source), last: HashMap::new() }
    }

    /// The paths of `feed` this view binds whose value moved since the last
    /// call, recorded as given.
    pub fn changes(&mut self, feed: &Feed) -> Vec<(String, Value)> {
        let mut out = Vec::new();
        for (path, value) in feed.entries() {
            if !self.roots.contains(root_of(path)) || self.last.get(path) == Some(value) {
                continue;
            }
            self.last.insert(path.to_string(), value.clone());
            out.push((path.to_string(), value.clone()));
        }
        out
    }

    /// The whole feed was just given (a document (re)loaded).
    pub fn given_all(&mut self, feed: &Feed) {
        self.last = feed.entries().map(|(p, v)| (p.to_string(), v.clone())).collect();
    }
}

/// `/shell/rail` -> `shell`.
fn root_of(path: &str) -> &str {
    path.trim_start_matches('/').split('/').next().unwrap_or_default()
}

/// The data roots a document can read: the first segment of every absolute
/// path written anywhere in it (`@path`, templates, function arguments; a
/// string that only looks like one costs a needless push, never a missed one).
pub fn bound_roots(source: &Value) -> std::collections::BTreeSet<String> {
    fn walk(v: &Value, out: &mut std::collections::BTreeSet<String>) {
        match v {
            Value::String(s) if s.starts_with('/') && s.len() > 1 => {
                out.insert(root_of(s).to_string());
            }
            Value::Array(a) => a.iter().for_each(|v| walk(v, out)),
            Value::Object(o) => o.values().for_each(|v| walk(v, out)),
            _ => {}
        }
    }
    let mut out = std::collections::BTreeSet::new();
    walk(source, &mut out);
    out
}

/// A window of the host: its kind and the bounds to ask for.
pub struct Placed {
    pub kind: WindowKind,
    pub bounds: Bounds<Pixels>,
}

/// How a document's surface becomes a window.
pub trait ShellHost {
    /// A layer document (`surface.layer`).
    fn layer(&self, header: &SurfaceHeader) -> Placed;
    /// A popup document (`surface.popup`) under `anchor_rect` of `parent`.
    fn popup(&self, parent: &Region, anchor_rect: Bounds<Pixels>, placement: Placement, size: (u32, u32), cx: &mut App) -> Placed;
    /// A hosted Popover / Menu layer of `parent` (VAPP-281).
    fn hosted(&self, parent: &Region, layer: &Layer, cx: &mut App) -> Option<Placed>;
    /// Hosted layers go to the host (true) or stay painted inside their surface.
    fn hosts_popups(&self) -> bool {
        true
    }
}

/// Exponential Desktop: layer-shell surfaces, popups as `xdg_popup`s.
#[cfg(target_os = "linux")]
pub struct LayerShellHost {
    /// The layer-shell namespace prefix; the document's name is appended
    /// (`exponential-shell:sidebar`), so a compositor rule can tell them apart.
    pub namespace: String,
}

#[cfg(target_os = "linux")]
impl Default for LayerShellHost {
    fn default() -> Self {
        Self { namespace: "exponential-shell".into() }
    }
}

/// Normal windows: `--windowed`, macOS.
pub struct WindowedHost;

/// The size a layer header asks for.
pub fn layer_bounds(header: &SurfaceHeader) -> Bounds<Pixels> {
    let SurfaceHeader::Layer { size: s, .. } = header else {
        unreachable!("a layer header");
    };
    let width = s.width.map(|w| w as f32).unwrap_or(DEFAULT_SIZE.0);
    let height = s.height.map(|h| h as f32).unwrap_or(DEFAULT_SIZE.1);
    Bounds { origin: point(px(0.0), px(0.0)), size: size(px(width), px(height)) }
}

/// The layer-shell options a layer header asks for.
#[cfg(target_os = "linux")]
pub fn layer_options(namespace: &str, header: &SurfaceHeader) -> (WindowKind, Bounds<Pixels>) {
    let SurfaceHeader::Layer { layer, anchor, exclusive_zone, margin, keyboard, .. } = header else {
        unreachable!("a layer header");
    };
    let mut anchors = Anchor::empty();
    for edge in anchor {
        anchors |= match edge {
            Edge::Top => Anchor::TOP,
            Edge::Bottom => Anchor::BOTTOM,
            Edge::Left => Anchor::LEFT,
            Edge::Right => Anchor::RIGHT,
        };
    }
    let kind = WindowKind::LayerShell(LayerShellOptions {
        namespace: namespace.into(),
        layer: match layer {
            DocLayer::Background => ShellLayer::Background,
            DocLayer::Bottom => ShellLayer::Bottom,
            DocLayer::Top => ShellLayer::Top,
            DocLayer::Overlay => ShellLayer::Overlay,
        },
        anchor: anchors,
        exclusive_zone: (*exclusive_zone != 0).then(|| px(*exclusive_zone as f32)),
        exclusive_edge: None,
        margin: Some((px(margin.top as f32), px(margin.right as f32), px(margin.bottom as f32), px(margin.left as f32))),
        keyboard_interactivity: match keyboard {
            Keyboard::None => KeyboardInteractivity::None,
            Keyboard::OnDemand => KeyboardInteractivity::OnDemand,
            Keyboard::Exclusive => KeyboardInteractivity::Exclusive,
        },
    });
    (kind, layer_bounds(header))
}

/// A popup document's placement as an `xdg_positioner` anchor + gravity.
pub fn placement_anchor(p: Placement) -> (PopupAnchor, PopupGravity) {
    match p {
        Placement::BottomStart => (PopupAnchor::BottomLeft, PopupGravity::BottomRight),
        Placement::BottomEnd => (PopupAnchor::BottomRight, PopupGravity::BottomLeft),
        Placement::TopStart => (PopupAnchor::TopLeft, PopupGravity::TopRight),
        Placement::TopEnd => (PopupAnchor::TopRight, PopupGravity::TopLeft),
        Placement::LeftStart => (PopupAnchor::TopLeft, PopupGravity::BottomLeft),
        Placement::RightStart => (PopupAnchor::TopRight, PopupGravity::BottomRight),
    }
}

/// A hosted layer's side + (physical) alignment as an `xdg_positioner`
/// anchor + gravity: the popup sits on `side` of its anchor, aligned at
/// `align` along that side.
pub fn side_anchor(side: OverlaySide, align: OverlayAlign) -> (PopupAnchor, PopupGravity) {
    match (side, align) {
        (OverlaySide::Top, OverlayAlign::Start) => (PopupAnchor::TopLeft, PopupGravity::TopRight),
        (OverlaySide::Top, OverlayAlign::Center) => (PopupAnchor::Top, PopupGravity::Top),
        (OverlaySide::Top, OverlayAlign::End) => (PopupAnchor::TopRight, PopupGravity::TopLeft),
        (OverlaySide::Bottom, OverlayAlign::Start) => (PopupAnchor::BottomLeft, PopupGravity::BottomRight),
        (OverlaySide::Bottom, OverlayAlign::Center) => (PopupAnchor::Bottom, PopupGravity::Bottom),
        (OverlaySide::Bottom, OverlayAlign::End) => (PopupAnchor::BottomRight, PopupGravity::BottomLeft),
        (OverlaySide::Left, OverlayAlign::Start) => (PopupAnchor::TopLeft, PopupGravity::BottomLeft),
        (OverlaySide::Left, OverlayAlign::Center) => (PopupAnchor::Left, PopupGravity::Left),
        (OverlaySide::Left, OverlayAlign::End) => (PopupAnchor::BottomLeft, PopupGravity::TopLeft),
        (OverlaySide::Right, OverlayAlign::Start) => (PopupAnchor::TopRight, PopupGravity::BottomRight),
        (OverlaySide::Right, OverlayAlign::Center) => (PopupAnchor::Right, PopupGravity::Right),
        (OverlaySide::Right, OverlayAlign::End) => (PopupAnchor::BottomRight, PopupGravity::TopRight),
    }
}

/// The hosted layer's root frame (its unclipped size and where the core
/// would put it, surface coordinates) and its anchor frame.
pub fn hosted_frames(layer: &Layer) -> Option<(Bounds<Pixels>, Bounds<Pixels>)> {
    let root = layer.frames.iter().find(|f| f.index == layer.root).or_else(|| layer.frames.first())?;
    let anchor = layer.anchor_frame?;
    let to_bounds = |x: f32, y: f32, w: f32, h: f32| Bounds { origin: point(px(x), px(y)), size: size(px(w), px(h)) };
    Some((to_bounds(root.x, root.y, root.w, root.h), to_bounds(anchor.x, anchor.y, anchor.w, anchor.h)))
}

fn popup_bounds(anchor_rect: Bounds<Pixels>, size_px: (f32, f32)) -> Bounds<Pixels> {
    Bounds { origin: point(anchor_rect.origin.x, anchor_rect.origin.y + anchor_rect.size.height + px(POPUP_GAP)), size: size(px(size_px.0), px(size_px.1)) }
}

#[cfg(target_os = "linux")]
impl ShellHost for LayerShellHost {
    fn layer(&self, header: &SurfaceHeader) -> Placed {
        let (kind, bounds) = layer_options(&self.namespace, header);
        Placed { kind, bounds }
    }

    fn popup(&self, parent: &Region, anchor_rect: Bounds<Pixels>, placement: Placement, size: (u32, u32), _cx: &mut App) -> Placed {
        let (anchor, gravity) = placement_anchor(placement);
        let parent_handle: AnyWindowHandle = parent.handle.into();
        let kind = WindowKind::AnchoredPopup(PopupOptions { parent: parent_handle, anchor_rect, anchor, gravity, constraint_adjustment: PopupConstraintAdjustment::SLIDE_X | PopupConstraintAdjustment::SLIDE_Y | PopupConstraintAdjustment::FLIP_Y, offset: point(px(0.0), px(POPUP_GAP)), grab: false });
        Placed { kind, bounds: popup_bounds(anchor_rect, (size.0 as f32, size.1 as f32)) }
    }

    fn hosted(&self, parent: &Region, layer: &Layer, _cx: &mut App) -> Option<Placed> {
        let (root, anchor_rect) = hosted_frames(layer)?;
        let side = layer.placement.map(|p| p.side).unwrap_or(OverlaySide::Bottom);
        let (anchor, gravity) = side_anchor(side, layer.align.unwrap_or_default());
        let parent_handle: AnyWindowHandle = parent.handle.into();
        let offset = match side {
            OverlaySide::Top => point(px(0.0), px(-POPUP_GAP)),
            OverlaySide::Bottom => point(px(0.0), px(POPUP_GAP)),
            OverlaySide::Left => point(px(-POPUP_GAP), px(0.0)),
            OverlaySide::Right => point(px(POPUP_GAP), px(0.0)),
        };
        let adjustment = match side {
            OverlaySide::Top | OverlaySide::Bottom => PopupConstraintAdjustment::SLIDE_X | PopupConstraintAdjustment::FLIP_Y,
            OverlaySide::Left | OverlaySide::Right => PopupConstraintAdjustment::SLIDE_Y | PopupConstraintAdjustment::FLIP_X,
        };
        let kind = WindowKind::AnchoredPopup(PopupOptions { parent: parent_handle, anchor_rect, anchor, gravity, constraint_adjustment: adjustment, offset, grab: false });
        Some(Placed { kind, bounds: Bounds { origin: root.origin, size: root.size } })
    }
}

impl ShellHost for WindowedHost {
    fn layer(&self, header: &SurfaceHeader) -> Placed {
        Placed { kind: WindowKind::Normal, bounds: layer_bounds(header) }
    }

    fn popup(&self, parent: &Region, anchor_rect: Bounds<Pixels>, _placement: Placement, size: (u32, u32), cx: &mut App) -> Placed {
        let origin = parent.handle.update(cx, |_, window, _| window.bounds().origin).unwrap_or_default();
        let mut bounds = popup_bounds(anchor_rect, (size.0 as f32, size.1 as f32));
        bounds.origin = point(origin.x + bounds.origin.x, origin.y + bounds.origin.y);
        Placed { kind: WindowKind::PopUp, bounds }
    }

    fn hosted(&self, parent: &Region, layer: &Layer, cx: &mut App) -> Option<Placed> {
        let (root, _) = hosted_frames(layer)?;
        let origin = parent.handle.update(cx, |_, window, _| window.bounds().origin).unwrap_or_default();
        Some(Placed { kind: WindowKind::PopUp, bounds: Bounds { origin: point(origin.x + root.origin.x, origin.y + root.origin.y), size: root.size } })
    }
}

/// Makes the host plugin of one surface (the application's actions and
/// functions router, told which document it serves).
pub type PluginFor = Rc<dyn Fn(&str) -> Rc<dyn HostPlugin>>;

/// Hears that a document's frame was drawn (eDE: the supervisor's frame line).
pub type OnFrame = Rc<dyn Fn(&str, &str)>;

/// The open windows of the shell.
pub struct Surfaces {
    host: Box<dyn ShellHost>,
    theme: std::sync::Arc<ResolvedTheme>,
    plugin_for: PluginFor,
    on_frame: OnFrame,
    app_id: String,
    open: HashMap<String, Region>,
    /// Hosted popup windows by (document, owner).
    hosted: HashMap<(String, String), WindowHandle<SurfaceRoot>>,
    /// The size each hosted popup window was last given.
    hosted_sizes: HashMap<(String, String), gpui::Size<Pixels>>,
}

impl Surfaces {
    pub fn new(host: Box<dyn ShellHost>, theme: std::sync::Arc<ResolvedTheme>, plugin_for: PluginFor, on_frame: OnFrame, app_id: &str) -> Self {
        Self { host, theme, plugin_for, on_frame, app_id: app_id.into(), open: HashMap::new(), hosted: HashMap::new(), hosted_sizes: HashMap::new() }
    }

    pub fn is_open(&self, name: &str) -> bool {
        self.open.contains_key(name)
    }

    pub fn region(&self, name: &str) -> Option<&Region> {
        self.open.get(name)
    }

    pub fn open_names(&self) -> impl Iterator<Item = &str> {
        self.open.keys().map(String::as_str)
    }

    /// Opens `doc` in a window of `kind`; the view records bounds so popups
    /// can anchor at a component.
    fn open_window(&mut self, doc: &Document, placed: Placed, feed: &Feed, cx: &mut App) -> Option<Region> {
        let theme = self.theme.clone();
        let plugin = (self.plugin_for)(&doc.name);
        let hosted_popups = self.host.hosts_popups();
        let name = doc.name.clone();
        let digest = doc.digest.clone();
        let on_frame = self.on_frame.clone();
        let doc_name = doc.name.clone();
        let doc = doc.clone();
        let feed = feed.clone();
        let (source, feed_now) = (doc.source.clone(), feed.clone());
        let view_out: Rc<RefCell<Option<Entity<SurfaceView>>>> = Rc::new(RefCell::new(None));
        let out = view_out.clone();
        let opened = cx.open_window(
            WindowOptions {
                kind: placed.kind,
                window_bounds: Some(WindowBounds::Windowed(placed.bounds)),
                titlebar: None,
                app_id: Some(self.app_id.clone()),
                focus: false,
                is_resizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            },
            move |window, cx| {
                let view = cx.new(|cx| {
                    let settings = SurfaceSettings { locale: "en-US".into(), mode: ModeSetting::Dark, hosted_popups, ..SurfaceSettings::default() };
                    let mut v = SurfaceView::new(SurfaceViewOptions { surface_id: name.clone(), theme: Some(theme), mode: Mode::Dark, host: plugin, settings: Some(settings), ..Default::default() }, window, cx);
                    v.record_bounds(true);
                    load(&mut v, &doc, &feed, cx);
                    v
                });
                *out.borrow_mut() = Some(view.clone());
                let n = name.clone();
                window.on_next_frame(move |_, _| on_frame(&n, &digest));
                cx.new(|_| SurfaceRoot { view, hosted: None })
            },
        );
        match opened {
            Ok(handle) => view_out.borrow().clone().map(|view| {
                let mut pushed = Pushed::for_source(&source);
                pushed.given_all(&feed_now);
                Region { name: doc_name, handle, view, pushed: Rc::new(RefCell::new(pushed)) }
            }),
            Err(e) => {
                tracing::warn!(document = %doc_name, "window: {e}");
                None
            }
        }
    }

    /// A layer document on screen (not twice).
    pub fn open_layer(&mut self, doc: &Document, feed: &Feed, cx: &mut App) -> bool {
        if self.open.contains_key(&doc.name) || !matches!(doc.surface, SurfaceHeader::Layer { .. }) {
            return false;
        }
        let placed = self.host.layer(&doc.surface);
        match self.open_window(doc, placed, feed, cx) {
            Some(region) => {
                tracing::info!(document = %doc.name, "surface opened");
                self.open.insert(doc.name.clone(), region);
                true
            }
            None => false,
        }
    }

    /// A popup document under its anchor in its parent; `false` when the
    /// parent is not open.
    pub fn open_popup(&mut self, doc: &Document, feed: &Feed, cx: &mut App) -> bool {
        if self.open.contains_key(&doc.name) {
            return true;
        }
        let SurfaceHeader::Popup { popup, .. } = &doc.surface else { return false };
        let Some(parent) = self.open.get(&popup.parent).cloned() else {
            tracing::warn!(document = %doc.name, parent = %popup.parent, "popup: parent surface not open");
            return false;
        };
        let anchor_rect = parent.view.read(cx).painted_bounds(&popup.anchor).unwrap_or(Bounds { origin: point(px(0.0), px(0.0)), size: size(px(120.0), px(DEFAULT_SIZE.1)) });
        let placed = self.host.popup(&parent, anchor_rect, popup.placement, (popup.size.width, popup.size.height), cx);
        match self.open_window(doc, placed, feed, cx) {
            Some(region) => {
                tracing::info!(document = %doc.name, "popup opened");
                self.open.insert(doc.name.clone(), region);
                true
            }
            None => false,
        }
    }

    /// Closes a document's window (and its hosted popups).
    pub fn close(&mut self, name: &str, cx: &mut App) -> bool {
        let hosted: Vec<(String, String)> = self.hosted.keys().filter(|(doc, _)| doc == name).cloned().collect();
        for key in hosted {
            self.hosted_sizes.remove(&key);
            if let Some(h) = self.hosted.remove(&key) {
                let _ = h.update(cx, |_, window, _| window.remove_window());
            }
        }
        match self.open.remove(name) {
            Some(region) => {
                let _ = region.handle.update(cx, |_, window, _| window.remove_window());
                true
            }
            None => false,
        }
    }

    /// The open popup documents (by `is_popup`), closed.
    pub fn close_popups(&mut self, is_popup: impl Fn(&str) -> bool, cx: &mut App) -> Vec<String> {
        let names: Vec<String> = self.open.keys().filter(|n| is_popup(n)).cloned().collect();
        for n in &names {
            self.close(n, cx);
        }
        names
    }

    /// A surface's hosted Popover / Menu layers as popup windows: opened
    /// for the new ones, closed for the gone ones (VAPP-281).
    pub fn sync_hosted(&mut self, document: &str, layers: &[Layer], cx: &mut App) {
        let Some(parent) = self.open.get(document).cloned() else { return };
        let wanted: Vec<&Layer> = layers.iter().filter(|l| l.hosted).collect();
        let gone: Vec<(String, String)> = self.hosted.keys().filter(|(doc, owner)| doc == document && !wanted.iter().any(|l| &l.owner == owner)).cloned().collect();
        for key in gone {
            self.hosted_sizes.remove(&key);
            if let Some(h) = self.hosted.remove(&key) {
                let _ = h.update(cx, |_, window, _| window.remove_window());
            }
        }
        for layer in wanted {
            let key = (document.to_string(), layer.owner.clone());
            let Some(placed) = self.host.hosted(&parent, layer, cx) else { continue };
            if let Some(handle) = self.hosted.get(&key) {
                // #EDE-65: the layer laid out again (its data arrived after
                // it opened, the taskbar previews' thumbnails): the window
                // follows, else it stays at its first, clipped size. A popup
                // the compositor has not configured yet cannot reposition, so
                // the size goes again once it has drawn.
                if self.hosted_sizes.get(&key) != Some(&placed.bounds.size) {
                    let size = placed.bounds.size;
                    let _ = handle.update(cx, |_, window, _| {
                        window.resize(size);
                        window.on_next_frame(move |window, _| {
                            if window.bounds().size != size {
                                window.resize(size);
                            }
                        });
                    });
                    self.hosted_sizes.insert(key, placed.bounds.size);
                }
                continue;
            }
            let view = parent.view.clone();
            let owner = layer.owner.clone();
            let opened = cx.open_window(
                WindowOptions { kind: placed.kind, window_bounds: Some(WindowBounds::Windowed(placed.bounds)), titlebar: None, app_id: Some(self.app_id.clone()), focus: false, is_resizable: false, window_background: WindowBackgroundAppearance::Transparent, ..Default::default() },
                move |_, cx| cx.new(|_| SurfaceRoot { view, hosted: Some(owner) }),
            );
            match opened {
                Ok(handle) => {
                    tracing::info!(document, owner = %layer.owner, "hosted popup opened");
                    self.hosted_sizes.insert(key.clone(), placed.bounds.size);
                    self.hosted.insert(key, handle);
                }
                Err(e) => tracing::warn!(document, owner = %layer.owner, "hosted popup: {e}"),
            }
        }
    }

    /// The compositor dismissed every popup (a press outside): the hosted
    /// layers close in their surfaces (which then tells [`Self::sync_hosted`]).
    pub fn dismiss_hosted(&mut self, cx: &mut App) {
        let keys: Vec<(String, String)> = self.hosted.keys().cloned().collect();
        for (doc, owner) in keys {
            if let Some(region) = self.open.get(&doc).cloned() {
                let _ = region.handle.update(cx, |_, window, cx| region.view.update(cx, |v, cx| v.dismiss_hosted_popup(&owner, window, cx)));
            }
        }
    }

    /// Redraws an open document from its (edited) source and data.
    pub fn redraw(&mut self, doc: &Document, feed: &Feed, cx: &mut App) -> bool {
        let Some(region) = self.open.get(&doc.name).cloned() else { return false };
        let digest = doc.digest.clone();
        let n = doc.name.clone();
        let on_frame = self.on_frame.clone();
        let feed = feed.clone();
        let doc = doc.clone();
        *region.pushed.borrow_mut() = Pushed::for_source(&doc.source);
        region.pushed.borrow_mut().given_all(&feed);
        let _ = region.handle.update(cx, |_, window, cx| {
            region.view.update(cx, |v, cx| load(v, &doc, &feed, cx));
            window.on_next_frame(move |_, _| on_frame(&n, &digest));
        });
        true
    }

    /// An open layer document at its header's new size and exclusive zone
    /// (the output changed under it, the rail): the same window resized in
    /// place, so the bar never leaves the screen for a frame (#EDE-65).
    pub fn resize_layer(&mut self, doc: &Document, feed: &Feed, cx: &mut App) -> bool {
        let Some(region) = self.open.get(&doc.name).cloned() else { return false };
        let SurfaceHeader::Layer { exclusive_zone, .. } = &doc.surface else { return false };
        let zone = *exclusive_zone;
        let bounds = layer_bounds(&doc.surface);
        let feed = feed.clone();
        let doc = doc.clone();
        // The reload gives the view the whole feed: what it was given starts over.
        *region.pushed.borrow_mut() = Pushed::for_source(&doc.source);
        region.pushed.borrow_mut().given_all(&feed);
        let _ = region.handle.update(cx, |_, window, cx| {
            window.resize(bounds.size);
            if zone != 0 {
                window.set_exclusive_zone(px(zone as f32));
            }
            region.view.update(cx, |v, cx| load(v, &doc, &feed, cx));
        });
        true
    }

    /// Gives every open view the paths it binds that moved; a view with
    /// none is left alone (no layout, no frame).
    pub fn feed_all(&self, feed: &Feed, cx: &mut App) {
        for region in self.open.values() {
            let changes = region.pushed.borrow_mut().changes(feed);
            if changes.is_empty() {
                continue;
            }
            let _ = region.handle.update(cx, |_, _, cx| {
                region.view.update(cx, |v, cx| {
                    for (path, value) in changes {
                        let _ = v.set_data(&path, Some(value), cx);
                    }
                })
            });
        }
    }

    /// Keyboard focus into a component of an open document (a search box).
    pub fn focus_component(&self, document: &str, component: &str, cx: &mut App) {
        let Some(region) = self.open.get(document).cloned() else { return };
        let id = component.to_string();
        let _ = region.handle.update(cx, |_, _, cx| {
            region.view.update(cx, |v, cx| {
                v.command(&SurfaceCommand::Focus { id }, cx);
                cx.notify();
            })
        });
    }
}

/// The chat document's rows (`apps/ede/ui/chat.json`).
pub const CHAT_SCROLL: &str = "chat_scroll";

/// #EXP-1780: the chat's rows scroll above its composer, held at the newest
/// row until the reader scrolls up (the IDE's FOLLOW, `keep_list_place`).
pub fn follow_chat(view: &mut SurfaceView, cx: &mut gpui::Context<SurfaceView>) {
    view.keep_list_place(CHAT_SCROLL, true, 0, cx);
}

/// The document's tree and data into a view, then the feed on top.
pub fn load(view: &mut SurfaceView, doc: &Document, feed: &Feed, cx: &mut gpui::Context<SurfaceView>) {
    if let Some(obj) = doc.data.as_ref().and_then(Value::as_object) {
        for (k, val) in obj {
            if let Err(e) = view.set_data(&format!("/{k}"), Some(val.clone()), cx) {
                tracing::warn!(document = %doc.name, "data /{k}: {e}");
            }
        }
    }
    match &doc.body {
        ede_ui::Body::Nested(tree) => {
            view.set_nested(tree.clone(), cx);
        }
        ede_ui::Body::Flat(components) => {
            view.set_components(components.clone(), cx);
        }
    }
    push_feed(view, feed, cx);
}

/// Every path of the feed into the view.
pub fn push_feed(view: &mut SurfaceView, feed: &Feed, cx: &mut gpui::Context<SurfaceView>) {
    for (path, value) in feed.entries() {
        let _ = view.set_data(path, Some(value.clone()), cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #EDE-65: a refresh gives a view only the paths its document binds
    /// that moved; the taskbar never hears the ground's data.
    #[test]
    fn a_view_gets_only_the_bound_paths_that_moved() {
        let taskbar = uicore_docs::DOCS.iter().find(|d| d.name == "taskbar").unwrap();
        let mut pushed = Pushed::for_source(&serde_json::from_str(taskbar.source).unwrap());
        assert!(pushed.roots.contains("shell") && pushed.roots.contains("previews") && pushed.roots.contains("time"), "{:?}", pushed.roots);
        let mut feed = Feed::new();
        feed.set("/shell", serde_json::json!({"rail": false}));
        feed.set("/time", serde_json::json!("12:00"));
        feed.set("/unbound", serde_json::json!(1));
        pushed.given_all(&feed);
        assert!(pushed.changes(&feed).is_empty(), "nothing moved");
        feed.set("/time", serde_json::json!("12:01"));
        feed.set("/unbound", serde_json::json!(2));
        let moved: Vec<String> = pushed.changes(&feed).into_iter().map(|(p, _)| p).collect();
        assert_eq!(moved, vec!["/time".to_string()]);
        assert!(pushed.changes(&feed).is_empty(), "given once");
    }

    #[test]
    fn bound_roots_are_the_first_segments_of_absolute_paths() {
        let roots = bound_roots(&serde_json::json!({"a": {"@path": "/shell/rail"}, "b": [{"path": "/previews/windows"}, "relative", "/"]}));
        assert_eq!(roots.into_iter().collect::<Vec<_>>(), vec!["previews".to_string(), "shell".to_string()]);
    }

    #[test]
    fn a_hosted_layer_above_a_panel_anchors_at_its_top_edge() {
        let (anchor, gravity) = side_anchor(OverlaySide::Top, OverlayAlign::End);
        assert!(matches!(anchor, PopupAnchor::TopRight));
        assert!(matches!(gravity, PopupGravity::TopLeft));
        let (anchor, gravity) = side_anchor(OverlaySide::Bottom, OverlayAlign::Start);
        assert!(matches!(anchor, PopupAnchor::BottomLeft));
        assert!(matches!(gravity, PopupGravity::BottomRight));
    }

    #[test]
    fn a_popup_document_below_its_anchor_grows_down_and_right() {
        let (anchor, gravity) = placement_anchor(Placement::BottomStart);
        assert!(matches!(anchor, PopupAnchor::BottomLeft));
        assert!(matches!(gravity, PopupGravity::BottomRight));
    }
}
