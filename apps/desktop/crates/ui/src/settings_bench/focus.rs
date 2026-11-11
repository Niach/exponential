//! A new page's name field holds the focus once drawn (`doc::focus_input`).
use super::*;

/// The node holding the focus once a freshly opened pane has drawn.
pub(super) fn focus_after_open<P: Render + 'static>(field: &str, build: impl FnOnce(&mut Window, &mut gpui::Context<P>) -> P + 'static, surface: impl Fn(&P) -> gpui::Entity<eui_gpui::view::SurfaceView>) -> (Option<u32>, Option<u32>) {
    let mut cx = headless();
    cx.update(|cx| {
        crate::init(cx);
        let store = sync::Store::open(cx, None, None);
        cx.set_global(store);
    });
    let mut frame = None;
    let handle = cx
        .open_window(size(px(PANE.0), px(PANE.1)), |window: &mut Window, cx| {
            let pane = cx.new(|cx| build(window, cx));
            let view = cx.new(|_| Frame { pane: pane.clone().into() });
            frame = Some(pane);
            cx.new(|cx| gpui_component::Root::new(view, window, cx))
        })
        .expect("a headless window");
    let window: gpui::AnyWindowHandle = handle.into();
    let pane = frame.expect("the pane");
    // Headless has no frame loop: each draw delivers the next frame's
    // callbacks by hand (`on_next_frame`, which the focus retries ride).
    for _ in 0..6 {
        draw(window, &mut cx);
        cx.update_window(window, |_, window, cx| window.simulate_next_frame(cx)).expect("the frame's callbacks");
        cx.run_until_parked();
    }
    cx.update(|cx| {
        let view = surface(pane.read(cx));
        let view = view.read(cx);
        (view.focused(), view.index_of(field))
    })
}

/// A NEW prompt's page opens with its name field focused (`doc::focus_input`
/// reaches the Input's `name.field` part, not the Input node).
#[test]
#[ignore]
fn a_new_prompt_page_focuses_its_name() {
    let (focused, field) = focus_after_open("name.field", |window, cx| PromptPage::new("team-focus".into(), None, window, cx), |page| page.surface());
    assert!(field.is_some(), "the page has a name field");
    assert_eq!(focused, field);
}

/// A NEW skill's page opens with its name field focused.
#[test]
#[ignore]
fn a_new_skill_page_focuses_its_name() {
    let (focused, field) = focus_after_open("name.field", |window, cx| SkillPage::new("team-focus".into(), None, window, cx), |page| page.surface());
    assert!(field.is_some(), "the page has a name field");
    assert_eq!(focused, field);
}
