import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { router, authedProcedure } from "@/lib/trpc"
import { apikeys, users } from "@/db/auth-schema"
import { workspaces, workspaceMembers } from "@/db/schema"
import { auth } from "@/lib/auth"
import { getReadableUserIdsInWorkspaces } from "@/lib/workspace-membership"
import { and, desc, eq, inArray, sql } from "drizzle-orm"

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

  // ── Self-service account deletion ─────────────────────────────────────────
  // App Store guideline 5.1.1(v) requires in-app account deletion when the app
  // supports account creation (email-only deletion is explicitly insufficient).
  // Mirrors admin.deleteUser: the users row delete cascades sessions, accounts,
  // apikeys, memberships, issues/comments authored, fcm tokens, notifications.
  // Additionally removes workspaces where the caller is the ONLY member (their
  // personal workspace + solo workspaces) so no orphaned data survives — the
  // privacy policy promises deletion of "all associated data".
  deleteAccount: authedProcedure
    .input(z.object({ confirm: z.literal(true) }))
    .mutation(async ({ ctx }) => {
      const userId = ctx.session.user.id

      const [me] = await ctx.db
        .select({ isAdmin: users.isAdmin, isAgent: users.isAgent })
        .from(users)
        .where(eq(users.id, userId))
      if (!me) throw new TRPCError({ code: `NOT_FOUND` })
      if (me.isAgent) {
        // Widget-helpdesk bot users own widget-created issues; deleting one
        // cascades those issues away. They also never have interactive
        // sessions — refuse defensively.
        throw new TRPCError({ code: `FORBIDDEN` })
      }
      if (me.isAdmin) {
        const [{ adminCount }] = await ctx.db
          .select({ adminCount: sql<number>`count(*)::int` })
          .from(users)
          .where(eq(users.isAdmin, true))
        if (adminCount <= 1) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `You are the last admin of this instance — promote another admin before deleting your account`,
          })
        }
      }

      await ctx.db.transaction(async (tx) => {
        // Workspaces where the caller is the ONLY owner but OTHER members
        // exist would be orphaned by the user-row cascade: the caller's
        // membership disappears, leaving members with no owner — invites,
        // billing, and settings all become unreachable, violating the
        // last-owner invariant workspaceMembers.remove/updateRole enforce.
        // Fail closed: block deletion until ownership is transferred or the
        // other members are removed. (Inside the tx so nothing is deleted
        // when this throws.)
        const stranded = await tx
          .select({ id: workspaceMembers.workspaceId })
          .from(workspaceMembers)
          .groupBy(workspaceMembers.workspaceId)
          .having(
            sql`count(*) > 1 and bool_or(${workspaceMembers.userId} = ${userId} and ${workspaceMembers.role} = 'owner') and bool_and(${workspaceMembers.userId} = ${userId} or ${workspaceMembers.role} <> 'owner')`
          )
        if (stranded.length > 0) {
          const rows = await tx
            .select({ name: workspaces.name })
            .from(workspaces)
            .where(
              inArray(
                workspaces.id,
                stranded.map((w) => w.id)
              )
            )
          const names = rows.map((w) => `"${w.name}"`).join(`, `)
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `You are the only owner of ${names}, which still ${rows.length === 1 ? `has` : `have`} other members — transfer ownership or remove those members before deleting your account`,
          })
        }

        // Workspaces whose entire membership is just this user. bool_and
        // guards a race where someone joins between select and delete; the
        // is_public check keeps the bootstrap feedback board untouchable.
        const solo = await tx
          .select({ id: workspaceMembers.workspaceId })
          .from(workspaceMembers)
          .groupBy(workspaceMembers.workspaceId)
          .having(
            sql`count(*) = 1 and bool_and(${workspaceMembers.userId} = ${userId})`
          )
        if (solo.length > 0) {
          await tx.delete(workspaces).where(
            and(
              inArray(
                workspaces.id,
                solo.map((w) => w.id)
              ),
              eq(workspaces.isPublic, false)
            )
          )
        }
        await tx.delete(users).where(eq(users.id, userId))
      })

      return { ok: true }
    }),
})
