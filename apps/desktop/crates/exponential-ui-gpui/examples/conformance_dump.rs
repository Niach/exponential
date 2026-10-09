//! The DESKTOP half of the renderer conformance harness
//! (`packages/exponential-ui/conformance`): every case of
//! `fixtures/conformance-cases.json` laid out by the gpui painter HEADLESS —
//! a `gpui::HeadlessAppContext` whose text system is gpui's own Linux one
//! (`gpui_wgpu::CosmicTextSystem`) holding ONLY the conformance font files
//! (`conformance/fonts.json`, no system fonts), so the frames are the ones the
//! IDE would place with those fonts, without a display — and dumped in the
//! shared frame-dump format (`conformance/dump.ts`).
//!
//! ```text
//! cargo run -p exponential-ui-gpui --example conformance_dump -- \
//!     [--out <file>] [--only <substring of the case key>]
//! ```
//!
//! `--window <case> [--height <px>]` shows one case in a REAL window instead
//! (the perceptual step's desktop capture, under Xvfb).
//!
//! `tests/conformance.rs` includes this file as a module and gates the dump
//! against the committed web baseline. Linux/FreeBSD only for now (macOS and
//! Windows would plug their platform text system in the same way).

#![allow(dead_code)]

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use exponential_ui::layout_tree::NodeKind;
use exponential_ui::surface::{PlacedNode, Surface};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The dump format version (`conformance/dump.ts` `DUMP_FORMAT`).
pub const DUMP_FORMAT: &str = "xui-frame-dump/1";
pub const RENDERER: &str = "gpui-cosmic-headless";

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..").canonicalize().expect("the repo root")
}

pub fn read_json(rel: &str) -> Value {
    let path = repo_root().join(rel);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

pub fn manifest() -> Value {
    read_json("packages/exponential-ui/fixtures/conformance-cases.json")
}

/// One cell of the matrix.
#[derive(Debug, Clone)]
pub struct Case {
    pub key: String,
    pub fixture: String,
    pub theme: String,
    pub mode: String,
    pub width: f32,
    pub direction: String,
}

/// `<fixture>/<theme>/<mode>/<width>/<direction>` — the key both sides use.
pub fn case_key(fixture: &str, theme: &str, mode: &str, width: f32, direction: &str) -> String {
    format!("{fixture}/{theme}/{mode}/{}/{direction}", width as i64)
}

pub fn cases(manifest: &Value) -> Vec<Case> {
    let strs = |k: &str| -> Vec<String> { manifest[k].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default() };
    let widths: Vec<f32> = manifest["widths"].as_array().map(|a| a.iter().filter_map(Value::as_f64).map(|w| w as f32).collect()).unwrap_or_default();
    let mut out = Vec::new();
    for fixture in manifest["fixtures"].as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default() {
        for theme in strs("themes") {
            for mode in strs("modes") {
                for &width in &widths {
                    for direction in strs("directions") {
                        out.push(Case { key: case_key(&fixture, &theme, &mode, width, &direction), fixture: fixture.clone(), theme: theme.clone(), mode: mode.clone(), width, direction });
                    }
                }
            }
        }
    }
    out
}

/// The case's tree (root `direction` = the case's) and its data model.
pub fn fixture_input(manifest: &Value, case: &Case) -> (Value, Map<String, Value>) {
    let spec = &manifest["fixtures"][&case.fixture];
    // `EXP_UI_CONFORMANCE_TREE=<file>`: lay out that tree instead (a
    // debugging aid: one component under the case's theme and width).
    let (mut tree, data) = if let Some(file) = std::env::var_os("EXP_UI_CONFORMANCE_TREE") {
        let text = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.to_string_lossy()));
        (serde_json::from_str(&text).expect("the tree parses"), Map::new())
    } else if let Some(geometry) = spec["geometry"].as_str() {
        let g = read_json(geometry);
        (g["surface"].clone(), g["data"].as_object().cloned().unwrap_or_default())
    } else {
        let tree = read_json(spec["tree"].as_str().expect("fixture tree"));
        let mut data = spec["data"].as_str().map(read_json).and_then(|d| d.as_object().cloned()).unwrap_or_default();
        data.remove("$comment");
        (tree, data)
    };
    if let Some(root) = tree.as_object_mut() {
        let style = root.entry("style").or_insert_with(|| Value::Object(Map::new()));
        if let Some(style) = style.as_object_mut() {
            style.insert("direction".into(), Value::String(case.direction.clone()));
        }
    }
    (tree, data)
}

/// The conformance font set: the files' bytes, the family a theme NAME maps
/// to (substitutes resolved) and the default face.
pub struct FontSet {
    pub files: Vec<(String, Vec<u8>)>,
    pub families: HashMap<String, String>,
    pub default: String,
}

pub fn font_set(manifest: &Value) -> FontSet {
    let fonts = read_json(manifest["fonts"].as_str().expect("fonts"));
    let mut files = Vec::new();
    let mut families = HashMap::new();
    let table = fonts["families"].as_object().cloned().unwrap_or_default();
    for (name, spec) in &table {
        if let Some(faces) = spec["faces"].as_array() {
            families.insert(name.clone(), name.clone());
            for face in faces {
                let rel = face["file"].as_str().expect("face file");
                let bytes = std::fs::read(repo_root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
                files.push((rel.to_string(), bytes));
            }
        } else if let Some(sub) = spec["substitute"].as_str() {
            families.insert(name.clone(), sub.to_string());
        }
    }
    if let Some(path) = fonts["lastResort"]["families"].as_array().into_iter().flatten().filter_map(Value::as_str).find_map(system_font_file) {
        if let Ok(bytes) = std::fs::read(&path) {
            files.push((path, bytes));
        }
    }
    FontSet { files, families, default: fonts["default"].as_str().unwrap_or("Inter").to_string() }
}

/// The file fontconfig resolves for exactly `family` (the last resort).
fn system_font_file(family: &str) -> Option<String> {
    let out = std::process::Command::new("fc-match").args(["-f", "%{family}\n%{file}", family]).output().ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    let (found, file) = text.split_once('\n')?;
    found.split(',').any(|f| f.trim() == family).then(|| file.trim().to_string()).filter(|f| !f.is_empty())
}

/// One placed node (`conformance/dump.ts` `DumpNode`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DumpNode {
    pub id: String,
    pub component: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lh: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseDump {
    pub width: f32,
    pub height: f32,
    pub nodes: Vec<DumpNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dump {
    pub format: String,
    pub renderer: String,
    pub fonts: String,
    pub cases: BTreeMap<String, CaseDump>,
}

fn round2(v: f32) -> f32 {
    (v * 100.0).round() / 100.0
}

/// The text a leaf lays out (the props the measurer shapes).
fn leaf_text(n: &PlacedNode) -> Option<String> {
    for key in ["text", "label", "title", "value"] {
        let s = match n.props.get(key) {
            Some(Value::String(s)) => s.clone(),
            Some(v @ (Value::Number(_) | Value::Bool(_))) => exponential_ui::json::to_js_string(v),
            _ => continue,
        };
        let t = s.split_whitespace().collect::<Vec<_>>().join(" ");
        if !t.is_empty() {
            return Some(t.chars().take(80).collect());
        }
    }
    None
}

/// The main tree's placed nodes, relative to the root's frame; `lines` for
/// the manifest's `textComponents`: the painter's own count when `painted`
/// knows it (a stretched Text keeps its lines), else box height over line
/// height (a Link is one line).
pub fn dump_surface(surface: &mut Surface, text_components: &[String], painted: &HashMap<u32, u32>) -> CaseDump {
    let nodes = surface.nodes();
    let ids: HashMap<u32, String> = nodes.iter().filter(|n| !n.removed).map(|n| (n.index, n.id.clone())).collect();
    let root = nodes.iter().find(|n| !n.removed && n.parent.is_none() && n.layer == 0).and_then(|n| surface.last_frame(n.index)).unwrap_or_default();
    // Pre-order through `children` (paint order), like the DOM's order.
    let mut order: Vec<usize> = Vec::new();
    let mut stack: Vec<usize> = nodes.iter().filter(|n| !n.removed && n.parent.is_none() && n.layer == 0).map(|n| n.index as usize).rev().collect();
    while let Some(i) = stack.pop() {
        order.push(i);
        stack.extend(nodes[i].children.iter().rev().map(|c| *c as usize));
    }
    let mut out = Vec::new();
    for n in order.iter().filter_map(|i| nodes.get(*i)) {
        if n.removed || n.hidden || n.layer != 0 {
            continue;
        }
        let Some(f) = surface.last_frame(n.index) else { continue };
        let mut node = DumpNode { id: n.id.clone(), component: n.component.clone(), part: n.part.clone(), parent: n.parent.and_then(|p| ids.get(&p).cloned()), x: round2(f.x - root.x), y: round2(f.y - root.y), w: round2(f.w), h: round2(f.h), text: None, lines: None, lh: None };
        if n.kind == NodeKind::Leaf {
            if let (Some(text), Some(ts)) = (leaf_text(n), surface.text_style(n.index)) {
                let v = surface.visual(n.index);
                let pad = v.and_then(|v| v.padding).map(|p| p[0] + p[2]).unwrap_or(0.0);
                let border = v.and_then(|v| v.border_widths.map(|b| b[0] + b[2]).or(v.border_width.map(|b| 2.0 * b))).unwrap_or(0.0);
                if ts.line_height > 0.0 && text_components.contains(&n.component) {
                    let lines = match painted.get(&n.index) {
                        Some(l) => *l,
                        None if n.component == "Link" => 1,
                        None => ((f.h - pad - border) / ts.line_height).round().max(1.0) as u32,
                    };
                    node.lines = Some(lines);
                    node.lh = Some(round2(ts.line_height));
                }
                node.text = Some(text);
            }
        }
        out.push(node);
    }
    CaseDump { width: round2(root.w), height: round2(root.h), nodes: drop_collapsed(out) }
}

/// `conformance/dump.ts` `dropCollapsed`: a node whose box is 0×0 together
/// with every descendant's is not placed (the core's `display: none`
/// subtrees; the web drops them outright).
pub fn drop_collapsed(nodes: Vec<DumpNode>) -> Vec<DumpNode> {
    let parent_of: HashMap<&str, Option<&str>> = nodes.iter().map(|n| (n.id.as_str(), n.parent.as_deref())).collect();
    let mut visible: std::collections::HashSet<String> = std::collections::HashSet::new();
    for n in nodes.iter().filter(|n| n.w > 0.0 || n.h > 0.0) {
        // The node and every ancestor are placed (slots are not pre-order).
        let mut id = Some(n.id.as_str());
        while let Some(i) = id.filter(|i| !visible.contains(*i)) {
            visible.insert(i.to_string());
            id = parent_of.get(i).copied().flatten();
        }
    }
    nodes.into_iter().filter(|n| visible.contains(&n.id)).collect()
}

/// The view setup both modes share: the case's theme, mode, locale and
/// width, the fixture tree and its data, an unbounded viewport.
pub mod setup {
    use super::*;
    use std::rc::Rc;

    use exponential_ui::surface::SurfaceSettings;
    use exponential_ui::theme::{Mode, ModeSetting};
    use exponential_ui::NestedNode;
    use exponential_ui_gpui::host::HostPlugin;
    use exponential_ui_gpui::view::{SurfaceView, SurfaceViewOptions};

    /// Maps theme family names onto the conformance families.
    pub struct FontHost(pub HashMap<String, String>);

    impl HostPlugin for FontHost {
        fn font_family(&self, family: &str) -> gpui::SharedString {
            self.0.get(family).cloned().unwrap_or_else(|| family.to_string()).into()
        }
    }

    pub fn options(manifest: &Value, case: &Case, host: Rc<dyn HostPlugin>) -> SurfaceViewOptions {
        let theme = exponential_ui::themes::builtin_theme(&case.theme);
        assert!(theme.is_some(), "unknown theme {}", case.theme);
        let mode = if case.mode == "light" { Mode::Light } else { Mode::Dark };
        let locale = manifest["locale"].as_str().unwrap_or("en-US").to_string();
        let settings = SurfaceSettings { locale, mode: if mode == Mode::Light { ModeSetting::Light } else { ModeSetting::Dark }, ..SurfaceSettings::default() };
        SurfaceViewOptions { surface_id: "conformance".into(), theme, mode, host, width: Some(case.width), settings: Some(settings), ..Default::default() }
    }

    /// Load the case's tree + data into a fresh view.
    pub fn load(view: &mut SurfaceView, manifest: &Value, case: &Case, cx: &mut gpui::Context<SurfaceView>) {
        let (tree, data) = fixture_input(manifest, case);
        let tree: NestedNode = serde_json::from_value(tree).expect("the fixture tree parses");
        let outcome = view.set_nested(tree, cx);
        assert!(outcome.issues.is_empty(), "{}: {:?}", case.key, outcome.issues);
        for (k, v) in data {
            view.set_data(&format!("/{k}"), Some(v), cx);
        }
        // Unbounded: the web surface grows with its content.
        view.set_viewport_height(1_000_000.0, cx);
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub mod headless {
    use super::*;
    use std::borrow::Cow;
    use std::rc::Rc;
    use std::sync::Arc;

    use exponential_ui_gpui::host::HostPlugin;
    use exponential_ui_gpui::view::SurfaceView;
    use gpui::{px, size, AppContext as _, HeadlessAppContext, PlatformTextSystem};

    /// Lay every case out and dump it.
    pub fn dump(cases: &[Case]) -> Dump {
        let manifest = manifest();
        let fonts = font_set(&manifest);
        let text_system = Arc::new(gpui_wgpu::CosmicTextSystem::new_without_system_fonts(&fonts.default));
        text_system.add_fonts(fonts.files.iter().map(|(_, b)| Cow::Owned(b.clone())).collect()).expect("the conformance fonts load");
        let mut cx = HeadlessAppContext::new(text_system);
        cx.update(gpui_component::init);
        let host: Rc<dyn HostPlugin> = Rc::new(setup::FontHost(fonts.families.clone()));
        let text_components: Vec<String> = manifest["textComponents"].as_array().into_iter().flatten().filter_map(|v| v.as_str().map(String::from)).collect();
        let mut out = BTreeMap::new();
        for case in cases {
            let options = setup::options(&manifest, case, host.clone());
            let window = cx.open_window(size(px(case.width), px(1200.0)), move |window, cx| cx.new(|cx| SurfaceView::new(options, window, cx))).expect("a headless window");
            let dump = window
                .update(&mut cx, |view, window, cx| {
                    setup::load(view, &manifest, case, cx);
                    view.layout_now(window, cx);
                    view.layout_now(window, cx);
                    let painted: HashMap<u32, u32> = view.placed_nodes().iter().filter_map(|n| view.painted_lines(n.index, window).map(|l| (n.index, l))).collect();
                    dump_surface(view.surface_mut(), &text_components, &painted)
                })
                .expect("the window updates");
            let _ = window.update(&mut cx, |_, window, _| window.remove_window());
            cx.run_until_parked();
            out.insert(case.key.clone(), dump);
        }
        let font_ids: Vec<&str> = fonts.files.iter().map(|(p, _)| p.rsplit('/').next().unwrap_or(p)).collect();
        Dump { format: DUMP_FORMAT.into(), renderer: RENDERER.into(), fonts: font_ids.join(","), cases: out }
    }
}

/// `--window <case> [--height <px>]`: the case in a REAL gpui window at
/// (0, 0), `width × height`, titled `xui-conformance` — the pixel step runs
/// it under Xvfb (FONTCONFIG_FILE pointing at the conformance fonts only) and
/// captures it with `import`. The window paints on input: the capture script
/// moves the pointer once before grabbing.
pub fn window_mode(key: &str, height: f32) {
    use std::borrow::Cow;
    use std::rc::Rc;

    use exponential_ui_gpui::host::HostPlugin;
    use exponential_ui_gpui::view::SurfaceView;
    use gpui::{div, point, prelude::*, px, size, App, Bounds, TitlebarOptions, WindowBounds, WindowOptions};

    let manifest = manifest();
    let case = cases(&manifest).into_iter().find(|c| c.key == key).unwrap_or_else(|| panic!("unknown conformance case {key}"));
    let fonts = font_set(&manifest);
    let app = gpui_platform::application().with_assets(gpui_component_assets::Assets);
    app.run(move |cx: &mut App| {
        gpui_component::init(cx);
        let _ = cx.text_system().add_fonts(fonts.files.iter().map(|(_, b)| Cow::Owned(b.clone())).collect());
        let host: Rc<dyn HostPlugin> = Rc::new(setup::FontHost(fonts.families.clone()));
        let options = setup::options(&manifest, &case, host);
        let bg = options.theme.as_ref().and_then(|t| t.modes.get(options.mode).color.get("background").cloned()).and_then(|c| exponential_ui_gpui::paint::color::parse_hex(&c)).unwrap_or(gpui::black());
        let bounds = Bounds { origin: point(px(0.0), px(0.0)), size: size(px(case.width), px(height)) };
        cx.open_window(WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), titlebar: Some(TitlebarOptions { title: Some("xui-conformance".into()), ..Default::default() }), ..Default::default() }, move |window, cx| {
            let view = cx.new(|cx| {
                let mut v = SurfaceView::new(options, window, cx);
                setup::load(&mut v, &manifest, &case, cx);
                v
            });
            cx.new(|_| Shell { view, bg })
        })
        .expect("a window");
        cx.activate(true);
    });

    struct Shell {
        view: gpui::Entity<SurfaceView>,
        bg: gpui::Hsla,
    }

    impl Render for Shell {
        fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
            div().size_full().bg(self.bg).child(self.view.clone())
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    if let Some(key) = arg("--window") {
        let height = arg("--height").and_then(|h| h.parse::<f32>().ok()).unwrap_or(1200.0);
        window_mode(&key, height);
        return;
    }
    let manifest = manifest();
    let mut all = cases(&manifest);
    if let Some(only) = arg("--only") {
        all.retain(|c| c.key.contains(&only));
    }
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        let started = std::time::Instant::now();
        let dump = headless::dump(&all);
        let json = serde_json::to_string(&dump).expect("the dump serializes");
        match arg("--out") {
            Some(path) => {
                std::fs::write(&path, json).unwrap_or_else(|e| panic!("{path}: {e}"));
                eprintln!("conformance_dump: {} cases → {path} ({} ms)", dump.cases.len(), started.elapsed().as_millis());
            }
            None => println!("{json}"),
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
    {
        let _ = all;
        eprintln!("conformance_dump: the headless text system is wired for Linux/FreeBSD only");
        std::process::exit(2);
    }
}
