//! EXP-1175: `run-status-row` (Special components): the Run face's status
//! row, the IDE half of the styleguide entry the web page draws.
//!
//! The REAL row (`crate::run_rows::run_status_row`) over the shared rules'
//! own output (`run_row_caption`, `show_work_label`) in three states: a
//! working run with its newest tool call muted under the caption, a run
//! waiting on its owner, and an ended one (the dimmed mark, Hide work: the
//! transcript is showing). EXP-1245: plus a SETTLED turn of the owner's
//! thread (`turn_row_caption`: `Done on <device> · <turn time>`).

use gpui::{div, px, App, Div, ParentElement as _, SharedString, Styled as _, Window};

use crate::run_rows::{
    run_row_caption, run_status_row, stamp_ms, RunRowState, RunStatusMark, RunStatusRowSpec,
};

pub(crate) const ID: &str = "run-status-row";
pub(crate) const OWNER: &str = "EXP-1175";

/// The three demo rows: (state, mark, tool line, show work).
fn states() -> Vec<(RunRowState, RunStatusMark, Option<&'static str>, bool)> {
    use crate::queries::CodingSessionDisplay as D;
    vec![
        (
            RunRowState::Working,
            RunStatusMark::Live(Some(D::Working)),
            Some("Read apps/web/src/lib/work-faces.ts"),
            false,
        ),
        (RunRowState::NeedsInput, RunStatusMark::Live(Some(D::NeedsInput)), None, false),
        (RunRowState::Ended, RunStatusMark::Ended, None, true),
    ]
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let started = stamp_ms(Some("2026-10-06T10:00:00Z"));
    let now = stamp_ms(Some("2026-10-06T10:12:04Z")).unwrap_or_default();
    let ended = stamp_ms(Some("2026-10-06T10:45:30Z"));
    let rows = states()
        .into_iter()
        .enumerate()
        .map(|(index, (state, mark, tool_line, show_work))| {
            let (caption, tone) = run_row_caption(state, "MacBook Pro", started, ended, now);
            run_status_row(
                RunStatusRowSpec {
                    id: SharedString::from(format!("styleguide-run-status-row-{index}")),
                    agent: Some(coding::CodingAgent::default()),
                    mark,
                    caption: SharedString::from(caption),
                    tone,
                    tool_line: tool_line.map(SharedString::from),
                    show_work,
                    on_toggle: None,
                },
                cx,
            )
        });
    let settled = crate::session_results::turn_row_caption(
        stamp_ms(Some("2026-10-06T21:16:00Z")),
        stamp_ms(Some("2026-10-06T21:23:17Z")),
        RunRowState::Working,
        "macbook",
        None,
        now,
        true,
    )
    .map(|(caption, tone)| {
        run_status_row(
            RunStatusRowSpec {
                id: SharedString::from("styleguide-run-status-row-settled-turn"),
                agent: Some(coding::CodingAgent::default()),
                mark: RunStatusMark::Ended,
                caption: SharedString::from(caption),
                tone,
                tool_line: None,
                show_work: false,
                on_toggle: None,
            },
            cx,
        )
    });
    div().flex().flex_col().gap_3().pt_2().w(px(560.)).children(rows).children(settled)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The demo reads the shared rules: the working caption ticks off the
    /// start, the ended one measures start to end.
    #[test]
    fn the_demo_reads_the_shared_captions() {
        let started = stamp_ms(Some("2026-10-06T10:00:00Z"));
        let now = stamp_ms(Some("2026-10-06T10:12:04Z")).unwrap_or_default();
        let ended = stamp_ms(Some("2026-10-06T10:45:30Z"));
        let captions: Vec<String> = states()
            .into_iter()
            .map(|(state, ..)| run_row_caption(state, "MacBook Pro", started, ended, now).0)
            .collect();
        assert_eq!(
            captions,
            vec![
                "Building on MacBook Pro · 12m 04s",
                "Needs input · MacBook Pro",
                "Ended on MacBook Pro · 45m 30s",
            ]
        );
    }
}
