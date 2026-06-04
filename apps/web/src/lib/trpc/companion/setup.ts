import { TRPCError } from "@trpc/server"
import { z } from "zod"
import { eq } from "drizzle-orm"
import { randomUUID } from "node:crypto"
import { authedProcedure } from "@/lib/trpc"
import { users } from "@/db/auth-schema"
import {
  projects,
  workspaceAgents,
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
          id: workspaceAgents.id,
          workspaceId: workspaceAgents.workspaceId,
          userId: workspaceAgents.userId,
          ownerUserId: workspaceAgents.ownerUserId,
          name: workspaceAgents.name,
          lastSeenAt: workspaceAgents.lastSeenAt,
          githubUserLogin: workspaceAgents.githubUserLogin,
          githubRepos: workspaceAgents.githubRepos,
          createdAt: workspaceAgents.createdAt,
          updatedAt: workspaceAgents.updatedAt,
          email: users.email,
          ownerName: users.name,
        })
        .from(workspaceAgents)
        .innerJoin(users, eq(users.id, workspaceAgents.userId))
        .where(eq(workspaceAgents.workspaceId, input.workspaceId))

      return { agents: rows }
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

      const agentUserId = randomUUID()
      const email = `agent-${agentUserId}@exponential.local`
      const ownerUserId = ctx.session.user.id
      const now = new Date()
      const baseUrl = baseUrlFromRequest(ctx.request)

      const result = await ctx.db.transaction(async (tx) => {
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

        await tx.insert(workspaceMembers).values({
          workspaceId: input.workspaceId,
          userId: agentUserId,
          role: `agent`,
        })

        const credential = await mintAgentCredential(tx, {
          agentUserId,
          agentName: input.name,
          baseUrl,
        })

        const [agent] = await tx
          .insert(workspaceAgents)
          .values({
            workspaceId: input.workspaceId,
            userId: agentUserId,
            ownerUserId,
            name: input.name,
            oauthClientId: credential.clientId,
          })
          .returning()

        return { agent, credential }
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
          userId: agentUserId,
          name: input.name,
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
        oauth: {
          githubClientId:
            process.env.EXPONENTIAL_GITHUB_OAUTH_CLIENT_ID || null,
        },
      }
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
