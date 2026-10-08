//! VAPP-90 — the Exponential UI kitchen sink, DEV-ONLY
//! (`EXP_DEV_SCREEN=exponential-ui-kitchen-sink`, [`Screen::ExponentialUiKitchenSink`]).
//!
//! The core catalog's kitchen-sink fixture
//! (`packages/exponential-ui/fixtures/kitchen-sink.json`) painted INSIDE the
//! IDE by the SDK's gpui painter (`exponential-ui-gpui`), the desktop twin of
//! the web route `/exponential-ui-kitchen-sink` and the surface the
//! view-catalog entry `exponential-ui-kitchen-sink` photographs. The IDE is a
//! HOST here like any third party would be: it hands the painter a
//! [`HostPlugin`] (its icon registry by concept, an action log, the 150 ms
//! input echo the web harness performs) and nothing else — no IDE type
//! reaches the SDK.
//!
//! Env (all DEV-ONLY, silently ignored when unset or unparseable):
//! `EXP_DEV_EXPONENTIAL_UI_THEME=neutral|exponential|playful` (default
//! `exponential`), `EXP_DEV_EXPONENTIAL_UI_MODE=light|dark` (default `dark`,
//! the IDE's own), `EXP_DEV_EXPONENTIAL_UI_BENCH=<n>` (the core's synthetic
//! n-node bench tree instead of the fixture — the 200-node budget check),
//! `EXP_DEV_EXPONENTIAL_UI_POSTS=<n>` (rows for the fixture's `/posts` List;
//! past 24 it windows. The web reference seeds none, so the default is none),
//! `EXP_DEV_EXPONENTIAL_UI_SCROLL=<px>` (pre-scroll the pane once the content
//! is laid out, so a capture photographs a lower section of the sink),
//! `EXP_DEV_EXPONENTIAL_UI_LOCALE=<bcp47>` (`ar` / `he` paint the sink RTL),
//! `EXP_DEV_EXPONENTIAL_UI_DENSITY=compact|comfortable`,
//! `EXP_DEV_EXPONENTIAL_UI_CONTRAST=high`, `EXP_DEV_EXPONENTIAL_UI_REDUCED_MOTION=1`
//! and `EXP_DEV_EXPONENTIAL_UI_MODE=system` (follows the window appearance).
//! `EXP_UI_TRACE=1` makes the painter print one line per layout pass.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use exponential_ui::surface::{ContrastSetting, PlacedNode, SurfaceSettings};
use exponential_ui::theme::{Density, Mode, ModeSetting};
use exponential_ui::types::NestedNode;
use exponential_ui_gpui::host::{ActionEvent, HostPlugin, InputEvent, InputKind, UploadEvent};
use exponential_ui_gpui::view::{SurfaceView, SurfaceViewOptions};
use gpui::{
    div, prelude::FluentBuilder as _, px, App, AppContext as _, Context, Entity,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement as _, Styled as _, WeakEntity, Window,
};
use gpui_component::{ActiveTheme as _, IconNamed as _};
use serde_json::{json, Value};

use crate::navigation::Screen;
use crate::surface::{PillMode, PillSize};

/// The shared fixture, byte for byte what the web route renders.
const KITCHEN_SINK_JSON: &str =
    include_str!("../../../../../packages/exponential-ui/fixtures/kitchen-sink.json");

/// The fake host round trip of the typing test (the web harness's 150 ms).
const ECHO_DELAY: Duration = Duration::from_millis(150);

/// The surface id, the same the web harness uses.
const SURFACE_ID: &str = "kitchen-sink";

/// The built-in theme ids, in the order the switcher shows them.
const THEME_IDS: [&str; 3] = ["neutral", "exponential", "playful"];

/// The IDE as an Exponential UI host: icons by CONCEPT from the generated
/// registry (then raw Lucide names, like the web host map), actions logged
/// into the caption, host-owned input edits echoed back 150 ms later into the
/// bound data path (so the field proves the echo rule: a stale echo never
/// clobbers typing).
struct DevHost {
    screen: RefCell<Option<WeakEntity<ExponentialUiKitchenSink>>>,
    actions: RefCell<Vec<String>>,
    echo: RefCell<String>,
}

impl DevHost {
    fn notify(&self, cx: &mut App) {
        if let Some(screen) = self.screen.borrow().clone() {
            let _ = screen.update(cx, |_, cx| cx.notify());
        }
    }
}

impl HostPlugin for DevHost {
    fn icon(&self, name: &str) -> Option<SharedString> {
        crate::icons::registry::concept_by_name(name)
            .or_else(|| crate::icons::registry::icon_by_name(name))
            .map(|icon| icon.path())
    }

    fn on_action(&self, event: &ActionEvent, cx: &mut App) {
        let line = format!("{} ← {}", event.name, event.component_id);
        log::info!("[exponential-ui kitchen sink] action {line} {} {:?}", event.context, event.payload);
        let mut actions = self.actions.borrow_mut();
        actions.push(line);
        if actions.len() > 3 {
            let drop = actions.len() - 3;
            actions.drain(..drop);
        }
        drop(actions);
        self.notify(cx);
    }

    fn on_input(&self, event: &InputEvent, cx: &mut App) {
        if !matches!(event.kind, InputKind::Change | InputKind::Commit) {
            return;
        }
        let Some(screen) = self.screen.borrow().clone() else { return };
        let path = event.path.clone();
        let value = event.value.clone();
        cx.spawn(async move |cx| {
            cx.background_executor().timer(ECHO_DELAY).await;
            let _ = screen.update(cx, |this, cx| this.apply_echo(path.as_deref(), value, cx));
        })
        .detach();
    }

    fn announce(&self, text: &str, live: &str, _cx: &mut App) {
        log::info!("[exponential-ui kitchen sink] announce ({live}): {text}");
    }

    fn on_upload(&self, event: &UploadEvent, cx: &mut App) {
        let line = format!("upload {} file(s) ← {}", event.paths.len(), event.component_id);
        log::info!("[exponential-ui kitchen sink] {line} {:?}", event.paths);
        self.actions.borrow_mut().push(line);
        self.notify(cx);
    }

    fn on_unknown(&self, node: &PlacedNode) {
        log::warn!("[exponential-ui kitchen sink] unknown component {} ({})", node.component, node.id);
    }
}

fn env_trimmed(key: &str) -> Option<String> {
    std::env::var(key).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

fn parse_mode(spec: Option<&str>) -> Mode {
    match spec {
        Some("light") => Mode::Light,
        _ => Mode::Dark,
    }
}

/// The kitchen-sink fixture. Its root pins `direction: ltr`; an RTL locale
/// drops the pin so the locale decides (the core's rule).
fn kitchen_sink_tree(locale: Option<&str>) -> NestedNode {
    let mut raw: serde_json::Value = serde_json::from_str(KITCHEN_SINK_JSON).expect("the kitchen-sink fixture parses");
    if locale.is_some_and(|l| exponential_ui::locale::text_direction(l) == "rtl") {
        if let Some(style) = raw.get_mut("style").and_then(serde_json::Value::as_object_mut) {
            style.remove("direction");
        }
    }
    serde_json::from_value(raw).expect("the kitchen-sink fixture parses")
}

/// The surface settings the DEV env asks for (`None` = the defaults).
fn parse_settings(mode: Mode) -> Option<SurfaceSettings> {
    let locale = env_trimmed("EXP_DEV_EXPONENTIAL_UI_LOCALE");
    let density = env_trimmed("EXP_DEV_EXPONENTIAL_UI_DENSITY").and_then(|d| Density::parse(&d));
    let high = env_trimmed("EXP_DEV_EXPONENTIAL_UI_CONTRAST").as_deref() == Some("high");
    let reduced = env_trimmed("EXP_DEV_EXPONENTIAL_UI_REDUCED_MOTION").is_some_and(|v| v != "0");
    let system = env_trimmed("EXP_DEV_EXPONENTIAL_UI_MODE").as_deref() == Some("system");
    if locale.is_none() && density.is_none() && !high && !reduced && !system {
        return None;
    }
    Some(SurfaceSettings {
        locale: locale.unwrap_or_else(|| "en-US".into()),
        mode: if system {
            ModeSetting::System
        } else if mode == Mode::Light {
            ModeSetting::Light
        } else {
            ModeSetting::Dark
        },
        density: density.unwrap_or_default(),
        contrast: if high { ContrastSetting::High } else { ContrastSetting::Normal },
        reduced_motion: reduced,
        ..SurfaceSettings::default()
    })
}

fn parse_theme(spec: Option<&str>) -> &'static str {
    THEME_IDS.iter().copied().find(|id| Some(*id) == spec).unwrap_or("exponential")
}

/// The fixture's data model: what the web harness seeds (`draft.title`
/// empty) plus `n` posts when asked (the List windows past 24).
fn seed_data(posts: usize) -> Vec<(&'static str, Value)> {
    let mut data = vec![("/draft", json!({"title": ""})), ("/ui", json!({"confirmOpen": false}))];
    if posts > 0 {
        let rows: Vec<Value> = (0..posts)
            .map(|i| json!({"title": format!("Post {}", i + 1), "score": format!("{} points", (posts - i) * 7)}))
            .collect();
        data.push(("/posts", Value::Array(rows)));
    }
    data
}

pub struct ExponentialUiKitchenSink {
    view: Entity<SurfaceView>,
    host: Rc<DevHost>,
    scroll: ScrollHandle,
    theme_id: &'static str,
    mode: Mode,
    bench: Option<usize>,
    /// `EXP_DEV_EXPONENTIAL_UI_SCROLL`, applied once the pane can scroll.
    pending_scroll: Option<f32>,
}

impl ExponentialUiKitchenSink {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let theme_id = parse_theme(env_trimmed("EXP_DEV_EXPONENTIAL_UI_THEME").as_deref());
        let mode = parse_mode(env_trimmed("EXP_DEV_EXPONENTIAL_UI_MODE").as_deref());
        let bench = env_trimmed("EXP_DEV_EXPONENTIAL_UI_BENCH")
            .and_then(|n| n.parse::<usize>().ok())
            .filter(|n| *n > 0);
        let posts = env_trimmed("EXP_DEV_EXPONENTIAL_UI_POSTS")
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap_or(0);
        let pending_scroll = env_trimmed("EXP_DEV_EXPONENTIAL_UI_SCROLL")
            .and_then(|n| n.parse::<f32>().ok())
            .filter(|n| *n > 0.);
        let host = Rc::new(DevHost {
            screen: RefCell::new(None),
            actions: RefCell::new(Vec::new()),
            echo: RefCell::new(String::new()),
        });
        let options = SurfaceViewOptions {
            surface_id: SURFACE_ID.to_string(),
            theme: exponential_ui::themes::builtin_theme(theme_id),
            mode,
            host: host.clone(),
            settings: parse_settings(mode),
            ..SurfaceViewOptions::default()
        };
        let tree: NestedNode = match bench {
            Some(n) => exponential_ui::bench::bench_tree(n),
            None => kitchen_sink_tree(env_trimmed("EXP_DEV_EXPONENTIAL_UI_LOCALE").as_deref()),
        };
        let view = cx.new(|cx| {
            let mut view = SurfaceView::new(options, window, cx);
            for (path, value) in seed_data(posts) {
                view.set_data(path, Some(value), cx);
            }
            let outcome = view.set_nested(tree, cx);
            for issue in &outcome.issues {
                log::warn!("[exponential-ui kitchen sink] {}: {}", issue.id, issue.message);
            }
            view
        });
        *host.screen.borrow_mut() = Some(cx.entity().downgrade());
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        Self {
            view,
            host,
            scroll: ScrollHandle::new(),
            theme_id,
            mode,
            bench,
            pending_scroll,
        }
    }

    /// The host's echo landed: write it into the bound path (the painter
    /// applies it to the field only when no newer revision is outstanding).
    fn apply_echo(&mut self, path: Option<&str>, value: Value, cx: &mut Context<Self>) {
        *self.host.echo.borrow_mut() = match &value {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        if let Some(path) = path {
            self.view.update(cx, |view, cx| view.set_data(path, Some(value), cx));
        }
        cx.notify();
    }

    fn set_theme(&mut self, id: &'static str, cx: &mut Context<Self>) {
        if self.theme_id == id {
            return;
        }
        self.theme_id = id;
        let theme = exponential_ui::themes::builtin_theme(id);
        self.view.update(cx, |view, cx| view.set_theme(theme, cx));
        cx.notify();
    }

    fn set_mode(&mut self, mode: Mode, cx: &mut Context<Self>) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        self.view.update(cx, |view, cx| view.set_mode(mode, cx));
        cx.notify();
    }

    fn caption(&self, cx: &App) -> String {
        let stats = self.view.read(cx).stats().clone();
        let mut caption = format!(
            "{} nodes · {} measure calls · taffy {} µs · wall {} µs · pass {}",
            stats.nodes,
            stats.measure_calls,
            stats.layout_ns / 1_000,
            stats.wall_ns / 1_000,
            stats.passes
        );
        if let Some(n) = self.bench {
            caption = format!("bench {n} · {caption}");
        }
        caption
    }

    fn theme_pill(&self, id: &'static str, cx: &mut Context<Self>) -> impl IntoElement {
        crate::surface::glass_pill(
            SharedString::from(format!("xui-theme-{id}")),
            PillSize::Sm,
            PillMode::Select { selected: self.theme_id == id },
            cx,
        )
        .child(SharedString::from(id))
        .on_click(cx.listener(move |this, _, _, cx| this.set_theme(id, cx)))
    }

    fn mode_pill(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let next = match self.mode {
            Mode::Dark => Mode::Light,
            Mode::Light => Mode::Dark,
        };
        crate::surface::glass_pill("xui-mode", PillSize::Sm, PillMode::Action, cx)
            .child(SharedString::from(self.mode.as_str().to_string()))
            .on_click(cx.listener(move |this, _, _, cx| this.set_mode(next, cx)))
    }
}

impl Render for ExponentialUiKitchenSink {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // DEV-ONLY: the pre-scroll lands once the previous frame gave the
        // pane something to scroll (max_offset is known after a paint).
        if let Some(y) = self.pending_scroll {
            if self.scroll.max_offset().y > px(0.) {
                self.scroll.set_offset(gpui::point(px(0.), px(-y)));
                self.pending_scroll = None;
                cx.notify();
            }
        }
        let muted = cx.theme().muted_foreground;
        let caption = SharedString::from(self.caption(cx));
        let actions = self.host.actions.borrow().join(" · ");
        let echo = self.host.echo.borrow().clone();
        let host_line = SharedString::from(match (actions.is_empty(), echo.is_empty()) {
            (true, true) => String::new(),
            (false, true) => actions,
            (true, false) => format!("host: {echo}"),
            (false, false) => format!("{actions} · host: {echo}"),
        });
        let _ = Screen::ExponentialUiKitchenSink;
        gpui_component::v_flex()
            .id("exponential-ui-kitchen-sink")
            .size_full()
            .child(
                gpui_component::h_flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_4()
                    .py_1()
                    .text_xs()
                    .text_color(muted)
                    .child(
                        gpui_component::h_flex()
                            .items_center()
                            .gap_1()
                            .child("Exponential UI")
                            .children(THEME_IDS.iter().map(|id| self.theme_pill(id, cx)))
                            .child(self.mode_pill(cx)),
                    )
                    .child(div().truncate().child(caption)),
            )
            .when(!host_line.is_empty(), |col| {
                col.child(
                    div()
                        .flex_shrink_0()
                        .px_4()
                        .pb_1()
                        .font_family("JetBrains Mono")
                        .text_xs()
                        .text_color(muted)
                        .truncate()
                        .child(host_line),
                )
            })
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    // The painter lays out at the pane's width from the first
                    // frame and reads the pane's clip itself (windowed lists,
                    // Dialogs centred in what is visible).
                    .child(crate::scroll_pane::v_scroll_pane(
                        "exponential-ui-kitchen-sink-scroll",
                        &self.scroll,
                        div().w_full().child(self.view.clone()),
                    )),
            )
    }
}

/// VAPP-93: the site's specimens (`packages/exponential-ui/fixtures/specimens.json`):
/// one surface per core component plus the home demo, each painted alone by
/// [`ExponentialUiSpecimen`] for the ui.exponential.at desktop shots.
const SPECIMENS_JSON: &str =
    include_str!("../../../../../packages/exponential-ui/fixtures/specimens.json");

/// The specimens fixture's entries, `(id, node)`, parsed once.
fn specimens() -> &'static [(String, Value)] {
    static SPECIMENS: std::sync::OnceLock<Vec<(String, Value)>> = std::sync::OnceLock::new();
    SPECIMENS.get_or_init(|| {
        let doc: Value = serde_json::from_str(SPECIMENS_JSON).expect("the specimens fixture parses");
        doc.get("specimens")
            .and_then(Value::as_array)
            .map(|rows| {
                rows.iter()
                    .filter_map(|row| {
                        let id = row.get("id")?.as_str()?.to_string();
                        Some((id, row.get("node")?.clone()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// Whether `id` names an entry of the specimens fixture (an
/// `EXP_DEV_SCREEN` value that opens [`Screen::ExponentialUiSpecimen`]).
pub(crate) fn is_specimen_id(id: &str) -> bool {
    specimens().iter().any(|(entry, _)| entry == id)
}

/// The specimen's host: icons by concept like the kitchen sink, actions only
/// logged (a specimen is a still photograph).
struct SpecimenHost;

impl HostPlugin for SpecimenHost {
    fn icon(&self, name: &str) -> Option<SharedString> {
        crate::icons::registry::concept_by_name(name)
            .or_else(|| crate::icons::registry::icon_by_name(name))
            .map(|icon| icon.path())
    }

    fn on_action(&self, event: &ActionEvent, _cx: &mut App) {
        log::info!("[exponential-ui specimen] action {} ← {}", event.name, event.component_id);
    }

    fn on_input(&self, _event: &InputEvent, _cx: &mut App) {}

    fn on_unknown(&self, node: &PlacedNode) {
        log::warn!("[exponential-ui specimen] unknown component {} ({})", node.component, node.id);
    }
}

/// VAPP-93 (DEV-ONLY): ONE specimen of `fixtures/specimens.json` painted by
/// the gpui painter with no switcher chrome — just the specimen column at
/// the top-left of the content area (`EXP_DEV_SCREEN=<specimen id>`). Theme
/// and mode follow the kitchen sink's env overrides (default exponential,
/// dark).
pub struct ExponentialUiSpecimen {
    id: String,
    view: Entity<SurfaceView>,
    scroll: ScrollHandle,
}

impl ExponentialUiSpecimen {
    pub fn new(id: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let theme_id = parse_theme(env_trimmed("EXP_DEV_EXPONENTIAL_UI_THEME").as_deref());
        let mode = parse_mode(env_trimmed("EXP_DEV_EXPONENTIAL_UI_MODE").as_deref());
        let options = SurfaceViewOptions {
            surface_id: id.to_string(),
            theme: exponential_ui::themes::builtin_theme(theme_id),
            mode,
            host: Rc::new(SpecimenHost),
            ..SurfaceViewOptions::default()
        };
        let node = specimens().iter().find(|(entry, _)| entry == id).map(|(_, node)| node.clone());
        let tree: Option<NestedNode> = node.and_then(|node| match serde_json::from_value(node) {
            Ok(tree) => Some(tree),
            Err(err) => {
                log::warn!("[exponential-ui specimen] {id} does not parse: {err}");
                None
            }
        });
        let view = cx.new(|cx| {
            let mut view = SurfaceView::new(options, window, cx);
            if let Some(tree) = tree {
                let outcome = view.set_nested(tree, cx);
                for issue in &outcome.issues {
                    log::warn!("[exponential-ui specimen] {}: {}", issue.id, issue.message);
                }
            }
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        Self { id: id.to_string(), view, scroll: ScrollHandle::new() }
    }

    /// The specimen id this screen paints.
    pub fn id(&self) -> &str {
        &self.id
    }

}

impl Render for ExponentialUiSpecimen {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // The painter lays out at the pane's width from the first frame and
        // reads the pane's clip itself (round 1: no viewport probe).
        div()
            .id("exponential-ui-specimen")
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .child(crate::scroll_pane::v_scroll_pane(
                "exponential-ui-specimen-scroll",
                &self.scroll,
                div().w_full().child(self.view.clone()),
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fixture_parses_as_a_nested_node() {
        let tree: NestedNode = serde_json::from_str(KITCHEN_SINK_JSON).expect("parses");
        assert_eq!(tree.id, "root");
        assert_eq!(tree.component, "Box");
    }

    #[test]
    fn every_specimen_parses_as_a_nested_node() {
        // Every visible core component (81 since round 1) + the home demo.
        assert_eq!(specimens().len(), 82);
        for (id, node) in specimens() {
            let tree: Result<NestedNode, _> = serde_json::from_value(node.clone());
            assert!(tree.is_ok(), "{id}: {:?}", tree.err());
        }
        assert!(is_specimen_id("exponential-ui-button"));
        assert!(is_specimen_id("exponential-ui-demo"));
        assert!(!is_specimen_id("exponential-ui-kitchen-sink"));
    }

    #[test]
    fn env_picks_theme_and_mode_with_defaults() {
        assert_eq!(parse_theme(None), "exponential");
        assert_eq!(parse_theme(Some("playful")), "playful");
        assert_eq!(parse_theme(Some("nope")), "exponential");
        assert_eq!(parse_mode(Some("light")), Mode::Light);
        assert_eq!(parse_mode(None), Mode::Dark);
    }

    #[test]
    fn an_rtl_locale_unpins_the_kitchen_sinks_direction() {
        let pinned = |t: &NestedNode| serde_json::to_value(t).unwrap()["style"].get("direction").cloned();
        assert_eq!(pinned(&kitchen_sink_tree(None)), Some(serde_json::json!("ltr")));
        assert_eq!(pinned(&kitchen_sink_tree(Some("de-DE"))), Some(serde_json::json!("ltr")));
        assert_eq!(pinned(&kitchen_sink_tree(Some("ar"))), None);
    }

    #[test]
    fn no_settings_env_means_the_core_defaults() {
        if std::env::var_os("EXP_DEV_EXPONENTIAL_UI_LOCALE").is_none()
            && std::env::var_os("EXP_DEV_EXPONENTIAL_UI_DENSITY").is_none()
            && std::env::var_os("EXP_DEV_EXPONENTIAL_UI_CONTRAST").is_none()
            && std::env::var_os("EXP_DEV_EXPONENTIAL_UI_REDUCED_MOTION").is_none()
            && std::env::var_os("EXP_DEV_EXPONENTIAL_UI_MODE").is_none()
        {
            assert!(parse_settings(Mode::Dark).is_none());
        }
    }

    #[test]
    fn seed_matches_the_web_harness_unless_posts_are_asked_for() {
        let paths: Vec<&str> = seed_data(0).into_iter().map(|(p, _)| p).collect();
        assert_eq!(paths, vec!["/draft", "/ui"]);
        let with_posts = seed_data(30);
        assert_eq!(with_posts[2].0, "/posts");
        assert_eq!(with_posts[2].1.as_array().map(Vec::len), Some(30));
    }

    #[test]
    fn concept_icons_resolve_for_the_fixture_names() {
        for name in ["ui-send", "nav-inbox", "ui-close", "editor-bold", "nav-boards"] {
            assert!(
                crate::icons::registry::concept_by_name(name).is_some(),
                "{name} must resolve by concept"
            );
        }
        assert!(crate::icons::registry::concept_by_name("no-such-concept").is_none());
    }
}
