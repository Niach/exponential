import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { workspaces, workspaceMembers } from "@/db/schema"
import { and, eq } from "drizzle-orm"
import { randomBytes } from "crypto"
import { assertWorkspaceOwner } from "@/lib/workspace-membership"

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

  update: authedProcedure
    .input(
      z.object({
        id: z.string().uuid(),
        name: z.string().min(1).max(255).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const { id, ...updates } = input
      const [target] = await ctx.db
        .select({ isPublic: workspaces.isPublic })
        .from(workspaces)
        .where(eq(workspaces.id, id))
        .limit(1)
      if (target?.isPublic) {
        throw new TRPCError({
          code: `FORBIDDEN`,
          message: `The public workspace cannot be renamed`,
        })
      }
      await assertWorkspaceOwner(ctx.session.user.id, id)

      const [workspace] = await ctx.db
        .update(workspaces)
        .set(updates)
        .where(eq(workspaces.id, id))
        .returning()
      return { workspace }
    }),
})
