//! NEW-2: `schedule-picker` (General components): THE schedule picker
//! ×4 (`fixtures/schedule-picker.json`) — the REAL
//! [`crate::schedule_fields::ScheduleFieldsState`] rows in ONE glass group,
//! seeded with the web specimen's draft: weekly Monday–Friday at 09:00 in
//! Europe/Vienna (Repeat · Days · Time · Time zone). Kept alive by
//! `window.use_keyed_state`; every pick is live.

use gpui::{div, px, App, Context, Div, IntoElement, ParentElement as _, Render, Styled as _, Window};

use crate::schedule_fields::ScheduleFieldsState;
use crate::surface;

pub(crate) const ID: &str = "schedule-picker";
pub(crate) const OWNER: &str = "NEW-2";

/// The specimen's stored trigger: weekdays at 09:00, Vienna.
const CRON: &str = "0 9 * * 1-5";
const TZ: &str = "Europe/Vienna";

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let demo = window.use_keyed_state("sg-schedule-picker", cx, SchedulePickerDemo::new);
    // The web island's `w-[24rem]`.
    div().w(px(384.)).child(demo)
}

struct SchedulePickerDemo {
    schedule: ScheduleFieldsState,
}

impl SchedulePickerDemo {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut schedule = ScheduleFieldsState::new(window, cx);
        schedule.seed(CRON, TZ, window, cx);
        Self { schedule }
    }
}

impl Render for SchedulePickerDemo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        surface::glass_group_rows(self.schedule.render_rows(
            "sg-schedule",
            |view: &mut Self| &mut view.schedule,
            window,
            cx,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::schedule_picker::{schedule_from_cron, schedule_summary, ScheduleRepeat};

    /// The specimen reads as a weekly Monday–Friday at 09:00 (no Cron row).
    #[test]
    fn the_specimen_is_weekdays_at_nine_in_vienna() {
        let schedule = schedule_from_cron(CRON).expect("the rows express it");
        assert_eq!(schedule.repeat, ScheduleRepeat::Weekly);
        assert_eq!(schedule.weekdays, [1, 2, 3, 4, 5]);
        assert_eq!(schedule_summary(CRON, Some(TZ)), "Weekdays at 09:00 (Europe/Vienna)");
    }

    /// The real rows paint in a window and settle (the zone list, the days
    /// multi picker and the time menus all mount).
    #[gpui::test]
    async fn the_entry_paints_and_settles(cx: &mut gpui::TestAppContext) {
        struct Host;

        impl Render for Host {
            fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                div().child(render(window, cx))
            }
        }

        cx.update(|cx| {
            gpui_component::init(cx);
            theme::init(cx);
        });
        let (_view, cx) = cx.add_window_view(|_, _| Host);
        for _ in 0..3 {
            cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.run_until_parked();
        }
    }
}
