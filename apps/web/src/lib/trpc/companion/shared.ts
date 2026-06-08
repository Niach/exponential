import { TRPCError } from "@trpc/server"
import { and, eq, inArray } from "drizzle-orm"
import { agentRegistrations, workspaceMembers } from "@/db/schema"
import { assertWorkspaceMember } from "@/lib/workspace-membership"

export function baseUrlFromRequest(request: Request): string {
  const configured = process.env.BETTER_AUTH_URL?.replace(/\/$/, ``)
  if (configured) return configured
  return new URL(request.url).origin
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
    .from(agentRegistrations)
    .where(eq(agentRegistrations.id, agentId))
    .limit(1)

  if (!agent) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Device not found` })
  }

  // A device is account-level: only its human owner may manage it.
  if (agent.ownerUserId !== ownerUserId) {
    throw new TRPCError({ code: `FORBIDDEN`, message: `Not your device` })
  }
  return agent
}

// A desktop device is a member (role=agent) of every workspace its owner
// belongs to, so it is assignable to any issue the owner could be. Insert the
// device's agent user into all such workspaces (idempotent — safe to re-run on
// every auto-register/launch, which is how the device catches up on workspaces
// the owner joined since last time). Returns the workspace count.
export async function fanOutDeviceMembership(
  // eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
  db: typeof import("@/db/connection").db,
  ownerUserId: string,
  agentUserId: string
): Promise<number> {
  const rows = await db
    .select({ workspaceId: workspaceMembers.workspaceId })
    .from(workspaceMembers)
    .where(
      and(
        eq(workspaceMembers.userId, ownerUserId),
        inArray(workspaceMembers.role, [`owner`, `member`])
      )
    )
  if (rows.length === 0) return 0
  await db
    .insert(workspaceMembers)
    .values(
      rows.map((r) => ({
        workspaceId: r.workspaceId,
        userId: agentUserId,
        role: `agent` as const,
      }))
    )
    .onConflictDoNothing()
  return rows.length
}

export async function loadAgentForSessionUser(
  // eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
  db: typeof import("@/db/connection").db,
  userId: string
) {
  const [agent] = await db
    .select()
    .from(agentRegistrations)
    .where(eq(agentRegistrations.userId, userId))
    .limit(1)

  if (!agent) {
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: `Authenticated user is not a companion agent`,
    })
  }

  return agent
}
