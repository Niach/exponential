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
    /// finished update's note under Codex.
    Device,
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
    model: crate::coding_selects::ChoiceSelect,
    work_screen: crate::coding_selects::ChoiceSelect,
    block: gpui::Entity<crate::settings::readiness::ReadinessBlock>,
}

impl BlockShot {
    fn new(kind: BlockKind, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let settings = coding::Settings::default();
        Self {
            kind,
            model: crate::coding_selects::model_select(coding::CodingAgent::Claude, &settings.computer_use_model, window, cx),
            work_screen: crate::coding_selects::work_screen_select(&crate::styleguide::entries::work_screen::demo_displays(), None, window, cx),
            block: cx.new(|cx| crate::settings::readiness::ReadinessBlock::new("shot", window, cx)),
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
    block("readiness-device", BlockKind::Device);
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
            BlockKind::Device => {
                let mut props = BlockProps {
                    on_action: Some(noop_action),
                    on_toggle: Some(noop_toggle),
                    computer_use_tail: crate::device_settings::computer_use_rows(&self.model, &self.work_screen),
                    ..BlockProps::default()
                };
                props.notes.insert("codex".into(), "Updated to 0.156.1".into());
                show(Shape::Sections(&readiness::sections(&doctor_case(CASE_GIT_MISSING), false, Some(true))), props, cx);
            }
            BlockKind::DeviceBare => {
                let props = BlockProps {
                    on_toggle: Some(noop_toggle),
                    computer_use_tail: crate::device_settings::computer_use_rows(&self.model, &self.work_screen),
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
        div().w_full().child(self.block.clone())
    }
}
