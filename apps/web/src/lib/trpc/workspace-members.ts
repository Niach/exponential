import { z } from "zod"
import { router, authedProcedure } from "@/lib/trpc"
import { issueSubscribers, workspaceMembers } from "@/db/schema"
import { and, eq } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { assertWorkspaceMember } from "@/lib/workspace-membership"
import { isUserAdmin } from "@/lib/admin"

// v7: the self-service `join` procedure is gone with public workspaces —
// membership is invite-only everywhere; public feedback boards are read-only
// for non-members (writes arrive via the embedded widget).

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

      await assertCanManageMembers(ctx.session.user.id, target.workspaceId)

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

      // Members may always leave a workspace themselves; removing someone
      // else requires owner/admin rights.
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

      // Membership end = subscription end: also drop the user's
      // issue_subscribers rows in this workspace so notification fan-out and
      // the workspace-scoped issue-subscribers shape stop referencing an
      // ex-member. Manual-unsubscribe tombstones go too — a re-invited user
      // starts with a clean slate. (deliver() additionally re-checks live
      // membership, so rows left behind by pre-fix removals are already inert
      // for delivery.)
      await ctx.db.transaction(async (tx) => {
        await tx
          .delete(workspaceMembers)
          .where(eq(workspaceMembers.id, input.memberId))
        await tx
          .delete(issueSubscribers)
          .where(
            and(
              eq(issueSubscribers.workspaceId, target.workspaceId),
              eq(issueSubscribers.userId, target.userId)
            )
          )
      })

      return { ok: true }
    }),
})
