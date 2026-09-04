// EXP-705/EXP-707 — API conventions: ONE model per concept across the MCP
// tools (lib/mcp/tools.ts) and the tRPC routers (lib/trpc/*). Drift gates
// live in lib/mcp/api-conventions.test.ts; new tools and procedures follow
// these rules, and a deviation needs a reason written next to it.
//
// 1. STRICT INPUTS (EXP-705). Every MCP tool wraps its shape in
//    `strictInput(...)` — an unknown key is an immediate "unrecognized key"
//    error, never a silent strip. Serialized as additionalProperties:false.
//
// 2. ID NAMING. The tool's PRIMARY SUBJECT is `id` (the row the tool
//    gets/updates/deletes); every other reference is `<entity>Id`
//    (issueId, boardId, teamId, labelId, deviceId, ...). Row mutations never
//    require a teamId that is derivable from the row (the MCP layer derives
//    it for grant checks). tRPC mirrors the rule for NEW procedures;
//    teamMembers keeps `memberId` = the team_members ROW id (members_list
//    returns it alongside the user `id`).
//
// 3. IDENTIFIER ACCEPTANCE. Every issue-taking param accepts a UUID or a
//    human identifier ("EXP-42"), resolved by the ONE resolver
//    (lib/issue-resolver.ts): team-scoped, trash/archive-aware, newest match
//    wins deterministically, optionally confined to the MCP OAuth grant.
//
// 4. NULLABILITY. `null` clears, `undefined` skips — applied per field on
//    every optional clearable input (description, dueDate, assigneeId, icon,
//    repositoryId, agent/model/effort). A field that cannot be cleared (a
//    NOT NULL column with a default, like board color) stays non-nullable.
//
// 5. ENVELOPES. Mutations return `{ <row>, txId }` (the row echoed with the
//    Electric sync barrier — camelCase `txId`, NEVER `txid`); deletes return
//    `{ ok: true, id, txId }`; action-shaped calls use the ONE flag
//    vocabulary `ok: true`. Idempotent no-op paths may omit `txId` (nothing
//    changed, nothing to await) — callers treat a missing txId as no-wait.
//
// 6. PAGINATION. Every list surface declares `limit` (default 50, cap 200)
//    and `offset`; cursors are validated (ISO datetime), never a bare
//    string.
//
// 7. SHARED SCHEMAS. Validation primitives come from ONE place:
//    `hexColorSchema` / `DEFAULT_ACCENT_COLOR` / `dateOnlySchema` / `UUID_RE`
//    / `customizableStatusCategoryValues` from @exp/db-schema/domain, agent
//    enums from contract `codingAgent.values`, unique-violation probing from
//    lib/trpc/db-errors.ts. Never hand-copy an enum or a regex.
//
// 8. PINNED PROJECTIONS. MCP reads ship the same server-pinned columns as
//    the Electric shapes (lib/issue-columns.ts for issues; local mirrors in
//    tools.ts for boards/comments/notifications) — never a bare select()
//    that would leak the REV2-5/EXP-500 scoping mirrors or a future
//    server-only column.
//
// TRANSITIONAL ALIASES. A rename is only "hard" once every shipped client
// that sends the old key is retired by a version floor; until then the
// procedure accepts the legacy key OR the new one (exactly one required,
// normalized in the handler) and carries a comment naming its removal
// trigger. None are open right now (the EXP-707 set went with the
// 0.14.24/0.14.26/0.14.31 floors, EXP-730).
export {}
