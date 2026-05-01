import { z } from "zod"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { workspaces, workspaceMembers } from "@/db/schema"
import { eq } from "drizzle-orm"
import { randomBytes } from "crypto"
import { assertWorkspaceOwner } from "@/lib/workspace-membership"

export const workspacesRouter = router({
  ensureDefault: authedProcedure.mutation(async ({ ctx }) => {
    const userId = ctx.session.user.id
    const userName = ctx.session.user.name || `My`

    return await ctx.db.transaction(async (tx) => {
      // Check if user has any workspace memberships
      const memberships = await tx
        .select({ workspaceId: workspaceMembers.workspaceId })
        .from(workspaceMembers)
        .where(eq(workspaceMembers.userId, userId))
        .limit(1)

      if (memberships.length > 0) {
        const [workspace] = await tx
          .select()
          .from(workspaces)
          .where(eq(workspaces.id, memberships[0].workspaceId))
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
      await assertWorkspaceOwner(ctx.session.user.id, id)

      const [workspace] = await ctx.db
        .update(workspaces)
        .set(updates)
        .where(eq(workspaces.id, id))
        .returning()
      return { workspace }
    }),
})
