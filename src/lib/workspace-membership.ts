import { TRPCError } from "@trpc/server"
import { db } from "@/db/connection"
import { workspaceMembers, projects, labels } from "@/db/schema"
import { eq, and, inArray } from "drizzle-orm"

export async function getUserWorkspaceIds(userId: string): Promise<string[]> {
  const rows = await db
    .select({ workspaceId: workspaceMembers.workspaceId })
    .from(workspaceMembers)
    .where(eq(workspaceMembers.userId, userId))
  return rows.map((r) => r.workspaceId)
}

export async function getUserProjectIds(userId: string): Promise<string[]> {
  const workspaceIds = await getUserWorkspaceIds(userId)
  if (workspaceIds.length === 0) return []
  const rows = await db
    .select({ id: projects.id })
    .from(projects)
    .where(inArray(projects.workspaceId, workspaceIds))
  return rows.map((r) => r.id)
}

export async function getUserLabelIds(userId: string): Promise<string[]> {
  const workspaceIds = await getUserWorkspaceIds(userId)
  if (workspaceIds.length === 0) return []
  const rows = await db
    .select({ id: labels.id })
    .from(labels)
    .where(inArray(labels.workspaceId, workspaceIds))
  return rows.map((r) => r.id)
}

export async function getUserIdsInWorkspaces(
  userId: string
): Promise<string[]> {
  const workspaceIds = await getUserWorkspaceIds(userId)
  if (workspaceIds.length === 0) return []
  const rows = await db
    .select({ userId: workspaceMembers.userId })
    .from(workspaceMembers)
    .where(inArray(workspaceMembers.workspaceId, workspaceIds))
  return [...new Set(rows.map((r) => r.userId))]
}

export function buildWhereClause(column: string, ids: string[]): string {
  if (ids.length === 0) {
    return `"${column}" = '00000000-0000-0000-0000-000000000000'`
  }
  const escaped = ids.map((id) => `'${id}'`).join(`,`)
  return `"${column}" IN (${escaped})`
}

export async function assertWorkspaceMember(
  userId: string,
  workspaceId: string,
  requiredRoles?: Array<`owner` | `member`>
): Promise<void> {
  const [member] = await db
    .select()
    .from(workspaceMembers)
    .where(
      and(
        eq(workspaceMembers.workspaceId, workspaceId),
        eq(workspaceMembers.userId, userId)
      )
    )
    .limit(1)

  if (!member) {
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: `Not a member of this workspace`,
    })
  }

  if (requiredRoles && !requiredRoles.includes(member.role)) {
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: `Insufficient role. Required: ${requiredRoles.join(`, `)}`,
    })
  }
}
