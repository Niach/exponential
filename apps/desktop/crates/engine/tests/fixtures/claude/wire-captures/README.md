# Raw stream-json captures (claude 2.1.269, 2026-09-12)

Verbatim `claude -p --output-format stream-json --verbose` output with the
`stream_event` partials stripped. Ground truth for the EXP-850 / EXP-856
engine work; scenario dirs for `fake-claude.sh` are cut from these.

- `workflow.jsonl` — one `Workflow` tool call: `system/task_started`
  (`task_type: local_workflow`, `workflow_name`, the script as `prompt`),
  nine `system/task_progress` frames carrying the `workflow_progress`
  array (`workflow_phase` + `workflow_agent` entries, latest state per
  `type:index`), `background_tasks_changed`, `task_updated`,
  `task_notification` (completed). Script `log()` lines never reach the
  wire in this build; the final `workflowProgress` is also inside the
  TaskOutput tool result, not the Workflow tool result.
- `background-tasks.jsonl` — `Bash` with `run_in_background: true`
  (`task_started` `task_type: local_bash`, `is_backgrounded`), a blocking
  `TaskOutput`, a `Monitor` (also `local_bash`), each bracketed by
  `background_tasks_changed` and closed by `task_notification`.
- `duplicate-agent.jsonl` — the EXP-856 reproduction: a workflow agent
  (`slowpoke`) messages main; main replies with `SendMessage` to the live
  agent id; the tool result says `Resuming agent` and a SECOND
  `system/task_started` (`task_type: local_agent`, `task_id` == the agent
  id, `description` == the workflow label, `prompt: "pong"`) starts while
  the original still runs inside the workflow. Note the original workflow
  agent never emitted a `task_started` of its own in this run.
- `workflow-agents-2.1.286.jsonl` (claude 2.1.286, 2026-10-06, verbatim) —
  EXP-1224 / EXP-1225: a two-agent `Workflow`, then one background `Agent`
  (`run_in_background`) whose notification is waited for. Workflow agents
  NEVER get a `task_started` and never stream with `parent_tool_use_id`:
  they exist only as `workflow_agent` entries in `task_progress`, their
  `state` going `start` (queued until `startedAt` appears) → `progress` →
  `done` | `error`, with `agentId` (absent while queued), `lastToolName`,
  `lastToolSummary`, `toolCalls`, `tokens`, `resultPreview`. After each
  `result` the CLI, with a background-task notification pending, CONTINUES
  ON ITS OWN: it re-announces `system/init` (same `session_id`) and streams
  the next turn up to another `result`, with no prompt from us; only the
  last one is followed by `session_state_changed: idle`.

Scenario dirs cut from these (EXP-850 / EXP-856, `tests/claude_adapter.rs`):
`../workflow/`, `../background-tasks/`, `../duplicate-agent/` — each one's
`turn1.jsonl` is the capture up to and including its FIRST `result` frame, so
one prompt replays the whole probe. `../plan-flap/` and `../plan-stuck/`
(EXP-853) are hand-built from the `../plan/` recording's `init` +
`ExitPlanMode` frames: the same replay, once inside the mode-announcement
grace window and once past it (`EXP_MODE_ANNOUNCE_GRACE_MS=0`).
`../workflow-agents/` (EXP-1225) is `workflow-agents-2.1.286.jsonl` up to its
first `result`. `../continuation/` (EXP-1224, `tests/claude_engine.rs`) is its
background-`Agent` turn and the continuation after it, with the agent's
completion frames moved past the turn's `result` (the order the reporting
run's journal measured: the agent outlived the turn) and a `@@SLEEP 0.5` line
— the fake's stand-in for the CLI's latency — between that notification and
the continuation's `init`.
`../workflow-continuation/` (EXP-1224 review, `tests/claude_engine.rs`) is
`workflow-agents-2.1.286.jsonl` WHOLE and verbatim, plus a `@@SLEEP 0.5`
before each continuation's `init`: both of its notifications land MID-turn
(before that turn's `result`), the shape where nothing is between turns when
the notification comes.
