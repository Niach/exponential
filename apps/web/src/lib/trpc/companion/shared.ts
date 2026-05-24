import { TRPCError } from "@trpc/server"
import { z } from "zod"
import { createHash, randomBytes } from "node:crypto"
import { eq } from "drizzle-orm"
import { workspaceAgents } from "@/db/schema"
import { assertWorkspaceMember } from "@/lib/workspace-membership"

export const SETUP_TOKEN_PREFIX = `expc_`
export const SETUP_TOKEN_TTL_MS = 30 * 60 * 1000

export const setupTokenSchema = z
  .string()
  .regex(/^expc_[A-Za-z0-9_-]{32,}$/, `Invalid setup token`)

export function hashSetupToken(token: string): string {
  return createHash(`sha256`).update(token).digest(`hex`)
}

export function generateSetupToken() {
  const token = `${SETUP_TOKEN_PREFIX}${randomBytes(32).toString(`base64url`)}`
  return {
    token,
    hash: hashSetupToken(token),
    expiresAt: new Date(Date.now() + SETUP_TOKEN_TTL_MS),
  }
}

export function baseUrlFromRequest(request: Request): string {
  const configured = process.env.BETTER_AUTH_URL?.replace(/\/$/, ``)
  if (configured) return configured
  return new URL(request.url).origin
}

export function installCommand(baseUrl: string, setupToken: string): string {
  return `curl -fsSL ${baseUrl}/install/companion.sh | bash -s -- --server ${baseUrl} --setup-token ${setupToken}`
}

export async function assertOwner(userId: string, workspaceId: string) {
  await assertWorkspaceMember(userId, workspaceId, [`owner`])
}

export async function loadOwnedAgent(
  // eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
  db: typeof import("@/db/connection").db,
  ownerUserId: string,
  agentId: string
) {
  const [agent] = await db
    .select()
    .from(workspaceAgents)
    .where(eq(workspaceAgents.id, agentId))
    .limit(1)

  if (!agent) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Agent not found` })
  }

  await assertOwner(ownerUserId, agent.workspaceId)
  return agent
}

export async function loadAgentForSessionUser(
  // eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
  db: typeof import("@/db/connection").db,
  userId: string
) {
  const [agent] = await db
    .select()
    .from(workspaceAgents)
    .where(eq(workspaceAgents.userId, userId))
    .limit(1)

  if (!agent) {
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: `Authenticated user is not a companion agent`,
    })
  }

  return agent
}
