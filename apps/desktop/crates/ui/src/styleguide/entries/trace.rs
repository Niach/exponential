//! EXP-1324: `trace` (General components): THE Trace — the REAL painter
//! (`crate::trace`) over the contract models — the web island
//! (`apps/styleguide/src/entries/trace.tsx`) line for line, identical ×4:
//! the transcript's tail at sm (a running command, reasoning, a command
//! opened onto its Output, a subagent with its badge, a failed test run, a
//! pending line), THE Queue, then two lines at the default size.

use gpui::{div, px, App, Div, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::Icon;
use steer::task_queue::{task_queue, TaskQueueInput, TaskQueueItemState};
use steer::trace::{
    parse_test_results, trace_failed_tests, trace_reasoning, trace_subagent, trace_tool, TraceStatus,
};

use crate::trace::{trace_collapsible, trace_icon, trace_line, trace_stack, TraceLine, TraceSize};

pub(crate) const ID: &str = "trace";
pub(crate) const OWNER: &str = "EXP-1324";

const PROFILE_OUTPUT: &str = "> Task :app:startupProfile\nPixel 6a     p50 740 ms   p90 812 ms\nGalaxy A54   p50 790 ms   p90 840 ms\nBUILD SUCCESSFUL in 41s";

pub(crate) const BENCHMARK_OUTPUT: &str = "StartupBenchmark > coldStart FAILED\n    java.lang.AssertionError: p90 812 ms > 800 ms\n\n12 tests completed, 1 failed";

fn noop(_: &mut Window, _: &mut App) {}

fn output(id: &str, code: &str, destructive: bool, cx: &App) -> gpui::AnyElement {
    crate::code_block::code_block(
        crate::code_block::CodeBlockProps {
            id: SharedString::from(format!("styleguide-trace-{id}")),
            title: Some(SharedString::from(steer::trace::copy::OUTPUT)),
            code: code.to_string(),
            language: "ansi",
            destructive,
            copyable: true,
        },
        cx,
    )
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let line = |id: &'static str, model: &steer::trace::TraceLine| TraceLine::from_model(id, model);
    let gradle = Some("./gradlew :app:startupProfile");
    let tests = trace_failed_tests(parse_test_results(Some(BENCHMARK_OUTPUT)).as_ref())
        .expect("the demo output has a failure");
    let transcript = trace_stack(
        vec![
            trace_line(line("sg-trace-running", &trace_tool("execute", "Bash", gradle, false, false)), window, cx),
            trace_line(line("sg-trace-thought", &trace_reasoning(false, Some(5200))), window, cx),
            trace_collapsible(
                line("sg-trace-gradle", &trace_tool("execute", "Bash", gradle, true, false)),
                true,
                noop,
                Some(output("profile", PROFILE_OUTPUT, false, cx)),
                window,
                cx,
            ),
            trace_line(
                line("sg-trace-subagent", &trace_subagent("Explore", false, false, Some(72_000)))
                    .badge(true)
                    .chevron(true),
                window,
                cx,
            ),
            trace_collapsible(line("sg-trace-tests", &tests), false, noop, None, window, cx),
            trace_line(
                line("sg-trace-read", &trace_tool("read", "Read", Some("README.md"), true, false))
                    .status(Some(TraceStatus::Pending)),
                window,
                cx,
            ),
        ],
        TraceSize::Sm,
    );
    // THE Queue: "to do" open, "completed" folded.
    let input: TaskQueueInput = serde_json::from_value(serde_json::json!({
        "tasks": [
            { "content": "Profile the cold start", "status": "completed" },
            { "content": "Defer the snapshot cache warm-up", "status": "completed" },
            { "content": "Measuring on the Galaxy A54", "status": "in_progress" },
            { "content": "Open the PR with the numbers", "status": "pending" },
        ],
        "backgroundTasks": [
            { "id": "b1", "kind": "shell", "description": "Rerun the web and web-phone captures", "startedAt": 0 },
        ],
        "waits": [],
        "now": 130_000,
    }))
    .expect("the specimen queue parses");
    let queue = task_queue(&input).expect("the specimen has a queue");
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
                line("sg-trace-default-grep", &trace_tool("search", "Grep", Some("persistOnStop"), true, false))
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
        .w(px(672.))
        .flex()
        .flex_col()
        .gap_6()
        .child(transcript)
        .child(queue_block)
        .child(default_size)
}
