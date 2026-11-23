use std::cell::RefCell;
use std::rc::Rc;

use eui_core::theme::Mode;
use eui_gpui::runtime::{ExponentialHost, HostOptions};
use eui_gpui::transport::{HttpTransportOptions, JsonlStreamTransport, SseTransport, WireFormat};
use gpui::{div, prelude::*, px, size, App, Bounds, Context, Entity, WeakEntity, Window, WindowBounds, WindowOptions};
use serde_json::json;

struct Main {
    host: Entity<ExponentialHost>,
    /// The surfaces painted, top to bottom.
    surfaces: &'static [&'static str],
}

impl Render for Main {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The surfaces the agent (and the app) create, once they exist; you own the scroller.
        let host = self.host.read(cx);
        div()
            .id("main")
            .size_full()
            .overflow_y_scroll()
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .children(self.surfaces.iter().filter_map(|id| host.surface(id)))
    }
}

/// The prose is yours: the SDK hands it to `on_text` and draws none of it.
/// Here it goes into two surfaces the app owns (a Markdown each): the run
/// before the block above the agent's surface, the run after it below.
struct Prose {
    host: Option<WeakEntity<ExponentialHost>>,
    target: &'static str,
    run: String,
}

impl Prose {
    fn piece(&mut self, piece: &str, cx: &mut App) {
        let Some(host) = self.host.clone() else { return };
        let mut messages = Vec::new();
        if self.target.is_empty() {
            self.target = "intro";
            for surface_id in ["intro", "outro"] {
                messages.push(json!({"version": "1.0", "createSurface": {"surfaceId": surface_id, "catalogId": eui_core::CORE_CATALOG_ID,
                    "components": [{"id": "root", "component": "Markdown", "text": {"@path": "/text"}}], "dataModel": {"text": ""}}}));
            }
        }
        if piece == "\n\n" {
            // A block passed: the next run starts.
            self.target = "outro";
            self.run.clear();
        } else {
            self.run.push_str(piece);
            messages.push(json!({"version": "1.0", "updateDataModel": {"surfaceId": self.target, "path": "/text", "value": self.run}}));
        }
        // The hook runs inside the host's update: apply after it.
        cx.defer(move |cx| {
            let _ = host.update(cx, |host, cx| messages.iter().for_each(|m| drop(host.receive(m, cx))));
        });
    }
}

fn main() {
    // `exponential-ui-guide-gpui express`: the Express stream (a model's
    // prose + the UI it writes; guides/agent `GET /express` replays one).
    let express = std::env::args().nth(1).as_deref() == Some("express");
    gpui_platform::application().with_assets(gpui_component_assets::Assets).run(move |cx: &mut App| {
        gpui_component::init(cx);
        let theme = eui_core::themes::builtin_theme("exponential");
        let (host, surfaces): (Entity<ExponentialHost>, &'static [&'static str]) = if express {
            // `WireFormat::express`: the agent writes Express (prose around `<a2ui>` blocks).
            let transport = SseTransport::new(HttpTransportOptions::new("http://localhost:4300/express").post_url("http://localhost:4300/action").format(WireFormat::express("main")));
            let prose = Rc::new(RefCell::new(Prose { host: None, target: "", run: String::new() }));
            let on_text = prose.clone();
            let options = HostOptions {
                transport: Some(Box::new(transport)),
                theme,
                mode: Mode::Dark,
                on_text: Some(Rc::new(move |piece: &str, cx: &mut App| on_text.borrow_mut().piece(piece, cx))),
                ..Default::default()
            };
            let host = cx.new(|cx| ExponentialHost::new(options, cx));
            prose.borrow_mut().host = Some(host.downgrade());
            (host, &["intro", "main", "outro"])
        } else {
            // One host per app: A2UI messages in over a transport, actions back out.
            let transport = JsonlStreamTransport::new(HttpTransportOptions::new("http://localhost:4300/a2ui.jsonl").post_url("http://localhost:4300/action"));
            let options = HostOptions { transport: Some(Box::new(transport)), theme, mode: Mode::Dark, ..Default::default() };
            (cx.new(|cx| ExponentialHost::new(options, cx)), &["main"])
        };
        host.update(cx, |host, cx| host.connect(cx));
        // The Express answer's dashboard wants a wider window.
        let bounds = if express { size(px(900.0), px(960.0)) } else { size(px(540.0), px(640.0)) };
        let window = WindowOptions { window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, bounds, cx))), ..Default::default() };
        cx.open_window(window, |_, cx| cx.new(|cx| {
            cx.observe(&host, |_, _, cx| cx.notify()).detach();
            Main { host: host.clone(), surfaces }
        }))
        .expect("a window");
    });
}
