//! #EXP-1588 — every desktop confirm through the REAL dialog shell
//! ([`crate::native_dialog::open_alert`] over [`crate::native_dialog::render_layer`]),
//! drawn HEADLESS under the platform's real text system and captured one
//! picture each: every `prompts.json` confirm, the hand-written ones and
//! Add device. Run with
//! `EXP_ALERT_SHOTS=/tmp/alerts cargo test -p ui -- --ignored alert_gallery --nocapture`.
use std::sync::Arc;

use gpui::{div, px, size, App, AppContext as _, HeadlessAppContext, IntoElement, ParentElement as _, Render, Styled as _, Window};

use crate::native_dialog::{self, AlertSpec};

const VIEWPORT: (f32, f32) = (900., 720.);

struct Root;

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "Root").entered();
        div()
            .size_full()
            .bg(gpui::rgb(0x808080))
            .children(native_dialog::render_layer(window, cx))
    }
}

type Opener = Box<dyn FnOnce(&mut Window, &mut App)>;

fn alert(spec: impl FnOnce() -> AlertSpec + 'static) -> Opener {
    Box::new(move |window, cx| native_dialog::open_alert(window, cx, spec()))
}

fn openers() -> Vec<(String, Opener)> {
    let mut out: Vec<(String, Opener)> = vec![
        ("add-device".into(), Box::new(|window, cx| crate::machines::open_add_server_dialog(window, cx))),
        ("chat-delete".into(), alert(|| crate::chat_actions::delete_alert(|_, _| true))),
        (
            "draft-leave".into(),
            alert(|| crate::issue_draft_screen::leave_alert(|_, _| true, |_, _| true, |_, _| true)),
        ),
        (
            "close-pr".into(),
            alert(|| {
                AlertSpec::new(
                    crate::issue_header::CLOSE_PR_TITLE,
                    crate::issue_header::CLOSE_PR_BODY,
                    crate::issue_header::CLOSE_PR_CONFIRM,
                )
                .ok_variant(gpui_component::button::ButtonVariant::Danger)
            }),
        ),
        (
            "meeting-consent".into(),
            alert(|| {
                use etalk::meeting::{who, AudioApp, DesktopAudio, DesktopAudioCapability};
                let apps = [AudioApp { id: "z".into(), name: "Zoom".into() }, AudioApp { id: "s".into(), name: "Slack".into() }];
                let who = who(DesktopAudio::Include, &apps, DesktopAudioCapability::PerApp);
                crate::meeting_mode::consent_alert(&who, "Ops", |_, _| true)
            }),
        ),
    ];
    for spec in domain::prompts::ALL {
        let title_key = spec.copy.iter().map(|(k, _)| *k).find(|k| k.starts_with("title")).unwrap();
        let body_key = spec.copy.iter().map(|(k, _)| *k).find(|k| k.starts_with("body"));
        let values: Vec<(&str, &str)> = spec.params.iter().map(|p| (*p, "Example")).collect();
        let prompt = spec.render(title_key, body_key, &values);
        if prompt.actions.iter().filter(|a| a.role != domain::prompts::Role::Cancel).count() != 1 {
            continue;
        }
        out.push((format!("prompt-{}", spec.id), alert(move || AlertSpec::from_prompt("Confirm", &prompt))));
    }
    out
}

#[test]
#[ignore]
fn alert_gallery() {
    let dir = std::env::var("EXP_ALERT_SHOTS").expect("EXP_ALERT_SHOTS = the output folder");
    let platform = gpui_platform::current_platform(true);
    let mut cx = HeadlessAppContext::with_platform(
        platform.text_system(),
        Arc::new(gpui_component_assets::Assets),
        gpui_platform::current_headless_renderer,
    );
    cx.update(|cx| {
        gpui_component::init(cx);
        theme::init(cx);
        native_dialog::init(cx);
        let store = sync::Store::open(cx, None, None);
        cx.set_global(store);
    });
    for (name, open) in openers() {
        let window = cx
            .open_window(size(px(VIEWPORT.0), px(VIEWPORT.1)), |_, cx| cx.new(|_| Root))
            .expect("a headless window");
        cx.update_window(window.into(), move |_, window, cx| open(window, cx)).expect("opens");
        for _ in 0..3 {
            cx.update_window(window.into(), |_, window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            })
            .expect("a frame draws");
        }
        let shot = cx.capture_screenshot(window.into()).expect("a capture");
        shot.save(format!("{dir}/{name}.png")).expect("saved");
        eprintln!("{name}");
    }
    // The headless app's leak check would fail the views still alive.
    std::mem::forget(cx);
}
