import { TRPCError } from "@trpc/server"
import { z } from "zod"
import { and, asc, eq, gt, isNull } from "drizzle-orm"
import { createHash, randomBytes, randomUUID } from "node:crypto"
import { router, authedProcedure, publicProcedure } from "@/lib/trpc"
import { auth } from "@/lib/auth"
import { apikeys, users } from "@/db/auth-schema"
import {
  issues,
  projects,
  workspaceAgents,
  workspaceMembers,
  workspaces,
} from "@/db/schema"
import { assertWorkspaceMember } from "@/lib/workspace-membership"
import { revokeWorkspaceAgent } from "@/lib/companion-agents"

const SETUP_TOKEN_PREFIX = `expc_`
const SETUP_TOKEN_TTL_MS = 30 * 60 * 1000

const setupTokenSchema = z
  .string()
  .regex(/^expc_[A-Za-z0-9_-]{32,}$/, `Invalid setup token`)

function hashSetupToken(token: string): string {
  return createHash(`sha256`).update(token).digest(`hex`)
}

function generateSetupToken() {
  const token = `${SETUP_TOKEN_PREFIX}${randomBytes(32).toString(`base64url`)}`
  return {
    token,
    hash: hashSetupToken(token),
    expiresAt: new Date(Date.now() + SETUP_TOKEN_TTL_MS),
  }
}

function baseUrlFromRequest(request: Request): string {
  const configured = process.env.BETTER_AUTH_URL?.replace(/\/$/, ``)
  if (configured) return configured
  return new URL(request.url).origin
}

function installCommand(baseUrl: string, setupToken: string): string {
  return `curl -fsSL ${baseUrl}/install/companion.sh | bash -s -- --server ${baseUrl} --setup-token ${setupToken}`
}

async function assertOwner(userId: string, workspaceId: string) {
  await assertWorkspaceMember(userId, workspaceId, [`owner`])
}

async function loadOwnedAgent(
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

async function loadAgentForSessionUser(
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

export const companionRouter = router({
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

  heartbeat: authedProcedure.mutation(async ({ ctx }) => {
    const agent = await loadAgentForSessionUser(ctx.db, ctx.session.user.id)
    const [updated] = await ctx.db
      .update(workspaceAgents)
      .set({ lastSeenAt: new Date() })
      .where(eq(workspaceAgents.id, agent.id))
      .returning({ lastSeenAt: workspaceAgents.lastSeenAt })

    return { ok: true, lastSeenAt: updated?.lastSeenAt ?? null }
  }),

  // `mutation` instead of `query` because the daemon's tRPC client uses POST
  // for everything, and we still want to update `lastSeenAt` on each poll —
  // so a write-shaped semantics fits.
  //
  // `activityCursor` is the daemon's last-seen `issues.updated_at` timestamp.
  // We return any issues currently assigned to this agent that have been
  // updated since the cursor, plus a new cursor value. This is the fallback
  // path for new comments / row updates when the Electric ShapeStream isn't
  // delivering live events (e.g., due to proxy long-poll header stripping).
  pollControl: authedProcedure
    .input(
      z
        .object({
          activityCursor: z.string().datetime().optional(),
        })
        .optional()
    )
    .mutation(async ({ ctx, input }) => {
      const agent = await loadAgentForSessionUser(ctx.db, ctx.session.user.id)
      const now = new Date()
      await ctx.db
        .update(workspaceAgents)
        .set({ lastSeenAt: now })
        .where(eq(workspaceAgents.id, agent.id))

      let activityIssues: Array<{
        id: string
        identifier: string
        title: string
        projectId: string
        assigneeId: string | null
        updatedAt: string
      }> = []
      let nextCursor = input?.activityCursor ?? now.toISOString()

      if (input?.activityCursor) {
        const since = new Date(input.activityCursor)
        const rows = await ctx.db
          .select({
            id: issues.id,
            identifier: issues.identifier,
            title: issues.title,
            projectId: issues.projectId,
            assigneeId: issues.assigneeId,
            archivedAt: issues.archivedAt,
            updatedAt: issues.updatedAt,
          })
          .from(issues)
          .where(
            and(
              eq(issues.assigneeId, agent.userId),
              isNull(issues.archivedAt),
              gt(issues.updatedAt, since)
            )
          )
          .orderBy(asc(issues.updatedAt))
          .limit(50)

        activityIssues = rows.map((row) => ({
          id: row.id,
          identifier: row.identifier,
          title: row.title,
          projectId: row.projectId,
          assigneeId: row.assigneeId,
          updatedAt: row.updatedAt.toISOString(),
        }))

        const lastRow = rows.at(-1)
        if (lastRow) {
          nextCursor = lastRow.updatedAt.toISOString()
        }
      }

      return {
        activity: {
          cursor: nextCursor,
          issues: activityIssues,
        },
      }
    }),

  uninstallSelf: authedProcedure.mutation(async ({ ctx }) => {
    const agent = await loadAgentForSessionUser(ctx.db, ctx.session.user.id)
    await revokeWorkspaceAgent(ctx.db, agent)
    return { ok: true }
  }),

  reportGithubIdentity: authedProcedure
    .input(
      z.object({
        login: z.string().min(1).max(128),
        repos: z
          .array(
            z.object({
              fullName: z
                .string()
                .regex(/^[A-Za-z0-9._-]+\/[A-Za-z0-9._-]+$/),
              defaultBranch: z.string().min(1).max(255),
              private: z.boolean(),
            })
          )
          .max(2000),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const agent = await loadAgentForSessionUser(ctx.db, ctx.session.user.id)
      await ctx.db
        .update(workspaceAgents)
        .set({
          githubUserLogin: input.login,
          githubRepos: input.repos,
          lastSeenAt: new Date(),
        })
        .where(eq(workspaceAgents.id, agent.id))
      return { ok: true, count: input.repos.length }
    }),

  clearGithubIdentity: authedProcedure.mutation(async ({ ctx }) => {
    const agent = await loadAgentForSessionUser(ctx.db, ctx.session.user.id)
    await ctx.db
      .update(workspaceAgents)
      .set({
        githubUserLogin: null,
        githubRepos: null,
        lastSeenAt: new Date(),
      })
      .where(eq(workspaceAgents.id, agent.id))
    return { ok: true }
  }),
})
