import { z } from "zod"
import { router, authedProcedure } from "@/lib/trpc"
import { devices, issues, issueSubscribers, teamMembers } from "@/db/schema"
import { and, eq, sql } from "drizzle-orm"
import { TRPCError } from "@trpc/server"
import { assertTeamMember } from "@/lib/team-membership"
import { invalidateMembershipCaches } from "@/lib/auth/membership-cache"
import { isUserAdmin } from "@/lib/admin"
import { endForeignHostedSessions } from "@/lib/coding-session-kill"

// v7: the self-service `join` procedure is gone with public teams —
// membership is invite-only everywhere; the only anonymous write path is
// the embedded widget (non-members have no read access at all).

/** Member management is allowed for a team owner OR a global admin. */
async function assertCanManageMembers(userId: string, teamId: string) {
  if (await isUserAdmin(userId)) {
    return
  }
  await assertTeamMember(userId, teamId, [`owner`])
}

type Tx = Parameters<
  // eslint-disable-next-line quotes
  Parameters<typeof import("@/db/connection").db.transaction>[0]
>[0]

// REV-23: serialize ALL owner-affecting membership writes for a team on one
// advisory lock, taken as the FIRST statement of the write transaction.
// Nothing in the DB enforces "a team has ≥1 owner", so the last-owner guard
// below is the only line of defense — and as a bare check-then-act it raced:
// two concurrent demotes/removes of the last two owners each counted 2 owner
// rows, both passed, and the team ended ownerless (recoverable only by a
// global admin). One team-scoped lock (statuses.ts pattern) beats FOR UPDATE
// row locks here: demote and remove would otherwise lock target-row-then-
// owner-rows in opposite member order and deadlock. xact-scoped — released
// at commit.
async function lockTeamMembers(tx: Tx, teamId: string): Promise<void> {
  await tx.execute(
    sql`SELECT pg_advisory_xact_lock(hashtext(${`team_members:${teamId}`}))`
  )
}

/**
 * Under the team lock: re-read the target row (the pre-lock snapshot may be
 * stale by the time the lock is granted) and, when the write would end an
 * ownership, verify another owner remains. Returns the fresh target row.
 */
async function assertNotLastOwner(
  tx: Tx,
  memberId: string,
  endsOwnership: (freshRole: string) => boolean,
  message: string
) {
  const [fresh] = await tx
    .select()
    .from(teamMembers)
    .where(eq(teamMembers.id, memberId))
    .limit(1)
  if (!fresh) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Member not found` })
  }
  if (endsOwnership(fresh.role)) {
    const owners = await tx
      .select()
      .from(teamMembers)
      .where(
        and(
          eq(teamMembers.teamId, fresh.teamId),
          eq(teamMembers.role, `owner`)
        )
      )
    if (owners.length <= 1) {
      throw new TRPCError({ code: `BAD_REQUEST`, message })
    }
  }
  return fresh
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
      // last one (mirrors the guard in `remove`). Guard + write share one
      // team-locked transaction (REV-23): checked outside it, two
      // concurrent demotes of the last two owners both passed.
      const updated = await ctx.db.transaction(async (tx) => {
        await lockTeamMembers(tx, target.teamId)
        await assertNotLastOwner(
          tx,
          input.memberId,
          (freshRole) => freshRole === `owner` && input.role === `member`,
          `Cannot demote the last owner of a team`
        )
        const [row] = await tx
          .update(teamMembers)
          .set({ role: input.role })
          .where(eq(teamMembers.id, input.memberId))
          .returning()
        return row
      })

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

      // Membership end = subscription end: also drop the user's
      // issue_subscribers rows in this team so notification fan-out and
      // the team-scoped issue-subscribers shape stop referencing an
      // ex-member. Manual-unsubscribe tombstones go too — a re-invited user
      // starts with a clean slate. (deliver() additionally re-checks live
      // membership, so rows left behind by pre-fix removals are already inert
      // for delivery.)
      // Membership end = assignment end too (REV2-28): an ex-member's `users`
      // row stops syncing to the remaining members, so a left-behind
      // assignee_id renders as the unassigned placeholder on every client —
      // an invisible ghost assignee no one can see or clear. It is also a
      // state the write side refuses to create (assertAssigneeInTeam). No
      // assignee_changed events: this is bulk offboarding hygiene, not an
      // editorial change by the remover.
      let clearedDeviceShare = false
      await ctx.db.transaction(async (tx) => {
        await lockTeamMembers(tx, target.teamId)
        // Prevent removing the last owner — inside the team-locked
        // transaction (REV-23), so a concurrent remove/demote of the other
        // owner serializes and the loser fails this count.
        await assertNotLastOwner(
          tx,
          input.memberId,
          (freshRole) => freshRole === `owner`,
          `Cannot remove the last owner of a team`
        )
        await tx.delete(teamMembers).where(eq(teamMembers.id, input.memberId))
        await tx
          .delete(issueSubscribers)
          .where(
            and(
              eq(issueSubscribers.teamId, target.teamId),
              eq(issueSubscribers.userId, target.userId)
            )
          )
        await tx
          .update(issues)
          .set({ assigneeId: null })
          .where(
            and(
              eq(issues.teamId, target.teamId),
              eq(issues.assigneeId, target.userId)
            )
          )
        // Membership end = share end (EXP-481). The devices/device_worktrees
        // shapes scope on shared_team_id single-table — an Electric where
        // clause cannot re-check membership the way devices.list's join does,
        // so an ex-member's shared box would keep streaming to the team.
        // Clearing here (trigger #13 heals the worktree mirrors) makes the
        // membership boundary hold; list/resolveTargetDevice keep their joins
        // as belt-and-braces for pre-existing rows.
        const cleared = await tx
          .update(devices)
          .set({ sharedTeamId: null, updatedAt: new Date() })
          .where(
            and(
              eq(devices.userId, target.userId),
              eq(devices.sharedTeamId, target.teamId)
            )
          )
          .returning({ id: devices.id })
        clearedDeviceShare = cleared.length > 0
      })
      // Post-commit (never inside the tx — a concurrent shape renewal would
      // repopulate the cache with pre-commit membership).
      invalidateMembershipCaches()
      // The share was the consent that let teammates run sessions on the
      // ex-member's box — end any still-live foreign-hosted runs (EXP-445
      // semantics, same post-write ordering as devices.setShared).
      if (clearedDeviceShare) {
        await endForeignHostedSessions(target.userId, target.teamId)
      }

      return { ok: true }
    }),
})
