import { TRPCError } from "@trpc/server"
import { z } from "zod"
import { eq } from "drizzle-orm"
import { randomUUID } from "node:crypto"
import { authedProcedure, publicProcedure } from "@/lib/trpc"
import { auth } from "@/lib/auth"
import { apikeys, users } from "@/db/auth-schema"
import {
  projects,
  workspaceAgents,
  workspaceMembers,
  workspaces,
} from "@/db/schema"
import { revokeWorkspaceAgent } from "@/lib/companion-agents"
import {
  assertOwner,
  baseUrlFromRequest,
  generateSetupToken,
  hashSetupToken,
  installCommand,
  loadOwnedAgent,
  setupTokenSchema,
} from "./shared"

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
          name: workspaceAgents.name,
          apiKeyId: workspaceAgents.apiKeyId,
          setupTokenExpiresAt: workspaceAgents.setupTokenExpiresAt,
          setupTokenConsumedAt: workspaceAgents.setupTokenConsumedAt,
          lastSeenAt: workspaceAgents.lastSeenAt,
          githubUserLogin: workspaceAgents.githubUserLogin,
          githubRepos: workspaceAgents.githubRepos,
          createdAt: workspaceAgents.createdAt,
          updatedAt: workspaceAgents.updatedAt,
          email: users.email,
        })
        .from(workspaceAgents)
        .innerJoin(users, eq(users.id, workspaceAgents.userId))
        .where(eq(workspaceAgents.workspaceId, input.workspaceId))

      return { agents: rows }
    }),

  create: authedProcedure
    .input(
      z.object({
        workspaceId: z.string().uuid(),
        name: z.string().min(1).max(255).default(`Companion`),
      })
    )
    .mutation(async ({ ctx, input }) => {
      await assertOwner(ctx.session.user.id, input.workspaceId)

      const [workspace] = await ctx.db
        .select({ id: workspaces.id, slug: workspaces.slug })
        .from(workspaces)
        .where(eq(workspaces.id, input.workspaceId))
        .limit(1)
      if (!workspace) {
        throw new TRPCError({
          code: `NOT_FOUND`,
          message: `Workspace not found`,
        })
      }

      const setup = generateSetupToken()
      const agentUserId = randomUUID()
      const email = `agent-${agentUserId}@exponential.local`
      const now = new Date()

      const agent = await ctx.db.transaction(async (tx) => {
        await tx.insert(users).values({
          id: agentUserId,
          name: input.name,
          email,
          emailVerified: true,
          image: null,
          isAdmin: false,
          createdAt: now,
          updatedAt: now,
        })

        await tx.insert(workspaceMembers).values({
          workspaceId: input.workspaceId,
          userId: agentUserId,
          role: `agent`,
        })

        const [created] = await tx
          .insert(workspaceAgents)
          .values({
            workspaceId: input.workspaceId,
            userId: agentUserId,
            name: input.name,
            setupTokenHash: setup.hash,
            setupTokenExpiresAt: setup.expiresAt,
          })
          .returning()

        return created
      })

      const baseUrl = baseUrlFromRequest(ctx.request)
      return {
        agent,
        setupToken: setup.token,
        installCommand: installCommand(baseUrl, setup.token),
      }
    }),

  regenerateSetup: authedProcedure
    .input(z.object({ agentId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const agent = await loadOwnedAgent(
        ctx.db,
        ctx.session.user.id,
        input.agentId
      )
      const setup = generateSetupToken()

      await ctx.db.transaction(async (tx) => {
        if (agent.apiKeyId) {
          await tx.delete(apikeys).where(eq(apikeys.id, agent.apiKeyId))
        }
        await tx
          .update(workspaceAgents)
          .set({
            setupTokenHash: setup.hash,
            setupTokenExpiresAt: setup.expiresAt,
            setupTokenConsumedAt: null,
            apiKeyId: null,
            lastSeenAt: null,
          })
          .where(eq(workspaceAgents.id, agent.id))
      })

      const baseUrl = baseUrlFromRequest(ctx.request)
      return {
        setupToken: setup.token,
        installCommand: installCommand(baseUrl, setup.token),
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

  claimSetup: publicProcedure
    .input(z.object({ setupToken: setupTokenSchema }))
    .mutation(async ({ ctx, input }) => {
      const setupTokenHash = hashSetupToken(input.setupToken)
      const [agent] = await ctx.db
        .select({
          id: workspaceAgents.id,
          workspaceId: workspaceAgents.workspaceId,
          userId: workspaceAgents.userId,
          name: workspaceAgents.name,
          setupTokenExpiresAt: workspaceAgents.setupTokenExpiresAt,
          setupTokenConsumedAt: workspaceAgents.setupTokenConsumedAt,
          workspaceSlug: workspaces.slug,
          workspaceName: workspaces.name,
        })
        .from(workspaceAgents)
        .innerJoin(workspaces, eq(workspaces.id, workspaceAgents.workspaceId))
        .where(eq(workspaceAgents.setupTokenHash, setupTokenHash))
        .limit(1)

      if (!agent) {
        throw new TRPCError({
          code: `NOT_FOUND`,
          message: `Setup token not found`,
        })
      }
      if (agent.setupTokenConsumedAt) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Setup token has already been used`,
        })
      }
      if (agent.setupTokenExpiresAt < new Date()) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Setup token has expired`,
        })
      }

      const apiKey = await auth.api.createApiKey({
        body: {
          name: `Companion: ${agent.name}`,
          userId: agent.userId,
          expiresIn: null,
          rateLimitEnabled: false,
          metadata: {
            kind: `companion`,
            agentId: agent.id,
            workspaceId: agent.workspaceId,
          },
        },
      })

      await ctx.db
        .update(workspaceAgents)
        .set({
          setupTokenConsumedAt: new Date(),
          apiKeyId: apiKey.id,
        })
        .where(eq(workspaceAgents.id, agent.id))

      const projectRows = await ctx.db
        .select({
          id: projects.id,
          name: projects.name,
          slug: projects.slug,
          prefix: projects.prefix,
        })
        .from(projects)
        .where(eq(projects.workspaceId, agent.workspaceId))

      return {
        apiKey: apiKey.key,
        agent: {
          id: agent.id,
          userId: agent.userId,
          name: agent.name,
        },
        workspace: {
          id: agent.workspaceId,
          slug: agent.workspaceSlug,
          name: agent.workspaceName,
        },
        projects: projectRows,
        oauth: {
          // Daemon stores this in its config.toml and uses it for the
          // GitHub device-flow login. Self-hosters set the env var to
          // their own OAuth App's Client ID; null = not configured.
          githubClientId: process.env.EXPONENTIAL_GITHUB_OAUTH_CLIENT_ID || null,
        },
      }
    }),
}
