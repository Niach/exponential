import { z } from "zod"
import { router, authedProcedure } from "@/lib/trpc"
import { db } from "@/db/connection"
import { workspaceMembers } from "@/db/schema"
import { and, eq } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { assertWorkspaceMember } from "@/lib/workspace-membership"

export const workspaceMembersRouter = router({
  updateRole: authedProcedure
    .input(
      z.object({
        memberId: z.string().uuid(),
        role: z.enum([`owner`, `member`]),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const [target] = await db
        .select()
        .from(workspaceMembers)
        .where(eq(workspaceMembers.id, input.memberId))
        .limit(1)

      if (!target) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Member not found` })
      }

      await assertWorkspaceMember(ctx.session.user.id, target.workspaceId, [
        `owner`,
      ])

      const [updated] = await db
        .update(workspaceMembers)
        .set({ role: input.role })
        .where(eq(workspaceMembers.id, input.memberId))
        .returning()

      return { member: updated }
    }),

  remove: authedProcedure
    .input(
      z.object({
        memberId: z.string().uuid(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const [target] = await db
        .select()
        .from(workspaceMembers)
        .where(eq(workspaceMembers.id, input.memberId))
        .limit(1)

      if (!target) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Member not found` })
      }

      const isSelfRemove = target.userId === ctx.session.user.id
      if (!isSelfRemove) {
        await assertWorkspaceMember(ctx.session.user.id, target.workspaceId, [
          `owner`,
        ])
      }

      // Prevent removing the last owner
      if (target.role === `owner`) {
        const owners = await db
          .select()
          .from(workspaceMembers)
          .where(
            and(
              eq(workspaceMembers.workspaceId, target.workspaceId),
              eq(workspaceMembers.role, `owner`)
            )
          )
        if (owners.length <= 1) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Cannot remove the last owner of a workspace`,
          })
        }
      }

      await db
        .delete(workspaceMembers)
        .where(eq(workspaceMembers.id, input.memberId))

      return { success: true }
    }),
})
