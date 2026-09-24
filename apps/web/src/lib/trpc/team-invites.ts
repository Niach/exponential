import { z } from "zod"
import { router, procedure, authedProcedure, generateTxId } from "@/lib/trpc"
import {
  emailDeliveries,
  teamInvites,
  teamMembers,
  teams,
  users,
} from "@/db/schema"
import { and, count, eq, gt, isNull, ne, sql } from "drizzle-orm"
import { randomBytes } from "crypto"
import { TRPCError } from "@trpc/server"
import { db } from "@/db/connection"
import { assertTeamMember } from "@/lib/team-membership"
import { invalidateMembershipCaches } from "@/lib/auth/membership-cache"
import { buildAuthConfig } from "@/lib/auth/config"
import {
  claimPlaceholder,
  createPlaceholderMember,
  mergePlaceholderIntoUser,
  normalizeInviteEmail,
  resolvePlaceholderIdentity,
} from "@/lib/placeholder-members"
import { recordConversionEvent } from "@/lib/conversion/events"
import { assertCanInviteMember } from "@/lib/billing"
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
// (team-members.ts): a team owner. Instance admins get no bypass (EXP-557).
async function assertCanManageMembers(userId: string, teamId: string) {
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
  placeholderUserId: teamInvites.placeholderUserId,
  acceptedAt: teamInvites.acceptedAt,
  expiresAt: teamInvites.expiresAt,
  createdAt: teamInvites.createdAt,
  updatedAt: teamInvites.updatedAt,
} as const

// A re-invite supersedes the placeholder's earlier links in THIS team: the
// unaccepted rows go (the fresh row inserted in the same transaction is the
// one member lists read "invited, not joined" from), so exactly one live
// invite row exists per placeholder. Expiring them instead left every
// superseded link in older clients' pending lists (desktop ≤0.14.49 filters
// on accepted_at only). Revoke keeps its expire-in-place semantics.
async function deleteSupersededInvitesFor(
  tx: Pick<typeof db, `delete`>,
  teamId: string,
  placeholderUserId: string
) {
  await tx
    .delete(teamInvites)
    .where(
      and(
        eq(teamInvites.teamId, teamId),
        eq(teamInvites.placeholderUserId, placeholderUserId),
        isNull(teamInvites.acceptedAt)
      )
    )
}

// Whether an invited person could ever sign in AS the placeholder row: a
// mail transport (the sign-in code and the password reset land on the
// mailbox) or a social/OIDC provider (the login links onto the row by
// address). A password-only instance without mail has neither, so there an
// email invite stays the plain token invite — the person creates a password
// account and accepts the link. Passkeys need a session first, so they never
// count. Exported for the tests.
export function placeholderClaimable(): boolean {
  const config = buildAuthConfig()
  return (
    config.passwordResetEnabled ||
    config.emailOtpEnabled ||
    config.googleLoginEnabled ||
    config.appleLoginEnabled ||
    config.oidcProviders.length > 0
  )
}

export const teamInvitesRouter = router({
  create: authedProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
        role: z.enum([`owner`, `member`]).default(`member`),
        // Recipient address (EXP-188 invite-by-email). EXP-630: an email
        // invite also puts the person on the roster right away as a
        // PLACEHOLDER member (lib/placeholder-members.ts) — assignable and
        // attributable before they join. accept() stays token-bound.
        email: z.string().email().max(255).optional(),
        // The placeholder's display name; empty = the mailbox local part.
        name: z.string().trim().max(180).optional(),
        // Re-invite an unclaimed placeholder member (Members "Resend invite"),
        // optionally at a corrected address — the row keeps its attributions.
        placeholderUserId: z.string().min(1).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      await assertCanManageMembers(ctx.session.user.id, input.teamId)

      const token = randomBytes(32).toString(`hex`)
      const now = new Date()
      const expiresAt = new Date(now.getTime() + 7 * 24 * 60 * 60 * 1000) // 7 days
      const email = input.email ? normalizeInviteEmail(input.email) : null

      let memberAdded = false
      const { invite, placeholderUserId } = await ctx.db.transaction(
        async (tx) => {
          let placeholderUserId: string | null = null

          if (input.placeholderUserId) {
            if (!email) {
              throw new TRPCError({
                code: `BAD_REQUEST`,
                message: `An email address is needed to re-invite a member.`,
              })
            }
            const [target] = await tx
              .select({
                id: users.id,
                email: users.email,
                placeholderAt: users.placeholderAt,
              })
              .from(users)
              .innerJoin(
                teamMembers,
                and(
                  eq(teamMembers.userId, users.id),
                  eq(teamMembers.teamId, input.teamId)
                )
              )
              .where(eq(users.id, input.placeholderUserId))
              .limit(1)
            if (!target) {
              throw new TRPCError({
                code: `NOT_FOUND`,
                message: `Member not found`,
              })
            }
            if (!target.placeholderAt) {
              throw new TRPCError({
                code: `BAD_REQUEST`,
                message: `That member has already joined.`,
              })
            }
            if (email !== normalizeInviteEmail(target.email)) {
              // `users.email` is global, and the claim is by address: an
              // owner may only re-address a placeholder that belongs to THIS
              // team alone. One seated elsewhere too would otherwise be
              // hijacked — rewrite its address, sign in there, become a
              // member of the other team.
              const [foreign] = await tx
                .select({ id: teamMembers.id })
                .from(teamMembers)
                .where(
                  and(
                    eq(teamMembers.userId, target.id),
                    ne(teamMembers.teamId, input.teamId)
                  )
                )
                .limit(1)
              if (foreign) {
                throw new TRPCError({
                  code: `FORBIDDEN`,
                  message: `That member is also on another team's roster; their address can only be corrected by re-inviting the new one.`,
                })
              }
              const [taken] = await tx
                .select({ id: users.id })
                .from(users)
                .where(sql`lower(${users.email}) = ${email}`)
                .limit(1)
              if (taken) {
                throw new TRPCError({
                  code: `BAD_REQUEST`,
                  message: `${email} already belongs to another account.`,
                })
              }
            }
            const identity = resolvePlaceholderIdentity({
              email,
              name: input.name,
            })
            await tx
              .update(users)
              .set({
                email: identity.email,
                ...(input.name?.trim() ? { name: identity.name } : {}),
                updatedAt: now,
              })
              .where(eq(users.id, target.id))
            await deleteSupersededInvitesFor(tx, input.teamId, target.id)
            placeholderUserId = target.id
          } else if (email) {
            const [existing] = await tx
              .select({ id: users.id, placeholderAt: users.placeholderAt })
              .from(users)
              .where(sql`lower(${users.email}) = ${email}`)
              .limit(1)
            const [member] = existing
              ? await tx
                  .select({ id: teamMembers.id })
                  .from(teamMembers)
                  .where(
                    and(
                      eq(teamMembers.teamId, input.teamId),
                      eq(teamMembers.userId, existing.id)
                    )
                  )
                  .limit(1)
              : [undefined]
            if (existing && member) {
              if (!existing.placeholderAt) {
                throw new TRPCError({
                  code: `BAD_REQUEST`,
                  message: `${email} is already a member of this team.`,
                })
              }
              // An unclaimed placeholder already on the roster: a fresh link.
              await deleteSupersededInvitesFor(tx, input.teamId, existing.id)
              placeholderUserId = existing.id
            } else if (existing) {
              // A real account — or another team's unclaimed placeholder,
              // which is treated exactly the same: a typed address proves
              // nothing, so it is never seated here; the person joins by
              // accepting, and the invite stays unbound (no placeholder id).
              await assertCanInviteMember(input.teamId)
            } else if (placeholderClaimable()) {
              await assertCanInviteMember(input.teamId)
              const created = await createPlaceholderMember(tx, {
                teamId: input.teamId,
                role: input.role,
                identity: resolvePlaceholderIdentity({
                  email,
                  name: input.name,
                }),
                now,
              })
              placeholderUserId = created.userId
              memberAdded = true
            } else {
              // No way to ever sign in as the placeholder on this instance
              // (password-only, no mail): the plain token invite instead.
              await assertCanInviteMember(input.teamId)
            }
          } else {
            await assertCanInviteMember(input.teamId)
          }

          const [invite] = await tx
            .insert(teamInvites)
            .values({
              teamId: input.teamId,
              invitedById: ctx.session.user.id,
              role: input.role,
              token,
              email,
              placeholderUserId,
              expiresAt,
            })
            .returning()
          return { invite, placeholderUserId }
        }
      )
      // Post-commit (never inside the tx — a concurrent shape renewal would
      // repopulate the cache with pre-commit membership).
      if (memberAdded) {
        invalidateMembershipCaches()
      }

      // Email delivery is best-effort AFTER the insert — a transport failure
      // must never roll back the invite (the owner still holds the link and
      // can share it by hand). null = no email requested; false = requested
      // but not delivered (no transport / send error / per-address cap).
      // Every attempt is ledgered in email_deliveries (kind team_invite) so
      // bounces trace per-message.
      let emailDelivered: boolean | null = null
      if (email) {
        try {
          const capped =
            (await countRecentInviteEmails(email)) >=
            INVITE_EMAILS_PER_ADDRESS_PER_WEEK
          if (capped) {
            emailDelivered = false
            await ctx.db.insert(emailDeliveries).values({
              userId: null,
              toEmail: email,
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
              to: email,
              teamName: team?.name ?? `a team`,
              inviterName:
                ctx.session.user.name || ctx.session.user.email || `A teammate`,
              inviteUrl: `${appBaseUrl()}/invite/${token}`,
            })
            emailDelivered = result.delivered
            await ctx.db.insert(emailDeliveries).values({
              userId: null,
              toEmail: email,
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

      // memberUserId = the placeholder member this invite is bound to (null
      // for link invites and invites to an existing account).
      return { invite, token, emailDelivered, memberUserId: placeholderUserId }
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
        // the alreadyMember paths below. The IS NULL predicate keeps an
        // existing timestamp untouched.
        const now = new Date()
        const stampOnboarding = async () => {
          await tx
            .update(users)
            .set({ onboardingCompletedAt: now, updatedAt: now })
            .where(
              and(
                eq(users.id, ctx.session.user.id),
                isNull(users.onboardingCompletedAt)
              )
            )
        }

        // EXP-630 happy path: the invitee signed in THROUGH the placeholder's
        // mailbox (sign-in code, OAuth link, password reset), so the session
        // hook already claimed the row and stamped the invite accepted before
        // this page was reached. They ARE the member the invite seated —
        // used/expired do not apply; report the membership. claimPlaceholder
        // stays idempotent for the race where the hook has not run yet.
        if (
          invite.placeholderUserId &&
          invite.placeholderUserId === ctx.session.user.id &&
          existing
        ) {
          await stampOnboarding()
          const txId = await generateTxId(tx)
          await claimPlaceholder(tx, ctx.session.user.id, now)
          if (!invite.acceptedAt) {
            captured.joined = { teamId: invite.teamId, inviteId: invite.id }
          }
          return { team, alreadyMember: true, txId }
        }

        if (invite.acceptedAt) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Invite has already been used`,
          })
        }

        if (invite.expiresAt < now) {
          throw new TRPCError({
            code: `BAD_REQUEST`,
            message: `Invite has expired`,
          })
        }

        await stampOnboarding()

        // An existing member must not burn the single-use invite — and never
        // absorbs a placeholder bound to it (a colleague opening the link
        // would otherwise take over the invitee's attributions and seat).
        if (existing) {
          return { team, alreadyMember: true }
        }

        // EXP-630: an invite bound to a placeholder member is CLAIMED, not
        // joined — the roster row exists already. From another account the
        // placeholder's attributions and seat move here; the placeholder
        // itself only lands here when its seat was removed meanwhile (then
        // it joins afresh). Either way the single-use claim still applies.
        if (invite.placeholderUserId) {
          const claimed = await tx
            .update(teamInvites)
            .set({ acceptedAt: now })
            .where(
              and(
                eq(teamInvites.id, invite.id),
                isNull(teamInvites.acceptedAt)
              )
            )
            .returning({ id: teamInvites.id })
          if (claimed.length === 0) {
            throw new TRPCError({
              code: `BAD_REQUEST`,
              message: `Invite has already been used`,
            })
          }
          const txId = await generateTxId(tx)
          let joinedFresh = false
          if (invite.placeholderUserId === ctx.session.user.id) {
            await claimPlaceholder(tx, ctx.session.user.id, now)
            joinedFresh = true
          } else {
            // The seat: the placeholder's, handed over. Only when it was
            // removed meanwhile does the accepter take a NEW one.
            const [seated] = await tx
              .select({ id: teamMembers.id })
              .from(teamMembers)
              .where(
                and(
                  eq(teamMembers.teamId, invite.teamId),
                  eq(teamMembers.userId, invite.placeholderUserId)
                )
              )
              .limit(1)
            if (!seated) await assertCanInviteMember(invite.teamId)
            const { merged } = await mergePlaceholderIntoUser(tx, {
              placeholderId: invite.placeholderUserId,
              userId: ctx.session.user.id,
              teamId: invite.teamId,
              userEmail: ctx.session.user.email,
              role: invite.role,
            })
            // The flag was NULL inside the tx (a claim raced this accept):
            // nothing to merge — an ordinary join instead.
            if (!merged) joinedFresh = true
          }
          if (joinedFresh) {
            await assertCanInviteMember(invite.teamId)
            await tx.insert(teamMembers).values({
              teamId: invite.teamId,
              userId: ctx.session.user.id,
              role: invite.role,
            })
          }
          captured.joined = { teamId: invite.teamId, inviteId: invite.id }
          return { team, alreadyMember: false, txId }
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

      // EXP-630: a placeholder's invite row is what member lists read
      // "invited, not joined" from — revoking kills the LINK (expires now)
      // and keeps the row; the member stays until removed from the roster.
      if (invite.placeholderUserId && !invite.acceptedAt) {
        await ctx.db
          .update(teamInvites)
          .set({ expiresAt: new Date() })
          .where(eq(teamInvites.id, input.id))
        return { ok: true }
      }

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
