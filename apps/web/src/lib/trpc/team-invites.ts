import { z } from "zod"
import { router, procedure, authedProcedure, generateTxId } from "@/lib/trpc"
import { teamInvites, teamMembers, teams, users } from "@/db/schema"
import { and, eq, isNull } from "drizzle-orm"
import { randomBytes } from "crypto"
import { TRPCError } from "@trpc/server"
import { assertTeamMember } from "@/lib/team-membership"
import { invalidateMembershipCaches } from "@/lib/auth/membership-cache"
import { assertCanInviteMember } from "@/lib/billing"
import { isUserAdmin } from "@/lib/admin"
import { sendTeamInviteEmail } from "@/lib/email"
import { appBaseUrl } from "@/lib/notification-email-policy"

// Invites are member management, so mint/revoke match assertCanManageMembers
// (team-members.ts): a team owner OR a global instance admin.
async function assertCanManageMembers(userId: string, teamId: string) {
  if (await isUserAdmin(userId)) return
  await assertTeamMember(userId, teamId, [`owner`])
}

// The invite `token` is a single-use BEARER SECRET: accept() is not
// recipient-bound and grants membership at the invite's role, so whoever
// reads a pending token can join (or escalate, for owner invites). It is
// returned exactly once — from `create`, to the owner who minted it — and
// never from `list` (member-visible; relayed verbatim by MCP
// exponential_invites_list) nor from the Electric shape (columns allowlist
// in routes/api/shapes/team-invites.ts).
export const inviteListSelection = {
  id: teamInvites.id,
  teamId: teamInvites.teamId,
  invitedById: teamInvites.invitedById,
  role: teamInvites.role,
  email: teamInvites.email,
  acceptedAt: teamInvites.acceptedAt,
  expiresAt: teamInvites.expiresAt,
  createdAt: teamInvites.createdAt,
  updatedAt: teamInvites.updatedAt,
} as const

export const teamInvitesRouter = router({
  create: authedProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
        role: z.enum([`owner`, `member`]).default(`member`),
        // Optional recipient address (EXP-188): persisted for the pending
        // list and used to deliver the invite link by email. Display/delivery
        // metadata only — accept() stays token-bound.
        email: z.string().email().max(255).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      await assertCanManageMembers(ctx.session.user.id, input.teamId)
      await assertCanInviteMember(input.teamId)

      const token = randomBytes(32).toString(`hex`)
      const expiresAt = new Date(Date.now() + 7 * 24 * 60 * 60 * 1000) // 7 days

      const [invite] = await ctx.db
        .insert(teamInvites)
        .values({
          teamId: input.teamId,
          invitedById: ctx.session.user.id,
          role: input.role,
          token,
          email: input.email,
          expiresAt,
        })
        .returning()

      // Email delivery is best-effort AFTER the insert — a transport failure
      // must never roll back the invite (the owner still holds the link and
      // can share it by hand). null = no email requested; false = requested
      // but not delivered (no transport / send error).
      let emailDelivered: boolean | null = null
      if (input.email) {
        try {
          const [team] = await ctx.db
            .select({ name: teams.name })
            .from(teams)
            .where(eq(teams.id, input.teamId))
            .limit(1)
          const result = await sendTeamInviteEmail({
            to: input.email,
            teamName: team?.name ?? `a team`,
            inviterName:
              ctx.session.user.name || ctx.session.user.email || `A teammate`,
            inviteUrl: `${appBaseUrl()}/invite/${token}`,
          })
          emailDelivered = result.delivered
        } catch (err) {
          // Log the invite id, not the recipient address — no PII in server logs.
          console.error(
            `[team-invites] invite email for invite ${invite.id} failed:`,
            err
          )
          emailDelivered = false
        }
      }

      return { invite, token, emailDelivered }
    }),

  accept: authedProcedure
    .input(z.object({ token: z.string() }))
    .mutation(async ({ ctx, input }) => {
      const [precheck] = await ctx.db
        .select({ teamId: teamInvites.teamId })
        .from(teamInvites)
        .where(eq(teamInvites.token, input.token))
        .limit(1)
      if (precheck) {
        await assertCanInviteMember(precheck.teamId)
      }

      const result = await ctx.db.transaction(async (tx) => {
        const [invite] = await tx
          .select()
          .from(teamInvites)
          .where(eq(teamInvites.token, input.token))
          .limit(1)

        if (!invite) {
          throw new TRPCError({
            code: `NOT_FOUND`,
            message: `Invite not found`,
          })
        }

        if (invite.acceptedAt) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Invite has already been used`,
          })
        }

        if (invite.expiresAt < new Date()) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Invite has expired`,
          })
        }

        // Check if already a member
        const [existing] = await tx
          .select()
          .from(teamMembers)
          .where(
            and(
              eq(teamMembers.teamId, invite.teamId),
              eq(teamMembers.userId, ctx.session.user.id)
            )
          )
          .limit(1)

        const [team] = await tx
          .select()
          .from(teams)
          .where(eq(teams.id, invite.teamId))
          .limit(1)

        // Accepting an invite is onboarding evidence (EXP-188): stamp the
        // flag so an invite-link signup skips the first-run wizard — also on
        // the alreadyMember path below. The IS NULL predicate keeps an
        // existing timestamp untouched.
        const now = new Date()
        await tx
          .update(users)
          .set({ onboardingCompletedAt: now, updatedAt: now })
          .where(
            and(
              eq(users.id, ctx.session.user.id),
              isNull(users.onboardingCompletedAt)
            )
          )

        // An existing member must not burn the single-use invite.
        if (existing) {
          return { team, alreadyMember: true }
        }

        // Mark invite as accepted (the acceptedAt IS NULL predicate guards
        // against two concurrent accepts both consuming the invite).
        const accepted = await tx
          .update(teamInvites)
          .set({ acceptedAt: new Date() })
          .where(
            and(
              eq(teamInvites.id, invite.id),
              isNull(teamInvites.acceptedAt)
            )
          )
          .returning({ id: teamInvites.id })

        if (accepted.length === 0) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Invite has already been used`,
          })
        }

        const txId = await generateTxId(tx)

        // Create membership
        await tx.insert(teamMembers).values({
          teamId: invite.teamId,
          userId: ctx.session.user.id,
          role: invite.role,
        })

        return { team, alreadyMember: false, txId }
      })
      // Post-commit (never inside the tx — a concurrent shape renewal would
      // repopulate the cache with pre-commit membership).
      if (!result.alreadyMember) {
        invalidateMembershipCaches()
      }
      return result
    }),

  list: authedProcedure
    .input(z.object({ teamId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      await assertTeamMember(ctx.session.user.id, input.teamId)

      const invites = await ctx.db
        .select(inviteListSelection)
        .from(teamInvites)
        .where(
          and(
            eq(teamInvites.teamId, input.teamId),
            isNull(teamInvites.acceptedAt)
          )
        )

      return { invites }
    }),

  revoke: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const [invite] = await ctx.db
        .select()
        .from(teamInvites)
        .where(eq(teamInvites.id, input.id))
        .limit(1)

      if (!invite) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Invite not found` })
      }

      await assertCanManageMembers(ctx.session.user.id, invite.teamId)

      await ctx.db.delete(teamInvites).where(eq(teamInvites.id, input.id))

      return { ok: true }
    }),

  getByToken: procedure
    .input(z.object({ token: z.string() }))
    .query(async ({ ctx, input }) => {
      const [invite] = await ctx.db
        .select({
          id: teamInvites.id,
          teamId: teamInvites.teamId,
          role: teamInvites.role,
          acceptedAt: teamInvites.acceptedAt,
          expiresAt: teamInvites.expiresAt,
          teamName: teams.name,
        })
        .from(teamInvites)
        .innerJoin(teams, eq(teamInvites.teamId, teams.id))
        .where(eq(teamInvites.token, input.token))
        .limit(1)

      if (!invite) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `Invite not found` })
      }

      return { invite }
    }),
})
