//! #FEED-244: one app screen painted HEADLESS to a PNG.
//!
//! The server resolves the screen (`resolveAppScreen`: the package, the
//! template, its data model with every binding already read AS THE VIEWER)
//! into a [`Bundle`]; the session's device runs `exponential-shot` on it and
//! uploads the picture. The painter is the IDE's own (`eui_gpui`'s
//! `ExponentialHost` + `SurfaceView`, the IDE's icons and fonts), drawn by
//! gpui's offscreen renderer (wgpu on Linux, also llvmpipe/lavapipe; Metal on
//! macOS) with the platform's real text system: no window, no display.
//!
//! The picture is STATIC: the template's binds are dropped (the data is
//! already resolved), no source, transport, function or action runs.

use std::borrow::Cow;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use eui_core::theme::{Mode, ResolvedTheme};
use eui_gpui::host::HostPlugin;
use eui_gpui::runtime::{ExponentialHost, HostOptions};
use gpui::{px, size, AppContext as _, AssetSource, Entity, HeadlessAppContext, Render, Result as GpuiResult, SharedString};
use serde_json::{json, Value};

/// The bundle format this binary reads (`schema`). The server writes
/// `BUNDLE_SCHEMA` in `apps/web/src/lib/apps/screenshot.ts`.
pub const BUNDLE_SCHEMA: u64 = 1;

/// The largest edge a bundle may ask for (px).
pub const MAX_EDGE: f32 = 4096.0;

/// The surface id the screen is painted on.
const SURFACE_ID: &str = "shot";

/// One screen to paint.
#[derive(Debug, Clone, PartialEq)]
pub struct Bundle {
    pub package: Value,
    pub template_id: String,
    pub data: Value,
    pub width: f32,
    pub height: f32,
    pub mode: Mode,
}

/// Parse and check a bundle: every refusal is a sentence for the agent.
pub fn parse_bundle(value: &Value) -> Result<Bundle, String> {
    let schema = value.get("schema").and_then(Value::as_u64).unwrap_or(0);
    if schema != BUNDLE_SCHEMA {
        return Err(format!("this exponential-shot reads bundle schema {BUNDLE_SCHEMA}, got {schema}: update Exponential on this device"));
    }
    let package = value.get("package").filter(|p| p.is_object()).cloned().ok_or("the bundle has no package")?;
    let template_id = value.get("templateId").and_then(Value::as_str).filter(|t| !t.is_empty()).ok_or("the bundle has no templateId")?.to_string();
    if package.get("templates").and_then(|t| t.get(&template_id)).is_none() {
        return Err(format!("the package has no template {template_id}"));
    }
    let data = match value.get("data") {
        None | Some(Value::Null) => json!({}),
        Some(d @ Value::Object(_)) => d.clone(),
        Some(_) => return Err("the bundle's data is not an object".into()),
    };
    let edge = |key: &str| -> Result<f32, String> {
        let v = value.get(key).and_then(Value::as_f64).ok_or_else(|| format!("the bundle has no {key}"))? as f32;
        if !(1.0..=MAX_EDGE).contains(&v) {
            return Err(format!("{key} {v} is outside 1..{MAX_EDGE}"));
        }
        Ok(v)
    };
    let mode = match value.get("mode").and_then(Value::as_str) {
        None | Some("dark") => Mode::Dark,
        Some("light") => Mode::Light,
        Some(other) => return Err(format!("unknown mode {other}")),
    };
    Ok(Bundle { package, template_id, data, width: edge("width")?, height: edge("height")?, mode })
}

/// The messages that open the screen: the template's `createSurface` (its
/// record macros filled, the resolved data as its model) WITHOUT its
/// `bindDataModel`s: the data is already read, nothing may re-read it here.
pub fn surface_messages(bundle: &Bundle) -> Result<Vec<Value>, String> {
    let messages = eui_core::host::package::template_messages(&bundle.package, &bundle.template_id, SURFACE_ID, Some(&bundle.data))
        .ok_or_else(|| format!("the package has no template {}", bundle.template_id))?;
    Ok(messages.into_iter().filter(|m| m.get("bindDataModel").is_none()).collect())
}

/// The package's own theme (`theme`, a built-in id), else Exponential's.
pub fn package_theme(package: &Value) -> Arc<ResolvedTheme> {
    package
        .get("theme")
        .and_then(Value::as_str)
        .and_then(eui_core::themes::builtin_theme)
        .unwrap_or_else(|| eui_core::themes::builtin_theme("exponential").unwrap_or_else(eui_core::themes::default_theme))
}

#[derive(rust_embed::RustEmbed)]
#[folder = "../../assets"]
#[include = "fonts/**/*.ttf"]
#[include = "icons/**/*.svg"]
struct Embedded;

struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> GpuiResult<Option<Cow<'static, [u8]>>> {
        Ok(Embedded::get(path).map(|f| f.data))
    }

    fn list(&self, path: &str) -> GpuiResult<Vec<SharedString>> {
        Ok(Embedded::iter().filter(|p| p.starts_with(path)).map(|p| SharedString::from(p.to_string())).collect())
    }
}

/// The IDE's icon rule: a registry concept → its lucide SVG, else the name
/// itself as a lucide glyph.
struct Icons(HashMap<String, String>);

impl HostPlugin for Icons {
    fn icon(&self, name: &str) -> Option<SharedString> {
        let lucide = self.0.get(name).map(String::as_str).unwrap_or(name);
        let path = format!("icons/{lucide}.svg");
        Embedded::get(&path).is_some().then(|| SharedString::from(path))
    }
}

fn icons() -> Icons {
    let concepts: Value = serde_json::from_str(include_str!("../../../../../packages/icons/icons.json")).unwrap_or(Value::Null);
    Icons(
        concepts["semantic"]
            .as_object()
            .map(|m| m.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect())
            .unwrap_or_default(),
    )
}

/// The app extension (IconDisc, LiveDot…), so a package using it validates;
/// its natives paint their fallback here.
const APP_EXTENSION_JSON: &str = include_str!("../../../../../packages/ui/exponential-ui/extension.json");

struct Frame {
    surface: Option<Entity<eui_gpui::view::SurfaceView>>,
    background: gpui::Hsla,
}

impl Render for Frame {
    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
        use gpui::{div, ParentElement as _, Styled as _};
        div().size_full().bg(self.background).children(self.surface.clone())
    }
}

/// Paint the bundle. `Err` = a sentence the device sends back (no GPU
/// adapter, an invalid package…).
pub fn render(bundle: &Bundle) -> Result<image::RgbaImage, String> {
    let messages = surface_messages(bundle)?;
    // Probe the renderer first: no adapter = a clear refusal, not a panic.
    gpui_platform::current_headless_renderer().map_err(|e| format!("this device has no GPU renderer for a headless picture ({e:#})"))?;
    let text = gpui_platform::current_platform(true).text_system();
    let mut cx = HeadlessAppContext::with_platform(text, Arc::new(Assets), gpui_platform::current_headless_renderer);
    cx.update(|cx| {
        let fonts = Embedded::iter().filter(|p| p.ends_with(".ttf")).filter_map(|p| Embedded::get(&p).map(|f| f.data)).collect();
        let _ = cx.text_system().add_fonts(fonts);
        gpui_component::init(cx);
    });
    let theme = package_theme(&bundle.package);
    let background = theme
        .modes
        .get(bundle.mode)
        .color
        .get("background")
        .and_then(|hex| eui_gpui::paint::color::parse_hex(hex))
        .unwrap_or_else(|| if bundle.mode == Mode::Dark { gpui::rgb(0x0c0c0f).into() } else { gpui::rgb(0xffffff).into() });
    let issues: Rc<std::cell::RefCell<Vec<String>>> = Rc::default();
    let options = HostOptions {
        extensions: eui_core::extension::parse_extension(APP_EXTENSION_JSON).map(|e| vec![e]).unwrap_or_default(),
        theme: Some(theme),
        mode: bundle.mode,
        plugin: Rc::new(icons()),
        read_only: true,
        ..HostOptions::default()
    };
    let host = cx.new(|cx| ExponentialHost::new(options, cx));
    let refused = host.update(&mut cx, |host, cx| {
        for issue in host.install_package(&bundle.package) {
            issues.borrow_mut().push(format!("{issue:?}"));
        }
        let mut refused = Vec::new();
        for message in &messages {
            for op in host.receive_trusted(message, cx) {
                if let Some(error) = op.get("message").and_then(|m| m.get("error")) {
                    refused.push(error.get("message").and_then(Value::as_str).unwrap_or("the screen was refused").to_string());
                }
            }
        }
        refused
    });
    let surface = host.read_with(&cx, |host, _| host.surface(SURFACE_ID));
    let Some(surface) = surface else {
        let mut why = refused;
        why.extend(issues.borrow().iter().cloned());
        return Err(format!("the screen did not open: {}", if why.is_empty() { "no surface".to_string() } else { why.join("; ") }));
    };
    let window = cx
        .open_window(size(px(bundle.width), px(bundle.height)), move |_, cx| cx.new(|_| Frame { surface: Some(surface), background }))
        .map_err(|e| format!("no headless window: {e:#}"))?;
    // Measure, then lay out at the measured sizes, then settle (images,
    // fonts): the IDE's own three frames.
    for _ in 0..3 {
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .map_err(|e| format!("a frame failed: {e:#}"))?;
    }
    let shot = cx.capture_screenshot(window.into()).map_err(|e| format!("the capture failed: {e:#}"))?;
    // The headless app's leak check would flag the live views.
    std::mem::forget(cx);
    Ok(shot)
}

/// Encode a picture as PNG bytes.
pub fn png(image: &image::RgbaImage) -> Result<Vec<u8>, String> {
    let mut out = std::io::Cursor::new(Vec::new());
    image.write_to(&mut out, image::ImageFormat::Png).map_err(|e| format!("PNG encode: {e}"))?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg() -> Value {
        json!({
            "id": "demo",
            "catalogId": "https://ui.exponential.at/catalogs/core/v1",
            "sources": ["records"],
            "records": {"note": {"schema": {"type": "object", "properties": {"title": {"type": "string"}}}}},
            "templates": {"home": {
                "components": [{"id": "root", "component": "Text", "text": {"@path": "/title"}}],
                "bindings": [{"path": "/notes", "source": "records:note"}]
            }}
        })
    }

    fn bundle() -> Value {
        json!({"schema": 1, "package": pkg(), "templateId": "home", "data": {"title": "Hello"}, "width": 390, "height": 844})
    }

    #[test]
    fn a_bundle_parses_with_its_defaults() {
        let b = parse_bundle(&bundle()).unwrap();
        assert_eq!((b.width, b.height, b.mode), (390.0, 844.0, Mode::Dark));
        assert_eq!(b.data, json!({"title": "Hello"}));
    }

    #[test]
    fn a_bad_bundle_is_a_sentence() {
        let mut v = bundle();
        v["schema"] = json!(2);
        assert!(parse_bundle(&v).unwrap_err().contains("update Exponential"));
        let mut v = bundle();
        v["templateId"] = json!("nope");
        assert_eq!(parse_bundle(&v).unwrap_err(), "the package has no template nope");
        let mut v = bundle();
        v["width"] = json!(9000);
        assert!(parse_bundle(&v).unwrap_err().starts_with("width"));
        let mut v = bundle();
        v["mode"] = json!("sepia");
        assert_eq!(parse_bundle(&v).unwrap_err(), "unknown mode sepia");
    }

    #[test]
    fn the_screen_opens_without_its_binds_on_the_resolved_data() {
        let b = parse_bundle(&bundle()).unwrap();
        let messages = surface_messages(&b).unwrap();
        assert_eq!(messages.len(), 1, "{messages:?}");
        let create = &messages[0]["createSurface"];
        assert_eq!(create["surfaceId"], json!(SURFACE_ID));
        assert_eq!(create["dataModel"], json!({"title": "Hello"}));
    }

    #[test]
    fn the_theme_is_the_packages_else_exponentials() {
        let exp = package_theme(&json!({}));
        assert_eq!(package_theme(&json!({"theme": "no-such-theme"})).id, exp.id);
    }
}
