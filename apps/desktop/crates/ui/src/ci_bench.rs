//! #EXP-2214: the CI page's tabs and a merge queue run page, drawn HEADLESS
//! with the real text system over their specimens (ci-page.json `scenario`,
//! ci-run-view.json's merge queue run): the PR's desktop pictures. Run with
//! `EXP_CI_SHOTS=<dir> cargo test -p ui --release -- --ignored ci_bench --nocapture`.
use gpui::{div, prelude::*, px, size, AnyView, Window};
use gpui_component::ActiveTheme as _;

use domain::ci_page::CiTab;

use crate::shell_bench::headless;
use crate::transcript_bench::draw;

struct Frame {
    page: AnyView,
}

impl Render for Frame {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div().size_full().bg(cx.theme().background).child(self.page.clone())
    }
}

fn shoot<P: Render + 'static>(name: &str, (w, h): (f32, f32), build: impl FnOnce(&mut Window, &mut gpui::Context<P>) -> P + 'static) {
    let Some(dir) = std::env::var_os("EXP_CI_SHOTS") else { return };
    let mut cx = headless();
    cx.update(|cx| {
        crate::init(cx);
        let store = sync::Store::open(cx, None, None);
        cx.set_global(store);
    });
    let handle = cx
        .open_window(size(px(w), px(h)), |window: &mut Window, cx| {
            let page = cx.new(|cx| build(window, cx));
            let view = cx.new(|_| Frame { page: page.into() });
            cx.new(|cx| gpui_component::Root::new(view, window, cx))
        })
        .expect("a headless window");
    let window: gpui::AnyWindowHandle = handle.into();
    for _ in 0..8 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    std::thread::sleep(std::time::Duration::from_millis(600));
    for _ in 0..3 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    let shot = cx.capture_screenshot(window).expect("the frame");
    shot.save(std::path::PathBuf::from(dir).join(format!("{name}.png"))).expect("the PNG");
    std::mem::forget(cx);
}

fn page(name: &str, tab: CiTab) {
    shoot(name, (1180., 1400.), move |window, cx| {
        let mut page = crate::ci_page::CiPageView::specimen(window, cx);
        page.set_tab(tab);
        page
    });
}

#[test]
#[ignore]
fn ci_bench_merge_queue() {
    page("desktop-ci-merge-queue", CiTab::Train);
}

#[test]
#[ignore]
fn ci_bench_pull_requests() {
    page("desktop-ci-pull-requests", CiTab::Prs);
}

#[test]
#[ignore]
fn ci_bench_releases() {
    page("desktop-ci-releases", CiTab::Releases);
}

#[test]
#[ignore]
fn ci_bench_merge_queue_run() {
    std::env::set_var("EXP_DEV_CI_RUN_SOURCE", "train");
    shoot("desktop-ci-run-merge-queue", (1180., 1000.), crate::ci_run::CiRunView::specimen);
}
