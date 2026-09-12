# Steer wire additions for EXP-850 / EXP-853 / EXP-856

Pinned before implementation (2026-09-12). Every lane implements exactly this;
`apps/steer-relay/src/protocol.ts` (zod) and `crates/steer/src/frames.rs` are
the schemas of record and must agree byte-for-byte with the shapes below.
Raw CLI captures: `apps/desktop/crates/engine/tests/fixtures/claude/wire-captures/`.
Contract vocabulary (already generated): `toolKind` += `wait`; new
`subagentStatus`, `workflowAgentState`, `workflowStatus`, `backgroundTaskKind`,
`steerWorking { verbs[24], tokenTickMs: 2000, previewMax: 160 }`.

## 1. `tool` rows with `toolKind: "wait"`

Claude `TaskOutput` and `Monitor` calls are waits. The engine emits their
`tool` row with `toolKind: "wait"`, `name` = the tool name, `detail` = a human
label: for `TaskOutput` the description of the task with that `task_id` from the
latest `background_tasks` (fallback: the id), for `Monitor` its `description`
input. `toolGroupSummary` treats `wait` exactly like `other` (no fixture change).
An unsettled `wait` row is rendered at the bottom strip (section 2) as
`Waiting on {detail}`; in the transcript it stays an ordinary tool row.

## 2. `background_tasks` (latest-wins slot)

```
{ kind: "background_tasks", tasks: [{ id, kind, description, toolId? }], at }
```
`kind` ∈ contract `backgroundTaskKind` (`local_bash` → `shell`,
`local_workflow` → `workflow`, `local_agent` → `agent`, else `other`);
`description` ≤ 160 chars; `toolId` = the launching tool_use_id when known.
Source: `system/background_tasks_changed` (the FULL current list; empty
array = nothing running). Latest-wins everywhere: relay `LATEST_WINS_KINDS`,
in-memory journal slot, on-disk history fold; replay order after `turn`.
Clients render a compact strip directly above the composer, one line per task
(`↻ {description}`) plus one line per open `wait` row (`Waiting on {detail}`);
the strip is absent when both are empty. This is the ×4 "monitors and shell
commands at the bottom" surface.

## 3. `workflow` (latest-wins per workflow id)

```
{ kind: "workflow", id, name, description?, status,
  phases: [{ index, title }],
  agents: [{ index, label, phaseIndex?, agentId?, model?, state,
             tokens?, toolCalls?, durationMs?, lastTool?, lastToolSummary?,
             resultPreview?, error? }],
  summary?, at }
```
- `id` = the `Workflow` tool_use_id (the same id as its `tool` row). `name`
  from `task_started.workflow_name`, `description` from `task_started.description`.
- `status` ∈ contract `workflowStatus`: `running` until `task_notification` /
  `task_updated` says `completed` → `completed`, `failed` → `failed`,
  anything else terminal (`stopped`, `cancelled`, `killed`) → `stopped`.
  `summary` = the notification's `summary` (≤ 160).
- agents come from `task_progress.workflow_progress` entries of type
  `workflow_agent` (latest per `index`); `state` mapping: CLI `start` without
  `startedAt` → `queued`, `start` with `startedAt` → `running`, `done` →
  `done`, `error` → `error`. `lastTool` = `lastToolName`; every preview /
  summary / error string is cut at `steerWorking.previewMax` (160) with `…`.
  `phases` from `workflow_phase` entries (latest per index).
- Publish cadence: at most once per 1000 ms per workflow while running, plus
  immediately on every agent state change (queued→running→done/error) and on
  the terminal status. The relay keeps ONE frame per id (`workflow:{id}`
  key in the latest-wins store); the in-memory journal and the on-disk history
  fold the same way (cap 16 workflow ids per session, oldest evicted).
  Replay order: after the log, before `background_tasks`.
- Clients patch the event onto the `tool` row with the same `id` and render
  that row as the workflow card (name, description, phase strip with per-phase
  queued/running/done/error counts, one nested row per agent, summary when
  finished). If the tool row is missing (never expected), append the card as
  its own row. The Workflow tool row's own settle (`tool_update`) is folded
  into the card, never a second row.
- Agents of a running workflow are NOT offered as subagent tabs and are never
  steerable; a workflow agent row expands to a collapsible preview of the
  nested `subagentId`-tagged events when any exist.

## 4. `subagent` additions

- New optional field `workflowId` (the `workflow.id`) on every subagent edge
  whose task id equals a workflow agent's `agentId`; such an edge also carries
  `title` = the agent's label and `agentType` = the entry's `agentType` or
  `"agent"`. Clients nest these edges under the card, never as loose rows,
  and `toolGroupSummary` / "N tool calls" counts never include them.
- New status `duplicate` (contract `subagentStatus`): published when a
  `task_started` arrives whose `task_id` equals a subagent id that is still
  live in this session (a running workflow agent or an unfinished ordinary
  subagent). Shape: `{ kind: "subagent", id, agentType, status: "duplicate",
  title?, workflowId?, detail }` with `detail` =
  `Second copy of {label} started while the first is still running (resumed by SendMessage)`.
  The duplicate's own later `started`/`completed` edges still flow under the
  same id. Clients render the `duplicate` edge as an amber warning row (under
  the workflow card when `workflowId` is set, inline otherwise); the desktop
  additionally raises an OS notification. Old clients ignore the unknown
  status (never fatal).

## 5. `turn` additions

```
{ kind: "turn", state: "started" | "ended", startedAt?, tokens?, at }
```
`startedAt` = the turn's start (ms) on BOTH states; `tokens` = output tokens
produced in this turn so far (`system/thinking_tokens.estimated_tokens` plus
assistant `usage.output_tokens`, monotone within a turn), republished at most
every `steerWorking.tokenTickMs` while started (latest-wins slot, so no feed
growth). Clients derive the working caption:
`{verb}… ({duration} · ↓ {tokens} tokens)` with `verb =
steerWorking.verbs[startedAt % verbs.length]`, duration formatted `37s` /
`2m 04s` / `1h 03m`, tokens `2.0k` / `812` / `1.2M`; the duration/tokens
group is omitted while unknown. While a `workflow` with status `running`
exists, the caption text is the workflow caption (section 7) and the same
duration/tokens suffix. Beside the caption every client draws the running
agent's brand mark (claude/codex) pulsing (opacity 0.4↔1, 1.4 s ease-in-out,
respects reduced motion) instead of the generic assistant glyph.

## 6. `config_state` dedupe and ordering (EXP-853)

1. A `config_state` byte-identical to the last published one is dropped
   (existing rule, keep).
2. An `init`-derived mode (the CLI re-announces `permissionMode` on every
   turn init) that contradicts an explicit mode change (ExitPlanMode approval,
   `set_permission_mode`, the EnterPlanMode hook) within 10 s of that change is
   dropped; the explicit mode wins. After 10 s the CLI's announced mode is the
   truth and is published.
3. On ExitPlanMode approval the engine sends `set_permission_mode` to the CLI
   with the approved mode (idempotent) so the next init agrees; the wire
   sequence for approve is exactly one `plan` → `bypassPermissions` edge.
4. Fixtures: a `plan-flap` scenario (init plan → ask → approve → init plan
   3 s later → init bypassPermissions) must publish `plan, bypassPermissions`
   and nothing else; a `plan-stuck` scenario (approval answered, CLI keeps
   announcing plan for > 10 s) must publish the CLI's `plan` again so the chip
   tells the truth.

## 7. Workflow caption (shared pure function, ×4 + relay-free)

`workflowCaption(w)`:
- running, no agents → `Workflow {name} · starting`
- running → `Workflow {name} · {done}/{total} agents done · {phase}` with
  done = agents in `done`|`error`, total = agents.length, phase = the
  `phaseTitle` of the running agent with the highest index, else of the
  highest-index agent; the ` · {phase}` segment is omitted without a title.
- completed → `Workflow {name} · done · {total} agents`;
  failed → `Workflow {name} · failed`; stopped → `Workflow {name} · stopped`.
Byte-locked by `packages/domain-contract/fixtures/workflow-caption.json`
(TS implementation `packages/domain-contract/src/workflow-caption.ts`,
mirrors: `crates/steer/src/workflow.rs`, web `lib/agent-feed.ts`, iOS
`ExpCore/Sources/Domain/WorkflowCaption.swift`, Android
`domain/WorkflowCaption.kt`). The Rust function is `steer::workflow_caption`.

## 8. `coding_sessions.agent_caption`

Nullable `text` column (≤ 160 chars), in the shape allowlist, device-written
via tRPC `codingSessions.setAgentCaption({ id, caption: string | null })`
with the same guards as `setAgentBusy` (owner or host, running/in_review rows
only, silent refusal). The device writes the workflow caption (section 7) of
the newest running workflow, throttled to one write per 5 s and only on
change, and writes `null` when no workflow runs, at turn end and at teardown.
Every session list row (desktop rail + Agent page, web sidebar + Agent page +
issue coding rows, iOS `AgentSessionsList`, Android `AgentSessionsList`)
renders a non-null caption as its second line, before the device byline.
Desktop rows hosted by the local engine read the in-process caption signal
(same precedence as `agent_busy`).

## 9. Pending ask/plan card at the bottom

Row projection ×4: every question row (single question or an ask group) that
is still pending (unresolved, not dismissed) is moved after all later rows,
keeping pending rows in their original relative order; once resolved it
returns to its natural position. The synthetic working row stays hidden while
a card is pending.

## 10. Session header (web + desktop)

`[Back] [dot identifier subject / caption]  …  [Plan chip (read-only, when in plan)] [Pin] [Context] [Diff] [Merge] [Stop]`
- Pin: ghost / borderless icon button, no circle stroke, no fill; the same
  variant everywhere a pin toggle renders (issue header, action dialog).
- Context: a small pill `199k / 200k` (clock glyph) opening the existing usage
  sheet/dialog; hidden when usage is unknown or the run ended.
- Diff: a small pill with the `coding-diff` glyph and `+N -M`; toggles the
  diff pane (section 11). Hidden when there is no diff.
- Merge: the existing merge control, only when merge is offered today.
- Stop: unchanged.
- The `…` overflow menu is removed on both; `/compact` remains a slash command.

## 11. Diff pane (web + desktop; mobile keeps its sheet)

A right-hand pane INSIDE the session view (splits the transcript column,
initial width 45 %, min 360 px, remembered per window/session). Header:
left a file-list toggle button (rendered only when > 1 file), then the
selected file's path with its `+N -M`; right a single close `×`. Body: the
same diff renderer the Changes bar used, one file at a time (or all files
stacked, matching what the renderer does today) with a collapsible file list
column on the left listing every file with `+N -M`; picking a file scrolls
to it. The bottom Changes bar is deleted. The diff shown is the session diff
(`latestDiff`); a per-message file card (section 12) opens the pane scrolled
to that file.

## 12. Inline per-turn file cards (web + desktop)

Derived, never on the wire: for each assistant turn segment (from a
`user_message` / turn start to the next `user_message` or turn end), collect
the `edit`/`delete`/`move` tool rows that settled with a `diff`; when at
least one exists, render one card at the end of the segment: title
`{N} files edited` (`1 file edited`), up to 5 rows `path  +a -d`, then
`{rest} more` toggling the remainder. Clicking a row opens the diff pane at
that file. Counts come from the row's diff (`+`/`-` lines).

## 13. Composer glyph and option rows

The steer composers (session composer + the Agent page composer) use the
`ui-add` (plus) concept for the attach button ×4; comment and description
editors keep `editor-image`. Ask/plan option rows use the row radius
(design token `radius.md` = 10 px) ×4, never a capsule; the number-key chip
uses `radius.sm`.
