import { TRPCError } from "@trpc/server"
import { z } from "zod"
import { and, eq, isNotNull, sql } from "drizzle-orm"
import { randomUUID } from "node:crypto"
import { authedProcedure } from "@/lib/trpc"
import { users } from "@/db/auth-schema"
import {
  githubInstallations,
  issues,
  projects,
  agentRegistrations,
  workspaceMembers,
  workspaces,
} from "@/db/schema"
import { mintAgentCredential } from "@/lib/auth/agent-credential"
import { revokeWorkspaceAgent } from "@/lib/companion-agents"
import { assertOwner, baseUrlFromRequest, loadOwnedAgent } from "./shared"

export const setupProcedures = {
  list: authedProcedure
    .input(z.object({ workspaceId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      await assertOwner(ctx.session.user.id, input.workspaceId)

      const rows = await ctx.db
        .select({
          id: agentRegistrations.id,
          workspaceId: agentRegistrations.workspaceId,
          userId: agentRegistrations.userId,
          ownerUserId: agentRegistrations.ownerUserId,
          name: agentRegistrations.name,
          lastSeenAt: agentRegistrations.lastSeenAt,
          createdAt: agentRegistrations.createdAt,
          updatedAt: agentRegistrations.updatedAt,
          email: users.email,
          ownerName: users.name,
        })
        .from(agentRegistrations)
        .innerJoin(users, eq(users.id, agentRegistrations.userId))
        .where(eq(agentRegistrations.workspaceId, input.workspaceId))

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
          ctx.db
            .select({
              count: sql<number>`count(*)::int`,
              seen: sql<number>`count(${agentRegistrations.lastSeenAt})::int`,
            })
            .from(agentRegistrations)
            .where(
              and(
                eq(agentRegistrations.workspaceId, input.workspaceId),
                eq(agentRegistrations.ownerUserId, userId)
              )
            ),
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

  // Human-session-authorized registration. The logged-in owner (already
  // authenticated in the desktop/mac app) calls this; the server creates the
  // agent sub-identity (a hidden users row owned by the caller) and mints a
  // refreshable OAuth credential for it. No setup token, no public claim step.
  register: authedProcedure
    .input(
      z.object({
        workspaceId: z.string().uuid(),
        name: z.string().min(1).max(255).default(`Agent`),
      })
    )
    .mutation(async ({ ctx, input }) => {
      await assertOwner(ctx.session.user.id, input.workspaceId)

      const [workspace] = await ctx.db
        .select({
          id: workspaces.id,
          slug: workspaces.slug,
          name: workspaces.name,
        })
        .from(workspaces)
        .where(eq(workspaces.id, input.workspaceId))
        .limit(1)
      if (!workspace) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Workspace not found` })
      }

      const ownerUserId = ctx.session.user.id
      const now = new Date()
      const baseUrl = baseUrlFromRequest(ctx.request)

      // Idempotent re-register: if this owner already has an agent in THIS
      // workspace, reuse that identity and just mint a fresh credential —
      // re-registering the same machine shouldn't stack duplicate agents.
      const [existingAgent] = await ctx.db
        .select({
          id: agentRegistrations.id,
          userId: agentRegistrations.userId,
          name: agentRegistrations.name,
        })
        .from(agentRegistrations)
        .where(
          and(
            eq(agentRegistrations.ownerUserId, ownerUserId),
            eq(agentRegistrations.workspaceId, input.workspaceId)
          )
        )
        .limit(1)

      const result = existingAgent
        ? await ctx.db.transaction(async (tx) => {
            const credential = await mintAgentCredential(tx, {
              agentUserId: existingAgent.userId,
              agentName: existingAgent.name,
              baseUrl,
            })
            // Ensure the agent member row exists (self-heal) + point the agent
            // record at the freshest credential.
            await tx
              .insert(workspaceMembers)
              .values({
                workspaceId: input.workspaceId,
                userId: existingAgent.userId,
                role: `agent`,
              })
              .onConflictDoNothing()
            await tx
              .update(agentRegistrations)
              .set({ oauthClientId: credential.clientId, updatedAt: now })
              .where(eq(agentRegistrations.id, existingAgent.id))
            return {
              agent: {
                id: existingAgent.id,
                userId: existingAgent.userId,
                name: existingAgent.name,
              },
              credential,
            }
          })
        : await ctx.db.transaction(async (tx) => {
            const agentUserId = randomUUID()
            const email = `agent-${agentUserId}@exponential.local`
            await tx.insert(users).values({
              id: agentUserId,
              name: input.name,
              email,
              emailVerified: true,
              image: null,
              isAdmin: false,
              isAgent: true,
              createdAt: now,
              updatedAt: now,
            })

            await tx
              .insert(workspaceMembers)
              .values({
                workspaceId: input.workspaceId,
                userId: agentUserId,
                role: `agent`,
              })
              .onConflictDoNothing()

            const credential = await mintAgentCredential(tx, {
              agentUserId,
              agentName: input.name,
              baseUrl,
            })

            const [agent] = await tx
              .insert(agentRegistrations)
              .values({
                workspaceId: input.workspaceId,
                userId: agentUserId,
                ownerUserId,
                name: input.name,
                oauthClientId: credential.clientId,
              })
              .returning()

            return {
              agent: { id: agent.id, userId: agentUserId, name: input.name },
              credential,
            }
          })

      const projectRows = await ctx.db
        .select({
          id: projects.id,
          name: projects.name,
          slug: projects.slug,
          prefix: projects.prefix,
        })
        .from(projects)
        .where(eq(projects.workspaceId, input.workspaceId))

      return {
        agent: {
          id: result.agent.id,
          userId: result.agent.userId,
          name: result.agent.name,
        },
        workspace: {
          id: workspace.id,
          slug: workspace.slug,
          name: workspace.name,
        },
        projects: projectRows,
        // The refreshable agent credential. The app stores this and presents
        // `Authorization: Bearer <accessToken>` to tRPC/Electric/MCP, refreshing
        // via `POST tokenEndpoint` (grant_type=refresh_token, public client).
        credential: {
          accessToken: result.credential.accessToken,
          refreshToken: result.credential.refreshToken,
          accessTokenExpiresAt:
            result.credential.accessTokenExpiresAt.toISOString(),
          clientId: result.credential.clientId,
          tokenEndpoint: result.credential.tokenEndpoint,
        },
      }
    }),

  // Every agent the caller owns, across ALL their workspaces (not filtered to
  // one). Lets the UI surface an agent that was registered against the wrong
  // workspace (the pre-fix orphan case) so the owner can revoke it.
  listMine: authedProcedure.query(async ({ ctx }) => {
    const rows = await ctx.db
      .select({
        id: agentRegistrations.id,
        name: agentRegistrations.name,
        workspaceId: agentRegistrations.workspaceId,
        lastSeenAt: agentRegistrations.lastSeenAt,
        workspaceName: workspaces.name,
        workspaceSlug: workspaces.slug,
        workspaceIsPublic: workspaces.isPublic,
      })
      .from(agentRegistrations)
      .innerJoin(workspaces, eq(workspaces.id, agentRegistrations.workspaceId))
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
      await revokeWorkspaceAgent(ctx.db, agent)

      return { ok: true }
    }),
}
