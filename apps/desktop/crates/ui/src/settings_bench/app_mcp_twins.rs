//! #EXP-1778: App detail's Accounts Select (`app_mcp::upstream_select`, an
//! outline sm Button opening the pointer menu) closed and open, and the
//! Webhooks rows' middle-truncated URL and shown-once secret
//! (the IDE twin's `mid_text`) at a narrow width, against genUI's `button` Select
//! and `truncate: "middle"` Text. Shoot `settings_app_mcp_ide`, then
//! `settings_app_mcp_eui` with `EXP_SETTINGS_BEFORE` at the first.
use super::*;
use gpui_component::Selectable as _;
use serde_json::json;

const URL: &str = "https://app.exponential.at/api/apps/hooks/0f6a3c1e-7d2b-4a8e-9c51-2be4d7a1f093/github-push";
const SECRET: &str = "whsec_9f8e7d6c5b4a39281706f5e4d3c2b1a0ffeeddccbbaa";
/// The narrow column the two texts sit in.
const NARROW: f32 = 220.;
/// The Select's width (`app_mcp`'s).
const SELECT_W: f32 = 144.;

/// The IDE's middle-truncated run, as App detail drew it before genUI's
/// `truncate: "middle"` (kept as this bench's reference).
struct MidText {
    text: String,
    /// `text_sm` (else `text_xs`).
    sm: bool,
    muted: bool,
}

fn mid_text(text: &MidText, cx: &gpui::App) -> gpui::AnyElement {
    let theme = cx.theme();
    let run = div()
        .font_family(theme.mono_font_family.clone())
        .text_color(if text.muted { theme.muted_foreground } else { theme.foreground })
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis_middle()
        .child(text.text.clone());
    let run = if text.sm { run.text_sm() } else { run.text_xs() };
    div().w_full().line_height(gpui::phi()).child(run).into_any_element()
}

fn options() -> Vec<(String, String)> {
    vec![("none".into(), "Not set up".into()), ("s1".into(), "GitHub".into()), ("s2".into(), "GitHub (work account)".into()), ("s3".into(), "Linear".into())]
}

/// The IDE twin; `open` draws the trigger's menu (`upstream_menu`) in
/// place where its popover hangs it (a headless frame never paints the
/// trigger's popover): under the trigger by the popover's offset (a
/// quarter rem, rounded to whole px as gpui's `Anchored` does), start-aligned, at the menu's 16 px rem, no row selected
/// (a pointer open).
pub(super) struct Ide {
    open: bool,
    menu: Option<gpui::Entity<gpui_component::menu::PopupMenu>>,
}

impl Ide {
    fn new(open: bool) -> Self {
        Self { open, menu: None }
    }
}

impl Render for Ide {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        use gpui::Focusable as _;
        let picker = domain::apps::UpstreamPicker { value: "s1".into(), options: options() };
        let menu = if self.open {
            Some(match self.menu.clone() {
                Some(menu) => menu,
                None => {
                    let p = picker.clone();
                    let menu = gpui_component::menu::PopupMenu::build(window, cx, move |menu, _, _| crate::settings::app_mcp::upstream_menu(menu, &p, std::rc::Rc::new(|_, _| {})));
                    menu.focus_handle(cx).focus(window, cx);
                    self.menu = Some(menu.clone());
                    menu
                }
            })
        } else {
            None
        };
        let offset = window.rem_size() * 0.25;
        gpui_component::v_flex()
            .relative()
            .gap(px(16.))
            .child(div().flex().child(if self.open {
                // Open: the trigger as its popover selects it.
                crate::settings::app_mcp::upstream_trigger("bench-upstream".into(), "github".into(), &picker, false, cx).selected(true).into_any_element()
            } else {
                crate::settings::app_mcp::upstream_select("bench-upstream".into(), "github".into(), picker, false, |_, _| {}, cx).into_any_element()
            }))
            .child(div().w(px(NARROW)).flex().flex_col().child(mid_text(&MidText { text: URL.into(), sm: false, muted: true }, cx)))
            .child(div().w(px(NARROW)).flex().flex_col().child(mid_text(&MidText { text: SECRET.into(), sm: true, muted: false }, cx)))
            .children(menu.map(|menu| div().absolute().left_0().top((px(32.) + offset).round()).child(crate::controls::with_pointer_layer(menu))))
    }
}

pub(super) struct Eui {
    view: gpui::Entity<eui_gpui::view::SurfaceView>,
}

impl Eui {
    fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let ink = crate::settings::doc_ink(cx);
        let binding = cx.new(|_| eui_gpui::binding::Binding::new("/pane", json!({}), |s: &serde_json::Value, _| s.clone()));
        let options: Vec<serde_json::Value> = options().into_iter().map(|(value, label)| json!({"label": label, "value": value})).collect();
        let tree = json!({"id": "root", "component": "Stack", "props": {"gap": "lg"}, "style": {"width": "100%"}, "children": [
            {"id": "row", "component": "Stack", "props": {"direction": "horizontal"}, "children": [
                {"id": "upstream", "component": "Select", "props": {"label": "github", "name": "upstream", "triggerVariant": "button", "value": "s1", "options": options}, "style": {"width": SELECT_W}},
            ]},
            {"id": "url_col", "component": "Stack", "style": {"width": NARROW}, "children": [
                {"id": "url", "component": "Text", "props": {"text": URL, "truncate": "middle"}, "style": {"fontFamily": "$type.family.mono", "fontSize": 10.5, "lineHeight": 16.99, "color": ink.3}},
            ]},
            {"id": "secret_col", "component": "Stack", "style": {"width": NARROW}, "children": [
                {"id": "secret", "component": "Text", "props": {"text": SECRET, "truncate": "middle"}, "style": {"fontFamily": "$type.family.mono", "fontSize": 12.25, "lineHeight": 19.82}},
            ]},
        ]});
        let view = crate::settings::doc_mount_lifted("bench-app-mcp", tree, &binding, window, cx);
        Self { view }
    }
}

impl Render for Eui {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        // The headless clock stands still: the popup's enter fade runs
        // out on the frames after the press.
        if self.view.read(cx).layers().iter().any(|layer| layer.owner == "upstream") {
            self.view.update(cx, |view, cx| view.advance_clock(std::time::Duration::from_millis(500), cx));
        }
        div().w_full().child(self.view.clone())
    }
}

/// The trigger's centre (the frame's 24 px padding).
const PRESS: (f32, f32) = (24. + SELECT_W / 2., 24. + 16.);

#[test]
#[ignore]
fn settings_app_mcp_ide() {
    shoot_pane("app-mcp", |_, _| Ide::new(false), |_, _| {});
    shoot_pane("app-mcp-open", |_, _| Ide::new(true), |_, _| {});
}

#[test]
#[ignore]
fn settings_app_mcp_eui() {
    shoot_pane("app-mcp", |window, cx| Eui::new(window, cx), |_, _| {});
    shoot_pane_clicked("app-mcp-open", |window, cx| Eui::new(window, cx), |_, _| {}, Some(std::time::Duration::from_millis(1200)), &[PRESS]);
}
