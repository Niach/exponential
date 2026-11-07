//! eUI round 8: `canvas` (General components): THE eUI Canvas painted by
//! the gpui painter (`eui_gpui::canvas`: the shapes' tiny-skia raster, the
//! `text` ops set natively on top). The specimens are the web entry's own
//! file (`packages/exponential-ui/fixtures/canvas-specimen.json`, also the
//! generated `exponential-ui-canvas` specimen), so the twin cannot drift: a bar chart with axis labels, a line plot, and a pointer
//! pad whose readout (`/pressed`) shows the last press in drawing units.

use std::cell::RefCell;
use std::rc::Rc;

use eui_core::theme::Mode;
use eui_core::types::NestedNode;
use eui_gpui::host::{ActionEvent, HostPlugin};
use eui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{div, App, Div, Entity, ParentElement as _, Styled as _, WeakEntity, Window};
use gpui_component::ActiveTheme as _;
use serde_json::{json, Value};

pub(crate) const ID: &str = "canvas";
pub(crate) const OWNER: &str = "FEED-197";

const SPECIMENS: &str = include_str!("../../../../../../../packages/exponential-ui/fixtures/canvas-specimen.json");

fn specimens() -> Value {
    serde_json::from_str(SPECIMENS).expect("canvas-specimen.json")
}

/// The pad's host: a `canvasPress` writes `x <x> · y <y>` into `/pressed`.
struct PointerHost(Rc<RefCell<Option<WeakEntity<SurfaceView>>>>);

impl HostPlugin for PointerHost {
    fn on_action(&self, event: &ActionEvent, cx: &mut App) {
        if event.name != "canvasPress" {
            return;
        }
        let p = event.payload.as_ref().unwrap_or(&event.context);
        let (x, y) = (p.get("x").and_then(Value::as_f64).unwrap_or(0.0), p.get("y").and_then(Value::as_f64).unwrap_or(0.0));
        let Some(view) = self.0.borrow().clone() else { return };
        // The action arrives inside the view's own update: write after it.
        cx.defer(move |cx| {
            let _ = view.update(cx, |v, cx| v.set_data("/pressed", Some(json!(format!("x {x} · y {y}"))), cx));
        });
    }
}

fn surface(window: &mut Window, cx: &mut App) -> Entity<SurfaceView> {
    let dark = cx.theme().is_dark();
    window.use_keyed_state("sg-canvas", cx, move |window, cx| {
        let doc = specimens();
        let slot = Rc::new(RefCell::new(None));
        let options = SurfaceViewOptions {
            surface_id: "sg-canvas".into(),
            theme: eui_core::themes::builtin_theme("exponential"),
            mode: if dark { Mode::Dark } else { Mode::Light },
            host: Rc::new(PointerHost(slot.clone())),
            ..SurfaceViewOptions::default()
        };
        let mut view = SurfaceView::new(options, window, cx);
        for (k, v) in doc["data"].as_object().into_iter().flatten() {
            let _ = view.set_data(&format!("/{k}"), Some(v.clone()), cx);
        }
        match serde_json::from_value::<NestedNode>(doc["cases"].clone()) {
            Ok(tree) => {
                for issue in &view.set_nested(tree, cx).issues {
                    log::warn!("[exponential-ui styleguide] sg-canvas: {}: {}", issue.id, issue.message);
                }
            }
            Err(err) => log::warn!("[exponential-ui styleguide] sg-canvas does not parse: {err}"),
        }
        *slot.borrow_mut() = Some(cx.entity().downgrade());
        view
    })
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    div().w_full().child(surface(window, cx))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every Canvas of the shared specimens compiles in the engine.
    #[test]
    fn the_specimens_compile() {
        fn walk(n: &Value, found: &mut usize) {
            if n["component"] == "Canvas" {
                *found += 1;
                let props = n["props"].as_object().unwrap().clone();
                if let Err(e) = eui_gpui::canvas::CanvasPlayer::compile(&props) {
                    panic!("{}: {e}", n["id"]);
                }
            }
            for c in n["children"].as_array().into_iter().flatten() {
                walk(c, found);
            }
        }
        let mut found = 0;
        walk(&specimens()["cases"], &mut found);
        assert_eq!(found, 3);
    }
}
