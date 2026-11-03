# Automations: schedule, record and event triggers

> Generated from docs/spec/apps.md §7.1 (eUI 0.10.0) by packages/exponential-ui/scripts/generate-skills.ts — do not edit.

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
- **`run`:** an `exposes` run (`records`, `action`, `upstream`) with the same validator: no destructive runs, an action must be granted by an owner, an upstream must be declared. `agent` comes with A2 (§7.3). Templates: `${record.<field>}` (record triggers: its data + `id`; on delete, the data it had), `${event.<field>}` (event triggers: the payload + `name`), `${firedAt}`. No action through `exponential.apps.*`.
- **Executor:** the **server broker** (`runBrokerTool`, the same records service, grants and vault as a tool call), on the **published** version only (V1). Every run, and every refusal, is one `mcp_calls` row with `client_id = automation:<name>`, `tool` = `records.<op>:<kind>`, the action, or `<server>/<tool>` (Settings › MCP › Activity).
- **Run-as** (C23): an automation acts as its **sponsor**: the member who published the running version (`apps.published_by`), or a member it names by email (`sponsor`), honoured only when an owner published. The sponsor must still be a member with write access to the app. Upstream runs use the sponsor's credential; writes read "via <app>" (§9.3). Otherwise the automation pauses (`sponsorGone`, `sponsorReadOnly`, `sponsorNotAllowed`), the app's owners get one notification per pause, and a publish by a member with access resumes it. There is no bot identity (decided 2026-11-01).
- **Caps:** at most 16 automations per app; at most 30 runs a minute per app (over it: audited `rateLimited`, not run). **No chaining:** a run executes in an automation scope, so its records write and the events its action emits fire no automation, its own included.
- **Settings:** the app's settings page lists each automation with its trigger, its last run and its last failure (from its audit rows) and a pause, ×4.
