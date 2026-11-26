# @exp/emcp

MCP Proxy's contract: how an Exponential app exposes MCP tools (`exposes` bindings, `${input.x}` resolution, `<app>__<tool>` naming), who sees them (the grant model and the access rule), the `mcp_calls` audit row, app automations (trigger matching, the loop guard, the sponsor), the vault format and the upstream OAuth states. Each `fixtures/*.json` is the spec (its `$comment`) plus the cases; `src/` holds the pure TypeScript references that replay them. The server half (`apps/web/src/lib/emcp`) and the Rust crate `emcp` replay the same fixtures.

```ts
import { validateExposes, qualifiedToolName } from "@exp/emcp/exposes"
import fixture from "@exp/emcp/fixtures/exposes.json" with { type: "json" }
```

| fixture | covers |
|---|---|
| `exposes.json` | the binding format, validation, template resolution, tool naming + collisions, `_meta` provenance |
| `access.json` | `mcp_grants` (`kind`, `all_apps`, `app_ids`, `access`), the member policy, effective access, visibility, revocation |
| `audit.json` | the `mcp_calls` row, canonical JSON + the args hash, statuses, 90-day retention |
| `vault.json` | `VAULT_KEY` keyrings, `ev1.<kid>.<iv>.<body>` AES-256-GCM, the row-binding AAD, rotation, exact vectors |
| `upstream.json` | one member's upstream connection: states, transitions, callable, the consent page's missing upstreams; `binding` = `upstream:<server>/<tool>` read bindings (parse, declare, read-only, cache key, value, banners) |
| `deletes.json` | the destructive ops: one explicit id, the delete burst guard + its team event, the audit `target`, tool tests refusing prod |
| `hub.json` | the device's local hub (#EXP-1507): server names, a run's tool list under its grant (a level per server), routing, the `mcp_calls` row per call |
| `skills.json` | Skills over MCP (#NEW-145): `skill://<team-slug>/<name>/SKILL.md` + the built-ins at `skill://exponential/…`, complete manifests, `sha256:` digests over the served bytes, `ttlMs` / `cacheScope`, `skills/list`, `skills/get`, `resources/read`, `resources/directory/read`, -32602 cases |
| `elicitation.json` | form confirms on every delete and session stop, team and device picks, URL mode only to `/mcp/elicit/<id>`, the no-secret rule, decline / cancel / no capability, the MRTR `requestState` (exact vectors; shared with files) |
| `tasks.json` | MCP tasks as the second watcher kind on `session_watches` (eagent rules.json `watcher`): who gets one, the status mapping from transitions, cards, reports and event firings, tasks/get, update (card-answer), cancel (stop), TTL, principals, rate limit |
| `hub-client.json` | the device hub as the MCP client of a run's team servers (#NEW-148): one `team` entry, client capabilities, MRTR elicitation into the session's card, hub-driven tasks (poll clamp, trace steps, store, cancel), 401 refresh, skills (verify, digest set, owner approval, dir names, rewritten SKILL.md) |
| `files.json` | file uploads behind `MCP_FILES_EXTENSION`: inline up to 1 MiB, `upload_required` to staging, the retry's state, argument and byte checks |

## Migration: team MCP tool names (#NEW-148)

Team MCP servers no longer appear as their own entries in the agent CLI's
config. They are mounted behind the device's hub, and the CLI sees ONE entry,
`team` (`hub-client.json` `toolNameCases`):

| | before | after |
|---|---|---|
| Claude Code | `mcp__linear__create_issue` | `mcp__team__linear__create_issue` |
| Codex | server `linear`, tool `create_issue` | server `team`, tool `linear__create_issue` |
| code mode | upstream `linear` | upstream `team`, tool `linear__create_issue` |

The server part is the hub name: the server's config key with `_` folded to
`-` (`linear_app` → `linear-app`). Prompts, skills, permission allow-lists or
hooks that name the old `mcp__<server>__*` tools need the new names. The
credentials never reach the CLI's environment any more: the hub holds them
and refreshes an expired OAuth token mid-run.

Licence: Apache-2.0
