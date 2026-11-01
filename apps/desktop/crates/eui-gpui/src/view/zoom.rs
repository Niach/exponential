//! EXP-1586: the surface's ZoomableImage leaves. Each one is a
//! [`ZoomableImage`] view the surface owns (like its text fields): created
//! when the node appears, fed its props on every pass (src, natural size,
//! initial, maxScale, controls, the bound `scale`), its focus handle the
//! node's tab stop, its `zoom` / `tap` reported through the core
//! (`zoom` writes the bound `scale` back, `Surface::event`).

use std::collections::HashSet;

use eui_core::surface::PlacedNode;
use eui_core::zoom::ZoomInitial;
use gpui::{AnyElement, AppContext as _, Context, Entity, IntoElement as _, SharedString, Subscription, Window};
use serde_json::{json, Value};

use super::SurfaceView;
use crate::controls::{ZoomControls, ZoomStrings, ZoomableImage, ZoomableImageEvent, ZoomableImageOptions};

/// One painted ZoomableImage.
pub(crate) struct Zoom {
    pub entity: Entity<ZoomableImage>,
    _subscription: Subscription,
}

fn num(n: &PlacedNode, key: &str) -> Option<f64> {
    match n.props.get(key)? {
        Value::Number(v) => v.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn text<'a>(n: &'a PlacedNode, key: &str) -> &'a str {
    n.props.get(key).and_then(Value::as_str).unwrap_or("")
}

impl SurfaceView {
    fn zoom_options(&self, n: &PlacedNode) -> ZoomableImageOptions {
        let s = |id: &str| SharedString::from(self.builtin_string(id));
        ZoomableImageOptions {
            alt: SharedString::from(text(n, "alt").to_string()),
            initial: ZoomInitial::parse(text(n, "initial")),
            max_scale: num(n, "maxScale").filter(|v| v.is_finite() && *v > 0.0),
            controls: ZoomControls::parse(text(n, "controls")),
            theme: self.surface.theme().cloned(),
            mode: self.surface.mode(),
            host: self.host.clone(),
            strings: ZoomStrings { zoom_in: s("zoomIn"), zoom_out: s("zoomOut"), zoom_to_fit: s("zoomToFit"), zoom_level: s("zoomLevel"), zoom_fit: s("zoomFit") },
            reduced_motion: self.surface.settings().reduced_motion,
            rtl: self.rtl,
            font_family: Some(self.fonts.default.clone()),
        }
    }

    /// Create views for new ZoomableImages, feed the live ones their props,
    /// drop the gone ones.
    pub(crate) fn sync_zooms(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut keep = HashSet::new();
        let nodes: Vec<PlacedNode> = self.cache.nodes.iter().filter(|n| !n.removed && n.component == "ZoomableImage" && n.part.is_none()).cloned().collect();
        for n in nodes {
            keep.insert(n.id.clone());
            let src = text(&n, "src").to_string();
            let image = crate::media::image_source(self.host.as_ref(), &src);
            let natural = num(&n, "naturalWidth").zip(num(&n, "naturalHeight"));
            let options = self.zoom_options(&n);
            let scale = num(&n, "scale");
            if let Some(z) = self.zooms.get(&n.id) {
                z.entity.update(cx, |z, cx| {
                    z.set_source(src, image, natural, cx);
                    z.set_options(options, cx);
                    z.set_scale(scale, cx);
                });
                continue;
            }
            let entity = cx.new(|cx| {
                let mut z = ZoomableImage::new(src, image, natural, options, cx);
                z.set_scale(scale, cx);
                z
            });
            let id = n.id.clone();
            let sub = cx.subscribe_in(&entity, window, move |this, _, ev: &ZoomableImageEvent, window, cx| this.on_zoom_event(&id, ev, window, cx));
            self.zooms.insert(n.id.clone(), Zoom { entity, _subscription: sub });
        }
        self.zooms.retain(|id, _| keep.contains(id));
    }

    fn on_zoom_event(&mut self, id: &str, ev: &ZoomableImageEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.cache.index_of(id) else { return };
        match ev {
            ZoomableImageEvent::Zoom { scale, at_fit } => self.fire(index, "zoom", Some(json!({"scale": scale, "atFit": at_fit})), cx),
            ZoomableImageEvent::Tap => self.fire(index, "tap", None, cx),
            ZoomableImageEvent::Focus => {
                self.track_focus(window, cx);
                cx.notify();
            }
        }
    }

    /// The view of ZoomableImage `id` (tests, automation).
    pub fn zoomable_image(&self, id: &str) -> Option<Entity<ZoomableImage>> {
        self.zooms.get(id).map(|z| z.entity.clone())
    }

    /// The leaf's content: the node's view at the leaf's size.
    pub(crate) fn zoom_element(&self, id: &str, w: f32, h: f32, cx: &mut Context<Self>) -> Option<AnyElement> {
        let z = self.zooms.get(id)?;
        z.entity.update(cx, |z, _| z.set_viewport_hint(w, h));
        Some(z.entity.clone().into_any_element())
    }

    /// A key on a focused ZoomableImage (`true` = it zoomed).
    pub(crate) fn zoom_key(&mut self, id: &str, key: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(z) = self.zooms.get(id).map(|z| z.entity.clone()) else { return false };
        z.update(cx, |z, cx| z.handle_key(key, window, cx))
    }
}
