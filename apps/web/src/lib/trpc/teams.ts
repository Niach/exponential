import { z } from "zod"
import { TRPCError } from "@trpc/server"
import {
  router,
  authedProcedure,
  publicProcedure,
  generateTxId,
} from "@/lib/trpc"
import { teams, teamMembers } from "@/db/schema"
import { asc, eq } from "drizzle-orm"
import { teamColumns } from "@/lib/team-columns"
import { emailEnabled } from "@/lib/email-enabled"
import { deleteStorageObjects } from "@/lib/storage/issue-attachment-cleanup"
import { collectTeamStorageKeys } from "@/lib/storage/team-storage-keys"
import { invalidateMembershipCaches } from "@/lib/auth/membership-cache"
import { recordConversionEvent } from "@/lib/conversion/events"
import { randomBytes } from "crypto"
import {
  assertTeamMember,
  assertTeamOwner,
  getTeamMember,
} from "@/lib/team-membership"
import {
  assertCanCreateTeam,
  assertCanUseHelpdesk,
  getInviteCapacity,
} from "@/lib/billing"
import { assertTeamDeletableBilling } from "@/lib/billing/billing-handover"

function slugify(input: string): string {
  return input
    .toLowerCase()
    .normalize(`NFKD`)
    .replace(/[̀-ͯ]/g, ``)
    .replace(/[^a-z0-9]+/g, `-`)
    .replace(/^-+|-+$/g, ``)
    .slice(0, 48)
}

type DbOrTx = {
  // eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
  select: typeof import("@/db/connection").db.select
}

// Oldest membership — the user's "default" team.
async function findOldestMembership(db: DbOrTx, userId: string) {
  const [membership] = await db
    .select({ teamId: teamMembers.teamId })
    .from(teamMembers)
    .where(eq(teamMembers.userId, userId))
    .orderBy(asc(teamMembers.createdAt))
    .limit(1)
  return membership
}

async function uniqueSlug(tx: DbOrTx, base: string): Promise<string> {
  const root = slugify(base) || `team`
  let candidate = root
  let suffix = 0
  while (suffix < 5) {
    const [existing] = await tx
      .select({ id: teams.id })
      .from(teams)
      .where(eq(teams.slug, candidate))
      .limit(1)
    if (!existing) return candidate
    suffix += 1
    candidate = `${root}-${suffix}`
  }
  return `${root}-${randomBytes(3).toString(`hex`)}`
}

export const teamsRouter = router({
  // The user's default landing team (EXP-188): oldest membership, or null
  // when the user has none — signup no longer auto-creates a team, so
  // clients route null to the onboarding create-or-join choice. Never
  // creates anything.
  getDefault: authedProcedure.query(async ({ ctx }) => {
    const membership = await findOldestMembership(
      ctx.db,
      ctx.session.user.id
    )
    if (!membership) return { team: null }

    const [team] = await ctx.db
      .select(teamColumns)
      .from(teams)
      .where(eq(teams.id, membership.teamId))
      .limit(1)
    return { team: team ?? null }
  }),

  // Open to every user (EXP-188) — the creator becomes owner. The only gate
  // is the invisible free-tier owned-team abuse cap (lib/billing.ts).
  create: authedProcedure
    .input(
      z.object({
        name: z.string().min(1).max(255),
        iconUrl: z.string().url().max(2048).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      await assertCanCreateTeam(userId)

      const result = await ctx.db.transaction(async (tx) => {
        const slug = await uniqueSlug(tx, input.name)

        const txId = await generateTxId(tx)
        const [team] = await tx
          .insert(teams)
          .values({
            name: input.name,
            slug,
            iconUrl: input.iconUrl,
          })
          .returning(teamColumns)

        await tx.insert(teamMembers).values({
          teamId: team.id,
          userId,
          role: `owner`,
        })

        return { team, txId }
      })
      // Post-commit (never inside the tx — a concurrent shape renewal would
      // repopulate the cache with pre-commit membership).
      invalidateMembershipCaches()
      // Funnel event (EXP-362), also post-commit on the global handle:
      // recordConversionEvent swallows errors, so a non-conflict insert
      // failure inside the tx would poison it and turn the COMMIT into a
      // silent ROLLBACK. Idempotent via ON CONFLICT DO NOTHING.
      await recordConversionEvent(ctx.db, {
        name: `team_created`,
        userId,
        properties: { teamId: result.team.id },
      })
      return result
    }),

  // EXP-725: how many more invites the team can mint right now, for the
  // onboarding invite step and the natives' members section (they REMOVE the
  // control at zero, App Store 3.1.1). Any member may ask; `null` = unlimited.
  // Pending invites count, so parallel senders converge (lib/billing.ts).
  inviteCapacity: authedProcedure
    .input(z.object({ teamId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      await assertTeamMember(ctx.session.user.id, input.teamId)
      return await getInviteCapacity(input.teamId)
    }),

  // Teams are always private — there are no visibility flags here.
  // `helpdeskEnabled` is the team-level helpdesk switch (owner-only like
  // every field on this procedure; ENABLING is plan-gated, disabling is
  // always allowed).
  update: authedProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
        name: z.string().min(1).max(255).optional(),
        iconUrl: z.string().url().max(2048).nullable().optional(),
        helpdeskEnabled: z.boolean().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const { teamId: id, ...updates } = input
      await assertTeamOwner(ctx.session.user.id, id)

      if (updates.helpdeskEnabled === true) {
        // REV2-10: the reporter's ONLY credential is the emailed magic link,
        // so a helpdesk on an instance with no mail transport accepts tickets
        // into a guaranteed black hole. Refuse at setup time, where the
        // person flipping the switch can still fix it.
        if (!emailEnabled) {
          throw new TRPCError({
            code: `PRECONDITION_FAILED`,
            message: `Email sending is not configured on this server, and support reporters can only reach their conversation through an emailed link. Set AWS_SES_REGION (Amazon SES) or SMTP_HOST, then enable support.`,
          })
        }
        await assertCanUseHelpdesk(id)
      }

      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [team] = await tx
          .update(teams)
          .set({ ...updates, updatedAt: new Date() })
          .where(eq(teams.id, id))
          .returning(teamColumns)
        return { team, txId }
      })
    }),

  delete: authedProcedure
    .input(z.object({ teamId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      await assertTeamOwner(ctx.session.user.id, input.teamId)

      // A paying team must be un-subscribed BEFORE it can be deleted
      // (REV2-55): `creem_subscriptions.team_id` goes `set null` on delete,
      // after which the remote Creem subscription would keep charging with
      // nothing left to find it by (the paying-ghost bug). Cancelling is the
      // owner's own call, not a side effect of a delete confirm — a
      // cancellation already scheduled for period end passes the gate.
      await assertTeamDeletableBilling(input.teamId)

      // Collected inside the tx BEFORE the cascade drops the attachment rows;
      // the cascade never touches S3, so without this the blobs orphan.
      let storageKeys: string[] = []
      // No last-team guard (EXP-188): deleting your only team is allowed —
      // nothing self-heals a replacement anymore, clients route the
      // team-less state back into onboarding.
      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        storageKeys = await collectTeamStorageKeys(tx, [input.teamId])
        await tx.delete(teams).where(eq(teams.id, input.teamId))
        return { ok: true, txId }
      })
      // Post-commit: the cascade dropped every member's teamMembers row.
      invalidateMembershipCaches()

      // No remote cancellation here — the gate above already proved there is
      // nothing live left to cancel (a scheduled cancellation keeps serving
      // the period the customer paid for and then ends by itself).
      await deleteStorageObjects(storageKeys)

      return result
    }),

  // Member-only read of minimal team metadata by slug (used by the web
  // route guard). Returns NOT_FOUND for non-members and anonymous callers
  // alike, to avoid leaking existence.
  getBySlug: publicProcedure
    .input(z.object({ slug: z.string().min(1).max(255) }))
    .query(async ({ ctx, input }) => {
      const [team] = await ctx.db
        .select({
          id: teams.id,
          name: teams.name,
          slug: teams.slug,
          iconUrl: teams.iconUrl,
        })
        .from(teams)
        .where(eq(teams.slug, input.slug))
        .limit(1)

      if (!team) {
        throw new TRPCError({ code: `NOT_FOUND` })
      }

      const userId = ctx.session?.user?.id
      const member = userId ? await getTeamMember(userId, team.id) : undefined

      if (!member) {
        throw new TRPCError({ code: `NOT_FOUND` })
      }
      return { ...team, membership: member.role }
    }),
})
