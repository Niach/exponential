import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, desc, eq, inArray, sql } from "drizzle-orm"
import { router, adminProcedure, generateTxId } from "@/lib/trpc"
import { users, accounts } from "@/db/auth-schema"
import { workspaces, workspaceMembers, projects } from "@/db/schema"
import { getWorkspacePlan } from "@/lib/billing"
import { assertNotPublicWorkspace } from "@/lib/workspace-membership"
import { guardAndCleanupWorkspacesForUserDeletion } from "@/lib/account-deletion"
import {
  cancelCreemSubscriptionsBestEffort,
  findActiveSubscriptionsForUser,
  findActiveSubscriptionsForWorkspaces,
} from "@/lib/billing/creem-subscriptions"
import type { db as Database } from "@/db/connection"

export const adminRouter = router({
  listUsers: adminProcedure.query(async ({ ctx }) => {
    const rows = await ctx.db
      .select({
        id: users.id,
        name: users.name,
        email: users.email,
        image: users.image,
        isAdmin: users.isAdmin,
        createdAt: users.createdAt,
        workspaceCount: sql<number>`count(distinct ${workspaceMembers.workspaceId})::int`,
        providers: sql<string[]>`coalesce(array_agg(distinct ${accounts.providerId}) filter (where ${accounts.providerId} is not null), '{}')`,
      })
      .from(users)
      .where(eq(users.isAgent, false))
      .leftJoin(workspaceMembers, eq(workspaceMembers.userId, users.id))
      .leftJoin(accounts, eq(accounts.userId, users.id))
      .groupBy(users.id)
      .orderBy(desc(users.createdAt))

    return rows
  }),

  setUserAdmin: adminProcedure
    .input(
      z.object({
        userId: z.string(),
        isAdmin: z.boolean(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      // Block demoting the last admin (including self).
      if (!input.isAdmin) {
        const [{ adminCount }] = await ctx.db
          .select({ adminCount: sql<number>`count(*)::int` })
          .from(users)
          .where(eq(users.isAdmin, true))
        if (
          adminCount <= 1 &&
          (await isTargetAdmin(ctx.db, input.userId))
        ) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Cannot demote the last admin`,
          })
        }
      }

      await ctx.db
        .update(users)
        .set({ isAdmin: input.isAdmin, updatedAt: new Date() })
        .where(eq(users.id, input.userId))

      return { ok: true }
    }),

  deleteUser: adminProcedure
    .input(z.object({ userId: z.string() }))
    .mutation(async ({ ctx, input }) => {
      if (input.userId === ctx.session.user.id) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Cannot delete yourself`,
        })
      }

      // Block deleting the last admin.
      if (await isTargetAdmin(ctx.db, input.userId)) {
        const [{ adminCount }] = await ctx.db
          .select({ adminCount: sql<number>`count(*)::int` })
          .from(users)
          .where(eq(users.isAdmin, true))
        if (adminCount <= 1) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Cannot delete the last admin`,
          })
        }
      }

      // Subscriptions the user purchased, captured BEFORE the delete — the
      // buyer FK cascades with the users row, after which the remote Creem
      // subscription would keep charging with nothing left to find it by.
      const doomedSubscriptions = await findActiveSubscriptionsForUser(
        input.userId
      )

      await ctx.db.transaction(async (tx) => {
        // Same orphan safety as users.deleteAccount (lib/account-deletion.ts):
        // fail closed when the user is the sole owner of a workspace that
        // still has other members — an admin delete must not silently strand
        // a team — and delete workspaces where they are the only member.
        await guardAndCleanupWorkspacesForUserDeletion(tx, input.userId, `admin`)
        await tx.delete(users).where(eq(users.id, input.userId))
      })

      // Best-effort AFTER commit: a Creem API failure logs loudly but never
      // leaves the user half-deleted.
      await cancelCreemSubscriptionsBestEffort(doomedSubscriptions)

      return { ok: true }
    }),

  listWorkspaces: adminProcedure.query(async ({ ctx }) => {
    const wsRows = await ctx.db
      .select({
        id: workspaces.id,
        name: workspaces.name,
        slug: workspaces.slug,
        createdAt: workspaces.createdAt,
      })
      .from(workspaces)
      .orderBy(desc(workspaces.createdAt))

    if (wsRows.length === 0) return []

    const ids = wsRows.map((w) => w.id)

    const [memberRows, projectRows, ownerRows] = await Promise.all([
      ctx.db
        .select({
          workspaceId: workspaceMembers.workspaceId,
          count: sql<number>`count(*)::int`,
        })
        .from(workspaceMembers)
        .where(inArray(workspaceMembers.workspaceId, ids))
        .groupBy(workspaceMembers.workspaceId),
      ctx.db
        .select({
          workspaceId: projects.workspaceId,
          count: sql<number>`count(*)::int`,
        })
        .from(projects)
        .where(inArray(projects.workspaceId, ids))
        .groupBy(projects.workspaceId),
      ctx.db
        .select({
          workspaceId: workspaceMembers.workspaceId,
          userId: users.id,
          name: users.name,
          email: users.email,
        })
        .from(workspaceMembers)
        .innerJoin(users, eq(users.id, workspaceMembers.userId))
        .where(
          and(
            inArray(workspaceMembers.workspaceId, ids),
            eq(workspaceMembers.role, `owner`)
          )
        ),
    ])

    const memberCounts = new Map(memberRows.map((r) => [r.workspaceId, r.count]))
    const projectCounts = new Map(projectRows.map((r) => [r.workspaceId, r.count]))
    const ownersByWs = new Map<
      string,
      { id: string; name: string; email: string }[]
    >()
    for (const o of ownerRows) {
      const list = ownersByWs.get(o.workspaceId) ?? []
      list.push({ id: o.userId, name: o.name, email: o.email })
      ownersByWs.set(o.workspaceId, list)
    }

    const plans = await Promise.all(
      wsRows.map(async (w) => {
        const { plan } = await getWorkspacePlan(w.id)
        return [w.id, plan] as const
      })
    )
    const planMap = new Map(plans)

    return wsRows.map((w) => ({
      ...w,
      plan: planMap.get(w.id) ?? `free`,
      memberCount: memberCounts.get(w.id) ?? 0,
      projectCount: projectCounts.get(w.id) ?? 0,
      owners: ownersByWs.get(w.id) ?? [],
    }))
  }),

  deleteWorkspace: adminProcedure
    .input(z.object({ workspaceId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      await assertNotPublicWorkspace(input.workspaceId, {
        message: `The public workspace cannot be deleted`,
        code: `BAD_REQUEST`,
      })

      // Capture BEFORE the delete: creem_subscriptions.workspace_id goes
      // `set null` when the workspace row is deleted.
      const doomedSubscriptions = await findActiveSubscriptionsForWorkspaces([
        input.workspaceId,
      ])

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx.delete(workspaces).where(eq(workspaces.id, input.workspaceId))
        return { ok: true, txId }
      })

      // Best-effort AFTER commit: a Creem API failure logs loudly but never
      // leaves the workspace half-deleted.
      await cancelCreemSubscriptionsBestEffort(doomedSubscriptions)

      return result
    }),
})

async function isTargetAdmin(
  db: typeof Database,
  userId: string
): Promise<boolean> {
  const [u] = await db
    .select({ isAdmin: users.isAdmin })
    .from(users)
    .where(eq(users.id, userId))
    .limit(1)
  return Boolean(u?.isAdmin)
}
