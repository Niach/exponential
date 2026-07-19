import { z } from "zod"
import { router, procedure, authedProcedure, generateTxId } from "@/lib/trpc"
import { teamInvites, teamMembers, teams } from "@/db/schema"
import { and, eq, isNull } from "drizzle-orm"
import { randomBytes } from "crypto"
import { TRPCError } from "@trpc/server"
import { assertTeamMember } from "@/lib/team-membership"
import { assertCanInviteMember } from "@/lib/billing"
import { isUserAdmin } from "@/lib/admin"

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
          expiresAt,
        })
        .returning()

      return { invite, token }
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

      return await ctx.db.transaction(async (tx) => {
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
