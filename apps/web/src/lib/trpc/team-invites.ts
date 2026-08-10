import { z } from "zod"
import { router, procedure, authedProcedure, generateTxId } from "@/lib/trpc"
import {
  emailDeliveries,
  teamInvites,
  teamMembers,
  teams,
  users,
} from "@/db/schema"
import { and, count, eq, gt, isNull, sql } from "drizzle-orm"
import { randomBytes } from "crypto"
import { TRPCError } from "@trpc/server"
import { db } from "@/db/connection"
import { assertTeamMember } from "@/lib/team-membership"
import { invalidateMembershipCaches } from "@/lib/auth/membership-cache"
import { recordConversionEvent } from "@/lib/conversion/events"
import { assertCanInviteMember } from "@/lib/billing"
import { isUserAdmin } from "@/lib/admin"
import { deliveryStatus, sendTeamInviteEmail } from "@/lib/email"
import { appBaseUrl } from "@/lib/notification-email-policy"

// Platform-wide cap on invite EMAILS per recipient address (the invite row
// itself is unaffected — the owner still gets the link to share by hand).
// Closes the invite-bombing vector: without it, anyone with a team could
// direct an unbounded email stream at a stranger's address.
const INVITE_EMAILS_PER_ADDRESS_PER_WEEK = 3

// Sent invite emails to this address across the whole platform in the last 7
// days, from the delivery ledger (suppressed/failed/capped attempts don't
// count against the recipient).
async function countRecentInviteEmails(email: string): Promise<number> {
  const weekAgo = new Date(Date.now() - 7 * 24 * 60 * 60 * 1000)
  const [row] = await db
    .select({ value: count() })
    .from(emailDeliveries)
    .where(
      and(
        eq(emailDeliveries.kind, `team_invite`),
        eq(emailDeliveries.status, `sent`),
        eq(sql`lower(${emailDeliveries.toEmail})`, email.trim().toLowerCase()),
        gt(emailDeliveries.createdAt, weekAgo)
      )
    )
  return row?.value ?? 0
}

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
      // but not delivered (no transport / send error / per-address cap).
      // Every attempt is ledgered in email_deliveries (kind team_invite) so
      // bounces trace per-message.
      let emailDelivered: boolean | null = null
      if (input.email) {
        try {
          const capped =
            (await countRecentInviteEmails(input.email)) >=
            INVITE_EMAILS_PER_ADDRESS_PER_WEEK
          if (capped) {
            emailDelivered = false
            await ctx.db.insert(emailDeliveries).values({
              userId: null,
              toEmail: input.email,
              issueId: null,
              kind: `team_invite`,
              status: `suppressed`,
              provider: null,
              providerMessageId: null,
              sentAt: null,
              error: `per-address invite email cap reached`,
            })
          } else {
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
            await ctx.db.insert(emailDeliveries).values({
              userId: null,
              toEmail: input.email,
              issueId: null,
              kind: `team_invite`,
              status: deliveryStatus(result),
              provider: result.provider,
              providerMessageId: result.messageId,
              subject: result.subject,
              sentAt: result.delivered ? new Date() : null,
            })
          }
        } catch (err) {
          // Log the invite id, not the recipient address — no PII in server logs.
          console.error(
            `[team-invites] invite email for invite ${invite.id} failed:`,
            err
          )
          emailDelivered = false
        }
      }

      await recordConversionEvent(ctx.db, {
        name: `invite_sent`,
        userId: ctx.session.user.id,
        properties: { teamId: input.teamId },
      })

      return { invite, token, emailDelivered }
    }),

  accept: authedProcedure
    .input(z.object({ token: z.string() }))
    .mutation(async ({ ctx, input }) => {
      // Filled on the fresh-join path only; consumed AFTER the transaction
      // commits (see the conversion event below). A box rather than a plain
      // `let` so the analytics payload stays out of the tRPC response shape.
      const captured: {
        joined: { teamId: string; inviteId: string } | null
      } = { joined: null }

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
            and(eq(teamInvites.id, invite.id), isNull(teamInvites.acceptedAt))
          )
          .returning({ id: teamInvites.id })

        if (accepted.length === 0) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Invite has already been used`,
          })
        }

        // Seat gate applies to the FRESH-JOIN path only, and only once the
        // invite is validated + claimed (REV2-71): an over-seat team must
        // never lock existing members out, so the alreadyMember no-op above
        // and the used/expired errors win over the plan-limit error. A throw
        // here rolls the claim back, leaving the invite pending.
        await assertCanInviteMember(invite.teamId)

        const txId = await generateTxId(tx)

        // Create membership
        await tx.insert(teamMembers).values({
          teamId: invite.teamId,
          userId: ctx.session.user.id,
          role: invite.role,
        })

        captured.joined = { teamId: invite.teamId, inviteId: invite.id }

        return { team, alreadyMember: false, txId }
      })
      // Post-commit (never inside the tx — a concurrent shape renewal would
      // repopulate the cache with pre-commit membership).
      if (!result.alreadyMember) {
        invalidateMembershipCaches()
      }
      // Funnel event (EXP-362), post-commit on the global handle for the same
      // reason: recordConversionEvent swallows errors, so a non-conflict
      // insert failure inside the tx would poison it and turn the COMMIT into
      // a silent ROLLBACK. Idempotent via ON CONFLICT DO NOTHING.
      const joined = captured.joined
      if (joined) {
        await recordConversionEvent(ctx.db, {
          name: `invite_accepted`,
          userId: ctx.session.user.id,
          properties: { teamId: joined.teamId, inviteId: joined.inviteId },
        })
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
