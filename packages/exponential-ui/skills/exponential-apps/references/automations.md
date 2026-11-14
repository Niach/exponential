# Automations: schedule, record, event and webhook triggers; agent hand-off

> Generated from docs/spec/apps.md §7.1-§7.3 (genUI 0.21.0) by packages/exponential-ui/scripts/generate-skills.ts — do not edit.

### 7.1 Triggers

✅ #EXP-1829 (ONE trigger system, replacing A1's package `automations`): an app's automations are **its agent's triggers** (`agents.triggers`, the same list as every agent's, Settings › Agents and the app's Events tab ×4). They are not part of the package any more: `apps upsert` refuses `automations`, and an old app file's `automations` become the new agent's triggers on import. The agent is created with the app (`apps.agent_id`); its root session is the app's editor.

```jsonc
// exponential_agents_write {op: "update", id: <the app's agentId>, triggers: [...]}
[
  { "kind": "schedule", "cron": "0 8 * * MON", "tz": "Europe/Vienna",
    "run": { "records": { "op": "upsert", "kind": "rotation", "data": { "startedAt": "${firedAt}" } } } },
  { "kind": "record", "record": "request", "op": "create",
    "run": { "action": "exponential.comments.create", "args": { "issueId": "${record.issue}", "body": "New request: ${record.title}" } } },
  { "kind": "event", "name": "pr.merged", "prompt": "Log the merge of ${event.title} if it touches billing." }
]
```

The contract is `packages/emcp/fixtures/automations.json` (TS `@exp/emcp/automations`, Rust `emcp::contract::automations`) for a run and `packages/db-schema` `agentTriggerSchema` for the list; the server is `apps/web/src/lib/agents/{triggers,app-triggers}.ts` and the executor `apps/web/src/lib/apps/automations.ts`.

- **When** (a trigger's `kind`):
  - `schedule {cron, tz?}`: a 5-field cron (names, ranges, steps), read in `tz` (UTC when absent). The watchdog tick runs the latest due minute since its last check, caught up once after downtime (24 h), never twice. A schedule that runs a binding fires at most every 15 minutes.
  - `event {name}`: the team stream (`issue.created`, `pr.merged`, `session.ended`… or any `events_emit` name). A run starts once the event is committed.
  - `record {record, op}` (an app's agent only): a committed create/update/delete of a declared kind (members' writes, package actions, tools). A write the app's own agent made never wakes it.
  - `webhook {name}` (an app's agent only, §7.2): a signed `POST` to the app's hook URL; the body is `${event.body.<path>}`.
- **What it does:**
  - By default it **wakes the agent**: a `trigger_fired` card into its owner's conversation (the app's root), `prompt` (optional, any agent) as the card's text with its templates filled, record and webhook data in a fenced block marked as data.
  - With **`run`** (an app's agent only) it runs ONE binding on the server instead, no session: an `exposes` run (`records`, `action`, `upstream`, `agent`) with the same validator (no destructive runs, an action must be granted by an owner, an upstream must be declared, never `exponential.apps.*`). `agent` hands off to ANOTHER team agent (§7.3).
  - Templates in `prompt` and `run`: `${record.<field>}` (record triggers: its data + `id`; on delete, the data it had), `${event.<field>}` (event and webhook triggers: the payload + `name`), `${firedAt}`.
- **Executor:** the **server broker** (`runBrokerTool`, the same records service, grants and vault as a tool call), on the **published** version only (V1). A run whose binding the published version cannot run never fires. Every firing of an app's trigger (a run, a wake), and every refusal, is one `mcp_calls` row with `client_id = automation:t_<trigger id>`, `tool` = `records.<op>:<kind>`, the action, `<server>/<tool>` or `agent:<slug>` for a wake (Settings › MCP › Activity, the Events tab's recent runs).
- **Run-as** (C23, #EXP-1829): a run acts as **the agent's owner** (the app's creator), like every trigger. The owner must still be a member with write access to the app. Upstream runs use the owner's credential; writes read "via <app>" (§9.3). Otherwise the trigger pauses (`sponsorGone`, `sponsorReadOnly`), the team's owners get one notification per pause, and it resumes by itself once the owner can run it again. There is no bot identity (decided 2026-11-01).
- **Caps:** at most 16 triggers per agent; at most 30 firings a minute per app, counting only those that ran (#VAPP-257; over it: audited `rateLimited`, not run). **No chaining:** a run executes in an automation scope, so its records write and the events its action emits fire no run, its own included.
- **Writes:** `agents.update {triggers}` replaces the list; a kept trigger (same id) that leaves out `prompt` or `run` keeps them (`null` clears), so an older client's edit never erases them. The team's owners and the app's creator edit an app's agent.

### 7.2 Webhooks (✅ A3 server, #VAPP-237)

`on: {webhook: {name}}` runs an automation when an outside service posts to the hook. The contract is `packages/emcp/fixtures/automations.json` (`webhook`, TS `@exp/emcp/webhooks`); the server half is `apps/web/src/lib/apps/webhooks.ts`.

```jsonc
{ "name": "stripe_paid", "on": { "webhook": { "name": "stripe" } },
  "run": { "records": { "op": "upsert", "kind": "payment", "data": { "customer": "${event.body.data.customer}" } } } }
```

- **URL:** `POST /api/apps/hooks/<appId>/<name>`. The hook name is unique among the app's webhooks; the app agent's webhook triggers on it fire (#EXP-1829: a wake, or a run through the executor: the owner, the caps, the audit).
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
