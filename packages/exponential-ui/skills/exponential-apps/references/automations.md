# Automations: schedule, record, event and webhook triggers; agent hand-off

> Generated from docs/spec/apps.md §7.1-§7.3 (genUI 0.21.0) by packages/exponential-ui/scripts/generate-skills.ts — do not edit.

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
  - `webhook: {name}` (A3, §7.2): a signed `POST` to the app's hook URL; the body is `${event.body.<path>}`. `where` filters are not in A1.
- **`run`:** an `exposes` run (`records`, `action`, `upstream`, `agent`) with the same validator: no destructive runs, an action must be granted by an owner, an upstream must be declared. `agent` (A2) hands off to a team agent (§7.3). Templates: `${record.<field>}` (record triggers: its data + `id`; on delete, the data it had), `${event.<field>}` (event triggers: the payload + `name`), `${firedAt}`. No action through `exponential.apps.*`.
- **Executor:** the **server broker** (`runBrokerTool`, the same records service, grants and vault as a tool call), on the **published** version only (V1). Every run, and every refusal, is one `mcp_calls` row with `client_id = automation:<name>`, `tool` = `records.<op>:<kind>`, the action, or `<server>/<tool>` (Settings › MCP › Activity).
- **Run-as** (C23): an automation acts as its **sponsor**: the member who published the running version (`apps.published_by`), or a member it names by email (`sponsor`), honoured only when an owner published. The sponsor must still be a member with write access to the app. Upstream runs use the sponsor's credential; writes read "via <app>" (§9.3). Otherwise the automation pauses (`sponsorGone`, `sponsorReadOnly`, `sponsorNotAllowed`), the app's owners get one notification per pause, and a publish by a member with access resumes it. There is no bot identity (decided 2026-11-01).
- **Caps:** at most 16 automations per app; at most 30 runs a minute per app (over it: audited `rateLimited`, not run). **No chaining:** a run executes in an automation scope, so its records write and the events its action emits fire no automation, its own included.
- **Settings:** the app's settings page lists each automation with its trigger, its last run and its last failure (from its audit rows), the runs the cap dropped in the last 24 h (#VAPP-255: `N runs dropped in 24 h`; the drop stays, a run is never retried) and a pause, ×4.

### 7.2 Webhooks (✅ A3 server, #VAPP-237)

`on: {webhook: {name}}` runs an automation when an outside service posts to the hook. The contract is `packages/emcp/fixtures/automations.json` (`webhook`, TS `@exp/emcp/webhooks`); the server half is `apps/web/src/lib/apps/webhooks.ts`.

```jsonc
{ "name": "stripe_paid", "on": { "webhook": { "name": "stripe" } },
  "run": { "records": { "op": "upsert", "kind": "payment", "data": { "customer": "${event.body.data.customer}" } } } }
```

- **URL:** `POST /api/apps/hooks/<appId>/<name>`. The hook name is unique among the app's webhooks; the published package's automations on it run (A1's executor: the sponsor, the caps, the audit).
- **Secret:** one per hook, `whsec_` + 32 random bytes (base64url), minted by a team owner on the app's Settings (tRPC `appAccess.rotateHook`), answered ONCE, stored only vault-sealed (`ev1`, slot `app_hooks.secret_ciphertext`, re-sealed by the vault sweep), never synced. Minting again rotates it: the old secret stops at once. A hook without a secret answers 404.
- **Verification:** header `exponential-signature: t=<unix seconds>,v1=<hex HMAC-SHA256(secret, "<t>." + raw body)>` (Stripe's scheme; several `v1` may ride along). In order: body > 64 KB = 413, no header = 401, malformed = 401, `|now − t|` > 300 s = 401 (`stale`), no matching `v1` = 401, the signature already seen (the nonce cache: every accepted signature for 2 × the window, table `app_hook_nonces`, pruned by the watchdog) = 409 (`replayed`), not JSON = 400. The body is parsed only after the signature verifies. A verified delivery answers 202 with each run's status.
- **Payload:** the run's event = `{hook, body}`: `${event.body.<path>}` in the run (and in an agent hand-off's data block, never its instructions, §12.1).
- **Rate and audit:** at most 60 deliveries per hook per minute (429 beyond, not audited, so a flood never fills the audit table). Every delivery under the cap is one `mcp_calls` row: the run's (`automation:<name>`), or a refusal audited `denied` with its code (`badSignature`, `stale`, `replayed`, …) as the publisher.
- ✅ Settings ×4 (#VAPP-238, `apps.json` `webhookRowCases`): the Webhooks group under Automations, one row per hook with its URL (Copy URL), a Ready / No secret pill and the owner's Create secret / Rotate secret (the `rotate-webhook-secret` prompt first); the answer's secret is shown once under its hook (Copy secret), held in memory only. The trigger line "On webhook <name>" is ✅ ×4.

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
