import { z } from "zod"
import { TRPCError } from "@trpc/server"
import {
  router,
  authedProcedure,
  publicProcedure,
  generateTxId,
} from "@/lib/trpc"
import { workspaces, workspaceMembers } from "@/db/schema"
import { publicWritePolicySchema } from "@exp/db-schema/domain"
import { and, eq } from "drizzle-orm"
import { randomBytes } from "crypto"
import {
  assertWorkspaceOwner,
  assertNotPublicWorkspace,
  getWorkspaceMember,
  invalidatePublicWorkspaceCache,
} from "@/lib/workspace-membership"

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
      // Check if user has any non-public workspace memberships. We never
      // pick the public workspace as the user's "default" landing spot.
      const [membership] = await tx
        .select({ workspaceId: workspaceMembers.workspaceId })
        .from(workspaceMembers)
        .innerJoin(
          workspaces,
          eq(workspaces.id, workspaceMembers.workspaceId)
        )
        .where(
          and(
            eq(workspaceMembers.userId, userId),
            eq(workspaces.isPublic, false)
          )
        )
        .limit(1)

      if (membership) {
        const [workspace] = await tx
          .select()
          .from(workspaces)
          .where(eq(workspaces.id, membership.workspaceId))
          .limit(1)
        return { workspace, txId: 0 }
      }

      // Create a new workspace with unique slug
      const slug = `ws-${randomBytes(4).toString(`hex`)}`
      const txId = await generateTxId(tx)
      const [workspace] = await tx
        .insert(workspaces)
        .values({
          name: `${userName}'s Workspace`,
          slug,
        })
        .returning()

      // Add user as owner
      await tx.insert(workspaceMembers).values({
        workspaceId: workspace.id,
        userId,
        role: `owner`,
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

  update: authedProcedure
    .input(
      z.object({
        id: z.string().uuid(),
        name: z.string().min(1).max(255).optional(),
        isPublic: z.boolean().optional(),
        publicWritePolicy: publicWritePolicySchema.optional(),
        iconUrl: z.string().url().max(2048).nullable().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const { id, ...updates } = input
      await assertWorkspaceOwner(ctx.session.user.id, id)

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [workspace] = await tx
          .update(workspaces)
          .set({ ...updates, updatedAt: new Date() })
          .where(eq(workspaces.id, id))
          .returning()
        return { workspace, txId }
      })

      if (input.isPublic !== undefined) {
        invalidatePublicWorkspaceCache()
      }

      return result
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

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx.delete(workspaces).where(eq(workspaces.id, input.workspaceId))
        return { ok: true, txId }
      })
    }),

  // Public read of minimal workspace metadata by slug. Used by the route guard
  // to decide whether anonymous viewing is permitted. Returns NOT_FOUND for
  // private workspaces the caller can't access, to avoid leaking existence.
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
      if (workspace.isPublic) return workspace

      const userId = ctx.session?.user?.id
      if (!userId) {
        throw new TRPCError({ code: `NOT_FOUND` })
      }
      const member = await getWorkspaceMember(userId, workspace.id)
      if (!member) {
        throw new TRPCError({ code: `NOT_FOUND` })
      }
      return workspace
    }),
})
