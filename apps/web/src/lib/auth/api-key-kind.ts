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

/** Parse the plugin's `metadata` text into the key's kind (`personal` when
 * untagged — every key minted before EXP-1140). */
export function apiKeyKindOf(metadata: string | null | undefined): ApiKeyKind {
  if (!metadata) return `personal`
  try {
    const parsed: unknown = JSON.parse(metadata)
    if (
      parsed &&
      typeof parsed === `object` &&
      (parsed as { kind?: unknown }).kind === `agent`
    ) {
      return `agent`
    }
  } catch {
    // Not JSON — an untagged key.
  }
  return `personal`
}

/** The kind of the caller's own `expu_` key behind `session`, or `null` when
 * the request rode a real (cookie/bearer) session — one indexed lookup. */
export async function apiKeySessionKind(
  db: Context[`db`],
  session: NonNullable<Context[`session`]>
): Promise<ApiKeyKind | null> {
  const sessionId = session.session?.id
  if (!sessionId) return null
  const [row] = await db
    .select({ metadata: apikeys.metadata })
    .from(apikeys)
    .where(
      and(eq(apikeys.id, sessionId), eq(apikeys.referenceId, session.user.id))
    )
    .limit(1)
  return row === undefined ? null : apiKeyKindOf(row.metadata)
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
export const AGENT_KEY_MANAGES_MCP_MESSAGE = `Agent keys cannot manage MCP servers or their credentials`
