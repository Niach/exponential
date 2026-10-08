use exponential_ui::theme::Mode;
use exponential_ui_gpui::runtime::{ExponentialHost, HostOptions};
use exponential_ui_gpui::transport::{HttpTransportOptions, JsonlStreamTransport};
use gpui::{div, prelude::*, px, size, App, Bounds, Context, Entity, Window, WindowBounds, WindowOptions};

struct Main {
    host: Entity<ExponentialHost>,
}

impl Render for Main {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The surface the agent creates, once it exists; you own the scroller.
        div().id("main").size_full().overflow_y_scroll().p(px(16.0)).children(self.host.read(cx).surface("main"))
    }
}

fn main() {
    gpui_platform::application().with_assets(gpui_component_assets::Assets).run(|cx: &mut App| {
        gpui_component::init(cx);
        // One host per app: A2UI messages in over a transport, actions back out.
        let transport = JsonlStreamTransport::new(HttpTransportOptions::new("http://localhost:4300/a2ui.jsonl").post_url("http://localhost:4300/action"));
        let options = HostOptions { transport: Some(Box::new(transport)), theme: exponential_ui::themes::builtin_theme("exponential"), mode: Mode::Dark, ..Default::default() };
        let host = cx.new(|cx| ExponentialHost::new(options, cx));
        host.update(cx, |host, cx| host.connect(cx));
        let window = WindowOptions { window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(540.0), px(640.0)), cx))), ..Default::default() };
        cx.open_window(window, |_, cx| cx.new(|cx| {
            cx.observe(&host, |_, _, cx| cx.notify()).detach();
            Main { host: host.clone() }
        }))
        .expect("a window");
    });
}
