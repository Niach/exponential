//! NEW-59 — `work-screen` (3 Special components): the Work screen picker
//! (`fixtures/work-screen.json`), the twin of the web entry. The computer-use
//! card of device settings as the dialog draws it (the readiness block's
//! Computer use band: the switch on, the Computer use model row, the Work
//! screen row under it, both rows the dialog's own
//! `device_settings::computer_use_rows`), then the Work screen picker's
//! options drawn OPEN: Auto · Display 1 · 2560 × 1440 · Display 2 · 3024 ×
//! 1964 · Primary (`domain::work_screen::work_screen_options`), Auto picked.

use std::rc::Rc;

use gpui::{div, px, App, Context, Div, IntoElement, ParentElement as _, Render, Styled as _, Window};
use gpui_component::v_flex;

use coding::device_doctor::DeviceDoctor;
use coding::CodingAgent;
use domain::work_screen::{work_screen_options, WorkScreenDisplay};

use super::composer_dialog::OpenMenu;
use crate::coding_selects::{model_select, work_screen_select, ChoiceSelect};
use crate::device_readiness;

pub(crate) const ID: &str = "work-screen";
pub(crate) const OWNER: &str = "NEW-59";

/// The demo machine's two displays (the web entry's): a 1440p external, the
/// built-in Retina as the primary, each with its stable id (#FEED-161).
pub(crate) fn demo_displays() -> Vec<WorkScreenDisplay> {
    vec![
        WorkScreenDisplay { id: "uuid-a".into(), number: 1, width: 2560, height: 1440, primary: false },
        WorkScreenDisplay { id: "uuid-b".into(), number: 2, width: 3024, height: 1964, primary: true },
    ]
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let card = window.use_keyed_state("sg-work-screen-card", cx, WorkScreenCard::new);
    let options = window.use_keyed_state("sg-work-screen-options", cx, |window, cx| {
        OpenMenu::new(window, cx, |mut menu, _, _| {
            menu = menu.min_w(px(280.));
            for option in work_screen_options(&demo_displays(), None) {
                menu = menu.item(crate::controls::pointer_check_item(
                    option.label,
                    option.value.is_none(),
                    |_, _| {},
                ));
            }
            menu
        })
    });
    v_flex()
        .w(px(520.))
        .gap_3()
        .child(card)
        .child(div().flex().justify_end().child(options))
}

/// The computer-use card: the doctor's "computer use on" case with the
/// dialog's tail rows.
struct WorkScreenCard {
    doctor: Option<DeviceDoctor>,
    computer_use: bool,
    model: ChoiceSelect,
    work_screen: ChoiceSelect,
}

impl WorkScreenCard {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = coding::Settings::default();
        Self {
            doctor: super::device_settings::computer_use_on_doctor(),
            computer_use: true,
            model: model_select(CodingAgent::Claude, &settings.computer_use_model, window, cx),
            work_screen: work_screen_select(&demo_displays(), None, window, cx),
        }
    }
}

impl Render for WorkScreenCard {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(doctor) = self.doctor.as_ref() else {
            return div();
        };
        let view = cx.entity().downgrade();
        let props = device_readiness::BlockProps {
            id: "sg-work-screen-readiness".into(),
            on_toggle: Some(Rc::new(move |on, _window, cx| {
                let _ = view.update(cx, |this, cx| {
                    this.computer_use = on;
                    cx.notify();
                });
            })),
            computer_use_tail: if self.computer_use {
                crate::device_settings::computer_use_rows(&self.model, &self.work_screen, cx)
            } else {
                Vec::new()
            },
            ..device_readiness::BlockProps::default()
        };
        let sections: Vec<_> = device_readiness::sections(doctor, false, Some(self.computer_use))
            .into_iter()
            .filter(|section| section.group == coding::device_doctor::DoctorGroup::ComputerUse)
            .collect();
        device_readiness::render_sections(&sections, props, cx)
    }
}
