//! #EXP-1629: the settings panes drawn HEADLESS with the real text system:
//! the pictures a pane's move onto the native binding is judged by (the same
//! states before and after, pixel for pixel). Run with
//! `EXP_SETTINGS_SHOTS=<dir> cargo test -p ui --release -- --ignored settings_ --nocapture`.
use std::collections::HashMap;

use gpui::{div, prelude::*, px, size, AnyView, Window};
use gpui_component::ActiveTheme as _;

use api::notifications::EmailPrefs;

use crate::coding_flow::CodingHub;
use crate::settings::local_repos::{LocalReposPane, LocalShot};
use crate::settings::about::AboutPane;
use crate::settings::account::{AccountPane, AccountShot};
use crate::settings::sessions::SessionsPane;
use crate::settings::api_keys::{ApiKeysPane, ShotLoad as KeysLoad};
use crate::settings::widget::WidgetPane;
use crate::settings::all_boards::AllBoardsPane;
use crate::settings::sign_in_methods::SignInShot;
use crate::settings::notifications_prefs::{NotificationsPrefsPane, ShotLoad};
use crate::settings::storage::{ShotLoad as StorageLoad, StoragePane};
use crate::settings::voice_prefs::{VoicePrefsPane, VoiceShot};
use crate::settings::tools::{ToolsPane, ToolsShot};
use crate::settings::team_general::{GeneralPane, GeneralShot, PromptShot};
use crate::settings::board_detail::{BoardDetailPane, BoardShot};
use crate::settings::prompts::{PageShot as PromptPageShot, PromptPage, PromptsPane, PromptsShot};
use crate::settings::branch_row_shots::{EuiBranchRow, IdeBranchRow};
use crate::settings::account_row_shots::{EuiAccountRow, IdeAccountRow};
use crate::settings::branch_picker_shots::{EuiBranchPicker, IdeBranchPicker, PickerState};
use crate::settings::statuses::{DeleteStatusContent, StatusesPane, StatusesShot};
use crate::settings::skills::{ListShot as SkillsListShot, SkillPage, SkillsPane};
use crate::shell_bench::headless;
use crate::transcript_bench::draw;

/// The settings detail column's width at the default window.
const PANE: (f32, f32) = (760., 1240.);

struct Frame {
    pane: AnyView,
}

impl Render for Frame {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "Frame").entered();
        div().size_full().bg(cx.theme().background).p(px(24.)).child(self.pane.clone())
    }
}

/// One pane in one state: built, shown, drawn and saved (and diffed).
fn shoot_pane<P: Render + 'static>(name: &str, build: impl FnOnce(&mut Window, &mut gpui::Context<P>) -> P + 'static, show: impl FnOnce(&mut P, &mut gpui::Context<P>) + 'static) {
    shoot_pane_settled(name, build, show, None);
}

/// [`shoot_pane`], drawn again after `settle` of wall time (the
/// scrollbar's fade runs on the real clock, so a frame can only be taken
/// after it, never pinned inside it).
fn shoot_pane_settled<P: Render + 'static>(name: &str, build: impl FnOnce(&mut Window, &mut gpui::Context<P>) -> P + 'static, show: impl FnOnce(&mut P, &mut gpui::Context<P>) + 'static, settle: Option<std::time::Duration>) {
    shoot_pane_clicked(name, build, show, settle, &[]);
}

/// [`shoot_pane_settled`] after left clicks at `clicks` (window points),
/// each followed by frames and a short settle (a popup's fade).
fn shoot_pane_clicked<P: Render + 'static>(name: &str, build: impl FnOnce(&mut Window, &mut gpui::Context<P>) -> P + 'static, show: impl FnOnce(&mut P, &mut gpui::Context<P>) + 'static, settle: Option<std::time::Duration>, clicks: &[(f32, f32)]) {
    shoot_pane_acted(name, build, show, settle, clicks, |_: &mut P, _: &mut Window, _: &mut gpui::Context<P>| {});
}

/// [`shoot_pane_clicked`], then `act` on the pane (a press by node id).
fn shoot_pane_acted<P: Render + 'static>(name: &str, build: impl FnOnce(&mut Window, &mut gpui::Context<P>) -> P + 'static, show: impl FnOnce(&mut P, &mut gpui::Context<P>) + 'static, settle: Option<std::time::Duration>, clicks: &[(f32, f32)], act: impl FnOnce(&mut P, &mut Window, &mut gpui::Context<P>) + 'static) {
    let Some(dir) = std::env::var_os("EXP_SETTINGS_SHOTS") else { return };
    let mut cx = headless();
    cx.update(|cx| {
        crate::init(cx);
        let store = sync::Store::open(cx, None, None);
        cx.set_global(store);
        let hub = CodingHub::global(cx);
        hub.update(cx, |hub, _| hub.settings.os_notifications = true);
    });
    // Under gpui-component's Root, as in the app: its rem (the theme's font
    // size) and Inter are what the IDE's rows are measured in.
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
    let frame = frame.expect("the frame");
    cx.update(|cx| frame.update(cx, |pane, cx| show(pane, cx)));
    for _ in 0..6 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    for &(x, y) in clicks {
        let position = gpui::point(px(x), px(y));
        cx.update_window(window, |_, window, cx| {
            let moved = gpui::MouseMoveEvent { position, pressed_button: None, modifiers: Default::default() };
            window.dispatch_event(gpui::PlatformInput::MouseMove(moved), cx);
            let down = gpui::MouseDownEvent { button: gpui::MouseButton::Left, position, modifiers: Default::default(), click_count: 1, first_mouse: false };
            window.dispatch_event(gpui::PlatformInput::MouseDown(down), cx);
        })
        .expect("a press");
        draw(window, &mut cx);
        cx.run_until_parked();
        cx.update_window(window, |_, window, cx| {
            let up = gpui::MouseUpEvent { button: gpui::MouseButton::Left, position, modifiers: Default::default(), click_count: 1 };
            window.dispatch_event(gpui::PlatformInput::MouseUp(up), cx);
        })
        .expect("a release");
        for _ in 0..6 {
            draw(window, &mut cx);
            cx.run_until_parked();
        }
        std::thread::sleep(std::time::Duration::from_millis(600));
        for _ in 0..3 {
            draw(window, &mut cx);
            cx.run_until_parked();
        }
    }
    cx.update_window(window, |_, window, cx| frame.update(cx, |pane, cx| act(pane, window, cx))).expect("the act");
    for _ in 0..6 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    std::thread::sleep(std::time::Duration::from_millis(600));
    for _ in 0..3 {
        draw(window, &mut cx);
        cx.run_until_parked();
    }
    if let Some(settle) = settle {
        std::thread::sleep(settle);
        for _ in 0..3 {
            draw(window, &mut cx);
            cx.run_until_parked();
        }
    }
    let shot = cx.capture_screenshot(window).expect("the frame");
    shot.save(std::path::Path::new(&dir).join(format!("{name}.png"))).expect("the PNG");
    // `EXP_SETTINGS_BEFORE=<dir>`: the same state's earlier picture; the
    // differing pixels are counted and painted red over a dimmed copy.
    if let Some(before) = std::env::var_os("EXP_SETTINGS_BEFORE") {
        let before = image::open(std::path::Path::new(&before).join(format!("{name}.png"))).expect("the earlier picture").to_rgba8();
        let after = image::RgbaImage::from_raw(shot.width(), shot.height(), shot.as_raw().to_vec()).expect("the frame's pixels");
        assert_eq!(before.dimensions(), after.dimensions());
        let mut diff = after.clone();
        let (mut count, mut bounds) = (0u32, (u32::MAX, u32::MAX, 0u32, 0u32));
        for (x, y, pixel) in diff.enumerate_pixels_mut() {
            let a = before.get_pixel(x, y);
            if a == pixel {
                pixel.0 = [pixel.0[0] / 3, pixel.0[1] / 3, pixel.0[2] / 3, 255];
                continue;
            }
            count += 1;
            bounds = (bounds.0.min(x), bounds.1.min(y), bounds.2.max(x), bounds.3.max(y));
            pixel.0 = [255, 0, 0, 255];
        }
        eprintln!("{name}: {count} pixels differ, within {bounds:?}");
        diff.save(std::path::Path::new(&dir).join(format!("{name}.diff.png"))).expect("the diff");
    }
    std::mem::forget(cx);
}

/// A 96 px picture (a two-colour gradient), PNG-encoded.
fn picture() -> Vec<u8> {
    let image = image::RgbaImage::from_fn(96, 96, |x, y| image::Rgba([(x * 255 / 95) as u8, 96, (y * 255 / 95) as u8, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).expect("the PNG");
    bytes.into_inner()
}

// One file per pane (EXP-1629): a new pane's bench is a new file + its `mod` line.
mod about;
mod account;
mod agent_detail;
mod all_boards;
mod api_keys;
mod app_detail;
mod board_detail;
mod box_fields;
mod change_email;
mod field_variants;
mod focus;
mod labels;
mod local_repos;
mod mcp_servers;
mod members;
mod menu_icon_color;
mod new_agent;
mod notifications;
mod pending_pill;
mod prompts;
mod readiness;
mod repositories;
mod row_parity;
mod select_icon_color;
mod select_multi;
mod select_variants;
mod sessions;
mod skills;
mod spinner_parity;
mod statuses;
mod storage;
mod team_general;
mod tools;
mod voice;
mod widget;
