import { z } from "zod"
import { router, authedProcedure } from "@/lib/trpc"
import { agentRegistrations, workspaceMembers } from "@/db/schema"
import { and, eq } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import {
  assertWorkspaceMember,
  assertNotPublicWorkspace,
} from "@/lib/workspace-membership"
import { isUserAdmin } from "@/lib/admin"
import { revokeWorkspaceAgent } from "@/lib/companion-agents"

/** Member management is allowed for a workspace owner OR a global admin. */
async function assertCanManageMembers(userId: string, workspaceId: string) {
  if (await isUserAdmin(userId)) {
    return
  }
  await assertWorkspaceMember(userId, workspaceId, [`owner`])
}

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
      await assertCanManageMembers(ctx.session.user.id, target.workspaceId)

      if (target.role === `agent`) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Agent members are managed from Agent Members`,
        })
      }

      // A workspace must always keep at least one owner — block demoting the
      // last one (mirrors the guard in `remove`).
      if (target.role === `owner` && input.role === `member`) {
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
            message: `Cannot demote the last owner of a workspace`,
          })
        }
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
        await assertCanManageMembers(ctx.session.user.id, target.workspaceId)

        const [agent] = await ctx.db
          .select()
          .from(agentRegistrations)
          .where(
            and(
              eq(agentRegistrations.workspaceId, target.workspaceId),
              eq(agentRegistrations.userId, target.userId)
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
        await assertCanManageMembers(ctx.session.user.id, target.workspaceId)
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
