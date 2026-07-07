import { z } from "zod"
import { TRPCError } from "@trpc/server"
import {
  router,
  authedProcedure,
  publicProcedure,
  generateTxId,
} from "@/lib/trpc"
import { workspaces, workspaceMembers } from "@/db/schema"
import { eq } from "drizzle-orm"
import { randomBytes } from "crypto"
import { isUserAdmin } from "@/lib/admin"
import {
  createPersonalWorkspace,
  findNonPublicMembership,
} from "@/lib/auth/personal-workspace"
import {
  assertWorkspaceOwner,
  assertNotPublicWorkspace,
  getWorkspaceMember,
} from "@/lib/workspace-membership"
import {
  cancelCreemSubscriptionsBestEffort,
  findActiveSubscriptionsForWorkspaces,
} from "@/lib/billing/creem-subscriptions"

function slugify(input: string): string {
  return input
    .toLowerCase()
    .normalize(`NFKD`)
    .replace(/[̀-ͯ]/g, ``)
    .replace(/[^a-z0-9]+/g, `-`)
    .replace(/^-+|-+$/g, ``)
    .slice(0, 48)
}

type DbOrTx = {
  // eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
  select: typeof import("@/db/connection").db.select
}

async function uniqueSlug(tx: DbOrTx, base: string): Promise<string> {
  const root = slugify(base) || `workspace`
  let candidate = root
  let suffix = 0
  while (suffix < 5) {
    const [existing] = await tx
      .select({ id: workspaces.id })
      .from(workspaces)
      .where(eq(workspaces.slug, candidate))
      .limit(1)
    if (!existing) return candidate
    suffix += 1
    candidate = `${root}-${suffix}`
  }
  return `${root}-${randomBytes(3).toString(`hex`)}`
}

export const workspacesRouter = router({
  ensureDefault: authedProcedure.mutation(async ({ ctx }) => {
    const userId = ctx.session.user.id
    const userName = ctx.session.user.name || `My`

    return await ctx.db.transaction(async (tx) => {
      // Normally the signup hook already created the personal workspace
      // (lib/auth/personal-workspace.ts); this is the self-heal path for
      // legacy accounts. We never pick the public workspace as the user's
      // "default" landing spot.
      const membership = await findNonPublicMembership(tx, userId)

      if (membership) {
        const [workspace] = await tx
          .select()
          .from(workspaces)
          .where(eq(workspaces.id, membership.workspaceId))
          .limit(1)
        return { workspace, txId: 0 }
      }

      const txId = await generateTxId(tx)
      const workspace = await createPersonalWorkspace(tx, {
        userId,
        userName,
      })

      return { workspace, txId }
    })
  }),

  create: authedProcedure
    .input(
      z.object({
        name: z.string().min(1).max(255),
        iconUrl: z.string().url().max(2048).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      // Regular users live in their single auto-created personal workspace and
      // collaborate via invites — only instance admins may create additional
      // workspaces. (ensureDefault is the personal-workspace path and stays
      // open to everyone.)
      if (!(await isUserAdmin(userId))) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `Only instance admins can create workspaces`,
        })
      }

      return await ctx.db.transaction(async (tx) => {
        const slug = await uniqueSlug(tx, input.name)

        const txId = await generateTxId(tx)
        const [workspace] = await tx
          .insert(workspaces)
          .values({
            name: input.name,
            slug,
            iconUrl: input.iconUrl,
          })
          .returning()

        await tx.insert(workspaceMembers).values({
          workspaceId: workspace.id,
          userId,
          role: `owner`,
        })

        return { workspace, txId }
      })
    }),

  // `isPublic`/`publicWritePolicy` are deliberately NOT updatable here: the
  // only public workspace is the bootstrap-created feedback board, and its
  // flags are written directly by the bootstrap. Regular workspaces stay
  // private, always.
  update: authedProcedure
    .input(
      z.object({
        id: z.string().uuid(),
        name: z.string().min(1).max(255).optional(),
        iconUrl: z.string().url().max(2048).nullable().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const { id, ...updates } = input
      await assertWorkspaceOwner(ctx.session.user.id, id)

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [workspace] = await tx
          .update(workspaces)
          .set({ ...updates, updatedAt: new Date() })
          .where(eq(workspaces.id, id))
          .returning()
        return { workspace, txId }
      })
    }),

  delete: authedProcedure
    .input(z.object({ workspaceId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      await assertWorkspaceOwner(ctx.session.user.id, input.workspaceId)

      // Block deletion of the public workspace
      await assertNotPublicWorkspace(input.workspaceId, {
        message: `Cannot delete the public workspace`,
        code: `BAD_REQUEST`,
      })

      // Capture BEFORE the delete: creem_subscriptions.workspace_id goes
      // `set null` when the workspace row is deleted, after which the remote
      // Creem subscription would keep charging with nothing left to find it
      // by (the paying-ghost bug).
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

  // Public read of minimal workspace metadata by slug. Used by the route guard
  // to decide whether anonymous viewing is permitted and whether an authed
  // caller needs the join gate (`membership` is the caller's role, null for
  // anonymous callers and non-members). Returns NOT_FOUND for private
  // workspaces the caller can't access, to avoid leaking existence.
  getBySlug: publicProcedure
    .input(z.object({ slug: z.string().min(1).max(255) }))
    .query(async ({ ctx, input }) => {
      const [workspace] = await ctx.db
        .select({
          id: workspaces.id,
          name: workspaces.name,
          slug: workspaces.slug,
          iconUrl: workspaces.iconUrl,
          isPublic: workspaces.isPublic,
          publicWritePolicy: workspaces.publicWritePolicy,
        })
        .from(workspaces)
        .where(eq(workspaces.slug, input.slug))
        .limit(1)

      if (!workspace) {
        throw new TRPCError({ code: `NOT_FOUND` })
      }

      const userId = ctx.session?.user?.id
      const member = userId
        ? await getWorkspaceMember(userId, workspace.id)
        : undefined

      if (!workspace.isPublic && !member) {
        throw new TRPCError({ code: `NOT_FOUND` })
      }
      return { ...workspace, membership: member?.role ?? null }
    }),
})
