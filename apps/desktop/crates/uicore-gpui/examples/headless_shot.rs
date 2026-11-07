//! One shell document painted HEADLESS to a PNG: gpui's offscreen renderer
//! with the platform's real text system, no window on any display. The
//! pictures of the IDE's uicore regions (#EXP-1725) come from here, so a
//! capture never touches the machine's screen.
//!
//! ```text
//! cargo run -p uicore-gpui --example headless_shot -- \
//!     --doc sidebar --feed feed.json --size 272x800 --out sidebar.png
//! ```

use std::{borrow::Cow, collections::HashMap, path::PathBuf, rc::Rc, sync::Arc};

use eui_core::theme::Mode;
use eui_gpui::host::HostPlugin;
use gpui::{px, size, AppContext as _, AssetSource, Entity, HeadlessAppContext, Render, Result as GpuiResult, SharedString};
use serde_json::Value;
use uicore_data::Feed;

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

struct Icons(HashMap<String, String>);

impl HostPlugin for Icons {
    fn icon(&self, name: &str) -> Option<SharedString> {
        let lucide = self.0.get(name).map(String::as_str).unwrap_or(name);
        let path = format!("icons/{lucide}.svg");
        Embedded::get(&path).is_some().then(|| SharedString::from(path))
    }
}

struct Frame(Entity<eui_gpui::view::SurfaceView>);

impl Render for Frame {
    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
        use gpui::{div, ParentElement as _, Styled as _};
        div().size_full().bg(gpui::rgb(0x0c0c0f)).text_color(gpui::rgb(0xfafafa)).child(uicore_gpui::inline::region(&self.0))
    }
}

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let name = arg(&args, "--doc").expect("--doc NAME");
    let out = PathBuf::from(arg(&args, "--out").expect("--out FILE"));
    let (w, h) = arg(&args, "--size").and_then(|s| s.split_once('x').map(|(w, h)| (w.parse::<f32>().unwrap(), h.parse::<f32>().unwrap()))).unwrap_or((272.0, 800.0));
    let mut feed = Feed::new();
    if let Some(path) = arg(&args, "--feed") {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(path).expect("the feed file")).expect("the feed's JSON");
        for (k, v) in v.as_object().expect("a feed object") {
            feed.set(k, v.clone());
        }
    }
    let doc = uicore_gpui::inline::document(uicore_docs::doc(&name).expect("a factory document"));

    let text = gpui_platform::current_platform(true).text_system();
    let mut cx = HeadlessAppContext::with_platform(text, Arc::new(Assets), gpui_platform::current_headless_renderer);
    let concepts: Value = serde_json::from_str(include_str!("../../../../../packages/icons/icons.json")).expect("icons.json");
    let icons: HashMap<String, String> = concepts["semantic"].as_object().map(|m| m.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect()).unwrap_or_default();
    cx.update(|cx| {
        cx.text_system().add_fonts(Embedded::iter().filter(|p| p.ends_with(".ttf")).filter_map(|p| Embedded::get(&p).map(|f| f.data)).collect()).expect("the fonts");
        gpui_component::init(cx);
    });
    let theme = uicore_gpui::inline::theme(&eui_core::themes::builtin_theme("exponential").expect("the theme"));
    let plugin: Rc<dyn HostPlugin> = Rc::new(Icons(icons));
    let window = cx
        .open_window(size(px(w), px(h)), |window, cx| {
            let view = uicore_gpui::inline::mount(&doc, &feed, theme, plugin, Mode::Dark, window, cx);
            cx.new(|_| Frame(view))
        })
        .expect("a headless window");
    // The first frame measures the region, the second lays out at its height.
    cx.run_until_parked();
    let _ = cx.capture_screenshot(window.into());
    cx.run_until_parked();
    let shot = cx.capture_screenshot(window.into()).expect("the frame");
    shot.save(&out).expect("the PNG");
    println!("{}", out.display());
}
