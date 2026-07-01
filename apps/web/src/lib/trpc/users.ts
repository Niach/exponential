import { z } from "zod"
import { router, authedProcedure } from "@/lib/trpc"
import { apikeys, users } from "@/db/auth-schema"
import { auth } from "@/lib/auth"
import { getReadableUserIdsInWorkspaces } from "@/lib/workspace-membership"
import { and, desc, eq, inArray } from "drizzle-orm"

export const usersRouter = router({
  listByWorkspaceIds: authedProcedure.query(async ({ ctx }) => {
    // Same email-safe scoping as the users shape: only co-members of
    // workspaces the caller actually joined (not all public workspaces).
    const userIds = await getReadableUserIdsInWorkspaces(ctx.session.user.id)

    if (userIds.length === 0) {
      return { users: [] }
    }

    const userRows = await ctx.db
      .select({ id: users.id, name: users.name, email: users.email })
      .from(users)
      .where(inArray(users.id, userIds))

    return { users: userRows }
  }),

  // ── Personal API keys (expu_) ─────────────────────────────────────────────
  // The user's own long-lived credential for desktop coding sessions + MCP
  // clients: the launcher writes it into the worktree's .mcp.json, and the
  // Better Auth apiKey plugin resolves `Authorization: Bearer expu_…` back to
  // this user. The raw key is returned exactly once at mint time (only a hash
  // is stored); revoke by deleting the row.

  mintPersonalApiKey: authedProcedure
    .input(z.object({ name: z.string().min(1).max(180).optional() }).optional())
    .mutation(async ({ ctx, input }) => {
      const created = await auth.api.createApiKey({
        body: {
          name: (input?.name ?? `Personal key`).slice(0, 180),
          userId: ctx.session.user.id,
          expiresIn: null,
          rateLimitEnabled: false,
          metadata: { kind: `personal` },
        },
      })
      // `key` is the RAW credential — returned exactly once (only a hash is
      // stored). The rest is display metadata so the client can render the new
      // row without a follow-up list call.
      return {
        key: created.key,
        id: created.id,
        name: created.name ?? null,
        start: created.start ?? null,
        prefix: created.prefix ?? null,
        createdAt: created.createdAt,
      }
    }),

  listPersonalApiKeys: authedProcedure.query(async ({ ctx }) => {
    const rows = await ctx.db
      .select({
        id: apikeys.id,
        name: apikeys.name,
        start: apikeys.start,
        prefix: apikeys.prefix,
        createdAt: apikeys.createdAt,
        lastRequest: apikeys.lastRequest,
      })
      .from(apikeys)
      .where(eq(apikeys.referenceId, ctx.session.user.id))
      .orderBy(desc(apikeys.createdAt))
    return { keys: rows }
  }),

  revokePersonalApiKey: authedProcedure
    .input(z.object({ id: z.string().min(1) }))
    .mutation(async ({ ctx, input }) => {
      await ctx.db
        .delete(apikeys)
        .where(
          and(
            eq(apikeys.id, input.id),
            eq(apikeys.referenceId, ctx.session.user.id)
          )
        )
      return { ok: true }
    }),
})
