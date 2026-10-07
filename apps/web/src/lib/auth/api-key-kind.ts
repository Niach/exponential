// EXP-1140: which KIND of `expu_` key authenticated a request.
//
// The desktop/CLI launcher silently mints a hidden personal key for the
// AGENT it spawns (`users.mintPersonalApiKey({ purpose: 'agent' })`, written
// into the run's MCP config as the agent's Exponential credential). That key
// is the agent's identity, not the person's: a prompt-injected agent holds
// it, so anything a person would never want an agent to reach (the member's
// decrypted MCP credentials, `mcpServers.resolveForLaunch`) refuses it by
// kind. Keys the person mints under Settings → Security stay `personal` and
// keep full access: a CLI daemon on `EXP_TOKEN` and human MCP clients run on
// those.
//
// FEED-76: a person's key may also carry a SCOPE — the team/board selection
// the OAuth consent screen offers, stored in the same `metadata` JSON as
// `scope: {allTeams: false, teamIds, boardIds}`. A scoped key is an MCP-only
// credential: /api/mcp confines it exactly like an OAuth grant
// (lib/mcp/scope.ts) and the general surface (tRPC, shapes, attachments)
// refuses it (lib/auth/resolve-bearer.ts), the way OAuth tokens are refused
// there. A key without a stored scope (every key minted before FEED-76,
// device keys, agent keys, "Everything" picks) stays a full key.
//
// How a request maps to its key: the Better Auth api-key plugin
// (`enableSessionForAPIKeys`) builds a mock session whose `session.id` IS
// the api-key row id (never a `sessions` row id, those never collide: both
// are random ids in separate tables). One indexed lookup on `apikeys`
// answers "was this an agent key?"; a cookie or bearer-session request finds
// no row and reads as not-an-agent. Metadata is the plugin's JSON string.
import { and, eq } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { apikeys } from "@/db/auth-schema"
import type { Context } from "@/lib/trpc"

export const API_KEY_KINDS = [`personal`, `agent`] as const
export type ApiKeyKind = (typeof API_KEY_KINDS)[number]

/** A stored key scope: never "everything" (an all-teams pick stores NO
 * scope, so the key stays an ordinary full key). */
export interface ApiKeyScope {
  allTeams: false
  teamIds: string[]
  boardIds: string[]
}

export interface ApiKeyMetadata {
  kind: ApiKeyKind
  /** `null` = unscoped (full membership access everywhere). */
  scope: ApiKeyScope | null
}

const UNSCOPED_PERSONAL: ApiKeyMetadata = { kind: `personal`, scope: null }

const isStringArray = (value: unknown): value is string[] =>
  Array.isArray(value) && value.every((item) => typeof item === `string`)

/** Parse the plugin's `metadata` text: the key's kind (`personal` when
 * untagged — every key minted before EXP-1140) and its scope (`null` when
 * absent or `allTeams`). A PRESENT but malformed scope fails CLOSED (nothing
 * granted): only this server writes it, so a bad shape is a bug, never a
 * reason to widen a key. */
export function parseApiKeyMetadata(
  metadata: string | null | undefined
): ApiKeyMetadata {
  if (!metadata) return UNSCOPED_PERSONAL
  let parsed: unknown
  try {
    parsed = JSON.parse(metadata)
  } catch {
    // Not JSON — an untagged key.
    return UNSCOPED_PERSONAL
  }
  if (!parsed || typeof parsed !== `object`) return UNSCOPED_PERSONAL
  const record = parsed as { kind?: unknown; scope?: unknown }
  const kind: ApiKeyKind = record.kind === `agent` ? `agent` : `personal`
  if (record.scope === undefined || record.scope === null) {
    return { kind, scope: null }
  }
  const scope = record.scope as {
    allTeams?: unknown
    teamIds?: unknown
    boardIds?: unknown
  }
  if (!scope || typeof scope !== `object`) {
    return { kind, scope: { allTeams: false, teamIds: [], boardIds: [] } }
  }
  if (scope.allTeams === true) return { kind, scope: null }
  if (!isStringArray(scope.teamIds) || !isStringArray(scope.boardIds)) {
    return { kind, scope: { allTeams: false, teamIds: [], boardIds: [] } }
  }
  return {
    kind,
    scope: { allTeams: false, teamIds: scope.teamIds, boardIds: scope.boardIds },
  }
}

/** The key's kind alone (`personal` when untagged). */
export function apiKeyKindOf(metadata: string | null | undefined): ApiKeyKind {
  return parseApiKeyMetadata(metadata).kind
}

/** The parsed metadata of the caller's own `expu_` key behind `session`, or
 * `null` when the request rode a real (cookie/bearer) session — one indexed
 * lookup (the PK + `reference_id`). */
export async function apiKeySessionMetadata(
  db: Context[`db`],
  session: NonNullable<Context[`session`]>
): Promise<ApiKeyMetadata | null> {
  const sessionId = session.session?.id
  if (!sessionId) return null
  const [row] = await db
    .select({ metadata: apikeys.metadata })
    .from(apikeys)
    .where(
      and(eq(apikeys.id, sessionId), eq(apikeys.referenceId, session.user.id))
    )
    .limit(1)
  return row === undefined ? null : parseApiKeyMetadata(row.metadata)
}

/** The kind of the caller's own `expu_` key behind `session`, or `null` when
 * the request rode a real (cookie/bearer) session. */
export async function apiKeySessionKind(
  db: Context[`db`],
  session: NonNullable<Context[`session`]>
): Promise<ApiKeyKind | null> {
  return (await apiKeySessionMetadata(db, session))?.kind ?? null
}

/** True when the request that built `session` authenticated with a key the
 * launcher minted for an agent. */
export async function isAgentApiKeySession(
  db: Context[`db`],
  session: NonNullable<Context[`session`]>
): Promise<boolean> {
  return (await apiKeySessionKind(db, session)) === `agent`
}

/** True when the request authenticated with ANY `expu_` key (a person's or
 * an agent's), never with a browser/native session. */
export async function isApiKeySession(
  db: Context[`db`],
  session: NonNullable<Context[`session`]>
): Promise<boolean> {
  return (await apiKeySessionKind(db, session)) !== null
}

// Identity changes (the primary email, linked providers, passkeys) need a
// REAL session: the api-key plugin mocks one for every endpoint
// (`enableSessionForAPIKeys`), so without this rule a leaked key, or the
// agent's hidden one, could re-home the whole account. The Better Auth guard
// (lib/auth/sign-in-methods.ts) answers with the same code and message.
export const API_KEY_IDENTITY_CODE = `API_KEY_CANNOT_CHANGE_IDENTITY`
export const API_KEY_IDENTITY_MESSAGE = `Sign-in methods and the account email can only be changed from a signed-in browser or app, not with an API key.`

/** tRPC flavour: refuse an identity-changing mutation reached with a key. */
export async function assertNotApiKeySession(
  db: Context[`db`],
  session: NonNullable<Context[`session`]>
): Promise<void> {
  if (await isApiKeySession(db, session)) {
    throw new TRPCError({
      code: `UNAUTHORIZED`,
      message: API_KEY_IDENTITY_MESSAGE,
    })
  }
}

/** tRPC flavour: refuse the agent's own key (a person's key passes). */
export async function assertNotAgentApiKeySession(
  db: Context[`db`],
  session: NonNullable<Context[`session`]>,
  message: string
): Promise<void> {
  if (await isAgentApiKeySession(db, session)) {
    throw new TRPCError({ code: `FORBIDDEN`, message })
  }
}

export const AGENT_KEY_MANAGES_KEYS_MESSAGE = `Agent keys cannot manage API keys`
/** FEED-76: a scoped key reaching anything but /api/mcp. */
export const SCOPED_API_KEY_MESSAGE = `This API key is scoped to selected teams/boards and only works with the MCP endpoint (/api/mcp). Create an unscoped key for the API.`
/** FEED-76: the agent's hidden key must keep the launcher's full access. */
export const AGENT_KEY_SCOPE_MESSAGE = `An agent key cannot be scoped`
export const AGENT_KEY_MANAGES_MCP_MESSAGE = `Agent keys cannot manage MCP servers or their credentials`
