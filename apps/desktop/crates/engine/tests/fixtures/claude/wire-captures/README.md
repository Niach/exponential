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
