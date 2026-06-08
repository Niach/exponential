import { z } from "zod"
import { and, eq, isNotNull, sql } from "drizzle-orm"
import { randomUUID } from "node:crypto"
import { authedProcedure } from "@/lib/trpc"
import { users, apikeys } from "@/db/auth-schema"
import {
  githubInstallations,
  issues,
  projects,
  agentRegistrations,
} from "@/db/schema"
import { mintAgentApiKey } from "@/lib/auth/agent-credential"
import { revokeDeviceAgent } from "@/lib/companion-agents"
import { assertOwner, fanOutDeviceMembership, loadOwnedAgent } from "./shared"

export const setupProcedures = {
  // The desktop devices this account has registered. Account-level: a device is
  // a member of every workspace the owner belongs to, so the list is the same
  // regardless of which workspace's settings you opened it from. The workspaceId
  // input only gates the view to that workspace's owner.
  list: authedProcedure
    .input(z.object({ workspaceId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      await assertOwner(ctx.session.user.id, input.workspaceId)

      const rows = await ctx.db
        .select({
          id: agentRegistrations.id,
          deviceId: agentRegistrations.deviceId,
          userId: agentRegistrations.userId,
          name: agentRegistrations.name,
          lastSeenAt: agentRegistrations.lastSeenAt,
          createdAt: agentRegistrations.createdAt,
          updatedAt: agentRegistrations.updatedAt,
          ownerName: users.name,
        })
        .from(agentRegistrations)
        .innerJoin(users, eq(users.id, agentRegistrations.ownerUserId))
        .where(eq(agentRegistrations.ownerUserId, ctx.session.user.id))

      return { agents: rows }
    }),

  // Server-computed signals backing the "Set up coding agent" checklist, shared
  // by web and desktop so both render identical state. Several signals are not
  // in client Electric collections (github_installations / users.isAgent), which
  // is why this is a query rather than a live useLiveQuery.
  setupStatus: authedProcedure
    .input(z.object({ workspaceId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      await assertOwner(ctx.session.user.id, input.workspaceId)
      const userId = ctx.session.user.id

      const [agents, github, allProjects, repoProjects, agentIssue, me] =
        await Promise.all([
          // Device registration is account-level, not per-workspace.
          ctx.db
            .select({
              count: sql<number>`count(*)::int`,
              seen: sql<number>`count(${agentRegistrations.lastSeenAt})::int`,
            })
            .from(agentRegistrations)
            .where(eq(agentRegistrations.ownerUserId, userId)),
          ctx.db
            .select({ count: sql<number>`count(*)::int` })
            .from(githubInstallations)
            .where(eq(githubInstallations.userId, userId)),
          ctx.db
            .select({ count: sql<number>`count(*)::int` })
            .from(projects)
            .where(eq(projects.workspaceId, input.workspaceId)),
          ctx.db
            .select({ count: sql<number>`count(*)::int` })
            .from(projects)
            .where(
              and(
                eq(projects.workspaceId, input.workspaceId),
                isNotNull(projects.githubRepo)
              )
            ),
          ctx.db
            .select({ count: sql<number>`count(*)::int` })
            .from(issues)
            .innerJoin(projects, eq(projects.id, issues.projectId))
            .leftJoin(users, eq(users.id, issues.assigneeId))
            .where(
              and(
                eq(projects.workspaceId, input.workspaceId),
                sql`(${users.isAgent} = true OR ${issues.agentPlanState} IS NOT NULL)`
              )
            ),
          ctx.db
            .select({ dismissedAt: users.setupChecklistDismissedAt })
            .from(users)
            .where(eq(users.id, userId))
            .limit(1),
        ])

      return {
        hasProject: (allProjects[0]?.count ?? 0) > 0,
        machineRegistered: (agents[0]?.count ?? 0) > 0,
        agentSeen: (agents[0]?.seen ?? 0) > 0,
        githubConnected: (github[0]?.count ?? 0) > 0,
        repoLinked: (repoProjects[0]?.count ?? 0) > 0,
        firstIssueAssignedToAgent: (agentIssue[0]?.count ?? 0) > 0,
        dismissed: me[0]?.dismissedAt != null,
      }
    }),

  // Account-level device registration. The signed-in owner (already
  // authenticated in the desktop app) auto-calls this on launch with a stable
  // hardware id. The server creates one synthetic agent user for the device,
  // mints a single long-lived expk_ API key, and fans the device out as an
  // `agent` member of every workspace the owner belongs to — so it's assignable
  // to any issue the owner could be. Idempotent per (owner, device): a re-launch
  // reuses the agent identity and just rotates the key + refreshes membership.
  register: authedProcedure
    .input(
      z.object({
        deviceId: z.string().min(1).max(255),
        name: z.string().min(1).max(255).default(`This device`),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const ownerUserId = ctx.session.user.id
      const now = new Date()

      const [existing] = await ctx.db
        .select({
          id: agentRegistrations.id,
          userId: agentRegistrations.userId,
        })
        .from(agentRegistrations)
        .where(
          and(
            eq(agentRegistrations.ownerUserId, ownerUserId),
            eq(agentRegistrations.deviceId, input.deviceId)
          )
        )
        .limit(1)

      let registrationId: string
      let agentUserId: string

      if (existing) {
        agentUserId = existing.userId
        registrationId = existing.id
      } else {
        agentUserId = randomUUID()
        await ctx.db.insert(users).values({
          id: agentUserId,
          name: input.name,
          email: `agent-${agentUserId}@exponential.local`,
          emailVerified: true,
          image: null,
          isAdmin: false,
          isAgent: true,
          createdAt: now,
          updatedAt: now,
        })
        const [reg] = await ctx.db
          .insert(agentRegistrations)
          .values({
            deviceId: input.deviceId,
            userId: agentUserId,
            ownerUserId,
            name: input.name,
          })
          .returning({ id: agentRegistrations.id })
        registrationId = reg.id
      }

      // Rotate the runtime key: register always returns a fresh, usable key
      // (storage is hashed, so a prior key can't be re-handed-out). Drop any
      // existing keys for this device's agent user first; the FK nulls the
      // registration's apiKeyId, which we re-point below.
      await ctx.db.delete(apikeys).where(eq(apikeys.referenceId, agentUserId))
      const apiKey = await mintAgentApiKey({
        agentUserId,
        deviceName: input.name,
        deviceId: input.deviceId,
      })
      await ctx.db
        .update(agentRegistrations)
        .set({ apiKeyId: apiKey.keyId, name: input.name, updatedAt: now })
        .where(eq(agentRegistrations.id, registrationId))

      const workspaceCount = await fanOutDeviceMembership(
        ctx.db,
        ownerUserId,
        agentUserId
      )

      return {
        agent: { id: registrationId, userId: agentUserId, name: input.name },
        deviceId: input.deviceId,
        // The device credential — `Authorization: Bearer <apiKey>`. Returned
        // once; the app persists it. No refresh flow (long-lived expk_ key).
        apiKey: apiKey.key,
        workspaceCount,
      }
    }),

  // Every desktop device this account has registered (account-level).
  listMine: authedProcedure.query(async ({ ctx }) => {
    const rows = await ctx.db
      .select({
        id: agentRegistrations.id,
        name: agentRegistrations.name,
        deviceId: agentRegistrations.deviceId,
        lastSeenAt: agentRegistrations.lastSeenAt,
      })
      .from(agentRegistrations)
      .where(eq(agentRegistrations.ownerUserId, ctx.session.user.id))

    return { agents: rows }
  }),

  revoke: authedProcedure
    .input(z.object({ agentId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const agent = await loadOwnedAgent(
        ctx.db,
        ctx.session.user.id,
        input.agentId
      )
      await revokeDeviceAgent(ctx.db, agent)

      return { ok: true }
    }),
}
