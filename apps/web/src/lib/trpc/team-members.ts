import { z } from "zod"
import { router, authedProcedure } from "@/lib/trpc"
import { issueSubscribers, teamMembers } from "@/db/schema"
import { and, eq } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { assertTeamMember } from "@/lib/team-membership"
import { isUserAdmin } from "@/lib/admin"

// v7: the self-service `join` procedure is gone with public teams —
// membership is invite-only everywhere; public feedback boards are read-only
// for non-members (writes arrive via the embedded widget).

/** Member management is allowed for a team owner OR a global admin. */
async function assertCanManageMembers(userId: string, teamId: string) {
  if (await isUserAdmin(userId)) {
    return
  }
  await assertTeamMember(userId, teamId, [`owner`])
}

export const teamMembersRouter = router({
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
        .from(teamMembers)
        .where(eq(teamMembers.id, input.memberId))
        .limit(1)

      if (!target) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Member not found` })
      }

      await assertCanManageMembers(ctx.session.user.id, target.teamId)

      // A team must always keep at least one owner — block demoting the
      // last one (mirrors the guard in `remove`).
      if (target.role === `owner` && input.role === `member`) {
        const owners = await ctx.db
          .select()
          .from(teamMembers)
          .where(
            and(
              eq(teamMembers.teamId, target.teamId),
              eq(teamMembers.role, `owner`)
            )
          )
        if (owners.length <= 1) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Cannot demote the last owner of a team`,
          })
        }
      }

      const [updated] = await ctx.db
        .update(teamMembers)
        .set({ role: input.role })
        .where(eq(teamMembers.id, input.memberId))
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
        .from(teamMembers)
        .where(eq(teamMembers.id, input.memberId))
        .limit(1)

      if (!target) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Member not found` })
      }

      // Members may always leave a team themselves; removing someone
      // else requires owner/admin rights.
      const isSelfRemove = target.userId === ctx.session.user.id
      if (!isSelfRemove) {
        await assertCanManageMembers(ctx.session.user.id, target.teamId)
      }

      // Prevent removing the last owner
      if (target.role === `owner`) {
        const owners = await ctx.db
          .select()
          .from(teamMembers)
          .where(
            and(
              eq(teamMembers.teamId, target.teamId),
              eq(teamMembers.role, `owner`)
            )
          )
        if (owners.length <= 1) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Cannot remove the last owner of a team`,
          })
        }
      }

      // Membership end = subscription end: also drop the user's
      // issue_subscribers rows in this team so notification fan-out and
      // the team-scoped issue-subscribers shape stop referencing an
      // ex-member. Manual-unsubscribe tombstones go too — a re-invited user
      // starts with a clean slate. (deliver() additionally re-checks live
      // membership, so rows left behind by pre-fix removals are already inert
      // for delivery.)
      await ctx.db.transaction(async (tx) => {
        await tx
          .delete(teamMembers)
          .where(eq(teamMembers.id, input.memberId))
        await tx
          .delete(issueSubscribers)
          .where(
            and(
              eq(issueSubscribers.teamId, target.teamId),
              eq(issueSubscribers.userId, target.userId)
            )
          )
      })

      return { ok: true }
    }),
})
