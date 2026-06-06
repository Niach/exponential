import { and, eq, inArray } from "drizzle-orm"
import { apikeys, oauthApplications } from "@/db/auth-schema"
import {
  issues,
  projects,
  agentRegistrations,
  workspaceMembers,
} from "@/db/schema"

interface WorkspaceAgentRecord {
  id: string
  workspaceId: string
  userId: string
  apiKeyId: string | null
  oauthClientId: string | null
}

export async function revokeWorkspaceAgent(
  // eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
  db: typeof import("@/db/connection").db,
  agent: WorkspaceAgentRecord
) {
  const projectRows = await db
    .select({ id: projects.id })
    .from(projects)
    .where(eq(projects.workspaceId, agent.workspaceId))

  await db.transaction(async (tx) => {
    // Legacy expk_ key (pre-OAuth agents).
    if (agent.apiKeyId) {
      await tx.delete(apikeys).where(eq(apikeys.id, agent.apiKeyId))
    }
    // The OAuth client; its access/refresh tokens cascade-delete with it.
    if (agent.oauthClientId) {
      await tx
        .delete(oauthApplications)
        .where(eq(oauthApplications.clientId, agent.oauthClientId))
    }

    if (projectRows.length > 0) {
      await tx
        .update(issues)
        .set({ assigneeId: null })
        .where(
          and(
            inArray(
              issues.projectId,
              projectRows.map((project) => project.id)
            ),
            eq(issues.assigneeId, agent.userId)
          )
        )
    }

    await tx
      .delete(workspaceMembers)
      .where(
        and(
          eq(workspaceMembers.workspaceId, agent.workspaceId),
          eq(workspaceMembers.userId, agent.userId)
        )
      )
    await tx.delete(agentRegistrations).where(eq(agentRegistrations.id, agent.id))
  })
}
