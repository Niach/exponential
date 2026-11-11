use super::*;

// ---------------------------------------------------------------------------
// #EXP-1770: the device readiness block in every host
// ---------------------------------------------------------------------------

/// A `device-doctor.json` case's block.
fn doctor_case(name: &str) -> coding::device_doctor::DeviceDoctor {
    let fixture: serde_json::Value = serde_json::from_str(include_str!("../../../../../../packages/domain-contract/fixtures/device-doctor.json")).expect("the fixture");
    let case = fixture["cases"].as_array().expect("cases").iter().find(|case| case["name"].as_str() == Some(name)).expect("the case");
    crate::device_readiness::parse(Some(&case["doctor"])).expect("a block")
}

const CASE_READY: &str = "ready: git + claude, codex missing, computer use off";
const CASE_TOO_OLD: &str = "claude too old, computer use on with one macOS permission missing";
const CASE_GIT_MISSING: &str = "git missing, claude signed out, codex ok, computer use on (no OS permissions here)";
const CASE_LOW_DISK: &str = "EXP-1375: low disk with two kept worktrees: a warning, never a blocker";

/// What a [`BlockShot`] draws.
#[derive(Clone, Copy)]
enum BlockKind {
    /// A fixture case's block (`local` = this device), its switch wired or
    /// not, `busy` rows spinning.
    Case { case: &'static str, local: bool, toggle: bool, busy: &'static [&'static str] },
    /// The device settings dialog: another device's block with the switch
    /// on, the Computer use model + Work screen rows under it and a
    /// finished update's note under Codex; `model` / `screen` the
    /// pickers' values (`None` = Auto).
    Device { model: &'static str, screen: Option<&'static str> },
    /// The device settings dialog for an older build (no doctor): the bare
    /// switch row and its tail.
    DeviceBare,
    /// The chat composer's row: the agent's blocker or the low-disk warning.
    Composer { case: &'static str, warning: bool },
    /// No report yet.
    Loading,
}

struct BlockShot {
    kind: BlockKind,
    tail: Vec<crate::device_readiness::TailPicker>,
    block: gpui::Entity<crate::settings::readiness::ReadinessBlock>,
    /// A node to press once laid out (a picker's trigger), frames to wait.
    press: Option<(&'static str, u8)>,
}

impl BlockShot {
    fn new(kind: BlockKind, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let settings = coding::Settings::default();
        let (model, screen) = match kind {
            BlockKind::Device { model, screen } => (model.to_string(), screen),
            _ => (settings.computer_use_model.clone(), None),
        };
        let screens = domain::work_screen::work_screen_options(&crate::coding_selects::demo_displays(), screen);
        Self {
            kind,
            tail: crate::device_settings::computer_use_tail(model, crate::coding_selects::work_screen_value(screen).to_string(), &screens),
            block: cx.new(|cx| crate::settings::readiness::ReadinessBlock::new("shot", window, cx)),
            press: None,
        }
    }
}

fn block(name: &str, kind: BlockKind) {
    shoot_pane(name, move |window, cx| BlockShot::new(kind, window, cx), |_, _| {});
}

#[test]
#[ignore]
fn settings_readiness_primary() {
    block("readiness-primary", BlockKind::Case { case: CASE_GIT_MISSING, local: true, toggle: true, busy: &[] });
}

#[test]
#[ignore]
fn settings_readiness_busy() {
    block("readiness-busy", BlockKind::Case { case: CASE_TOO_OLD, local: true, toggle: true, busy: &["claude"] });
}

#[test]
#[ignore]
fn settings_readiness_switch_read_only() {
    block("readiness-switch-read-only", BlockKind::Case { case: CASE_READY, local: false, toggle: false, busy: &[] });
}

#[test]
#[ignore]
fn settings_readiness_warning() {
    block("readiness-warning", BlockKind::Case { case: CASE_LOW_DISK, local: false, toggle: false, busy: &[] });
}

#[test]
#[ignore]
fn settings_readiness_loading() {
    block("readiness-loading", BlockKind::Loading);
}

#[test]
#[ignore]
fn settings_readiness_device() {
    block("readiness-device", BlockKind::Device { model: coding::settings::DEFAULT_COMPUTER_USE_MODEL, screen: None });
}

/// #EXP-1771: the pickers at other values (Opus, the second display).
#[test]
#[ignore]
fn settings_readiness_device_picked() {
    block("readiness-device-picked", BlockKind::Device { model: "opus", screen: Some("uuid-b") });
}

/// A device block picker's menu open: its trigger pressed by node id (a
/// headless click never opened the IDE Select's popup, so these have no
/// earlier picture).
fn device_menu(name: &str, trigger: &'static str) {
    let kind = BlockKind::Device { model: coding::settings::DEFAULT_COMPUTER_USE_MODEL, screen: Some("uuid-b") };
    shoot_pane_settled(name, move |window, cx| BlockShot { press: Some((trigger, 2)), ..BlockShot::new(kind, window, cx) }, |_, _| {}, Some(std::time::Duration::from_millis(1200)));
}

/// #EXP-1771: the Computer use model picker open.
#[test]
#[ignore]
fn settings_readiness_menu_model() {
    device_menu("readiness-menu-model", "readiness_computer_use_computer_use_model.trigger");
}

/// #EXP-1771: the Work screen picker open.
#[test]
#[ignore]
fn settings_readiness_menu_work_screen() {
    device_menu("readiness-menu-work-screen", "readiness_computer_use_work_screen.trigger");
}

#[test]
#[ignore]
fn settings_readiness_device_bare() {
    block("readiness-device-bare", BlockKind::DeviceBare);
}

#[test]
#[ignore]
fn settings_readiness_composer_blocker() {
    block("readiness-composer-blocker", BlockKind::Composer { case: CASE_TOO_OLD, warning: false });
}

#[test]
#[ignore]
fn settings_readiness_composer_warning() {
    block("readiness-composer-warning", BlockKind::Composer { case: CASE_LOW_DISK, warning: true });
}

impl Render for BlockShot {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        use crate::device_readiness::{self as readiness, BlockProps};
        use crate::settings::readiness::Shape;
        let noop_action: readiness::ActionHandler = std::rc::Rc::new(|_, _, _, _| {});
        let noop_toggle: readiness::ToggleHandler = std::rc::Rc::new(|_, _, _| {});
        let block = self.block.clone();
        let mut show = |shape: Shape, props: BlockProps, cx: &mut gpui::Context<Self>| block.update(cx, |block, cx| block.show(shape, props, cx));
        match self.kind {
            BlockKind::Case { case, local, toggle, busy } => {
                let props = BlockProps {
                    on_action: Some(noop_action),
                    on_toggle: toggle.then_some(noop_toggle),
                    busy: busy.iter().map(|key| key.to_string()).collect(),
                    ..BlockProps::default()
                };
                show(Shape::Sections(&readiness::sections(&doctor_case(case), local, None)), props, cx);
            }
            BlockKind::Device { .. } => {
                let mut props = BlockProps {
                    on_action: Some(noop_action),
                    on_toggle: Some(noop_toggle),
                    computer_use_tail: self.tail.clone(),
                    ..BlockProps::default()
                };
                props.notes.insert("codex".into(), "Updated to 0.156.1".into());
                show(Shape::Sections(&readiness::sections(&doctor_case(CASE_GIT_MISSING), false, Some(true))), props, cx);
            }
            BlockKind::DeviceBare => {
                let props = BlockProps {
                    on_toggle: Some(noop_toggle),
                    computer_use_tail: self.tail.clone(),
                    ..BlockProps::default()
                };
                show(Shape::Single(&readiness::bare_switch_row(true)), props, cx);
            }
            BlockKind::Composer { case, warning } => {
                let doctor = doctor_case(case);
                let row = if warning { readiness::warning_row(&doctor, true) } else { readiness::blocking_row(&doctor, coding::CodingAgent::Claude, true) }.expect("a row");
                show(Shape::Single(&row), crate::chat_screen::ChatScreenView::readiness_row_props(None, cx), cx);
                // The chat's notes column under the composer.
                return div().w_full().child(gpui_component::v_flex().w_full().min_w_0().gap_0p5().px_1().text_xs().child(self.block.clone()));
            }
            BlockKind::Loading => show(Shape::Loading, BlockProps::default(), cx),
        }
        let view = self.block.read(cx).view_for_shots();
        match &mut self.press {
            Some((trigger, 0)) => {
                let trigger = *trigger;
                self.press = None;
                view.update(cx, |view, cx| {
                    let ix = view.index_of(trigger).expect("the trigger");
                    view.fire(ix, "press", None, cx);
                });
            }
            Some((_, wait)) => {
                *wait -= 1;
                cx.notify();
            }
            // The headless clock stands still: run the popup's fade out.
            None if !view.read(cx).layers().is_empty() => view.update(cx, |view, cx| view.advance_clock(std::time::Duration::from_millis(500), cx)),
            None => {}
        }
        div().w_full().child(self.block.clone())
    }
}
