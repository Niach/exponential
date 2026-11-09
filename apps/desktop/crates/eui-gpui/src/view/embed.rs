//! #EDE-65 lane M: the Embed native (`fixtures/embed.json`). A nested
//! [`SurfaceView`] painted in place, SANDBOXED: its own surface (id
//! `eui_core::embed::embed_surface_id(host, painted id)`), its own data and
//! ids, never the host's. It is as wide as its slot and as tall as its
//! content: the measurer lays the child out at the leaf's width
//! ([`EmbedSpawn::measure`]); a child whose height moves marks its leaf dirty.
//!
//! The child inherits the host's theme, mode and settings (its popups stay
//! in-surface) and talks to the host through [`EmbedHost`]: icons, media,
//! the URL policy, `open_url`, text links and the markdown hooks are the
//! host's; an inner `event` action re-fires on the host's Embed node as
//! `action` `{name, context}` (merged over `on.action`'s context like any
//! component payload); an inner host function call is DENIED (never
//! forwarded, its event does not follow). Inner text edits, uploads, hovers,
//! the camera and task toggles stay inside.
//!
//! Children are cached per painted id + the resolved `surface` value: a new
//! value rebuilds the child, a node that goes drops it.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use eui_core::embed::{data_key_path, embed_error_root, embed_mount, embed_payload, embed_surface_id, EMBED_EVENT};
use eui_core::host::{MediaOptions, MediaRequest, TextLink, UrlPolicy};
use eui_core::measure::TextStyle;
use eui_core::surface::{PlacedNode, SurfaceSettings};
use eui_core::theme::{Mode, ModeSetting, ResolvedTheme};
use gpui::{div, prelude::*, AnyElement, App, Context, Entity, Subscription, WeakEntity, Window};
use serde_json::{Map, Value};

use super::{SurfaceView, SurfaceViewOptions};
use crate::host::{ActionEvent, AppFiles, FunctionCallEvent, HostPlugin, MarkdownAttachment, MarkdownOptions};

/// What a child takes from its host (compared each pass: a change is
/// handed down).
#[derive(Clone)]
pub(crate) struct Inherit {
    theme: Option<Arc<ResolvedTheme>>,
    mode: Mode,
    settings: SurfaceSettings,
    pixel_snap: bool,
}

impl PartialEq for Inherit {
    fn eq(&self, other: &Self) -> bool {
        same_theme(self.theme.as_ref(), other.theme.as_ref()) && self.mode == other.mode && self.settings == other.settings && self.pixel_snap == other.pixel_snap
    }
}

fn same_theme(a: Option<&Arc<ResolvedTheme>>, b: Option<&Arc<ResolvedTheme>>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

/// One mounted Embed.
pub(crate) struct EmbedChild {
    /// The resolved `surface` value it was built from.
    surface: Value,
    pub(crate) view: Entity<SurfaceView>,
    /// The last (width, height) the measurer answered for it.
    measured: Option<(f32, f32)>,
    inherit: Inherit,
    _resized: Subscription,
}

/// The children by painted Embed id.
pub(crate) type Embeds = Rc<RefCell<HashMap<String, EmbedChild>>>;

/// Everything creating and measuring a child needs, taken from the host
/// view once per pass (the measurer holds no view).
pub(crate) struct EmbedSpawn {
    host_surface_id: String,
    host: Rc<dyn HostPlugin>,
    this: WeakEntity<SurfaceView>,
    inherit: Inherit,
    /// The host's width: an Embed's max-content width (it fills its slot).
    width: f32,
    trace: bool,
}

impl EmbedSpawn {
    /// The child for Embed `id` showing `surface`: the cached one, else a
    /// new view (the value changed or it is new).
    pub(crate) fn child(&self, embeds: &Embeds, id: &str, surface: &Value, window: &mut Window, cx: &mut App) -> Entity<SurfaceView> {
        if let Some(c) = embeds.borrow().get(id).filter(|c| c.surface == *surface) {
            return c.view.clone();
        }
        let view = self.spawn(id, surface, window, cx);
        let (this, key) = (self.this.clone(), id.to_string());
        let resized = cx.observe(&view, move |child, cx| {
            let height = child.read(cx).surface_height();
            let _ = this.update(cx, |host, cx| host.embed_resized(&key, height, cx));
        });
        embeds.borrow_mut().insert(id.to_string(), EmbedChild { surface: surface.clone(), view: view.clone(), measured: None, inherit: self.inherit.clone(), _resized: resized });
        view
    }

    fn spawn(&self, id: &str, surface: &Value, window: &mut Window, cx: &mut App) -> Entity<SurfaceView> {
        let mount = embed_mount(surface);
        let (tree, data) = match mount.card.node() {
            Some(tree) => (tree, mount.card.data),
            None => (embed_error_root(), Map::new()),
        };
        let host: Rc<dyn HostPlugin> = Rc::new(EmbedHost { inner: self.host.clone(), host_view: self.this.clone(), embed_id: id.to_string() });
        let options = SurfaceViewOptions {
            surface_id: embed_surface_id(&self.host_surface_id, id),
            theme: self.inherit.theme.clone(),
            mode: self.inherit.mode,
            host,
            settings: Some(child_settings(&self.inherit.settings)),
            pixel_snap: self.inherit.pixel_snap,
            ..Default::default()
        };
        let trace = self.trace;
        let view = cx.new(|cx| SurfaceView::new(options, window, cx));
        view.update(cx, |v, cx| {
            v.trace_paint(trace);
            // The child changes only through itself: an unchanged one skips
            // its pass on every frame's measure.
            v.set_measure_memo(true, cx);
            let outcome = v.set_nested(tree, cx);
            if !outcome.issues.is_empty() && std::env::var_os("EXP_UI_TRACE").is_some() {
                eprintln!("[exponential-ui gpui] embed {id}: {:?}", outcome.issues);
            }
            for (key, value) in data {
                let _ = v.set_data(&data_key_path(&key), Some(value), cx);
            }
        });
        view
    }

    /// The Embed leaf at `wrap` (`None` = max-content: the host's width;
    /// `Some(0)` = min-content: no width): the child's height at that width.
    pub(crate) fn measure(&self, embeds: &Embeds, id: &str, surface: &Value, wrap: Option<f32>, window: &mut Window, cx: &mut App) -> (f32, f32) {
        let child = self.child(embeds, id, surface, window, cx);
        let width = match wrap {
            Some(w) if w > 0.0 && w.is_finite() => w,
            _ => self.width,
        };
        let height = f32::from(child.update(cx, |v, cx| v.measure_at(Some(width), window, cx)).height);
        if let Some(c) = embeds.borrow_mut().get_mut(id) {
            c.measured = Some((width, height));
        }
        (if wrap == Some(0.0) { 0.0 } else { width }, height)
    }
}

/// The child's settings: the host's, its popups in-surface (the host hosts
/// only its own layers).
fn child_settings(settings: &SurfaceSettings) -> SurfaceSettings {
    SurfaceSettings { hosted_popups: false, ..settings.clone() }
}

impl SurfaceView {
    pub(crate) fn embed_spawn(&self) -> EmbedSpawn {
        EmbedSpawn {
            host_surface_id: self.surface.id.clone(),
            host: self.host.clone(),
            this: self.this.clone(),
            inherit: Inherit { theme: self.surface.theme().cloned(), mode: self.surface.mode(), settings: self.surface.settings().clone(), pixel_snap: self.pixel_snap },
            width: self.width,
            trace: self.paint_trace.borrow().is_some(),
        }
    }

    /// Before a pass: hand a changed theme / mode / settings down, and mark
    /// the leaf of a child whose height moved since it was measured.
    pub(crate) fn sync_embeds(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.embeds.borrow().is_empty() {
            return;
        }
        let inherit = self.embed_spawn().inherit;
        let children: Vec<(String, Entity<SurfaceView>, Option<(f32, f32)>, bool)> = self.embeds.borrow().iter().map(|(id, c)| (id.clone(), c.view.clone(), c.measured, c.inherit != inherit)).collect();
        for (id, view, measured, stale) in children {
            if stale {
                let inherit = inherit.clone();
                view.update(cx, |v, cx| {
                    if !same_theme(v.surface.theme(), inherit.theme.as_ref()) {
                        v.set_theme(inherit.theme.clone(), cx);
                    }
                    v.set_settings(child_settings(&inherit.settings), window, cx);
                    if inherit.settings.mode != ModeSetting::System {
                        v.set_mode(inherit.mode, cx);
                    }
                });
                if let Some(c) = self.embeds.borrow_mut().get_mut(&id) {
                    c.inherit = inherit.clone();
                }
            }
            let Some((width, height)) = measured else { continue };
            let now = f32::from(view.update(cx, |v, cx| v.measure_at(Some(width), window, cx)).height);
            if (now - height).abs() > 0.25 {
                if let Some(i) = self.cache.index_of(&id) {
                    self.surface.mark_dirty(i);
                }
            }
        }
    }

    /// After a pass: drop the children whose Embed went.
    pub(crate) fn prune_embeds(&mut self) {
        if self.embeds.borrow().is_empty() {
            return;
        }
        let live: HashSet<&str> = self.cache.nodes.iter().filter(|n| !n.removed && n.component == "Embed").map(|n| n.id.as_str()).collect();
        self.embeds.borrow_mut().retain(|id, _| live.contains(id.as_str()));
    }

    /// A child laid out at a new height (it changed inside): its leaf
    /// measures again.
    pub(crate) fn embed_resized(&mut self, id: &str, height: f32, cx: &mut Context<Self>) {
        let moved = self.embeds.borrow().get(id).and_then(|c| c.measured).is_some_and(|(_, h)| (h - height).abs() > 0.25);
        if moved {
            if let Some(i) = self.cache.index_of(id) {
                self.surface.mark_dirty(i);
            }
            self.changed = true;
            cx.notify();
        }
    }

    /// An inner `event` action: the host's Embed `id` fires `action`
    /// `{name, context}` through the host surface (no `on.action` = nothing).
    pub(crate) fn embed_action(&mut self, id: &str, name: &str, context: &Value, cx: &mut Context<Self>) {
        if let Some(index) = self.cache.index_of(id) {
            self.fire(index, EMBED_EVENT, Some(embed_payload(name, Some(context))), cx);
        }
    }

    /// The Embed leaf's content: its child view filling the frame (an
    /// exiting layer paints none: the view is the open tree's).
    pub(crate) fn embed_element(&self, n: &PlacedNode, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.ghosting.get() {
            return None;
        }
        let surface = n.props.get("surface").cloned().unwrap_or(Value::Null);
        let child = self.embed_spawn().child(&self.embeds, &n.id, &surface, window, cx);
        Some(div().size_full().child(child).into_any_element())
    }

    /// #EDE-65: the nested view Embed `id` paints (a test and automation
    /// aid like [`Self::painted_bounds`]).
    pub fn embedded(&self, id: &str) -> Option<Entity<SurfaceView>> {
        self.embeds.borrow().get(id).map(|c| c.view.clone())
    }
}

/// The child's host: the host's look-up hooks, the Embed's `action` for
/// inner events, nothing else.
struct EmbedHost {
    inner: Rc<dyn HostPlugin>,
    host_view: WeakEntity<SurfaceView>,
    embed_id: String,
}

impl HostPlugin for EmbedHost {
    fn icon(&self, name: &str) -> Option<gpui::SharedString> {
        self.inner.icon(name)
    }

    fn icon_element(&self, name: &str, size: f32, color: gpui::Hsla, cx: &App) -> Option<AnyElement> {
        self.inner.icon_element(name, size, color, cx)
    }

    fn on_action(&self, event: &ActionEvent, cx: &mut App) {
        let (host, id, name, context) = (self.host_view.clone(), self.embed_id.clone(), event.name.clone(), event.context.clone());
        // Deferred: the inner press runs inside the child's update, and the
        // host may be mid-update itself.
        cx.defer(move |cx| {
            let _ = host.update(cx, |v, cx| v.embed_action(&id, &name, &context, cx));
        });
    }

    /// DENIED: an inner host function never reaches the host.
    fn on_function_call(&self, event: &FunctionCallEvent, _cx: &mut App) {
        if std::env::var_os("EXP_UI_TRACE").is_some() {
            eprintln!("[exponential-ui gpui] embed {}: host function {} denied", event.surface_id, event.name);
        }
    }

    /// DENIED, and its event does not follow.
    fn on_function_call_then(&self, event: &FunctionCallEvent, _then: Box<dyn FnOnce(&mut App)>, cx: &mut App) {
        self.on_function_call(event, cx);
    }

    fn url_policy(&self) -> Option<UrlPolicy> {
        self.inner.url_policy()
    }

    fn media_options(&self) -> MediaOptions {
        self.inner.media_options()
    }

    fn media_allowed(&self, src: &str) -> bool {
        self.inner.media_allowed(src)
    }

    fn media_request(&self, src: &str) -> Option<MediaRequest> {
        self.inner.media_request(src)
    }

    fn open_url(&self, url: &str, cx: &mut App) {
        self.inner.open_url(url, cx)
    }

    fn text_links(&self, text: &str, cx: &App) -> Option<Vec<TextLink>> {
        self.inner.text_links(text, cx)
    }

    fn image_size(&self, src: &str, cx: &App) -> Option<(f32, f32)> {
        self.inner.image_size(src, cx)
    }

    fn image_pressable(&self, cx: &App) -> bool {
        self.inner.image_pressable(cx)
    }

    fn on_image_press(&self, src: &str, window: &mut Window, cx: &mut App) {
        self.inner.on_image_press(src, window, cx)
    }

    fn attachment(&self, href: &str, cx: &App) -> Option<MarkdownAttachment> {
        self.inner.attachment(href, cx)
    }

    fn markdown_hooks_version(&self, text: &str, cx: &App) -> Option<u64> {
        self.inner.markdown_hooks_version(text, cx)
    }

    fn markdown_window_selection(&self, cx: &App) -> bool {
        self.inner.markdown_window_selection(cx)
    }

    fn chip_icon(&self, name: &str, size: f32, color: gpui::Hsla) -> Option<AnyElement> {
        self.inner.chip_icon(name, size, color)
    }

    fn markdown_options(&self, cx: &App) -> MarkdownOptions {
        self.inner.markdown_options(cx)
    }

    fn open_media_file(&self, path: &std::path::Path, cx: &mut App) {
        self.inner.open_media_file(path, cx)
    }

    fn app_files(&self) -> Option<Arc<dyn AppFiles>> {
        self.inner.app_files()
    }

    fn resolve_url(&self, src: &str) -> String {
        self.inner.resolve_url(src)
    }

    fn font_family(&self, family: &str) -> gpui::SharedString {
        self.inner.font_family(family)
    }

    fn announce(&self, text: &str, live: &str, cx: &mut App) {
        self.inner.announce(text, live, cx)
    }

    fn markdown(&self, text: &str, text_style: &TextStyle, width: f32, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        self.inner.markdown(text, text_style, width, window, cx)
    }
}
