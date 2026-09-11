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
// trigger. (The EXP-707 set went with the 0.14.24/0.14.26/0.14.31 floors,
// EXP-730.) Open right now — the EXP-825 set, every one marked
// `EXP-825 compat` at its site, removable when ios min >= 0.14.29,
// android min >= 0.14.31, desktop/cli min >= 0.14.36:
//
// - steer.startSession (+ MCP exponential_sessions_start, which forwards
//   to it): the builtin text INPUTS fold into `prompt` when it is blank —
//   `inputs.prompt` for builtin:chat, `inputs.description` (+ `name` as a
//   `Name: <name>` line) for builtin:create-action; the keys leave
//   `inputs`. `foldLegacyBuiltinInputs` in lib/trpc/steer.ts.
// - steer.startSession, target side: a device WITHOUT the `start-prompt`
//   cap gets the two builtins' text back on the legacy input key (`prompt`
//   / `description`) with no top-level `prompt`; only an embed-carrying
//   prompt is refused. Every other subject with a prompt refuses
//   (PRECONDITION_FAILED "older Exponential app that ignores start
//   instructions") — that refusal is an invariant check and stays.
// - actions.create / actions.update (+ MCP exponential_actions_create /
//   _update): `compatActionInputsSchema` accepts the retired `text` /
//   `textarea` input kinds; `retireLegacyActionInputs` (lib/action-inputs.ts)
//   drops them and seeds `promptPlaceholder` from the first dropped def
//   (placeholder, else label, LEFT 200) when the row has none — migration
//   0108's rule. The stored shape never carries the retired kinds.
//
// Also open — the FEED-33 set (`devices.shared_team_id` → `shared_team_ids`
// uuid[]), every one marked `FEED-33 compat` at its site, removable when
// CLIENT_MIN_VERSION_IOS >= 0.14.30 AND CLIENT_MIN_VERSION_ANDROID >=
// 0.14.32 AND CLIENT_MIN_VERSION_DESKTOP (which also floors the CLI) >=
// 0.14.37:
//
// - devices.setShared, legacy single-team form `{deviceId, teamId}` (no
//   `shared`): `teamId` ADDS that team to the share set (membership-checked,
//   sorted, deduped like the toggle form); `teamId: null` still clears the
//   whole set. It used to REPLACE the set with `[teamId]` — a pre-FEED-33
//   client reads only the single alias below, renders a box shared with A
//   and C as unshared, and its "share with B" then silently revoked A and C
//   (and the revoke fan-out ended their teammates' live runs). The toggle
//   form `{deviceId, teamId, shared}` is unchanged. `nextSharedTeamIds` in
//   lib/trpc/devices.ts.
// - devices.list / steer.devices rows (`steerDeviceFromRow`,
//   lib/steer-devices.ts) and MCP exponential_devices_list rows
//   (lib/mcp/tools.ts): `sharedTeamId: sharedTeamIds[0] ?? null` rides
//   beside `sharedTeamIds` — the old "Shared" badge key (desktop/CLI
//   0.14.36 `DeviceRow.shared_team_id`, iOS 0.14.28/0.14.29 and Android
//   0.14.31 `sharedTeamId`). Additive; no web reader, no MCP description
//   change.
// - devices SHAPE column `shared_team_id` (migration 0114 re-added it, NO
//   foreign key; `DEVICE_COLUMNS` in routes/api/shapes/devices.ts): a
//   trigger-maintained mirror of `shared_team_ids[1]` (custom/0001_triggers.sql
//   #17 `mirror_device_shared_team_id`, BEFORE INSERT OR UPDATE OF
//   shared_team_ids + a boot heal pass; NULL when private, deterministic
//   because the array is sorted). Server-written only — no router or the
//   register/heartbeat path writes it, and `device_worktrees` never gets it.
//   The same old clients read it off the SHAPE (iOS `row.sharedTeamId ==
//   teamId` in DeviceRows.swift, Android `@SerialName("shared_team_id")`,
//   desktop `DeviceRow.shared_team_id`): without it teammates' shared servers
//   vanish from their device pickers. The where clause stays on
//   `shared_team_ids`. Dropping it = column + trigger #17 + the allowlist
//   entry, at the same floor.
export {}
