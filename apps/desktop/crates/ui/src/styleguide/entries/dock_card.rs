//! N-12 — `dock-card` (3 Special components): THE DockCard
//! (`fixtures/dock.json` ×4), the IDE half of the web entry
//! (`apps/styleguide/src/entries/dock-card.tsx`), drawn from the REAL
//! [`crate::dock`] parts: one card, a banner (its action trails, it never
//! opens), five cards piled (the front one whole, the rest peeking out
//! above it), the pile fanned, and the sheet's face at rest
//! ([`crate::dock::sheet_frame`]: the pinned header, the scrolling body,
//! the pinned footer).

use domain::dock::{
    dock_rows, DockQueue, DockQueueCompleted, DockQueueItem, DockQueueTodo, DockQueuedMessage, DockRow, DockSource,
    DockTint,
};
use gpui::{div, px, AnyElement, App, Div, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{button::{Button, ButtonVariants as _}, h_flex, v_flex, ActiveTheme as _, Sizable as _};

use crate::dock::{DockState, SheetContent};

pub(crate) const ID: &str = "dock-card";
pub(crate) const OWNER: &str = "N-12";

/// The web entry's sources, line for line.
pub(super) fn sources() -> Vec<DockSource> {
    vec![
        DockSource::Queued {
            messages: vec![
                DockQueuedMessage { id: "m1".into(), text: "also check the watchdog once the root is idle".into() },
                DockQueuedMessage { id: "m2".into(), text: "then open the PR".into() },
            ],
        },
        DockSource::Tasks {
            queue: Some(DockQueue {
                todo: DockQueueTodo {
                    title: "3 to do".into(),
                    items: vec![
                        DockQueueItem { label: "Measuring on the Galaxy A54".into(), description: Some("2m 35s".into()), state: "running".into() },
                        DockQueueItem { label: "Rerun the web captures".into(), description: None, state: "pending".into() },
                        DockQueueItem { label: "Open the PR with the numbers".into(), description: None, state: "pending".into() },
                    ],
                },
                completed: Some(DockQueueCompleted { title: "2 completed".into() }),
            }),
        },
        DockSource::Suggestions {
            id: "m4".into(),
            label: "3 suggested tasks".into(),
            caption: Some("1 started".into()),
            title: "Lazy-load the markdown editor".into(),
            open: 2,
        },
        DockSource::Banner {
            id: "wall".into(),
            icon: "ui-usage".into(),
            label: "Usage limit".into(),
            text: "Claude's 5-hour window is used up until 14:00".into(),
            tone: DockTint::Warning,
            action: Some("Switch account".into()),
        },
        DockSource::Question {
            id: "q1".into(),
            text: "Ship both fixes in one PR, or split them?".into(),
            session_name: Some("Dock polish".into()),
            created_at: 30,
        },
    ]
}

fn switch_button(key: &str) -> AnyElement {
    Button::new(SharedString::from(format!("{key}-switch"))).outline().small().label("Switch account").into_any_element()
}

fn caption(text: &'static str, cx: &App) -> Div {
    div().text_xs().text_color(cx.theme().muted_foreground).child(text)
}

/// The pile at rest under `key` (`fanned` = every card, `stack.gap` apart).
pub(super) fn pile_specimen(key: &'static str, rows: &[DockRow], fanned: bool, window: &mut Window, cx: &mut App) -> AnyElement {
    let state = window
        .use_keyed_state(SharedString::from(key), cx, move |_, _| if fanned { DockState::fanned() } else { DockState::default() })
        .clone();
    let mode = match (rows.len(), fanned) {
        (0, _) => domain::dock::DockMode::None,
        (1, _) => domain::dock::DockMode::Single,
        (_, true) => domain::dock::DockMode::Fanned,
        _ => domain::dock::DockMode::Pile,
    };
    let mut actions = vec![("banner:wall".to_string(), switch_button(key))];
    crate::dock::pile(&state, &SharedString::from(key), rows, mode, &mut actions, window, cx)
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let rows = dock_rows(&sources());
    let find = |kind: domain::dock::DockKind| rows.iter().find(|row| row.kind == kind).cloned().expect("the kind's card");
    let question = find(domain::dock::DockKind::Question);
    let banner = find(domain::dock::DockKind::Banner);
    let sheet = crate::dock::sheet_frame(
        &question,
        SheetContent {
            meta: None,
            action: None,
            body: crate::dock::sheet_body(
                "sg-dock-card-sheet-body",
                v_flex()
                    .child(div().child("Ship both fixes in one PR, or split them?"))
                    .child(div().mt_3().text_xs().text_color(cx.theme().muted_foreground).child("Choose one")),
            ),
            footer: Some(crate::dock::sheet_footer(
                h_flex().w_full().justify_end().child(Button::new("sg-dock-card-answer").primary().small().label("Answer")),
            )),
        },
        300.,
        cx,
    );
    v_flex()
        .w(px(672.))
        .gap_3()
        .child(caption("One card: the label, the caption, the meta, the title, the chevron", cx))
        .child(crate::dock::dock_card(&question, 0., None, cx))
        .child(caption("A banner: never opens; its action word trails", cx))
        .child(crate::dock::dock_card(&banner, 0., Some(switch_button("sg-dock-card-banner")), cx))
        .child(caption("Five cards piled: the front one whole, the rest peeking out above it", cx))
        .child(pile_specimen("sg-dock-card-pile", &rows, false, window, cx))
        .child(caption("The pile fanned (hover or a tap): every card, 6px apart", cx))
        .child(pile_specimen("sg-dock-card-fan", &rows, true, window, cx))
        .child(caption("The sheet: the pinned header, the scrolling body, the pinned footer", cx))
        .child(sheet)
}
