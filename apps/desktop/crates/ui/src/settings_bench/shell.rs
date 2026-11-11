//! #EXP-1629: the settings SHELL in every state: the left nav (owner vs
//! member, the lit row on General, a board, an agent, the Assistant, New
//! agent, a sub-page, a device pane, a personal pane, MCP Activity, the
//! Boards/Agents/Apps groups with several entries, a long board name, no
//! team, a hovered row, the back row) and the detail column (the header
//! over a pane, wide and narrow, a skeleton).
use super::*;
use gpui::{App, Entity};
use crate::settings::shell::{pane_of, DetailShell, PaneBuild};
use crate::settings::{NavShot, SettingsNavPanel, SettingsSection};

/// The nav column at its default width (`DEFAULT_SETTINGS`), tall enough
/// for every group.
const NAV: (f32, f32) = (272., 1160.);

fn frames(window: gpui::AnyWindowHandle, cx: &mut gpui::HeadlessAppContext) {
    for _ in 0..6 {
        draw(window, cx);
        cx.run_until_parked();
    }
    std::thread::sleep(std::time::Duration::from_millis(600));
    for _ in 0..3 {
        draw(window, cx);
        cx.run_until_parked();
    }
}

fn mouse(window: gpui::AnyWindowHandle, cx: &mut gpui::HeadlessAppContext, event: gpui::PlatformInput) {
    cx.update_window(window, |_, window, cx| window.dispatch_event(event, cx)).expect("a pointer event");
}

/// Save `window`'s frame as `name` (and diff it against
/// `EXP_SETTINGS_BEFORE`'s).
fn save(name: &str, window: gpui::AnyWindowHandle, cx: &mut gpui::HeadlessAppContext) {
    let Some(dir) = std::env::var_os("EXP_SETTINGS_SHOTS") else { return };
    let shot = cx.capture_screenshot(window).expect("the frame");
    shot.save(std::path::Path::new(&dir).join(format!("{name}.png"))).expect("the PNG");
    if let Some(before) = std::env::var_os("EXP_SETTINGS_BEFORE") {
        let before = image::open(std::path::Path::new(&before).join(format!("{name}.png"))).expect("the earlier picture").to_rgba8();
        let after = image::RgbaImage::from_raw(shot.width(), shot.height(), shot.as_raw().to_vec()).expect("the frame's pixels");
        assert_eq!(before.dimensions(), after.dimensions());
        let mut diff = after.clone();
        let (mut count, mut bounds, mut worst) = (0u32, (u32::MAX, u32::MAX, 0u32, 0u32), 0u8);
        for (x, y, pixel) in diff.enumerate_pixels_mut() {
            let a = before.get_pixel(x, y);
            if a == pixel {
                pixel.0 = [pixel.0[0] / 3, pixel.0[1] / 3, pixel.0[2] / 3, 255];
                continue;
            }
            worst = worst.max((0..3).map(|c| a.0[c].abs_diff(pixel.0[c])).max().unwrap_or(0));
            count += 1;
            bounds = (bounds.0.min(x), bounds.1.min(y), bounds.2.max(x), bounds.3.max(y));
            pixel.0 = [255, 0, 0, 255];
        }
        eprintln!("{name}: {count} pixels differ (worst level {worst}), within {bounds:?}");
        diff.save(std::path::Path::new(&dir).join(format!("{name}.diff.png"))).expect("the diff");
    }
}

fn app_cx() -> gpui::HeadlessAppContext {
    let mut cx = headless();
    cx.update(|cx| {
        crate::init(cx);
        let store = sync::Store::open(cx, None, None);
        cx.set_global(store);
    });
    cx
}

// ---------------------------------------------------------------------------
// The nav
// ---------------------------------------------------------------------------

struct NavFrame {
    nav: Entity<SettingsNavPanel>,
}

impl Render for NavFrame {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "NavFrame").entered();
        div().size_full().bg(cx.theme().background).child(self.nav.clone())
    }
}

fn board(id: &str, name: &str, color: &str, icon: Option<&str>) -> domain::rows::Board {
    serde_json::from_value(serde_json::json!({"id": id, "team_id": "t1", "name": name, "color": color, "icon": icon})).expect("a board")
}

fn agent(id: &str, name: &str, icon: &str, color: &str, assistant: bool) -> domain::session_tree::AgentRow {
    domain::session_tree::AgentRow { agent_id: id.into(), team_id: "t1".into(), name: name.into(), icon: icon.into(), color: color.into(), busy: false, needs_you: false, assistant }
}

fn app(id: &str, name: &str, icon: Option<&str>, color: Option<&str>) -> crate::apps_view::AppEntry {
    crate::apps_view::AppEntry { id: id.into(), slug: id.into(), name: name.into(), icon: icon.map(Into::into), color: color.map(Into::into), head_version: 1, published_version: 1, row: None }
}

/// A team with three boards, the Assistant and two agents, two apps.
fn team(owner: bool, selected: SettingsSection) -> NavShot {
    NavShot {
        owner,
        team_id: Some("t1".into()),
        selected,
        boards: vec![board("b1", "Mobile App", "#22c55e", Some("smartphone")), board("b2", "Website", "#3b82f6", None), board("b3", "Infrastructure", "#f97316", Some("server"))],
        agents: vec![agent("a0", "Assistant", "bot", "#a1a1aa", true), agent("a1", "Release manager", "rocket", "#8b5cf6", false), agent("a2", "Triage", "bot", "#ef4444", false)],
        apps: vec![app("p1", "Standup", Some("calendar"), Some("#eab308")), app("p2", "Roadmap", None, None)],
    }
}

/// The nav showing `shot`, after the pointer moved to `hover`.
fn shoot_nav(name: &str, shot: NavShot, hover: Option<(f32, f32)>, fold: Option<&'static str>) {
    if std::env::var_os("EXP_SETTINGS_SHOTS").is_none() {
        return;
    }
    let mut cx = app_cx();
    let mut nav = None;
    let handle = cx
        .open_window(size(px(NAV.0), px(NAV.1)), |window: &mut Window, cx| {
            let panel = cx.new(|cx| SettingsNavPanel::new(window, cx));
            let view = cx.new(|_| NavFrame { nav: panel.clone() });
            nav = Some(panel);
            cx.new(|cx| gpui_component::Root::new(view, window, cx))
        })
        .expect("a headless window");
    let window: gpui::AnyWindowHandle = handle.into();
    let nav = nav.expect("the nav");
    cx.update(|cx| nav.update(cx, |nav, cx| nav.show(shot, cx)));
    if let Some(label) = fold {
        cx.update(|cx| nav.update(cx, |nav, cx| nav.fold_for_shots(label, cx)));
    }
    frames(window, &mut cx);
    if let Some((x, y)) = hover {
        let position = gpui::point(px(x), px(y));
        mouse(window, &mut cx, gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent { position, pressed_button: None, modifiers: Default::default() }));
        frames(window, &mut cx);
    }
    save(name, window, &mut cx);
    std::mem::forget(cx);
}

#[test]
#[ignore]
fn settings_shell_nav_owner_general() {
    shoot_nav("shell-nav-owner-general", team(true, SettingsSection::General), None, None);
}

#[test]
#[ignore]
fn settings_shell_nav_member() {
    // A member's General is gated: the first visible page (Members) lights.
    shoot_nav("shell-nav-member", team(false, SettingsSection::General), None, None);
}

#[test]
#[ignore]
fn settings_shell_nav_board() {
    shoot_nav("shell-nav-board", team(true, SettingsSection::Board("b2".into())), None, None);
}

#[test]
#[ignore]
fn settings_shell_nav_agent() {
    shoot_nav("shell-nav-agent", team(true, SettingsSection::Agent("a1".into())), None, None);
}

#[test]
#[ignore]
fn settings_shell_nav_assistant() {
    shoot_nav("shell-nav-assistant", team(false, SettingsSection::Agent("a0".into())), None, None);
}

#[test]
#[ignore]
fn settings_shell_nav_agents_index() {
    shoot_nav("shell-nav-agents-index", team(true, SettingsSection::Agents), None, None);
}

#[test]
#[ignore]
fn settings_shell_nav_new_agent() {
    shoot_nav("shell-nav-new-agent", team(true, SettingsSection::NewAgent), None, None);
}

#[test]
#[ignore]
fn settings_shell_nav_labels() {
    shoot_nav("shell-nav-labels", team(true, SettingsSection::Labels), None, None);
}

#[test]
#[ignore]
fn settings_shell_nav_issues_folded() {
    shoot_nav("shell-nav-issues-folded", team(true, SettingsSection::Issues), None, Some("Issues"));
}

#[test]
#[ignore]
fn settings_shell_nav_device() {
    shoot_nav("shell-nav-device", team(true, SettingsSection::Tools), None, None);
}

#[test]
#[ignore]
fn settings_shell_nav_personal() {
    shoot_nav("shell-nav-personal", team(false, SettingsSection::Notifications), None, None);
}

#[test]
#[ignore]
fn settings_shell_nav_mcp_activity() {
    shoot_nav("shell-nav-mcp-activity", team(true, SettingsSection::McpActivity), None, None);
}

#[test]
#[ignore]
fn settings_shell_nav_prompt() {
    shoot_nav("shell-nav-prompt", team(true, SettingsSection::Prompt("x".into())), None, None);
}

#[test]
#[ignore]
fn settings_shell_nav_long_board() {
    let mut shot = team(true, SettingsSection::Board("b9".into()));
    shot.boards.insert(1, board("b9", "Customer onboarding and the long-running migration programme", "#14b8a6", Some("rocket")));
    shoot_nav("shell-nav-long-board", shot, None, None);
}

#[test]
#[ignore]
fn settings_shell_nav_no_team() {
    let shot = NavShot { owner: false, team_id: None, selected: SettingsSection::Account, boards: vec![], agents: vec![], apps: vec![] };
    shoot_nav("shell-nav-no-team", shot, None, None);
}

#[test]
#[ignore]
fn settings_shell_nav_hover_row() {
    // The pointer on Members (owner, General lit).
    shoot_nav("shell-nav-hover-row", team(true, SettingsSection::General), Some((120., 128.)), None);
}

#[test]
#[ignore]
fn settings_shell_nav_hover_lit() {
    // The pointer on the lit General row.
    shoot_nav("shell-nav-hover-lit", team(true, SettingsSection::General), Some((120., 99.)), None);
}

#[test]
#[ignore]
fn settings_shell_nav_hover_back() {
    shoot_nav("shell-nav-hover-back", team(true, SettingsSection::General), Some((120., 24.)), None);
}

// ---------------------------------------------------------------------------
// The detail column
// ---------------------------------------------------------------------------

struct DetailFrame {
    shell: DetailShell,
    pane: PaneBuild,
}

impl Render for DetailFrame {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "DetailFrame").entered();
        let column = self.shell.render("settings-detail-shot".into(), self.pane.clone(), window, cx);
        div().size_full().bg(cx.theme().background).child(column)
    }
}

/// The column at `width` × 800 showing the pane `build` makes.
fn shoot_detail(name: &str, width: f32, build: impl FnOnce(&mut Window, &mut App) -> PaneBuild + 'static) {
    if std::env::var_os("EXP_SETTINGS_SHOTS").is_none() {
        return;
    }
    let mut cx = app_cx();
    let handle = cx
        .open_window(size(px(width), px(800.)), |window: &mut Window, cx| {
            let pane = build(window, cx);
            let shell = DetailShell::new(window, cx);
            let view = cx.new(|_| DetailFrame { shell, pane });
            cx.new(|cx| gpui_component::Root::new(view, window, cx))
        })
        .expect("a headless window");
    let window: gpui::AnyWindowHandle = handle.into();
    frames(window, &mut cx);
    save(name, window, &mut cx);
    std::mem::forget(cx);
}

fn sessions(window: &mut Window, cx: &mut App) -> PaneBuild {
    let pane = cx.new(|cx| SessionsPane::for_shots(window, cx));
    pane.update(cx, |pane, cx| pane.show(Some(30), cx));
    pane_of(&pane)
}

#[test]
#[ignore]
fn settings_shell_detail_sessions() {
    shoot_detail("shell-detail-sessions", 1100., sessions);
}

#[test]
#[ignore]
fn settings_shell_detail_narrow() {
    shoot_detail("shell-detail-narrow", 640., sessions);
}

#[test]
#[ignore]
fn settings_shell_detail_about() {
    shoot_detail("shell-detail-about", 1100., |window, cx| pane_of(&cx.new(|cx| AboutPane::for_shots(window, cx))));
}

#[test]
#[ignore]
fn settings_shell_detail_skeleton() {
    shoot_detail("shell-detail-skeleton", 1100., |_, _| std::rc::Rc::new(|_, cx| crate::settings::settings_skeleton("shot-skeleton", cx)));
}
