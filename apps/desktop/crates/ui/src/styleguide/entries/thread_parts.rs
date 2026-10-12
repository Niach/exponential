//! EXP-1284: `thread-parts` (Special components): the THREAD's pieces, the
//! IDE half of the styleguide entry — the REAL painters
//! (`crate::thread_screen`) over the contract model
//! (`domain::thread_view::build_thread_view`), fed the fixture's own cases
//! (`fixtures/thread-view.json`): draft 1 (two issues the agent linked, one
//! live run) and draft 2 (a resume, then two children of Run 2 in ONE
//! parallel card). Above them the title row and the child strip; below them
//! the thread list rows (Running / Recent) and the issue page's `Threads`
//! chip.

use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, App, Div, IntoElement, ParentElement as _,
    SharedString, Styled as _, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::thread_view::{
    build_thread_view, ThreadBlock, ThreadChild, ThreadView, ThreadViewIssue, ThreadViewItem,
    ThreadViewRun, ThreadViewThread,
};

use crate::icons::registry;
use crate::thread_screen::{
    block_header, folded_line, guide_dot, link_row, parallel_card, parallel_meta, parallel_row,
    run_mark, strip_chip, strip_divider, strip_issue_body, thread_row, title_row, ThreadRowSpec,
    CONVERSATION_LABEL, GUIDE_LABEL, SHOW_WORK_LABEL,
};

pub(crate) const ID: &str = "thread-parts";
pub(crate) const OWNER: &str = "EXP-1284";

const FIXTURE: &str =
    include_str!("../../../../../../../packages/domain-contract/fixtures/thread-view.json");

/// The fixture cases the demo draws, by index: draft 1, draft 2.
const CASES: [usize; 2] = [0, 1];

#[derive(serde::Deserialize)]
struct Input {
    thread: ThreadViewThread,
    items: Vec<ThreadViewItem>,
    issues: Vec<ThreadViewIssue>,
    runs: Vec<ThreadViewRun>,
}

fn cases() -> Vec<(ThreadView, Vec<ThreadViewRun>)> {
    let fixture: serde_json::Value = serde_json::from_str(FIXTURE).expect("thread-view.json parses");
    CASES
        .iter()
        .filter_map(|&index| serde_json::from_value::<Input>(fixture["views"][index]["input"].clone()).ok())
        .map(|input| {
            let view = build_thread_view(&input.thread, &input.items, &input.issues, &input.runs);
            (view, input.runs)
        })
        .collect()
}

/// The fixture's "now" — a little after its newest run started.
const NOW: &str = "2026-10-12T11:02:00.000Z";

fn strip(view: &ThreadView, case: usize, cx: &App) -> AnyElement {
    let mut chips: Vec<AnyElement> = Vec::new();
    for (index, child) in view.strip.iter().enumerate() {
        let id = SharedString::from(format!("sg-thread-strip-{case}-{index}"));
        chips.push(match child {
            ThreadChild::Conversation => strip_chip(id, true, cx)
                .child(Icon::new(registry::ACTION_CHAT).xsmall())
                .child(CONVERSATION_LABEL)
                .into_any_element(),
            ThreadChild::Issue { identifier, title, .. } => strip_chip(id, false, cx)
                .children(strip_issue_body(None, identifier, title, false, cx))
                .into_any_element(),
            ThreadChild::Add => h_flex()
                .gap_1()
                .items_center()
                .child(
                    Icon::new(registry::UI_ADD)
                        .xsmall()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(strip_divider(cx))
                .into_any_element(),
            ThreadChild::Guide { dot } => strip_chip(id, false, cx)
                .child(Icon::new(registry::NAV_REVIEWS).xsmall())
                .child(GUIDE_LABEL)
                .when(*dot, |chip| chip.child(guide_dot(cx)))
                .into_any_element(),
        });
    }
    h_flex()
        .w_full()
        .gap_1()
        .items_center()
        .children(chips)
        .child(div().flex_1())
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(SharedString::from(view.header_caption.clone())),
        )
        .into_any_element()
}

fn agent() -> Option<coding::CodingAgent> {
    Some(coding::CodingAgent::default())
}

fn link_chip(key: String, identifier: &str, title: &str) -> AnyElement {
    crate::issue_chip::issue_chip(key, identifier.to_string(), title.to_string())
        .max_title_width(px(220.))
        .into_any_element()
}

fn conversation(view: &ThreadView, runs: &[ThreadViewRun], case: usize, cx: &App) -> AnyElement {
    let now_ms = crate::run_rows::stamp_ms(Some(NOW)).unwrap_or(0);
    let mut column = v_flex().w_full().gap_4();
    for (index, block) in view.conversation.iter().enumerate() {
        let mut body = v_flex().w_full().gap_2();
        match block {
            ThreadBlock::Run(block) => {
                let busy = runs.iter().any(|run| run.id == block.run_id && run.agent_busy);
                body = body.child(block_header(
                    run_mark(agent(), block.state, busy),
                    &block.label,
                    &block.caption,
                    block.state,
                    cx,
                ));
                if block.folded {
                    let name = runs
                        .iter()
                        .find(|run| run.id == block.run_id)
                        .map(|run| run.name.clone())
                        .unwrap_or_default();
                    let toggle = crate::controls::text_button(
                        SharedString::from(format!("sg-thread-show-work-{case}-{index}")),
                        SHOW_WORK_LABEL,
                        crate::controls::TextButtonVariant::Text,
                        cx,
                    )
                    .into_any_element();
                    body = body.child(folded_line(&name, toggle, cx));
                }
            }
            ThreadBlock::Parallel(block) => {
                body = body.child(block_header(
                    run_mark(agent(), block.state, false),
                    &block.label,
                    &block.caption,
                    block.state,
                    cx,
                ));
                let rows = block
                    .runs
                    .iter()
                    .enumerate()
                    .map(|(row_ix, row)| {
                        let chip = row.issue_id.as_ref().and_then(|issue_id| {
                            view.strip.iter().find_map(|child| match child {
                                ThreadChild::Issue { issue_id: id, identifier, title } if id == issue_id => {
                                    Some(
                                        crate::issue_chip::issue_chip(
                                            SharedString::from(format!("sg-thread-par-chip-{case}-{row_ix}")),
                                            identifier.clone(),
                                            title.clone(),
                                        )
                                        .small()
                                        .into_any_element(),
                                    )
                                }
                                _ => None,
                            })
                        });
                        parallel_row(
                            SharedString::from(format!("sg-thread-par-{case}-{row_ix}")),
                            run_mark(agent(), row.state, false),
                            &row.name,
                            chip,
                            &parallel_meta(row.device_label.as_deref(), &row.started_at, now_ms),
                            row_ix == 0,
                            cx,
                        )
                        .into_any_element()
                    })
                    .collect();
                body = body.child(parallel_card(rows, cx));
            }
        }
        for link in block.links() {
            let mark = crate::coding_selects::agent_mark(coding::CodingAgent::default())
                .size(px(14.))
                .into_any_element();
            body = body.child(link_row(
                mark,
                &link.label,
                link_chip(
                    format!("sg-thread-link-{case}-{}", link.item_id),
                    &link.identifier,
                    &link.title,
                ),
                cx,
            ));
        }
        column = column.child(body);
    }
    column.into_any_element()
}

fn list_rows(views: &[(ThreadView, Vec<ThreadViewRun>)], cx: &App) -> AnyElement {
    let mut column = v_flex().w(px(300.)).gap_0p5();
    for (index, (view, _)) in views.iter().enumerate() {
        column = column.child(thread_row(
            ThreadRowSpec {
                id_prefix: "sg-thread-list",
                index,
                title: SharedString::from(view.title.clone()),
                caption: SharedString::from(view.list_caption.clone()),
                agent: coding::CodingAgent::default(),
                live: view.live,
                display: if view.needs_input {
                    Some(crate::queries::CodingSessionDisplay::NeedsInput)
                } else if view.busy {
                    Some(crate::queries::CodingSessionDisplay::Working)
                } else {
                    None
                },
                active: index == 0,
                glass: true,
                on_open: Box::new(|_, _, _| {}),
            },
            cx,
        ));
    }
    column.into_any_element()
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let views = cases();
    let mut page = v_flex().w(px(760.)).gap_8();
    for (case, (view, runs)) in views.iter().enumerate() {
        page = page.child(
            v_flex()
                .w_full()
                .gap_3()
                .child(title_row(&view.title, cx))
                .child(strip(view, case, cx))
                .child(div().w_full().h(px(1.)).bg(cx.theme().border))
                .child(conversation(view, runs, case, cx)),
        );
    }
    let chip = views.first().map(|(view, _)| {
        crate::thread_screen::issue_thread_chip(
            0,
            &crate::threads::IssueThreadChip {
                thread_id: "sg-thread".into(),
                title: SharedString::from(view.title.clone()),
                live: view.live,
                busy: view.busy,
                needs_input: view.needs_input,
                agent: coding::CodingAgent::default(),
            },
            cx,
        )
    });
    page.child(list_rows(&views, cx)).child(
        h_flex()
            .gap_1p5()
            .items_center()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(crate::thread_screen::THREADS_LABEL),
            )
            .children(chip),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn both_fixture_cases_build() {
        assert_eq!(super::cases().len(), 2);
    }
}
