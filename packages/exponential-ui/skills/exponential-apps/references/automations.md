# Automations: schedule, record and event triggers; agent hand-off

> Generated from docs/spec/apps.md §7.1, §7.3 (eUI 0.10.0) by packages/exponential-ui/scripts/generate-skills.ts — do not edit.

### 7.1 Triggers

✅ A1 (#VAPP-220): `automations`, one trigger → one binding, run by the server as a sponsor.

```jsonc
"automations": [
  { "name": "monday_rotation", "on": { "schedule": { "cron": "0 8 * * MON" } },
    "run": { "records": { "op": "upsert", "kind": "rotation", "data": { "startedAt": "${firedAt}" } } } },
  { "name": "notify_approver", "on": { "record": { "kind": "request", "op": "create" } },
    "run": { "action": "exponential.comments.create", "args": { "issueId": "${record.issue}", "body": "New request: ${record.title}" } } },
  { "name": "log_merges", "on": { "event": "pr.merged" }, "sponsor": "lead@acme.test",
    "run": { "records": { "op": "upsert", "kind": "merge", "data": { "title": "${event.title}" } } } }
]
```

The contract is `packages/emcp/fixtures/automations.json` (TS `@exp/emcp/automations`, Rust `emcp::contract::automations`); the server rules and the executor are `apps/web/src/lib/apps/{package,automations}.ts`.

- **`on`** (exactly one):
  - `schedule: {cron, tz?}`: a 5-field cron (the agents' trigger cron: names, ranges, steps), read in `tz` or the team time zone; the minutes of a day it fires on are ≥ 15 min apart. The watchdog tick runs the latest due minute since its last check, caught up once after downtime (24 h), never twice.
  - `record: {kind, op}`: a committed create/update/delete of a declared kind (members' writes, package actions, tools).
  - `event: "<name>"`: the team stream (`issue.created`, `pr.merged`, `session.ended`… or any `events_emit` name), the same events agents' triggers use ✅. It runs once the event is committed.
  - `webhook` comes with A3 (§7.2). `where` filters are not in A1.
- **`run`:** an `exposes` run (`records`, `action`, `upstream`, `agent`) with the same validator: no destructive runs, an action must be granted by an owner, an upstream must be declared. `agent` (A2) hands off to a team agent (§7.3). Templates: `${record.<field>}` (record triggers: its data + `id`; on delete, the data it had), `${event.<field>}` (event triggers: the payload + `name`), `${firedAt}`. No action through `exponential.apps.*`.
- **Executor:** the **server broker** (`runBrokerTool`, the same records service, grants and vault as a tool call), on the **published** version only (V1). Every run, and every refusal, is one `mcp_calls` row with `client_id = automation:<name>`, `tool` = `records.<op>:<kind>`, the action, or `<server>/<tool>` (Settings › MCP › Activity).
- **Run-as** (C23): an automation acts as its **sponsor**: the member who published the running version (`apps.published_by`), or a member it names by email (`sponsor`), honoured only when an owner published. The sponsor must still be a member with write access to the app. Upstream runs use the sponsor's credential; writes read "via <app>" (§9.3). Otherwise the automation pauses (`sponsorGone`, `sponsorReadOnly`, `sponsorNotAllowed`), the app's owners get one notification per pause, and a publish by a member with access resumes it. There is no bot identity (decided 2026-11-01).
- **Caps:** at most 16 automations per app; at most 30 runs a minute per app (over it: audited `rateLimited`, not run). **No chaining:** a run executes in an automation scope, so its records write and the events its action emits fire no automation, its own included.
- **Settings:** the app's settings page lists each automation with its trigger, its last run and its last failure (from its audit rows) and a pause, ×4.

### 7.3 Agent hand-off (✅ A2, #VAPP-236)

`run.agent` hands judgement to a team agent. The contract is `packages/emcp/fixtures/exposes.json` (`agent`, `$comment` the spec) and `automations.json` (the agent loop guard); the server half is `apps/web/src/lib/apps/agent-handoff.ts`.

```jsonc
{ "name": "route_request", "on": { "record": { "kind": "request", "op": "create" } },
  "run": { "agent": "triage-bot", "prompt": "Route the request ${record.title} to its owner.", "device": "default" } }
```

- **The form:** `{agent, prompt, device?}`. `agent` = the agent's slug (its name lower-cased, everything outside `a-z0-9` one `-`: "Triage Bot" = `triage-bot`; the team's oldest agent with that slug, never a personal Assistant). `prompt` = at most 4000 characters, frozen per version. `device` absent or `"default"` (the agent's own device). It always writes.
- **Where it runs:** an automation (A1, as its **sponsor**), an `exposes` tool (as the **calling member**) and the `handoff` action `exponential.apps.handoff {slug, tool, input, record?}` (a press runs one of the package's published `agent` exposes as the viewer, behind the consent card unless an owner granted it; display row `apps_handoff`).
- **The session:** `sessions.send {agentId}`, the one entrypoint: the run-as member's own conversation with that agent (created on first use on the agent's device, then resumed), reading "via <app>" (M3 `via_app_id`). The agent's output lands in that conversation; it calls back into the app only through the app's `exposes`, if its MCP grant includes the app.
- **The brief (data is never instructions, §12.1):** the prompt with each `${…}` filled as inline data (backticks, quotes stripped of backticks and newlines, at most 200 characters each), then `From the app <name> (<automation|tool|handoff> <name>).`, for a record trigger `Record: <kind> <id> of the app <slug>.` (the ref back to the record), and the whole input as a fenced JSON block marked as data (at most 8000 characters).
- **Caps:** at most 10 hand-offs per app per rolling minute (over it: `rateLimited`, audited), beside the automations' 30 runs. **Loop guard:** a record write or an event caused from inside an agent's session tree (the MCP gate tags each call with the caller's own session) never fires an automation that hands off to that same agent; every other automation still runs.
- **Audit:** an automation's row reads `tool = agent:<slug>`; an exposed tool's row is the broker's as before.
