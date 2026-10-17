//! `ask-dock` (Special components): THE ask dock (`fixtures/ask-dock.json`
//! ×4), the IDE half of the styleguide entry the web page draws: the
//! thread's REAL dock (`crate::thread_screen::conversation_dock`) in its
//! three states: one plan = THE big card in the composer's slot; two plans
//! and a question of three child sessions = the stack (oldest first, the
//! session name as caption, a click focuses one); the focused card, with
//! the back arrow leading its header.

use domain::session_tree::{QuestionOption, SessionThreadRow};
use gpui::{div, px, App, Div, InteractiveElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{v_flex, ActiveTheme as _};

use crate::ask_dock::AskDockState;
use crate::markdown::ImageCache;

pub(crate) const ID: &str = "ask-dock";
pub(crate) const OWNER: &str = "EXP-1341";

// The web `AskDockSpecimen` rows (packages/ui/src/ask-dock-specimen.tsx), line for line.
const PLAN_A: &str = "## Plan: one ask dock ×4\n\n1. Add `fixtures/ask-dock.json` + the TS model, mirrored ×4.\n2. Move waiting cards out of the transcript into the composer's slot.\n3. Stack several cards; a tap grows one into the big card.\n4. Share THE plan options: Yes · Yes, and start with a fresh context · No.\n5. Styleguide entry + scoped shots on every platform.\n6. Open the PR with the Guide.";
const PLAN_B: &str = "1. Group the thread list by state\n2. Drop the per-row counters";
const QUESTION: &str = "Ship both fixes in one PR, or split them?";

fn plan(id: &str, session: &str, text: &str) -> SessionThreadRow {
    SessionThreadRow::Plan {
        id: id.to_string(),
        session_id: session.to_string(),
        plan: text.to_string(),
        decision: None,
        fresh: false,
    }
}

fn question(id: &str, session: &str) -> SessionThreadRow {
    SessionThreadRow::Question {
        id: id.to_string(),
        session_id: session.to_string(),
        question: QUESTION.to_string(),
        options: ["One PR", "Split them"]
            .into_iter()
            .map(|label| QuestionOption { label: label.to_string(), description: None })
            .collect(),
        custom: true,
        answer: None,
        answered: false,
    }
}

/// The stack's rows: two plans and a question, three child sessions.
fn stack_rows(prefix: &str) -> Vec<SessionThreadRow> {
    vec![
        plan(&format!("{prefix}-p1"), "s-dock", PLAN_A),
        plan(&format!("{prefix}-p2"), "s-strip", PLAN_B),
        question(&format!("{prefix}-q1"), "s-polish"),
    ]
}

fn session_name(id: &str) -> Option<String> {
    match id {
        "s-dock" => Some("Ask dock".into()),
        "s-strip" => Some("Thread list".into()),
        "s-polish" => Some("Dock polish".into()),
        _ => None,
    }
}

fn caption(text: &'static str, cx: &App) -> Div {
    div().text_xs().text_color(cx.theme().muted_foreground).child(text)
}

fn demo(
    key: &'static str,
    label: &'static str,
    rows: Vec<SessionThreadRow>,
    state: AskDockState,
    images: &gpui::Entity<ImageCache>,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let state = window.use_keyed_state(SharedString::from(key), cx, move |_, _| state).clone();
    let dock = crate::thread_screen::conversation_dock(key, &rows, session_name, &state, &crate::thread_screen::ThreadText::default(), images, window, cx)
        .map(|(dock, _)| dock);
    v_flex()
        .w_full()
        .gap_2()
        .child(caption(label, cx))
        .child(div().id(SharedString::from(key)).w_full().children(dock))
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let images = window
        .use_keyed_state("sg-ask-dock-images", cx, |_, _| ImageCache::new(None))
        .clone();
    v_flex()
        .w(px(720.))
        .gap_10()
        .child(demo(
            "sg-ask-dock-single",
            "One waiting plan",
            vec![plan("sg-ask-dock-single-p1", "s-dock", PLAN_A)],
            AskDockState::default(),
            &images,
            window,
            cx,
        ))
        .child(demo(
            "sg-ask-dock-stack",
            "Two plans and a question",
            stack_rows("sg-ask-dock-stack"),
            AskDockState::default(),
            &images,
            window,
            cx,
        ))
        .child(demo(
            "sg-ask-dock-focused",
            "Focused out of the stack",
            stack_rows("sg-ask-dock-focused"),
            AskDockState::focused_on("sg-ask-dock-focused-p2"),
            &images,
            window,
            cx,
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::ask_dock::{ask_dock, thread_pending_asks, AskDockMode};

    /// The three demos are the dock's three states.
    #[test]
    fn the_demos_are_single_stack_and_focused() {
        let single = thread_pending_asks(&[plan("p1", "s-dock", PLAN_A)], session_name);
        assert_eq!(ask_dock(&single, None).mode, AskDockMode::Single);
        let rows = stack_rows("x");
        let pending = thread_pending_asks(&rows, session_name);
        let stack = ask_dock(&pending, None);
        assert_eq!(stack.mode, AskDockMode::Stack);
        assert_eq!(stack.rows.len(), 3);
        assert_eq!(stack.rows[0].title, "Plan: one ask dock ×4");
        assert_eq!(stack.rows[2].caption.as_deref(), Some("Dock polish"));
        let focused = ask_dock(&pending, Some("x-p2"));
        assert_eq!(focused.mode, AskDockMode::Focused);
        assert!(focused.back);
    }
}
