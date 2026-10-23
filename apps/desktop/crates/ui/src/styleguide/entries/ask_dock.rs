//! `ask-dock` (Special components): THE ask cards on THE Dock
//! (`fixtures/ask-dock.json` + `dock.json` ×4), the IDE half of the web
//! entry (`AskDockSpecimen`, packages/ui/src/ask-dock-specimen.tsx): the
//! REAL DockCard rows of a waiting plan and question in four states: one
//! waiting plan (a single card); a plan, a question, the suggested tasks,
//! THE Queue and the queued messages piled; the pile fanned; the plan's sheet (the REAL
//! [`crate::plan_card::plan_sheet`] in the sheet's static frame: "Question
//! 1 of 2" in the header, the question, Back / Next in the footer).

use domain::dock::{dock_rows, DockSource};
use domain::session_tree::{PlanQuestion, QuestionOption, SessionThreadRow};
use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::{v_flex, ActiveTheme as _};

use crate::markdown::ImageCache;

pub(crate) const ID: &str = "ask-dock";
pub(crate) const OWNER: &str = "EXP-1341";

// The web `AskDockSpecimen` rows, line for line.
const LONG_PLAN: &str = "## Plan: one Dock ×4\n\n1. Add `fixtures/dock.json` + the TS model, mirrored ×4.\n2. Float the waiting cards above the composer; the transcript is the one scroll.\n3. Pile several cards like the toasts; a tap opens one as a sheet.\n4. Share THE plan options: Yes · Yes, and start with a fresh context · No.\n5. Styleguide entry + scoped shots on every platform.\n6. Open the PR with the Guide.";

fn option(label: &str, description: Option<&str>) -> QuestionOption {
    QuestionOption { label: label.into(), description: description.map(str::to_string), attachment_id: None }
}

fn questions() -> Vec<PlanQuestion> {
    vec![
        PlanQuestion {
            id: "q-scope".into(),
            prompt: "Which cards move into the Dock first?".into(),
            multi: false,
            custom: true,
            options: vec![
                option("Plan and question cards", Some("The rest follows as its own issue")),
                option("Everything", Some("Plan, question, tasks, queued messages, banners")),
            ],
        },
        PlanQuestion {
            id: "q-eui".into(),
            prompt: "Should DockCard also go into Exponential UI?".into(),
            multi: false,
            custom: false,
            options: vec![option("Yes, as DockCard", None), option("Styleguide only for now", None)],
        },
    ]
}

/// The rich plan row the sheet draws.
pub(super) fn rich_plan(id: &str) -> SessionThreadRow {
    SessionThreadRow::Plan {
        id: id.to_string(),
        session_id: "a".into(),
        plan: LONG_PLAN.to_string(),
        questions: questions(),
        surface: None,
        decision: None,
        fresh: false,
        answers: None,
        feedback: None,
        surface_data: None,
    }
}

fn asks() -> Vec<DockSource> {
    vec![
        DockSource::Plan { id: "p1".into(), text: LONG_PLAN.into(), session_name: Some("Ask dock".into()), questions: 2, created_at: 10 },
        DockSource::Question {
            id: "q1".into(),
            text: "Ship both fixes in one PR, or split them?".into(),
            session_name: Some("Dock polish".into()),
            created_at: 30,
        },
    ]
}

fn case(label: &'static str, body: impl gpui::IntoElement, cx: &App) -> Div {
    v_flex()
        .w_full()
        .gap_1p5()
        .child(div().text_xs().text_color(cx.theme().muted_foreground).child(label))
        .child(body)
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let images = window.use_keyed_state("sg-ask-dock-images", cx, |_, _| ImageCache::new(None)).clone();
    let single = dock_rows(&asks()[..1]);
    let mut all: Vec<DockSource> = super::dock_card::sources().into_iter().take(3).collect();
    all.extend(asks());
    let rows = dock_rows(&all);
    // The sheet at rest on page 0, the plan (web `ThreadPlanSheet`, iOS and
    // Android draw the same page: Next in the footer, no step in the header).
    let plan_row = rich_plan("sg-ask-dock-sheet-plan");
    let sheet = crate::dock::sheet_host(|| {
        crate::thread_screen::ask_row_sheet(&plan_row, &crate::thread_screen::ThreadText::default(), &images, window, cx)
    })
    .map(|content| crate::dock::sheet_frame(&single[0], content, 420., cx));
    v_flex()
        .w(px(672.))
        .gap_6()
        .child(case("One waiting plan", crate::dock::dock_card(&single[0], 0., None, cx), cx))
        .child(case(
            "A plan, a question, the suggested tasks, the Queue and the queued messages, piled",
            super::dock_card::pile_specimen("sg-ask-dock-pile", &rows, false, window, cx),
            cx,
        ))
        .child(case("The pile fanned (hover or a tap)", super::dock_card::pile_specimen("sg-ask-dock-fan", &rows, true, window, cx), cx))
        .child(case("The plan's sheet", div().w_full().children(sheet), cx))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pile's rows are the web specimen's (`[...STATE, ...ASKS]`), in
    /// the Dock's order.
    #[test]
    fn the_pile_is_the_state_then_the_asks() {
        let mut all: Vec<DockSource> = super::super::dock_card::sources().into_iter().take(3).collect();
        all.extend(asks());
        let ids: Vec<String> = dock_rows(&all).into_iter().map(|row| row.id).collect();
        assert_eq!(ids, ["queued", "tasks", "m4", "p1", "q1"]);
        assert_eq!(dock_rows(&asks()[..1])[0].meta.as_deref(), Some("2 questions"));
    }
}
