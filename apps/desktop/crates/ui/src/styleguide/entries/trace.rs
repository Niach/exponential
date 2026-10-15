//! EXP-1324: `trace` (General components): THE Trace — the REAL painter
//! (`crate::trace`) over the contract models (`fixtures/trace.json`,
//! `fixtures/task-queue.json`): the transcript's tail as in the owner's
//! draft (reasoning, a command opened onto its Output, a subagent with its
//! badge, a failed test run, a running call), then THE Queue.

use gpui::{div, px, App, Div, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::Icon;
use steer::task_queue::{task_queue, TaskQueueInput, TaskQueueItemState};
use steer::trace::{
    parse_test_results, trace_failed_tests, trace_reasoning, trace_subagent, trace_tool, TraceStatus,
};

use crate::trace::{trace_collapsible, trace_icon, trace_line, trace_stack, TraceLine, TraceSize};

pub(crate) const ID: &str = "trace";
pub(crate) const OWNER: &str = "EXP-1324";

const QUEUE_FIXTURE: &str =
    include_str!("../../../../../../../packages/domain-contract/fixtures/task-queue.json");

/// The draft's command output, with its ANSI colours.
pub(crate) const GRADLE_OUTPUT: &str = "\u{1b}[2m> Task :app:startupProfile\u{1b}[0m\nPixel 6a     p50 740 ms   p90 812 ms\nGalaxy A54   p50 790 ms   p90 840 ms\n\u{1b}[32mBUILD SUCCESSFUL in 41s\u{1b}[0m";

const TEST_OUTPUT: &str = "PASS src/a.test.ts\nFAIL src/bench/StartupBenchmark.test.ts\n\nTests:       1 failed, 11 passed, 12 total";

fn noop(_: &mut Window, _: &mut App) {}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let line = |id: &'static str, model: &steer::trace::TraceLine| TraceLine::from_model(id, model);
    let output = crate::code_block::code_block(
        crate::code_block::CodeBlockProps {
            id: SharedString::from("styleguide-trace-output"),
            title: Some(SharedString::from(steer::trace::copy::OUTPUT)),
            code: GRADLE_OUTPUT.to_string(),
            language: "ansi",
            destructive: false,
            copyable: true,
        },
        cx,
    );
    let tests = trace_failed_tests(parse_test_results(Some(TEST_OUTPUT)).as_ref())
        .expect("the demo output has a failure");
    let transcript = trace_stack(
        vec![
            trace_collapsible(
                line("sg-trace-thought", &trace_reasoning(false, Some(5400))),
                false,
                noop,
                None,
                window,
                cx,
            ),
            trace_collapsible(
                line(
                    "sg-trace-gradle",
                    &trace_tool("execute", "Bash", Some("./gradlew :app:startupProfile"), true, false),
                ),
                true,
                noop,
                Some(output),
                window,
                cx,
            ),
            trace_line(
                line("sg-trace-subagent", &trace_subagent("Explore", false, false, Some(72_000)))
                    .badge(true),
                window,
                cx,
            ),
            trace_collapsible(line("sg-trace-tests", &tests), false, noop, None, window, cx),
            trace_line(
                line("sg-trace-running", &trace_tool("execute", "Bash", Some("bun test"), false, false)),
                window,
                cx,
            ),
            trace_line(
                line("sg-trace-read", &trace_tool("read", "Read", Some("src/startup.kt"), true, false)),
                window,
                cx,
            ),
            trace_line(
                line("sg-trace-thinking", &trace_reasoning(true, None)),
                window,
                cx,
            ),
        ],
        TraceSize::Sm,
    );
    // THE Queue: the fixture's draft case, "to do" open, "completed" folded.
    let fixture: serde_json::Value = serde_json::from_str(QUEUE_FIXTURE).expect("task-queue.json parses");
    let input: TaskQueueInput =
        serde_json::from_value(fixture["cases"][2]["input"].clone()).unwrap_or_default();
    let queue = task_queue(&input).expect("the draft case has a queue");
    let items: Vec<_> = queue
        .todo
        .items
        .iter()
        .map(|item| {
            trace_line(
                TraceLine::new(
                    SharedString::from(format!("sg-queue-{}", item.key)),
                    Icon::from(trace_icon(&item.icon)),
                    item.label.clone(),
                )
                .description(item.description.clone())
                .status(match item.state {
                    TaskQueueItemState::Running => Some(TraceStatus::Running),
                    _ => Some(TraceStatus::Pending),
                }),
                window,
                cx,
            )
        })
        .collect();
    let completed = queue.completed.as_ref().expect("two completed");
    let queue_block = trace_stack(
        vec![
            trace_collapsible(
                TraceLine::new("sg-queue-todo", Icon::from(trace_icon(&queue.todo.icon)), queue.todo.title.clone()),
                true,
                noop,
                Some(trace_stack(items, TraceSize::Sm).into_any_element()),
                window,
                cx,
            ),
            trace_collapsible(
                TraceLine::new("sg-queue-completed", Icon::from(trace_icon(&completed.icon)), completed.title.clone()),
                false,
                noop,
                None,
                window,
                cx,
            ),
        ],
        TraceSize::Sm,
    );
    // The default (text-sm) scale.
    let default_size = trace_stack(
        vec![
            trace_line(
                line("sg-trace-default-edit", &trace_tool("edit", "Edit", Some("src/startup.kt"), true, false))
                    .size(TraceSize::Default),
                window,
                cx,
            ),
            trace_line(
                line("sg-trace-default-failed", &trace_tool("execute", "Bash", Some("bun test"), true, true))
                    .size(TraceSize::Default),
                window,
                cx,
            ),
        ],
        TraceSize::Default,
    );
    div()
        .w(px(640.))
        .flex()
        .flex_col()
        .gap_6()
        .child(transcript)
        .child(queue_block)
        .child(default_size)
}
