# @exp/emcp

eMCP's contract: how an Exponential app exposes MCP tools (`exposes` bindings, `${input.x}` resolution, `<app>__<tool>` naming), who sees them (the grant model and the access rule), the `mcp_calls` audit row, the vault format and the upstream OAuth states. Each `fixtures/*.json` is the spec (its `$comment`) plus the cases; `src/` holds the pure TypeScript references that replay them. The server half (`apps/web/src/lib/emcp`) and the Rust crate `emcp` replay the same fixtures.

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
| `upstream.json` | one member's upstream connection: states, transitions, callable, the consent page's missing upstreams |
| `hub.json` | the device's local hub (#EXP-1507): server names, a run's tool list under its grant (a level per server), routing, the `mcp_calls` row per call |

Licence: Apache-2.0
