import { z } from "zod"
import { router, authedProcedure } from "@/lib/trpc"
import { workspaceAgents, workspaceMembers } from "@/db/schema"
import { and, eq } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import {
  assertWorkspaceMember,
  assertNotPublicWorkspace,
} from "@/lib/workspace-membership"
import { revokeWorkspaceAgent } from "@/lib/companion-agents"

export const workspaceMembersRouter = router({
  updateRole: authedProcedure
    .input(
      z.object({
        memberId: z.string().uuid(),
        role: z.enum([`owner`, `member`]),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const [target] = await ctx.db
        .select()
        .from(workspaceMembers)
        .where(eq(workspaceMembers.id, input.memberId))
        .limit(1)

      if (!target) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Member not found` })
      }

      await assertNotPublicWorkspace(target.workspaceId, {
        message: `Membership on the public workspace cannot be modified`,
      })
      await assertWorkspaceMember(ctx.session.user.id, target.workspaceId, [
        `owner`,
      ])

      if (target.role === `agent`) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Agent members are managed from Agent Members`,
        })
      }

      const [updated] = await ctx.db
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
      const [target] = await ctx.db
        .select()
        .from(workspaceMembers)
        .where(eq(workspaceMembers.id, input.memberId))
        .limit(1)

      if (!target) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Member not found` })
      }

      await assertNotPublicWorkspace(target.workspaceId, {
        message: `Membership on the public workspace cannot be modified`,
      })

      if (target.role === `agent`) {
        await assertWorkspaceMember(ctx.session.user.id, target.workspaceId, [
          `owner`,
        ])

        const [agent] = await ctx.db
          .select()
          .from(workspaceAgents)
          .where(
            and(
              eq(workspaceAgents.workspaceId, target.workspaceId),
              eq(workspaceAgents.userId, target.userId)
            )
          )
          .limit(1)

        if (agent) {
          await revokeWorkspaceAgent(ctx.db, agent)
        } else {
          await ctx.db
            .delete(workspaceMembers)
            .where(eq(workspaceMembers.id, input.memberId))
        }

        return { ok: true }
      }

      const isSelfRemove = target.userId === ctx.session.user.id
      if (!isSelfRemove) {
        await assertWorkspaceMember(ctx.session.user.id, target.workspaceId, [
          `owner`,
        ])
      }

      // Prevent removing the last owner
      if (target.role === `owner`) {
        const owners = await ctx.db
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

      await ctx.db
        .delete(workspaceMembers)
        .where(eq(workspaceMembers.id, input.memberId))

      return { ok: true }
    }),
})
